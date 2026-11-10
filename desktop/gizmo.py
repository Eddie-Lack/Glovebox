"""The transform gizmo: arrows to move, rings to rotate and handles to scale an invisible pivot.

It only changes the pivot (pos, quat, scale); the editor works
out the edit from the pivot's change since the drag began. Handles are picked on screen, within a few pixels.
"""
import math
import numpy as np
from . import vec
from .scene import Geometry, Material, Mesh, Node

COLORS = {"X": (1.0, 0.25, 0.25), "Y": (0.35, 0.9, 0.35), "Z": (0.3, 0.5, 1.0), "XYZ": (0.9, 0.9, 0.9)}
HOVER = (1.0, 0.85, 0.2)
AXES = {"X": vec.v3(1, 0, 0), "Y": vec.v3(0, 1, 0), "Z": vec.v3(0, 0, 1)}
PIXELS = 90        # the gizmo's length on screen
PICK = 9           # pixels


def tube(points, radius, closed=False, sides=6):
    """A thin tube along points (the gizmo's lines, which GL draws one pixel wide)"""
    pts = np.asarray(points, float)
    n = len(pts)
    ring = []
    for i in range(n):
        a = pts[(i - 1) % n] if closed or i > 0 else pts[i]
        b = pts[(i + 1) % n] if closed or i < n - 1 else pts[i]
        t = b - a
        t = t / (np.linalg.norm(t) or 1)
        u = np.cross(t, [0, 1, 0] if abs(t[1]) < 0.9 else [1, 0, 0])
        u /= np.linalg.norm(u)
        v = np.cross(t, u)
        for k in range(sides):
            ang = 2 * math.pi * k / sides
            ring.append(pts[i] + radius * (math.cos(ang) * u + math.sin(ang) * v))
    idx = []
    segs = n if closed else n - 1
    for i in range(segs):
        j = (i + 1) % n
        for k in range(sides):
            a, b = i * sides + k, i * sides + (k + 1) % sides
            c, d = j * sides + k, j * sides + (k + 1) % sides
            idx += [a, c, b, b, c, d]
    return Geometry(np.array(ring), idx)


def cone(base, tip, radius, sides=12):
    base, tip = np.asarray(base, float), np.asarray(tip, float)
    t = tip - base
    t /= np.linalg.norm(t)
    u = np.cross(t, [0, 1, 0] if abs(t[1]) < 0.9 else [1, 0, 0])
    u /= np.linalg.norm(u)
    v = np.cross(t, u)
    pts = [tip, base] + [base + radius * (math.cos(2 * math.pi * k / sides) * u + math.sin(2 * math.pi * k / sides) * v)
                         for k in range(sides)]
    idx = []
    for k in range(sides):
        a, b = 2 + k, 2 + (k + 1) % sides
        idx += [0, a, b, 1, b, a]
    return Geometry(np.array(pts), idx)


def cube(centre, half):
    c = np.asarray(centre, float)
    p = np.array([[x, y, z] for x in (-1, 1) for y in (-1, 1) for z in (-1, 1)]) * half + c
    idx = [0, 1, 3, 0, 3, 2, 4, 6, 7, 4, 7, 5, 0, 4, 5, 0, 5, 1, 2, 3, 7, 2, 7, 6, 0, 2, 6, 0, 6, 4, 1, 5, 7, 1, 7, 3]
    return Geometry(p, idx)


def plane_hit(origin, direction, point, normal):
    d = float(direction @ normal)
    if abs(d) < 1e-9:
        return None
    t = float((point - origin) @ normal) / d
    return origin + direction * t if t > 0 else None


