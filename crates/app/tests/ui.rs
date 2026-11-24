//! The editor's window on the fixture, without a ROM.
//!
//! The test drives the real window through `egui_kittest`: it clicks the widgets it finds by their labels, types
//! into fields, presses the shortcut keys, and sends pointer events at pixel positions into the 3D view (picking,
//! the gizmo, box select, the path tool). Checks named "click: ..." come last: one for each button the earlier
//! checks do not click.
//!
//! `GLOVEBOX_ROM=PATH GLOVEBOX_GUIDE=DIR` also runs `guide_shots`, which saves the pictures of the guide (`guide/mod.rs`).
//!
//! `GLOVEBOX_SHOTS=DIR` saves pictures of the window after the main steps. `GLOVEBOX_ROM=PATH` also runs
//! `the_rom`: setup with the real ROM, all 48 levels, a ROM build and a patch. `GLOVEBOX_THEME=ID` runs every
//! test in that theme (`themes` chooses its own).

use egui::{Key, Modifiers};
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;
use glovebox::app::{App, Start, Window};
use glovebox::dialogs::Scripted;
use glovebox_core::backend::Backend;
use glovebox_core::xml::NodeId;
use glovebox_editor::editor::{Editor, Scatter};
use glovebox_editor::entities::{camera_zone, Ent, EntId};
use glovebox_editor::fixture::{FixtureBackend, SIGN};
use glovebox_editor::gizmo::{Handle, Mode};
use glovebox_editor::presets::PRESETS;
use glovebox_editor::sanitize::{sanitize_level_xml, FormatModel};
use glovebox_editor::xmlutil::{body, num0};
use glovebox_editor::{scripts, vec};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

mod guide;

struct T {
    h: Harness<'static, App>,
    results: Vec<(String, bool, String)>,
    shots: Option<PathBuf>,
    files: Scripted,
    home: PathBuf,
}

fn ent(ed: &Editor, pred: impl Fn(&Ent) -> bool) -> Option<EntId> {
    ed.find(pred)
}

fn kinds(ed: &Editor, kind: &str) -> Vec<EntId> {
    (0..ed.entities.len()).filter(|&i| ed.entities[i].kind == kind).collect()
}

fn tags(ed: &Editor, n: NodeId) -> Vec<String> {
    ed.doc.children(n).iter().map(|c| ed.doc.tag(*c).to_string()).collect()
}

fn px(ed: &Editor, p: vec::V3) -> (f64, f64) {
    let v = ed.camera.to_pixels(p);
    (v[0].round(), v[1].round())
}

fn puzzles(ed: &Editor) -> usize {
    ed.doc.kids(ed.root(), "Puzzle").len()
}

impl T {
    fn new(name: &str, backend: Option<Arc<dyn Backend>>, size: [f32; 2]) -> T {
        let theme = std::env::var("GLOVEBOX_THEME").ok();
        T::with_theme(name, backend, size, theme.as_deref())
    }

    /// A window on an empty user folder whose settings name a theme. The settings also say that the welcome was
    /// answered and that the notes are shown: the checks read the notes in the panels.
    fn with_theme(name: &str, backend: Option<Arc<dyn Backend>>, size: [f32; 2], theme: Option<&str>) -> T {
        let theme = theme.map(|id| format!("theme={id}\n")).unwrap_or_default();
        T::with_settings(name, backend, size, &format!("[General]\ntour=seen\ninfo=shown\n{theme}"))
    }

    /// A window on an empty user folder with this settings file. An empty text gives no file: the first start.
    fn with_settings(name: &str, backend: Option<Arc<dyn Backend>>, size: [f32; 2], settings: &str) -> T {
        let home = std::env::temp_dir().join(format!("glovebox-ui-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        if !settings.is_empty() {
            std::fs::write(home.join("settings.ini"), settings).unwrap();
        }
        T::open(home, backend, size)
    }

    /// A window on a user folder as it is.
    fn open(home: PathBuf, backend: Option<Arc<dyn Backend>>, size: [f32; 2]) -> T {
        let files = Scripted::default();
        let start = Start { home: home.clone(), backend, dialogs: Box::new(files.clone()) };
        let h = Harness::builder().with_size(egui::vec2(size[0], size[1])).with_max_steps(8).wgpu().build_eframe(move |cc| App::new(cc, start));
        let shots = std::env::var_os("GLOVEBOX_SHOTS").map(PathBuf::from);
        if let Some(dir) = &shots {
            std::fs::create_dir_all(dir).unwrap();
        }
        T { h, results: Vec::new(), shots, files, home }
    }

    fn w(&mut self) -> &mut Window {
        self.h.state_mut().window().expect("the editor's window")
    }

    fn ed(&mut self) -> &mut Editor {
        &mut self.w().ed
    }

    fn check(&mut self, name: &str, ok: bool) {
        self.detail(name, ok, String::new());
    }

    fn detail(&mut self, name: &str, ok: bool, detail: String) {
        println!("{}  {}{}", if ok { "ok  " } else { "FAIL" }, name, if ok || detail.is_empty() { String::new() } else { format!("  ({detail})") });
        if std::env::var_os("GLOVEBOX_TEST_DEBUG").is_some() {
            println!("      entities: {:?}", self.h.state_mut().window().map(|w| w.ed.entities.len()));
        }
        self.results.push((name.to_string(), ok, detail));
    }

    fn settle(&mut self) {
        for _ in 0..3 {
            self.h.step();
        }
    }

    /// Steps until no level is loading and no job runs.
    fn idle(&mut self) {
        let t0 = Instant::now();
        loop {
            self.h.step();
            let w = self.w();
            if !w.busy && w.jobs.running == 0 && w.ed.has_level {
                break;
            }
            assert!(t0.elapsed() < Duration::from_secs(300), "the window stayed busy");
            std::thread::sleep(Duration::from_millis(3));
        }
        self.settle();
    }

    /// Steps until `done` holds, for at most `secs` seconds.
    fn until(&mut self, secs: u64, done: impl Fn(&mut Window) -> bool) -> bool {
        let t0 = Instant::now();
        loop {
            self.h.step();
            if done(self.w()) {
                self.settle();
                return true;
            }
            if t0.elapsed() > Duration::from_secs(secs) {
                return false;
            }
            std::thread::sleep(Duration::from_millis(3));
        }
    }

    fn shot(&mut self, name: &str) {
        let Some(dir) = self.shots.clone() else { return };
        self.settle();
        let path = dir.join(format!("{name}.png"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        match self.h.render() {
            Ok(img) => img.save(path).unwrap(),
            Err(e) => println!("no picture {name}: {e}"),
        }
    }

    /// The id of the window's theme.
    fn theme(&self) -> &'static str {
        glovebox::style::theme(&self.h.ctx).id
    }

    /// Chooses an entry of the toolbar's Options menu: a theme, a state of the Info texts, or Take the tour.
    fn option(&mut self, entry: &str) -> bool {
        let ok = self.click("Options") && self.click(entry);
        self.settle();
        ok
    }

    /// Chooses a theme in the Options menu.
    fn set_theme(&mut self, name: &str) -> bool {
        self.option(name)
    }

    /// How the window shows its notes.
    fn info(&self) -> glovebox::style::Info {
        glovebox::style::info(&self.h.ctx)
    }

    /// True when a text of the window holds `part`: a label in a panel, or a tooltip that is open. A tooltip that
    /// is not open does not count.
    fn reads(&self, part: &str) -> bool {
        self.find(part, false).iter().any(|n| n.accesskit_node().role() == egui::accesskit::Role::Label)
    }

    /// Rests the pointer on the first widget whose label holds `label`, long enough for a tooltip.
    fn hover(&mut self, label: &str, role: Option<egui::accesskit::Role>) -> bool {
        let found = match self.find(label, false).into_iter().find(|n| role.is_none_or(|r| n.accesskit_node().role() == r)) {
            Some(n) => {
                n.hover();
                true
            }
            None => false,
        };
        for _ in 0..8 {
            self.h.step();
        }
        found
    }

    /// The top of the first widget with this label.
    fn top_of(&self, label: &str) -> Option<f32> {
        self.find(label, true).first().map(|n| n.rect().min.y)
    }

    /// True when the window's top left corner, which is the toolbar's ground, has the theme's color.
    fn toolbar_is(&mut self, th: &glovebox::theme::Theme) -> bool {
        self.settle();
        let Ok(img) = self.h.render() else { return false };
        let (p, want) = (img.get_pixel(2, 2).0, th.deep.to_array());
        (0..3).all(|k| p[k].abs_diff(want[k]) <= 2)
    }

    fn status(&mut self) -> String {
        self.w().status.0.clone()
    }

    // -- Widgets

    /// The widgets with a label, in the window's order: what can be pressed or typed into first, plain text
    /// after. `exact` wants the whole label, otherwise a part of it.
    fn find<'a>(&'a self, label: &'a str, exact: bool) -> Vec<egui_kittest::Node<'a>> {
        let all: Vec<_> = if exact { self.h.query_all_by_label(label).collect() } else { self.h.query_all_by_label_contains(label).collect() };
        let (mut first, rest): (Vec<_>, Vec<_>) = all.into_iter().partition(|n| n.accesskit_node().role() != egui::accesskit::Role::Label);
        first.extend(rest);
        first
    }

    fn has(&self, label: &str) -> bool {
        !self.find(label, true).is_empty()
    }

    fn has_containing(&self, label: &str) -> bool {
        !self.find(label, false).is_empty()
    }

    /// The whole label of the first widget whose label holds `part`.
    fn label_with(&self, part: &str) -> String {
        self.find(part, false).first().and_then(|n| n.accesskit_node().label()).unwrap_or_default()
    }

    fn press(&mut self, label: &str, exact: bool, n: usize, by_action: bool) -> bool {
        // A widget scrolled out of view is brought into view first: a click goes to a place in the window
        let seen = match self.find(label, exact).get(n) {
            Some(node) if !by_action => {
                node.scroll_to_me();
                true
            }
            other => other.is_some(),
        };
        if seen && !by_action {
            for _ in 0..4 {
                self.h.step();
            }
        }
        let found = match self.find(label, exact).get(n) {
            Some(node) if by_action => {
                node.click_accesskit();
                true
            }
            Some(node) => {
                node.click();
                true
            }
            None => false,
        };
        self.settle();
        if !found {
            println!("      no widget labeled {label:?} (#{n})");
            if std::env::var_os("GLOVEBOX_TEST_DEBUG").is_some() {
                let part: String = label.chars().take(4).collect();
                let near: Vec<String> = self.h.query_all_by_label_contains(&part).filter_map(|n| n.accesskit_node().label()).collect();
                println!("      labels with {part:?}: {near:?}");
            }
        }
        found
    }

    /// Clicks the `n`th widget with this label. False when there is none.
    fn click_nth(&mut self, label: &str, n: usize) -> bool {
        self.press(label, true, n, false)
    }

    fn click(&mut self, label: &str) -> bool {
        self.press(label, true, 0, false)
    }

    fn click_containing(&mut self, label: &str) -> bool {
        self.press(label, false, 0, false)
    }

    /// Clicks an entry of an open menu or list by its accessibility action: the entry may be scrolled out of view.
    /// An entry in view gets a real click.
    fn choose(&mut self, label: &str) -> bool {
        let window = self.h.ctx.content_rect();
        if let Some(n) = self.find(label, false).first() {
            n.scroll_to_me();
        }
        for _ in 0..4 {
            self.h.step();
        }
        let in_view = self.find(label, false).first().is_some_and(|n| window.contains(n.rect().center()));
        if in_view {
            let node = self.find(label, false).remove(0);
            node.click();
            self.settle();
            return true;
        }
        self.press(label, false, 0, true)
    }

    /// Opens the combo box or menu `label` and picks the entry `entry`.
    fn pick(&mut self, label: &str, entry: &str) -> bool {
        self.click(label) && self.choose(entry)
    }

    /// Types into the text box whose label contains `label`, and presses Enter.
    fn fill(&mut self, label: &str, text: &str) -> bool {
        if let Some(node) = self.find(label, false).into_iter().find(|n| n.accesskit_node().role() == egui::accesskit::Role::TextInput) {
            node.scroll_to_me();
        }
        for _ in 0..4 {
            self.h.step();
        }
        let found = match self.find(label, false).into_iter().find(|n| n.accesskit_node().role() == egui::accesskit::Role::TextInput) {
            Some(node) => {
                node.click();
                true
            }
            None => false,
        };
        if !found {
            println!("      no text box labeled like {label:?}");
            return false;
        }
        self.settle();
        // Without the focus the keys below would be the editor's shortcuts
        if !self.h.ctx.egui_wants_keyboard_input() {
            let rects: Vec<_> = self.find(label, false).iter().map(|n| (n.accesskit_node().role(), n.rect())).collect();
            println!("      the text box {label:?} did not take the focus: {rects:?}");
            return false;
        }
        self.h.key_press_modifiers(Modifiers::COMMAND, Key::A);
        self.h.step();
        self.h.key_press(Key::Backspace);
        self.h.step();
        if !text.is_empty() {
            self.h.event(egui::Event::Text(text.to_string()));
            self.h.step();
        }
        self.h.key_press(Key::Enter);
        self.settle();
        true
    }

    fn key(&mut self, key: Key) {
        self.h.key_press(key);
        self.settle();
    }

    fn key_with(&mut self, mods: Modifiers, key: Key) {
        self.h.key_press_modifiers(mods, key);
        self.settle();
    }

    // -- The mouse in the 3D view. Positions are pixels from the view's top left corner.

    fn at(&mut self, p: (f64, f64)) -> egui::Pos2 {
        let r = self.w().view.rect;
        r.min + egui::vec2(p.0 as f32, p.1 as f32)
    }

    fn mouse_move(&mut self, p: (f64, f64)) {
        let pos = self.at(p);
        self.h.event(egui::Event::PointerMoved(pos));
        self.h.step();
    }

    fn mouse_button(&mut self, p: (f64, f64), pressed: bool, modifiers: Modifiers) {
        let pos = self.at(p);
        if !modifiers.is_none() {
            self.h.event(egui::Event::ModifiersChanged(modifiers));
        }
        self.h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers });
        if !modifiers.is_none() && !pressed {
            self.h.event(egui::Event::ModifiersChanged(Modifiers::default()));
        }
        self.h.step();
    }

    fn view_click(&mut self, p: (f64, f64), modifiers: Modifiers) {
        self.mouse_move(p);
        self.mouse_button(p, true, modifiers);
        self.mouse_button(p, false, modifiers);
        self.settle();
    }

    fn drag(&mut self, path: &[(f64, f64)]) {
        self.mouse_move(path[0]);
        self.mouse_button(path[0], true, Modifiers::default());
        for p in &path[1..] {
            self.mouse_move(*p);
        }
        self.mouse_button(*path.last().unwrap(), false, Modifiers::default());
        self.settle();
    }

    /// A drag with the left button between two points of the window.
    fn drag_window(&mut self, from: egui::Pos2, to: egui::Pos2) {
        let button = |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::default() };
        self.h.event(egui::Event::PointerMoved(from));
        self.h.step();
        self.h.event(button(from, true));
        self.h.step();
        for k in 1..=4 {
            self.h.event(egui::Event::PointerMoved(from + (to - from) * (k as f32 / 4.0)));
            self.h.step();
        }
        self.h.event(button(to, false));
        self.h.step();
        self.settle();
    }

    /// The middle of the text box with this label, in the window.
    fn field_at(&mut self, label: &str) -> Option<egui::Pos2> {
        let role = egui::accesskit::Role::TextInput;
        if let Some(node) = self.find(label, true).into_iter().find(|n| n.accesskit_node().role() == role) {
            node.scroll_to_me();
        }
        for _ in 0..4 {
            self.h.step();
        }
        self.find(label, true).into_iter().find(|n| n.accesskit_node().role() == role).map(|n| n.rect().center())
    }

    /// Presses on a text box and moves the pointer `dx` points to the side, in steps. The button stays down.
    fn field_press_and_move(&mut self, label: &str, dx: f32, modifiers: Modifiers) -> Option<egui::Pos2> {
        let from = self.field_at(label)?;
        self.h.event(egui::Event::PointerMoved(from));
        self.h.step();
        self.h.event(egui::Event::ModifiersChanged(modifiers));
        self.h.event(egui::Event::PointerButton { pos: from, button: egui::PointerButton::Primary, pressed: true, modifiers });
        self.h.step();
        let steps = (dx.abs() / 5.0).ceil().clamp(1.0, 12.0) as usize;
        for k in 1..=steps {
            self.h.event(egui::Event::PointerMoved(from + egui::vec2(dx * k as f32 / steps as f32, 0.0)));
            self.h.step();
        }
        self.h.step();
        Some(from + egui::vec2(dx, 0.0))
    }

    fn release(&mut self, at: egui::Pos2) {
        self.h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: false, modifiers: Modifiers::default() });
        self.h.event(egui::Event::ModifiersChanged(Modifiers::default()));
        self.settle();
    }

    /// A whole drag on a text box: press, move `dx` points, release.
    fn field_drag(&mut self, label: &str, dx: f32, modifiers: Modifiers) -> bool {
        match self.field_press_and_move(label, dx, modifiers) {
            Some(at) => {
                self.release(at);
                true
            }
            None => false,
        }
    }

    fn undo_to(&mut self, mark: usize) {
        while self.ed().undo_stack.len() > mark {
            self.ed().undo();
        }
        self.settle();
    }

    fn has_check(&mut self, level: Option<&str>, text: &str) -> bool {
        self.settle();
        self.w().checks.iter().any(|c| level.is_none_or(|l| c.level == l) && c.text.contains(text))
    }

    /// The next Save dialog writes into the user folder. Gives the path.
    fn save_to(&mut self, name: &str) -> PathBuf {
        let path = self.home.join(name);
        let _ = std::fs::remove_file(&path);
        self.files.0.borrow_mut().save.push_back(path.clone());
        path
    }

    fn finish(&mut self) {
        let failed: Vec<&(String, bool, String)> = self.results.iter().filter(|r| !r.1).collect();
        println!("{} checks, {} failed", self.results.len(), failed.len());
        let _ = std::fs::remove_dir_all(&self.home);
        assert!(failed.is_empty(), "failed: {:?}", failed.iter().map(|r| &r.0).collect::<Vec<_>>());
    }
}

/// A binary glTF file with one triangle.
fn tiny_glb() -> Vec<u8> {
    let mut bin = Vec::new();
    for v in [[0.0f32, 0.0, 0.0], [60.0, 0.0, 0.0], [0.0, 60.0, 40.0]] {
        for c in v {
            bin.extend(c.to_le_bytes());
        }
    }
    let mut json = br#"{"asset":{"version":"2.0"},"scene":0,"scenes":[{"nodes":[0]}],"nodes":[{"mesh":0}],"meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[60,60,40]}],"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36}],"buffers":[{"byteLength":36}]}"#.to_vec();
    while json.len() % 4 != 0 {
        json.push(b' ');
    }
    let mut out = Vec::new();
    out.extend(b"glTF");
    out.extend(2u32.to_le_bytes());
    out.extend((12 + 8 + json.len() as u32 + 8 + bin.len() as u32).to_le_bytes());
    out.extend((json.len() as u32).to_le_bytes());
    out.extend(b"JSON");
    out.extend(&json);
    out.extend((bin.len() as u32).to_le_bytes());
    out.extend(b"BIN\0");
    out.extend(&bin);
    out
}

