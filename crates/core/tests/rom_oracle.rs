//! The ROM side of the back end against the Python tools, on the real ROM.
//!
//! `GLOVEBOX_ROM` is the ROM's path. `GLOVEBOX_ORACLE` is the folder with the Python outputs: its `rom/` folder is
//! made by `rom/gen.py`, `rom/ptrs.py` and the Python app's ROM dump. Without the two variables every test here
//! prints that it was skipped and passes.

use glovebox_core::rom::{Key, Qol, Rom};
use glovebox_core::{bps, hwcheck, modpatch, n64, rom, tables};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

struct Env {
    rom: Vec<u8>,
    oracle: PathBuf,
}

fn env(test: &str) -> Option<Env> {
    match (std::env::var("GLOVEBOX_ROM"), std::env::var("GLOVEBOX_ORACLE")) {
        (Ok(rom), Ok(oracle)) => Some(Env { rom: std::fs::read(&rom).expect("GLOVEBOX_ROM can be read"), oracle: PathBuf::from(oracle).join("rom") }),
        _ => {
            println!("{}: skipped (GLOVEBOX_ROM and GLOVEBOX_ORACLE are not both set)", test);
            None
        }
    }
}

fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("{}: {}", path.display(), e))
}

fn json(path: &Path) -> Value {
    serde_json::from_slice(&read(path)).unwrap_or_else(|e| panic!("{}: {}", path.display(), e))
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
}

/// Compares two ROMs without printing megabytes when they differ.
fn same(name: &str, got: &[u8], want: &[u8]) {
    if got != want {
        let at = got.iter().zip(want).position(|(a, b)| a != b).unwrap_or(got.len().min(want.len()));
        panic!("{}: {} bytes, Python has {} bytes; the first difference is at 0x{:X}", name, got.len(), want.len(), at);
    }
}

impl Env {
    /// A ROM by its name in the reference data: "retail", a build case, or a kept file.
    fn named_rom(&self, name: &str) -> Vec<u8> {
        if name == "retail" {
            return self.rom.clone();
        }
        read(&self.oracle.join("build").join(format!("{}.z64", name.trim_end_matches(".z64"))))
    }

    fn input(&self, name: &str) -> Vec<u8> {
        read(&self.oracle.join("inputs").join(name))
    }
}

// ---------------------------------------------------------------------------------------------
// n64

#[test]
fn the_rom_is_the_supported_one() {
    let Some(e) = env("the_rom_is_the_supported_one") else { return };
    assert_eq!(n64::sha1_hex(&e.rom), n64::ROM_SHA1);
    assert!(n64::is_supported_rom(&e.rom));
    assert_eq!(n64::identify_cic(&e.rom), Some(6102));
    assert_eq!(n64::header_crc(&e.rom).unwrap(), (0x8E6E01FF, 0xCCB4F948));
    assert_eq!(n64::stored_crc(&e.rom).unwrap(), (0x8E6E01FF, 0xCCB4F948));

    // The two other byte orders give the same ROM back
    let mut v64 = e.rom.clone();
    v64.chunks_exact_mut(2).for_each(|p| p.swap(0, 1));
    let mut le = e.rom.clone();
    le.chunks_exact_mut(4).for_each(|w| w.reverse());
    assert_eq!(n64::supported_rom(&e.rom).unwrap(), e.rom);
    assert_eq!(n64::supported_rom(&v64).unwrap(), e.rom);
    assert_eq!(n64::supported_rom(&le).unwrap(), e.rom);
    assert!(!n64::is_supported_rom(&v64));
    let mut other = e.rom.clone();
    other[0x2000] ^= 1;
    assert!(matches!(n64::supported_rom(&other), Err(n64::RomFileError::WrongRom(_))));
    assert_eq!(n64::supported_rom(&e.rom[4..]), Err(n64::RomFileError::NotN64));
}

// ---------------------------------------------------------------------------------------------
// rom: reading

