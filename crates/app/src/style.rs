//! The app's look, and the small widgets the panels share.
//!
//! Everything here is drawn from the active theme (`theme.rs`): the colors, the shapes of the controls, the fonts
//! and the details. `apply` sets a theme for a whole window. `theme` gives the active one. `Look` gives egui's
//! buttons, text boxes and separators their shapes. Title bars, checkboxes, the logo, the dividers, the frames
//! and the setup screen's background are drawn here, each in the variants the themes name.

use crate::numfield::Spec;
pub use crate::theme::Theme;
use crate::theme::{Backdrop, Bar, Border, Divider, Field, Logo, Mark, Shape, THEMES};
use egui::class::HasClasses as _;
use egui::emath::GuiRounding as _;
use egui::theme::StyleProvider;
use egui::widget_style::{AtomLayoutStyle, ButtonStyle, SeparatorStyle, StyleArgs, TextEditStyle, TextVisuals, WidgetState};
use egui::{Color32, CornerRadius, FontFamily, FontId, Margin, Rect, Response, RichText, Shadow, Stroke, Ui};

/// The key of the theme in the settings file.
pub const SETTING: &str = "theme";

/// The active theme of a window.
pub fn theme(ctx: &egui::Context) -> &'static Theme {
    ctx.data(|d| d.get_temp::<&'static Theme>(egui::Id::NULL)).unwrap_or(THEMES[0])
}

/// The bold font's family. It is the text font when the computer has no bold one.
pub fn bold() -> FontFamily {
    FontFamily::Name("bold".into())
}

/// The family of the capitals of a title bar.
pub fn label_family() -> FontFamily {
    FontFamily::Name("label".into())
}

/// The logo's font family.
pub fn logo_family() -> FontFamily {
    FontFamily::Name("logo".into())
}

/// "#rrggbb" as a color.
pub fn hex(s: &str) -> Color32 {
    let v = u32::from_str_radix(s.trim_start_matches('#'), 16).unwrap_or(0xffffff);
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// A font file of the computer, when it is there and smaller than 8 MiB.
fn system_font(names: &[&str]) -> Option<Vec<u8>> {
    let mut dirs: Vec<std::path::PathBuf> = Vec::new();
    if let Some(win) = std::env::var_os("WINDIR") {
        dirs.push(std::path::Path::new(&win).join("Fonts"));
    }
    let more = [
        "/System/Library/Fonts/Supplemental",
        "/Library/Fonts",
        "/usr/share/fonts/truetype/dejavu",
        "/usr/share/fonts/truetype/liberation",
        "/usr/share/fonts/truetype/liberation2",
        "/usr/share/fonts/truetype/crosextra",
        "/usr/share/fonts/truetype/freefont",
        "/usr/share/fonts/TTF",
        "/usr/share/fonts/dejavu",
        "/usr/share/fonts/liberation",
    ];
    dirs.extend(more.map(std::path::PathBuf::from));
    for name in names {
        for dir in &dirs {
            let path = dir.join(name);
            let small = std::fs::metadata(&path).is_ok_and(|m| m.is_file() && m.len() < 8 << 20);
            if let (true, Ok(data)) = (small, std::fs::read(&path)) {
                // A TrueType or OpenType file
                if data.len() > 12 && (data[..4] == [0, 1, 0, 0] || &data[..4] == b"OTTO" || &data[..4] == b"true") {
                    return Some(data);
                }
            }
        }
    }
    None
}

/// The fonts of a theme: the files it names when the computer has them, then the fonts compiled in. Those have
/// the arrows and shapes the buttons show. A family without a file of its own takes the family before it: the
/// bold font the text font, the label font the bold one, the logo's font the label's.
fn fonts(th: &Theme) -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    let mut rest = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    // The code font has arrows and shapes the text font lacks. A theme in the code font has it first.
    rest.retain(|f| f != "Hack");
    if th.fonts.mono {
        rest.insert(0, "Hack".to_string());
    } else {
        rest.push("Hack".to_string());
    }
    let mut family = |key: &str, files: &[&str], after: &[String]| -> Vec<String> {
        let mut list: Vec<String> = Vec::new();
        if let Some(data) = system_font(files) {
            fonts.font_data.insert(key.to_string(), std::sync::Arc::new(egui::FontData::from_owned(data)));
            list.push(key.to_string());
        }
        list.extend(after.iter().cloned());
        let fam = if key == "text" { FontFamily::Proportional } else { FontFamily::Name(key.into()) };
        fonts.families.insert(fam, list.clone());
        list
    };
    let text = family("text", th.fonts.text, &rest);
    let bold = family("bold", th.fonts.bold, &text);
    let label = family("label", th.fonts.label, &bold);
    family("logo", th.fonts.logo, &label);
    fonts
}

/// Sets a theme for the whole window. It can be called again at any time: the next frame has the new look.
pub fn apply(ctx: &egui::Context, th: &'static Theme) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::NULL, th));
    ctx.set_theme(if th.dark { egui::Theme::Dark } else { egui::Theme::Light });
    ctx.set_fonts(fonts(th));
    ctx.replace_widget_theme::<ButtonStyle>(Look(th));
    ctx.replace_widget_theme::<TextEditStyle>(Look(th));
    ctx.replace_widget_theme::<SeparatorStyle>(Look(th));
    ctx.all_styles_mut(|style| {
        use egui::TextStyle::*;
        style.text_styles = [
            (Heading, FontId::proportional(16.0)),
            (Body, FontId::proportional(th.fonts.body)),
            (Button, FontId::proportional(th.fonts.body)),
            (Small, FontId::proportional(11.0)),
            (Monospace, FontId::monospace(12.0)),
        ]
        .into();
        let s = &mut style.spacing;
        s.item_spacing = egui::vec2(6.0, 4.0);
        s.button_padding = th.button_padding.into();
        s.interact_size = egui::vec2(28.0, 22.0);
        s.icon_width = 14.0;
        s.icon_width_inner = 8.0;
        s.combo_width = 60.0;
        s.scroll = egui::style::ScrollStyle::solid();
        s.scroll.bar_width = 10.0;
        s.scroll.foreground_color = false;
        s.menu_margin = Margin::same(4);
        style.interaction.selectable_labels = false;
        let v = &mut style.visuals;
        v.dark_mode = th.dark;
        v.override_text_color = None;
        v.panel_fill = th.bg;
        v.window_fill = th.popup;
        v.window_stroke = Stroke::new(th.popup_edge.0, th.popup_edge.1);
        v.window_corner_radius = CornerRadius::same(th.round);
        v.menu_corner_radius = CornerRadius::same(th.round);
        v.popup_shadow = Shadow { offset: [3, 3], blur: 0, spread: 0, color: th.popup_shadow };
        v.window_shadow = v.popup_shadow;
        v.extreme_bg_color = th.trough;
        v.text_edit_bg_color = Some(th.field_tone.fill);
        v.faint_bg_color = if th.panel == Color32::TRANSPARENT { th.bg } else { th.panel };
        v.code_bg_color = th.well;
        v.hyperlink_color = th.link;
        v.warn_fg_color = th.warn;
        v.error_fg_color = th.err;
        v.selection.bg_fill = th.text_sel.0;
        v.selection.stroke = Stroke::new(1.0, th.text_sel.1);
        v.text_cursor.stroke = Stroke::new(2.0, th.field_tone.text);
        v.disabled_alpha = th.disabled;
        // egui's own widgets take the text color in every state. `Look` gives the buttons and the fields theirs.
        // The fills here are the handle of a scroll bar. None may be clear: a clear one marks a menu's entries.
        let w = &mut v.widgets;
        for (state, face) in [(&mut w.noninteractive, th.bg), (&mut w.inactive, th.scroll[0]), (&mut w.hovered, th.scroll[1]), (&mut w.active, th.scroll[2]), (&mut w.open, th.scroll[1])] {
            state.bg_fill = face;
            state.weak_bg_fill = face;
            state.bg_stroke = Stroke::new(1.0, th.line);
            state.fg_stroke = Stroke::new(1.0, th.text);
            state.corner_radius = CornerRadius::same(th.round.min(5));
            state.expansion = 0.0;
        }
        w.hovered.fg_stroke = Stroke::new(1.0, th.hover_text);
        w.active.fg_stroke = Stroke::new(1.0, th.hover_text);
    });
}

// ---------------------------------------------------------------------------------------------------------------
// The shapes of egui's own widgets

/// A button that is the main action.
pub const CLASS_PRIMARY: &str = "glovebox::primary";
/// A toolbar switch that is on.
pub const CLASS_ON: &str = "glovebox::on";
/// A button whose letters are bold.
pub const CLASS_BOLD: &str = "glovebox::bold";

/// A hard edge of one pixel under and right of a box. With a stroke of the other tone it makes a bevel.
fn edge(color: Color32) -> Shadow {
    Shadow { offset: [1, 1], blur: 0, spread: 0, color }
}

/// The widget theme: push buttons, flat list entries and text boxes in the shapes of a theme.
#[derive(Clone)]
pub struct Look(pub &'static Theme);

impl std::fmt::Debug for Look {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Look({})", self.0.id)
    }
}

