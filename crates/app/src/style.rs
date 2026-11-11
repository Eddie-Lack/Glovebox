//! The app's look, and the small widgets the panels share.
//!
//! The look is a 1998 game web site, as the patcher page has it (`web/patcher.html`, after the practice ROM's
//! patch page): navy panels, grey bevelled buttons, white sunken fields, red title bars with yellow capitals, a
//! yellow logo and a rainbow divider. The colours are the constants below. `Look` gives egui's buttons, text
//! boxes and separators their shapes. Bevels, title bars, checkboxes, the logo and the stars are drawn here.

use crate::numfield::Spec;
use egui::class::HasClasses as _;
use egui::emath::GuiRounding as _;
use egui::theme::StyleProvider;
use egui::widget_style::{AtomLayoutStyle, ButtonStyle, SeparatorStyle, StyleArgs, TextEditStyle, TextVisuals, WidgetState};
use egui::{Color32, CornerRadius, FontFamily, FontId, Margin, Rect, Response, RichText, Shadow, Stroke, Ui};

// The palette of the patcher page
pub const NAVY: Color32 = Color32::from_rgb(0x00, 0x00, 0x80);
pub const DEEP: Color32 = Color32::from_rgb(0x00, 0x00, 0x40);
pub const BOX: Color32 = Color32::from_rgb(0x00, 0x00, 0x60);
pub const GREY: Color32 = Color32::from_rgb(0xc0, 0xc0, 0xc0);
pub const GREY_SOFT: Color32 = Color32::from_rgb(0xdf, 0xdf, 0xdf);
pub const GREY_DARK: Color32 = Color32::from_rgb(0x80, 0x80, 0x80);
pub const GREY_DARKER: Color32 = Color32::from_rgb(0x40, 0x40, 0x40);
pub const RED: Color32 = Color32::from_rgb(0xcc, 0x00, 0x00);
pub const RED_LIGHT: Color32 = Color32::from_rgb(0xff, 0x60, 0x60);
pub const RED_DARK: Color32 = Color32::from_rgb(0x80, 0x00, 0x00);
pub const YELLOW: Color32 = Color32::from_rgb(0xff, 0xff, 0x00);
pub const WHITE: Color32 = Color32::WHITE;
pub const BLACK: Color32 = Color32::BLACK;
/// The band under the pointer in a list.
pub const HILITE: Color32 = Color32::from_rgb(0x20, 0x20, 0xd0);

// What the panels use
pub const BG: Color32 = NAVY;
pub const PANEL: Color32 = BOX;
/// Dark wells: the pictures of the palette, the trough of a bar.
pub const INPUT: Color32 = DEEP;
pub const BORDER: Color32 = GREY_DARK;
pub const BORDER_HOVER: Color32 = GREY;
pub const TEXT: Color32 = WHITE;
pub const MUTED: Color32 = Color32::from_rgb(0xc0, 0xc0, 0xff);
pub const DISABLED: Color32 = Color32::from_rgb(0x90, 0x90, 0xc8);
pub const GOLD: Color32 = YELLOW;
pub const GOLD_DIM: Color32 = Color32::from_rgb(0xc0, 0x80, 0x00);
pub const ON_GOLD: Color32 = BLACK;
pub const OK: Color32 = Color32::from_rgb(0x30, 0xe0, 0x30);
pub const ERR: Color32 = Color32::from_rgb(0xff, 0x70, 0x70);
pub const WARN: Color32 = Color32::from_rgb(0xff, 0xc0, 0x00);
pub const IMP: Color32 = Color32::from_rgb(0x66, 0xdd, 0xff);
pub const TOK: Color32 = Color32::from_rgb(0xff, 0x99, 0xff);

/// The bold font's family. It is the text font when the computer has no bold one.
pub fn bold() -> FontFamily {
    FontFamily::Name("bold".into())
}

/// The logo's font family.
pub fn logo_family() -> FontFamily {
    FontFamily::Name("logo".into())
}

