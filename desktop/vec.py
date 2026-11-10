"""Vectors, quaternions and matrices as numpy arrays, with three.js's conventions.

Vectors are float arrays [x, y, z]. Quaternions are [x, y, z, w]. Matrices are 4x4 and act on column vectors
(M @ [x, y, z, 1]).
"""
import math
import numpy as np

# The game's rotation order (see EULER_ORDER in desktop/editor.py): R = Ry·Rz·Rx, three.js's Euler order 'YZX'


def v3(x=0.0, y=0.0, z=0.0):
    return np.array([x, y, z], dtype=float)


def quat_identity():
    return np.array([0.0, 0.0, 0.0, 1.0])


def quat_from_euler_yzx(x, y, z):
    """three.js Quaternion.setFromEuler for order 'YZX'"""
    c1, c2, c3 = math.cos(x / 2), math.cos(y / 2), math.cos(z / 2)
    s1, s2, s3 = math.sin(x / 2), math.sin(y / 2), math.sin(z / 2)
    return np.array([s1 * c2 * c3 + c1 * s2 * s3,
                     c1 * s2 * c3 + s1 * c2 * s3,
                     c1 * c2 * s3 - s1 * s2 * c3,
                     c1 * c2 * c3 - s1 * s2 * s3])


def quat_from_euler_yxz(x, y, z):
    """three.js Quaternion.setFromEuler for order 'YXZ' (the fly camera's look)"""
    c1, c2, c3 = math.cos(x / 2), math.cos(y / 2), math.cos(z / 2)
    s1, s2, s3 = math.sin(x / 2), math.sin(y / 2), math.sin(z / 2)
    return np.array([s1 * c2 * c3 + c1 * s2 * s3,
                     c1 * s2 * c3 - s1 * c2 * s3,
                     c1 * c2 * s3 - s1 * s2 * c3,
                     c1 * c2 * c3 + s1 * s2 * s3])


def quat_to_mat3(q):
    x, y, z, w = q
    x2, y2, z2 = x + x, y + y, z + z
    xx, xy, xz = x * x2, x * y2, x * z2
    yy, yz, zz = y * y2, y * z2, z * z2
    wx, wy, wz = w * x2, w * y2, w * z2
    return np.array([[1 - (yy + zz), xy - wz, xz + wy],
                     [xy + wz, 1 - (xx + zz), yz - wx],
                     [xz - wy, yz + wx, 1 - (xx + yy)]])


def euler_yzx_from_quat(q):
    """three.js Euler.setFromQuaternion(q, 'YZX'), as [x, y, z]"""
    m = quat_to_mat3(q)
    m11, m12, m13 = m[0]
    m21, m22, m23 = m[1]
    m31, m32, m33 = m[2]
    z = math.asin(max(-1.0, min(1.0, m21)))
    if abs(m21) < 0.9999999:
        x = math.atan2(-m23, m22)
        y = math.atan2(-m31, m11)
    else:
        x = 0.0
        y = math.atan2(m13, m33)
    return v3(x, y, z)


def euler_yxz_from_quat(q):
    """three.js Euler.setFromQuaternion(q, 'YXZ'), as [x, y, z]"""
    m = quat_to_mat3(q)
    m11, m12, m13 = m[0]
    m21, m22, m23 = m[1]
    m31, m32, m33 = m[2]
    x = math.asin(-max(-1.0, min(1.0, m23)))
    if abs(m23) < 0.9999999:
        y = math.atan2(m13, m33)
        z = math.atan2(m21, m22)
    else:
        y = math.atan2(-m31, m11)
        z = 0.0
    return v3(x, y, z)


def quat_mul(a, b):
    """a·b: b first, then a"""
    ax, ay, az, aw = a
    bx, by, bz, bw = b
    return np.array([ax * bw + aw * bx + ay * bz - az * by,
                     ay * bw + aw * by + az * bx - ax * bz,
                     az * bw + aw * bz + ax * by - ay * bx,
                     aw * bw - ax * bx - ay * by - az * bz])


def quat_inv(q):
    return np.array([-q[0], -q[1], -q[2], q[3]])


