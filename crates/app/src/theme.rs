//! The themes: each look of the app as data.
//!
//! A `Theme` holds the colors, the shapes of the controls, the fonts and the small details of one look. `style.rs`
//! draws everything from the active theme. No other file holds a color of the window.
//!
//! Each theme follows one model from the world. "1998" is the patcher page (`web/patcher.html`): a game web site
//! of that year. "Cartridge" is a gray console and its controller. "Blueprint" is a drafting sheet. "Paper" is a
//! printed manual in two inks. "CRT" is a debug monitor with green phosphor. "Light", "Dark" and "High contrast" are
//! the plain looks of a desktop tool: they follow the system's own windows and have no decoration.
//!
//! The palette themes ("Sage", "Sand", "Ocean", "Lavender", "Coral", "Slate", "Plum") follow a phone app: flat
//! surfaces told apart by tone, round corners, hairlines, and five colors each (a ground, a card, a main color,
//! a second color and the text). Every other color of such a theme is a mix of those five.
//!
//! To add a theme: write a `Theme` here, add it to `THEMES`, and give `style.rs` a drawing for any new variant of
//! the enums below. docs/technical.md, "Themes", lists what a theme has to keep.

use egui::Color32;

/// A color from its hex number.
const fn c(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// A see-through color: the hex number and the alpha.
const fn ca(v: u32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied_const((v >> 16) as u8, (v >> 8) as u8, v as u8, a)
}

const NONE: Color32 = Color32::TRANSPARENT;

/// A surface with a text on it and a line around it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tone {
    pub fill: Color32,
    pub text: Color32,
    pub edge: Color32,
}

const fn tone(fill: u32, text: u32, edge: u32) -> Tone {
    Tone { fill: c(fill), text: c(text), edge: c(edge) }
}

/// How a push button is built.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    /// A two-tone bevel, sunken while held or on.
    Bevel,
    /// A molded key: a line around it and a thick lip under it. The lip goes while it is held or on.
    Key,
    /// A hairline around a flat face.
    Outline,
}

/// How a text box is built.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Field {
    /// A sunken box.
    Sunken,
    /// A line around the box.
    Boxed,
    /// A tint with a line under it, as a form on paper.
    Underlined,
}

/// How a title bar is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Bar {
    /// A filled bar with a light line over it and a dark line under it.
    Solid,
    /// A quiet header: a tinted strip. A panel's has a hairline under it, a card's has the card's round top
    /// corners. A small heading has no fill and a hairline under it.
    Soft,
    /// A dark label plate. A panel's plate has a colored end.
    Plate,
    /// No fill. A dimension line with ticks and arrowheads runs from the title to the right edge.
    Dimension,
    /// Printed rules: a heavy rule over a panel's or a card's title, a hairline under a small heading.
    Rule,
    /// Inverse video: a panel's title on a filled line, a card's title on a filled block with a line after it.
    Inverse,
}

/// How the name of the app is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Logo {
    /// Slanted letters with two hard shadows.
    WordArt,
    /// Capitals on a dark sticker with a colored stripe under them.
    Label,
    /// Spaced capitals in a title block with corner marks.
    TitleBlock,
    /// The name with a colored rule under it, as the head of a manual.
    Masthead,
    /// A prompt, the name in capitals and a block cursor.
    Prompt,
    /// The name in bold letters, and nothing else.
    Plain,
}

/// The line under a dialog's title and under the setup screen's logo.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Divider {
    Rainbow,
    /// Four blocks in the four mark colors.
    Stripes,
    /// A ruler: a line with short and long ticks.
    Scale,
    /// A heavy rule and a hairline.
    DoubleRule,
    /// Two hairlines.
    DoubleLine,
    /// One hairline.
    Line,
}

/// What is behind the setup screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Backdrop {
    Stars,
    /// Ribbed plastic: pairs of a dark and a light line.
    Ribs,
    /// A sheet with a fine and a coarse grid.
    Grid,
    /// Ruled notebook paper with a margin line.
    Ruled,
    /// The dark lines of a picture tube.
    Scanlines,
    /// One flat color.
    Plain,
}

/// The frame of the setup screen's box, of a dialog and of the note in the view. Every frame is `style::RIDGE`
/// points wide.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Border {
    /// The ridge frame of a 1998 page's table.
    Ridge,
    /// A dark band with a light line inside it.
    Band,
    /// Lines, each as (distance from the outer edge, width).
    Lines(&'static [(f32, f32)]),
}

/// The mark before a line of the Checks panel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mark {
    /// A small beveled block.
    Block,
    /// A round lamp.
    Lamp,
    /// A drawn sign for each level: circle, triangle, crossed square, diamond.
    Drawn,
    /// A printed sign for each level: check mark, triangle, square, dot.
    Printed,
    /// Two letters in inverse video: OK, !!, XX.
    Code,
}

/// A font file to look for, by its file name.
pub type Files = &'static [&'static str];

pub struct Fonts {
    /// The text font. Empty with `mono` uses the code font compiled in.
    pub text: Files,
    /// The bold font. Empty uses the text font.
    pub bold: Files,
    /// The font of title bars. Empty uses the bold font.
    pub label: Files,
    /// The font of the logo. Empty uses the label font.
    pub logo: Files,
    /// The code font compiled in comes first for every text.
    pub mono: bool,
    /// The size of body and button text.
    pub body: f32,
    /// The space between the capitals of a title bar.
    pub label_spacing: f32,
}

pub struct Theme {
    /// The name in the settings file.
    pub id: &'static str,
    /// The name in the Theme list.
    pub name: &'static str,
    pub dark: bool,
    /// How much of a control that cannot be used is left: 1 is all of it.
    pub disabled: f32,

    // Surfaces
    /// The panels.
    pub bg: Color32,
    /// Cards and the lines of the Checks panel.
    pub panel: Color32,
    /// The toolbar, the status bar and the tab strip.
    pub deep: Color32,
    /// Wells: the pictures of the palette.
    pub well: Color32,
    /// The line around a card. `NONE` for no line.
    pub card_edge: Color32,
    /// A hairline under each line of the Checks panel, for a theme without boxes.
    pub ruled: Color32,
    /// Separators and the edges of panels.
    pub line: Color32,
    /// A fine grid on the panels: the fine and the coarse line.
    pub grid: Option<(Color32, Color32)>,
    /// Pop-ups, menus and tooltips, and their edge.
    pub popup: Color32,
    pub popup_edge: (f32, Color32),
    pub popup_shadow: Color32,
    /// What dims the window behind a dialog.
    pub scrim: Color32,

    // Text
    pub text: Color32,
    pub muted: Color32,
    /// A text or a label under the pointer, and links.
    pub hover_text: Color32,
    pub link: Color32,
    /// The keyboard focus.
    pub focus: Color32,
    /// The keyboard focus fills the button, with this text color.
    pub focus_block: Option<Color32>,
    /// Good, bad, warning, important and token texts.
    pub ok: Color32,
    pub err: Color32,
    pub warn: Color32,
    pub imp: Color32,
    pub tok: Color32,

    // Controls
    pub shape: Shape,
    /// The corner radius of buttons and fields.
    pub radius: u8,
    /// The corner radius of cards, pop-ups, dialogs and chips.
    pub round: u8,
    pub button: Tone,
    pub button_hover: Tone,
    pub button_down: Tone,
    /// A toolbar switch or a tab that is on.
    pub on: Tone,
    pub on_hover: Tone,
    /// The main action.
    pub primary: Tone,
    pub primary_hover: Tone,
    /// The four tones of a bevel, from the lightest: wells, checkboxes and beveled buttons.
    pub bevel: [Color32; 4],
    pub button_padding: [f32; 2],
    pub field: Field,
    pub field_tone: Tone,
    pub field_hint: Color32,
    /// Typed text is in the code font.
    pub field_mono: bool,
    /// A checkbox that is on, when it has no color of its own.
    pub check: Color32,
    /// The chosen row of a list and the row under the pointer.
    pub sel: Tone,
    pub hover: Tone,
    /// Selected text in a field: the band and the letters.
    pub text_sel: (Color32, Color32),
    /// The handle of a scroll bar: idle, under the pointer, dragged. Then the trough.
    pub scroll: [Color32; 3],
    pub trough: Color32,
    /// The memory bar of the Checks panel: the trough, the fill, the fill when over.
    pub meter: [Color32; 3],

    // Details
    pub bar: Bar,
    /// A bar's fill, its light and dark lines, its text, and the text of a small heading.
    pub bar_fill: Color32,
    pub bar_lines: (Color32, Color32),
    pub bar_text: Color32,
    pub bar_text_small: Color32,
    pub logo: Logo,
    /// The logo's colors: the letters, then two more.
    pub logo_colors: [Color32; 3],
    pub divider: Divider,
    pub backdrop: Backdrop,
    /// The backdrop's ground and its two line colors.
    pub backdrop_colors: [Color32; 3],
    /// The band behind the setup screen's logo.
    pub banner: Color32,
    pub border: Border,
    /// The frame's colors, from the lightest.
    pub border_colors: [Color32; 4],
    pub mark: Mark,
    /// The marks of ok, bad, warning, important and anything else.
    pub marks: [Color32; 5],
    /// The chips over the 3D view: the fill, the edge, the text, the level's name and the color of "edited".
    pub chip: Tone,
    pub chip_title: Color32,
    pub chip_edited: Color32,
    /// The box of a box select: its line and its fill.
    pub select_box: (Color32, Color32),
    /// The 3D view of a level without fog (sRGB, 0 to 1).
    pub view_bg: [f64; 3],
    pub fonts: Fonts,
}

