"""Three candles, melted unevenly, with flames and a pool of wax (about 90 tris). The flames are bright and flat."""
import math, random

import PIL.ImageDraw

import meshkit as mk

NAME = 'candles'


def wax():
    rnd = random.Random(21)
    return mk.noise(8, 16, rnd, -12, 8, (214, 200, 160))


def flame():
    im = mk.canvas(8, 16, (255, 190, 40, 255))
    d = PIL.ImageDraw.Draw(im)
    d.rectangle([2, 4, 5, 15], fill=(255, 235, 110, 255))
    d.rectangle([3, 9, 4, 15], fill=(255, 255, 220, 255))
    return im


def build():
    s = mk.Soup()
    w = s.material(wax(), bright=1.0)
    f = s.material(flame(), bright=1.0)
    pool = s.material(mk.noise(8, 8, random.Random(5), -8, 6, (200, 188, 150)))
    mk.lathe(s, [(0, 0.2), (13, 0.2), (0, 1.8)], 6, pool)
    for x, z, h, r in ((-6, 2, 16, 3.2), (5, -3, 11, 3.0), (3, 7, 7, 2.6)):
        p = mk.Soup()
        mk.lathe(p, [(r + 1.5, 0), (r, 5), (r + 0.6, h * 0.7), (r - 0.2, h), (0, h - 0.8)], 5, w)
        s.add(p, offset=(x, 1.0, z), shared=True)
        fl = mk.Soup()
        mk.lathe(fl, [(0, 0), (1.5, 2.5), (0, 7.5)], 4, f)
        s.add(fl, offset=(x, h + 0.6, z), shared=True)
    return s
