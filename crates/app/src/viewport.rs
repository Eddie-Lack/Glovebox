//! The 3D view: draws the level into the renderer's picture and shows it, and turns the mouse and the fly keys into
//! camera moves, picks, box selections and gizmo drags. Over the picture it draws the HUD: the level's name, the
//! autosave chip, the hint line and the loading note.
//!
//! Click: select · Ctrl+click: add/remove · B or Ctrl+Alt+drag: box select · drag: orbit · right drag: pan · wheel:
//! zoom. The fly camera (C): WASD moves along the view, Space and Shift move straight up and down, dragging turns
//! the view, and the wheel sets the speed.

use crate::app::Window;
use crate::render::icons::Icons;
use crate::render::{Fog, Frame, Renderer};
use crate::style;
use crate::tour::Target;
use eframe::egui_wgpu::{wgpu, RenderState};
use glovebox_editor::editor::{Button, Editor, Mods};
use glovebox_editor::entities::EntId;
use glovebox_editor::scene::{self, Batch, Item, LineMode, Lines, Prefab};
use glovebox_editor::vec;
use std::cell::RefCell;
use std::collections::{BTreeSet, HashSet};
use std::rc::Rc;

pub const HINT_ORBIT: &str = "Click: select · Ctrl+click: add/remove · B: box select · Ctrl+A: all · W/E/R: move/rotate/scale · Shift+click: drop garib · A: add · C: fly · P: path";
pub const HINT_FLY: &str = "Flying · WASD: move · Space/Shift: up/down · drag: look · wheel: speed · click: select · C or Esc: stop";

/// The fly keys: W, S, A, D and Space, as an index into `FLY_DIRS`.
pub fn fly_key(key: egui::Key) -> Option<u8> {
    Some(match key {
        egui::Key::W => 0,
        egui::Key::S => 1,
        egui::Key::A => 2,
        egui::Key::D => 3,
        egui::Key::Space => 4,
        _ => return None,
    })
}

const FLY_DIRS: [[f64; 3]; 6] = [[0.0, 0.0, -1.0], [0.0, 0.0, 1.0], [-1.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, -1.0, 0.0]];

pub struct Fly {
    pub on: bool,
    /// Units per second.
    pub speed: f64,
    /// A drag that turns the view: where it began and the camera's angles then.
    look: Option<(f64, f64, vec::V3)>,
}

pub struct View {
    rs: Option<RenderState>,
    renderer: Option<Renderer>,
    texture: Option<egui::TextureId>,
    sel_node: scene::NodeId,
    batches: Vec<Batch>,
    batched: HashSet<(scene::NodeId, usize)>,
    batch_key: Option<(Vec<String>, Vec<EntId>)>,
    pub fly: Fly,
    pub fly_keys: BTreeSet<u8>,
    /// The picture is out of date.
    pub dirty: bool,
    /// Where the view is in the window, in points.
    pub rect: egui::Rect,
    /// The background of a level without fog: the theme's.
    pub background: [f64; 3],
    /// The button a press in the view holds until it comes up.
    held: Option<Button>,
    /// Why the 3D view could not start.
    pub error: Option<String>,
    /// The number of frames drawn.
    pub frames: u64,
}

impl View {
    pub fn new(rs: Option<RenderState>, ed: &mut Editor) -> View {
        let icons = Rc::new(RefCell::new(Icons::new()));
        ed.icon_maker = Some(Box::new(move |glyph, color| Some(icons.borrow_mut().texture(glyph, color))));
        let sel_node = ed.graph.lines_node(Lines::new(Vec::new(), [0.95, 0.76, 0.31], LineMode::Segments));
        let renderer = rs.as_ref().map(|rs| Renderer::new(&rs.device, &rs.queue));
        let error = renderer.is_none().then(|| "The 3D view could not start: the window has no graphics device.".to_string());
        View {
            rs,
            renderer,
            texture: None,
            sel_node,
            batches: Vec::new(),
            batched: HashSet::new(),
            batch_key: None,
            fly: Fly { on: false, speed: 800.0, look: None },
            fly_keys: BTreeSet::new(),
            dirty: true,
            rect: egui::Rect::NOTHING,
            background: crate::theme::VIEW_BG,
            held: None,
            error,
            frames: 0,
        }
    }

    /// A new level: the merged meshes and the GPU copies of the old one go.
    pub fn forget_level(&mut self) {
        self.batch_key = None;
        self.dirty = true;
    }