impl Theme {
    pub fn view_color(&self) -> Color32 {
        let b = |v: f64| (v * 255.0).round().clamp(0.0, 255.0) as u8;
        Color32::from_rgb(b(self.view_bg[0]), b(self.view_bg[1]), b(self.view_bg[2]))
    }
}

/// The 3D view's background for a level without fog, as the editor's state gives it.
pub const VIEW_BG: [f64; 3] = [0.051, 0.055, 0.071];

/// The colors of the rainbow divider, from left to right.
pub const RAINBOW: [Color32; 7] = [c(0xff0000), c(0xff8000), c(0xffff00), c(0x00c000), c(0x0080ff), c(0x8000ff), c(0xff00ff)];

/// The themes. The first is the default. The plain ones come after it, then the palette themes, then the styled
/// ones.
pub static THEMES: [&Theme; 15] = [&Y1998, &LIGHT, &DARK, &CONTRAST, &SAGE, &SAND, &OCEAN, &LAVENDER, &CORAL, &SLATE, &PLUM, &CARTRIDGE, &BLUEPRINT, &PAPER, &CRT];

/// The palette themes: the Theme list shows them in a column of their own.
pub static PALETTES: [&Theme; 7] = [&SAGE, &SAND, &OCEAN, &LAVENDER, &CORAL, &SLATE, &PLUM];

/// The font files of the plain themes: the system's sans face.
const SANS: Files = &["segoeui.ttf", "SegoeUI.ttf", "tahoma.ttf", "Arial.ttf", "arial.ttf", "LiberationSans-Regular.ttf", "DejaVuSans.ttf"];
const SANS_BOLD: Files = &["segoeuib.ttf", "SegoeUI-Bold.ttf", "tahomabd.ttf", "Arial Bold.ttf", "arialbd.ttf", "LiberationSans-Bold.ttf", "DejaVuSans-Bold.ttf"];
const SANS_LABEL: Files = &["seguisb.ttf", "segoeuib.ttf", "SegoeUI-Bold.ttf", "tahomabd.ttf", "Arial Bold.ttf", "arialbd.ttf", "LiberationSans-Bold.ttf", "DejaVuSans-Bold.ttf"];
const SANS_FONTS: Fonts = Fonts { text: SANS, bold: SANS_BOLD, label: SANS_LABEL, logo: &[], mono: false, body: 13.0, label_spacing: 0.4 };

/// The theme with an id. An unknown id gives the default.
pub fn by_id(id: &str) -> &'static Theme {
    THEMES.iter().copied().find(|t| t.id == id.trim()).unwrap_or(THEMES[0])
}

// ---------------------------------------------------------------------------------------------------------------
// 1998: the patcher page. Navy panels, gray beveled buttons, sunken white fields, red title bars with yellow
// capitals, a slanted yellow logo, a rainbow divider and stars.

pub static Y1998: Theme = Theme {
    id: "1998",
    name: "1998",
    dark: true,
    disabled: 0.55,
    bg: c(0x000080),
    panel: c(0x000060),
    deep: c(0x000040),
    well: c(0x000040),
    card_edge: c(0x000000),
    ruled: NONE,
    line: c(0x808080),
    grid: None,
    popup: c(0x000040),
    popup_edge: (2.0, c(0xc0c0c0)),
    popup_shadow: ca(0x000000, 160),
    scrim: ca(0x000030, 190),
    text: c(0xffffff),
    muted: c(0xc0c0ff),
    hover_text: c(0xffff00),
    link: c(0xffff00),
    focus: c(0xffff00),
    focus_block: None,
    ok: c(0x30e030),
    err: c(0xff7070),
    warn: c(0xffc000),
    imp: c(0x66ddff),
    tok: c(0xff99ff),
    shape: Shape::Bevel,
    radius: 0,
    round: 0,
    button: tone(0xc0c0c0, 0x000000, 0x404040),
    button_hover: tone(0xdfdfdf, 0x000000, 0x404040),
    button_down: tone(0xc0c0c0, 0x000000, 0x404040),
    on: tone(0xffff00, 0x000000, 0x404040),
    on_hover: tone(0xffff90, 0x000000, 0x404040),
    primary: tone(0xcc0000, 0xffff00, 0xff6060),
    primary_hover: tone(0xe81010, 0xffff00, 0xff6060),
    bevel: [c(0xffffff), c(0xdfdfdf), c(0x808080), c(0x404040)],
    button_padding: [8.0, 3.0],
    field: Field::Sunken,
    field_tone: tone(0xffffff, 0x000000, 0x404040),
    field_hint: c(0x606060),
    field_mono: false,
    check: c(0x00c000),
    sel: tone(0xffffff, 0x000080, 0xffffff),
    hover: tone(0x2020d0, 0xffffff, 0x2020d0),
    text_sel: (c(0x000080), c(0xffffff)),
    scroll: [c(0xc0c0c0), c(0xdfdfdf), c(0x808080)],
    trough: c(0x000040),
    meter: [c(0xc0c0c0), c(0x000080), c(0xcc0000)],
    bar: Bar::Solid,
    bar_fill: c(0xcc0000),
    bar_lines: (c(0xff6060), c(0x800000)),
    bar_text: c(0xffff00),
    bar_text_small: c(0xffff00),
    logo: Logo::WordArt,
    logo_colors: [c(0xffff00), c(0xcc0000), c(0x000000)],
    divider: Divider::Rainbow,
    backdrop: Backdrop::Stars,
    backdrop_colors: [c(0x0000a8), c(0xffff00), c(0x8c8cd8)],
    banner: c(0x000040),
    border: Border::Ridge,
    border_colors: [c(0xffffff), c(0xdfdfdf), c(0x808080), c(0x404040)],
    mark: Mark::Block,
    marks: [c(0x30e030), c(0xff7070), c(0xffc000), c(0x66ddff), c(0xc0c0c0)],
    chip: Tone { fill: ca(0x000060, 225), text: c(0xffffff), edge: c(0x000000) },
    chip_title: c(0xffff00),
    chip_edited: c(0xffff00),
    select_box: (c(0xffff00), ca(0xffff00, 30)),
    view_bg: VIEW_BG,
    fonts: Fonts {
        text: &["verdana.ttf", "Verdana.ttf", "tahoma.ttf", "Tahoma.ttf", "DejaVuSans.ttf"],
        bold: &["verdanab.ttf", "Verdana Bold.ttf", "tahomabd.ttf", "Tahoma Bold.ttf", "DejaVuSans-Bold.ttf"],
        label: &[],
        logo: &["ariblk.ttf", "Arial Black.ttf"],
        mono: false,
        body: 13.0,
        label_spacing: 1.0,
    },
};

// ---------------------------------------------------------------------------------------------------------------
// Cartridge: a gray console of the late 1990s and its controller. Warm gray molded plastic, charcoal label
// plates with white capitals, keys with a lip. The four colors of the face buttons mark states only: green for
// on and good, red for the main action and bad, yellow for warnings, blue for the selection.

