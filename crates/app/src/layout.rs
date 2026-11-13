//! The layout of the window: which panels show, on which side of the view each one is, and how the panels of one
//! side share it.
//!
//! The view is in the middle and takes the room the panels leave. Each side of it has at most one column. A
//! column holds the panels of its side, tabbed (a row of tabs, one panel shows) or stacked (one under the other,
//! with a divider to drag). The Checks panel is never a tab: it is the lowest part of its column, or a strip along
//! the bottom of the window under the view.
//!
//! `Add to level` and `Path` show while their tool is on. In a tabbed column of a wide window they get a column of
//! their own beside the others, as long as the view keeps `MIN_VIEW` points.
//!
//! The layout is the setting `panels`, the sizes the player dragged are `panel_sizes`.

use crate::app::Window;
use crate::tour::Target;
use crate::{checks, inspector, outliner, palette, path_panel, style};
use std::collections::BTreeMap;

/// A panel of the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PanelId {
    Level,
    Inspector,
    Checks,
    Add,
    Path,
}

impl PanelId {
    pub const ALL: [PanelId; 5] = [PanelId::Level, PanelId::Inspector, PanelId::Checks, PanelId::Add, PanelId::Path];

    /// The panel's title.
    pub fn name(self) -> &'static str {
        ["Level", "Inspector", "Checks", "Add to level", "Path"][self as usize]
    }

    /// The panel's name in the settings.
    pub fn key(self) -> &'static str {
        ["level", "inspector", "checks", "add", "path"][self as usize]
    }

    /// True for the panels that show while their tool is on.
    pub fn is_tool(self) -> bool {
        matches!(self, PanelId::Add | PanelId::Path)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
    /// Under the view. Only the Checks panel can be there.
    Bottom,
}

impl Side {
    pub fn key(self) -> &'static str {
        match self {
            Side::Left => "left",
            Side::Right => "right",
            Side::Bottom => "bottom",
        }
    }

    fn from_key(key: &str) -> Option<Side> {
        [Side::Left, Side::Right, Side::Bottom].into_iter().find(|s| s.key() == key)
    }
}

/// How the panels of one side share it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Stacked,
    Tabbed,
}

impl Mode {
    pub fn key(self) -> &'static str {
        match self {
            Mode::Stacked => "stacked",
            Mode::Tabbed => "tabbed",
        }
    }
}

/// The key of the setting with the layout, and of the one with the sizes.
pub const SETTING: &str = "panels";
pub const SIZES_SETTING: &str = "panel_sizes";

/// The least width the view keeps when a tabbed side gives `Add to level` and `Path` a column of their own.
pub const MIN_VIEW: f32 = 480.0;
/// The least width of the window for that column.
pub const WIDE: f32 = 1560.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    /// The side of each panel, by `PanelId`.
    side: [Side; 5],
    /// Whether each panel shows. `Add to level` and `Path` follow their tools and are not kept here.
    shown: [bool; 5],
    /// How the left and the right side hold their panels.
    mode: [Mode; 2],
}

impl Default for Layout {
    /// The level list on the left. On the right the inspector, with `Add to level` and `Path` as tabs beside it,
    /// over the checks.
    fn default() -> Self {
        Layout { side: [Side::Left, Side::Right, Side::Right, Side::Right, Side::Right], shown: [true; 5], mode: [Mode::Tabbed; 2] }
    }
}

impl Layout {
    pub fn side(&self, p: PanelId) -> Side {
        self.side[p as usize]
    }

    /// Whether the layout shows a panel. Always true for the two tool panels.
    pub fn shown(&self, p: PanelId) -> bool {
        p.is_tool() || self.shown[p as usize]
    }

    /// `side` is the left or the right side.
    pub fn mode(&self, side: Side) -> Mode {
        self.mode[(side == Side::Right) as usize]
    }

    pub fn set_side(&mut self, p: PanelId, side: Side) {
        if side != Side::Bottom || p == PanelId::Checks {
            self.side[p as usize] = side;
        }
    }

    pub fn set_shown(&mut self, p: PanelId, on: bool) {
        if !p.is_tool() {
            self.shown[p as usize] = on;
        }
    }

    pub fn set_mode(&mut self, side: Side, mode: Mode) {
        if side != Side::Bottom {
            self.mode[(side == Side::Right) as usize] = mode;
        }
    }

