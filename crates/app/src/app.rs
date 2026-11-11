//! The application: the setup screen or the editor's window, and what the window does that is not one panel's
//! own: starting, opening levels, the editor's events, builds, Test level and the keyboard.
//! A port of the former Python editor's `window.py`.

use crate::dialogs::{Dialog, FileDialogs};
use crate::jobs::Jobs;
use crate::settings::{IniSettings, Shared};
use crate::{inspector, outliner, palette, path_panel, setup, shell, style, viewport};
use glovebox_core::backend::{Backend, CatalogEntry};
use glovebox_core::xml::NodeId;
use glovebox_editor::commands::Format;
use glovebox_editor::drafts::Drafts;
use glovebox_editor::editor::{BuildJob, BuildOutput, Check, Delays, Editor, Event};
use glovebox_editor::gizmo::Mode;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Something a widget asks for. It runs after the panels are drawn.
pub type Act = Box<dyn FnOnce(&mut Window)>;

/// What the app starts with.
pub struct Start {
    /// The user folder.
    pub home: PathBuf,
    /// A back end to use (the fixture). None opens the ROM copy in the user folder, or asks for the ROM.
    pub backend: Option<Arc<dyn Backend>>,
    pub dialogs: Box<dyn FileDialogs>,
}

pub enum Screen {
    Setup(setup::Setup),
    Editor(Box<Window>),
}

pub struct App {
    pub screen: Screen,
    home: PathBuf,
    first_frame: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, start: Start) -> App {
        style::apply(&cc.egui_ctx);
        let Start { home, backend, dialogs } = start;
        let screen = match backend {
            Some(b) => Screen::Editor(Box::new(Window::new(cc, b, &home, dialogs))),
            None => match open_rom(&home) {
                Ok(b) => Screen::Editor(Box::new(Window::new(cc, b, &home, dialogs))),
                Err(e) => Screen::Setup(setup::Setup::new(&home, dialogs, e)),
            },
        };
        App { screen, home, first_frame: true }
    }

    /// The editor's window, once the setup is done.
    pub fn window(&mut self) -> Option<&mut Window> {
        match &mut self.screen {
            Screen::Editor(w) => Some(w),
            Screen::Setup(_) => None,
        }
    }
}

/// Opens the ROM copy of the user folder. The error is None when there is no copy yet.
fn open_rom(home: &Path) -> Result<Arc<dyn Backend>, Option<String>> {
    let rom = shell::rom_path(home);
    if !rom.is_file() {
        return Err(None);
    }
    let t = std::time::Instant::now();
    match glovebox_core::backend::GameBackend::open(&rom, home) {
        Ok(b) => {
            shell::log(&format!("The game data opened in {:.0} ms.", t.elapsed().as_secs_f64() * 1000.0));
            Ok(Arc::new(b))
        }
        Err(e) => Err(Some(format!("The ROM copy in the user folder can't be used: {e}"))),
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if std::mem::take(&mut self.first_frame) {
            shell::log(&format!("First frame {:.0} ms after the start.", shell::since_start() * 1000.0));
        }
        let mut next = None;
        match &mut self.screen {
            Screen::Setup(s) => {
                if s.ui(ui) {
                    match (open_rom(&self.home), frame.wgpu_render_state()) {
                        (Ok(b), rs) => {
                            let dialogs = s.take_dialogs();
                            next = Some(Screen::Editor(Box::new(Window::with_state(ui.ctx(), rs.cloned(), b, &self.home, dialogs))));
                        }
                        (Err(e), _) => s.error = e.unwrap_or_default(),
                    }
                }
            }
            Screen::Editor(w) => w.ui(ui),
        }
        if let Some(n) = next {
            self.screen = n;
            ui.ctx().request_repaint();
        }
    }

    fn on_exit(&mut self) {
        if let Screen::Editor(w) = &mut self.screen {
            w.ed.flush_draft();
        }
    }
}

