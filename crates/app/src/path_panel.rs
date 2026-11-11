//! The path tool's panel: what to place along the path, scattering the selection beside it, and camera zones over
//! it.

use crate::app::Window;
use crate::style::{self, card, fieldname, muted, note};
use glovebox_core::pyfmt::parse_float;
use glovebox_editor::editor::Scatter;
use glovebox_editor::entities::CAM_PRESETS;

pub struct State {
    what: usize,
    garib: usize,
    step: String,
    side: usize,
    off: String,
    up: String,
    turn: bool,
    sc_space: [String; 2],
    sc_dist: [String; 2],
    sc_scale: [String; 2],
    sc_dy: [String; 2],
    sc_side: usize,
    sc_turn: usize,
    sc_tilt: bool,
    sc_solid: bool,
    pub sc_seed: i64,
    /// The last scatter: the undo entry it made, the level right after it and the selection it used.
    scattered: Option<(String, String, Vec<Vec<usize>>)>,
    zn_kind: usize,
    zn_span: [String; 2],
    zn_split: String,
}

fn pair(a: &str, b: &str) -> [String; 2] {
    [a.to_string(), b.to_string()]
}

impl Default for State {
    fn default() -> State {
        State {
            what: 0,
            garib: 0,
            step: "60".to_string(),
            side: 0,
            off: "120".to_string(),
            up: "12".to_string(),
            turn: false,
            sc_space: pair("80", "170"),
            sc_dist: pair("180", "420"),
            sc_scale: pair("0.8", "1.2"),
            sc_dy: pair("-25", "15"),
            sc_side: 0,
            sc_turn: 0,
            sc_tilt: false,
            sc_solid: false,
            sc_seed: 0,
            scattered: None,
            zn_kind: 0,
            zn_span: pair("0", "100"),
            zn_split: "600".to_string(),
        }
    }
}

const WHAT: [(&str, &str); 2] = [("garib", "garibs"), ("copy", "copies of the selection")];
const GARIBS: [(&str, &str); 4] = [("garib", "garib"), ("bang_500pt", "500-point bang"), ("extra_life", "extra life"), ("mad_garib", "mad garib")];
const SIDES: [(f64, &str); 4] = [(0.0, "on the path"), (1.0, "right side"), (-1.0, "left side"), (2.0, "both sides")];
const SC_SIDES: [(&[f64], &str); 3] = [(&[1.0, -1.0], "both sides"), (&[1.0], "right side"), (&[-1.0], "left side")];
const SC_TURNS: [(&str, &str); 3] = [("free", "any way"), ("player", "towards the path"), ("path", "along the path")];

fn row(ui: &mut egui::Ui, name: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        let r = fieldname(ui, name);
        ui.add_space((104.0 - r.rect.width()).max(0.0));
        add(ui);
    });
}

fn pick(ui: &mut egui::Ui, id: &str, label: &str, current: &mut usize, items: &[&str]) -> bool {
    let mut changed = false;
    let r = egui::ComboBox::from_id_salt(id).selected_text(items.get(*current).copied().unwrap_or("")).width(ui.available_width().clamp(60.0, 190.0)).truncate().show_ui(ui, |ui| {
        for (i, text) in items.iter().enumerate() {
            if ui.selectable_label(*current == i, *text).clicked() {
                *current = i;
                changed = true;
            }
        }
    });
    let label = label.to_string();
    r.response.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, &label));
    changed
}

fn text(ui: &mut egui::Ui, value: &mut String, label: &str, width: f32) {
    let r = ui.add(egui::TextEdit::singleline(value).desired_width(width));
    style::name(&r, label);
}

fn two(ui: &mut egui::Ui, values: &mut [String; 2], label: &str) {
    text(ui, &mut values[0], &format!("{label} from"), 60.0);
    text(ui, &mut values[1], &format!("{label} to"), 60.0);
}

/// Two finite numbers, or None.
fn vals(p: &[String; 2]) -> Option<(f64, f64)> {
    let (a, b) = (parse_float(p[0].trim())?, parse_float(p[1].trim())?);
    (a.is_finite() && b.is_finite()).then_some((a, b))
}