#[test]
fn editor_on_the_fixture() {
    let backend: Arc<dyn Backend> = Arc::new(FixtureBackend::new());
    let mut t = T::new("fixture", Some(backend.clone()), [1500.0, 950.0]);
    let none = Modifiers::default();
    t.idle();

    let (sid, n) = (t.ed().scene_id().to_string(), t.ed().entities.len());
    t.detail("level loads", sid == "10" && n > 15, n.to_string());
    let have: Vec<&str> = t.ed().entities.iter().map(|e| e.kind).collect();
    let all = ["land", "platform", "garib", "enemy", "pickup", "tip", "spawn", "wind", "hazard", "sound", "trigger"];
    t.detail("every kind of entity is built", all.iter().all(|k| have.contains(k)), format!("{have:?}"));
    let ed = t.ed();
    let land = ent(ed, |e| e.kind == "land").unwrap();
    let ok = !ed.graph[ed.nodes[land].obj].children().is_empty() && !ed.entities.iter().any(|e| e.missing_model);
    t.check("models load", ok);
    let ok = t.has_check(None, "No new problems") && t.has_containing("No new problems");
    t.check("checks run", ok);
    t.shot("01-loaded");

    // Picking: a click on the enemy selects it
    let ed = t.ed();
    let enemy = ent(ed, |e| e.kind == "enemy").unwrap();
    ed.focus_on(enemy);
    t.settle();
    let ed = t.ed();
    let at = px(ed, ed.obj_pos(enemy));
    t.view_click(at, none);
    let sel = t.ed().selection.clone();
    t.detail("click picks the enemy", sel == vec![enemy], format!("{sel:?}"));
    let label = t.ed().entities[enemy].label.clone();
    let ok = t.has(&label);
    t.check("the inspector shows it", ok);

    // The gizmo: drag the X arrow
    let ed = t.ed();
    let ok = ed.gizmo.attached && ed.gizmo.mode == Mode::Translate;
    t.check("the gizmo is attached", ok);
    let ed = t.ed();
    let pos = |ed: &Editor| {
        let e = ent(ed, |e| e.kind == "enemy").unwrap();
        ed.entities[e].world_pos(&ed.doc, &ed.entities)
    };
    let before = pos(ed);
    let s = ed.gizmo.size(&ed.camera);
    let a = px(ed, vec::add(ed.gizmo.pos, [s * 0.7, 0.0, 0.0]));
    let path: Vec<(f64, f64)> = (0..7).map(|i| (a.0 + 10.0 * f64::from(i), a.1)).collect();
    t.drag(&path);
    let after = pos(t.ed());
    t.detail(
        "dragging the X arrow moves the enemy along x",
        (after[0] - before[0]).abs() > 1.0 && (after[1] - before[1]).abs() < 1e-6 && (after[2] - before[2]).abs() < 1e-6,
        format!("{before:?} {after:?}"),
    );
    t.detail("snap 1 keeps whole numbers", (after[0] - after[0].round()).abs() < 1e-6, format!("{after:?}"));
    let n = t.ed().undo_stack.len();
    t.detail("the drag is one undo step", n == 1, n.to_string());
    t.click("Undo");
    let ed = t.ed();
    let ok = pos(ed) == before && ed.selection == vec![ent(ed, |e| e.kind == "enemy").unwrap()];
    t.check("undo puts it back", ok);
    t.click("Redo");
    let ok = pos(t.ed()) == after;
    t.check("redo moves it again", ok);

    // Rotation: E, then drag the Y ring
    t.key(Key::E);
    let ed = t.ed();
    let ok = ed.mode == Mode::Rotate && ed.gizmo.mode == Mode::Rotate;
    t.check("E switches to rotate", ok);
    let ed = t.ed();
    let ring = ed.gizmo.handles(&ed.camera).into_iter().find(|h| h.0 == Handle::Y).unwrap().2;
    let pts: Vec<(f64, f64)> = ring.iter().map(|p| px(ed, *p)).collect();
    let i0 = (0..pts.len()).max_by(|a, b| pts[*a].1.partial_cmp(&pts[*b].1).unwrap()).unwrap(); // the ring's nearest point
    let (a, b) = (pts[i0], pts[(i0 + 6) % pts.len()]);
    let mut path: Vec<(f64, f64)> = (0..8).map(|k| f64::from(k) / 7.0).map(|f| ((a.0 + (b.0 - a.0) * f).trunc(), (a.1 + (b.1 - a.1) * f).trunc())).collect();
    path[0] = a;
    path.push(b);
    t.drag(&path);
    let ed = t.ed();
    let node = ed.entities[ent(ed, |e| e.kind == "enemy").unwrap()].node;
    let yr = num0(&ed.doc, node, "y_rotation");
    let ok = yr.abs() > 0.1 && ed.doc.first(node, "SetActorRotation").is_none();
    t.detail("dragging the Y ring turns the enemy (y_rotation)", ok, yr.to_string());
    t.detail("rotation snaps to 15°", (yr.to_degrees() / 15.0 - (yr.to_degrees() / 15.0).round()).abs() < 1e-3, yr.to_degrees().to_string());
    t.key(Key::W);

    // Numeric transform in the inspector
    t.ed().apply_transform(None, Some(90.0), 0, None);
    let ed = t.ed();
    let node = ed.entities[ent(ed, |e| e.kind == "enemy").unwrap()].node;
    let ok = ed.doc.first(node, "SetActorRotation").is_some() && !ed.mod_users().is_empty();
    t.check("a tilt about x needs the game mod (SetActorRotation)", ok);
    let ok = t.has_check(None, "Expansion Pak game mod");
    t.check("the checks say so", ok);
    t.key_with(Modifiers::COMMAND, Key::Z);

    // Scale a landscape piece: the inspector's number boxes
    let ed = t.ed();
    let grave = ent(ed, |e| e.label == "grave");
    ed.select(grave, false);
    t.settle();
    for axis in ["scale x", "scale y", "scale z"] {
        t.fill(axis, "2");
    }
    let ed = t.ed();
    let grave = &ed.entities[ent(ed, |e| e.label == "grave").unwrap()];
    let scale = grave.get_scale(&ed.doc);
    let y = vec::euler_yzx_from_quat(grave.get_quat(&ed.doc).unwrap())[1];
    t.detail("scale writes SetActorScale", scale == Some([2.0, 2.0, 2.0]), format!("{scale:?}"));
    t.detail("rotation survives a scale", (y - 0.6).abs() < 1e-3, y.to_string());

    // Group rotation about the group center: "rotate by" 90 about y, then Apply
    let center = |ed: &Editor| {
        let g = kinds(ed, "garib");
        let mut c = [0.0; 3];
        for &i in &g {
            c = vec::add(c, ed.entities[i].world_pos(&ed.doc, &ed.entities));
        }
        vec::div(c, g.len() as f64)
    };
    let ed = t.ed();
    let garibs = kinds(ed, "garib");
    ed.set_selection(garibs);
    let c0 = center(ed);
    t.settle();
    t.click("Apply rotation");
    let ed = t.ed();
    let c1 = center(ed);
    let ok = (0..3).all(|k| (c0[k] - c1[k]).abs() < 0.01) && ed.selection.len() == 3 && ed.undo_stack.last().is_some();
    t.detail("a group rotation keeps the center and moves the garibs", ok && c0 != [0.0; 3], format!("{c0:?} {c1:?}"));

    // Box select
    let ed = t.ed();
    ed.set_selection(Vec::new());
    ed.frame_level();
    t.settle();
    t.key(Key::B);
    let ed = t.ed();
    let ok = ed.box_mode && !ed.gizmo.attached;
    t.check("B turns box select on", ok);
    let (vw, vh) = (t.ed().camera.width, t.ed().camera.height);
    t.drag(&[(5.0, 5.0), ((vw / 2.0).floor(), (vh / 2.0).floor()), (vw - 5.0, vh - 5.0)]);
    let ed = t.ed();
    let land_in = ed.selection.iter().any(|&i| ed.entities[i].kind == "land");
    let (n, box_mode) = (ed.selection.len(), ed.box_mode);
    t.detail("box select takes what is in the box, without the landscape", n > 5 && !land_in && !box_mode, n.to_string());
    t.shot("02-box-select");

    // Ctrl+click toggles
    let ed = t.ed();
    let first = ed.selection[0];
    ed.focus_on(first);
    t.settle();
    let ed = t.ed();
    let n = ed.selection.len();
    let at = px(ed, ed.obj_pos(first));
    t.view_click(at, Modifiers::CTRL);
    let ed = t.ed();
    let ok = ed.selection.len() == n - 1 && !ed.selection.contains(&first);
    t.check("Ctrl+click takes one out of the selection", ok);

    // Duplicate and delete
    let ed = t.ed();
    let tip = ent(ed, |e| e.kind == "tip");
    ed.select(tip, false);
    t.settle();
    t.key_with(Modifiers::CTRL, Key::D);
    let ed = t.ed();
    let tips = kinds(ed, "tip");
    let x = |i: EntId| ed.entities[i].world_pos(&ed.doc, &ed.entities)[0];
    let ok = tips.len() == 2 && (x(tips[1]) - x(tips[0]) - 40.0).abs() < 1e-6;
    t.check("Ctrl+D duplicates, 40 to the side", ok);
    t.key(Key::Delete);
    let n = kinds(t.ed(), "tip").len();
    t.check("Delete removes the copy", n == 1);

    // Camera zones: the lone trigger volume duplicates with its puzzle; a volume of a puzzle with two doesn't
    let ed = t.ed();
    let zone = ent(ed, |e| e.zone);
    ed.select(zone, false);
    t.settle();
    t.click("Duplicate");
    let ed = t.ed();
    let ok = ed.entities.iter().filter(|e| e.zone).count() == 2 && puzzles(ed) == 3;
    t.check("a camera zone duplicates whole", ok);
    t.click("Undo");
    let ed = t.ed();
    let two = ent(ed, |e| e.kind == "trigger" && !e.zone);
    ed.select(two, false);
    t.settle();
    t.click("Delete");
    let (n, status) = (puzzles(t.ed()), t.status());
    t.detail("a volume of a two-condition puzzle stays", n == 2 && status.contains("can\u{2019}t be duplicated"), status);
    // The camera zone panel's preset list
    let ed = t.ed();
    let zone = ent(ed, |e| e.zone).unwrap();
    ed.select(Some(zone), false);
    t.settle();
    t.pick("preset", "High: rises");
    let ed = t.ed();
    let z = camera_zone(&ed.doc, ed.entities[ent(ed, |e| e.zone).unwrap()].node).unwrap();
    t.detail("a preset rewrites the zone's actions", z.height == Some(320.0) && z.distance == Some(140.0) && z.look.is_none(), format!("{z:?}"));
    t.ed().add_camera_zone("fixed");
    let ed = t.ed();
    let sel = ed.entities[ed.selected().unwrap()].node;
    let ok = camera_zone(&ed.doc, sel).unwrap().fly.is_some();
    t.check("Add camera zone adds a puzzle with a fly-to point", ok);

    // Shift+click drops a garib on the ground
    let ed = t.ed();
    ed.set_selection(Vec::new());
    ed.frame_level();
    t.settle();
    let n = kinds(t.ed(), "garib").len();
    t.view_click(((vw / 2.0).floor(), (vh * 0.75).trunc()), Modifiers::SHIFT);
    let n2 = kinds(t.ed(), "garib").len();
    t.detail("Shift+click drops a garib", n2 == n + 1, format!("{n} {n2}"));

    // The palette
    t.key(Key::A);
    let open = t.w().palette_open;
    t.check("A opens the palette", open);
    for tab in ["Collectables", "Enemies", "Models", "Kit", "Presets"] {
        t.click(tab);
    }
    // A real click on a preset's Add button
    let platforms = |ed: &Editor| ed.doc.kids(ed.root(), "Platform").len();
    let n = platforms(t.ed());
    t.click(&format!("Add {}", PRESETS[2].name));
    let (n2, status) = (platforms(t.ed()), t.status());
    t.detail("clicking a preset's Add adds it", n2 > n, status);
    t.click("Undo");
    t.click("Models");
    let drawn = t.until(60, |w| w.palette.thumbs_done() >= 3);
    let n = t.w().palette.thumbs_done();
    t.detail("model thumbnails are drawn", drawn, n.to_string());
    t.shot("03-palette");
    t.fill("Search models", "sign");
    t.click("Scenery sign");
    let ok = t.ed().current_imports().contains(&SIGN.to_string());
    t.check("a model from another bank is an import", ok);
    // The plan runs in the background, a moment after the edit
    let planned = t.until(30, |w| w.ed.plan.as_ref().is_some_and(|p| p.objects.contains(&SIGN.to_string())));
    t.check("the import plan runs", planned);
    let ok = t.has("IMPORTS AT ROM BUILD");
    t.check("imports show in the checks", ok);
    t.click("Enemies");
    t.click("Add dennis");
    t.shot("03a-enemies");
    let n = kinds(t.ed(), "enemy").len();
    t.detail("Add enemy places one", n == 2, n.to_string());
    t.click("Collectables");
    t.click_containing("bowlingball");
    t.click("? Mr. Tip");
    let ed = t.ed();
    let (a, b) = (kinds(ed, "pickup").len(), kinds(ed, "tip").len());
    t.detail("Add powerup and Mr. Tip", a == 2 && b == 2, format!("{a} {b}"));
    t.shot("03b-collectables");
    t.key(Key::Escape);

    // The path tool
    t.ed().set_selection(Vec::new());
    t.settle();
    t.click("Path");
    for x in [0.3, 0.5, 0.7] {
        t.view_click(((vw * x).trunc(), (vh * 0.7).trunc()), none);
    }
    let n = t.ed().path.pts.len();
    t.detail("clicks lay a path", n == 3, n.to_string());
    let n = kinds(t.ed(), "garib").len();
    t.click("Place");
    let ed = t.ed();
    let added = kinds(ed, "garib").len() - n;
    let sel = ed.selection.len();
    t.detail("Place puts garibs along it", added >= 3 && sel == added, format!("{added} {sel}"));
    // Camera zones over the path: boxes 140 to the sides, 300 below and 400 above
    let ed = t.ed();
    let nz = puzzles(ed);
    ed.path_zones("high", 0.0, 1.0, 140.0, 300.0, 400.0, 0.0);
    let pz = ed.doc.kids(ed.root(), "Puzzle");
    let vol = ed.doc.iter_tags(*pz.last().unwrap(), &["PuzzleCondGloverWithinVolume"]).first().copied().filter(|_| pz.len() > nz);
    let pts = ed.path.pts.clone();
    let low = |k: usize| pts.iter().map(|p| p[k]).fold(f64::INFINITY, f64::min);
    let high = |k: usize| pts.iter().map(|p| p[k]).fold(f64::NEG_INFINITY, f64::max);
    let ok = vol.is_some_and(|v| (num0(&ed.doc, v, "x") - (low(0) - 140.0)).abs() <= 21.0 && (num0(&ed.doc, v, "w") - (high(1) - low(1) + 700.0)).abs() <= 21.0);
    let mut acts: Vec<String> = ed.doc.kids(*pz.last().unwrap(), "PuzzleAction").iter().map(|a| ed.doc.attr(*a, "action_type").to_string()).collect();
    acts.sort();
    t.check("a camera zone fits the path", ok);
    t.detail("the zone has the preset's actions", acts == ["70", "71"], format!("{acts:?}"));
    let ed = t.ed();
    ed.undo();
    ed.path_zones("side", 0.0, 1.0, 140.0, 300.0, 400.0, 100.0);
    let n = puzzles(ed) - nz;
    ed.undo();
    t.detail("a long stretch is split into boxes", n >= 2, n.to_string());
    // Scatter: the same seed gives the same result, and the children come first
    let ed = t.ed();
    let land = ent(ed, |e| e.tag == "BackgroundActor").or(ent(ed, |e| e.tag == "LandActor")).unwrap();
    let (name, tag) = (ed.doc.attr(ed.entities[land].node, "name").to_string(), ed.entities[land].tag.clone());
    ed.set_selection(vec![land]);
    let before = ed.serialize();
    let o = Scatter { spacing: (150.0, 200.0), seed: 3, ..Scatter::default() };
    ed.scatter_along_path(&o);
    let one = ed.serialize();
    ed.undo();
    let again = ent(ed, |e| e.tag == tag && ed.doc.attr(e.node, "name") == name);
    ed.set_selection(again.into_iter().collect());
    ed.scatter_along_path(&o);
    let same = ed.serialize() == one && one != before;
    let copy = ed.top_level().into_iter().filter(|n| ed.doc.tag(*n) == "BackgroundActor" && ed.doc.first(*n, "SetActorRotation").is_some()).next_back();
    let first = copy.is_some_and(|c| tags(ed, c)[0] == "SetActorRotation");
    t.check("scatter with the same seed gives the same level", same);
    t.check("a scattered copy is turned with SetActorRotation first", first);
    t.settle();
    let status = t.status();
    t.detail("the status line gives the heap cost", status.contains("KB of heap"), status);
    t.ed().undo();
    t.shot("04-path");
    t.key(Key::Escape);
    let on = t.ed().path.on;
    t.check("Esc leaves the path tool", !on);

    // Fly camera
    t.key(Key::C);
    let on = t.w().view.fly.on;
    t.check("C starts flying", on);
    let p0 = t.ed().camera.pos;
    t.h.key_down(Key::W);
    let ahead = |w: &mut Window, p0: vec::V3| vec::dot(vec::sub(w.ed.camera.pos, p0), w.ed.camera.forward());
    let flew = t.until(20, move |w| ahead(w, p0) > 10.0);
    t.h.key_up(Key::W);
    t.settle();
    t.check("W flies forward", flew);
    t.key(Key::Escape);
    let on = t.w().view.fly.on;
    t.check("Esc stops flying", !on);

    // Layers: the toolbar's Triggers switch
    t.click("Triggers");
    let ed = t.ed();
    let hidden = kinds(ed, "trigger").iter().all(|&i| !ed.graph[ed.nodes[i].obj].visible);
    t.check("hiding a layer hides it", hidden);
    t.click("Triggers");

    // Autosave and restore: through the level list
    t.ed().flush_draft();
    let saved = t.ed().drafts.load("10").is_some();
    t.check("edits are autosaved", saved);
    let xml = t.ed().serialize();
    t.pick("Level", "Fixture 2");
    t.until(60, |w| !w.busy && w.ed.scene_id() == "11");
    let sid = t.ed().scene_id().to_string();
    t.shot("04b-level-2");
    t.pick("Level", "Fixture 1");
    t.until(60, |w| !w.busy && w.ed.scene_id() == "10");
    let (same, status) = (t.ed().serialize() == xml, t.status());
    t.detail("the draft comes back", sid == "11" && same && status.contains("Restored your edits"), status);
    let w = t.w();
    let label = w.ed.scene.clone().map(|s| w.scene_label(&s)).unwrap_or_default();
    t.detail("the level list marks edited levels", label.contains("● edited"), label);
    t.click("Reset level");
    t.shot("04c-confirm-reset");
    t.click("Yes");
    let ed = t.ed();
    let ok = !ed.is_edited() && ed.drafts.load("10").is_none();
    t.check("Reset level goes back to the shipped level and drops the draft", ok);
    t.click("Undo");
    let same = t.ed().serialize() == xml;
    t.check("undo brings the edits back after a reset", same);

    // Building: the fixture assembles .lev files. The toolbar's buttons, with a Save dialog that answers.
    let path = t.save_to("out.lev");
    t.click(".lev");
    t.until(60, |w| w.jobs.running == 0 && w.loading.is_empty());
    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let asked = t.files.0.borrow().asked.last().cloned().unwrap_or_default();
    let status = t.status();
    t.detail(".lev builds and the schema accepts the edits", size > 100 && asked.ends_with(".lev"), format!("{size} {asked} {status}"));
    // Export level pack: the dialog, Cancel, a refused name, then a pack in a folder of its own
    t.click("Export patch");
    t.click("Export level pack");
    t.shot("16b-export-pack");
    let open = t.has_containing("A level pack for g64coopdx");
    let named = matches!(&t.w().dialog, Some(glovebox::dialogs::Dialog::Pack { name }) if name == "Fixture 1");
    t.click("Cancel");
    let closed = t.w().dialog.is_none();
    t.click("Export patch");
    t.click("Export level pack");
    if let Some(glovebox::dialogs::Dialog::Pack { name }) = &mut t.w().dialog {
        name.clear();
    }
    t.click("Export pack");
    t.until(60, |w| w.jobs.running == 0 && w.loading.is_empty());
    let refused = t.status();
    let dir = t.home.join("packs");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    t.files.0.borrow_mut().save.push_back(dir.clone());
    t.click("Export patch");
    t.click("Export level pack");
    t.click("Export pack");
    t.until(60, |w| w.jobs.running == 0 && w.loading.is_empty());
    let status = t.status();
    let folder = dir.join("Fixture_1.pack");
    let manifest = std::fs::read(folder.join("pack.txt")).unwrap_or_default();
    let lev = std::fs::read(folder.join("level.lev")).unwrap_or_default();
    // The fixture's sign is in its made-up object bank 9, which models.bin names
    let models = std::fs::read(folder.join("models.bin")).ok();
    let sign = t.ed().current_imports().contains(&glovebox_editor::fixture::SIGN.to_string());
    // The sign was placed earlier, and the reset with its undo brought it back
    let models_ok = sign && models.as_deref() == Some(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 9, 0, 0, 0][..]);
    let id = glovebox_core::pack::pack_id(&manifest, &lev, None, models.as_deref());
    let hex: String = id.iter().map(|b| format!("{b:02x}")).collect();
    let written = manifest == b"format=1\nname=Fixture 1\nlevel=16\n" && glovebox_core::pack::check_level(&lev).is_ok() && models_ok;
    t.detail(
        "click: Export patch, Export level pack, Cancel and Export pack",
        open && named && closed && refused.contains("No level pack") && written && status.contains("Exported the level pack") && status.contains(&hex)
            && status.contains(glovebox_core::pack::GAME_BANKS_TEXT),
        format!("{refused} / {status} / sign {sign}, models {models:?}"),
    );
    let path = t.save_to("out.xml");
    t.click("Export XML");
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let model = FormatModel::shared();
    let res = sanitize_level_xml(&text, model, "file");
    t.detail("Export XML passes the import checks", res.ok, format!("{:?}", res.errors));
    t.click("Build ROM");
    t.until(60, |w| w.jobs.running == 0 && w.loading.is_empty());
    let status = t.status();
    t.detail("ROM builds say the fixture has no ROM", status.contains("no ROM"), status);

    // Import XML: a good file and hostile ones
    let ok = sanitize_level_xml(&xml, model, "file");
    t.detail("the sanitizer accepts the editor's own XML", ok.ok, format!("{:?}", ok.errors));
    let bad = [
        r#"<!DOCTYPE x [<!ENTITY a "b">]><Level/>"#,
        r#"<Level name="x"><Script/><EndLevelData/></Level>"#,
        r#"<Level name="x"><LandActor object_id="zz" name="a" x="0" y="0" z="0"/><EndLevelData/></Level>"#,
        r#"<Level name="x"><PlatScale x="1" y="1" z="1"/><EndLevelData/></Level>"#,
        r#"<Level name="x"><EndLevelData/><MrTip x="0" y="0" z="0" message_id="1"/></Level>"#,
        r#"<Level name="x"><LandActor object_id="0x1" name="averylongname" x="0" y="0" z="0"/><EndLevelData/></Level>"#,
    ];
    t.check("the sanitizer refuses hostile and broken XML", bad.iter().all(|b| !sanitize_level_xml(b, model, "file").ok));
    // The dialog: a hostile file is refused, the editor's own file goes in
    let bad_file = t.home.join("bad.xml");
    std::fs::write(&bad_file, bad[1]).unwrap();
    let good_file = t.home.join("good.xml");
    std::fs::write(&good_file, &xml).unwrap();
    t.files.0.borrow_mut().open.extend([bad_file, good_file]);
    t.click("Import XML");
    t.click("Choose file…");
    let refused = t.has_containing("Not imported: ");
    t.click("Choose file…");
    t.shot("04d-import-xml");
    let valid = t.has_containing("Valid level XML");
    t.click("Import");
    let ed = t.ed();
    let ok = ed.serialize() == xml && !ed.undo_stack.is_empty();
    t.detail("an import is one undo step", ok && refused && valid, format!("{refused} {valid}"));

    // The far plane and the checks' click targets
    let dl = t.w().checks.iter().filter(|c| c.text.starts_with("Draw load")).count();
    t.detail("the draw load check is there", dl == 1, dl.to_string());
    t.click_containing("Draw load");
    let shown = t.ed().far_sphere.is_some();
    t.check("clicking it shows the far plane", shown);
    t.shot("05-far-plane");

    // Rules from the decomp: a bad trigger volume
    let ed = t.ed();
    let vol = ed.doc.iter_tags(ed.root(), &["PuzzleCondBallWithinRangeOfPoint"])[0];
    ed.set_attr(vol, "range", "0.0");
    let ok = t.has_check(None, "range = 0");
    t.check("a zero-range trigger is flagged", ok);
    t.ed().undo();

    // Inspector edits: type into a field
    let ed = t.ed();
    let tip = ent(ed, |e| e.kind == "tip");
    ed.select(tip, false);
    t.settle();
    t.fill("message_id", "5");
    let ed = t.ed();
    let v = ed.doc.attr(ed.entities[ent(ed, |e| e.kind == "tip").unwrap()].node, "message_id").to_string();
    t.detail("an inspector field edits the XML", v == "5", v);
    t.shot("06-inspector");

    // Commands inside elements: add, refuse a second one, move, remove; the schema check
    let ed = t.ed();
    let spin = ed.entities[ent(ed, |e| e.label == "spinner").unwrap()].node;
    ed.select(ent(ed, |e| e.label == "spinner"), false);
    t.settle();
    let allowed = glovebox_editor::commands::Format::shared().children_of("Platform");
    t.check("the Add command menu lists platform commands", ["PlatSpecial0xb9", "SetExit", "PlatCheckpoint"].iter().all(|c| allowed.contains(&c.to_string())));
    // The card's + menu
    t.pick("+", "PlatSpecial0xb9");
    t.ed().add_command(spin, "SetActorRotation", &[]);
    let tg = tags(t.ed(), spin);
    t.detail("SetActorRotation goes first, others last", tg[0] == "SetActorRotation" && tg[tg.len() - 1] == "PlatSpecial0xb9", format!("{tg:?}"));
    let ed = t.ed();
    let n = ed.undo_stack.len();
    ed.add_command(spin, "PlatSpecial0xb9", &[]);
    let ok = ed.undo_stack.len() == n && ed.last_status.0.contains("already has");
    t.check("a second one is refused", ok);
    let ok = backend.validate(&t.ed().serialize()).is_empty();
    t.check("the schema accepts the commands", ok);
    // The ↑ and × of the command's own card: the platform's card comes first, then one card for each command
    t.settle();
    let at = tags(t.ed(), spin).iter().position(|x| x == "PlatSpecial0xb9").unwrap();
    t.click_nth("↑", at + 1);
    let tg = tags(t.ed(), spin);
    t.detail("↑ moves a command earlier", tg.iter().position(|x| x == "PlatSpecial0xb9") == Some(tg.len() - 2), format!("{tg:?}"));
    let at = tg.iter().position(|x| x == "PlatSpecial0xb9").unwrap();
    t.click_nth("×", at + 1);
    let gone = t.ed().doc.first(spin, "PlatSpecial0xb9").is_none();
    t.check("× removes it", gone);
    let ed = t.ed();
    let enemy = ed.entities[ent(ed, |e| e.kind == "enemy").unwrap()].node;
    ed.add_command(enemy, "SetActorScale", &[]);
    let ok = tags(ed, enemy).last().map(|s| s.as_str()) == Some("EnemyFinalize");
    t.check("nothing goes after EnemyFinalize", ok);
    let ed = t.ed();
    let end = ed.end_marker().unwrap();
    ed.remove_node(end);
    let ok = ed.end_marker().is_some();
    t.check("EndLevelData can't be removed", ok);
    let ed = t.ed();
    let top = ed.top_level();
    ed.move_node(top[top.len() - 2], 1);
    let ok = tags(ed, ed.root()).last().map(|s| s.as_str()) == Some("EndLevelData");
    t.check("nothing moves past EndLevelData", ok);
    let ed = t.ed();
    let bad = ed.doc.first(spin, "PlatConstantSpin").unwrap();
    ed.set_attr(bad, "axis", "w");
    let ok = t.has_check(None, "libgarib's format");
    t.check("a value the format refuses shows in the checks", ok);
    t.ed().undo();

    // The order the game reads a platform in, and what a command needs first
    let ed = t.ed();
    let mark = ed.undo_stack.len();
    let spin = ed.entities[ent(ed, |e| e.label == "spinner").unwrap()].node;
    let oid = ed.doc.attr(spin, "object_id").to_string();
    ed.add_element(
        |ed| {
            let p = ed.doc.make("Platform", &[("object_id", &oid), ("name", "order")]);
            let pos = ed.doc.make("PlatSetInitialPos", &[("x", "0.0"), ("y", "0.0"), ("z", "0.0")]);
            ed.doc.append(p, pos);
            p
        },
        None,
    );
    let top = ed.top_level();
    let plat = top[top.len() - 2];
    ed.add_command(plat, "PlatScale", &[]);
    let tg = tags(ed, plat);
    let sc = ed.doc.first(plat, "PlatScale").unwrap();
    let one = ed.doc.attr(sc, "x") == "1.0";
    t.detail("PlatScale goes before the platform's position", tg[..2] == ["PlatScale", "PlatSetInitialPos"], format!("{tg:?}"));
    t.check("a new scale is 1, not 0", one);
    let ed = t.ed();
    ed.move_node(ed.doc.children(plat)[1], -1);
    let (ok, status) = (ed.doc.children(plat)[0] == sc && ed.last_status.0.contains("Can't move"), ed.last_status.0.clone());
    t.detail("a move the game would read wrong is refused", ok, status);
    let ed = t.ed();
    let n = ed.undo_stack.len();
    ed.add_command(plat, "PlatSpinPause0x7c", &[]);
    let ok = ed.undo_stack.len() == n && ed.last_status.0.contains("spin");
    t.check("a spin pause without a spin is refused", ok);
    let ed = t.ed();
    ed.add_command(plat, "PlatConstantSpin", &[]);
    ed.add_command(plat, "PlatSetTag", &[]);
    ed.add_command(plat, "PlatSpinPause0x7c", &[]);
    let tg = tags(ed, plat);
    let at = |x: &str| tg.iter().position(|y| y == x);
    t.detail("a spin pause goes right after its spin", at("PlatSpinPause0x7c") == at("PlatConstantSpin").map(|i| i + 1), format!("{tg:?}"));
    let ed = t.ed();
    let n = ed.undo_stack.len();
    ed.add_command(plat, "PlatRocking", &[]);
    let ok = ed.undo_stack.len() == n;
    t.check("a second motion is refused", ok);
    let ed = t.ed();
    ed.remove_node(ed.doc.first(plat, "PlatConstantSpin").unwrap());
    let ok = ed.doc.first(plat, "PlatSpinPause0x7c").is_none() && ed.doc.first(plat, "PlatSetTag").is_some();
    t.check("removing a spin removes its pause", ok);
    let ed = t.ed();
    for _ in 0..20 {
        ed.add_command(plat, "PlatPathPoint", &[]);
    }
    let n = ed.undo_stack.len();
    ed.add_command(plat, "PlatPathPoint", &[]);
    let pts = ed.doc.kids(plat, "PlatPathPoint");
    let ok = pts.len() == 20 && ed.undo_stack.len() == n;
    let apart = num0(&ed.doc, pts[1], "x") - num0(&ed.doc, pts[0], "x") == 100.0;
    t.detail("a 21st path point is refused", ok, pts.len().to_string());
    t.check("path points follow each other 100 apart", apart);
    // Tidy order puts back an order made by hand: the Tidy button of the platform's card
    let ed = t.ed();
    for p in pts {
        ed.doc.detach(p);
    }
    ed.doc.append(plat, sc);
    ed.after_edit(Some(&[plat]), false);
    t.settle();
    t.click("Tidy");
    let ed = t.ed();
    let tidy = tags(ed, plat)[0] == "PlatScale";
    t.check("Tidy order moves the scale back before the position", tidy);
    t.settle();
    let order = t.w().checks.iter().find(|c| c.level == "err" && c.text.contains("order")).map(|c| c.text.clone());
    t.detail("a tidy platform has no order errors", order.is_none(), format!("{order:?}"));

    // Enemies keep a normal instruction and their EnemyFinalize; empty groups go
    let ed = t.ed();
    let enemy = ed.entities[ent(ed, |e| e.kind == "enemy").unwrap()].node;
    let ok = ed.removal_refused(ed.doc.first(enemy, "EnemyFinalize").unwrap()).is_some();
    t.check("EnemyFinalize can't be removed", ok);
    let ed = t.ed();
    let kept = ["FogConfiguration", "AmbientLight"].iter().all(|x| ed.doc.first(ed.root(), x).is_none_or(|n| ed.removal_refused(n).is_some()));
    t.check("fog and ambient light can't be removed", kept);
    let ed = t.ed();
    ed.add_enemy("unknown-type");
    let new = *ed.doc.kids(ed.root(), "Enemy").last().unwrap();
    let ok = tags(ed, new) == ["NormalInstructions", "EnemyFinalize"];
    t.check("an enemy with no shipped script gets a rest instruction", ok);
    let ed = t.ed();
    let ins = ed.doc.first(ed.doc.first(new, "NormalInstructions").unwrap(), "EnemyInstruction").unwrap();
    let ok = ed.removal_refused(ins).is_some();
    t.check("its only normal instruction can't be removed", ok);
    let ed = t.ed();
    let grp = ed.doc.make("AttackInstructions", &[]);
    let copy = ed.doc.deep_copy(ins);
    ed.doc.append(grp, copy);
    let fin = ed.doc.first(new, "EnemyFinalize").unwrap();
    ed.doc.add_prev(fin, grp);
    ed.remove_node(copy);
    let ok = ed.doc.first(new, "AttackInstructions").is_none();
    t.check("removing a group's last instruction removes the group", ok);
    t.undo_to(mark);

    // The puzzle editor: a new puzzle, a condition and an action, their types, the checks
    let mark = t.ed().undo_stack.len();
    t.ed().set_selection(Vec::new());
    t.settle();
    t.click("+ New puzzle");
    let ed = t.ed();
    let p = ed.puzzle_shown();
    let title = format!("Puzzle {}", puzzles(ed));
    let ok = p.is_some() && t.has(&title);
    t.check("a new puzzle opens in the inspector", ok);
    let p = p.unwrap();
    // The + Add menus of the conditions and of the actions
    t.pick("+ Add condition", "(16)");
    t.pick("+ Add action", "(54)");
    let ed = t.ed();
    let tg = tags(ed, p);
    t.detail("conditions go before actions, after the limit", tg == ["PuzzleNumtimes", "PuzzleCond", "PuzzleAction"], format!("{tg:?}"));
    let ed = t.ed();
    let b = body(&ed.doc, ed.doc.first(p, "PuzzleAction").unwrap()).unwrap();
    let ok = ed.doc.attr(b, "hide_enabled") == "0x3F800000";
    t.check("float fields are written as bits", ok);
    let ed = t.ed();
    let cond = ed.doc.first(p, "PuzzleCond").unwrap();
    ed.set_part_type(cond, 33);
    let ok = ed.doc.tag(body(&ed.doc, cond).unwrap()) == "PuzzleCondSpecificEnemyExists" && ed.doc.attr(cond, "cond_type") == "33";
    t.check("changing a condition's type replaces its body", ok);
    t.ed().set_logic(p, "least", 3);
    let ok = t.has_check(Some("err"), "never fires");
    t.check("a puzzle that needs more conditions than it has is an error", ok);
    let ed = t.ed();
    ed.set_logic(p, "all", 0);
    ed.set_fires(p, None);
    let ok = ed.doc.first(p, "PuzzleNumtimes").is_none();
    t.check("no limit removes PuzzleNumtimes", ok);
    let ok = backend.validate(&t.ed().serialize()).is_empty();
    t.check("the schema accepts the puzzle", ok);
    let ed = t.ed();
    ed.edit(|ed| ed.doc.set(cond, "cond_type", "35"));
    let ok = t.has_check(None, "expects a <PuzzleCondGloverWithinVolume>");
    t.check("a body that doesn't match its type is an error", ok);
    // A real click on a condition card's ×
    let n = t.ed().doc.kids(p, "PuzzleCond").len();
    t.click_nth("×", 1);
    let (n2, status) = (t.ed().doc.kids(p, "PuzzleCond").len(), t.status());
    t.detail("clicking × on a condition removes it", n2 == n - 1, status);
    t.ed().undo();
    // The checks of what a puzzle reads through a platform's pointers
    let ed = t.ed();
    let p = ed.puzzle_shown().unwrap();
    ed.add_puzzle_part(p, "PuzzleAction", 45);
    let a = *ed.doc.kids(p, "PuzzleAction").last().unwrap();
    ed.set_attr(body(&ed.doc, a).unwrap(), "platform_tag", "-201");
    let ok = t.has_check(Some("err"), "crashes when there is none");
    t.check("a negative nudge tag with no platform is an error", ok);
    t.shot("07-puzzle");
    t.undo_to(mark);
    t.ed().open_puzzle(None);
    t.settle();

    // Enemy scripts: a ready-made script, moving and removing with Goto targets that follow
    let ed = t.ed();
    let mark = ed.undo_stack.len();
    let enemy = ent(ed, |e| e.kind == "enemy").unwrap();
    let en = ed.entities[enemy].node;
    ed.select(Some(enemy), false);
    t.settle();
    t.pick("Ready-made", "Chase the glove");
    let codes = |ed: &Editor| -> Vec<i128> { scripts::list_of(&ed.doc, en, "NormalInstructions").iter().map(|r| scripts::code(&ed.doc, *r)).collect() };
    let c = codes(t.ed());
    t.detail("a ready-made script replaces the normal list", c == [4, 9, 18], format!("{c:?}"));
    let ed = t.ed();
    let flags = ed.flag_enum();
    let ok = scripts::rules(&ed.doc, en, flags.as_ref()).is_empty();
    t.check("the ready-made script passes the script checks", ok);
    let ed = t.ed();
    ed.add_instruction(en, "NormalInstructions", 2);
    ed.move_instruction(scripts::list_of(&ed.doc, en, "NormalInstructions")[3], -1);
    let rows = scripts::list_of(&ed.doc, en, "NormalInstructions");
    let ok = codes(ed) == [4, 9, 2, 18] && ed.doc.attr(body(&ed.doc, rows[3]).unwrap(), "instr_idx") == "1";
    t.check("moving an instruction keeps Goto pointing at the same one", ok);
    let ed = t.ed();
    ed.remove_instruction(rows[0]);
    let rows = scripts::list_of(&ed.doc, en, "NormalInstructions");
    let ok = ed.doc.attr(body(&ed.doc, *rows.last().unwrap()).unwrap(), "instr_idx") == "0";
    t.check("removing an instruction shifts later Goto targets", ok);
    let ed = t.ed();
    ed.add_instruction(en, "ConditionalInstructions", 8);
    let tg = tags(ed, en);
    let ok = tg[tg.len() - 2..] == ["ConditionalInstructions", "EnemyFinalize"];
    t.check("a new kind gets its group before EnemyFinalize", ok);
    let ed = t.ed();
    let c0 = scripts::list_of(&ed.doc, en, "ConditionalInstructions")[0];
    let ok = scripts::flags_value(ed.doc.get(c0, "flags"), flags.as_ref()) & 0x1C0 != 0;
    t.check("Face the player gets a target", ok);
    let ed = t.ed();
    ed.set_instruction_type(*scripts::list_of(&ed.doc, en, "NormalInstructions").last().unwrap(), 9);
    let ok = t.has_check(Some("err"), "reads past the list");
    t.check("a list that can run off its end is an error", ok);
    let ok = backend.validate(&t.ed().serialize()).is_empty();
    t.check("the schema accepts the script", ok);
    // A real click on the ↓ of instruction #0
    t.settle();
    let before = codes(t.ed());
    let cards_before = tags(t.ed(), en).iter().filter(|x| !scripts::GROUPS.contains(&x.as_str()) && *x != "EnemyFinalize").count();
    t.click_nth("↓", 1 + cards_before);
    let after = codes(t.ed());
    t.detail("clicking ↓ on an instruction moves it", after[..2] == [before[1], before[0]], format!("{before:?} {after:?}"));
    t.shot("08-enemy-script");
    t.undo_to(mark);

    // Level settings: a look preset, the file name check
    let mark = t.ed().undo_stack.len();
    t.ed().set_selection(Vec::new());
    t.settle();
    t.pick("Look", "Graveyard violet");
    let ed = t.ed();
    let fog = ed.doc.first(ed.root(), "FogConfiguration").unwrap();
    let ok = ed.doc.attr(fog, "near_clip") == "1150" && ed.doc.kids(ed.root(), "DiffuseLight").len() == 2;
    t.check("a look preset sets the fog and lights", ok);
    t.fill("File name", "LEVEL1");
    let ed = t.ed();
    let name = ed.doc.attr(ed.root(), "name").to_string();
    let ok = t.has_check(Some("err"), "odd length");
    t.detail("an even-length file name is an error", ok && name == "LEVEL1", name);
    t.shot("09-level-settings");
    t.undo_to(mark);
    t.fill("title", "my level");
    t.pick("music", "17  ");
    let info = t.ed().level_info();
    let ok = info.name.as_deref() == Some("MY LEVEL") && info.song == Some(17) && info.bonus_time.is_none() && !info.extra;
    t.detail("the level's title and music are kept", ok, format!("{info:?}"));
    let ok = t.ed().project_levels().iter().any(|l| l.0 == "10");
    t.check("Build all lists the level", ok);
    let ed = t.ed();
    ed.set_level_info(|i| i.name = None);
    ed.set_level_info(|i| i.song = None);

    // Platform presets: each one is valid, and adds no problem of its own
    let ed = t.ed();
    let mark = ed.undo_stack.len();
    let base: Vec<String> = ed.checks().into_iter().filter(|c| c.level == "err").map(|c| c.text).collect();
    for p in PRESETS {
        ed.add_preset(p.key);
    }
    let errs: Vec<String> = ed.checks().into_iter().filter(|c| c.level == "err" && !base.contains(&c.text) && !c.text.contains("isn't in any object bank")).map(|c| c.text).collect();
    let names = ["hoop.nd", "heart", "ballbit", "lift", "glovebu", "stepy.n", "ootelep"];
    let n = ed.doc.iter_tags(ed.root(), &["Platform"]).iter().filter(|p| names.contains(&ed.doc.attr(**p, "name"))).count();
    t.detail("every platform preset passes the checks", errs.is_empty(), format!("{errs:?}"));
    t.detail("the presets add their platforms", n >= 16, n.to_string());
    t.undo_to(mark);

    more_clicks(&mut t);
    t.finish();
}