/// The editor's window.
pub struct Window {
    pub ed: Editor,
    pub home: PathBuf,
    pub settings: Shared,
    pub dialogs: Box<dyn FileDialogs>,
    pub jobs: Jobs<Window>,
    /// The kit's previews and the model pictures get their own worker: a tab of them must not hold up builds.
    pub side_jobs: Jobs<Window>,
    /// A level is being opened.
    pub busy: bool,
    /// The note in the middle of the view. Empty hides it.
    pub loading: String,
    /// The status line and its class: "ok", "err" or "".
    pub status: (String, String),
    pub status_size: String,
    pub hud_level: String,
    pub view: viewport::View,
    pub outliner: outliner::State,
    pub inspector: inspector::State,
    pub palette: palette::State,
    pub palette_open: bool,
    pub path_panel: path_panel::State,
    /// The lines of the Checks panel, worked out after each rebuild.
    pub checks: Vec<Check>,
    /// Build ROM's "boot here".
    pub quickboot: bool,
    pub dialog: Option<Dialog>,
    /// The levels with a draft, for the marks in the level list.
    pub edited_levels: BTreeSet<String>,
    pub acts: Vec<Act>,
    /// What to do once a level is open: its id and the action.
    pub after_load: Option<(String, Act)>,
    /// The last file written by Build ROM, Export and the like.
    pub last_saved: Option<PathBuf>,
    clock: std::time::Instant,
    ctx: egui::Context,
}

/// The format of the back end's level table.
pub fn with_format<R>(ed: &Editor, f: impl FnOnce(&Format) -> R) -> R {
    let decomp = ed.backend.decomp();
    if std::ptr::eq(decomp, glovebox_core::tables::decomp()) {
        f(Format::shared())
    } else {
        f(&Format::new(decomp))
    }
}

fn s<'a>(v: &'a serde_json::Value, key: &str) -> &'a str {
    v.get(key).and_then(|x| x.as_str()).unwrap_or("")
}

impl Window {
    pub fn new(cc: &eframe::CreationContext<'_>, backend: Arc<dyn Backend>, home: &Path, dialogs: Box<dyn FileDialogs>) -> Window {
        Window::with_state(&cc.egui_ctx, cc.wgpu_render_state.clone(), backend, home, dialogs)
    }

    pub fn with_state(ctx: &egui::Context, rs: Option<eframe::egui_wgpu::RenderState>, backend: Arc<dyn Backend>, home: &Path, dialogs: Box<dyn FileDialogs>) -> Window {
        let _ = std::fs::create_dir_all(home);
        let settings = Shared::new(IniSettings::open(home.join("settings.ini")));
        let mut ed = Editor::new(backend, Drafts::new(home.join("drafts")), Some(Box::new(settings.clone())));
        ed.delays = Some(Delays::default());
        ed.format_time = shell::local_date_time;
        ed.show_fog = settings.get("fog").as_deref() != Some("off");
        if settings.get("triggers").as_deref() == Some("off") {
            ed.hidden.insert("trigger".to_string());
        }
        let view = viewport::View::new(rs, &mut ed);
        let mut w = Window {
            ed,
            home: home.to_path_buf(),
            settings,
            dialogs,
            jobs: Jobs::new(2, ctx.clone()),
            side_jobs: Jobs::new(1, ctx.clone()),
            busy: false,
            loading: String::new(),
            status: ("Starting…".to_string(), String::new()),
            status_size: String::new(),
            hud_level: String::new(),
            view,
            outliner: outliner::State::default(),
            inspector: inspector::State::default(),
            palette: palette::State::default(),
            palette_open: false,
            path_panel: path_panel::State::default(),
            checks: Vec::new(),
            quickboot: true,
            dialog: None,
            edited_levels: BTreeSet::new(),
            acts: Vec::new(),
            after_load: None,
            last_saved: None,
            clock: std::time::Instant::now(),
            ctx: ctx.clone(),
        };
        w.start();
        w
    }

    pub fn now(&self) -> f64 {
        self.clock.elapsed().as_secs_f64()
    }

    pub fn set_status(&mut self, msg: &str, cls: &str) {
        self.ed.set_status(msg, cls);
    }

    // -----------------------------------------------------------------------------------------------------------
    // Start and levels

    fn start(&mut self) {
        self.ed.start();
        self.loading = "Indexing every model in the game…".to_string();
        self.busy = true;
        let backend = self.ed.backend.clone();
        self.jobs.run(
            move || backend.catalog(),
            |w: &mut Window, cat| {
                match cat {
                    Ok(c) => w.ed.set_catalog(c),
                    Err(e) => {
                        w.ed.set_catalog(BTreeMap::new());
                        w.set_status(&format!("Couldn’t index the game’s models: {e}"), "err");
                    }
                }
                w.busy = false;
                w.after_catalog();
            },
        );
    }

