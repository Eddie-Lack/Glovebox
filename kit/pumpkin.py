"""Jack-o'-lantern: a ribbed pumpkin, 46 across, with a carved face that glows and a curled stem. The face is a 64x32 picture on
the front 120 degrees (the rest of the skin is the picture's edge, which is plain orange)."""
import math, random

import numpy as np
import PIL.Image
import PIL.ImageDraw

import meshkit as mk

NAME = 'pumpkin'


def skin(mean=(205, 90, 12)):
    rnd = random.Random(7)
    return mk.noise(32, 32, rnd, -14, 10, mean)


def face_texture(kind=0):
    """The carved face, lit from inside. kind 0 grins, 1 howls, 2 stares with a single round eye, 3 weeps, 4 screams with rows of teeth,
    5 has its eyes sewn shut and smiles too wide, 6 has a glove's print for a face, 7 has six small eyes."""
    W, H = 64, 32
    rnd = random.Random(11 + kind)
    im = mk.noise(W, H, rnd, -14, 10, (205, 90, 12))
    d = PIL.ImageDraw.Draw(im)
    for x in range(0, W, 4):                       # darker ribs between the lobes
        d.line([(x, 0), (x, H)], fill=(150, 60, 8, 255))
    glow, hot = (255, 214, 60, 255), (255, 250, 170, 255)
    dark = (60, 20, 4, 255)
    if kind == 0:
        for cx in (18, 46):                                     # triangular eyes
            d.polygon([(cx - 6, 17), (cx + 6, 17), (cx, 7)], fill=dark)
            d.polygon([(cx - 4, 16), (cx + 4, 16), (cx, 9)], fill=glow)
        d.polygon([(32, 17), (29, 21), (35, 21)], fill=glow)       # nose
        mouth = [(10, 22), (16, 25), (20, 23), (26, 27), (32, 24), (38, 27), (44, 23), (48, 25), (54, 22), (50, 29), (42, 31), (22, 31), (14, 29)]
        d.polygon(mouth, fill=dark)
        inner = [(13, 24), (17, 26), (21, 25), (26, 28), (32, 26), (38, 28), (43, 25), (47, 26), (51, 24), (48, 29), (41, 30), (23, 30), (16, 28)]
        d.polygon(inner, fill=glow)
        for x in (19, 27, 37, 45):                                   # teeth
            d.rectangle([x, 24, x + 2, 27], fill=dark)
        d.rectangle([30, 26, 34, 29], fill=hot)
    elif kind == 1:
        for cx in (17, 47):
            d.ellipse([cx - 5, 6, cx + 5, 18], fill=dark)
            d.ellipse([cx - 3, 8, cx + 3, 16], fill=glow)
            d.ellipse([cx - 1, 10, cx + 1, 14], fill=hot)
        d.ellipse([21, 19, 43, 32], fill=dark)                       # a howl
        d.ellipse([24, 21, 40, 31], fill=glow)
        d.ellipse([28, 23, 36, 30], fill=hot)
    elif kind == 3:
        for cx in (19, 45):                                          # eyes that droop at the outer corners
            d.polygon([(cx - 7, 9), (cx + 7, 9), (cx + (6 if cx < 32 else -6), 17)], fill=dark)
            d.polygon([(cx - 5, 10), (cx + 5, 10), (cx + (4 if cx < 32 else -4), 15)], fill=glow)
        for cx in (17, 47):                                          # tears running down to the mouth
            d.line([(cx, 17), (cx - 1, 26)], fill=glow)
        d.arc([20, 22, 44, 36], 200, 340, fill=dark, width=2)       # a mouth turned down
        d.arc([22, 24, 42, 36], 210, 330, fill=glow, width=1)
    elif kind == 4:
        for cx in (20, 44):
            d.ellipse([cx - 4, 5, cx + 4, 13], fill=dark)
            d.ellipse([cx - 2, 7, cx + 2, 11], fill=hot)
        d.ellipse([12, 14, 52, 32], fill=dark)                       # a mouth as wide as the face
        d.ellipse([15, 16, 49, 31], fill=glow)
        for x in range(15, 50, 4):                                   # two rows of fangs
            d.polygon([(x, 16), (x + 3, 16), (x + 1.5, 21)], fill=dark)
            d.polygon([(x + 2, 31), (x + 5, 31), (x + 3.5, 26)], fill=dark)
    elif kind == 5:
        for cx in (18, 46):                                          # eyes sewn shut
            d.line([(cx - 7, 12), (cx + 7, 12)], fill=dark, width=2)
            for x in range(cx - 6, cx + 7, 3):
                d.line([(x, 9), (x + 1, 15)], fill=dark)
        mouth = [(4, 18), (14, 26), (32, 29), (50, 26), (60, 18), (52, 29), (32, 32), (12, 29)]
        d.polygon(mouth, fill=dark)
        d.polygon([(8, 20), (16, 27), (32, 30), (48, 27), (56, 20), (50, 28), (32, 31), (14, 28)], fill=glow)
    elif kind == 6:
        d.rectangle([22, 14, 42, 30], fill=dark)                     # a glove's print: the palm and four fingers
        d.rectangle([24, 16, 40, 29], fill=glow)
        for x in (22, 28, 34, 40):
            d.rectangle([x - 1, 3, x + 3, 15], fill=dark)
            d.rectangle([x, 5, x + 2, 15], fill=glow)
        d.polygon([(42, 20), (52, 14), (54, 17), (44, 24)], fill=dark)
    elif kind == 7:
        for cx, cy in ((14, 8), (24, 13), (20, 21), (44, 9), (40, 17), (50, 22)):   # six small eyes, all different
            d.ellipse([cx - 3, cy - 3, cx + 3, cy + 3], fill=dark)
            d.ellipse([cx - 1, cy - 1, cx + 1, cy + 1], fill=hot)
        d.line([(24, 28), (40, 28)], fill=dark, width=2)
    else:
        d.ellipse([22, 5, 42, 23], fill=dark)                        # one huge eye
        d.ellipse([25, 8, 39, 20], fill=glow)
        d.ellipse([30, 11, 34, 17], fill=(120, 20, 0, 255))
        for x in range(8, 58, 5):                                    # a stitched mouth
            d.line([(x, 26), (x + 3, 29)], fill=dark)
            d.line([(x + 3, 26), (x, 29)], fill=dark)
    return im


