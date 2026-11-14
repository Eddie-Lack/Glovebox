//! The welcome on the first start, and the tour of the window.
//!
//! The welcome is a small dialog with two buttons. It shows once, when the first level is open. The tour is a row of
//! steps (`STEPS`). Each step dims the window, leaves one part of it bright (the spot) and shows a bubble with a
//! short text next to it.
//!
//! The spot is the rectangle of a real widget. The toolbar and the panels report where they are in every frame
//! (`State::mark`), and a step names its part with a `Target`. A step can also show what it talks about: it selects
//! something or opens the Add panel, and it shows a panel the player hid for the step that is about it. The tour never
//! edits the level, and `Window::end_tour` puts the selection, the panels, the layout and the camera back.

use crate::app::Window;
use crate::style;
use egui::{Pos2, Rect, Vec2};
use glovebox_editor::camera::{Camera, Orbit};
use glovebox_editor::editor::Editor;
use glovebox_editor::entities::EntId;
use std::collections::BTreeMap;
use std::sync::Arc;

/// The key of the setting that says the welcome was answered, and its value.
pub const SETTING: &str = "tour";
pub const SEEN: &str = "seen";

/// The guide for players.
pub const GUIDE: &str = "https://github.com/Eddie-Lack/Glovebox/tree/main/docs/guide";

/// A part of the window a step can point at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Target {
    /// The level list of the toolbar.
    LevelList,
    /// The 3D view.
    View,
    /// The Level panel on the left.
    LevelPanel,
    /// The buttons Move, Rotate and Scale.
    Tools,
    /// The button "+ Add".
    Add,
    /// The inspector: the right panel over the Checks panel, when its tab shows.
    Inspector,
    /// The Checks panel.
    Checks,
    /// The buttons Test level and Build ROM.
    Build,
    /// The button Export patch.
    Patch,
    /// The buttons Reset level, Undo and Redo.
    Undo,
    /// The Options menu's button.
    Options,
    /// The whole toolbar.
    Toolbar,
    /// The whole right panel.
    RightPanel,
}

impl Target {
    /// The part to light when this one is not in the window.
    fn fallback(self) -> Option<Target> {
        match self {
            Target::Inspector | Target::Checks => Some(Target::RightPanel),
            Target::LevelList | Target::Tools | Target::Add | Target::Build | Target::Patch | Target::Undo | Target::Options => Some(Target::Toolbar),
            Target::View | Target::LevelPanel | Target::Toolbar | Target::RightPanel => None,
        }
    }
}

/// What a step shows in the window while it is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Show {
    /// Nothing selected, the Add panel closed.
    Plain,
    /// One thing near the level's start, selected.
    Selection,
    /// The Add panel, open.
    AddPanel,
}

/// One step of the tour. `{levels}` in a text is the number of levels in the list.
pub struct Step {
    pub title: &'static str,
    pub text: &'static str,
    pub target: Target,
    pub show: Show,
}

pub const STEPS: [Step; 12] = [
    Step { title: "The level list", text: "Pick any of the {levels} levels here. Your edits save themselves, level by level", target: Target::LevelList, show: Show::Plain },
    Step { title: "The view", text: "Drag to turn the camera, right-drag to slide it, wheel to zoom. C flies: W A S D move, drag looks around", target: Target::View, show: Show::Plain },
    Step { title: "Selecting", text: "Click a thing in the view, or in this list. Its numbers show in the Inspector on the right", target: Target::LevelPanel, show: Show::Selection },
    Step { title: "Move, Rotate, Scale", text: "W, E and R pick the tool. Drag its handles in the view, or drag a number in the Inspector sideways", target: Target::Tools, show: Show::Selection },
    Step { title: "+ Add", text: "Collectables, enemies, models from any level and the kit. Pick one and it's in your level. A opens this too", target: Target::Add, show: Show::AddPanel },
    Step { title: "The Inspector", text: "What you selected, on cards: a platform's commands, an enemy's script. Deselect everything for the level's settings and its puzzles", target: Target::Inspector, show: Show::Selection },
    Step { title: "Checks", text: "The game's own rules, checked after every edit. Click a line to see the spot", target: Target::Checks, show: Show::Plain },
    Step { title: "Test level and Build ROM", text: "Test level opens your level in your emulator. Build ROM saves a ROM. With boot here it starts in this level", target: Target::Build, show: Show::Plain },
    Step { title: "Export patch", text: "A patch is how you share a level. Patcher ↗ at the end of the row opens the page players patch their ROM with", target: Target::Patch, show: Show::Plain },
    Step { title: "Undo and Reset level", text: "Undo takes back an edit (Ctrl+Z). Reset level gives you the shipped level back, and Undo still undoes that", target: Target::Undo, show: Show::Plain },
    Step { title: "Options", text: "Themes and the info texts. Take the tour starts this again", target: Target::Options, show: Show::Plain },
    Step { title: "That's it", text: "", target: Target::Inspector, show: Show::Plain },
];