pub static CARTRIDGE: Theme = Theme {
    id: "cartridge",
    name: "Cartridge",
    dark: false,
    disabled: 0.55,
    bg: c(0xbdb9b0),
    panel: c(0xcbc7be),
    deep: c(0xa9a59c),
    well: c(0x2b2a28),
    card_edge: c(0x77736a),
    ruled: NONE,
    line: c(0x77736a),
    grid: None,
    popup: c(0xd6d2c9),
    popup_edge: (2.0, c(0x2b2a28)),
    popup_shadow: ca(0x000000, 90),
    scrim: ca(0x1a1917, 170),
    text: c(0x1a1917),
    muted: c(0x3f3c37),
    hover_text: c(0x173a96),
    link: c(0x173a96),
    focus: c(0x1c4fb8),
    focus_block: None,
    ok: c(0x0b5f27),
    err: c(0xa50d1a),
    warn: c(0x6e4600),
    imp: c(0x173a96),
    tok: c(0x6d1b7b),
    shape: Shape::Key,
    radius: 2,
    round: 0,
    button: tone(0xe2dfd6, 0x1a1917, 0x4d4a44),
    button_hover: tone(0xf0eee8, 0x1a1917, 0x4d4a44),
    button_down: tone(0xaeaaa1, 0x1a1917, 0x4d4a44),
    on: tone(0x11793a, 0xffffff, 0x083f1d),
    on_hover: tone(0x168a44, 0xffffff, 0x083f1d),
    primary: tone(0xc8102e, 0xffffff, 0x6b0717),
    primary_hover: tone(0xdb2340, 0xffffff, 0x6b0717),
    bevel: [c(0xf0eee8), c(0xd6d2c9), c(0x77736a), c(0x2b2a28)],
    button_padding: [8.0, 3.0],
    field: Field::Boxed,
    field_tone: tone(0xf4f2ec, 0x111110, 0x4d4a44),
    field_hint: c(0x6b675f),
    field_mono: false,
    check: c(0x1fa04a),
    sel: tone(0x1c4fb8, 0xffffff, 0x1c4fb8),
    hover: tone(0xa39f96, 0x1a1917, 0xa39f96),
    text_sel: (c(0x1c4fb8), c(0xffffff)),
    scroll: [c(0x77736a), c(0x4d4a44), c(0x2b2a28)],
    trough: c(0xa9a59c),
    meter: [c(0x2b2a28), c(0x1fa04a), c(0xd81e2c)],
    bar: Bar::Plate,
    bar_fill: c(0x2b2a28),
    bar_lines: (c(0xc8102e), c(0x000000)),
    bar_text: c(0xf4f2ec),
    bar_text_small: c(0xf4f2ec),
    logo: Logo::Label,
    logo_colors: [c(0xf4f2ec), c(0x2b2a28), c(0xc8102e)],
    divider: Divider::Stripes,
    backdrop: Backdrop::Ribs,
    backdrop_colors: [c(0x8f8b83), c(0x7d7971), c(0xa29e95)],
    banner: c(0xa9a59c),
    border: Border::Band,
    border_colors: [c(0xe2dfd6), c(0x4d4a44), c(0x2b2a28), c(0x111110)],
    mark: Mark::Lamp,
    marks: [c(0x1fa04a), c(0xd81e2c), c(0xf5c400), c(0x2a62d9), c(0x77736a)],
    chip: Tone { fill: ca(0x2b2a28, 235), text: c(0xf4f2ec), edge: c(0x111110) },
    chip_title: c(0xf4f2ec),
    chip_edited: c(0xf5c400),
    select_box: (c(0xf5c400), ca(0xf5c400, 34)),
    view_bg: [0.133, 0.129, 0.122],
    fonts: Fonts {
        text: &["arial.ttf", "Arial.ttf", "LiberationSans-Regular.ttf", "FreeSans.ttf", "DejaVuSans.ttf"],
        bold: &["arialbd.ttf", "Arial Bold.ttf", "LiberationSans-Bold.ttf", "FreeSansBold.ttf", "DejaVuSans-Bold.ttf"],
        label: &["impact.ttf", "Impact.ttf", "LiberationSansNarrow-Bold.ttf", "DejaVuSansCondensed-Bold.ttf", "LiberationSans-Bold.ttf", "DejaVuSans-Bold.ttf"],
        logo: &[],
        mono: false,
        body: 13.0,
        label_spacing: 1.2,
    },
};

// ---------------------------------------------------------------------------------------------------------------
// Blueprint: a drafting sheet. Blue paper with a grid, white and pale cyan line work, controls drawn as hairlines
// without fills, titles followed by dimension lines, a ruler for a divider. Marks are red, yellow and green
// pencil.

pub static BLUEPRINT: Theme = Theme {
    id: "blueprint",
    name: "Blueprint",
    dark: true,
    disabled: 0.55,
    bg: c(0x16407a),
    panel: NONE,
    deep: c(0x113566),
    well: c(0x0d2b54),
    card_edge: c(0x7fa9d6),
    ruled: NONE,
    line: c(0x7fa9d6),
    grid: Some((c(0x1b4985), c(0x245694))),
    popup: c(0x113566),
    popup_edge: (1.0, c(0xdcebfa)),
    popup_shadow: ca(0x061a36, 150),
    scrim: ca(0x061a36, 180),
    text: c(0xf2f7fc),
    muted: c(0xb3d0ee),
    hover_text: c(0x7fe3ff),
    link: c(0x7fe3ff),
    focus: c(0x7fe3ff),
    focus_block: None,
    ok: c(0x9df2b0),
    err: c(0xffa99f),
    warn: c(0xffdd80),
    imp: c(0x7fe3ff),
    tok: c(0xe6c2ff),
    shape: Shape::Outline,
    radius: 0,
    round: 0,
    button: Tone { fill: NONE, text: c(0xf2f7fc), edge: c(0xc5dcf3) },
    button_hover: Tone { fill: ca(0xffffff, 36), text: c(0xffffff), edge: c(0xffffff) },
    button_down: Tone { fill: ca(0xffffff, 80), text: c(0xffffff), edge: c(0xffffff) },
    on: tone(0xf2f7fc, 0x0d2b54, 0xf2f7fc),
    on_hover: tone(0xffffff, 0x0d2b54, 0xffffff),
    primary: tone(0x7fe3ff, 0x0a2548, 0x7fe3ff),
    primary_hover: tone(0xb0eeff, 0x0a2548, 0xb0eeff),
    bevel: [c(0xdcebfa), c(0x7fa9d6), c(0x7fa9d6), c(0xdcebfa)],
    button_padding: [8.0, 3.0],
    field: Field::Boxed,
    field_tone: Tone { fill: ca(0x061a36, 110), text: c(0xffffff), edge: c(0x7fa9d6) },
    field_hint: c(0x9dbfe3),
    field_mono: false,
    check: c(0x7fe3ff),
    sel: tone(0xf2f7fc, 0x0d2b54, 0xf2f7fc),
    hover: Tone { fill: ca(0xffffff, 40), text: c(0xffffff), edge: NONE },
    text_sel: (c(0xf2f7fc), c(0x0d2b54)),
    scroll: [c(0x7fa9d6), c(0xc5dcf3), c(0xf2f7fc)],
    trough: c(0x113566),
    meter: [c(0x0d2b54), c(0xf2f7fc), c(0xffa99f)],
    bar: Bar::Dimension,
    bar_fill: NONE,
    bar_lines: (c(0xc5dcf3), c(0x7fa9d6)),
    bar_text: c(0xffffff),
    bar_text_small: c(0x7fe3ff),
    logo: Logo::TitleBlock,
    logo_colors: [c(0xffffff), c(0xc5dcf3), c(0x7fe3ff)],
    divider: Divider::Scale,
    backdrop: Backdrop::Grid,
    backdrop_colors: [c(0x123a70), c(0x18447f), c(0x24569a)],
    banner: NONE,
    border: Border::Lines(&[(0.0, 2.0), (4.0, 1.0)]),
    border_colors: [c(0xf2f7fc), c(0xc5dcf3), c(0x7fa9d6), c(0x0d2b54)],
    mark: Mark::Drawn,
    marks: [c(0x9df2b0), c(0xffa99f), c(0xffdd80), c(0x7fe3ff), c(0xc5dcf3)],
    chip: Tone { fill: ca(0x113566, 235), text: c(0xf2f7fc), edge: c(0xc5dcf3) },
    chip_title: c(0xffffff),
    chip_edited: c(0xffdd80),
    select_box: (c(0xffffff), ca(0xffffff, 30)),
    view_bg: [0.043, 0.129, 0.259],
    fonts: Fonts {
        text: &["bahnschrift.ttf", "Bahnschrift.ttf", "LiberationSansNarrow-Regular.ttf", "DejaVuSansCondensed.ttf", "Carlito-Regular.ttf", "LiberationSans-Regular.ttf"],
        bold: &["bahnschrift.ttf", "Bahnschrift.ttf", "LiberationSansNarrow-Bold.ttf", "DejaVuSansCondensed-Bold.ttf", "Carlito-Bold.ttf", "LiberationSans-Bold.ttf"],
        label: &[],
        logo: &[],
        mono: false,
        body: 13.5,
        label_spacing: 2.0,
    },
};

// ---------------------------------------------------------------------------------------------------------------
// Paper: a printed manual in two inks. Off-white paper, black ink, one spot color (vermilion). A serif face.
// Rules instead of boxes, fields as tinted blanks with a line under them and typed entries, printed signs for
// the checks, a notebook page behind the setup screen.