    /// The merged meshes are out of date (the entities were built again).
    pub fn rebuilt(&mut self) {
        self.batch_key = None;
        self.dirty = true;
    }

    pub fn set_fly(&mut self, ed: &mut Editor, on: bool) {
        self.fly.on = on;
        self.fly.look = None;
        self.fly_keys.clear();
        ed.set_flying(on);
        if !on {
            // Orbit about a point ahead of the camera, as far away as the old target was
            let d = vec::norm(vec::sub(ed.camera.pos, ed.orbit.target)).clamp(100.0, 3000.0);
            ed.orbit.target = vec::add(ed.camera.pos, vec::scale(ed.camera.forward(), d));
        }
        self.dirty = true;
    }

    /// A picture of a model for the palette. None when it has nothing to draw.
    pub fn thumbnail(&mut self, model: &Prefab) -> Option<egui::ColorImage> {
        const SIZE: u32 = 128;
        let rgba = self.renderer.as_mut()?.render_thumbnail(model, SIZE)?;
        Some(egui::ColorImage::from_rgba_premultiplied([SIZE as usize, SIZE as usize], &rgba))
    }

    /// The view's picture as RGBA bytes with its width and height in pixels (tests).
    pub fn pixels(&self) -> Option<(u32, u32, Vec<u8>)> {
        let r = self.renderer.as_ref()?;
        let (w, h) = r.size()?;
        Some((w, h, r.read_pixels().ok()?))
    }

    fn draw(&mut self, ed: &mut Editor, width: u32, height: u32) {
        let (Some(r), Some(rs)) = (self.renderer.as_mut(), self.rs.as_ref()) else { return };
        if r.resize(width, height) || self.texture.is_none() {
            let Some(view) = r.view() else { return };
            let mut egui_renderer = rs.renderer.write();
            match self.texture {
                Some(id) => egui_renderer.update_egui_texture_from_wgpu_texture(&rs.device, view, wgpu::FilterMode::Linear, id),
                None => self.texture = Some(egui_renderer.register_native_texture(&rs.device, view, wgpu::FilterMode::Linear)),
            }
        }
        // A level without fog has the theme's background
        r.background = if ed.background == crate::theme::VIEW_BG { self.background } else { ed.background };
        r.fog = ed.fog().map(|(color, near, far)| Fog { color, near, far });
        r.ambient = ed.ambient;
        let points = ed.selection_box_points();
        if let Some(Item::Lines(l)) = ed.graph.get_mut(self.sel_node).and_then(|n| n.items.first_mut()) {
            if !(points.is_empty() && l.points().is_empty()) {
                l.set_points(points);
            }
        }
        let mut roots = vec![ed.world, self.sel_node, ed.path_node];
        roots.extend(ed.far_sphere);
        // Opaque meshes are merged per material once. What moves without a rebuild (a drag, the motion preview)
        // is left out and drawn on its own.
        let moving = ed.moving();
        let key = (ed.hidden.iter().cloned().collect::<Vec<_>>(), moving.clone());
        let fresh = self.batch_key.as_ref() != Some(&key);
        if fresh {
            let skip = |_: scene::NodeId, n: &scene::Node| n.entity.is_some_and(|e| moving.contains(&(e as usize)));
            (self.batches, self.batched) = scene::make_batches(&ed.graph, &[ed.world], &skip);
            self.batch_key = Some(key);
        }
        let gizmo = ed.gizmo.node(&mut ed.graph, &ed.camera);
        let overlay = [gizmo];
        r.draw(&Frame { overlay: &overlay, batches: &self.batches, batched: Some(&self.batched), ..Frame::new(&ed.graph, &roots, &ed.camera) });
        ed.graph.delete(gizmo);
        if fresh {
            r.collect(&ed.graph, &roots, &self.batches);
        }
        self.frames += 1;
        self.dirty = false;
    }
}

fn button(b: egui::PointerButton) -> Option<Button> {
    match b {
        egui::PointerButton::Primary => Some(Button::Left),
        egui::PointerButton::Secondary => Some(Button::Right),
        egui::PointerButton::Middle => Some(Button::Middle),
        _ => None,
    }
}

fn mods(m: egui::Modifiers) -> Mods {
    Mods { shift: m.shift, ctrl: m.ctrl || m.command, alt: m.alt }
}