#[test]
fn pointer_pairs_decode_like_the_emulation() {
    let Some(e) = env("pointer_pairs_decode_like_the_emulation") else { return };
    let list = json(&e.oracle.join("ptrs.json"));
    let list = list.as_array().unwrap();
    assert_eq!(list.len(), 280);
    for p in list {
        let at = p["address"].as_u64().unwrap() as usize;
        let reg = rom::reg_number(p["reg"].as_str().unwrap()).unwrap();
        let (a, b) = (n64::read_u32(&e.rom, at).unwrap(), n64::read_u32(&e.rom, at + 4).unwrap());
        assert_eq!(a as u64, p["words"][0].as_u64().unwrap());
        assert_eq!(b as u64, p["words"][1].as_u64().unwrap());
        assert_eq!(rom::decode_pointer(a, b, reg).unwrap() as u64, p["value"].as_u64().unwrap(), "pair at 0x{:X}", at);
        // Any other register is refused
        assert!(rom::decode_pointer(a, b, reg % 31 + 1).is_err());
    }
}

/// The manifest of libgarib's `rom-asset-tool dump`: every region key with its file, or its files by index.
fn dump_manifest(dir: &Path) -> Vec<(String, Option<usize>, String)> {
    let text = String::from_utf8(read(&dir.join("manifest.yaml"))).unwrap();
    let mut out = Vec::new();
    let mut key = String::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("   ") {
            let (i, file) = rest.split_once(": ").unwrap();
            out.push((key.clone(), Some(i.parse().unwrap()), file.to_string()));
        } else if let Some(k) = line.strip_suffix(':') {
            key = k.to_string();
        } else {
            let (k, file) = line.rsplit_once(": ").unwrap();
            out.push((k.to_string(), None, file.to_string()));
        }
    }
    out
}

#[test]
fn regions_are_libgaribs_dump() {
    let Some(e) = env("regions_are_libgaribs_dump") else { return };
    let rom = Rom::parse(&e.rom).unwrap();
    assert!(rom.is_retail_checksum());
    assert_eq!(rom.regions().len(), 143);
    assert_eq!(rom.level_count(), 48);
    assert!(rom.regions().windows(2).all(|w| w[0].offset() <= w[1].offset()));

    // The dump the Python app made (cache/romdump): every region's bytes, by the key in its manifest
    let home = e.oracle.parent().unwrap().parent().unwrap().join("gbhome-py");
    let dump = home.join("cache/romdump");
    let dir = std::fs::read_dir(&dump).unwrap().next().unwrap().unwrap().path();
    let manifest = dump_manifest(&dir);
    let mut seen = std::collections::BTreeSet::new();
    let mut files = 0;
    for (key_text, index, file) in &manifest {
        let (key, none) = Key::parse(key_text).unwrap_or_else(|| panic!("key {}", key_text));
        assert_eq!(none, None);
        assert_eq!(&key.to_string(), key_text);
        let region = rom.region(&key).unwrap_or_else(|| panic!("no region {}", key_text));
        let want = read(&dir.join(file));
        match index {
            Some(i) => {
                assert!(region.is_array());
                same(file, rom.element(&key, *i).unwrap(), &want);
            }
            None => same(file, rom.bytes(&key).unwrap(), &want),
        }
        seen.insert(key);
        files += 1;
    }
    assert_eq!(seen.len(), 143);
    let elements: usize = rom.regions().iter().map(|r| if r.is_array() { r.elements().len() } else { 1 }).sum();
    assert_eq!(files, elements);

    // rom_keys.json: the bank name to key table the Python app derives from that manifest
    let keys = json(&e.oracle.parent().unwrap().join("rom_keys.json"));
    for (name, key_text) in keys.as_object().unwrap() {
        let (key, _) = Key::parse(key_text.as_str().unwrap()).unwrap();
        let bank = rom.bank(&key.data_type, name).unwrap_or_else(|| panic!("no bank {}", name));
        assert_eq!(bank.key(), &key, "bank {}", name);
    }
}

