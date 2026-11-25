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
        assert_eq!(p.id, pack::pack_id(&p.manifest, &p.level, None, None));
        // The export with the level's imports finds none in a shipped level: it uses only its own banks.
        let e = be.level_pack(&scene.id, &xml, "Shipped").unwrap_or_else(|e| panic!("level {} does not export: {}", scene.id, e));
        assert_eq!((e.models.as_deref(), e.level.as_slice()), (None, p.level.as_slice()), "level {}", scene.id);
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

/// A level that imports the cube (from a user folder) and a model of another world: the cube goes into the pack's own
/// banks, the other model's banks into models.bin by number.
#[test]
fn a_pack_with_imports() {
    let Ok(rom) = std::env::var("GLOVEBOX_ROM") else {
        println!("a_pack_with_imports: skipped (GLOVEBOX_ROM is not set)");
        return;
    };
    let home = std::env::temp_dir().join(format!("glovebox-pack-imports-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let be = GameBackend::open(Path::new(&rom), home.clone()).expect("the ROM opens");
    let meta = be.import_custom(include_bytes!("data/cube.gltf"), "cube", 1.0).expect("the cube imports");
    let cube = meta["pieces"][0]["id"].as_str().unwrap().to_string();
    let xml = be.level_xml("0a").unwrap();
    let at = xml.find('\n').and_then(|i| xml[i + 1..].find('\n').map(|j| i + j + 2)).unwrap();
    let with = |lines: &str| format!("{}{}{}", &xml[..at], lines, &xml[at..]);
    // 0x45F8316C is a bell of Carnival 1
    let bell = r#"  <LandActor object_id="0x45F8316C" name="bell" x="0.0" y="40.0" z="0.0"/>
"#;
    let land = format!("  <LandActor object_id=\"{cube}\" name=\"cube\" x=\"0.0\" y=\"40.0\" z=\"0.0\"/>\n");
    let p = be.level_pack("0a", &with(&land), "Cube").expect("the cube's pack");
    let m = p.models.as_ref().expect("the pack has models");
    assert_eq!(&m[8..12], &[0, 0, 0, 0], "no bank of the game's");
    assert!(u32::from_be_bytes([m[0], m[1], m[2], m[3]]) > 0 && u32::from_be_bytes([m[4], m[5], m[6], m[7]]) > 0);
    assert_eq!(p.id, pack::pack_id(&p.manifest, &p.level, None, Some(m)));
    let p = be.level_pack("0a", &with(bell), "Bell").expect("the bell's pack");
    let m = p.models.as_ref().expect("the pack names the bell's banks");
    assert_eq!(&m[..8], &[0; 8], "no banks of the pack's own");
    assert!(m[10] == 1, "one object bank of the game's: {m:?}");
    let p = be.level_pack("0a", &with(&format!("{land}{bell}")), "Both").expect("the pack of both");
    assert!(p.models.as_ref().is_some_and(|m| m[10] == 1 && m[0..4] != [0; 4]));
    // 0x23313E06 is a body of Carnival's shared bank, object bank 19. Its own faces need no texture bank of
    // Carnival's, but other objects of the bank have sprites with a texture of Carnival's texture bank (4).
    // g64coopdx refuses the bank without the textures of its sprites, so the export names bank 4 as well.
    let body = r#"  <LandActor object_id="0x23313E06" name="body" x="0.0" y="40.0" z="0.0"/>
"#;
    let p = be.level_pack("0a", &with(body), "Body").expect("the body's pack");
    let m = p.models.as_ref().expect("the pack names the body's banks");
    assert_eq!(&m[9..14], &[1, 1, 0, 4, 19], "texture bank 4 and object bank 19: {m:?}");
    let game = gamedata::GameData::open(std::fs::read(&rom).unwrap()).unwrap();
    let raw = game.raw_bank("CARNIVAL_SHARED.obj.fla").unwrap();
    let bank = objbank::Bank::parse(raw).unwrap();
    let banks = ["FONT_TEX_BANK", "GENERIC_TEX_BANK", "ATLANTIS_TEX_BANK", "CARNIVAL_TEX_BANK"];
    let mut sprites = 0;
    for s in bank.objects.iter().flat_map(|o| o.meshes.iter()).flat_map(|m| m.sprite_list()) {
        sprites += 1;
        let found = banks.iter().any(|b| game.texture_ids(&format!("{b}.tex.fla")).contains(&s.texture_id));
        assert!(found, "sprite texture 0x{:08X} is in no bank the pack has", s.texture_id);
    }
    assert!(sprites > 0, "object bank 19 has sprites");
    // Models of five worlds need five of the game's texture banks, one more than a pack can name.
    let loaded = game.loaded_ids("0a").unwrap();
    let mut worlds: Vec<String> = Vec::new();
    let mut lines = String::new();
    for (id, e) in game.catalog() {
        if worlds.len() == 5 || loaded.contains(id) || worlds.contains(&e.world) || e.world == "ATLANTIS" {
            continue;
        }
        let plan = glovebox_core::imports::plan_runtime(&game, &Default::default(), "0a", &[gamedata::hex(*id)]).unwrap();
        if plan.runtime.is_some_and(|r| !r.texture_numbers.is_empty()) {
            worlds.push(e.world.clone());
            lines += &format!("  <LandActor object_id=\"{}\" name=\"m\" x=\"0.0\" y=\"40.0\" z=\"0.0\"/>\n", gamedata::hex(*id));
        }
    }
    assert_eq!(worlds.len(), 5, "{worlds:?}");
    let e = be.level_pack("0a", &with(&lines), "Five").unwrap_err().0;
    assert!(e.contains("can name 4 at most"), "{e}");
    // The cube as a platform. A console ROM must not draw a converted model as a platform, but g64coopdx can: the
    // pack's object bank has the rotation key of no turn in place of the converter's key of zeros.
    let end = xml.find("  <EndLevelData").expect("the level ends with EndLevelData");
    let plat = format!("  <Platform object_id=\"{cube}\" name=\"cube\">\n    <PlatSetInitialPos x=\"0.0\" y=\"40.0\" z=\"0.0\"/>\n  </Platform>\n");
    let plat_xml = format!("{}{}{}", &xml[..end], plat, &xml[end..]);
    let p = be.level_pack("0a", &plat_xml, "Cube platform").expect("the pack of the cube as a platform");
    let m = p.models.as_ref().expect("the pack has the cube's banks");
    assert!(u32::from_be_bytes([m[0], m[1], m[2], m[3]]) > 0 && u32::from_be_bytes([m[4], m[5], m[6], m[7]]) > 0);
    assert!(pack::object_refs(&p.level).unwrap().contains(&(98, gamedata::parse_id(&cube).unwrap())), "the platform names the cube");
    // GLOVEBOX_PACK_OUT keeps the pack, for a check with g64coopdx's `g64pack check DIR --rom ROM`
    if let Ok(out) = std::env::var("GLOVEBOX_PACK_OUT") {
        p.write_to(Path::new(&out)).expect("the pack is written");
    }
    // A cameo of the cube, and a model that is in no bank
    let cameo = format!("  <AnimatedBackgroundActor object_id=\"{cube}\" name=\"cube\" x=\"0.0\" y=\"40.0\" z=\"0.0\"/>\n");
    assert!(be.level_pack("0a", &with(&cameo), "Cameo").unwrap_err().0.contains("cameo"));
    let none = r#"  <LandActor object_id="0x0BADF00D" name="none" x="0.0" y="40.0" z="0.0"/>
"#;
    assert!(be.level_pack("0a", &with(none), "None").unwrap_err().0.contains("0x0BADF00D"));
    let _ = std::fs::remove_dir_all(&home);
}
