//! Level packs for g64coopdx, the online co-op game on PC: a folder with a manifest and one level file. The level
//! takes the place of one of the game's levels for a session. A pack holds no game data: the level file names
//! models and textures by their ids, and each player's own ROM supplies them.
//!
//! The format is g64coopdx's (its docs/discovery/level-pack.md):
//!
//! - `pack.txt`: lines of `key=value` that end with a line feed alone: `format=1`, `name=` (1 to 32 printable ASCII
//!   characters) and `level=` (the number of the level whose place the pack's level takes, 0 to 47).
//! - `level.lev`: the level in the game's own format. It starts with its size.
//!
//! - `models.bin`, optional: the pack's own models and the numbers of the game's banks it needs (its
//!   docs/discovery/model-banks.md).
//!
//! The pack's id is the first 16 bytes of the BLAKE2b hash (32 bytes of output) of: the text `g64pack 1` and a zero
//! byte, then for each file the pack has, in the order `pack.txt`, `level.lev`, `script.lua`, `models.bin`, the
//! file's name and a zero byte, its length (4 bytes, little-endian) and its bytes. g64coopdx computes the same
//! (`g64_pack_make_id` in its src/client/pack.c), and a host refuses a player whose pack has another id.

use crate::gamedata::{hex, GameData};
use crate::imports::{self, Customs, Plan};
use crate::level;
use crate::objbank;
use blake2::digest::{Update, VariableOutput};
use blake2::Blake2bVar;
use std::collections::BTreeSet;
use std::path::Path;

/// The most characters of a pack's name, and the most bytes of its level file.
pub const NAME_MAX: usize = 32;
pub const LEVEL_MAX: usize = 1024 * 1024;
/// The game has 48 levels.
pub const LEVELS: usize = 48;

/// A level pack's files and its id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LevelPack {
    pub manifest: Vec<u8>,
    pub level: Vec<u8>,
    /// `models.bin`, when the level needs models that its own banks lack
    pub models: Option<Vec<u8>>,
    pub id: [u8; 16],
}

/// The id of a pack with these files. `script` and `models` are the optional `script.lua` and `models.bin`.
pub fn pack_id(manifest: &[u8], level: &[u8], script: Option<&[u8]>, models: Option<&[u8]>) -> [u8; 16] {
    let mut h = Blake2bVar::new(32).expect("32 bytes is a valid BLAKE2b output size");
    h.update(b"g64pack 1\0");
    let files = [
        (&b"pack.txt\0"[..], Some(manifest)),
        (&b"level.lev\0"[..], Some(level)),
        (&b"script.lua\0"[..], script),
        (&b"models.bin\0"[..], models),
    ];
    for (name, data) in files {
        let Some(data) = data else { continue };
        h.update(name);
        h.update(&(data.len() as u32).to_le_bytes());
        h.update(data);
    }
    let mut out = [0u8; 32];
    h.finalize_variable(&mut out).expect("the buffer has the output's size");
    let mut id = [0u8; 16];
    id.copy_from_slice(&out[..16]);
    id
}

/// The manifest of a pack with this name, whose level takes the place of level number `level_index`.
pub fn manifest(name: &str, level_index: usize) -> Result<Vec<u8>, String> {
    if name.is_empty() || name.len() > NAME_MAX || !name.bytes().all(|c| (32..=126).contains(&c)) {
        return Err(format!("A pack's name has 1 to {} characters: letters, digits, spaces and signs of plain ASCII", NAME_MAX));
    }
    if level_index >= LEVELS {
        return Err(format!("The game has no level {}", level_index));
    }
    Ok(format!("format=1\nname={}\nlevel={}\n", name, level_index).into_bytes())
}

