//! Vectors, quaternions and matrices with three.js's conventions.
//!
//! A vector is `[x, y, z]`. A quaternion is `[x, y, z, w]`. A matrix is row-major and acts on column vectors
//! (`M · [x, y, z, 1]`). The game's rotation order is R = Ry·Rz·Rx, three.js's Euler order 'YZX'.
//!
//! The formulas are the ones of the Python editor (`desktop/vec.py`), in the same order of operations. Where Python
//! called numpy's dot products, the sums here are fused in the order the reference machine's numpy used, and the
//! results are the same doubles.

pub type V3 = [f64; 3];
pub type Quat = [f64; 4];
pub type Mat3 = [[f64; 3]; 3];
pub type Mat4 = [[f64; 4]; 4];

pub fn v3(x: f64, y: f64, z: f64) -> V3 {
    [x, y, z]
}

pub fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub fn scale(a: V3, s: f64) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}

pub fn div(a: V3, s: f64) -> V3 {
    [a[0] / s, a[1] / s, a[2] / s]
}

/// The dot product, summed as numpy's `a @ b` sums it.
pub fn dot(a: V3, b: V3) -> f64 {
    a[2].mul_add(b[2], a[1].mul_add(b[1], a[0] * b[0]))
}

/// The length, as numpy's `linalg.norm` computes it.
pub fn norm(a: V3) -> f64 {
    dot(a, a).sqrt()
}

/// numpy's `cross(a, b)`.
pub fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// A 3x3 matrix times a vector, summed as numpy's `m @ v` sums it.
pub fn mat3_mul_v(m: &Mat3, v: V3) -> V3 {
    let row = |r: &[f64; 3]| r[2].mul_add(v[2], r[0].mul_add(v[0], r[1] * v[1]));
    [row(&m[0]), row(&m[1]), row(&m[2])]
}

pub fn quat_identity() -> Quat {
    [0.0, 0.0, 0.0, 1.0]
}

/// three.js `Quaternion.setFromEuler` for order 'YZX'.
pub fn quat_from_euler_yzx(x: f64, y: f64, z: f64) -> Quat {
    let (c1, c2, c3) = ((x / 2.0).cos(), (y / 2.0).cos(), (z / 2.0).cos());
    let (s1, s2, s3) = ((x / 2.0).sin(), (y / 2.0).sin(), (z / 2.0).sin());
    [
        s1 * c2 * c3 + c1 * s2 * s3,
        c1 * s2 * c3 + s1 * c2 * s3,
        c1 * c2 * s3 - s1 * s2 * c3,
        c1 * c2 * c3 - s1 * s2 * s3,
    ]
}

/// three.js `Quaternion.setFromEuler` for order 'YXZ' (the fly camera's look).
pub fn quat_from_euler_yxz(x: f64, y: f64, z: f64) -> Quat {
    let (c1, c2, c3) = ((x / 2.0).cos(), (y / 2.0).cos(), (z / 2.0).cos());
    let (s1, s2, s3) = ((x / 2.0).sin(), (y / 2.0).sin(), (z / 2.0).sin());
    [
        s1 * c2 * c3 + c1 * s2 * s3,
        c1 * s2 * c3 - s1 * c2 * s3,
        c1 * c2 * s3 - s1 * s2 * c3,
        c1 * c2 * c3 + s1 * s2 * s3,
    ]
}

pub fn quat_to_mat3(q: Quat) -> Mat3 {
    let [x, y, z, w] = q;
    let (x2, y2, z2) = (x + x, y + y, z + z);
    let (xx, xy, xz) = (x * x2, x * y2, x * z2);
    let (yy, yz, zz) = (y * y2, y * z2, z * z2);
    let (wx, wy, wz) = (w * x2, w * y2, w * z2);
    [
        [1.0 - (yy + zz), xy - wz, xz + wy],
        [xy + wz, 1.0 - (xx + zz), yz - wx],
        [xz - wy, yz + wx, 1.0 - (xx + yy)],
    ]
}

/// Python's `max(-1.0, min(1.0, v))`. A NaN gives 1.0, as in Python.
pub fn clamp1(v: f64) -> f64 {
    let m = if v < 1.0 { v } else { 1.0 };
    if m > -1.0 {
        m
    } else {
        -1.0
    }
}