/// "#rrggbb" as a colour.
pub fn hex(s: &str) -> Color32 {
    let v = u32::from_str_radix(s.trim_start_matches('#'), 16).unwrap_or(0xffffff);
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// A font file of the computer, when it is there and of a sane size.
fn system_font(names: &[&str]) -> Option<Vec<u8>> {
    let mut dirs: Vec<std::path::PathBuf> = Vec::new();
    if let Some(win) = std::env::var_os("WINDIR") {
        dirs.push(std::path::Path::new(&win).join("Fonts"));
    }
    dirs.extend(["/System/Library/Fonts/Supplemental", "/Library/Fonts", "/usr/share/fonts/truetype/dejavu", "/usr/share/fonts/TTF", "/usr/share/fonts/dejavu"].map(std::path::PathBuf::from));
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

/// The fonts: Verdana or Tahoma when the computer has them (DejaVu Sans on Linux), then the fonts compiled in.
/// Those have the arrows and shapes the buttons show.
fn fonts() -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    let mut rest = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    // The code font has arrows and shapes the text font lacks
    if !rest.iter().any(|f| f == "Hack") {
        rest.push("Hack".to_string());
    }
    let family = |fonts: &mut egui::FontDefinitions, key: &str, files: &[&str], before: &[&str]| -> bool {
        let found = system_font(files);
        let mut list: Vec<String> = Vec::new();
        if let Some(data) = &found {
            fonts.font_data.insert(key.to_string(), std::sync::Arc::new(egui::FontData::from_owned(data.clone())));
            list.push(key.to_string());
        }
        list.extend(before.iter().map(|x| x.to_string()));
        list.extend(rest.iter().cloned());
        let fam = if key == "text" { FontFamily::Proportional } else { FontFamily::Name(key.into()) };
        fonts.families.insert(fam, list);
        found.is_some()
    };
    family(&mut fonts, "text", &["verdana.ttf", "Verdana.ttf", "tahoma.ttf", "Tahoma.ttf", "DejaVuSans.ttf"], &[]);
    let has_bold = family(&mut fonts, "bold", &["verdanab.ttf", "Verdana Bold.ttf", "tahomabd.ttf", "Tahoma Bold.ttf", "DejaVuSans-Bold.ttf"], &[]);
    let after: &[&str] = if has_bold { &["bold"] } else { &[] };
    family(&mut fonts, "logo", &["ariblk.ttf", "Arial Black.ttf"], after);
    fonts
}

/// Sets the look for the whole app.
pub fn apply(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    ctx.set_fonts(fonts());
    ctx.replace_widget_theme::<ButtonStyle>(Look);
    ctx.replace_widget_theme::<TextEditStyle>(Look);
    ctx.replace_widget_theme::<SeparatorStyle>(Look);
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
        s.button_padding = egui::vec2(8.0, 3.0);
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
        v.dark_mode = true;
        v.override_text_color = None;
        v.panel_fill = BG;
        v.window_fill = DEEP;
        v.window_stroke = Stroke::new(2.0, GREY);
        v.window_corner_radius = CornerRadius::ZERO;
        v.menu_corner_radius = CornerRadius::ZERO;
        v.popup_shadow = Shadow { offset: [3, 3], blur: 0, spread: 0, color: Color32::from_black_alpha(160) };
        v.window_shadow = v.popup_shadow;
        v.extreme_bg_color = DEEP;
        v.text_edit_bg_color = Some(WHITE);
        v.faint_bg_color = BOX;
        v.code_bg_color = DEEP;
        v.hyperlink_color = YELLOW;
        v.warn_fg_color = WARN;
        v.error_fg_color = ERR;
        // Selected text in a field: white on navy
        v.selection.bg_fill = NAVY;
        v.selection.stroke = Stroke::new(1.0, WHITE);
        v.text_cursor.stroke = Stroke::new(2.0, BLACK);
        v.disabled_alpha = 0.55;
        // Texts are white on navy in every state. `Look` gives the grey buttons and the white fields their black.
        let w = &mut v.widgets;
        for (state, face) in [(&mut w.noninteractive, NAVY), (&mut w.inactive, GREY), (&mut w.hovered, GREY_SOFT), (&mut w.active, GREY_DARK), (&mut w.open, GREY_SOFT)] {
            state.bg_fill = face;
            state.weak_bg_fill = face;
            state.bg_stroke = Stroke::new(1.0, GREY_DARK);
            state.fg_stroke = Stroke::new(1.0, TEXT);
            state.corner_radius = CornerRadius::ZERO;
            state.expansion = 0.0;
        }
        w.hovered.fg_stroke = Stroke::new(1.0, YELLOW);
        w.active.fg_stroke = Stroke::new(1.0, YELLOW);
    });
}