/// Makes a pack from a level's XML. `level_index` is the number of the level whose place it takes: the level the
/// XML was made for, whose model and texture banks the game loads with it.
///
/// A level that needs the console's game mod is refused: g64coopdx does not have the mod's features yet.
pub fn make_pack(name: &str, level_index: usize, xml: &[u8]) -> Result<LevelPack, String> {
    let manifest = manifest(name, level_index)?;
    let (lev, needs_mod) = level::assemble_xml(xml).map_err(|e| e.to_string())?;
    if needs_mod {
        return Err("This level uses features of the console's game mod, which g64coopdx does not have yet. A level pack cannot hold it".into());
    }
    check_level(&lev)?;
    let id = pack_id(&manifest, &lev, None, None);
    Ok(LevelPack { manifest, level: lev, models: None, id })
}

/// A level file as g64coopdx takes it: it starts with its own size, and is 1 MB at most.
pub fn check_level(lev: &[u8]) -> Result<(), String> {
    if lev.len() < 8 || lev.len() > LEVEL_MAX {
        return Err("The level file is too short or too long for a level pack".into());
    }
    let size = u32::from_be_bytes([lev[0], lev[1], lev[2], lev[3]]) as usize;
    if size != lev.len() {
        return Err("The level file does not start with its own size".into());
    }
    Ok(())
}

/// The most bytes of a pack's own models: its texture bank and its object bank together.
pub const MODELS_MAX: usize = 4 << 20;

/// The most banks of the game's of each kind that `models.bin` may name.
pub const RETAIL_MAX: usize = 4;

/// The game's banks that `models.bin` may not name: every level loads them already. The font and generic texture
/// banks, and object banks 0 (system) and 1 (generic). g64coopdx refuses them (`retail_ok` in its bankcheck.c).
pub const NOT_RETAIL_TEXTURES: [u8; 2] = [10, 2];
pub const NOT_RETAIL_OBJECTS: [u8; 2] = [0, 1];

/// `models.bin`, the optional file of a pack's models, in g64coopdx's layout (its docs/discovery/model-banks.md,
/// "The file"). All words are big-endian.
///
/// - 0: the size T of the pack's texture bank. 4: the size O of its object bank.
/// - 8: the flags. Bit 0, `first`: the game's texture banks named here win over the level's own on a texture id
///   that both have. Then the number of the game's texture banks named, the number of its object banks, a zero.
/// - 12: the numbers of those texture banks, then of those object banks, one byte each.
/// - Zeros up to the next multiple of 8, then the texture bank, then the object bank.
///
/// Both of the pack's banks are empty or neither is. Each is a multiple of 8 bytes, and together they are at most
/// `MODELS_MAX`.
pub fn models_bin(first: bool, retail_textures: &[u8], retail_objects: &[u8], textures: &[u8], objects: &[u8]) -> Result<Vec<u8>, String> {
    if textures.is_empty() != objects.is_empty() {
        return Err("A pack's models need both a texture bank and an object bank".into());
    }
    if !textures.len().is_multiple_of(8) || !objects.len().is_multiple_of(8) {
        return Err("A bank in a pack's models must be a multiple of 8 bytes".into());
    }
    if textures.len() + objects.len() > MODELS_MAX {
        return Err(format!(
            "The pack's own models are {} bytes. A level pack holds at most 4 MiB ({} bytes) of models",
            textures.len() + objects.len(),
            MODELS_MAX
        ));
    }
    if retail_textures.len() > RETAIL_MAX || retail_objects.len() > RETAIL_MAX {
        return Err(format!(
            "The level needs {} texture banks and {} object banks of the game's that it does not load itself. A level pack names at most {} of each",
            retail_textures.len(),
            retail_objects.len(),
            RETAIL_MAX
        ));
    }
    if let Some(n) = retail_textures.iter().find(|n| NOT_RETAIL_TEXTURES.contains(n)) {
        return Err(format!("A level pack cannot name texture bank {n}: every level loads the font and generic texture banks"));
    }
    if let Some(n) = retail_objects.iter().find(|n| NOT_RETAIL_OBJECTS.contains(n)) {
        return Err(format!("A level pack cannot name object bank {n}: every level loads object banks 0 and 1"));
    }
    for list in [retail_textures, retail_objects] {
        let unique: BTreeSet<&u8> = list.iter().collect();
        if unique.len() != list.len() {
            return Err("A level pack names one of the game's banks twice".into());
        }
    }
    if first && retail_textures.is_empty() {
        return Err("The flag for the game's texture banks needs a texture bank of the game's".into());
    }
    let head = 12 + retail_textures.len() + retail_objects.len();
    let mut out = vec![0u8; head.next_multiple_of(8)];
    out[0..4].copy_from_slice(&(textures.len() as u32).to_be_bytes());
    out[4..8].copy_from_slice(&(objects.len() as u32).to_be_bytes());
    out[8] = first as u8;
    out[9] = retail_textures.len() as u8;
    out[10] = retail_objects.len() as u8;
    out[12..head].copy_from_slice(&[retail_textures, retail_objects].concat());
    out.extend_from_slice(textures);
    out.extend_from_slice(objects);
    Ok(out)
}

