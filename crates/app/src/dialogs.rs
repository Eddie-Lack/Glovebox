//! The system's file dialogs behind a trait. The app uses `rfd`'s native dialogs. Tests give the paths
//! themselves.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

/// A file filter: a name and its extensions, without dots. No extensions means every file.
pub type Filter<'a> = (&'a str, &'a [&'a str]);

pub trait FileDialogs {
    /// Asks for a file to open. None when the player cancels.
    fn open_file(&mut self, title: &str, folder: Option<&Path>, filters: &[Filter]) -> Option<PathBuf>;
    /// Asks where to save a file. None when the player cancels.
    fn save_file(&mut self, title: &str, folder: &Path, name: &str) -> Option<PathBuf>;
}

/// The system's dialogs.
pub struct Native;

impl FileDialogs for Native {
    fn open_file(&mut self, title: &str, folder: Option<&Path>, filters: &[Filter]) -> Option<PathBuf> {
        let mut d = rfd::FileDialog::new().set_title(title);
        if let Some(f) = folder.filter(|f| f.is_dir()) {
            d = d.set_directory(f);
        }
        for (name, exts) in filters {
            d = if exts.is_empty() { d.add_filter(*name, &["*"]) } else { d.add_filter(*name, exts) };
        }
        d.pick_file()
    }

    fn save_file(&mut self, title: &str, folder: &Path, name: &str) -> Option<PathBuf> {
        let mut d = rfd::FileDialog::new().set_title(title).set_file_name(name);
        if folder.is_dir() {
            d = d.set_directory(folder);
        }
        d.save_file()
    }
}

/// Dialogs for tests: each call takes the next path of its queue, and an empty queue cancels. `asked` records
/// the titles.
#[derive(Default, Clone)]
pub struct Scripted(pub std::rc::Rc<std::cell::RefCell<ScriptedState>>);

#[derive(Default)]
pub struct ScriptedState {
    pub open: VecDeque<PathBuf>,
    pub save: VecDeque<PathBuf>,
    pub asked: Vec<String>,
}

impl FileDialogs for Scripted {
    fn open_file(&mut self, title: &str, _folder: Option<&Path>, _filters: &[Filter]) -> Option<PathBuf> {
        let mut s = self.0.borrow_mut();
        s.asked.push(title.to_string());
        s.open.pop_front()
    }