/// One click for each button the earlier checks do not click.
fn more_clicks(t: &mut T) {
    let none = Modifiers::default();
    // The toolbar's tools
    t.ed().set_selection(Vec::new());
    t.settle();
    for (label, mode) in [("Rotate", Mode::Rotate), ("Scale", Mode::Scale), ("Move", Mode::Translate)] {
        t.click(label);
        let ok = t.ed().mode == mode;
        t.check(&format!("click: {label}"), ok);
    }
    t.click("Box select");
    let on = t.ed().box_mode;
    t.click("Box select");
    let off = !t.ed().box_mode;
    t.check("click: Box select", on && off);
    t.click("Fly");
    let on = t.w().view.fly.on;
    t.click("Fly");
    let off = !t.w().view.fly.on;
    t.check("click: Fly", on && off);
    t.click("Motion");
    let on = t.ed().preview;
    t.click("Motion");
    let off = !t.ed().preview;
    t.check("click: Motion", on && off);
    t.click("Fog");
    let (off, saved) = (!t.ed().show_fog, t.w().settings.get("fog"));
    t.click("Fog");
    let on = t.ed().show_fog;
    t.check("click: Fog", on && off && saved.as_deref() == Some("off"));
    let ed = t.ed();
    let enemy = ent(ed, |e| e.kind == "enemy");
    ed.select(enemy, false);
    ed.orbit.target = [0.0; 3];
    t.settle();
    t.click("Focus");
    let moved = t.ed().orbit.target != [0.0; 3];
    t.check("click: Focus", moved);
    t.pick("Snap", "10");
    let snap = t.ed().snap;
    t.pick("Snap", "1");
    let now = t.ed().snap;
    t.check("click: Snap", snap == 10.0 && now == 1.0);
    t.pick("Pivot", "each origin");
    let pivot = t.ed().pivot_mode.clone();
    t.pick("Pivot", "group center");
    let now = t.ed().pivot_mode.clone();
    t.check("click: Pivot", pivot == "individual" && now == "group");
    let checked = t.w().quickboot;
    t.click("boot here");
    let now = t.w().quickboot;
    t.click("boot here");
    t.check("click: boot here", checked && !now);

    // The outliner
    t.click("all garibs");
    let ed = t.ed();
    let ok = !ed.selection.is_empty() && ed.selection.iter().all(|&i| ed.entities[i].kind == "garib");
    t.check("click: all (a group of the outliner)", ok);
    t.click_nth("×", 0);
    let n = t.ed().selection.len();
    let garibs = kinds(t.ed(), "garib").len();
    t.detail("click: × takes one out of the selection", n + 1 == garibs, format!("{n} {garibs}"));
    t.click("Apply move");
    t.click("Apply scale");
    let n = t.ed().undo_stack.len();
    t.fill("move by y", "25");
    t.click("Apply move");
    let moved = t.ed().undo_stack.len() == n + 1;
    t.click("Apply scale");
    t.check("click: Apply (move by, scale by)", moved);
    t.ed().undo();
    t.ed().undo();
    t.fill("Filter entities", "spinner");
    let shown = t.has_containing("PLATFORMS  1");
    t.fill("Filter entities", "");
    t.check("the filter narrows the outliner", shown);
    let n = kinds(t.ed(), "tip").len();
    t.click_containing("Mr. Tip ");
    let hidden = t.ed().hidden.contains("tip");
    t.click_containing("Mr. Tip ");
    let ok = !t.ed().hidden.contains("tip") && n > 0;
    t.check("click: a layer's checkbox", hidden && ok);
    let ed = t.ed();
    let tip = ent(ed, |e| e.kind == "tip").unwrap();
    let row = format!("{}   {}", ed.entities[tip].label, ed.entities[tip].tag);
    t.click(&row);
    let sel = t.ed().selection.clone();
    t.detail("click: a row of the outliner selects", sel == vec![tip], row);
    t.click("box select includes landscape");
    let on = t.ed().include_land;
    t.click("box select includes landscape");
    let off = !t.ed().include_land;
    t.check("click: box select includes landscape", on && off);

    // The inspector
    let ed = t.ed();
    let trigger = ent(ed, |e| e.kind == "trigger" && !e.zone);
    ed.select(trigger, false);
    t.settle();
    t.shot("09a-trigger");
    t.click("Edit its puzzle (conditions and actions)");
    let shown = t.ed().puzzle_shown().is_some();
    t.check("click: Edit its puzzle", shown);
    t.click("← Level settings and all puzzles");
    let back = t.ed().puzzle_shown().is_none() && t.has("LEVEL SETTINGS");
    t.check("click: ← Level settings and all puzzles", back);
    t.click_containing("1. ");
    let shown = t.ed().puzzle_index == Some(0);
    t.check("click: a row of the puzzle list", shown);
    let n = puzzles(t.ed());
    t.click_nth("×", 0);
    let n2 = puzzles(t.ed());
    t.ed().undo();
    t.ed().open_puzzle(None);
    t.settle();
    t.detail("click: × deletes a puzzle", n2 + 1 == n, format!("{n} {n2}"));
    let n = t.ed().undo_stack.len();
    t.pick("+ Add level setting", "Gravity");
    let ed = t.ed();
    let ok = ed.doc.first(ed.root(), "SetGravity").is_some();
    t.check("click: + Add (level settings)", ok);
    t.undo_to(n);
    t.pick("clock", "No clock");
    let bt = t.ed().level_info().bonus_time;
    t.pick("clock", "Keep the shipped clock");
    let kept = t.ed().level_info().bonus_time.is_none();
    t.check("click: the bonus clock", bt == Some(-1) && kept);
    // A camera zone
    let ed = t.ed();
    let zone = ent(ed, |e| e.zone);
    ed.select(zone, false);
    t.settle();
    let n = t.ed().undo_stack.len();
    t.pick("preset", "Fixed: flies");
    let cam = t.ed().camera.pos;
    t.shot("10-camera-zone");
    let has = t.click("Look from the zone’s camera");
    let moved = t.ed().camera.pos != cam;
    t.check("click: Look from the zone’s camera", has && moved);
    let before = t.ed().undo_stack.len();
    t.click("distance");
    let changed = t.ed().undo_stack.len() == before + 1;
    t.check("click: a camera zone's switch", changed);
    t.undo_to(n);
    // An enemy's script
    let ed = t.ed();
    let enemy = ent(ed, |e| e.kind == "enemy").unwrap();
    let en = ed.entities[enemy].node;
    ed.select(Some(enemy), false);
    let n = ed.undo_stack.len();
    ed.add_script_template(en, "Chase the glove");
    t.settle();
    t.click_nth("…", 0);
    let open = t.w().inspector.script_open.is_some();
    t.check("click: … opens an instruction's fields", open);
    t.click_nth("…", 0);
    let count = scripts::list_of(&t.ed().doc, en, "NormalInstructions").len();
    t.pick("+ Add instruction", "Rest");
    let count2 = scripts::list_of(&t.ed().doc, en, "NormalInstructions").len();
    t.detail("click: + Add (an instruction)", count2 == count + 1, format!("{count} {count2}"));
    t.undo_to(n);
    // New look: the fixture can't make one, and says so
    let ed = t.ed();
    let grave = ent(ed, |e| e.label == "grave");
    ed.select(grave, false);
    t.settle();
    t.click("Apply");
    t.until(20, |w| w.jobs.running == 0 && w.loading.is_empty());
    let status = t.status();
    t.detail("click: Apply (new look)", status.contains("Could not make a new look"), status);
    t.click("Try another");
    t.until(20, |w| w.jobs.running == 0 && w.loading.is_empty());
    let seed = t.w().inspector.reskin_seed;
    t.check("click: Try another (new look)", seed == 1);
    // A garib's handle: Select its owner
    let ed = t.ed();
    let handle = ent(ed, |e| e.is_handle && e.owner.is_some());
    if let Some(h) = handle {
        let owner = ed.entities[h].owner;
        let label = format!("Select {}", ed.entities[owner.unwrap()].label);
        ed.select(Some(h), false);
        t.settle();
        t.click(&label);
        let sel = t.ed().selected();
        t.check("click: Select (a handle's owner)", sel == owner);
    }

    // The palette's other buttons
    t.ed().set_selection(Vec::new());
    t.click("+ Add");
    let open = t.w().palette_open;
    t.check("click: + Add", open);
    t.click("Collectables");
    let n = kinds(t.ed(), "garib").len();
    t.click("★ Garib");
    t.click("! 500-point bang");
    t.click("♥ Extra life");
    t.click("✴ Mad garib");
    let n2 = kinds(t.ed(), "garib").len();
    t.detail("click: the garib buttons", n2 == n + 4, format!("{n} {n2}"));
    for _ in 0..4 {
        t.ed().undo();
    }
    t.click("Camera");
    let n = puzzles(t.ed());
    t.click("Add Behind");
    let n2 = puzzles(t.ed());
    t.check("click: Add (a camera zone)", n2 == n + 1);
    t.ed().undo();
    t.click("Models");
    t.fill("Search models", "block");
    let n = t.ed().undo_stack.len();
    t.click("Background block");
    t.click("Platform block");
    let n2 = t.ed().undo_stack.len();
    t.detail("click: Background and Platform (a model)", n2 == n + 2, format!("{n} {n2}"));
    t.undo_to(n);
    t.fill("Search models", "");
    t.pick("Bank", "OTHER_BANK");
    let only = t.has("Scenery sign") && !t.has("Scenery block");
    t.pick("Bank", "All banks");
    t.check("click: the bank list", only);
    // The kit: a picture of each prop, then one placed as scenery and one as background
    t.click("Kit");
    t.until(120, |w| w.side_jobs.running == 0);
    t.shot("11-kit");
    let n = t.ed().undo_stack.len();
    let label = t.label_with("Scenery ");
    t.click(&label);
    t.until(120, |w| w.jobs.running == 0 && w.loading.is_empty());
    let (placed, status) = (t.ed().undo_stack.len() == n + 1, t.status());
    t.detail("click: Scenery (a kit model)", placed, format!("{label}: {status}"));
    let label = label.replace("Scenery ", "Background ");
    t.click(&label);
    t.until(120, |w| w.jobs.running == 0 && w.loading.is_empty());
    let placed = t.ed().undo_stack.len() == n + 2;
    t.check("click: Background (a kit model)", placed);
    // The kit model is now one of the author's imported models: Place, then Remove with its question
    t.click("Models");
    t.shot("12-imported-models");
    let n2 = t.ed().undo_stack.len();
    let place = t.label_with("Place ");
    t.click(&place);
    let placed = t.ed().undo_stack.len() == n2 + 1;
    t.detail("click: Place (an imported model)", placed, place.clone());
    t.undo_to(n);
    t.click(&place.replace("Place ", "Remove "));
    let asked = t.has("Remove model");
    t.click("No");
    let kept = t.has(&place);
    t.click(&place.replace("Place ", "Remove "));
    t.click("Yes");
    let gone = !t.has(&place);
    t.detail("click: Remove (an imported model), No and Yes", asked && kept && gone, format!("{asked} {kept} {gone}"));
    // Import model: a file that is no model, then a model of one triangle
    let bad = t.home.join("bad.glb");
    std::fs::write(&bad, b"not a model").unwrap();
    let glb = t.home.join("tri.glb");
    std::fs::write(&glb, tiny_glb()).unwrap();
    t.files.0.borrow_mut().open.extend([bad, glb.clone()]);
    t.click("Import model…");
    let open = t.has("Import model");
    t.click("Choose .glb…");
    let refused = t.has_containing("can't be read as a glTF model");
    t.click("Choose .glb…");
    let read = t.has_containing("1 triangles, in 1 piece");
    t.shot("13-import-model");
    let n = t.ed().undo_stack.len();
    t.click("Import and place");
    t.until(60, |w| w.jobs.running == 0 && w.loading.is_empty());
    let (placed, status) = (t.ed().undo_stack.len() == n + 1, t.status());
    t.detail("click: Import model…, Choose .glb… and Import and place", open && refused && read && placed, format!("{open} {refused} {read} {status}"));
    t.undo_to(n);
    t.click("Import model…");
    t.click("Cancel");
    let closed = t.w().dialog.is_none();
    t.check("click: Cancel (Import model)", closed);
    // A model file dropped on the window opens the same dialog
    t.w().file_dropped(glb);
    t.settle();
    let open = t.has("Import and place");
    t.click("Cancel");
    t.check("a dropped model opens Import model", open);
    let xml_file = t.home.join("good.xml");
    t.w().file_dropped(xml_file);
    t.settle();
    let open = t.has_containing("Valid level XML");
    t.click("Cancel");
    t.check("a dropped XML file opens Import level XML", open);
    t.key(Key::Escape);
    let closed = !t.w().palette_open;
    t.check("Esc closes the palette", closed);

    // The path panel's other buttons
    t.click("Path");
    let (vw, vh) = (t.ed().camera.width, t.ed().camera.height);
    for x in [0.3, 0.5, 0.7] {
        t.view_click(((vw * x).trunc(), (vh * 0.7).trunc()), none);
    }
    t.key(Key::Backspace);
    let n = t.ed().path.pts.len();
    t.check("Backspace takes the last path point off", n == 2);
    t.view_click(((vw * 0.7).trunc(), (vh * 0.7).trunc()), none);
    let ed = t.ed();
    let mark = ed.undo_stack.len();
    let land = ent(ed, |e| e.tag == "BackgroundActor").or(ent(ed, |e| e.tag == "LandActor"));
    ed.set_selection(land.into_iter().collect());
    t.settle();
    t.click("Scatter");
    let (one, seed) = (t.ed().undo_stack.len(), t.w().path_panel.sc_seed);
    t.click("Try another");
    let (two, seed2) = (t.ed().undo_stack.len(), t.w().path_panel.sc_seed);
    t.detail("click: Scatter and Try another", one == mark + 1 && two == mark + 1 && seed2 == seed + 1, format!("{mark} {one} {two}"));
    t.undo_to(mark);
    let n = puzzles(t.ed());
    t.click("Add zones");
    let n2 = puzzles(t.ed());
    t.detail("click: Add zones", n2 > n, format!("{n} {n2}"));
    t.undo_to(mark);
    t.click("Clear path");
    let n = t.ed().path.pts.len();
    t.check("click: Clear path", n == 0);
    t.click("Path");
    let off = !t.ed().path.on;
    t.check("click: Path turns the tool off", off);

    // The checks panel
    t.ed().add_model(SIGN, "LandActor");
    t.until(30, |w| w.ed.plan.is_some());
    t.click_containing("sign (");
    let ed = t.ed();
    let ok = !ed.selection.is_empty() && ed.selection.iter().all(|&i| ed.entities[i].object_id.as_deref() == Some(SIGN));
    t.check("click: an import's line selects its objects", ok);
    t.click("Copy the models into the ROM instead");
    let on = t.ed().copy_imports;
    t.shot("14-imports");
    t.click("Copy the models into the ROM instead");
    let off = !t.ed().copy_imports;
    t.check("click: Copy the models into the ROM instead", on && off);
    t.ed().undo();

    // Export patch, Build all, Test level and the patcher
    t.click("Export patch");
    t.shot("15-export-patch");
    let open = t.has_containing("A BPS patch for other players");
    t.click("Cancel");
    let closed = t.w().dialog.is_none();
    t.click("Export patch");
    t.click("Export .bps");
    t.until(60, |w| w.jobs.running == 0 && w.loading.is_empty());
    let status = t.status();
    t.detail("click: Export patch, Cancel and Export .bps", open && closed && status.contains("no ROM"), status);
    t.ed().set_level_info(|i| i.song = Some(3));
    t.click("Build all…");
    t.shot("16-build-all");
    let listed = t.has_containing("Fixture 1 (10)");
    t.click("Cancel");
    let closed = t.w().dialog.is_none();
    t.click("Build all…");
    t.click("Export .bps");
    t.until(60, |w| w.jobs.running == 0 && w.loading.is_empty());
    let bps = t.status();
    t.click("Build all…");
    t.click_nth("Build ROM", 1);
    t.until(60, |w| w.jobs.running == 0 && w.loading.is_empty());
    let rom = t.status();
    t.ed().set_level_info(|i| i.song = None);
    t.detail("click: Build all…, Cancel, Export .bps and Build ROM", listed && closed && !bps.is_empty() && !rom.is_empty(), format!("{bps} / {rom}"));
    t.click("Test level");
    let asked = t.has_containing("Test level opens the built ROM in your emulator");
    t.shot("17-test-level");
    t.click("OK");
    let status = t.status();
    t.detail("click: Test level asks for the emulator once", asked && status.contains("Test level needs an emulator."), status);
    // With an emulator chosen, the level is built for it (the fixture has no ROM to build)
    let emulator = t.home.join("emulator");
    std::fs::write(&emulator, b"").unwrap();
    t.files.0.borrow_mut().open.push_back(emulator.clone());
    t.key(Key::T);
    t.click("OK");
    t.until(60, |w| w.jobs.running == 0 && w.loading.is_empty());
    let (saved, status) = (t.w().settings.get("emulator"), t.status());
    t.detail("T tests the level with the chosen emulator", saved.is_some() && status.contains("no ROM"), status);
    t.click("Patcher ↗");
    let page = t.home.join("patcher.html");
    let ok = std::fs::read_to_string(&page).is_ok_and(|p| p.contains("<html"));
    t.check("click: Patcher ↗ writes the page", ok);
    let ini = std::fs::read_to_string(t.home.join("settings.ini")).unwrap_or_default();
    t.detail("the settings are kept in settings.ini", ini.contains("scene=10") && ini.contains("emulator="), ini);

    // Keys
    t.ed().set_selection(Vec::new());
    t.settle();
    t.key_with(Modifiers::CTRL, Key::A);
    let ed = t.ed();
    let ok = !ed.selection.is_empty() && ed.selection.iter().all(|&i| ed.entities[i].kind != "land");
    t.check("Ctrl+A selects all but the landscape", ok);
    t.key(Key::Escape);
    let none_left = t.ed().selection.is_empty();
    t.check("Esc clears the selection", none_left);
    t.key(Key::R);
    let scale = t.ed().mode == Mode::Scale;
    t.key(Key::W);
    t.key(Key::P);
    let path = t.ed().path.on;
    t.key(Key::P);
    let ed = t.ed();
    let back = ed.mode == Mode::Translate && !ed.path.on;
    t.check("R, W and P switch tools", scale && path && back);
    let ed = t.ed();
    let enemy = ent(ed, |e| e.kind == "enemy");
    ed.select(enemy, false);
    let target = ed.orbit.target;
    ed.orbit.target = [0.0, 0.0, 0.0];
    t.settle();
    t.key(Key::F);
    let ok = t.ed().orbit.target != [0.0, 0.0, 0.0] && target != [0.0; 3];
    t.check("F frames the selection", ok);
    let n = t.ed().undo_stack.len();
    t.fill("position x", "123");
    t.key_with(Modifiers::CTRL, Key::Z);
    let undone = t.ed().undo_stack.len() == n;
    t.key_with(Modifiers::CTRL | Modifiers::SHIFT, Key::Z);
    let redone = t.ed().undo_stack.len() == n + 1;
    t.key_with(Modifiers::CTRL, Key::Z);
    t.key_with(Modifiers::CTRL, Key::Y);
    let again = t.ed().undo_stack.len() == n + 1;
    t.key_with(Modifiers::CTRL, Key::Z);
    t.check("Ctrl+Z, Ctrl+Shift+Z and Ctrl+Y undo and redo", undone && redone && again);
    // Keys typed into a text box stay there
    t.click_containing("Filter entities");
    t.h.event(egui::Event::Text("b".to_string()));
    t.h.key_press(Key::B);
    t.settle();
    let ok = !t.ed().box_mode && t.ed().filter == "b";
    t.fill("Filter entities", "");
    t.check("keys typed into a text box are not shortcuts", ok);
    // The wheel zooms, a right drag pans
    let cam = t.ed().camera.pos;
    let pos = t.at((300.0, 300.0));
    t.h.event(egui::Event::PointerMoved(pos));
    t.h.event(egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Line, delta: egui::vec2(0.0, 1.0), phase: egui::TouchPhase::Move, source: egui::MouseWheelSource::Wheel, modifiers: none });
    t.settle();
    let zoomed = t.ed().camera.pos != cam;
    t.check("the wheel zooms", zoomed);
    let target = t.ed().orbit.target;
    for (pressed, at) in [(true, (300.0, 300.0)), (false, (340.0, 320.0))] {
        let pos = t.at(at);
        t.h.event(egui::Event::PointerMoved(pos));
        t.h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Secondary, pressed, modifiers: none });
        t.h.step();
    }
    let panned = t.ed().orbit.target != target;
    t.check("a right drag pans", panned);
    review_fixes(t);
    t.shot("18-end");
}

