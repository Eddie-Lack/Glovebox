//! The editor on the fixture, without a ROM: the checks that need no widgets.

use glovebox_core::backend::Backend;
use glovebox_editor::drafts::Drafts;
use glovebox_editor::editor::{Button, Editor, MemorySettings, Mods, Scatter};
use glovebox_editor::entities::{camera_zone, preset_zone, set_camera_zone, zone_center, Ent, EntId};
use glovebox_editor::fixture::{FixtureBackend, SIGN};
use glovebox_editor::gizmo::{Handle, Mode};
use glovebox_editor::presets::PRESETS;
use glovebox_editor::sanitize::{sanitize_level_xml, FormatModel};
use glovebox_editor::xmlutil::{body, num0};
use glovebox_editor::{scripts, vec};
use std::sync::Arc;

struct Checks(Vec<(String, bool, String)>);

impl Checks {
    fn check(&mut self, name: &str, ok: bool) {
        self.detail(name, ok, String::new());
    }
    fn detail(&mut self, name: &str, ok: bool, detail: String) {
        println!("{}  {}{}", if ok { "ok  " } else { "FAIL" }, name, if ok { String::new() } else { format!("  ({detail})") });
        self.0.push((name.to_string(), ok, detail));
    }
}

fn ent(ed: &Editor, pred: impl Fn(&Ent) -> bool) -> Option<EntId> {
    ed.find(pred)
}

fn px(ed: &Editor, p: vec::V3) -> (f64, f64) {
    let v = ed.camera.to_pixels(p);
    (v[0].round(), v[1].round())
}

fn click(ed: &mut Editor, at: (f64, f64), mods: Mods) {
    ed.mouse_press(at.0, at.1, Button::Left, mods);
    ed.mouse_release(at.0, at.1, Button::Left, mods);
}

fn kinds(ed: &Editor, kind: &str) -> Vec<EntId> {
    (0..ed.entities.len()).filter(|&i| ed.entities[i].kind == kind).collect()
}

fn tags(ed: &Editor, n: glovebox_core::xml::NodeId) -> Vec<String> {
    ed.doc.children(n).iter().map(|c| ed.doc.tag(*c).to_string()).collect()
}

fn has_check(ed: &mut Editor, level: Option<&str>, text: &str) -> bool {
    ed.checks().iter().any(|c| level.is_none_or(|l| c.level == l) && c.text.contains(text))
}

fn undo_to(ed: &mut Editor, mark: usize) {
    while ed.undo_stack.len() > mark {
        ed.undo();
    }
}

