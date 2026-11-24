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
//! The pack's id is the first 16 bytes of the BLAKE2b hash (32 bytes of output) of: the text `g64pack 1` and a zero
//! byte, then for `pack.txt` and `level.lev` in that order the file's name and a zero byte, its length (4 bytes,
//! little-endian) and its bytes. g64coopdx computes the same, and a host refuses a player whose pack has another id.

use crate::level;
use blake2::digest::{Update, VariableOutput};
use blake2::Blake2bVar;
use std::path::Path;

/// The most characters of a pack's name, and the most bytes of its level file.
pub const NAME_MAX: usize = 32;
pub const LEVEL_MAX: usize = 1024 * 1024;
/// The game has 48 levels.
pub const LEVELS: usize = 48;

/// A level pack's two files and its id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LevelPack {
    pub manifest: Vec<u8>,
    pub level: Vec<u8>,
    pub id: [u8; 16],
}

/// The id of a pack with these two files.
pub fn pack_id(manifest: &[u8], level: &[u8]) -> [u8; 16] {
    let mut h = Blake2bVar::new(32).expect("32 bytes is a valid BLAKE2b output size");
    h.update(b"g64pack 1\0");
    for (name, data) in [(&b"pack.txt\0"[..], manifest), (&b"level.lev\0"[..], level)] {
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
    let id = pack_id(&manifest, &lev);
    Ok(LevelPack { manifest, level: lev, id })
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

impl LevelPack {
    /// The id as text: 32 hex digits.
    pub fn id_hex(&self) -> String {
        self.id.iter().map(|b| format!("{:02x}", b)).collect()
    }

    /// Writes the pack's two files into `dir`, which is made if it is not there.
    pub fn write_to(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        std::fs::write(dir.join("pack.txt"), &self.manifest)?;
        std::fs::write(dir.join("level.lev"), &self.level)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The same two files as g64coopdx's own test (tests/net/pack_test.c), which prints the first 8 bytes of the
    /// id it computes in C: both programs must agree.
    #[test]
    fn id_matches_g64coopdx() {
        let manifest = b"format=1\nname=A test level\nlevel=10\n";
        let level = [0u8, 0, 0, 12, b't', b'e', b's', b't', b'.', b'l', b'n', 0];
        let id = pack_id(manifest, &level);
        assert_eq!(id[..8], [0xde, 0x99, 0x04, 0x4c, 0xa5, 0x59, 0x5d, 0xc7]);
        assert_ne!(pack_id(manifest, &level[..11]), id);
        assert_ne!(pack_id(&manifest[..manifest.len() - 1], &level), id);
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
                assert_eq!(pack.id, pack_id(&pack.manifest, &pack.level));
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
