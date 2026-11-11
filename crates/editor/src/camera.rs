//! The view's camera and its orbit controls (with panning in the screen's plane).
//!
//! Sizes and pointer movements are plain numbers. The window code passes them in.

use crate::vec::{self, Mat4, Quat, V3};

#[derive(Clone, Debug, PartialEq)]
pub struct Camera {
    pub fov: f64,
    pub near: f64,
    pub far: f64,
    pub pos: V3,
    pub quat: Quat,
    /// The view's size in pixels.
    pub width: f64,
    pub height: f64,
}

impl Default for Camera {
    fn default() -> Self {
        Camera::new(55.0, 1.0, 60000.0)
    }
}

fn at_least_1(v: f64) -> f64 {
    if v > 1.0 {
        v
    } else {
        1.0
    }
}

fn norm2(a: [f64; 2]) -> f64 {
    a[1].mul_add(a[1], a[0] * a[0]).sqrt()
}

impl Camera {
    pub fn new(fov: f64, near: f64, far: f64) -> Camera {
        Camera { fov, near, far, pos: [400.0, 600.0, 900.0], quat: vec::quat_identity(), width: 1.0, height: 1.0 }
    }

    pub fn aspect(&self) -> f64 {
        self.width / at_least_1(self.height)
    }

    pub fn look_at(&mut self, target: V3) {
        self.quat = vec::look_rotation(self.pos, target, vec::UP);
    }

    pub fn forward(&self) -> V3 {
        vec::quat_rotate(self.quat, [0.0, 0.0, -1.0])
    }

    pub fn right(&self) -> V3 {
        vec::quat_rotate(self.quat, [1.0, 0.0, 0.0])
    }

    pub fn up(&self) -> V3 {
        vec::quat_rotate(self.quat, [0.0, 1.0, 0.0])
    }

    /// The world-to-camera matrix.
    pub fn view(&self) -> Mat4 {
        // The camera's matrix is a turn and a move. Its inverse is the turn transposed and the move turned back.
        let r = vec::quat_to_mat3(self.quat);
        let mut m = [[0.0; 4]; 4];
        for i in 0..3 {
            for j in 0..3 {
                m[i][j] = r[j][i];
            }
            m[i][3] = -(r[0][i] * self.pos[0] + r[1][i] * self.pos[1] + r[2][i] * self.pos[2]);
        }
        m[3][3] = 1.0;
        m
    }

    pub fn proj(&self) -> Mat4 {
        vec::perspective(self.fov, self.aspect(), self.near, self.far)
    }

    /// The projection for another view size. A size of 0 means the camera's own.
    pub fn proj_for(&self, width: f64, height: f64) -> Mat4 {
        let w = if width != 0.0 { width } else { self.width };
        let h = if height != 0.0 { height } else { self.height };
        vec::perspective(self.fov, w / at_least_1(h), self.near, self.far)
    }

    /// World point -> (ndc x, ndc y, ndc z).
    pub fn project(&self, p: V3) -> V3 {
        let c = vec::mat4_mul_v(&vec::mat4_mul(&self.proj(), &self.view()), [p[0], p[1], p[2], 1.0]);
        if c[3] != 0.0 {
            [c[0] / c[3], c[1] / c[3], c[2] / c[3]]
        } else {
            [0.0, 0.0, 2.0]
        }
    }

    /// World point -> (pixel x, pixel y, ndc z).
    pub fn to_pixels(&self, p: V3) -> V3 {
        let n = self.project(p);
        [(n[0] + 1.0) / 2.0 * self.width, (1.0 - n[1]) / 2.0 * self.height, n[2]]
    }

    /// The ray through pixel (x, y): origin and unit direction. `None` when the camera's matrices cannot be
    /// inverted (a view with no size).
    pub fn ray(&self, x: f64, y: f64) -> Option<(V3, V3)> {
        let nx = x / at_least_1(self.width) * 2.0 - 1.0;
        let ny = -(y / at_least_1(self.height) * 2.0 - 1.0);
        self.ray_ndc(nx, ny)
    }

    pub fn ray_ndc(&self, nx: f64, ny: f64) -> Option<(V3, V3)> {
        let inv = vec::mat4_inv(&vec::mat4_mul(&self.proj(), &self.view()))?;
        let a = vec::mat4_mul_v(&inv, [nx, ny, -1.0, 1.0]);
        let b = vec::mat4_mul_v(&inv, [nx, ny, 1.0, 1.0]);
        let a = [a[0] / a[3], a[1] / a[3], a[2] / a[3]];
        let b = [b[0] / b[3], b[1] / b[3], b[2] / b[3]];
        let d = vec::sub(b, a);
        Some((self.pos, vec::div(d, vec::norm(d))))
    }