#[test]
fn regions_hold_noclips_split_files() {
    let Some(e) = env("regions_hold_noclips_split_files") else { return };
    let rom = Rom::parse(&e.rom).unwrap();
    let data = e.oracle.parent().unwrap().parent().unwrap().join("gbhome-py/data/Glover");
    let mut names: Vec<String> = std::fs::read_dir(&data).unwrap().map(|f| f.unwrap().file_name().into_string().unwrap()).collect();
    names.sort();
    assert_eq!(names.len(), 122);
    let (mut levels, mut objs, mut texs) = (0, 0, 0);
    for name in &names {
        let want = read(&data.join(name));
        if let Some(stem) = name.strip_suffix(".n64.lev") {
            // NN.NAME: the level's index and the name inside its file
            let (index, level_name) = stem.split_once('.').unwrap();
            let index: usize = index.parse().unwrap();
            // noclip drops the dot of the name inside the file ("HUB1.ln")
            assert_eq!(rom.level_name(index).map(|n| n.replace('.', "")).as_deref(), Some(level_name), "{}", name);
            same(name, rom.level(index).unwrap(), &want);
            levels += 1;
        } else if let Some(bank) = name.strip_suffix(".obj.fla") {
            same(name, rom.object_bank(bank).unwrap_or_else(|| panic!("no object bank {}", bank)), &want);
            objs += 1;
        } else if let Some(bank) = name.strip_suffix(".tex.fla") {
            same(name, rom.texture_bank(bank).unwrap_or_else(|| panic!("no texture bank {}", bank)), &want);
            texs += 1;
        } else {
            panic!("unexpected file {}", name);
        }
    }
    assert_eq!((levels, objs, texs), (48, 56, 18));
    // LEVELEND_TEX_BANK is two banks. noclip's file is number 14; a build replaces number 1, as the Python app does.
    assert_eq!(rom.texture_bank("LEVELEND_TEX_BANK"), rom.bytes(&Key::num("texbank", 14)));
    assert_eq!(rom.bank("texbank", "LEVELEND_TEX_BANK").unwrap().key(), &Key::num("texbank", 1));
    assert_ne!(rom.bytes(&Key::num("texbank", 1)), rom.bytes(&Key::num("texbank", 14)));
    assert_eq!(rom.texture_bank("JAPANESE_FONT_TEX_BANK"), None);
    // scenes.json names the same files
    for s in tables::scenes().as_array().unwrap() {
        let index = s["index"].as_u64().unwrap() as usize;
        let want = format!("{:02}.{}.n64.lev", index, rom.level_name(index).unwrap().replace('.', ""));
        assert_eq!(s["landscape"].as_str().unwrap(), want);
        for b in s["objectBanks"].as_array().unwrap() {
            assert!(rom.object_bank(b.as_str().unwrap().strip_suffix(".obj.fla").unwrap()).is_some());
        }
        for b in s["textureBanks"].as_array().unwrap() {
            assert!(rom.texture_bank(b.as_str().unwrap().strip_suffix(".tex.fla").unwrap()).is_some());
        }
    }
}

// ---------------------------------------------------------------------------------------------
// rom: building

#[test]
fn built_roms_are_pythons() {
    let Some(e) = env("built_roms_are_pythons") else { return };
    let cases = json(&e.oracle.join("build.json"));
    let cases = cases.as_array().unwrap();
    assert!(cases.len() >= 12);
    let parsed = Rom::parse(&e.rom).unwrap();
    let mut reparsed = 0;
    for c in cases {
        let name = c["name"].as_str().unwrap();
        let mut rom = parsed.clone();
        // rom-asset-tool applies the QoL patches first, then the manifest
        if let Some(level) = c["qol"].as_u64() {
            rom.apply_qol(&[Qol::CopySkip, Qol::QuickBoot(level as u16)]).unwrap();
        }
        for l in c["levels"].as_array().unwrap() {
            rom.replace_level(l[0].as_u64().unwrap() as usize, &e.input(l[1].as_str().unwrap())).unwrap();
        }
        for b in c["banks"].as_array().unwrap() {
            let (key, index) = Key::parse(b[0].as_str().unwrap()).unwrap();
            assert_eq!(index, None);
            rom.replace(&key, &e.input(b[1].as_str().unwrap())).unwrap();
        }
        let got = rom.finalize().unwrap();
        same(name, &got, &e.named_rom(name));
        assert_eq!(n64::sha1_hex(&got), c["sha1"].as_str().unwrap());
        assert_eq!(n64::header_crc(&got).unwrap(), n64::stored_crc(&got).unwrap());
        // Building twice gives the same ROM, and the built ROM parses again into the same regions
        same(name, &rom.finalize().unwrap(), &got);
        // A list of files that needed padding can't be read again: the padding looks like a file of length 0
        let Ok(again) = Rom::parse(&got) else {
            assert!(rom.region(&Key::landscapes()).unwrap().size() % 8 != 0, "{}", name);
            continue;
        };
        reparsed += 1;
        for b in rom.regions() {
            // The Japanese font has a fixed place in the map and the wave tables a fixed size: a rebuilt ROM moves them
            if b.start_is_pointer() && b.key().data_type != "audio_wave_table" {
                let a = again.region(b.key()).unwrap();
                let pad = |n: usize| (n + 7) & !7;
                assert_eq!(a.size(), pad(b.size()), "{} {}", name, a.key());
                if !a.is_array() {
                    assert_eq!(&a.bytes().unwrap()[..b.size()], b.bytes().unwrap(), "{} {}", name, a.key());
                }
            }
        }
    }
    assert!(reparsed >= 8);
}