pub fn ui(w: &mut Window, ui: &mut egui::Ui) {
    egui::ScrollArea::vertical().id_salt("path").auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        egui::Frame::new().inner_margin(egui::Margin { left: 4, right: 8, top: 4, bottom: 8 }).show(ui, |ui| body(w, ui));
    });
}

fn body(w: &mut Window, ui: &mut egui::Ui) {
    muted(ui, &w.ed.path_info());
    let st = &mut w.path_panel;
    row(ui, "place", |ui| {
        if pick(ui, "what", "place", &mut st.what, &WHAT.map(|x| x.1)) {
            st.up = if st.what == 0 { "12" } else { "0" }.to_string();
        }
    });
    row(ui, "garib type", |ui| {
        pick(ui, "garib", "garib type", &mut st.garib, &GARIBS.map(|x| x.1));
    });
    row(ui, "every", |ui| text(ui, &mut st.step, "every", 80.0));
    row(ui, "where", |ui| {
        pick(ui, "side", "where", &mut st.side, &SIDES.map(|x| x.1));
    });
    row(ui, "to the side", |ui| text(ui, &mut st.off, "to the side", 80.0));
    row(ui, "over the ground", |ui| text(ui, &mut st.up, "over the ground", 80.0));
    row(ui, "random turn", |ui| {
        ui.checkbox(&mut st.turn, "one selected model, about y");
    });
    note(ui, "Click the ground to add points. Backspace takes the last one off. Each copy is dropped onto the ground under it.");
    ui.horizontal(|ui| {
        if ui.button("Clear path").clicked() {
            w.acts.push(Box::new(|w: &mut Window| {
                w.ed.path.pts.clear();
                w.ed.draw_path();
            }));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.add(style::primary("Place")).clicked() {
                w.acts.push(Box::new(place));
            }
        });
    });
    scatter_box(w, ui);
    zone_box(w, ui);
}

/// Places garibs or copies of the selection along the path.
pub fn place(w: &mut Window) {
    let st = &w.path_panel;
    let f = |s: &str, d: f64| if s.trim().is_empty() { Some(d) } else { parse_float(s.trim()) };
    let (Some(step), Some(off), Some(up)) = (f(&st.step, 60.0), f(&st.off, 0.0), f(&st.up, 0.0)) else {
        return w.ed.set_status("The path settings must be numbers.", "err");
    };
    let (what, garib, side, turn) = (WHAT[st.what].0, GARIBS[st.garib].0, SIDES[st.side].0, st.turn);
    w.ed.place_along_path(what, garib, step, side, off, up, turn);
}

/// Scatter: copies of the selected scenery beside the path at random spacing, distance, scale and turn.
fn scatter_box(w: &mut Window, ui: &mut egui::Ui) {
    let st = &mut w.path_panel;
    card(ui, "Scatter the selection", None, |_| {}, |ui| {
        row(ui, "spacing", |ui| two(ui, &mut st.sc_space, "spacing"));
        row(ui, "from the path", |ui| two(ui, &mut st.sc_dist, "from the path"));
        row(ui, "where", |ui| {
            pick(ui, "sc side", "scatter where", &mut st.sc_side, &SC_SIDES.map(|x| x.1));
        });
        row(ui, "scale", |ui| two(ui, &mut st.sc_scale, "scale"));
        row(ui, "height", |ui| two(ui, &mut st.sc_dy, "height"));
        row(ui, "facing", |ui| {
            pick(ui, "sc turn", "facing", &mut st.sc_turn, &SC_TURNS.map(|x| x.1));
        });
        row(ui, "", |ui| {
            ui.checkbox(&mut st.sc_tilt, "tilt a little");
        });
        row(ui, "", |ui| {
            ui.checkbox(&mut st.sc_solid, "solid (LandActor)");
        });
        row(ui, "seed", |ui| {
            let r = ui.add(egui::DragValue::new(&mut st.sc_seed).range(0..=99999).speed(0.1));
            r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::DragValue, true, "scatter seed"));
        });
        note(ui, "Select one or more landscape pieces or kit models; each copy picks one of them at random. Every copy takes heap memory: the status line says how much.");
        ui.horizontal(|ui| {
            if ui.button("Try another").on_hover_text("Undo the last scatter and scatter again with the next seed").clicked() {
                w.acts.push(Box::new(rescatter));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(style::primary("Scatter")).clicked() {
                    w.acts.push(Box::new(scatter));
                }
            });
        });
    });
}

