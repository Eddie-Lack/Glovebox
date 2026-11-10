"""Things that wait beside the path: a shade, a hand out of the ground, a hanging cage and a lantern. The front is +z; the origin is the base.

    shade     a figure too tall and too thin (115 high), black, with two pale points for eyes and its arms down to its knees. One material, so it
              is one piece
    hand      a white glove as big as a tree, reaching up out of the ground, the fingers bent, dirt on the cuff
    cage      a gibbet: a post with an arm and a round iron cage hanging from it on a chain, a limp glove inside
    lantern   a post with a lantern on a hook, its panes glowing
"""
import math, random

import PIL.ImageDraw

import meshkit as mk

NAME = 'stalker'


def shade_tex():
    """32x32: black cloth with a little noise; the top right 8x8 is pale (the eyes are mapped there)."""
    rnd = random.Random(61)
    im = mk.noise(32, 32, rnd, -4, 6, (10, 8, 12))
    d = PIL.ImageDraw.Draw(im)
    d.rectangle([24, 0, 31, 7], fill=(236, 232, 214, 255))
    return im


def build_shade():
    s = mk.Soup()
    m = s.material(shade_tex(), clamp=True, bright=1.0)
    # the body and the legs: one long tube, narrow at the ankles and the neck
    mk.tube(s, [(0, 0, 0), (0, 40, 0), (0, 80, 0), (0, 98, 0)], [3.0, 6.0, 7.5, 3.0], 6, m, cap_end=False, u_repeat=0.5)
    # the head, bent forward
    mk.tube(s, [(0, 98, 0), (0, 106, 3), (0, 114, 5)], [5.5, 6.0, 3.5], 6, m, cap_end=True, u_repeat=0.5)
    # the arms, from the shoulders to below the knees
    for sx in (-1, 1):
        mk.tube(s, [(sx * 7, 94, 0), (sx * 10, 70, 1), (sx * 11, 40, 2), (sx * 10, 30, 3)], [2.0, 1.8, 1.6, 1.0], 4, m, cap_end=True, u_repeat=0.25)
    # the eyes: two small quads in front of the head, mapped on the pale corner
    pale = ((0.78, 0.22), (0.97, 0.22), (0.97, 0.03), (0.78, 0.03))
    for sx in (-1, 1):
        x0, x1 = sx * 1.2 - 0.8, sx * 1.2 + 0.8
        z = 11.2
        s.quad(mk.v3(x0, 106.5, z), mk.v3(x1, 106.5, z), mk.v3(x1, 108.0, z), mk.v3(x0, 108.0, z), *pale, m)
    return s


def glove_tex(seed=71):
    rnd = random.Random(seed)
    im = mk.noise(32, 32, rnd, -16, 6, (214, 210, 200))
    d = PIL.ImageDraw.Draw(im)
    for _ in range(40):                                  # dirt
        x, y = rnd.randrange(32), rnd.randrange(32)
        d.point((x, y), fill=(76, 60, 44, 255))
    for y in range(24, 32):                              # the cuff's end is caked in earth
        for x in range(32):
            if rnd.random() < 0.6:
                d.point((x, y), fill=(60 + rnd.randint(0, 20), 46, 32, 255))
    return im


def build_hand():
    s = mk.Soup()
    g = s.material(glove_tex())
    soil = s.material(mk.noise(16, 16, random.Random(72), -10, 10, (48, 36, 28)))
    # the cuff out of the ground, the palm
    mk.lathe(s, [(30, -6), (27, 20), (24, 50), (26, 70)], 8, g)
    mk.box(s, (-26, 70, -10), (26, 130, 10), g)
    # four fingers, bent toward the front, the last joints hanging
    for k, x in enumerate((-19, -6.5, 6.5, 19)):
        h = 70 if k in (1, 2) else 58
        pts = [(x, 128, 0), (x * 1.08, 128 + h * 0.6, 4), (x * 1.12, 128 + h * 0.9, 18), (x * 1.12, 128 + h * 0.85, 34)]
        mk.tube(s, pts, [7.0, 6.5, 6.0, 5.0], 6, g, cap_end=True)
    # the thumb, out to the side
    mk.tube(s, [(26, 92, 0), (40, 112, 6), (46, 130, 14)], [7.0, 6.0, 5.0], 6, g, cap_end=True)
    # a mound of earth round the cuff
    mk.lathe(s, [(48, -4), (34, 6), (24, 10)], 8, soil)
    return s