impl StyleProvider<ButtonStyle> for Look {
    fn style(&mut self, args: &StyleArgs<'_>) -> ButtonStyle {
        let th = self.0;
        let StyleArgs { classes, style, state, ctx, .. } = args;
        let spacing = &style.spacing;
        let has = |c: &str| classes.has_class(c);
        let small = has(&egui::Button::CLASS_SMALL);
        let mut margin: Margin = spacing.button_padding.into();
        if small {
            margin = Margin { left: 5, right: 5, top: 1, bottom: 1 };
        }
        let min_size = if small { egui::Vec2::ZERO } else { egui::vec2(0.0, spacing.interact_size.y) };
        let body = |frame: egui::Frame, color: Color32, family: FontFamily| ButtonStyle {
            atom_layout: AtomLayoutStyle {
                min_size,
                gap: spacing.icon_spacing,
                frame,
                text_style: TextVisuals { font_id: style.override_font_id.clone().unwrap_or(FontId::new(th.fonts.body, family)), color },
                image_tint: Color32::WHITE,
                ..Default::default()
            },
        };
        let lit = matches!(state, WidgetState::Hovered | WidgetState::Active);
        let selected = has(&egui::Button::CLASS_SELECTED);
        let framed = has(&egui::Button::CLASS_FRAME) || (!has(&egui::Button::CLASS_NO_FRAME) && style.visuals.button_frame);
        if !framed {
            return body(egui::Frame::new(), if lit { th.hover_text } else { th.text }, FontFamily::Proportional);
        }
        // An entry of a menu or of a list is flat. The chosen one and the one under the pointer take a band.
        let in_menu = style.visuals.widgets.inactive.weak_bg_fill == Color32::TRANSPARENT;
        let entry = in_menu || selected || has(&egui::Button::CLASS_HIDE_FRAME_WHEN_INACTIVE);
        let (primary, on) = (has(CLASS_PRIMARY), has(CLASS_ON));
        if entry && !on && !primary {
            let (fill, color) = match (selected, lit) {
                (true, _) => (th.sel.fill, th.sel.text),
                (false, true) => (th.hover.fill, th.hover.text),
                (false, false) => (Color32::TRANSPARENT, th.text),
            };
            let frame = egui::Frame { fill, inner_margin: margin, corner_radius: CornerRadius::same(row_radius(th) as u8), ..Default::default() };
            return body(frame, color, if selected { bold() } else { FontFamily::Proportional });
        }
        // A push button
        let held = *state == WidgetState::Active && ctx.input(|i| i.pointer.any_down());
        let focused = *state == WidgetState::Active && !held;
        let hovered = *state == WidgetState::Hovered;
        let mut tone = if primary {
            if hovered { th.primary_hover } else { th.primary }
        } else if on {
            if hovered { th.on_hover } else { th.on }
        } else if hovered || has(&egui::ComboBox::CLASS_OPEN) {
            th.button_hover
        } else if held {
            th.button_down
        } else {
            th.button
        };
        let sunken = held || on;
        let (stroke, shadow) = match th.shape {
            Shape::Bevel => {
                let (light, dark) = if primary { (tone.edge, th.card_edge) } else { (th.bevel[0], th.bevel[3]) };
                match (sunken, focused) {
                    (true, _) => (Stroke::new(1.0, dark), edge(light)),
                    (false, true) => (Stroke::new(1.0, th.card_edge), edge(dark)),
                    (false, false) => (Stroke::new(1.0, light), edge(dark)),
                }
            }
            Shape::Key => {
                let lip = if sunken { Shadow::NONE } else { Shadow { offset: [0, 2], blur: 0, spread: 0, color: tone.edge } };
                (if focused { Stroke::new(2.0, th.focus) } else { Stroke::new(1.0, tone.edge) }, lip)
            }
            Shape::Outline => match (focused, th.focus_block) {
                (true, Some(text)) => {
                    tone.fill = th.focus;
                    tone.text = text;
                    (Stroke::new(1.0, th.focus), Shadow::NONE)
                }
                (true, None) => (Stroke::new(2.0, th.focus), Shadow::NONE),
                (false, _) => (Stroke::new(1.0, tone.edge), Shadow::NONE),
            },
        };
        let frame = egui::Frame { fill: tone.fill, inner_margin: margin, shadow, corner_radius: CornerRadius::same(th.radius), ..Default::default() }.apply_stroke_and_expansion_without_layout_shift(stroke, 0.0);
        let family = if primary || has(CLASS_BOLD) { bold() } else { FontFamily::Proportional };
        body(frame, tone.text, family)
    }
}

impl StyleProvider<TextEditStyle> for Look {
    fn style(&mut self, args: &StyleArgs<'_>) -> TextEditStyle {
        let th = self.0;
        let StyleArgs { classes, style, state, .. } = args;
        let read_only = classes.has_class(&egui::TextEdit::CLASS_READ_ONLY);
        let tone = th.field_tone;
        let active = *state == WidgetState::Active;
        // With the keyboard in it, the box's line is thicker and has the color of the focus
        let line = if active { Stroke::new(2.0, th.focus) } else { Stroke::new(1.0, tone.edge) };
        let boxed = egui::Frame { fill: tone.fill, inner_margin: Margin::symmetric(5, 3), corner_radius: CornerRadius::same(th.radius), ..Default::default() };
        let frame = match th.field {
            _ if read_only => egui::Frame { inner_margin: style.spacing.button_padding.into(), ..Default::default() },
            Field::Sunken => egui::Frame { shadow: edge(th.bevel[0]), ..boxed }.apply_stroke_and_expansion_without_layout_shift(line, 0.0),
            Field::Boxed => boxed.apply_stroke_and_expansion_without_layout_shift(line, 0.0),
            // The line under the tint is a hard shadow that shows below it only
            Field::Underlined => egui::Frame { shadow: Shadow { offset: [0, line.width as i8], blur: 0, spread: 0, color: line.color }, ..boxed },
        };
        let color = if read_only { th.text } else { tone.text };
        let text_style = if th.field_mono && !read_only { TextVisuals { font_id: FontId::monospace(12.0), color } } else { TextVisuals::new(style, egui::TextStyle::Body, color) };
        TextEditStyle {
            atom_layout: AtomLayoutStyle { frame, min_size: egui::vec2(0.0, style.spacing.interact_size.y), gap: style.spacing.icon_spacing, text_style, align2: Some(egui::Align2::LEFT_TOP), ..Default::default() },
            hint_text_color: th.field_hint,
            prefix_suffix_color: tone.text,
        }
    }
}

impl StyleProvider<SeparatorStyle> for Look {
    fn style(&mut self, _: &StyleArgs<'_>) -> SeparatorStyle {
        SeparatorStyle { spacing: 6.0, stroke: Stroke::new(1.0, self.0.line) }
    }
}

/// The corner radius of the band of a chosen row and of the row under the pointer.
pub fn row_radius(th: &Theme) -> f32 {
    th.round.min(4) as f32
}

/// The arrow of a combo box, in the color of its button's text.
pub fn combo_icon(ui: &Ui, rect: Rect, visuals: &egui::style::WidgetVisuals, open: bool) {
    let th = theme(ui.ctx());
    // egui hands over the colors of the button's state. The pointer is on the button when they are the lit ones.
    let lit = open || visuals.fg_stroke.color == th.hover_text;
    let rect = Rect::from_center_size(rect.center(), egui::vec2(rect.width() * 0.7, rect.height() * 0.45));
    ui.painter().add(egui::Shape::rotated_triangle(rect, 0.0, if lit { th.button_hover.text } else { th.button.text }));
}

// ---------------------------------------------------------------------------------------------------------------
// Edges, frames, dividers, the logo and the backdrops

/// A two-tone edge inside `rect`, two pixels wide: raised like a button, or sunken like a field.
fn bevel(painter: &egui::Painter, rect: Rect, tones: [Color32; 4], raised: bool) {
    let t = if raised { tones } else { [tones[2], tones[3], tones[1], tones[0]] };
    edges(painter, rect, t[0], t[3]);
    edges(painter, rect.shrink(1.0), t[1], t[2]);
}

/// The edge of a well (a picture of the palette, the trough of a bar), inside `rect`.
pub fn well_edge(painter: &egui::Painter, rect: Rect) {
    let th = theme(painter.ctx());
    match th.shape {
        Shape::Bevel => bevel(painter, rect, th.bevel, false),
        _ => {
            painter.rect_stroke(rect, 0.0, Stroke::new(1.0, th.bevel[2]), egui::StrokeKind::Inside);
        }
    }
}

/// One line along the top and left of `rect` and one along its bottom and right.
fn edges(painter: &egui::Painter, r: Rect, top_left: Color32, bottom_right: Color32) {
    let line = |a: egui::Pos2, b: egui::Pos2, c: Color32| painter.rect_filled(Rect::from_min_max(a, b), 0.0, c);
    line(r.min, egui::pos2(r.max.x, r.min.y + 1.0), top_left);
    line(r.min, egui::pos2(r.min.x + 1.0, r.max.y), top_left);
    line(egui::pos2(r.min.x, r.max.y - 1.0), r.max, bottom_right);
    line(egui::pos2(r.max.x - 1.0, r.min.y), r.max, bottom_right);
}

/// The width of the frame of the setup screen's box, of a dialog and of the note in the view.
pub const RIDGE: f32 = 6.0;