// ---------------------------------------------------------------------------------------------------------------
// The shapes of egui's own widgets

/// A button that is the main action: red with yellow letters.
pub const CLASS_PRIMARY: &str = "glovebox::primary";
/// A toolbar switch that is on: sunken and yellow.
pub const CLASS_ON: &str = "glovebox::on";
/// A button whose letters are bold.
pub const CLASS_BOLD: &str = "glovebox::bold";

/// A hard edge of one pixel under and right of a box. With a stroke of the other tone it makes a bevel.
fn edge(color: Color32) -> Shadow {
    Shadow { offset: [1, 1], blur: 0, spread: 0, color }
}

/// The theme: grey bevelled buttons, flat list entries, white sunken text boxes.
#[derive(Debug, Clone)]
pub struct Look;

impl StyleProvider<ButtonStyle> for Look {
    fn style(&mut self, args: &StyleArgs<'_>) -> ButtonStyle {
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
                text_style: TextVisuals { font_id: style.override_font_id.clone().unwrap_or(FontId::new(13.0, family)), color },
                image_tint: WHITE,
                ..Default::default()
            },
        };
        let lit = matches!(state, WidgetState::Hovered | WidgetState::Active);
        let selected = has(&egui::Button::CLASS_SELECTED);
        let framed = has(&egui::Button::CLASS_FRAME) || (!has(&egui::Button::CLASS_NO_FRAME) && style.visuals.button_frame);
        if !framed {
            return body(egui::Frame::new(), if lit { YELLOW } else { TEXT }, FontFamily::Proportional);
        }
        // An entry of a menu or of a list: flat, white on navy. The chosen one is navy on white.
        let in_menu = style.visuals.widgets.inactive.weak_bg_fill == Color32::TRANSPARENT;
        let entry = in_menu || selected || has(&egui::Button::CLASS_HIDE_FRAME_WHEN_INACTIVE);
        if entry && !has(CLASS_ON) && !has(CLASS_PRIMARY) {
            let (fill, color) = match (selected, lit) {
                (true, _) => (WHITE, NAVY),
                (false, true) => (HILITE, WHITE),
                (false, false) => (Color32::TRANSPARENT, TEXT),
            };
            let frame = egui::Frame { fill, inner_margin: margin, ..Default::default() };
            return body(frame, color, if selected { bold() } else { FontFamily::Proportional });
        }
        // A push button: a grey bevel, sunken while it is held or switched on
        let held = *state == WidgetState::Active && ctx.input(|i| i.pointer.any_down());
        let focused = *state == WidgetState::Active && !held;
        let hovered = *state == WidgetState::Hovered;
        let (face, color) = if has(CLASS_PRIMARY) {
            (if hovered { Color32::from_rgb(0xe8, 0x10, 0x10) } else { RED }, YELLOW)
        } else if has(CLASS_ON) {
            (if hovered { Color32::from_rgb(0xff, 0xff, 0x90) } else { YELLOW }, BLACK)
        } else if has(&egui::ComboBox::CLASS_OPEN) {
            (GREY_SOFT, BLACK)
        } else {
            (if hovered { GREY_SOFT } else { GREY }, BLACK)
        };
        let sunken = held || has(CLASS_ON);
        let (light, dark) = if has(CLASS_PRIMARY) { (RED_LIGHT, BLACK) } else { (WHITE, GREY_DARKER) };
        let (stroke, shadow) = match (sunken, focused) {
            (true, _) => (dark, edge(light)),
            (false, true) => (BLACK, edge(dark)),
            (false, false) => (light, edge(dark)),
        };
        let frame = egui::Frame { fill: face, inner_margin: margin, shadow, ..Default::default() }.apply_stroke_and_expansion_without_layout_shift(Stroke::new(1.0, stroke), 0.0);
        let family = if has(CLASS_PRIMARY) || has(CLASS_BOLD) { bold() } else { FontFamily::Proportional };
        body(frame, color, family)
    }
}