/// The text of the last step: where the notes are, by the Info texts setting.
fn last_text(info: style::Info) -> &'static str {
    match info {
        style::Info::Hover => "A dotted line under a name means it has a note. Rest the pointer on it to read it. The guide has everything else",
        style::Info::Shown => "The small texts under titles and fields are notes. Options can move them to tooltips. The guide has everything else",
        style::Info::Off => "The notes are off. Options brings them back. The guide has everything else",
    }
}

/// The welcome, the tour that runs, and where the tour's targets are in this frame.
#[derive(Default)]
pub struct State {
    /// The welcome is open.
    pub welcome: bool,
    /// The settings were asked whether to show the welcome.
    asked: bool,
    pub run: Option<Run>,
    targets: BTreeMap<Target, Rect>,
}

/// A tour that runs.
pub struct Run {
    pub step: usize,
    /// The step whose `Show` the window has now.
    entered: Option<usize>,
    before: Before,
    /// The thing the steps select.
    demo: Option<EntId>,
    /// The bright part in the last frame. None when the whole window was dimmed.
    pub spot: Option<Rect>,
    /// The bubble in the last frame.
    pub bubble: Rect,
}

/// What the tour changes, as it was before the tour.
struct Before {
    selection: Vec<EntId>,
    puzzle: Option<usize>,
    palette_open: bool,
    layout: crate::layout::Layout,
    front: Vec<crate::layout::PanelId>,
    box_mode: bool,
    fly: bool,
    camera: Camera,
    orbit: Orbit,
}

impl State {
    /// True while the welcome or the tour is over the window. The editor's keys and the view's mouse are off then.
    pub fn active(&self) -> bool {
        self.welcome || self.run.is_some()
    }

    /// Forgets the targets of the last frame.
    pub fn begin_frame(&mut self) {
        self.targets.clear();
    }

    /// Reports where a target is in this frame. Several widgets of one target give the rectangle around them all.
    pub fn mark(&mut self, target: Target, rect: Rect) {
        if rect.is_positive() {
            self.targets.entry(target).and_modify(|r| *r = r.union(rect)).or_insert(rect);
        }
    }

    /// Where a target was reported in this frame.
    pub fn target(&self, target: Target) -> Option<Rect> {
        self.targets.get(&target).copied()
    }

    /// The part of the window to light for a target: the target, or its fallback when it is not in the window.
    pub fn spot(&self, target: Target, window: Rect) -> Option<Rect> {
        let mut next = Some(target);
        while let Some(t) = next {
            if let Some(r) = self.target(t).map(|r| r.intersect(window)).filter(|r| r.width() >= 4.0 && r.height() >= 4.0) {
                return Some(r);
            }
            next = t.fallback();
        }
        None
    }
}

/// Something near the level's start for the steps to select: the nearest enemy or platform, or anything else.
fn demo(ed: &Editor) -> Option<EntId> {
    let from = ed.find(|e| e.kind == "spawn").map_or(ed.orbit.target, |s| ed.obj_pos(s));
    let near = |i: &EntId| {
        let d = glovebox_editor::vec::sub(ed.obj_pos(*i), from);
        (glovebox_editor::vec::norm(d) * 1000.0) as i64
    };
    let all: Vec<EntId> = ed.pickable().into_iter().filter(|&i| !ed.entities[i].is_handle && ed.entities[i].bind.is_some()).collect();
    let best = all.iter().copied().filter(|&i| matches!(ed.entities[i].kind, "enemy" | "platform")).min_by_key(near);
    best.or_else(|| all.iter().copied().min_by_key(near))
}

