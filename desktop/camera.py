"""The view's camera, its orbit controls (with screen-space panning) and the fly camera."""
import math
import numpy as np
from . import vec


class Camera:
    def __init__(self, fov=55.0, near=1.0, far=60000.0):
        self.fov, self.near, self.far = fov, near, far
        self.pos = vec.v3(400, 600, 900)
        self.quat = vec.quat_identity()
        self.width, self.height = 1, 1

    @property
    def aspect(self):
        return self.width / max(1, self.height)

    def look_at(self, target):
        self.quat = vec.look_rotation(self.pos, target)

    def forward(self):
        return vec.quat_rotate(self.quat, (0, 0, -1))

    def right(self):
        return vec.quat_rotate(self.quat, (1, 0, 0))

    def up(self):
        return vec.quat_rotate(self.quat, (0, 1, 0))

    def view(self):
        return np.linalg.inv(vec.compose(self.pos, self.quat, (1, 1, 1)))

    def proj(self, width=None, height=None):
        aspect = (width or self.width) / max(1, height or self.height)
        return vec.perspective(self.fov, aspect, self.near, self.far)

    def project(self, p):
        """World point -> (ndc x, ndc y, ndc z)"""
        c = self.proj() @ self.view() @ np.append(np.asarray(p, float), 1.0)
        return c[:3] / c[3] if c[3] else np.array([0.0, 0.0, 2.0])

    def to_pixels(self, p):
        n = self.project(p)
        return np.array([(n[0] + 1) / 2 * self.width, (1 - n[1]) / 2 * self.height, n[2]])

    def ray(self, x, y):
        """The ray through pixel (x, y): origin, unit direction"""
        nx, ny = x / max(1, self.width) * 2 - 1, -(y / max(1, self.height) * 2 - 1)
        return self.ray_ndc(nx, ny)

    def ray_ndc(self, nx, ny):
        inv = np.linalg.inv(self.proj() @ self.view())
        a = inv @ np.array([nx, ny, -1.0, 1.0])
        b = inv @ np.array([nx, ny, 1.0, 1.0])
        a, b = a[:3] / a[3], b[:3] / b[3]
        d = b - a
        return self.pos.copy(), d / np.linalg.norm(d)

    def segment_hit(self, a, b, origin, direction, px):
        """The nearest of the segments a[i]-b[i] that passes within px pixels of the ray's pixel: (distance, point)"""
        best = None
        far = origin + direction * 1000
        sp = self.to_pixels(far)[:2]
        for p, q in zip(a, b):
            pp, qq = self.to_pixels(p), self.to_pixels(q)
            if pp[2] > 1 or qq[2] > 1 or pp[2] < -1 and qq[2] < -1:
                continue
            d = qq[:2] - pp[:2]
            L = float(d @ d)
            t = 0.0 if L == 0 else max(0.0, min(1.0, float((sp - pp[:2]) @ d) / L))
            if np.linalg.norm(pp[:2] + d * t - sp) <= px:
                w = p + (q - p) * t
                dist = float(np.linalg.norm(w - origin))
                if best is None or dist < best[0]:
                    best = (dist, w)
        return best


class Orbit:
    """Left drag turns about the target, right drag pans in the screen's plane, the wheel moves in and out"""

    def __init__(self, camera):
        self.camera = camera
        self.target = vec.v3()
        self.enabled = True

    def update(self):
        self.camera.look_at(self.target)

    def rotate(self, dx, dy):
        c = self.camera
        off = c.pos - self.target
        r = np.linalg.norm(off) or 1e-6
        theta = math.atan2(off[0], off[2])
        phi = math.acos(max(-1.0, min(1.0, off[1] / r)))
        theta -= 2 * math.pi * dx / max(1, c.height)
        phi -= 2 * math.pi * dy / max(1, c.height)
        phi = max(1e-6, min(math.pi - 1e-6, phi))
        c.pos = self.target + r * np.array([math.sin(phi) * math.sin(theta), math.cos(phi), math.sin(phi) * math.cos(theta)])
        self.update()

    def pan(self, dx, dy):
        c = self.camera
        dist = np.linalg.norm(c.pos - self.target) * math.tan(math.radians(c.fov) / 2)
        move = (-2 * dx * dist / max(1, c.height)) * c.right() + (2 * dy * dist / max(1, c.height)) * c.up()
        c.pos = c.pos + move
        self.target = self.target + move

    def dolly(self, steps):
        """steps > 0 moves in"""
        c = self.camera
        off = c.pos - self.target
        scale = 0.95 ** steps
        r = max(1.0, min(1e6, np.linalg.norm(off) * scale))
        c.pos = self.target + off / (np.linalg.norm(off) or 1) * r
        self.update()
