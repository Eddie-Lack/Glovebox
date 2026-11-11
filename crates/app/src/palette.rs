//! The Add palette: collectables, enemies, models from any level, the kit, platform presets and camera zones.
//! Model pictures are drawn when their row scrolls into view, and kept.

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

pub const TABS: [&str; 6] = ["Collectables", "Enemies", "Models", "Kit", "Presets", "Camera"];

#[derive(Default)]
pub struct State {
    /// Which side panel shows: 0 the inspector, 1 the palette, 2 the path tool.
    pub side_tab: u8,
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
}

impl State {
    /// The number of model pictures drawn so far.
    pub fn thumbs_done(&self) -> usize {
        self.thumbs.values().filter(|t| t.is_some()).count()
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
        if w.palette.thumbs.contains_key(&oid) || !w.palette.thumbs_loading.insert(oid.clone()) {
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
        w.palette.thumbs.insert(oid, handle);
        ctx.request_repaint();
    }
}

/// The 64 by 64 box of a row's picture.
fn thumb_box(ui: &mut egui::Ui, texture: Option<&egui::TextureHandle>) -> egui::Rect {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(64.0, 64.0), egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter().rect(rect, 6.0, style::INPUT, egui::Stroke::new(1.0, style::BORDER), egui::StrokeKind::Inside);
        if let Some(t) = texture {
            ui.painter().image(t.id(), rect.shrink(1.0), egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
        }
    }
    rect
}

struct Px<'a> {
    w: &'a mut Window,
    acts: Vec<Act>,
}

