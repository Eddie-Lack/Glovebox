//! The retail ROM as a list of regions (the code, every bank, the level files, the audio data), and a rebuilt ROM
//! with some of them replaced. This is libgarib's ROM tool (`rom.py`, `rom-asset-tool.py`), with the same output
//! bytes.
//!
//! The game's code holds the start and the end of most regions as a pair of instructions that load a cart address
//! into a register. `Rom::parse` reads those pairs. `Rom::finalize` packs the regions again from offset 0, each
//! padded to 8 bytes, writes the new addresses into the pairs and recomputes the header CRCs.
//!
//! The map of the retail ROM is `data/rom_map.json`, made by `tools/gen_rom_map.py` from libgarib's yaml file.

use crate::n64;
use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RomError(pub String);

impl std::fmt::Display for RomError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for RomError {}

impl From<n64::OutOfBounds> for RomError {
    fn from(e: n64::OutOfBounds) -> Self {
        RomError(format!("the ROM is damaged or is not the retail ROM: {}", e))
    }
}

fn err<T>(msg: impl Into<String>) -> Result<T, RomError> {
    Err(RomError(msg.into()))
}

// ---------------------------------------------------------------------------------------------
// The map

#[derive(Debug, Clone, Deserialize)]
struct MapPointer {
    address: usize,
    reg: String,
}

#[derive(Debug, Clone, Deserialize)]
struct MapRegion {
    data_type: String,
    region_type: String,
    namespaced_id: Option<u32>,
    name: Option<String>,
    #[serde(default)]
    start_pointers: Vec<MapPointer>,
    #[serde(default)]
    end_pointers: Vec<MapPointer>,
    start: Option<usize>,
    end: Option<usize>,
    size: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
struct Map {
    rom_checksum: (u32, u32),
    regions: Vec<MapRegion>,
}

fn map() -> &'static Map {
    static MAP: OnceLock<Map> = OnceLock::new();
    MAP.get_or_init(|| serde_json::from_str(include_str!("../data/rom_map.json")).expect("the compiled-in ROM map is valid"))
}

// ---------------------------------------------------------------------------------------------
// The instruction pairs that hold an address

/// A register's number, from its name in the map ("v0", "A1").
pub fn reg_number(name: &str) -> Option<u32> {
    const NAMES: [&str; 32] = [
        "", "at", "v0", "v1", "a0", "a1", "a2", "a3", "t0", "t1", "t2", "t3", "t4", "t5", "t6", "t7", "s0", "s1", "s2", "s3",
        "s4", "s5", "s6", "s7", "t8", "t9", "", "", "gp", "sp", "fp", "ra",
    ];
    let lower = name.to_ascii_lowercase();
    NAMES.iter().position(|n| !n.is_empty() && *n == lower).map(|n| n as u32)
}

const OP_ADDIU: u32 = 9;
const OP_ORI: u32 = 13;
const OP_LUI: u32 = 15;

/// The value a pair of instructions leaves in `reg`. The pair must be `lui reg, hi` and then `addiu reg, reg, lo`
/// (the low half is sign-extended) or `ori reg, reg, lo`. Anything else is refused.
pub fn decode_pointer(first: u32, second: u32, reg: u32) -> Result<u32, RomError> {
    let (op1, rs1, rt1) = (first >> 26, (first >> 21) & 31, (first >> 16) & 31);
    let (op2, rs2, rt2) = (second >> 26, (second >> 21) & 31, (second >> 16) & 31);
    if op1 != OP_LUI || rs1 != 0 || (op2 != OP_ADDIU && op2 != OP_ORI) {
        return err(format!("the instructions {:08X} {:08X} are not a lui with an addiu or an ori", first, second));
    }
    if rt2 != reg {
        return err(format!("Inconsistent pointer register (expected {}, found {})", reg, rt2));
    }
    if rt1 != reg || rs2 != reg {
        return err(format!("the instructions {:08X} {:08X} do not build one address in register {}", first, second, reg));
    }
    if reg == 0 {
        return err("an address can't be built in register zero");
    }
    let hi = (first & 0xFFFF) << 16;
    let lo = second & 0xFFFF;
    Ok(if op2 == OP_ADDIU { hi.wrapping_add(lo as u16 as i16 as i32 as u32) } else { hi | lo })
}

