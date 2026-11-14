//! The Add palette: collectables, enemies, models from any level, the kit, platform presets and camera zones.
//! Model pictures are drawn when their row or tile scrolls into view. At most `THUMBS_MAX` are kept: the ones that
//! were in view longest ago go first.
//!
//! The Enemies, Models and Kit tabs have two views. `List` has a row for each thing, with its name, its notes and
//! a button for each way to place it. `Grid` has the pictures only, as square tiles: a click places the thing, in
//! the way the radio buttons over the grid say. The grid lays out the rows of tiles in view and no others.

use crate::app::{Act, Window};
use crate::style::{self, h4, muted};
use egui::RichText;
use glovebox_core::backend::{Backend, ModelRef};
use glovebox_editor::entities::{CAM_PRESETS, WHY_CUSTOM_PLATFORM};
use glovebox_editor::presets::PRESETS;
use glovebox_editor::scene::{Prefab, Texture};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

const NEW_OBJECTS: &str = "New objects appear where the view’s center meets the ground.";

pub const TABS: [&str; 6] = ["Collectables", "Enemies", "Models", "Kit", "Presets", "Camera"];

/// The most model pictures kept at a time.
pub const THUMBS_MAX: usize = 320;
/// The most models read for their pictures at a time. A tile that is still in view asks again.
const LOADING_MAX: usize = 32;
/// The least width of a tile of the grid view, and the room between two tiles.
const TILE: f32 = 80.0;
const GAP: f32 = 4.0;

/// The keys of the settings: the view of the three tabs, and what a click on a tile places.
pub const VIEW_SETTING: &str = "add_view";
pub const MODELS_AS_SETTING: &str = "add_models_as";
pub const KIT_AS_SETTING: &str = "add_kit_as";

/// A way to place a model: its name in the settings, its button, the level's tag and the button's tooltip.
struct As {
    key: &'static str,
    label: &'static str,
    tag: &'static str,
    tip: &'static str,
}

const MODELS_AS: [As; 3] = [
    As { key: "scenery", label: "Scenery", tag: "LandActor", tip: "LandActor: solid scenery with collision" },
    As { key: "background", label: "Background", tag: "BackgroundActor", tip: "BackgroundActor: decoration without collision" },
    As { key: "platform", label: "Platform", tag: "Platform", tip: "Platform: solid, can move, spin and be scaled" },
];
const KIT_AS: [As; 2] = [
    As { key: "background", label: "Background", tag: "BackgroundActor", tip: "BackgroundActor: decoration without collision" },
    As { key: "scenery", label: "Scenery", tag: "LandActor", tip: "LandActor: solid, built from the model’s triangles" },
];

const ENEMIES_NOTE: &str = "A new enemy copies the shortest script that type has in a shipped level, with its patrol/target points moved to the new spot. Bosses are tied to their arenas and may not behave elsewhere.";
const KIT_NOTE: &str = "Props made by the editor, free to share. Background has no collision; Scenery is solid. They are copied into the level’s banks at ROM build, like imported models.";
const NO_MODELS: &str = "No model matches.";
const NO_ENEMY_MODEL: &str = "This enemy has no model.";

#[derive(Default)]
pub struct State {
    pub tab: usize,
    pub query: String,
    pub bank: String,
    /// The pictures of models by object id. None for a model with nothing to draw.
    pub thumbs: HashMap<String, Option<egui::TextureHandle>>,
    /// The models whose picture a shown row asked for.
    pub thumbs_wanted: Vec<String>,
    thumbs_loading: HashSet<String>,
    /// Models read by a worker, waiting for their picture.
    thumbs_ready: Vec<(String, Option<Arc<Prefab>>)>,
    kit: Option<Vec<Value>>,
    /// The settings of each kit item.
    kit_vals: HashMap<String, serde_json::Map<String, Value>>,
    /// The picture each kit item shows, and the settings it was made with.
    kit_images: HashMap<String, (String, Option<egui::TextureHandle>)>,
    kit_loading: HashSet<String>,
    /// When to draw a kit item again after a change to its settings.
    kit_due: HashMap<String, f64>,
    /// The tab drawn in the last frame the palette was shown. None right after it opens.
    pub shown_tab: Option<usize>,
    /// The Enemies, Models and Kit tabs show tiles and not rows.
    pub grid: bool,
    /// What a click on a tile places: an index of `MODELS_AS` and one of `KIT_AS`.
    pub models_as: usize,
    pub kit_as: usize,
    /// The number of columns the grid had in the last frame.
    pub grid_cols: usize,
    /// The kit item whose settings show over the grid.
    pub kit_settings: Option<String>,
    /// The frame each model's picture was last in view, and the number of this frame.
    thumbs_used: HashMap<String, u64>,
    frame: u64,
    /// How many model pictures were drawn since the start.
    pub thumbs_made: usize,
}

impl State {
    /// The settings a kit item's picture was last asked with, and whether it has a picture.
    pub fn kit_picture(&self, item: &str) -> Option<(String, bool)> {
        self.kit_images.get(item).map(|x| (x.0.clone(), x.1.is_some()))
    }

    /// The number of model pictures drawn so far.
    pub fn thumbs_done(&self) -> usize {
        self.thumbs.values().filter(|t| t.is_some()).count()
    }

    /// Reads the view and what a tile places from the settings. An unknown value gives the list and the first way.
    pub fn load(&mut self, settings: &crate::settings::Shared) {
        self.grid = settings.get(VIEW_SETTING).as_deref() == Some("grid");
        let index = |list: &[As], key: &str| settings.get(key).and_then(|v| list.iter().position(|a| a.key == v)).unwrap_or(0);
        self.models_as = index(&MODELS_AS, MODELS_AS_SETTING);
        self.kit_as = index(&KIT_AS, KIT_AS_SETTING);
    }

    fn save(&self, settings: &crate::settings::Shared) {
        settings.set(VIEW_SETTING, if self.grid { "grid" } else { "list" });
        settings.set(MODELS_AS_SETTING, MODELS_AS[self.models_as.min(2)].key);
        settings.set(KIT_AS_SETTING, KIT_AS[self.kit_as.min(1)].key);
    }