pub static PAPER: Theme = Theme {
    id: "paper",
    name: "Paper",
    dark: false,
    disabled: 0.55,
    bg: c(0xf3eee2),
    panel: NONE,
    deep: c(0xe8e2d2),
    well: c(0xe0d9c7),
    card_edge: NONE,
    ruled: c(0xa8a294),
    line: c(0x1b1a18),
    grid: None,
    popup: c(0xfbf8f0),
    popup_edge: (1.0, c(0x1b1a18)),
    popup_shadow: ca(0x1b1a18, 70),
    scrim: ca(0x3a372f, 150),
    text: c(0x1b1a18),
    muted: c(0x565145),
    hover_text: c(0xb02a14),
    link: c(0xb02a14),
    focus: c(0xc2321b),
    focus_block: None,
    ok: c(0x1c6330),
    err: c(0xb02a14),
    warn: c(0x7d5200),
    imp: c(0x1b1a18),
    tok: c(0x6b2a76),
    shape: Shape::Outline,
    radius: 0,
    round: 0,
    button: tone(0xfbf8f0, 0x1b1a18, 0x1b1a18),
    button_hover: tone(0xe0d9c7, 0x1b1a18, 0x1b1a18),
    button_down: tone(0x1b1a18, 0xfbf8f0, 0x1b1a18),
    on: tone(0x1b1a18, 0xfbf8f0, 0x1b1a18),
    on_hover: tone(0x3a372f, 0xfbf8f0, 0x1b1a18),
    primary: tone(0xc2321b, 0xfffaf0, 0x7d1c0c),
    primary_hover: tone(0xd4442c, 0xfffaf0, 0x7d1c0c),
    bevel: [c(0x1b1a18), c(0xa8a294), c(0xa8a294), c(0x1b1a18)],
    // The serif letters are wide. With less room beside them the toolbar keeps to two rows at 1280 points.
    button_padding: [6.0, 3.0],
    field: Field::Underlined,
    field_tone: tone(0xe9e3d3, 0x1b1a18, 0x1b1a18),
    field_hint: c(0x6f695c),
    field_mono: true,
    check: c(0x1b1a18),
    sel: tone(0x1b1a18, 0xfbf8f0, 0x1b1a18),
    hover: tone(0xe0d9c7, 0x1b1a18, 0xe0d9c7),
    text_sel: (c(0xc2321b), c(0xfffaf0)),
    scroll: [c(0xa8a294), c(0x565145), c(0x1b1a18)],
    trough: c(0xe8e2d2),
    meter: [c(0xe0d9c7), c(0x1b1a18), c(0xc2321b)],
    bar: Bar::Rule,
    bar_fill: NONE,
    bar_lines: (c(0x1b1a18), c(0xa8a294)),
    bar_text: c(0x1b1a18),
    bar_text_small: c(0xb02a14),
    logo: Logo::Masthead,
    logo_colors: [c(0x1b1a18), c(0xc2321b), c(0xc2321b)],
    divider: Divider::DoubleRule,
    backdrop: Backdrop::Ruled,
    backdrop_colors: [c(0xece6d6), c(0xc3cfdc), c(0xdf9a8c)],
    banner: NONE,
    border: Border::Lines(&[(0.0, 2.0)]),
    border_colors: [c(0x1b1a18), c(0x1b1a18), c(0x1b1a18), c(0x1b1a18)],
    mark: Mark::Printed,
    marks: [c(0x1c6330), c(0xc2321b), c(0x9a6400), c(0x1b1a18), c(0x565145)],
    chip: Tone { fill: ca(0xf3eee2, 240), text: c(0x1b1a18), edge: c(0x1b1a18) },
    chip_title: c(0x1b1a18),
    chip_edited: c(0xb02a14),
    select_box: (c(0xc2321b), ca(0xc2321b, 40)),
    view_bg: [0.165, 0.157, 0.141],
    fonts: Fonts {
        text: &["georgia.ttf", "Georgia.ttf", "DejaVuSerif.ttf", "LiberationSerif-Regular.ttf", "FreeSerif.ttf"],
        bold: &["georgiab.ttf", "Georgia Bold.ttf", "DejaVuSerif-Bold.ttf", "LiberationSerif-Bold.ttf", "FreeSerifBold.ttf"],
        label: &[],
        logo: &[],
        mono: false,
        body: 13.0,
        label_spacing: 1.5,
    },
};

// ---------------------------------------------------------------------------------------------------------------
// CRT: a debug monitor. Black glass, green phosphor in one fixed-width face, amber for the main action and
// warnings. No fills at rest: a control under the pointer, a switch that is on and the chosen row are inverse
// video. The keyboard focus is a bright block, as a cursor. Frames are double lines. The checks carry codes.

pub static CRT: Theme = Theme {
    id: "crt",
    name: "CRT",
    dark: true,
    disabled: 0.55,
    bg: c(0x040804),
    panel: c(0x040804),
    deep: c(0x091209),
    well: c(0x000000),
    card_edge: c(0x1c7430),
    ruled: NONE,
    line: c(0x1c7430),
    grid: None,
    popup: c(0x040804),
    popup_edge: (1.0, c(0x3dfc6f)),
    popup_shadow: NONE,
    scrim: ca(0x000000, 200),
    text: c(0x4dfa7b),
    muted: c(0x2fb352),
    hover_text: c(0xffb000),
    link: c(0xffb000),
    focus: c(0xeaffea),
    focus_block: Some(c(0x000000)),
    ok: c(0xa8ffbd),
    err: c(0xff6257),
    warn: c(0xffb000),
    imp: c(0x7fdcff),
    tok: c(0xd9a11e),
    shape: Shape::Outline,
    radius: 0,
    round: 0,
    button: Tone { fill: NONE, text: c(0x4dfa7b), edge: c(0x2fb352) },
    button_hover: tone(0x4dfa7b, 0x000000, 0x4dfa7b),
    button_down: tone(0xa8ffbd, 0x000000, 0xa8ffbd),
    on: tone(0x3dfc6f, 0x000000, 0x3dfc6f),
    on_hover: tone(0xa8ffbd, 0x000000, 0xa8ffbd),
    primary: tone(0xffb000, 0x000000, 0xffb000),
    primary_hover: tone(0xffd060, 0x000000, 0xffd060),
    bevel: [c(0x3dfc6f), c(0x1c7430), c(0x1c7430), c(0x3dfc6f)],
    button_padding: [7.0, 3.0],
    field: Field::Boxed,
    field_tone: tone(0x000000, 0x7dffa0, 0x1c7430),
    field_hint: c(0x2fb352),
    field_mono: true,
    check: c(0x3dfc6f),
    sel: tone(0x3dfc6f, 0x000000, 0x3dfc6f),
    hover: tone(0x125a24, 0xa8ffbd, 0x125a24),
    text_sel: (c(0x3dfc6f), c(0x000000)),
    scroll: [c(0x1c7430), c(0x2fb352), c(0x4dfa7b)],
    trough: c(0x000000),
    meter: [c(0x000000), c(0x3dfc6f), c(0xff6257)],
    bar: Bar::Inverse,
    bar_fill: c(0x3dfc6f),
    bar_lines: (c(0x2fb352), c(0x1c7430)),
    bar_text: c(0x000000),
    bar_text_small: c(0x000000),
    logo: Logo::Prompt,
    logo_colors: [c(0x4dfa7b), c(0xffb000), c(0x2fb352)],
    divider: Divider::DoubleLine,
    backdrop: Backdrop::Scanlines,
    backdrop_colors: [c(0x000000), c(0x081a0c), c(0x081a0c)],
    banner: NONE,
    border: Border::Lines(&[(1.0, 1.0), (4.0, 1.0)]),
    border_colors: [c(0x3dfc6f), c(0x3dfc6f), c(0x1c7430), c(0x000000)],
    mark: Mark::Code,
    marks: [c(0x3dfc6f), c(0xff6257), c(0xffb000), c(0x7fdcff), c(0x2fb352)],
    chip: Tone { fill: ca(0x000000, 235), text: c(0x4dfa7b), edge: c(0x1c7430) },
    chip_title: c(0xa8ffbd),
    chip_edited: c(0xffb000),
    select_box: (c(0xffb000), ca(0xffb000, 30)),
    view_bg: [0.0, 0.0, 0.0],
    fonts: Fonts { text: &[], bold: &[], label: &[], logo: &[], mono: true, body: 12.0, label_spacing: 1.0 },
};

// ---------------------------------------------------------------------------------------------------------------
// Light: a desktop tool in the system's light colors. Neutral grays, white fields and buttons with a thin line
// and a small corner radius, one blue for the selection, the focus and the main action. A switch that is on is
// filled dark gray. No decoration.

pub static LIGHT: Theme = Theme {
    id: "light",
    name: "Light",
    dark: false,
    disabled: 0.62,
    bg: c(0xf3f3f3),
    panel: c(0xffffff),
    deep: c(0xe6e6e6),
    well: c(0xdcdcdc),
    card_edge: c(0xcfcfcf),
    ruled: NONE,
    line: c(0xc4c4c4),
    grid: None,
    popup: c(0xffffff),
    popup_edge: (1.0, c(0xa6a6a6)),
    popup_shadow: ca(0x000000, 40),
    scrim: ca(0x000000, 110),
    text: c(0x1b1b1b),
    muted: c(0x595959),
    hover_text: c(0x005a9e),
    link: c(0x005a9e),
    focus: c(0x005fb8),
    focus_block: None,
    ok: c(0x0b6a0b),
    err: c(0xb3261e),
    warn: c(0x7a4a00),
    imp: c(0x005a9e),
    tok: c(0x7a2a8c),
    shape: Shape::Outline,
    radius: 3,
    round: 0,
    button: tone(0xffffff, 0x1b1b1b, 0xb4b4b4),
    button_hover: tone(0xe5f1fb, 0x1b1b1b, 0x005fb8),
    button_down: tone(0xcce4f7, 0x1b1b1b, 0x005fb8),
    on: tone(0x505050, 0xffffff, 0x3a3a3a),
    on_hover: tone(0x626262, 0xffffff, 0x3a3a3a),
    primary: tone(0x005fb8, 0xffffff, 0x004c94),
    primary_hover: tone(0x1a70c2, 0xffffff, 0x004c94),
    bevel: [c(0xffffff), c(0xdcdcdc), c(0xa6a6a6), c(0x8a8a8a)],
    button_padding: [8.0, 3.0],
    field: Field::Boxed,
    field_tone: tone(0xffffff, 0x1b1b1b, 0x8a8a8a),
    field_hint: c(0x6b6b6b),
    field_mono: false,
    check: c(0xffffff),
    sel: tone(0x005fb8, 0xffffff, 0x005fb8),
    hover: tone(0xe5f1fb, 0x1b1b1b, 0xe5f1fb),
    text_sel: (c(0x0067c0), c(0xffffff)),
    scroll: [c(0xc1c1c1), c(0xa6a6a6), c(0x7a7a7a)],
    trough: c(0xededed),
    meter: [c(0xdcdcdc), c(0x005fb8), c(0xc42b1c)],
    bar: Bar::Solid,
    bar_fill: c(0xe2e2e2),
    bar_lines: (c(0xe2e2e2), c(0xc4c4c4)),
    bar_text: c(0x1b1b1b),
    bar_text_small: c(0x1b1b1b),
    logo: Logo::Plain,
    logo_colors: [c(0x1b1b1b), c(0x005fb8), c(0x005fb8)],
    divider: Divider::Line,
    backdrop: Backdrop::Plain,
    backdrop_colors: [c(0xe6e6e6), c(0xe6e6e6), c(0xe6e6e6)],
    banner: NONE,
    border: Border::Lines(&[(0.0, 1.0)]),
    border_colors: [c(0xa6a6a6), c(0xa6a6a6), c(0xa6a6a6), c(0xa6a6a6)],
    mark: Mark::Printed,
    marks: [c(0x107c10), c(0xc42b1c), c(0x9d5d00), c(0x005fb8), c(0x767676)],
    chip: Tone { fill: ca(0xffffff, 235), text: c(0x1b1b1b), edge: c(0xa6a6a6) },
    chip_title: c(0x1b1b1b),
    chip_edited: c(0x7a4a00),
    select_box: (c(0x4cc2ff), ca(0x4cc2ff, 34)),
    view_bg: [0.169, 0.169, 0.169],
    fonts: SANS_FONTS,
};

