//! The editor against the reference dumps, on the real game.
//!
//! The dumps hold level content. They stay outside the repository. The tests need `GLOVEBOX_ROM` (the ROM),
//! `GLOVEBOX_REFERENCE` (the dump folder) and `GLOVEBOX_REFHOME` (a user folder, for the imported
//! models of its catalog). Without them each test says that it was skipped, and passes.

use glovebox_core::backend::{Backend, GameBackend};
use glovebox_editor::drafts::Drafts;
use glovebox_editor::editor::Editor;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn setup() -> Option<(Arc<dyn Backend>, PathBuf)> {
    let get = |k: &str| std::env::var(k).ok();
    let (Some(rom), Some(reference), Some(home)) = (get("GLOVEBOX_ROM"), get("GLOVEBOX_REFERENCE"), get("GLOVEBOX_REFHOME")) else {
        println!("skipped: GLOVEBOX_ROM, GLOVEBOX_REFERENCE and GLOVEBOX_REFHOME aren't all set");
        return None;
    };
    let backend = GameBackend::open(Path::new(&rom), home).expect("the ROM opens");
    Some((Arc::new(backend), Path::new(&reference).join("editor")))
}

fn read(path: &Path) -> Value {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {}", path.display(), e));
    serde_json::from_str(&text).expect("the dump is JSON")
}