/// What a review of the window found.
fn review_fixes(t: &mut T) {
    // Esc and Enter answer a question and a note
    let ed = t.ed();
    let tip = ent(ed, |e| e.kind == "tip");
    ed.select(tip, false);
    ed.apply_transform(Some([5.0, 0.0, 0.0]), None, 1, None);
    t.settle();
    t.click("Reset level");
    let asked = t.w().dialog.is_some();
    t.key(Key::Escape);
    let kept = t.w().dialog.is_none() && t.ed().is_edited();
    t.click("Reset level");
    t.key(Key::Enter);
    let reset = t.w().dialog.is_none() && !t.ed().is_edited();
    t.detail("Esc answers No and Enter answers Yes in a question", asked && kept && reset, format!("{asked} {kept} {reset}"));
    t.click("Undo");
    t.w().settings.remove("emulator");
    t.click("Test level");
    let asked = t.w().dialog.is_some();
    t.key(Key::Escape);
    let (closed, status) = (t.w().dialog.is_none(), t.status());
    t.click("Test level");
    t.key(Key::Enter);
    let closed2 = t.w().dialog.is_none();
    t.detail("Esc and Enter close a note", asked && closed && closed2 && status.contains("Test level needs an emulator."), status);

    // Fly mode: Shift typed into a text box, or held at a dialog, does not move the camera
    t.ed().set_selection(Vec::new());
    t.settle();
    t.click("Fly");
    t.fill("Filter entities", "");
    t.click_containing("Filter entities");
    let p0 = t.ed().camera.pos;
    t.h.event(egui::Event::ModifiersChanged(Modifiers::SHIFT));
    for _ in 0..4 {
        t.h.step();
    }
    let still = t.ed().camera.pos == p0;
    t.h.key_press(Key::Enter);
    t.settle();
    // Without the text box the same key moves it
    t.h.event(egui::Event::ModifiersChanged(Modifiers::SHIFT));
    for _ in 0..4 {
        t.h.step();
    }
    let moved = t.ed().camera.pos[1] < p0[1];
    t.click("Reset level");
    let p1 = t.ed().camera.pos;
    for _ in 0..4 {
        t.h.step();
    }
    let still_at_dialog = t.w().dialog.is_none() || t.ed().camera.pos == p1;
    t.h.event(egui::Event::ModifiersChanged(Modifiers::default()));
    t.key(Key::Escape);
    t.click("Fly");
    let off = !t.w().view.fly.on;
    t.detail("fly keys are ignored in a text box and at a dialog", still && moved && still_at_dialog && off, format!("{still} {moved} {still_at_dialog} {off}"));

    // The clear buttons of the filter and of the model search
    t.fill("Filter entities", "spin");
    let had = t.ed().filter == "spin" && t.has("Clear filter");
    t.click("Clear filter");
    let cleared = t.ed().filter.is_empty() && !t.has("Clear filter");
    t.detail("click: the filter's clear button", had && cleared, format!("{had} {cleared}"));
    // The Models tab opens with the search box ready for typing
    t.click("+ Add");
    t.click("Collectables");
    t.click("Models");
    let focused = t.find("Search models", false).iter().any(|n| n.is_focused());
    t.h.event(egui::Event::Text("gra".to_string()));
    t.settle();
    let typed = t.w().palette.query == "gra";
    t.detail("the Models tab opens with the search box focused", focused && typed, format!("{focused} {typed}"));
    t.click("Clear search");
    let cleared = t.w().palette.query.is_empty() && !t.has("Clear search");
    t.check("click: the search's clear button", cleared);

    // The kit: a value an item can't be made with does not stop its picture for good
    t.click("Kit");
    t.until(120, |w| w.side_jobs.running == 0 && w.palette.kit_picture("grave").is_some());
    t.fill("Lean", "x");
    let refused = t.until(30, |w| w.side_jobs.running == 0 && w.palette.kit_picture("grave").is_some_and(|p| p.0.contains("\"x\"")));
    t.fill("Lean", "9");
    let again = t.until(30, |w| w.side_jobs.running == 0 && w.palette.kit_picture("grave").is_some_and(|p| p.0.contains("\"9\"") && p.1));
    t.detail("a kit picture is drawn again after a wrong value", refused && again, format!("{refused} {again}"));
    t.click("+ Add");

    // The path panel's camera list says what each camera does
    t.click("Path");
    t.click("camera");
    if let Some(n) = t.find("High", true).first() {
        n.hover();
    }
    for _ in 0..8 {
        t.h.step();
    }
    let tip = t.has_containing("rises and looks down");
    t.choose("High");
    t.click("Path");
    t.check("the camera list of the path panel has tooltips", tip);
}

/// The window at 1280 by 720 and at a display scale of 1.5: nothing panics and the panels stay inside.
/// The exit triggers: the toolbar's Exits switch, the layer's checkbox, the spheres and the inspector's block.
#[test]
fn exit_triggers() {
    use glovebox_editor::exits::LAYER;
    let backend: Arc<dyn Backend> = Arc::new(FixtureBackend::new());
    let mut t = T::new("exits", Some(backend), [1500.0, 950.0]);
    let none = Modifiers::default();
    t.idle();
    let ed = t.ed();
    let goal = ent(ed, |e| e.label == "goal").unwrap();
    ed.focus_on(goal);
    let trigger = ed.exit_trigger(goal).unwrap();
    let r = trigger.radius.unwrap();
    let sphere = ed.nodes[goal].exit.unwrap();
    let shown = ed.graph[sphere].visible && r > 20.0;
    t.detail("an exit platform has a sphere of the game's radius", shown, format!("{r}"));
    // A garib inside the sphere, on one side of the block. A spot on the sphere on the other side.
    let ed = t.ed();
    let across = vec::cross(ed.camera.forward(), [0.0, 1.0, 0.0]);
    let side = vec::scale(across, (r - 4.0) / vec::norm(across));
    let inside = vec::sub(trigger.center, side);
    ed.add_garib(inside, "garib");
    ed.select(None, false);
    let goal = ent(ed, |e| e.label == "goal").unwrap();
    let sphere = ed.nodes[goal].exit.unwrap();
    t.settle();
    let spot = px(t.ed(), vec::add(trigger.center, side));
    let pixel = |t: &mut T| {
        let at = t.at(spot);
        t.settle();
        t.h.render().ok().map(|img| img.get_pixel(at.x as u32, at.y as u32).0)
    };
    t.shot("50-exits-on");

    // A click on the sphere goes through it
    t.view_click(spot, none);
    let sel = t.ed().selection.clone();
    t.detail("a click on a sphere does not select its platform", !sel.contains(&goal), format!("{sel:?}"));
    let at = px(t.ed(), inside);
    t.view_click(at, none);
    let ed = t.ed();
    let picked = ed.selection.first().map(|&i| ed.entities[i].kind);
    t.detail("a click on a sphere selects what is inside it", picked == Some("garib"), format!("{picked:?}"));
    t.ed().select(None, false);
    let at = px(t.ed(), trigger.center);
    t.view_click(at, none);
    let sel = t.ed().selection.clone();
    let what: Vec<String> = sel.iter().map(|&i| t.ed().entities[i].label.clone()).collect();
    t.detail("a click on the platform selects it", sel == vec![goal], format!("{sel:?} {what:?} {goal}"));
    let block = t.has("Exit trigger") && t.has("the ball (its center)") && t.has("Glover must hold the ball.") && t.has(&format!("{r:.1}"));
    t.check("the inspector shows the exit trigger", block);
    t.shot("51-exits-inspector");
    t.ed().select(None, false);
    let with = pixel(&mut t);

    // The toolbar's switch
    t.click("Exits");
    let ed = t.ed();
    let off = ed.hidden.contains(LAYER) && !ed.graph[sphere].visible && ed.graph[ed.nodes[goal].obj].visible;
    let saved = t.w().settings.get("exits");
    t.detail("click: Exits hides the spheres and keeps the platforms", off && saved.as_deref() == Some("off"), format!("{saved:?}"));
    let without = pixel(&mut t);
    t.detail("the view draws the sphere only when the layer is on", with.is_some() && with != without, format!("{with:?} {without:?}"));
    t.shot("52-exits-off");
    t.click("Exits");
    let ed = t.ed();
    let on = !ed.hidden.contains(LAYER) && ed.graph[sphere].visible;
    let saved = t.w().settings.get("exits");
    t.check("click: Exits shows them again", on && saved.as_deref() == Some("on"));
    let back = pixel(&mut t);
    t.detail("the sphere is drawn again", back == with, format!("{back:?} {with:?}"));

    // The layer's checkbox is the same switch
    let label = t.has("Exit triggers 1");
    t.click("Exit triggers 1");
    let ed = t.ed();
    let off = ed.hidden.contains(LAYER) && !ed.graph[sphere].visible;
    t.click("Exit triggers 1");
    let on = !t.ed().hidden.contains(LAYER);
    t.check("click: the Exit triggers layer's checkbox", label && off && on);
    t.finish();
}