/// The theme's frame inside `rect`: `RIDGE` points wide.
pub fn border(painter: &egui::Painter, rect: Rect) {
    let th = theme(painter.ctx());
    let c = th.border_colors;
    match th.border {
        Border::Ridge => {
            painter.rect_stroke(rect, 0.0, Stroke::new(RIDGE, th.button.fill), egui::StrokeKind::Inside);
            edges(painter, rect, c[0], c[3]);
            edges(painter, rect.shrink(1.0), c[1], c[2]);
            edges(painter, rect.shrink(RIDGE - 2.0), c[2], c[1]);
            edges(painter, rect.shrink(RIDGE - 1.0), c[3], c[0]);
        }
        Border::Band => {
            painter.rect_stroke(rect, 0.0, Stroke::new(RIDGE, c[2]), egui::StrokeKind::Inside);
            painter.rect_stroke(rect, 0.0, Stroke::new(1.0, c[3]), egui::StrokeKind::Inside);
            edges(painter, rect.shrink(1.0), c[1], c[3]);
            painter.rect_stroke(rect.shrink(RIDGE - 1.0), 0.0, Stroke::new(1.0, c[0]), egui::StrokeKind::Inside);
        }
        Border::Lines(lines) => {
            for (i, (inset, width)) in lines.iter().enumerate() {
                painter.rect_stroke(rect.shrink(*inset), (th.round as f32 - inset).max(0.0), Stroke::new(*width, c[i.min(3)]), egui::StrokeKind::Inside);
            }
        }
    }
}

/// A box in the theme's frame: the setup screen and the dialogs use it.
pub fn framed<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let th = theme(ui.ctx());
    let out = egui::Frame::new().fill(th.bg).corner_radius(th.round).inner_margin(Margin::same(RIDGE as i8)).show(ui, add);
    border(ui.painter(), out.response.rect);
    out.inner
}

/// A filled box with the theme's frame around it: the note in the middle of the 3D view.
pub fn plaque(painter: &egui::Painter, rect: Rect) {
    let outer = rect.expand(RIDGE);
    let th = theme(painter.ctx());
    painter.rect_filled(outer, th.round as f32, th.bg);
    border(painter, outer);
}

/// The spotlight of the tour: the window is dimmed with the color of the theme's scrim, `spot` stays bright and gets a line in
/// the theme's focus color. Without a spot the whole window is dimmed.
pub fn spotlight(painter: &egui::Painter, window: Rect, spot: Option<Rect>) {
    let th = theme(painter.ctx());
    // Lighter than a dialog's scrim, and as strong in every theme: the window stays readable
    let [r, g, b, _] = th.scrim.to_srgba_unmultiplied();
    let dim = Color32::from_rgba_unmultiplied(r, g, b, 90);
    let Some(spot) = spot else {
        painter.rect_filled(window, 0.0, dim);
        return;
    };
    let s = spot.intersect(window);
    for r in [
        Rect::from_min_max(window.min, egui::pos2(window.max.x, s.min.y)),
        Rect::from_min_max(egui::pos2(window.min.x, s.max.y), window.max),
        Rect::from_min_max(egui::pos2(window.min.x, s.min.y), egui::pos2(s.min.x, s.max.y)),
        Rect::from_min_max(egui::pos2(s.max.x, s.min.y), egui::pos2(window.max.x, s.max.y)),
    ] {
        if r.is_positive() {
            painter.rect_filled(r, 0.0, dim);
        }
    }
    painter.rect_stroke(s, f32::from(th.radius), Stroke::new(2.0, th.focus), egui::StrokeKind::Inside);
}

/// The length of a callout's pointer.
pub const POINTER: f32 = 9.0;

/// The bubble of the tour: a box in the colors of the theme's menus, with a pointer whose tip is `tip`. The
/// pointer leaves the side of the box that is nearest to the tip.
pub fn callout(painter: &egui::Painter, rect: Rect, tip: Option<egui::Pos2>) {
    let th = theme(painter.ctx());
    let round = f32::from(th.round);
    let edge = Stroke::new(th.popup_edge.0.max(1.0), th.popup_edge.1);
    painter.rect_filled(rect.translate(egui::vec2(3.0, 3.0)), round, th.popup_shadow);
    painter.rect_filled(rect, round, th.popup);
    painter.rect_stroke(rect, round, edge, egui::StrokeKind::Inside);
    let Some(tip) = tip else { return };
    // The base of the pointer lies on the box's edge, clear of the round corners
    let half = POINTER * 0.8;
    let keep = round + half + 2.0;
    let along_x = |x: f32| x.clamp(rect.min.x + keep, (rect.max.x - keep).max(rect.min.x + keep));
    let along_y = |y: f32| y.clamp(rect.min.y + keep, (rect.max.y - keep).max(rect.min.y + keep));
    let (base, side, inward) = if tip.y < rect.min.y {
        (egui::pos2(along_x(tip.x), rect.min.y), egui::vec2(half, 0.0), egui::vec2(0.0, 1.0))
    } else if tip.y > rect.max.y {
        (egui::pos2(along_x(tip.x), rect.max.y), egui::vec2(half, 0.0), egui::vec2(0.0, -1.0))
    } else if tip.x < rect.min.x {
        (egui::pos2(rect.min.x, along_y(tip.y)), egui::vec2(0.0, half), egui::vec2(1.0, 0.0))
    } else if tip.x > rect.max.x {
        (egui::pos2(rect.max.x, along_y(tip.y)), egui::vec2(0.0, half), egui::vec2(-1.0, 0.0))
    } else {
        return;
    };
    // The base sits inside the box's line: the fill of the pointer covers the line between its two sides
    let (a, b) = (base - side + inward * (edge.width + 0.5), base + side + inward * (edge.width + 0.5));
    let point = base - inward * POINTER;
    painter.add(egui::Shape::convex_polygon(vec![a, point, b], th.popup, Stroke::NONE));
    painter.line_segment([base - side, point], edge);
    painter.line_segment([point, base + side], edge);
}

/// Shows a panel's contents in the room the panel has now. The contents cannot make the panel larger: what
/// does not fit is clipped. A panel's size then comes from its default and from drags on its edge only.
pub fn fixed<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let rect = ui.available_rect_before_wrap();
    ui.set_min_size(rect.size());
    let mut child = ui.new_child(egui::UiBuilder::new().id_salt("fixed").max_rect(rect).layout(egui::Layout::top_down(egui::Align::Min)));
    child.shrink_clip_rect(rect);
    add(&mut child)
}

/// Notes how much wider than its room a scroll area's content was. `overflow` gives the largest so far.
pub fn fits<R>(ui: &Ui, name: &'static str, out: &egui::scroll_area::ScrollAreaOutput<R>) {
    let over = out.content_size.x - out.inner_rect.width();
    let id = egui::Id::unique("panel overflow");
    ui.data_mut(|d| {
        let worst = d.get_temp_mut_or(id, (0.0f32, ""));
        if over > worst.0 {
            *worst = (over, name);
        }
    });
}

/// The largest overflow `fits` saw since the last `reset_overflow`, in points, and the scroll area's name.
pub fn overflow(ctx: &egui::Context) -> (f32, &'static str) {
    ctx.data(|d| d.get_temp(egui::Id::unique("panel overflow"))).unwrap_or((0.0, ""))
}

pub fn reset_overflow(ctx: &egui::Context) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("panel overflow"), (0.0f32, "")));
}

/// The theme's fine grid behind a panel's content, when it has one.
pub fn ground(ui: &Ui) {
    if let Some((fine, coarse)) = theme(ui.ctx()).grid {
        let rect = ui.max_rect();
        grid(&ui.painter().with_clip_rect(rect), rect, fine, coarse);
    }
}

/// Lines every 16 points, a stronger one every 80. The lines keep their places in the window, whatever `rect` is.
fn grid(painter: &egui::Painter, rect: Rect, fine: Color32, coarse: Color32) {
    const STEP: f32 = 16.0;
    let mut mesh = egui::Mesh::default();
    let (mut pass, colors) = (0, [fine, coarse]);
    while pass < 2 {
        let mut k = (rect.min.x / STEP).ceil() as i32;
        while k as f32 * STEP < rect.max.x {
            if (k % 5 == 0) == (pass == 1) {
                let x = k as f32 * STEP;
                mesh.add_colored_rect(Rect::from_min_max(egui::pos2(x, rect.min.y), egui::pos2(x + 1.0, rect.max.y)), colors[pass]);
            }
            k += 1;
        }
        let mut k = (rect.min.y / STEP).ceil() as i32;
        while k as f32 * STEP < rect.max.y {
            if (k % 5 == 0) == (pass == 1) {
                let y = k as f32 * STEP;
                mesh.add_colored_rect(Rect::from_min_max(egui::pos2(rect.min.x, y), egui::pos2(rect.max.x, y + 1.0)), colors[pass]);
            }
            k += 1;
        }
        pass += 1;
    }
    painter.add(mesh);
}