/// The pair `lui reg, hi` and `ori reg, reg, lo` that loads `address`, as 8 bytes.
pub fn encode_pointer(address: u32, reg: u32) -> [u8; 8] {
    let reg = reg & 31;
    let lui = (OP_LUI << 26) | (reg << 16) | (address >> 16);
    let ori = (OP_ORI << 26) | (reg << 21) | (reg << 16) | (address & 0xFFFF);
    let mut out = [0u8; 8];
    out[..4].copy_from_slice(&lui.to_be_bytes());
    out[4..].copy_from_slice(&ori.to_be_bytes());
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Pointer {
    address: usize,
    reg: u32,
}

fn pointers(list: &[MapPointer]) -> Result<Vec<Pointer>, RomError> {
    list.iter()
        .map(|p| match reg_number(&p.reg) {
            Some(reg) => Ok(Pointer { address: p.address, reg }),
            None => err(format!("the ROM map names an unknown register {:?}", p.reg)),
        })
        .collect()
}

/// The one value all the pairs of a list hold, without the top four bits.
fn scrape(rom: &[u8], list: &[Pointer]) -> Result<usize, RomError> {
    let mut val = None;
    for p in list {
        let v = decode_pointer(n64::read_u32(rom, p.address)?, n64::read_u32(rom, p.address + 4)?, p.reg)?;
        if val.is_some() && val != Some(v) {
            return err("Inconsistent pointer values");
        }
        val = Some(v);
    }
    match val {
        Some(v) => Ok((v & 0x0FFFFFFF) as usize),
        None => err("a region of the ROM map has no pointer"),
    }
}

// ---------------------------------------------------------------------------------------------
// Keys

/// What a key names a region by: its number in the game (bank numbers), or its name in the map.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Id {
    Num(u32),
    Name(String),
}

/// A region's key, as in libgarib's manifests: the data type ("objbank", "texbank", "landscape", "midi_sequence",
/// "sndbank", "audio_ptr_table", "audio_wave_table", "opaque") and an id. A region with a number is keyed by the
/// number, else by its name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Key {
    pub data_type: String,
    pub id: Id,
}

impl Key {
    pub fn num(data_type: &str, n: u32) -> Key {
        Key { data_type: data_type.to_string(), id: Id::Num(n) }
    }

    pub fn name(data_type: &str, name: &str) -> Key {
        Key { data_type: data_type.to_string(), id: Id::Name(name.to_string()) }
    }

    /// The level files: `('landscape', 'Landscapes')`.
    pub fn landscapes() -> Key {
        Key::name("landscape", "Landscapes")
    }

    /// The game's code and data: `('opaque', 'code')`.
    pub fn code() -> Key {
        Key::name("opaque", "code")
    }

    /// A key from its text in a manifest, such as `('objbank', 12)` or `('landscape', 'Landscapes')`. The second
    /// value of the result is the element index of a three part key, `('landscape', 'Landscapes', 3)`.
    pub fn parse(text: &str) -> Option<(Key, Option<usize>)> {
        let inner = text.trim().strip_prefix('(')?.strip_suffix(')')?;
        let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
        if parts.len() != 2 && parts.len() != 3 {
            return None;
        }
        fn quoted(s: &str) -> Option<&str> {
            let q = s.chars().next()?;
            if q != '\'' && q != '"' {
                return None;
            }
            let body = s.strip_prefix(q)?.strip_suffix(q)?;
            if body.contains(['\\', '\'', '"']) {
                return None;
            }
            Some(body)
        }
        let data_type = quoted(parts[0])?.to_string();
        let id = match quoted(parts[1]) {
            Some(name) => Id::Name(name.to_string()),
            None => Id::Num(parts[1].parse().ok()?),
        };
        let index = match parts.get(2) {
            Some(p) => Some(p.parse().ok()?),
            None => None,
        };
        Some((Key { data_type, id }, index))
    }
}