// ---------------------------------------------------------------------------------------------------------------
// Dark: the same tool in the system's dark colors. Near-black grays, a light blue for the selection, the focus
// and the main action. A switch that is on is filled light gray.

pub static DARK: Theme = Theme {
    id: "dark",
    name: "Dark",
    dark: true,
    disabled: 0.6,
    bg: c(0x242424),
    panel: c(0x2b2b2b),
    deep: c(0x1c1c1c),
    well: c(0x181818),
    card_edge: c(0x404040),
    ruled: NONE,
    line: c(0x454545),
    grid: None,
    popup: c(0x2c2c2c),
    popup_edge: (1.0, c(0x5a5a5a)),
    popup_shadow: ca(0x000000, 110),
    scrim: ca(0x000000, 150),
    text: c(0xe6e6e6),
    muted: c(0xababab),
    hover_text: c(0x60cdff),
    link: c(0x60cdff),
    focus: c(0x60cdff),
    focus_block: None,
    ok: c(0x6ccb5f),
    err: c(0xff8f87),
    warn: c(0xf2c661),
    imp: c(0x60cdff),
    tok: c(0xd7a6e8),
    shape: Shape::Outline,
    radius: 3,
    round: 0,
    button: tone(0x383838, 0xf0f0f0, 0x5a5a5a),
    button_hover: tone(0x464646, 0xffffff, 0x60cdff),
    button_down: tone(0x2e2e2e, 0xffffff, 0x60cdff),
    on: tone(0xd6d6d6, 0x111111, 0xf0f0f0),
    on_hover: tone(0xeaeaea, 0x111111, 0xffffff),
    primary: tone(0x60cdff, 0x00131f, 0x60cdff),
    primary_hover: tone(0x86d9ff, 0x00131f, 0x86d9ff),
    bevel: [c(0x757575), c(0x4a4a4a), c(0x4a4a4a), c(0x111111)],
    button_padding: [8.0, 3.0],
    field: Field::Boxed,
    field_tone: tone(0x1b1b1b, 0xf0f0f0, 0x757575),
    field_hint: c(0x9a9a9a),
    field_mono: false,
    check: c(0x60cdff),
    sel: tone(0x0b5a8f, 0xffffff, 0x0b5a8f),
    hover: tone(0x3a3a3a, 0xffffff, 0x3a3a3a),
    text_sel: (c(0x264f78), c(0xffffff)),
    scroll: [c(0x555555), c(0x6e6e6e), c(0x8a8a8a)],
    trough: c(0x1c1c1c),
    meter: [c(0x181818), c(0x60cdff), c(0xff7b72)],
    bar: Bar::Solid,
    bar_fill: c(0x363636),
    bar_lines: (c(0x363636), c(0x161616)),
    bar_text: c(0xe6e6e6),
    bar_text_small: c(0xe6e6e6),
    logo: Logo::Plain,
    logo_colors: [c(0xf0f0f0), c(0x60cdff), c(0x60cdff)],
    divider: Divider::Line,
    backdrop: Backdrop::Plain,
    backdrop_colors: [c(0x1c1c1c), c(0x1c1c1c), c(0x1c1c1c)],
    banner: NONE,
    border: Border::Lines(&[(0.0, 1.0)]),
    border_colors: [c(0x5a5a5a), c(0x5a5a5a), c(0x5a5a5a), c(0x5a5a5a)],
    mark: Mark::Printed,
    marks: [c(0x6ccb5f), c(0xff7b72), c(0xf2c661), c(0x60cdff), c(0x9a9a9a)],
    chip: Tone { fill: ca(0x2b2b2b, 235), text: c(0xe6e6e6), edge: c(0x5a5a5a) },
    chip_title: c(0xffffff),
    chip_edited: c(0xf2c661),
    select_box: (c(0x60cdff), ca(0x60cdff, 34)),
    view_bg: [0.094, 0.094, 0.094],
    fonts: SANS_FONTS,
};

// ---------------------------------------------------------------------------------------------------------------
// High contrast: white on black, as the system's high contrast mode. Every control has a white line. Yellow is
// for the pointer and the keyboard focus, which fills the button. Cyan is for the selection and the main action.
// A switch that is on is white with black letters.

pub static CONTRAST: Theme = Theme {
    id: "contrast",
    name: "High contrast",
    dark: true,
    disabled: 0.6,
    bg: c(0x000000),
    panel: c(0x000000),
    deep: c(0x000000),
    well: c(0x000000),
    card_edge: c(0xffffff),
    ruled: NONE,
    line: c(0xffffff),
    grid: None,
    popup: c(0x000000),
    popup_edge: (2.0, c(0xffffff)),
    popup_shadow: NONE,
    scrim: ca(0x000000, 200),
    text: c(0xffffff),
    muted: c(0xd9d9d9),
    hover_text: c(0xffff00),
    link: c(0xffff00),
    focus: c(0xffff00),
    focus_block: Some(c(0x000000)),
    ok: c(0x3ff23f),
    err: c(0xff6b6b),
    warn: c(0xffd500),
    imp: c(0x1aebff),
    tok: c(0xff9cf5),
    shape: Shape::Outline,
    radius: 0,
    round: 0,
    button: tone(0x000000, 0xffffff, 0xffffff),
    button_hover: tone(0x000000, 0xffff00, 0xffff00),
    button_down: tone(0xffff00, 0x000000, 0xffff00),
    on: tone(0xffffff, 0x000000, 0xffffff),
    on_hover: tone(0xffff00, 0x000000, 0xffff00),
    primary: tone(0x1aebff, 0x000000, 0x1aebff),
    primary_hover: tone(0x9df6ff, 0x000000, 0x9df6ff),
    bevel: [c(0xffffff), c(0xffffff), c(0xffffff), c(0xffffff)],
    button_padding: [8.0, 3.0],
    field: Field::Boxed,
    field_tone: tone(0x000000, 0xffffff, 0xffffff),
    field_hint: c(0xc8c8c8),
    field_mono: false,
    check: c(0x1aebff),
    sel: tone(0x1aebff, 0x000000, 0x1aebff),
    hover: tone(0x004b52, 0xffffff, 0x004b52),
    text_sel: (c(0x1aebff), c(0x000000)),
    scroll: [c(0xffffff), c(0xffff00), c(0x1aebff)],
    trough: c(0x000000),
    meter: [c(0x000000), c(0x1aebff), c(0xff6b6b)],
    bar: Bar::Solid,
    bar_fill: c(0x000000),
    bar_lines: (c(0x000000), c(0xffffff)),
    bar_text: c(0xffffff),
    bar_text_small: c(0xffffff),
    logo: Logo::Plain,
    logo_colors: [c(0xffffff), c(0xffff00), c(0xffff00)],
    divider: Divider::Line,
    backdrop: Backdrop::Plain,
    backdrop_colors: [c(0x000000), c(0x000000), c(0x000000)],
    banner: NONE,
    border: Border::Lines(&[(0.0, 2.0)]),
    border_colors: [c(0xffffff), c(0xffffff), c(0xffffff), c(0xffffff)],
    mark: Mark::Printed,
    marks: [c(0x3ff23f), c(0xff6b6b), c(0xffd500), c(0x1aebff), c(0xffffff)],
    chip: Tone { fill: c(0x000000), text: c(0xffffff), edge: c(0xffffff) },
    chip_title: c(0xffffff),
    chip_edited: c(0xffff00),
    select_box: (c(0xffff00), ca(0xffff00, 40)),
    view_bg: [0.0, 0.0, 0.0],
    fonts: SANS_FONTS,
};

