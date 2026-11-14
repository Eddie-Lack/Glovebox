//! Number fields that change by dragging, as in a paint program.
//!
//! A press on a number field that moves left or right changes the value: one step for each point the pointer
//! moves. Shift makes the step ten times finer and Ctrl ten times coarser. A press without movement puts the
//! caret in the field for typing. Esc during a drag puts the old value back.
//!
//! A field without the keyboard focus is drawn as a text box that takes no input, under an area that takes the
//! click and the drag. A field with the focus is the plain text box: a drag in it selects text.
//!
//! The fields do not know the editor. They leave signals (`Signal`) that the window reads once a frame: it opens
//! one undo step when a drag begins and closes it when the drag ends.

use egui::{Id, Response, Sense, Ui};

/// What a drag may do to a field's number.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spec {
    /// The change for one point of pointer movement.
    pub step: f64,
    pub min: f64,
    pub max: f64,
    /// A whole number: the step never goes under 1.
    pub integer: bool,
}

impl Spec {
    /// A number with decimals and no limits.
    pub const fn float(step: f64) -> Spec {
        Spec { step, min: f64::NEG_INFINITY, max: f64::INFINITY, integer: false }
    }

    /// A whole number between two limits.
    pub const fn int(min: i64, max: i64) -> Spec {
        Spec { step: 1.0, min: min as f64, max: max as f64, integer: true }
    }

    pub fn at_least(mut self, min: f64) -> Spec {
        self.min = min;
        self
    }

    /// The step that suits a number as it is written: 1 for a whole number, 0.1 or 0.01 for one with decimals.
    pub fn for_text(text: &str) -> Spec {
        let decimals = text.trim().split_once('.').map_or(0, |(_, d)| d.bytes().take_while(u8::is_ascii_digit).count());
        Spec::float([1.0, 0.1, 0.01][decimals.min(2)])
    }

    /// The limits of a field of the level format: `u1`, `s2`, `f4` and the like. None for any other type.
    pub fn for_type(ty: &str, name: &str, text: &str) -> Option<Spec> {
        let bits = match ty.get(1..)? {
            "1" => 8,
            "2" => 16,
            "4" => 32,
            _ => return None,
        };
        match ty.as_bytes()[0] {
            b'u' => Some(Spec::int(0, (1i64 << bits) - 1)),
            b's' => Some(Spec::int(-(1i64 << (bits - 1)), (1i64 << (bits - 1)) - 1)),
            b'f' if bits == 32 => Some(if name.contains("scale") { Spec::float(0.01) } else { Spec::for_text(text) }),
            _ => None,
        }
    }
}

/// What the fields tell the window about the drag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signal {
    Begin,
    End,
    Cancel,
}

fn signals_id() -> Id {
    Id::unique("number field signals")
}

fn signal(ui: &Ui, s: Signal) {
    ui.data_mut(|d| d.get_temp_mut_or_default::<Vec<Signal>>(signals_id()).push(s));
}

/// The signals since the last call.
pub fn take_signals(ctx: &egui::Context) -> Vec<Signal> {
    ctx.data_mut(|d| d.remove_temp::<Vec<Signal>>(signals_id())).unwrap_or_default()
}

#[derive(Clone)]
struct Drag {
    start: f64,
    moved: f64,
    before: String,
    canceled: bool,
}

/// A number as a field shows it, rounded to `unit`.
fn text_of(v: f64, unit: f64, integer: bool) -> String {
    if integer {
        return format!("{}", v.round() as i64);
    }
    let decimals = (-unit.log10()).ceil().clamp(0.0, 6.0) as usize;
    let t = format!("{v:.decimals$}");
    let t = if t.contains('.') { t.trim_end_matches('0').trim_end_matches('.') } else { &t };
    if t == "-0" {
        "0".to_string()
    } else {
        t.to_string()
    }
}

pub const TIP: &str = "Drag left or right to change the number. Shift: ten times finer. Ctrl: ten times coarser. Click to type.";