    /// The level's tag a click on a Models tile places, and the one a Kit tile places.
    pub fn places(&self) -> (&'static str, &'static str) {
        (MODELS_AS[self.models_as.min(2)].tag, KIT_AS[self.kit_as.min(1)].tag)
    }

    /// Notes that a model's picture is in view in this frame.
    fn touch(&mut self, oid: &str) {
        match self.thumbs_used.get_mut(oid) {
            Some(f) => *f = self.frame,
            None => {
                self.thumbs_used.insert(oid.to_string(), self.frame);
            }
        }
    }
}

/// Reads a model and its textures for a picture, as the editor does for the level.
fn load_prefab(backend: &dyn Backend, r: &ModelRef) -> Option<Arc<Prefab>> {
    let model = backend.model(r).ok()?;
    let mut tex = |id: u32| -> Option<Arc<Texture>> {
        let img = backend.texture(r, id).ok()?;
        Texture::new(img.width, img.height, img.rgba.clone()).map(Arc::new)
    };
    Some(Arc::new(Prefab::from_model(&model, &mut tex)))
}

/// Starts reading the models that were asked for, and draws the pictures of those that were read.
fn work(w: &mut Window, ctx: &egui::Context) {
    let wanted = std::mem::take(&mut w.palette.thumbs_wanted);
    for oid in wanted {
        if w.palette.thumbs.contains_key(&oid) || w.palette.thumbs_loading.len() >= LOADING_MAX || !w.palette.thumbs_loading.insert(oid.clone()) {
            continue;
        }
        let Some(r) = w.ed.model_ref(&oid) else {
            w.palette.thumbs.insert(oid, None);
            continue;
        };
        let backend = w.ed.backend.clone();
        w.side_jobs.run(move || load_prefab(&*backend, &r), move |w: &mut Window, p| w.palette.thumbs_ready.push((oid, p)));
    }
    // A few pictures a frame: the window stays quick
    for _ in 0..3 {
        let Some((oid, prefab)) = w.palette.thumbs_ready.pop() else { break };
        let image = prefab.and_then(|p| w.view.thumbnail(&p));
        let handle = image.map(|img| ctx.load_texture(format!("thumb {oid}"), img, egui::TextureOptions::LINEAR));
        w.palette.thumbs_loading.remove(&oid);
        w.palette.thumbs_made += handle.is_some() as usize;
        w.palette.thumbs.insert(oid, handle);
        ctx.request_repaint();
    }
    // Too many pictures: the ones that are not in view go, the oldest first
    let st = &mut w.palette;
    if st.thumbs.len() > THUMBS_MAX {
        let mut old: Vec<(u64, String)> = st.thumbs.keys().map(|k| (st.thumbs_used.get(k).copied().unwrap_or(0), k.clone())).filter(|x| x.0 < st.frame).collect();
        old.sort();
        let over = st.thumbs.len() - THUMBS_MAX * 3 / 4;
        for (_, oid) in old.into_iter().take(over) {
            st.thumbs.remove(&oid);
            st.thumbs_used.remove(&oid);
        }
    }
    st.frame += 1;
}

/// The 64 by 64 box of a row's picture.
fn thumb_box(ui: &mut egui::Ui, texture: Option<&egui::TextureHandle>) -> egui::Rect {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(64.0, 64.0), egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter().rect_filled(rect, 0.0, style::theme(ui.ctx()).well);
        style::well_edge(ui.painter(), rect);
        if let Some(t) = texture {
            ui.painter().image(t.id(), rect.shrink(2.0), egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
        }
    }
    rect
}

struct Px<'a> {
    w: &'a mut Window,
    acts: Vec<Act>,
    /// The note at the top of the open tab, when it is not drawn there: it is the tooltip of the tab's button.
    intro: Option<&'static str>,
}

impl Px<'_> {
    /// The note at the top of a tab.
    fn intro(&mut self, ui: &mut egui::Ui, text: &'static str) {
        if style::info(ui.ctx()) == style::Info::Shown {
            style::para(ui, text);
        } else {
            self.intro = Some(text);
        }
    }
}

impl Px<'_> {
    /// The picture box of a model. Asks for the picture when the box is in view.
    fn thumb(&mut self, ui: &mut egui::Ui, oid: Option<&str>) {
        let texture = oid.and_then(|o| self.w.palette.thumbs.get(o)).and_then(|t| t.as_ref());
        let rect = thumb_box(ui, texture);
        if let Some(o) = oid {
            if ui.is_rect_visible(rect) {
                self.w.palette.touch(o);
                if !self.w.palette.thumbs.contains_key(o) {
                    self.w.palette.thumbs_wanted.push(o.to_string());
                }
            }
        }
    }

    /// The picture of a model for a tile in view, and whether the model is known to have none. Asks for the
    /// picture when there is none yet.
    fn picture(&mut self, oid: Option<&str>) -> (Option<egui::TextureHandle>, bool) {
        let Some(o) = oid else { return (None, true) };
        let st = &mut self.w.palette;
        st.touch(o);
        match st.thumbs.get(o) {
            Some(t) => (t.clone(), t.is_none()),
            None => {
                st.thumbs_wanted.push(o.to_string());
                (None, false)
            }
        }
    }

    /// A row: a picture, a title with a smaller second part, a line under it, more widgets, and buttons at the
    /// right. Gives the index of the button that was clicked.
    #[allow(clippy::too_many_arguments)]
    fn prow(&mut self, ui: &mut egui::Ui, picture: impl FnOnce(&mut Self, &mut egui::Ui), title: &str, small: Option<RichText>, sub: &str, buttons: &[(&str, &str, bool)], extra: impl FnOnce(&mut Self, &mut egui::Ui)) -> Option<usize> {
        let mut clicked = None;
        ui.horizontal_top(|ui| {
            picture(self, ui);
            let buttons_width = buttons.iter().map(|b| style::button_width(ui, b.0, 13.0)).fold(0.0, f32::max);
            let mid_width = (ui.available_width() - buttons_width - ui.spacing().item_spacing.x).max(60.0);
            ui.allocate_ui_with_layout(egui::vec2(mid_width, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width(mid_width);
                ui.spacing_mut().item_spacing.y = 1.0;
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 5.0;
                    ui.label(RichText::new(title).strong().color(style::theme(ui.ctx()).text));
                    if let Some(s) = small {
                        ui.label(s);
                    }
                });
                if !sub.is_empty() {
                    muted(ui, sub);
                }
                extra(self, ui);
            });
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                for (i, (text, tip, enabled)) in buttons.iter().enumerate() {
                    let mut r = ui.add_enabled(*enabled, egui::Button::new(*text).min_size(egui::vec2(buttons_width, 0.0)));
                    let (enabled, label) = (*enabled, format!("{text} {title}"));
                    r.widget_info(move || egui::WidgetInfo::labeled(egui::Role::Button, enabled, &label));
                    if !tip.is_empty() {
                        r = r.on_hover_text(*tip).on_disabled_hover_text(*tip);
                    }
                    if r.clicked() {
                        clicked = Some(i);
                    }
                }
            });
        });
        ui.separator();
        clicked
    }
}