/// Number fields change by dragging: a press that moves to the side changes the value, as one undo step.
#[test]
fn number_fields_drag() {
    let backend: Arc<dyn Backend> = Arc::new(FixtureBackend::new());
    let mut t = T::new("numfield", Some(backend), [1500.0, 950.0]);
    let none = Modifiers::default();
    t.idle();
    let ed = t.ed();
    let enemy = ent(ed, |e| e.kind == "enemy").unwrap();
    ed.select(Some(enemy), false);
    t.settle();
    let x = |t: &mut T| {
        let ed = t.ed();
        let i = ed.selection[0];
        let bind = ed.entities[i].bind.clone().unwrap();
        bind.get(&ed.doc, &ed.entities)[0]
    };
    let (x0, mark, xml0) = (x(&mut t), t.ed().undo_stack.len(), t.ed().serialize());

    // A drag to the right: one unit for each point, live, and one undo step
    let at = t.field_press_and_move("position x", 25.0, none);
    let live = x(&mut t);
    let held = t.ed().undo_stack.len();
    t.detail("drag: the value follows the pointer while the button is down", at.is_some() && (live - x0.round() - 25.0).abs() < 1e-6 && held == mark, format!("{x0} {live} {held}"));
    t.shot("60-field-drag");
    t.release(at.unwrap());
    let (x1, steps) = (x(&mut t), t.ed().undo_stack.len());
    let moved = t.ed().serialize() != xml0 && !t.h.ctx.egui_wants_keyboard_input();
    t.detail("drag: 25 points to the right add 25 to position x, as one undo step", (x1 - x0.round() - 25.0).abs() < 1e-6 && steps == mark + 1 && moved, format!("{x0} {x1} {steps} {mark}"));
    t.ed().undo();
    t.settle();
    let back = t.ed().serialize() == xml0;
    t.check("drag: one undo puts the level back", back);

    // To the left
    t.field_drag("position x", -12.0, none);
    let x1 = x(&mut t);
    t.detail("drag: to the left the value goes down", (x1 - x0.round() + 12.0).abs() < 1e-6, format!("{x0} {x1}"));
    t.undo_to(mark);

    // Shift is ten times finer, Ctrl ten times coarser
    t.field_drag("position x", 30.0, Modifiers::SHIFT);
    let x1 = x(&mut t);
    let steps = t.ed().undo_stack.len();
    t.detail("drag: with Shift, 30 points add 3", (x1 - x0 - 3.0).abs() < 0.051 && steps == mark + 1, format!("{x0} {x1}"));
    t.undo_to(mark);
    t.field_drag("position x", 20.0, Modifiers::CTRL);
    let x1 = x(&mut t);
    t.detail("drag: with Ctrl, 20 points add 200", (x1 - x0).abs() > 190.0 && (x1 - x0).abs() < 210.0 && (x1 / 10.0).fract() == 0.0, format!("{x0} {x1}"));
    t.undo_to(mark);

    // Esc during the drag
    let at = t.field_press_and_move("position x", 40.0, none);
    let live = x(&mut t);
    t.h.key_press(Key::Escape);
    t.settle();
    let after_esc = x(&mut t);
    let at = at.unwrap();
    t.h.event(egui::Event::PointerMoved(at + egui::vec2(15.0, 0.0)));
    t.h.step();
    t.release(at + egui::vec2(15.0, 0.0));
    let ed = t.ed();
    let same = ed.serialize() == xml0 && ed.undo_stack.len() == mark && ed.selection.len() == 1;
    t.detail("drag: Esc puts the old value back and adds no undo step", live != x0 && after_esc == x0 && same, format!("{x0} {live} {after_esc}"));

    // A plain click still puts the caret in the field, and a typed number is written
    let at = t.field_at("position x").unwrap();
    t.h.event(egui::Event::PointerMoved(at));
    t.h.step();
    t.h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: true, modifiers: none });
    t.h.step();
    t.release(at);
    let focused = t.h.ctx.egui_wants_keyboard_input() && t.ed().serialize() == xml0;
    t.check("click: a press without movement gives the field the keyboard", focused);
    // With the keyboard in the field, a drag selects text and changes nothing
    let at = t.field_press_and_move("position x", 20.0, none).unwrap();
    t.release(at);
    let still = t.ed().serialize() == xml0 && t.h.ctx.egui_wants_keyboard_input();
    t.check("drag: in a field that has the keyboard, a drag does not change the number", still);
    t.key(Key::Enter);
    t.h.event(egui::Event::PointerMoved(egui::pos2(700.0, 500.0)));
    t.settle();
    let typed = t.fill("position x", "123.5");
    let x1 = x(&mut t);
    let steps = t.ed().undo_stack.len();
    t.detail("typing a number into the field still works", typed && x1 == 123.5 && steps == mark + 1, format!("{x1}"));
    t.undo_to(mark);

    // A scale steps by a hundredth
    let ed = t.ed();
    let s0 = ed.entities[ed.selection[0]].get_scale(&ed.doc);
    if let Some(s0) = s0 {
        t.field_drag("scale x", 20.0, none);
        let ed = t.ed();
        let s1 = ed.entities[ed.selection[0]].get_scale(&ed.doc).unwrap();
        t.detail("drag: a scale changes by 0.01 for each point", (s1[0] - s0[0] - 0.2).abs() < 0.006, format!("{s0:?} {s1:?}"));
        t.undo_to(mark);
    }

    // A whole-number command field keeps the limits of its type
    t.ed().select(None, false);
    t.settle();
    let found = [("  u1", "255", "0", none), ("  u2", "65535", "0", Modifiers::CTRL), ("  s2", "32767", "-32768", Modifiers::CTRL)].into_iter().find_map(|(ty, max, min, mods)| {
        let label = [None, Some(enemy)].into_iter().find_map(|sel| {
            t.ed().select(sel, false);
            t.settle();
            t.find(ty, false).iter().filter(|n| n.accesskit_node().role() == egui::accesskit::Role::TextInput).find_map(|n| n.accesskit_node().label())
        })?;
        Some((label, max, min, mods))
    });
    t.detail("the fixture shows a whole-number field", found.is_some(), format!("{found:?}"));
    if let Some((label, max, min, mods)) = found {
        let before = t.ed().serialize();
        t.field_drag(&label, 7000.0, mods);
        let up = t.ed().serialize();
        t.field_drag(&label, -14000.0, mods);
        let down = t.ed().serialize();
        // The attribute that changed, in the two documents
        let changed = |a: &str, b: &str| -> Option<String> {
            let at = a.bytes().zip(b.bytes()).position(|(x, y)| x != y)?;
            let from = b[..at].rfind('"')? + 1;
            let to = from + b[from..].find('"')?;
            Some(b[from..to].to_string())
        };
        let (hi, lo) = (changed(&before, &up), changed(&up, &down));
        t.detail(&format!("drag: the field {label} stops at {max} and at {min}"), hi.as_deref() == Some(max) && lo.as_deref() == Some(min), format!("{hi:?} {lo:?}"));
        let steps = t.ed().undo_stack.len();
        t.check("drag: each drag on the whole-number field is one undo step", steps == mark + 2);
        t.undo_to(mark);
    }

    // A text field does not change on a drag
    t.ed().select(None, false);
    t.settle();
    let ed = t.ed();
    let name = ed.doc.attr(ed.doc.root(), "name").to_string();
    let dragged = t.field_drag("File name", 40.0, none);
    let ed = t.ed();
    let same = ed.doc.attr(ed.doc.root(), "name") == name && ed.undo_stack.len() == mark;
    t.check("drag: a text field does not change", dragged && same);

    // Pictures of a list and of a menu, for the look
    if t.shots.is_some() {
        t.click("Level");
        t.shot("61-level-list");
        t.key(Key::Escape);
        t.click("+ Add level setting");
        t.shot("62-menu");
        t.key(Key::Escape);
    }
    t.finish();
}

#[test]
fn small_and_scaled_windows() {
    for (name, size, scale) in [("1280x720", [1280.0, 720.0], 1.0), ("scale-150", [1920.0, 1080.0], 1.5), ("1920x1080", [1920.0, 1080.0], 1.0)] {
        let backend: Arc<dyn Backend> = Arc::new(FixtureBackend::new());
        let mut t = T::new(name, Some(backend), size);
        t.h.set_pixels_per_point(scale);
        t.idle();
        let ed = t.ed();
        let enemy = ent(ed, |e| e.kind == "enemy");
        ed.select(enemy, false);
        t.settle();
        t.shot(&format!("20-{name}"));
        t.click("+ Add");
        t.click("Models");
        t.until(30, |w| w.palette.thumbs_done() >= 3);
        t.shot(&format!("21-{name}-palette"));
        let view = t.w().view.rect;
        let (frames, pixels) = (t.w().view.frames, t.w().view.pixels());
        t.detail(&format!("{name}: the view has room"), view.width() >= 200.0 && view.height() >= 200.0, format!("{view:?}"));
        let px_ok = pixels.is_some_and(|(w, h, p)| w == (view.width() * scale).round() as u32 && h == (view.height() * scale).round() as u32 && p.chunks_exact(4).any(|c| c[..3] != p[..3]));
        t.detail(&format!("{name}: the view is drawn at the display's scale"), frames > 0 && px_ok, frames.to_string());
        t.click("Enemies");
        t.click("Add dennis");
        let n = kinds(t.ed(), "enemy").len();
        t.check(&format!("{name}: buttons work"), n == 2);
        t.finish();
    }
}

/// The side panels start at their default sizes, their contents fit, and a drag on an edge sets a size that stays.
#[test]
fn panels_resize() {
    use glovebox::app::{ADD_WIDTH, INSPECTOR_WIDTH, LEVEL_WIDTH};
    let near = |a: f32, b: f32| (a - b).abs() <= 1.0;
    let backend: Arc<dyn Backend> = Arc::new(FixtureBackend::new());
    let mut t = T::new("panels", Some(backend), [1280.0, 720.0]);
    t.h.step();
    let first = t.w().panels;
    t.detail("panels: the inspector starts at its default width", near(first.inspector.width(), INSPECTOR_WIDTH), format!("{first:?}"));
    t.detail("panels: the level panel starts at its default width", near(first.level.width(), LEVEL_WIDTH), format!("{first:?}"));
    t.idle();
    let (p, view) = (t.w().panels, t.w().view.rect);
    t.detail("panels: the widths hold once a level is open", near(p.inspector.width(), INSPECTOR_WIDTH) && near(p.level.width(), LEVEL_WIDTH), format!("{p:?}"));
    t.detail("panels: the 3D view has half of a 1280 wide window", near(view.width(), 1280.0 - LEVEL_WIDTH - INSPECTOR_WIDTH) && view.width() >= 620.0, format!("{view:?}"));
    t.detail("panels: the checks start at their share of the height", (180.0..=320.0).contains(&p.checks.height()), format!("{:?}", p.checks));

    // Every kind of object in the inspector: nothing is wider than the panel, and the panel keeps its width
    let mut wide = Vec::new();
    for i in 0..t.ed().entities.len() {
        t.ed().select(Some(i), false);
        t.settle();
        // A card measures its buttons in its first frame, so the frames after it count
        glovebox::style::reset_overflow(&t.h.ctx);
        t.settle();
        let over = glovebox::style::overflow(&t.h.ctx);
        if over.0 > 0.5 {
            wide.push((t.ed().entities[i].label.clone(), over));
        }
    }
    // Many objects, a puzzle, the level's settings, and the tabs a narrow window puts in the inspector's panel
    let fits = |t: &mut T, name: &str, wide: &mut Vec<(String, (f32, &'static str))>| {
        t.settle();
        glovebox::style::reset_overflow(&t.h.ctx);
        t.settle();
        let over = glovebox::style::overflow(&t.h.ctx);
        t.shot(&format!("24-fits-{name}"));
        if over.0 > 0.5 {
            wide.push((name.to_string(), over));
        }
    };
    let all: Vec<EntId> = (0..t.ed().entities.len()).collect();
    t.ed().set_selection(all);
    fits(&mut t, "many", &mut wide);
    let trigger = ent(t.ed(), |e| e.kind == "trigger").map(|i| t.ed().entities[i].node);
    t.ed().set_selection(Vec::new());
    t.ed().open_puzzle(trigger);
    fits(&mut t, "puzzle", &mut wide);
    t.ed().open_puzzle(None);
    fits(&mut t, "level-settings", &mut wide);
    t.click("+ Add");
    for tab in glovebox::palette::TABS {
        t.click(tab);
        fits(&mut t, &format!("add-{tab}"), &mut wide);
    }
    t.click("+ Add");
    t.click("Path");
    fits(&mut t, "path", &mut wide);
    t.key(Key::Escape);
    t.ed().set_selection(Vec::new());
    t.settle();
    t.detail("panels: every object's rows fit the inspector's default width", wide.is_empty(), format!("{wide:?}"));
    let p = t.w().panels;
    t.detail("panels: selections do not widen the inspector", near(p.inspector.width(), INSPECTOR_WIDTH), format!("{p:?}"));

    // Drags on the inspector's edge
    let y = p.inspector.center().y;
    let edge = |t: &mut T| egui::pos2(t.w().panels.inspector.left(), y);
    for (to, want) in [(1280.0 - 320.0, 320.0), (1280.0 - 500.0, 500.0), (1280.0 - 100.0, 300.0), (1280.0 - 900.0, 560.0), (1280.0 - 420.0, 420.0)] {
        let from = edge(&mut t);
        t.drag_window(from, egui::pos2(to, y + 5.0));
        let got = t.w().panels.inspector.width();
        t.detail(&format!("panels: a drag sets the inspector's width to {want}"), near(got, want), got.to_string());
    }
    let enemy = ent(t.ed(), |e| e.kind == "enemy");
    t.ed().select(enemy, false);
    t.settle();
    t.ed().set_selection(Vec::new());
    t.settle();
    let (p, view) = (t.w().panels, t.w().view.rect);
    t.detail("panels: the dragged width stays", near(p.inspector.width(), 420.0) && near(view.right(), p.inspector.left()), format!("{p:?}"));

    // The level panel's edge
    for (to, want) in [(220.0, 220.0), (350.0, 350.0), (50.0, 200.0), (300.0, 300.0)] {
        let from = egui::pos2(t.w().panels.level.right(), y);
        t.drag_window(from, egui::pos2(to, y + 5.0));
        let got = t.w().panels.level.width();
        t.detail(&format!("panels: a drag sets the level panel's width to {want}"), near(got, want), got.to_string());
    }
    t.settle();
    let (p, view) = (t.w().panels, t.w().view.rect);
    t.detail("panels: the view lies between the dragged panels", near(view.left(), 300.0) && near(view.right(), 1280.0 - 420.0), format!("{p:?} {view:?}"));

    // The checks panel's top edge
    let x = p.checks.center().x;
    let bottom = p.checks.bottom();
    for h in [140.0, 300.0, 200.0] {
        let from = egui::pos2(x, t.w().panels.checks.top());
        t.drag_window(from, egui::pos2(x + 5.0, bottom - h));
        let got = t.w().panels.checks.height();
        t.detail(&format!("panels: a drag sets the checks' height to {h}"), near(got, h), got.to_string());
    }
    t.shot("22-panels-dragged");
    t.finish();

    // A wide window shows "Add to level" in a panel of its own
    let backend: Arc<dyn Backend> = Arc::new(FixtureBackend::new());
    let mut t = T::new("panels-wide", Some(backend), [1920.0, 1080.0]);
    t.idle();
    t.click("+ Add");
    t.settle();
    let p = t.w().panels;
    t.detail("panels: the add panel starts at its default width", near(p.add.width(), ADD_WIDTH) && near(p.inspector.width(), INSPECTOR_WIDTH), format!("{p:?}"));
    let mut wide = Vec::new();
    for tab in ["Enemies", "Models", "Kit", "Presets", "Camera", "Collectables"] {
        let found = t.click(tab);
        t.settle();
        glovebox::style::reset_overflow(&t.h.ctx);
        t.settle();
        let over = glovebox::style::overflow(&t.h.ctx);
        t.shot(&format!("23-add-{tab}"));
        if !found || over.0 > 0.5 {
            wide.push((tab, found, over));
        }
    }
    t.detail("panels: every tab of the add panel fits its default width", wide.is_empty(), format!("{wide:?}"));
    let y = p.add.center().y;
    for want in [310.0, 480.0, 360.0] {
        let add = t.w().panels.add;
        t.drag_window(egui::pos2(add.left(), y), egui::pos2(add.right() - want, y + 5.0));
        let got = t.w().panels.add.width();
        t.detail(&format!("panels: a drag sets the add panel's width to {want}"), near(got, want), got.to_string());
    }
    let p = t.w().panels;
    t.detail("panels: the inspector keeps its width beside the add panel", near(p.inspector.width(), INSPECTOR_WIDTH) && near(p.inspector.left(), p.add.right()), format!("{p:?}"));
    t.finish();
}

/// The fonts of every theme have every symbol the app's buttons and texts show.
#[test]
fn the_fonts_have_the_symbols() {
    use glovebox::style;
    for th in glovebox::theme::THEMES {
        let ctx = egui::Context::default();
        style::apply(&ctx, th);
        let symbols = "↗●★♥✴↑↓×…▸▾←→⚠°·—’“”–!?+>";
        let families = [egui::FontFamily::Proportional, egui::FontFamily::Monospace, style::bold(), style::label_family(), style::logo_family()];
        let mut missing = String::new();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            for family in &families {
                let id = egui::FontId::new(13.0, family.clone());
                missing.extend(symbols.chars().filter(|c| !ui.fonts_mut(|f| f.has_glyph(&id, *c))));
            }
        });
        out.textures_delta.clear();
        assert!(missing.is_empty(), "theme {}: no glyph for {missing}", th.id);
    }
}

/// The Theme list of the Options menu: every theme through a click, at three window sizes. The toolbar keeps to
/// two rows at each size. The choice is kept in the
/// settings and read back by a new window and by the setup screen. An unknown name gives the default.
///
/// With `GLOVEBOX_SHOTS` it saves, for every theme, pictures of a level with a selection, the Options menu, the Add
/// palette, a dialog and the setup screen in `DIR/<theme>/`. With `GLOVEBOX_ROM` also a level of the game.
#[test]
fn themes() {
    use glovebox::theme::THEMES;
    let fixture = || -> Option<Arc<dyn Backend>> { Some(Arc::new(FixtureBackend::new())) };
    let mut results = Vec::new();
    let mut t = T::with_theme("themes", fixture(), [1500.0, 950.0], None);
    for (size, scale) in [([1500.0, 950.0], 1.0), ([1280.0, 720.0], 1.0), ([1920.0, 1080.0], 1.5)] {
        if scale != 1.0 || size[0] != 1500.0 {
            results.append(&mut t.results);
            t.finish();
            t = T::with_theme("themes", fixture(), size, None);
            t.h.set_pixels_per_point(scale);
        }
        t.idle();
        let first = t.theme() == "1998" && t.toolbar_is(THEMES[0]);
        t.check("without a setting the theme is 1998", first);
        let ed = t.ed();
        let enemy = ent(ed, |e| e.kind == "enemy");
        ed.select(enemy, false);
        t.settle();
        let window = t.h.ctx.content_rect();
        let at = format!("{}x{} at {scale}", size[0], size[1]);
        t.detail(&format!("{at}: the window has this size"), (window.width() * scale - size[0]).abs() < 2.0 && t.h.ctx.pixels_per_point() == scale, format!("{window:?}"));
        for th in THEMES.iter().rev() {
            let chosen = t.set_theme(th.name);
            let looks = t.theme() == th.id && t.toolbar_is(th);
            let saved = std::fs::read_to_string(t.home.join("settings.ini")).unwrap_or_default().contains(&format!("theme={}", th.id));
            t.detail(&format!("{at}: the Theme list sets {}", th.name), chosen && looks && saved, format!("{chosen} {looks} {saved} {}", t.theme()));
            // Everything of the toolbar is in the window, and the view keeps its room
            let places: Vec<_> = ["Options", "Build ROM", "Patcher ↗", "Focus", "Undo", "Box select"].iter().map(|l| t.find(l, true).first().map(|n| n.rect())).collect();
            let inside = places.iter().all(|r| r.is_some_and(|r| window.contains_rect(r)));
            // Two rows: the last button of each row is on the row of its first
            let row = |a: usize, b: usize| matches!((places[a], places[b]), (Some(a), Some(b)) if (a.center().y - b.center().y).abs() < 6.0);
            t.detail(&format!("{at}, {}: the toolbar has two rows", th.name), row(3, 4) && row(0, 5), format!("{places:?}"));
            let view = t.w().view.rect;
            t.detail(&format!("{at}, {}: the toolbar fits and the view has room", th.name), inside && view.width() >= 200.0 && view.height() >= 200.0, format!("{view:?} in {window:?}: {places:?}"));
            if scale == 1.0 && size[0] == 1500.0 {
                // A button works in this theme, and the pictures
                let n = kinds(t.ed(), "enemy").len();
                t.shot(&format!("{}/1-level", th.id));
                if t.shots.is_some() {
                    t.click("Options");
                    t.shot(&format!("{}/0-menu", th.id));
                    t.key(Key::Escape);
                }
                t.click("+ Add");
                t.click("Enemies");
                t.shot(&format!("{}/2-add", th.id));
                t.click("Add dennis");
                let added = kinds(t.ed(), "enemy").len() == n + 1;
                t.ed().undo();
                t.click("+ Add");
                t.click("Export patch");
                let open = t.has_containing("A BPS patch for other players");
                t.shot(&format!("{}/3-dialog", th.id));
                t.click("Cancel");
                let closed = t.w().dialog.is_none() && !t.w().palette_open;
                t.detail(&format!("{}: Add, a dialog and Cancel work", th.name), added && open && closed, format!("{added} {open} {closed}"));
                let enemy = ent(t.ed(), |e| e.kind == "enemy");
                t.ed().select(enemy, false);
                t.settle();
            } else {
                t.shot(&format!("{}/6-{}", th.id, if scale == 1.0 { "1280x720" } else { "scale-150" }));
            }
        }
    }
    // A new window on the same user folder has the theme that was chosen last
    t.set_theme("Paper");
    let home = t.home.clone();
    drop(t.h);
    let mut again = T::open(home, fixture(), [1280.0, 720.0]);
    again.idle();
    let kept = again.theme() == "paper" && again.toolbar_is(glovebox::theme::by_id("paper"));
    again.check("a new window has the theme of the settings", kept);
    again.finish();
    results.append(&mut t.results);

    // An unknown theme in the settings is 1998
    let mut odd = T::with_theme("themes-unknown", fixture(), [1280.0, 720.0], Some("glass"));
    odd.idle();
    let default = odd.theme() == "1998" && odd.toolbar_is(THEMES[0]);
    odd.check("an unknown theme in the settings gives 1998", default);
    odd.finish();

    // The setup screen has the theme of the settings
    for th in THEMES {
        let mut setup = T::with_theme("themes-setup", None, [1280.0, 720.0], Some(th.id));
        setup.settle();
        let shown = setup.has("Choose the ROM…") && setup.theme() == th.id;
        setup.shot(&format!("{}/4-setup", th.id));
        setup.detail(&format!("the setup screen is in {}", th.name), shown, setup.theme().to_string());
        setup.finish();
    }

    // A level of the game in every theme
    if let Some(rom) = std::env::var_os("GLOVEBOX_ROM").map(PathBuf::from) {
        let mut game = T::with_theme("themes-rom", None, [1500.0, 950.0], None);
        game.files.0.borrow_mut().open.push_back(rom);
        game.settle();
        game.h.get_by_label("Choose the ROM…").click();
        game.settle();
        game.idle();
        game.click("Level");
        game.choose("  (0a)");
        game.until(300, |w| !w.busy && w.ed.scene_id() == "0a" && w.jobs.running == 0);
        let ed = game.ed();
        let some = ent(ed, |e| e.kind == "enemy" && !e.is_handle);
        ed.select(some, false);
        game.settle();
        for th in THEMES.iter().rev() {
            let ok = game.set_theme(th.name) && game.theme() == th.id;
            game.shot(&format!("{}/5-game", th.id));
            game.check(&format!("a level of the game in {}", th.name), ok);
        }
        game.finish();
    } else {
        println!("skipped: GLOVEBOX_ROM is not set (a level of the game in every theme)");
    }
    let failed: Vec<&(String, bool, String)> = results.iter().filter(|r| !r.1).collect();
    assert!(failed.is_empty(), "failed: {:?}", failed.iter().map(|r| &r.0).collect::<Vec<_>>());
}

