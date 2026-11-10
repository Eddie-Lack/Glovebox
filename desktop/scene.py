"""A small scene graph for the 3D view: nodes with transforms, and the meshes, lines and sprites they draw.

It is plain data. desktop/render.py draws it with OpenGL; this module also finds what a ray hits (picking, the ground
under a point) and the boxes around things, on the CPU with numpy.
"""
import itertools, math
import numpy as np
from . import vec

_ids = itertools.count(1)


class Geometry:
    """Vertex data: positions (n, 3), and optionally normals, uvs, colors (n, 4) and triangle indices (m, 3)"""

    def __init__(self, positions, indices=None, normals=None, uvs=None, colors=None):
        self.id = next(_ids)
        self.version = 0
        self.positions = np.asarray(positions, np.float32).reshape(-1, 3)
        self.indices = None if indices is None else np.asarray(indices, np.uint32).reshape(-1, 3)
        self.normals = None if normals is None else np.asarray(normals, np.float32).reshape(-1, 3)
        self.uvs = None if uvs is None else np.asarray(uvs, np.float32).reshape(-1, 2)
        self.colors = None if colors is None else np.asarray(colors, np.float32).reshape(-1, 4)
        self._box = None

    def box(self):
        if self._box is None:
            self._box = vec.Box().add_points(self.positions)
        return self._box

    def triangles(self):
        """(m, 3, 3) corner positions"""
        if self.indices is not None:
            return self.positions[self.indices]
        n = len(self.positions) // 3 * 3
        return self.positions[:n].reshape(-1, 3, 3)


class Material:
    def __init__(self, color=(1, 1, 1), texture=None, vertex_colors=False, lit=True, alpha_test=0.0, transparent=False,
                 opacity=1.0, depth_write=True, depth_test=True, double_sided=True, wrap=(10497, 10497)):
        self.color = tuple(color)
        self.texture = texture        # a Texture, or None
        self.vertex_colors = vertex_colors
        self.lit = lit
        self.alpha_test = alpha_test
        self.transparent = transparent
        self.opacity = opacity
        self.depth_write = depth_write
        self.depth_test = depth_test
        self.double_sided = double_sided
        self.wrap = wrap              # GL wrap modes (s, t)

    def copy(self, **kw):
        m = Material.__new__(Material)
        m.__dict__.update(self.__dict__)
        m.__dict__.update(kw)
        return m


class Texture:
    """RGBA pixels (h, w, 4) uint8, uploaded once"""

    def __init__(self, rgba, srgb=True):
        self.id = next(_ids)
        self.rgba = np.ascontiguousarray(rgba, np.uint8)
        self.srgb = srgb


class Mesh:
    def __init__(self, geometry, material, matrix=None):
        self.geometry, self.material = geometry, material
        self.matrix = np.eye(4) if matrix is None else matrix   # within its node