impl StyleProvider<TextEditStyle> for Look {
    fn style(&mut self, args: &StyleArgs<'_>) -> TextEditStyle {
        let StyleArgs { classes, style, state, .. } = args;
        let read_only = classes.has_class(&egui::TextEdit::CLASS_READ_ONLY);
        // A sunken white box with black text. With the keyboard in it, the edge is yellow.
        let frame = if read_only {
            egui::Frame { inner_margin: style.spacing.button_padding.into(), ..Default::default() }
        } else {
            let stroke = if *state == WidgetState::Active { Stroke::new(2.0, YELLOW) } else { Stroke::new(1.0, GREY_DARKER) };
            egui::Frame { fill: WHITE, inner_margin: Margin::symmetric(5, 3), shadow: edge(WHITE), ..Default::default() }.apply_stroke_and_expansion_without_layout_shift(stroke, 0.0)
        };
        TextEditStyle {
            atom_layout: AtomLayoutStyle {
                frame,
                min_size: egui::vec2(0.0, style.spacing.interact_size.y),
                gap: style.spacing.icon_spacing,
                text_style: TextVisuals::new(style, egui::TextStyle::Body, if read_only { TEXT } else { BLACK }),
                align2: Some(egui::Align2::LEFT_TOP),
                ..Default::default()
            },
            hint_text_color: Color32::from_rgb(0x60, 0x60, 0x60),
            prefix_suffix_color: BLACK,
        }
    }
}

impl StyleProvider<SeparatorStyle> for Look {
    fn style(&mut self, _: &StyleArgs<'_>) -> SeparatorStyle {
        SeparatorStyle { spacing: 6.0, stroke: Stroke::new(1.0, GREY_DARK) }
    }
}

/// The arrow of a combo box: black on its grey button.
pub fn combo_icon(ui: &Ui, rect: Rect, _: &egui::style::WidgetVisuals, _: bool) {
    let rect = Rect::from_center_size(rect.center(), egui::vec2(rect.width() * 0.7, rect.height() * 0.45));
    ui.painter().add(egui::Shape::rotated_triangle(rect, 0.0, BLACK));
}

// ---------------------------------------------------------------------------------------------------------------
// Bevels, bars, the logo and the stars

/// A two-tone edge inside `rect`, two pixels wide: raised like a button, or sunken like a field.
pub fn bevel(painter: &egui::Painter, rect: Rect, raised: bool) {
    let tones = if raised { [WHITE, GREY_SOFT, GREY_DARK, GREY_DARKER] } else { [GREY_DARK, GREY_DARKER, GREY_SOFT, WHITE] };
    edges(painter, rect, tones[0], tones[3]);
    edges(painter, rect.shrink(1.0), tones[1], tones[2]);
}

