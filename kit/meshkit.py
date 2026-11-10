"""A small mesh and texture kit for the editor's kit models (tools/kit.py). Everything is built by code: no downloaded assets.

A `Soup` is a list of triangles with a position, a normal and a texture coordinate at each corner, and a texture ("material")
each. `Soup.glb()` writes a binary glTF with one primitive per texture, which tools/custom_model.py turns into Glover models.
Units are game units (the ball is 9 across a radius, the glove 13). y is up. The textures must be small: at most 2,048 texels
(32x64, 64x32, 32x32 and so on), because one texture load of the N64 takes no more. They are 16 bit, so a pixel is opaque or
fully clear (alpha cut-outs are fine).
"""
import io, math, random

import numpy as np
import PIL.Image
import PIL.ImageDraw
import pygltflib as gl

Vec = np.array
BRIGHT = 1.4


def v3(x, y, z):
    return np.array([x, y, z], dtype=np.float64)


def norm(v):
    n = np.linalg.norm(v)
    return v / n if n > 1e-12 else v


class Soup:
    def __init__(self):
        self.tris = []      # (p[3,3], n[3,3], uv[3,2], mat)
        self.mats = []      # PIL images, in order of first use
        self.clamp = {}     # mat -> True when the texture must not repeat

    def material(self, image, clamp=False, bright=BRIGHT):
        """A texture. The game lights models dark (ambient and diffuse light multiply the picture), so pictures are made `bright` times
        lighter than they are meant to look, clipped at white."""
        im = image.convert("RGBA")
        if bright != 1.0:
            a = np.asarray(im, np.float64).copy()
            a[:, :, :3] = np.clip(a[:, :, :3] * bright, 0, 255)
            im = PIL.Image.fromarray(a.astype(np.uint8), "RGBA")
        self.mats.append(im)
        self.clamp[len(self.mats) - 1] = clamp
        return len(self.mats) - 1

    def tri(self, a, b, c, ua, ub, uc, mat, na=None, nb=None, nc=None):
        a, b, c = Vec(a, dtype=np.float64), Vec(b, dtype=np.float64), Vec(c, dtype=np.float64)
        f = np.cross(b - a, c - a)
        if np.linalg.norm(f) < 1e-9:
            return
        f = norm(f)
        n = np.array([f if x is None else norm(Vec(x, dtype=np.float64)) for x in (na, nb, nc)])
        self.tris.append((np.array([a, b, c]), n, np.array([ua, ub, uc], dtype=np.float64), mat))

    def quad(self, a, b, c, d, ua, ub, uc, ud, mat, na=None, nb=None, nc=None, nd=None):
        """Corners counter-clockwise seen from outside."""
        self.tri(a, b, c, ua, ub, uc, mat, na, nb, nc)
        self.tri(a, c, d, ua, uc, ud, mat, na, nc, nd)

    def add(self, other, offset=(0, 0, 0), yaw=0.0, scale=1.0, tilt=(0.0, 0.0), shared=False):
        """Another soup (with `shared`, one built with this soup's own materials), tilted (about x, then z, in radians), turned about y, scaled and moved; its textures join this soup's."""
        base = 0 if shared else len(self.mats)       # shared: `other` was built with this soup's materials
        if not shared:
            self.mats += other.mats
            for i, c in other.clamp.items():
                self.clamp[base + i] = c
        cy, sy = math.cos(yaw), math.sin(yaw)
        ry = np.array([[cy, 0, sy], [0, 1, 0], [-sy, 0, cy]])
        cx, sx = math.cos(tilt[0]), math.sin(tilt[0])
        rx = np.array([[1, 0, 0], [0, cx, -sx], [0, sx, cx]])
        cz, sz = math.cos(tilt[1]), math.sin(tilt[1])
        rz = np.array([[cz, -sz, 0], [sz, cz, 0], [0, 0, 1]])
        rot = ry @ rz @ rx
        for p, n, uv, m in other.tris:
            self.tris.append(((p * scale) @ rot.T + Vec(offset), n @ rot.T, uv, m + base))
        return self

    def count(self):
        return len(self.tris)

    def bounds(self):
        P = np.concatenate([t[0] for t in self.tris])
        return P.min(0), P.max(0)

    def glb(self):
        g = gl.GLTF2(asset=gl.Asset(version="2.0"), scene=0)
        blob = bytearray()

        def view(data, target=None):
            while len(blob) % 4:
                blob.append(0)
            g.bufferViews.append(gl.BufferView(buffer=0, byteOffset=len(blob), byteLength=len(data), target=target))
            blob.extend(data)
            return len(g.bufferViews) - 1

        def acc(arr, kind, ctype=gl.FLOAT, target=gl.ARRAY_BUFFER):
            data = arr.astype("<f4" if ctype == gl.FLOAT else "<u4").tobytes()
            a = gl.Accessor(bufferView=view(data, target), componentType=ctype, count=len(arr), type=kind)
            if kind == gl.VEC3 and ctype == gl.FLOAT:
                a.min, a.max = arr.min(0).tolist(), arr.max(0).tolist()
            g.accessors.append(a)
            return len(g.accessors) - 1

        prims = []
        used = sorted({t[3] for t in self.tris})
        for m in used:
            sel = [t for t in self.tris if t[3] == m]
            P = np.concatenate([t[0] for t in sel])
            N = np.concatenate([t[1] for t in sel])
            UV = np.concatenate([t[2] for t in sel])
            buf = io.BytesIO()
            self.mats[m].save(buf, "PNG")
            g.images.append(gl.Image(bufferView=view(buf.getvalue()), mimeType="image/png"))
            wrap = gl.CLAMP_TO_EDGE if self.clamp.get(m) else gl.REPEAT
            g.samplers.append(gl.Sampler(wrapS=wrap, wrapT=wrap))
            g.textures.append(gl.Texture(source=len(g.images) - 1, sampler=len(g.samplers) - 1))
            g.materials.append(gl.Material(pbrMetallicRoughness=gl.PbrMetallicRoughness(
                baseColorTexture=gl.TextureInfo(index=len(g.textures) - 1))))
            attrs = gl.Attributes(POSITION=acc(P, gl.VEC3), NORMAL=acc(N, gl.VEC3), TEXCOORD_0=acc(UV, gl.VEC2))
            idx = acc(np.arange(len(P)), gl.SCALAR, gl.UNSIGNED_INT, gl.ELEMENT_ARRAY_BUFFER)
            prims.append(gl.Primitive(attributes=attrs, indices=idx, material=len(g.materials) - 1))
        g.meshes.append(gl.Mesh(primitives=prims))
        g.nodes.append(gl.Node(mesh=0))
        g.scenes.append(gl.Scene(nodes=[0]))
        g.buffers.append(gl.Buffer(byteLength=len(blob)))
        g.set_binary_blob(bytes(blob))
        return b"".join(g.save_to_bytes())