class Lines:
    """mode: 'strip', 'loop' or 'segments'. dash: (dash, gap) in world units, or None."""

    def __init__(self, points, color, mode="strip", dash=None, depth_test=True, opacity=1.0):
        self.id = next(_ids)
        self.version = 0
        self.color = color
        self.mode = mode
        self.dash = dash
        self.depth_test = depth_test
        self.opacity = opacity
        self.matrix = np.eye(4)
        self.set_points(points)

    def set_points(self, points):
        self.points = np.asarray(points, np.float32).reshape(-1, 3)
        self.version += 1

    def segments(self):
        p = self.points
        if self.mode == "segments":
            return p[0::2][:len(p) // 2], p[1::2][:len(p) // 2]
        if self.mode == "loop" and len(p) > 1:
            return p, np.roll(p, -1, 0)
        return p[:-1], p[1:]


class Sprite:
    """A camera-facing square, size world units across (three.js Sprite)"""

    def __init__(self, texture, size, opacity=1.0, depth_test=True):
        self.texture, self.size, self.opacity, self.depth_test = texture, size, opacity, depth_test
        self.matrix = np.eye(4)


class Node:
    def __init__(self, name=""):
        self.name = name
        self.pos = vec.v3()
        self.quat = vec.quat_identity()
        self.scale = np.ones(3)
        self.children = []
        self.parent = None
        self.items = []
        self.visible = True
        self.entity = None
        self.order = 0           # drawn after lower orders; > 0 draws over everything

    def add(self, *nodes):
        for n in nodes:
            if n.parent is not None:
                n.parent.children.remove(n)
            n.parent = self
            self.children.append(n)
        return self

    def remove(self, n):
        if n in self.children:
            self.children.remove(n)
            n.parent = None

    def clear(self):
        for c in self.children:
            c.parent = None
        self.children = []

    def local_matrix(self):
        return vec.compose(self.pos, self.quat, self.scale)

    def world_matrix(self):
        m = self.local_matrix()
        p = self.parent
        while p is not None:
            m = p.local_matrix() @ m
            p = p.parent
        return m

    def world_pos(self):
        return self.world_matrix()[:3, 3].copy()

    def shown(self):
        n = self
        while n is not None:
            if not n.visible:
                return False
            n = n.parent
        return True

    def walk(self, m=None, visible_only=True):
        """(node, world matrix) for this node and every node under it"""
        m = self.local_matrix() if m is None else m
        if visible_only and not self.visible:
            return
        yield self, m
        for c in self.children:
            yield from c.walk(m @ c.local_matrix(), visible_only)

    def world_walk(self, visible_only=True):
        base = self.parent.world_matrix() if self.parent is not None else np.eye(4)
        return self.walk(base @ self.local_matrix(), visible_only)

    def box(self):
        """The world box around everything under this node (three.js Box3.setFromObject)"""
        b = vec.Box()
        for n, m in self.world_walk(visible_only=False):
            for it in n.items:
                if isinstance(it, Mesh):
                    b.add_points(transform_points(m @ it.matrix, it.geometry.box().corners()))
                elif isinstance(it, Lines):
                    b.add_points(transform_points(m @ it.matrix, it.points))
                elif isinstance(it, Sprite):
                    mm = m @ it.matrix
                    c = mm[:3, 3]
                    s = vec.decompose_scale(mm) * it.size / 2
                    b.add_points([c - s, c + s])
        return b


def transform_points(m, pts):
    pts = np.asarray(pts, float).reshape(-1, 3)
    return pts @ m[:3, :3].T + m[:3, 3]


# ---------------------------------------------------------------------------------------------------------------
# Shapes

def box_geometry(w=1.0, h=1.0, d=1.0):
    p, idx = [], []
    for axis in range(3):
        for sign in (-1, 1):
            n = np.zeros(3)
            n[axis] = sign
            u, v = np.roll(np.eye(3), 1, 0)[axis], np.roll(np.eye(3), 2, 0)[axis]
            base = len(p)
            for du, dv in ((-1, -1), (1, -1), (1, 1), (-1, 1)):
                p.append((n + du * u + dv * v) * 0.5)
            q = [base, base + 1, base + 2, base, base + 2, base + 3]
            idx += q if sign > 0 else q[::-1]
    pts = np.array(p) * [w, h, d]
    return Geometry(pts, idx, normals=_flat_normals(pts, idx))


def _flat_normals(pts, idx):
    pts = np.asarray(pts, float)
    n = np.zeros_like(pts)
    t = np.asarray(idx).reshape(-1, 3)
    fn = np.cross(pts[t[:, 1]] - pts[t[:, 0]], pts[t[:, 2]] - pts[t[:, 0]])
    for k in range(3):
        np.add.at(n, t[:, k], fn)
    return n / np.maximum(np.linalg.norm(n, axis=1, keepdims=True), 1e-9)


def box_edges(w=1.0, h=1.0, d=1.0):
    c = np.array([[x, y, z] for x in (-.5, .5) for y in (-.5, .5) for z in (-.5, .5)]) * [w, h, d]
    pairs = [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)]
    return np.array([c[i] for pair in pairs for i in pair])


def sphere_geometry(r=1.0, ws=24, hs=16):
    pts, idx = [], []
    for j in range(hs + 1):
        th = math.pi * j / hs
        for i in range(ws + 1):
            ph = 2 * math.pi * i / ws
            pts.append((-r * math.cos(ph) * math.sin(th), r * math.cos(th), r * math.sin(ph) * math.sin(th)))
    for j in range(hs):
        for i in range(ws):
            a, b = j * (ws + 1) + i, (j + 1) * (ws + 1) + i
            idx += [a, b, a + 1, b, b + 1, a + 1]
    pts = np.array(pts)
    return Geometry(pts, idx, normals=pts / max(r, 1e-9))


def cylinder_geometry(r=1.0, h=1.0, n=32):
    pts, idx = [], []
    for i in range(n + 1):
        a = 2 * math.pi * i / n
        pts += [(r * math.sin(a), h / 2, r * math.cos(a)), (r * math.sin(a), -h / 2, r * math.cos(a))]
    for i in range(n):
        a = 2 * i
        idx += [a, a + 1, a + 2, a + 1, a + 3, a + 2]
    top, bot = len(pts), len(pts) + 1
    pts += [(0, h / 2, 0), (0, -h / 2, 0)]
    for i in range(n):
        idx += [top, 2 * i, 2 * i + 2, bot, 2 * i + 3, 2 * i + 1]
    pts = np.array(pts)
    return Geometry(pts, idx, normals=_flat_normals(pts, idx))


def octahedron_geometry(r=1.0):
    p = np.array([[r, 0, 0], [-r, 0, 0], [0, r, 0], [0, -r, 0], [0, 0, r], [0, 0, -r]])
    idx = [0, 2, 4, 0, 4, 3, 0, 3, 5, 0, 5, 2, 1, 2, 5, 1, 5, 3, 1, 3, 4, 1, 4, 2]
    return Geometry(p, idx, normals=_flat_normals(p, idx))