// ---------------------------------------------------------------------------------------------------------------
// Sage: a calm app in muted greens. A very pale green ground, off-white cards, a deep sage for the main action,
// the selection and the focus, moss for the headers, ochre for the box of a box select.

pub static SAGE: Theme = Theme {
    id: "sage",
    name: "Sage",
    dark: false,
    disabled: 0.71,
    bg: c(0xe9f0e4),
    panel: c(0xf8faf5),
    deep: c(0xd4ded0),
    well: c(0xd7dfd3),
    card_edge: c(0xdce0db),
    ruled: NONE,
    line: c(0xc7cfc4),
    grid: None,
    popup: c(0xf8faf5),
    popup_edge: (1.0, c(0xadb6ac)),
    popup_shadow: ca(0x0e1311, 26),
    scrim: ca(0x111815, 105),
    text: c(0x22302a),
    muted: c(0x56625b),
    hover_text: c(0x3c5f48),
    link: c(0x3c5f48),
    focus: c(0x456f52),
    focus_block: None,
    ok: c(0x2e693e),
    err: c(0xa63a30),
    warn: c(0x825509),
    imp: c(0x3c5f48),
    tok: c(0x6e4a80),
    shape: Shape::Outline,
    radius: 6,
    round: 8,
    button: tone(0xf8faf5, 0x22302a, 0xcdd2cc),
    button_hover: tone(0xdfe7de, 0x22302a, 0x456f52),
    button_down: tone(0xc9d6cb, 0x22302a, 0x456f52),
    on: tone(0xb8c8ba, 0x2d4835, 0x456f52),
    on_hover: tone(0xacbfb0, 0x2d4835, 0x456f52),
    primary: tone(0x456f52, 0xffffff, 0x456f52),
    primary_hover: tone(0x587d63, 0xffffff, 0x587d63),
    bevel: [c(0xf8faf5), c(0xdce0db), c(0xadb6ac), c(0x959f96)],
    button_padding: [8.0, 3.0],
    field: Field::Boxed,
    field_tone: tone(0xfcfdfb, 0x18221d, 0x8a928d),
    field_hint: c(0x6b7571),
    field_mono: false,
    check: c(0x456f52),
    sel: tone(0x456f52, 0xffffff, 0x456f52),
    hover: tone(0xd2ded0, 0x22302a, 0xd2ded0),
    text_sel: (c(0x456f52), c(0xffffff)),
    scroll: [c(0xb9c2b7), c(0xa1aba1), c(0x869087)],
    trough: c(0xe0e7dc),
    meter: [c(0xd7dfd3), c(0x456f52), c(0xa63a30)],
    bar: Bar::Soft,
    bar_fill: c(0xdde5dd),
    bar_lines: (c(0xc7cfc4), c(0xc7cfc4)),
    bar_text: c(0x34503e),
    bar_text_small: c(0x34503e),
    logo: Logo::Plain,
    logo_colors: [c(0x34503e), c(0x456f52), c(0xb98a3e)],
    divider: Divider::Line,
    backdrop: Backdrop::Plain,
    backdrop_colors: [c(0xd4ded0), c(0xd4ded0), c(0xd4ded0)],
    banner: NONE,
    border: Border::Lines(&[(0.0, 1.0)]),
    border_colors: [c(0xadb6ac), c(0xadb6ac), c(0xadb6ac), c(0xadb6ac)],
    mark: Mark::Printed,
    marks: [c(0x2f6b3f), c(0xa63a30), c(0x8a5a0a), c(0x456f52), c(0x7c867e)],
    chip: Tone { fill: ca(0xf8faf5, 240), text: c(0x22302a), edge: c(0xadb6ac) },
    chip_title: c(0x22302a),
    chip_edited: c(0x825509),
    select_box: (c(0xb98a3e), ca(0xb98a3e, 36)),
    view_bg: [0.125, 0.180, 0.149],
    fonts: SANS_FONTS,
};

// ---------------------------------------------------------------------------------------------------------------
// Sand: warm paper tones. A sand ground, cream cards, a caramel brown for the main action, the selection and
// the focus, teal for the box of a box select.

pub static SAND: Theme = Theme {
    id: "sand",
    name: "Sand",
    dark: false,
    disabled: 0.7,
    bg: c(0xf4eee3),
    panel: c(0xfffcf6),
    deep: c(0xe7dbcc),
    well: c(0xe3dcd1),
    card_edge: c(0xe5e1da),
    ruled: NONE,
    line: c(0xd4cdc2),
    grid: None,
    popup: c(0xfffcf6),
    popup_edge: (1.0, c(0xbbb3a8)),
    popup_shadow: ca(0x15110d, 26),
    scrim: ca(0x1a1510, 105),
    text: c(0x352a20),
    muted: c(0x675e53),
    hover_text: c(0x8a5228),
    link: c(0x8a5228),
    focus: c(0xa65f2b),
    focus_block: None,
    ok: c(0x3a6b2c),
    err: c(0xa8322a),
    warn: c(0x83550a),
    imp: c(0x8a5228),
    tok: c(0x7a4374),
    shape: Shape::Outline,
    radius: 6,
    round: 8,
    button: tone(0xfffcf6, 0x352a20, 0xd7d2cb),
    button_hover: tone(0xf3e6da, 0x352a20, 0xa65f2b),
    button_down: tone(0xe8d3c1, 0x352a20, 0xa65f2b),
    on: tone(0xdfc3ad, 0x6c3e1c, 0xa65f2b),
    on_hover: tone(0xd9b9a0, 0x6c3e1c, 0xa65f2b),
    primary: tone(0xa65f2b, 0xffffff, 0xa65f2b),
    primary_hover: tone(0xaf6f40, 0xffffff, 0xaf6f40),
    bevel: [c(0xfffcf6), c(0xe5e1da), c(0xbbb3a8), c(0xa49c91)],
    button_padding: [8.0, 3.0],
    field: Field::Boxed,
    field_tone: tone(0xfffefb, 0x251d16, 0x98928b),
    field_hint: c(0x7b736c),
    field_mono: false,
    check: c(0xa65f2b),
    sel: tone(0xa65f2b, 0xffffff, 0xa65f2b),
    hover: tone(0xe9dac9, 0x352a20, 0xe9dac9),
    text_sel: (c(0xa65f2b), c(0xffffff)),
    scroll: [c(0xc6bfb4), c(0xafa79d), c(0x948c82)],
    trough: c(0xebe5da),
    meter: [c(0xe3dcd1), c(0xa65f2b), c(0xa8322a)],
    bar: Bar::Soft,
    bar_fill: c(0xf2e4d8),
    bar_lines: (c(0xd4cdc2), c(0xd4cdc2)),
    bar_text: c(0x6e4426),
    bar_text_small: c(0x6e4426),
    logo: Logo::Plain,
    logo_colors: [c(0x6e4426), c(0xa65f2b), c(0x3d7a76)],
    divider: Divider::Line,
    backdrop: Backdrop::Plain,
    backdrop_colors: [c(0xe7dbcc), c(0xe7dbcc), c(0xe7dbcc)],
    banner: NONE,
    border: Border::Lines(&[(0.0, 1.0)]),
    border_colors: [c(0xbbb3a8), c(0xbbb3a8), c(0xbbb3a8), c(0xbbb3a8)],
    mark: Mark::Printed,
    marks: [c(0x3a6b2c), c(0xa8322a), c(0x86570a), c(0xa65f2b), c(0x8b8278)],
    chip: Tone { fill: ca(0xfffcf6, 240), text: c(0x352a20), edge: c(0xbbb3a8) },
    chip_title: c(0x352a20),
    chip_edited: c(0x83550a),
    select_box: (c(0x3d7a76), ca(0x3d7a76, 36)),
    view_bg: [0.227, 0.157, 0.102],
    fonts: SANS_FONTS,
};

// ---------------------------------------------------------------------------------------------------------------
// Ocean: cool sea tones. A pale blue-gray ground, near-white cards, a deep teal blue for the main action, the
// selection and the focus, a warm orange for the box of a box select.

