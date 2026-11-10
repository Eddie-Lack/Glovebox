"""Loads the .glb models libgarib's objbank-tool writes (and imported models' previews) into scene nodes.

The static pose, Lambert shading with the vertex colours libgarib exports,
alpha cut-outs kept, and libgarib's EXT_node_tsr_inheritance {scale: false} honoured (the node ignores its
parents' scale, as Glover's skeletons do).
"""
import io, posixpath
import numpy as np
import PIL.Image
import pygltflib as gl
from . import vec
from .scene import Geometry, Material, Mesh, Node, Texture

COMPONENTS = {5120: np.int8, 5121: np.uint8, 5122: np.int16, 5123: np.uint16, 5125: np.uint32, 5126: np.float32}
WIDTH = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4, "MAT4": 16}
NORMALIZE = {5120: 127.0, 5121: 255.0, 5122: 32767.0, 5123: 65535.0}


class Model:
    """A loaded glTF scene: a node tree to copy, and the number of object nodes the game draws (each takes one of a
    frame's object matrices, object.c: the glTF's nodes with a mesh)"""

    def __init__(self, root, draw_nodes):
        self.root, self.draw_nodes = root, draw_nodes

    def instance(self):
        return copy_tree(self.root)


def copy_tree(n):
    c = Node(n.name)
    c.pos, c.quat, c.scale = n.pos.copy(), n.quat.copy(), n.scale.copy()
    c.items = list(n.items)   # meshes share their geometry and material
    c.order = n.order
    for k in n.children:
        c.add(copy_tree(k))
    return c


def accessor(g, blob, index):
    a = g.accessors[index]
    dtype = COMPONENTS[a.componentType]
    width = WIDTH[a.type]
    bv = g.bufferViews[a.bufferView]
    buf = blob[bv.buffer]
    start = (bv.byteOffset or 0) + (a.byteOffset or 0)
    item = np.dtype(dtype).itemsize * width
    stride = bv.byteStride or item
    if stride == item:
        arr = np.frombuffer(buf, dtype, a.count * width, start).reshape(a.count, width)
    else:
        raw = np.frombuffer(buf, np.uint8, (a.count - 1) * stride + item, start)
        arr = np.lib.stride_tricks.as_strided(raw, (a.count, item), (stride, 1)).copy().view(dtype).reshape(a.count, width)
    arr = arr.astype(np.float32) if a.componentType == 5126 else arr
    if a.normalized and a.componentType in NORMALIZE:
        arr = arr.astype(np.float32) / NORMALIZE[a.componentType]
    return arr