#[test]
fn replace_refuses_wrong_keys() {
    let Some(e) = env("replace_refuses_wrong_keys") else { return };
    let mut rom = Rom::parse(&e.rom).unwrap();
    assert!(rom.replace(&Key::num("objbank", 16), b"x").is_err());
    assert!(rom.replace(&Key::landscapes(), b"x").is_err());
    assert!(rom.replace_level(48, b"x").is_err());
    assert!(rom.replace_element(&Key::num("objbank", 1), 0, b"x").is_err());
    assert!(rom.apply_qol(&[Qol::QuickBoot(0x8000)]).is_err());
    // A ROM cut short or with a changed pointer is an error, not a panic
    for len in [0usize, 0x17, 0x1000, 0x50000, 0x100000, 0x400000, 0x700000] {
        assert!(Rom::parse(&e.rom[..len]).is_err(), "length {}", len);
    }
    let mut bad = e.rom.clone();
    bad[0x50564] = 0x24;
    assert!(Rom::parse(&bad).is_err());
    let mut bad = e.rom.clone();
    bad[0x50564 + 1] = 0x03;
    assert!(Rom::parse(&bad).is_err());
}

// ---------------------------------------------------------------------------------------------
// modpatch

fn opt_str(v: &Value) -> Option<String> {
    v.as_str().map(str::to_string)
}

fn run_steps(e: &Env, base: &[u8], steps: &Value) -> Result<Vec<u8>, modpatch::ModError> {
    let mut rom = base.to_vec();
    for s in steps.as_array().unwrap() {
        if let Some(m) = s.get("mod") {
            let extra: Option<BTreeMap<i64, modpatch::ExtraBanks>> = m.get("extra").and_then(Value::as_object).map(|x| {
                x.iter()
                    .map(|(k, v)| {
                        let list = |v: &Value| v.as_array().unwrap().iter().map(|n| n.as_i64().unwrap()).collect::<Vec<_>>();
                        let first = v.get(2).and_then(Value::as_bool).unwrap_or(false);
                        (k.parse().unwrap(), modpatch::ExtraBanks { textures: list(&v[0]), objects: list(&v[1]), first })
                    })
                    .collect()
            });
            let custom: Option<BTreeMap<u8, modpatch::CustomBanks>> = m.get("custom").and_then(Value::as_object).map(|x| {
                x.iter()
                    .map(|(k, v)| {
                        let banks = modpatch::CustomBanks { textures: e.input(v[0].as_str().unwrap()), objects: e.input(v[1].as_str().unwrap()) };
                        (k.parse().unwrap(), banks)
                    })
                    .collect()
            });
            let slots = m.get("slots").filter(|s| s.is_object()).map(|s| modpatch::Slots {
                levels: s["levels"]
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(k, v)| {
                        let level = modpatch::SlotLevel {
                            lev: e.input(v["lev"].as_str().unwrap()),
                            name: opt_str(&v["name"]),
                            song: v["song"].as_u64().map(|n| n as u8),
                            bonus_time: v["bonus_time"].as_i64().map(|n| n as i16),
                        };
                        (k.parse().unwrap(), level)
                    })
                    .collect(),
                boot: s["boot"].as_bool().unwrap(),
                entry_hub: s["entry_hub"].as_u64().unwrap() as u8,
                entry_tag: s["entry_tag"].as_u64().unwrap() as u16,
            });
            rom = modpatch::apply_mod(&rom, extra.as_ref(), custom.as_ref(), slots.as_ref())?;
        } else if let Some(rows) = s.get("info") {
            let rows: Vec<(i64, modpatch::LevelInfo)> = rows
                .as_array()
                .unwrap()
                .iter()
                .map(|r| {
                    let info = modpatch::LevelInfo { name: opt_str(&r[1]["name"]), song: r[1]["song"].as_i64(), bonus_time: r[1]["bonus_time"].as_i64().map(|n| n as i16) };
                    (r[0].as_i64().unwrap(), info)
                })
                .collect();
            rom = modpatch::apply_level_info(&rom, &rows)?;
        } else {
            rom = modpatch::apply_easy_controls(&rom)?;
        }
    }
    Ok(rom)
}

