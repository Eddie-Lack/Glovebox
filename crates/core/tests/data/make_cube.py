#!/usr/bin/env python3
"""Writes cube.gltf: a cube drawn from scratch, for tests.

The cube has 6 faces of 2 triangles each, with its own normals and texture
coordinates. Its one texture is an 8 by 8 checker of two colors, drawn here.
The buffer and the PNG are data URIs inside the file. Nothing comes from the
game. Run it with no arguments; it writes cube.gltf next to itself.
"""

import base64
import json
import os
import struct
import zlib

# Half the edge. It is not a round number, and it is large enough to keep its
# size in the display list, which stores whole units.
HALF = 21.3


def png(width, height, rows):
    """A PNG of 8-bit RGBA rows, with no filter."""
    raw = b"".join(b"\x00" + bytes(row) for row in rows)

    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)

    header = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(raw, 9))
            + chunk(b"IEND", b""))


def checker():
    light, dark = (240, 200, 40, 255), (40, 90, 200, 255)
    rows = []
    for y in range(8):
        row = []
        for x in range(8):
            row.extend(light if (x // 2 + y // 2) % 2 == 0 else dark)
        rows.append(row)
    return png(8, 8, rows)


def cube():
    # Each face: its normal, and two axes that span it. The corners go counter-clockwise seen from outside.
    faces = [
        ((1, 0, 0), (0, 0, -1), (0, 1, 0)),
        ((-1, 0, 0), (0, 0, 1), (0, 1, 0)),
        ((0, 1, 0), (1, 0, 0), (0, 0, -1)),
        ((0, -1, 0), (1, 0, 0), (0, 0, 1)),
        ((0, 0, 1), (1, 0, 0), (0, 1, 0)),
        ((0, 0, -1), (-1, 0, 0), (0, 1, 0)),
    ]
    pos, nrm, uv, idx = [], [], [], []
    for n, u, v in faces:
        base = len(pos)
        for su, sv, tu, tv in ((-1, -1, 0, 1), (1, -1, 1, 1), (1, 1, 1, 0), (-1, 1, 0, 0)):
            pos.append(tuple(HALF * (n[a] + su * u[a] + sv * v[a]) for a in range(3)))
            nrm.append(n)
            uv.append((tu, tv))
        idx += [base, base + 1, base + 2, base, base + 2, base + 3]
    return pos, nrm, uv, idx


def main():
    pos, nrm, uv, idx = cube()
    blob = b"".join(struct.pack("<3f", *p) for p in pos)
    blob += b"".join(struct.pack("<3f", *n) for n in nrm)
    blob += b"".join(struct.pack("<2f", *t) for t in uv)
    blob += b"".join(struct.pack("<H", i) for i in idx)
    n = len(pos)
    views = [
        {"buffer": 0, "byteOffset": 0, "byteLength": 12 * n, "target": 34962},
        {"buffer": 0, "byteOffset": 12 * n, "byteLength": 12 * n, "target": 34962},
        {"buffer": 0, "byteOffset": 24 * n, "byteLength": 8 * n, "target": 34962},
        {"buffer": 0, "byteOffset": 32 * n, "byteLength": 2 * len(idx), "target": 34963},
    ]
    doc = {
        "asset": {"version": "2.0", "generator": "make_cube.py"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"name": "cube", "mesh": 0}],
        "meshes": [{"name": "cube", "primitives": [{
            "attributes": {"POSITION": 0, "NORMAL": 1, "TEXCOORD_0": 2},
            "indices": 3,
            "material": 0,
        }]}],
        "materials": [{"name": "checker", "pbrMetallicRoughness": {"baseColorTexture": {"index": 0}}}],
        "textures": [{"source": 0, "sampler": 0}],
        "samplers": [{"magFilter": 9728, "minFilter": 9728, "wrapS": 10497, "wrapT": 10497}],
        "images": [{"uri": "data:image/png;base64," + base64.b64encode(checker()).decode()}],
        "buffers": [{"byteLength": len(blob),
                     "uri": "data:application/octet-stream;base64," + base64.b64encode(blob).decode()}],
        "bufferViews": views,
        "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": n, "type": "VEC3",
             "min": [-HALF] * 3, "max": [HALF] * 3},
            {"bufferView": 1, "componentType": 5126, "count": n, "type": "VEC3"},
            {"bufferView": 2, "componentType": 5126, "count": n, "type": "VEC2"},
            {"bufferView": 3, "componentType": 5123, "count": len(idx), "type": "SCALAR"},
        ],
    }
    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "cube.gltf")
    with open(out, "w", newline="\n") as f:
        json.dump(doc, f, indent=1)
        f.write("\n")


if __name__ == "__main__":
    main()