# ---------------------------------------------------------------------------------------------
# Shapes

def lathe(s, profile, segs, mat, v0=0.0, v1=1.0, u_repeat=1.0, smooth=True, phase=0.0):
    """A surface of revolution about y. `profile` is [(radius, y), ...] from the bottom up. u runs around (0 to u_repeat),
    v along the profile (v0 at its start, v1 at its end). The first and last radius may be 0 (a point)."""
    P = [(r, y) for r, y in profile]
    n = len(P)
    for i in range(n - 1):
        (r0, y0), (r1, y1) = P[i], P[i + 1]
        va = v0 + (v1 - v0) * i / (n - 1)
        vb = v0 + (v1 - v0) * (i + 1) / (n - 1)
        dr, dy = r1 - r0, y1 - y0
        # normal of the profile segment (outward is +r when going up)
        sn = norm(v3(dy, -dr, 0)) if (dr or dy) else v3(1, 0, 0)
        for k in range(segs):
            a0 = phase + 2 * math.pi * k / segs
            a1 = phase + 2 * math.pi * (k + 1) / segs
            ua, ub = u_repeat * k / segs, u_repeat * (k + 1) / segs

            def pt(r, y, a):
                return v3(r * math.cos(a), y, r * math.sin(a))

            def nm(a):
                return v3(sn[0] * math.cos(a), sn[1], sn[0] * math.sin(a)) if smooth else None
            p00, p10 = pt(r0, y0, a0), pt(r0, y0, a1)
            p01, p11 = pt(r1, y1, a0), pt(r1, y1, a1)
            # seen from outside, going around increasing angle is clockwise from above in a right-handed y-up frame;
            # the order below gives outward-facing triangles
            if r0 > 1e-9 and r1 > 1e-9:
                s.quad(p00, p01, p11, p10, (ua, va), (ua, vb), (ub, vb), (ub, va), mat, nm(a0), nm(a0), nm(a1), nm(a1))
            elif r0 <= 1e-9:
                s.tri(p01, p11, p00, (ua, vb), (ub, vb), (ua, va), mat, nm(a0), nm(a1), nm(a0))
            else:
                s.tri(p00, p01, p10, (ua, va), (ua, vb), (ub, va), mat, nm(a0), nm(a0), nm(a1))
    return s