/// The level file's commands that name an object, and where in their fields the object's id is. These are the
/// tokens g64coopdx reads (`g64_lev_object_ids` in its src/client/pack.c): PLATFORM, BACKGROUND, LANDSCAPE and
/// CAMEO_OBJ (Glovebox's AnimatedBackgroundActor) at 0, SINEBRIDGE (PlatSine) at 4, DESTRUCTIBLE (its debris) and
/// ROPEBRIDGE (Rope) at 6, ORBITBRIDGE (PlatOrbit) at 8, WATER at 38.
const ID_TOKENS: [(u32, usize); 9] = [(98, 0), (145, 0), (146, 0), (188, 0), (144, 4), (101, 6), (141, 6), (143, 8), (160, 38)];
const DESTRUCTIBLE: u32 = 101;
/// A cameo actor (CAMEO_OBJ) is animated at once. A pack's own model has no animation.
pub const CAMEO: u32 = 188;

/// Each object id that the level file names, with the command that names it, in file order. A DESTRUCTIBLE whose
/// debris id is 0 has no debris and is left out.
pub fn object_refs(lev: &[u8]) -> Result<Vec<(u32, u32)>, String> {
    let mut out = Vec::new();
    for (code, at) in level::commands(lev).map_err(|e| e.to_string())? {
        let Some((_, off)) = ID_TOKENS.iter().find(|(c, _)| *c == code) else { continue };
        let b = lev.get(at + off..at + off + 4).ok_or("A command of the level file ends before its object id")?;
        let id = u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
        if code == DESTRUCTIBLE && id == 0 {
            continue;
        }
        out.push((code, id));
    }
    Ok(out)
}

/// Ids that the game finds enemies, bullets and pick-ups by, which the retail ROM lacks. A pack's object may not
/// have one: it would be that enemy or pick-up. Copied from `g64_models_game_ids` in g64coopdx's
/// src/client/bankcheck.c.
pub const GAME_NAME_IDS: [u32; 6] = [0x5e9f46bf, 0xf90dc11e, 0x61af1e01, 0x0ee2f4e2, 0x571f7f95, 0x8516ac66];

/// The names of the ball's and the hand's objects. The game points the ball's and the hand's textures at an
/// object with one of these names, so a pack's object may not have one (g64coopdx's bankcheck.c).
pub const RESERVED_NAMES: [&str; 5] = ["gball", "cball", "GShinL", "GShinR", "gbl_bt"];

/// Makes a pack from a level's XML with the models it imports. `sid` is the level whose place it takes.
///
/// The imports are the object ids of the level file that the level's own banks lack (`GameData::loaded_ids`).
/// `imports::plan_runtime` sorts them, as for a ROM with the game mod: imported models go into the pack's own
/// banks, and the game's other banks go into `models.bin` by number, for each player's game to load from their own
/// ROM. A pack never copies the game's models. An id in no bank of the game's and in no imported model is an
/// error.
pub fn export(gd: &GameData, customs: &Customs, sid: &str, name: &str, xml: &[u8]) -> Result<LevelPack, String> {
    let scene = crate::gamedata::scene(sid).map_err(|e| e.0)?;
    let mut pack = make_pack(name, scene.index, xml)?;
    let refs = object_refs(&pack.level)?;
    let loaded = gd.loaded_ids(sid).map_err(|e| e.0)?;
    let mut ids: Vec<String> = Vec::new();
    for (_, id) in &refs {
        let h = hex(*id);
        if !loaded.contains(id) && !ids.contains(&h) {
            ids.push(h);
        }
    }
    if ids.is_empty() {
        return Ok(pack);
    }
    let plan = imports::plan_runtime(gd, customs, sid, &ids)?;
    pack.models = pack_models(&plan, customs, &refs)?;
    pack.id = pack_id(&pack.manifest, &pack.level, None, pack.models.as_deref());
    Ok(pack)
}