def wood():
    rnd = random.Random(3)
    return mk.noise(16, 16, rnd, -12, 12, (70, 90, 40))


def build(kind=0):
    s = mk.Soup()
    face = s.material(face_texture(kind), clamp=True)
    stem = s.material(wood())
    R, Hh = 23.0, 19.0
    ribs = 4

    def body(u, v):
        a = 2 * math.pi * u
        t = math.pi * v                   # from the bottom pole to the top pole
        r = R * math.sin(t) * (1 + 0.09 * math.cos(ribs * a)) * (1 + 0.04 * math.sin(t) ** 2)
        if v > 0.93:
            r *= 1 - (v - 0.93) * 4       # a dimple round the stem
        y = Hh - Hh * math.cos(t) - (0 if v < 0.93 else (v - 0.93) * 12 * (1 - (1 - v) * 0))
        return (r * math.cos(a), y, r * math.sin(a))

    def uv(u, v):
        # the front is +z, which is angle 90 degrees = u 0.25; the picture covers 120 degrees round it
        ang = u * 360.0
        fu = (ang - 30.0) / 120.0
        return (fu, 1.0 - v) if True else (u, v)
    mk.param(s, body, 12, 6, face, uv=lambda u, v: ((u * 360.0 - 30.0) / 120.0, 1.0 - v), flip=True)
    stem_pts = [(0, Hh * 2 - 6, 0), (0, Hh * 2 + 2, 0), (2, Hh * 2 + 8, 1), (6, Hh * 2 + 12, 2)]
    mk.tube(s, stem_pts, [4.5, 3.6, 3.0, 2.0], 6, stem, cap_end=True)
    return s