def quat_axis_angle(axis, angle):
    axis = np.asarray(axis, float)
    axis = axis / (np.linalg.norm(axis) or 1.0)
    s = math.sin(angle / 2)
    return np.array([axis[0] * s, axis[1] * s, axis[2] * s, math.cos(angle / 2)])


def quat_normalize(q):
    n = np.linalg.norm(q)
    return q / n if n else quat_identity()


def quat_rotate(q, v):
    return quat_to_mat3(q) @ np.asarray(v, float)


def compose(pos, quat, scale):
    m = np.eye(4)
    m[:3, :3] = quat_to_mat3(quat) * np.asarray(scale, float)[None, :]
    m[:3, 3] = pos
    return m


def decompose_scale(m):
    return np.linalg.norm(m[:3, :3], axis=0)


def translate(m, p):
    return (m @ np.append(np.asarray(p, float), 1.0))[:3]


def perspective(fov_deg, aspect, near, far):
    f = 1.0 / math.tan(math.radians(fov_deg) / 2)
    m = np.zeros((4, 4))
    m[0, 0] = f / aspect
    m[1, 1] = f
    m[2, 2] = (far + near) / (near - far)
    m[2, 3] = 2 * far * near / (near - far)
    m[3, 2] = -1
    return m


def look_rotation(eye, target, up=(0.0, 1.0, 0.0)):
    """The quaternion of a camera at eye looking at target (three.js Object3D.lookAt for cameras)"""
    z = np.asarray(eye, float) - np.asarray(target, float)
    if not np.linalg.norm(z):
        z = v3(0, 0, 1)
    z = z / np.linalg.norm(z)
    x = np.cross(up, z)
    if not np.linalg.norm(x):
        z = z + v3(1e-4, 0, 0)
        x = np.cross(up, z)
    x = x / np.linalg.norm(x)
    y = np.cross(z, x)
    return mat3_to_quat(np.stack([x, y, z], 1))


def mat3_to_quat(m):
    m11, m12, m13 = m[0]
    m21, m22, m23 = m[1]
    m31, m32, m33 = m[2]
    trace = m11 + m22 + m33
    if trace > 0:
        s = 0.5 / math.sqrt(trace + 1.0)
        return np.array([(m32 - m23) * s, (m13 - m31) * s, (m21 - m12) * s, 0.25 / s])
    if m11 > m22 and m11 > m33:
        s = 2.0 * math.sqrt(1.0 + m11 - m22 - m33)
        return np.array([0.25 * s, (m12 + m21) / s, (m13 + m31) / s, (m32 - m23) / s])
    if m22 > m33:
        s = 2.0 * math.sqrt(1.0 + m22 - m11 - m33)
        return np.array([(m12 + m21) / s, 0.25 * s, (m23 + m32) / s, (m13 - m31) / s])
    s = 2.0 * math.sqrt(1.0 + m33 - m11 - m22)
    return np.array([(m13 + m31) / s, (m23 + m32) / s, 0.25 * s, (m21 - m12) / s])


class Box:
    """An axis-aligned box (three.js Box3)"""

    def __init__(self):
        self.min = np.full(3, np.inf)
        self.max = np.full(3, -np.inf)

    def empty(self):
        return bool(np.any(self.min > self.max))

    def add_points(self, pts):
        pts = np.asarray(pts, float).reshape(-1, 3)
        if len(pts):
            self.min = np.minimum(self.min, pts.min(0))
            self.max = np.maximum(self.max, pts.max(0))
        return self

    def add_box(self, other):
        if not other.empty():
            self.min = np.minimum(self.min, other.min)
            self.max = np.maximum(self.max, other.max)
        return self

    def center(self):
        return (self.min + self.max) / 2

    def size(self):
        return self.max - self.min

    def sphere_radius(self):
        return float(np.linalg.norm(self.size()) / 2)

    def corners(self):
        a, b = self.min, self.max
        return np.array([[x, y, z] for x in (a[0], b[0]) for y in (a[1], b[1]) for z in (a[2], b[2])])