/// The theme's divider, as wide as the room and 5 points high: under a dialog's title and the setup screen's
/// logo.
pub fn divider(ui: &mut Ui) {
    let th = theme(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 5.0), egui::Sense::hover());
    let painter = ui.painter().with_clip_rect(rect);
    let band = |y0: f32, y1: f32, c: Color32| {
        painter.rect_filled(Rect::from_min_max(egui::pos2(rect.min.x, rect.min.y + y0), egui::pos2(rect.max.x, rect.min.y + y1)), 0.0, c);
    };
    match th.divider {
        Divider::Rainbow => {
            let stops = crate::theme::RAINBOW;
            let mut mesh = egui::Mesh::default();
            for (i, c) in stops.iter().enumerate() {
                let x = rect.min.x + rect.width() * i as f32 / (stops.len() - 1) as f32;
                mesh.colored_vertex(egui::pos2(x, rect.min.y), *c);
                mesh.colored_vertex(egui::pos2(x, rect.max.y), *c);
                if i > 0 {
                    let k = 2 * i as u32;
                    mesh.add_triangle(k - 2, k - 1, k);
                    mesh.add_triangle(k - 1, k, k + 1);
                }
            }
            painter.add(mesh);
        }
        Divider::Stripes => {
            // Blue, green, yellow, red: four blocks, as wide as each other
            let w = rect.width() / 4.0;
            for (i, m) in [3, 0, 2, 1].into_iter().enumerate() {
                let r = Rect::from_min_max(egui::pos2(rect.min.x + w * i as f32, rect.min.y), egui::pos2(rect.min.x + w * (i + 1) as f32, rect.max.y));
                painter.rect_filled(r, 0.0, th.marks[m]);
            }
        }
        Divider::Scale => {
            band(4.0, 5.0, th.bar_lines.0);
            let mut mesh = egui::Mesh::default();
            let mut k = 0;
            while k as f32 * 8.0 < rect.width() {
                let x = rect.min.x + k as f32 * 8.0;
                let top = if k % 5 == 0 { 0.0 } else { 2.0 };
                mesh.add_colored_rect(Rect::from_min_max(egui::pos2(x, rect.min.y + top), egui::pos2(x + 1.0, rect.max.y)), th.bar_lines.0);
                k += 1;
            }
            painter.add(mesh);
        }
        Divider::DoubleRule => {
            band(0.0, 3.0, th.bar_lines.0);
            band(4.0, 5.0, th.bar_lines.0);
        }
        Divider::DoubleLine => {
            band(1.0, 2.0, th.bar_lines.0);
            band(3.0, 4.0, th.bar_lines.0);
        }
        Divider::Line => band(2.0, 3.0, th.line),
    }
}

/// The name of the app in the theme's logo. Screen readers and tests get the text as it is written.
pub fn logo(ui: &mut Ui, text: &str, size: f32) -> Response {
    let th = theme(ui.ctx());
    let [front, second, third] = th.logo_colors;
    let shown = match th.logo {
        Logo::WordArt | Logo::Masthead | Logo::Plain => text.to_string(),
        Logo::Prompt => format!("> {}", text.to_uppercase()),
        _ => text.to_uppercase(),
    };
    let (italics, apart, shrink) = match th.logo {
        Logo::WordArt => (true, (size / 22.0).round(), 1.0),
        Logo::Label => (true, (size / 16.0).round(), 0.86),
        Logo::TitleBlock => (false, (size / 4.0).round(), 0.8),
        Logo::Masthead => (false, 0.0, 0.98),
        Logo::Prompt => (false, 0.0, 0.9),
        Logo::Plain => (false, 0.0, 1.0),
    };
    let job = |color: Color32| {
        let format = egui::TextFormat { font_id: FontId::new((size * shrink).round(), logo_family()), color, italics, extra_letter_spacing: apart, ..Default::default() };
        egui::text::LayoutJob::single_section(shown.clone(), format)
    };
    let step = (size / 14.0).round().max(1.0);
    let galley = ui.fonts_mut(|f| f.layout_job(job(front)));
    let text_size = galley.size();
    // The room around the letters, and where the letters start in it
    let (extra, at) = match th.logo {
        Logo::WordArt => (egui::vec2(2.0 * step + 2.0, 2.0 * step), egui::Vec2::ZERO),
        Logo::Label => (egui::vec2(6.0 * step + apart, 2.0 * step + 1.0), egui::vec2(3.0 * step, 0.0)),
        Logo::TitleBlock => (egui::vec2(8.0 * step + apart, 2.0 * step), egui::vec2(4.0 * step + apart, step)),
        Logo::Masthead => (egui::vec2(0.0, 2.0 * step), egui::Vec2::ZERO),
        Logo::Prompt => (egui::vec2((size * 0.62).round() + 2.0, 2.0 * step), egui::vec2(0.0, step)),
        Logo::Plain => (egui::Vec2::ZERO, egui::Vec2::ZERO),
    };
    let (rect, r) = ui.allocate_exact_size(text_size + extra, egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let rect = rect.round_to_pixels(ui.pixels_per_point());
        match th.logo {
            Logo::WordArt => {
                for (k, color) in [(2.0, third), (1.0, second)] {
                    let g = ui.fonts_mut(|f| f.layout_job(job(color)));
                    painter.galley(rect.min + egui::vec2(k * step, k * step), g, color);
                }
            }
            Logo::Label => {
                // The sticker, and a stripe along its lower edge
                painter.rect_filled(rect, th.radius as f32, second);
                painter.rect_filled(Rect::from_min_max(egui::pos2(rect.min.x, rect.max.y - step - 1.0), rect.max), 0.0, third);
            }
            Logo::TitleBlock => {
                // The block's line, with the corner marks of a sheet
                painter.rect_stroke(rect, 0.0, Stroke::new(1.0, second), egui::StrokeKind::Inside);
                let m = 2.0 * step;
                for corner in [rect.left_top(), rect.right_top(), rect.left_bottom(), rect.right_bottom()] {
                    let (dx, dy) = (if corner.x < rect.center().x { m } else { -m }, if corner.y < rect.center().y { m } else { -m });
                    let a = egui::pos2(corner.x + dx, corner.y + dy);
                    painter.rect_filled(Rect::from_two_pos(a, egui::pos2(a.x + dx.signum(), corner.y)), 0.0, third);
                    painter.rect_filled(Rect::from_two_pos(a, egui::pos2(corner.x, a.y + dy.signum())), 0.0, third);
                }
            }
            Logo::Masthead => {
                painter.rect_filled(Rect::from_min_max(egui::pos2(rect.min.x, rect.max.y - step - 1.0), egui::pos2(rect.max.x, rect.max.y - 1.0)), 0.0, second);
            }
            Logo::Prompt => {
                // The block cursor after the name
                let h = (text_size.y * 0.72).round();
                let top = rect.min.y + at.y + (text_size.y - h) / 2.0;
                painter.rect_filled(Rect::from_min_max(egui::pos2(rect.min.x + text_size.x + 2.0, top), egui::pos2(rect.max.x, top + h)), 0.0, second);
            }
            Logo::Plain => {}
        }
        painter.galley(rect.min + at, galley, front);
    }
    let label = text.to_string();
    r.widget_info(move || egui::WidgetInfo::labeled(egui::Role::Label, true, &label));
    r
}

/// What is behind the setup screen, over `rect`.
pub fn backdrop(painter: &egui::Painter, rect: Rect) {
    let th = theme(painter.ctx());
    let [ground, first, second] = th.backdrop_colors;
    painter.rect_filled(rect, 0.0, ground);
    let painter = painter.with_clip_rect(rect);
    let mut mesh = egui::Mesh::default();
    let rows = |mesh: &mut egui::Mesh, step: f32, lines: &[(f32, f32, Color32)]| {
        let mut y = rect.min.y;
        while y < rect.max.y {
            for (dy, h, c) in lines {
                mesh.add_colored_rect(Rect::from_min_max(egui::pos2(rect.min.x, y + dy), egui::pos2(rect.max.x, y + dy + h)), *c);
            }
            y += step;
        }
    };
    match th.backdrop {
        Backdrop::Stars => return stars(&painter, rect, first, second, th.text),
        Backdrop::Ribs => rows(&mut mesh, 10.0, &[(0.0, 2.0, first), (2.0, 1.0, second)]),
        Backdrop::Grid => return grid(&painter, rect, first, second),
        Backdrop::Ruled => {
            rows(&mut mesh, 24.0, &[(23.0, 1.0, first)]);
            mesh.add_colored_rect(Rect::from_min_max(egui::pos2(rect.min.x + 72.0, rect.min.y), egui::pos2(rect.min.x + 73.5, rect.max.y)), second);
        }
        Backdrop::Scanlines => rows(&mut mesh, 3.0, &[(0.0, 1.0, first)]),
        Backdrop::Plain => return,
    }
    painter.add(mesh);
}