    /// The layout as the setting's text: `level:left:on,inspector:right:on,checks:right:on,add:right,path:right,`
    /// `left:tabbed,right:tabbed`.
    pub fn to_text(&self) -> String {
        let mut parts = Vec::new();
        for p in PanelId::ALL {
            let shown = if p.is_tool() { "" } else if self.shown(p) { ":on" } else { ":off" };
            parts.push(format!("{}:{}{shown}", p.key(), self.side(p).key()));
        }
        for side in [Side::Left, Side::Right] {
            parts.push(format!("{}:{}", side.key(), self.mode(side).key()));
        }
        parts.join(",")
    }

    /// Reads the setting's text. None when a part is missing, unknown or there twice.
    pub fn from_text(text: &str) -> Option<Layout> {
        let mut out = Layout::default();
        let mut seen = Vec::new();
        for part in text.split(',') {
            let mut words = part.trim().split(':');
            let (name, value, extra) = (words.next()?, words.next()?, words.next());
            if words.next().is_some() || seen.contains(&name) {
                return None;
            }
            seen.push(name);
            if let Some(p) = PanelId::ALL.into_iter().find(|p| p.key() == name) {
                let side = Side::from_key(value)?;
                if side == Side::Bottom && p != PanelId::Checks {
                    return None;
                }
                out.side[p as usize] = side;
                match (p.is_tool(), extra) {
                    (true, None) => {}
                    (false, Some("on")) => out.shown[p as usize] = true,
                    (false, Some("off")) => out.shown[p as usize] = false,
                    _ => return None,
                }
            } else {
                let side = Side::from_key(name).filter(|s| *s != Side::Bottom)?;
                let mode = [Mode::Stacked, Mode::Tabbed].into_iter().find(|m| m.key() == value)?;
                if extra.is_some() {
                    return None;
                }
                out.set_mode(side, mode);
            }
        }
        (seen.len() == 7).then_some(out)
    }
}

/// The sizes of the setting `panel_sizes`: `name:points` parts with commas between. A part that can't be read is
/// left out.
pub fn sizes_from_text(text: &str) -> BTreeMap<String, f32> {
    let mut out = BTreeMap::new();
    for part in text.split(',') {
        if let Some((name, value)) = part.split_once(':') {
            if let Ok(v) = value.trim().parse::<f32>() {
                if v.is_finite() && (20.0..=4000.0).contains(&v) {
                    out.insert(name.trim().to_string(), v);
                }
            }
        }
    }
    out
}

fn sizes_to_text(sizes: &BTreeMap<String, f32>) -> String {
    sizes.iter().map(|(k, v)| format!("{k}:{v:.0}")).collect::<Vec<_>>().join(",")
}

/// Where the panels were in the last frame. A rectangle is empty when its panel is not in the window.
#[derive(Clone, Copy, Debug)]
pub struct Panels {
    /// The column with the Level panel.
    pub level: egui::Rect,
    /// The column with the inspector.
    pub inspector: egui::Rect,
    /// The Checks panel.
    pub checks: egui::Rect,
    /// The column `Add to level` and `Path` have for themselves.
    pub add: egui::Rect,
    /// The part of the window each panel shows in, by `PanelId`. Empty for a panel behind another tab.
    pub slots: [egui::Rect; 5],
}

impl Default for Panels {
    fn default() -> Self {
        let no = egui::Rect::NOTHING;
        Panels { level: no, inspector: no, checks: no, add: no, slots: [no; 5] }
    }
}

impl Panels {
    pub fn slot(&self, p: PanelId) -> egui::Rect {
        self.slots[p as usize]
    }
}

/// The widths the columns start with, in points.
pub const LEVEL_WIDTH: f32 = 270.0;
pub const INSPECTOR_WIDTH: f32 = 380.0;
pub const ADD_WIDTH: f32 = 360.0;

/// One column of panels beside the view.
struct Column {
    side: Side,
    /// The column of the tool panels beside the side's other panels.
    tools: bool,
    /// The panels of the column other than the checks.
    group: Vec<PanelId>,
    checks: bool,
    mode: Mode,
}