def ring_points(axis, offset=0.0, n=48):
    a = np.linspace(0, 2 * math.pi, n, endpoint=False)
    c, s = np.cos(a), np.sin(a)
    o = np.full(n, offset)
    if axis == "x":
        return np.stack([o, c, s], 1)
    if axis == "y":
        return np.stack([c, o, s], 1)
    return np.stack([c, s, o], 1)


def mesh_node(geometry, material):
    n = Node()
    n.items.append(Mesh(geometry, material))
    return n


def lines_node(points, color, mode="strip", dash=None, depth_test=True, opacity=1.0):
    n = Node()
    n.items.append(Lines(points, color, mode, dash, depth_test, opacity))
    return n


# ---------------------------------------------------------------------------------------------------------------
# Rays

class Hit:
    def __init__(self, distance, point, node, item):
        self.distance, self.point, self.node, self.item = distance, point, node, item


def ray_triangles(origin, direction, tris):
    """Distances along the ray to each triangle it crosses (both faces), or inf (Möller–Trumbore)"""
    v0, v1, v2 = tris[:, 0], tris[:, 1], tris[:, 2]
    e1, e2 = v1 - v0, v2 - v0
    p = np.cross(direction, e2)
    det = np.einsum("ij,ij->i", e1, p)
    ok = np.abs(det) > 1e-12
    inv = np.where(ok, 1.0 / np.where(ok, det, 1.0), 0.0)
    t = origin - v0
    u = np.einsum("ij,ij->i", t, p) * inv
    q = np.cross(t, e1)
    v = (q @ direction) * inv
    d = np.einsum("ij,ij->i", e2, q) * inv
    hit = ok & (u >= 0) & (v >= 0) & (u + v <= 1) & (d > 0)
    return np.where(hit, d, np.inf)


def ray_box(origin, direction, box):
    with np.errstate(divide="ignore", invalid="ignore"):
        inv = 1.0 / direction
        t1 = (box.min - origin) * inv
        t2 = (box.max - origin) * inv
    tmin = np.nanmax(np.minimum(t1, t2))
    tmax = np.nanmin(np.maximum(t1, t2))
    return tmax >= max(tmin, 0.0)


def raycast(roots, origin, direction, near=0.0, far=np.inf, meshes_only=False, camera=None, line_px=None):
    """Every mesh triangle and sprite the ray meets, nearest first. Lines count when they pass within line_px pixels
    on screen (camera: a Camera, for sprites and lines)."""
    origin = np.asarray(origin, float)
    direction = np.asarray(direction, float)
    direction = direction / np.linalg.norm(direction)
    hits = []
    for root in roots:
        for node, m in root.world_walk():
            for it in node.items:
                if isinstance(it, Mesh):
                    mm = m @ it.matrix
                    try:
                        inv = np.linalg.inv(mm)
                    except np.linalg.LinAlgError:
                        continue
                    lo = inv[:3, :3] @ origin + inv[:3, 3]
                    ld = inv[:3, :3] @ direction
                    if not ray_box(lo, ld, it.geometry.box()):
                        continue
                    d = ray_triangles(lo, ld, it.geometry.triangles().astype(float))
                    i = int(np.argmin(d)) if len(d) else -1
                    if i < 0 or not np.isfinite(d[i]):
                        continue
                    pw = transform_points(mm, lo + ld * d[i])[0]
                    dist = float(np.linalg.norm(pw - origin))
                    if near <= dist <= far:
                        hits.append(Hit(dist, pw, node, it))
                elif meshes_only:
                    continue
                elif isinstance(it, Sprite) and camera is not None:
                    mm = m @ it.matrix
                    c = mm[:3, 3]
                    half = (vec.decompose_scale(mm)[0] or 1.0) * it.size / 2
                    # The sprite faces the camera: test the ray against that square
                    to = c - origin
                    t = float(to @ direction)
                    if t <= 0:
                        continue
                    right, up = camera.right(), camera.up()
                    plane_n = -camera.forward()
                    denom = direction @ plane_n
                    if abs(denom) < 1e-9:
                        continue
                    tt = float(((c - origin) @ plane_n) / denom)
                    p = origin + direction * tt
                    if abs((p - c) @ right) <= half and abs((p - c) @ up) <= half and near <= tt <= far:
                        hits.append(Hit(tt, p, node, it))
                elif isinstance(it, Lines) and camera is not None and line_px:
                    a, b = it.segments()
                    if not len(a):
                        continue
                    mm = m @ it.matrix
                    hit = camera.segment_hit(transform_points(mm, a), transform_points(mm, b), origin, direction, line_px)
                    if hit is not None and near <= hit[0] <= far:
                        hits.append(Hit(hit[0], hit[1], node, it))
    hits.sort(key=lambda h: h.distance)
    return hits