    fn after_catalog(&mut self) {
        let has = |id: &str| self.ed.scenes.iter().any(|x| s(x, "id") == id);
        let mut start = self.settings.get("scene").unwrap_or_default();
        if !has(&start) {
            start = if has("0a") { "0a".to_string() } else { self.ed.scenes.first().map(|x| s(x, "id").to_string()).unwrap_or_default() };
        }
        self.load_scene(&start);
    }

    /// Opens a level. The slow part runs on a worker thread.
    pub fn load_scene(&mut self, sid: &str) {
        if sid.is_empty() || self.busy {
            return;
        }
        let Some(name) = self.ed.scenes.iter().find(|x| s(x, "id") == sid).map(|x| s(x, "name").to_string()) else { return };
        self.busy = true;
        self.loading = format!("Loading {name}…");
        let job = self.ed.level_job(sid);
        self.jobs.run(
            move || job.run(),
            |w: &mut Window, res| {
                let res = res.and_then(|data| w.ed.apply_level(data));
                if let Err(e) = res {
                    shell::log(&format!("Failed to load level: {e}"));
                    w.set_status(&format!("Failed to load level: {e}"), "err");
                }
                w.busy = false;
                w.loading.clear();
                if let Some((sid, act)) = w.after_load.take() {
                    if w.ed.scene_id() == sid {
                        act(w);
                    }
                }
            },
        );
    }

    /// Opens a level, then runs `then` when it is the open level.
    pub fn load_scene_then(&mut self, sid: &str, then: Act) {
        if self.busy {
            return;
        }
        self.after_load = Some((sid.to_string(), then));
        self.load_scene(sid);
    }

    fn on_scene_loaded(&mut self) {
        let ed = &self.ed;
        let name = ed.scene.as_ref().map(|x| s(x, "name").to_string()).unwrap_or_default();
        self.hud_level = format!("{} — {}", name, ed.doc.attr(ed.doc.root(), "name"));
        self.status_size = ed.orig_lev_size.map(|n| format!("{n} B .lev (original)")).unwrap_or_default();
        self.view.forget_level();
        self.inspector = inspector::State::default();
        self.palette.thumbs_wanted.clear();
    }

    /// The label of a level in the level list.
    pub fn scene_label(&self, scene: &serde_json::Value) -> String {
        let sid = s(scene, "id");
        format!("   {}  ({}){}", s(scene, "name"), sid, if self.edited_levels.contains(sid) { "  ● edited" } else { "" })
    }

    /// The autosave chip's text, tooltip and whether the level has edits.
    pub fn chip(&self) -> (String, String, bool) {
        let ed = &self.ed;
        if !ed.is_edited() {
            return ("Original level".to_string(), "No edits to this level.".to_string(), false);
        }
        let text = match ed.draft_saved_at {
            Some(t) => format!("Edited · autosaved {}", shell::local_time(t, "%H:%M:%S")),
            None => "Edited · saving…".to_string(),
        };
        let tip = format!("Your edits are saved in the user folder ({}) and come back when you reopen this level.", ed.drafts.folder.display());
        (text, tip, true)
    }

    /// Handles what the editor reported since the last call.
    pub fn pump(&mut self) {
        let events = self.ed.drain_events();
        if events.is_empty() {
            return;
        }
        let mut rebuilt = false;
        for e in events {
            match e {
                Event::Status { message, class } => self.status = (message, class),
                Event::SceneLoaded => self.on_scene_loaded(),
                Event::Rebuilt => {
                    rebuilt = true;
                    self.view.rebuilt();
                }
                Event::ImportsChanged => rebuilt = true,
                Event::Edited => self.edited_levels = self.ed.drafts.list().into_keys().collect(),
                Event::Loading(t) => self.loading = t,
                Event::SelectionChanged | Event::ViewChanged | Event::ModeChanged => {}
            }
        }
        if rebuilt && self.ed.has_level {
            self.checks = self.ed.checks();
            self.outliner.dirty = true;
        }
        self.view.dirty = true;
        self.ctx.request_repaint();
    }

    // -----------------------------------------------------------------------------------------------------------
    // Tools

    pub fn toggle_palette(&mut self, on: Option<bool>) {
        self.palette_open = on.unwrap_or(!self.palette_open);
        self.palette.shown_tab = None;
        if self.palette_open {
            self.palette.side_tab = 1;
        }
    }

