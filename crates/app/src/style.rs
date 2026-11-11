//! The editor's dark look, close to the Qt style sheet of the Python app (`STYLE` in `desktop/window.py`), and the
//! small widgets the panels share.

use egui::{Color32, CornerRadius, FontId, Margin, Response, RichText, Stroke, Ui};

pub const BG: Color32 = Color32::from_rgb(0x1d, 0x1f, 0x26);
pub const PANEL: Color32 = Color32::from_rgb(0x25, 0x28, 0x32);
pub const INPUT: Color32 = Color32::from_rgb(0x15, 0x16, 0x1b);
pub const BORDER: Color32 = Color32::from_rgb(0x32, 0x36, 0x42);
pub const BORDER_HOVER: Color32 = Color32::from_rgb(0x4a, 0x50, 0x60);
pub const HOVER: Color32 = Color32::from_rgb(0x2c, 0x30, 0x3b);
pub const TEXT: Color32 = Color32::from_rgb(0xe6, 0xe7, 0xec);
pub const MUTED: Color32 = Color32::from_rgb(0x9a, 0xa0, 0xae);
pub const DISABLED: Color32 = Color32::from_rgb(0x6b, 0x70, 0x80);
pub const GOLD: Color32 = Color32::from_rgb(0xf2, 0xc1, 0x4e);
pub const GOLD_DIM: Color32 = Color32::from_rgb(0x8a, 0x6f, 0x2c);
pub const GOLD_BG: Color32 = Color32::from_rgb(0x3a, 0x32, 0x20);
pub const ON_GOLD: Color32 = Color32::from_rgb(0x23, 0x1a, 0x00);
pub const OK: Color32 = Color32::from_rgb(0x6b, 0xd8, 0x8a);
pub const ERR: Color32 = Color32::from_rgb(0xff, 0x6b, 0x6b);
pub const WARN: Color32 = Color32::from_rgb(0xff, 0xb3, 0x6b);
pub const IMP: Color32 = Color32::from_rgb(0x6f, 0xb7, 0xff);
pub const TOK: Color32 = Color32::from_rgb(0xc5, 0x8b, 0xff);