def disc(s, centre, radius, segs, mat, up=True, uvc=(0.5, 0.5), uvr=0.5):
    cx, cy, cz = centre
    for k in range(segs):
        a0, a1 = 2 * math.pi * k / segs, 2 * math.pi * (k + 1) / segs
        p0 = v3(cx + radius * math.cos(a0), cy, cz + radius * math.sin(a0))
        p1 = v3(cx + radius * math.cos(a1), cy, cz + radius * math.sin(a1))
        u0 = (uvc[0] + uvr * math.cos(a0), uvc[1] + uvr * math.sin(a0))
        u1 = (uvc[0] + uvr * math.cos(a1), uvc[1] + uvr * math.sin(a1))
        if up:
            s.tri(v3(cx, cy, cz), p1, p0, uvc, u1, u0, mat)
        else:
            s.tri(v3(cx, cy, cz), p0, p1, uvc, u0, u1, mat)


# Corners for a picture on a side face of `box` (+x, -x, +z or -z), upright and not mirrored seen from outside: box's own corners run the
# picture's rows along the face's width (fine for noise, sideways and mirrored for words or a face).
UPRIGHT = ((0, 1), (1, 1), (1, 0), (0, 0))


def box(s, lo, hi, mats, uvs=None):
    """An axis-aligned box. `mats` is one material or six (+x, -x, +y, -y, +z, -z). Each face gets the whole picture; `uvs` maps a material to
    its four corners (UPRIGHT for a picture on a side face)."""
    x0, y0, z0 = lo
    x1, y1, z1 = hi
    if not isinstance(mats, (list, tuple)):
        mats = [mats] * 6
    full = ((0, 1), (0, 0), (1, 0), (1, 1))
    faces = [  # corners ccw seen from outside
        ((x1, y0, z1), (x1, y0, z0), (x1, y1, z0), (x1, y1, z1)),      # +x
        ((x0, y0, z0), (x0, y0, z1), (x0, y1, z1), (x0, y1, z0)),      # -x
        ((x0, y1, z1), (x1, y1, z1), (x1, y1, z0), (x0, y1, z0)),      # +y
        ((x0, y0, z0), (x1, y0, z0), (x1, y0, z1), (x0, y0, z1)),      # -y
        ((x0, y0, z1), (x1, y0, z1), (x1, y1, z1), (x0, y1, z1)),      # +z
        ((x1, y0, z0), (x0, y0, z0), (x0, y1, z0), (x1, y1, z0)),      # -z
    ]
    for f, m in zip(faces, mats):
        if m is None:
            continue
        u = uvs.get(m, full) if uvs else full
        s.quad(*[v3(*p) for p in f], *u, m)
    return s


def tube(s, pts, radii, sides, mat, v_scale=1.0, cap_end=True, cap_start=False, u_repeat=1.0):
    """A tube along a polyline. radii: one per point (or one number)."""
    pts = [Vec(p, dtype=np.float64) for p in pts]
    if not isinstance(radii, (list, tuple)):
        radii = [radii] * len(pts)
    rings, frames = [], []
    up = v3(0, 1, 0)
    for i, p in enumerate(pts):
        t = norm(pts[min(i + 1, len(pts) - 1)] - pts[max(i - 1, 0)])
        a = norm(np.cross(t, up)) if abs(t[1]) < 0.95 else v3(1, 0, 0)
        b = np.cross(t, a)
        ring = [p + radii[i] * (math.cos(2 * math.pi * k / sides) * a + math.sin(2 * math.pi * k / sides) * b) for k in range(sides)]
        nrm = [norm(r - p) for r in ring]
        rings.append((ring, nrm, p, t))
    length = 0.0
    vs = [0.0]
    for i in range(1, len(pts)):
        length += np.linalg.norm(pts[i] - pts[i - 1])
        vs.append(length)
    for i in range(len(pts) - 1):
        r0, n0, _, _ = rings[i]
        r1, n1, _, _ = rings[i + 1]
        for k in range(sides):
            k2 = (k + 1) % sides
            ua, ub = u_repeat * k / sides, u_repeat * (k + 1) / sides
            va, vb = vs[i] * v_scale / max(length, 1e-9), vs[i + 1] * v_scale / max(length, 1e-9)
            s.quad(r0[k], r0[k2], r1[k2], r1[k], (ua, va), (ub, va), (ub, vb), (ua, vb), mat, n0[k], n0[k2], n1[k2], n1[k])
    if cap_end:
        ring, _, p, t = rings[-1]
        for k in range(sides):
            s.tri(p + t * 0.001, ring[k], ring[(k + 1) % sides], (0.5, 0.5), (0.5, 0.5), (0.5, 0.5), mat)
    if cap_start:
        ring, _, p, t = rings[0]
        for k in range(sides):
            s.tri(p, ring[(k + 1) % sides], ring[k], (0.5, 0.5), (0.5, 0.5), (0.5, 0.5), mat)
    return s


