//! The transform engine. The gizmo moves the pivot. Every change applies the whole change since the drag began
//! to what the entities were then.

use super::{Drag, DragItem, Editor, Event};
use crate::entities::EntId;
use crate::gizmo::{Handle, Mode};
use crate::vec::{self, V3};

fn mul3(a: V3, b: V3) -> V3 {
    [a[0] * b[0], a[1] * b[1], a[2] * b[2]]
}

impl Editor {
    fn bset(&mut self, i: EntId, v: V3) {
        if let Some(b) = &self.entities[i].bind {
            b.set(&mut self.doc, &self.entities, v);
        }
    }

    fn pget(&self, i: EntId, k: usize) -> V3 {
        self.entities[i].points[k].get(&self.doc, &self.entities)
    }

    fn pset(&mut self, i: EntId, k: usize, v: V3) {
        self.entities[i].points[k].set(&mut self.doc, &self.entities, v);
    }

    /// Moves an entity and its attached points, each from where it is now.
    fn shift_now(&mut self, i: EntId, f: &dyn Fn(V3) -> V3) {
        let p = self.bind_get(i);
        self.bset(i, f(p));
        for k in 0..self.entities[i].points.len() {
            let p = self.pget(i, k);
            self.pset(i, k, f(p));
        }
    }

    /// The gizmo took a press: remembers where everything is.
    pub fn begin_drag(&mut self) {
        let set = self.transform_set();
        let items = set
            .iter()
            .map(|&i| {
                let e = &self.entities[i];
                DragItem {
                    e: i,
                    p: self.bind_get(i),
                    pts: (0..e.points.len()).map(|k| self.pget(i, k)).collect(),
                    q: e.get_quat(&self.doc),
                    s: e.get_scale(&self.doc),
                    size: e.volume.as_ref().map(|v| v.box_size0(&self.doc)),
                }
            })
            .collect();
        self.drag = Some(Drag { before: self.serialize(), set, p0: self.gizmo.pos, q0: self.gizmo.quat, items });
    }

    /// The gizmo's pivot moved.
    pub fn on_drag(&mut self) {
        let Some(dr) = self.drag.take() else { return };
        let mode = self.gizmo.mode;
        let group = dr.set.len() > 1 && self.pivot_mode == "group";
        let p0 = dr.p0;
        let move_item = |ed: &mut Editor, it: &DragItem, f: &dyn Fn(V3) -> V3| {
            ed.bset(it.e, f(it.p));
            for (k, p) in it.pts.iter().enumerate() {
                ed.pset(it.e, k, f(*p));
            }
        };
        match mode {
            Mode::Translate => {
                let d = vec::sub(self.gizmo.pos, p0);
                for it in &dr.items {
                    move_item(self, it, &|p| vec::add(p, d));
                }
            }
            Mode::Rotate => {
                let dq = vec::quat_mul(self.gizmo.quat, vec::quat_inv(dr.q0));
                for it in &dr.items {
                    if group {
                        move_item(self, it, &|p| vec::add(p0, vec::quat_rotate(dq, vec::sub(p, p0))));
                    }
                    if let Some(q) = it.q {
                        self.entities[it.e].set_quat(&mut self.doc, vec::quat_mul(dq, q));
                    }
                }
            }
            Mode::Scale => {
                let f = self.gizmo.scale;
                for it in &dr.items {
                    if group {
                        move_item(self, it, &|p| vec::add(p0, mul3(vec::sub(p, p0), f)));
                    }
                    let e = &self.entities[it.e];
                    match (e.scl, it.s, it.size, &e.volume) {
                        (Some("xyz"), Some(s), _, _) => e.set_scale(&mut self.doc, mul3(s, f)),
                        (Some("box"), _, Some(size), Some(v)) => v.set_box(&mut self.doc, size, f),
                        _ => {}
                    }
                }
            }
        }
        for it in &dr.items {
            self.place_ent(it.e);
        }
        self.drag = Some(dr);
        self.emit(Event::SelectionChanged); // the inspector's values
        self.emit(Event::ViewChanged);
    }

    /// The drag ended: one undo step when it changed the level.
    pub fn end_drag(&mut self) {
        let Some(dr) = self.drag.take() else { return };
        if dr.before == self.serialize() {
            return;
        }
        let mode = self.gizmo.mode;
        let n = dr.set.len();
        let unsupported = dr
            .set
            .iter()
            .filter(|&&i| {
                let e = &self.entities[i];
                (mode == Mode::Rotate && e.rot.is_none()) || (mode == Mode::Scale && e.scl.is_none())
            })
            .count();
        self.push_undo(dr.before);
        self.after_edit(None, false);
        if unsupported > 0 {
            let (did, what) = if mode == Mode::Rotate { ("Rotated", "a rotation") } else { ("Scaled", "a size") };
            let msg = format!("{} {} objects; {} can\u{2019}t store {} and were only moved around the pivot.", did, n, unsupported, what);
            self.set_status(&msg, "");
        }
    }

    /// True while the gizmo is being dragged.
    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// The axis handle the gizmo drags or hovers.
    pub fn gizmo_axis(&self) -> Option<Handle> {
        self.gizmo.axis
    }

    /// Numeric transforms (the inspector), applied the same way as a gizmo drag. `axis` is 0, 1 or 2 for x, y
    /// or z. A rotation of 0 degrees and a scale of 0 do nothing.
    pub fn apply_transform(&mut self, move_: Option<V3>, rot_deg: Option<f64>, axis: usize, scale: Option<f64>) {
        let tset = self.transform_set();
        if tset.is_empty() {
            return;
        }
        let rot_deg = rot_deg.filter(|r| *r != 0.0);
        let scale = scale.filter(|s| *s != 0.0);
        self.edit(|ed| {
            let c = ed.centre_of(&tset);
            let group = tset.len() > 1 && ed.pivot_mode == "group";
            for &i in &tset {
                if let Some(m) = move_ {
                    ed.shift_now(i, &|p| vec::add(p, m));
                }
                if let Some(deg) = rot_deg {
                    let mut ax = [0.0; 3];
                    ax[axis.min(2)] = 1.0;
                    let dq = vec::quat_axis_angle(ax, vec::radians(deg));
                    if group {
                        ed.shift_now(i, &|p| vec::add(c, vec::quat_rotate(dq, vec::sub(p, c))));
                    }
                    let e = &ed.entities[i];
                    if let Some(q) = e.get_quat(&ed.doc) {
                        e.set_quat(&mut ed.doc, vec::quat_mul(dq, q));
                    }
                }
                if let Some(sc) = scale {
                    let f = [sc; 3];
                    if group {
                        ed.shift_now(i, &|p| vec::add(c, mul3(vec::sub(p, c), f)));
                    }
                    let e = &ed.entities[i];
                    match (e.scl, &e.volume) {
                        (Some("xyz"), _) => {
                            if let Some(s) = e.get_scale(&ed.doc) {
                                e.set_scale(&mut ed.doc, mul3(s, f));
                            }
                        }
                        (Some("box"), Some(v)) => {
                            let s0 = v.box_size0(&ed.doc);
                            v.set_box(&mut ed.doc, s0, f);
                        }
                        _ => {}
                    }
                }
            }
        });
    }
}