    pub fn set_path_mode(&mut self, on: bool) {
        self.ed.set_path_mode(on);
        if on {
            self.palette.side_tab = 2;
        }
    }

    pub fn toggle_fog(&mut self, on: bool) {
        self.ed.show_fog = on;
        self.settings.set("fog", if on { "on" } else { "off" });
        self.view.dirty = true;
    }

    pub fn set_fly(&mut self, on: bool) {
        self.view.set_fly(&mut self.ed, on);
    }

    pub fn focus_selected(&mut self) {
        if let Some(e) = self.ed.selected() {
            self.ed.focus_on(e);
        }
    }

    pub fn reset_level(&mut self) {
        let name = self.ed.scene.as_ref().map(|x| s(x, "name").to_string()).unwrap_or_default();
        if !self.ed.is_edited() {
            return self.set_status(&format!("{name} is already the shipped level."), "");
        }
        self.dialog = Some(Dialog::Confirm {
            title: "Reset level".to_string(),
            text: format!("Reset {name} to the shipped level? This discards your edits and their autosave. You can still undo it in this session."),
            yes: Some(Box::new(|w: &mut Window| w.ed.reset_level())),
        });
    }

    /// Makes a kit model and places it (a background job).
    pub fn place_kit(&mut self, id: String, name: String, vals: serde_json::Value, as_: &'static str) {
        self.loading = format!("Making {}…", name.to_lowercase());
        let backend = self.ed.backend.clone();
        self.jobs.run(
            move || backend.make_kit(&id, &vals),
            move |w: &mut Window, res| {
                w.loading.clear();
                match res {
                    Ok(meta) => {
                        w.reload_catalog();
                        w.ed.place_custom(s(&meta, "set"), as_, None);
                    }
                    Err(e) => w.set_status(&format!("Could not make {}: {e}", name.to_lowercase()), "err"),
                }
            },
        );
    }

    pub fn reload_catalog(&mut self) {
        if let Ok(c) = self.ed.backend.catalog() {
            self.ed.set_catalog(c);
        }
    }

    /// Gives the selected scenery a copy of its model with other worlds' textures (a background job).
    pub fn reskin(&mut self, node: NodeId, worlds: Vec<String>, seed: i64) {
        let ed = &self.ed;
        let Some(oid) = glovebox_editor::xmlutil::hex_id(ed.doc.get(node, "object_id")) else { return };
        let src = ed.catalog.get(&oid).and_then(|c| c.custom.as_ref()).and_then(|c| c.reskin_from.clone()).unwrap_or(oid);
        let src_name = ed.catalog.get(&src).map(|c| c.name.clone()).unwrap_or_else(|| src.clone());
        let (wher, sid, had) = (ed.doc.path_of(node), ed.scene_id().to_string(), ed.doc.get(node, "object_id").map(str::to_string));
        self.loading = format!("Making a new look for {src_name}…");
        let backend = self.ed.backend.clone();
        let w2 = worlds.clone();
        self.jobs.run(
            move || backend.reskin(&src, &w2, seed),
            move |w: &mut Window, res| {
                w.loading.clear();
                let new = match res {
                    Ok(n) => n,
                    Err(e) => return w.set_status(&format!("Could not make a new look: {e}"), "err"),
                };
                w.reload_catalog();
                // The level may have changed while the job ran (undo, another level): find the piece again
                let ed = &mut w.ed;
                let node = if ed.scene_id() == sid { ed.doc.node_at(&wher) } else { None };
                let Some(node) = node.filter(|n| ed.doc.get(*n, "object_id").map(str::to_string) == had) else {
                    return ed.set_status("The new look is ready, but the piece changed meanwhile. It is listed with your imported models.", "");
                };
                let piece = ed.catalog.get(&new).and_then(|c| c.custom.as_ref()).map(|c| c.piece.clone()).filter(|p| !p.is_empty());
                let name: String = piece.unwrap_or_else(|| ed.doc.attr(node, "name").to_string()).chars().take(7).collect();
                ed.edit(|ed| {
                    ed.doc.set(node, "object_id", &new);
                    ed.doc.set(node, "name", &name);
                });
                let titles: Vec<String> = worlds.iter().map(|x| title_case(x)).collect();
                ed.set_status(&format!("The model now has textures from {}. It is copied into the level's banks at ROM build.", titles.join(", ")), "ok");
            },
        );
    }

