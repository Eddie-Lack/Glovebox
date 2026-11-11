//! The transform gizmo: arrows to move, rings to rotate and handles to scale an invisible pivot.
//!
//! A port of `desktop/gizmo.py`. It only changes the pivot (`pos`, `quat`, `scale`). The editor works out the edit
//! from the pivot's change since the drag began. Handles are picked on screen, within a few pixels. `node` builds
//! the scene nodes the renderer draws on top of everything, at a constant size on screen.

use crate::camera::Camera;
use crate::scene::{Geometry, Item, Material, Mesh, Node, NodeId, Scene};
use crate::vec::{self, Quat, V3};
use std::collections::HashMap;
use std::f64::consts::PI;
use std::sync::Arc;

/// The gizmo's length on screen, in pixels.
pub const PIXELS: f64 = 90.0;
/// How near the pointer must be to a handle, in pixels.
pub const PICK: f64 = 9.0;
pub const HOVER: [f64; 3] = [1.0, 0.85, 0.2];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    Translate,
    Rotate,
    Scale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Space {
    World,
    Local,
}

/// A handle: one axis, a plane of two axes, or the centre.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Handle {
    X,
    Y,
    Z,
    XY,
    YZ,
    XZ,
    XYZ,
}

impl Handle {
    /// The indices of the axes the handle moves along.
    pub fn axes(self) -> &'static [usize] {
        match self {
            Handle::X => &[0],
            Handle::Y => &[1],
            Handle::Z => &[2],
            Handle::XY => &[0, 1],
            Handle::YZ => &[1, 2],
            Handle::XZ => &[0, 2],
            Handle::XYZ => &[0, 1, 2],
        }
    }

    /// The axis of a one-axis handle.
    fn single(self) -> Option<usize> {
        match self {
            Handle::X => Some(0),
            Handle::Y => Some(1),
            Handle::Z => Some(2),
            _ => None,
        }
    }

    pub fn color(self) -> [f64; 3] {
        match self {
            Handle::X => [1.0, 0.25, 0.25],
            Handle::Y => [0.35, 0.9, 0.35],
            Handle::Z => [0.3, 0.5, 1.0],
            Handle::XYZ => [0.9, 0.9, 0.9],
            _ => [0.9, 0.9, 0.9],
        }
    }
}

const SINGLE: [Handle; 3] = [Handle::X, Handle::Y, Handle::Z];

/// How a handle is drawn and picked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Line,
    Quad,
    Dot,
    Ring,
}

/// The steps the drags snap to, or none.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Snap {
    pub translate: Option<f64>,
    /// In radians.
    pub rotate: Option<f64>,
    pub scale: Option<f64>,
}

#[derive(Debug, Clone)]
struct Drag {
    axis: Handle,
    pos: V3,
    quat: Quat,
    basis: [V3; 3],
    plane: V3,
    start: V3,
    x: f64,
    y: f64,
}

type UnitGeometry = Vec<(Handle, Vec<Arc<Geometry>>)>;

pub struct Gizmo {
    pub mode: Mode,
    pub space: Space,
    pub attached: bool,
    pub enabled: bool,
    pub pos: V3,
    pub quat: Quat,
    pub scale: V3,
    /// The hovered or dragged handle.
    pub axis: Option<Handle>,
    pub dragging: bool,
    pub snap: Snap,
    drag: Option<Drag>,
    unit: HashMap<Mode, UnitGeometry>,
}

impl Default for Gizmo {
    fn default() -> Self {
        Gizmo::new()
    }
}

fn unit(v: V3) -> V3 {
    vec::div(v, vec::norm(v))
}

/// Two unit vectors square to `t` and to each other.
fn side_axes(t: V3) -> (V3, V3) {
    let u = unit(vec::cross(t, if t[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] }));
    (u, vec::cross(t, u))
}

