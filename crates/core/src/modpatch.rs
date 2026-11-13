//! Adds the Expansion Pak game mod (`mod/mod.json` and `mod/mod.bin`) to a built ROM, and the two small patches
//! every built ROM can get: Easy's controls and the rows of the level table. The
//! reference tests hold the ROM bytes and the error texts.
//!
//! `apply_mod` checks every word it replaces, writes the loader and the hooks into the game's code, stores the mod
//! at 12 MiB (past anything the ROM builder writes), and recomputes the header CRCs. The mod needs the Expansion
//! Pak; without one the loader undoes the hooks. The result is checked against the rules of real hardware
//! (`hwcheck`).

use crate::hwcheck;
use crate::n64;
use crate::tables;
use serde_json::Value;
use std::collections::BTreeMap;

/// Why a ROM can't be patched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModError(pub String);

impl std::fmt::Display for ModError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ModError {}

fn err<T>(msg: impl Into<String>) -> Result<T, ModError> {
    Err(ModError(msg.into()))
}

fn bad_json<T>() -> Result<T, ModError> {
    err("mod/mod.json is incomplete; run tools/build_mod.py")
}

fn hex(v: &Value) -> Result<usize, ModError> {
    let s = v.as_str().unwrap_or("");
    let s = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    usize::from_str_radix(s, 16).or_else(|_| bad_json())
}