def dome(s, radius, rings, segs, mat, squash=(1.0, 1.0, 0.5), back=None):
    """The front half (+z) of an ellipsoid at the origin, with its picture projected from the front: u = x, v = y.
    `back` (a material) closes the back with a flat disc."""
    sx, sy, sz = squash
    for i in range(rings):
        t0, t1 = (math.pi / 2) * i / rings, (math.pi / 2) * (i + 1) / rings   # from the front pole outward
        for k in range(segs):
            a0, a1 = 2 * math.pi * k / segs, 2 * math.pi * (k + 1) / segs

            def pt(t, a):
                x = math.sin(t) * math.cos(a)
                y = math.sin(t) * math.sin(a)
                z = math.cos(t)
                p = v3(radius * sx * x, radius * sy * y, radius * sz * z)
                n = norm(v3(x / sx, y / sy, z / sz))
                return p, n, (0.5 + 0.5 * x, 0.5 - 0.5 * y)
            (p00, n00, u00), (p10, n10, u10) = pt(t0, a0), pt(t0, a1)
            (p01, n01, u01), (p11, n11, u11) = pt(t1, a0), pt(t1, a1)
            if i == 0:
                s.tri(p00, p01, p11, u00, u01, u11, mat, n00, n01, n11)
            else:
                s.quad(p00, p01, p11, p10, u00, u01, u11, u10, mat, n00, n01, n11, n10)
    if back is not None:
        for k in range(segs):
            a0, a1 = 2 * math.pi * k / segs, 2 * math.pi * (k + 1) / segs
            p0 = v3(radius * sx * math.cos(a0), radius * sy * math.sin(a0), 0)
            p1 = v3(radius * sx * math.cos(a1), radius * sy * math.sin(a1), 0)
            s.tri(v3(0, 0, 0), p1, p0, (0.5, 0.5), (0.5, 0.5), (0.5, 0.5), back, v3(0, 0, -1), v3(0, 0, -1), v3(0, 0, -1))
    return s


# ---------------------------------------------------------------------------------------------
# Textures

def canvas(w, h, colour=(0, 0, 0, 255)):
    return PIL.Image.new("RGBA", (w, h), colour)


def noise(w, h, rnd, lo, hi, base, grain=1):
    """A noisy picture of one colour: each texel moves by a random amount between lo and hi (per channel together)."""
    a = np.zeros((h, w, 4), np.uint8)
    for y in range(h):
        for x in range(w):
            d = rnd.randint(lo, hi)
            a[y, x] = [max(0, min(255, base[0] + d)), max(0, min(255, base[1] + d)), max(0, min(255, base[2] + d)), 255]
    return PIL.Image.fromarray(a, "RGBA")


def font(size):
    from PIL import ImageFont
    return ImageFont.load_default(size=size)


def text_centered(im, lines, y0, line_h, colour, size=8):
    d = PIL.ImageDraw.Draw(im)
    d.fontmode = "1"
    f = font(size)
    for i, t in enumerate(lines):
        w = d.textlength(t, font=f)
        d.text(((im.width - w) / 2, y0 + i * line_h), t, fill=colour, font=f)
    return im


