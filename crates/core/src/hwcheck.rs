//! Checks a built ROM against the rules of real N64 hardware, which an emulator does not enforce. This is
//! `tools/hw_check.py`, with the same findings and the same texts.
//!
//! `check_rom` returns a list of problems (empty: all fine). The mod patcher runs it on every ROM it builds and
//! refuses a ROM with a problem. The rules (docs/technical.md, "Real hardware"):
//!
//! - The header holds the CRCs of the first megabyte of code. The boot code of a console does not start a ROM whose
//!   CRCs are wrong.
//! - The ROM is a whole number of mebibytes, and everything the mod reads from it lies inside it.
//! - Every DMA the mod makes from the ROM starts at a multiple of 16 and has a length that is a multiple of 16.
//! - A texture bank's records are each a multiple of 8 bytes long. The pixels of every texture then start on a
//!   64 bit boundary, where the RDP loads them from.
//! - The pointers inside an object bank are aligned for what they point at.
//! - Every call site the mod hooks is a `jal` to a word inside the mod.
//! - Every extra level stored behind the mod starts on a 16 byte boundary, has a sane size word and zero padding
//!   to 16.

use crate::n64::{self, read_u32};
use crate::tables;
use serde_json::Value;
use std::collections::HashSet;

/// mod/slots.c MAX_LEVEL_FILE
const MAX_LEVEL_FILE: u64 = 0x40000;
/// mod/slots.c SlotLevel
const SLOT_LEVEL_SIZE: usize = 8 + 0x28;

fn hex(v: &Value) -> Option<u64> {
    let s = v.as_str()?;
    let s = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    u64::from_str_radix(s, 16).ok()
}

fn check_texture_bank(data: &[u8], label: &str, problems: &mut Vec<String>) {
    if data.len() < 4 {
        problems.push(format!("{}: texture bank is empty", label));
        return;
    }
    let n = read_u32(data, 0).unwrap_or(0);
    let mut pos = 4usize;
    for i in 0..n {
        if pos + 0x24 > data.len() {
            problems.push(format!("{}: texture {} starts past the end of the bank", label, i));
            return;
        }
        let size = read_u32(data, pos + 20).unwrap_or(0);
        if size < 0x24 || size % 8 != 0 {
            problems.push(format!(
                "{}: texture {} has size {}: not a multiple of 8, so the pixels of the next one are not on a 64 bit boundary",
                label, i, size
            ));
            return;
        }
        pos += size as usize;
    }
    if pos > data.len() {
        problems.push(format!("{}: the textures run past the end of the bank", label));
    }
}

/// The CPU reads an object bank after the mod has turned its offsets into pointers (mod/banks.c LoadCustomBanks,
/// the game's RestoreObjectPointers). Every word, float and halfword it reads must sit on its own alignment, or
/// the console raises an address error. This walks the entries the way the loader does.
struct ObjectBank<'a> {
    data: &'a [u8],
    label: &'a str,
    problems: &'a mut Vec<String>,
}