fn editor(backend: &Arc<dyn Backend>, tag: &str) -> (Editor, PathBuf) {
    let home = std::env::temp_dir().join(format!("glovebox-editor-reference-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let mut ed = Editor::new(backend.clone(), Drafts::new(home.join("drafts")), None);
    ed.load_textures = false;
    ed.start();
    ed.set_catalog(backend.catalog().expect("the catalog"));
    ed.set_view_size(1100.0, 800.0);
    (ed, home)
}

fn v3(p: [f64; 3]) -> Value {
    json!([p[0], p[1], p[2]])
}

/// A level's summary, in the layout of the reference dumps.
fn summary(ed: &mut Editor) -> Value {
    let checks: Vec<Value> = ed.checks().iter().map(|c| json!([c.level, c.text, c.sub])).collect();
    let d = ed.draw_load();
    let b = ed.import_budget();
    json!({
        "entities": ed.entities.len(),
        "labels": ed.entities.iter().map(|e| e.label.clone()).collect::<Vec<_>>(),
        "checks": checks,
        "load": {
            "far": d.far, "n": d.worst.0, "at": d.worst.1.map(v3), "total": d.total,
            "spots": d.spots.iter().map(|(p, n)| json!([v3(*p), n])).collect::<Vec<_>>(),
        },
        "ground": ed.ground_checks().iter().map(|c| c.text.clone()).collect::<Vec<_>>(),
        "imports": ed.current_imports(),
        "budget": b.map(|b| json!({
            "rows": b.rows.iter().map(|r| json!([r.sid, r.cost, r.room])).collect::<Vec<_>>(),
            "extra": b.extra,
            "least": [json!(b.least_with_pak.0), json!(b.least_with_pak.2)],
            "over": b.over.iter().map(|r| r.sid.clone()).collect::<Vec<_>>(),
        })),
        "mod_users": ed.mod_users().iter().map(|n| ed.doc.path_of(*n)).collect::<Vec<_>>(),
        "use_mod": ed.use_mod(),
    })
}

/// The keys of two summaries that differ, with both values cut short.
fn differences(got: &Value, want: &Value) -> Vec<String> {
    let mut out = Vec::new();
    for (k, w) in want.as_object().expect("an object") {
        let g = &got[k];
        if g == w {
            continue;
        }
        let detail = match (g.as_array(), w.as_array()) {
            (Some(a), Some(b)) => {
                let i = (0..a.len().max(b.len())).find(|&i| a.get(i) != b.get(i)).unwrap_or(0);
                format!("item {} of {}/{}: {:?} != {:?}", i, a.len(), b.len(), a.get(i), b.get(i))
            }
            _ => format!("{} != {}", g, w),
        };
        out.push(format!("{}: {}", k, detail.chars().take(600).collect::<String>()));
    }
    out
}

/// Every level: the entities, the checks, the draw load and the ground checks are the reference's, and no check fires
/// on a shipped level.
#[test]
fn all_levels_match_the_reference() {
    let Some((backend, dir)) = setup() else { return };
    let want = read(&dir.join("all.json"));
    let (mut ed, home) = editor(&backend, "all");
    let mut bad = Vec::new();
    let sids: Vec<String> = ed.scenes.iter().map(|s| s["id"].as_str().unwrap().to_string()).collect();
    assert_eq!(sids.len(), 48);
    for sid in &sids {
        ed.load_scene(sid).unwrap_or_else(|e| panic!("{sid}: {e}"));
        let got = summary(&mut ed);
        for d in differences(&got, &want[sid]) {
            bad.push(format!("{sid} {d}"));
        }
        let texts: Vec<&str> = got["checks"].as_array().unwrap().iter().map(|c| c[1].as_str().unwrap()).collect();
        if !texts.iter().any(|t| t.starts_with("No new problems")) {
            bad.push(format!("{sid}: a check fires on the shipped level: {texts:?}"));
        }
    }
    let _ = std::fs::remove_dir_all(&home);
    assert!(bad.is_empty(), "{} differences:\n{}", bad.len(), bad.join("\n"));
}

fn path(v: &Value) -> Vec<usize> {
    v.as_array().expect("a path").iter().map(|x| x.as_u64().unwrap() as usize).collect()
}

fn s(v: &Value) -> &str {
    v.as_str().expect("text")
}

fn apply(ed: &mut Editor, op: &Value) {
    let a = |i: usize| &op[i];
    let node = |ed: &Editor, i: usize| ed.doc.node_at(&path(&op[i])).expect("the path names a node");
    match s(a(0)) {
        "select" => ed.set_selection(path(a(1))),
        "focus" => ed.focus_on(a(1).as_u64().unwrap() as usize),
        "transform" => {
            let t = a(1);
            let mv = t["move"].as_array().map(|m| [m[0].as_f64().unwrap(), m[1].as_f64().unwrap(), m[2].as_f64().unwrap()]);
            let axis = ["x", "y", "z"].iter().position(|x| *x == s(&t["axis"])).unwrap();
            ed.apply_transform(mv, t["rot"].as_f64(), axis, t["scale"].as_f64());
        }
        "add_garib" => {
            let p = a(1).as_array().unwrap();
            ed.add_garib([p[0].as_f64().unwrap(), p[1].as_f64().unwrap(), p[2].as_f64().unwrap()], s(a(2)));
        }
        "add_enemy" => ed.add_enemy(s(a(1))),
        "add_model" => ed.add_model(s(a(1)), s(a(2))),
        "add_powerup" => ed.add_powerup(a(1).as_i64().unwrap(), s(a(2))),
        "add_tip" => ed.add_tip(),
        "add_camera_zone" => ed.add_camera_zone(s(a(1))),
        "add_preset" => ed.add_preset(s(a(1))),
        "duplicate" => ed.duplicate(),
        "remove" => ed.remove(),
        "undo" => ed.undo(),
        "redo" => ed.redo(),
        "add_puzzle" => ed.add_puzzle(),
        "add_command" => {
            let n = node(ed, 1);
            ed.add_command(n, s(a(2)), &[]);
        }
        "remove_node" => {
            let n = node(ed, 1);
            ed.remove_node(n);
        }
        "move_node" => {
            let n = node(ed, 1);
            ed.move_node(n, a(2).as_i64().unwrap() as i32);
        }
        "tidy" => {
            let n = node(ed, 1);
            ed.tidy_node(n);
        }
        "add_instruction" => {
            let n = node(ed, 1);
            ed.add_instruction(n, s(a(2)), a(3).as_i64().unwrap().into());
        }
        "script_template" => {
            let n = node(ed, 1);
            ed.add_script_template(n, s(a(2)));
        }
        "move_instruction" => {
            let n = node(ed, 1);
            ed.move_instruction(n, a(2).as_i64().unwrap() as i32);
        }
        "remove_instruction" => {
            let n = node(ed, 1);
            ed.remove_instruction(n);
        }
        "set_instruction_type" => {
            let n = node(ed, 1);
            ed.set_instruction_type(n, a(2).as_i64().unwrap().into());
        }
        "add_part" => {
            let n = node(ed, 1);
            ed.add_puzzle_part(n, s(a(2)), a(3).as_i64().unwrap().into());
        }
        "set_part_type" => {
            let n = node(ed, 1);
            ed.set_part_type(n, a(2).as_i64().unwrap().into());
        }
        "set_logic" => {
            let n = node(ed, 1);
            ed.set_logic(n, s(a(2)), a(3).as_i64().unwrap());
        }
        "set_fires" => {
            let n = node(ed, 1);
            ed.set_fires(n, a(2).as_i64());
        }
        "set_attr" => {
            let n = node(ed, 1);
            ed.set_attr(n, s(a(2)), s(a(3)));
        }
        "apply_look" => ed.apply_look(s(a(1))),
        other => panic!("an operation the test doesn't know: {other}"),
    }
}

/// The first place two texts differ, with some of each around it.
fn first_difference(a: &str, b: &str) -> String {
    let i = a.bytes().zip(b.bytes()).position(|(x, y)| x != y).unwrap_or(a.len().min(b.len()));
    let cut = |t: &str| String::from_utf8_lossy(&t.as_bytes()[i.saturating_sub(200)..(i + 120).min(t.len())]).into_owned();
    format!("at byte {}:\n  got:       {}\n  reference: {}", i, cut(a), cut(b))
}

/// Whether two levels are the same but for the last bits of numbers. Returns how many whole numbers differ by one.
///
/// A point under the view comes from a ray through the camera. The reference ray came from LAPACK's matrix
/// inverse, whose last bits depend on the BLAS build and the CPU. The ray here agrees to about 1e-14, and a point
/// it hits to about 1e-11. A preset writes such a point in full, so its last digits differ. `r1` rounds it to a
/// whole number, and a point that lies on a half (the edge of a model placed at whole numbers) can round either
/// way.
fn near(a: &str, b: &str) -> Option<usize> {
    use glovebox_core::xml::Doc;
    let (a, b) = (Doc::parse(a).ok()?, Doc::parse(b).ok()?);
    let (na, nb) = (a.iter(a.root()), b.iter(b.root()));
    if na.len() != nb.len() {
        return None;
    }
    let mut flips = 0;
    for (x, y) in na.iter().zip(&nb) {
        if a.tag(*x) != b.tag(*y) || a.len(*x) != b.len(*y) || a.attrs(*x).len() != b.attrs(*y).len() {
            return None;
        }
        for ((k, v), (l, w)) in a.attrs(*x).iter().zip(b.attrs(*y)) {
            if k != l {
                return None;
            }
            if v == w {
                continue;
            }
            let (p, q): (f64, f64) = (v.parse().ok()?, w.parse().ok()?);
            if (p - q).abs() <= 1e-9 * p.abs().max(1.0) {
                continue;
            }
            if p.fract() == 0.0 && q.fract() == 0.0 && (p - q).abs() == 1.0 {
                flips += 1;
                continue;
            }
            return None;
        }
    }
    Some(flips)
}

/// Goes on from the reference level, with the same selection.
fn adopt(ed: &mut Editor, xml: &str) {
    let keys: Vec<(Vec<usize>, Option<String>)> = ed
        .selection
        .iter()
        .map(|&i| {
            let e = &ed.entities[i];
            (ed.doc.path_of(e.sub.unwrap_or(e.node)), e.is_handle.then(|| e.label.clone()))
        })
        .collect();
    assert!(ed.restore(xml));
    ed.build_entities();
    let mut sel = Vec::new();
    for (p, handle) in keys {
        let n = ed.doc.node_at(&p);
        sel.extend(ed.find(|e| !e.derived && Some(e.sub.unwrap_or(e.node)) == n && e.is_handle.then(|| e.label.clone()) == handle));
    }
    ed.set_selection(sel);
}

/// The recorded edits on real levels: the same XML text, selection and status line after each, and the same
/// checks, draw load, imports and budget at the end.
#[test]
fn recorded_edits_match_the_reference() {
    let Some((backend, dir)) = setup() else { return };
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir).expect("the dump folder").filter_map(|e| e.ok()).map(|e| e.path()).collect();
    files.retain(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("ops_") && n.ends_with(".json")));
    files.sort();
    assert!(!files.is_empty(), "no ops_*.json in {}", dir.display());
    let mut bad = Vec::new();
    let mut noisy: Vec<String> = Vec::new();
    for file in &files {
        let want = read(file);
        let sid = s(&want["sid"]);
        let (mut ed, home) = editor(&backend, sid);
        ed.load_scene(sid).unwrap();
        let steps = want["steps"].as_array().unwrap();
        for (i, step) in steps.iter().enumerate() {
            apply(&mut ed, &step["op"]);
            let xml = ed.serialize();
            let at = format!("{} step {} {}", sid, i, step["op"]);
            if xml != s(&step["xml"]) {
                // See `near`: only what comes from a ray through the view may differ, and only in its last bits
                let from_view = matches!(s(&step["op"][0]), "add_model" | "add_enemy" | "add_tip" | "add_powerup" | "add_camera_zone" | "add_preset" | "undo" | "redo");
                match near(&xml, s(&step["xml"])).filter(|_| from_view) {
                    Some(flips) => {
                        noisy.push(format!("{at} ({flips} rounded the other way)"));
                        adopt(&mut ed, s(&step["xml"]));
                    }
                    None => {
                        bad.push(format!("{at}: XML {}", first_difference(&xml, s(&step["xml"]))));
                        break; // the later steps start from another level
                    }
                }
            }
            if ed.selection != path(&step["sel"]) {
                bad.push(format!("{at}: selection {:?} != {}", ed.selection, step["sel"]));
            }
            if ed.undo_stack.len() as u64 != step["undo"].as_u64().unwrap() {
                bad.push(format!("{at}: undo steps {} != {}", ed.undo_stack.len(), step["undo"]));
            }
            if ed.last_status.0 != s(&step["status"][0]) || ed.last_status.1 != s(&step["status"][1]) {
                bad.push(format!("{at}: status {:?} != {}", ed.last_status, step["status"]));
            }
        }
        if bad.is_empty() {
            for d in differences(&summary(&mut ed), &want["end"]) {
                bad.push(format!("{sid} end {d}"));
            }
        }
        println!("{}: {} steps", sid, steps.len());
        let _ = std::fs::remove_dir_all(&home);
    }
    println!("{} steps differ from the reference only in the last bits of a point under the view:\n{}", noisy.len(), noisy.join("\n"));
    assert!(bad.is_empty(), "{} differences:\n{}", bad.len(), bad.iter().take(25).cloned().collect::<Vec<_>>().join("\n"));
}
