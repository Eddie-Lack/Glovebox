"""A coffin with its lid pushed aside: dark wood, a black inside with two red points of light. About 70 long (z), 30 wide."""
import math, random

import PIL.ImageDraw

import meshkit as mk

NAME = 'coffin'

# outline seen from above, in the order of increasing angle (x towards z)
OUTLINE = [(-9, -6), (9, -6), (15, 14), (8, 62), (-8, 62), (-15, 14)]


def inside():
    rnd = random.Random(12)
    im = mk.noise(16, 16, rnd, -4, 3, (10, 8, 10))
    d = PIL.ImageDraw.Draw(im)
    d.point((5, 7), fill=(255, 40, 20, 255))
    d.point((10, 7), fill=(255, 40, 20, 255))
    return im


def lid_tex():
    rnd = random.Random(13)
    im = mk.wood(rnd, 32, 32, (66, 44, 30))
    d = PIL.ImageDraw.Draw(im)
    d.rectangle([14, 6, 17, 26], fill=(150, 130, 90, 255))        # a cross
    d.rectangle([9, 11, 22, 14], fill=(150, 130, 90, 255))
    return im


def build(ajar=12.0, seed=1):
    rnd = random.Random(seed)
    s = mk.Soup()
    w = s.material(mk.wood(rnd, 16, 32, (58, 40, 28)))
    lt = s.material(lid_tex())
    ins = s.material(inside(), clamp=True)
    # the box: walls only, so the inside shows as a black floor 4 under the rim
    mk.prism(s, OUTLINE, 0, 16, w, ins, w, top_uv=lambda x, z: ((x + 15) / 30.0, (z + 6) / 68.0))
    # the lid sits on the box, turned and slid back so a wedge of the inside shows
    lid = mk.Soup()
    mk.prism(lid, [(x * 1.04, z * 1.02 + 0) for x, z in OUTLINE], 0, 3.0, w, lt, w, top_uv=lambda x, z: ((x + 15) / 30.0, (z + 6) / 68.0))
    s.add(lid, offset=(ajar * 0.6, 16.5, ajar * 1.2), yaw=-math.radians(ajar * 0.9), shared=True)
    return s