impl Window {
    /// Starts the tour at its first step. It needs an open level.
    pub fn start_tour(&mut self) {
        if !self.ed.has_level || self.busy || self.tour.run.is_some() {
            return;
        }
        self.tour.welcome = false;
        self.tour.asked = true;
        self.settings.set(SETTING, SEEN);
        let ed = &self.ed;
        let before = Before {
            selection: ed.selection.clone(),
            puzzle: ed.puzzle_index,
            palette_open: self.palette_open,
            layout: self.layout.clone(),
            front: self.front.clone(),
            box_mode: ed.box_mode,
            fly: self.view.fly.on,
            camera: ed.camera.clone(),
            orbit: ed.orbit.clone(),
        };
        if before.fly {
            self.set_fly(false);
            (self.ed.camera, self.ed.orbit) = (before.camera.clone(), before.orbit.clone());
        }
        if before.box_mode {
            self.ed.set_box_mode(false);
        }
        let demo = demo(&self.ed);
        self.tour.run = Some(Run { step: 0, entered: None, before, demo, spot: None, bubble: Rect::NOTHING });
        // A text box that had the keyboard gives it up: the tour's keys are its own
        self.ctx.memory_mut(|m| {
            if let Some(id) = m.focused() {
                m.surrender_focus(id);
            }
        });
        self.ctx.request_repaint();
    }

    /// Ends the tour. The selection, the panels and the camera are as they were before it.
    pub fn end_tour(&mut self) {
        let Some(run) = self.tour.run.take() else { return };
        let b = run.before;
        if self.palette_open != b.palette_open {
            self.toggle_palette(Some(b.palette_open));
        }
        // The tour's own changes to the layout were not written to the settings
        self.layout = b.layout;
        self.front = b.front;
        if self.ed.selection != b.selection {
            self.ed.set_selection(b.selection);
        }
        self.ed.puzzle_index = b.puzzle;
        if b.box_mode {
            self.ed.set_box_mode(true);
        }
        (self.ed.camera, self.ed.orbit) = (b.camera, b.orbit);
        if b.fly {
            self.view.set_fly(&mut self.ed, true);
        }
        self.view.dirty = true;
        self.ctx.request_repaint();
    }

    /// Gives the window what the step shows.
    fn enter_step(&mut self, step: usize) {
        let Some(run) = &self.tour.run else { return };
        let (s, demo) = (&STEPS[step], run.demo.filter(|&d| d < self.ed.entities.len()));
        // The player's layout, with the panel the step is about shown and in front of its tabs
        use crate::layout::PanelId;
        self.layout = run.before.layout.clone();
        let palette = s.show == Show::AddPanel;
        if self.palette_open != palette {
            self.toggle_palette(Some(palette));
        }
        let panel = match s.target {
            Target::Inspector => Some(PanelId::Inspector),
            Target::LevelPanel => Some(PanelId::Level),
            Target::Checks => Some(PanelId::Checks),
            _ => None,
        };
        if let Some(p) = panel {
            self.layout.set_shown(p, true);
            self.bring_front(p);
        }
        let selection: Vec<EntId> = if s.show == Show::Selection { demo.into_iter().collect() } else { Vec::new() };
        if self.ed.selection != selection {
            self.ed.set_selection(selection);
        }
        if let (Show::Selection, Some(d)) = (s.show, demo) {
            self.ed.focus_on(d);
        }
        self.view.dirty = true;
        if let Some(run) = &mut self.tour.run {
            run.entered = Some(step);
        }
    }
}

/// The bubble's width, the least width it can have, the room between it and the spot, and its room to the
/// window's edge.
const WIDTH: f32 = 330.0;
const MIN_WIDTH: f32 = 280.0;
const GAP: f32 = style::POINTER + 3.0;
const EDGE: f32 = 8.0;
/// The bubble's inner margin and the height of its row of buttons.
const PAD: f32 = 12.0;
const ROW: f32 = 28.0;

