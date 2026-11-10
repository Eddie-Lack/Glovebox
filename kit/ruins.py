"""Ruins: a scorch mark, a heap of rubble and a broken signpost. All of it is non-solid scenery. Origins are on the ground.

    scorch        a flat black disc of soot with cracks and embers, 280 across, 14 triangles
    rubble        a heap of broken stone blocks, about 90 across and 40 high
    sign          a broken signpost: a name in peeling paint and a red word under it (62 across, 55 high, 8 triangles)
"""
import math, random

import PIL.Image
import PIL.ImageDraw

import meshkit as mk

NAME = 'ruins'


def board_tex(top, bottom, size, seed):
    """A painted board (64x32): the name in peeling white, a red word under it, and rot."""
    rnd = random.Random(seed)
    im = mk.wood(rnd, 64, 32, (82, 60, 42))
    d = PIL.ImageDraw.Draw(im)
    d.rectangle([1, 1, 62, 30], outline=(40, 28, 20, 255))
    f = mk.font(size)
    d.fontmode = '1'
    w = d.textlength(top, font=f)
    d.text(((64 - w) / 2, 4), top, fill=(214, 206, 188, 255), font=f)
    f2 = mk.font(8)
    w2 = d.textlength(bottom, font=f2)
    d.text(((64 - w2) / 2, 17), bottom, fill=(176, 24, 20, 255), font=f2)
    for _ in range(40):                                  # peeled paint
        x, y = rnd.randrange(3, 61), rnd.randrange(3, 29)
        d.point((x, y), fill=(82, 60, 42, 255))
    for _ in range(3):                                   # drips
        x0 = rnd.randrange(8, 56)
        for y in range(22, 22 + rnd.randrange(4, 9)):
            d.point((x0, y), fill=(120, 14, 10, 255))
    return im


def build_sign(top, bottom, size, seed):
    """A leaning post with a board nailed on at a slant. 62 across, about 55 high; the front is +z."""
    rnd = random.Random(seed)
    s = mk.Soup()
    wd = s.material(mk.wood(rnd, 16, 32, (58, 42, 30)))
    face = s.material(board_tex(top, bottom, size, seed), clamp=True, bright=1.0)
    post = mk.Soup()
    mk.box(post, (-3, -6, -3), (3, 58, 3), wd)
    s.add(post, tilt=(0.0, 0.10), shared=True)
    board = mk.Soup()
    mk.box(board, (-30, -15, -1.5), (30, 15, 1.5), [wd, wd, wd, wd, face, wd], uvs={face: mk.UPRIGHT})
    s.add(board, offset=(1.0, 38.0, 4.0), tilt=(0.0, -0.22), shared=True)
    return s


def soot(seed=3):
    """Soot with cracks and a few embers (32x32); the corners are clear so the disc has an uneven edge."""
    rnd = random.Random(seed)
    im = mk.noise(32, 32, rnd, -8, 10, (26, 22, 26))
    d = PIL.ImageDraw.Draw(im)
    for _ in range(6):                                   # cracks running out from the middle
        a = rnd.uniform(0, 2 * math.pi)
        x, y = 16.0, 16.0
        for _ in range(rnd.randint(5, 9)):
            a += rnd.uniform(-0.5, 0.5)
            nx, ny = x + 2.2 * math.cos(a), y + 2.2 * math.sin(a)
            d.line([(x, y), (nx, ny)], fill=(8, 6, 8, 255))
            x, y = nx, ny
    for _ in range(14):                                  # embers
        x, y = rnd.randrange(4, 28), rnd.randrange(4, 28)
        d.point((x, y), fill=(255, 90, 28, 255))
        d.point((x + 1, y), fill=(160, 40, 18, 255))
    for y in range(32):                                  # the edge: clear outside an uneven radius
        for x in range(32):
            r = math.hypot(x - 15.5, y - 15.5)
            lim = 14.5 + 1.4 * math.sin(math.atan2(y - 15.5, x - 15.5) * 5 + seed)
            if r > lim:
                im.putpixel((x, y), (0, 0, 0, 0))
    return im


def build_scorch():
    s = mk.Soup()
    m = s.material(soot(), clamp=True, bright=1.2)
    mk.disc(s, (0, 0, 0), 140.0, 14, m)
    return s


def build_rubble(seed=5):
    rnd = random.Random(seed)
    s = mk.Soup()
    st = s.material(mk.stone(rnd, 32, 32, base=(88, 90, 88)))
    for k in range(7):
        b = mk.Soup()
        w, h, d = rnd.uniform(14, 30), rnd.uniform(9, 18), rnd.uniform(12, 24)
        mk.box(b, (-w / 2, 0, -d / 2), (w / 2, h, d / 2), st)
        a = 2 * math.pi * k / 7 + rnd.uniform(-0.4, 0.4)
        r = 0.0 if k == 0 else rnd.uniform(14, 36)
        s.add(b, offset=(r * math.cos(a), 0.0 if k < 5 else 12.0, r * math.sin(a)), yaw=rnd.uniform(0, 6.28),
              tilt=(rnd.uniform(-0.4, 0.4), rnd.uniform(-0.4, 0.4)), shared=True)
    return s