/// The key as libgarib's ROM tool prints it, a Python tuple: `('objbank', 12)`.
impl std::fmt::Display for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match &self.id {
            Id::Num(n) => write!(f, "('{}', {})", self.data_type, n),
            Id::Name(s) => write!(f, "('{}', '{}')", self.data_type, s),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Regions

#[derive(Debug, Clone, PartialEq, Eq)]
enum Kind {
    /// A fixed place in the ROM. No instruction points at it.
    Static,
    /// One block between a start and an end address.
    BoundedSingle,
    /// One block with a start address and a size from the map.
    UnboundedSingle,
    /// Files one after the other between a start and an end address. Each starts with its own length.
    BoundedArray,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Data {
    Single(Vec<u8>),
    Array(Vec<Vec<u8>>),
}

/// One region of the ROM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Region {
    key: Key,
    name: Option<String>,
    offset: usize,
    kind: Kind,
    start_pointers: Vec<Pointer>,
    end_pointers: Vec<Pointer>,
    data: Data,
}

impl Region {
    pub fn key(&self) -> &Key {
        &self.key
    }

    pub fn data_type(&self) -> &str {
        &self.key.data_type
    }

    /// The region's name in the map. The sound banks have none.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Where the region starts in the ROM that was parsed.
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// Whether an instruction pair in the code holds the region's start. False for the code and the Japanese font,
    /// which the map gives a fixed place.
    pub fn start_is_pointer(&self) -> bool {
        self.kind != Kind::Static
    }

    /// Whether the region is a list of files (the level files, the songs).
    pub fn is_array(&self) -> bool {
        matches!(self.data, Data::Array(_))
    }

    /// The region's bytes. None for a list of files.
    pub fn bytes(&self) -> Option<&[u8]> {
        match &self.data {
            Data::Single(d) => Some(d),
            Data::Array(_) => None,
        }
    }

    /// The files of a list. Empty for a region that is one block.
    pub fn elements(&self) -> &[Vec<u8>] {
        match &self.data {
            Data::Single(_) => &[],
            Data::Array(v) => v,
        }
    }

    /// The size in bytes, without padding.
    pub fn size(&self) -> usize {
        match &self.data {
            Data::Single(d) => d.len(),
            Data::Array(v) => v.iter().map(Vec::len).sum(),
        }
    }
}

/// The name a level file carries: the bytes from offset 4 up to the first zero byte.
pub fn landscape_name(file: &[u8]) -> Option<String> {
    let rest = file.get(4..)?;
    let end = rest.iter().position(|&b| b == 0)?;
    Some(rest[..end].iter().map(|&b| b as char).collect())
}

/// A quality-of-life patch to the game's code (libgarib's `--qol`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Qol {
    /// Skips the copyright screen.
    CopySkip,
    /// Makes the opening logo scene playable and sends it to this level (`quickboot(level_id=0x..)`).
    QuickBoot(u16),
}

/// The words a patch writes into the code region, with their offsets. These are the words Keystone assembles for
/// libgarib's patch text. An address 0x80xxxxxx of the text is the offset (address & 0xFFFFF) + 0x1000.
fn qol_words(q: Qol) -> Result<Vec<(usize, u32)>, RomError> {
    Ok(match q {
        // .text 0x8010e86c: j 0x8010eb0c; addi $v0, $zero, 1
        Qol::CopySkip => vec![(0xF86C, 0x08043AC3), (0xF870, 0x20020001)],
        Qol::QuickBoot(level) => {
            if level > 0x7FFF {
                return err(format!("level id 0x{:x} does not fit the quickboot patch", level));
            }
            // .text 0x8012bcfc: addi $v0, $zero, 0
            // .text 0x8012bd0c: addi $a0, $zero, level
            vec![(0x2CCFC, 0x20020000), (0x2CD0C, 0x20040000 | level as u32)]
        }
    })
}