/// The star background of a 1998 page, tiled over `rect`.
fn stars(painter: &egui::Painter, rect: Rect, bright: Color32, faint: Color32, dot: Color32) {
    const TILE: f32 = 72.0;
    // The stars of the page's tile: the corners of each, from its SVG path
    let big: [(f32, f32); 10] = [(18.0, 6.0), (21.0, 13.0), (28.0, 13.0), (22.0, 17.0), (24.0, 25.0), (18.0, 20.0), (12.0, 25.0), (14.0, 17.0), (8.0, 13.0), (15.0, 13.0)];
    let small: [(f32, f32); 10] = [(54.0, 42.0), (56.0, 47.0), (61.0, 47.0), (57.0, 50.0), (59.0, 56.0), (54.0, 52.0), (49.0, 56.0), (51.0, 50.0), (47.0, 47.0), (52.0, 47.0)];
    let dots = [(44.0, 12.0, dot), (10.0, 52.0, dot), (64.0, 26.0, bright.lerp_to_gamma(dot, 0.5))];
    let mut mesh = egui::Mesh::default();
    let star = |mesh: &mut egui::Mesh, at: egui::Vec2, pts: &[(f32, f32); 10], color: Color32| {
        let n = pts.len() as f32;
        let center = egui::pos2(pts.iter().map(|p| p.0).sum::<f32>() / n, pts.iter().map(|p| p.1).sum::<f32>() / n);
        let base = mesh.vertices.len() as u32;
        mesh.colored_vertex(center + at, color);
        for p in pts {
            mesh.colored_vertex(egui::pos2(p.0, p.1) + at, color);
        }
        for k in 0..10u32 {
            mesh.add_triangle(base, base + 1 + k, base + 1 + (k + 1) % 10);
        }
    };
    let mut y = rect.min.y;
    while y < rect.max.y {
        let mut x = rect.min.x;
        while x < rect.max.x {
            let at = egui::vec2(x, y);
            star(&mut mesh, at, &big, bright);
            star(&mut mesh, at, &small, faint);
            for (dx, dy, c) in dots {
                mesh.add_colored_rect(Rect::from_min_size(egui::pos2(x + dx, y + dy), egui::vec2(2.0, 2.0)), c);
            }
            x += TILE;
        }
        y += TILE;
    }
    painter.add(mesh);
}

// ---------------------------------------------------------------------------------------------------------------
// Title bars and texts

/// Which title a bar holds: a panel's or a dialog's, a card's, or a small heading.
#[derive(Clone, Copy, PartialEq)]
enum Rank {
    Panel,
    Card,
    Small,
}

fn bar_color(th: &Theme, rank: Rank) -> Color32 {
    if rank == Rank::Small {
        th.bar_text_small
    } else {
        th.bar_text
    }
}

/// A text for a title bar: capitals in the label font, a little apart.
fn bar_text(th: &Theme, text: &str, size: f32, rank: Rank) -> RichText {
    RichText::new(text.to_uppercase()).font(FontId::new(size, label_family())).color(bar_color(th, rank)).extra_letter_spacing(th.fonts.label_spacing)
}

/// What a title bar has behind and beside its text. `rect` is the bar, `text` the place of its letters.
fn bar_shape(th: &Theme, rect: Rect, text: Rect, rank: Rank) -> egui::Shape {
    let fill = |r: Rect, c: Color32| egui::Shape::rect_filled(r, 0.0, c);
    let rows = |y0: f32, y1: f32| Rect::from_min_max(egui::pos2(rect.min.x, y0), egui::pos2(rect.max.x, y1));
    let (light, dark) = th.bar_lines;
    // A line beside the letters, to the bar's right edge
    let mid = text.center().y.round() + 0.5;
    let (x0, x1) = (text.max.x + 8.0, rect.max.x - 8.0);
    let room = x1 - x0 > 24.0 && text.height() < 24.0;
    let run = |dy: f32, c: Color32| fill(Rect::from_min_max(egui::pos2(x0, mid + dy - 0.5), egui::pos2(x1, mid + dy + 0.5)), c);
    let mut shapes = Vec::new();
    match th.bar {
        Bar::Solid => {
            shapes.extend([fill(rect, th.bar_fill), fill(rows(rect.min.y, rect.min.y + 1.0), light), fill(rows(rect.max.y - 1.0, rect.max.y), dark)]);
        }
        Bar::Soft => match rank {
            Rank::Panel => shapes.extend([fill(rect, th.bar_fill), fill(rows(rect.max.y - 1.0, rect.max.y), dark)]),
            Rank::Card => {
                // The strip is inside the card's line
                let r = th.round.saturating_sub(1);
                shapes.push(egui::Shape::rect_filled(rect, CornerRadius { nw: r, ne: r, sw: 0, se: 0 }, th.bar_fill));
            }
            Rank::Small => shapes.push(fill(rows(rect.max.y - 1.0, rect.max.y), light)),
        },
        Bar::Plate => {
            shapes.push(fill(rect, th.bar_fill));
            if rank == Rank::Panel {
                shapes.push(fill(Rect::from_min_max(rect.min, egui::pos2(rect.min.x + 4.0, rect.max.y)), light));
            }
        }
        Bar::Dimension => {
            if room {
                shapes.push(run(0.0, light));
                for (x, way) in [(x0, 1.0), (x1, -1.0)] {
                    shapes.push(fill(Rect::from_min_max(egui::pos2(x - 0.5, mid - 4.5), egui::pos2(x + 0.5, mid + 4.5)), light));
                    if rank != Rank::Small {
                        let head = vec![egui::pos2(x, mid), egui::pos2(x + way * 7.0, mid - 2.5), egui::pos2(x + way * 7.0, mid + 2.5)];
                        shapes.push(egui::Shape::convex_polygon(head, light, Stroke::NONE));
                    }
                }
            }
            if rank == Rank::Panel {
                shapes.push(fill(rows(rect.max.y - 1.0, rect.max.y), dark));
            }
        }
        Bar::Rule => match rank {
            Rank::Panel => shapes.extend([fill(rows(rect.min.y, rect.min.y + 3.0), light), fill(rows(rect.max.y - 1.0, rect.max.y), dark)]),
            Rank::Card => shapes.push(fill(rows(rect.min.y, rect.min.y + 2.0), light)),
            Rank::Small => shapes.push(fill(rows(rect.max.y - 1.0, rect.max.y), light)),
        },
        Bar::Inverse => {
            if rank == Rank::Panel {
                shapes.push(fill(rect, th.bar_fill));
            } else {
                shapes.push(fill(text.expand2(egui::vec2(4.0, 1.0)), th.bar_fill));
                if room && rank == Rank::Card {
                    shapes.extend([run(-1.5, light), run(1.5, light)]);
                } else if room {
                    shapes.push(run(0.0, dark));
                }
            }
        }
    }
    egui::Shape::Vec(shapes)
}

/// Shows `text` in a title bar's letters. Screen readers and tests get the text as it is written.
fn bar_label(ui: &mut Ui, text: &str, size: f32, wrap: bool, rank: Rank) -> Response {
    let label = egui::Label::new(bar_text(theme(ui.ctx()), text, size, rank));
    let r = ui.add(if wrap { label.wrap() } else { label.truncate() });
    let name = text.to_string();
    r.widget_info(move || egui::WidgetInfo::labeled(egui::Role::Label, true, &name));
    r
}

/// A bar over the whole width with a title in it.
fn bar(ui: &mut Ui, title: &str, size: f32, margin: Margin, rank: Rank) -> Response {
    let out = egui::Frame::new().inner_margin(margin).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let back = ui.painter().add(egui::Shape::Noop);
        let r = bar_label(ui, title, size, false, rank);
        (back, r)
    });
    let (back, r) = out.inner;
    let th = theme(ui.ctx());
    ui.painter().set(back, bar_shape(th, out.response.rect, r.rect, rank));
    anchor(ui, out.response.rect, Some((r.rect, bar_color(th, rank))));
    r
}

/// A title bar over the whole width: the title of a panel or of a dialog.
pub fn title_bar(ui: &mut Ui, title: &str) -> Response {
    bar(ui, title, 12.0, Margin { left: 8, right: 8, top: 3, bottom: 3 }, Rank::Panel)
}

/// A panel's title bar with widgets at its right end.
pub fn title_bar_with(ui: &mut Ui, title: &str, right: impl FnOnce(&mut Ui)) -> Response {
    let out = egui::Frame::new().inner_margin(Margin { left: 8, right: 4, top: 3, bottom: 3 }).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let back = ui.painter().add(egui::Shape::Noop);
        let row = ui.horizontal(|ui| {
            let r = bar_label(ui, title, 12.0, false, Rank::Panel);
            let widgets = ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), right).response.rect;
            (r, widgets.min.x)
        });
        (back, row.inner.0, row.inner.1)
    });
    let (back, r, widgets) = out.inner;
    let th = theme(ui.ctx());
    let rect = out.response.rect;
    // A bar with a line beside its letters ends the line before the widgets
    let shape = match th.bar {
        Bar::Dimension => bar_shape(th, Rect::from_min_max(rect.min, egui::pos2(widgets, rect.max.y)), r.rect, Rank::Panel),
        _ => bar_shape(th, rect, r.rect, Rank::Panel),
    };
    ui.painter().set(back, shape);
    anchor(ui, rect, Some((r.rect, bar_color(th, Rank::Panel))));
    r
}

pub fn h2(ui: &mut Ui, text: &str) -> Response {
    let th = theme(ui.ctx());
    let r = ui.label(RichText::new(text).font(FontId::new(15.0, bold())).color(th.text));
    anchor(ui, r.rect, Some((r.rect, th.muted)));
    r
}

pub fn h3(ui: &mut Ui, text: &str) -> Response {
    ui.label(RichText::new(text).font(FontId::new(13.0, bold())).color(theme(ui.ctx()).text))
}

/// A small heading in capitals, on a bar as wide as the room.
pub fn h4(ui: &mut Ui, text: &str) -> Response {
    bar(ui, text, 11.0, Margin { left: 6, right: 6, top: 2, bottom: 2 }, Rank::Small)
}

pub fn muted(ui: &mut Ui, text: &str) -> Response {
    ui.label(RichText::new(text).color(theme(ui.ctx()).muted))
}