impl Column {
    /// The column's name in `panel_sizes` and in its id: the name of the panel in it that needs the most room.
    /// A width the player dragged then stays with that panel when it changes sides.
    fn name(&self) -> &'static str {
        let lead = [PanelId::Inspector, PanelId::Add, PanelId::Path, PanelId::Level].into_iter().find(|p| self.group.contains(p));
        lead.map_or("checks-column", |p| p.key())
    }

    /// The width the column starts with, and its least and largest width. They are those of the panel in it that
    /// needs the most room.
    fn widths(&self) -> (f32, f32, f32) {
        if self.group.contains(&PanelId::Inspector) {
            (INSPECTOR_WIDTH, 300.0, 560.0)
        } else if self.group.iter().any(|p| p.is_tool()) || self.group.is_empty() {
            (ADD_WIDTH, 300.0, 520.0)
        } else {
            (LEVEL_WIDTH, 200.0, 420.0)
        }
    }
}

fn padded() -> egui::Frame {
    egui::Frame::new().inner_margin(egui::Margin { left: 8, right: 4, top: 4, bottom: 4 })
}

impl Window {
    /// True when a panel is in the window now: the layout shows it, or its tool is on.
    pub fn present(&self, p: PanelId) -> bool {
        match p {
            PanelId::Add => self.palette_open,
            PanelId::Path => self.ed.path.on,
            _ => self.layout.shown(p),
        }
    }

    /// Changes the layout and remembers it.
    pub fn set_layout(&mut self, layout: Layout) {
        self.settings.set(SETTING, &layout.to_text());
        self.layout = layout;
        self.view.dirty = true;
        self.ctx.request_repaint();
    }

    /// Shows or hides a panel. `Add to level` and `Path` follow their tools: showing one turns its tool on.
    pub fn show_panel(&mut self, p: PanelId, on: bool) {
        match p {
            PanelId::Add => self.toggle_palette(Some(on)),
            PanelId::Path => self.set_path_mode(on),
            _ => {
                let mut l = self.layout.clone();
                l.set_shown(p, on);
                self.set_layout(l);
                if on {
                    self.bring_front(p);
                }
            }
        }
    }

    /// Puts a panel on a side of the view. Only the checks can go to the bottom.
    pub fn move_panel(&mut self, p: PanelId, side: Side) {
        let mut l = self.layout.clone();
        l.set_side(p, side);
        self.set_layout(l);
        self.bring_front(p);
    }

    pub fn set_side_mode(&mut self, side: Side, mode: Mode) {
        let mut l = self.layout.clone();
        l.set_mode(side, mode);
        self.set_layout(l);
    }

    /// The layout and the sizes the window starts with on a new user folder.
    pub fn reset_layout(&mut self) {
        self.settings.remove(SIZES_SETTING);
        self.sizes.clear();
        self.sizes_seen.clear();
        // The panels get new ids: egui forgets the sizes it kept for the old ones
        self.layout_gen += 1;
        self.set_layout(Layout::default());
        self.set_status("The panels are back in their places.", "ok");
    }

    /// Makes a panel the one that shows among the tabs it shares a column with.
    pub fn bring_front(&mut self, p: PanelId) {
        self.front.retain(|x| *x != p);
        self.front.insert(0, p);
    }

    /// The size the player gave a part, when there is one.
    fn size(&self, name: &str) -> Option<f32> {
        self.sizes.get(name).copied()
    }

    /// Notes the size a part has now. A size that changed while a mouse button was down is a drag on the part's
    /// edge: it is remembered.
    fn keep_size(&mut self, name: &str, now: f32) {
        let before = self.sizes_seen.insert(name.to_string(), now);
        let pressed = self.ctx.input(|i| i.pointer.any_down() || i.pointer.any_released());
        if pressed && before.is_some_and(|b| (b - now).abs() > 0.5) {
            self.sizes.insert(name.to_string(), now);
            self.sizes_dirty = true;
        }
    }

    /// The columns to show: for each side its panels, and a column for the tool panels where there is room.
    fn columns(&self, window: f32) -> Vec<Column> {
        let mut cols = Vec::new();
        for side in [Side::Left, Side::Right] {
            let group: Vec<PanelId> = [PanelId::Level, PanelId::Inspector, PanelId::Add, PanelId::Path].into_iter().filter(|p| self.layout.side(*p) == side && self.present(*p)).collect();
            let checks = self.present(PanelId::Checks) && self.layout.side(PanelId::Checks) == side;
            let mode = self.layout.mode(side);
            let (tools, rest): (Vec<PanelId>, Vec<PanelId>) = group.iter().partition(|p| p.is_tool());
            if mode == Mode::Tabbed && window >= WIDE && !tools.is_empty() && !rest.is_empty() {
                cols.push(Column { side, tools: false, group: rest, checks, mode });
                cols.push(Column { side, tools: true, group: tools, checks: false, mode });
            } else if !group.is_empty() || checks {
                cols.push(Column { side, tools: false, group, checks, mode });
            }
        }
        // The view keeps its least width: the tool panels go back among the tabs, on the right side first
        let width = |cols: &[Column]| cols.iter().map(|c| self.size(c.name()).unwrap_or(c.widths().0)).sum::<f32>();
        while window - width(&cols) < MIN_VIEW {
            let Some(i) = cols.iter().rposition(|c| c.tools) else { break };
            let tools = cols.remove(i);
            cols[i - 1].group.extend(tools.group);
        }
        cols
    }