pub static OCEAN: Theme = Theme {
    id: "ocean",
    name: "Ocean",
    dark: false,
    disabled: 0.7,
    bg: c(0xeaf2f6),
    panel: c(0xfafdfe),
    deep: c(0xcfe0e6),
    well: c(0xd7e1e5),
    card_edge: c(0xdce2e5),
    ruled: NONE,
    line: c(0xc6d1d6),
    grid: None,
    popup: c(0xfafdfe),
    popup_edge: (1.0, c(0xaab8be)),
    popup_shadow: ca(0x081318, 26),
    scrim: ca(0x0a181e, 105),
    text: c(0x14303b),
    muted: c(0x4d646d),
    hover_text: c(0x195e78),
    link: c(0x195e78),
    focus: c(0x1b6e8c),
    focus_block: None,
    ok: c(0x1f6b45),
    err: c(0xa8362e),
    warn: c(0x80560a),
    imp: c(0x195e78),
    tok: c(0x6a4a8c),
    shape: Shape::Outline,
    radius: 6,
    round: 8,
    button: tone(0xfafdfe, 0x14303b, 0xccd4d7),
    button_hover: tone(0xdbe9ee, 0x14303b, 0x1b6e8c),
    button_down: tone(0xc0d8e0, 0x14303b, 0x1b6e8c),
    on: tone(0xaacad5, 0x12485b, 0x1b6e8c),
    on_hover: tone(0x9cc1ce, 0x12485b, 0x1b6e8c),
    primary: tone(0x1b6e8c, 0xffffff, 0x1b6e8c),
    primary_hover: tone(0x327c98, 0xffffff, 0x327c98),
    bevel: [c(0xfafdfe), c(0xdce2e5), c(0xaab8be), c(0x90a1a7)],
    button_padding: [8.0, 3.0],
    field: Field::Boxed,
    field_tone: tone(0xfdfeff, 0x0e2229, 0x86959b),
    field_hint: c(0x64777e),
    field_mono: false,
    check: c(0x1b6e8c),
    sel: tone(0x1b6e8c, 0xffffff, 0x1b6e8c),
    hover: tone(0xcde0e7, 0x14303b, 0xcde0e7),
    text_sel: (c(0x1b6e8c), c(0xffffff)),
    scroll: [c(0xb7c3c9), c(0x9dacb3), c(0x7f9198)],
    trough: c(0xe0e9ee),
    meter: [c(0xd7e1e5), c(0x1b6e8c), c(0xa8362e)],
    bar: Bar::Soft,
    bar_fill: c(0xd9e8ed),
    bar_lines: (c(0xc6d1d6), c(0xc6d1d6)),
    bar_text: c(0x184f64),
    bar_text_small: c(0x184f64),
    logo: Logo::Plain,
    logo_colors: [c(0x184f64), c(0x1b6e8c), c(0xe0884a)],
    divider: Divider::Line,
    backdrop: Backdrop::Plain,
    backdrop_colors: [c(0xcfe0e6), c(0xcfe0e6), c(0xcfe0e6)],
    banner: NONE,
    border: Border::Lines(&[(0.0, 1.0)]),
    border_colors: [c(0xaab8be), c(0xaab8be), c(0xaab8be), c(0xaab8be)],
    mark: Mark::Printed,
    marks: [c(0x1f6b45), c(0xa8362e), c(0x80560a), c(0x1b6e8c), c(0x74878f)],
    chip: Tone { fill: ca(0xfafdfe, 240), text: c(0x14303b), edge: c(0xaab8be) },
    chip_title: c(0x14303b),
    chip_edited: c(0x80560a),
    select_box: (c(0xe0884a), ca(0xe0884a, 36)),
    view_bg: [0.063, 0.180, 0.227],
    fonts: SANS_FONTS,
};

// ---------------------------------------------------------------------------------------------------------------
// Lavender: soft violets. A pale lilac ground, near-white cards, a violet for the main action, the selection
// and the focus, rose for the box of a box select.

pub static LAVENDER: Theme = Theme {
    id: "lavender",
    name: "Lavender",
    dark: false,
    disabled: 0.7,
    bg: c(0xf2eff8),
    panel: c(0xfdfcff),
    deep: c(0xded9ec),
    well: c(0xe0dde8),
    card_edge: c(0xe2e0e6),
    ruled: NONE,
    line: c(0xd0cdd9),
    grid: None,
    popup: c(0xfdfcff),
    popup_edge: (1.0, c(0xb6b2c1)),
    popup_shadow: ca(0x110f1a, 26),
    scrim: ca(0x151221, 105),
    text: c(0x2a2542),
    muted: c(0x605c73),
    hover_text: c(0x574594),
    link: c(0x574594),
    focus: c(0x6650b0),
    focus_block: None,
    ok: c(0x2c6b47),
    err: c(0xa8324a),
    warn: c(0x82560c),
    imp: c(0x574594),
    tok: c(0x8a3f86),
    shape: Shape::Outline,
    radius: 6,
    round: 8,
    button: tone(0xfdfcff, 0x2a2542, 0xd3d1d9),
    button_hover: tone(0xe8e4f4, 0x2a2542, 0x6650b0),
    button_down: tone(0xd6cfea, 0x2a2542, 0x6650b0),
    on: tone(0xc7bee3, 0x423472, 0x6650b0),
    on_hover: tone(0xbdb3de, 0x423472, 0x6650b0),
    primary: tone(0x6650b0, 0xffffff, 0x6650b0),
    primary_hover: tone(0x7562b8, 0xffffff, 0x7562b8),
    bevel: [c(0xfdfcff), c(0xe2e0e6), c(0xb6b2c1), c(0x9e9aac)],
    button_padding: [8.0, 3.0],
    field: Field::Boxed,
    field_tone: tone(0xfefeff, 0x1d1a2e, 0x928f9e),
    field_hint: c(0x757285),
    field_mono: false,
    check: c(0x6650b0),
    sel: tone(0x6650b0, 0xffffff, 0x6650b0),
    hover: tone(0xded9ee, 0x2a2542, 0xded9ee),
    text_sel: (c(0x6650b0), c(0xffffff)),
    scroll: [c(0xc2bfcc), c(0xaaa6b6), c(0x8e8a9d)],
    trough: c(0xe9e6f0),
    meter: [c(0xe0dde8), c(0x6650b0), c(0xa8324a)],
    bar: Bar::Soft,
    bar_fill: c(0xe6e2f3),
    bar_lines: (c(0xd0cdd9), c(0xd0cdd9)),
    bar_text: c(0x483a79),
    bar_text_small: c(0x483a79),
    logo: Logo::Plain,
    logo_colors: [c(0x483a79), c(0x6650b0), c(0xc85c8e)],
    divider: Divider::Line,
    backdrop: Backdrop::Plain,
    backdrop_colors: [c(0xded9ec), c(0xded9ec), c(0xded9ec)],
    banner: NONE,
    border: Border::Lines(&[(0.0, 1.0)]),
    border_colors: [c(0xb6b2c1), c(0xb6b2c1), c(0xb6b2c1), c(0xb6b2c1)],
    mark: Mark::Printed,
    marks: [c(0x2c6b47), c(0xa8324a), c(0x82560c), c(0x6650b0), c(0x848094)],
    chip: Tone { fill: ca(0xfdfcff, 240), text: c(0x2a2542), edge: c(0xb6b2c1) },
    chip_title: c(0x2a2542),
    chip_edited: c(0x82560c),
    select_box: (c(0xc85c8e), ca(0xc85c8e, 36)),
    view_bg: [0.161, 0.133, 0.267],
    fonts: SANS_FONTS,
};

// ---------------------------------------------------------------------------------------------------------------
// Coral: warm pinks. A blush ground, near-white cards, a coral red for the main action, the selection and the
// focus, teal for the box of a box select. Bad is a deep crimson, apart from the coral.

pub static CORAL: Theme = Theme {
    id: "coral",
    name: "Coral",
    dark: false,
    disabled: 0.7,
    bg: c(0xfbf0ec),
    panel: c(0xfffaf8),
    deep: c(0xf0dad5),
    well: c(0xeadeda),
    card_edge: c(0xe6dedc),
    ruled: NONE,
    line: c(0xdaceca),
    grid: None,
    popup: c(0xfffaf8),
    popup_edge: (1.0, c(0xc1b3af)),
    popup_shadow: ca(0x180f0e, 26),
    scrim: ca(0x1e1311, 105),
    text: c(0x3b2622),
    muted: c(0x6e5d59),
    hover_text: c(0x9f4336),
    link: c(0x9f4336),
    focus: c(0xc9503f),
    focus_block: None,
    ok: c(0x2f6b3c),
    err: c(0x9c1c40),
    warn: c(0x85560a),
    imp: c(0x9f4336),
    tok: c(0x76407e),
    shape: Shape::Outline,
    radius: 6,
    round: 8,
    button: tone(0xfffaf8, 0x3b2622, 0xd8d0cd),
    button_hover: tone(0xf7e2de, 0x3b2622, 0xc9503f),
    button_down: tone(0xf1cec8, 0x3b2622, 0xc9503f),
    on: tone(0xecbdb5, 0x803328, 0xc9503f),
    on_hover: tone(0xe8b2a9, 0x803328, 0xc9503f),
    primary: tone(0xc54e3e, 0xffffff, 0xc54e3e),
    primary_hover: tone(0xcb6051, 0xffffff, 0xcb6051),
    bevel: [c(0xfffaf8), c(0xe6dedc), c(0xc1b3af), c(0xaa9b97)],
    button_padding: [8.0, 3.0],
    field: Field::Boxed,
    field_tone: tone(0xfffdfc, 0x291b18, 0x9b8f8d),
    field_hint: c(0x7e706d),
    field_mono: false,
    check: c(0xc54e3e),
    sel: tone(0xc54e3e, 0xffffff, 0xc54e3e),
    hover: tone(0xf4dad4, 0x3b2622, 0xf4dad4),
    text_sel: (c(0xc54e3e), c(0xffffff)),
    scroll: [c(0xcdc0bc), c(0xb6a7a3), c(0x9b8b87)],
    trough: c(0xf2e7e3),
    meter: [c(0xeadeda), c(0xc54e3e), c(0x9c1c40)],
    bar: Bar::Soft,
    bar_fill: c(0xf7e0dc),
    bar_lines: (c(0xdaceca), c(0xdaceca)),
    bar_text: c(0x823b30),
    bar_text_small: c(0x823b30),
    logo: Logo::Plain,
    logo_colors: [c(0x823b30), c(0xc9503f), c(0x2c7f7b)],
    divider: Divider::Line,
    backdrop: Backdrop::Plain,
    backdrop_colors: [c(0xf0dad5), c(0xf0dad5), c(0xf0dad5)],
    banner: NONE,
    border: Border::Lines(&[(0.0, 1.0)]),
    border_colors: [c(0xc1b3af), c(0xc1b3af), c(0xc1b3af), c(0xc1b3af)],
    mark: Mark::Printed,
    marks: [c(0x2f6b3c), c(0x9c1c40), c(0x85560a), c(0xc9503f), c(0x91817d)],
    chip: Tone { fill: ca(0xfffaf8, 240), text: c(0x3b2622), edge: c(0xc1b3af) },
    chip_title: c(0x3b2622),
    chip_edited: c(0x85560a),
    select_box: (c(0x2c7f7b), ca(0x2c7f7b, 36)),
    view_bg: [0.267, 0.137, 0.118],
    fonts: SANS_FONTS,
};

