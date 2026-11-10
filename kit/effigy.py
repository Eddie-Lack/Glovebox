"""A giant stitched rag-doll Glover on a post, 330 tall: a patchwork glove with button eyes and a sewn grin, fingers hanging, tied to a
cross. About 330 tris. The face looks along +z; origin at the foot of the post."""
import math, random

import numpy as np
import PIL.Image
import PIL.ImageDraw

import meshkit as mk

NAME = 'effigy'


def patch(seed, base=(150, 142, 120)):
    rnd = random.Random(seed)
    im = mk.noise(32, 32, rnd, -14, 12, base)
    d = PIL.ImageDraw.Draw(im)
    cols = [(120, 70, 60), (90, 96, 110), (140, 128, 90)]
    for i in range(3):
        x0, y0 = rnd.randrange(0, 20), rnd.randrange(0, 20)
        d.rectangle([x0, y0, x0 + rnd.randrange(8, 14), y0 + rnd.randrange(8, 14)], fill=cols[i] + (255,))
        for k in range(0, 14, 2):                          # stitches round the patch
            d.point((x0 + k, y0), fill=(40, 30, 28, 255))
    for y in range(0, 32, 4):
        d.line([(0, y), (2, y + 1)], fill=(40, 30, 28, 255))
    return im


def face():
    im = mk.noise(64, 32, random.Random(31), -10, 10, (168, 158, 134))
    d = PIL.ImageDraw.Draw(im)
    dark, thread = (24, 18, 18, 255), (50, 38, 34, 255)
    d.rectangle([0, 0, 63, 31], outline=thread)
    for x in range(1, 63, 3):                               # stitched seam along the top and the bottom
        d.line([(x, 1), (x + 1, 2)], fill=thread)
    # eyes: two black buttons crossed with thread; the right one hangs low and is bigger
    d.ellipse([11, 6, 25, 20], fill=dark)
    d.line([(13, 8), (23, 18)], fill=(200, 190, 160, 255))
    d.line([(23, 8), (13, 18)], fill=(200, 190, 160, 255))
    d.ellipse([38, 9, 56, 27], fill=dark)
    d.ellipse([44, 14, 50, 22], fill=(190, 30, 20, 255))    # a red pupil in the button
    d.line([(36, 4), (38, 12)], fill=thread)                # the loose thread
    # a wide sewn smile
    pts = [(6, 26), (14, 29), (24, 27), (34, 30), (44, 29), (54, 26), (60, 22)]
    d.line(pts, fill=dark, width=1)
    for (x, y) in pts[1:-1]:
        d.line([(x, y - 3), (x, y + 2)], fill=dark)
    for x in range(8, 58, 6):
        d.line([(x, 28), (x + 2, 31)], fill=dark)
    for y in range(20, 31):                                  # a dark stain running from the eye
        d.point((14, y), fill=(60, 30, 26, 255))
    return im


def build():
    s = mk.Soup()
    rnd = random.Random(8)
    wd = s.material(mk.wood(rnd, 16, 32, (56, 42, 32)))
    fc = s.material(face(), clamp=True)
    pa = s.material(patch(1))
    pb = s.material(patch(2, (140, 132, 110)))
    rope = s.material(mk.noise(8, 16, random.Random(2), -14, 10, (110, 90, 60)))
    mk.box(s, (-9, -8, -9), (9, 330, 9), wd)
    mk.box(s, (-90, 196, -8), (90, 214, 8), wd)
    # the body: the upper half of the palm carries the face, the lower half is patchwork
    cx, cy, cz = 0.0, 232.0, 14.0
    W, Hh, T = 46.0, 52.0, 13.0
    mk.box(s, (cx - W, cy - Hh, cz - T), (cx + W, cy, cz + T), [pa, pa, None, pa, pb, pa])
    mk.box(s, (cx - W, cy, cz - T), (cx + W, cy + Hh, cz + T), [pa, pa, pa, None, fc, pb], uvs={fc: mk.UPRIGHT})
    # the cuff
    mk.lathe(s, [(0, 0), (48, 0), (50, 4), (44, 28), (40, 30)], 8, pb)
    # fingers hang limp from the top edge of the palm (y = cy + Hh)
    top = cy + Hh
    for i, (x, L, bend) in enumerate(((-33, 62, -0.5), (-11, 74, 0.2), (11, 70, -0.1), (33, 56, 0.6))):
        pts = [(x, top - 2, cz), (x + bend * 14, top + L * 0.55, cz + 3), (x + bend * 36, top + L * 0.9, cz + 14), (x + bend * 46, top + L * 0.9 - 10, cz + 22)]
        mk.tube(s, pts, [11.5, 10.5, 9.5, 8.0], 5, pa)
    mk.tube(s, [(W - 3, cy + 8, cz), (W + 18, cy + 18, cz + 3), (W + 36, cy - 4, cz + 8), (W + 38, cy - 28, cz + 12)], [11, 10, 9, 7], 5, pb)  # the thumb
    # ropes tying the doll to the cross
    for x in (-60, 60):
        mk.tube(s, [(x, 218, cz + 12), (x * 0.8, 204, cz + 6), (x * 0.9, 196, cz + 2)], [2.2, 2.2, 2.2], 4, rope)
    # a rope round the neck of the post and a hanging loop
    mk.tube(s, [(0, 280, 11), (4, 268, 12), (0, 256, 11)], [2.2, 2.2, 2.2], 4, rope)
    # straw sticking out of the cuff
    for k in range(6):
        a = 2 * math.pi * k / 6
        mk.tube(s, [(math.cos(a) * 30, 2, cz + math.sin(a) * 6), (math.cos(a) * 36, -10, cz + math.sin(a) * 8)], [1.5, 0.6], 3, pb)
    return s