/// The mouse in the view. Positions are points from the view's top left corner.
fn input(w: &mut Window, ui: &egui::Ui, rect: egui::Rect, over: bool) {
    let blocked = w.modal();
    let (ed, view) = (&mut w.ed, &mut w.view);
    let events = ui.input(|i| i.events.clone());
    for e in events {
        match e {
            egui::Event::PointerButton { pos, button: b, pressed, modifiers } => {
                let Some(b) = button(b) else { continue };
                let (x, y) = (f64::from(pos.x - rect.min.x), f64::from(pos.y - rect.min.y));
                if pressed {
                    if !over || blocked || !rect.contains(pos) || view.held.is_some() {
                        continue;
                    }
                    view.held = Some(b);
                    let took = ed.mouse_press(x, y, b, mods(modifiers));
                    if !took && view.fly.on && b != Button::Middle {
                        view.fly.look = Some((x, y, vec::euler_yxz_from_quat(ed.camera.quat)));
                    }
                    view.dirty = true;
                } else if view.held == Some(b) {
                    view.held = None;
                    view.fly.look = None;
                    ed.mouse_release(x, y, b, mods(modifiers));
                    view.dirty = true;
                }
            }
            egui::Event::PointerMoved(pos) => {
                if view.held.is_none() && !(over && rect.contains(pos)) {
                    continue;
                }
                let (x, y) = (f64::from(pos.x - rect.min.x), f64::from(pos.y - rect.min.y));
                if let Some((lx, ly, r)) = view.fly.look {
                    let k = 0.004;
                    ed.camera.quat = vec::quat_from_euler_yxz((r[0] - (y - ly) * k).clamp(-1.55, 1.55), r[1] - (x - lx) * k, 0.0);
                    view.dirty = true;
                } else if ed.mouse_move(x, y) {
                    view.dirty = true;
                }
            }
            egui::Event::MouseWheel { unit, delta, .. } if over && !blocked => {
                let steps = match unit {
                    egui::MouseWheelUnit::Point => f64::from(delta.y) / 50.0,
                    egui::MouseWheelUnit::Line => f64::from(delta.y),
                    egui::MouseWheelUnit::Page => f64::from(delta.y) * 10.0,
                };
                if steps == 0.0 {
                    continue;
                }
                if view.fly.on {
                    view.fly.speed = (view.fly.speed * if steps > 0.0 { 1.25 } else { 0.8 }).clamp(50.0, 20000.0);
                    ed.set_status(&format!("Fly speed {} units per second", view.fly.speed.round()), "");
                } else {
                    ed.orbit.dolly(&mut ed.camera, steps);
                    view.dirty = true;
                }
            }
            _ => {}
        }
    }
}

/// Moves the fly camera by the keys held.
fn fly_step(w: &mut Window, ui: &egui::Ui) {
    let view = &mut w.view;
    // Keys typed into a text box or at a dialog are not the camera's
    if !view.fly.on || w.dialog.is_some() || w.tour.active() || ui.ctx().egui_wants_keyboard_input() {
        return;
    }
    let (shift, dt) = ui.input(|i| (i.modifiers.shift && !i.modifiers.ctrl && !i.modifiers.command, f64::from(i.unstable_dt)));
    let mut v = [0.0; 3];
    for k in view.fly_keys.iter().map(|k| usize::from(*k)).chain(shift.then_some(5)) {
        v = vec::add(v, FLY_DIRS[k]);
    }
    if view.fly_keys.is_empty() && !shift {
        return;
    }
    ui.ctx().request_repaint();
    if v == [0.0; 3] {
        return;
    }
    let up = v[1];
    v[1] = 0.0;
    v = vec::quat_rotate(w.ed.camera.quat, v);
    v[1] += up;
    let n = vec::norm(v);
    if n > 0.0 {
        w.ed.camera.pos = vec::add(w.ed.camera.pos, vec::scale(v, view.fly.speed * dt.min(0.1) / n));
        view.dirty = true;
    }
}

