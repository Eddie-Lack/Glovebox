//! Level packs on the real game: every shipped level's own XML must make a pack.
//!
//! `GLOVEBOX_ROM` is the ROM's path. Without it the test prints that it was skipped and passes.

use glovebox_core::backend::{Backend, GameBackend};
use glovebox_core::{gamedata, pack};
use std::path::Path;

#[test]
fn every_shipped_level_makes_a_pack() {
    let Ok(rom) = std::env::var("GLOVEBOX_ROM") else {
        println!("every_shipped_level_makes_a_pack: skipped (GLOVEBOX_ROM is not set)");
        return;
    };
    let home = std::env::temp_dir().join(format!("glovebox-pack-rom-{}", std::process::id()));
    let be = GameBackend::open(Path::new(&rom), home.clone()).expect("the ROM opens");
    let mut made = 0;
    for scene in gamedata::scenes() {
        let xml = be.level_xml(&scene.id).unwrap_or_else(|e| panic!("level {} has no XML: {}", scene.id, e));
        let p = pack::make_pack(&scene.name.chars().filter(char::is_ascii).take(pack::NAME_MAX).collect::<String>(), scene.index, xml.as_bytes())
            .unwrap_or_else(|e| panic!("level {} ({}) does not make a pack: {}", scene.id, scene.name, e));
        // A pack of an unchanged level holds the level file as the game has it: it starts with its own size.
        pack::check_level(&p.level).unwrap();
        assert_eq!(p.id, pack::pack_id(&p.manifest, &p.level));
        made += 1;
    }
    assert_eq!(made, pack::LEVELS, "the game has 48 levels");
    let _ = std::fs::remove_dir_all(&home);
}