class Gizmo:
    def __init__(self):
        self.mode = "translate"
        self.space = "world"
        self.attached = False
        self.enabled = True
        self.pos = vec.v3()
        self.quat = vec.quat_identity()
        self.scale = np.ones(3)
        self.axis = None          # the hovered or dragged handle: X, Y, Z, XY, YZ, XZ or XYZ
        self.dragging = False
        self.snap = {"translate": None, "rotate": None, "scale": None}
        self._drag = None

    def attach(self, pos, quat):
        self.attached = True
        self.pos, self.quat, self.scale = np.array(pos, float), np.array(quat, float), np.ones(3)

    def detach(self):
        self.attached = False
        self.axis = None
        self.dragging = False

    def basis(self):
        """The handles' axes in the world"""
        if self.space == "local":
            m = vec.quat_to_mat3(self.quat)
            return {k: m[:, i] for i, k in enumerate("XYZ")}
        return dict(AXES)

    def size(self, camera):
        d = float(np.linalg.norm(camera.pos - self.pos))
        return d * math.tan(math.radians(camera.fov) / 2) * 2 * PIXELS / max(1, camera.height)

    # -- Handles: name -> polyline in the world (for picking and drawing)

    def handles(self, camera):
        s = self.size(camera)
        b = self.basis()
        c = self.pos
        out = {}
        if self.mode == "translate":
            for k, a in b.items():
                out[k] = ("line", [c + a * s * 0.15, c + a * s])
            for k in ("XY", "YZ", "XZ"):
                u, v = b[k[0]], b[k[1]]
                q = s * 0.3
                out[k] = ("quad", [c + u * q * 0.4 + v * q * 0.4, c + u * q + v * q * 0.4, c + u * q + v * q, c + u * q * 0.4 + v * q])
            out["XYZ"] = ("dot", [c])
        elif self.mode == "rotate":
            for k, a in b.items():
                u = np.cross(a, [0, 1, 0] if abs(a[1]) < 0.9 else [1, 0, 0])
                u /= np.linalg.norm(u)
                v = np.cross(a, u)
                ang = np.linspace(0, 2 * math.pi, 64, endpoint=False)
                out[k] = ("ring", [c + s * 0.8 * (math.cos(t) * u + math.sin(t) * v) for t in ang])
        else:
            for k, a in b.items():
                out[k] = ("line", [c + a * s * 0.15, c + a * s])
            out["XYZ"] = ("dot", [c])
        return out

    def pick(self, camera, x, y):
        if not self.attached or not self.enabled:
            return None
        best, best_d = None, PICK
        for k, (kind, pts) in self.handles(camera).items():
            px = [camera.to_pixels(p) for p in pts]
            if any(p[2] > 1 for p in px):
                continue
            m = np.array([x, y], float)
            if kind == "dot":
                d = np.linalg.norm(px[0][:2] - m) - 6
            elif kind == "quad":
                poly = np.array([p[:2] for p in px])
                d = 0 if point_in_poly(m, poly) else min(seg_dist(m, poly[i], poly[(i + 1) % 4]) for i in range(4))
            else:
                segs = list(zip(px, px[1:] + ([px[0]] if kind == "ring" else [])))
                d = min(seg_dist(m, a[:2], b[:2]) for a, b in segs)
                if kind == "line":
                    d -= 3
            if d < best_d:
                best, best_d = k, d
        return best

    def hover(self, camera, x, y):
        if self.dragging:
            return False
        a = self.pick(camera, x, y)
        changed = a != self.axis
        self.axis = a
        return changed

    # -- Dragging

    def begin(self, camera, x, y):
        a = self.pick(camera, x, y)
        if a is None:
            return False
        self.axis = a
        origin, direction = camera.ray(x, y)
        self._drag = {"axis": a, "pos": self.pos.copy(), "quat": self.quat.copy(), "basis": self.basis(),
                      "plane": self._plane(camera, a), "start": None, "x": x, "y": y}
        self._drag["start"] = plane_hit(origin, direction, self.pos, self._drag["plane"])
        if self._drag["start"] is None:
            self._drag = None
            return False
        self.scale = np.ones(3)
        self.dragging = True
        return True

    def _plane(self, camera, a):
        """The plane the drag moves in: through the pivot, holding the axis and as square to the view as can be"""
        b = self.basis()
        view = -camera.forward()
        if self.mode == "rotate":
            return b[a]
        if a in b:
            axis = b[a]
            n = np.cross(axis, np.cross(view, axis))
            if np.linalg.norm(n) < 1e-6:
                n = np.cross(axis, camera.up())
            return n / np.linalg.norm(n)
        if a == "XYZ":
            return view
        n = np.cross(b[a[0]], b[a[1]])
        return n / np.linalg.norm(n)

    def move(self, camera, x, y):
        if not self.dragging:
            return False
        dr = self._drag
        origin, direction = camera.ray(x, y)
        p = plane_hit(origin, direction, dr["pos"], dr["plane"])
        if p is None:
            return False
        a, b, start = dr["axis"], dr["basis"], dr["start"]
        if self.mode == "translate":
            d = p - start
            if a in b:
                d = b[a] * float(d @ b[a])
            elif a != "XYZ":
                pass
            new = dr["pos"] + d
            snap = self.snap["translate"]
            if snap:
                if self.space == "world":
                    axes = [i for i, k in enumerate("XYZ") if k in a] if a != "XYZ" else [0, 1, 2]
                    for i in axes:
                        new[i] = round(new[i] / snap) * snap
                else:
                    for k in (a if a != "XYZ" else "XYZ"):
                        along = float((new - dr["pos"]) @ b[k])
                        new = new + b[k] * (round(along / snap) * snap - along)
            self.pos = new
        elif self.mode == "rotate":
            u = start - dr["pos"]
            v = p - dr["pos"]
            if np.linalg.norm(u) < 1e-9 or np.linalg.norm(v) < 1e-9:
                return False
            n = b[a]
            ang = math.atan2(float(np.cross(u, v) @ n), float(u @ v))
            snap = self.snap["rotate"]
            if snap:
                ang = round(ang / snap) * snap
            dq = vec.quat_axis_angle(n, ang)
            self.quat = vec.quat_normalize(vec.quat_mul(dq, dr["quat"]))
        else:
            if a == "XYZ":
                # Up and right on screen grow it, as far as the mouse goes
                f = max(0.01, 1 + ((x - dr["x"]) - (y - dr["y"])) / 100)
                s = np.full(3, f)
            else:
                ax = b[a]
                s0 = float((start - dr["pos"]) @ ax)
                s1 = float((p - dr["pos"]) @ ax)
                f = s1 / s0 if abs(s0) > 1e-9 else 1.0
                s = np.ones(3)
                s["XYZ".index(a)] = f
            snap = self.snap["scale"]
            if snap:
                s = np.array([round(v / snap) * snap or snap for v in s])
            self.scale = s
        return True

    def end(self):
        was = self.dragging
        self.dragging = False
        self._drag = None
        return was

    # -- Drawing

    def node(self, camera):
        """The gizmo to draw: unit-sized handles, made once per mode, placed and sized for the view"""
        root = Node("gizmo")
        root.order = 100
        if not self.attached:
            return root
        root.pos = self.pos.copy()
        root.quat = self.quat.copy() if self.space == "local" else vec.quat_identity()
        root.scale = np.full(3, self.size(camera))
        for k, geoms in self._unit().items():
            col = HOVER if k == self.axis else COLORS.get(k, (0.9, 0.9, 0.9))
            mat = Material(color=col, lit=False, depth_test=False)
            for g in geoms:
                root.items.append(Mesh(g, mat.copy(transparent=True, opacity=0.5) if k in ("XY", "YZ", "XZ") else mat))
        return root

    def _unit(self):
        cache = self.__dict__.setdefault("_unit_cache", {})
        if self.mode not in cache:
            saved = (self.pos, self.quat, self.space)
            self.pos, self.quat, self.space = vec.v3(), vec.quat_identity(), "world"

            class Unit:  # a camera at which size() is 1
                pos, fov, height = vec.v3(0, 0, 1 / (2 * math.tan(math.radians(55) / 2)) * 1), 55.0, PIXELS
            out = {}
            r = 0.018
            for k, (kind, pts) in self.handles(Unit).items():
                if kind == "line":
                    a = AXES[k]
                    tip = cone(a * 0.85, a * 1.08, 0.06) if self.mode == "translate" else cube(a, 0.05)
                    out[k] = [tube(pts, r), tip]
                elif kind == "ring":
                    out[k] = [tube(pts, r, closed=True)]
                elif kind == "quad":
                    out[k] = [Geometry(np.array(pts), [0, 1, 2, 0, 2, 3])]
                else:
                    out[k] = [cube(vec.v3(), 0.05)]
            self.pos, self.quat, self.space = saved
            cache[self.mode] = out
        return cache[self.mode]


def seg_dist(p, a, b):
    d = b - a
    L = float(d @ d)
    t = 0.0 if L == 0 else max(0.0, min(1.0, float((p - a) @ d) / L))
    return float(np.linalg.norm(a + d * t - p))


def point_in_poly(p, poly):
    inside = False
    n = len(poly)
    for i in range(n):
        a, b = poly[i], poly[(i + 1) % n]
        if (a[1] > p[1]) != (b[1] > p[1]) and p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1] + 1e-12) + a[0]:
            inside = not inside
    return inside
