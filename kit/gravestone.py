"""Crooked gravestones with engraved words (about 60 tris each). The front picture is 32x64, the back is plain stone."""
import math, random

import numpy as np
import PIL.ImageDraw

import meshkit as mk

NAME = 'gravestone'


def front(lines, seed):
    rnd = random.Random(seed)
    im = mk.stone(rnd, 32, 64, moss=True)
    d = PIL.ImageDraw.Draw(im)
    d.rectangle([2, 2, 29, 61], outline=(70, 76, 72, 255))          # a worn border
    # a crack
    x = 4 + seed % 6
    for y in range(40, 62):
        x += rnd.choice((-1, 0, 1))
        d.point((x, y), fill=(30, 32, 30, 255))
    mk.text_centered(im, lines, 9 if len(lines) > 3 else 14, 10 if len(lines) <= 4 else 9, (30, 32, 30, 255), size=7)
    # blood drips under the words
    for _ in range(3):
        x0 = rnd.randrange(4, 28)
        y0 = rnd.randrange(46, 52)
        for y in range(y0, y0 + rnd.randrange(5, 14)):
            d.point((x0, y), fill=(120, 14, 10, 255))
    return im


def stone_soup(lines, lean, seed):
    s = mk.Soup()
    fr = s.material(front(lines, seed), clamp=True)
    bk = s.material(mk.stone(random.Random(seed + 50), 32, 32))
    W, H, T = 21.0, 40.0, 6.0
    arc = [(W * math.cos(math.pi * k / 7), H + W * 0.8 * math.sin(math.pi * k / 7)) for k in range(0, 8)]
    prof = [(-W, 0.0), (W, 0.0)] + arc          # counter-clockwise seen from the front (+z)
    # the front: a fan over the outline, picture projected from the front
    top = H + W * 0.8
    uv = lambda x, y: ((x + W) / (2 * W), 1.0 - y / top)
    cx, cy = 0.0, H * 0.5
    n = len(prof)
    for i in range(n):
        (x0, y0), (x1, y1) = prof[i], prof[(i + 1) % n]
        s.tri(mk.v3(cx, cy, T / 2), mk.v3(x0, y0, T / 2), mk.v3(x1, y1, T / 2), uv(cx, cy), uv(x0, y0), uv(x1, y1), fr, mk.v3(0, 0, 1), mk.v3(0, 0, 1), mk.v3(0, 0, 1))
        s.tri(mk.v3(cx, cy, -T / 2), mk.v3(x1, y1, -T / 2), mk.v3(x0, y0, -T / 2), (0.5, 0.5), (0.5, 0.5), (0.5, 0.5), bk, mk.v3(0, 0, -1), mk.v3(0, 0, -1), mk.v3(0, 0, -1))
        nrm = mk.norm(mk.v3(y1 - y0, -(x1 - x0), 0))
        s.quad(mk.v3(x0, y0, T / 2), mk.v3(x1, y1, T / 2), mk.v3(x1, y1, -T / 2), mk.v3(x0, y0, -T / 2),
               (0, 0), (1, 0), (1, 1), (0, 1), bk, nrm, nrm, nrm, nrm)
    # a mound of earth at the foot
    soil = s.material(mk.noise(16, 16, random.Random(seed + 9), -10, 10, (44, 34, 26)))
    mk.lathe(s, [(30, -4), (14, 9), (0, 11)], 7, soil)
    out = mk.Soup()
    out.add(s, offset=(0, 0, 0), tilt=(0.0, math.radians(lean)))
    return out