/// Where the bubble goes: beside `spot`, inside `window`, not over the spot. `size` gives the bubble's size for a
/// width. Also gives the point of the spot the bubble's pointer aims at. Without a spot, or without room on any
/// side of it, the bubble goes to the middle of the window's lower half and has no pointer.
pub fn place(window: Rect, spot: Option<Rect>, size: impl Fn(f32) -> Vec2) -> (Rect, Option<Pos2>) {
    let inner = window.shrink(EDGE);
    let full = WIDTH.min(inner.width());
    if let Some(s) = spot {
        // A tall part has the bubble beside it, a low part under or over it
        let tall = s.height() > window.height() * 0.5;
        let order: [u8; 4] = if tall { [2, 3, 0, 1] } else { [0, 1, 2, 3] };
        for side in order {
            let found = match side {
                // Under and over
                0 | 1 => {
                    let b = size(full);
                    let x = (s.center().x - b.x / 2.0).clamp(inner.min.x, (inner.max.x - b.x).max(inner.min.x));
                    let y = if side == 0 { s.max.y + GAP } else { s.min.y - GAP - b.y };
                    let tip = egui::pos2(s.center().x, if side == 0 { s.max.y } else { s.min.y });
                    Some((Rect::from_min_size(egui::pos2(x, y), b), tip))
                }
                // Right and left
                _ => {
                    let room = if side == 2 { inner.max.x - s.max.x - GAP } else { s.min.x - GAP - inner.min.x };
                    (room >= MIN_WIDTH.min(full)).then(|| {
                        let b = size(full.min(room));
                        let x = if side == 2 { s.max.x + GAP } else { s.min.x - GAP - b.x };
                        let y = (s.center().y - b.y / 2.0).clamp(inner.min.y, (inner.max.y - b.y).max(inner.min.y));
                        let tip = egui::pos2(if side == 2 { s.max.x } else { s.min.x }, s.center().y.clamp(y, y + b.y));
                        (Rect::from_min_size(egui::pos2(x, y), b), tip)
                    })
                }
            };
            if let Some((r, tip)) = found {
                if inner.expand(0.5).contains_rect(r) && !r.intersects(s) {
                    return (r, Some(tip));
                }
            }
        }
    }
    let b = size(full);
    let at = egui::pos2(window.center().x - b.x / 2.0, (window.center().y + window.height() * 0.25 - b.y / 2.0).min(inner.max.y - b.y).max(inner.min.y));
    (Rect::from_min_size(at, b), None)
}

/// What the player asked for in a frame of the tour.
enum Ask {
    Back,
    Next,
    End,
}

/// The welcome and the tour, over everything else of the window.
pub fn ui(w: &mut Window, ctx: &egui::Context) {
    // The first time a level is open, the settings say whether the welcome shows
    if !w.tour.asked && w.ed.has_level && !w.busy && w.dialog.is_none() {
        w.tour.asked = true;
        w.tour.welcome = w.settings.get(SETTING).as_deref() != Some(SEEN);
    }
    if w.tour.welcome {
        welcome(w, ctx);
    } else if w.tour.run.is_some() {
        if w.dialog.is_some() || !w.ed.has_level {
            return w.end_tour();
        }
        tour(w, ctx);
    }
}

fn welcome(w: &mut Window, ctx: &egui::Context) {
    let (enter, esc) = ctx.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
    let mut answer = None;
    let modal = egui::Modal::new(egui::Id::unique("welcome")).frame(egui::Frame::NONE).backdrop_color(style::theme(ctx).scrim).show(ctx, |ui| {
        style::framed(ui, |ui| {
            egui::Frame::new().inner_margin(egui::Margin::same(12)).show(ui, |ui| {
                ui.set_width(380.0f32.min(ctx.content_rect().width() - 60.0));
                crate::dialogs::title_bar(ui, "Welcome to Glovebox");
                ui.label("Is this your first time here? Want a quick tour of the editor?");
                crate::dialogs::buttons(ui, |ui| {
                    if ui.add(style::primary("Show me around")).clicked() || enter {
                        answer = Some(true);
                    }
                    if ui.button("No thanks").clicked() || esc {
                        answer = Some(false);
                    }
                });
            });
        });
    });
    if answer.is_none() && modal.should_close() {
        answer = Some(false);
    }
    if let Some(yes) = answer {
        w.tour.welcome = false;
        w.settings.set(SETTING, SEEN);
        if yes {
            w.start_tour();
        }
        ctx.request_repaint();
    }
}

/// The texts of a step, laid out for a bubble of a width, and the bubble's size.
struct Laid {
    title: Arc<egui::Galley>,
    text: Arc<egui::Galley>,
    link: Option<Arc<egui::Galley>>,
    size: Vec2,
}

const LINK: &str = "Open the guide ↗";

fn lay(ctx: &egui::Context, title: &str, text: &str, link: bool, width: f32) -> Laid {
    let th = style::theme(ctx);
    let wrap = width - 2.0 * PAD;
    let (title, text, link) = ctx.fonts_mut(|f| {
        (
            f.layout(title.to_string(), egui::FontId::new(14.0, style::bold()), th.text, wrap),
            f.layout(text.to_string(), egui::FontId::proportional(13.0), th.text, wrap),
            link.then(|| f.layout(LINK.to_string(), egui::FontId::proportional(13.0), th.link, wrap)),
        )
    });
    let link_height = link.as_ref().map_or(0.0, |l| l.size().y + 6.0);
    let height = PAD + title.size().y + 6.0 + text.size().y + link_height + 10.0 + ROW + PAD;
    Laid { title, text, link, size: egui::vec2(width, height.ceil()) }
}