    pub fn remove_custom(&mut self, set: String) {
        let pieces = self.ed.custom_pieces(&set);
        let name = pieces.first().map(|p| p.1.bank.replace("Custom: ", "")).unwrap_or_else(|| set.clone());
        let n2 = name.clone();
        self.dialog = Some(Dialog::Confirm {
            title: "Remove model".to_string(),
            text: format!("Remove the imported model {name}? Levels that use it then show a placeholder, and can't be built until you remove those pieces."),
            yes: Some(Box::new(move |w: &mut Window| {
                let _ = w.ed.backend.delete_custom(&set);
                w.reload_catalog();
                w.set_status(&format!("Removed {n2}."), "ok");
            })),
        });
    }

    // -----------------------------------------------------------------------------------------------------------
    // Build, export and test

    /// Builds the level as "xml", "lev", "rom" or "bps" and asks where to save it.
    pub fn export(&mut self, fmt_: &'static str, quickboot: Option<bool>) {
        let qb = quickboot.unwrap_or(self.quickboot);
        let imports = self.ed.current_imports();
        if matches!(fmt_, "rom" | "bps") {
            let what = if imports.is_empty() {
                "Assembling the level and patching the ROM with libgarib".to_string()
            } else {
                format!("Importing {} model(s) and patching the ROM with libgarib", imports.len())
            };
            self.set_status(&format!("{what}{}", if fmt_ == "bps" { ", then diffing it against the original…" } else { "…" }), "");
        }
        // Reads the level now. The worker only builds.
        let job = match self.ed.build_job(fmt_, qb) {
            Ok(j) => j,
            Err(e) => return self.set_status(&e, "err"),
        };
        if fmt_ == "xml" {
            if let Ok(out) = job.run() {
                self.save(out);
            }
            return;
        }
        self.loading = "Building…".to_string();
        self.jobs.run(
            move || job.run(),
            move |w: &mut Window, res| {
                w.loading.clear();
                match res {
                    Ok(out) => {
                        if matches!(fmt_, "rom" | "lev") {
                            if let Some(n) = lev_bytes(&out.message) {
                                w.status_size = format!("{n} B .lev");
                            }
                        }
                        w.save(out);
                    }
                    Err(e) => w.set_status(&e, "err"),
                }
            },
        );
    }

    /// Asks where to save a built file and writes it. True when it was saved.
    pub fn save(&mut self, out: BuildOutput) -> bool {
        let BuildOutput { data, name, message } = out;
        let folder = self.settings.get("saveDir").map(PathBuf::from).filter(|p| p.is_dir()).or_else(home_dir).unwrap_or_else(|| PathBuf::from("."));
        let Some(path) = self.dialogs.save_file(&format!("Save {name}"), &folder, &name) else {
            self.set_status("Not saved.", "");
            return false;
        };
        if let Err(e) = std::fs::write(&path, &data) {
            self.set_status(&format!("Couldn’t save {}: {e}", path.display()), "err");
            return false;
        }
        if let Some(dir) = path.parent() {
            self.settings.set("saveDir", &dir.to_string_lossy());
        }
        let shown = path.display().to_string();
        let msg = if message.is_empty() {
            format!("Saved {shown}.")
        } else {
            message.replace(&format!("Built {name}"), &format!("Built {shown}")).replace(&format!("Exported {name}"), &format!("Exported {shown}"))
        };
        self.last_saved = Some(path);
        self.set_status(&msg, "ok");
        true
    }

    /// Runs a "build all" job and saves its file.
    pub fn build_all(&mut self, sids: Vec<String>, fmt_: &'static str, quickboot: Option<String>) {
        self.loading = format!("Building {} levels…", sids.len());
        let job: BuildJob = match self.ed.build_all_job(&sids, fmt_, quickboot) {
            Ok(j) => j,
            Err(e) => {
                self.loading.clear();
                return self.set_status(&e, "err");
            }
        };
        self.jobs.run(
            move || job.run(),
            |w: &mut Window, res| {
                w.loading.clear();
                match res {
                    Ok(out) => {
                        w.save(out);
                    }
                    Err(e) => w.set_status(&e, "err"),
                }
            },
        );
    }