impl ObjectBank<'_> {
    fn u32(&self, o: usize) -> Option<u32> {
        read_u32(self.data, o).ok()
    }

    /// The offset a pointer field holds: Some(0) for a null pointer, None after a problem.
    fn ptr(&mut self, o: usize, align: usize, what: &str) -> Option<usize> {
        let v = match self.u32(o) {
            Some(v) => v,
            None => {
                self.problems.push(format!("{}: {} is read past the end of the bank", self.label, what));
                return None;
            }
        };
        if v == 0 {
            return Some(0);
        }
        let off = (v & 0xFFFFFF) as usize;
        if off >= self.data.len() || off % align != 0 {
            let why = if off >= self.data.len() { "outside the bank".to_string() } else { format!("not {} byte aligned", align) };
            self.problems.push(format!("{}: {} points to offset 0x{:X}: {}", self.label, what, off, why));
            return None;
        }
        Some(off)
    }

    /// The fields of one object and of its mesh. Returns nothing; the children are walked by `walk`.
    fn fields(&mut self, o: usize) {
        const OBJECT: [(&str, usize, usize); 6] =
            [("mesh", 0x14, 4), ("scaleKeys", 0x1C, 4), ("posKeys", 0x20, 4), ("rotKeys", 0x24, 4), ("sprites", 0x2C, 4), ("sphere", 0x3C, 4)];
        const MESH: [(&str, usize, usize); 7] = [
            ("verts", 4, 4),
            ("faces", 8, 2),
            ("faceColours", 0xC, 1),
            ("uv", 0x10, 2),
            ("colours", 0x18, 1),
            ("faceFlags", 0x1C, 1),
            ("textures", 0x20, 4),
        ];
        for (what, at, align) in OBJECT {
            self.ptr(o + at, align, &format!("object field {}", what));
        }
        if let Some(m) = self.ptr(o + 0x14, 4, "mesh") {
            if m != 0 {
                for (what, at, align) in MESH {
                    self.ptr(m + at, align, &format!("mesh field {}", what));
                }
            }
        }
    }

    /// An object, then its child and everything under it, then its sibling. Each object is visited once. The
    /// Python code recurses; this keeps its order with a list of the objects still to finish.
    fn walk(&mut self, root: usize, seen: &mut HashSet<usize>) {
        // (object, which of the two links comes next)
        let mut stack: Vec<(usize, u8)> = Vec::new();
        if seen.insert(root) {
            self.fields(root);
            stack.push((root, 0));
        }
        while let Some(top) = stack.last_mut() {
            let (o, step) = *top;
            if step >= 2 {
                stack.pop();
                continue;
            }
            top.1 += 1;
            let at = if step == 0 { 0x34 } else { 0x38 };
            if let Some(c) = self.ptr(o + at, 4, "child or sibling") {
                if c != 0 && seen.insert(c) {
                    self.fields(c);
                    stack.push((c, 0));
                }
            }
        }
    }

    fn check(&mut self) {
        let mut seen = HashSet::new();
        let mut n = 0usize;
        while n < 4096 {
            let inst = match (self.u32(8 * n), self.u32(8 * n + 4)) {
                (Some(_), Some(inst)) => inst,
                _ => {
                    self.problems.push(format!("{}: the entry list has no end", self.label));
                    return;
                }
            };
            if inst == 0 {
                break;
            }
            let i = (inst & 0xFFFFFF) as usize;
            if i + 0x1C > self.data.len() || i % 4 != 0 {
                self.problems.push(format!("{}: entry {} points to 0x{:X}: outside the bank or not aligned", self.label, n, i));
                return;
            }
            if let Some(o) = self.ptr(i + 0x0C, 4, "instance object") {
                if o != 0 {
                    self.walk(o, &mut seen);
                }
            }
            n += 1;
        }
    }
}

fn check_object_bank(data: &[u8], label: &str, problems: &mut Vec<String>) {
    ObjectBank { data, label, problems }.check();
}

/// The problems of a ROM that has the game mod of `mod/mod.json`.
pub fn check_rom(rom: &[u8]) -> Vec<String> {
    check_rom_with(rom, tables::mod_info())
}