/// The ROM, split into the regions of the map and sorted by offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rom {
    regions: Vec<Region>,
    checksum: (u32, u32),
}

impl Rom {
    /// Splits a ROM in .z64 byte order. An error when it doesn't hold what the map of the retail ROM says.
    pub fn parse(rom: &[u8]) -> Result<Rom, RomError> {
        let checksum = n64::stored_crc(rom)?;
        let mut regions = Vec::new();
        for m in &map().regions {
            let key = match (m.namespaced_id, &m.name) {
                (Some(n), _) => Key::num(&m.data_type, n),
                (None, Some(name)) => Key::name(&m.data_type, name),
                (None, None) => return err("a region of the ROM map has no key"),
            };
            let start_pointers = pointers(&m.start_pointers)?;
            let end_pointers = pointers(&m.end_pointers)?;
            let (kind, start, end) = match m.region_type.as_str() {
                "static" => match (m.start, m.end) {
                    (Some(s), Some(e)) => (Kind::Static, s, e),
                    _ => return err("a static region of the ROM map has no start or no end"),
                },
                "bounded_single" => (Kind::BoundedSingle, scrape(rom, &start_pointers)?, scrape(rom, &end_pointers)?),
                "bounded_array" => (Kind::BoundedArray, scrape(rom, &start_pointers)?, scrape(rom, &end_pointers)?),
                "unbounded_single" => {
                    let s = scrape(rom, &start_pointers)?;
                    match m.size {
                        Some(size) => (Kind::UnboundedSingle, s, s + size),
                        None => return err("a region of the ROM map has no size"),
                    }
                }
                other => return err(format!("the ROM map has an unknown region type {:?}", other)),
            };
            if end < start {
                return err(format!("region {}: its end (0x{:X}) is before its start (0x{:X})", key, end, start));
            }
            let data = if kind == Kind::BoundedArray {
                let mut elems = Vec::new();
                let mut cursor = start;
                while cursor < end {
                    let len = n64::read_u32(rom, cursor)? as usize;
                    if len == 0 {
                        return err(format!("region {}: the file at 0x{:X} has length 0", key, cursor));
                    }
                    elems.push(n64::bytes_at(rom, cursor, len)?.to_vec());
                    cursor += len;
                }
                Data::Array(elems)
            } else {
                Data::Single(n64::bytes_at(rom, start, end - start)?.to_vec())
            };
            regions.push(Region { key, name: m.name.clone(), offset: start, kind, start_pointers, end_pointers, data });
        }
        // A stable sort: regions with the same offset keep the order of the map.
        regions.sort_by_key(|r| r.offset);
        Ok(Rom { regions, checksum })
    }

    /// Whether the header CRCs of the parsed ROM are the retail ROM's.
    pub fn is_retail_checksum(&self) -> bool {
        self.checksum == map().rom_checksum
    }

    /// Every region, sorted by its offset in the parsed ROM.
    pub fn regions(&self) -> &[Region] {
        &self.regions
    }

    pub fn region(&self, key: &Key) -> Option<&Region> {
        self.regions.iter().find(|r| &r.key == key)
    }

    fn region_mut(&mut self, key: &Key) -> Result<&mut Region, RomError> {
        match self.regions.iter_mut().find(|r| &r.key == key) {
            Some(r) => Ok(r),
            None => err(format!("the ROM has no region {}", key)),
        }
    }

    /// The bytes of a region that is one block.
    pub fn bytes(&self, key: &Key) -> Option<&[u8]> {
        self.region(key)?.bytes()
    }

    /// One file of a region that is a list of files.
    pub fn element(&self, key: &Key, index: usize) -> Option<&[u8]> {
        self.region(key)?.elements().get(index).map(Vec::as_slice)
    }