    /// Builds the level into a ROM in the user folder and opens it in the emulator chosen in the settings.
    pub fn test_level(&mut self) {
        if !self.ed.has_level {
            return;
        }
        let emulator = self.settings.get("emulator").map(PathBuf::from).filter(|p| p.exists());
        match emulator {
            Some(e) => self.test_level_with(e),
            None => {
                self.dialog = Some(Dialog::Message {
                    title: "Test level".to_string(),
                    text: "Test level opens the built ROM in your emulator. Choose the emulator's program (for example ares, simple64 or Project64) in the next window.\n\n\
                           Expansion Pak game mod levels need the emulator's Expansion Pak turned on (most have it on)."
                        .to_string(),
                    then: Some(Box::new(|w: &mut Window| {
                        let filters: &[crate::dialogs::Filter] = if cfg!(windows) { &[("Programs", &["exe"]), ("All files", &[])] } else { &[("All files", &[])] };
                        match w.dialogs.open_file("Choose your emulator", None, filters) {
                            Some(p) => {
                                w.settings.set("emulator", &p.to_string_lossy());
                                w.test_level_with(p);
                            }
                            None => w.set_status("Test level needs an emulator.", "err"),
                        }
                    })),
                });
            }
        }
    }

    fn test_level_with(&mut self, emulator: PathBuf) {
        let imports = self.ed.current_imports();
        let msg = if imports.is_empty() { "Building the level to test…".to_string() } else { format!("Importing {} model(s) and building the level to test…", imports.len()) };
        self.set_status(&msg, "");
        let (sid, name) = (self.ed.scene_id().to_string(), self.ed.scene.as_ref().map(|x| s(x, "name").to_string()).unwrap_or_default());
        // The same build as Build ROM with boot here: title, music, clock and the extra-level setting included
        let job = match self.ed.build_job("rom", true) {
            Ok(j) => j,
            Err(e) => return self.set_status(&e, "err"),
        };
        self.loading = "Building the level to test…".to_string();
        let folder = self.home.join("test");
        self.jobs.run(
            move || -> Result<PathBuf, String> {
                let out = job.run()?;
                let path = folder.join(format!("glover-{sid}-test.z64"));
                std::fs::create_dir_all(&folder).and_then(|_| std::fs::write(&path, &out.data)).map_err(|e| format!("Couldn’t save {}: {e}", path.display()))?;
                Ok(path)
            },
            move |w: &mut Window, res| {
                w.loading.clear();
                match res {
                    Ok(path) => {
                        if shell::launch(&emulator, &path) {
                            let prog = emulator.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                            w.set_status(&format!("Testing {name} in {prog}. The ROM boots straight into the level: {}", path.display()), "ok");
                        } else {
                            w.settings.remove("emulator"); // the next Test level asks again
                            w.set_status(&format!("Built {}, but {} didn't start. Test level asks for the emulator again next time.", path.display(), emulator.display()), "err");
                        }
                    }
                    Err(e) => w.set_status(&e, "err"),
                }
            },
        );
    }

    /// Writes the patcher page into the user folder and opens it in the browser.
    pub fn open_patcher(&mut self) {
        let path = self.home.join("patcher.html");
        match std::fs::write(&path, PATCHER) {
            Ok(()) => {
                let url = format!("file://{}{}", if cfg!(windows) { "/" } else { "" }, path.to_string_lossy().replace('\\', "/"));
                self.ctx.open_url(egui::OpenUrl::new_tab(url));
                self.set_status(&format!("The patcher page is {}. Share it with your patch.", path.display()), "ok");
            }
            Err(e) => self.set_status(&format!("Couldn’t write {}: {e}", path.display()), "err"),
        }
    }

    // -----------------------------------------------------------------------------------------------------------
    // Keys and files dropped on the window

    fn keys(&mut self, ctx: &egui::Context) {
        let mods = ctx.input(|i| i.modifiers);
        let ctrl = mods.ctrl || mods.command;
        // Holding Ctrl suspends the gizmo: Ctrl+click reaches the object under it
        self.ed.gizmo.enabled = !ctrl;
        if self.dialog.is_some() || ctx.egui_wants_keyboard_input() {
            self.view.fly_keys.clear();
            return;
        }
        let events = ctx.input(|i| i.events.clone());
        for e in events {
            let egui::Event::Key { key, pressed, repeat, modifiers, .. } = e else { continue };
            let m = modifiers.ctrl || modifiers.command;
            if self.view.fly.on && !m {
                if let Some(k) = viewport::fly_key(key) {
                    if pressed {
                        self.view.fly_keys.insert(k);
                    } else {
                        self.view.fly_keys.remove(&k);
                    }
                    continue;
                }
            }
            if !pressed {
                continue;
            }
            self.key(key, m, modifiers.shift, repeat);
        }
    }