/// The problems of a ROM, for this description of the mod (the content of a `mod.json`).
pub fn check_rom_with(rom: &[u8], info: &Value) -> Vec<String> {
    let mut problems = Vec::new();
    let len = rom.len() as u64;
    if len % 0x100000 != 0 || len < 0x800000 || len > 0x4000000 {
        problems.push(format!("the ROM is {} bytes: it must be a whole number of MiB, at least 8 MiB and at most 64 MiB", len));
    }
    match (n64::header_crc(rom), n64::stored_crc(rom)) {
        (Ok(crc), Ok(have)) => {
            if crc != have {
                problems.push(format!("header CRC is {:08X} {:08X}, the code gives {:08X} {:08X}", have.0, have.1, crc.0, crc.1));
            }
        }
        _ => {
            // Python stops with an exception here
            problems.push("the ROM is too short to hold the code the header CRCs cover".to_string());
            return problems;
        }
    }
    let (mod_rom, mod_size, mod_ram, mod_end) = match (hex(&info["mod_rom"]), info["mod_size"].as_u64(), hex(&info["mod_ram"]), hex(&info["mod_end"])) {
        (Some(a), Some(b), Some(c), Some(d)) => (a, b, c, d),
        _ => {
            problems.push("the mod's description (mod.json) is incomplete".to_string());
            return problems;
        }
    };
    if mod_rom % 16 != 0 || mod_size % 16 != 0 || mod_rom.saturating_add(mod_size) > len {
        problems.push(format!("the mod (ROM 0x{:X}, {} bytes) is not 16 byte aligned or is not inside the ROM", mod_rom, mod_size));
    }
    if let Some(symbols) = info.get("hw_symbols").and_then(Value::as_object) {
        for (name, addr) in symbols {
            if hex(addr).map_or(true, |a| a % 16 != 0) {
                problems.push(format!("mod symbol {} is at {}: not 16 byte aligned", name, addr.as_str().unwrap_or("?")));
            }
        }
    }
    if mod_ram % 16 != 0 || mod_end > 0x80800000 {
        problems.push("the mod does not fit in RAM below 0x80800000".to_string());
    }
    for h in info["hooks"].as_array().map(Vec::as_slice).unwrap_or(&[]) {
        let why = h["why"].as_str().unwrap_or("");
        let ram = h["ram"].as_str().unwrap_or("?");
        // The loader is in the game's own code (over ScreenShot)
        let loader = why.ends_with("(loader)");
        let ok = match hex(&h["new"]) {
            Some(word) => {
                let target = ((word & 0x03FFFFFF) << 2) | 0x80000000;
                let into_mod = mod_ram <= target && target < mod_ram.saturating_add(mod_size);
                word >> 26 == 3 && into_mod != loader
            }
            None => false,
        };
        if !ok {
            problems.push(format!("hook at {} is not a jal to {} ({})", ram, if loader { "the loader" } else { "a word inside the mod" }, why));
        }
    }
    let at = |offset: u64| usize::try_from(mod_rom.saturating_add(offset)).unwrap_or(usize::MAX);
    let magic = |t: usize, m: &[u8]| n64::bytes_at(rom, t, 4).map_or(false, |b| b == m);
    if let Some(t) = info.get("slots").and_then(|s| s["offset"].as_u64()).map(at) {
        if magic(t, b"SLT1") {
            for i in 0..7usize {
                let label = format!("extra level {}", 35 + i);
                let r = match read_u32(rom, t.saturating_add(8 + i * SLOT_LEVEL_SIZE)) {
                    Ok(r) => r as u64,
                    Err(_) => {
                        problems.push(format!("{}: its table entry is outside the ROM", label));
                        continue;
                    }
                };
                if r == 0 {
                    continue;
                }
                if r % 16 != 0 || r + 8 > len {
                    problems.push(format!("{} starts at 0x{:X}: not 16 byte aligned or outside the ROM", label, r));
                    continue;
                }
                let r = r as usize;
                let size = read_u32(rom, r).unwrap_or(0) as u64;
                if size % 4 != 0 || !(16..=MAX_LEVEL_FILE).contains(&size) {
                    problems.push(format!("{}: its size word is {}", label, size));
                    continue;
                }
                let size = size as usize;
                let end = r + ((size + 15) & !15);
                if end > rom.len() {
                    problems.push(format!("{}: the DMA of {} bytes runs past the end of the ROM", label, end - r));
                } else if rom[r + size..end].iter().any(|&b| b != 0) {
                    problems.push(format!("{}: the padding after the file is not zero", label));
                }
            }
        }
    }
    if let Some(c) = info["custom"]["offset"].as_u64().map(at) {
        if magic(c, b"CUS1") {
            let count = read_u32(rom, c.saturating_add(4)).unwrap_or(0);
            for i in 0..count.min(8) as usize {
                let e = c.saturating_add(8 + i * 20);
                let (key, words) = match (n64::read_u8(rom, e), n64::bytes_at(rom, e.saturating_add(4), 16)) {
                    (Ok(k), Ok(w)) => (k, w),
                    _ => {
                        problems.push(format!("custom bank entry {} is outside the ROM", i));
                        continue;
                    }
                };
                let word = |n: usize| read_u32(words, n * 4).unwrap_or(0) as u64;
                for (what, a, b) in [("textures", word(0), word(1)), ("objects", word(2), word(3))] {
                    if a == 0 && b == 0 {
                        continue;
                    }
                    let label = format!("custom banks of key 0x{:02X}, {}", key, what);
                    if a % 16 != 0 || b % 16 != 0 || b <= a || b > len {
                        problems.push(format!("{}: 0x{:X} to 0x{:X}: the start and the end must be multiples of 16 inside the ROM", label, a, b));
                        continue;
                    }
                    let data = &rom[a as usize..b as usize];
                    if what == "textures" {
                        check_texture_bank(data, &label, &mut problems);
                    } else {
                        check_object_bank(data, &label, &mut problems);
                    }
                }
            }
        }
    }
    problems
}

