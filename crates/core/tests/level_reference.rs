//! `level` against the reference dumps.
//!
//! Needs `GLOVEBOX_REFERENCE`: the folder with `levels/<id>.xml` and `.lev` for the 48 shipped levels, and
//! `level/variants.json`, the edited levels and what libgarib does with each. Without the variable the tests pass without doing anything.

use glovebox_core::level;
use std::path::PathBuf;

fn reference() -> Option<PathBuf> {
    match std::env::var_os("GLOVEBOX_REFERENCE") {
        Some(p) => Some(PathBuf::from(p)),
        None => {
            println!("skipped: GLOVEBOX_REFERENCE is not set");
            None
        }
    }
}

/// The 48 levels as (id, .lev bytes, XML text).
fn levels(dir: &PathBuf) -> Vec<(String, Vec<u8>, String)> {
    let mut out = Vec::new();
    let mut names: Vec<PathBuf> = std::fs::read_dir(dir.join("levels")).unwrap().map(|e| e.unwrap().path()).collect();
    names.sort();
    for p in names {
        if p.extension().is_some_and(|e| e == "lev") {
            let id = p.file_stem().unwrap().to_string_lossy().to_string();
            let xml = std::fs::read_to_string(p.with_extension("xml")).unwrap();
            out.push((id, std::fs::read(&p).unwrap(), xml));
        }
    }
    assert_eq!(out.len(), 48, "the reference has 48 levels");
    out
}

#[test]
fn every_level_disassembles_to_the_same_xml() {
    let Some(dir) = reference() else { return };
    for (id, lev, xml) in levels(&dir) {
        let doc = level::disassemble(&lev).unwrap_or_else(|e| panic!("level {}: {}", id, e));
        assert!(doc.pretty_no_newline() == xml, "level {}: the XML differs", id);
    }
}

#[test]
fn every_level_assembles_to_the_same_bytes() {
    let Some(dir) = reference() else { return };
    let path = dir.join("level").join("reassembled.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {}", path.display(), e));
    let recorded: serde_json::Value = serde_json::from_str(&text).unwrap();
    let (mut same_as_shipped, mut shorter, mut reordered) = (0, 0, Vec::new());
    for (id, lev, xml) in levels(&dir) {
        let hex = |b: &[u8]| b.iter().map(|x| format!("{:02x}", x)).collect::<String>();
        let expected = recorded[&id].as_str().unwrap();
        // From the tree that disassemble made, and from the reference XML text
        let doc = level::disassemble(&lev).unwrap();
        let mine = level::assemble(&doc).unwrap();
        assert!(hex(&mine) == expected, "level {}: the bytes differ from libgarib's", id);
        let doc = level::parse_level_xml(xml.as_bytes()).unwrap_or_else(|e| panic!("level {}: {}", id, e));
        assert!(level::assemble(&doc).unwrap() == mine, "level {}: the bytes differ from the XML text", id);
        let (bytes, needs_mod) = level::assemble_xml(xml.as_bytes()).unwrap();
        assert!(bytes == mine && !needs_mod, "level {}", id);
        // The shipped file holds the same commands. Many shipped files end in 8 more zero bytes: empty commands
        // that the XML leaves out. One level has an empty command between an object's commands; the XML puts it
        // after the object, as libgarib does.
        assert!(mine.len() <= lev.len() && lev.len() - mine.len() <= 8, "level {}", id);
        assert_eq!(lev[..4], (lev.len() as u32).to_be_bytes(), "level {}", id);
        if mine == lev {
            same_as_shipped += 1;
        } else if mine[4..] == lev[4..mine.len()] && lev[mine.len()..].iter().all(|b| *b == 0) {
            shorter += 1;
        } else {
            reordered.push(id);
        }
    }
    assert!(reordered.len() <= 1, "{:?}", reordered);
    println!("{} levels are 8 zero bytes shorter than the shipped file; other order: {:?}", shorter, reordered);
    println!("{} of 48 levels assemble to the shipped file byte for byte", same_as_shipped);
}

#[test]
fn every_level_passes_the_schema_check() {
    let Some(dir) = reference() else { return };
    for (id, _, xml) in levels(&dir) {
        let doc = level::parse_level_xml(xml.as_bytes()).unwrap();
        let problems = level::validate(&doc);
        assert!(problems.is_empty(), "level {}: {:?}", id, problems);
        assert!(!level::needs_mod(&doc), "level {}", id);
    }
}

#[test]
fn a_damaged_level_file_never_panics() {
    let Some(dir) = reference() else { return };
    for (_, lev, _) in levels(&dir).into_iter().step_by(7) {
        // Cut short, and with single bytes changed
        for n in (0..lev.len()).step_by(37) {
            let _ = level::disassemble(&lev[..n]);
        }
        let mut seed = 12345u32;
        for _ in 0..300 {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let mut bad = lev.clone();
            let at = (seed >> 8) as usize % bad.len();
            bad[at] ^= (seed & 0xFF) as u8 | 1;
            if let Ok(doc) = level::disassemble(&bad) {
                let _ = level::validate(&doc);
                let _ = level::assemble(&doc);
            }
        }
    }
}

#[test]
fn edited_levels_match_libgarib() {
    let Some(dir) = reference() else { return };
    let path = dir.join("level").join("variants.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {}", path.display(), e));
    let variants: serde_json::Value = serde_json::from_str(&text).unwrap();
    let variants = variants.as_array().unwrap();
    assert!(variants.len() > 2000);
    let mut failures = Vec::new();
    let (mut assembled, mut refused, mut valid, mut invalid) = (0, 0, 0, 0);
    for v in variants {
        let name = v["name"].as_str().unwrap();
        let xml = v["xml"].as_str().unwrap();
        let doc = match level::parse_level_xml(xml.as_bytes()) {
            Ok(doc) => doc,
            Err(e) => {
                if v["parses"].as_bool().unwrap() {
                    failures.push(format!("{}: libgarib parses it, this says {}", name, e));
                }
                continue;
            }
        };
        if !v["parses"].as_bool().unwrap() {
            failures.push(format!("{}: libgarib refuses to parse it", name));
            continue;
        }
        let problems = level::validate(&doc);
        assert!(problems.len() <= level::MAX_MESSAGES);
        let schema_ok = v["schema_ok"].as_bool().unwrap();
        if problems.is_empty() != schema_ok {
            failures.push(format!("{}: schema says {}, validate says {:?}", name, schema_ok, problems));
        }
        if schema_ok {
            valid += 1;
        } else {
            invalid += 1;
        }
        if level::needs_mod(&doc) != v["needs_mod"].as_bool().unwrap() {
            failures.push(format!("{}: needs_mod differs", name));
        }
        let expected = v["lev"].as_str();
        match (level::assemble(&doc), expected) {
            (Ok(bytes), Some(hex)) => {
                assembled += 1;
                let mine: String = bytes.iter().map(|b| format!("{:02x}", b)).collect();
                if mine != hex {
                    failures.push(format!("{}: the bytes differ", name));
                }
            }
            (Err(_), None) => refused += 1,
            (Ok(_), None) => failures.push(format!("{}: libgarib fails, Rust assembles", name)),
            (Err(e), Some(_)) => failures.push(format!("{}: libgarib assembles, Rust says {}", name, e)),
        }
    }
    println!("{} assembled, {} refused, {} valid, {} not valid", assembled, refused, valid, invalid);
    assert!(assembled > 500 && refused > 300 && valid > 500 && invalid > 300);
    assert!(failures.is_empty(), "{} of {} differ:\n{}", failures.len(), variants.len(), failures[..failures.len().min(40)].join("\n"));
}
