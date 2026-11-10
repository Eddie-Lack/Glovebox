"""A cobweb: a flat disc of thread on a clear background (the texel is opaque or clear). Both sides are drawn. 40 across."""
import math, random

import PIL.Image
import PIL.ImageDraw

import meshkit as mk

NAME = 'web'


def tex(seed=1):
    rnd = random.Random(seed)
    im = PIL.Image.new('RGBA', (32, 32), (0, 0, 0, 0))
    d = PIL.ImageDraw.Draw(im)
    white = (214, 214, 220, 255)
    spokes = 9
    for k in range(spokes):
        a = 2 * math.pi * k / spokes + rnd.uniform(-0.1, 0.1)
        d.line([(15.5, 15.5), (15.5 + 15.5 * math.cos(a), 15.5 + 15.5 * math.sin(a))], fill=white)
    for r in (4, 7, 10, 13):
        pts = []
        for k in range(spokes + 1):
            a = 2 * math.pi * k / spokes
            rr = r * rnd.uniform(0.85, 1.0)
            pts.append((15.5 + rr * math.cos(a), 15.5 + rr * math.sin(a)))
        d.line(pts, fill=white)
    return im


def build():
    s = mk.Soup()
    m = s.material(tex(), clamp=True, bright=1.0)
    h = 20.0
    a, b, c, d = mk.v3(-h, -h, 0), mk.v3(h, -h, 0), mk.v3(h, h, 0), mk.v3(-h, h, 0)
    s.quad(a, b, c, d, (0, 1), (1, 1), (1, 0), (0, 0), m)
    s.quad(b, a, d, c, (1, 1), (0, 1), (0, 0), (1, 0), m, mk.v3(0, 0, -1), mk.v3(0, 0, -1), mk.v3(0, 0, -1), mk.v3(0, 0, -1))
    return s