    /// Shows the panels round the view. The view takes the room they leave.
    pub(crate) fn side_panels(&mut self, ui: &mut egui::Ui) {
        self.panels = Panels::default();
        let th = style::theme(ui.ctx());
        let bare = egui::Frame::new().fill(th.bg);
        for col in self.columns(ui.available_width()) {
            let (default, least, most) = col.widths();
            let name = col.name();
            let id = (name, self.layout_gen);
            let panel = if col.side == Side::Left { egui::Panel::left(id) } else { egui::Panel::right(id) };
            // Each panel's contents go through `style::fixed`: a panel that grows to fit its contents remembers
            // the larger size, and one wide row then holds the panel at its largest size against every drag.
            let out = panel.frame(bare).default_size(self.size(name).unwrap_or(default)).size_range(least..=most).resizable(true).show(ui, |ui| {
                style::fixed(ui, |ui| {
                    style::ground(ui);
                    match col.mode {
                        _ if col.group.is_empty() => self.checks_part(ui),
                        Mode::Tabbed => self.tabbed(ui, &col),
                        Mode::Stacked => self.stacked(ui, &col),
                    }
                });
            });
            let rect = out.response.rect;
            self.keep_size(name, rect.width());
            if col.group.contains(&PanelId::Level) {
                self.panels.level = rect;
            }
            if col.group.contains(&PanelId::Inspector) {
                self.panels.inspector = rect;
                self.tour.mark(Target::RightPanel, rect);
            } else if col.checks && !self.present(PanelId::Inspector) {
                self.tour.mark(Target::RightPanel, rect);
            }
            if col.group.iter().all(|p| p.is_tool()) && !col.group.is_empty() {
                self.panels.add = rect;
            }
            if col.group.is_empty() {
                self.panels.checks = rect;
                self.panels.slots[PanelId::Checks as usize] = rect;
                self.tour.mark(Target::Checks, rect);
            }
        }
        if self.present(PanelId::Checks) && self.layout.side(PanelId::Checks) == Side::Bottom {
            let total = ui.available_height();
            let default = self.size("checks-bottom").unwrap_or(200.0);
            let out = egui::Panel::bottom(("checks-bottom", self.layout_gen)).frame(bare).default_size(default).size_range(100.0..=(total - 200.0).max(100.0)).resizable(true).show(ui, |ui| {
                style::fixed(ui, |ui| {
                    style::ground(ui);
                    self.checks_part(ui);
                });
            });
            let rect = out.response.rect;
            self.keep_size("checks-bottom", rect.height());
            self.panels.checks = rect;
            self.panels.slots[PanelId::Checks as usize] = rect;
            self.tour.mark(Target::Checks, rect);
        }
        // The sizes are written once a drag is over
        if self.sizes_dirty && !ui.input(|i| i.pointer.any_down()) {
            self.sizes_dirty = false;
            self.settings.set(SIZES_SETTING, &sizes_to_text(&self.sizes));
        }
    }

    /// The Checks panel under the other panels of a column.
    fn checks_under(&mut self, ui: &mut egui::Ui) {
        let total = ui.available_height();
        let default = (total * 0.36).clamp(180.0, 320.0);
        let start = self.size("checks").unwrap_or(default);
        let out = egui::Panel::bottom(("checks", self.layout_gen)).frame(egui::Frame::new()).default_size(start).size_range(120.0..=(total - 160.0).max(120.0)).resizable(true).show(ui, |ui| {
            style::fixed(ui, |ui| self.checks_part(ui));
        });
        let rect = out.response.rect;
        self.keep_size("checks", rect.height());
        self.panels.checks = rect;
        self.panels.slots[PanelId::Checks as usize] = rect;
        self.tour.mark(Target::Checks, rect);
    }