    /// How many level files the ROM has.
    pub fn level_count(&self) -> usize {
        self.region(&Key::landscapes()).map_or(0, |r| r.elements().len())
    }

    /// A level file, by the level's index.
    pub fn level(&self, index: usize) -> Option<&[u8]> {
        self.element(&Key::landscapes(), index)
    }

    /// The name inside a level file ("HUB1ln").
    pub fn level_name(&self, index: usize) -> Option<String> {
        landscape_name(self.level(index)?)
    }

    /// The banks with this name, each with its number.
    fn banks(&self, data_type: &str, name: &str) -> Vec<(u32, &Region)> {
        let named = self.regions.iter().filter(|r| r.key.data_type == data_type && r.name.as_deref() == Some(name));
        named
            .map(|r| match r.key.id {
                Id::Num(n) => (n, r),
                Id::Name(_) => (u32::MAX, r),
            })
            .collect()
    }

    /// The region a build replaces for a bank ("objbank" or "texbank") named like this in the map. Two texture
    /// banks are named LEVELEND_TEX_BANK (1 and 14): the lower number wins.
    pub fn bank(&self, data_type: &str, name: &str) -> Option<&Region> {
        self.banks(data_type, name).into_iter().min_by_key(|b| b.0).map(|b| b.1)
    }

    /// An object bank's bytes by its name ("HUB_PART1"), as noclip's ROM splitter writes them to NAME.obj.fla.
    pub fn object_bank(&self, name: &str) -> Option<&[u8]> {
        self.bank("objbank", name)?.bytes()
    }

    /// A texture bank's bytes by its name ("HUB_TEX_BANK"), as noclip's ROM splitter writes them to NAME.tex.fla.
    /// Of the two banks named LEVELEND_TEX_BANK this is number 14, the one noclip keeps. `bank` names number 1.
    pub fn texture_bank(&self, name: &str) -> Option<&[u8]> {
        self.banks("texbank", name).into_iter().filter(|b| b.0 != u32::MAX).max_by_key(|b| b.0).map(|b| b.1)?.bytes()
    }

    /// Replaces a region that is one block. Empty data deletes the region's content.
    pub fn replace(&mut self, key: &Key, data: &[u8]) -> Result<(), RomError> {
        match &mut self.region_mut(key)?.data {
            Data::Single(d) => {
                *d = data.to_vec();
                Ok(())
            }
            Data::Array(_) => err(format!("region {} is a list of files: name the file to replace", key)),
        }
    }

    /// Replaces one file of a region that is a list of files (a level file, a song).
    pub fn replace_element(&mut self, key: &Key, index: usize, data: &[u8]) -> Result<(), RomError> {
        match &mut self.region_mut(key)?.data {
            Data::Array(v) => match v.get_mut(index) {
                Some(e) => {
                    *e = data.to_vec();
                    Ok(())
                }
                None => err(format!("region {} has no file {}", key, index)),
            },
            Data::Single(_) => err(format!("region {} is not a list of files", key)),
        }
    }

    /// Replaces a level file.
    pub fn replace_level(&mut self, index: usize, lev: &[u8]) -> Result<(), RomError> {
        self.replace_element(&Key::landscapes(), index, lev)
    }

    /// Writes quality-of-life patches into the code.
    pub fn apply_qol(&mut self, patches: &[Qol]) -> Result<(), RomError> {
        let mut words = Vec::new();
        for &q in patches {
            words.extend(qol_words(q)?);
        }
        match &mut self.region_mut(&Key::code())?.data {
            Data::Single(code) => {
                for (offset, word) in words {
                    n64::write_u32(code, offset, word).map_err(|e| RomError(format!("the code region is too short for the patch: {}", e)))?;
                }
                Ok(())
            }
            Data::Array(_) => err("the code region is not one block"),
        }
    }