pub fn ui(w: &mut Window, ui: &mut egui::Ui) {
    if !w.ed.has_level {
        return;
    }
    let ctx = ui.ctx().clone();
    let mut open = egui::Rect::NOTHING;
    let mut open_tab = None;
    let strip = ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
        for (i, name) in TABS.iter().enumerate() {
            let on = w.palette.tab == i;
            let r = style::tab(ui, on, name);
            if on {
                open = r.rect;
            }
            if r.clicked() {
                w.palette.tab = i;
            }
            if on {
                open_tab = Some(r);
            }
        }
    });
    let strip = strip.response.rect;
    let mut px = Px { w, acts: Vec::new(), intro: None };
    // Room for the note under the list. A narrow panel wraps it.
    let foot = if style::info(ui.ctx()) == style::Info::Shown {
        let galley = ui.painter().layout(NEW_OBJECTS.to_string(), egui::TextStyle::Body.resolve(ui.style()), egui::Color32::PLACEHOLDER, ui.available_width());
        galley.size().y + ui.spacing().item_spacing.y + 4.0
    } else {
        0.0
    };
    let tab = px.w.palette.tab;
    let views = matches!(tab, 1..=3);
    if views {
        view_row(&mut px, ui, tab);
    }
    let grid = views && px.w.palette.grid;
    match (grid, tab) {
        (false, _) => {}
        (true, 1) => enemies_grid(&mut px, ui, foot),
        (true, 2) => models_grid(&mut px, ui, foot),
        (true, _) => kit_grid(&mut px, ui, foot),
    }
    let list = |px: &mut Px, ui: &mut egui::Ui| egui::ScrollArea::vertical().id_salt(("palette", tab)).auto_shrink([false, false]).max_height((ui.available_height() - foot).max(60.0)).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        egui::Frame::new().inner_margin(egui::Margin { left: 2, right: 8, top: 2, bottom: 4 }).show(ui, |ui| match tab {
            0 => collect(px, ui),
            1 => enemies(px, ui),
            2 => models(px, ui),
            3 => kit_tab(px, ui),
            4 => {
                px.intro(ui, "Ready-made platforms: rings, exits, lifts, spikes and more, with the commands in the order the game reads them. Each one uses the game's own models.");
                for p in PRESETS {
                    let key = p.key;
                    ui.push_id(key, |ui| {
                        if px.prow(ui, |px, ui| px.thumb(ui, Some(p.model.0)), p.name, None, p.about, &[("Add", "", true)], |_, _| {}).is_some() {
                            px.acts.push(Box::new(move |w: &mut Window| w.ed.add_preset(key)));
                        }
                    });
                }
            }
            _ => {
                px.intro(ui, "A camera zone is a box: while Glover is inside it, the camera moves as chosen, and it goes back when he leaves. It is a puzzle like the shipped levels’ camera zones, so it needs no game mod. The zone expects the player to walk the way the view looks; change it in the inspector.");
                for (key, lab, about) in CAM_PRESETS {
                    ui.push_id(key, |ui| {
                        if px.prow(ui, |_, _| {}, lab, None, about, &[("Add", "", true)], |_, _| {}).is_some() {
                            px.acts.push(Box::new(move |w: &mut Window| w.ed.add_camera_zone(key)));
                        }
                    });
                }
            }
        });
    });
    if !grid {
        let out = list(&mut px, ui);
        style::fits(ui, "palette", &out);
    } else if let (Some(text), Some(r)) = (px.intro.take(), open_tab) {
        // The grid has pictures only: the tab's note is the tooltip of its button, unless the notes are off
        if style::info(ui.ctx()) != style::Info::Off {
            r.on_hover_text(text);
        }
    }
    // The tab's note belongs to its button, the note about new objects to the row of tabs
    if let Some(text) = px.intro {
        style::para_on(ui, open, None, text);
    }
    style::para_on(ui, strip, None, NEW_OBJECTS);
    px.w.palette.shown_tab = Some(tab);
    let acts = px.acts;
    w.acts.extend(acts);
    work(w, &ctx);
}

/// Buttons in two columns. Gives the index of the one clicked.
fn grid(ui: &mut egui::Ui, id: &str, buttons: Vec<(egui::Button, String)>) -> Option<usize> {
    let mut clicked = None;
    let width = (ui.available_width() - 6.0) / 2.0;
    egui::Grid::new(id).num_columns(2).spacing([4.0, 4.0]).show(ui, |ui| {
        for (i, (b, tip)) in buttons.into_iter().enumerate() {
            let mut r = ui.add_sized([width, 24.0], b);
            if !tip.is_empty() {
                r = r.on_hover_text(tip);
            }
            if r.clicked() {
                clicked = Some(i);
            }
            if i % 2 == 1 {
                ui.end_row();
            }
        }
    });
    clicked
}

