"""Limp gloves hanging by a rope from a gallows beam, and one single glove on a rope. The glove is a cuff, a palm and five fingers
that droop; its cloth is grey-white with brown stains and a seam."""
import math, random

import numpy as np
import PIL.ImageDraw

import meshkit as mk

NAME = 'gloverope'


def cloth(seed):
    rnd = random.Random(seed)
    im = mk.noise(32, 32, rnd, -16, 10, (176, 170, 158))
    d = PIL.ImageDraw.Draw(im)
    for _ in range(5):                                       # stains
        x, y = rnd.randrange(32), rnd.randrange(32)
        r = rnd.randrange(2, 5)
        d.ellipse([x - r, y - r, x + r, y + r], fill=(110, 76, 52, 255))
    d.line([(16, 0), (16, 31)], fill=(70, 60, 54, 255))        # seam
    for y in range(0, 32, 3):
        d.line([(13, y), (19, y + 1)], fill=(60, 50, 46, 255))
    return im


def rope_tex():
    rnd = random.Random(4)
    im = mk.noise(8, 16, rnd, -14, 10, (120, 98, 64))
    d = PIL.ImageDraw.Draw(im)
    for y in range(0, 16, 3):
        d.line([(0, y), (7, y + 2)], fill=(70, 56, 36, 255))
    return im


def glove(s, mat, seed, limp=1.0):
    """A hanging glove, origin at the top of the cuff, fingers pointing down (-y). About 22 tall wrist to palm, 52 with the fingers."""
    rnd = random.Random(seed)
    mk.lathe(s, [(5.5, 0), (7.5, -1), (8.5, -9), (6.5, -10)], 6, mat, v0=0.0, v1=0.4)       # the cuff
    mk.lathe(s, [(0, 1), (5.5, 0)], 6, mat)                                                  # the wrist's top
    palm = mk.Soup()
    mk.box(palm, (-8.5, -20, -4), (8.5, -9.5, 4), mat)
    s.add(palm, tilt=(0.0, 0.0), shared=True)
    fingers = [(-6.0, 10.5, 0.0), (-2.0, 12.0, 0.0), (2.0, 11.5, 0.0), (6.0, 9.5, 0.0)]
    for i, (x, L, _) in enumerate(fingers):
        sway = rnd.uniform(-0.25, 0.25) * limp
        pts = [(x, -19.5, 0), (x + sway * 3, -19.5 - L * 0.6, 2.0 * limp), (x + sway * 6, -19.5 - L - 8, 6.0 * limp)]
        mk.tube(s, pts, [2.6, 2.3, 1.6], 3, mat, v_scale=1.0)
    mk.tube(s, [(8, -13, 0), (14, -19, 1), (16, -25, 3)], [3.0, 2.6, 2.0], 3, mat)            # the thumb
    return s


def rope_to(s, mat, x, y_top, y_bottom, z=0.0, r=1.4):
    mk.tube(s, [(x, y_top, z), (x + 0.3, (y_top + y_bottom) / 2, z), (x, y_bottom, z)], [r, r, r], 5, mat, v_scale=max((y_top - y_bottom) / 16.0, 1.0), cap_end=True)


def build_single(seed=1, drop=60.0):
    """One glove on a rope. The rope end is at the top (y = drop + 12); the glove's cuff is at y = 0."""
    s = mk.Soup()
    c = s.material(cloth(seed))
    r = s.material(rope_tex())
    glove(s, c, seed)
    rope_to(s, r, 0, drop + 12.0, 0.5)
    return s


def build_gallows(seed=3):
    """A post and a beam with three gloves hanging at different lengths. Origin at the foot of the post; the beam's end is 70 along +x."""
    rnd = random.Random(seed)
    s = mk.Soup()
    wd = s.material(mk.wood(rnd, 16, 32, (58, 42, 30)))
    c = s.material(cloth(seed))
    r = s.material(rope_tex())
    mk.box(s, (-6, -4, -6), (6, 140, 6), wd)
    mk.box(s, (-6, 130, -5), (78, 140, 5), wd)
    brace = mk.Soup()
    mk.box(brace, (-2, -2, -3), (2, 34, 3), wd)
    s.add(brace, offset=(10, 100, 0), tilt=(0.0, -0.7), shared=True)
    for x, drop in ((30.0, 40.0), (60.0, 62.0)):
        g = mk.Soup()
        glove(g, c, rnd.randint(0, 99))
        top = 130.0
        y = top - drop - 12.0
        rope_to(s, r, x, top, y + 0.5)
        s.add(g, offset=(x, y, 0.0), yaw=rnd.uniform(-1, 1), tilt=(rnd.uniform(-0.06, 0.06), rnd.uniform(-0.1, 0.1)), shared=True)
    return s