    /// The rebuilt ROM: the regions in the order of the parsed ROM, packed from offset 0, each padded with zeros to
    /// a multiple of 8, the instruction pairs rewritten for the new places, and the header CRCs recomputed.
    pub fn finalize(&self) -> Result<Vec<u8>, RomError> {
        let mut out: Vec<u8> = Vec::new();
        let mut places = Vec::with_capacity(self.regions.len());
        for r in &self.regions {
            let offset = out.len();
            match &r.data {
                Data::Single(d) => out.extend_from_slice(d),
                Data::Array(v) => v.iter().for_each(|e| out.extend_from_slice(e)),
            }
            let size = out.len() - offset;
            out.resize(out.len() + (8 - size % 8) % 8, 0);
            places.push((offset, out.len() - offset));
        }
        if out.len() > 0x1000_0000 {
            return err("the rebuilt ROM is too large");
        }
        for (r, &(offset, size)) in self.regions.iter().zip(&places) {
            if r.kind == Kind::Static {
                continue;
            }
            let start = offset as u32 | 0xB000_0000;
            for p in &r.start_pointers {
                write_pair(&mut out, p, start)?;
            }
            if r.kind != Kind::UnboundedSingle {
                for p in &r.end_pointers {
                    write_pair(&mut out, p, start.wrapping_add(size as u32))?;
                }
            }
        }
        match n64::identify_cic(&out) {
            Some(6101) | Some(6102) => {}
            _ => return err("the rebuilt ROM does not have the boot code of the retail ROM"),
        }
        n64::fix_header_crc(&mut out).map_err(|_| RomError("the rebuilt ROM is too short to hold the code".into()))?;
        Ok(out)
    }
}