fn collect(px: &mut Px, ui: &mut egui::Ui) {
    h4(ui, "GARIBS");
    let head = style::last_anchor(ui);
    let garibs = [("garib", "★ Garib"), ("bang_500pt", "! 500-point bang"), ("extra_life", "♥ Extra life"), ("mad_garib", "✴ Mad garib")];
    if let Some(i) = grid(ui, "garibs", garibs.iter().map(|g| (egui::Button::new(g.1), String::new())).collect()) {
        let t = garibs[i].0;
        px.acts.push(Box::new(move |w: &mut Window| {
            let p = glovebox_editor::vec::add(w.ed.placement_point(), [0.0, 12.0, 0.0]);
            w.ed.add_garib(p, t);
        }));
    }
    style::para_at(ui, head, "Added to the selected garib’s group, or a new visible group.");
    h4(ui, "POWERUPS");
    let ed = &px.w.ed;
    let pickups: Vec<(String, i64)> = ed
        .backend
        .decomp()
        .get("decompEnums")
        .and_then(|e| e.get("Pickup"))
        .and_then(|p| p.as_object())
        .map(|p| p.iter().filter_map(|(k, v)| Some((k.clone(), v.as_i64()?))).filter(|x| x.1 < 15).collect())
        .unwrap_or_default();
    let mut buttons = Vec::new();
    let mut want = Vec::new();
    for (name, v) in &pickups {
        let oid = ed.powerup_model_id(*v);
        let import = oid.as_deref().is_some_and(|o| ed.is_import(o));
        let text = format!("{}{}", name.to_lowercase(), if import { "  (import)" } else { "" });
        let tip = oid.as_ref().map_or("no model".to_string(), |o| format!("model {o}"));
        let texture = oid.as_ref().and_then(|o| px.w.palette.thumbs.get(o)).and_then(|t| t.clone());
        if let Some(o) = oid.filter(|o| ed.model_ref(o).is_some() && !px.w.palette.thumbs.contains_key(o)) {
            want.push(o);
        }
        let b = match texture {
            Some(t) => egui::Button::image_and_text(egui::Image::new((t.id(), egui::vec2(20.0, 20.0))), text),
            None => egui::Button::new(text),
        };
        buttons.push((b, tip));
    }
    px.w.palette.thumbs_wanted.extend(want);
    if let Some(i) = grid(ui, "powerups", buttons) {
        let (name, v) = pickups[i].clone();
        px.acts.push(Box::new(move |w: &mut Window| w.ed.add_powerup(v, &name)));
    }
    h4(ui, "OTHER");
    if grid(ui, "other", vec![(egui::Button::new("? Mr. Tip"), String::new())]).is_some() {
        px.acts.push(Box::new(|w: &mut Window| w.ed.add_tip()));
    }
}

/// The view switch of the Enemies, Models and Kit tabs, and in the grid view what a click on a tile places.
fn view_row(px: &mut Px, ui: &mut egui::Ui, tab: usize) {
    let st = &mut px.w.palette;
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
        for (grid, text, tip) in [(false, "List", "Rows with names, notes and buttons"), (true, "Grid", "Pictures only. A click on a picture adds it to the level")] {
            if style::tab(ui, st.grid == grid, text).on_hover_text(tip).clicked() && st.grid != grid {
                st.grid = grid;
                changed = true;
            }
        }
        let (choices, chosen): (&[As], &mut usize) = match (st.grid, tab) {
            (true, 2) => (&MODELS_AS, &mut st.models_as),
            (true, 3) => (&KIT_AS, &mut st.kit_as),
            _ => return,
        };
        ui.add_space(6.0);
        for (i, c) in choices.iter().enumerate() {
            let on = *chosen == i;
            let r = ui.radio(on, c.label).on_hover_text(c.tip);
            let label = format!("Place as {}", c.label);
            r.widget_info(move || egui::WidgetInfo::selected(egui::Role::RadioButton, true, on, &label));
            if r.clicked() && !on {
                *chosen = i;
                changed = true;
            }
        }
    });
    if changed {
        px.w.palette.save(&px.w.settings);
    }
}

/// A tile of the grid view.
struct Tile {
    /// What tells the tile from the others of its tab.
    id: String,
    name: String,
    tip: String,
    texture: Option<egui::TextureHandle>,
    /// The thing has no picture: the tile shows its name.
    blank: bool,
    /// The color of the mark in the tile's corner.
    mark: Option<egui::Color32>,
    /// Why a click does not place the thing.
    refuse: Option<&'static str>,
    /// The tile has a corner that opens settings.
    settings: bool,
}

/// What the player did to a tile, with the tile's index.
enum Hit {
    Place(usize),
    Settings(usize),
}

