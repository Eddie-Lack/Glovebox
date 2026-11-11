//! Selection, the pivot the gizmo sits on, the tool modes, framing the camera, and the mouse in the 3D view.

use super::{Editor, Event};
use crate::entities::{EntId, WHY_NO_ORIENTATION, WHY_NO_SCALE};
use crate::gizmo::{Mode, Snap, Space};
use crate::scene::RayOptions;
use crate::vec::{self, Box3, V3};

/// The keys held during a mouse event. `ctrl` is Ctrl or the Mac's command key.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
    Middle,
}

/// What the view's mouse is doing.
#[derive(Clone, Debug, Default)]
pub(crate) struct Mouse {
    down: Option<(f64, f64)>,
    last: Option<(f64, f64)>,
    button: Option<Button>,
    box_start: Option<(f64, f64)>,
    at: (f64, f64),
    dragging_gizmo: bool,
    flying: bool,
}

fn hypot(x: f64, y: f64) -> f64 {
    x.hypot(y)
}

impl Editor {
    // -----------------------------------------------------------------------------------------------------------
    // Selection

    pub fn select(&mut self, ent: Option<EntId>, focus: bool) {
        self.set_selection(ent.into_iter().collect());
        if let (true, Some(e)) = (focus, ent) {
            self.focus_on(e);
        }
    }

    pub fn toggle_select(&mut self, ent: Option<EntId>) {
        let Some(e) = ent else { return };
        let mut sel = self.selection.clone();
        match sel.iter().position(|x| *x == e) {
            Some(i) => {
                sel.remove(i);
            }
            None => sel.push(e),
        }
        self.set_selection(sel);
    }

    pub fn set_selection(&mut self, items: Vec<EntId>) {
        let mut out: Vec<EntId> = Vec::new();
        for e in items {
            if e < self.entities.len() && !out.contains(&e) {
                out.push(e);
            }
        }
        if !out.is_empty() {
            self.puzzle_index = None;
        }
        self.selection = out;
        if !self.box_mode {
            self.update_pivot();
        }
        self.emit(Event::SelectionChanged);
        self.emit(Event::ViewChanged);
    }

    /// The entity the inspector shows: the last one selected.
    pub fn selected(&self) -> Option<EntId> {
        self.selection.last().copied()
    }

    /// Shift+click in a list: adds everything between the selected entity and `e`.
    pub fn range_select(&mut self, items: &[EntId], e: EntId) {
        let a = self.selected().and_then(|s| items.iter().position(|x| *x == s));
        let (Some(a), Some(b)) = (a, items.iter().position(|x| *x == e)) else {
            return self.select(Some(e), false);
        };
        let mut sel = self.selection.clone();
        sel.extend_from_slice(&items[a.min(b)..=a.max(b)]);
        self.set_selection(sel);
    }

    /// Objects the transform applies to. Path handles whose platform is also selected and child platforms whose
    /// parent is selected are left out: they are stored relative to it and would move twice.
    pub fn transform_set(&self) -> Vec<EntId> {
        let sel = &self.selection;
        sel.iter()
            .copied()
            .filter(|&i| {
                let e = &self.entities[i];
                e.bind.is_some()
                    && !e.derived
                    && !(e.is_handle && e.owner.is_some_and(|o| sel.contains(&o)))
                    && !e.parent_ent.is_some_and(|p| sel.contains(&p))
            })
            .collect()
    }

    pub(crate) fn bind_get(&self, i: EntId) -> V3 {
        match &self.entities[i].bind {
            Some(b) => b.get(&self.doc, &self.entities),
            None => [0.0; 3],
        }
    }

    /// The mean of the entities' positions, summed as Python's `sum` did.
    pub(crate) fn centre_of(&self, set: &[EntId]) -> V3 {
        let mut c = [0.0; 3];
        for &i in set {
            c = vec::add(c, self.bind_get(i));
        }
        vec::div(c, set.len() as f64)
    }