def param(s, fn, nu, nv, mat, wrap_u=True, uv=lambda u, v: (u, v), flip=False):
    """A parametric surface. fn(u, v) -> point for u, v in 0..1 (u wraps round when wrap_u). Normals come from the surface's
    slopes. `uv` maps (u, v) to texture coordinates. Degenerate triangles at poles are dropped. flip reverses the winding."""
    eps = 1e-3

    def at(i, j):
        u, v = i / nu, j / nv
        p = Vec(fn(u, v), dtype=np.float64)
        du = Vec(fn(min(u + eps, 1.0 + (eps if wrap_u else 0)), v), dtype=np.float64) - Vec(fn(max(u - eps, -eps if wrap_u else 0), v), dtype=np.float64)
        dv = Vec(fn(u, min(v + eps, 1.0)), dtype=np.float64) - Vec(fn(u, max(v - eps, 0.0)), dtype=np.float64)
        n = np.cross(du, dv)
        if np.linalg.norm(n) < 1e-12:
            # at a pole: use the neighbouring ring's direction from the surface point
            n = np.zeros(3)
        return p, n, uv(u, v)
    grid = [[at(i, j) for j in range(nv + 1)] for i in range(nu + 1)]
    # replace zero normals (poles) by the average of the ring next to them
    for j in range(nv + 1):
        for i in range(nu + 1):
            if np.linalg.norm(grid[i][j][1]) < 1e-12:
                jj = j + 1 if j == 0 else j - 1
                avg = sum(grid[k][jj][1] for k in range(nu + 1))
                grid[i][j] = (grid[i][j][0], avg, grid[i][j][2])
    sgn = -1.0 if flip else 1.0
    for i in range(nu):
        for j in range(nv):
            a, b, c, d = grid[i][j], grid[i + 1][j], grid[i + 1][j + 1], grid[i][j + 1]
            quad = [a, b, c, d]
            if flip:
                quad = [a, d, c, b]
            na = [norm(q[1]) * sgn if np.linalg.norm(q[1]) > 0 else None for q in quad]
            s.tri(quad[0][0], quad[1][0], quad[2][0], quad[0][2], quad[1][2], quad[2][2], mat, na[0], na[1], na[2])
            s.tri(quad[0][0], quad[2][0], quad[3][0], quad[0][2], quad[2][2], quad[3][2], mat, na[0], na[2], na[3])
    return s


def prism(s, poly, y0, y1, side, top, bottom=None, u_scale=1.0, v_scale=1.0, top_uv=None):
    """A convex polygon (x, z points, counter-clockwise seen from above) extruded from y0 to y1. Side quads get the whole `side`
    picture along their length (u) and height (v); the top is a fan with texture coordinates from `top_uv` (x, z -> u, v)."""
    n = len(poly)
    for i in range(n):
        (x0, z0), (x1, z1) = poly[i], poly[(i + 1) % n]
        L = math.hypot(x1 - x0, z1 - z0)
        nrm = norm(v3(z1 - z0, 0, -(x1 - x0)))
        s.quad(v3(x0, y0, z0), v3(x0, y1, z0), v3(x1, y1, z1), v3(x1, y0, z1),
               (0, v_scale), (0, 0), (u_scale * L / 40.0, 0), (u_scale * L / 40.0, v_scale), side, nrm, nrm, nrm, nrm)
    tuv = top_uv or (lambda x, z: (x / 40.0 + 0.5, z / 40.0 + 0.5))
    cx = sum(p[0] for p in poly) / n
    cz = sum(p[1] for p in poly) / n
    for i in range(n):
        (x0, z0), (x1, z1) = poly[i], poly[(i + 1) % n]
        s.tri(v3(cx, y1, cz), v3(x1, y1, z1), v3(x0, y1, z0), tuv(cx, cz), tuv(x1, z1), tuv(x0, z0), top, v3(0, 1, 0), v3(0, 1, 0), v3(0, 1, 0))
        if bottom is not None:
            s.tri(v3(cx, y0, cz), v3(x0, y0, z0), v3(x1, y0, z1), tuv(cx, cz), tuv(x0, z0), tuv(x1, z1), bottom, v3(0, -1, 0), v3(0, -1, 0), v3(0, -1, 0))
    return s


def stone(rnd, w=32, h=32, base=(96, 104, 98), moss=True):
    im = noise(w, h, rnd, -18, 14, base)
    if moss:
        d = PIL.ImageDraw.Draw(im)
        for _ in range(w * h // 40):
            x, y = rnd.randrange(w), rnd.randrange(h)
            d.point((x, y), fill=(52 + rnd.randint(0, 20), 80 + rnd.randint(0, 24), 48, 255))
    return im


def wood(rnd, w=16, h=32, base=(74, 52, 36)):
    """Dark wood with vertical grain."""
    a = np.zeros((h, w, 4), np.uint8)
    cols = [rnd.randint(-14, 12) for _ in range(w)]
    for y in range(h):
        for x in range(w):
            d = cols[x] + rnd.randint(-5, 5)
            a[y, x] = [max(0, min(255, base[0] + d)), max(0, min(255, base[1] + d)), max(0, min(255, base[2] + d)), 255]
    return PIL.Image.fromarray(a, "RGBA")