def load(data, fetch, base_url=""):
    """data: the .glb bytes. fetch(url) returns the bytes of a file the model refers to (its textures)."""
    g = gl.GLTF2.load_from_bytes(data)
    blob = []
    for b in g.buffers:
        if b.uri is None:
            blob.append(g.binary_blob())
        elif b.uri.startswith("data:"):
            blob.append(g.get_data_from_buffer_uri(b.uri))
        else:
            blob.append(fetch(posixpath.join(posixpath.dirname(base_url), b.uri)))

    textures = {}

    def texture(i):
        if i not in textures:
            t = g.textures[i]
            img = g.images[t.source]
            try:
                if img.uri:
                    raw = (g.get_data_from_buffer_uri(img.uri) if img.uri.startswith("data:")
                           else fetch(posixpath.normpath(posixpath.join(posixpath.dirname(base_url), img.uri))))
                else:
                    bv = g.bufferViews[img.bufferView]
                    raw = bytes(blob[bv.buffer][bv.byteOffset or 0:(bv.byteOffset or 0) + bv.byteLength])
                rgba = np.asarray(PIL.Image.open(io.BytesIO(raw)).convert("RGBA"))
                textures[i] = (Texture(rgba), t.sampler)
            except Exception:
                textures[i] = (None, None)
        return textures[i]

    materials = {}

    def material(i, has_colors):
        key = (i, has_colors)
        if key not in materials:
            m = g.materials[i] if i is not None else None
            tex, wrap = None, (10497, 10497)
            color, opacity, transparent, alpha_test = (1, 1, 1), 1.0, False, 0.0
            if m is not None:
                pbr = m.pbrMetallicRoughness
                if pbr and pbr.baseColorTexture is not None:
                    tex, si = texture(pbr.baseColorTexture.index)
                    if si is not None:
                        s = g.samplers[si]
                        wrap = (s.wrapS or 10497, s.wrapT or 10497)
                if pbr and pbr.baseColorFactor:
                    color = tuple(pbr.baseColorFactor[:3])
                    opacity = pbr.baseColorFactor[3]
                transparent = m.alphaMode == "BLEND"
                if m.alphaMode == "MASK":
                    alpha_test = m.alphaCutoff if m.alphaCutoff is not None else 0.5
            materials[key] = Material(color=color, texture=tex, vertex_colors=has_colors, transparent=transparent,
                                      opacity=opacity, alpha_test=alpha_test or (0.3 if tex is not None else 0.0),
                                      wrap=wrap)
        return materials[key]

    meshes = {}

    def mesh_items(i):
        if i not in meshes:
            items = []
            for p in g.meshes[i].primitives:
                if p.mode not in (None, 4):
                    continue
                at = p.attributes
                pos = accessor(g, blob, at.POSITION)
                idx = accessor(g, blob, p.indices).reshape(-1) if p.indices is not None else None
                nrm = accessor(g, blob, at.NORMAL) if at.NORMAL is not None else None
                uv = accessor(g, blob, at.TEXCOORD_0) if at.TEXCOORD_0 is not None else None
                col = None
                if at.COLOR_0 is not None:
                    col = accessor(g, blob, at.COLOR_0).astype(np.float32)
                    if col.shape[1] == 3:
                        col = np.hstack([col, np.ones((len(col), 1), np.float32)])
                if idx is not None:
                    idx = idx[:len(idx) // 3 * 3]
                if nrm is None:
                    nrm = vertex_normals(pos, idx)
                items.append(Mesh(Geometry(pos, idx, nrm, uv, col), material(p.material, col is not None)))
            meshes[i] = items
        return meshes[i]

    nodes = []
    for n in g.nodes:
        node = Node(n.name or "")
        if n.matrix:
            m = np.array(n.matrix, float).reshape(4, 4).T
            node.pos = m[:3, 3].copy()
            node.scale = vec.decompose_scale(m)
            node.quat = vec.mat3_to_quat(m[:3, :3] / np.where(node.scale == 0, 1, node.scale)[None, :])
        else:
            if n.translation:
                node.pos = np.array(n.translation, float)
            if n.rotation:
                node.quat = np.array(n.rotation, float)
            if n.scale:
                node.scale = np.array(n.scale, float)
        if n.mesh is not None:
            node.items = list(mesh_items(n.mesh))
        nodes.append(node)
    for n, node in zip(g.nodes, nodes):
        for c in n.children or []:
            node.add(nodes[c])
    root = Node("scene")
    scene = g.scenes[g.scene or 0] if g.scenes else None
    for i in (scene.nodes if scene else [i for i, n in enumerate(nodes) if n.parent is None]):
        root.add(nodes[i])
    # EXT_node_tsr_inheritance {scale: false}: undo the inherited part of the scale so static poses match the game
    worlds = {id(node): m for node, m in root.walk()}
    for n, node in zip(g.nodes, nodes):
        ext = (n.extensions or {}).get("EXT_node_tsr_inheritance")
        if ext and ext.get("scale") is False and node.parent is not None and id(node.parent) in worlds:
            ps = vec.decompose_scale(worlds[id(node.parent)])
            node.scale = node.scale / np.where(ps == 0, 1, ps)
    draw_nodes = max(1, sum(1 for n in g.nodes if n.mesh is not None))
    return Model(root, draw_nodes)


def vertex_normals(pos, idx):
    t = idx.reshape(-1, 3) if idx is not None else np.arange(len(pos) // 3 * 3).reshape(-1, 3)
    n = np.zeros_like(pos, dtype=np.float32)
    fn = np.cross(pos[t[:, 1]] - pos[t[:, 0]], pos[t[:, 2]] - pos[t[:, 0]])
    for k in range(3):
        np.add.at(n, t[:, k], fn)
    return n / np.maximum(np.linalg.norm(n, axis=1, keepdims=True), 1e-9)


def stats(data):
    """Triangles and the box of a glTF (Import model's dialog), or raises"""
    model = load(data, lambda url: b"")
    tris = 0
    for node, m in model.root.walk():
        for it in node.items:
            tris += len(it.geometry.triangles())
    return tris, model.root.box()
