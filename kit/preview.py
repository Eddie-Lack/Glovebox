"""A quick software preview of the generated models: python3 preview.py OUT.png [name ...]. Flat shaded triangles in painter's order,
coloured by the texture at each triangle's middle. For a look at the shapes, not for the game's lighting."""
import math, sys

import numpy as np
import PIL.Image
import PIL.ImageDraw

import meshkit as mk


def render(soup, size=220, yaw=0.6, pitch=0.25):
    lo, hi = soup.bounds()
    c = (lo + hi) / 2
    r = float(np.linalg.norm(hi - lo)) / 2 or 1.0
    cy, sy, cp, sp = math.cos(yaw), math.sin(yaw), math.cos(pitch), math.sin(pitch)
    ry = np.array([[cy, 0, sy], [0, 1, 0], [-sy, 0, cy]])
    rx = np.array([[1, 0, 0], [0, cp, -sp], [0, sp, cp]])
    R = rx @ ry
    im = PIL.Image.new('RGB', (size, size), (24, 22, 30))
    d = PIL.ImageDraw.Draw(im)
    items = []
    for p, n, uv, m in soup.tris:
        q = (p - c) @ R.T
        nn = n.mean(0) @ R.T
        fn = np.cross(p[1] - p[0], p[2] - p[0]) @ R.T
        if fn[2] > 0:
            continue                      # back face (the camera looks along +z, faces toward the camera have normal -z)
        tex = soup.mats[m]
        u, v = uv.mean(0)
        w, h = tex.size
        px = tex.getpixel((int(min(max(u, 0), 0.999) * w), int(min(max(v, 0), 0.999) * h)))
        light = 0.35 + 0.65 * max(0.0, -nn[2] / (np.linalg.norm(nn) or 1))
        col = tuple(int(min(255, px[i] * light)) for i in range(3))
        if px[3] < 128:
            continue
        items.append((q[:, 2].mean(), q, col))
    for _, q, col in sorted(items, key=lambda t: -t[0]):
        pts = [(size / 2 + x / r * size * 0.45, size / 2 - y / r * size * 0.45) for x, y, z in q]
        d.polygon(pts, fill=col)
    return im


if __name__ == '__main__':
    import importlib, os
    sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))))
    from props import MODEL_MODULES
    out = sys.argv[1]
    want = sys.argv[2:]
    shots = []
    for mod in MODEL_MODULES:
        m = importlib.import_module(mod)
        for name, make in m.variants().items():
            if want and name not in want:
                continue
            s = make()
            shots.append((name, render(s), render(s, yaw=3.2, pitch=0.2)))
    cols = 4
    rows = (len(shots) + cols - 1) // cols
    sheet = PIL.Image.new('RGB', (cols * 220, rows * 450), (0, 0, 0))
    for i, (name, a, b) in enumerate(shots):
        x, y = (i % cols) * 220, (i // cols) * 450
        sheet.paste(a, (x, y))
        sheet.paste(b, (x, y + 225))
        PIL.ImageDraw.Draw(sheet).text((x + 4, y + 4), name, fill=(255, 255, 255))
    sheet.save(out)