/// Where the ray meets the plane through `point` with the normal `normal`, in front of the origin.
pub fn plane_hit(origin: V3, direction: V3, point: V3, normal: V3) -> Option<V3> {
    let d = vec::dot(direction, normal);
    if d.abs() < 1e-9 {
        return None;
    }
    let t = vec::dot(vec::sub(point, origin), normal) / d;
    (t > 0.0).then(|| vec::add(origin, vec::scale(direction, t)))
}

/// The distance from `p` to the segment a-b, in the plane.
pub fn seg_dist(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let d = [b[0] - a[0], b[1] - a[1]];
    let l = d[0] * d[0] + d[1] * d[1];
    let t = if l == 0.0 { 0.0 } else { (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / l).clamp(0.0, 1.0) };
    (a[0] + d[0] * t - p[0]).hypot(a[1] + d[1] * t - p[1])
}

pub fn point_in_poly(p: [f64; 2], poly: &[[f64; 2]]) -> bool {
    let mut inside = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1] + 1e-12) + a[0] {
            inside = !inside;
        }
    }
    inside
}

/// A thin tube along the points (the gizmo's lines, which a GPU draws one pixel wide).
pub fn tube(pts: &[V3], radius: f64, closed: bool, sides: u32) -> Geometry {
    let n = pts.len();
    let mut ring = Vec::with_capacity(n * sides as usize);
    for i in 0..n {
        let a = if closed || i > 0 { pts[(i + n - 1) % n] } else { pts[i] };
        let b = if closed || i < n - 1 { pts[(i + 1) % n] } else { pts[i] };
        let t = vec::sub(b, a);
        let l = vec::norm(t);
        let t = vec::div(t, if l != 0.0 { l } else { 1.0 });
        let (u, v) = side_axes(t);
        for k in 0..sides {
            let ang = 2.0 * PI * f64::from(k) / f64::from(sides);
            ring.push(vec::add(pts[i], vec::scale(vec::add(vec::scale(u, ang.cos()), vec::scale(v, ang.sin())), radius)));
        }
    }
    let mut idx = Vec::new();
    let segs = if closed { n } else { n.saturating_sub(1) };
    let sides = sides as usize;
    for i in 0..segs {
        let j = (i + 1) % n;
        for k in 0..sides {
            let (a, b) = (i * sides + k, i * sides + (k + 1) % sides);
            let (c, d) = (j * sides + k, j * sides + (k + 1) % sides);
            idx.extend([a, c, b, b, c, d].map(|v| v as u32));
        }
    }
    Geometry::from_f64(&ring, Some(&idx))
}

pub fn cone(base: V3, tip: V3, radius: f64, sides: u32) -> Geometry {
    let t = unit(vec::sub(tip, base));
    let (u, v) = side_axes(t);
    let mut pts = vec![tip, base];
    for k in 0..sides {
        let a = 2.0 * PI * f64::from(k) / f64::from(sides);
        pts.push(vec::add(base, vec::scale(vec::add(vec::scale(u, a.cos()), vec::scale(v, a.sin())), radius)));
    }
    let mut idx = Vec::new();
    for k in 0..sides {
        let (a, b) = (2 + k, 2 + (k + 1) % sides);
        idx.extend([0, a, b, 1, b, a]);
    }
    Geometry::from_f64(&pts, Some(&idx))
}

pub fn cube(centre: V3, half: f64) -> Geometry {
    let mut p = Vec::new();
    for x in [-1.0, 1.0] {
        for y in [-1.0, 1.0] {
            for z in [-1.0, 1.0] {
                p.push([x * half + centre[0], y * half + centre[1], z * half + centre[2]]);
            }
        }
    }
    let idx = [0, 1, 3, 0, 3, 2, 4, 6, 7, 4, 7, 5, 0, 4, 5, 0, 5, 1, 2, 3, 7, 2, 7, 6, 0, 2, 6, 0, 6, 4, 1, 5, 7, 1, 7, 3];
    Geometry::from_f64(&p, Some(&idx))
}