/// One line along the top and left of `rect` and one along its bottom and right.
fn edges(painter: &egui::Painter, r: Rect, top_left: Color32, bottom_right: Color32) {
    let line = |a: egui::Pos2, b: egui::Pos2, c: Color32| painter.rect_filled(Rect::from_min_max(a, b), 0.0, c);
    line(r.min, egui::pos2(r.max.x, r.min.y + 1.0), top_left);
    line(r.min, egui::pos2(r.min.x + 1.0, r.max.y), top_left);
    line(egui::pos2(r.min.x, r.max.y - 1.0), r.max, bottom_right);
    line(egui::pos2(r.max.x - 1.0, r.min.y), r.max, bottom_right);
}

/// The ridge frame of the pages' table, inside `rect`: `RIDGE` points wide.
pub fn ridge(painter: &egui::Painter, rect: Rect) {
    painter.rect_stroke(rect, 0.0, Stroke::new(RIDGE, GREY), egui::StrokeKind::Inside);
    edges(painter, rect, WHITE, GREY_DARKER);
    edges(painter, rect.shrink(1.0), GREY_SOFT, GREY_DARK);
    edges(painter, rect.shrink(RIDGE - 2.0), GREY_DARK, GREY_SOFT);
    edges(painter, rect.shrink(RIDGE - 1.0), GREY_DARKER, WHITE);
}

pub const RIDGE: f32 = 6.0;

/// A box in the ridge frame, navy inside: the table of the pages. The setup screen and the dialogs use it.
pub fn framed<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let out = egui::Frame::new().fill(NAVY).inner_margin(Margin::same(RIDGE as i8)).show(ui, add);
    ridge(ui.painter(), out.response.rect);
    out.inner
}

/// The rainbow divider, as wide as the room.
pub fn rainbow(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 5.0), egui::Sense::hover());
    let stops = [0xff0000, 0xff8000, 0xffff00, 0x00c000, 0x0080ff, 0x8000ff, 0xff00ffu32].map(|v| Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8));
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
    ui.painter().add(mesh);
}

/// The name in the logo's letters: yellow and slanted, with a red and a black hard shadow.
pub fn logo(ui: &mut Ui, text: &str, size: f32) -> Response {
    let job = |color: Color32| {
        let format = egui::TextFormat { font_id: FontId::new(size, logo_family()), color, italics: true, extra_letter_spacing: (size / 22.0).round(), ..Default::default() };
        egui::text::LayoutJob::single_section(text.to_string(), format)
    };
    let step = (size / 14.0).round().max(1.0);
    let front = ui.fonts_mut(|f| f.layout_job(job(YELLOW)));
    let (rect, r) = ui.allocate_exact_size(front.size() + egui::vec2(2.0 * step + 2.0, 2.0 * step), egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        for (k, color) in [(2.0, BLACK), (1.0, RED)] {
            let g = ui.fonts_mut(|f| f.layout_job(job(color)));
            ui.painter().galley(rect.min + egui::vec2(k * step, k * step), g, color);
        }
        ui.painter().galley(rect.min, front, YELLOW);
    }
    let label = text.to_string();
    r.widget_info(move || egui::WidgetInfo::labeled(egui::Role::Label, true, &label));
    r
}