fn hex_bytes(v: &Value) -> Result<Vec<u8>, ModError> {
    let s = v.as_str().unwrap_or("").as_bytes();
    if s.len() % 2 != 0 || s.is_empty() {
        return bad_json();
    }
    s.chunks(2)
        .map(|c| std::str::from_utf8(c).ok().and_then(|t| u8::from_str_radix(t, 16).ok()).map_or_else(bad_json, Ok))
        .collect()
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// A string in quotes for an error text, with the quote and the control characters escaped. Characters outside
/// ASCII are kept as they are.
fn py_repr(s: &str) -> String {
    let quote = if s.contains('\'') && !s.contains('"') { '"' } else { '\'' };
    let mut out = String::new();
    out.push(quote);
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || (0x7F..0xA1).contains(&(c as u32)) || c as u32 == 0xAD => out.push_str(&format!("\\x{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

/// Where the mod has one of its tables: the offset and the size.
fn table_place(info: &Value, name: &str) -> Option<(usize, usize)> {
    let t = info.get(name)?;
    Some((t["offset"].as_u64()? as usize, t["size"].as_u64()? as usize))
}

/// Writes `table` over the mod's bytes at `offset`.
fn splice(blob: &mut [u8], offset: usize, table: &[u8]) -> Result<(), ModError> {
    n64::write_bytes(blob, offset, table).or_else(|_| err("mod/mod.bin is too short for its tables; run tools/build_mod.py"))
}

// ---------------------------------------------------------------------------------------------
// Extra banks

/// The game's bank numbers the mod loads from the ROM before a level is parsed (mod/banks.c).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExtraBanks {
    pub textures: Vec<i64>,
    pub objects: Vec<i64>,
    /// The extra banks win on a texture id that is also in the level's own banks.
    pub first: bool,
}

/// modExtraBanks (mod/banks.c): per level, the level number, the counts, then the bank numbers. 0xFF ends it. With
/// `first`, bit 7 of the level number is set.
pub fn extra_banks_table(extra_banks: &BTreeMap<i64, ExtraBanks>, size: usize) -> Result<Vec<u8>, ModError> {
    let mut table = Vec::new();
    for (&level, v) in extra_banks {
        let numbers_fit = v.textures.iter().chain(&v.objects).all(|&n| (0..0xFF).contains(&n));
        if !(0..0x7F).contains(&level) || v.textures.len() > 8 || v.objects.len() > 16 || !numbers_fit {
            return err(format!("level {}: too many extra banks (8 texture and 16 object banks at most)", level));
        }
        table.push(level as u8 | if v.first { 0x80 } else { 0 });
        table.push(v.textures.len() as u8);
        table.push(v.objects.len() as u8);
        table.extend(v.textures.iter().chain(&v.objects).map(|&n| n as u8));
    }
    if table.len() + 1 > size {
        return err(format!("the extra bank list is {} bytes; the mod has room for {}", table.len() + 1, size));
    }
    table.push(0xFF);
    Ok(table)
}

/// What the mod stores behind itself starts this far past its own start. The mod's code is under 128 KiB.
pub const STORE: usize = 0x20000;

// ---------------------------------------------------------------------------------------------
// Easy's controls and the level table

/// Ball walking (ProcessInput, gloveworld src/controller.c): on Medium and Hard the stick pushes the ball the other
/// way from Easy. A `bnez` skips Easy's branch when the save slot's difficulty is not Easy. A nop in its place gives
/// every difficulty Easy's direction. Nothing else about the difficulty changes.
const EASY_CONTROLS_ROM: usize = 0x3A6D0;
const EASY_CONTROLS_RAM: &str = "0x801396D0";
const EASY_CONTROLS_OLD: [u8; 4] = [0x14, 0x60, 0x00, 0x15];

/// levelInfo (gloveworld include/game.h): 48 rows of 0x38 bytes in the game's uncompressed data. A row holds the
/// name (0x28 bytes, shown in the level select), objBank, bit, sfxDist, a byte, the song (+0x35) and the bonus
/// clock in seconds (+0x36, -1 for none).
pub const LEVEL_INFO: usize = 0xE7910;
pub const LEVEL_ROW: usize = 0x38;

/// New values for one row of the level table. None keeps the shipped value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LevelInfo {
    pub name: Option<String>,
    /// The low 8 bits are stored.
    pub song: Option<i64>,
    pub bonus_time: Option<i16>,
}

/// The header CRCs, as libgarib computes them: it first looks which boot code the ROM has.
fn fix_crc(out: &mut [u8]) -> Result<(), ModError> {
    match n64::identify_cic(out) {
        Some(6101) | Some(6102) => {}
        _ => return err("the ROM does not have the boot code of the retail Glover (USA) ROM"),
    }
    n64::fix_header_crc(out).map(|_| ()).or_else(|_| err("the ROM is too short to hold the game's code"))
}

/// The ROM with these rows of the level table changed, in the order given. Refuses a ROM whose table isn't where
/// it should be.
pub fn apply_level_info(rom: &[u8], rows: &[(i64, LevelInfo)]) -> Result<Vec<u8>, ModError> {
    let mut out = rom.to_vec();
    let starts = |i: usize, text: &[u8]| n64::bytes_at(rom, LEVEL_INFO + LEVEL_ROW * i, text.len()).map_or(false, |b| b == text);
    if !starts(0, b"HUB 1\0") || !starts(41, b"SPACE BONUS\0") {
        return err("the ROM's level table isn't where the retail Glover (USA) ROM has it");
    }
    let short = |_| ModError("the ROM ends inside its level table".to_string());
    for (slot, info) in rows {
        if !(0..48).contains(slot) {
            return err(format!("no level slot {}", slot));
        }
        let at = LEVEL_INFO + LEVEL_ROW * *slot as usize;
        if let Some(name) = &info.name {
            if !name.is_ascii() {
                return err(format!("level name {} isn't plain ASCII", py_repr(name)));
            }
            if name.len() > 0x27 {
                return err(format!("a level name is {} characters; 39 fit", name.len()));
            }
            let mut field = [0u8; 0x28];
            field[..name.len()].copy_from_slice(name.as_bytes());
            n64::write_bytes(&mut out, at, &field).map_err(short)?;
        }
        if let Some(song) = info.song {
            n64::write_u8(&mut out, at + 0x35, (song & 0xFF) as u8).map_err(short)?;
        }
        if let Some(time) = info.bonus_time {
            n64::write_u16(&mut out, at + 0x36, time as u16).map_err(short)?;
        }
    }
    Ok(out)
}

/// The ROM with Easy's ball-walking controls on every difficulty, and its header CRCs recomputed.
pub fn apply_easy_controls(rom: &[u8]) -> Result<Vec<u8>, ModError> {
    let mut out = rom.to_vec();
    if n64::bytes_at(rom, EASY_CONTROLS_ROM, 4).ok() != Some(&EASY_CONTROLS_OLD[..]) {
        return err(format!("the ROM's code at {} isn't the difficulty check the controls patch expects", EASY_CONTROLS_RAM));
    }
    out[EASY_CONTROLS_ROM..EASY_CONTROLS_ROM + 4].fill(0);
    fix_crc(&mut out)?;
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// Custom models

/// Generated models the mod loads for one key (the key of the extra banks).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CustomBanks {
    pub textures: Vec<u8>,
    pub objects: Vec<u8>,
}

fn pad16(store: &mut Vec<u8>) {
    store.resize((store.len() + 15) & !15, 0);
}

/// modCustom (mod/banks.c): the magic, the count, then per entry the key and the cart addresses of a texture bank
/// and an object bank. Returns the table and the bytes to store at `base`. Each bank starts on a 16 byte boundary
/// and its end address is rounded up to 16: every DMA the game makes for it then has an even length that is a
/// multiple of 8 (the PI's rules, docs/technical.md, "Real hardware"). The padding is zeros.
pub fn custom_table(entries: &BTreeMap<u8, CustomBanks>, base: usize, size: usize) -> Result<(Vec<u8>, Vec<u8>), ModError> {
    if entries.len() > 8 {
        return err("more than 8 custom bank sets");
    }
    let mut out = Vec::new();
    out.extend_from_slice(&0x43555331u32.to_be_bytes());
    out.extend_from_slice(&(entries.len() as u32).to_be_bytes());
    let mut store = Vec::new();
    let address = |n: usize| u32::try_from(n).or_else(|_| err("the custom banks do not fit in a ROM"));
    for (&key, banks) in entries {
        let ts = base + store.len();
        store.extend_from_slice(&banks.textures);
        pad16(&mut store);
        let te = base + store.len();
        let os = te;
        store.extend_from_slice(&banks.objects);
        pad16(&mut store);
        let oe = base + store.len();
        let (ts, te) = if banks.textures.is_empty() { (0, 0) } else { (address(ts)?, address(te)?) };
        let (os, oe) = if banks.objects.is_empty() { (0, 0) } else { (address(os)?, address(oe)?) };
        out.extend_from_slice(&[key, 0, 0, 0]);
        for word in [ts, te, os, oe] {
            out.extend_from_slice(&word.to_be_bytes());
        }
    }
    if out.len() > size {
        return err(format!("the custom model table is {} bytes; the mod has {}", out.len(), size));
    }
    out.resize(size, 0);
    Ok((out, store))
}

// ---------------------------------------------------------------------------------------------
// Extra level slots (mod/slots.c): levels stored behind the mod in the Space slots 35 to 41

pub const SLOTS_FIRST: i64 = 35;
pub const SLOTS_COUNT: i64 = 7;
/// modExtraBanks and modCustom entries of an extra level are keyed with its slot + SLOT_KEY.
pub const SLOT_KEY: i64 = 0x40;
const NAME_LEN: usize = 0x28;
/// Mod_LoadLandscapeFile reads at most 256 KiB.
pub const MAX_LEVEL_FILE: usize = 0x40000;
pub const ANY_HUB: u8 = 0xFF;

/// One extra level.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SlotLevel {
    /// The level file, in the game's size-prefixed format.
    pub lev: Vec<u8>,
    pub name: Option<String>,
    /// None keeps the slot's retail song.
    pub song: Option<u8>,
    /// None keeps the slot's retail bonus clock.
    pub bonus_time: Option<i16>,
}

/// The extra levels of a ROM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slots {
    /// By slot, 35 to 41.
    pub levels: BTreeMap<i64, SlotLevel>,
    /// Boot into the extra level of the slot the quickboot patch names.
    pub boot: bool,
    /// The hub (0 to 7) whose entry leads to the extra levels, or `ANY_HUB`.
    pub entry_hub: u8,
    /// The tag of the entry platform.
    pub entry_tag: u16,
}