fn tour(w: &mut Window, ctx: &egui::Context) {
    let Some((step, entered)) = w.tour.run.as_ref().map(|r| (r.step.min(STEPS.len() - 1), r.entered)) else { return };
    let window = ctx.content_rect();
    let area = egui::Area::new(egui::Id::unique("tour")).kind(egui::UiKind::Modal).sense(egui::Sense::hover()).order(egui::Order::Foreground).interactable(true).fixed_pos(window.min);
    if entered != Some(step) {
        // The window changes for the step now. Its parts are in their new places in the next frame.
        w.enter_step(step);
        ctx.request_repaint();
    }
    let s = &STEPS[step];
    let last = step + 1 == STEPS.len();
    let levels = w.ed.scenes.len().to_string();
    let text = if last { last_text(style::info(ctx)).to_string() } else { s.text.replace("{levels}", &levels) };
    let spot = w.tour.spot(s.target, window);
    let (bubble, tip) = place(window, spot, |width| lay(ctx, s.title, &text, last, width).size);
    let laid = lay(ctx, s.title, &text, last, bubble.width());
    let mut ask = None;
    egui::Modal::new(egui::Id::unique("tour")).area(area).frame(egui::Frame::NONE).backdrop_color(egui::Color32::TRANSPARENT).show(ctx, |ui| {
        let th = style::theme(ctx);
        ui.allocate_rect(window, egui::Sense::hover());
        style::spotlight(ui.painter(), window, spot);
        style::callout(ui.painter(), bubble, tip);
        let inner = bubble.shrink(PAD);
        let mut y = inner.min.y;
        let mut put = |g: &Arc<egui::Galley>, gap: f32| {
            let r = Rect::from_min_size(egui::pos2(inner.min.x, y), g.size());
            y += g.size().y + gap;
            r
        };
        let r = put(&laid.title, 6.0);
        ui.put(r, egui::Label::new(laid.title.clone()).selectable(false));
        let r = put(&laid.text, 6.0);
        ui.put(r, egui::Label::new(laid.text.clone()).selectable(false));
        if let Some(link) = &laid.link {
            let r = put(link, 0.0);
            let resp = ui.put(r, egui::Label::new(link.clone()).selectable(false).sense(egui::Sense::click()));
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Link, true, LINK));
            if resp.hovered() {
                ui.painter().hline(r.x_range(), r.max.y - 1.0, egui::Stroke::new(1.0, th.link));
            }
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(GUIDE).clicked() {
                ctx.open_url(egui::OpenUrl::new_tab(GUIDE));
            }
        }
        let row = Rect::from_min_max(egui::pos2(inner.min.x, inner.max.y - ROW), inner.max);
        ui.scope_builder(egui::UiBuilder::new().max_rect(row).layout(egui::Layout::right_to_left(egui::Align::Center)), |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if ui.add(style::primary(if last { "Done" } else { "Next" })).clicked() {
                ask = Some(if last { Ask::End } else { Ask::Next });
            }
            if ui.add_enabled(step > 0, egui::Button::new("Back")).clicked() {
                ask = Some(Ask::Back);
            }
            if !last && ui.button("Skip").on_hover_text("End the tour (Esc)").clicked() {
                ask = Some(Ask::End);
            }
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.label(egui::RichText::new(format!("{} of {}", step + 1, STEPS.len())).size(12.0).color(th.muted));
            });
        });
    });
    // The keys, unless a button was pressed in this frame: Enter on a button with the focus is its click
    if ask.is_none() {
        use egui::{Key, Modifiers};
        let pressed = |k: Key| ctx.input_mut(|i| i.consume_key(Modifiers::NONE, k));
        if pressed(Key::Escape) {
            ask = Some(Ask::End);
        } else if pressed(Key::ArrowRight) | pressed(Key::Enter) {
            ask = Some(if last { Ask::End } else { Ask::Next });
        } else if pressed(Key::ArrowLeft) && step > 0 {
            ask = Some(Ask::Back);
        }
    }
    if let Some(run) = &mut w.tour.run {
        (run.spot, run.bubble) = (spot, bubble);
        match ask {
            Some(Ask::Next) => run.step = step + 1,
            Some(Ask::Back) => run.step = step - 1,
            Some(Ask::End) | None => {}
        }
    }
    if matches!(ask, Some(Ask::End)) {
        w.end_tour();
    }
    if ask.is_some() {
        ctx.request_repaint();
    }
}
