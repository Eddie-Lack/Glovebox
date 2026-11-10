"""Dead trees: a bent trunk and bare branches (a few hundred tris), grey-brown bark. Two shapes."""
import math, random

import numpy as np
import PIL.ImageDraw

import meshkit as mk

NAME = 'tree'


def bark(seed):
    rnd = random.Random(seed)
    im = mk.noise(16, 32, rnd, -16, 12, (62, 54, 48))
    d = PIL.ImageDraw.Draw(im)
    for x in range(0, 16, 3):
        d.line([(x + rnd.randint(0, 1), 0), (x + rnd.randint(0, 2), 31)], fill=(30, 26, 24, 255))
    return im


def branch(s, mat, start, direction, length, radius, depth, rnd, sides=3):
    """One branch as a three-segment tube that curls, then its children."""
    d = mk.norm(np.array(direction, dtype=np.float64))
    pts, p = [np.array(start, dtype=np.float64)], np.array(start, dtype=np.float64)
    seg = length / 3.0
    for i in range(3):
        d = mk.norm(d + np.array([rnd.uniform(-0.35, 0.35), rnd.uniform(-0.1, 0.25), rnd.uniform(-0.35, 0.35)]))
        p = p + d * seg
        pts.append(p.copy())
    rad = [radius, radius * 0.78, radius * 0.55, max(radius * 0.25, 0.6)]
    mk.tube(s, pts, rad, sides, mat, v_scale=max(length / 40.0, 0.5))
    if depth > 0:
        n = 2 if depth > 1 else 2
        for k in range(n):
            i = rnd.choice((1, 2))
            ang = rnd.uniform(0, 2 * math.pi)
            side = np.array([math.cos(ang), rnd.uniform(0.5, 1.4), math.sin(ang)])
            branch(s, mat, pts[i], mk.norm(d * 0.5 + side), length * 0.62, rad[i] * 0.7, depth - 1, rnd, sides)
    return pts


def build(seed=1, height=140.0, crooked=0.3):
    rnd = random.Random(seed)
    s = mk.Soup()
    m = s.material(bark(seed))
    # a gnarled root flare and a bent trunk
    pts, p = [np.array([0.0, -6.0, 0.0])], np.array([0.0, 0.0, 0.0])
    lean = np.array([rnd.uniform(-1, 1), 0, rnd.uniform(-1, 1)]) * crooked
    for i in range(1, 5):
        p = p + np.array([0, height / 4, 0]) + lean * (height / 4) * (rnd.uniform(0.4, 1.0))
        pts.append(p.copy())
    pts[0] = np.array([0.0, -6.0, 0.0])
    mk.tube(s, pts, [13.0, 8.5, 6.5, 5.0, 3.2], 5, m, v_scale=3.0)
    # roots
    for k in range(2):
        a = rnd.uniform(0, 2 * math.pi) + k * 3.0
        mk.tube(s, [(0, 8, 0), (math.cos(a) * 14, 3, math.sin(a) * 14), (math.cos(a) * 30, -2, math.sin(a) * 30)], [8, 5, 2], 4, m)
    top = pts[-1]
    for k, (hf, depth) in enumerate(((0.5, 0), (0.8, 1), (1.0, 1), (0.65, 0))):
        a = 2 * math.pi * (k / 5.0) + rnd.uniform(-0.3, 0.3)
        base = pts[min(4, 1 + int(hf * 3))] if hf < 1.0 else top
        branch(s, m, base, (math.cos(a), rnd.uniform(0.3, 0.9), math.sin(a)), height * rnd.uniform(0.38, 0.55), 4.0, depth, rnd)
    return s