/// "#rrggbb" as a colour.
pub fn hex(s: &str) -> Color32 {
    let v = u32::from_str_radix(s.trim_start_matches('#'), 16).unwrap_or(0xffffff);
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// Sets the look for the whole app.
pub fn apply(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    ctx.all_styles_mut(|style| {
        use egui::TextStyle::*;
        style.text_styles = [
            (Heading, FontId::proportional(16.0)),
            (Body, FontId::proportional(13.0)),
            (Button, FontId::proportional(13.0)),
            (Small, FontId::proportional(11.0)),
            (Monospace, FontId::monospace(12.0)),
        ]
        .into();
        let s = &mut style.spacing;
        s.item_spacing = egui::vec2(6.0, 4.0);
        s.button_padding = egui::vec2(9.0, 3.0);
        s.interact_size = egui::vec2(28.0, 22.0);
        s.icon_width = 14.0;
        s.icon_width_inner = 8.0;
        s.combo_width = 60.0;
        s.scroll.bar_width = 8.0;
        s.scroll.floating = false;
        s.menu_margin = Margin::same(6);
        let v = &mut style.visuals;
        v.dark_mode = true;
        v.override_text_color = None;
        v.panel_fill = BG;
        v.window_fill = BG;
        v.window_stroke = Stroke::new(1.0, BORDER_HOVER);
        v.window_corner_radius = CornerRadius::same(8);
        v.extreme_bg_color = INPUT;
        v.text_edit_bg_color = Some(INPUT);
        v.faint_bg_color = PANEL;
        v.code_bg_color = INPUT;
        v.hyperlink_color = IMP;
        v.warn_fg_color = WARN;
        v.error_fg_color = ERR;
        v.selection.bg_fill = Color32::from_rgb(0x5a, 0x4b, 0x22);
        v.selection.stroke = Stroke::new(1.0, GOLD);
        let radius = CornerRadius::same(6);
        let w = &mut v.widgets;
        w.noninteractive.bg_fill = BG;
        w.noninteractive.weak_bg_fill = BG;
        w.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
        w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
        w.noninteractive.corner_radius = radius;
        w.inactive.bg_fill = PANEL;
        w.inactive.weak_bg_fill = PANEL;
        w.inactive.bg_stroke = Stroke::new(1.0, BORDER);
        w.inactive.fg_stroke = Stroke::new(1.0, TEXT);
        w.inactive.corner_radius = radius;
        w.inactive.expansion = 0.0;
        w.hovered.bg_fill = HOVER;
        w.hovered.weak_bg_fill = HOVER;
        w.hovered.bg_stroke = Stroke::new(1.0, BORDER_HOVER);
        w.hovered.fg_stroke = Stroke::new(1.0, TEXT);
        w.hovered.corner_radius = radius;
        w.hovered.expansion = 0.0;
        w.active.bg_fill = GOLD_BG;
        w.active.weak_bg_fill = GOLD_BG;
        w.active.bg_stroke = Stroke::new(1.0, GOLD);
        w.active.fg_stroke = Stroke::new(1.0, GOLD);
        w.active.corner_radius = radius;
        w.active.expansion = 0.0;
        w.open.bg_fill = HOVER;
        w.open.weak_bg_fill = HOVER;
        w.open.bg_stroke = Stroke::new(1.0, BORDER_HOVER);
        w.open.fg_stroke = Stroke::new(1.0, TEXT);
        w.open.corner_radius = radius;
    });
}

pub fn h2(ui: &mut Ui, text: &str) -> Response {
    ui.label(RichText::new(text).size(16.0).strong().color(TEXT))
}

pub fn h3(ui: &mut Ui, text: &str) -> Response {
    ui.label(RichText::new(text).strong().color(TEXT))
}

/// A small heading in capitals.
pub fn h4(ui: &mut Ui, text: &str) -> Response {
    ui.add_space(4.0);
    ui.label(RichText::new(text).size(11.0).strong().color(MUTED).extra_letter_spacing(1.0))
}

pub fn muted(ui: &mut Ui, text: &str) -> Response {
    ui.label(RichText::new(text).color(MUTED))
}

pub fn note(ui: &mut Ui, text: &str) -> Response {
    ui.label(RichText::new(text).size(12.0).color(MUTED))
}

pub fn src(ui: &mut Ui, text: &str) -> Response {
    ui.label(RichText::new(text).size(11.0).color(MUTED))
}

pub fn fieldname(ui: &mut Ui, text: &str) -> Response {
    ui.add(egui::Label::new(RichText::new(text).monospace().size(12.0).color(MUTED)).wrap_mode(egui::TextWrapMode::Extend))
}

pub fn tok(ui: &mut Ui, text: &str) -> Response {
    ui.label(RichText::new(text).monospace().color(TOK))
}

/// The gold button of a dialog or toolbar.
pub fn primary(text: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.to_string()).strong().color(ON_GOLD)).fill(GOLD).stroke(Stroke::new(1.0, GOLD))
}

pub fn accent(text: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.to_string()).color(GOLD)).stroke(Stroke::new(1.0, GOLD_DIM))
}

/// A toolbar button that stays down while its tool is on.
pub fn toggle(ui: &mut Ui, on: bool, text: &str, tip: &str) -> Response {
    let b = if on {
        egui::Button::new(RichText::new(text).color(GOLD)).fill(GOLD_BG).stroke(Stroke::new(1.0, GOLD)).selected(true)
    } else {
        egui::Button::new(text)
    };
    ui.add(b).on_hover_text(tip)
}

/// A small button for a card's header.
pub fn mini(ui: &mut Ui, text: &str, tip: &str) -> Response {
    let r = ui.add(egui::Button::new(RichText::new(text).size(11.0)).small().corner_radius(4));
    if tip.is_empty() {
        r
    } else {
        r.on_hover_text(tip)
    }
}

/// A small button that opens a menu.
pub fn mini_menu(ui: &mut Ui, text: &str, tip: &str, add: impl FnOnce(&mut Ui)) {
    let b = egui::Button::new(RichText::new(text).size(11.0)).small().corner_radius(4);
    let r = egui::containers::menu::MenuButton::from_button(b).ui(ui, |ui| {
        egui::ScrollArea::vertical().max_height(420.0).show(ui, add);
    });
    if !tip.is_empty() {
        r.0.on_hover_text(tip);
    }
}

pub fn level_color(level: &str) -> Color32 {
    match level {
        "ok" => OK,
        "err" => ERR,
        "warn" => WARN,
        "imp" => IMP,
        _ => MUTED,
    }
}