/// The tiles of a tab, in as many columns as fit the panel. Only the rows in view are laid out: `tile` is asked
/// for those and for no others.
fn tile_grid(px: &mut Px, ui: &mut egui::Ui, foot: f32, count: usize, mut tile: impl FnMut(&mut Px, &egui::Context, usize) -> Tile) -> Option<Hit> {
    let th = style::theme(ui.ctx());
    let tab = px.w.palette.tab;
    // The list's margins: 2 points on the left, 8 for the scroll bar on the right
    let width = (ui.available_width() - 10.0).max(TILE);
    let cols = (((width + GAP) / (TILE + GAP)).floor() as usize).max(1);
    let size = ((width - GAP * (cols - 1) as f32) / cols as f32).floor();
    px.w.palette.grid_cols = cols;
    let mut hit = None;
    ui.spacing_mut().item_spacing = egui::vec2(GAP, GAP);
    let height = (ui.available_height() - foot).max(60.0);
    let out = egui::ScrollArea::vertical().id_salt(("palette grid", tab)).auto_shrink([false, false]).max_height(height).show_rows(ui, size, count.div_ceil(cols), |ui, rows| {
        let origin = ui.max_rect().min + egui::vec2(2.0, 0.0);
        let (first, n) = (rows.start, rows.len());
        for row in rows {
            for col in 0..cols {
                let i = row * cols + col;
                if i >= count {
                    break;
                }
                let at = origin + egui::vec2(col as f32, (row - first) as f32) * (size + GAP);
                let rect = egui::Rect::from_min_size(at, egui::Vec2::splat(size));
                let t = tile(px, ui.ctx(), i);
                let id = ui.scope_id().with(("tile", &t.id));
                let r = ui.interact(rect, id, egui::Sense::click());
                let (enabled, name) = (t.refuse.is_none(), t.name.clone());
                r.widget_info(move || egui::WidgetInfo::labeled(egui::Role::Button, enabled, &name));
                let corner = egui::Rect::from_min_max(rect.max - egui::vec2(20.0, 16.0), rect.max - egui::vec2(2.0, 2.0));
                let gear = t.settings.then(|| {
                    let g = ui.interact(corner, id.with("settings"), egui::Sense::click());
                    let name = format!("Settings {}", t.name);
                    g.widget_info(move || egui::WidgetInfo::labeled(egui::Role::Button, true, &name));
                    g
                });
                if ui.is_rect_visible(rect) {
                    let p = ui.painter();
                    p.rect_filled(rect, 0.0, th.well);
                    style::well_edge(p, rect);
                    let tint = if t.refuse.is_some() { egui::Color32::WHITE.gamma_multiply(0.25) } else { egui::Color32::WHITE };
                    match &t.texture {
                        Some(tex) => {
                            p.image(tex.id(), rect.shrink(3.0), egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), tint);
                        }
                        None if t.blank => {
                            let galley = p.layout(t.name.clone(), egui::FontId::proportional(10.0), th.muted, size - 8.0);
                            p.with_clip_rect(rect.shrink(3.0)).galley(rect.center() - galley.size() / 2.0, galley, th.muted);
                        }
                        None => {}
                    }
                    if let Some(color) = t.mark {
                        let c = egui::pos2(rect.max.x - 2.0, rect.min.y + 2.0);
                        p.add(egui::Shape::convex_polygon(vec![c - egui::vec2(13.0, 0.0), c, c + egui::vec2(0.0, 13.0)], color, egui::Stroke::new(1.0, th.well)));
                    }
                    if let Some(g) = &gear {
                        p.rect_filled(corner, 2.0, if g.hovered() { th.focus } else { th.panel });
                        p.rect_stroke(corner, 2.0, egui::Stroke::new(1.0, th.line), egui::StrokeKind::Inside);
                        for k in [-1.0, 0.0, 1.0] {
                            p.circle_filled(corner.center() + egui::vec2(k * 4.5, 0.0), 1.3, if g.hovered() { th.well } else { th.text });
                        }
                    }
                    if r.hovered() || r.has_focus() {
                        p.rect_stroke(rect, 0.0, egui::Stroke::new(2.0, th.focus), egui::StrokeKind::Inside);
                    }
                }
                if r.clicked() {
                    match t.refuse {
                        Some(why) => px.acts.push(Box::new(move |w: &mut Window| w.set_status(why, "err"))),
                        None => hit = Some(Hit::Place(i)),
                    }
                }
                if r.secondary_clicked() && t.settings {
                    hit = Some(Hit::Settings(i));
                }
                if let Some(g) = gear {
                    if g.on_hover_text("Settings").clicked() {
                        hit = Some(Hit::Settings(i));
                    }
                }
                r.on_hover_text(t.tip);
            }
        }
        ui.allocate_rect(egui::Rect::from_min_size(origin, egui::vec2(width, (n as f32 * (size + GAP) - GAP).max(0.0))), egui::Sense::hover());
    });
    style::fits(ui, "palette", &out);
    hit
}

/// What the Enemies tab says about one enemy type.
struct EnemyRow {
    name: String,
    token: String,
    /// Where the model and the script come from.
    sub: String,
    boss: bool,
    /// The model, when it can be drawn. Without one the enemy can't be added.
    model: Option<String>,
}

fn enemy_rows(px: &Px) -> Vec<EnemyRow> {
    let ed = &px.w.ed;
    let backend = ed.backend.clone();
    let Some(types) = backend.decomp().get("ksyEnums").and_then(|e| e.get("enemy::enemy_type")).and_then(|e| e.as_object()) else { return Vec::new() };
    let templates = backend.templates();
    let mut rows = Vec::new();
    for (v, name) in types {
        let Some(name) = name.as_str() else { continue };
        let oid = ed.enemy_model_id(Some(name));
        let t = templates.get(name);
        let where_ = match &oid {
            None => "no model".to_string(),
            Some(o) if ed.loaded.contains(o) => "in this level".to_string(),
            Some(o) => ed.catalog.get(o).map_or("model missing".to_string(), |c| format!("imports from {}", c.bank)),
        };
        let script = match t {
            Some(t) => format!(" · script from {}", t.get("level").and_then(|l| l.as_str()).unwrap_or("")),
            None => " · no shipped script".to_string(),
        };
        let boss = t.and_then(|t| t.get("boss")).is_some_and(|b| b.as_bool().unwrap_or(false));
        let token = v.parse::<i64>().ok().and_then(|n| ed.decomp_enum_name("ActorType", Some(n))).unwrap_or_default();
        let model = oid.as_deref().filter(|o| ed.model_ref(o).is_some()).map(str::to_string);
        rows.push(EnemyRow { name: name.to_string(), token, sub: format!("{where_}{script}{}", if boss { " · boss" } else { "" }), boss, model });
    }
    rows
}

fn enemies(px: &mut Px, ui: &mut egui::Ui) {
    px.intro(ui, ENEMIES_NOTE);
    for e in enemy_rows(px) {
        ui.push_id(&e.name, |ui| {
            let small = RichText::new(&e.token).monospace().color(style::theme(ui.ctx()).tok);
            let has = e.model.is_some();
            let clicked = px.prow(ui, |px, ui| px.thumb(ui, e.model.as_deref()), &e.name, Some(small), &e.sub, &[("Add", "", has)], |_, _| {});
            if clicked.is_some() {
                let name = e.name.clone();
                px.acts.push(Box::new(move |w: &mut Window| w.ed.add_enemy(&name)));
            }
        });
    }
}