#[test]
fn modded_roms_are_pythons() {
    let Some(e) = env("modded_roms_are_pythons") else { return };
    let cases = json(&e.oracle.join("mod.json"));
    let cases = cases.as_array().unwrap();
    assert!(cases.len() >= 49);
    let (mut good, mut bad) = (0, 0);
    for c in cases {
        let name = c["name"].as_str().unwrap();
        let base = e.named_rom(c["base"].as_str().unwrap());
        match (run_steps(&e, &base, &c["steps"]), c["error"].as_str()) {
            (Ok(rom), None) => {
                assert_eq!(rom.len() as u64, c["len"].as_u64().unwrap(), "{}", name);
                assert_eq!(n64::sha1_hex(&rom), c["sha1"].as_str().unwrap(), "{}", name);
                if let Some(file) = c["file"].as_str() {
                    same(name, &rom, &e.named_rom(file));
                }
                good += 1;
            }
            (Err(err), Some(want)) => {
                assert_eq!(err.0, want, "{}", name);
                bad += 1;
            }
            (Ok(_), Some(want)) => panic!("{}: Python refuses with {:?}", name, want),
            (Err(err), None) => panic!("{}: refused with {:?}, Python builds a ROM", name, err.0),
        }
    }
    assert!(good >= 20 && bad >= 25, "{} good, {} refused", good, bad);
}

// ---------------------------------------------------------------------------------------------
// hwcheck

fn strings(v: &Value) -> Vec<String> {
    v.as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect()
}

/// mod.json with the values of a case changed. A path's parts are joined with dots; null removes the value.
fn changed_info(changes: &Value) -> Value {
    let mut info = tables::mod_info().clone();
    for (path, value) in changes.as_object().unwrap() {
        let parts: Vec<&str> = path.split('.').collect();
        let mut d = &mut info;
        for p in &parts[..parts.len() - 1] {
            d = if d.is_array() { &mut d[p.parse::<usize>().unwrap()] } else { &mut d[*p] };
        }
        let last = parts[parts.len() - 1];
        if value.is_null() {
            d.as_object_mut().unwrap().shift_remove(last);
        } else if d.is_array() {
            d[last.parse::<usize>().unwrap()] = value.clone();
        } else {
            d[last] = value.clone();
        }
    }
    info
}

#[test]
fn hardware_findings_are_pythons() {
    let Some(e) = env("hardware_findings_are_pythons") else { return };
    let spec = json(&e.oracle.join("hw.json"));
    let mut roms: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for (name, file) in spec["files"].as_object().unwrap() {
        let rom = if name == "retail" { e.rom.clone() } else { e.named_rom(file.as_str().unwrap()) };
        roms.insert(name.clone(), rom);
    }
    let cases = spec["cases"].as_array().unwrap();
    assert!(cases.len() >= 100);
    let (mut clean, mut findings) = (0, 0);
    for c in cases {
        let name = c["name"].as_str().unwrap();
        let mut r = roms[c["base"].as_str().unwrap()].clone();
        for p in c["pokes"].as_array().unwrap() {
            n64::write_bytes(&mut r, p[0].as_u64().unwrap() as usize, &unhex(p[1].as_str().unwrap())).unwrap();
        }
        if let Some(n) = c["truncate"].as_u64() {
            r.truncate(n as usize);
        }
        r.resize(r.len() + c["append"].as_u64().unwrap() as usize, 0);
        if c["crc"].as_bool().unwrap() {
            n64::fix_header_crc(&mut r).unwrap();
        }
        let want = strings(&c["problems"]);
        let got = if c["info"].as_object().unwrap().is_empty() {
            assert_eq!(hwcheck::check_rom(&r), hwcheck::check_rom_with(&r, tables::mod_info()));
            hwcheck::check_rom(&r)
        } else {
            hwcheck::check_rom_with(&r, &changed_info(&c["info"]))
        };
        assert_eq!(got, want, "{}", name);
        if want.is_empty() {
            clean += 1;
        } else {
            findings += want.len();
        }
    }
    assert!(clean >= 6 && findings >= 100, "{} clean, {} findings", clean, findings);
}