    /// One key press. `m` is Ctrl or the Mac's command key.
    pub fn key(&mut self, key: egui::Key, m: bool, shift: bool, repeat: bool) {
        use egui::Key;
        let ed = &mut self.ed;
        match key {
            Key::Z if m && shift => ed.redo(),
            Key::Z if m => ed.undo(),
            Key::Y if m => ed.redo(),
            Key::D if m => ed.duplicate(),
            Key::A if m => {
                let all = ed.pickable().into_iter().filter(|&i| !ed.entities[i].is_handle && ed.entities[i].kind != "land").collect();
                ed.set_selection(all);
            }
            _ if m => {}
            Key::Backspace if ed.path.on => {
                ed.path.pts.pop();
                ed.draw_path();
            }
            Key::Delete | Key::Backspace => ed.remove(),
            _ if repeat => {}
            Key::F => self.focus_selected(),
            Key::W => ed.set_mode(Mode::Translate),
            Key::E => ed.set_mode(Mode::Rotate),
            Key::R => ed.set_mode(Mode::Scale),
            Key::B => ed.set_box_mode(!ed.box_mode),
            Key::C => self.set_fly(!self.view.fly.on),
            Key::T => self.test_level(),
            Key::A => self.toggle_palette(None),
            Key::P => self.set_path_mode(!self.ed.path.on),
            Key::Escape => {
                if self.view.fly.on {
                    self.set_fly(false);
                } else if ed.path.on {
                    self.set_path_mode(false);
                } else if ed.box_mode {
                    ed.set_box_mode(false);
                } else if self.palette_open {
                    self.toggle_palette(Some(false));
                } else {
                    ed.set_selection(Vec::new());
                }
            }
            _ => {}
        }
    }

    /// A file dropped on the window: a level XML opens the import dialog, a model the Import model dialog.
    pub fn file_dropped(&mut self, path: PathBuf) {
        if !self.ed.has_level || self.busy {
            return;
        }
        let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        if ext == "glb" || ext == "gltf" {
            crate::dialogs::open_model_dialog(self, Some(path));
        } else {
            crate::dialogs::open_import_dialog(self, Some(path));
        }
    }

    // -----------------------------------------------------------------------------------------------------------
    // One frame

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        for done in self.jobs.take_done() {
            done(self);
        }
        for done in self.side_jobs.take_done() {
            done(self);
        }
        let now = self.now();
        if let Some(plan) = self.ed.tick(now) {
            self.jobs.run(move || plan.run(), |w: &mut Window, res| w.ed.apply_plan(res));
        }
        self.pump();
        self.keys(&ctx);
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        if let Some(p) = dropped.into_iter().next() {
            if self.dialog.is_none() {
                self.file_dropped(p);
            }
        }