/// Breaks a good ROM in the ways the rules name and says, for each break, whether `check_rom` finds it. The ROM
/// must have the mod's custom banks. An error when it is too short to break.
pub fn self_test(rom: &[u8]) -> Result<Vec<(&'static str, bool)>, n64::OutOfBounds> {
    self_test_with(rom, tables::mod_info())
}

pub fn self_test_with(rom: &[u8], info: &Value) -> Result<Vec<(&'static str, bool)>, n64::OutOfBounds> {
    let base = hex(&info["mod_rom"]).unwrap_or(0) as usize;
    let c = base.saturating_add(info["custom"]["offset"].as_u64().unwrap_or(0) as usize);
    let entry = c.saturating_add(8);
    let ts = read_u32(rom, entry.saturating_add(4))?;
    let te = read_u32(rom, entry.saturating_add(8))?;
    let mut cases: Vec<(&'static str, Vec<u8>)> = Vec::new();

    let mut r = rom.to_vec();
    n64::write_u8(&mut r, 0x14, n64::read_u8(rom, 0x14)? ^ 1)?;
    cases.push(("wrong CRC", r));

    let mut r = rom.to_vec();
    r.push(0);
    cases.push(("ROM not a whole MiB", r));

    let mut r = rom.to_vec();
    n64::write_u32(&mut r, entry + 8, te.wrapping_sub(1))?;
    cases.push(("odd DMA length of a custom bank", r));

    let mut r = rom.to_vec();
    let size_at = (ts as usize).saturating_add(4 + 20);
    n64::write_u32(&mut r, size_at, read_u32(rom, size_at)?.wrapping_add(4))?;
    cases.push(("texture record not a multiple of 8", r));

    let mut r = rom.to_vec();
    n64::write_u32(&mut r, entry + 4, ts.wrapping_add(2))?;
    cases.push(("unaligned bank start", r));

    let os0 = read_u32(rom, entry + 12)? as usize;
    let inst = read_u32(rom, os0.saturating_add(4))?;
    let obj_at = os0.saturating_add((inst & 0xFFFFFF) as usize + 0x0C);
    let mut r = rom.to_vec();
    n64::write_u32(&mut r, obj_at, read_u32(rom, obj_at)?.wrapping_add(2))?;
    cases.push(("object pointer not aligned", r));

    Ok(cases.into_iter().map(|(name, data)| (name, !check_rom_with(&data, info).is_empty())).collect())
}

/// Whether every break of `self_test` is found.
pub fn self_test_passes(rom: &[u8]) -> bool {
    self_test(rom).map_or(false, |cases| cases.iter().all(|c| c.1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(d: &mut [u8], o: usize, v: u32) {
        n64::write_u32(d, o, v).unwrap();
    }

    fn tex(data: &[u8]) -> Vec<String> {
        let mut p = Vec::new();
        check_texture_bank(data, "T", &mut p);
        p
    }

    fn obj(data: &[u8]) -> Vec<String> {
        let mut p = Vec::new();
        check_object_bank(data, "O", &mut p);
        p
    }

    #[test]
    fn texture_banks() {
        assert_eq!(tex(&[0, 0]), ["T: texture bank is empty"]);
        assert!(tex(&[0, 0, 0, 0]).is_empty());
        // Two records of 0x28 bytes
        let mut d = vec![0u8; 4 + 0x50];
        put(&mut d, 0, 2);
        put(&mut d, 4 + 20, 0x28);
        put(&mut d, 4 + 0x28 + 20, 0x28);
        assert!(tex(&d).is_empty());
        put(&mut d, 4 + 0x28 + 20, 0x2C);
        assert_eq!(tex(&d), ["T: texture 1 has size 44: not a multiple of 8, so the pixels of the next one are not on a 64 bit boundary"]);
        put(&mut d, 4 + 0x28 + 20, 0x30);
        assert_eq!(tex(&d), ["T: the textures run past the end of the bank"]);
        put(&mut d, 0, 3);
        put(&mut d, 4 + 0x28 + 20, 0x28);
        assert_eq!(tex(&d), ["T: texture 2 starts past the end of the bank"]);
        put(&mut d, 4 + 20, 0x20);
        assert_eq!(tex(&d)[0], "T: texture 0 has size 32: not a multiple of 8, so the pixels of the next one are not on a 64 bit boundary");
    }

    /// A bank with one entry, one instance at 0x10 and one object at 0x40 with a mesh at 0x80.
    fn bank() -> Vec<u8> {
        let mut d = vec![0u8; 0x100];
        put(&mut d, 4, 0x0100_0010);
        put(&mut d, 0x10 + 0x0C, 0x40);
        put(&mut d, 0x40 + 0x14, 0x80);
        put(&mut d, 0x80 + 4, 0xC0);
        put(&mut d, 0x80 + 8, 0xC2);
        put(&mut d, 0x80 + 0xC, 0xC1);
        d
    }

    #[test]
    fn object_banks() {
        assert!(obj(&bank()).is_empty());
        assert_eq!(obj(&[]), ["O: the entry list has no end"]);
        assert!(obj(&[0u8; 8]).is_empty());

        let mut d = bank();
        put(&mut d, 0x10 + 0x0C, 0x42);
        assert_eq!(obj(&d), ["O: instance object points to offset 0x42: not 4 byte aligned"]);

        let mut d = bank();
        put(&mut d, 4, 0x12);
        assert_eq!(obj(&d), ["O: entry 0 points to 0x12: outside the bank or not aligned"]);
        put(&mut d, 4, 0xF0);
        assert_eq!(obj(&d), ["O: entry 0 points to 0xF0: outside the bank or not aligned"]);

        // The mesh pointer is read twice, as in Python
        let mut d = bank();
        put(&mut d, 0x40 + 0x14, 0x82);
        assert_eq!(obj(&d), ["O: object field mesh points to offset 0x82: not 4 byte aligned", "O: mesh points to offset 0x82: not 4 byte aligned"]);

        let mut d = bank();
        put(&mut d, 0x80 + 8, 0xC1);
        put(&mut d, 0x80 + 0x20, 0x1000);
        assert_eq!(
            obj(&d),
            ["O: mesh field faces points to offset 0xC1: not 2 byte aligned", "O: mesh field textures points to offset 0x1000: outside the bank"]
        );

        // An object near the end: its fields are read past the bank
        let mut d = bank();
        put(&mut d, 0x10 + 0x0C, 0xE0);
        let p = obj(&d);
        assert_eq!(p[0], "O: object field posKeys is read past the end of the bank");
        assert_eq!(p.last().unwrap(), "O: child or sibling is read past the end of the bank");
    }

    #[test]
    fn children_come_before_siblings_and_loops_end() {
        // 0x40 has child 0x60 and sibling 0xA0; 0x60 has a bad sibling; 0xA0 points back at 0x40 and has a bad child
        let mut d = vec![0u8; 0x100];
        put(&mut d, 4, 0x10);
        put(&mut d, 0x10 + 0x0C, 0x40);
        put(&mut d, 0x40 + 0x34, 0x60);
        put(&mut d, 0x40 + 0x38, 0xA0);
        put(&mut d, 0x60 + 0x38, 0x61);
        put(&mut d, 0xA0 + 0x34, 0x40);
        put(&mut d, 0xA0 + 0x38, 0x400);
        put(&mut d, 0xA0 + 0x1C, 0xA2);
        assert_eq!(
            obj(&d),
            [
                "O: child or sibling points to offset 0x61: not 4 byte aligned",
                "O: object field scaleKeys points to offset 0xA2: not 4 byte aligned",
                "O: child or sibling points to offset 0x400: outside the bank",
            ]
        );
    }

    #[test]
    fn a_short_rom_is_a_problem_not_a_panic() {
        let p = check_rom(&[0u8; 100]);
        assert_eq!(p.len(), 2);
        assert!(p[0].starts_with("the ROM is 100 bytes"));
        assert!(self_test(&[0u8; 100]).is_err());
        assert!(!self_test_passes(&[]));
    }

    #[test]
    fn an_empty_rom_of_the_right_size() {
        // 16 MiB of zeros with the right CRCs: the mod is not there, so only its tables are skipped
        let mut rom = vec![0u8; 0x1000000];
        n64::fix_header_crc(&mut rom).unwrap();
        assert!(check_rom(&rom).is_empty());
        rom[0x2000] = 1;
        let p = check_rom(&rom);
        assert_eq!(p.len(), 1);
        assert!(p[0].starts_with("header CRC is "));
        // Past the mod's place in an 8 MiB ROM
        let mut rom = vec![0u8; 0x800000];
        n64::fix_header_crc(&mut rom).unwrap();
        let p = check_rom(&rom);
        assert_eq!(p.len(), 1);
        assert!(p[0].starts_with("the mod (ROM 0xC00000, "));
    }
}
