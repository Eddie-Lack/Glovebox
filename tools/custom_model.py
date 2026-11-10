#!/usr/bin/env python3
"""Converts a glTF model (.glb, or .gltf with embedded data) into Glover models, for the editor's Import model.

    python3 tools/custom_model.py MODEL.glb --out DIR [--name NAME] [--scale S]

Bakes every node's transform and the scale into the vertices. A large model, such as a whole map, is cut into
pieces of at most MAX_TRIANGLES triangles. Glover builds its levels from many solid pieces (land actors), and each
piece needs its own object. Each piece is re-centred on its own middle; meta.json records where it goes.

Textures become 16-bit RGBA textures of at most 2,048 texels with power-of-two sides, the most one G_LOADBLOCK
loads. A material without a texture gets a small texture of its colour, because libgarib's packer needs a texture
on every triangle.

DIR receives:
  meta.json            name, scale, size, and for each piece its object ID, name, offset, bank size and textures
  0xIIIIIIII.obj.bin   each piece as a single-object bank (objbank-tool pack)
  textures/0xTTTTTTTT.png, textures.tex.bin
                       the converted textures, and a texture bank of them (texbank-tool pack)
  preview/0xIIIIIIII-NAME.glb
                       each piece unpacked again (objbank-tool unpack): what the game gets, for the editor
"""
import argparse, base64, concurrent.futures, hashlib, io, json, math, os, re, sys

import numpy as np
import PIL.Image
import pygltflib as gl

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "vendor", "libgarib", "python"))
from libgarib.hash import hash_str  # noqa: E402

MAX_TRIANGLES = 1500
MAX_TEXELS = 2048
MAX_SIDE = 256
MAX_INPUT_TRIANGLES = 200000
PIL.Image.MAX_IMAGE_PIXELS = 4096 * 4096


class ModelError(ValueError):
    pass


# ---------------------------------------------------------------------------------------------
# Reading the glTF

COMPONENTS = {gl.BYTE: "b", gl.UNSIGNED_BYTE: "B", gl.SHORT: "h", gl.UNSIGNED_SHORT: "H", gl.UNSIGNED_INT: "I",
              gl.FLOAT: "f"}
WIDTHS = {gl.SCALAR: 1, gl.VEC2: 2, gl.VEC3: 3, gl.VEC4: 4}
NORMALISED = {gl.BYTE: 127, gl.UNSIGNED_BYTE: 255, gl.SHORT: 32767, gl.UNSIGNED_SHORT: 65535}


def buffers_of(g):
    out = []
    for b in g.buffers:
        if b.uri is None:
            out.append(g.binary_blob() or b"")
        elif b.uri.startswith("data:"):
            out.append(base64.b64decode(b.uri.split(",", 1)[1]))
        else:
            raise ModelError("The model refers to a separate file (%s). Export it as a single .glb file." % b.uri)
    return out


def view_bytes(g, bufs, index):
    v = g.bufferViews[index]
    return bufs[v.buffer][v.byteOffset or 0:(v.byteOffset or 0) + v.byteLength], v.byteStride


def accessor(g, bufs, index):
    a = g.accessors[index]
    if a.sparse is not None:
        raise ModelError("Sparse accessors are not supported. Export the model without them.")
    width, dtype = WIDTHS.get(a.type), np.dtype(COMPONENTS[a.componentType]).newbyteorder("<")
    if width is None:
        raise ModelError("Unsupported accessor type %s" % a.type)
    if a.bufferView is None:
        return np.zeros((a.count, width), np.float32)
    data, stride = view_bytes(g, bufs, a.bufferView)
    item = width * dtype.itemsize
    stride = stride or item
    start = a.byteOffset or 0
    if start + stride * (a.count - 1) + item > len(data):
        raise ModelError("An accessor reads past the end of its buffer")
    raw = np.lib.stride_tricks.as_strided(np.frombuffer(data, np.uint8, offset=start), (a.count, item), (stride, 1))
    arr = np.ascontiguousarray(raw).view(dtype).reshape(a.count, width).astype(np.float64)
    if a.normalized and a.componentType in NORMALISED:
        arr = np.maximum(arr / NORMALISED[a.componentType], -1)
    return arr


