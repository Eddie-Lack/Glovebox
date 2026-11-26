//! The editor's checks on the vanilla levels, with the real game. The test needs `GLOVEBOX_ROM`. Without it, it says
//! that it was skipped, and passes.

use glovebox_core::backend::{Backend, GameBackend};
use glovebox_editor::drafts::Drafts;
use glovebox_editor::editor::Editor;
use std::path::Path;
use std::sync::Arc;

fn draw_load_level(ed: &mut Editor) -> String {
    ed.load_checks().iter().find(|c| c.text.starts_with("Draw load")).map(|c| c.level.to_string()).unwrap_or_default()
}

/// No vanilla level gets a warning or an error for its draw load, also the ones whose estimate is over the limits.
/// A level that is busier than the vanilla one does, also after its draft is restored.
#[test]
fn the_draw_load_of_vanilla_levels() {
    let Ok(rom) = std::env::var("GLOVEBOX_ROM") else {
        println!("skipped: GLOVEBOX_ROM is not set");
        return;
    };
    let home = std::env::temp_dir().join(format!("glovebox-vanilla-rom-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let backend: Arc<dyn Backend> = Arc::new(GameBackend::open(Path::new(&rom), home.clone()).expect("the ROM opens"));
    let mut ed = Editor::new(backend.clone(), Drafts::new(home.join("drafts")), None);
    ed.load_textures = false;
    ed.start();
    ed.set_catalog(backend.catalog().expect("the catalog"));
    ed.set_view_size(1100.0, 800.0);
    let sids: Vec<String> = ed.scenes.iter().map(|s| s["id"].as_str().unwrap().to_string()).collect();
    assert_eq!(sids.len(), 48);
    let mut bad = Vec::new();
    let mut over = 0;
    for sid in &sids {
        ed.load_scene(sid).unwrap_or_else(|e| panic!("{sid}: {e}"));
        let level = draw_load_level(&mut ed);
        let n = ed.draw_load().worst.0;
        println!("{sid}: draw load {n}, {level}");
        if level != "ok" && level != "note" {
            bad.push(format!("{sid}: {n} gives {level}"));
        }
        over += usize::from(level == "note");
        // Glover starts at the first checkpoint, also in a level with a GloverSpawnPoint
        let spawn = ed.entities.iter().any(|e| e.tag == "GloverSpawnPoint");
        let gate = ed.entities.iter().any(|e| e.gate.is_some());
        let marker = ed.entities.iter().any(|e| e.derived && e.label == "Glover start (first checkpoint)");
        if spawn && gate {
            println!("{sid}: a GloverSpawnPoint and a checkpoint");
        }
        if gate != marker {
            bad.push(format!("{sid}: a checkpoint {gate}, the start marker {marker}"));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
    // Hub 1's estimate is 261, over the error's 240
    assert!(over > 0, "no vanilla level is over the warning's limit");
    ed.load_scene("00").unwrap();
    assert!(ed.draw_load().worst.0 >= 240, "hub 1 is not over the error's limit");
    // One more copy of every landscape piece makes hub 1 busier than the vanilla level
    let land: Vec<usize> = (0..ed.entities.len()).filter(|&i| ed.entities[i].kind == "land" && !ed.entities[i].derived).collect();
    ed.set_selection(land);
    ed.duplicate();
    assert_eq!(draw_load_level(&mut ed), "err", "a busier hub 1");
    ed.flush_draft();
    ed.load_scene("01").unwrap();
    ed.load_scene("00").unwrap();
    assert!(ed.draft_saved_at.is_some(), "the draft is restored");
    assert_eq!(draw_load_level(&mut ed), "err", "a busier hub 1 from its draft");
    let _ = std::fs::remove_dir_all(&home);
}

/// No object of a vanilla level is far outside the level (`far_checks`).
#[test]
fn no_vanilla_object_is_far_outside_its_level() {
    let Ok(rom) = std::env::var("GLOVEBOX_ROM") else {
        println!("skipped: GLOVEBOX_ROM is not set");
        return;
    };
    let home = std::env::temp_dir().join(format!("glovebox-vanilla-far-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let backend: Arc<dyn Backend> = Arc::new(GameBackend::open(Path::new(&rom), home.clone()).expect("the ROM opens"));
    let mut ed = Editor::new(backend.clone(), Drafts::new(home.join("drafts")), None);
    ed.load_textures = false;
    ed.start();
    ed.set_catalog(backend.catalog().expect("the catalog"));
    let sids: Vec<String> = ed.scenes.iter().map(|s| s["id"].as_str().unwrap().to_string()).collect();
    let mut far = Vec::new();
    for sid in &sids {
        ed.load_scene(sid).unwrap_or_else(|e| panic!("{sid}: {e}"));
        let (gap, label) = ed.farthest();
        println!("{sid}: the farthest object is {label}, {gap:.2} level sizes out");
        far.extend(ed.checks().into_iter().filter(|c| c.text.contains("far outside the level")).map(|c| format!("{sid}: {}", c.text)));
    }
    let _ = std::fs::remove_dir_all(&home);
    assert!(far.is_empty(), "{far:#?}");
}