/// The star background of the pages, tiled over `rect`.
pub fn stars(painter: &egui::Painter, rect: Rect) {
    painter.rect_filled(rect, 0.0, Color32::from_rgb(0x00, 0x00, 0xa8));
    const TILE: f32 = 72.0;
    // The stars of the page's tile: the corners of each, from its SVG path
    let big: [(f32, f32); 10] = [(18.0, 6.0), (21.0, 13.0), (28.0, 13.0), (22.0, 17.0), (24.0, 25.0), (18.0, 20.0), (12.0, 25.0), (14.0, 17.0), (8.0, 13.0), (15.0, 13.0)];
    let small: [(f32, f32); 10] = [(54.0, 42.0), (56.0, 47.0), (61.0, 47.0), (57.0, 50.0), (59.0, 56.0), (54.0, 52.0), (49.0, 56.0), (51.0, 50.0), (47.0, 47.0), (52.0, 47.0)];
    let dots = [(44.0, 12.0, WHITE), (10.0, 52.0, WHITE), (64.0, 26.0, Color32::from_rgb(0xff, 0xff, 0x80))];
    let mut mesh = egui::Mesh::default();
    let star = |mesh: &mut egui::Mesh, at: egui::Vec2, pts: &[(f32, f32); 10], color: Color32| {
        let n = pts.len() as f32;
        let centre = egui::pos2(pts.iter().map(|p| p.0).sum::<f32>() / n, pts.iter().map(|p| p.1).sum::<f32>() / n);
        let base = mesh.vertices.len() as u32;
        mesh.colored_vertex(centre + at, color);
        for p in pts {
            mesh.colored_vertex(egui::pos2(p.0, p.1) + at, color);
        }
        for k in 0..10u32 {
            mesh.add_triangle(base, base + 1 + k, base + 1 + (k + 1) % 10);
        }
    };
    let faint = Color32::from_rgb(0x8c, 0x8c, 0xd8);
    let mut y = rect.min.y;
    while y < rect.max.y {
        let mut x = rect.min.x;
        while x < rect.max.x {
            let at = egui::vec2(x, y);
            star(&mut mesh, at, &big, YELLOW);
            star(&mut mesh, at, &small, faint);
            for (dx, dy, c) in dots {
                mesh.add_colored_rect(Rect::from_min_size(egui::pos2(x + dx, y + dy), egui::vec2(2.0, 2.0)), c);
            }
            x += TILE;
        }
        y += TILE;
    }
    painter.with_clip_rect(rect).add(mesh);
}

/// A text for a title bar: bold yellow capitals, a little apart.
fn bar_text(text: &str, size: f32) -> RichText {
    RichText::new(text.to_uppercase()).font(FontId::new(size, bold())).color(YELLOW).extra_letter_spacing(1.0)
}

/// The red of a title bar on `rect`, with its light top line and dark bottom line.
fn bar_shape(rect: Rect) -> egui::Shape {
    let top = Rect::from_min_max(rect.min, egui::pos2(rect.max.x, rect.min.y + 1.0));
    let bottom = Rect::from_min_max(egui::pos2(rect.min.x, rect.max.y - 1.0), rect.max);
    egui::Shape::Vec(vec![egui::Shape::rect_filled(rect, 0.0, RED), egui::Shape::rect_filled(top, 0.0, RED_LIGHT), egui::Shape::rect_filled(bottom, 0.0, RED_DARK)])
}

/// Shows `text` in a title bar's letters. Screen readers and tests get the text as it is written.
fn bar_label(ui: &mut Ui, text: &str, size: f32, wrap: bool) -> Response {
    let label = egui::Label::new(bar_text(text, size));
    let r = ui.add(if wrap { label.wrap() } else { label.truncate() });
    let name = text.to_string();
    r.widget_info(move || egui::WidgetInfo::labeled(egui::Role::Label, true, &name));
    r
}

/// A red title bar over the whole width: the title of a panel or of a dialog.
pub fn title_bar(ui: &mut Ui, title: &str) -> Response {
    let out = egui::Frame::new().inner_margin(Margin { left: 8, right: 8, top: 3, bottom: 3 }).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let back = ui.painter().add(egui::Shape::Noop);
        let r = bar_label(ui, title, 12.0, false);
        (back, r)
    });
    let (back, r) = out.inner;
    ui.painter().set(back, bar_shape(out.response.rect));
    r
}

pub fn h2(ui: &mut Ui, text: &str) -> Response {
    ui.label(RichText::new(text).font(FontId::new(15.0, bold())).color(TEXT))
}

pub fn h3(ui: &mut Ui, text: &str) -> Response {
    ui.label(RichText::new(text).font(FontId::new(13.0, bold())).color(TEXT))
}