def node_matrix(n):
    if n.matrix:
        return np.array(n.matrix, np.float64).reshape(4, 4).T
    t = np.array(n.translation or (0, 0, 0), np.float64)
    x, y, z, w = n.rotation or (0, 0, 0, 1)
    r = np.array([[1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)],
                  [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)],
                  [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)]])
    m = np.eye(4)
    m[:3, :3] = r * np.array(n.scale or (1, 1, 1), np.float64)
    m[:3, 3] = t
    return m


def image_of(g, bufs, texture_index):
    """The PIL image behind a texture index, or None."""
    if texture_index is None or texture_index >= len(g.textures):
        return None
    src = g.textures[texture_index].source
    if src is None or src >= len(g.images):
        return None
    im = g.images[src]
    if im.bufferView is not None:
        data = view_bytes(g, bufs, im.bufferView)[0]
    elif im.uri and im.uri.startswith("data:"):
        data = base64.b64decode(im.uri.split(",", 1)[1])
    else:
        raise ModelError("The model refers to a separate image (%s). Export it as a single .glb file." % im.uri)
    try:
        return PIL.Image.open(io.BytesIO(data)).convert("RGBA")
    except (PIL.UnidentifiedImageError, PIL.Image.DecompressionBombError, OSError) as e:
        raise ModelError("Could not read an image in the model: %s" % e)