/// three.js `Euler.setFromQuaternion(q, 'YZX')`, as `[x, y, z]`.
pub fn euler_yzx_from_quat(q: Quat) -> V3 {
    let m = quat_to_mat3(q);
    let (m11, m13) = (m[0][0], m[0][2]);
    let (m21, m22, m23) = (m[1][0], m[1][1], m[1][2]);
    let (m31, m33) = (m[2][0], m[2][2]);
    let z = clamp1(m21).asin();
    if m21.abs() < 0.9999999 {
        [(-m23).atan2(m22), (-m31).atan2(m11), z]
    } else {
        [0.0, m13.atan2(m33), z]
    }
}

/// three.js `Euler.setFromQuaternion(q, 'YXZ')`, as `[x, y, z]`.
pub fn euler_yxz_from_quat(q: Quat) -> V3 {
    let m = quat_to_mat3(q);
    let (m11, m13) = (m[0][0], m[0][2]);
    let (m21, m22, m23) = (m[1][0], m[1][1], m[1][2]);
    let (m31, m33) = (m[2][0], m[2][2]);
    let x = (-clamp1(m23)).asin();
    if m23.abs() < 0.9999999 {
        [x, m13.atan2(m33), m21.atan2(m22)]
    } else {
        [x, (-m31).atan2(m11), 0.0]
    }
}

/// a·b: b first, then a.
pub fn quat_mul(a: Quat, b: Quat) -> Quat {
    let [ax, ay, az, aw] = a;
    let [bx, by, bz, bw] = b;
    [
        ax * bw + aw * bx + ay * bz - az * by,
        ay * bw + aw * by + az * bx - ax * bz,
        az * bw + aw * bz + ax * by - ay * bx,
        aw * bw - ax * bx - ay * by - az * bz,
    ]
}

pub fn quat_inv(q: Quat) -> Quat {
    [-q[0], -q[1], -q[2], q[3]]
}

pub fn quat_axis_angle(axis: V3, angle: f64) -> Quat {
    let n = norm(axis);
    let axis = div(axis, if n != 0.0 { n } else { 1.0 });
    let s = (angle / 2.0).sin();
    [axis[0] * s, axis[1] * s, axis[2] * s, (angle / 2.0).cos()]
}

pub fn quat_normalize(q: Quat) -> Quat {
    let n = q[3].mul_add(q[3], q[2].mul_add(q[2], q[1].mul_add(q[1], q[0] * q[0]))).sqrt();
    if n != 0.0 {
        [q[0] / n, q[1] / n, q[2] / n, q[3] / n]
    } else {
        quat_identity()
    }
}

pub fn quat_rotate(q: Quat, v: V3) -> V3 {
    mat3_mul_v(&quat_to_mat3(q), v)
}

/// The matrix T·R·S.
pub fn compose(pos: V3, quat: Quat, scale: V3) -> Mat4 {
    let r = quat_to_mat3(quat);
    let mut m = [[0.0; 4]; 4];
    for i in 0..3 {
        for j in 0..3 {
            m[i][j] = r[i][j] * scale[j];
        }
        m[i][3] = pos[i];
    }
    m[3][3] = 1.0;
    m
}

/// The lengths of the matrix's three axes.
pub fn decompose_scale(m: &Mat4) -> V3 {
    let col = |j: usize| (m[0][j] * m[0][j] + m[1][j] * m[1][j] + m[2][j] * m[2][j]).sqrt();
    [col(0), col(1), col(2)]
}

/// A 4x4 matrix times a 4-vector, summed as numpy's `m @ v` sums it.
pub fn mat4_mul_v(m: &Mat4, v: [f64; 4]) -> [f64; 4] {
    let row = |r: &[f64; 4]| (r[0] * v[0] + r[2] * v[2]) + (r[1] * v[1] + r[3] * v[3]);
    [row(&m[0]), row(&m[1]), row(&m[2]), row(&m[3])]
}

/// The product a·b of two 4x4 matrices, summed as numpy's `a @ b` sums it.
pub fn mat4_mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut m = [[0.0; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            let mut s = a[i][0] * b[0][j];
            for k in 1..4 {
                s = a[i][k].mul_add(b[k][j], s);
            }
            m[i][j] = s;
        }
    }
    m
}