// ---------------------------------------------------------------------------------------------------------------
// Slate: a dark app in blue-grays. A slate ground, lighter slate cards, a soft teal for the main action, the
// selection and the focus, amber for the box of a box select.

pub static SLATE: Theme = Theme {
    id: "slate",
    name: "Slate",
    dark: true,
    disabled: 0.63,
    bg: c(0x222932),
    panel: c(0x2b3440),
    deep: c(0x1b2128),
    well: c(0x181d23),
    card_edge: c(0x414a55),
    ruled: NONE,
    line: c(0x434a52),
    grid: None,
    popup: c(0x313945),
    popup_edge: (1.0, c(0x5c636b)),
    popup_shadow: ca(0x000000, 70),
    scrim: ca(0x0a0c0f, 150),
    text: c(0xe3e9ef),
    muted: c(0xa5acb3),
    hover_text: c(0x7ecabf),
    link: c(0x7ecabf),
    focus: c(0x6cc4b6),
    focus_block: None,
    ok: c(0x8fd694),
    err: c(0xff9a8f),
    warn: c(0xf2c46b),
    imp: c(0x7ecabf),
    tok: c(0xd3a9e8),
    shape: Shape::Outline,
    radius: 6,
    round: 8,
    button: tone(0x38414c, 0xe3e9ef, 0x5a636d),
    button_hover: tone(0x3f535b, 0xe3e9ef, 0x6cc4b6),
    button_down: tone(0x466368, 0xe3e9ef, 0x6cc4b6),
    on: tone(0x4b7072, 0xf2faf9, 0x6cc4b6),
    on_hover: tone(0x4e7879, 0xf2faf9, 0x6cc4b6),
    primary: tone(0x6cc4b6, 0x13171c, 0x6cc4b6),
    primary_hover: tone(0x86cfc3, 0x13171c, 0x86cfc3),
    bevel: [c(0x2b3440), c(0x414a55), c(0x5c636b), c(0x737a81)],
    button_padding: [8.0, 3.0],
    field: Field::Boxed,
    field_tone: tone(0x181e24, 0xebf0f4, 0x666c72),
    field_hint: c(0x92989e),
    field_mono: false,
    check: c(0x6cc4b6),
    sel: tone(0x416a69, 0xffffff, 0x416a69),
    hover: tone(0x2e4247, 0xe3e9ef, 0x2e4247),
    text_sel: (c(0x416a69), c(0xffffff)),
    scroll: [c(0x50575f), c(0x676e76), c(0x828990)],
    trough: c(0x1e242c),
    meter: [c(0x181d23), c(0x6cc4b6), c(0xff9a8f)],
    bar: Bar::Soft,
    bar_fill: c(0x385158),
    bar_lines: (c(0x434a52), c(0x434a52)),
    bar_text: c(0xadd8d5),
    bar_text_small: c(0xadd8d5),
    logo: Logo::Plain,
    logo_colors: [c(0xadd8d5), c(0x6cc4b6), c(0xf2b56b)],
    divider: Divider::Line,
    backdrop: Backdrop::Plain,
    backdrop_colors: [c(0x1b2128), c(0x1b2128), c(0x1b2128)],
    banner: NONE,
    border: Border::Lines(&[(0.0, 1.0)]),
    border_colors: [c(0x5c636b), c(0x5c636b), c(0x5c636b), c(0x5c636b)],
    mark: Mark::Printed,
    marks: [c(0x8fd694), c(0xff9a8f), c(0xf2c46b), c(0x6cc4b6), c(0x8c939a)],
    chip: Tone { fill: ca(0x313945, 240), text: c(0xe3e9ef), edge: c(0x5c636b) },
    chip_title: c(0xe3e9ef),
    chip_edited: c(0xf2c46b),
    select_box: (c(0xf2b56b), ca(0xf2b56b, 36)),
    view_bg: [0.098, 0.133, 0.149],
    fonts: SANS_FONTS,
};

// ---------------------------------------------------------------------------------------------------------------
// Plum: a dark app in warm purples. A plum ground, lighter plum cards, a rose for the main action, the
// selection and the focus, gold for the box of a box select.

pub static PLUM: Theme = Theme {
    id: "plum",
    name: "Plum",
    dark: true,
    disabled: 0.6,
    bg: c(0x2a2030),
    panel: c(0x352a3c),
    deep: c(0x221a26),
    well: c(0x1d1622),
    card_edge: c(0x4c4152),
    ruled: NONE,
    line: c(0x4c4251),
    grid: None,
    popup: c(0x3b3041),
    popup_edge: (1.0, c(0x665c6a)),
    popup_shadow: ca(0x000000, 70),
    scrim: ca(0x0d0a0e, 150),
    text: c(0xf2e8f0),
    muted: c(0xb2a8b3),
    hover_text: c(0xe9a7c0),
    link: c(0xe9a7c0),
    focus: c(0xe79bb8),
    focus_block: None,
    ok: c(0x9ad9a0),
    err: c(0xff9c7e),
    warn: c(0xf3c673),
    imp: c(0xe9a7c0),
    tok: c(0xc0b2ff),
    shape: Shape::Outline,
    radius: 6,
    round: 8,
    button: tone(0x423749, 0xf2e8f0, 0x655a6a),
    button_hover: tone(0x594559, 0xf2e8f0, 0xe79bb8),
    button_down: tone(0x6d5166, 0xf2e8f0, 0xe79bb8),
    on: tone(0x7d5b71, 0xfbeef3, 0xe79bb8),
    on_hover: tone(0x886178, 0xfbeef3, 0xe79bb8),
    primary: tone(0xe79bb8, 0x17121a, 0xe79bb8),
    primary_hover: tone(0xebadc5, 0x17121a, 0xebadc5),
    bevel: [c(0x352a3c), c(0x4c4152), c(0x665c6a), c(0x7e7481)],
    button_padding: [8.0, 3.0],
    field: Field::Boxed,
    field_tone: tone(0x1e1723, 0xf6eff4, 0x6d656e),
    field_hint: c(0x9d949e),
    field_mono: false,
    check: c(0xe79bb8),
    sel: tone(0x795469, 0xffffff, 0x795469),
    hover: tone(0x483446, 0xf2e8f0, 0x483446),
    text_sel: (c(0x795469), c(0xffffff)),
    scroll: [c(0x5a505e), c(0x726875), c(0x8e8490)],
    trough: c(0x251c2a),
    meter: [c(0x1d1622), c(0xe79bb8), c(0xff9c7e)],
    bar: Bar::Soft,
    bar_fill: c(0x594155),
    bar_lines: (c(0x4c4251), c(0x4c4251)),
    bar_text: c(0xedc5d7),
    bar_text_small: c(0xedc5d7),
    logo: Logo::Plain,
    logo_colors: [c(0xedc5d7), c(0xe79bb8), c(0xf3c37b)],
    divider: Divider::Line,
    backdrop: Backdrop::Plain,
    backdrop_colors: [c(0x221a26), c(0x221a26), c(0x221a26)],
    banner: NONE,
    border: Border::Lines(&[(0.0, 1.0)]),
    border_colors: [c(0x665c6a), c(0x665c6a), c(0x665c6a), c(0x665c6a)],
    mark: Mark::Printed,
    marks: [c(0x9ad9a0), c(0xff9c7e), c(0xf3c673), c(0xe79bb8), c(0x988e9a)],
    chip: Tone { fill: ca(0x3b3041, 240), text: c(0xf2e8f0), edge: c(0x665c6a) },
    chip_title: c(0xf2e8f0),
    chip_edited: c(0xf3c673),
    select_box: (c(0xf3c37b), ca(0xf3c37b, 36)),
    view_bg: [0.137, 0.102, 0.145],
    fonts: SANS_FONTS,
};
