//! Level packs on the real game: every shipped level's own XML must make a pack.
//!
//! `GLOVEBOX_ROM` is the ROM's path. Without it the test prints that it was skipped and passes.

use glovebox_core::backend::{Backend, GameBackend};
use glovebox_core::{custom, gamedata, objbank, pack, rom};
use std::collections::HashSet;
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

/// The cube drawn from scratch (`tests/data/cube.gltf`) gives the same banks with the game's ids taken as without
/// them: its ids are in none of the game's banks. `gbtool modelbanks` converts it without the ROM.
#[test]
fn the_cube_clashes_with_no_game_id() {
    let Ok(rom) = std::env::var("GLOVEBOX_ROM") else {
        println!("the_cube_clashes_with_no_game_id: skipped (GLOVEBOX_ROM is not set)");
        return;
    };
    let rom_bytes = std::fs::read(&rom).expect("the ROM reads");
    // The object ids come from every object bank in the ROM map. The catalog holds only the banks the scenes
    // name, and it misses SYSTEM and TITLE_BANK.
    let map = rom::Rom::parse(&rom_bytes).expect("the ROM splits");
    let mut taken_ids: HashSet<u32> = HashSet::new();
    let mut object_banks = 0;
    for r in map.regions().iter().filter(|r| r.data_type() == "objbank") {
        let name = r.name().unwrap_or("?");
        let packed = r.bytes().unwrap_or_else(|| panic!("object bank {name} is one block"));
        let singles = objbank::split(packed).unwrap_or_else(|e| panic!("object bank {name}: {e}"));
        taken_ids.extend(singles.iter().map(|s| s.id));
        object_banks += 1;
    }
    let names: HashSet<&str> = map.regions().iter().filter(|r| r.data_type() == "objbank").filter_map(|r| r.name()).collect();
    assert!(names.contains("SYSTEM") && names.contains("TITLE_BANK"), "the map has SYSTEM and TITLE_BANK");
    println!("the_cube_clashes_with_no_game_id: {object_banks} object banks, {} object ids", taken_ids.len());
    let game = gamedata::GameData::open(rom_bytes).expect("the ROM opens");
    assert!(game.catalog().keys().all(|id| taken_ids.contains(id)), "the catalog's ids are among the map's");
    let mut taken_textures: HashSet<u32> = HashSet::new();
    for bank in game.all_texture_banks() {
        taken_textures.extend(game.texture_ids(bank));
    }
    let cube = include_bytes!("data/cube.gltf");
    let banks = |ids: &HashSet<u32>, textures: &HashSet<u32>| {
        let set = custom::convert(cube, "cube", 1.0, ids, textures).unwrap();
        let pieces: Vec<_> = set.pieces.iter().collect();
        custom::pack::model_banks(&pieces, &[&set.texture_bank]).unwrap()
    };
    let set = custom::convert(cube, "cube", 1.0, &HashSet::new(), &HashSet::new()).unwrap();
    for p in &set.pieces {
        assert!(!taken_ids.contains(&p.id), "object 0x{:08X} is the game's", p.id);
    }
    for (t, _) in &set.textures {
        assert!(!taken_textures.contains(t), "texture 0x{t:08X} is the game's");
    }
    assert_eq!(banks(&taken_ids, &taken_textures), banks(&HashSet::new(), &HashSet::new()));
}