/// A small heading in capitals, on a red bar as wide as the room.
pub fn h4(ui: &mut Ui, text: &str) -> Response {
    let out = egui::Frame::new().inner_margin(Margin { left: 6, right: 6, top: 2, bottom: 2 }).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let back = ui.painter().add(egui::Shape::Noop);
        let r = bar_label(ui, text, 11.0, false);
        (back, r)
    });
    let (back, r) = out.inner;
    ui.painter().set(back, bar_shape(out.response.rect));
    r
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

/// The button of the main action of a dialog or of the toolbar: red, with bold yellow letters.
pub fn primary(text: &str) -> egui::Button<'static> {
    egui::Button::new(text.to_string()).with_class(CLASS_PRIMARY)
}

/// A button that stands out a little: bold letters.
pub fn accent(text: &str) -> egui::Button<'static> {
    egui::Button::new(text.to_string()).with_class(CLASS_BOLD)
}

/// A toolbar button that stays down while its tool is on: sunken and yellow.
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

/// A checkbox: a grey bevelled box, green with a tick when it is on.
pub fn checkbox(ui: &mut Ui, on: &mut bool, text: impl Into<egui::WidgetText>) -> Response {
    tick_box(ui, on, text.into(), Color32::from_rgb(0x00, 0xc0, 0x00))
}

/// A checkbox whose box takes a colour when it is on: a layer of the level list.
pub fn tick_box(ui: &mut Ui, on: &mut bool, text: egui::WidgetText, lit: Color32) -> Response {
    const SIDE: f32 = 14.0;
    let name = text.text().to_string();
    let gap = if name.is_empty() { 0.0 } else { 6.0 };
    let galley = text.into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Body);
    let size = egui::vec2(SIDE + gap + galley.size().x, galley.size().y.max(SIDE).max(ui.spacing().interact_size.y - 2.0));
    let (rect, mut r) = ui.allocate_exact_size(size, egui::Sense::click());
    if r.clicked() {
        *on = !*on;
        r.mark_changed();
    }
    let (checked, enabled) = (*on, ui.is_enabled());
    r.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, enabled, checked, &name));
    if ui.is_rect_visible(rect) {
        let square = Rect::from_min_size(egui::pos2(rect.min.x, rect.center().y - SIDE / 2.0), egui::Vec2::splat(SIDE));
        let square = square.round_to_pixels(ui.pixels_per_point());
        let painter = ui.painter();
        painter.rect_filled(square, 0.0, if checked { lit } else if r.hovered() { WHITE } else { GREY });
        bevel(painter, square, !r.is_pointer_button_down_on());
        if checked {
            // The tick, dark on the lit box
            let c = square.center();
            let dark = if lit.r() as u32 + lit.g() as u32 + lit.b() as u32 > 200 { BLACK } else { WHITE };
            let pts = vec![c + egui::vec2(-3.5, 0.0), c + egui::vec2(-1.0, 2.5), c + egui::vec2(3.5, -3.0)];
            painter.add(egui::Shape::line(pts, Stroke::new(2.0, dark)));
        }
        if r.has_focus() {
            painter.rect_stroke(rect.expand(1.0), 0.0, Stroke::new(1.0, YELLOW), egui::StrokeKind::Outside);
        }
        let color = if r.hovered() { YELLOW } else { TEXT };
        painter.galley(egui::pos2(square.max.x + gap, rect.center().y - galley.size().y / 2.0), galley, color);
    }
    r
}

pub fn level_color(level: &str) -> Color32 {
    match level {
        "ok" => OK,
        "err" => ERR,
        "warn" => WARN,
        "imp" => IMP,
        _ => GREY,
    }
}