pub fn scatter(w: &mut Window) {
    let st = &w.path_panel;
    let (Some(spacing), Some(dist), Some(scale), Some(dy)) = (vals(&st.sc_space), vals(&st.sc_dist), vals(&st.sc_scale), vals(&st.sc_dy)) else {
        return w.ed.set_status("The scatter settings must be numbers.", "err");
    };
    let o = Scatter {
        spacing,
        dist,
        scale,
        dy,
        sides: SC_SIDES[st.sc_side].0.to_vec(),
        turn: SC_TURNS[st.sc_turn].0.to_string(),
        tilt: if st.sc_tilt { 0.15 } else { 0.0 },
        solid: st.sc_solid,
        seed: st.sc_seed,
        ..Scatter::default()
    };
    let ed = &mut w.ed;
    let before = (ed.undo_stack.len(), ed.undo_stack.last().cloned());
    let sel: Vec<Vec<usize>> = ed.selection.iter().filter(|&&i| !ed.entities[i].is_handle && !ed.entities[i].derived).map(|&i| ed.doc.path_of(ed.entities[i].node)).collect();
    ed.scatter_along_path(&o);
    // The scatter's own undo entry and the level right after it: Try another undoes only that
    let made = ed.undo_stack.last().cloned().filter(|_| (ed.undo_stack.len(), ed.undo_stack.last().cloned()) != before);
    w.path_panel.scattered = made.map(|m| (m, w.ed.serialize(), sel));
}

pub fn rescatter(w: &mut Window) {
    let ed = &mut w.ed;
    if let Some((made, after, paths)) = &w.path_panel.scattered {
        if ed.undo_stack.last() == Some(made) && ed.serialize() == *after {
            ed.undo();
            let nodes: Vec<_> = paths.iter().filter_map(|p| ed.doc.node_at(p)).collect();
            let sel = (0..ed.entities.len()).filter(|&i| !ed.entities[i].is_handle && !ed.entities[i].derived && nodes.contains(&ed.entities[i].node)).collect();
            ed.set_selection(sel);
        }
    }
    w.path_panel.sc_seed += 1;
    scatter(w);
}

/// Camera zones fitted to a stretch of the path.
fn zone_box(w: &mut Window, ui: &mut egui::Ui) {
    let st = &mut w.path_panel;
    card(ui, "Camera zone over the path", None, |_| {}, |ui| {
        row(ui, "camera", |ui| {
            pick(ui, "zone kind", "camera", &mut st.zn_kind, &CAM_PRESETS.map(|x| x.1));
        });
        row(ui, "from, to %", |ui| two(ui, &mut st.zn_span, "zone"));
        row(ui, "boxes up to", |ui| text(ui, &mut st.zn_split, "boxes up to", 60.0));
        note(ui, "Boxes 140 to each side of the path, 300 below and 400 above. The boxes line up with the axes, so a diagonal stretch is split into shorter boxes.");
        if ui.button("Add zones").clicked() {
            w.acts.push(Box::new(add_zones));
        }
    });
}

pub fn add_zones(w: &mut Window) {
    let st = &w.path_panel;
    let split = if st.zn_split.trim().is_empty() { Some(0.0) } else { parse_float(st.zn_split.trim()) };
    let (Some((a, b)), Some(split)) = (vals(&st.zn_span), split) else {
        return w.ed.set_status("The zone settings must be numbers.", "err");
    };
    let key = CAM_PRESETS[st.zn_kind].0;
    w.ed.path_zones(key, a / 100.0, b / 100.0, 140.0, 300.0, 400.0, split);
}