/// `models.bin` for a plan of `imports::plan_runtime`, or None when the plan needs no models. `refs` are the
/// level's object ids (`object_refs`). This part needs no ROM.
pub fn pack_models(plan: &Plan, customs: &Customs, refs: &[(u32, u32)]) -> Result<Option<Vec<u8>>, String> {
    let copied: &[String] = plan.copy.as_ref().map_or(&[], |c| c.objects.as_slice());
    let (tex_numbers, obj_numbers) = match &plan.runtime {
        Some(r) => (r.texture_numbers.clone(), r.object_numbers.clone()),
        None => (Vec::new(), Vec::new()),
    };
    if copied.is_empty() && tex_numbers.is_empty() && obj_numbers.is_empty() {
        return Ok(None);
    }
    let byte = |n: &i64| u8::try_from(*n).map_err(|_| format!("The game has no bank {n} that a level pack can name"));
    let tex_numbers: Vec<u8> = tex_numbers.iter().map(byte).collect::<Result<_, _>>()?;
    let obj_numbers: Vec<u8> = obj_numbers.iter().map(byte).collect::<Result<_, _>>()?;
    let mut pieces: Vec<crate::custom::Piece> = Vec::new();
    let mut sets: Vec<&str> = Vec::new();
    for oid in copied {
        let c = customs.get(oid).ok_or_else(|| format!("Object {oid} is not an imported model"))?;
        let id = crate::gamedata::parse_id(oid).ok_or_else(|| format!("{oid} is not an object id"))?;
        if c.reskin_from.is_some() {
            return Err(format!(
                "{} is a copy of one of the game's models with other textures. A level pack cannot hold it: it holds the game's own data",
                c.name
            ));
        }
        if GAME_NAME_IDS.contains(&id) {
            return Err(format!(
                "{} has the id {oid}, which the game uses for an enemy or a pick-up. Import the model again under another file name",
                c.name
            ));
        }
        if refs.iter().any(|(code, r)| *code == CAMEO && *r == id) {
            return Err(format!(
                "A cameo uses the imported model {}. The game animates a cameo, and an imported model has no animation. Use it as a landscape, a background or a platform",
                c.name
            ));
        }
        let bank = objbank::Bank::parse(&c.bank).map_err(|e| format!("{}: {}", c.name, e))?;
        for mesh in bank.objects.iter().flat_map(|o| o.meshes.iter()) {
            if RESERVED_NAMES.contains(&mesh.name) {
                return Err(format!(
                    "{} has a part named {}, the name of the ball or the hand in the game. Rename the model's file or its mesh and import it again",
                    c.name, mesh.name
                ));
            }
        }
        let textures = c.textures.iter().map(|t| crate::gamedata::parse_id(t).ok_or_else(|| format!("{t} is not a texture id")));
        pieces.push(crate::custom::Piece {
            id,
            name: c.piece.clone(),
            offset: c.offset,
            triangles: 0,
            size: [0.0; 3],
            textures: textures.collect::<Result<_, _>>()?,
            bank: c.bank.clone(),
        });
        if !sets.contains(&c.set.as_str()) {
            sets.push(&c.set);
        }
    }
    let (textures, objects) = if pieces.is_empty() {
        (Vec::new(), Vec::new())
    } else {
        let banks = sets.iter().map(|s| imports::custom_tex_bank(customs, s)).collect::<Result<Vec<_>, _>>()?;
        let banks: Vec<&[u8]> = banks.iter().map(|b| b.as_ref()).collect();
        let refs: Vec<&crate::custom::Piece> = pieces.iter().collect();
        crate::custom::pack::model_banks(&refs, &banks).map_err(|e| e.0)?
    };
    // The game mod's builds never set the flag (crates/core/src/build.rs), and neither does a pack.
    models_bin(false, &tex_numbers, &obj_numbers, &textures, &objects).map(Some)
}