/// A small muted text about the player's own level. It shows whatever the Info texts setting is.
pub fn remark(ui: &mut Ui, text: &str) -> Response {
    ui.label(RichText::new(text).size(12.0).color(theme(ui.ctx()).muted))
}

// ---------------------------------------------------------------------------------------------------------------
// Notes: the texts that explain how the game or the editor works
//
// Every such text goes through `note`, `note_on`, `card_note`, `para` or `para_on`, or is the second line of
// `check_line`. The Info texts setting says how they show. A text about the player's own level (a value, a
// status, a warning) is not a note: it uses `muted` or `remark`.

/// The key of the Info texts setting in the settings file.
pub const INFO_SETTING: &str = "info";

/// How the notes show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Info {
    /// Under the thing they explain.
    Shown,
    /// As a tooltip of the thing they explain.
    Hover,
    /// Not at all.
    Off,
}

impl Info {
    pub const ALL: [Info; 3] = [Info::Shown, Info::Hover, Info::Off];

    /// The name in the settings file.
    pub fn id(self) -> &'static str {
        match self {
            Info::Shown => "shown",
            Info::Hover => "hover",
            Info::Off => "off",
        }
    }

    /// The name in the Options menu.
    pub fn name(self) -> &'static str {
        match self {
            Info::Shown => "Shown",
            Info::Hover => "On hover",
            Info::Off => "Off",
        }
    }

    /// The state a window has without a setting.
    pub const DEFAULT: Info = Info::Hover;

    /// The state with a name of the settings file. An unknown or missing name gives the default, `Hover`.
    pub fn from_id(id: &str) -> Info {
        Info::ALL.into_iter().find(|i| i.id() == id.trim()).unwrap_or(Info::DEFAULT)
    }
}

/// How the notes of a window show.
pub fn info(ctx: &egui::Context) -> Info {
    ctx.data(|d| d.get_temp::<Info>(egui::Id::NULL)).unwrap_or(Info::DEFAULT)
}

/// Sets how the notes of a window show. The next frame follows it.
pub fn set_info(ctx: &egui::Context, info: Info) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::NULL, info));
}

/// The last thing a note can belong to: the place the pointer rests on, and the letters that get the cue.
#[derive(Clone, Copy)]
pub struct Anchor {
    pass: u64,
    hover: Rect,
    cue: Option<(Rect, Color32)>,
}

/// Names a thing the next `note` belongs to: a title, a row, a checkbox. `hover` is the place of the tooltip.
/// `cue` is the place of its letters and their color.
pub fn anchor(ui: &Ui, hover: Rect, cue: Option<(Rect, Color32)>) {
    if info(ui.ctx()) == Info::Hover {
        let pass = ui.ctx().cumulative_pass_nr();
        ui.data_mut(|d| d.insert_temp(egui::Id::NULL, Anchor { pass, hover, cue }));
    }
}

/// The thing `anchor` named last, to give to `note_at` after other widgets.
pub fn last_anchor(ui: &Ui) -> Option<Anchor> {
    let pass = ui.ctx().cumulative_pass_nr();
    ui.data(|d| d.get_temp::<Anchor>(egui::Id::NULL)).filter(|m| m.pass == pass && info(ui.ctx()) == Info::Hover)
}

/// Makes the tooltip's place of the thing `anchor` named last as big as `hover`: a checkbox with its fields.
pub fn widen(ui: &Ui, hover: Rect) {
    if info(ui.ctx()) == Info::Hover {
        let pass = ui.ctx().cumulative_pass_nr();
        ui.data_mut(|d| {
            if let Some(mut m) = d.get_temp::<Anchor>(egui::Id::NULL).filter(|m| m.pass == pass) {
                m.hover = hover;
                d.insert_temp(egui::Id::NULL, m);
            }
        });
    }
}

/// `anchor` for a row of fields: the tooltip is on the whole row, the cue under the row's name.
pub fn anchor_row(ui: &Ui, row: Rect, name: Rect) {
    anchor(ui, row, Some((name, theme(ui.ctx()).muted)));
}

/// What is in `ui` from the height `top` down to its last widget.
pub fn span(ui: &Ui, top: f32) -> Rect {
    let r = ui.min_rect();
    Rect::from_min_max(egui::pos2(r.min.x, top.min(r.max.y)), r.max)
}

/// A note in the state of the Info texts setting. `place` is the thing it explains. Without one it is the
/// thing `anchor` named last, when that is just over the note.
fn explain(ui: &mut Ui, text: &str, size: Option<f32>, place: Option<(Rect, Option<(Rect, Color32)>)>) {
    let th = theme(ui.ctx());
    match info(ui.ctx()) {
        Info::Off => {}
        Info::Shown => {
            let text = RichText::new(text).color(th.muted);
            ui.label(match size {
                Some(s) => text.size(s),
                None => text,
            });
        }
        Info::Hover => {
            let pass = ui.ctx().cumulative_pass_nr();
            let near = ui.cursor().top();
            let last = ui.data(|d| d.get_temp::<Anchor>(egui::Id::NULL)).filter(|m| m.pass == pass && (near - m.hover.max.y).abs() < 48.0);
            let Some((hover, cue)) = place.or(last.map(|m| (m.hover, m.cue))) else {
                // Nothing to hang the note on: a small sign in its place carries it
                ui.label(RichText::new("?").size(11.0).color(th.muted)).on_hover_text(text);
                return;
            };
            // The area senses the pointer only. Clicks and drags go to the widgets under it.
            let id = ui.scope_id().with(("note", text, hover.min.y as i32));
            ui.interact(hover, id, egui::Sense::hover()).on_hover_text(text);
            if let Some((letters, color)) = cue {
                if ui.is_rect_visible(letters) {
                    let y = (letters.max.y + 0.5).round_to_pixels(ui.pixels_per_point());
                    let dots = egui::Shape::dotted_line(&[egui::pos2(letters.min.x + 1.0, y), egui::pos2(letters.max.x, y)], color.gamma_multiply(0.8), 3.0, 0.6);
                    ui.painter().extend(dots);
                }
            }
        }
    }
}

/// A small note under the thing `anchor` named last: a title bar, a heading, a row, a checkbox.
pub fn note(ui: &mut Ui, text: &str) {
    explain(ui, text, Some(12.0), None);
}

/// A small note under a thing that `last_anchor` gave: a heading with widgets between it and the note.
pub fn note_at(ui: &mut Ui, at: Option<Anchor>, text: &str) {
    explain(ui, text, Some(12.0), at.map(|m| (m.hover, m.cue)));
}

/// A small note about what is in `hover`. The cue goes under `cue`, in the color of muted text.
pub fn note_on(ui: &mut Ui, hover: Rect, cue: Option<Rect>, text: &str) {
    let color = theme(ui.ctx()).muted;
    explain(ui, text, Some(12.0), Some((hover, cue.map(|c| (c, color)))));
}

/// `note_on` in the smaller letters of a source line.
pub fn src_on(ui: &mut Ui, hover: Rect, text: &str) {
    explain(ui, text, Some(11.0), Some((hover, None)));
}

/// A small note about the whole card it is in. The tooltip is on the card as far as it is drawn, the cue under
/// the card's title.
pub fn card_note(ui: &mut Ui, text: &str) {
    let title = ui.data(|d| d.get_temp::<(Rect, Color32)>(ui.scope_id().with("card title")));
    explain(ui, text, Some(12.0), Some((ui.min_rect(), title)));
}

/// A note in the size of body text, under the thing `anchor` named last: the paragraph under a dialog's title.
pub fn para(ui: &mut Ui, text: &str) {
    explain(ui, text, None, None);
}

/// `para` under a thing that `last_anchor` gave.
pub fn para_at(ui: &mut Ui, at: Option<Anchor>, text: &str) {
    explain(ui, text, None, at.map(|m| (m.hover, m.cue)));
}

/// `para` about what is in `hover`.
pub fn para_on(ui: &mut Ui, hover: Rect, cue: Option<Rect>, text: &str) {
    let color = theme(ui.ctx()).muted;
    explain(ui, text, None, Some((hover, cue.map(|c| (c, color)))));
}

/// A tooltip's text with a note added to it, when notes show on hover.
pub fn tip_with_note(ctx: &egui::Context, tip: Option<&str>, note: Option<&str>) -> Option<String> {
    let note = note.filter(|_| info(ctx) == Info::Hover);
    match (note, tip) {
        (Some(n), Some(t)) => Some(format!("{n}\n\n{t}")),
        (n, t) => n.or(t).map(str::to_string),
    }
}

pub fn src(ui: &mut Ui, text: &str) -> Response {
    ui.label(RichText::new(text).size(11.0).color(theme(ui.ctx()).muted))
}

pub fn fieldname(ui: &mut Ui, text: &str) -> Response {
    ui.add(egui::Label::new(RichText::new(text).monospace().size(12.0).color(theme(ui.ctx()).muted)).wrap_mode(egui::TextWrapMode::Extend))
}

pub fn tok(ui: &mut Ui, text: &str) -> Response {
    ui.label(RichText::new(text).monospace().color(theme(ui.ctx()).tok))
}

/// The button of the main action of a dialog or of the toolbar.
pub fn primary(text: &str) -> egui::Button<'static> {
    egui::Button::new(text.to_string()).with_class(CLASS_PRIMARY)
}