    /// The checks with their menu, in the room the panel has.
    fn checks_part(&mut self, ui: &mut egui::Ui) {
        let room = ui.available_rect_before_wrap();
        padded().show(ui, |ui| style::fixed(ui, |ui| checks::ui(self, ui)));
        // The menu's button is on the heading's line, beside the scroll bar
        let at = egui::Rect::from_min_max(egui::pos2(room.right() - 60.0, room.top() + 5.0), egui::pos2(room.right() - 18.0, room.top() + 23.0));
        if self.ed.has_level {
            ui.scope_builder(egui::UiBuilder::new().max_rect(at).layout(egui::Layout::right_to_left(egui::Align::Center)), |ui| panel_menu(self, ui, PanelId::Checks, false));
        }
    }

    /// A column whose panels are tabs: the tabs, the panel in front, and the checks under it.
    fn tabbed(&mut self, ui: &mut egui::Ui, col: &Column) {
        let top = ui.available_rect_before_wrap();
        let active = self.front.iter().copied().find(|p| col.group.contains(p)).unwrap_or(col.group[0]);
        if col.group.len() > 1 || active.is_tool() {
            tab_strip(self, ui, &col.group, active);
        } else {
            title(self, ui, active);
        }
        if col.checks {
            self.checks_under(ui);
        }
        let room = ui.available_rect_before_wrap();
        self.panels.slots[active as usize] = egui::Rect::from_min_max(top.min, room.max);
        match active {
            PanelId::Inspector => self.tour.mark(Target::Inspector, room),
            PanelId::Level => self.tour.mark(Target::LevelPanel, egui::Rect::from_min_max(top.min, room.max)),
            _ => {}
        }
        egui::CentralPanel::default().frame(padded()).show(ui, |ui| style::fixed(ui, |ui| self.body(ui, active)));
    }

    /// A column whose panels are one under the other, each with its title, and the checks under them.
    fn stacked(&mut self, ui: &mut egui::Ui, col: &Column) {
        if col.checks {
            self.checks_under(ui);
        }
        let n = col.group.len();
        for (i, p) in col.group.iter().copied().enumerate() {
            let room = ui.available_rect_before_wrap();
            let left = (n - i) as f32;
            let part = |w: &mut Window, ui: &mut egui::Ui| {
                style::fixed(ui, |ui| {
                    let top = ui.available_rect_before_wrap();
                    title(w, ui, p);
                    let room = ui.available_rect_before_wrap();
                    match p {
                        PanelId::Inspector => w.tour.mark(Target::Inspector, room),
                        PanelId::Level => w.tour.mark(Target::LevelPanel, top),
                        _ => {}
                    }
                    padded().show(ui, |ui| style::fixed(ui, |ui| w.body(ui, p)));
                });
            };
            if i + 1 < n {
                let name = format!("stack-{}", p.key());
                let default = (room.height() / left).max(90.0);
                let most = (room.height() - 90.0 * (left - 1.0)).max(90.0);
                let out = egui::Panel::top((name.as_str(), self.layout_gen)).frame(egui::Frame::new()).default_size(self.size(&name).unwrap_or(default)).size_range(90.0..=most).resizable(true).show(ui, |ui| part(self, ui));
                self.keep_size(&name, out.response.rect.height());
                self.panels.slots[p as usize] = out.response.rect;
            } else {
                self.panels.slots[p as usize] = room;
                egui::CentralPanel::default().frame(egui::Frame::new()).show(ui, |ui| part(self, ui));
            }
        }
    }

    fn body(&mut self, ui: &mut egui::Ui, p: PanelId) {
        match p {
            PanelId::Level => outliner::ui(self, ui),
            PanelId::Inspector => inspector::ui(self, ui),
            PanelId::Checks => checks::ui(self, ui),
            PanelId::Add => palette::ui(self, ui),
            PanelId::Path => path_panel::ui(self, ui),
        }
    }
}

/// A panel's title bar with its menu.
fn title(w: &mut Window, ui: &mut egui::Ui, p: PanelId) {
    style::title_bar_with(ui, p.name(), |ui| panel_menu(w, ui, p, true));
}