impl Default for Slots {
    fn default() -> Self {
        Slots { levels: BTreeMap::new(), boot: false, entry_hub: ANY_HUB, entry_tag: 99 }
    }
}

/// modSlots (mod/slots.c): the magic 'SLT1', the boot flag, the entry hub, the entry platform tag, then per slot 35
/// to 41 the cart address of its file, the bonus time (0x7FFF keeps the retail one), the song (0xFF keeps it) and
/// the name.
pub fn slots_table(slots: &Slots, offsets: &BTreeMap<i64, u32>) -> Result<Vec<u8>, ModError> {
    let mut out = Vec::new();
    out.extend_from_slice(&0x534C5431u32.to_be_bytes());
    out.push(slots.boot as u8);
    out.push(slots.entry_hub);
    out.extend_from_slice(&slots.entry_tag.to_be_bytes());
    for slot in SLOTS_FIRST..SLOTS_FIRST + SLOTS_COUNT {
        let d = match slots.levels.get(&slot) {
            Some(d) => d,
            None => {
                out.resize(out.len() + 8 + NAME_LEN, 0);
                continue;
            }
        };
        let name = d.name.as_deref().unwrap_or("");
        if !name.is_ascii() {
            return err(format!("level name {} isn't plain ASCII", py_repr(name)));
        }
        if name.len() >= NAME_LEN {
            return err(format!("level name {} is too long", py_repr(name)));
        }
        out.extend_from_slice(&offsets.get(&slot).copied().unwrap_or(0).to_be_bytes());
        out.extend_from_slice(&d.bonus_time.unwrap_or(0x7FFF).to_be_bytes());
        out.push(d.song.unwrap_or(0xFF));
        out.push(0);
        out.extend_from_slice(name.as_bytes());
        out.resize(out.len() + NAME_LEN - name.len(), 0);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// The mod

/// The ROM with the game mod of `mod/`. `extra_banks` is by level (an extra level: its slot + `SLOT_KEY`).
/// `custom` is by the same key. An empty map is the same as None.
pub fn apply_mod(
    rom: &[u8],
    extra_banks: Option<&BTreeMap<i64, ExtraBanks>>,
    custom: Option<&BTreeMap<u8, CustomBanks>>,
    slots: Option<&Slots>,
) -> Result<Vec<u8>, ModError> {
    apply_mod_with(rom, tables::mod_info(), tables::MOD_BIN, extra_banks, custom, slots)
}

/// `apply_mod` for another build of the mod: `info` is the content of its `mod.json`, `blob` its `mod.bin`.
pub fn apply_mod_with(
    rom: &[u8],
    info: &Value,
    blob: &[u8],
    extra_banks: Option<&BTreeMap<i64, ExtraBanks>>,
    custom: Option<&BTreeMap<u8, CustomBanks>>,
    slots: Option<&Slots>,
) -> Result<Vec<u8>, ModError> {
    let mut blob = blob.to_vec();
    if Some(n64::sha1_hex(&blob).as_str()) != info["mod_sha1"].as_str() {
        return err("mod/mod.bin doesn't match mod/mod.json; run tools/build_mod.py");
    }
    let mod_rom = hex(&info["mod_rom"])?;
    let has_magic = |blob: &[u8], offset: usize, magic: &[u8]| n64::bytes_at(blob, offset, 4).map_or(false, |b| b == magic);
    if let Some(extra) = extra_banks.filter(|e| !e.is_empty()) {
        let (offset, size) = table_place(info, "extra_banks").map_or_else(bad_json, Ok)?;
        let table = extra_banks_table(extra, size)?;
        splice(&mut blob, offset, &table)?;
    }
    let mut store: Vec<u8> = Vec::new();
    if let Some(slots) = slots.filter(|s| !s.levels.is_empty()) {
        let (offset, size) = match table_place(info, "slots") {
            Some(t) if has_magic(&blob, t.0, b"SLT1") => t,
            _ => return err("mod/mod.bin has no extra slot table; run tools/build_mod.py"),
        };
        let mut offsets = BTreeMap::new();
        let base = mod_rom + STORE;
        for (&slot, d) in &slots.levels {
            if !(SLOTS_FIRST..SLOTS_FIRST + SLOTS_COUNT).contains(&slot) {
                return err(format!("extra level slot {} is outside 35 to 41", slot));
            }
            let lev = &d.lev;
            if n64::read_u32(lev, 0).ok().map(|n| n as usize) != Some(lev.len()) || lev.len() % 4 != 0 {
                return err(format!("the level file of slot {} is not in the game's size-prefixed format", slot));
            }
            if lev.len() > MAX_LEVEL_FILE {
                return err(format!("the level file of slot {} is {} KB; the mod reads 256 KB at most", slot, lev.len() / 1024));
            }
            let at = u32::try_from(base + store.len()).or_else(|_| bad_json())?;
            offsets.insert(slot, at);
            store.extend_from_slice(lev);
            pad16(&mut store);
        }
        let table = slots_table(slots, &offsets)?;
        if table.len() != size {
            return err(format!("the extra slot table is {} bytes; the mod has {}", table.len(), size));
        }
        splice(&mut blob, offset, &table)?;
    }
    if let Some(custom) = custom.filter(|c| !c.is_empty()) {
        let (offset, size) = table_place(info, "custom").map_or_else(bad_json, Ok)?;
        if !has_magic(&blob, offset, b"CUS1") {
            return err("mod/mod.bin has no custom model table; run tools/build_mod.py");
        }
        let base = mod_rom + STORE + store.len();
        let (table, cstore) = custom_table(custom, base, size)?;
        splice(&mut blob, offset, &table)?;
        store.extend_from_slice(&cstore);
    }
    if blob.len() > STORE {
        return err(format!("the mod is {} bytes; what it stores starts at +0x{:X}", blob.len(), STORE));
    }
    let mut out = rom.to_vec();
    for r in info["regions"].as_array().map_or_else(bad_json, Ok)? {
        let (off, new) = (hex(&r["rom"])?, hex_bytes(&r["new"])?);
        let (ram, why) = (r["ram"].as_str().unwrap_or("?"), r["why"].as_str().unwrap_or("?"));
        // A ROM that ends early never matches
        let old = n64::bytes_at(&out, off, new.len()).ok().map(n64::sha1_hex);
        if old.as_deref() != r["old_sha1"].as_str() || old.is_none() {
            return err(format!("the ROM's code at {} isn't what the mod expects ({})", ram, why));
        }
        out[off..off + new.len()].copy_from_slice(&new);
    }
    for h in info["hooks"].as_array().map_or_else(bad_json, Ok)? {
        let (off, new) = (hex(&h["rom"])?, hex_bytes(&h["new"])?);
        let (ram, why) = (h["ram"].as_str().unwrap_or("?"), h["why"].as_str().unwrap_or("?"));
        let old = n64::bytes_at(&out, off, 4).ok().map(to_hex);
        if new.len() != 4 {
            return bad_json();
        }
        if old.as_deref() != h["old"].as_str() || old.is_none() {
            return err(format!("the ROM's code at {} isn't what the mod expects ({})", ram, why));
        }
        out[off..off + 4].copy_from_slice(&new);
    }
    if out.len() > mod_rom {
        return err(format!("the built ROM is {} bytes, past where the mod goes (0x{:X})", out.len(), mod_rom));
    }
    out.resize(mod_rom, 0);
    out.extend_from_slice(&blob);
    if !store.is_empty() {
        out.resize(mod_rom + STORE, 0);
        out.extend_from_slice(&store);
    }
    out.resize((out.len() + 0xFFFFF) & !0xFFFFF, 0); // whole MiB
    fix_crc(&mut out)?;
    let problems = hwcheck::check_rom_with(&out, info);
    if !problems.is_empty() {
        return err(format!("the ROM breaks a rule of real N64 hardware (hwcheck): {}", problems.join("; ")));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn banks(t: &[i64], o: &[i64], first: bool) -> ExtraBanks {
        ExtraBanks { textures: t.to_vec(), objects: o.to_vec(), first }
    }

    #[test]
    fn extra_bank_tables() {
        let mut m = BTreeMap::new();
        assert_eq!(extra_banks_table(&m, 4).unwrap(), [0xFF]);
        m.insert(9, banks(&[3], &[10, 12], false));
        m.insert(2, banks(&[], &[1], true));
        assert_eq!(extra_banks_table(&m, 256).unwrap(), [0x82, 0, 1, 1, 9, 1, 2, 3, 10, 12, 0xFF]);
        assert_eq!(extra_banks_table(&m, 11).unwrap().len(), 11);
        assert_eq!(extra_banks_table(&m, 10).unwrap_err().0, "the extra bank list is 11 bytes; the mod has room for 10");
        let bad = "too many extra banks (8 texture and 16 object banks at most)";
        for (level, v) in [
            (0x7F, banks(&[], &[], false)),
            (-1, banks(&[], &[], false)),
            (1, banks(&[0; 9], &[], false)),
            (1, banks(&[], &[0; 17], false)),
            (1, banks(&[0xFF], &[], false)),
            (1, banks(&[], &[-1], false)),
        ] {
            let m = BTreeMap::from([(level, v)]);
            assert_eq!(extra_banks_table(&m, 256).unwrap_err().0, format!("level {}: {}", level, bad));
        }
        let m = BTreeMap::from([(0x7E, banks(&[0; 8], &[0xFE; 16], true))]);
        assert_eq!(extra_banks_table(&m, 256).unwrap().len(), 3 + 24 + 1);
    }

    #[test]
    fn custom_tables() {
        let mut m = BTreeMap::new();
        m.insert(5u8, CustomBanks { textures: vec![1; 20], objects: vec![2; 3] });
        m.insert(1u8, CustomBanks { textures: vec![], objects: vec![3; 16] });
        let (table, store) = custom_table(&m, 0x1000, 168).unwrap();
        assert_eq!(table.len(), 168);
        assert_eq!(&table[..8], [0x43, 0x55, 0x53, 0x31, 0, 0, 0, 2]);
        let words = |row: usize| (0..5).map(|i| n64::read_u32(&table, 8 + row * 20 + i * 4).unwrap()).collect::<Vec<_>>();
        assert_eq!(words(0), [0x0100_0000, 0, 0, 0x1000, 0x1010]);
        assert_eq!(words(1), [0x0500_0000, 0x1010, 0x1030, 0x1030, 0x1040]);
        assert_eq!(store.len(), 0x40);
        assert_eq!(&store[0x10..0x24], [1; 20]);
        assert_eq!(&store[0x24..0x30], [0; 12]);
        assert_eq!(&store[0x30..0x33], [2; 3]);
        let nine: BTreeMap<u8, CustomBanks> = (0..9).map(|k| (k, CustomBanks::default())).collect();
        assert_eq!(custom_table(&nine, 0, 168).unwrap_err().0, "more than 8 custom bank sets");
    }

    #[test]
    fn slot_tables() {
        let mut s = Slots::default();
        let empty = slots_table(&s, &BTreeMap::new()).unwrap();
        assert_eq!(empty.len(), 344);
        assert_eq!(&empty[..8], [0x53, 0x4C, 0x54, 0x31, 0, 0xFF, 0, 99]);
        assert!(empty[8..].iter().all(|&b| b == 0));
        s.boot = true;
        s.entry_hub = 3;
        s.entry_tag = 0x1234;
        s.levels.insert(36, SlotLevel { lev: vec![], name: Some("ABC".into()), song: Some(7), bonus_time: Some(-2) });
        s.levels.insert(41, SlotLevel::default());
        let t = slots_table(&s, &BTreeMap::from([(36, 0xC20000), (41, 0xC20010)])).unwrap();
        assert_eq!(&t[..8], [0x53, 0x4C, 0x54, 0x31, 1, 3, 0x12, 0x34]);
        assert_eq!(&t[8 + 48..8 + 48 + 12], [0, 0xC2, 0, 0, 0xFF, 0xFE, 7, 0, b'A', b'B', b'C', 0]);
        assert_eq!(&t[8 + 6 * 48..8 + 6 * 48 + 9], [0, 0xC2, 0, 0x10, 0x7F, 0xFF, 0xFF, 0, 0]);
        s.levels.get_mut(&36).unwrap().name = Some("x".repeat(40));
        assert_eq!(slots_table(&s, &BTreeMap::new()).unwrap_err().0, format!("level name '{}' is too long", "x".repeat(40)));
        s.levels.get_mut(&36).unwrap().name = Some("caf\u{e9}".into());
        assert_eq!(slots_table(&s, &BTreeMap::new()).unwrap_err().0, "level name 'caf\u{e9}' isn't plain ASCII");
    }

    #[test]
    fn strings_are_quoted() {
        assert_eq!(py_repr("abc"), "'abc'");
        assert_eq!(py_repr("it's"), "\"it's\"");
        assert_eq!(py_repr("a'b\"c"), "'a\\'b\"c'");
        assert_eq!(py_repr("a\\b\n\t\x01"), "'a\\\\b\\n\\t\\x01'");
    }

    #[test]
    fn made_up_roms_are_refused() {
        let rom = vec![0u8; 0x200000];
        assert_eq!(apply_easy_controls(&rom).unwrap_err().0, "the ROM's code at 0x801396D0 isn't the difficulty check the controls patch expects");
        assert!(apply_easy_controls(&[]).is_err());
        let rows = [(0, LevelInfo::default())];
        assert_eq!(apply_level_info(&rom, &rows).unwrap_err().0, "the ROM's level table isn't where the retail Glover (USA) ROM has it");
        assert!(apply_level_info(&[], &rows).is_err());
        assert!(apply_mod(&rom, None, None, None).unwrap_err().0.starts_with("the ROM's code at "));
        assert!(apply_mod(&[], None, None, None).is_err());
        assert_eq!(
            apply_mod_with(&rom, tables::mod_info(), &[1, 2, 3], None, None, None).unwrap_err().0,
            "mod/mod.bin doesn't match mod/mod.json; run tools/build_mod.py"
        );
    }

    #[test]
    fn the_level_table_takes_rows() {
        // A made-up ROM with only the two names the check reads
        let mut rom = vec![0xEEu8; LEVEL_INFO + LEVEL_ROW * 48];
        rom[LEVEL_INFO..LEVEL_INFO + 6].copy_from_slice(b"HUB 1\0");
        rom[LEVEL_INFO + LEVEL_ROW * 41..][..12].copy_from_slice(b"SPACE BONUS\0");
        let rows = [
            (3, LevelInfo { name: Some("NEW".into()), song: Some(0x105), bonus_time: Some(-1) }),
            (47, LevelInfo { name: None, song: None, bonus_time: Some(300) }),
            (3, LevelInfo { name: None, song: Some(9), bonus_time: None }),
        ];
        let out = apply_level_info(&rom, &rows).unwrap();
        let at = LEVEL_INFO + LEVEL_ROW * 3;
        assert_eq!(&out[at..at + 4], b"NEW\0");
        assert!(out[at + 3..at + 0x28].iter().all(|&b| b == 0));
        assert_eq!(out[at + 0x28], 0xEE);
        assert_eq!(out[at + 0x35], 9);
        assert_eq!(&out[at + 0x36..at + 0x38], [0xFF, 0xFF]);
        assert_eq!(&out[out.len() - 2..], [0x01, 0x2C]);
        assert_eq!(out.len(), rom.len());
        let e = |row| apply_level_info(&rom, &[row]).unwrap_err().0;
        assert_eq!(e((48, LevelInfo::default())), "no level slot 48");
        assert_eq!(e((-1, LevelInfo::default())), "no level slot -1");
        assert_eq!(e((1, LevelInfo { name: Some("x".repeat(40)), ..Default::default() })), "a level name is 40 characters; 39 fit");
        assert_eq!(e((1, LevelInfo { name: Some("\u{e9}".into()), ..Default::default() })), "level name '\u{e9}' isn't plain ASCII");
        assert!(apply_level_info(&rom, &[(1, LevelInfo { name: Some("x".repeat(39)), ..Default::default() })]).is_ok());
    }
}