/// A line of the Checks panel or of a dialog's report: a box with a pixel bullet in the level's colour. True when
/// it was clicked.
pub fn check_line(ui: &mut Ui, level: &str, text: &str, clickable: bool, link: Option<&(String, String)>, sub: Option<&str>, title: Option<&str>) -> bool {
    let frame = egui::Frame::new().fill(PANEL).stroke(Stroke::new(1.0, BLACK)).inner_margin(Margin { left: 22, right: 8, top: 4, bottom: 4 });
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
    // The bullet: a little bevelled block, as the pages have before each line
    let bullet = Rect::from_min_size(rect.min + egui::vec2(7.0, 8.0), egui::Vec2::splat(9.0)).round_to_pixels(ui.pixels_per_point());
    ui.painter().rect_filled(bullet, 0.0, level_color(level));
    edges(ui.painter(), bullet, Color32::from_white_alpha(170), Color32::from_black_alpha(150));
    let mut clicked = out.inner;
    if clickable {
        let whole = ui.interact(rect, ui.scope_id().with(("chk", rect.min.y as i32, text.len())), egui::Sense::click());
        clicked |= whole.clicked();
        if whole.hovered() {
            ui.painter().rect_stroke(rect, 0.0, Stroke::new(1.0, YELLOW), egui::StrokeKind::Inside);
        }
        whole.on_hover_cursor(egui::CursorIcon::PointingHand);
    }
    if let Some(t) = title {
        out.response.on_hover_text(t);
    }
    clicked
}

/// The width a row of buttons with these texts takes.
pub fn button_width(ui: &Ui, text: &str, font_size: f32) -> f32 {
    let galley = ui.painter().layout_no_wrap(text.to_string(), FontId::proportional(font_size), TEXT);
    galley.size().x + 2.0 * ui.spacing().button_padding.x + 4.0
}

/// True for a title in plain words. A command's tag, as `FogConfiguration`, keeps its letters: in capitals its
/// words run together.
fn plain_words(title: &str) -> bool {
    let b = title.as_bytes();
    !b.windows(2).any(|w| w[0].is_ascii_lowercase() && w[1].is_ascii_uppercase())
}

/// A titled box: a red title bar, then the body. `head` adds the buttons at the bar's right, `body` what is under
/// the bar.
///
/// The title wraps in the room the buttons leave. Their width is the one measured in the frame before.
pub fn card<R>(ui: &mut Ui, title: &str, op: Option<&str>, head: impl FnOnce(&mut Ui), body: impl FnOnce(&mut Ui) -> R) -> R {
    let frame = egui::Frame::new().fill(PANEL).stroke(Stroke::new(1.0, BLACK)).inner_margin(Margin { left: 8, right: 8, top: 0, bottom: 7 });
    frame
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let id = ui.scope_id().with("card head");
            let head_width: f32 = ui.data(|d| d.get_temp(id)).unwrap_or(0.0);
            let back = ui.painter().add(egui::Shape::Noop);
            ui.add_space(3.0);
            let row = ui.horizontal_top(|ui| {
                let title_width = (ui.available_width() - head_width - 6.0).max(40.0);
                ui.allocate_ui_with_layout(egui::vec2(title_width, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width(title_width);
                    if plain_words(title) {
                        bar_label(ui, title, 12.0, true);
                    } else {
                        ui.label(RichText::new(title).font(FontId::new(13.0, bold())).color(YELLOW));
                    }
                });
                let r = ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if let Some(op) = op {
                        ui.label(RichText::new(op).monospace().color(WHITE));
                    }
                    head(ui);
                });
                let width = r.response.rect.width();
                if (width - head_width).abs() > 0.5 {
                    ui.data_mut(|d| d.insert_temp(id, width));
                    ui.ctx().request_repaint();
                }
            });
            // The bar goes from edge to edge of the box, behind the title's row
            let row = row.response.rect;
            let bar = Rect::from_min_max(egui::pos2(row.min.x - 8.0, row.min.y - 3.0), egui::pos2(row.min.x + ui.available_width() + 8.0, row.max.y + 3.0));
            ui.painter().set(back, bar_shape(bar));
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

/// Python's float(): a number, or None.
pub fn parse_f64(s: &str) -> Option<f64> {
    glovebox_core::pyfmt::parse_float(s.trim())
}
