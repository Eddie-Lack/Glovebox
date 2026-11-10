"""A broken fence: leaning posts, two rails, some planks hanging by a nail (about 150 tris). 150 long along x."""
import math, random

import meshkit as mk

NAME = 'fence'


def build(seed=2, length=150.0):
    rnd = random.Random(seed)
    s = mk.Soup()
    w = s.material(mk.wood(rnd, 16, 32))
    posts = 4
    xs = [-length / 2 + length * i / (posts - 1) for i in range(posts)]
    for i, x in enumerate(xs):
        p = mk.Soup()
        h = rnd.uniform(40, 58)
        mk.box(p, (-3, 0, -3), (3, h, 3), w)
        s.add(p, offset=(x, -4, 0), yaw=rnd.uniform(-0.3, 0.3), tilt=(rnd.uniform(-0.12, 0.12), rnd.uniform(-0.18, 0.18)), shared=True)
    for y, wob in ((18, 0.02), (36, -0.05)):
        for i in range(posts - 1):
            if rnd.random() < 0.25:
                continue                                    # a missing rail
            r = mk.Soup()
            L = xs[i + 1] - xs[i]
            mk.box(r, (-L / 2, -2, -1.2), (L / 2, 2, 1.2), w)
            s.add(r, offset=((xs[i] + xs[i + 1]) / 2, y + rnd.uniform(-3, 3), 4), tilt=(0.0, wob + rnd.uniform(-0.1, 0.1)), shared=True)
    for i in range(7):                                      # pickets
        x = -length / 2 + 8 + i * (length - 16) / 6
        if rnd.random() < 0.28:
            continue
        pk = mk.Soup()
        h = rnd.uniform(28, 46)
        mk.box(pk, (-3, 0, -0.8), (3, h, 0.8), w)
        mk.prism(pk, [(-3, 0.8), (3, 0.8), (0, 0.8)], h, h + 1.0, w, w)
        s.add(pk, offset=(x, 4 + rnd.uniform(-4, 6), 6), tilt=(0.0, rnd.uniform(-0.25, 0.25)), shared=True)
    return s