#[test]
fn editor_on_the_fixture() {
    let home = std::env::temp_dir().join(format!("glovebox-editor-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let backend: Arc<dyn Backend> = Arc::new(FixtureBackend::new());
    let mut ed = Editor::new(backend.clone(), Drafts::new(home.join("drafts")), Some(Box::new(MemorySettings::default())));
    ed.start();
    ed.set_catalog(backend.catalog().unwrap());
    ed.set_view_size(1100.0, 800.0);
    ed.load_scene("10").unwrap();
    let mut t = Checks(Vec::new());
    let none = Mods::default();

    t.detail("level loads", ed.scene_id() == "10" && ed.entities.len() > 15, ed.entities.len().to_string());
    let have: Vec<&str> = ed.entities.iter().map(|e| e.kind).collect();
    let all = ["land", "platform", "garib", "enemy", "pickup", "tip", "spawn", "wind", "hazard", "sound", "trigger"];
    t.check("every kind of entity is built", all.iter().all(|k| have.contains(k)));
    let land = ent(&ed, |e| e.kind == "land").unwrap();
    t.check("models load", !ed.graph[ed.nodes[land].obj].children().is_empty() && !ed.entities.iter().any(|e| e.missing_model));
    t.check("checks run", has_check(&mut ed, None, "No new problems"));

    // Picking: a click on the enemy selects it
    let enemy = ent(&ed, |e| e.kind == "enemy").unwrap();
    ed.focus_on(enemy);
    let at = px(&ed, ed.obj_pos(enemy));
    click(&mut ed, at, none);
    t.check("click picks the enemy", ed.selection == vec![enemy]);

    // The gizmo: drag the X arrow
    t.check("the gizmo is attached", ed.gizmo.attached && ed.gizmo.mode == Mode::Translate);
    let before = ed.entities[enemy].world_pos(&ed.doc, &ed.entities);
    let s = ed.gizmo.size(&ed.camera);
    let a = px(&ed, vec::add(ed.gizmo.pos, [s * 0.7, 0.0, 0.0]));
    ed.mouse_move(a.0, a.1);
    ed.mouse_press(a.0, a.1, Button::Left, none);
    for i in 1..7 {
        ed.mouse_move(a.0 + 10.0 * f64::from(i), a.1);
    }
    ed.mouse_release(a.0 + 60.0, a.1, Button::Left, none);
    let pos = |ed: &Editor| {
        let e = ent(ed, |e| e.kind == "enemy").unwrap();
        ed.entities[e].world_pos(&ed.doc, &ed.entities)
    };
    let after = pos(&ed);
    t.detail(
        "dragging the X arrow moves the enemy along x",
        (after[0] - before[0]).abs() > 1.0 && (after[1] - before[1]).abs() < 1e-6 && (after[2] - before[2]).abs() < 1e-6,
        format!("{before:?} {after:?}"),
    );
    t.check("snap 1 keeps whole numbers", (after[0] - after[0].round()).abs() < 1e-6);
    t.check("the drag is one undo step", ed.undo_stack.len() == 1);
    ed.undo();
    t.check("undo puts it back", pos(&ed) == before && ed.selection == vec![ent(&ed, |e| e.kind == "enemy").unwrap()]);
    ed.redo();
    t.check("redo moves it again", pos(&ed) == after);

    // Rotation: E, then drag the Y ring
    ed.set_mode(Mode::Rotate);
    t.check("E switches to rotate", ed.mode == Mode::Rotate && ed.gizmo.mode == Mode::Rotate);
    let ring = ed.gizmo.handles(&ed.camera).into_iter().find(|h| h.0 == Handle::Y).unwrap().2;
    let pts: Vec<(f64, f64)> = ring.iter().map(|p| px(&ed, *p)).collect();
    let i0 = (0..pts.len()).max_by(|a, b| pts[*a].1.partial_cmp(&pts[*b].1).unwrap()).unwrap(); // the ring's nearest point
    let (a, b) = (pts[i0], pts[(i0 + 6) % pts.len()]);
    ed.mouse_move(a.0, a.1);
    ed.mouse_press(a.0, a.1, Button::Left, none);
    for k in 0..8 {
        let f = f64::from(k) / 7.0;
        ed.mouse_move((a.0 + (b.0 - a.0) * f).trunc(), (a.1 + (b.1 - a.1) * f).trunc());
    }
    ed.mouse_release(b.0, b.1, Button::Left, none);
    let enemy = ent(&ed, |e| e.kind == "enemy").unwrap();
    let node = ed.entities[enemy].node;
    let yr = num0(&ed.doc, node, "y_rotation");
    t.detail(
        "dragging the Y ring turns the enemy (y_rotation)",
        yr.abs() > 0.1 && ed.doc.first(node, "SetActorRotation").is_none(),
        yr.to_string(),
    );
    t.check("rotation snaps to 15°", (yr.to_degrees() / 15.0 - (yr.to_degrees() / 15.0).round()).abs() < 1e-3);
    ed.set_mode(Mode::Translate);

    // Numeric transform in the inspector
    ed.apply_transform(None, Some(90.0), 0, None);
    let node = ed.entities[ent(&ed, |e| e.kind == "enemy").unwrap()].node;
    t.check("a tilt about x needs the game mod (SetActorRotation)", ed.doc.first(node, "SetActorRotation").is_some() && !ed.mod_users().is_empty());
    t.check("the checks say so", has_check(&mut ed, None, "Expansion Pak game mod"));
    ed.undo();

    // Scale a landscape piece
    let grave = ent(&ed, |e| e.label == "grave");
    ed.select(grave, false);
    ed.apply_transform(None, None, 1, Some(2.0));
    let grave = &ed.entities[ent(&ed, |e| e.label == "grave").unwrap()];
    t.check("scale writes SetActorScale", grave.get_scale(&ed.doc) == Some([2.0, 2.0, 2.0]));
    let y = vec::euler_yzx_from_quat(grave.get_quat(&ed.doc).unwrap())[1];
    t.check("rotation survives a scale", (y - 0.6).abs() < 1e-3);

    // Group rotation about the group center
    let center = |ed: &Editor| {
        let g = kinds(ed, "garib");
        let mut c = [0.0; 3];
        for &i in &g {
            c = vec::add(c, ed.entities[i].world_pos(&ed.doc, &ed.entities));
        }
        vec::div(c, g.len() as f64)
    };
    let garibs = kinds(&ed, "garib");
    ed.set_selection(garibs);
    let c0 = center(&ed);
    ed.apply_transform(None, Some(90.0), 1, None);
    let c1 = center(&ed);
    t.check(
        "a group rotation keeps the center and moves the garibs",
        (0..3).all(|k| (c0[k] - c1[k]).abs() < 0.01) && ed.selection.len() == 3,
    );

    // Box select
    ed.set_selection(Vec::new());
    ed.frame_level();
    ed.set_box_mode(true);
    t.check("B turns box select on", ed.box_mode && !ed.gizmo.attached);
    let (w, h) = (ed.camera.width, ed.camera.height);
    ed.mouse_press(5.0, 5.0, Button::Left, none);
    ed.mouse_move(w / 2.0, h / 2.0);
    ed.mouse_move(w - 5.0, h - 5.0);
    ed.mouse_release(w - 5.0, h - 5.0, Button::Left, none);
    let land_in = ed.selection.iter().any(|&i| ed.entities[i].kind == "land");
    t.detail("box select takes what is in the box, without the landscape", ed.selection.len() > 5 && !land_in && !ed.box_mode, ed.selection.len().to_string());

    // Ctrl+click toggles
    let first = ed.selection[0];
    ed.focus_on(first);
    let n = ed.selection.len();
    let at = px(&ed, ed.obj_pos(first));
    click(&mut ed, at, Mods { ctrl: true, ..none });
    t.check("Ctrl+click takes one out of the selection", ed.selection.len() == n - 1 && !ed.selection.contains(&first));

    // Duplicate and delete
    let tip = ent(&ed, |e| e.kind == "tip");
    ed.select(tip, false);
    ed.duplicate();
    let tips = kinds(&ed, "tip");
    let x = |i: EntId| ed.entities[i].world_pos(&ed.doc, &ed.entities)[0];
    t.check("Ctrl+D duplicates, 40 to the side", tips.len() == 2 && (x(tips[1]) - x(tips[0]) - 40.0).abs() < 1e-6);
    ed.remove();
    t.check("Delete removes the copy", kinds(&ed, "tip").len() == 1);

    // Camera zones: the lone trigger volume duplicates with its puzzle; a volume of a puzzle with two doesn't
    let puzzles = |ed: &Editor| ed.doc.kids(ed.root(), "Puzzle").len();
    let zone = ent(&ed, |e| e.zone);
    ed.select(zone, false);
    ed.duplicate();
    t.check("a camera zone duplicates whole", ed.entities.iter().filter(|e| e.zone).count() == 2 && puzzles(&ed) == 3);
    ed.undo();
    let two = ent(&ed, |e| e.kind == "trigger" && !e.zone);
    ed.select(two, false);
    ed.remove();
    t.check("a volume of a two-condition puzzle stays", puzzles(&ed) == 2 && ed.last_status.0.contains("can't be duplicated"));
    let zone = ent(&ed, |e| e.zone).unwrap();
    ed.select(Some(zone), false);
    let znode = ed.entities[zone].node;
    ed.edit(|ed| {
        let vol = camera_zone(&ed.doc, znode).unwrap().vol.unwrap();
        let z = preset_zone("high", 0.0, zone_center(&ed.doc, vol)).unwrap();
        set_camera_zone(&mut ed.doc, znode, &z);
    });
    let z = camera_zone(&ed.doc, ed.entities[ent(&ed, |e| e.zone).unwrap()].node).unwrap();
    t.check("a preset rewrites the zone's actions", z.height == Some(320.0) && z.distance == Some(140.0) && z.look.is_none());
    ed.add_camera_zone("fixed");
    let sel = ed.entities[ed.selected().unwrap()].node;
    t.check("Add camera zone adds a puzzle with a fly-to point", camera_zone(&ed.doc, sel).unwrap().fly.is_some());

    // Shift+click drops a garib on the ground
    ed.set_selection(Vec::new());
    ed.frame_level();
    let n = kinds(&ed, "garib").len();
    click(&mut ed, ((w / 2.0).floor(), (h * 0.75).trunc()), Mods { shift: true, ..none });
    t.check("Shift+click drops a garib", kinds(&ed, "garib").len() == n + 1);

    // The palette
    let n = ed.doc.kids(ed.root(), "Platform").len();
    ed.add_preset(PRESETS[2].key);
    t.detail("clicking a preset's Add adds it", ed.doc.kids(ed.root(), "Platform").len() > n, ed.last_status.0.clone());
    ed.undo();
    ed.add_model(SIGN, "LandActor");
    t.check("a model from another bank is an import", ed.current_imports().contains(&SIGN.to_string()));
    t.check("the import plan runs", ed.plan.as_ref().is_some_and(|p| p.objects.contains(&SIGN.to_string())));
    ed.add_enemy("dennis");
    t.check("Add enemy places one", kinds(&ed, "enemy").len() == 2);
    ed.add_powerup(7, "BOWLINGBALL");
    ed.add_tip();
    t.check("Add powerup and Mr. Tip", kinds(&ed, "pickup").len() == 2 && kinds(&ed, "tip").len() == 2);

    // The path tool
    ed.set_selection(Vec::new());
    ed.set_path_mode(true);
    for x in [0.3, 0.5, 0.7] {
        click(&mut ed, ((w * x).trunc(), (h * 0.7).trunc()), none);
    }
    t.detail("clicks lay a path", ed.path.pts.len() == 3, ed.path.pts.len().to_string());
    let n = kinds(&ed, "garib").len();
    ed.place_along_path("garib", "garib", 60.0, 0.0, 0.0, 12.0, false);
    let added = kinds(&ed, "garib").len() - n;
    t.detail("Place puts garibs along it", added >= 3 && ed.selection.len() == added, added.to_string());
    // Camera zones over the path: boxes 140 to the sides, 300 below and 400 above
    let nz = puzzles(&ed);
    ed.path_zones("high", 0.0, 1.0, 140.0, 300.0, 400.0, 0.0);
    let pz = ed.doc.kids(ed.root(), "Puzzle");
    let vol = ed.doc.iter_tags(*pz.last().unwrap(), &["PuzzleCondGloverWithinVolume"]).first().copied().filter(|_| pz.len() > nz);
    let pts = ed.path.pts.clone();
    let low = |k: usize| pts.iter().map(|p| p[k]).fold(f64::INFINITY, f64::min);
    let high = |k: usize| pts.iter().map(|p| p[k]).fold(f64::NEG_INFINITY, f64::max);
    t.check(
        "a camera zone fits the path",
        vol.is_some_and(|v| (num0(&ed.doc, v, "x") - (low(0) - 140.0)).abs() <= 21.0 && (num0(&ed.doc, v, "w") - (high(1) - low(1) + 700.0)).abs() <= 21.0),
    );
    let mut acts: Vec<&str> = ed.doc.kids(*pz.last().unwrap(), "PuzzleAction").iter().map(|a| ed.doc.attr(*a, "action_type")).collect();
    acts.sort();
    t.check("the zone has the preset's actions", acts == ["70", "71"]);
    ed.undo();
    ed.path_zones("side", 0.0, 1.0, 140.0, 300.0, 400.0, 100.0);
    t.check("a long stretch is split into boxes", puzzles(&ed) - nz >= 2);
    ed.undo();
    // Scatter: the same seed gives the same result, and the children come first
    let land = ent(&ed, |e| e.tag == "BackgroundActor").or(ent(&ed, |e| e.tag == "LandActor")).unwrap();
    let (name, tag) = (ed.doc.attr(ed.entities[land].node, "name").to_string(), ed.entities[land].tag.clone());
    ed.set_selection(vec![land]);
    let before = ed.serialize();
    let o = Scatter { spacing: (150.0, 200.0), seed: 3, ..Scatter::default() };
    ed.scatter_along_path(&o);
    let one = ed.serialize();
    ed.undo();
    let again = ent(&ed, |e| e.tag == tag && ed.doc.attr(e.node, "name") == name);
    ed.set_selection(again.into_iter().collect());
    ed.scatter_along_path(&o);
    t.check("scatter with the same seed gives the same level", ed.serialize() == one && one != before);
    let copy = ed.top_level().into_iter().filter(|n| ed.doc.tag(*n) == "BackgroundActor" && ed.doc.first(*n, "SetActorRotation").is_some()).last();
    t.check("a scattered copy is turned with SetActorRotation first", copy.is_some_and(|c| tags(&ed, c)[0] == "SetActorRotation"));
    t.detail("the status line gives the heap cost", ed.last_status.0.contains("KB of heap"), ed.last_status.0.clone());
    ed.undo();
    ed.set_path_mode(false);
    t.check("Esc leaves the path tool", !ed.path.on);

    // Layers
    ed.set_layer("trigger", false);
    t.check("hiding a layer hides it", kinds(&ed, "trigger").iter().all(|&i| !ed.graph[ed.nodes[i].obj].visible));
    ed.set_layer("trigger", true);

    // Autosave and restore
    ed.flush_draft();
    t.check("edits are autosaved", ed.drafts.load("10").is_some());
    let xml = ed.serialize();
    ed.load_scene("11").unwrap();
    ed.load_scene("10").unwrap();
    t.detail("the draft comes back", ed.serialize() == xml && ed.last_status.0.contains("Restored your edits"), ed.last_status.0.clone());
    ed.reset_level();
    t.check("Reset level goes back to the shipped level and drops the draft", !ed.is_edited() && ed.drafts.load("10").is_none());
    ed.undo();
    t.check("undo brings the edits back after a reset", ed.serialize() == xml);

    // Building: the fixture assembles .lev files
    let out = ed.build("lev", false);
    t.detail(".lev builds and the schema accepts the edits", out.as_ref().is_ok_and(|o| o.data.len() > 100 && o.name.ends_with(".lev")), format!("{:?}", out.as_ref().err()));
    let out = ed.build("xml", false).unwrap();
    let model = FormatModel::shared();
    let res = sanitize_level_xml(std::str::from_utf8(&out.data).unwrap(), model, "file");
    t.detail("Export XML passes the import checks", res.ok, format!("{:?}", res.errors));
    t.check("ROM builds say the fixture has no ROM", ed.build("rom", false).is_err_and(|e| e.contains("no ROM")));

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
    ed.confirm_import(&ok);
    t.check("an import is one undo step", ed.serialize() == xml && !ed.undo_stack.is_empty());

    // The far plane and the checks' click targets
    let dl: Vec<_> = ed.checks().into_iter().filter(|c| c.text.starts_with("Draw load")).collect();
    t.check("the draw load check is there", dl.len() == 1);
    ed.click_check(dl[0].click.as_ref().unwrap());
    t.check("clicking it shows the far plane", ed.far_sphere.is_some());

    // Rules from the decomp: a bad trigger volume
    let vol = ed.doc.iter_tags(ed.root(), &["PuzzleCondBallWithinRangeOfPoint"])[0];
    ed.set_attr(vol, "range", "0.0");
    t.check("a zero-range trigger is flagged", has_check(&mut ed, None, "range = 0"));
    ed.undo();

    // Inspector edits
    let tip = ent(&ed, |e| e.kind == "tip").unwrap();
    ed.select(Some(tip), false);
    ed.set_attr(ed.entities[tip].node, "message_id", "5");
    t.check("an inspector field edits the XML", ed.doc.attr(ed.entities[ent(&ed, |e| e.kind == "tip").unwrap()].node, "message_id") == "5");

    // Commands inside elements: add, refuse a second one, move, remove; the schema check
    let spin = ed.entities[ent(&ed, |e| e.label == "spinner").unwrap()].node;
    ed.select(ent(&ed, |e| e.label == "spinner"), false);
    let allowed = glovebox_editor::commands::Format::shared().children_of("Platform");
    t.check("the Add command menu lists platform commands", ["PlatSpecial0xb9", "SetExit", "PlatCheckpoint"].iter().all(|c| allowed.contains(&c.to_string())));
    ed.add_command(spin, "PlatSpecial0xb9", &[]);
    ed.add_command(spin, "SetActorRotation", &[]);
    let tg = tags(&ed, spin);
    t.detail("SetActorRotation goes first, others last", tg[0] == "SetActorRotation" && tg[tg.len() - 1] == "PlatSpecial0xb9", format!("{tg:?}"));
    let n = ed.undo_stack.len();
    ed.add_command(spin, "PlatSpecial0xb9", &[]);
    t.check("a second one is refused", ed.undo_stack.len() == n && ed.last_status.0.contains("already has"));
    t.check("the schema accepts the commands", backend.validate(&ed.serialize()).is_empty());
    let sp = ed.doc.first(spin, "PlatSpecial0xb9").unwrap();
    ed.move_node(sp, -1);
    let tg = tags(&ed, spin);
    t.detail("↑ moves a command earlier", tg.iter().position(|x| x == "PlatSpecial0xb9") == Some(tg.len() - 2), format!("{tg:?}"));
    ed.remove_node(sp);
    t.check("× removes it", ed.doc.first(spin, "PlatSpecial0xb9").is_none());
    let enemy = ed.entities[ent(&ed, |e| e.kind == "enemy").unwrap()].node;
    ed.add_command(enemy, "SetActorScale", &[]);
    t.check("nothing goes after EnemyFinalize", tags(&ed, enemy).last().map(|s| s.as_str()) == Some("EnemyFinalize"));
    let end = ed.end_marker().unwrap();
    ed.remove_node(end);
    t.check("EndLevelData can't be removed", ed.end_marker().is_some());
    let top = ed.top_level();
    ed.move_node(top[top.len() - 2], 1);
    t.check("nothing moves past EndLevelData", tags(&ed, ed.root()).last().map(|s| s.as_str()) == Some("EndLevelData"));
    let bad = ed.doc.first(spin, "PlatConstantSpin").unwrap();
    ed.set_attr(bad, "axis", "w");
    t.check("a value the format refuses shows in the checks", has_check(&mut ed, None, "libgarib's format"));
    ed.undo();

    // The order the game reads a platform in, and what a command needs first
    let mark = ed.undo_stack.len();
    let spin = ed.entities[ent(&ed, |e| e.label == "spinner").unwrap()].node;
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
    t.detail("PlatScale goes before the platform's position", tags(&ed, plat)[..2] == ["PlatScale", "PlatSetInitialPos"], format!("{:?}", tags(&ed, plat)));
    let sc = ed.doc.first(plat, "PlatScale").unwrap();
    t.check("a new scale is 1, not 0", ed.doc.attr(sc, "x") == "1.0");
    ed.move_node(ed.doc.children(plat)[1], -1);
    t.detail("a move the game would read wrong is refused", ed.doc.children(plat)[0] == sc && ed.last_status.0.contains("Can't move"), ed.last_status.0.clone());
    let n = ed.undo_stack.len();
    ed.add_command(plat, "PlatSpinPause0x7c", &[]);
    t.check("a spin pause without a spin is refused", ed.undo_stack.len() == n && ed.last_status.0.contains("spin"));
    ed.add_command(plat, "PlatConstantSpin", &[]);
    ed.add_command(plat, "PlatSetTag", &[]);
    ed.add_command(plat, "PlatSpinPause0x7c", &[]);
    let tg = tags(&ed, plat);
    let at = |x: &str| tg.iter().position(|y| y == x);
    t.detail("a spin pause goes right after its spin", at("PlatSpinPause0x7c") == at("PlatConstantSpin").map(|i| i + 1), format!("{tg:?}"));
    let n = ed.undo_stack.len();
    ed.add_command(plat, "PlatRocking", &[]);
    t.check("a second motion is refused", ed.undo_stack.len() == n);
    ed.remove_node(ed.doc.first(plat, "PlatConstantSpin").unwrap());
    t.check("removing a spin removes its pause", ed.doc.first(plat, "PlatSpinPause0x7c").is_none() && ed.doc.first(plat, "PlatSetTag").is_some());
    for _ in 0..20 {
        ed.add_command(plat, "PlatPathPoint", &[]);
    }
    let n = ed.undo_stack.len();
    ed.add_command(plat, "PlatPathPoint", &[]);
    let pts = ed.doc.kids(plat, "PlatPathPoint");
    t.detail("a 21st path point is refused", pts.len() == 20 && ed.undo_stack.len() == n, pts.len().to_string());
    t.check("path points follow each other 100 apart", num0(&ed.doc, pts[1], "x") - num0(&ed.doc, pts[0], "x") == 100.0);
    // Tidy order puts back an order made by hand
    for p in pts {
        ed.doc.detach(p);
    }
    ed.doc.append(plat, sc);
    ed.tidy_node(plat);
    t.check("Tidy order moves the scale back before the position", tags(&ed, plat)[0] == "PlatScale");
    let order = ed.checks().into_iter().find(|c| c.level == "err" && c.text.contains("order"));
    t.detail("a tidy platform has no order errors", order.is_none(), format!("{:?}", order.map(|c| c.text)));

    // Enemies keep a normal instruction and their EnemyFinalize; empty groups go
    let enemy = ed.entities[ent(&ed, |e| e.kind == "enemy").unwrap()].node;
    t.check("EnemyFinalize can't be removed", ed.removal_refused(ed.doc.first(enemy, "EnemyFinalize").unwrap()).is_some());
    let kept = ["FogConfiguration", "AmbientLight"].iter().all(|x| ed.doc.first(ed.root(), x).is_none_or(|n| ed.removal_refused(n).is_some()));
    t.check("fog and ambient light can't be removed", kept);
    ed.add_enemy("unknown-type");
    let new = *ed.doc.kids(ed.root(), "Enemy").last().unwrap();
    t.check("an enemy with no shipped script gets a rest instruction", tags(&ed, new) == ["NormalInstructions", "EnemyFinalize"]);
    let ins = ed.doc.first(ed.doc.first(new, "NormalInstructions").unwrap(), "EnemyInstruction").unwrap();
    t.check("its only normal instruction can't be removed", ed.removal_refused(ins).is_some());
    let grp = ed.doc.make("AttackInstructions", &[]);
    let copy = ed.doc.deep_copy(ins);
    ed.doc.append(grp, copy);
    let fin = ed.doc.first(new, "EnemyFinalize").unwrap();
    ed.doc.add_prev(fin, grp);
    ed.remove_node(copy);
    t.check("removing a group's last instruction removes the group", ed.doc.first(new, "AttackInstructions").is_none());
    undo_to(&mut ed, mark);

    // The puzzle editor: a new puzzle, a condition and an action, their types, the checks
    let mark = ed.undo_stack.len();
    ed.add_puzzle();
    let p = ed.puzzle_shown();
    t.check("a new puzzle opens in the inspector", p.is_some() && ed.puzzle_index == Some(puzzles(&ed) - 1));
    let p = p.unwrap();
    ed.add_puzzle_part(p, "PuzzleCond", 16);
    ed.add_puzzle_part(p, "PuzzleAction", 54);
    let p = ed.puzzle_shown().unwrap();
    t.check("conditions go before actions, after the limit", tags(&ed, p) == ["PuzzleNumtimes", "PuzzleCond", "PuzzleAction"]);
    let b = body(&ed.doc, ed.doc.first(p, "PuzzleAction").unwrap()).unwrap();
    t.check("float fields are written as bits", ed.doc.attr(b, "hide_enabled") == "0x3F800000");
    let cond = ed.doc.first(p, "PuzzleCond").unwrap();
    ed.set_part_type(cond, 33);
    t.check(
        "changing a condition's type replaces its body",
        ed.doc.tag(body(&ed.doc, cond).unwrap()) == "PuzzleCondSpecificEnemyExists" && ed.doc.attr(cond, "cond_type") == "33",
    );
    ed.set_logic(p, "least", 3);
    t.check("a puzzle that needs more conditions than it has is an error", has_check(&mut ed, Some("err"), "never fires"));
    ed.set_logic(p, "all", 0);
    ed.set_fires(p, None);
    t.check("no limit removes PuzzleNumtimes", ed.doc.first(p, "PuzzleNumtimes").is_none());
    t.check("the schema accepts the puzzle", backend.validate(&ed.serialize()).is_empty());
    ed.doc.set(cond, "cond_type", "35");
    t.check("a body that doesn't match its type is an error", has_check(&mut ed, None, "expects a <PuzzleCondGloverWithinVolume>"));
    // The checks of what a puzzle reads through a platform's pointers
    ed.add_puzzle_part(p, "PuzzleAction", 45);
    let a = *ed.doc.kids(p, "PuzzleAction").last().unwrap();
    ed.set_attr(body(&ed.doc, a).unwrap(), "platform_tag", "-201");
    t.check("a negative nudge tag with no platform is an error", has_check(&mut ed, Some("err"), "crashes when there is none"));
    undo_to(&mut ed, mark);
    ed.open_puzzle(None);

    // Enemy scripts: a ready-made script, moving and removing with Goto targets that follow
    let mark = ed.undo_stack.len();
    let enemy = ent(&ed, |e| e.kind == "enemy").unwrap();
    let en = ed.entities[enemy].node;
    ed.select(Some(enemy), false);
    ed.add_script_template(en, "Chase the glove");
    let codes = |ed: &Editor| -> Vec<i128> { scripts::list_of(&ed.doc, en, "NormalInstructions").iter().map(|r| scripts::code(&ed.doc, *r)).collect() };
    t.detail("a ready-made script replaces the normal list", codes(&ed) == [4, 9, 18], format!("{:?}", codes(&ed)));
    let flags = ed.flag_enum();
    t.check("the ready-made script passes the script checks", scripts::rules(&ed.doc, en, flags.as_ref()).is_empty());
    ed.add_instruction(en, "NormalInstructions", 2);
    ed.move_instruction(scripts::list_of(&ed.doc, en, "NormalInstructions")[3], -1);
    let rows = scripts::list_of(&ed.doc, en, "NormalInstructions");
    t.check(
        "moving an instruction keeps Goto pointing at the same one",
        codes(&ed) == [4, 9, 2, 18] && ed.doc.attr(body(&ed.doc, rows[3]).unwrap(), "instr_idx") == "1",
    );
    ed.remove_instruction(rows[0]);
    let rows = scripts::list_of(&ed.doc, en, "NormalInstructions");
    t.check("removing an instruction shifts later Goto targets", ed.doc.attr(body(&ed.doc, *rows.last().unwrap()).unwrap(), "instr_idx") == "0");
    ed.add_instruction(en, "ConditionalInstructions", 8);
    let tg = tags(&ed, en);
    t.check("a new kind gets its group before EnemyFinalize", tg[tg.len() - 2..] == ["ConditionalInstructions", "EnemyFinalize"]);
    let c0 = scripts::list_of(&ed.doc, en, "ConditionalInstructions")[0];
    t.check("Face the player gets a target", scripts::flags_value(ed.doc.get(c0, "flags"), flags.as_ref()) & 0x1C0 != 0);
    ed.set_instruction_type(*scripts::list_of(&ed.doc, en, "NormalInstructions").last().unwrap(), 9);
    t.check("a list that can run off its end is an error", has_check(&mut ed, Some("err"), "reads past the list"));
    t.check("the schema accepts the script", backend.validate(&ed.serialize()).is_empty());
    let before = codes(&ed);
    ed.move_instruction(scripts::list_of(&ed.doc, en, "NormalInstructions")[0], 1);
    let after = codes(&ed);
    t.check("clicking ↓ on an instruction moves it", after[..2] == [before[1], before[0]]);
    undo_to(&mut ed, mark);

    // Level settings: a look preset, the file name check
    let mark = ed.undo_stack.len();
    ed.apply_look("Graveyard violet");
    let fog = ed.doc.first(ed.root(), "FogConfiguration").unwrap();
    t.check("a look preset sets the fog and lights", ed.doc.attr(fog, "near_clip") == "1150" && ed.doc.kids(ed.root(), "DiffuseLight").len() == 2);
    ed.set_attr(ed.root(), "name", "LEVEL1");
    t.check("an even-length file name is an error", has_check(&mut ed, Some("err"), "odd length"));
    undo_to(&mut ed, mark);
    ed.set_level_info(|i| i.name = Some("MY LEVEL".to_string()));
    ed.set_level_info(|i| i.song = Some(17));
    let info = ed.level_info();
    t.check("the level's title and music are kept", info.name.as_deref() == Some("MY LEVEL") && info.song == Some(17) && info.bonus_time.is_none() && !info.extra);
    t.check("Build all lists the level", ed.project_levels().iter().any(|l| l.0 == "10"));
    ed.set_level_info(|i| i.name = None);
    ed.set_level_info(|i| i.song = None);

    // Platform presets: each one is valid, and adds no problem of its own
    let mark = ed.undo_stack.len();
    let base: Vec<String> = ed.checks().into_iter().filter(|c| c.level == "err").map(|c| c.text).collect();
    for p in PRESETS {
        ed.add_preset(p.key);
    }
    let errs: Vec<String> = ed
        .checks()
        .into_iter()
        .filter(|c| c.level == "err" && !base.contains(&c.text) && !c.text.contains("isn't in any object bank"))
        .map(|c| c.text)
        .collect();
    t.detail("every platform preset passes the checks", errs.is_empty(), format!("{errs:?}"));
    let names = ["hoop.nd", "heart", "ballbit", "lift", "glovebu", "stepy.n", "ootelep"];
    let n = ed.doc.iter_tags(ed.root(), &["Platform"]).iter().filter(|n| names.contains(&ed.doc.attr(**n, "name"))).count();
    t.detail("the presets add their platforms", n >= 16, n.to_string());
    undo_to(&mut ed, mark);

    let _ = std::fs::remove_dir_all(&home);
    let failed: Vec<&(String, bool, String)> = t.0.iter().filter(|c| !c.1).collect();
    println!("{} checks, {} failed", t.0.len(), failed.len());
    assert!(failed.is_empty(), "{failed:#?}");
}

/// The debounce: with delays, the draft and the plan wait for `tick`.
#[test]
fn timers_and_jobs() {
    let home = std::env::temp_dir().join(format!("glovebox-editor-tick-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let backend: Arc<dyn Backend> = Arc::new(FixtureBackend::new());
    let mut ed = Editor::new(backend.clone(), Drafts::new(home.join("drafts")), None);
    ed.start();
    ed.set_catalog(backend.catalog().unwrap());
    ed.set_view_size(800.0, 600.0);
    // A level read on another thread
    let job = ed.level_job("10");
    let data = std::thread::spawn(move || job.run()).join().unwrap().unwrap();
    ed.apply_level(data).unwrap();
    ed.delays = Some(Default::default());
    assert!(ed.tick(10.0).is_none());
    ed.drain_events();
    ed.add_model(SIGN, "LandActor");
    assert!(ed.plan.is_none() && ed.drafts.load("10").is_none());
    assert!(ed.tick(10.1).is_none());
    let job = ed.tick(10.3).expect("the plan is due after 250 ms");
    assert!(ed.drafts.load("10").is_none());
    ed.tick(10.5);
    assert!(ed.drafts.load("10").is_some());
    let res = std::thread::spawn(move || job.run()).join().unwrap();
    ed.apply_plan(res);
    assert!(ed.plan.is_some() && ed.import_budget().is_some());
    let events = ed.drain_events();
    assert!(events.contains(&glovebox_editor::editor::Event::ImportsChanged));
    // A plan that an edit overtook is dropped
    ed.add_tip();
    let stale = ed.tick(20.0).unwrap();
    ed.undo();
    ed.undo();
    ed.apply_plan(stale.run());
    assert!(ed.plan.is_none());
    // A build job runs away from the editor
    let job = ed.build_job("lev", false).unwrap();
    let out = std::thread::spawn(move || job.run()).join().unwrap().unwrap();
    assert!(out.message.starts_with("Built 10.FIX10lnd.n64.lev"));
    let _ = std::fs::remove_dir_all(&home);
}

/// The exit triggers on the fixture: the sphere of the platform "goal", the layer and the check.
#[test]
fn exit_triggers_on_the_fixture() {
    use glovebox_editor::exits::{Who, Zero, LAYER};
    let home = std::env::temp_dir().join(format!("glovebox-exit-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let backend: Arc<dyn Backend> = Arc::new(FixtureBackend::new());
    let mut ed = Editor::new(backend.clone(), Drafts::new(home.join("drafts")), Some(Box::new(MemorySettings::default())));
    ed.start();
    ed.set_catalog(backend.catalog().unwrap());
    ed.set_view_size(1100.0, 800.0);
    ed.load_scene("10").unwrap();
    let goal = ent(&ed, |e| e.label == "goal").unwrap();
    assert_eq!(ed.exits(), [goal]);
    // The block is a cube of 100. At a scale of 0.3 its corners are 15 * sqrt(3) from the middle.
    let t = ed.exit_trigger(goal).unwrap();
    let corner = f64::from((3.0f32 * 15.0 * 15.0).sqrt());
    assert_eq!((t.exit_type, t.center, t.rule.who, t.zero.clone()), (1, [380.0, 120.0, -60.0], Who::Ball, None));
    assert!((t.radius.unwrap() - corner).abs() < 1e-4, "{:?}", t.radius);
    assert!(!t.dead() && !has_check(&mut ed, None, "can never fire"));
    // The other platforms have no exit and no sphere
    let lift = ent(&ed, |e| e.label == "lift").unwrap();
    assert!(ed.exit_trigger(lift).is_none() && ed.nodes[lift].exit.is_none());

    // The sphere: in the world at the platform, as large as the radius, outside the entity's box
    let sphere = ed.nodes[goal].exit.unwrap();
    let node = &ed.graph[sphere];
    assert!(node.visible && node.pos == t.center && (node.scale[0] - corner).abs() < 1e-4);
    let (obj_box, sphere_box) = (ed.obj_box(goal), ed.graph.bounds(sphere));
    assert!(sphere_box.max[0] - sphere_box.min[0] > obj_box.max[0] - obj_box.min[0] + 10.0);
    // A click on the sphere, beside the block, selects what is behind it: nothing here, or the ground
    let none = Mods::default();
    let across = vec::cross(ed.camera.forward(), [0.0, 1.0, 0.0]);
    let beside = px(&ed, vec::add(t.center, vec::scale(across, (corner - 3.0) / vec::norm(across))));
    click(&mut ed, beside, none);
    assert!(!ed.selection.contains(&goal), "{:?}", ed.selection);
    let on = px(&ed, t.center);
    click(&mut ed, on, none);
    assert_eq!(ed.selection, [goal]);

    // It follows a move and a scale
    ed.apply_transform(Some([20.0, 0.0, 0.0]), None, 0, None);
    assert_eq!(ed.graph[ed.nodes[goal].exit.unwrap()].pos, [400.0, 120.0, -60.0]);
    ed.apply_transform(None, None, 0, Some(2.0));
    let goal = ed.selection[0];
    let big = ed.exit_trigger(goal).unwrap().radius.unwrap();
    assert!((big - 2.0 * corner).abs() < 1e-3 && (ed.graph[ed.nodes[goal].exit.unwrap()].scale[1] - big).abs() < 1e-9);

    // The layer hides the sphere and keeps the platform. The setting is remembered.
    ed.set_layer(LAYER, false);
    let n = &ed.nodes[goal];
    assert!(!ed.graph[n.exit.unwrap()].visible && ed.graph[n.obj].visible && ed.selection == [goal]);
    assert_eq!(ed.settings.as_ref().unwrap().value("exits").as_deref(), Some("off"));
    ed.set_layer(LAYER, true);
    assert!(ed.graph[ed.nodes[goal].exit.unwrap()].visible);
    assert_eq!(ed.settings.as_ref().unwrap().value("exits").as_deref(), Some("on"));
    // Hiding the platforms hides their spheres too
    ed.set_layer("platform", false);
    assert!(!ed.graph[ed.nodes[goal].exit.unwrap()].visible);
    ed.set_layer("platform", true);

    // The loading-zone SetExit before the position: the game never measures the platform
    let node = ed.entities[goal].node;
    ed.edit(|ed| {
        let exit = ed.doc.first(node, "SetExit").unwrap();
        ed.doc.detach(exit);
        ed.doc.insert(node, 0, exit);
    });
    let goal = ent(&ed, |e| e.label == "goal").unwrap();
    let t = ed.exit_trigger(goal).unwrap();
    assert_eq!((t.radius, t.zero.clone()), (Some(0.0), Some(Zero::Before("The loading-zone SetExit"))));
    assert!(t.dead() && !ed.graph[ed.nodes[goal].exit.unwrap()].visible);
    assert!(has_check(&mut ed, Some("err"), "can never fire"));
    assert!(ed.issues_for(goal).iter().any(|l| l.0 == "err" && l.1.contains("can never fire")));
    ed.undo();
    let goal = ent(&ed, |e| e.label == "goal").unwrap();
    assert!(ed.graph[ed.nodes[goal].exit.unwrap()].visible && !has_check(&mut ed, None, "can never fire"));
    // Exit type 3 tests no distance: no sphere, and no check
    let exit = ed.doc.first(ed.entities[goal].node, "SetExit").unwrap();
    ed.set_attr(exit, "behavior", "3");
    let goal = ent(&ed, |e| e.label == "goal").unwrap();
    assert!(ed.exits() == [goal] && !ed.graph[ed.nodes[goal].exit.unwrap()].visible && !ed.nodes[goal].exit_on);
    // Exit type 0 is no exit to the game
    let exit = ed.doc.first(ed.entities[goal].node, "SetExit").unwrap();
    ed.set_attr(exit, "behavior", "0");
    assert!(ed.exits().is_empty() && ed.nodes.iter().all(|n| n.exit.is_none()));
    let _ = std::fs::remove_dir_all(&home);
}