        crate::toolbar::ui(self, ui);
        egui::Panel::bottom("status").frame(egui::Frame::new().fill(style::BG).inner_margin(egui::Margin::symmetric(8, 4))).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !self.status_size.is_empty() {
                        ui.label(egui::RichText::new(&self.status_size).color(style::MUTED));
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(true), |ui| {
                        let color = match self.status.1.as_str() {
                            "ok" => style::OK,
                            "err" => style::ERR,
                            _ => style::TEXT,
                        };
                        ui.label(egui::RichText::new(&self.status.0).color(color));
                    });
                });
            });
        });
        let wide = ui.available_width() >= 1560.0;
        let side = self.palette_open || self.ed.path.on;
        let bare = egui::Frame::new().fill(style::BG);
        let padded = egui::Frame::new().inner_margin(egui::Margin { left: 8, right: 4, top: 4, bottom: 4 });
        egui::Panel::left("level").frame(bare).default_size(270.0).size_range(200.0..=420.0).resizable(true).show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            panel_title(ui, "Level");
            padded.show(ui, |ui| outliner::ui(self, ui));
        });
        egui::Panel::right("inspector").frame(bare).default_size(380.0).size_range(300.0..=560.0).resizable(true).show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            if side && !wide {
                side_tabs(self, ui, true);
            } else {
                panel_title(ui, "Inspector");
            }
            let tab = if side && !wide { self.palette.side_tab } else { 0 };
            let total = ui.available_height();
            let checks_height = (total * 0.36).clamp(180.0, 320.0);
            egui::Panel::bottom("checks").frame(padded).default_size(checks_height).size_range(120.0..=(total - 160.0).max(120.0)).resizable(true).show(ui, |ui| {
                ui.set_min_size(ui.available_size());
                crate::checks::ui(self, ui);
            });
            egui::CentralPanel::default().frame(padded).show(ui, |ui| match tab {
                1 => palette::ui(self, ui),
                2 => path_panel::ui(self, ui),
                _ => inspector::ui(self, ui),
            });
        });
        if side && wide {
            egui::Panel::right("add").frame(bare).default_size(360.0).size_range(300.0..=520.0).resizable(true).show(ui, |ui| {
                ui.set_min_size(ui.available_size());
                side_tabs(self, ui, false);
                padded.show(ui, |ui| match self.palette.side_tab {
                    2 => path_panel::ui(self, ui),
                    _ => palette::ui(self, ui),
                });
            });
        }
        egui::CentralPanel::default().frame(egui::Frame::new().fill(egui::Color32::BLACK)).show(ui, |ui| {
            viewport::ui(self, ui);
        });
        crate::dialogs::ui(self, &ctx);

        let acts = std::mem::take(&mut self.acts);
        let ran = !acts.is_empty();
        for a in acts {
            a(self);
        }
        self.pump();
        if ran {
            self.view.dirty = true;
            ctx.request_repaint();
        }
        if self.busy || self.jobs.running > 0 || self.side_jobs.running > 0 {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
        if self.ed.draft_saved_at.is_none() && self.ed.is_edited() {
            // The draft is saved a moment after the last edit
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
    }
}

/// Which side panels are open, as tabs. `with_inspector` adds the inspector's tab (a narrow window).
fn side_tabs(w: &mut Window, ui: &mut egui::Ui, with_inspector: bool) {
    let mut tabs: Vec<(u8, &str)> = Vec::new();
    if with_inspector {
        tabs.push((0, "Inspector"));
    }
    if w.palette_open {
        tabs.push((1, "Add to level"));
    }
    if w.ed.path.on {
        tabs.push((2, "Path"));
    }
    if !tabs.iter().any(|t| t.0 == w.palette.side_tab) {
        w.palette.side_tab = tabs.last().map_or(0, |t| t.0);
    }
    egui::Frame::new().fill(style::PANEL).inner_margin(egui::Margin::symmetric(6, 3)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            for (id, name) in tabs {
                let on = w.palette.side_tab == id;
                let text = egui::RichText::new(name).color(if on { style::GOLD } else { style::TEXT });
                if ui.add(egui::Button::selectable(on, text)).clicked() {
                    w.palette.side_tab = id;
                }
            }
        });
    });
}

fn panel_title(ui: &mut egui::Ui, title: &str) {
    egui::Frame::new().fill(style::PANEL).inner_margin(egui::Margin::symmetric(8, 4)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(title);
    });
}

/// "level is 1234 bytes" in a build's message.
fn lev_bytes(msg: &str) -> Option<&str> {
    let rest = &msg[msg.find("level is ")? + 9..];
    let end = rest.find(|c: char| !c.is_ascii_digit())?;
    (end > 0 && rest[end..].starts_with(" bytes")).then(|| &rest[..end])
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from).filter(|p| p.is_dir())
}

/// Python's `str.title()` for a world's name.
pub fn title_case(s: &str) -> String {
    let mut out = String::new();
    let mut start = true;
    for c in s.chars() {
        if c.is_alphabetic() {
            out.extend(if start { c.to_uppercase().collect::<Vec<_>>() } else { c.to_lowercase().collect() });
            start = false;
        } else {
            out.push(c);
            start = true;
        }
    }
    out
}

/// The entry of the catalog for an id, when there is one.
pub fn cat<'a>(ed: &'a Editor, oid: &str) -> Option<&'a CatalogEntry> {
    ed.catalog.get(oid)
}

/// The page players patch their ROM with (`tools/build_patcher.py` writes it).
const PATCHER: &str = include_str!("../../../web/patcher.html");