/// The point p moved by the matrix.
pub fn translate(m: &Mat4, p: V3) -> V3 {
    let r = mat4_mul_v(m, [p[0], p[1], p[2], 1.0]);
    [r[0], r[1], r[2]]
}

pub fn perspective(fov_deg: f64, aspect: f64, near: f64, far: f64) -> Mat4 {
    let f = 1.0 / (radians(fov_deg) / 2.0).tan();
    let mut m = [[0.0; 4]; 4];
    m[0][0] = f / aspect;
    m[1][1] = f;
    m[2][2] = (far + near) / (near - far);
    m[2][3] = 2.0 * far * near / (near - far);
    m[3][2] = -1.0;
    m
}

/// Python's `math.radians`: the value times (pi / 180).
pub fn radians(deg: f64) -> f64 {
    deg * (std::f64::consts::PI / 180.0)
}

/// The inverse of a 4x4 matrix, by Gauss-Jordan elimination with partial pivoting. A singular matrix gives `None`.
pub fn mat4_inv(m: &Mat4) -> Option<Mat4> {
    let mut a = *m;
    let mut inv = [[0.0; 4]; 4];
    for (i, row) in inv.iter_mut().enumerate() {
        row[i] = 1.0;
    }
    for c in 0..4 {
        let mut p = c;
        for r in c + 1..4 {
            if a[r][c].abs() > a[p][c].abs() {
                p = r;
            }
        }
        if a[p][c] == 0.0 || !a[p][c].is_finite() {
            return None;
        }
        a.swap(c, p);
        inv.swap(c, p);
        let d = a[c][c];
        for j in 0..4 {
            a[c][j] /= d;
            inv[c][j] /= d;
        }
        for r in 0..4 {
            if r != c {
                let f = a[r][c];
                if f != 0.0 {
                    for j in 0..4 {
                        a[r][j] -= f * a[c][j];
                        inv[r][j] -= f * inv[c][j];
                    }
                }
            }
        }
    }
    Some(inv)
}

/// The quaternion of a camera at `eye` that looks at `target` (three.js `Object3D.lookAt` for cameras).
pub fn look_rotation(eye: V3, target: V3, up: V3) -> Quat {
    let mut z = sub(eye, target);
    if norm(z) == 0.0 {
        z = [0.0, 0.0, 1.0];
    }
    z = div(z, norm(z));
    let mut x = cross(up, z);
    if norm(x) == 0.0 {
        z = add(z, [1e-4, 0.0, 0.0]);
        x = cross(up, z);
    }
    x = div(x, norm(x));
    let y = cross(z, x);
    mat3_to_quat(&[[x[0], y[0], z[0]], [x[1], y[1], z[1]], [x[2], y[2], z[2]]])
}

pub const UP: V3 = [0.0, 1.0, 0.0];

pub fn mat3_to_quat(m: &Mat3) -> Quat {
    let [m11, m12, m13] = m[0];
    let [m21, m22, m23] = m[1];
    let [m31, m32, m33] = m[2];
    let trace = m11 + m22 + m33;
    if trace > 0.0 {
        let s = 0.5 / (trace + 1.0).sqrt();
        return [(m32 - m23) * s, (m13 - m31) * s, (m21 - m12) * s, 0.25 / s];
    }
    if m11 > m22 && m11 > m33 {
        let s = 2.0 * (1.0 + m11 - m22 - m33).sqrt();
        return [0.25 * s, (m12 + m21) / s, (m13 + m31) / s, (m32 - m23) / s];
    }
    if m22 > m33 {
        let s = 2.0 * (1.0 + m22 - m11 - m33).sqrt();
        return [(m12 + m21) / s, 0.25 * s, (m23 + m32) / s, (m13 - m31) / s];
    }
    let s = 2.0 * (1.0 + m33 - m11 - m22).sqrt();
    [(m13 + m31) / s, (m23 + m32) / s, 0.25 * s, (m21 - m12) / s]
}

/// An axis-aligned box (three.js `Box3`). A new box is empty.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Box3 {
    pub min: V3,
    pub max: V3,
}

impl Default for Box3 {
    fn default() -> Self {
        Box3 { min: [f64::INFINITY; 3], max: [f64::NEG_INFINITY; 3] }
    }
}

impl Box3 {
    pub fn new() -> Box3 {
        Box3::default()
    }

    pub fn empty(&self) -> bool {
        (0..3).any(|i| self.min[i] > self.max[i])
    }