/// The Enemies tab as tiles. A click adds the enemy. A boss has a mark in its corner.
fn enemies_grid(px: &mut Px, ui: &mut egui::Ui, foot: f32) {
    px.intro = Some(ENEMIES_NOTE);
    let rows = enemy_rows(px);
    let boss = style::theme(ui.ctx()).warn;
    let hit = tile_grid(px, ui, foot, rows.len(), |px, _, i| {
        let e = &rows[i];
        let (texture, blank) = px.picture(e.model.as_deref());
        let refuse = e.model.is_none().then_some(NO_ENEMY_MODEL);
        let tip = format!("{}  {}\n{}{}", e.name, e.token, e.sub, refuse.map(|r| format!("\n{r}")).unwrap_or_default());
        Tile { id: e.name.clone(), name: e.name.clone(), tip, texture, blank, mark: e.boss.then_some(boss), refuse, settings: false }
    });
    if let Some(Hit::Place(i)) = hit {
        let name = rows[i].name.clone();
        px.acts.push(Box::new(move |w: &mut Window| w.ed.add_enemy(&name)));
    }
}

/// 1234 as "1,234".
fn thousands(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

type Catalog = BTreeMap<String, glovebox_core::backend::CatalogEntry>;

fn import_button(px: &mut Px, ui: &mut egui::Ui) {
    ui.horizontal_wrapped(|ui| {
        let r = ui.add(style::accent("Import model…"));
        if r.clicked() {
            px.acts.push(Box::new(|w: &mut Window| crate::dialogs::open_model_dialog(w, None)));
        }
        style::para_on(ui, r.rect, None, "from another game or a 3D program (.glb)");
    });
}

/// The search box and the bank list of the Models tab, and the models they leave: this level's first, then by
/// name. Also gives the level's banks.
fn model_rows<'a>(px: &mut Px, ui: &mut egui::Ui, catalog: &'a Catalog) -> (Vec<(&'a String, &'a glovebox_core::backend::CatalogEntry)>, HashSet<String>) {
    let st = &mut px.w.palette;
    ui.horizontal(|ui| {
        let banks: std::collections::BTreeSet<&str> = catalog.values().map(|c| c.bank.as_str()).collect();
        let shown = match st.bank.as_str() {
            "" => "All banks",
            "@level" => "This level’s banks",
            b => b,
        }
        .to_string();
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let r = egui::ComboBox::from_id_salt("bank").icon(style::combo_icon).selected_text(shown).width(120.0).truncate().height(400.0).show_ui(ui, |ui| {
                let mut pick = None;
                for (v, text) in [("", "All banks"), ("@level", "This level’s banks")].into_iter().chain(banks.iter().map(|b| (*b, *b))) {
                    if ui.selectable_label(st.bank == v, text).clicked() {
                        pick = Some(v.to_string());
                    }
                }
                if let Some(p) = pick {
                    st.bank = p;
                }
            });
            r.response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, "Bank"));
            let hint = format!("Search {} models by name, ID or bank…", thousands(catalog.len()));
            if !st.query.is_empty() {
                let clear = style::mini(ui, "×", "Clear");
                clear.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Clear search"));
                if clear.clicked() {
                    st.query.clear();
                }
            }
            // Its own id: the clear button comes and goes before it, and would shift an id made by counting
            let r = ui.add(egui::TextEdit::singleline(&mut st.query).id_salt("search models").hint_text(hint).desired_width(f32::INFINITY));
            style::name(&r, "Search models");
            // The tab opens ready for typing
            if st.shown_tab != Some(2) {
                r.request_focus();
            }
        });
    });
    let term = st.query.to_lowercase();
    let bank = st.bank.clone();
    let level_banks: HashSet<String> = px
        .w
        .ed
        .scene
        .as_ref()
        .and_then(|s| s.get("sureObjectBanks"))
        .and_then(|b| b.as_array())
        .map(|b| b.iter().filter_map(|x| x.as_str()).map(|x| x.split('.').next().unwrap_or(x).to_string()).collect())
        .unwrap_or_default();
    let mut rows: Vec<(&String, &glovebox_core::backend::CatalogEntry)> = catalog
        .iter()
        .filter(|(i, c)| {
            (bank.is_empty() || if bank == "@level" { level_banks.contains(&c.bank) } else { c.bank == bank })
                && (term.is_empty() || format!("{} {} {}", c.name, i, c.bank).to_lowercase().contains(&term))
        })
        .collect();
    rows.sort_by_key(|(_, c)| (!level_banks.contains(&c.bank), c.name.to_lowercase()));
    (rows, level_banks)
}

/// Where a model is from and how large it is: the second line of its row, and of its tile's tooltip.
fn model_sub(c: &glovebox_core::backend::CatalogEntry, here: bool) -> String {
    format!("{} · {:.1} KB{}", c.bank, c.bytes as f64 / 1024.0, if here { " · in this level" } else { " · imported at build" })
}

fn models(px: &mut Px, ui: &mut egui::Ui) {
    import_button(px, ui);
    let catalog = px.w.ed.catalog.clone();
    let mut sets: Vec<String> = Vec::new();
    for c in catalog.values() {
        if let Some(cu) = &c.custom {
            if !sets.contains(&cu.set) {
                sets.push(cu.set.clone());
            }
        }
    }
    if !sets.is_empty() {
        h4(ui, "YOUR IMPORTED MODELS");
        for set in sets {
            let pieces = px.w.ed.custom_pieces(&set);
            let Some((first, c)) = pieces.first() else { continue };
            let kb = pieces.iter().map(|p| p.1.bytes).sum::<u64>() as f64 / 1024.0;
            let sub = format!("{} piece{} · {:.0} KB", pieces.len(), if pieces.len() == 1 { "" } else { "s" }, kb);
            let buttons = [("Place", "Every piece as LandActor: solid scenery with collision", true), ("Remove", "Remove this model from the editor", true)];
            let clicked = ui.push_id(("set", &set), |ui| px.prow(ui, |px, ui| px.thumb(ui, Some(first)), &c.bank.replace("Custom: ", ""), None, &sub, &buttons, |_, _| {})).inner;
            match clicked {
                Some(0) => px.acts.push(Box::new(move |w: &mut Window| w.ed.place_custom(&set, "LandActor", None))),
                Some(_) => px.acts.push(Box::new(move |w: &mut Window| w.remove_custom(set))),
                None => {}
            }
        }
        h4(ui, "ALL MODELS");
    }
    let (rows, level_banks) = model_rows(px, ui, &catalog);
    if rows.is_empty() {
        muted(ui, NO_MODELS);
    }
    for (oid, c) in rows.iter().take(150) {
        let sub = model_sub(c, level_banks.contains(&c.bank));
        let custom = c.custom.is_some();
        let buttons: Vec<(&str, &str, bool)> = MODELS_AS.iter().map(|a| if a.tag == "Platform" && custom { (a.label, WHY_CUSTOM_PLATFORM, false) } else { (a.label, a.tip, true) }).collect();
        let small = RichText::new(oid.as_str()).size(11.0);
        let clicked = ui.push_id(oid, |ui| px.prow(ui, |px, ui| px.thumb(ui, Some(oid)), &c.name, Some(small), &sub, &buttons, |_, _| {})).inner;
        if let Some(i) = clicked {
            let (oid, as_) = (oid.to_string(), MODELS_AS[i].tag);
            px.acts.push(Box::new(move |w: &mut Window| w.ed.add_model(&oid, as_)));
        }
    }
    if rows.len() > 150 {
        muted(ui, &format!("{} more; narrow the search.", rows.len() - 150));
    }
}