/// A one-line text box. With a `spec` and without the keyboard focus, a drag changes its number.
///
/// Gives the box's own response (for `lost_focus` and `changed`), the response to name and to give a tooltip,
/// and the text a drag made in this frame. The caller writes that text to where the number lives. On Esc the
/// text is the one from before the drag.
pub fn field(ui: &mut Ui, id: Id, buf: &mut String, spec: Option<Spec>, edit: impl FnOnce(egui::TextEdit) -> egui::TextEdit) -> (Response, Response, Option<String>) {
    let drag_id = id.with("drag");
    let focused = ui.memory(|m| m.has_focus(id));
    let Some(spec) = spec.filter(|_| ui.is_enabled() && !focused) else {
        let r = ui.add(edit(egui::TextEdit::singleline(buf).id(id)));
        return (r.clone(), r, None);
    };
    let value = glovebox_core::pyfmt::parse_float(buf.trim()).filter(|v| v.is_finite());
    let r = ui.add(edit(egui::TextEdit::singleline(buf).id(id)).interactive(false));
    let over = ui.interact(r.rect, drag_id, Sense::click_and_drag());
    if over.clicked() || over.has_focus() {
        ui.memory_mut(|m| m.request_focus(id));
    }
    let mut out = None;
    let mut st = ui.data(|d| d.get_temp::<Drag>(drag_id));
    if let (true, Some(v)) = (over.drag_started_by(egui::PointerButton::Primary), value) {
        st = Some(Drag { start: v, moved: 0.0, before: buf.clone(), canceled: false });
        signal(ui, Signal::Begin);
    }
    // egui itself ends a drag on Esc, so Esc is looked at first
    let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
    if let Some(d) = st.as_mut().filter(|d| !d.canceled && (esc || over.dragged())) {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        if esc {
            d.canceled = true;
            signal(ui, Signal::Cancel);
            out = (d.before != *buf).then(|| d.before.clone());
        } else {
            // The frame the drag begins counts the way from the press: the value follows the pointer exactly
            let started = over.drag_started();
            let (dx, mods) = ui.input(|i| {
                let whole = i.pointer.press_origin().zip(i.pointer.latest_pos()).map(|(a, b)| b.x - a.x);
                (whole.filter(|_| started).unwrap_or(i.pointer.delta().x) as f64, i.modifiers)
            });
            let factor = if mods.shift {
                0.1
            } else if mods.ctrl || mods.command {
                10.0
            } else {
                1.0
            };
            d.moved += dx * spec.step * factor;
            let unit = if spec.integer { (spec.step * factor).max(1.0) } else { spec.step * factor };
            let v = (((d.start + d.moved) / unit).round() * unit).clamp(spec.min, spec.max);
            let text = text_of(v, unit.min(spec.step), spec.integer);
            out = (text != buf.trim()).then_some(text);
        }
    }
    if over.drag_stopped() || !ui.input(|i| i.pointer.primary_down()) {
        if let Some(d) = st.take() {
            if !d.canceled {
                signal(ui, Signal::End);
            }
        }
    }
    ui.data_mut(|m| match st {
        Some(d) => {
            m.insert_temp(drag_id, d);
        }
        None => m.remove::<Drag>(drag_id),
    });
    let over = if value.is_some() { over.on_hover_cursor(egui::CursorIcon::ResizeHorizontal).on_hover_text(TIP) } else { over.on_hover_cursor(egui::CursorIcon::Text) };
    (r, over, out)
}

/// A number box bound to a text the panel keeps: a drag writes the text at once.
pub fn bound(ui: &mut Ui, id: Id, value: &mut String, spec: Spec, edit: impl FnOnce(egui::TextEdit) -> egui::TextEdit) -> (Response, bool) {
    let (edit_r, r, out) = field(ui, id, value, Some(spec), edit);
    let mut changed = edit_r.changed();
    if let Some(text) = out {
        *value = text;
        changed = true;
    }
    (r, changed)
}

/// A whole-number box. Gives the new value when the player typed one (Enter, or the focus leaves) or drags.
pub fn int(ui: &mut Ui, id: Id, value: i64, range: std::ops::RangeInclusive<i64>, width: f32, label: &str) -> Option<i64> {
    let shown = value.to_string();
    let (text, r) = crate::style::commit_number(ui, id, &shown, Some(Spec::int(*range.start(), *range.end())), |t| t.desired_width(width));
    crate::style::name(&r, label);
    let v = glovebox_core::pyfmt::parse_float(text?.trim()).filter(|v| v.is_finite())?.round() as i64;
    let v = v.clamp(*range.start(), *range.end());
    (v != value).then_some(v)
}