/// The Info texts of the Options menu: Shown, On hover and Off, each through a click. A note is in the window when
/// shown, a tooltip of the thing it explains on hover, and gone when off. What is about the level itself stays:
/// a check's main line and a wrong value's message. The choice is kept in the settings and read back by a new
/// window. Without a setting, and with an unknown value, the notes are on hover.
///
/// With `GLOVEBOX_SHOTS` it saves the level's settings and the Checks panel in each state, in `DIR/info/`.
#[test]
fn info_texts() {
    use egui::accesskit::Role;
    use glovebox::style::Info;
    const FILE: &str = "The name inside the level file";
    const HEAD: &str = "The game reads these once";
    const CARD: &str = "The game's level table";
    const HELP: &str = "Click something in the level";
    let fixture = || -> Option<Arc<dyn Backend>> { Some(Arc::new(FixtureBackend::new())) };
    // No setting for the info texts: the default
    let theme = std::env::var("GLOVEBOX_THEME").map(|id| format!("theme={id}\n")).unwrap_or_default();
    let mut t = T::with_settings("info", fixture(), [1500.0, 950.0], &format!("[General]\ntour=seen\n{theme}"));
    t.idle();
    // A wrong value: its message is in the Checks panel, and in the inspector when its trigger is chosen
    let ed = t.ed();
    let vol = ed.doc.iter_tags(ed.root(), &["PuzzleCondBallWithinRangeOfPoint"])[0];
    ed.set_attr(vol, "range", "0.0");
    ed.select(None, false);
    t.settle();
    let sub = t.w().checks.iter().find(|c| c.text.starts_with("Draw load")).and_then(|c| c.sub.clone()).unwrap_or_default();
    let sub: String = sub.lines().next().unwrap_or("").chars().take(40).collect();
    t.detail("the draw load check has a second line", !sub.is_empty(), sub.clone());
    let first = t.info() == Info::Hover && !t.reads(FILE) && t.has("File name");
    t.check("without a setting the notes are on hover", first);

    let mut tops = Vec::new();
    for (state, name) in [(Info::Hover, "On hover"), (Info::Off, "Off"), (Info::Shown, "Shown")] {
        let chosen = t.option(name);
        let saved = std::fs::read_to_string(t.home.join("settings.ini")).unwrap_or_default().contains(&format!("info={}", state.id()));
        t.detail(&format!("the Options menu sets the info texts to {name}"), chosen && t.info() == state && saved, format!("{chosen} {:?} {saved}", t.info()));
        t.ed().select(None, false);
        t.settle();
        t.shot(&format!("info/{}-settings", state.id()));
        // The notes of a title, a field, a card, a paragraph, and a check's second line
        let seen: Vec<bool> = [FILE, HEAD, CARD, HELP, sub.as_str()].iter().map(|n| t.reads(n)).collect();
        let want = state == Info::Shown;
        t.detail(&format!("{name}: the notes are {}", if want { "in the panels" } else { "not in the panels" }), seen.iter().all(|s| *s == want), format!("{seen:?}"));
        // What is about the level stays
        let kept = t.has_containing("Draw load") && t.has_containing("range = 0") && t.has("File name") && t.has("LEVEL SETTINGS") && t.has("No selection");
        t.check(&format!("{name}: check lines, titles and field names stay"), kept);
        tops.push((state, t.top_of("title"), t.top_of("Title, music and clock")));
        if state == Info::Hover {
            // The pointer on a field, on its name, on a title and on a check's line
            let on_field = t.hover("File name", Some(Role::TextInput)) && t.reads(FILE);
            let on_name = t.hover("File name", Some(Role::Label)) && t.reads(FILE);
            let on_title = t.hover("LEVEL SETTINGS", None) && t.reads(HEAD);
            let away = !t.reads(FILE);
            let on_card = t.hover("Title, music and clock", None) && t.reads(CARD);
            let on_help = t.hover("No selection", None) && t.reads(HELP);
            let on_check = t.hover("Draw load", Some(Role::Label)) && t.reads(&sub);
            t.shot("info/hover-tooltip");
            t.detail("On hover: a note is the tooltip of what it explains", on_field && on_name && on_title && away && on_card && on_help && on_check, format!("{on_field} {on_name} {on_title} {away} {on_card} {on_help} {on_check}"));
            // A click still goes to the field under the note's area
            let typed = t.fill("File name", "abcdefg") && {
                let ed = t.ed();
                ed.doc.attr(ed.root(), "name") == "abcdefg"
            };
            t.ed().undo();
            t.check("On hover: a field under a note still takes a click", typed);
        }
        // A wrong value's message in the inspector, with its trigger chosen
        let ed = t.ed();
        let mut puzzle = vol;
        while ed.doc.tag(puzzle) != "Puzzle" {
            puzzle = ed.doc.parent(puzzle).unwrap();
        }
        let trigger = ent(ed, |e| e.kind == "trigger" && e.node == puzzle);
        ed.select(trigger, false);
        t.settle();
        let lines = t.find("range = 0", false).len();
        let with_trigger = t.ed().selection.len() == 1;
        t.detail(&format!("{name}: a wrong value's message stays in the inspector"), with_trigger && lines >= 2, lines.to_string());
        t.shot(&format!("info/{}-object", state.id()));
        t.ed().select(None, false);
        t.settle();
    }
    // With the notes hidden the panel closes up: what is under a note moves up
    let y = |s: Info, k: usize| tops.iter().find(|x| x.0 == s).and_then(|x| if k == 0 { x.1 } else { x.2 }).unwrap_or(f32::NAN);
    let closed = y(Info::Off, 0) < y(Info::Shown, 0) - 20.0 && y(Info::Hover, 0) < y(Info::Shown, 0) - 20.0 && y(Info::Off, 1) < y(Info::Shown, 1) - 20.0;
    t.detail("with the notes hidden the panel is shorter", closed, format!("{tops:?}"));

    // The other panels: the palette's notes and a dialog's paragraph
    for (state, name) in [(Info::Off, "Off"), (Info::Hover, "On hover"), (Info::Shown, "Shown")] {
        t.option(name);
        t.click("+ Add");
        t.click("Presets");
        let palette = t.reads("Ready-made platforms") && t.reads("New objects appear");
        let rows = !t.find("Add ", false).is_empty();
        let tip = state != Info::Hover || (t.hover("Presets", None) && t.reads("Ready-made platforms"));
        t.shot(&format!("info/{}-palette", state.id()));
        t.click("+ Add");
        t.click("Export patch");
        let dialog = t.reads("A BPS patch for other players");
        let title = t.has("Export patch") && t.has("Export .bps");
        let tip2 = state != Info::Hover || {
            let bar = t.find("Export patch", true).into_iter().find(|n| n.accesskit_node().role() == Role::Label).map(|n| n.hover()).is_some();
            for _ in 0..8 {
                t.h.step();
            }
            bar && t.reads("A BPS patch for other players")
        };
        t.click("Cancel");
        let want = state == Info::Shown;
        t.detail(&format!("{name}: the palette's and a dialog's notes follow the setting"), palette == want && dialog == want && rows && title && tip && tip2, format!("{palette} {dialog} {rows} {title} {tip} {tip2}"));
    }

    // A new window on the same user folder has the state that was chosen last
    t.option("On hover");
    let home = t.home.clone();
    let mut results = std::mem::take(&mut t.results);
    drop(t.h);
    let mut again = T::open(home, fixture(), [1280.0, 720.0]);
    again.idle();
    let kept = again.info() == Info::Hover && !again.reads(FILE) && again.has("File name");
    again.check("a new window has the info texts of the settings", kept);
    results.append(&mut again.results);
    again.finish();

    // A setting that says Shown or Off is kept
    for (id, state) in [("shown", Info::Shown), ("off", Info::Off)] {
        let mut set = T::with_settings("info-set", fixture(), [1280.0, 720.0], &format!("[General]\ntour=seen\ninfo={id}\n"));
        set.idle();
        let kept = set.info() == state && set.reads(FILE) == (state == Info::Shown);
        set.check(&format!("the setting {id} is kept"), kept);
        results.append(&mut set.results);
        set.finish();
    }

    // An unknown value in the settings is the default, on hover
    let home = std::env::temp_dir().join(format!("glovebox-ui-test-info-unknown-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("settings.ini"), "[General]\ntour=seen\ninfo=loud\n").unwrap();
    let mut odd = T::open(home, fixture(), [1280.0, 720.0]);
    odd.idle();
    let hover = odd.info() == Info::Hover && !odd.reads(FILE) && odd.has("File name");
    odd.check("an unknown value of the info texts gives On hover", hover);
    results.append(&mut odd.results);
    odd.finish();
    let failed: Vec<&(String, bool, String)> = results.iter().filter(|r| !r.1).collect();
    assert!(failed.is_empty(), "failed: {:?}", failed.iter().map(|r| &r.0).collect::<Vec<_>>());
}

/// What the tour must leave as it was.
#[derive(Debug, PartialEq)]
struct Kept {
    selection: Vec<EntId>,
    palette_open: bool,
    front: Vec<glovebox::layout::PanelId>,
    layout: glovebox::layout::Layout,
    undo: usize,
    xml: String,
    edited: bool,
    drafts: usize,
    mode: Mode,
    camera: glovebox_editor::camera::Camera,
}

fn kept(t: &mut T) -> Kept {
    let drafts = std::fs::read_dir(t.home.join("drafts")).map(|d| d.count()).unwrap_or(0);
    let w = t.w();
    Kept {
        selection: w.ed.selection.clone(),
        palette_open: w.palette_open,
        front: w.front.clone(),
        layout: w.layout.clone(),
        undo: w.ed.undo_stack.len(),
        xml: w.ed.serialize(),
        edited: w.ed.is_edited() || !w.edited_levels.is_empty(),
        drafts,
        mode: w.ed.mode,
        camera: w.ed.camera.clone(),
    }
}

/// The rectangle around the widgets with these labels.
fn around(t: &T, labels: &[&str]) -> Option<egui::Rect> {
    let mut out: Option<egui::Rect> = None;
    for l in labels {
        let r = t.find(l, true).first()?.rect();
        out = Some(out.map_or(r, |o| o.union(r)));
    }
    out
}

fn close(a: egui::Rect, b: egui::Rect) -> bool {
    (a.min - b.min).abs().max_elem() <= 1.0 && (a.max - b.max).abs().max_elem() <= 1.0
}

/// Goes through a running tour from its first step to Done. At each step the bright part is the target's
/// rectangle, inside the window, and the bubble is inside the window and beside it. `tag` names the pictures.
fn walk_tour(t: &mut T, tag: &str) {
    use glovebox::tour::{Show, Target, GUIDE, STEPS};
    let n = STEPS.len();
    let mut i = 0;
    let mut went_back = false;
    while i < n {
        t.settle();
        let s = &STEPS[i];
        let window = t.h.ctx.content_rect();
        let Some((step, spot, bubble)) = t.w().tour.run.as_ref().map(|r| (r.step, r.spot, r.bubble)) else {
            t.check(&format!("{tag}: the tour runs at step {}", i + 1), false);
            return;
        };
        let target = t.w().tour.target(s.target);
        // The target is the real widget or panel
        let w = t.w();
        let (panels, view) = (w.panels, w.view.rect);
        let widget = match s.target {
            Target::LevelList => around(t, &["Level"]),
            Target::Tools => around(t, &["Move", "Rotate", "Scale"]),
            Target::Add => around(t, &["+ Add"]),
            Target::Build => around(t, &["Test level", "Build ROM"]),
            Target::Patch => around(t, &["Export patch"]),
            Target::Undo => around(t, &["Reset level", "Undo", "Redo"]),
            Target::Options => around(t, &["Options"]),
            Target::View => Some(view),
            Target::LevelPanel => Some(panels.level),
            Target::Checks => Some(panels.checks),
            // The inspector is its column's part over the Checks panel, or down to the column's end without them
            Target::Inspector => {
                let under = panels.checks.is_positive() && panels.inspector.contains_rect(panels.checks);
                let bottom = if under { panels.checks.min.y } else { panels.inspector.max.y };
                target.filter(|r| panels.inspector.contains_rect(*r) && (r.max.y - bottom).abs() <= 1.0 && r.height() > 100.0)
            }
            Target::Toolbar | Target::RightPanel => None,
        };
        let on_target = match (spot, target, widget) {
            (Some(s), Some(t), Some(w)) => close(s, t.intersect(window)) && close(t, w),
            _ => false,
        };
        let size_ok = spot.is_some_and(|s| s.width() >= 8.0 && s.height() >= 8.0 && window.contains_rect(s));
        let bubble_ok = bubble.is_positive() && window.contains_rect(bubble) && spot.is_some_and(|s| !bubble.intersects(s));
        let texts = step == i && t.reads(s.title) && t.has(&format!("{} of {n}", i + 1)) && t.has(if i + 1 == n { "Done" } else { "Next" }) && t.has("Skip") == (i + 1 < n);
        let w = t.w();
        let (selected, palette) = (w.ed.selection.len(), w.palette_open);
        let shows = match s.show {
            Show::Plain => selected == 0 && !palette,
            Show::Selection => selected == 1 && !palette,
            Show::AddPanel => palette && t.has("Collectables"),
        };
        t.shot(&format!("{tag}-{:02}", i + 1));
        t.detail(
            &format!("{tag}: step {} ({}) lights its target, with the bubble beside it", i + 1, s.title),
            on_target && size_ok && bubble_ok && texts && shows,
            format!("{on_target} {size_ok} {bubble_ok} {texts} {shows} spot {spot:?} target {target:?} widget {widget:?} bubble {bubble:?} window {window:?}"),
        );
        if i == 1 {
            // The editor's keys and a click in the view do nothing
            let before = kept(t);
            let ed = t.ed();
            let some = ent(ed, |e| e.kind == "enemy" && !e.is_handle);
            let at = some.map(|e| px(ed, ed.obj_pos(e))).unwrap_or((100.0, 100.0));
            let had = ed.entities.len();
            for key in [Key::R, Key::A, Key::B, Key::C, Key::P, Key::Delete] {
                t.key(key);
            }
            t.key_with(Modifiers::COMMAND, Key::A);
            t.view_click(at, Modifiers::default());
            let w = t.w();
            let off = !w.ed.box_mode && !w.ed.path.on && !w.view.fly.on && w.ed.entities.len() == had && w.tour.run.as_ref().is_some_and(|r| r.step == 1);
            let same = kept(t) == before;
            t.check(&format!("{tag}: the editor's keys and a click in the view do nothing during the tour"), off && same);
        }
        if i == 3 && !went_back {
            // Back, by the button and by the key, then forward again by the keys
            went_back = true;
            let a = t.click("Back") && t.w().tour.run.as_ref().is_some_and(|r| r.step == 2);
            t.key(Key::ArrowLeft);
            let b = t.w().tour.run.as_ref().is_some_and(|r| r.step == 1);
            t.key(Key::ArrowRight);
            t.key(Key::Enter);
            let c = t.w().tour.run.as_ref().is_some_and(|r| r.step == 3);
            t.detail(&format!("{tag}: Back, the arrow keys and Enter move between the steps"), a && b && c, format!("{a} {b} {c}"));
            continue;
        }
        if i + 1 == n {
            // The link to the guide opens the browser
            let mut opened = false;
            if let Some(link) = t.find("Open the guide", false).first() {
                link.click();
            }
            for _ in 0..4 {
                t.h.step();
                opened |= t.h.output().platform_output.commands.iter().any(|c| matches!(c, egui::OutputCommand::OpenUrl(u) if u.url == GUIDE));
            }
            t.check(&format!("{tag}: the last step's link opens the guide"), opened);
            let done = t.click("Done") && t.w().tour.run.is_none() && !t.has("Done");
            t.check(&format!("{tag}: Done ends the tour"), done);
        } else {
            t.click("Next");
        }
        i += 1;
    }
}

/// The welcome on the first start, and the tour of the window.
///
/// On a user folder without settings the welcome shows once a level is open, and not before. `No thanks` closes it
/// and writes the setting: a new window on the folder has no welcome. `Show me around` starts the tour, and so does
/// `Take the tour` of the Options menu. The tour is walked at three window sizes (`walk_tour`). `Skip` and Esc end
/// it. After it the selection, the panels, the camera, the undo steps and the level are as before, and no draft is
/// written.
///
/// With `GLOVEBOX_SHOTS` it saves every step in `DIR/THEME/`, and with `GLOVEBOX_ROM` also on a level of the game.
#[test]
fn welcome_and_tour() {
    use glovebox::tour::STEPS;
    let fixture = || -> Option<Arc<dyn Backend>> { Some(Arc::new(FixtureBackend::new())) };
    let theme = std::env::var("GLOVEBOX_THEME").ok().filter(|id| !id.is_empty());
    let first_start = theme.as_ref().map(|id| format!("[General]\ntheme={id}\n")).unwrap_or_default();
    let dir = theme.clone().unwrap_or_else(|| "1998".to_string());
    let mut results = Vec::new();
    // The last window has the inspector on the left, as a tab beside Level, and no checks
    let moved = "panels=\"level:left:on,inspector:left:on,checks:right:off,add:right,path:right,left:tabbed,right:tabbed\"\n";
    for (k, (name, size, scale, layout)) in [("1500x950", [1500.0, 950.0], 1.0, ""), ("1280x720", [1280.0, 720.0], 1.0, ""), ("scale-150", [1920.0, 1080.0], 1.5, ""), ("moved", [1280.0, 720.0], 1.0, moved)].into_iter().enumerate() {
        let tag = format!("{dir}/{name}");
        let first_start = if layout.is_empty() { first_start.clone() } else { format!("[General]\n{}{layout}", first_start.trim_start_matches("[General]\n")) };
        let mut t = T::with_settings(&format!("tour-{name}"), fixture(), size, &first_start);
        if !layout.is_empty() {
            t.idle();
            let w = t.w();
            let ok = w.layout.side(glovebox::layout::PanelId::Inspector) == glovebox::layout::Side::Left && !w.layout.shown(glovebox::layout::PanelId::Checks);
            t.check("moved: the window starts with the inspector on the left and no checks", ok);
        }
        t.h.set_pixels_per_point(scale);
        // The welcome waits for the level
        let mut early = false;
        let t0 = Instant::now();
        loop {
            t.h.step();
            let w = t.w();
            early |= w.tour.welcome && (!w.ed.has_level || w.busy);
            if w.tour.welcome || t0.elapsed() > Duration::from_secs(60) {
                break;
            }
            std::thread::sleep(Duration::from_millis(3));
        }
        t.idle();
        let shown = t.w().tour.welcome && t.has("Show me around") && t.has("No thanks") && t.reads("first time");
        t.shot(&format!("{tag}-00-welcome"));
        t.detail(&format!("{name}: the welcome shows once the level is open, and not before"), shown && !early, format!("{shown} {early}"));
        let unset = !std::fs::read_to_string(t.home.join("settings.ini")).unwrap_or_default().contains("tour=");
        // The editor's keys are off under the welcome
        let before = kept(&mut t);
        t.key(Key::R);
        t.key(Key::A);
        let quiet = kept(&mut t) == before && t.w().tour.welcome;
        t.check(&format!("{name}: the editor's keys do nothing under the welcome"), unset && quiet);
        let saved = |t: &T| std::fs::read_to_string(t.home.join("settings.ini")).unwrap_or_default().contains("tour=seen");
        if k == 0 {
            // No thanks, then a new window on the same folder
            let closed = t.click("No thanks") && !t.w().tour.welcome && t.w().tour.run.is_none() && !t.has("Show me around");
            t.detail("No thanks closes the welcome and writes the setting", closed && saved(&t), format!("{closed}"));
            let home = t.home.clone();
            results.append(&mut t.results);
            drop(t.h);
            t = T::open(home, fixture(), size);
            t.idle();
            let none = !t.w().tour.welcome && !t.has("Show me around") && t.w().tour.run.is_none();
            t.check("a new window on the same folder has no welcome", none);
            // Something selected and the Add panel open, then the tour from the Options menu
            let ed = t.ed();
            let enemy = ent(ed, |e| e.kind == "enemy");
            ed.select(enemy, false);
            t.click("+ Add");
            t.click("Rotate");
            let before = kept(&mut t);
            let started = t.option("Take the tour") && t.w().tour.run.as_ref().is_some_and(|r| r.step == 0);
            t.check("Take the tour of the Options menu starts the tour", started && before.palette_open && before.selection.len() == 1);
            walk_tour(&mut t, &tag);
            t.settle();
            let after = kept(&mut t);
            t.detail("after the tour the editor is as it was before", after == before && after.drafts == 0 && !after.edited, format!("{:?} {:?} {:?} {} {:?}", after.selection, after.palette_open, after.front, after.undo, after.mode));
        } else {
            let before = kept(&mut t);
            let started = t.click("Show me around") && !t.w().tour.welcome && t.w().tour.run.as_ref().is_some_and(|r| r.step == 0);
            t.detail(&format!("{name}: Show me around starts the tour and writes the setting"), started && saved(&t), format!("{started}"));
            walk_tour(&mut t, &tag);
            t.settle();
            let after = kept(&mut t);
            t.check(&format!("{name}: after the tour the editor is as it was before"), after == before && after.drafts == 0 && !after.edited);
        }
        // Again from the Options menu: Skip ends it, and so does Esc, at any step
        let before = kept(&mut t);
        let again = t.option("Take the tour") && t.w().tour.run.is_some();
        t.click("Next");
        t.click("Next");
        let at_three = t.w().tour.run.as_ref().is_some_and(|r| r.step == 2) && t.ed().selection.len() == 1;
        let skipped = t.click("Skip") && t.w().tour.run.is_none() && kept(&mut t) == before;
        t.detail(&format!("{name}: Skip ends the tour"), again && at_three && skipped, format!("{again} {at_three} {skipped}"));
        t.option("Take the tour");
        for _ in 0..4 {
            t.key(Key::ArrowRight);
        }
        let open = t.w().tour.run.as_ref().is_some_and(|r| r.step == 4) && t.w().palette_open;
        t.key(Key::Escape);
        let ended = t.w().tour.run.is_none() && !t.has(&format!("5 of {}", STEPS.len())) && kept(&mut t) == before;
        t.detail(&format!("{name}: Esc ends the tour"), open && ended, format!("{open} {ended}"));
        // The editor's keys work again
        t.key(Key::R);
        let back = t.ed().mode == Mode::Scale;
        t.check(&format!("{name}: after the tour the editor's keys work"), back);
        if !layout.is_empty() {
            // The tour showed the checks for their step, and they are hidden again
            let w = t.w();
            let hidden = !w.panels.checks.is_positive() && !w.layout.shown(glovebox::layout::PanelId::Checks);
            let text = t.settings_text();
            t.detail("moved: after the tour the checks are hidden as before, and the settings have the player's layout", hidden && text.contains("checks:right:off"), text);
        }
        results.append(&mut t.results);
        t.finish();
    }

    // A level of the game, for the pictures
    match (std::env::var_os("GLOVEBOX_ROM").map(PathBuf::from), std::env::var_os("GLOVEBOX_SHOTS")) {
        (Some(rom), Some(_)) => {
            let mut t = T::with_settings("tour-rom", None, [1280.0, 800.0], &first_start);
            t.files.0.borrow_mut().open.push_back(rom);
            t.settle();
            let setup = !t.has("Show me around");
            t.h.get_by_label("Choose the ROM…").click();
            t.settle();
            t.idle();
            let shown = t.w().tour.welcome && t.has("Show me around");
            t.detail("the welcome is not over the setup screen, and shows on the first level of the game", setup && shown, format!("{setup} {shown}"));
            t.shot(&format!("{dir}/rom-00-welcome"));
            let before = kept(&mut t);
            t.click("Show me around");
            walk_tour(&mut t, &format!("{dir}/rom"));
            t.settle();
            let same = kept(&mut t) == before;
            t.check("a level of the game is as it was after the tour", same);
            results.append(&mut t.results);
            t.finish();
        }
        _ => println!("skipped: GLOVEBOX_ROM and GLOVEBOX_SHOTS are not both set (the tour on a level of the game)"),
    }
    let failed: Vec<&(String, bool, String)> = results.iter().filter(|r| !r.1).collect();
    assert!(failed.is_empty(), "failed: {:?}", failed.iter().map(|r| (&r.0, &r.2)).collect::<Vec<_>>());
}

// -----------------------------------------------------------------------------------------------------------------
// The grid view of the Add panel, and the layout of the panels

impl T {
    /// The tile of the Add panel's grid view with this name: a square button.
    fn tile(&self, name: &str) -> Option<egui::Rect> {
        self.tile_node(name).map(|n| n.rect())
    }

    fn tile_node<'a>(&'a self, name: &'a str) -> Option<egui_kittest::Node<'a>> {
        self.find(name, true).into_iter().find(|n| {
            let r = n.rect();
            n.accesskit_node().role() == egui::accesskit::Role::Button && r.width() >= 60.0 && (r.width() - r.height()).abs() < 1.0
        })
    }

    /// A real click on a tile, then the jobs it starts.
    fn click_tile(&mut self, name: &str) -> bool {
        let found = match self.tile_node(name) {
            Some(n) => {
                n.click();
                true
            }
            None => false,
        };
        self.settle();
        self.idle();
        found
    }

    /// Turns the wheel over the Add panel until a tile is in view.
    fn scroll_to_tile(&mut self, name: &str) -> bool {
        let slot = self.w().panels.slot(glovebox::layout::PanelId::Add);
        let at = egui::pos2(slot.center().x, slot.center().y + 60.0);
        for _ in 0..40 {
            if self.tile(name).is_some_and(|r| r.bottom() <= slot.bottom() - 8.0) {
                return true;
            }
            self.wheel(at, -150.0);
        }
        false
    }

    /// The tiles in view whose names are in `names`.
    fn tiles(&self, names: &[String]) -> Vec<egui::Rect> {
        names.iter().filter_map(|n| self.tile(n)).collect()
    }

    /// How many elements with this tag the level's XML has.
    fn count_tag(&mut self, tag: &str) -> usize {
        let xml = self.ed().serialize();
        xml.match_indices(&format!("<{tag}")).filter(|(i, m)| matches!(xml.as_bytes().get(i + m.len()), Some(b' ' | b'>' | b'/'))).count()
    }

    fn click_at(&mut self, pos: egui::Pos2) {
        self.h.event(egui::Event::PointerMoved(pos));
        self.h.step();
        for pressed in [true, false] {
            self.h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::default() });
            self.h.step();
        }
        self.settle();
    }

    /// Turns the wheel over a place in the window.
    fn wheel(&mut self, at: egui::Pos2, dy: f32) {
        self.h.event(egui::Event::PointerMoved(at));
        self.h.step();
        self.h.event(egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: egui::vec2(0.0, dy), phase: egui::TouchPhase::Move, source: egui::MouseWheelSource::Wheel, modifiers: Modifiers::default() });
        for _ in 0..6 {
            self.h.step();
        }
    }

    /// Clicks entries of Options > Panels, then closes the menu with a click on the status line.
    fn panels_menu(&mut self, entries: &[&str]) -> bool {
        let mut ok = self.click("Options") && self.click("Panels");
        for e in entries {
            ok &= self.click(e);
        }
        let window = self.h.ctx.content_rect();
        self.click_at(egui::pos2(window.center().x, window.bottom() - 5.0));
        ok && !self.has("Reset layout")
    }

    /// Clicks an entry of a panel's own menu.
    fn panel_menu(&mut self, panel: &str, entry: &str) -> bool {
        let ok = self.click(&format!("{panel} panel menu")) && self.click(entry);
        self.settle();
        ok
    }

    fn settings_text(&self) -> String {
        std::fs::read_to_string(self.home.join("settings.ini")).unwrap_or_default()
    }

    /// Waits for the pictures of the Add panel.
    fn pictures(&mut self) {
        for _ in 0..3 {
            self.settle();
            self.until(60, |w| w.side_jobs.running == 0 && w.palette.thumbs_wanted.is_empty());
        }
    }
}

