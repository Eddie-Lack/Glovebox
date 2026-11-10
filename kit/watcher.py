"""Things that look back: a bloodshot eye and a pale face with red points for pupils. Each is the front half of an ellipsoid with its
picture projected from the front, so it reads as an eye or a face only from the front (+z). Radius 100 at scale 1; scale them up in the
level. The eye is 288 tris, the face 288."""
import math, random

import numpy as np
import PIL.Image
import PIL.ImageDraw

import meshkit as mk

NAME = 'watcher'


def squash(im64):
    """Painted square, stored 64x32: the dome stretches the picture over a square again."""
    return im64.resize((64, 32), PIL.Image.LANCZOS)


def eye_tex():
    rnd = random.Random(41)
    im = PIL.Image.new('RGBA', (64, 64), (24, 6, 8, 255))
    d = PIL.ImageDraw.Draw(im)
    d.ellipse([0, 0, 63, 63], fill=(214, 204, 172, 255))
    # veins running in from the rim
    for _ in range(46):
        a = rnd.uniform(0, 2 * math.pi)
        r0 = 31.5
        x, y = 32 + r0 * math.cos(a), 32 + r0 * math.sin(a)
        for step in range(rnd.randrange(8, 20)):
            a += rnd.uniform(-0.35, 0.35)
            nx, ny = x - math.cos(a) * 1.7, y - math.sin(a) * 1.7
            d.line([(x, y), (nx, ny)], fill=(150 + rnd.randrange(30), 20, 22, 255))
            x, y = nx, ny
    d.ellipse([16, 16, 48, 48], fill=(120, 150, 24, 255))            # the iris
    d.ellipse([19, 19, 45, 45], fill=(170, 190, 40, 255))
    for k in range(18):                                              # fibres
        a = 2 * math.pi * k / 18
        d.line([(32 + 5 * math.cos(a), 32 + 5 * math.sin(a)), (32 + 14 * math.cos(a), 32 + 14 * math.sin(a))], fill=(90, 110, 20, 255))
    d.ellipse([27, 14, 37, 50], fill=(6, 2, 2, 255))                 # a slit pupil, a little too tall
    d.ellipse([22, 20, 25, 23], fill=(255, 255, 230, 255))
    return squash(im)


def face_tex():
    rnd = random.Random(43)
    im = PIL.Image.new('RGBA', (64, 64), (24, 22, 24, 255))
    d = PIL.ImageDraw.Draw(im)
    d.ellipse([0, 0, 63, 63], fill=(196, 192, 180, 255))
    for _ in range(300):                                             # pocked skin
        x, y = rnd.randrange(64), rnd.randrange(64)
        if (x - 32) ** 2 + (y - 32) ** 2 < 31 ** 2:
            c = rnd.randint(-26, 12)
            d.point((x, y), fill=(196 + c, 192 + c, 180 + c, 255))
    for cx in (20, 44):                                              # sockets
        d.ellipse([cx - 9, 18, cx + 9, 40], fill=(4, 2, 4, 255))
        d.ellipse([cx - 1, 27, cx + 1, 29], fill=(255, 30, 20, 255))
    d.polygon([(32, 32), (28, 44), (36, 44)], fill=(60, 50, 50, 255))   # a nose, a dark triangle
    mouth = [(14, 50), (22, 54), (28, 50), (34, 55), (40, 50), (46, 54), (52, 50)]   # a stitched grin
    d.line(mouth, fill=(10, 4, 4, 255), width=2)
    for (x, y) in mouth:
        d.line([(x, y - 4), (x, y + 4)], fill=(10, 4, 4, 255))
    return squash(im)


def build_eye():
    s = mk.Soup()
    m = s.material(eye_tex(), clamp=True, bright=1.0)
    back = s.material(mk.noise(8, 8, random.Random(2), -6, 6, (40, 8, 10)))
    mk.dome(s, 100.0, 5, 16, m, squash=(1.0, 1.0, 0.9), back=back)
    return s


def build_face():
    s = mk.Soup()
    m = s.material(face_tex(), clamp=True, bright=1.0)
    back = s.material(mk.noise(8, 8, random.Random(3), -6, 6, (50, 48, 50)))
    mk.dome(s, 100.0, 5, 16, m, squash=(0.8, 1.1, 0.4), back=back)
    return s