/// The Models tab as tiles: every model the search leaves, with no limit. A click places the model as the radio
/// buttons say. One of the player's own models has a mark in its corner, and can't be a platform.
fn models_grid(px: &mut Px, ui: &mut egui::Ui, foot: f32) {
    import_button(px, ui);
    let catalog = px.w.ed.catalog.clone();
    let (rows, level_banks) = model_rows(px, ui, &catalog);
    if rows.is_empty() {
        muted(ui, NO_MODELS);
        return;
    }
    let place = &MODELS_AS[px.w.palette.models_as.min(2)];
    let own = style::theme(ui.ctx()).imp;
    let hit = tile_grid(px, ui, foot, rows.len(), |px, _, i| {
        let (oid, c) = rows[i];
        let custom = c.custom.is_some();
        let refuse = (custom && place.tag == "Platform").then_some(WHY_CUSTOM_PLATFORM);
        let (texture, blank) = px.picture(Some(oid));
        let mut tip = format!("{}  {}\n{}", c.name, oid, model_sub(c, level_banks.contains(&c.bank)));
        if custom {
            tip.push_str("\nOne of your imported models");
        }
        if let Some(why) = refuse {
            tip.push('\n');
            tip.push_str(why);
        }
        Tile { id: oid.clone(), name: c.name.clone(), tip, texture, blank, mark: custom.then_some(own), refuse, settings: false }
    });
    if let Some(Hit::Place(i)) = hit {
        let (oid, as_) = (rows[i].0.clone(), place.tag);
        px.acts.push(Box::new(move |w: &mut Window| w.ed.add_model(&oid, as_)));
    }
}

fn decode_png(png_bytes: &[u8]) -> Option<egui::ColorImage> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width as usize, info.height as usize);
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => buf[..w * h * 4].to_vec(),
        png::ColorType::Rgb => buf[..w * h * 3].chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf[..w * h * 2].chunks_exact(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        png::ColorType::Grayscale => buf[..w * h].iter().flat_map(|p| [*p, *p, *p, 255]).collect(),
        png::ColorType::Indexed => return None,
    };
    Some(egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba))
}

fn sv(v: &Value, k: &str) -> String {
    v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string()
}

/// The kit's items, read once.
fn kit_items(px: &mut Px) -> Vec<Value> {
    if px.w.palette.kit.is_none() {
        let items = px.w.ed.backend.kit_items().ok().and_then(|v| v.as_array().cloned()).unwrap_or_default();
        px.w.palette.kit = Some(items);
    }
    px.w.palette.kit.clone().unwrap_or_default()
}

fn kit_params_of(item: &Value) -> Vec<Value> {
    item.get("params").and_then(|p| p.as_array()).cloned().unwrap_or_default()
}

/// Asks for the picture of a kit item with its settings as they are: at once the first time, a moment after the
/// last change otherwise.
fn kit_want(px: &mut Px, ctx: &egui::Context, id: &str, params: &[Value], now: f64) {
    let pal = &mut px.w.palette;
    let vals = pal.kit_vals.entry(id.to_string()).or_insert_with(|| params.iter().map(|p| (sv(p, "key"), p.get("default").cloned().unwrap_or(Value::Null))).collect()).clone();
    let key = Value::Object(vals.clone()).to_string();
    let current = pal.kit_images.get(id).map(|x| x.0.clone());
    if current.as_deref() != Some(&key) && !pal.kit_loading.contains(id) {
        let due = *pal.kit_due.entry(id.to_string()).or_insert(if current.is_none() { now } else { now + 0.3 });
        if now >= due {
            pal.kit_due.remove(id);
            pal.kit_loading.insert(id.to_string());
            let (backend, id2, vals2, key2, ctx) = (px.w.ed.backend.clone(), id.to_string(), Value::Object(vals), key, ctx.clone());
            px.w.side_jobs.run(
                move || (backend.kit_preview(&id2, &vals2).ok().and_then(|png| decode_png(&png)), id2, key2),
                move |w: &mut Window, (img, id, key)| {
                    w.palette.kit_loading.remove(&id);
                    // A value the item can't be made with keeps the old picture. The next change draws again.
                    let old = w.palette.kit_images.get(&id).and_then(|x| x.1.clone());
                    let t = img.map(|img| ctx.load_texture(format!("kit {id}"), img, egui::TextureOptions::LINEAR)).or(old);
                    w.palette.kit_images.insert(id, (key, t));
                },
            );
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}

/// The fields of a kit item's settings.
fn kit_params(px: &mut Px, ui: &mut egui::Ui, id: &str, params: &[Value], now: f64) {
    let mut changed = false;
    let Some(vals) = px.w.palette.kit_vals.get_mut(id) else { return };
    for p in params {
        let (pkey, label) = (sv(p, "key"), sv(p, "label"));
        let line = ui.available_width();
        ui.horizontal_wrapped(|ui| {
            // A name longer than the line wraps
            ui.add(egui::Label::new(RichText::new(&label).monospace().size(12.0).color(style::theme(ui.ctx()).muted)).wrap());
            // The field takes the room the name leaves, up to 130 points. After a long name it
            // goes on a line of its own.
            let left = ui.max_rect().right() - ui.cursor().left() - 10.0;
            let room = if left >= 60.0 { left.min(130.0) } else { (line - 10.0).clamp(40.0, 130.0) };
            if sv(p, "kind") == "choice" {
                let labels: Vec<String> = p.get("extra").and_then(|e| e.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
                let cur = vals.get(&pkey).map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())).and_then(|v| v.parse::<usize>().ok()).unwrap_or(1);
                let r = egui::ComboBox::from_id_salt(&pkey).icon(style::combo_icon).selected_text(labels.get(cur.saturating_sub(1)).cloned().unwrap_or_default()).width(room).truncate().show_ui(ui, |ui| {
                    for (i, l) in labels.iter().enumerate() {
                        if ui.selectable_label(cur == i + 1, l).clicked() {
                            vals.insert(pkey.clone(), Value::String((i + 1).to_string()));
                            changed = true;
                        }
                    }
                });
                let label = label.clone();
                r.response.widget_info(move || egui::WidgetInfo::labeled(egui::Role::ComboBox, true, &label));
            } else {
                let mut text = vals.get(&pkey).map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())).unwrap_or_default();
                // A number changes by dragging too. A text is typed only.
                let (r, edited) = if sv(p, "kind") == "number" {
                    let spec = crate::numfield::Spec::for_text(&text);
                    crate::numfield::bound(ui, ui.scope_id().with(&pkey), &mut text, spec, |t| t.desired_width(room))
                } else {
                    let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(room));
                    let edited = r.changed();
                    (r, edited)
                };
                style::name(&r, &label);
                let hint = sv(p, "extra");
                if !hint.is_empty() {
                    r.on_hover_text(hint);
                }
                if edited {
                    vals.insert(pkey.clone(), Value::String(text));
                    changed = true;
                }
            }
        });
    }
    if changed {
        px.w.palette.kit_due.insert(id.to_string(), now + 0.3);
    }
}