/// A button that stands out a little: bold letters.
pub fn accent(text: &str) -> egui::Button<'static> {
    egui::Button::new(text.to_string()).with_class(CLASS_BOLD)
}

/// A toolbar button that stays down while its tool is on.
pub fn toggle(ui: &mut Ui, on: bool, text: &str, tip: &str) -> Response {
    let b = if on { egui::Button::new(text).with_class(CLASS_ON).with_class(CLASS_BOLD).selected(true) } else { egui::Button::new(text) };
    ui.add(b).on_hover_text(tip)
}

/// A tab of a panel's title strip.
pub fn tab(ui: &mut Ui, on: bool, text: &str) -> Response {
    let b = if on { egui::Button::new(text).with_class(CLASS_ON).with_class(CLASS_BOLD).selected(true) } else { egui::Button::new(text) };
    ui.add(b)
}

/// A small button for a card's header.
pub fn mini(ui: &mut Ui, text: &str, tip: &str) -> Response {
    let r = ui.add(egui::Button::new(RichText::new(text).size(11.0)).small());
    if tip.is_empty() {
        r
    } else {
        r.on_hover_text(tip)
    }
}

/// A small button that opens a menu. `label` names it for screen readers and tests when the text alone says too
/// little.
pub fn mini_menu(ui: &mut Ui, text: &str, label: &str, tip: &str, add: impl FnOnce(&mut Ui)) {
    let b = egui::Button::new(RichText::new(text).size(11.0)).small();
    menu_of(ui, b, label, tip, add);
}

/// A small button on a panel's title bar that opens a menu: the bar's letter color, and no frame.
pub fn bar_menu(ui: &mut Ui, text: &str, label: &str, tip: &str, add: impl FnOnce(&mut Ui)) {
    let color = bar_color(theme(ui.ctx()), Rank::Panel);
    let b = egui::Button::new(RichText::new(text).size(12.0).color(color)).small().frame(false);
    menu_of(ui, b, label, tip, add);
}

fn menu_of(ui: &mut Ui, b: egui::Button, label: &str, tip: &str, add: impl FnOnce(&mut Ui)) {
    let r = egui::containers::menu::MenuButton::from_button(b).ui(ui, |ui| {
        egui::ScrollArea::vertical().max_height(420.0).show(ui, add);
    });
    if !label.is_empty() {
        let label = label.to_string();
        r.0.widget_info(move || egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
    }
    if !tip.is_empty() {
        r.0.on_hover_text(tip);
    }
}

/// A checkbox: a box in the theme's shape, filled with a check mark when it is on.
pub fn checkbox(ui: &mut Ui, on: &mut bool, text: impl Into<egui::WidgetText>) -> Response {
    let lit = theme(ui.ctx()).check;
    colored_checkbox(ui, on, text.into(), lit)
}

/// A checkbox whose box takes a color when it is on: a layer of the level list.
pub fn colored_checkbox(ui: &mut Ui, on: &mut bool, text: egui::WidgetText, lit: Color32) -> Response {
    const SIDE: f32 = 14.0;
    let th = theme(ui.ctx());
    let name = text.text().to_string();
    let gap = if name.is_empty() { 0.0 } else { 6.0 };
    // In a column a long text wraps in the room there is. In a row it stays on one line.
    let column = ui.layout().is_vertical() && ui.available_width() > 120.0;
    let (wrap, room) = if column { (egui::TextWrapMode::Wrap, ui.available_width() - SIDE - gap) } else { (egui::TextWrapMode::Extend, f32::INFINITY) };
    let galley = text.into_galley(ui, Some(wrap), room, egui::TextStyle::Body);
    let size = egui::vec2(SIDE + gap + galley.size().x, galley.size().y.max(SIDE).max(ui.spacing().interact_size.y - 2.0));
    let (rect, mut r) = ui.allocate_exact_size(size, egui::Sense::click());
    if r.clicked() {
        *on = !*on;
        r.mark_changed();
    }
    let (checked, enabled) = (*on, ui.is_enabled());
    r.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, enabled, checked, &name));
    if gap > 0.0 {
        let letters = Rect::from_min_size(egui::pos2(rect.min.x + SIDE + gap, rect.center().y - galley.size().y / 2.0), galley.size());
        anchor(ui, rect, Some((letters, th.muted)));
    }
    if ui.is_rect_visible(rect) {
        let square = Rect::from_min_size(egui::pos2(rect.min.x, rect.center().y - SIDE / 2.0), egui::Vec2::splat(SIDE));
        let square = square.round_to_pixels(ui.pixels_per_point());
        let painter = ui.painter();
        if th.shape == Shape::Bevel {
            painter.rect_filled(square, 0.0, if checked { lit } else if r.hovered() { th.bevel[0] } else { th.button.fill });
            bevel(painter, square, th.bevel, !r.is_pointer_button_down_on());
        } else {
            let line = if r.hovered() { th.hover_text } else { th.button.edge };
            let radius = th.radius.min(4) as f32;
            painter.rect_filled(square, radius, if checked { lit } else { th.field_tone.fill });
            painter.rect_stroke(square, radius, Stroke::new(1.0, line), egui::StrokeKind::Inside);
        }
        if checked {
            // The check mark: black on a light box, white on a dark one. A box in the color of the main action takes
            // the color of that button's letters.
            let c = square.center();
            let dark = if lit == th.primary.fill {
                th.primary.text
            } else if lit.r() as u32 + lit.g() as u32 + lit.b() as u32 > 200 {
                Color32::BLACK
            } else {
                Color32::WHITE
            };
            let pts = vec![c + egui::vec2(-3.5, 0.0), c + egui::vec2(-1.0, 2.5), c + egui::vec2(3.5, -3.0)];
            painter.add(egui::Shape::line(pts, Stroke::new(2.0, dark)));
        }
        if r.has_focus() {
            painter.rect_stroke(rect.expand(1.0), 0.0, Stroke::new(1.0, th.focus), egui::StrokeKind::Outside);
        }
        painter.galley(egui::pos2(square.max.x + gap, rect.center().y - galley.size().y / 2.0), galley, if r.hovered() { th.hover_text } else { th.text });
    }
    r
}

/// The color of a check's mark.
pub fn level_color(th: &Theme, level: &str) -> Color32 {
    th.marks[["ok", "err", "warn", "imp"].iter().position(|l| *l == level).unwrap_or(4)]
}

/// The theme's mark for a check of `level`, in the square `at`.
fn mark(painter: &egui::Painter, th: &Theme, level: &str, at: Rect) {
    let color = level_color(th, level);
    let c = at.center();
    let h = at.width() / 2.0;
    let pen = Stroke::new(1.5, color);
    let triangle = vec![egui::pos2(c.x, at.min.y), egui::pos2(at.max.x + 0.5, at.max.y), egui::pos2(at.min.x - 0.5, at.max.y)];
    let diamond = vec![egui::pos2(c.x, at.min.y - 1.0), egui::pos2(at.max.x + 1.0, c.y), egui::pos2(c.x, at.max.y + 1.0), egui::pos2(at.min.x - 1.0, c.y)];
    match th.mark {
        Mark::Block => {
            painter.rect_filled(at, 0.0, color);
            edges(painter, at, Color32::from_white_alpha(170), Color32::from_black_alpha(150));
        }
        Mark::Lamp => {
            painter.circle_filled(c, h + 0.5, color);
            painter.circle_stroke(c, h + 0.5, Stroke::new(1.0, th.border_colors[3]));
            painter.circle_filled(c - egui::vec2(1.5, 1.5), 1.2, Color32::from_white_alpha(150));
        }
        Mark::Drawn => match level {
            "ok" => {
                painter.circle_stroke(c, h, pen);
            }
            "warn" => {
                painter.add(egui::Shape::closed_line(triangle, pen));
            }
            "err" => {
                painter.rect_stroke(at, 0.0, pen, egui::StrokeKind::Middle);
                painter.line_segment([at.left_top(), at.right_bottom()], pen);
                painter.line_segment([at.right_top(), at.left_bottom()], pen);
            }
            "imp" => {
                painter.add(egui::Shape::closed_line(diamond, pen));
            }
            _ => {
                painter.line_segment([at.left_center(), at.right_center()], pen);
            }
        },
        Mark::Printed => match level {
            "ok" => {
                let pts = vec![c + egui::vec2(-4.0, 0.5), c + egui::vec2(-1.0, 3.5), c + egui::vec2(4.5, -4.0)];
                painter.add(egui::Shape::line(pts, Stroke::new(2.0, color)));
            }
            "warn" => {
                painter.add(egui::Shape::convex_polygon(triangle, color, Stroke::NONE));
            }
            "err" => {
                painter.rect_filled(at, 0.0, color);
            }
            "imp" => {
                painter.circle_filled(c, h - 0.5, color);
            }
            _ => {
                painter.line_segment([at.left_center(), at.right_center()], Stroke::new(2.0, color));
            }
        },
        Mark::Code => {
            let code = match level {
                "ok" => "OK",
                "warn" => "!!",
                "err" => "XX",
                "imp" => ">>",
                _ => "--",
            };
            let plate = Rect::from_min_max(egui::pos2(at.min.x - 5.0, at.min.y - 2.0), egui::pos2(at.max.x + 1.0, at.max.y + 2.0));
            painter.rect_filled(plate, 0.0, color);
            painter.text(plate.center(), egui::Align2::CENTER_CENTER, code, FontId::monospace(10.0), th.bg);
        }
    }
}