pub fn ui(w: &mut Window, ui: &mut egui::Ui) {
    let rect = ui.available_rect_before_wrap();
    // The view leaves the strips along its sides to the handles that resize the panels next to it
    let grab = ui.style().interaction.resize_grab_radius_side;
    ui.allocate_rect(rect, egui::Sense::hover());
    let mut area = rect.shrink2(egui::vec2(grab, 0.0));
    // And the strip along its bottom to the handle of the checks, when they are under it
    if w.panels.checks.is_positive() && w.layout.side(crate::layout::PanelId::Checks) == crate::layout::Side::Bottom {
        area.max.y -= grab;
    }
    let resp = ui.interact(area, ui.scope_id().with("3D view"), egui::Sense::click_and_drag());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Canvas, true, "3D view"));
    if rect != w.view.rect {
        w.view.rect = rect;
        w.view.dirty = true;
    }
    w.tour.mark(Target::View, rect);
    let background = style::theme(ui.ctx()).view_bg;
    if background != w.view.background {
        w.view.background = background;
        w.view.dirty = true;
    }
    w.ed.set_view_size(f64::from(rect.width()), f64::from(rect.height()));
    input(w, ui, rect, resp.contains_pointer());
    fly_step(w, ui);
    if w.ed.preview && !w.ed.gizmo.dragging {
        let now = w.now();
        w.ed.animate_at(now);
        w.view.dirty = true;
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
    }
    // The events of this frame's input decide what the picture shows
    w.pump();
    let ppp = ui.ctx().pixels_per_point();
    let (pw, ph) = ((rect.width() * ppp).round().max(1.0) as u32, (rect.height() * ppp).round().max(1.0) as u32);
    if w.view.dirty && w.ed.has_level && rect.width() > 4.0 && rect.height() > 4.0 {
        w.view.draw(&mut w.ed, pw, ph);
    }
    let painter = ui.painter_at(rect);
    let th = style::theme(ui.ctx());
    match w.view.texture {
        Some(t) if w.ed.has_level => {
            painter.image(t, rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
        }
        _ => {
            painter.rect_filled(rect, 0.0, th.view_color());
        }
    }
    if let Some((x0, y0, x1, y1)) = w.ed.box_rect() {
        let r = egui::Rect::from_min_max(rect.min + egui::vec2(x0 as f32, y0 as f32), rect.min + egui::vec2(x1 as f32, y1 as f32));
        painter.rect_filled(r, 0.0, th.select_box.1);
        painter.rect_stroke(r, 0.0, egui::Stroke::new(1.0, th.select_box.0), egui::StrokeKind::Inside);
    }
    if resp.contains_pointer() && (w.ed.box_mode || w.ed.path.on) {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    }
    hud(w, ui, rect);
}

/// The level's name, the autosave chip, the hint line and the loading note.
fn hud(w: &mut Window, ui: &mut egui::Ui, rect: egui::Rect) {
    let inner = rect.shrink2(egui::vec2(10.0, 8.0));
    // The chips are boxes of the theme, a little see-through
    let th = style::theme(ui.ctx());
    let mut top = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::top_down(egui::Align::Min)));
    top.spacing_mut().item_spacing.y = 3.0;
    if !w.hud_level.is_empty() {
        let frame = style::chip(th, false).inner_margin(egui::Margin::symmetric(6, 1));
        frame.show(&mut top, |ui| ui.label(egui::RichText::new(&w.hud_level).font(egui::FontId::new(13.0, style::bold())).color(th.chip_title)));
    }
    if w.ed.has_level {
        let (text, tip, edited) = w.chip();
        let fg = if edited { th.chip_edited } else { th.chip.text };
        let frame = style::chip(th, edited).inner_margin(egui::Margin::symmetric(6, 1));
        frame.show(&mut top, |ui| ui.label(egui::RichText::new(text).size(12.0).color(fg))).response.on_hover_text(tip);
    }
    let hint = if w.view.fly.on { HINT_FLY } else { HINT_ORBIT };
    let mut bottom = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::bottom_up(egui::Align::Min)));
    let hint_frame = style::chip(th, false).inner_margin(egui::Margin::symmetric(6, 2));
    // The key hints are a note: shown, behind a small chip on hover, or off
    match style::info(ui.ctx()) {
        style::Info::Shown => {
            hint_frame.show(&mut bottom, |ui| ui.label(egui::RichText::new(hint).size(12.0).color(th.chip.text)));
        }
        style::Info::Hover => {
            hint_frame.show(&mut bottom, |ui| ui.label(egui::RichText::new("Keys ?").size(12.0).color(th.chip.text))).response.on_hover_text(hint);
        }
        style::Info::Off => {}
    }

    let note = w.view.error.as_deref().unwrap_or(&w.loading);
    if !note.is_empty() {
        let galley = ui.painter().layout(note.to_string(), egui::FontId::proportional(13.0), th.text, (rect.width() - 80.0).max(100.0));
        let size = egui::vec2((galley.size().x + 40.0).max(260.0), galley.size().y.max(16.0) + 30.0);
        let r = egui::Rect::from_center_size(rect.center(), size);
        style::plaque(ui.painter(), r);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(r).layout(egui::Layout::centered_and_justified(egui::Direction::TopDown)));
        child.label(note);
    }
}