/// The grid view of the Add panel's Enemies, Models and Kit tabs.
///
/// `Grid` shows tiles and no rows, on each of the three tabs. A click on a tile adds the thing: an enemy, a model as
/// the radio buttons say (the level's XML gets a LandActor, a BackgroundActor or a Platform), a kit item with its
/// settings as they are. A converted model is refused as a platform. The view and the radio buttons stay through
/// closing the panel and through a new window. The search narrows the grid, a wider panel has more columns, and a
/// tile's tooltip has its name.
///
/// With `GLOVEBOX_ROM` the Models grid is scrolled through the whole game, and the number of pictures kept stays
/// under `THUMBS_MAX`. With `GLOVEBOX_SHOTS` it saves the three tabs in four themes in `DIR/grid/`.
#[test]
fn add_grid() {
    use glovebox::palette::THUMBS_MAX;
    use glovebox_editor::entities::WHY_CUSTOM_PLATFORM;
    let fixture = || -> Option<Arc<dyn Backend>> { Some(Arc::new(FixtureBackend::new())) };
    let mut t = T::new("grid", fixture(), [1280.0, 720.0]);
    t.idle();
    t.click("+ Add");

    // Enemies: rows in List, tiles in Grid, and no radio buttons
    t.click("Enemies");
    let rows = t.has("Add dennis") && t.tile("dennis").is_none() && t.has("List") && t.has("Grid") && !t.w().palette.grid;
    let clicked = t.click("Grid");
    let virtual_ = t.tile("dennis").is_none();
    let found = t.scroll_to_tile("dennis");
    t.detail("grid: a tile far down is not laid out until the grid scrolls to it", virtual_ && found, format!("{virtual_} {found}"));
    t.pictures();
    let tiles = t.tile("dennis").is_some() && !t.has("Add dennis") && !t.reads("script from") && !t.has_containing("Place as");
    let on = t.w().palette.grid;
    t.detail("grid: Enemies shows tiles and no rows", rows && clicked && tiles && on, format!("{rows} {clicked} {tiles}"));
    let enemy_tile = t.tile("dennis").unwrap_or(egui::Rect::NOTHING);
    let n = kinds(t.ed(), "enemy").len();
    let ok = t.click_tile("dennis");
    let n2 = kinds(t.ed(), "enemy").len();
    t.detail("grid: a click on an enemy's tile adds one enemy", ok && n2 == n + 1, format!("{n} {n2}"));
    // An enemy without a model: its tile is off, and a click says why
    let types: Vec<String> = t.ed().backend.decomp().get("ksyEnums").and_then(|e| e.get("enemy::enemy_type")).and_then(|e| e.as_object()).map(|o| o.values().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default();
    let other = types.iter().find(|n| *n != "dennis" && t.tile(n).is_some()).cloned().unwrap_or_default();
    let off = t.tile_node(&other).is_some_and(|n| n.accesskit_node().is_disabled());
    t.click_tile(&other);
    let (n3, status) = (kinds(t.ed(), "enemy").len(), t.status());
    t.detail("grid: an enemy without a model is not added", off && n3 == n2 && status.contains("no model"), format!("{other} {off} {n3} {status}"));
    t.shot("grid/fixture-enemies");

    // Models: the same view, with the radio buttons
    t.click("Models");
    t.pictures();
    let names: Vec<String> = t.ed().catalog.values().map(|c| c.name.clone()).collect();
    let shown = t.tiles(&names);
    let radios = t.has("Place as Scenery") && t.has("Place as Background") && t.has("Place as Platform");
    let tiles = shown.len() == names.len() && !t.has("Scenery sign") && !t.reads("imported at build");
    t.detail("grid: Models shows a tile for each model, no rows, and the radio buttons", tiles && radios, format!("{} of {} {radios}", shown.len(), names.len()));
    let same = shown.first().is_some_and(|r| (r.width() - enemy_tile.width()).abs() < 0.5);
    t.detail("grid: the tiles of Enemies and Models have one size", same, format!("{shown:?} {enemy_tile:?}"));
    for (radio, tag) in [("", "LandActor"), ("Place as Background", "BackgroundActor"), ("Place as Platform", "Platform"), ("Place as Scenery", "LandActor")] {
        let picked = radio.is_empty() || t.click(radio);
        let (n, undo) = (t.count_tag(tag), t.ed().undo_stack.len());
        let ok = t.click_tile("block");
        let (n2, undo2) = (t.count_tag(tag), t.ed().undo_stack.len());
        t.detail(&format!("grid: a click on a model's tile adds one {tag} ({radio})"), picked && ok && n2 == n + 1 && undo2 == undo + 1, format!("{picked} {ok} {n} {n2}"));
    }
    t.click("Place as Platform");
    let saved = t.settings_text();
    t.detail("grid: the view and the radio button are in the settings", saved.contains("add_view=grid") && saved.contains("add_models_as=platform"), saved);

    // The tooltip has the name and where the model is from
    if let Some(n) = t.tile_node("sign") {
        n.hover();
    }
    for _ in 0..10 {
        t.h.step();
    }
    let tip = t.reads("sign  0x") && t.reads("OTHER_BANK");
    t.check("grid: a tile's tooltip has the model's name and its bank", tip);
    t.shot("grid/fixture-models");

    // The keyboard: Enter on the tile with the focus
    let n = t.count_tag("Platform");
    if let Some(node) = t.tile_node("ball") {
        node.focus();
    }
    t.settle();
    t.key(Key::Enter);
    let n2 = t.count_tag("Platform");
    t.detail("grid: Enter on a tile with the focus places it", n2 == n + 1, format!("{n} {n2}"));

    // A wider panel has more columns
    let cols = t.w().palette.grid_cols;
    let first_row = |t: &T| {
        let all = t.tiles(&names);
        let top = all.iter().map(|r| r.top()).fold(f32::INFINITY, f32::min);
        all.iter().filter(|r| (r.top() - top).abs() < 1.0).count()
    };
    let in_row = first_row(&t);
    let p = t.w().panels.inspector;
    t.drag_window(egui::pos2(p.left(), p.center().y), egui::pos2(1280.0 - 560.0, p.center().y + 5.0));
    let (cols2, in_row2) = (t.w().palette.grid_cols, first_row(&t));
    t.detail("grid: a wider panel has more columns", cols >= 3 && in_row == cols.min(names.len()) && cols2 > cols && in_row2 == cols2.min(names.len()), format!("{cols} {in_row} {cols2} {in_row2}"));
    let p = t.w().panels.inspector;
    t.drag_window(egui::pos2(p.left(), p.center().y), egui::pos2(1280.0 - 380.0, p.center().y + 5.0));
    let back = t.w().palette.grid_cols == cols;
    t.check("grid: a narrower panel has fewer columns again", back);

    // The search narrows the grid
    t.fill("Search models", "sign");
    let left = t.tiles(&names).len();
    t.fill("Search models", "no such model");
    let none = t.tiles(&names).is_empty() && t.reads("No model matches.");
    t.fill("Search models", "");
    let all = t.tiles(&names).len();
    t.detail("grid: the search narrows the grid, and says when nothing matches", left == 1 && none && all == names.len(), format!("{left} {none} {all}"));

    // Closing the panel and opening it again keeps the view and the radio button
    t.click("+ Add");
    let closed = !t.w().palette_open && t.tile("sign").is_none();
    t.click("+ Add");
    let kept = t.w().palette.grid && t.w().palette.places().0 == "Platform" && t.tile("sign").is_some();
    t.detail("grid: the view and the radio button stay when the panel closes and opens", closed && kept, format!("{closed} {kept}"));

    // The kit: its own radio buttons, and the settings of an item
    t.click("Kit");
    t.until(120, |w| w.side_jobs.running == 0 && w.palette.kit_picture("grave").is_some());
    let kit: Vec<(String, String)> = t.ed().backend.kit_items().ok().and_then(|v| v.as_array().cloned()).unwrap_or_default().iter().map(|i| (i["id"].as_str().unwrap_or("").to_string(), i["name"].as_str().unwrap_or("").to_string())).collect();
    let grave = kit.iter().find(|k| k.0 == "grave").map(|k| k.1.clone()).unwrap_or_default();
    let kit_names: Vec<String> = kit.iter().map(|k| k.1.clone()).collect();
    let shown = t.tiles(&kit_names);
    let radios = t.has("Place as Background") && t.has("Place as Scenery") && !t.has("Place as Platform");
    let same = shown.first().is_some_and(|r| (r.width() - enemy_tile.width()).abs() < 0.5);
    let no_rows = !t.has(&format!("Background {grave}")) && !t.find("Lean", false).iter().any(|n| n.accesskit_node().role() == egui::accesskit::Role::TextInput);
    t.detail("grid: Kit shows tiles of the same size, no rows, and its radio buttons", shown.len() >= 4 && radios && same && no_rows, format!("{} {radios} {same} {no_rows}", shown.len()));
    let customs = |t: &mut T| t.ed().catalog.values().filter(|c| c.custom.is_some()).count();
    let (n, c) = (t.count_tag("BackgroundActor"), customs(&mut t));
    let ok = t.click_tile(&grave);
    let (n2, c2) = (t.count_tag("BackgroundActor"), customs(&mut t));
    t.detail("grid: a click on a kit tile makes the item and places it as Background", ok && n2 > n && c2 > c, format!("{ok} {n} {n2} {c} {c2}"));
    // Its settings: the corner opens them, a tile click uses them
    let opened = t.click(&format!("Settings {grave}")) && t.find("Lean", false).iter().any(|n| n.accesskit_node().role() == egui::accesskit::Role::TextInput);
    t.fill("Lean", "9");
    let drawn = t.until(30, |w| w.side_jobs.running == 0 && w.palette.kit_picture("grave").is_some_and(|p| p.0.contains("\"9\"") && p.1));
    t.shot("grid/fixture-kit-settings");
    t.click("Place as Scenery");
    let (n, c) = (t.count_tag("LandActor"), customs(&mut t));
    t.click_tile(&grave);
    let (n2, c2) = (t.count_tag("LandActor"), customs(&mut t));
    t.detail("grid: a kit tile's corner opens its settings, and a click uses them, as Scenery", opened && drawn && n2 > n && c2 > c, format!("{opened} {drawn} {n} {n2} {c} {c2}"));
    let closed = t.click("Close settings") && t.w().palette.kit_settings.is_none() && t.find("Lean", false).is_empty();
    if let Some(node) = t.tile_node(&grave) {
        node.click_secondary();
    }
    t.settle();
    let again = t.w().palette.kit_settings.as_deref() == Some("grave");
    t.detail("grid: Close closes the settings, and a right-click on the tile opens them", closed && again, format!("{closed} {again}"));
    t.click("Close settings");
    t.shot("grid/fixture-kit");

    // A converted model can't be a platform: its tile is off under Platform, and a click says why
    t.click("Models");
    let custom = t.ed().catalog.iter().find(|c| c.1.custom.is_some()).map(|c| (c.0.clone(), c.1.name.clone())).unwrap_or_default();
    t.fill("Search models", &custom.0);
    t.pictures();
    let is_platform = t.w().palette.places().0 == "Platform";
    let off = t.tile_node(&custom.1).is_some_and(|n| n.accesskit_node().is_disabled());
    let n = t.count_tag("Platform");
    t.click_tile(&custom.1);
    let (n2, status) = (t.count_tag("Platform"), t.status());
    for _ in 0..10 {
        t.h.step();
    }
    let why = t.reads("A platform must use a model from the game");
    t.shot("grid/fixture-models-platform");
    t.detail("grid: a converted model is refused as a platform, with the list's reason", is_platform && off && n2 == n && status == WHY_CUSTOM_PLATFORM && why, format!("{custom:?} {is_platform} {off} {n} {n2} {why} {status}"));
    t.click("Place as Scenery");
    let on = t.tile_node(&custom.1).is_some_and(|n| !n.accesskit_node().is_disabled());
    let n = t.count_tag("LandActor");
    t.click_tile(&custom.1);
    let n2 = t.count_tag("LandActor");
    t.detail("grid: the converted model is placed as Scenery", on && n2 == n + 1, format!("{on} {n} {n2}"));
    t.fill("Search models", "");
    t.click("Place as Background");

    // A new window on the same folder
    let home = t.home.clone();
    let mut results = std::mem::take(&mut t.results);
    drop(t.h);
    let mut t = T::open(home, fixture(), [1280.0, 720.0]);
    t.idle();
    t.click("+ Add");
    t.click("Models");
    t.pictures();
    let places = t.w().palette.places();
    let kept = t.w().palette.grid && t.tile("sign").is_some() && !t.has("Scenery sign") && places == ("BackgroundActor", "LandActor");
    t.detail("grid: a new window has the view and the radio buttons of the last one", kept, format!("{places:?}"));
    // Back to the list
    let listed = t.click("List") && t.has("Scenery sign") && t.tile("sign").is_none() && t.settings_text().contains("add_view=list");
    t.check("grid: List shows the rows again", listed);
    t.fill("Search models", "no such model");
    let none = t.reads("No model matches.");
    t.check("grid: the list says when nothing matches", none);
    results.append(&mut t.results);
    t.results = results;
    t.finish();

    // Pictures: the three tabs in four themes
    if std::env::var_os("GLOVEBOX_SHOTS").is_some() {
        for theme in ["1998", "sage", "dark", "crt"] {
            let mut t = T::with_theme(&format!("grid-{theme}"), fixture(), [1280.0, 720.0], Some(theme));
            t.idle();
            t.click("+ Add");
            t.click("Models");
            t.click("Grid");
            for (tab, over) in [("Enemies", "dennis"), ("Models", "sign"), ("Kit", "")] {
                t.click(tab);
                t.pictures();
                t.until(60, |w| w.side_jobs.running == 0);
                if let Some(n) = t.tile_node(over) {
                    n.hover();
                }
                t.settle();
                t.shot(&format!("grid/{theme}-{}", tab.to_lowercase()));
            }
            t.finish();
        }
    }

    // The whole game: the grid scrolls through every model and keeps a bounded number of pictures
    let Some(rom) = std::env::var_os("GLOVEBOX_ROM").map(PathBuf::from) else {
        println!("skipped: GLOVEBOX_ROM is not set (the Models grid on the whole game)");
        return;
    };
    let mut t = T::with_theme("grid-rom", None, [1280.0, 800.0], Some("sage"));
    t.files.0.borrow_mut().open.push_back(rom);
    t.settle();
    t.h.get_by_label("Choose the ROM…").click();
    t.settle();
    t.idle();
    t.click("+ Add");
    t.click("Models");
    t.click("Grid");
    t.pictures();
    let total = t.ed().catalog.len();
    let cols = t.w().palette.grid_cols.max(1);
    let slot = t.w().panels.slot(glovebox::layout::PanelId::Add);
    let at = egui::pos2(slot.center().x, slot.center().y + 60.0);
    t.shot("grid/rom-sage-models");
    let visible = |t: &T| -> Vec<(String, i32)> {
        let mut v: Vec<(String, i32)> = t.h.query_all_by_role(egui::accesskit::Role::Button).filter(|n| slot.contains_rect(n.rect()) && n.rect().width() >= 60.0 && (n.rect().width() - n.rect().height()).abs() < 1.0).map(|n| (n.accesskit_node().label().unwrap_or_default(), n.rect().top() as i32)).collect();
        v.sort();
        v
    };
    let in_view = visible(&t).len();
    let (mut most, mut most_tiles, mut turns) = (0, in_view, 0);
    let mut last = visible(&t);
    // Each turn of the wheel is about five rows
    for _ in 0..(total / cols + 20) {
        t.wheel(at, -450.0);
        most = most.max(t.w().palette.thumbs.len());
        let now = visible(&t);
        most_tiles = most_tiles.max(now.len());
        if now == last {
            break;
        }
        last = now;
        turns += 1;
    }
    t.pictures();
    most = most.max(t.w().palette.thumbs.len());
    let made = t.w().palette.thumbs_made;
    let rows_in_view = most_tiles.div_ceil(cols);
    t.detail(
        "grid: the Models grid scrolls through the whole game with a bounded number of pictures",
        total > 1000 && turns * 6 >= total / cols && made > THUMBS_MAX && most <= THUMBS_MAX && rows_in_view <= 12,
        format!("{total} models, {cols} columns, {turns} turns, {made} pictures made, at most {most} kept, at most {most_tiles} tiles in view"),
    );
    t.shot("grid/rom-sage-models-end");
    t.wheel(at, 100000.0);
    for (theme, tabs) in [("sage", ["Enemies", "Kit"].as_slice()), ("crt", ["Enemies", "Models", "Kit"].as_slice())] {
        t.w().set_theme(theme);
        for tab in tabs {
            t.click(tab);
            t.pictures();
            t.until(120, |w| w.side_jobs.running == 0);
            t.shot(&format!("grid/rom-{theme}-{}", tab.to_lowercase()));
        }
    }
    t.finish();
}

/// The layout of the panels: which show, on which side, stacked or tabbed.
///
/// Every panel is hidden and shown again by clicks: through its own menu and through Options > Panels. The view
/// takes the room a hidden panel leaves. Level goes to the right and the inspector to the left, the checks go under
/// the view, and two panels on one side are tabs or a stack with a divider to drag. `Reset layout` gives the first
/// layout and sizes. The layout and a dragged width come back in a new window, and a damaged setting gives the
/// default. `A` and `P` open their panels wherever they are. With every panel hidden the view fills the window and
/// a click in it still selects.
///
/// With `GLOVEBOX_SHOTS` it saves four layouts in two themes in `DIR/layout/`, and with `GLOVEBOX_ROM` on a level
/// of the game.
#[test]
fn panel_layout() {
    use glovebox::app::{INSPECTOR_WIDTH, LEVEL_WIDTH};
    use glovebox::layout::{Layout, Mode as Sharing, PanelId, Side};
    let near = |a: f32, b: f32| (a - b).abs() <= 1.0;
    let fixture = || -> Option<Arc<dyn Backend>> { Some(Arc::new(FixtureBackend::new())) };
    let mut t = T::new("layout", fixture(), [1280.0, 720.0]);
    t.idle();
    let (p0, v0) = (t.w().panels, t.w().view.rect);
    let slots = |t: &mut T| -> Vec<bool> { PanelId::ALL.iter().map(|p| t.w().panels.slot(*p).is_positive()).collect() };
    let start = t.w().layout == Layout::default() && slots(&mut t) == [true, true, true, false, false] && !t.settings_text().contains("panels");
    t.detail("layout: the window starts with the default layout", start && near(p0.level.left(), 0.0) && near(p0.inspector.right(), 1280.0) && near(p0.checks.right(), 1280.0), format!("{p0:?}"));

    // Level: hidden by its own menu, shown by the Options menu
    let hid = t.panel_menu("Level", "Hide");
    let (p, v) = (t.w().panels, t.w().view.rect);
    let gone = !p.slot(PanelId::Level).is_positive() && !p.level.is_positive() && !t.has("Level panel menu");
    t.detail("layout: Hide in the Level panel's menu hides it, and the view takes its room", hid && gone && near(v.left(), 0.0) && near(v.width(), v0.width() + LEVEL_WIDTH), format!("{hid} {gone} {v:?}"));
    let saved = t.settings_text().contains("level:left:off");
    let shown = t.panels_menu(&["Show Level"]);
    let (p, v) = (t.w().panels, t.w().view.rect);
    t.detail("layout: the checkbox in Options > Panels shows it again", saved && shown && close(p.slot(PanelId::Level), p0.slot(PanelId::Level)) && close(v, v0), format!("{saved} {shown} {p:?}"));

    // Checks, then the inspector: the right column goes when both are hidden
    let hid = t.panel_menu("Checks", "Hide");
    let p = t.w().panels;
    let grew = !p.checks.is_positive() && near(p.slot(PanelId::Inspector).bottom(), p0.checks.bottom()) && close(t.w().view.rect, v0);
    t.detail("layout: hiding the checks gives the inspector their room", hid && grew, format!("{hid} {p:?}"));
    let hid = t.panel_menu("Inspector", "Hide");
    let (p, v) = (t.w().panels, t.w().view.rect);
    let gone = !p.inspector.is_positive() && !p.slot(PanelId::Inspector).is_positive() && near(v.right(), 1280.0) && near(v.width(), v0.width() + INSPECTOR_WIDTH);
    t.detail("layout: hiding the inspector too gives the view the right side", hid && gone, format!("{hid} {p:?} {v:?}"));
    // A selection does not bring the inspector back, and the status line says what it is
    let enemy = ent(t.ed(), |e| e.kind == "enemy" && !e.is_handle);
    let ed = t.ed();
    let at = enemy.map(|e| px(ed, ed.obj_pos(e))).unwrap_or((100.0, 100.0));
    t.view_click(at, Modifiers::default());
    let picked = t.ed().selection.len() == 1;
    let status = t.status();
    let still = !t.w().panels.slot(PanelId::Inspector).is_positive();
    t.detail("layout: a click selects with the inspector hidden, and the status line says what", picked && still && status.starts_with("Selected "), format!("{picked} {still} {status}"));
    t.ed().set_selection(Vec::new());
    let shown = t.panels_menu(&["Show Inspector", "Show Checks"]);
    let p = t.w().panels;
    let v = t.w().view.rect;
    t.detail("layout: both checkboxes bring the right column back", shown && close(p.inspector, p0.inspector) && close(p.checks, p0.checks) && close(v, v0), format!("{shown} {p:?}"));

    // Add to level and Path follow their tools
    t.key(Key::A);
    let open = t.w().palette_open && t.w().panels.slot(PanelId::Add).is_positive();
    let hid = t.panel_menu("Add to level", "Hide");
    let closed = !t.w().palette_open && !t.w().panels.slot(PanelId::Add).is_positive() && t.w().panels.slot(PanelId::Inspector).is_positive();
    t.key(Key::A);
    let again = t.w().palette_open && t.w().panels.slot(PanelId::Add).is_positive();
    t.detail("layout: Hide closes Add to level, and A shows it again", open && hid && closed && again, format!("{open} {hid} {closed} {again}"));
    t.key(Key::A);
    t.key(Key::P);
    let open = t.ed().path.on && t.w().panels.slot(PanelId::Path).is_positive();
    let hid = t.panel_menu("Path", "Hide");
    let closed = !t.ed().path.on && !t.w().panels.slot(PanelId::Path).is_positive();
    t.key(Key::P);
    let again = t.ed().path.on && t.w().panels.slot(PanelId::Path).is_positive();
    t.key(Key::P);
    t.detail("layout: Hide closes Path, and P shows it again", open && hid && closed && again, format!("{open} {hid} {closed} {again}"));
    let checked = t.panels_menu(&["Show Add to level"]) && t.w().palette_open;
    let unchecked = t.panels_menu(&["Show Add to level"]) && !t.w().palette_open;
    t.detail("layout: the checkbox of Add to level opens and closes it", checked && unchecked, format!("{checked} {unchecked}"));

    // Sides: Level to the right, the inspector to the left
    // On one side they are tabs, and the menu is the one of the panel in front
    let moved = t.panel_menu("Level", "Move to right") && !t.has("Inspector panel menu") && t.click("Inspector") && t.panel_menu("Inspector", "Move to left");
    let (p, v) = (t.w().panels, t.w().view.rect);
    let (level, inspector) = (p.slot(PanelId::Level), p.slot(PanelId::Inspector));
    let sides = near(inspector.left(), 0.0) && near(inspector.right(), v.left()) && near(level.right(), 1280.0) && near(level.left(), v.right());
    // The checks stayed on the right, under the Level panel
    let under = near(p.checks.top(), level.bottom()) && near(p.checks.left(), level.left());
    t.shot("layout/fixture-mirrored");
    t.detail("layout: Move to right and Move to left swap the panels' sides", moved && sides && under, format!("{moved} {p:?} {v:?}"));
    let text = t.settings_text();
    t.detail("layout: the sides are in the settings", text.contains("level:right:on") && text.contains("inspector:left:on"), text);
    // With the inspector on the left, Add to level is a tab beside it
    t.key(Key::A);
    let p = t.w().panels;
    let right = p.slot(PanelId::Add).is_positive() && near(p.slot(PanelId::Add).right(), 1280.0) && !p.slot(PanelId::Level).is_positive() && t.has("Level") && p.slot(PanelId::Inspector).is_positive();
    t.detail("layout: Add to level opens as a tab of its own side", right, format!("{p:?}"));
    let moved = t.panel_menu("Add to level", "Move to left");
    let p = t.w().panels;
    let left = near(p.slot(PanelId::Add).left(), 0.0) && p.slot(PanelId::Level).is_positive() && !p.slot(PanelId::Inspector).is_positive();
    t.detail("layout: Move to left puts Add to level among the left side's tabs", moved && left, format!("{moved} {p:?}"));
    t.key(Key::A);

    // The checks under the view
    let moved = t.panel_menu("Checks", "Move to bottom");
    let (p, v) = (t.w().panels, t.w().view.rect);
    let under = near(p.checks.top(), v.bottom()) && near(p.checks.left(), v.left()) && near(p.checks.right(), v.right()) && v.height() < v0.height() - 90.0;
    let tall = near(p.slot(PanelId::Level).bottom(), p.checks.bottom());
    t.shot("layout/fixture-checks-bottom");
    t.detail("layout: Move to bottom puts the checks under the view", moved && under && tall, format!("{moved} {p:?} {v:?}"));
    // A click on a check's line still shows the spot
    let lines = t.w().checks.iter().filter(|c| c.click.is_some()).count();
    let high = t.w().panels.checks.height() > 90.0;
    t.detail("layout: the checks at the bottom show their lines", t.has("CHECKS") && high, format!("{lines}"));
    let top = t.w().panels.checks.top();
    let x = t.w().panels.checks.center().x;
    t.drag_window(egui::pos2(x, top), egui::pos2(x + 5.0, top - 80.0));
    let now = t.w().panels.checks;
    t.detail("layout: a drag on the bottom checks' edge changes their height", near(now.top(), top - 80.0), format!("{top} {now:?}"));
    let back = t.panels_menu(&["Checks on the left"]);
    let p = t.w().panels;
    t.detail("layout: the side choice in Options > Panels moves the checks to the left", back && near(p.checks.left(), 0.0) && near(p.checks.top(), p.slot(PanelId::Inspector).bottom()), format!("{back} {p:?}"));

    // Reset layout
    let reset = t.panels_menu(&["Reset layout"]);
    let (p, v) = (t.w().panels, t.w().view.rect);
    let same = t.w().layout == Layout::default() && close(p.level, p0.level) && close(p.inspector, p0.inspector) && close(p.checks, p0.checks) && close(v, v0);
    t.detail("layout: Reset layout gives the default layout and sizes", reset && same, format!("{reset} {p:?} {p0:?}"));

    // Two panels on one side: tabs, or a stack with a divider
    let moved = t.panels_menu(&["Level on the right"]);
    let p = t.w().panels;
    let (level, inspector) = (p.slot(PanelId::Level), p.slot(PanelId::Inspector));
    let tabs = level.is_positive() != inspector.is_positive() && near(t.w().view.rect.left(), 0.0) && t.find("Level", true).iter().any(|n| n.accesskit_node().role() == egui::accesskit::Role::Button);
    let front = t.click("Inspector") && t.w().panels.slot(PanelId::Inspector).is_positive() && !t.w().panels.slot(PanelId::Level).is_positive();
    t.shot("layout/fixture-tabbed");
    t.detail("layout: Level and the inspector on one side are tabs", moved && tabs && front, format!("{moved} {tabs} {front} {p:?}"));
    let stacked = t.panels_menu(&["Right side stacked"]) && t.w().layout.mode(Side::Right) == Sharing::Stacked;
    let p = t.w().panels;
    let (level, inspector) = (p.slot(PanelId::Level), p.slot(PanelId::Inspector));
    let both = level.is_positive() && inspector.is_positive() && near(level.bottom(), inspector.top()) && near(inspector.bottom(), p.checks.top()) && near(level.left(), inspector.left());
    let titles = t.has("Level panel menu") && t.has("Inspector panel menu");
    t.shot("layout/fixture-stacked");
    t.detail("layout: Stacked shows them one under the other", stacked && both && titles, format!("{stacked} {titles} {p:?}"));
    let y = level.bottom();
    t.drag_window(egui::pos2(level.center().x, y), egui::pos2(level.center().x + 4.0, y - 60.0));
    let p = t.w().panels;
    let dragged = near(p.slot(PanelId::Level).bottom(), y - 60.0) && near(p.slot(PanelId::Inspector).top(), y - 60.0);
    t.detail("layout: a drag on the divider of a stack changes the heights", dragged, format!("{p:?}"));
    let tabbed = t.panels_menu(&["Right side tabbed"]) && t.w().panels.slot(PanelId::Level).is_positive() != t.w().panels.slot(PanelId::Inspector).is_positive();
    t.check("layout: Tabbed shows them as tabs again", tabbed);
    t.panels_menu(&["Right side stacked"]);

    // A dragged width and the layout come back in a new window
    let p = t.w().panels.inspector;
    t.drag_window(egui::pos2(p.left(), 300.0), egui::pos2(1280.0 - 450.0, 305.0));
    let dragged = near(t.w().panels.inspector.width(), 450.0);
    let (layout, text) = (t.w().layout.clone(), t.settings_text());
    let written = text.contains("level:right:on") && text.contains("right:stacked") && text.contains("inspector:450");
    let home = t.home.clone();
    let mut results = std::mem::take(&mut t.results);
    drop(t.h);
    let mut t = T::open(home, fixture(), [1280.0, 720.0]);
    t.idle();
    let p = t.w().panels;
    let same = t.w().layout == layout && near(p.inspector.width(), 450.0) && p.slot(PanelId::Level).is_positive() && near(p.slot(PanelId::Level).right(), 1280.0) && near(t.w().view.rect.left(), 0.0);
    t.detail("layout: a new window has the layout and the dragged width", dragged && written && same, format!("{dragged} {written} {text} {p:?}"));

    // Every panel hidden: the view fills the window, and a click in it selects
    let hidden = t.panels_menu(&["Show Level", "Show Inspector", "Show Checks"]);
    let (v, window) = (t.w().view.rect, t.h.ctx.content_rect());
    let full = near(v.left(), 0.0) && near(v.right(), window.right()) && slots(&mut t) == [false; 5] && v.height() > window.height() - 110.0;
    let enemy = ent(t.ed(), |e| e.kind == "enemy" && !e.is_handle);
    let ed = t.ed();
    let at = enemy.map(|e| px(ed, ed.obj_pos(e))).unwrap_or((100.0, 100.0));
    t.view_click(at, Modifiers::default());
    let picked = t.ed().selection.len() == 1 && t.status().starts_with("Selected ");
    t.shot("layout/fixture-view-only");
    t.detail("layout: with every panel hidden the view fills the window and a click selects", hidden && full && picked, format!("{hidden} {full} {picked} {v:?}"));
    // A and P still open their panels
    t.ed().set_selection(Vec::new());
    t.key(Key::A);
    let add = t.w().panels.slot(PanelId::Add).is_positive() && t.has("Collectables");
    t.key(Key::A);
    t.key(Key::P);
    let path = t.w().panels.slot(PanelId::Path).is_positive();
    t.key(Key::P);
    let none = slots(&mut t) == [false; 5];
    t.detail("layout: A and P open their panels over a view-only window", add && path && none, format!("{add} {path} {none}"));
    results.append(&mut t.results);
    t.results = results;
    t.finish();

    // A damaged setting gives the default layout
    for (k, bad) in ["nonsense", "level:top:on,inspector:right:on,checks:right:on,add:right,path:right,left:tabbed,right:tabbed", "level:left:on,inspector:right:on", "level:bottom:on,inspector:right:on,checks:right:on,add:right,path:right,left:tabbed,right:tabbed"].into_iter().enumerate() {
        let mut t = T::with_settings(&format!("layout-bad-{k}"), fixture(), [1280.0, 720.0], &format!("[General]\ntour=seen\npanels=\"{bad}\"\npanel_sizes=\"right:banana,left:99999\"\n"));
        t.idle();
        let p = t.w().panels;
        let ok = t.w().layout == Layout::default() && near(p.level.width(), LEVEL_WIDTH) && near(p.inspector.width(), INSPECTOR_WIDTH) && near(p.inspector.right(), 1280.0);
        t.detail(&format!("layout: a damaged setting gives the default layout ({bad})"), ok, format!("{p:?}"));
        t.finish();
    }
    assert_eq!(Layout::from_text(&Layout::default().to_text()), Some(Layout::default()));

    // A wide window: Add to level beside the inspector while the view keeps its least width
    let mut t = T::new("layout-wide", fixture(), [1920.0, 1080.0]);
    t.idle();
    t.click("+ Add");
    let p = t.w().panels;
    let beside = p.add.is_positive() && near(p.add.right(), p.inspector.left()) && p.slot(PanelId::Inspector).is_positive();
    t.h.set_size(egui::vec2(1570.0, 900.0));
    t.settle();
    let p = t.w().panels;
    let still = p.add.is_positive() && t.w().view.rect.width() >= glovebox::layout::MIN_VIEW;
    // Wider panels would leave the view too little: Add to level goes back among the tabs
    t.drag_window(egui::pos2(p.inspector.left(), 400.0), egui::pos2(p.inspector.right() - 520.0, 405.0));
    let (p, v) = (t.w().panels, t.w().view.rect);
    let tabs = !p.add.is_positive() && p.slot(PanelId::Add).is_positive() && !p.slot(PanelId::Inspector).is_positive() && v.width() >= glovebox::layout::MIN_VIEW;
    t.detail("layout: Add to level has its own column only while the view keeps its least width", beside && still && tabs, format!("{beside} {still} {tabs} {p:?} {v:?}"));
    t.finish();

    // Pictures: four layouts in two themes
    let layouts = [
        ("default", Layout::default().to_text()),
        ("mirrored", "level:right:on,inspector:left:on,checks:left:on,add:left,path:left,left:tabbed,right:tabbed".to_string()),
        ("checks-bottom", "level:left:on,inspector:right:on,checks:bottom:on,add:right,path:right,left:tabbed,right:tabbed".to_string()),
        ("stacked", "level:right:on,inspector:right:on,checks:left:on,add:left,path:left,left:tabbed,right:stacked".to_string()),
        ("view-only", "level:left:off,inspector:right:off,checks:right:off,add:right,path:right,left:tabbed,right:tabbed".to_string()),
    ];
    if std::env::var_os("GLOVEBOX_SHOTS").is_some() {
        for theme in ["1998", "sage", "dark", "crt"] {
            let mut t = T::with_theme(&format!("layout-{theme}"), fixture(), [1280.0, 720.0], Some(theme));
            t.idle();
            let enemy = ent(t.ed(), |e| e.kind == "enemy");
            t.ed().select(enemy, false);
            for (name, text) in &layouts {
                let l = Layout::from_text(text).expect("a layout");
                t.w().set_layout(l);
                t.settle();
                t.shot(&format!("layout/{theme}-{name}"));
            }
            t.w().set_layout(Layout::default());
            t.click("Options");
            t.click("Panels");
            t.shot(&format!("layout/{theme}-menu"));
            t.click("Level panel menu");
            t.shot(&format!("layout/{theme}-panel-menu"));
            t.finish();
        }
    }
    match (std::env::var_os("GLOVEBOX_ROM").map(PathBuf::from), std::env::var_os("GLOVEBOX_SHOTS")) {
        (Some(rom), Some(_)) => {
            let mut t = T::with_theme("layout-rom", None, [1280.0, 800.0], Some("sage"));
            t.files.0.borrow_mut().open.push_back(rom);
            t.settle();
            t.h.get_by_label("Choose the ROM…").click();
            t.settle();
            t.idle();
            let enemy = ent(t.ed(), |e| e.kind == "enemy");
            t.ed().select(enemy, false);
            for (name, text) in &layouts {
                let l = Layout::from_text(text).expect("a layout");
                t.w().set_layout(l);
                t.settle();
                t.shot(&format!("layout/rom-sage-{name}"));
            }
            t.finish();
        }
        _ => println!("skipped: GLOVEBOX_ROM and GLOVEBOX_SHOTS are not both set (the layouts on a level of the game)"),
    }
}

/// The first-run screen: a wrong file is refused with the first-run screen's message.
#[test]
fn setup_refuses_a_wrong_file() {
    let mut t = T::new("setup", None, [1280.0, 720.0]);
    let wrong = t.home.join("wrong.z64");
    std::fs::write(&wrong, vec![0u8; 4096]).unwrap();
    t.files.0.borrow_mut().open.push_back(wrong);
    t.settle();
    let shown = t.has("Choose the ROM…") && t.h.state_mut().window().is_none();
    t.check("the setup screen asks for the ROM", shown);
    t.h.get_by_label("Choose the ROM…").click();
    t.settle();
    let refused = t.has_containing("is not a Nintendo 64 ROM");
    t.shot("30-setup");
    let still = t.h.state_mut().window().is_none();
    t.check("a file that is no ROM is refused", refused && still);
    // Choosing nothing changes nothing
    t.h.get_by_label("Choose the ROM…").click();
    t.settle();
    let still = t.h.state_mut().window().is_none();
    t.check("a canceled dialog keeps the screen", still);
    t.finish();
}

/// With `GLOVEBOX_ROM`: the setup screen takes the real ROM, all 48 levels open, a ROM builds and passes
/// `hwcheck`, and a patch is exported.
#[test]
fn the_rom() {
    let Some(rom) = std::env::var_os("GLOVEBOX_ROM").map(PathBuf::from) else {
        println!("skipped: GLOVEBOX_ROM is not set");
        return;
    };
    let mut t = T::new("rom", None, [1500.0, 950.0]);
    // The setup screen's own path
    t.files.0.borrow_mut().open.push_back(rom);
    t.settle();
    t.h.get_by_label("Choose the ROM…").click();
    let t0 = Instant::now();
    t.settle();
    let opened = t.h.state_mut().window().is_some() && t.home.join("rom").join("glover.z64").is_file();
    t.check("the setup screen takes the ROM and opens the editor", opened);
    t.idle();
    println!("first level open {:.1} s after the ROM was chosen", t0.elapsed().as_secs_f64());
    let scenes: Vec<(String, String)> = t.ed().scenes.iter().map(|s| (s["id"].as_str().unwrap().to_string(), s["name"].as_str().unwrap().to_string())).collect();
    t.detail("the level list has 48 levels", scenes.len() == 48, scenes.len().to_string());
    t.shot("40-rom-first-level");
    let mut bad = Vec::new();
    let (mut fired, mut unknown) = (Vec::new(), Vec::new());
    for (sid, name) in &scenes {
        let t0 = Instant::now();
        t.click("Level");
        t.choose(&format!("{name}  ({sid})"));
        let want = sid.clone();
        let loaded = t.until(300, move |w| !w.busy && w.ed.scene_id() == want && w.jobs.running == 0);
        t.settle();
        let ok = loaded && t.has_check(None, "No new problems") && t.has_containing("No new problems");
        let w = t.w();
        let errs: Vec<String> = w.checks.iter().filter(|c| c.level == "err").map(|c| c.text.clone()).collect();
        println!("{}  {sid} {name}: {} entities, {:.1} s, status: {}", if ok { "ok  " } else { "FAIL" }, w.ed.entities.len(), t0.elapsed().as_secs_f64(), w.status.0);
        if !ok {
            bad.push(format!("{sid}: {errs:?}"));
        } else if !errs.is_empty() {
            println!("      {sid} has errors of its own: {errs:?}");
        }
        if matches!(sid.as_str(), "0a" | "1b" | "3c") {
            t.shot(&format!("41-rom-{sid}"));
        }
        // The exits of the level as the game sizes them. The check on them must stay silent on a shipped level.
        let ed = t.ed();
        let list = ed.exits();
        for &i in &list {
            let x = ed.exit_trigger(i).unwrap();
            let oid = ed.entities[i].object_id.clone().unwrap_or_default();
            let model = ed.catalog.get(&oid).map(|c| c.name.clone()).unwrap_or_else(|| ed.entities[i].tag.clone());
            let radius = x.radius.map_or("unknown".to_string(), |r| format!("{r:.1}"));
            println!("      exit  {sid} {name} | {model} | type {} | radius {radius} | {:?}{}", x.exit_type, x.rule.who, if x.dead() { " | can never fire" } else { "" });
            if x.radius.is_none() {
                unknown.push(format!("{sid} {oid}"));
            }
        }
        fired.extend(ed.violations.iter().filter(|v| v.key.ends_with(":exit")).map(|v| format!("{sid}: {}", v.msg)));
        // A picture of a world entrance of hub 1 (the Fortress of Fear's, type 10) and of the ends of Atlantis 3
        // and Carnival 1
        let shown = list.iter().copied().find(|&i| sid != "00" || ed.exit_trigger(i).unwrap().exit_type == 10);
        if let (true, Some(i)) = (matches!(sid.as_str(), "00" | "0c" | "0f"), shown) {
            let x = ed.exit_trigger(i).unwrap();
            let r = x.radius.unwrap_or(0.0).max(40.0);
            // From the side Glover starts on, a little above
            let start = ent(ed, |e| e.kind == "spawn").map_or(ed.camera.pos, |s| ed.obj_pos(s));
            let flat = [start[0] - x.center[0], 0.0, start[2] - x.center[2]];
            let n = vec::norm(flat).max(1e-6);
            let away = [flat[0] / n, 0.45, flat[2] / n];
            ed.orbit.target = x.center;
            ed.camera.pos = vec::add(x.center, vec::scale(away, 4.0 * r / vec::norm(away)));
            ed.camera.look_at(x.center);
            ed.select(Some(i), false);
            let fog = ed.show_fog;
            ed.show_fog = false;
            t.w().view.dirty = true;
            t.shot(&format!("exit-{sid}"));
            t.ed().show_fog = fog;
            t.ed().select(None, false);
        }
    }
    t.detail("the exit check is silent on all 48 shipped levels", fired.is_empty(), format!("{fired:?}"));
    t.detail("the editor can measure every shipped exit", unknown.is_empty(), format!("{unknown:?}"));
    t.detail("all 48 levels open through the level list and say No new problems", bad.is_empty(), format!("{bad:?}"));

    // Build ROM for one level, through the button
    t.click("Level");
    t.choose("  (0a)");
    t.until(300, |w| !w.busy && w.ed.scene_id() == "0a" && w.jobs.running == 0);
    // A plain build is the ROM without its unused end. With an edit that needs the game mod the ROM is a whole
    // number of MiB, which is what hwcheck is for: tilt a platform, an enemy or a powerup.
    let ed = t.ed();
    let tilt = ent(ed, |e| matches!(e.kind, "platform" | "enemy" | "pickup") && !e.is_handle && e.rot.is_some());
    ed.select(tilt, false);
    ed.apply_transform(None, Some(90.0), 0, None);
    let with_mod = !ed.mod_users().is_empty();
    t.settle();
    t.check("a tilted object needs the game mod", with_mod);
    let path = t.save_to("built.z64");
    t.click("Build ROM");
    t.until(600, |w| w.jobs.running == 0 && w.loading.is_empty());
    let data = std::fs::read(&path).unwrap_or_default();
    let problems = if data.is_empty() { vec!["no file".to_string()] } else { glovebox_core::hwcheck::check_rom(&data) };
    let status = t.status();
    t.detail("Build ROM builds a ROM that passes hwcheck", problems.is_empty() && status.contains("Built"), format!("{problems:?} {status}"));
    // Export patch, through its dialog
    let path = t.save_to("patch.bps");
    t.click("Export patch");
    t.click("Export .bps");
    t.until(600, |w| w.jobs.running == 0 && w.loading.is_empty());
    let data = std::fs::read(&path).unwrap_or_default();
    let status = t.status();
    t.detail("Export patch writes a BPS patch", data.starts_with(b"BPS1") && status.contains("Exported"), format!("{} bytes, {status}", data.len()));
    // An edit, then Test level with a made-up emulator: the ROM is written to test/
    let emulator = t.home.join("emulator.sh");
    std::fs::write(&emulator, "#!/bin/sh\nexit 0\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&emulator, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    t.files.0.borrow_mut().open.push_back(emulator);
    t.click("Test level");
    t.click("OK");
    t.until(600, |w| w.jobs.running == 0 && w.loading.is_empty());
    let built = t.home.join("test").join("glover-0a-test.z64");
    let status = t.status();
    t.detail("Test level builds into test/ and starts the emulator", built.is_file() && status.contains("Testing"), status);
    t.shot("42-rom-end");
    t.finish();
}

/// With `GLOVEBOX_ROM` and `GLOVEBOX_GUIDE=DIR`: saves the pictures of the guide (`docs/guide/img`) into DIR.
#[test]
fn guide_shots() {
    let (Some(rom), Some(dir)) = (std::env::var_os("GLOVEBOX_ROM").map(PathBuf::from), std::env::var_os("GLOVEBOX_GUIDE").map(PathBuf::from)) else {
        println!("skipped: GLOVEBOX_ROM and GLOVEBOX_GUIDE are not both set");
        return;
    };
    // The folder is given from where cargo was started. The test runs in the crate's folder.
    let dir = if dir.is_absolute() { dir } else { PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").join(dir) };
    guide::run(rom, dir);
}