#[test]
fn the_self_test_finds_every_break() {
    let Some(e) = env("the_self_test_finds_every_break") else { return };
    let spec = json(&e.oracle.join("selftest.json"));
    for (name, s) in spec.as_object().unwrap() {
        let rom = e.named_rom(&format!("mod_{}", name));
        assert!(hwcheck::check_rom(&rom).is_empty());
        assert_eq!(hwcheck::self_test_passes(&rom), s["passes"].as_bool().unwrap());
        let found = hwcheck::self_test(&rom).unwrap();
        let cases = hwcheck::self_test_cases(&rom, tables::mod_info()).unwrap();
        let want = s["cases"].as_array().unwrap();
        assert_eq!(cases.len(), want.len());
        assert_eq!(found.len(), want.len());
        for (((case, broken), f), w) in cases.iter().zip(&found).zip(want) {
            assert_eq!(*case, w[0].as_str().unwrap());
            assert_eq!(f.0, *case);
            assert_eq!(n64::sha1_hex(broken), w[1].as_str().unwrap(), "{}", case);
            let problems = hwcheck::check_rom(broken);
            assert_eq!(problems, strings(&w[2]), "{}", case);
            assert_eq!(f.1, !problems.is_empty());
            assert!(f.1, "{} is not found", case);
        }
    }
    // The self-test needs custom banks: a ROM without the mod gives an error or a failed test, never a panic
    assert!(!hwcheck::self_test_passes(&e.rom));
    assert!(!hwcheck::self_test_passes(&e.named_rom("mod_plain")));
}

// ---------------------------------------------------------------------------------------------
// bps

#[test]
fn patches_are_pythons() {
    let Some(e) = env("patches_are_pythons") else { return };
    let spec = json(&e.oracle.join("bps.json"));
    let dir = e.oracle.join("bps");
    let cases = spec["create"].as_array().unwrap();
    assert!(cases.len() >= 26);
    for c in cases {
        let name = c["name"].as_str().unwrap();
        let (source, target) = match c["source"].as_str() {
            Some(s) => (e.named_rom(s), e.named_rom(c["target"].as_str().unwrap())),
            None => (read(&dir.join(format!("{}.src", name))), read(&dir.join(format!("{}.tgt", name)))),
        };
        let metadata = unhex(c["metadata"].as_str().unwrap());
        let want = read(&dir.join(format!("{}.bps", name)));
        let got = bps::create(&source, &target, &metadata);
        same(name, &got, &want);
        assert_eq!(n64::sha1_hex(&got), c["sha1"].as_str().unwrap());
        // The patch gives the target back, and names its metadata
        same(name, &bps::apply(&source, &got).unwrap(), &target);
        assert_eq!(bps::metadata_of(&got), Some(&metadata[..]));
        // Damage is refused: a changed byte, a cut patch, another source
        for at in [0, 4, got.len() / 2, got.len() - 13, got.len() - 1] {
            let mut bad = got.clone();
            bad[at] ^= 0x04;
            assert!(bps::apply(&source, &bad).is_err(), "{}: a change at {} is accepted", name, at);
        }
        assert!(bps::apply(&source, &got[..got.len() - 1]).is_err());
        if source != target {
            assert!(bps::apply(&target, &got).is_err(), "{}: the target is accepted as the source", name);
        }
    }
    let cases = spec["apply"].as_array().unwrap();
    assert!(cases.len() >= 44);
    for c in cases {
        let name = c["name"].as_str().unwrap();
        let (source, patch) = (unhex(c["source"].as_str().unwrap()), unhex(c["patch"].as_str().unwrap()));
        let got = match c["max_target"].as_u64() {
            Some(n) => bps::apply_with_limit(&source, &patch, n as usize),
            None => bps::apply(&source, &patch),
        };
        match (got, c["error"].as_str()) {
            (Ok(out), None) => assert_eq!(out, unhex(c["ok"].as_str().unwrap()), "{}", name),
            (Err(err), Some(want)) => assert_eq!(err.0, want, "{}", name),
            (Ok(_), Some(want)) => panic!("{}: Python refuses with {:?}", name, want),
            (Err(err), None) => panic!("{}: refused with {:?}, Python applies it", name, err.0),
        }
    }
}
