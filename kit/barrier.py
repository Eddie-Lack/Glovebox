"""Barriers for a doorway or a tunnel, and a clump of thorns: solid models (the game builds their collision from the triangles) that are
closed behind their bars, so nothing passes between them.

    gate_atl    a rusted cage of bones: seven bone bars and three iron crossbars over a black wall of glowing red runes, thorn vines wound through
                (140 wide, 112 high, 20 thick; the front is +z).
    gate_cave   a stitched curtain of dark flesh with eyes behind a net of black thorn vines (150 wide, 114 high, 20 thick; the front is +z).
    thorns      a clump of black thorns, 40 across and 34 high.

The core of each gate is a closed box (12 triangles); the bars, vines and thorns stand on its front face and only add to it.
"""
import math, random

import numpy as np
import PIL.Image
import PIL.ImageDraw

import meshkit as mk

NAME = 'barrier'


# ---------------------------------------------------------------------------------------------
# Textures

def bark(seed):
    """Black thorn bark with dark red veins (16x16)."""
    rnd = random.Random(seed)
    im = mk.noise(16, 16, rnd, -10, 12, (40, 30, 42))
    d = PIL.ImageDraw.Draw(im)
    for _ in range(4):
        x = rnd.randrange(16)
        for y in range(16):
            x = max(0, min(15, x + rnd.choice((-1, 0, 0, 1))))
            d.point((x, y), fill=(96, 20, 22, 255))
    return im


def bone(seed):
    rnd = random.Random(seed)
    im = mk.noise(16, 32, rnd, -22, 12, (196, 188, 160))
    d = PIL.ImageDraw.Draw(im)
    for _ in range(5):                                   # cracks and brown stains
        x, y = rnd.randrange(16), rnd.randrange(32)
        d.line([(x, y), (x + rnd.randint(-2, 2), y + rnd.randint(3, 9))], fill=(96, 78, 56, 255))
    return im


def rust(seed):
    rnd = random.Random(seed)
    im = mk.noise(16, 16, rnd, -24, 20, (116, 66, 40))
    d = PIL.ImageDraw.Draw(im)
    for _ in range(10):
        x, y = rnd.randrange(16), rnd.randrange(16)
        d.point((x, y), fill=(60, 30, 22, 255))
    return im