/// A line of the Checks panel or of a dialog's report: a box with the theme's mark in the level's color. True
/// when it was clicked.
pub fn check_line(ui: &mut Ui, level: &str, text: &str, clickable: bool, link: Option<&(String, String)>, sub: Option<&str>, title: Option<&str>) -> bool {
    let th = theme(ui.ctx());
    let frame = egui::Frame::new().fill(th.panel).stroke(Stroke::new(1.0, th.card_edge)).corner_radius(th.round).inner_margin(Margin { left: 22, right: 8, top: 4, bottom: 4 });
    let out = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 2.0;
        let sense = if clickable { egui::Sense::click() } else { egui::Sense::hover() };
        let mut r = ui.add(egui::Label::new(RichText::new(text).color(th.text)).sense(sense));
        if let Some(sub) = sub.filter(|_| info(ui.ctx()) == Info::Shown) {
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
    if th.ruled != Color32::TRANSPARENT {
        ui.painter().rect_filled(Rect::from_min_max(egui::pos2(rect.min.x, rect.max.y - 1.0), rect.max), 0.0, th.ruled);
    }
    let bullet = Rect::from_min_size(rect.min + egui::vec2(7.0, 8.0), egui::Vec2::splat(9.0)).round_to_pixels(ui.pixels_per_point());
    mark(ui.painter(), th, level, bullet);
    let mut clicked = out.inner;
    // The second line is a note. On hover it joins the line's tooltip.
    let tip = tip_with_note(ui.ctx(), title, sub);
    if clickable {
        let whole = ui.interact(rect, ui.scope_id().with(("chk", rect.min.y as i32, text.len())), egui::Sense::click());
        clicked |= whole.clicked();
        if whole.hovered() {
            ui.painter().rect_stroke(rect, th.round as f32, Stroke::new(1.0, th.hover_text), egui::StrokeKind::Inside);
        }
        // The tooltip is on the area that takes the click: it lies over the box
        let whole = whole.on_hover_cursor(egui::CursorIcon::PointingHand);
        if let Some(t) = &tip {
            whole.on_hover_text(t);
        }
    } else if let Some(t) = tip {
        out.response.on_hover_text(t);
    }
    clicked
}

/// A bar that fills from the left by `ratio`, in the color for "too much" when `over`.
pub fn meter(ui: &mut Ui, ratio: f32, over: bool) {
    let th = theme(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 10.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, th.meter[0]);
    let mut fill = rect.shrink(2.0);
    fill.set_width(fill.width() * ratio.clamp(0.0, 1.0));
    ui.painter().rect_filled(fill, 0.0, th.meter[if over { 2 } else { 1 }]);
    well_edge(ui.painter(), rect);
}

/// The frame of a box that is not a card: the same fill and line.
pub fn boxed(th: &Theme, fill: Color32) -> egui::Frame {
    egui::Frame::new().fill(fill).stroke(Stroke::new(1.0, th.card_edge)).corner_radius(th.round)
}

/// The frame of a chip over the 3D view. An edited level's chip has a line in the color of "edited".
pub fn chip(th: &Theme, edited: bool) -> egui::Frame {
    egui::Frame::new().fill(th.chip.fill).stroke(Stroke::new(1.0, if edited { th.chip_edited } else { th.chip.edge })).corner_radius(th.round)
}

/// The width a row of buttons with these texts takes.
pub fn button_width(ui: &Ui, text: &str, font_size: f32) -> f32 {
    let galley = ui.painter().layout_no_wrap(text.to_string(), FontId::proportional(font_size), Color32::PLACEHOLDER);
    galley.size().x + 2.0 * ui.spacing().button_padding.x + 4.0
}

/// True for a title in plain words. A command's tag, as `FogConfiguration`, keeps its letters: in capitals its
/// words run together.
fn plain_words(title: &str) -> bool {
    let b = title.as_bytes();
    !b.windows(2).any(|w| w[0].is_ascii_lowercase() && w[1].is_ascii_uppercase())
}

/// A titled box: a title bar, then the body. `head` adds the buttons at the bar's right, `body` what is under
/// the bar.
///
/// The title wraps in the room the buttons leave. Their width is the one measured in the frame before.
pub fn card<R>(ui: &mut Ui, title: &str, op: Option<&str>, head: impl FnOnce(&mut Ui), body: impl FnOnce(&mut Ui) -> R) -> R {
    let th = theme(ui.ctx());
    let frame = boxed(th, th.panel).inner_margin(Margin { left: 8, right: 8, top: 0, bottom: 7 });
    frame
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let id = ui.scope_id().with("card head");
            let head_width: f32 = ui.data(|d| d.get_temp(id)).unwrap_or(0.0);
            let back = ui.painter().add(egui::Shape::Noop);
            ui.add_space(3.0);
            let mut letters = Rect::NOTHING;
            let row = ui.horizontal_top(|ui| {
                let title_width = (ui.available_width() - head_width - 6.0).max(40.0);
                ui.allocate_ui_with_layout(egui::vec2(title_width, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width(title_width);
                    letters = if plain_words(title) {
                        bar_label(ui, title, 12.0, true, Rank::Card).rect
                    } else {
                        ui.label(RichText::new(title).font(FontId::new(13.0, bold())).color(th.bar_text)).rect
                    };
                });
                let r = ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if let Some(op) = op {
                        // The code of the command, on the bar when the bar is filled
                        ui.label(RichText::new(op).monospace().color(if th.bar == Bar::Plate { th.bar_text } else { th.text }));
                    }
                    head(ui);
                });
                let width = r.response.rect.width();
                if (width - head_width).abs() > 0.5 {
                    ui.data_mut(|d| d.insert_temp(id, width));
                    ui.ctx().request_repaint();
                }
                r.response.rect
            });
            // The bar goes from edge to edge of the box, behind the title's row. Its line stops before the buttons.
            let (row, buttons) = (row.response.rect, row.inner);
            let right = if buttons.width() > 1.0 { buttons.min.x } else { row.min.x + ui.available_width() + 8.0 };
            let bar = Rect::from_min_max(egui::pos2(row.min.x - 8.0, row.min.y - 3.0), egui::pos2(row.min.x + ui.available_width() + 8.0, row.max.y + 3.0));
            let shape = match th.bar {
                Bar::Dimension | Bar::Inverse => egui::Shape::Vec(vec![bar_shape(th, Rect::from_min_max(bar.min, egui::pos2(right, bar.max.y)), letters, Rank::Card)]),
                _ => bar_shape(th, bar, letters, Rank::Card),
            };
            ui.painter().set(back, shape);
            let cue = (letters, if plain_words(title) { bar_color(th, Rank::Card) } else { th.bar_text });
            ui.data_mut(|d| d.insert_temp(ui.scope_id().with("card title"), cue));
            anchor(ui, Rect::from_min_max(bar.min, egui::pos2(right, bar.max.y)), Some(cue));
            ui.add_space(5.0);
            body(ui)
        })
        .inner
}

/// A one-line text box that hands back its text when the player is done with it (Enter, or the focus leaves) and
/// the text is not `shown` any more. While it has the focus it keeps what was typed.
pub fn commit_text(ui: &mut Ui, id: egui::Id, shown: &str, edit: impl FnOnce(egui::TextEdit) -> egui::TextEdit) -> (Option<String>, Response) {
    commit_number(ui, id, shown, None, edit)
}

/// `commit_text` for a number: with a `spec`, a drag on the box also hands back a text, on every frame the number
/// changes (`numfield`).
pub fn commit_number(ui: &mut Ui, id: egui::Id, shown: &str, spec: Option<Spec>, edit: impl FnOnce(egui::TextEdit) -> egui::TextEdit) -> (Option<String>, Response) {
    let mut buf = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| shown.to_string());
    let (r, named, dragged) = crate::numfield::field(ui, id, &mut buf, spec, edit);
    if let Some(text) = dragged {
        ui.data_mut(|d| d.remove::<String>(id));
        return ((text != shown).then_some(text), named);
    }
    if r.lost_focus() {
        ui.data_mut(|d| d.remove::<String>(id));
        return ((buf != shown).then_some(buf), named);
    }
    if r.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, buf));
    } else {
        ui.data_mut(|d| d.remove::<String>(id));
    }
    (None, named)
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
/// A drag on it changes the number by `spec`.
pub fn num_input(ui: &mut Ui, id: egui::Id, value: f64, width: f32, label: &str, spec: Spec) -> Option<f64> {
    let shown = num_text(value);
    let (text, r) = commit_number(ui, id, &shown, Some(spec), |t| t.desired_width(width));
    name(&r, label);
    let v: f64 = text?.trim().parse().ok()?;
    ((v - value).abs() > 1e-9).then_some(v)
}

/// Gives a widget a name for screen readers and tests.
pub fn name(r: &Response, label: &str) {
    if !label.is_empty() {
        let enabled = r.enabled();
        let label = label.to_string();
        r.widget_info(move || egui::WidgetInfo::labeled(egui::Role::TextInput, enabled, &label));
    }
}

/// The text as a number, or None.
pub fn parse_f64(s: &str) -> Option<f64> {
    glovebox_core::pyfmt::parse_float(s.trim())
}