impl LevelPack {
    /// The id as text: 32 hex digits.
    pub fn id_hex(&self) -> String {
        self.id.iter().map(|b| format!("{:02x}", b)).collect()
    }

    /// Writes the pack's files into `dir`, which is made if it is not there. g64coopdx finds `models.bin` by its
    /// presence, so an old one is removed when the pack has none.
    pub fn write_to(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        std::fs::write(dir.join("pack.txt"), &self.manifest)?;
        std::fs::write(dir.join("level.lev"), &self.level)?;
        let models = dir.join("models.bin");
        match &self.models {
            Some(m) => std::fs::write(models, m),
            None if models.exists() => std::fs::remove_file(models),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn models_file() {
        let (t, o) = (vec![1u8; 16], vec![2u8; 24]);
        let m = models_bin(false, &[], &[], &t, &o).unwrap();
        assert_eq!(m.len(), 16 + 16 + 24);
        assert_eq!(&m[..16], &[0, 0, 0, 16, 0, 0, 0, 24, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(&m[16..32], &t[..]);
        assert_eq!(&m[32..], &o[..]);
        assert_eq!(models_bin(false, &[], &[], &[], &[]).unwrap(), vec![0u8; 16]);
        assert!(models_bin(false, &[], &[], &t, &[]).is_err());
        assert!(models_bin(false, &[], &[], &[0; 12], &o).is_err());
        assert!(models_bin(false, &[], &[], &vec![0; MODELS_MAX], &o).is_err());
    }

    #[test]
    fn models_file_with_the_games_banks() {
        // One bank of the game's and none of the pack's own: 13 bytes of header, 16 in all.
        let m = models_bin(false, &[], &[9], &[], &[]).unwrap();
        assert_eq!(m, [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 9, 0, 0, 0]);
        // Four of each kind: 20 bytes of header, then the banks at 24.
        let (t, o) = (vec![1u8; 8], vec![2u8; 8]);
        let m = models_bin(true, &[3, 4, 5, 6], &[7, 8, 9, 11], &t, &o).unwrap();
        assert_eq!(&m[..24], &[0, 0, 0, 8, 0, 0, 0, 8, 1, 4, 4, 0, 3, 4, 5, 6, 7, 8, 9, 11, 0, 0, 0, 0]);
        assert_eq!(&m[24..32], &t[..]);
        assert_eq!(&m[32..], &o[..]);
        // Two texture banks and one object bank: 15 bytes of header.
        assert_eq!(models_bin(false, &[3, 4], &[9], &[], &[]).unwrap().len(), 16);
        // Five of a kind, a bank that every level loads, a bank named twice, the flag with no texture bank
        assert!(models_bin(false, &[], &[3, 4, 5, 6, 7], &[], &[]).is_err());
        assert!(models_bin(false, &[3, 4, 5, 6, 7], &[], &[], &[]).is_err());
        assert!(models_bin(false, &[2], &[], &[], &[]).is_err());
        assert!(models_bin(false, &[10], &[], &[], &[]).is_err());
        assert!(models_bin(false, &[], &[0], &[], &[]).is_err());
        assert!(models_bin(false, &[], &[1], &[], &[]).is_err());
        assert!(models_bin(false, &[], &[9, 9], &[], &[]).is_err());
        assert!(models_bin(true, &[], &[9], &[], &[]).is_err());
    }

    /// The same two files as g64coopdx's own test (tests/net/pack_test.c), which prints the first 8 bytes of the
    /// id it computes in C: both programs must agree.
    #[test]
    fn id_matches_g64coopdx() {
        let manifest = b"format=1\nname=A test level\nlevel=10\n";
        let level = [0u8, 0, 0, 12, b't', b'e', b's', b't', b'.', b'l', b'n', 0];
        let id = pack_id(manifest, &level, None, None);
        assert_eq!(id[..8], [0xde, 0x99, 0x04, 0x4c, 0xa5, 0x59, 0x5d, 0xc7]);
        assert_ne!(pack_id(manifest, &level[..11], None, None), id);
        assert_ne!(pack_id(&manifest[..manifest.len() - 1], &level, None, None), id);

        // The same manifest and level with a script and the cube's models.bin. g64coopdx keeps the cube's file as the
        // fixture of its bank check (tests/fixtures/cube.models.bin), and this test makes the same bytes. The ids are
        // the ones that g64coopdx's pack tool prints for these files (`g64pack make DIR --name "A test level"
        // --level 10 --lev LEV [--script FILE] --models FILE`).
        let script = b"print(\"hi\")\n";
        let cube = include_bytes!("../tests/data/cube.gltf");
        let set = crate::custom::convert(cube, "cube", 1.0, &Default::default(), &Default::default()).unwrap();
        let pieces: Vec<_> = set.pieces.iter().collect();
        let (t, o) = crate::custom::pack::model_banks(&pieces, &[&set.texture_bank]).unwrap();
        let models = models_bin(false, &[], &[], &t, &o).unwrap();
        assert_eq!(models.len(), 1248);
        let hex = |id: [u8; 16]| id.iter().map(|b| format!("{b:02x}")).collect::<String>();
        assert_eq!(hex(pack_id(manifest, &level, None, Some(&models))), "dfffdef0a1eb5c74ca99df42b2a43eb2");
        assert_eq!(hex(pack_id(manifest, &level, Some(script), Some(&models))), "13a1ae26d488c9f540b5d44cbdecaf94");
    }

    #[test]
    fn manifest_rules() {
        assert_eq!(manifest("A test level", 10).unwrap(), b"format=1\nname=A test level\nlevel=10\n");
        assert!(manifest("", 1).is_err());
        assert!(manifest(&"x".repeat(33), 1).is_err());
        assert!(manifest("tab\there", 1).is_err());
        assert!(manifest("caf\u{e9}", 1).is_err());
        assert!(manifest("ok", 48).is_err());
        assert!(manifest(&"x".repeat(32), 47).is_ok());
    }

    #[test]
    fn level_checks() {
        assert!(check_level(&[0, 0, 0, 12, b't', b'e', b's', b't', b'.', b'l', b'n', 0]).is_ok());
        assert!(check_level(&[0, 0, 0, 13, b't', b'e', b's', b't', b'.', b'l', b'n', 0]).is_err());
        assert!(check_level(&[0, 0, 0, 4]).is_err());
    }

    #[test]
    fn a_pack_from_xml() {
        // The smallest level the format allows: a root with a name and no commands.
        let xml = br#"<Level name="pack.lnd" libgarib-version="a" data-format-version="b"></Level>"#;
        match make_pack("From XML", 3, xml) {
            Ok(pack) => {
                assert!(check_level(&pack.level).is_ok());
                assert_eq!(pack.manifest, b"format=1\nname=From XML\nlevel=3\n");
                assert_eq!(pack.id, pack_id(&pack.manifest, &pack.level, None, None));
                assert_eq!(pack.models, None);
                assert_eq!(pack.id_hex().len(), 32);
                let dir = std::env::temp_dir().join(format!("glovebox-pack-test-{}", std::process::id()));
                pack.write_to(&dir).unwrap();
                assert_eq!(std::fs::read(dir.join("pack.txt")).unwrap(), pack.manifest);
                assert_eq!(std::fs::read(dir.join("level.lev")).unwrap(), pack.level);
                let _ = std::fs::remove_dir_all(&dir);
            }
            Err(e) => panic!("the smallest level did not make a pack: {}", e),
        }
        assert!(make_pack("Bad", 3, b"<NotALevel/>").is_err());
    }
}
