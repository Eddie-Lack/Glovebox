//! The themes: each look of the app as data.
//!
//! A `Theme` holds the colours, the shapes of the controls, the fonts and the small details of one look. `style.rs`
//! draws everything from the active theme. No other file holds a colour of the window.
//!
//! Each theme follows one model from the world. "1998" is the patcher page (`web/patcher.html`): a game web site
//! of that year. "Cartridge" is a grey console and its controller. "Blueprint" is a drafting sheet. "Paper" is a
//! printed manual in two inks. "CRT" is a debug monitor with green phosphor.
//!
//! To add a theme: write a `Theme` here, add it to `THEMES`, and give `style.rs` a drawing for any new variant of
//! the enums below. docs/technical.md, "Themes", lists what a theme has to keep.

use egui::Color32;

/// A colour from its hex number.
const fn c(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// A see-through colour: the hex number and the alpha.
const fn ca(v: u32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied_const((v >> 16) as u8, (v >> 8) as u8, v as u8, a)
}

const NONE: Color32 = Color32::TRANSPARENT;

/// A surface with a text on it and a line round it.
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
    /// A moulded key: a line round it and a thick lip under it. The lip goes while it is held or on.
    Key,
    /// A hairline round a flat face.
    Outline,
}

/// How a text box is built.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Field {
    /// A sunken box.
    Sunken,
    /// A line round the box.
    Boxed,
    /// A tint with a line under it, as a form on paper.
    Underlined,
}

/// How a title bar is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Bar {
    /// A filled bar with a light line over it and a dark line under it.
    Solid,
    /// A dark label plate. A panel's plate has a coloured end.
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
    /// Capitals on a dark sticker with a coloured stripe under them.
    Label,
    /// Spaced capitals in a title block with corner marks.
    TitleBlock,
    /// The name with a coloured rule under it, as the head of a manual.
    Masthead,
    /// A prompt, the name in capitals and a block cursor.
    Prompt,
}

/// The line under a dialog's title and under the setup screen's logo.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Divider {
    Rainbow,
    /// Four blocks in the four mark colours.
    Stripes,
    /// A ruler: a line with short and long ticks.
    Scale,
    /// A heavy rule and a hairline.
    DoubleRule,
    /// Two hairlines.
    DoubleLine,
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
    /// A small bevelled block.
    Block,
    /// A round lamp.
    Lamp,
    /// A drawn sign for each level: circle, triangle, crossed square, diamond.
    Drawn,
    /// A printed sign for each level: tick, triangle, square, dot.
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

    // Surfaces
    /// The panels.
    pub bg: Color32,
    /// Cards and the lines of the Checks panel.
    pub panel: Color32,
    /// The toolbar, the status bar and the tab strip.
    pub deep: Color32,
    /// Wells: the pictures of the palette.
    pub well: Color32,
    /// The line round a card. `NONE` for no line.
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
    /// The keyboard focus fills the button, with this text colour.
    pub focus_block: Option<Color32>,
    /// Good, bad, warning, important and token texts.
    pub ok: Color32,
    pub err: Color32,
    pub warn: Color32,
    pub imp: Color32,
    pub tok: Color32,

    // Controls
    pub shape: Shape,
    pub radius: u8,
    pub button: Tone,
    pub button_hover: Tone,
    pub button_down: Tone,
    /// A toolbar switch or a tab that is on.
    pub on: Tone,
    pub on_hover: Tone,
    /// The main action.
    pub primary: Tone,
    pub primary_hover: Tone,
    /// The four tones of a bevel, from the lightest: wells, checkboxes and bevelled buttons.
    pub bevel: [Color32; 4],
    pub button_padding: [f32; 2],
    pub field: Field,
    pub field_tone: Tone,
    pub field_hint: Color32,
    /// Typed text is in the code font.
    pub field_mono: bool,
    /// A checkbox that is on, when it has no colour of its own.
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
    /// The logo's colours: the letters, then two more.
    pub logo_colors: [Color32; 3],
    pub divider: Divider,
    pub backdrop: Backdrop,
    /// The backdrop's ground and its two line colours.
    pub backdrop_colors: [Color32; 3],
    /// The band behind the setup screen's logo.
    pub banner: Color32,
    pub border: Border,
    /// The frame's colours, from the lightest.
    pub border_colors: [Color32; 4],
    pub mark: Mark,
    /// The marks of ok, bad, warning, important and anything else.
    pub marks: [Color32; 5],
    /// The chips over the 3D view: the fill, the edge, the text, the level's name and the colour of "edited".
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

/// The colours of the rainbow divider, from left to right.
pub const RAINBOW: [Color32; 7] = [c(0xff0000), c(0xff8000), c(0xffff00), c(0x00c000), c(0x0080ff), c(0x8000ff), c(0xff00ff)];

/// The themes. The first is the default.
pub static THEMES: [&Theme; 5] = [&Y1998, &CARTRIDGE, &BLUEPRINT, &PAPER, &CRT];

/// The theme with an id. An unknown id gives the default.
pub fn by_id(id: &str) -> &'static Theme {
    THEMES.iter().copied().find(|t| t.id == id.trim()).unwrap_or(THEMES[0])
}

// ---------------------------------------------------------------------------------------------------------------
// 1998: the patcher page. Navy panels, grey bevelled buttons, sunken white fields, red title bars with yellow
// capitals, a slanted yellow logo, a rainbow divider and stars.

pub static Y1998: Theme = Theme {
    id: "1998",
    name: "1998",
    dark: true,
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
// Cartridge: a grey console of the late 1990s and its controller. Warm grey moulded plastic, charcoal label
// plates with white capitals, keys with a lip. The four colours of the face buttons mark states only: green for
// on and good, red for the main action and bad, yellow for warnings, blue for the selection.

pub static CARTRIDGE: Theme = Theme {
    id: "cartridge",
    name: "Cartridge",
    dark: false,
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
// Paper: a printed manual in two inks. Off-white paper, black ink, one spot colour (vermilion). A serif face.
// Rules instead of boxes, fields as tinted blanks with a line under them and typed entries, printed signs for
// the checks, a notebook page behind the setup screen.

pub static PAPER: Theme = Theme {
    id: "paper",
    name: "Paper",
    dark: false,
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
    button: tone(0xfbf8f0, 0x1b1a18, 0x1b1a18),
    button_hover: tone(0xe0d9c7, 0x1b1a18, 0x1b1a18),
    button_down: tone(0x1b1a18, 0xfbf8f0, 0x1b1a18),
    on: tone(0x1b1a18, 0xfbf8f0, 0x1b1a18),
    on_hover: tone(0x3a372f, 0xfbf8f0, 0x1b1a18),
    primary: tone(0xc2321b, 0xfffaf0, 0x7d1c0c),
    primary_hover: tone(0xd4442c, 0xfffaf0, 0x7d1c0c),
    bevel: [c(0x1b1a18), c(0xa8a294), c(0xa8a294), c(0x1b1a18)],
    button_padding: [8.0, 3.0],
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