/// The handles of a mode: name, kind and polyline, for a pivot at `c` with the axes `b` and the size `s`.
fn handles_of(mode: Mode, c: V3, b: &[V3; 3], s: f64) -> Vec<(Handle, Kind, Vec<V3>)> {
    let at = |v: V3, k: f64| vec::add(c, vec::scale(v, k));
    let mut out = Vec::new();
    match mode {
        Mode::Translate => {
            for (k, a) in SINGLE.iter().zip(b) {
                out.push((*k, Kind::Line, vec![at(*a, s * 0.15), at(*a, s)]));
            }
            for k in [Handle::XY, Handle::YZ, Handle::XZ] {
                let (u, v) = (b[k.axes()[0]], b[k.axes()[1]]);
                let q = s * 0.3;
                let p = |fu: f64, fv: f64| vec::add(vec::add(c, vec::scale(u, fu)), vec::scale(v, fv));
                out.push((k, Kind::Quad, vec![p(q * 0.4, q * 0.4), p(q, q * 0.4), p(q, q), p(q * 0.4, q)]));
            }
            out.push((Handle::XYZ, Kind::Dot, vec![c]));
        }
        Mode::Rotate => {
            for (k, a) in SINGLE.iter().zip(b) {
                let (u, v) = side_axes(*a);
                let pts = (0..64)
                    .map(|i| {
                        let t = 2.0 * PI * f64::from(i) / 64.0;
                        vec::add(c, vec::scale(vec::add(vec::scale(u, t.cos()), vec::scale(v, t.sin())), s * 0.8))
                    })
                    .collect();
                out.push((*k, Kind::Ring, pts));
            }
        }
        Mode::Scale => {
            for (k, a) in SINGLE.iter().zip(b) {
                out.push((*k, Kind::Line, vec![at(*a, s * 0.15), at(*a, s)]));
            }
            out.push((Handle::XYZ, Kind::Dot, vec![c]));
        }
    }
    out
}