/// Makes a kit item with its settings as they are and places it.
fn kit_place(px: &mut Px, id: &str, name: &str, as_: &'static str) {
    let vals: BTreeMap<String, Value> = px.w.palette.kit_vals.get(id).map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect()).unwrap_or_default();
    let (id, name) = (id.to_string(), name.to_string());
    px.acts.push(Box::new(move |w: &mut Window| w.place_kit(id, name, serde_json::json!(vals), as_)));
}

/// The kit: models made by code, some with settings. Placing one builds it and converts it like an imported model.
/// It then shows under Models with the other imported models.
fn kit_tab(px: &mut Px, ui: &mut egui::Ui) {
    let now = px.w.now();
    let items = kit_items(px);
    px.intro(ui, KIT_NOTE);
    let mut group = String::new();
    for it in &items {
        let (id, name) = (sv(it, "id"), sv(it, "name"));
        if sv(it, "group") != group {
            group = sv(it, "group");
            h4(ui, &group.to_uppercase());
        }
        let params = kit_params_of(it);
        kit_want(px, ui.ctx(), &id, &params, now);
        let buttons: Vec<(&str, &str, bool)> = KIT_AS.iter().map(|a| (a.label, a.tip, true)).collect();
        let clicked = ui
            .push_id(("kit", &id), |ui| {
                px.prow(
                    ui,
                    |px, ui| {
                        let t = px.w.palette.kit_images.get(&id).and_then(|x| x.1.clone());
                        thumb_box(ui, t.as_ref());
                    },
                    &name,
                    None,
                    &sv(it, "note"),
                    &buttons,
                    |px, ui| kit_params(px, ui, &id, &params, now),
                )
            })
            .inner;
        if let Some(i) = clicked {
            kit_place(px, &id, &name, KIT_AS[i].tag);
        }
    }
}

/// The kit as tiles. A click makes the item with its settings as they are and places it as the radio buttons
/// say. The corner of a tile with settings, or a right-click on it, shows the settings over the grid.
fn kit_grid(px: &mut Px, ui: &mut egui::Ui, foot: f32) {
    let now = px.w.now();
    let items = kit_items(px);
    px.intro = Some(KIT_NOTE);
    if let Some(open) = px.w.palette.kit_settings.clone() {
        let item = items.iter().find(|it| sv(it, "id") == open).map(|it| (sv(it, "name"), kit_params_of(it)));
        match item {
            Some((name, params)) if !params.is_empty() => {
                let th = style::theme(ui.ctx());
                style::boxed(th, th.panel).inner_margin(egui::Margin::symmetric(8, 6)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&name).strong().color(th.text));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let close = style::mini(ui, "×", "Close");
                            close.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Close settings"));
                            if close.clicked() {
                                px.w.palette.kit_settings = None;
                            }
                        });
                    });
                    ui.push_id(("kit settings", &open), |ui| kit_params(px, ui, &open, &params, now));
                });
            }
            _ => px.w.palette.kit_settings = None,
        }
    }
    let place = KIT_AS[px.w.palette.kit_as.min(1)].tag;
    let hit = tile_grid(px, ui, foot, items.len(), |px, ctx, i| {
        let it = &items[i];
        let (id, name, params) = (sv(it, "id"), sv(it, "name"), kit_params_of(it));
        kit_want(px, ctx, &id, &params, now);
        let texture = px.w.palette.kit_images.get(&id).and_then(|x| x.1.clone());
        let mut tip = format!("{name}\n{}", sv(it, "group"));
        for line in [sv(it, "note"), if params.is_empty() { String::new() } else { "Right-click, or click the corner, for its settings".to_string() }] {
            if !line.is_empty() {
                tip.push('\n');
                tip.push_str(&line);
            }
        }
        Tile { id, name, tip, texture, blank: false, mark: None, refuse: None, settings: !params.is_empty() }
    });
    match hit {
        Some(Hit::Place(i)) => kit_place(px, &sv(&items[i], "id"), &sv(&items[i], "name"), place),
        Some(Hit::Settings(i)) => {
            let id = sv(&items[i], "id");
            let st = &mut px.w.palette;
            st.kit_settings = if st.kit_settings.as_deref() == Some(&id) { None } else { Some(id) };
        }
        None => {}
    }
}