/// The tabs of a column, and the menu of the panel in front.
fn tab_strip(w: &mut Window, ui: &mut egui::Ui, group: &[PanelId], active: PanelId) {
    egui::Frame::new().fill(style::theme(ui.ctx()).deep).inner_margin(egui::Margin::symmetric(6, 3)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            for p in group {
                if style::tab(ui, *p == active, p.name()).clicked() {
                    w.bring_front(*p);
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| panel_menu(w, ui, active, false));
        });
    });
}

/// The small menu of a panel: the sides it can move to, and Hide. `on_bar` is for a button on a title bar: it has
/// the bar's letter colour and no frame.
fn panel_menu(w: &mut Window, ui: &mut egui::Ui, p: PanelId, on_bar: bool) {
    let side = w.layout.side(p);
    let mut ask: Option<Option<Side>> = None;
    let (label, tip) = (format!("{} panel menu", p.name()), "Move or hide this panel");
    let entries = |ui: &mut egui::Ui| {
        let entry = |ui: &mut egui::Ui, text: &str| ui.add(egui::Button::selectable(false, text)).clicked();
        for (to, text) in [(Side::Left, "Move to left"), (Side::Right, "Move to right"), (Side::Bottom, "Move to bottom")] {
            if to != side && (to != Side::Bottom || p == PanelId::Checks) && entry(ui, text) {
                ask = Some(Some(to));
            }
        }
        if entry(ui, "Hide") {
            ask = Some(None);
        }
    };
    if on_bar {
        style::bar_menu(ui, "▾", &label, tip, entries);
    } else {
        style::mini_menu(ui, "▾", &label, tip, entries);
    }
    match ask {
        Some(Some(to)) => w.acts.push(Box::new(move |w: &mut Window| w.move_panel(p, to))),
        Some(None) => w.acts.push(Box::new(move |w: &mut Window| w.show_panel(p, false))),
        None => {}
    }
}

/// The Panels part of the Options menu: a tick and a side for each panel, how each side holds its panels, and
/// `Reset layout`. The menu stays open while its entries are clicked.
pub fn menu(w: &mut Window, ui: &mut egui::Ui) {
    ui.set_width(250.0);
    let mut act: Option<crate::app::Act> = None;
    for p in PanelId::ALL {
        ui.horizontal(|ui| {
            let mut on = w.present(p);
            let r = style::checkbox(ui, &mut on, p.name());
            let label = format!("Show {}", p.name());
            r.widget_info(move || egui::WidgetInfo::selected(egui::Role::CheckBox, true, on, &label));
            if r.changed() {
                act = Some(Box::new(move |w: &mut Window| w.show_panel(p, on)));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let sides: &[(Side, &str, &str)] = &[(Side::Bottom, "Bottom", "at the bottom"), (Side::Right, "Right", "on the right"), (Side::Left, "Left", "on the left")];
                for (side, text, place) in sides {
                    if *side == Side::Bottom && p != PanelId::Checks {
                        continue;
                    }
                    let chosen = w.layout.side(p) == *side;
                    let r = ui.selectable_label(chosen, *text);
                    let label = format!("{} {place}", p.name());
                    r.widget_info(move || egui::WidgetInfo::selected(egui::Role::Button, true, chosen, &label));
                    if r.clicked() {
                        let side = *side;
                        act = Some(Box::new(move |w: &mut Window| w.move_panel(p, side)));
                    }
                }
            });
        });
    }
    ui.add_space(4.0);
    style::h4(ui, "Panels on one side").on_hover_text("Stacked: one under the other, with a divider to drag. Tabbed: a row of tabs, one panel shows. The checks are never a tab");
    for (side, name) in [(Side::Left, "Left side"), (Side::Right, "Right side")] {
        ui.horizontal(|ui| {
            ui.label(name);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                for (mode, text) in [(Mode::Tabbed, "Tabbed"), (Mode::Stacked, "Stacked")] {
                    let chosen = w.layout.mode(side) == mode;
                    let r = ui.selectable_label(chosen, text);
                    let label = format!("{name} {}", text.to_lowercase());
                    r.widget_info(move || egui::WidgetInfo::selected(egui::Role::Button, true, chosen, &label));
                    if r.clicked() {
                        act = Some(Box::new(move |w: &mut Window| w.set_side_mode(side, mode)));
                    }
                }
            });
        });
    }
    ui.separator();
    if ui.add(egui::Button::selectable(false, "Reset layout")).on_hover_text("The panels in the places and sizes the editor starts with").clicked() {
        act = Some(Box::new(|w: &mut Window| w.reset_layout()));
    }
    w.acts.extend(act);
}