    fn save_file(&mut self, title: &str, _folder: &Path, name: &str) -> Option<PathBuf> {
        let mut s = self.0.borrow_mut();
        s.asked.push(format!("{title}: {name}"));
        s.save.pop_front().map(|p| if p.is_dir() { p.join(name) } else { p })
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The app's own dialogs: Import XML, Export patch, Build all, Import model, questions and messages. One is open at
// a time, over the window.

use crate::app::{Act, Window};
use crate::style::{self, check_line, muted};
use glovebox_editor::sanitize::{match_scenes, sanitize_level_xml, FormatModel, SanitizeResult, MAX_CHARS};

pub enum Dialog {
    /// A note with an OK button. `then` runs after OK.
    Message { title: String, text: String, then: Option<Act> },
    /// A question with Yes and No. `yes` runs after Yes.
    Confirm { title: String, text: String, yes: Option<Act> },
    ImportXml(Box<ImportXml>),
    Patch { lines: Vec<(&'static str, String)>, quickboot: bool },
    BuildAll { levels: Vec<(String, String, String, bool)>, boot: Option<usize> },
    ImportModel(Box<ImportModel>),
}

pub struct ImportXml {
    pub name: String,
    /// The report: check lines, then small lines.
    pub lines: Vec<(&'static str, String)>,
    pub small: Vec<String>,
    pub res: Option<SanitizeResult>,
    /// The levels the file can go into: (id, label).
    pub targets: Vec<(String, String)>,
    pub target: usize,
}

pub struct ImportModel {
    pub path: Option<PathBuf>,
    pub name: String,
    pub scale: String,
    /// The model's triangle count and size, or why it can't be read.
    pub stats: Option<Result<(usize, [f64; 3]), String>>,
    /// The size of the level's landscape.
    pub level: Option<[f64; 3]>,
}

fn s<'a>(v: &'a serde_json::Value, key: &str) -> &'a str {
    v.get(key).and_then(|x| x.as_str()).unwrap_or("")
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

/// Import XML: the file is read and checked here. Nothing else sees it first.
pub fn open_import_dialog(w: &mut Window, path: Option<PathBuf>) {
    let mut d = ImportXml { name: "No file chosen".to_string(), lines: Vec::new(), small: Vec::new(), res: None, targets: Vec::new(), target: 0 };
    if let Some(p) = path {
        read_xml(w, &mut d, &p);
    }
    w.dialog = Some(Dialog::ImportXml(Box::new(d)));
}

fn read_xml(w: &Window, d: &mut ImportXml, path: &Path) {
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let file = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    d.name = format!("{file} · {:.1} KB", size as f64 / 1024.0);
    (d.lines, d.small, d.res, d.targets) = (Vec::new(), Vec::new(), None, Vec::new());
    if size > MAX_CHARS as u64 {
        d.lines.push(("err", format!("This file is {:.1} MB; level XML is at most {} MB. It was not read.", size as f64 / 1048576.0, MAX_CHARS >> 20)));
        return;
    }
    let Some(text) = std::fs::read(path).ok().and_then(|b| String::from_utf8(b).ok()) else {
        d.lines.push(("err", "The file couldn’t be read.".to_string()));
        return;
    };
    let ed = &w.ed;
    let decomp = ed.backend.decomp();
    let res = if std::ptr::eq(decomp, glovebox_core::tables::decomp()) {
        sanitize_level_xml(&text, FormatModel::shared(), "file")
    } else {
        sanitize_level_xml(&text, &FormatModel::new(decomp), "file")
    };
    if !res.ok {
        d.lines.push(("err", format!("Not imported: {} problem{} found.", res.errors.len(), plural(res.errors.len()))));
        d.small.extend(res.errors.iter().take(12).cloned());
        if res.errors.len() > 12 {
            d.small.push(format!("…and {} more.", res.errors.len() - 12));
        }
        d.res = Some(res);
        return;
    }
    let matches: Vec<String> = match_scenes(res.info.name.as_deref(), ed.scenes.iter()).into_iter().map(|m| s(m, "id").to_string()).collect();
    let named = res.info.name.as_deref().filter(|n| !n.is_empty()).map(|n| format!("“{n}”, ")).unwrap_or_default();
    d.lines.push(("ok", format!("Valid level XML: {named}{} commands. Every element, field and value checked against libgarib’s level format.", res.info.elements)));
    d.lines.extend(res.warnings.iter().map(|x| ("note", x.clone())));
    if let Some(root) = &res.root {
        let rules = ed.run_rules(root);
        if let Some(first) = rules.first() {
            d.lines.push(("note", format!("{} level-parser warning{} (shown in Checks after import), e.g. {}", rules.len(), plural(rules.len()), first.msg)));
        }
    }
    let here = ed.scene_id().to_string();
    let mut ids = matches.clone();
    if !ids.contains(&here) {
        ids.push(here.clone());
    }
    for i in &ids {
        let name = ed.scenes.iter().find(|x| s(x, "id") == i).map(|x| s(x, "name")).unwrap_or("");
        let why = if matches.contains(i) {
            " — matches this file"
        } else if *i == here {
            " — current level"
        } else {
            ""
        };
        d.targets.push((i.clone(), format!("{name} ({i}){why}")));
    }
    let want = if matches.len() == 1 { &matches[0] } else { &here };
    d.target = ids.iter().position(|i| i == want).unwrap_or(0);
    d.res = Some(res);
}

/// Patches start the game normally unless the author opts in to booting straight into the level. The "boot here"
/// toolbar option only applies to Build ROM.
pub fn open_patch_dialog(w: &mut Window) {
    let ed = &w.ed;
    let Some(scene) = &ed.scene else { return };
    let name = s(scene, "name");
    let (obj_bank, tex_bank) = (s(scene, "importObjectBank"), s(scene, "importTextureBank"));
    let mut lines: Vec<(&'static str, String)> = Vec::new();
    let imports = ed.current_imports();
    if ed.is_edited() {
        lines.push(("ok", format!("{name}, with your edits.")));
    } else {
        lines.push(("note", format!("{name} has no edits; the patch would only change how the ROM is laid out.")));
    }
    let game = imports.iter().filter(|i| ed.catalog.get(*i).is_none_or(|c| c.custom.is_none())).count();
    let custom = imports.len() - game;
    if custom > 0 {
        lines.push(("note", format!("Uses {custom} piece(s) of your imported models. The patch contains them, in this level’s rebuilt model bank ({obj_bank}) and texture bank ({tex_bank}).")));
    }
    if game > 0 && ed.runtime_imports() {
        lines.push(("ok", format!("Uses {game} model(s) from other levels. The game mod loads them from the player’s own ROM. The patch contains no game data. Players need the Expansion Pak.")));
    } else if game > 0 {
        lines.push(("note", format!("Uses {game} model(s) from other levels. The patch then contains this level’s rebuilt model bank ({obj_bank}) and texture bank ({tex_bank}) in full, game data included. It is much larger than a plain edit.")));
    } else if custom == 0 {
        lines.push(("ok", "No imported models: the patch is a true diff and contains only what your edits add.".to_string()));
    }
    if !ed.mod_users().is_empty() {
        lines.push(("note", "Rotates or scales enemies, platforms, powerups or water. The patch adds the Expansion Pak game mod (about 5 KB). Players need the Expansion Pak; without one the game skips those rotations and scales.".to_string()));
    }
    if ed.needs_pak_heap() {
        lines.push(("note", "The imported models need more memory than the game’s own heap. The patch adds the Expansion Pak game mod, which makes the heap about 4 MB larger. Players need the Expansion Pak; without one the game runs out of memory in this level.".to_string()));
    }
    w.dialog = Some(Dialog::Patch { lines, quickboot: false });
}

/// Build all: every level with edits (its draft, or a title, music or clock) in one ROM or BPS patch.
pub fn open_all_dialog(w: &mut Window) {
    let levels = w.ed.project_levels();
    let rows = levels
        .into_iter()
        .map(|(sid, name, xml)| {
            let info = w.ed.drafts.info(&sid);
            let parts = [
                xml.is_some().then(|| "edited".to_string()),
                info.name.as_ref().map(|n| format!("title {n}")),
                info.song.map(|n| format!("music {n}")),
                info.bonus_time.map(|_| "clock".to_string()),
            ];
            let extra = parts.into_iter().flatten().collect::<Vec<_>>().join(", ");
            let label = format!("{name} ({sid})  {extra}");
            (sid, name, label, true)
        })
        .collect();
    w.dialog = Some(Dialog::BuildAll { levels: rows, boot: None });
}

/// Import model: one .glb, converted into Glover models by the back end.
pub fn open_model_dialog(w: &mut Window, path: Option<PathBuf>) {
    let mut land = glovebox_editor::vec::Box3::new();
    for (i, e) in w.ed.entities.iter().enumerate() {
        if e.kind == "land" {
            land.add_box(&w.ed.obj_box(i));
        }
    }
    let mut d = ImportModel { path: None, name: String::new(), scale: "1".to_string(), stats: None, level: (!land.empty()).then(|| land.size()) };
    if let Some(p) = path {
        read_model(&mut d, p);
    }
    w.dialog = Some(Dialog::ImportModel(Box::new(d)));
}

fn read_model(d: &mut ImportModel, path: PathBuf) {
    if d.name.is_empty() {
        let file = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let lower = file.to_lowercase();
        let stem = if lower.ends_with(".glb") { &file[..file.len() - 4] } else if lower.ends_with(".gltf") { &file[..file.len() - 5] } else { &file };
        d.name = stem.chars().take(40).collect();
    }
    let stats = || -> Result<(usize, [f64; 3]), String> {
        let data = std::fs::read(&path).map_err(|e| e.to_string())?;
        let gltf = glovebox_core::custom::gltf::Gltf::parse(&data).map_err(|e| e.0)?;
        let (tris, count) = gltf.triangles().map_err(|e| e.0)?;
        let mut b = glovebox_editor::vec::Box3::new();
        for t in &tris.p {
            b.add_points(t);
        }
        Ok((count, if b.empty() { [0.0; 3] } else { b.size() }))
    };
    d.stats = Some(stats());
    d.path = Some(path);
}

/// 1234 as "1,234".
fn thousands(n: usize) -> String {
    let t = n.to_string();
    let mut out = String::new();
    for (i, c) in t.chars().enumerate() {
        if i > 0 && (t.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A dialog's title: the title bar and the theme's divider.
fn title_bar(ui: &mut egui::Ui, title: &str) {
    ui.spacing_mut().item_spacing.y = 0.0;
    style::title_bar(ui, title);
    style::divider(ui);
    ui.spacing_mut().item_spacing.y = 6.0;
    ui.add_space(6.0);
}

fn buttons(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(6.0);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), add);
}

/// Draws the open dialog and does what its buttons ask.
pub fn ui(w: &mut Window, ctx: &egui::Context) {
    let Some(mut dialog) = w.dialog.take() else { return };
    let mut keep = true;
    let mut then: Option<Act> = None;
    // A question takes Enter for Yes and Esc for No. A note takes either for OK.
    let (enter, esc) = ctx.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
    // The dialog is a box in the theme's frame, with a title bar and the divider under it
    let backdrop = style::theme(ctx).scrim;
    let modal = egui::Modal::new(egui::Id::unique("dialog")).frame(egui::Frame::NONE).backdrop_color(backdrop).show(ctx, |ui| {
        style::framed(ui, |ui| {
        egui::Frame::new().inner_margin(egui::Margin::same(12)).show(ui, |ui| {
        ui.set_width(520.0f32.min(ctx.content_rect().width() - 60.0));
        ui.spacing_mut().item_spacing.y = 6.0;
        match &mut dialog {
            Dialog::Message { title, text, then: after } => {
                title_bar(ui, title);
                ui.label(text.as_str());
                buttons(ui, |ui| {
                    if ui.add(style::primary("OK")).clicked() || enter || esc {
                        keep = false;
                        then = after.take();
                    }
                });
            }
            Dialog::Confirm { title, text, yes } => {
                title_bar(ui, title);
                ui.label(text.as_str());
                buttons(ui, |ui| {
                    if ui.add(style::primary("Yes")).clicked() || enter {
                        keep = false;
                        then = yes.take();
                    }
                    if ui.button("No").clicked() || esc {
                        keep = false;
                    }
                });
            }
            Dialog::ImportXml(d) => {
                title_bar(ui, "Import level XML");
                muted(ui, "The editor reads and checks the file. It must be libgarib level XML. Anything unknown is refused.");
                let mut chosen = None;
                ui.horizontal(|ui| {
                    if ui.button("Choose file…").clicked() {
                        chosen = w.dialogs.open_file("Import level XML", None, &[("Level XML", &["xml"]), ("All files", &[])]);
                    }
                    muted(ui, &d.name);
                });
                if let Some(p) = chosen {
                    read_xml(w, d, &p);
                }
                muted(ui, "or drop an .xml file on the window");
                for (level, text) in &d.lines {
                    check_line(ui, level, text, false, None, None, None);
                }
                for t in &d.small {
                    style::src(ui, t);
                }
                ui.horizontal(|ui| {
                    ui.label("Import into");
                    let shown = d.targets.get(d.target).map(|t| t.1.clone()).unwrap_or_default();
                    let r = egui::ComboBox::from_id_salt("import target").icon(style::combo_icon).selected_text(shown).width(360.0).truncate().show_ui(ui, |ui| {
                        for (i, t) in d.targets.iter().enumerate() {
                            if ui.selectable_label(d.target == i, &t.1).clicked() {
                                d.target = i;
                            }
                        }
                    });
                    r.response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, "Import into"));
                });
                let ok = d.res.as_ref().is_some_and(|r| r.ok) && !d.targets.is_empty();
                buttons(ui, |ui| {
                    if ui.add_enabled(ok, style::primary("Import")).clicked() {
                        keep = false;
                        if let (Some(res), Some(target)) = (d.res.take(), d.targets.get(d.target)) {
                            let tid = target.0.clone();
                            then = Some(Box::new(move |w: &mut Window| {
                                if tid != w.ed.scene_id() {
                                    w.load_scene_then(&tid, Box::new(move |w: &mut Window| w.ed.confirm_import(&res)));
                                } else {
                                    w.ed.confirm_import(&res);
                                }
                            }));
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        keep = false;
                    }
                });
            }
            Dialog::Patch { lines, quickboot } => {
                title_bar(ui, "Export patch");
                muted(ui, "A BPS patch for other players. They apply it to their own Glover (USA) ROM with patcher.html. Share the .bps and patcher.html (one file, works offline).");
                for (level, text) in lines.iter() {
                    check_line(ui, level, text, false, None, None, None);
                }
                style::checkbox(ui, quickboot, "Patched ROM boots straight into this level (skips the title screens, for testing)");
                buttons(ui, |ui| {
                    if ui.add(style::primary("Export .bps")).clicked() {
                        keep = false;
                        let qb = *quickboot;
                        then = Some(Box::new(move |w: &mut Window| w.export("bps", Some(qb))));
                    }
                    if ui.button("Cancel").clicked() {
                        keep = false;
                    }
                });
            }
            Dialog::BuildAll { levels, boot } => {
                title_bar(ui, "Build all edited levels");
                if levels.is_empty() {
                    muted(ui, "No level has edits yet.");
                    if ui.button("Close").clicked() {
                        keep = false;
                    }
                    return;
                }
                muted(ui, "Every checked level goes into one ROM or patch, with its title, music and clock. Two levels of one world can't both copy models into their banks; the game mod then loads them from the ROM.");
                egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                    for (_, _, label, on) in levels.iter_mut() {
                        style::checkbox(ui, on, label.as_str());
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Boot into");
                    let shown = boot.and_then(|i| levels.get(i)).map_or("the title screen (normal start)".to_string(), |l| l.1.clone());
                    let r = egui::ComboBox::from_id_salt("boot into").icon(style::combo_icon).selected_text(shown).width(320.0).show_ui(ui, |ui| {
                        if ui.selectable_label(boot.is_none(), "the title screen (normal start)").clicked() {
                            *boot = None;
                        }
                        for (i, l) in levels.iter().enumerate() {
                            if ui.selectable_label(*boot == Some(i), &l.1).clicked() {
                                *boot = Some(i);
                            }
                        }
                    });
                    r.response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, "Boot into"));
                });
                let mut choice = None;
                buttons(ui, |ui| {
                    if ui.add(style::primary("Build ROM")).clicked() {
                        choice = Some("rom");
                    }
                    if ui.button("Export .bps").clicked() {
                        choice = Some("bps");
                    }
                    if ui.button("Cancel").clicked() {
                        keep = false;
                    }
                });
                if let Some(fmt_) = choice {
                    keep = false;
                    let sids: Vec<String> = levels.iter().filter(|l| l.3).map(|l| l.0.clone()).collect();
                    let qb = boot.and_then(|i| levels.get(i)).map(|l| l.0.clone());
                    then = Some(Box::new(move |w: &mut Window| w.build_all(sids, fmt_, qb)));
                }
            }
            Dialog::ImportModel(d) => {
                title_bar(ui, "Import model");
                muted(ui, "One .glb file (binary glTF) with its textures inside. Blender and most model rippers export these. The editor turns it into Glover models. Textures shrink to N64 textures of at most 2,048 pixels. A big model, like a map, is cut into solid pieces of at most 1,500 triangles.");
                let mut chosen = None;
                ui.horizontal(|ui| {
                    if ui.button("Choose .glb…").clicked() {
                        chosen = w.dialogs.open_file("Import model", None, &[("glTF models", &["glb", "gltf"]), ("All files", &[])]);
                    }
                    let name = d.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string());
                    muted(ui, name.as_deref().unwrap_or("No file chosen"));
                });
                if let Some(p) = chosen {
                    read_model(d, p);
                }
                ui.horizontal(|ui| {
                    ui.label("Name");
                    let r = ui.add(egui::TextEdit::singleline(&mut d.name).char_limit(40).hint_text("Model").desired_width(f32::INFINITY));
                    style::name(&r, "Name");
                });
                ui.horizontal(|ui| {
                    ui.label("Scale");
                    let (r, _) = crate::numfield::bound(ui, egui::Id::unique("import model scale"), &mut d.scale, crate::numfield::Spec::float(0.01).at_least(0.0), |t| t.desired_width(f32::INFINITY));
                    style::name(&r, "Scale");
                });
                let sc = glovebox_core::pyfmt::parse_float(d.scale.trim()).filter(|v| v.is_finite()).unwrap_or(0.0);
                let level = d.level.map(|lv| format!(" This level is about {} × {} units across.", lv[0].round(), lv[2].round())).unwrap_or_default();
                let text = match (&d.path, &d.stats) {
                    (None, _) => format!("Choose a .glb file.{level}"),
                    (_, None) => "Reading the model…".to_string(),
                    (_, Some(Err(e))) => format!("This file can't be read as a glTF model: {e}"),
                    (_, Some(Ok((tris, size)))) => {
                        let pieces = if *tris <= 1500 { 1 } else { 1usize << (*tris as f64 / 1500.0).log2().ceil() as u32 };
                        format!(
                            "{} triangles, in {pieces} piece{}. At this scale it is {} × {} × {} units (width × height × depth).{level}",
                            thousands(*tris),
                            plural(pieces),
                            (size[0] * sc).round(),
                            (size[1] * sc).round(),
                            (size[2] * sc).round()
                        )
                    }
                };
                muted(ui, &text);
                let ok = d.path.is_some() && sc > 0.0 && matches!(d.stats, Some(Ok(_)));
                buttons(ui, |ui| {
                    if ui.add_enabled(ok, style::primary("Import and place")).clicked() {
                        keep = false;
                        if let Some(path) = d.path.clone() {
                            let name = if d.name.trim().is_empty() { "Model".to_string() } else { d.name.trim().to_string() };
                            then = Some(Box::new(move |w: &mut Window| import_model(w, path, name, sc)));
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        keep = false;
                    }
                });
            }
        }
        });
        });
    });
    if modal.should_close() && !matches!(dialog, Dialog::Message { .. } | Dialog::Confirm { .. }) {
        keep = false;
    }
    if keep {
        w.dialog = Some(dialog);
    } else {
        ctx.request_repaint();
    }
    if let Some(act) = then {
        act(w);
    }
}

/// Converts a model file into Glover models and places it (a background job).
pub fn import_model(w: &mut Window, path: PathBuf, name: String, scale: f64) {
    w.loading = format!("Converting {name} into Glover models…");
    let backend = w.ed.backend.clone();
    let n2 = name.clone();
    w.jobs.run(
        move || {
            let data = std::fs::read(&path).map_err(|e| e.to_string())?;
            backend.import_custom(&data, &n2, scale).map_err(|e| e.0)
        },
        move |w: &mut Window, res: Result<serde_json::Value, String>| {
            w.loading.clear();
            match res {
                Ok(meta) => {
                    w.reload_catalog();
                    w.ed.place_custom(s(&meta, "set"), "LandActor", None);
                }
                Err(e) => w.set_status(&format!("Could not import {name}: {e}"), "err"),
            }
        },
    );
}