def triangles(g, bufs):
    """Every triangle in the default scene, in world space: positions (T,3,3), normals (T,3,3), uvs (T,3,2) and
    material indices (T,)."""
    P, N, UV, M = [], [], [], []
    scene = g.scenes[g.scene or 0] if g.scenes else None
    roots = scene.nodes if scene else range(len(g.nodes))

    def walk(i, parent, depth):
        if depth > 64:
            raise ModelError("The model's node tree is too deep")
        n = g.nodes[i]
        m = parent @ node_matrix(n)
        if n.mesh is not None:
            for prim in g.meshes[n.mesh].primitives:
                if prim.mode not in (None, gl.TRIANGLES):
                    continue
                pos = accessor(g, bufs, prim.attributes.POSITION)
                idx = accessor(g, bufs, prim.indices)[:, 0].astype(np.int64) if prim.indices is not None else np.arange(len(pos))
                idx = idx[:len(idx) // 3 * 3].reshape(-1, 3)
                if len(idx) and (idx.max() >= len(pos) or idx.min() < 0):
                    raise ModelError("A mesh has indices past its vertices")
                if np.linalg.det(m[:3, :3]) < 0:
                    idx = idx[:, ::-1]
                wp = pos @ m[:3, :3].T + m[:3, 3]
                if prim.attributes.NORMAL is not None:
                    nrm = accessor(g, bufs, prim.attributes.NORMAL) @ np.linalg.inv(m[:3, :3])
                    nrm /= np.maximum(np.linalg.norm(nrm, axis=1, keepdims=True), 1e-9)
                else:
                    nrm = None
                uv = accessor(g, bufs, prim.attributes.TEXCOORD_0) if prim.attributes.TEXCOORD_0 is not None else np.zeros((len(pos), 2))
                tri = wp[idx]
                if nrm is None:
                    fn = np.cross(tri[:, 1] - tri[:, 0], tri[:, 2] - tri[:, 0])
                    fn /= np.maximum(np.linalg.norm(fn, axis=1, keepdims=True), 1e-9)
                    tn = np.repeat(fn[:, None, :], 3, axis=1)
                else:
                    tn = nrm[idx]
                P.append(tri), N.append(tn), UV.append(uv[idx][:, :, :2])
                M.append(np.full(len(idx), -1 if prim.material is None else prim.material))
        for c in n.children or []:
            walk(c, m, depth + 1)

    for r in roots:
        walk(r, np.eye(4), 0)
    if not P:
        raise ModelError("The model has no triangles")
    return np.concatenate(P), np.concatenate(N), np.concatenate(UV), np.concatenate(M)


# ---------------------------------------------------------------------------------------------
# Textures

def pow2_at_most(n, cap):
    p = 1
    while p * 2 <= min(n, cap):
        p *= 2
    return p


def n64_texture(im, factor):
    """An RGBA image no larger than MAX_TEXELS texels, with power-of-two sides, tinted by the material's colour."""
    w, h = pow2_at_most(im.width, MAX_SIDE), pow2_at_most(im.height, MAX_SIDE)
    while w * h > MAX_TEXELS:
        if w >= h:
            w //= 2
        else:
            h //= 2
    out = im.resize((w, h), PIL.Image.LANCZOS)
    if any(abs(f - 1) > 1e-3 for f in factor):
        a = np.asarray(out, np.float64) * np.array(factor)
        out = PIL.Image.fromarray(np.clip(a + 0.5, 0, 255).astype(np.uint8), "RGBA")
    return out


def save_texture(im, path):
    from PIL.PngImagePlugin import PngInfo
    info = PngInfo()
    info.add_text("Comment", json.dumps({"color_format": "rgba", "compression_format": "uncompressed_16b"}))
    im.save(path, "PNG", pnginfo=info)


MAX_TEXELS_COORD = 1000


def near_zero(UV, M, materials):
    """The N64 holds texture coordinates as signed 10.5 fixed point in texels, so below 1,024 texels either way.
    A repeating texture can move coordinates by whole repeats (two for a mirrored one). This moves a piece's
    coordinates as one, or each triangle's on its own if that is not enough. A triangle that still spans too much
    is clamped."""
    UV = UV.copy()
    for m in set(M.tolist()):
        sel, (_, _, wrap, size) = M == m, materials[m]
        for axis in (0, 1):
            limit = MAX_TEXELS_COORD / size[axis]
            step = {gl.REPEAT: 1, gl.MIRRORED_REPEAT: 2}.get(wrap[axis])
            uv = UV[sel, :, axis]
            if step:
                uv -= math.floor(uv.min() / step) * step
                if np.abs(uv).max() > limit:
                    uv -= np.floor(uv.min(axis=1) / step)[:, None] * step
            UV[sel, :, axis] = np.clip(uv, -limit, limit)
    return UV


# ---------------------------------------------------------------------------------------------
# Splitting into pieces

def split(centroids, order, limit):
    """Cuts a list of triangles (indices into centroids) into groups of at most `limit`, across the longest side."""
    if len(order) <= limit:
        return [order]
    c = centroids[order]
    axis = int(np.argmax(c.max(0) - c.min(0)))
    order = order[np.argsort(c[:, axis], kind="stable")]
    half = len(order) // 2
    return split(centroids, order[:half], limit) + split(centroids, order[half:], limit)


# ---------------------------------------------------------------------------------------------
# Writing each piece as the glTF that objbank-tool pack reads

def piece_gltf(node_name, P, N, UV, M, materials):
    """A glTF with one root node (the object) and one primitive per material. libgarib takes the object's ID from
    the hash of the node's name, the mesh's name from its first 8 characters, and a texture's ID from its file
    name."""
    g = gl.GLTF2(asset=gl.Asset(version="2.0"), scene=0)
    blob = bytearray()
    prims, mat_index = [], {}

    def add(arr, target, kind, ctype=gl.FLOAT):
        nonlocal blob
        while len(blob) % 4:
            blob.append(0)
        data = arr.astype("<f4" if ctype == gl.FLOAT else "<u4").tobytes()
        g.bufferViews.append(gl.BufferView(buffer=0, byteOffset=len(blob), byteLength=len(data), target=target))
        blob += data
        acc = gl.Accessor(bufferView=len(g.bufferViews) - 1, componentType=ctype, count=len(arr), type=kind)
        if kind == gl.VEC3 and ctype == gl.FLOAT:
            acc.min, acc.max = arr.min(0).tolist(), arr.max(0).tolist()
        g.accessors.append(acc)
        return len(g.accessors) - 1

    for m in sorted(set(M.tolist())):
        sel = M == m
        tex_id, alpha_mode, wrap, _ = materials[m]
        if tex_id not in mat_index:
            g.images.append(gl.Image(uri="textures/0x%08X.png" % tex_id))
            g.samplers.append(gl.Sampler(wrapS=wrap[0], wrapT=wrap[1]))
            g.textures.append(gl.Texture(source=len(g.images) - 1, sampler=len(g.samplers) - 1))
            g.materials.append(gl.Material(name="0x%08X" % tex_id, alphaMode=alpha_mode,
                                           pbrMetallicRoughness=gl.PbrMetallicRoughness(
                                               baseColorTexture=gl.TextureInfo(index=len(g.textures) - 1))))
            mat_index[tex_id] = len(g.materials) - 1
        n = int(sel.sum()) * 3
        attrs = gl.Attributes(POSITION=add(P[sel].reshape(n, 3), gl.ARRAY_BUFFER, gl.VEC3),
                              NORMAL=add(N[sel].reshape(n, 3), gl.ARRAY_BUFFER, gl.VEC3),
                              TEXCOORD_0=add(UV[sel].reshape(n, 2), gl.ARRAY_BUFFER, gl.VEC2))
        prims.append(gl.Primitive(attributes=attrs, indices=add(np.arange(n), gl.ELEMENT_ARRAY_BUFFER, gl.SCALAR, gl.UNSIGNED_INT),
                                  material=mat_index[tex_id]))
    g.meshes.append(gl.Mesh(name=node_name, primitives=prims))
    g.nodes.append(gl.Node(name=node_name, mesh=0))
    g.scenes.append(gl.Scene(nodes=[0]))
    g.buffers.append(gl.Buffer(byteLength=len(blob), uri="data:application/octet-stream;base64," + base64.b64encode(bytes(blob)).decode()))
    return g


def short_name(name):
    """Mesh names hold 8 characters"""
    s = re.sub(r"[^A-Za-z0-9_]", "", name)[:8]
    return s or "custom"


def convert(data, out, run_tool, name="model", scale=1.0, taken_ids=(), taken_textures=()):
    """Converts the glTF bytes `data` into `out` (see the module's docstring) and returns the meta dict.
    run_tool(script, *args) runs a libgarib tool. taken_ids and taken_textures hold IDs the game already uses."""
    if not (0 < scale < 1e6):
        raise ModelError("The scale must be a positive number")
    try:
        g = gl.GLTF2.load_from_bytes(data)
    except Exception as e:
        raise ModelError("Not a glTF model: %s" % e)
    bufs = buffers_of(g)
    P, N, UV, M = triangles(g, bufs)
    if len(P) > MAX_INPUT_TRIANGLES:
        raise ModelError("The model has %d triangles. The most this importer takes is %d." % (len(P), MAX_INPUT_TRIANGLES))
    P = P * scale
    keep = np.linalg.norm(np.cross(P[:, 1] - P[:, 0], P[:, 2] - P[:, 0]), axis=1) > 1e-6
    P, N, UV, M = P[keep], N[keep], UV[keep], M[keep]
    if not len(P):
        raise ModelError("Every triangle of the model is too small at this scale")

    os.makedirs(os.path.join(out, "textures"), exist_ok=True)
    digest = hashlib.sha1(data + repr(scale).encode()).hexdigest()[:8]

    # Materials: texture ID, glTF alpha mode, wrap modes
    materials, textures = {}, {}
    for m in sorted(set(M.tolist())):
        mat = g.materials[m] if 0 <= m < len(g.materials) else None
        pbr = mat.pbrMetallicRoughness if mat else None
        factor = list(pbr.baseColorFactor) if pbr and pbr.baseColorFactor else [1, 1, 1, 1]
        tex_index = pbr.baseColorTexture.index if pbr and pbr.baseColorTexture else None
        src = image_of(g, bufs, tex_index)
        if src is None:
            src = PIL.Image.new("RGBA", (8, 8), (255, 255, 255, 255))
        im = n64_texture(src, factor)
        alpha = np.asarray(im)[:, :, 3]
        mode = mat.alphaMode if mat and mat.alphaMode in (gl.MASK, gl.BLEND) else (gl.MASK if alpha.min() < 128 else gl.OPAQUE)
        wrap = (gl.REPEAT, gl.REPEAT)
        if tex_index is not None and g.textures[tex_index].sampler is not None:
            s = g.samplers[g.textures[tex_index].sampler]
            wrap = (s.wrapS or gl.REPEAT, s.wrapT or gl.REPEAT)
        key = hashlib.sha1(im.tobytes() + bytes(str(im.size), "ascii")).digest()
        if key not in textures:
            tid = int.from_bytes(hashlib.sha1(key + digest.encode()).digest()[:4], "big")
            while tid in taken_textures or tid in {t for t, _ in textures.values()} or tid == 0:
                tid = (tid + 1) & 0xFFFFFFFF
            path = os.path.join(out, "textures", "0x%08X.png" % tid)
            save_texture(im, path)
            textures[key] = (tid, path)
        materials[m] = (textures[key][0], mode, wrap, im.size)

    # Pieces
    centroids = P.mean(axis=1)
    groups = split(centroids, np.arange(len(P)), MAX_TRIANGLES)
    base = short_name(name)
    pieces, jobs, used = [], [], set(taken_ids)
    lo, hi = P.reshape(-1, 3).min(0), P.reshape(-1, 3).max(0)
    for i, sel in enumerate(groups):
        p = P[sel]
        plo, phi = p.reshape(-1, 3).min(0), p.reshape(-1, 3).max(0)
        centre = np.round((plo + phi) / 2, 1)
        if np.abs(p - centre).max() > 32000:
            raise ModelError("Part of the model is more than 64,000 units across at this scale, more than a Glover model "
                             "holds. Use a smaller scale.")
        pname = base if len(groups) == 1 else (base[:5] + str(i))[:8]
        for n in range(1000):
            # The mesh keeps the first 8 characters; the padding ends its name there
            node_name = "%s%s%d%s" % (pname.ljust(8, "\0"), digest, i, "" if not n else "-%d" % n)
            oid = hash_str(node_name)
            # objbank-tool pack takes a file starting with "{" (0x7B) for glTF JSON: a bank whose first object ID starts with that byte
            # would be misread
            if oid not in used and oid != 0 and (oid >> 24) != 0x7B:
                break
        used.add(oid)
        doc = piece_gltf(node_name, p - centre, N[sel], near_zero(UV[sel], M[sel], materials), M[sel], materials)
        src = os.path.join(out, "%s-%d.gltf" % (base, i))
        doc.save(src)
        piece_textures = sorted({materials[m][0] for m in set(M[sel].tolist())})
        tex_args = [a for t in piece_textures for a in ("-t", os.path.join(out, "textures", "0x%08X.png" % t))]
        bank = os.path.join(out, "0x%08X.obj.bin" % oid)
        jobs.append(("pack", "--scale", "1", "--output-file", bank, *tex_args, src))
        pieces.append({"id": "0x%08X" % oid, "name": pname, "offset": centre.tolist(), "triangles": int(len(sel)),
                       "size": np.round(phi - plo, 1).tolist(), "textures": ["0x%08X" % t for t in piece_textures]})
    # Each pack runs in its own process
    with concurrent.futures.ThreadPoolExecutor(max_workers=min(8, os.cpu_count() or 1)) as pool:
        for f in [pool.submit(run_tool, "objbank-tool.py", *job) for job in jobs]:
            f.result()
    for p, job in zip(pieces, jobs):
        p["bytes"] = os.path.getsize(job[4])
        os.remove(job[-1])

    texbank = os.path.join(out, "textures.tex.bin")
    run_tool("texbank-tool.py", "pack", "--output-file", texbank, *[p for _, p in sorted(textures.values())])
    preview = os.path.join(out, "preview")
    run_tool("objbank-tool.py", "unpack", "--scale", "1", "--output-dir", preview, "-t", texbank,
             *[os.path.join(out, "%s.obj.bin" % p["id"]) for p in pieces])
    for p in pieces:
        found = [f for f in os.listdir(preview) if f.lower().startswith(p["id"].lower()) and f.endswith(".glb")]
        if not found:
            raise ModelError("libgarib could not read back piece %s" % p["id"])
        p["preview"] = found[0]
    meta = {"name": name, "scale": scale, "triangles": int(len(P)), "size": np.round(hi - lo, 1).tolist(),
            "pieces": pieces, "textures": {"0x%08X" % t: os.path.getsize(path) for t, path in textures.values()}}
    json.dump(meta, open(os.path.join(out, "meta.json"), "w"), indent=1)
    return meta


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("model")
    ap.add_argument("--out", required=True)
    ap.add_argument("--name", default=None)
    ap.add_argument("--scale", type=float, default=1.0)
    a = ap.parse_args()
    import subprocess
    tools = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "vendor", "libgarib", "tools")

    def run_tool(script, *args):
        r = subprocess.run([sys.executable, "-W", "ignore", os.path.join(tools, script), *args], capture_output=True, text=True)
        if r.returncode:
            sys.exit("%s failed:\n%s%s" % (script, r.stdout[-2000:], r.stderr[-4000:]))

    name = a.name or os.path.splitext(os.path.basename(a.model))[0]
    meta = convert(open(a.model, "rb").read(), a.out, run_tool, name, a.scale)
    print("%d triangles in %d pieces, %d textures" % (meta["triangles"], len(meta["pieces"]), len(meta["textures"])))


if __name__ == "__main__":
    main()