    pub fn add_point(&mut self, p: V3) -> &mut Box3 {
        for i in 0..3 {
            // numpy's minimum and maximum keep a NaN
            self.min[i] = np_min(self.min[i], p[i]);
            self.max[i] = np_max(self.max[i], p[i]);
        }
        self
    }

    pub fn add_points(&mut self, pts: &[V3]) -> &mut Box3 {
        for p in pts {
            self.add_point(*p);
        }
        self
    }

    pub fn add_box(&mut self, other: &Box3) -> &mut Box3 {
        if !other.empty() {
            for i in 0..3 {
                self.min[i] = np_min(self.min[i], other.min[i]);
                self.max[i] = np_max(self.max[i], other.max[i]);
            }
        }
        self
    }

    pub fn center(&self) -> V3 {
        div(add(self.min, self.max), 2.0)
    }

    pub fn size(&self) -> V3 {
        sub(self.max, self.min)
    }

    pub fn sphere_radius(&self) -> f64 {
        norm(self.size()) / 2.0
    }

    /// The eight corners, x slowest and z fastest.
    pub fn corners(&self) -> [V3; 8] {
        let (a, b) = (self.min, self.max);
        let mut out = [[0.0; 3]; 8];
        let mut n = 0;
        for x in [a[0], b[0]] {
            for y in [a[1], b[1]] {
                for z in [a[2], b[2]] {
                    out[n] = [x, y, z];
                    n += 1;
                }
            }
        }
        out
    }
}

fn np_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if b < a {
        b
    } else {
        a
    }
}

fn np_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if b > a {
        b
    } else {
        a
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-12
    }

    #[test]
    fn euler_round_trip() {
        let e = [0.3, -1.1, 0.7];
        let q = quat_from_euler_yzx(e[0], e[1], e[2]);
        let back = euler_yzx_from_quat(q);
        for i in 0..3 {
            assert!(close(e[i], back[i]));
        }
        let q = quat_from_euler_yxz(e[0], e[1], e[2]);
        let back = euler_yxz_from_quat(q);
        for i in 0..3 {
            assert!(close(e[i], back[i]));
        }
    }

    #[test]
    fn rotation_and_inverse() {
        let q = quat_axis_angle([0.0, 2.0, 0.0], std::f64::consts::FRAC_PI_2);
        let v = quat_rotate(q, [1.0, 0.0, 0.0]);
        assert!(close(v[0], 0.0) && close(v[2], -1.0));
        let i = quat_mul(q, quat_inv(q));
        assert!(close(i[3], 1.0) && close(i[0], 0.0));
        assert_eq!(quat_normalize([0.0; 4]), quat_identity());
    }

    #[test]
    fn matrices() {
        let m = compose([1.0, 2.0, 3.0], quat_from_euler_yzx(0.2, 0.4, 0.6), [2.0, 3.0, 4.0]);
        let s = decompose_scale(&m);
        assert!(close(s[0], 2.0) && close(s[1], 3.0) && close(s[2], 4.0));
        let inv = mat4_inv(&m).unwrap();
        let p = translate(&inv, translate(&m, [5.0, 6.0, 7.0]));
        assert!(close(p[0], 5.0) && close(p[1], 6.0) && close(p[2], 7.0));
        assert!(mat4_inv(&[[0.0; 4]; 4]).is_none());
        let id = mat4_mul(&m, &inv);
        assert!(close(id[0][0], 1.0) && close(id[1][2], 0.0));
    }

    #[test]
    fn look_and_box() {
        let q = look_rotation([0.0, 0.0, 5.0], [0.0, 0.0, 0.0], UP);
        assert!(close(q[3], 1.0));
        let q = look_rotation([0.0, 5.0, 0.0], [0.0, 0.0, 0.0], UP);
        assert!(q.iter().all(|v| v.is_finite()));
        let mut b = Box3::new();
        assert!(b.empty());
        b.add_points(&[[1.0, 2.0, 3.0], [-1.0, 0.0, 5.0]]);
        assert!(!b.empty());
        assert_eq!(b.center(), [0.0, 1.0, 4.0]);
        assert_eq!(b.size(), [2.0, 2.0, 2.0]);
        assert_eq!(b.corners()[1], [-1.0, 0.0, 5.0]);
    }
}