    /// The nearest of the segments that passes within `px` pixels of the ray's pixel: (distance, point).
    pub fn segment_hit(&self, segments: &[(V3, V3)], origin: V3, direction: V3, px: f64) -> Option<(f64, V3)> {
        let mut best: Option<(f64, V3)> = None;
        let far = vec::add(origin, vec::scale(direction, 1000.0));
        let sp = self.to_pixels(far);
        for (p, q) in segments {
            let (pp, qq) = (self.to_pixels(*p), self.to_pixels(*q));
            if pp[2] > 1.0 || qq[2] > 1.0 || (pp[2] < -1.0 && qq[2] < -1.0) {
                continue;
            }
            let d = [qq[0] - pp[0], qq[1] - pp[1]];
            let l = d[1].mul_add(d[1], d[0] * d[0]);
            let t = if l == 0.0 {
                0.0
            } else {
                let s = [sp[0] - pp[0], sp[1] - pp[1]];
                vec::clamp1(s[1].mul_add(d[1], s[0] * d[0]) / l).max(0.0)
            };
            if norm2([pp[0] + d[0] * t - sp[0], pp[1] + d[1] * t - sp[1]]) <= px {
                let w = vec::add(*p, vec::scale(vec::sub(*q, *p), t));
                let dist = vec::norm(vec::sub(w, origin));
                if best.map(|b| dist < b.0).unwrap_or(true) {
                    best = Some((dist, w));
                }
            }
        }
        best
    }
}

/// Left drag turns about the target, right drag pans in the screen's plane, the wheel moves in and out.
#[derive(Clone, Debug, PartialEq)]
pub struct Orbit {
    pub target: V3,
    pub enabled: bool,
}

impl Default for Orbit {
    fn default() -> Self {
        Orbit { target: [0.0; 3], enabled: true }
    }
}

impl Orbit {
    pub fn update(&self, c: &mut Camera) {
        c.look_at(self.target);
    }

    /// Turns about the target by a pointer movement in pixels.
    pub fn rotate(&self, c: &mut Camera, dx: f64, dy: f64) {
        use std::f64::consts::PI;
        let off = vec::sub(c.pos, self.target);
        let n = vec::norm(off);
        let r = if n != 0.0 { n } else { 1e-6 };
        let mut theta = off[0].atan2(off[2]);
        let mut phi = vec::clamp1(off[1] / r).acos();
        theta -= 2.0 * PI * dx / at_least_1(c.height);
        phi -= 2.0 * PI * dy / at_least_1(c.height);
        phi = if phi < PI - 1e-6 { phi } else { PI - 1e-6 };
        phi = if phi > 1e-6 { phi } else { 1e-6 };
        let dir = [phi.sin() * theta.sin(), phi.cos(), phi.sin() * theta.cos()];
        c.pos = vec::add(self.target, vec::scale(dir, r));
        self.update(c);
    }

    /// Moves the camera and the target in the screen's plane by a pointer movement in pixels.
    pub fn pan(&mut self, c: &mut Camera, dx: f64, dy: f64) {
        let dist = vec::norm(vec::sub(c.pos, self.target)) * (vec::radians(c.fov) / 2.0).tan();
        let h = at_least_1(c.height);
        let mv = vec::add(vec::scale(c.right(), -2.0 * dx * dist / h), vec::scale(c.up(), 2.0 * dy * dist / h));
        c.pos = vec::add(c.pos, mv);
        self.target = vec::add(self.target, mv);
    }

    /// Moves in (`steps` > 0) or out.
    pub fn dolly(&self, c: &mut Camera, steps: f64) {
        let off = vec::sub(c.pos, self.target);
        let n = vec::norm(off);
        let r = (n * 0.95f64.powf(steps)).min(1e6).max(1.0);
        c.pos = vec::add(self.target, vec::scale(vec::div(off, if n != 0.0 { n } else { 1.0 }), r));
        self.update(c);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projects_and_casts_back() {
        let mut c = Camera::default();
        c.width = 800.0;
        c.height = 600.0;
        c.look_at([0.0, 0.0, 0.0]);
        let px = c.to_pixels([0.0, 0.0, 0.0]);
        assert!((px[0] - 400.0).abs() < 1e-6 && (px[1] - 300.0).abs() < 1e-6 && px[2] < 1.0);
        let (o, d) = c.ray(400.0, 300.0).unwrap();
        assert_eq!(o, c.pos);
        let f = c.forward();
        for i in 0..3 {
            assert!((d[i] - f[i]).abs() < 1e-6);
        }
        let hit = c.segment_hit(&[([-10.0, 0.0, 0.0], [10.0, 0.0, 0.0]), ([500.0, 0.0, 0.0], [600.0, 0.0, 0.0])], o, d, 4.0);
        let (dist, w) = hit.unwrap();
        assert!(vec::norm(w) < 1e-6 && (dist - vec::norm(c.pos)).abs() < 1e-6);
    }

    #[test]
    fn orbit() {
        let mut c = Camera::default();
        c.height = 600.0;
        let mut o = Orbit::default();
        let r = vec::norm(c.pos);
        o.rotate(&mut c, 30.0, -20.0);
        assert!((vec::norm(c.pos) - r).abs() < 1e-9);
        o.dolly(&mut c, 2.0);
        assert!((vec::norm(c.pos) - r * 0.9025).abs() < 1e-9);
        o.pan(&mut c, 10.0, 5.0);
        assert!((vec::norm(vec::sub(c.pos, o.target)) - r * 0.9025).abs() < 1e-9);
        o.dolly(&mut c, 1e9);
        assert!((vec::norm(vec::sub(c.pos, o.target)) - 1.0).abs() < 1e-9);
    }
}