def build_cage():
    rnd = random.Random(81)
    s = mk.Soup()
    wd = s.material(mk.wood(rnd, 16, 32, (50, 38, 28)))
    iron = s.material(mk.noise(16, 16, rnd, -14, 10, (58, 52, 50)))
    glove = s.material(glove_tex(73))
    # the post and its arm
    mk.box(s, (-6, -4, -6), (6, 220, 6), wd)
    mk.box(s, (-6, 210, -5), (100, 220, 5), wd)
    # the chain and the cage under the arm's end
    cx, top = 90.0, 210.0
    mk.box(s, (cx - 1.2, 150, -1.2), (cx + 1.2, top, 1.2), iron)
    R, y0, y1 = 24.0, 70.0, 150.0
    for k in range(8):
        a = 2 * math.pi * k / 8
        x, z = cx + R * math.cos(a), R * math.sin(a)
        mk.box(s, (x - 1.3, y0, z - 1.3), (x + 1.3, y1, z + 1.3), iron)
    ends = mk.Soup()                                     # the domed lid and the floor, made round the origin and moved under the arm
    mk.lathe(ends, [(R + 1.5, y1 - 2), (R + 1.5, y1 + 2), (0, y1 + 14)], 8, iron)
    mk.lathe(ends, [(0, y0 - 3), (R + 1.5, y0 - 3), (R + 1.5, y0 + 2)], 8, iron)
    s.add(ends, offset=(cx, 0, 0), shared=True)
    # a limp glove slumped on the cage's floor
    limp = mk.Soup()
    mk.box(limp, (-11, 0, -5), (11, 16, 5), glove)
    for k, x in enumerate((-8, -2.5, 2.5, 8)):
        mk.tube(limp, [(x, 14, 0), (x, 22, 6), (x * 1.2, 20, 14)], [3.0, 2.8, 2.4], 4, glove, cap_end=True)
    s.add(limp, offset=(cx, y0 + 1, 2), yaw=0.6, tilt=(0.3, 0.4), shared=True)
    return s


def build_lantern():
    rnd = random.Random(91)
    s = mk.Soup()
    wd = s.material(mk.wood(rnd, 16, 32, (54, 40, 30)))
    iron = s.material(mk.noise(16, 16, rnd, -10, 10, (40, 36, 34)))
    pane = mk.noise(16, 16, rnd, -20, 0, (255, 190, 80))
    d = PIL.ImageDraw.Draw(pane)
    d.line([(8, 0), (8, 15)], fill=(40, 30, 20, 255))
    d.line([(0, 8), (15, 8)], fill=(40, 30, 20, 255))
    glow = s.material(pane, bright=1.8)
    mk.box(s, (-4, -4, -4), (4, 150, 4), wd)
    mk.box(s, (-4, 140, -3), (34, 146, 3), wd)
    mk.box(s, (29, 118, -1), (31, 140, 1), iron)
    # the lantern: glowing panes in an iron frame, a pointed roof
    mk.box(s, (21, 96, -9), (39, 118, 9), [glow, glow, iron, iron, glow, glow])
    roof = mk.Soup()
    mk.lathe(roof, [(14, 118), (14, 120), (0, 130)], 4, iron, phase=math.pi / 4)
    s.add(roof, offset=(30, 0, 0), shared=True)
    return s
