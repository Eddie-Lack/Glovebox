#!/usr/bin/env python3
"""Writes a small test map as a .glb, for tools/custom_model_test.py and for trying the editor's Import model.

    python3 tools/make_test_model.py OUT.glb [--cells N]

A hilly square of ground with a checkerboard texture (256 by 256 pixels, so the importer has to shrink it), and a
red block without a texture on a rotated, scaled child node. The ground is N by N cells of two triangles.
"""
import argparse, base64, io, math
import numpy as np
import PIL.Image
import pygltflib as gl


def build(cells=40, size=1200.0):
    xs = np.linspace(-size / 2, size / 2, cells + 1)
    gx, gz = np.meshgrid(xs, xs)
    gy = 60 * np.sin(gx / 150) * np.cos(gz / 190)
    pos = np.stack([gx, gy, gz], -1).reshape(-1, 3)
    uv = np.stack(np.meshgrid(np.linspace(0, 8, cells + 1), np.linspace(0, 8, cells + 1)), -1).reshape(-1, 2)
    idx = []
    for r in range(cells):
        for c in range(cells):
            a, b, d, e = r * (cells + 1) + c, r * (cells + 1) + c + 1, (r + 1) * (cells + 1) + c, (r + 1) * (cells + 1) + c + 1
            idx += [a, d, b, b, d, e]
    ground = (pos, uv, np.array(idx))
    # A unit cube, 24 vertices
    cube_p, cube_i = [], []
    for axis in range(3):
        for sign in (-1, 1):
            n = np.zeros(3); n[axis] = sign
            u, v = np.roll(np.eye(3), 1, 0)[axis], np.roll(np.eye(3), 2, 0)[axis]
            base = len(cube_p)
            for du, dv in ((-1, -1), (1, -1), (1, 1), (-1, 1)):
                cube_p.append(n + du * u + dv * v)
            q = [base, base + 1, base + 2, base, base + 2, base + 3]
            cube_i += q if sign > 0 else q[::-1]
    cube = (np.array(cube_p) * 0.5, np.zeros((24, 2)), np.array(cube_i))

    g = gl.GLTF2(asset=gl.Asset(version="2.0"), scene=0)
    blob = bytearray()

    def add(arr, kind, ctype=gl.FLOAT, target=gl.ARRAY_BUFFER):
        nonlocal blob
        while len(blob) % 4:
            blob.append(0)
        data = arr.astype("<f4" if ctype == gl.FLOAT else "<u4").tobytes()
        g.bufferViews.append(gl.BufferView(buffer=0, byteOffset=len(blob), byteLength=len(data), target=target))
        blob += data
        acc = gl.Accessor(bufferView=len(g.bufferViews) - 1, componentType=ctype, count=len(arr), type=kind)
        if kind == gl.VEC3:
            acc.min, acc.max = arr.min(0).tolist(), arr.max(0).tolist()
        g.accessors.append(acc)
        return len(g.accessors) - 1

    checker = PIL.Image.new("RGB", (256, 256))
    px = checker.load()
    for y in range(256):
        for x in range(256):
            px[x, y] = (230, 200, 90) if (x // 32 + y // 32) % 2 else (60, 140, 70)
    png = io.BytesIO(); checker.save(png, "PNG")
    while len(blob) % 4:
        blob.append(0)
    g.bufferViews.append(gl.BufferView(buffer=0, byteOffset=len(blob), byteLength=len(png.getvalue())))
    blob += png.getvalue()
    g.images.append(gl.Image(bufferView=len(g.bufferViews) - 1, mimeType="image/png"))
    g.samplers.append(gl.Sampler(wrapS=gl.REPEAT, wrapT=gl.REPEAT))
    g.textures.append(gl.Texture(source=0, sampler=0))
    g.materials.append(gl.Material(name="ground", pbrMetallicRoughness=gl.PbrMetallicRoughness(baseColorTexture=gl.TextureInfo(index=0))))
    g.materials.append(gl.Material(name="block", pbrMetallicRoughness=gl.PbrMetallicRoughness(baseColorFactor=[0.9, 0.2, 0.15, 1])))

    for (p, t, i), mat, name in ((ground, 0, "ground"), (cube, 1, "block")):
        g.meshes.append(gl.Mesh(name=name, primitives=[gl.Primitive(
            attributes=gl.Attributes(POSITION=add(p, gl.VEC3), TEXCOORD_0=add(t, gl.VEC2)),
            indices=add(i, gl.SCALAR, gl.UNSIGNED_INT, gl.ELEMENT_ARRAY_BUFFER), material=mat)]))
    q = math.sin(math.pi / 8)
    g.nodes.append(gl.Node(name="ground", mesh=0, children=[1]))
    g.nodes.append(gl.Node(name="block", mesh=1, translation=[0, 150, 0], rotation=[0, q, 0, math.cos(math.pi / 8)], scale=[200, 160, 120]))
    g.scenes.append(gl.Scene(nodes=[0]))
    g.buffers.append(gl.Buffer(byteLength=len(blob)))
    g.set_binary_blob(bytes(blob))
    return b"".join(g.save_to_bytes())


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("out")
    ap.add_argument("--cells", type=int, default=40)
    a = ap.parse_args()
    open(a.out, "wb").write(build(a.cells))