def runes(seed=5):
    """A black wall with glowing red signs in rows (64x32), a ring with a slit eye in the middle."""
    rnd = random.Random(seed)
    im = PIL.Image.new('RGBA', (64, 32), (12, 8, 16, 255))
    d = PIL.ImageDraw.Draw(im)
    for _ in range(90):                                  # soot
        x, y = rnd.randrange(64), rnd.randrange(32)
        c = rnd.randint(0, 22)
        d.point((x, y), fill=(12 + c, 8 + c // 2, 16 + c, 255))
    red, glow = (255, 48, 36, 255), (120, 16, 18, 255)

    def glyph(x0, y0):
        pts = [(x0 + rnd.randrange(0, 4) * 1.4, y0 + rnd.randrange(0, 4) * 1.6) for _ in range(rnd.randint(3, 5))]
        d.line(pts, fill=glow, width=2)
        d.line(pts, fill=red, width=1)
    for row in range(3):
        for col in range(8):
            x0, y0 = 2 + col * 7.8, 2 + row * 10.5
            if 22 < x0 < 40 and row == 1:
                continue
            glyph(x0, y0)
    d.ellipse([22, 9, 41, 23], outline=glow, width=2)    # the ring
    d.ellipse([23, 10, 40, 22], outline=red)
    d.ellipse([27, 12, 36, 20], fill=(224, 214, 160, 255))
    d.ellipse([30, 11, 33, 21], fill=(8, 2, 2, 255))     # the slit pupil
    d.rectangle([0, 0, 63, 0], fill=red)
    d.rectangle([0, 31, 63, 31], fill=red)
    return im


def curtain(seed=7):
    """Stitched patches of dark flesh with eyes (64x32)."""
    rnd = random.Random(seed)
    im = PIL.Image.new('RGBA', (64, 32), (50, 20, 26, 255))
    d = PIL.ImageDraw.Draw(im)
    for _ in range(5):                                   # patches of different cloth
        x, y = rnd.randrange(0, 56), rnd.randrange(0, 24)
        c = rnd.choice(((64, 28, 34), (38, 22, 34), (70, 44, 40), (30, 24, 28)))
        d.rectangle([x, y, x + rnd.randint(10, 24), y + rnd.randint(8, 16)], fill=c + (255,))
    thread = (10, 6, 8, 255)
    for x in (16, 33, 49):                               # seams with crossing stitches
        d.line([(x, 0), (x + rnd.randint(-1, 1), 31)], fill=thread)
        for y in range(1, 31, 3):
            d.line([(x - 2, y), (x + 2, y + 2)], fill=thread)
    for y in (10, 22):
        d.line([(0, y), (63, y)], fill=thread)
    for cx, cy in ((9, 6), (25, 16), (41, 5), (56, 14), (8, 26), (33, 27), (51, 26)):
        d.ellipse([cx - 5, cy - 3, cx + 5, cy + 3], fill=(226, 218, 190, 255), outline=(60, 10, 12, 255))
        d.ellipse([cx - 2, cy - 2, cx + 2, cy + 2], fill=(190, 40, 24, 255))
        d.line([(cx, cy - 3), (cx, cy + 3)], fill=(6, 2, 2, 255))
        d.point((cx - 3, cy - 1), fill=(160, 20, 20, 255))
    return im


# ---------------------------------------------------------------------------------------------
# Pieces

def thorn(s, mat, base, direction, length, radius):
    """A three sided thorn: its base middle, its direction, length and base radius. The faces look outward."""
    base = np.array(base, dtype=np.float64)
    dn = mk.norm(np.array(direction, dtype=np.float64))
    a = mk.norm(np.cross(dn, mk.v3(0, 1, 0))) if abs(dn[1]) < 0.95 else mk.v3(1, 0, 0)
    b = np.cross(dn, a)
    ring = [base + radius * (math.cos(2 * math.pi * k / 3) * a + math.sin(2 * math.pi * k / 3) * b) for k in range(3)]
    apex = base + dn * length
    for k in range(3):
        p, q = ring[k], ring[(k + 1) % 3]
        mid = (p + q + apex) / 3
        out = mid - base
        if np.dot(np.cross(q - p, apex - p), out) < 0:
            p, q = q, p
        s.tri(p, q, apex, (0, 1), (1, 1), (0.5, 0), mat)


def vine(s, mat, tmat, rnd, pts, r0, r1, sides=4, thorns=0, thorn_len=8.0, face=(0, 0, 1)):
    """A thorny vine along a polyline, its radius going from r0 to r1, with `thorns` thorns on the way (pointing away from the line, mostly to +face)."""
    n = len(pts)
    radii = [r0 + (r1 - r0) * i / (n - 1) for i in range(n)]
    mk.tube(s, pts, radii, sides, mat, v_scale=2.0, cap_end=True, cap_start=True)
    for i in range(thorns):
        t = (i + rnd.uniform(0.2, 0.8)) / thorns * (n - 1)
        k = min(int(t), n - 2)
        f = t - k
        p = np.array(pts[k], dtype=np.float64) * (1 - f) + np.array(pts[k + 1], dtype=np.float64) * f
        side = rnd.choice((-1, 1))
        d = np.array(face, dtype=np.float64) * 0.7 + np.array([rnd.uniform(-0.7, 0.7), rnd.uniform(-0.7, 0.7), 0.0]) * side
        thorn(s, tmat, p, d, thorn_len * rnd.uniform(0.7, 1.2), 2.2)


def core(s, lo, hi, front, side):
    """The closed box behind everything: the front picture on +z, a plain material on the other five faces."""
    mk.box(s, lo, hi, [side, side, side, side, front, side], uvs={front: mk.UPRIGHT})


# ---------------------------------------------------------------------------------------------
# The gates

def build_gate_atl(seed=11):
    rnd = random.Random(seed)
    s = mk.Soup()
    W, H, T = 70.0, 112.0, 10.0
    front = s.material(runes(), clamp=True, bright=1.0)
    side = s.material(bark(seed))
    bn = s.material(bone(seed + 1))
    ir = s.material(rust(seed + 2))
    core(s, (-W, 0, -T), (W, H, T), front, side)
    zf = T + 4.0
    for i in range(7):                                   # bone bars
        x = -60.0 + 20.0 * i + rnd.uniform(-1.5, 1.5)
        pts = [(x, -2, zf), (x + rnd.uniform(-1, 1), H * 0.5, zf + rnd.uniform(-1, 1)), (x + rnd.uniform(-1.5, 1.5), H + 2, zf)]
        mk.tube(s, pts, [3.6, 5.0, 3.4], 5, bn, v_scale=2.0, cap_end=True, cap_start=True)
        mk.tube(s, [(x, H, zf), (x, H + 3, zf)], [6.0, 4.0], 5, bn, cap_end=True)         # the knob on the top
    for y in (14.0, 56.0, 98.0):                         # iron crossbars
        mk.box(s, (-W - 3, y - 3, T), (W + 3, y + 3, T + 4.0), ir)
    for k, y0 in enumerate((22.0, 62.0, 90.0)):          # thorn vines wound through the bars
        pts = []
        for j in range(9):
            x = -W + 2 + (2 * W - 4) * j / 8.0
            pts.append((x, y0 + 14.0 * math.sin(j * 1.3 + k * 2.0), zf + (9.0 if j % 2 == 0 else -2.0)))
        vine(s, side, ir, rnd, pts, 2.8, 2.0, 4, thorns=8, thorn_len=9.0)
    return s


def build_gate_cave(seed=21):
    rnd = random.Random(seed)
    s = mk.Soup()
    W, H, T = 75.0, 114.0, 10.0
    front = s.material(curtain(), clamp=True, bright=1.0)
    side = s.material(bark(seed))
    ir = s.material(rust(seed + 2))
    core(s, (-W, 0, -T), (W, H, T), front, side)
    zf = T + 3.0
    for k in range(6):                                   # a net of black vines crossing the curtain
        x0 = -W + 12.0 + k * 26.0
        sgn = 1 if k % 2 == 0 else -1
        pts = []
        for j in range(8):
            f = j / 7.0
            pts.append((x0 + sgn * 34.0 * (f - 0.5) + 6.0 * math.sin(j * 1.7 + k), 4.0 + (H - 8.0) * f, zf + (5.0 if j % 2 == 0 else -1.5)))
        vine(s, side, ir, rnd, pts, 3.0, 2.2, 4, thorns=7, thorn_len=9.0)
    for k, y in enumerate((18.0, 60.0, 100.0)):          # three that run across
        pts = [(-W + 2 + (2 * W - 4) * j / 6.0, y + 8.0 * math.sin(j * 1.9 + k * 2.3), zf + (6.0 if j % 2 else 0.0)) for j in range(7)]
        vine(s, side, ir, rnd, pts, 3.2, 2.4, 4, thorns=8, thorn_len=10.0)
    return s


def build_thorns(seed=31):
    """A clump of black thorns for the spike platform: 40 across, 34 high; the origin is on the ground."""
    rnd = random.Random(seed)
    s = mk.Soup()
    bk = s.material(bark(seed))
    for k in range(8):
        a = 2 * math.pi * k / 8 + rnd.uniform(-0.3, 0.3)
        r = 0.0 if k == 0 else rnd.uniform(6.0, 15.0)
        lean = 0.0 if k == 0 else rnd.uniform(0.25, 0.7)
        h = 34.0 if k == 0 else rnd.uniform(18.0, 30.0)
        d = (math.cos(a) * lean, 1.0, math.sin(a) * lean)
        thorn(s, bk, (r * math.cos(a), 0.0, r * math.sin(a)), d, h, rnd.uniform(3.5, 5.0))
    return s