const WORLD: [V3; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

impl Gizmo {
    pub fn new() -> Gizmo {
        Gizmo {
            mode: Mode::Translate,
            space: Space::World,
            attached: false,
            enabled: true,
            pos: [0.0; 3],
            quat: vec::quat_identity(),
            scale: [1.0; 3],
            axis: None,
            dragging: false,
            snap: Snap::default(),
            drag: None,
            unit: HashMap::new(),
        }
    }

    pub fn attach(&mut self, pos: V3, quat: Quat) {
        self.attached = true;
        (self.pos, self.quat, self.scale) = (pos, quat, [1.0; 3]);
    }

    pub fn detach(&mut self) {
        self.attached = false;
        self.axis = None;
        self.dragging = false;
    }

    /// The handles' axes in the world.
    pub fn basis(&self) -> [V3; 3] {
        match self.space {
            Space::Local => {
                let m = vec::quat_to_mat3(self.quat);
                [0, 1, 2].map(|i| [m[0][i], m[1][i], m[2][i]])
            }
            Space::World => WORLD,
        }
    }

    /// The world length that is `PIXELS` long on screen at the pivot.
    pub fn size(&self, camera: &Camera) -> f64 {
        let d = vec::norm(vec::sub(camera.pos, self.pos));
        d * (vec::radians(camera.fov) / 2.0).tan() * 2.0 * PIXELS / camera.height.max(1.0)
    }

    /// Each handle's polyline in the world, for picking.
    pub fn handles(&self, camera: &Camera) -> Vec<(Handle, Kind, Vec<V3>)> {
        handles_of(self.mode, self.pos, &self.basis(), self.size(camera))
    }

    /// The handle under pixel (x, y), or none.
    pub fn pick(&self, camera: &Camera, x: f64, y: f64) -> Option<Handle> {
        if !self.attached || !self.enabled {
            return None;
        }
        let (mut best, mut best_d) = (None, PICK);
        for (k, kind, pts) in self.handles(camera) {
            let px: Vec<V3> = pts.iter().map(|p| camera.to_pixels(*p)).collect();
            if px.iter().any(|p| p[2] > 1.0) {
                continue;
            }
            let m = [x, y];
            let p2: Vec<[f64; 2]> = px.iter().map(|p| [p[0], p[1]]).collect();
            let segs = |closed: bool| {
                let n = if closed { p2.len() } else { p2.len() - 1 };
                (0..n).map(|i| seg_dist(m, p2[i], p2[(i + 1) % p2.len()])).fold(f64::INFINITY, f64::min)
            };
            let d = match kind {
                Kind::Dot => (p2[0][0] - m[0]).hypot(p2[0][1] - m[1]) - 6.0,
                Kind::Quad if point_in_poly(m, &p2) => 0.0,
                Kind::Quad | Kind::Ring => segs(true),
                Kind::Line => segs(false) - 3.0,
            };
            if d < best_d {
                (best, best_d) = (Some(k), d);
            }
        }
        best
    }

    /// Updates the hovered handle. True when it changed.
    pub fn hover(&mut self, camera: &Camera, x: f64, y: f64) -> bool {
        if self.dragging {
            return false;
        }
        let a = self.pick(camera, x, y);
        let changed = a != self.axis;
        self.axis = a;
        changed
    }

    /// Starts a drag at pixel (x, y). False when no handle is there.
    pub fn begin(&mut self, camera: &Camera, x: f64, y: f64) -> bool {
        let Some(a) = self.pick(camera, x, y) else { return false };
        self.axis = Some(a);
        let Some((origin, direction)) = camera.ray(x, y) else { return false };
        let plane = self.plane(camera, a);
        let Some(start) = plane_hit(origin, direction, self.pos, plane) else {
            self.drag = None;
            return false;
        };
        self.drag = Some(Drag { axis: a, pos: self.pos, quat: self.quat, basis: self.basis(), plane, start, x, y });
        self.scale = [1.0; 3];
        self.dragging = true;
        true
    }

    /// The plane the drag moves in: through the pivot, holding the axis and as square to the view as can be.
    fn plane(&self, camera: &Camera, a: Handle) -> V3 {
        let b = self.basis();
        let view = vec::scale(camera.forward(), -1.0);
        if self.mode == Mode::Rotate {
            // Only X, Y and Z are rings
            return b[a.axes()[0]];
        }
        if let Some(i) = a.single() {
            let axis = b[i];
            let mut n = vec::cross(axis, vec::cross(view, axis));
            if vec::norm(n) < 1e-6 {
                n = vec::cross(axis, camera.up());
            }
            return unit(n);
        }
        if a == Handle::XYZ {
            return view;
        }
        unit(vec::cross(b[a.axes()[0]], b[a.axes()[1]]))
    }

    /// Moves the drag to pixel (x, y). True when the pivot may have changed.
    pub fn drag_to(&mut self, camera: &Camera, x: f64, y: f64) -> bool {
        if !self.dragging {
            return false;
        }
        let Some(dr) = self.drag.clone() else { return false };
        let Some((origin, direction)) = camera.ray(x, y) else { return false };
        let Some(p) = plane_hit(origin, direction, dr.pos, dr.plane) else { return false };
        let (a, b) = (dr.axis, dr.basis);
        match self.mode {
            Mode::Translate => {
                let mut d = vec::sub(p, dr.start);
                if let Some(i) = a.single() {
                    d = vec::scale(b[i], vec::dot(d, b[i]));
                }
                let mut new = vec::add(dr.pos, d);
                if let Some(snap) = self.snap.translate.filter(|s| *s != 0.0) {
                    if self.space == Space::World {
                        for &i in a.axes() {
                            new[i] = (new[i] / snap).round_ties_even() * snap;
                        }
                    } else {
                        for &k in a.axes() {
                            let along = vec::dot(vec::sub(new, dr.pos), b[k]);
                            new = vec::add(new, vec::scale(b[k], (along / snap).round_ties_even() * snap - along));
                        }
                    }
                }
                self.pos = new;
            }
            Mode::Rotate => {
                let u = vec::sub(dr.start, dr.pos);
                let v = vec::sub(p, dr.pos);
                if vec::norm(u) < 1e-9 || vec::norm(v) < 1e-9 {
                    return false;
                }
                let n = b[a.axes()[0]];
                let mut ang = vec::dot(vec::cross(u, v), n).atan2(vec::dot(u, v));
                if let Some(snap) = self.snap.rotate.filter(|s| *s != 0.0) {
                    ang = (ang / snap).round_ties_even() * snap;
                }
                self.quat = vec::quat_normalize(vec::quat_mul(vec::quat_axis_angle(n, ang), dr.quat));
            }
            Mode::Scale => {
                let mut s = [1.0; 3];
                match a.single() {
                    Some(i) => {
                        let s0 = vec::dot(vec::sub(dr.start, dr.pos), b[i]);
                        let s1 = vec::dot(vec::sub(p, dr.pos), b[i]);
                        s[i] = if s0.abs() > 1e-9 { s1 / s0 } else { 1.0 };
                    }
                    // Up and right on screen grow it, as far as the mouse goes
                    None => s = [(1.0 + ((x - dr.x) - (y - dr.y)) / 100.0).max(0.01); 3],
                }
                if let Some(snap) = self.snap.scale.filter(|s| *s != 0.0) {
                    s = s.map(|v| {
                        let r = (v / snap).round_ties_even() * snap;
                        if r != 0.0 {
                            r
                        } else {
                            snap
                        }
                    });
                }
                self.scale = s;
            }
        }
        true
    }

    /// Ends the drag. True when one was going on.
    pub fn end(&mut self) -> bool {
        let was = self.dragging;
        self.dragging = false;
        self.drag = None;
        was
    }

    /// The handles' meshes for a gizmo of size 1 at the origin, made once per mode.
    pub fn unit_geometry(&mut self) -> &[(Handle, Vec<Arc<Geometry>>)] {
        let mode = self.mode;
        self.unit.entry(mode).or_insert_with(|| {
            let r = 0.018;
            handles_of(mode, [0.0; 3], &WORLD, 1.0)
                .into_iter()
                .map(|(k, kind, pts)| {
                    let geoms = match kind {
                        Kind::Line => {
                            let a = WORLD[k.axes()[0]];
                            let tip = if mode == Mode::Translate {
                                cone(vec::scale(a, 0.85), vec::scale(a, 1.08), 0.06, 12)
                            } else {
                                cube(a, 0.05)
                            };
                            vec![tube(&pts, r, false, 6), tip]
                        }
                        Kind::Ring => vec![tube(&pts, r, true, 6)],
                        Kind::Quad => vec![Geometry::from_f64(&pts, Some(&[0, 1, 2, 0, 2, 3]))],
                        Kind::Dot => vec![cube([0.0; 3], 0.05)],
                    };
                    (k, geoms.into_iter().map(Arc::new).collect())
                })
                .collect()
        })
    }

    /// Builds the gizmo to draw as a new detached node in `scene`, placed and sized for the view. The caller gives
    /// it to the renderer as an overlay and deletes it after the frame. An unattached gizmo gives an empty node.
    pub fn node(&mut self, scene: &mut Scene, camera: &Camera) -> NodeId {
        let mut root = Node::new("gizmo");
        root.order = 100;
        if self.attached {
            root.pos = self.pos;
            root.quat = if self.space == Space::Local { self.quat } else { vec::quat_identity() };
            root.scale = [self.size(camera); 3];
            let axis = self.axis;
            for (k, geoms) in self.unit_geometry() {
                let plane = matches!(k, Handle::XY | Handle::YZ | Handle::XZ);
                let mat = Arc::new(Material {
                    color: if Some(*k) == axis { HOVER } else { k.color() },
                    lit: false,
                    depth_test: false,
                    transparent: plane,
                    opacity: if plane { 0.5 } else { 1.0 },
                    ..Material::default()
                });
                for g in geoms {
                    root.items.push(Item::Mesh(Mesh::new(g.clone(), mat.clone())));
                }
            }
        }
        scene.insert(root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn camera() -> Camera {
        let mut c = Camera::new(55.0, 1.0, 60000.0);
        (c.width, c.height) = (800.0, 600.0);
        c.pos = [0.0, 0.0, 500.0];
        c.look_at([0.0, 0.0, 0.0]);
        c
    }

    fn px(c: &Camera, p: V3) -> (f64, f64) {
        let v = c.to_pixels(p);
        (v[0], v[1])
    }

    #[test]
    fn handles_are_picked_on_screen() {
        let c = camera();
        let mut g = Gizmo::new();
        assert_eq!(g.pick(&c, 400.0, 300.0), None);
        g.attach([0.0; 3], vec::quat_identity());
        let s = g.size(&c);
        // the gizmo is PIXELS long on screen
        let (x, _) = px(&c, [s, 0.0, 0.0]);
        assert!((x - 400.0 - PIXELS).abs() < 1e-6);
        assert_eq!(g.pick(&c, 400.0, 300.0), Some(Handle::XYZ));
        assert_eq!(g.pick(&c, 400.0 + 60.0, 300.0 + 5.0), Some(Handle::X));
        assert_eq!(g.pick(&c, 400.0 - 4.0, 300.0 - 70.0), Some(Handle::Y));
        assert_eq!(g.pick(&c, 400.0 + 18.0, 300.0 - 18.0), Some(Handle::XY));
        assert_eq!(g.pick(&c, 400.0 + 60.0, 300.0 + 30.0), None);
        g.enabled = false;
        assert_eq!(g.pick(&c, 400.0, 300.0), None);
        g.enabled = true;
        g.mode = Mode::Rotate;
        // the Z ring faces the camera: a circle of 0.8 of the size
        assert_eq!(g.pick(&c, 400.0 + 0.8 * PIXELS * 0.7071, 300.0 + 0.8 * PIXELS * 0.7071), Some(Handle::Z));
        assert!(g.hover(&c, 400.0 + 0.8 * PIXELS * 0.7071, 300.0 + 0.8 * PIXELS * 0.7071));
        assert!(!g.hover(&c, 400.0 + 0.8 * PIXELS * 0.7071, 300.0 + 0.8 * PIXELS * 0.7071));
    }

    #[test]
    fn drags_change_the_pivot() {
        let c = camera();
        let mut g = Gizmo::new();
        g.attach([0.0; 3], vec::quat_identity());
        let s = g.size(&c);
        let (x0, y0) = px(&c, [0.6 * s, 0.0, 0.0]);
        assert!(g.begin(&c, x0, y0));
        let (x1, y1) = px(&c, [0.6 * s + 30.0, 40.0, 0.0]);
        assert!(g.drag_to(&c, x1, y1));
        // along X only
        assert!((g.pos[0] - 30.0).abs() < 1e-6 && g.pos[1].abs() < 1e-9 && g.pos[2].abs() < 1e-9, "{:?}", g.pos);
        g.snap.translate = Some(25.0);
        g.drag_to(&c, x1, y1);
        assert_eq!(g.pos, [25.0, 0.0, 0.0]);
        assert!(g.end() && !g.end());
        assert!(!g.drag_to(&c, x1, y1));

        g.attach([0.0; 3], vec::quat_identity());
        g.mode = Mode::Rotate;
        let r = 0.8 * s;
        // at the ends of the axes the other two rings, seen edge on, lie over the Z ring
        let h = r * 0.5f64.sqrt();
        let (x0, y0) = px(&c, [h, h, 0.0]);
        assert!(g.begin(&c, x0, y0));
        assert_eq!(g.axis, Some(Handle::Z));
        let (x1, y1) = px(&c, [-h, h, 0.0]);
        g.drag_to(&c, x1, y1);
        let e = vec::quat_axis_angle([0.0, 0.0, 1.0], PI / 2.0);
        assert!((0..4).all(|i| (g.quat[i] - e[i]).abs() < 1e-9), "{:?}", g.quat);
        g.end();

        g.attach([0.0; 3], vec::quat_identity());
        g.mode = Mode::Scale;
        let (x0, y0) = px(&c, [0.0, 0.5 * s, 0.0]);
        assert!(g.begin(&c, x0, y0));
        let (x1, y1) = px(&c, [0.0, s, 0.0]);
        g.drag_to(&c, x1, y1);
        assert!((g.scale[1] - 2.0).abs() < 1e-9 && g.scale[0] == 1.0 && g.scale[2] == 1.0);
        g.end();
        // the centre scales all three by the mouse's way up and right
        assert!(g.begin(&c, 400.0, 300.0));
        g.drag_to(&c, 430.0, 280.0);
        assert_eq!(g.scale, [1.5; 3]);
        g.snap.scale = Some(4.0);
        g.drag_to(&c, 430.0, 280.0);
        assert_eq!(g.scale, [4.0; 3]);
    }

    #[test]
    fn the_node_holds_every_handle() {
        let c = camera();
        let mut g = Gizmo::new();
        let mut s = Scene::new();
        let empty = g.node(&mut s, &c);
        assert!(s[empty].items.is_empty());
        g.attach([10.0, 20.0, 30.0], vec::quat_identity());
        g.axis = Some(Handle::Y);
        let n = g.node(&mut s, &c);
        // three arrows of a tube and a cone, three squares, one cube
        assert_eq!((s[n].items.len(), s[n].order), (10, 100));
        assert_eq!(s[n].scale, [g.size(&c); 3]);
        let Item::Mesh(y) = &s[n].items[2] else { panic!() };
        assert_eq!((y.material.color, y.material.depth_test, y.material.lit), (HOVER, false, false));
        let Item::Mesh(q) = &s[n].items[6] else { panic!() };
        assert!(q.material.transparent && q.material.opacity == 0.5);
        // the geometry is made once
        let again = g.node(&mut s, &c);
        let (Item::Mesh(a), Item::Mesh(b)) = (&s[n].items[0], &s[again].items[0]) else { panic!() };
        assert!(Arc::ptr_eq(&a.geometry, &b.geometry));
        g.mode = Mode::Rotate;
        let r = g.node(&mut s, &c);
        assert_eq!(s[r].items.len(), 3);
        // a tube of 64 rings of 6 points, closed
        let Item::Mesh(ring) = &s[r].items[0] else { panic!() };
        assert_eq!((ring.geometry.positions.len(), ring.geometry.triangle_count()), (384, 768));
    }

    /// Replays the reference script `render/gizmo.py`: what the Python gizmo picked, how its drags moved the pivot,
    /// and the meshes it drew.
    #[test]
    fn python_gizmo_replays() {
        let Ok(dir) = std::env::var("GLOVEBOX_ORACLE") else {
            println!("skipped: GLOVEBOX_ORACLE is not set");
            return;
        };
        let text = std::fs::read(format!("{dir}/render/out/gizmo.json")).unwrap();
        let cases: serde_json::Value = serde_json::from_slice(&text).unwrap();
        let f = |v: &serde_json::Value| v.as_f64().unwrap();
        let v3 = |v: &serde_json::Value| [f(&v[0]), f(&v[1]), f(&v[2])];
        let handle = |v: &serde_json::Value| {
            v.as_str().map(|s| match s {
                "X" => Handle::X,
                "Y" => Handle::Y,
                "Z" => Handle::Z,
                "XY" => Handle::XY,
                "YZ" => Handle::YZ,
                "XZ" => Handle::XZ,
                _ => Handle::XYZ,
            })
        };
        let near = |a: f64, b: f64| (a - b).abs() <= 1e-7 * (1.0 + b.abs());
        let (mut picks, mut moves) = (0, 0);
        for case in cases.as_array().unwrap() {
            let c = &case["camera"];
            let mut cam = Camera::new(f(&c["fov"]), f(&c["near"]), f(&c["far"]));
            (cam.width, cam.height, cam.pos) = (f(&c["width"]), f(&c["height"]), v3(&c["pos"]));
            cam.quat = [f(&c["quat"][0]), f(&c["quat"][1]), f(&c["quat"][2]), f(&c["quat"][3])];
            let mut g = Gizmo::new();
            g.mode = match case["mode"].as_str().unwrap() {
                "translate" => Mode::Translate,
                "rotate" => Mode::Rotate,
                _ => Mode::Scale,
            };
            g.space = if case["space"] == "local" { Space::Local } else { Space::World };
            let pos = v3(&case["pos"]);
            let q = &case["quat"];
            let quat = [f(&q[0]), f(&q[1]), f(&q[2]), f(&q[3])];
            g.attach(pos, quat);
            assert!(near(g.size(&cam), f(&case["size"])));
            for p in case["picks"].as_array().unwrap() {
                assert_eq!(g.pick(&cam, f(&p[0]), f(&p[1])), handle(&p[2]), "pick at {} {}", p[0], p[1]);
                picks += 1;
            }
            for d in case["drags"].as_array().unwrap() {
                g.attach(pos, quat);
                g.snap = Snap::default();
                match g.mode {
                    Mode::Translate => g.snap.translate = d["snap"].as_f64(),
                    Mode::Rotate => g.snap.rotate = d["snap"].as_f64(),
                    Mode::Scale => g.snap.scale = d["snap"].as_f64(),
                }
                assert!(g.begin(&cam, f(&d["x"]), f(&d["y"])));
                assert_eq!(g.axis, handle(&d["axis"]));
                for m in d["moves"].as_array().unwrap() {
                    assert_eq!(g.drag_to(&cam, f(&m["x"]), f(&m["y"])), m["ok"].as_bool().unwrap());
                    for k in 0..3 {
                        assert!(near(g.pos[k], f(&m["pos"][k])), "pos {:?} {}", g.pos, m["pos"]);
                        assert!(near(g.scale[k], f(&m["scale"][k])), "scale {:?} {}", g.scale, m["scale"]);
                    }
                    for k in 0..4 {
                        assert!(near(g.quat[k], f(&m["quat"][k])), "quat {:?} {}", g.quat, m["quat"]);
                    }
                    moves += 1;
                }
                g.end();
            }
            let mut s = Scene::new();
            let n = g.node(&mut s, &cam);
            let want = case["meshes"].as_array().unwrap();
            assert_eq!(s[n].items.len(), want.len());
            for (it, w) in s[n].items.iter().zip(want) {
                let Item::Mesh(m) = it else { panic!() };
                let p = &m.geometry.positions;
                assert_eq!((p.len() as u64, m.geometry.triangle_count() as u64 * 3), (w[0].as_u64().unwrap(), w[1].as_u64().unwrap()));
                let abs: f64 = p.iter().flatten().map(|v| f64::from(v.abs())).sum();
                let mix: f64 = p.iter().map(|v| f64::from(v[0]) + 2.0 * f64::from(v[1]) + 3.0 * f64::from(v[2])).sum();
                assert!((abs - f(&w[2])).abs() < 1e-4 && (mix - f(&w[3])).abs() < 1e-4, "{abs} {mix} {w}");
            }
        }
        println!("{picks} picks and {moves} drag moves match");
    }
}