/// A line of the Checks panel or of a dialog's report: a box with a coloured left edge. True when it was clicked.
pub fn check_line(ui: &mut Ui, level: &str, text: &str, clickable: bool, link: Option<&(String, String)>, sub: Option<&str>, title: Option<&str>) -> bool {
    let frame = egui::Frame::new().fill(PANEL).corner_radius(6).inner_margin(Margin { left: 10, right: 8, top: 5, bottom: 5 });
    let out = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 2.0;
        let sense = if clickable { egui::Sense::click() } else { egui::Sense::hover() };
        let mut r = ui.add(egui::Label::new(RichText::new(text).color(TEXT)).sense(sense));
        if let Some(sub) = sub {
            src(ui, sub);
        }
        if let Some((href, name)) = link {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                src(ui, "game:");
                ui.hyperlink_to(RichText::new(name).size(11.0), href);
            });
        }
        if clickable {
            r = r.on_hover_cursor(egui::CursorIcon::PointingHand);
        }
        r.clicked()
    });
    let rect = out.response.rect;
    let edge = egui::Rect::from_min_max(rect.min, egui::pos2(rect.min.x + 3.0, rect.max.y));
    ui.painter().rect_filled(edge, CornerRadius { nw: 6, sw: 6, ne: 0, se: 0 }, level_color(level));
    let mut clicked = out.inner;
    if clickable {
        let whole = ui.interact(rect, ui.id().with(("chk", rect.min.y as i32, text.len())), egui::Sense::click());
        clicked |= whole.clicked();
        whole.on_hover_cursor(egui::CursorIcon::PointingHand);
    }
    if let Some(t) = title {
        out.response.on_hover_text(t);
    }
    clicked
}

/// A titled box. `head` adds the buttons at the header's right, `body` what is under the title.
pub fn card<R>(ui: &mut Ui, title: &str, op: Option<&str>, head: impl FnOnce(&mut Ui), body: impl FnOnce(&mut Ui) -> R) -> R {
    let frame = egui::Frame::new().fill(PANEL).stroke(Stroke::new(1.0, BORDER)).corner_radius(8).inner_margin(Margin { left: 8, right: 8, top: 6, bottom: 8 });
    frame
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 3.0;
                    // Right to left: the buttons come in their reading order
                    ui.scope(|ui| {
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), head);
                    });
                    if let Some(op) = op {
                        ui.label(RichText::new(op).monospace().color(MUTED));
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(true), |ui| {
                        h3(ui, title);
                    });
                });
            });
            body(ui)
        })
        .inner
}

/// A one-line text box that hands back its text when the player is done with it (Enter, or the focus leaves) and
/// the text is not `shown` any more. While it has the focus it keeps what was typed.
pub fn commit_text(ui: &mut Ui, id: egui::Id, shown: &str, edit: impl FnOnce(egui::TextEdit) -> egui::TextEdit) -> (Option<String>, Response) {
    let mut buf = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| shown.to_string());
    let r = ui.add(edit(egui::TextEdit::singleline(&mut buf).id(id)));
    if r.lost_focus() {
        ui.data_mut(|d| d.remove::<String>(id));
        return ((buf != shown).then_some(buf), r);
    }
    if r.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, buf));
    } else {
        ui.data_mut(|d| d.remove::<String>(id));
    }
    (None, r)
}

/// A number as the inspector shows it: three decimals, without trailing zeros.
pub fn num_text(v: f64) -> String {
    if v.is_nan() {
        return "nan".to_string();
    }
    let t = format!("{v:.3}");
    let t = t.trim_end_matches('0').trim_end_matches('.');
    if t == "-0" {
        "0".to_string()
    } else {
        t.to_string()
    }
}

/// A number box that writes on Enter or when it loses the focus. Gives the new value when it changed.
pub fn num_input(ui: &mut Ui, id: egui::Id, value: f64, width: f32, label: &str) -> Option<f64> {
    let shown = num_text(value);
    let (text, r) = commit_text(ui, id, &shown, |t| t.desired_width(width));
    name(&r, label);
    let v: f64 = text?.trim().parse().ok()?;
    ((v - value).abs() > 1e-9).then_some(v)
}

/// Gives a widget a name for screen readers and tests.
pub fn name(r: &Response, label: &str) {
    if !label.is_empty() {
        let enabled = r.enabled();
        let label = label.to_string();
        r.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, enabled, &label));
    }
}

/// Python's float(): a number, or None.
pub fn parse_f64(s: &str) -> Option<f64> {
    glovebox_core::pyfmt::py_float(s.trim()).ok()
}