    /// Puts the gizmo on the selection, or takes it away.
    pub fn update_pivot(&mut self) {
        self.gizmo.detach();
        let tset = self.transform_set();
        if tset.is_empty() {
            self.emit(Event::ViewChanged);
            return;
        }
        let single = (tset.len() == 1).then(|| tset[0]);
        if let Some(s) = single {
            let e = &self.entities[s];
            if self.mode == Mode::Rotate && e.rot.is_none() {
                let msg = format!("{}: {}", e.label, e.why_rot.unwrap_or(WHY_NO_ORIENTATION));
                return self.set_status(&msg, "");
            }
            if self.mode == Mode::Scale && e.scl.is_none() {
                let msg = format!("{}: {}", e.label, e.why_scl.unwrap_or(WHY_NO_SCALE));
                return self.set_status(&msg, "");
            }
        }
        let pos = match single {
            Some(s) => self.bind_get(s),
            None => self.centre_of(&tset),
        };
        // A single object rotates and scales about its own axes
        let quat = single.and_then(|s| self.entities[s].get_quat(&self.doc)).unwrap_or_else(vec::quat_identity);
        self.gizmo.mode = self.mode;
        let local = single.is_some_and(|s| self.entities[s].rot == Some("xyz"));
        self.gizmo.space = match self.mode {
            Mode::Translate => Space::World,
            _ if local => Space::Local,
            Mode::Scale => Space::Local,
            _ => Space::World,
        };
        let on = self.snap != 0.0;
        self.gizmo.snap = Snap { translate: on.then_some(self.snap), rotate: on.then(|| vec::radians(15.0)), scale: on.then_some(0.05) };
        self.gizmo.attach(pos, quat);
        self.emit(Event::ViewChanged);
    }

    pub fn set_mode(&mut self, m: Mode) {
        self.mode = m;
        self.update_pivot();
        self.emit(Event::ModeChanged);
    }

    pub fn set_box_mode(&mut self, on: bool) {
        self.box_mode = on;
        // The gizmo is detached while box-selecting. It would otherwise grab a drag that starts on it.
        if on {
            self.gizmo.detach();
        } else {
            self.update_pivot();
        }
        self.emit(Event::ModeChanged);
    }

    /// Moves the camera to look at an entity, or at the whole selection when it holds several.
    pub fn focus_on(&mut self, ent: EntId) {
        if ent >= self.entities.len() {
            return;
        }
        let mut b = Box3::new();
        if self.selection.len() > 1 {
            for &e in &self.selection {
                b.add_box(&self.obj_box(e));
            }
        } else {
            b.add_box(&self.obj_box(ent));
        }
        let c = if b.empty() { self.obj_pos(ent) } else { b.center() };
        let r = if b.empty() {
            60.0
        } else {
            let n = vec::norm(b.size());
            if n > 60.0 {
                n
            } else {
                60.0
            }
        };
        let d = vec::sub(self.camera.pos, self.orbit.target);
        let n = vec::norm(d);
        let d = vec::div(d, if n != 0.0 { n } else { 1.0 });
        self.orbit.target = c;
        self.camera.pos = vec::add(c, vec::scale(vec::scale(d, r), 1.4));
        self.camera.look_at(c);
        self.emit(Event::ViewChanged);
    }

    /// Moves the camera to see the whole level.
    pub fn frame_level(&mut self) {
        let mut b = Box3::new();
        for i in 0..self.entities.len() {
            if matches!(self.entities[i].kind, "land" | "platform") {
                b.add_box(&self.obj_box(i));
            }
        }
        if b.empty() {
            for i in 0..self.entities.len() {
                b.add_box(&self.obj_box(i));
            }
        }
        if b.empty() {
            return;
        }
        let (c, r) = (b.center(), vec::norm(b.size()));
        self.orbit.target = c;
        let d = [0.55, 0.6, 0.75];
        self.camera.pos = vec::add(c, vec::scale(vec::scale(vec::div(d, vec::norm(d)), r), 0.7));
        self.camera.look_at(c);
    }

    // -----------------------------------------------------------------------------------------------------------
    // The mouse in the 3D view. Positions are pixels from the view's top left corner.

    /// The size of the 3D view in pixels. Set it before any mouse call and whenever the view changes size.
    pub fn set_view_size(&mut self, width: f64, height: f64) {
        self.camera.width = if width > 1.0 { width } else { 1.0 };
        self.camera.height = if height > 1.0 { height } else { 1.0 };
    }

    /// The window's fly camera is on: clicks then do not lay the path or drop garibs, and drags do not orbit.
    pub fn set_flying(&mut self, on: bool) {
        self.mouse.flying = on;
    }

    /// A button went down. True when the editor took the press for a box selection or a gizmo drag.
    pub fn mouse_press(&mut self, x: f64, y: f64, button: Button, mods: Mods) -> bool {
        self.mouse.down = Some((x, y));
        self.mouse.last = Some((x, y));
        self.mouse.at = (x, y);
        self.mouse.button = Some(button);
        if button == Button::Left && (self.box_mode || (mods.ctrl && mods.alt)) && self.gizmo.axis.is_none() {
            self.mouse.box_start = Some((x, y));
            return true;
        }
        if button == Button::Left && !mods.ctrl && self.gizmo.attached && self.gizmo.begin(&self.camera, x, y) {
            self.mouse.dragging_gizmo = true;
            self.begin_drag();
            return true;
        }
        false
    }