fn write_pair(out: &mut [u8], p: &Pointer, address: u32) -> Result<(), RomError> {
    n64::write_bytes(out, p.address, &encode_pointer(address, p.reg))
        .map_err(|e| RomError(format!("an address of the ROM map is outside the rebuilt ROM: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_names() {
        assert_eq!(reg_number("v0"), Some(2));
        assert_eq!(reg_number("V1"), Some(3));
        assert_eq!(reg_number("a1"), Some(5));
        assert_eq!(reg_number("s1"), Some(17));
        assert_eq!(reg_number("ra"), Some(31));
        assert_eq!(reg_number("zero"), None);
        assert_eq!(reg_number(""), None);
        assert_eq!(reg_number("k0"), None);
    }

    #[test]
    fn pairs_decode() {
        // lui v0, 0xb026; addiu v0, v0, -0x1234: the low half is sign-extended
        assert_eq!(decode_pointer(0x3C02B026, 0x2442EDCC, 2), Ok(0xB025EDCC));
        // lui v0, 0xb026; addiu v0, v0, 0x1234
        assert_eq!(decode_pointer(0x3C02B026, 0x24421234, 2), Ok(0xB0261234));
        // lui s1, 0xb012; ori s1, s1, 0xf456: no sign extension
        assert_eq!(decode_pointer(0x3C11B012, 0x3631F456, 17), Ok(0xB012F456));
        // The addition wraps
        assert_eq!(decode_pointer(0x3C020000, 0x2442FFFF, 2), Ok(0xFFFFFFFF));
    }

    #[test]
    fn pairs_refused() {
        // Another register than the map says
        assert!(decode_pointer(0x3C02B026, 0x24421234, 3).unwrap_err().0.starts_with("Inconsistent pointer register"));
        // lui v1 then addiu v0, v0
        assert!(decode_pointer(0x3C03B026, 0x24421234, 2).is_err());
        // addiu v0, v1
        assert!(decode_pointer(0x3C02B026, 0x24621234, 2).is_err());
        // Not a lui
        assert!(decode_pointer(0x24021234, 0x24421234, 2).is_err());
        // lui then a load
        assert!(decode_pointer(0x3C02B026, 0x8C421234, 2).is_err());
        // lui with a source register field
        assert!(decode_pointer(0x3C42B026, 0x24421234, 2).is_err());
        // Register zero
        assert!(decode_pointer(0x3C00B026, 0x24001234, 0).is_err());
        // Two nops
        assert!(decode_pointer(0, 0, 2).is_err());
    }

    #[test]
    fn pairs_encode_like_keystone() {
        assert_eq!(encode_pointer(0xB0123456, 2), [0x3C, 0x02, 0xB0, 0x12, 0x34, 0x42, 0x34, 0x56]);
        assert_eq!(encode_pointer(0xB012F456, 17), [0x3C, 0x11, 0xB0, 0x12, 0x36, 0x31, 0xF4, 0x56]);
        assert_eq!(encode_pointer(0xFFFFFFFF, 31), [0x3C, 0x1F, 0xFF, 0xFF, 0x37, 0xFF, 0xFF, 0xFF]);
        assert_eq!(encode_pointer(0, 5), [0x3C, 0x05, 0, 0, 0x34, 0xA5, 0, 0]);
        for reg in 1..32 {
            for addr in [0u32, 0xB0000008, 0xB07FFFF8, 0xB012F456, 0xFFFFFFFF] {
                let c = encode_pointer(addr, reg);
                let (a, b) = (u32::from_be_bytes([c[0], c[1], c[2], c[3]]), u32::from_be_bytes([c[4], c[5], c[6], c[7]]));
                assert_eq!(decode_pointer(a, b, reg), Ok(addr));
            }
        }
    }

    #[test]
    fn keys() {
        assert_eq!(Key::num("objbank", 12).to_string(), "('objbank', 12)");
        assert_eq!(Key::landscapes().to_string(), "('landscape', 'Landscapes')");
        assert_eq!(Key::parse("('objbank', 12)"), Some((Key::num("objbank", 12), None)));
        assert_eq!(Key::parse("('landscape', 'Landscapes', 7)"), Some((Key::landscapes(), Some(7))));
        assert_eq!(Key::parse(" ( \"opaque\" , 'code' ) "), Some((Key::code(), None)));
        assert_eq!(Key::parse("('objbank', -1)"), None);
        assert_eq!(Key::parse("('objbank')"), None);
        assert_eq!(Key::parse("objbank, 1"), None);
        assert_eq!(Key::parse("('a', 'b', 'c')"), None);
        assert_eq!(Key::parse(""), None);
    }

    #[test]
    fn qol_words_are_keystones() {
        assert_eq!(qol_words(Qol::CopySkip).unwrap(), vec![(0xF86C, 0x08043AC3), (0xF870, 0x20020001)]);
        assert_eq!(qol_words(Qol::QuickBoot(0x29)).unwrap(), vec![(0x2CCFC, 0x20020000), (0x2CD0C, 0x20040029)]);
        assert!(qol_words(Qol::QuickBoot(0x8000)).is_err());
    }

    #[test]
    fn the_map_is_whole() {
        let m = map();
        assert_eq!(m.rom_checksum, (0x8E6E01FF, 0xCCB4F948));
        assert_eq!(m.regions.len(), 143);
        for r in &m.regions {
            assert!(r.namespaced_id.is_some() || r.name.is_some());
            assert!(pointers(&r.start_pointers).is_ok() && pointers(&r.end_pointers).is_ok());
        }
    }

    #[test]
    fn landscape_names() {
        assert_eq!(landscape_name(&[0, 0, 0, 16, b'A', b'b', 0, 9]), Some("Ab".to_string()));
        assert_eq!(landscape_name(&[0, 0, 0, 16, b'A', b'b']), None);
        assert_eq!(landscape_name(&[0, 0]), None);
    }

    #[test]
    fn a_made_up_rom_is_refused() {
        assert!(Rom::parse(&[]).is_err());
        assert!(Rom::parse(&vec![0u8; 0x800000]).is_err());
        assert!(Rom::parse(&vec![0xFFu8; 0x100000]).is_err());
    }
}