impl Px<'_> {
    /// The picture box of a model. Asks for the picture when the box is in view.
    fn thumb(&mut self, ui: &mut egui::Ui, oid: Option<&str>) {
        let texture = oid.and_then(|o| self.w.palette.thumbs.get(o)).and_then(|t| t.as_ref());
        let rect = thumb_box(ui, texture);
        if let Some(o) = oid {
            if ui.is_rect_visible(rect) && !self.w.palette.thumbs.contains_key(o) {
                self.w.palette.thumbs_wanted.push(o.to_string());
            }
        }
    }

    /// A row: a picture, a title with a smaller second part, a line under it, more widgets, and buttons at the
    /// right. Gives the index of the button that was clicked.
    #[allow(clippy::too_many_arguments)]
    fn prow(&mut self, ui: &mut egui::Ui, picture: impl FnOnce(&mut Self, &mut egui::Ui), title: &str, small: Option<RichText>, sub: &str, buttons: &[(&str, &str, bool)], extra: impl FnOnce(&mut Self, &mut egui::Ui)) -> Option<usize> {
        let mut clicked = None;
        ui.horizontal(|ui| {
            picture(self, ui);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    let width = buttons.iter().map(|b| b.0.len()).max().unwrap_or(3) as f32 * 7.0 + 20.0;
                    for (i, (text, tip, enabled)) in buttons.iter().enumerate() {
                        let mut r = ui.add_enabled(*enabled, egui::Button::new(*text).min_size(egui::vec2(width, 0.0)));
                        if !tip.is_empty() {
                            r = r.on_hover_text(*tip).on_disabled_hover_text(*tip);
                        }
                        if r.clicked() {
                            clicked = Some(i);
                        }
                    }
                });
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 5.0;
                        ui.label(RichText::new(title).strong());
                        if let Some(s) = small {
                            ui.label(s);
                        }
                    });
                    if !sub.is_empty() {
                        muted(ui, sub);
                    }
                    extra(self, ui);
                });
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
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(2.0, 2.0);
        for (i, name) in TABS.iter().enumerate() {
            let on = w.palette.tab == i;
            if ui.add(egui::Button::selectable(on, RichText::new(*name).color(if on { style::GOLD } else { style::TEXT }))).clicked() {
                w.palette.tab = i;
            }
        }
    });
    let mut px = Px { w, acts: Vec::new() };
    let foot = 26.0;
    let tab = px.w.palette.tab;
    egui::ScrollArea::vertical().id_salt(("palette", tab)).auto_shrink([false, false]).max_height((ui.available_height() - foot).max(60.0)).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        egui::Frame::new().inner_margin(egui::Margin { left: 2, right: 8, top: 2, bottom: 4 }).show(ui, |ui| match tab {
            0 => collect(&mut px, ui),
            1 => enemies(&mut px, ui),
            2 => models(&mut px, ui),
            3 => kit_tab(&mut px, ui),
            4 => {
                muted(ui, "Ready-made platforms: rings, exits, lifts, spikes and more, with the commands in the order the game reads them. Each one uses the game's own models.");
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
                muted(ui, "A camera zone is a box: while Glover is inside it, the camera moves as chosen, and it goes back when he leaves. It is a puzzle like the shipped levels’ camera zones, so it needs no game mod. The zone expects the player to walk the way the view looks; change it in the inspector.");
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
    muted(ui, "New objects appear where the view’s centre meets the ground.");
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
    let garibs = [("garib", "✦ Garib"), ("bang_500pt", "! 500-point bang"), ("extra_life", "♥ Extra life"), ("mad_garib", "✸ Mad garib")];
    if let Some(i) = grid(ui, "garibs", garibs.iter().map(|g| (egui::Button::new(g.1), String::new())).collect()) {
        let t = garibs[i].0;
        px.acts.push(Box::new(move |w: &mut Window| {
            let p = glovebox_editor::vec::add(w.ed.placement_point(), [0.0, 12.0, 0.0]);
            w.ed.add_garib(p, t);
        }));
    }
    muted(ui, "Added to the selected garib’s group, or a new visible group.");
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

fn enemies(px: &mut Px, ui: &mut egui::Ui) {
    muted(ui, "A new enemy copies the shortest script that type has in a shipped level, with its patrol/target points moved to the new spot. Bosses are tied to their arenas and may not behave elsewhere.");
    let backend = px.w.ed.backend.clone();
    let Some(types) = backend.decomp().get("ksyEnums").and_then(|e| e.get("enemy::enemy_type")).and_then(|e| e.as_object()) else { return };
    let templates = backend.templates();
    for (v, name) in types {
        let Some(name) = name.as_str() else { continue };
        let ed = &px.w.ed;
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
        let boss = if t.and_then(|t| t.get("boss")).is_some_and(|b| b.as_bool().unwrap_or(false)) { " · boss" } else { "" };
        let token = v.parse::<i64>().ok().and_then(|n| ed.decomp_enum_name("ActorType", Some(n))).unwrap_or_default();
        let drawable = oid.as_deref().filter(|o| ed.model_ref(o).is_some()).map(str::to_string);
        ui.push_id(name, |ui| {
            let small = RichText::new(token).monospace().color(style::TOK);
            let has = drawable.is_some();
            let clicked = px.prow(ui, |px, ui| px.thumb(ui, drawable.as_deref()), name, Some(small), &format!("{where_}{script}{boss}"), &[("Add", "", has)], |_, _| {});
            if clicked.is_some() {
                let name = name.to_string();
                px.acts.push(Box::new(move |w: &mut Window| w.ed.add_enemy(&name)));
            }
        });
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

fn models(px: &mut Px, ui: &mut egui::Ui) {
    ui.horizontal_wrapped(|ui| {
        if ui.add(style::accent("Import model…")).clicked() {
            px.acts.push(Box::new(|w: &mut Window| crate::dialogs::open_model_dialog(w, None)));
        }
        muted(ui, "from another game or a 3D program (.glb)");
    });
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
    let st = &mut px.w.palette;
    ui.horizontal(|ui| {
        let banks: std::collections::BTreeSet<&str> = catalog.values().map(|c| c.bank.as_str()).collect();
        let shown = match st.bank.as_str() {
            "" => "All banks",
            "@level" => "This level’s banks",
            b => b,
        };
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let r = egui::ComboBox::from_id_salt("bank").selected_text(shown.to_string()).width(120.0).truncate().height(400.0).show_ui(ui, |ui| {
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
            r.response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, "Bank"));
            let hint = format!("Search {} models by name, ID or bank…", thousands(catalog.len()));
            let r = ui.add(egui::TextEdit::singleline(&mut st.query).hint_text(hint).desired_width(f32::INFINITY));
            style::name(&r, "Search models");
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
    for (oid, c) in rows.iter().take(150) {
        let here = level_banks.contains(&c.bank);
        let sub = format!("{} · {:.1} KB{}", c.bank, c.bytes as f64 / 1024.0, if here { " · in this level" } else { " · imported at build" });
        let custom = c.custom.is_some();
        let buttons = [
            ("Scenery", "LandActor: solid scenery with collision", true),
            ("Background", "BackgroundActor: decoration without collision", true),
            ("Platform", if custom { WHY_CUSTOM_PLATFORM } else { "Platform: solid, can move, spin and be scaled" }, !custom),
        ];
        let small = RichText::new(oid.as_str()).size(11.0);
        let clicked = ui.push_id(oid, |ui| px.prow(ui, |px, ui| px.thumb(ui, Some(oid)), &c.name, Some(small), &sub, &buttons, |_, _| {})).inner;
        if let Some(i) = clicked {
            let (oid, as_) = (oid.to_string(), ["LandActor", "BackgroundActor", "Platform"][i]);
            px.acts.push(Box::new(move |w: &mut Window| w.ed.add_model(&oid, as_)));
        }
    }
    if rows.len() > 150 {
        muted(ui, &format!("{} more; narrow the search.", rows.len() - 150));
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

/// The kit: models made by code, some with settings. Placing one builds it and converts it like an imported model.
/// It then shows under Models with the other imported models.
fn kit_tab(px: &mut Px, ui: &mut egui::Ui) {
    let now = px.w.now();
    if px.w.palette.kit.is_none() {
        let items = px.w.ed.backend.kit_items().ok().and_then(|v| v.as_array().cloned()).unwrap_or_default();
        px.w.palette.kit = Some(items);
    }
    muted(ui, "Props made by the editor, free to share. Background has no collision; Scenery is solid. They are copied into the level’s banks at ROM build, like imported models.");
    let items = px.w.palette.kit.clone().unwrap_or_default();
    let st = |v: &Value, k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    let mut group = String::new();
    for it in &items {
        let (id, name) = (st(it, "id"), st(it, "name"));
        if st(it, "group") != group {
            group = st(it, "group");
            h4(ui, &group.to_uppercase());
        }
        let params: Vec<Value> = it.get("params").and_then(|p| p.as_array()).cloned().unwrap_or_default();
        let pal = &mut px.w.palette;
        let vals = pal.kit_vals.entry(id.clone()).or_insert_with(|| params.iter().map(|p| (st(p, "key"), p.get("default").cloned().unwrap_or(Value::Null))).collect()).clone();
        let key = Value::Object(vals.clone()).to_string();
        // Ask for the picture: at once the first time, a moment after the last change otherwise
        let current = pal.kit_images.get(&id).map(|x| x.0.clone());
        if current.as_deref() != Some(&key) && !pal.kit_loading.contains(&id) {
            let due = *pal.kit_due.entry(id.clone()).or_insert(if current.is_none() { now } else { now + 0.3 });
            if now >= due {
                pal.kit_due.remove(&id);
                pal.kit_loading.insert(id.clone());
                let (backend, id2, vals2, key2, ctx) = (px.w.ed.backend.clone(), id.clone(), Value::Object(vals.clone()), key.clone(), ui.ctx().clone());
                px.w.side_jobs.run(
                    move || backend.kit_preview(&id2, &vals2).ok().and_then(|png| decode_png(&png)).map(|img| (id2, key2, img)),
                    move |w: &mut Window, res| {
                        if let Some((id, key, img)) = res {
                            let t = ctx.load_texture(format!("kit {id}"), img, egui::TextureOptions::LINEAR);
                            w.palette.kit_loading.remove(&id);
                            w.palette.kit_images.insert(id, (key, Some(t)));
                        }
                    },
                );
            } else {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
            }
        }
        let buttons = [("Background", "BackgroundActor: decoration without collision", true), ("Scenery", "LandActor: solid, built from the model’s triangles", true)];
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
                    &st(it, "note"),
                    &buttons,
                    |px, ui| {
                        let mut changed = false;
                        let Some(vals) = px.w.palette.kit_vals.get_mut(&id) else { return };
                        for p in &params {
                            let (pkey, label) = (st(p, "key"), st(p, "label"));
                            ui.horizontal(|ui| {
                                style::fieldname(ui, &label);
                                if st(p, "kind") == "choice" {
                                    let labels: Vec<String> = p.get("extra").and_then(|e| e.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
                                    let cur = vals.get(&pkey).map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())).and_then(|v| v.parse::<usize>().ok()).unwrap_or(1);
                                    let r = egui::ComboBox::from_id_salt(&pkey).selected_text(labels.get(cur.saturating_sub(1)).cloned().unwrap_or_default()).width(130.0).show_ui(ui, |ui| {
                                        for (i, l) in labels.iter().enumerate() {
                                            if ui.selectable_label(cur == i + 1, l).clicked() {
                                                vals.insert(pkey.clone(), Value::String((i + 1).to_string()));
                                                changed = true;
                                            }
                                        }
                                    });
                                    let label = label.clone();
                                    r.response.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, &label));
                                } else {
                                    let mut text = vals.get(&pkey).map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())).unwrap_or_default();
                                    let mut r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(130.0));
                                    style::name(&r, &label);
                                    let hint = st(p, "extra");
                                    if !hint.is_empty() {
                                        r = r.on_hover_text(hint);
                                    }
                                    if r.changed() {
                                        vals.insert(pkey.clone(), Value::String(text));
                                        changed = true;
                                    }
                                }
                            });
                        }
                        if changed {
                            px.w.palette.kit_due.insert(id.clone(), now + 0.3);
                        }
                    },
                )
            })
            .inner;
        if let Some(i) = clicked {
            let vals: BTreeMap<String, Value> = px.w.palette.kit_vals.get(&id).map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect()).unwrap_or_default();
            let (id, name, as_) = (id.clone(), name.clone(), ["BackgroundActor", "LandActor"][i]);
            px.acts.push(Box::new(move |w: &mut Window| w.place_kit(id, name, serde_json::json!(vals), as_)));
        }
    }
}