    /// The mouse moved. True when the view has to be drawn again.
    pub fn mouse_move(&mut self, x: f64, y: f64) -> bool {
        self.mouse.at = (x, y);
        if self.mouse.dragging_gizmo {
            if self.gizmo.drag_to(&self.camera, x, y) {
                self.on_drag();
            }
            return true;
        }
        if self.mouse.box_start.is_some() {
            return true;
        }
        if self.mouse.flying {
            return false;
        }
        if let (Some(_), Some(last)) = (self.mouse.down, self.mouse.last) {
            let (dx, dy) = (x - last.0, y - last.1);
            match self.mouse.button {
                Some(Button::Left) => self.orbit.rotate(&mut self.camera, dx, dy),
                Some(Button::Right) => self.orbit.pan(&mut self.camera, dx, dy),
                Some(Button::Middle) => self.orbit.dolly(&mut self.camera, -dy / 10.0),
                None => {}
            }
            self.mouse.last = Some((x, y));
            return true;
        }
        self.gizmo.attached && self.gizmo.hover(&self.camera, x, y)
    }

    /// The rubber band of a box selection in progress: (left, top, right, bottom).
    pub fn box_rect(&self) -> Option<(f64, f64, f64, f64)> {
        let (x0, y0) = self.mouse.box_start?;
        let (x, y) = self.mouse.at;
        Some((x0.min(x), y0.min(y), x0.max(x), y0.max(y)))
    }

    /// A button came up: ends a gizmo drag or a box selection, or handles a click.
    pub fn mouse_release(&mut self, x: f64, y: f64, button: Button, mods: Mods) {
        let down = self.mouse.down.take();
        self.mouse.last = None;
        if self.mouse.dragging_gizmo {
            self.mouse.dragging_gizmo = false;
            self.gizmo.end();
            self.end_drag();
            self.update_pivot();
            return;
        }
        if let Some((x0, y0)) = self.mouse.box_start.take() {
            if hypot(x - x0, y - y0) > 4.0 {
                let (a, b) = ((x0.min(x), y0.min(y)), (x0.max(x), y0.max(y)));
                let with_land = !self.hidden.contains("land") && self.include_land;
                let mut hits = Vec::new();
                for i in self.pickable() {
                    if self.entities[i].kind == "land" && !mods.alt && !with_land {
                        continue;
                    }
                    let p = self.camera.to_pixels(self.obj_pos(i));
                    if p[2] < 1.0 && a.0 <= p[0] && p[0] <= b.0 && a.1 <= p[1] && p[1] <= b.1 {
                        hits.push(i);
                    }
                }
                let sel = if mods.shift { self.selection.iter().copied().chain(hits).collect() } else { hits };
                self.set_selection(sel);
                self.set_box_mode(false);
                return;
            }
        }
        let Some(down) = down else { return };
        if hypot(x - down.0, y - down.1) > 4.0 || button != Button::Left {
            return;
        }
        let flying = self.mouse.flying;
        if self.path.on && !flying {
            return self.path_click(x, y);
        }
        let Some((origin, d)) = self.camera.ray(x, y) else { return };
        if mods.shift && !mods.ctrl && !flying {
            let ground = self.ground_objects(None, false);
            let o = RayOptions { camera: Some(&self.camera), ..RayOptions::default() };
            let hits = self.graph.raycast(&ground, origin, d, &o);
            if let Some(h) = hits.first() {
                let p = vec::add(h.point, [0.0, 12.0, 0.0]);
                self.add_garib(p, "garib");
            }
            return;
        }
        let pick = self.pick(origin, d);
        if mods.ctrl {
            self.toggle_select(pick);
        } else {
            self.select(pick, false);
        }
    }

    /// The entity a ray picks: the nearest that is not landscape, else the nearest landscape piece.
    pub fn pick(&self, origin: V3, direction: V3) -> Option<EntId> {
        let objs: Vec<_> = self.pickable().into_iter().map(|i| self.nodes[i].obj).collect();
        let o = RayOptions { camera: Some(&self.camera), line_px: Some(5.0), ..RayOptions::default() };
        let hits = self.graph.raycast(&objs, origin, direction, &o);
        let picks: Vec<EntId> = hits.iter().filter_map(|h| self.entity_of(h.node)).collect();
        picks.iter().copied().find(|&p| self.entities[p].kind != "land").or(picks.first().copied())
    }

    /// The entity under a pixel of the view.
    pub fn pick_at(&self, x: f64, y: f64) -> Option<EntId> {
        let (origin, d) = self.camera.ray(x, y)?;
        self.pick(origin, d)
    }
}
