//! Import model against the Python converter (`tools/custom_model.py` with libgarib's tools).
//!
//! The reference sets are made by `convert/make.py` in the folder `GLOVEBOX_ORACLE` names. Each has the glTF file,
//! its name and scale, and the Python converter's `meta.json`, object banks and texture bank.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use glovebox_core::custom::{self, CustomSet};
use glovebox_core::{objbank, texbank};
use serde_json::Value;

fn oracle(test: &str) -> Option<PathBuf> {
    match std::env::var_os("GLOVEBOX_ORACLE") {
        Some(dir) if Path::new(&dir).join("convert").is_dir() => Some(Path::new(&dir).join("convert")),
        _ => {
            println!("{test}: skipped, GLOVEBOX_ORACLE is not set or has no convert folder");
            None
        }
    }
}

fn json(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn ids(v: &Value) -> HashSet<u32> {
    v.as_array().unwrap().iter().map(|i| i.as_u64().unwrap() as u32).collect()
}

/// Every bank of a set loads back: the object banks through the bank parser and the model reader, the texture
/// bank through the texture reader.
fn loads_back(set: &CustomSet) {
    let bank = texbank::TexBank::parse(&set.texture_bank).unwrap();
    assert_eq!(bank.textures.len(), set.textures.len());
    for t in &bank.textures {
        let px = t.decode().unwrap();
        assert_eq!(px.len(), t.width as usize * t.height as usize * 4);
        assert_eq!(t.length as usize, texbank::HEADER + t.data.len());
    }
    let (records, _) = (texbank::records(&set.texture_bank).unwrap(), 0);
    assert_eq!(records.records.len(), set.textures.len());
    let size = |id: u32| bank.get(id).map(|t| (t.width, t.height));
    for p in &set.pieces {
        let parsed = objbank::Bank::parse(&p.bank).unwrap();
        assert_eq!(parsed.objects.len(), 1);
        assert_eq!(parsed.objects[0].id, p.id);
        let model = objbank::model(&parsed, p.id, &size).unwrap();
        let used: Vec<u32> = objbank::texture_ids(&parsed, p.id, &size).into_iter().collect();
        assert_eq!(used, p.textures, "textures of piece 0x{:08X}", p.id);
        assert!(!model.nodes.is_empty());
        assert_eq!(objbank::faces(&p.bank) as usize, p.triangles);
        let singles = objbank::split(&p.bank).unwrap();
        assert_eq!(singles.len(), 1);
        assert_eq!(singles[0].id, p.id);
    }
}

/// The largest difference between two texture banks' pixels, and the share of channel values that differ.
fn texture_difference(ours: &[u8], theirs: &[u8]) -> (u8, f64) {
    let (a, b) = (texbank::TexBank::parse(ours).unwrap(), texbank::TexBank::parse(theirs).unwrap());
    let (mut worst, mut differ, mut total) = (0u8, 0usize, 0usize);
    for (x, y) in a.textures.iter().zip(&b.textures) {
        assert_eq!((x.width, x.height), (y.width, y.height));
        for (p, q) in x.decode().unwrap().iter().zip(y.decode().unwrap().iter()) {
            worst = worst.max(p.abs_diff(*q));
            differ += (p != q) as usize;
            total += 1;
        }
    }
    (worst, differ as f64 / total.max(1) as f64)
}

#[test]
fn sets_match_python() {
    let Some(dir) = oracle("sets_match_python") else { return };
    let mut cases: Vec<PathBuf> =
        std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).filter(|p| p.join("case.json").is_file()).collect();
    cases.sort();
    assert!(cases.len() >= 30, "only {} reference sets", cases.len());
    let (mut failures, mut identical) = (Vec::new(), 0);
    for case in &cases {
        let name = case.file_name().unwrap().to_string_lossy().to_string();
        let args = json(&case.join("case.json"));
        let data = std::fs::read(case.join("input.glb")).unwrap();
        let set = match custom::convert(
            &data,
            args["name"].as_str().unwrap(),
            args["scale"].as_f64().unwrap(),
            &ids(&args["taken"]),
            &ids(&args["taken_tex"]),
        ) {
            Ok(s) => s,
            Err(e) => {
                failures.push(format!("{name}: {e}"));
                continue;
            }
        };
        loads_back(&set);
        let out = case.join("out");
        // meta.json: every field but the preview's file name and the textures' PNG sizes
        let mut theirs = json(&out.join("meta.json"));
        let mut ours = set.meta();
        for p in theirs["pieces"].as_array_mut().unwrap() {
            p["preview"] = "".into();
        }
        let keys = |v: &Value| v["textures"].as_object().unwrap().keys().cloned().collect::<Vec<_>>();
        let same_textures = keys(&theirs) == keys(&ours);
        theirs["textures"] = Value::Null;
        ours["textures"] = Value::Null;
        let mut problems = Vec::new();
        if custom::py_json(&theirs) != custom::py_json(&ours) || !same_textures {
            problems.push("meta.json differs".to_string());
        }
        for p in &set.pieces {
            let path = out.join(format!("0x{:08X}.obj.bin", p.id));
            match std::fs::read(&path) {
                Ok(bytes) if bytes == p.bank => {}
                Ok(_) => problems.push(format!("bank 0x{:08X} differs", p.id)),
                Err(_) => problems.push(format!("Python has no bank 0x{:08X}", p.id)),
            }
        }
        let their_bank = std::fs::read(out.join("textures.tex.bin")).unwrap();
        if their_bank != set.texture_bank {
            if their_bank.len() == set.texture_bank.len() && same_textures {
                let (worst, share) = texture_difference(&set.texture_bank, &their_bank);
                problems.push(format!(
                    "texture pixels differ: at most {worst} of 255 in a channel, {:.2}% of the values",
                    share * 100.0
                ));
            } else {
                problems.push("texture bank differs in size or IDs".to_string());
            }
        }
        if problems.is_empty() {
            identical += 1;
            println!("{name}: identical ({} pieces, {} textures)", set.pieces.len(), set.textures.len());
        } else {
            println!("{name}: {}", problems.join("; "));
            if !KNOWN_TEXTURE_DIFFERENCES.contains(&name.as_str())
                || problems.iter().any(|p| !p.starts_with("texture pixels differ"))
            {
                failures.push(format!("{name}: {}", problems.join("; ")));
            }
        }
    }
    println!("{identical} of {} sets are byte-identical", cases.len());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Sets whose textures come from JPEG files. The JPEG decoder is not Pillow's, so pixels can differ by a little.
const KNOWN_TEXTURE_DIFFERENCES: [&str; 0] = [];

#[test]
fn errors_match_python() {
    let Some(dir) = oracle("errors_match_python") else { return };
    let errors = json(&dir.join("errors.json"));
    for (name, e) in errors.as_object().unwrap() {
        let data = std::fs::read(dir.join("errors").join(format!("{name}.glb"))).unwrap();
        let got = custom::convert(&data, "model", e["scale"].as_f64().unwrap(), &HashSet::new(), &HashSet::new());
        let want = e["error"].as_str().unwrap_or_else(|| panic!("{name}: Python gave no error"));
        let got = got.err().unwrap_or_else(|| panic!("{name}: no error, Python says {want}")).0;
        // Pillow names a Python object in this one
        let want = want.split(" <_io.BytesIO").next().unwrap();
        assert_eq!(got, want, "{name}");
    }
}

/// Pictures against Pillow: the decoded pixels of each file, and the Lanczos resize of Pillow's own pixels.
#[test]
fn images_match_pillow() {
    use glovebox_core::custom::image::{decode, resize, Rgba};
    let Some(dir) = oracle("images_match_pillow") else { return };
    let dir = dir.join("images");
    let mut failures = Vec::new();
    for im in json(&dir.join("index.json")).as_array().unwrap() {
        let name = im["name"].as_str().unwrap();
        let read = |ext: &str| std::fs::read(dir.join(format!("{name}.{ext}"))).unwrap();
        let size = |key: &str| (im[key][0].as_u64().unwrap() as usize, im[key][1].as_u64().unwrap() as usize);
        let (w, h) = size("size");
        let rgba = Rgba { w, h, px: read("rgba") };
        if !im["format"].is_null() {
            let ours = decode(&read("src")).unwrap();
            assert_eq!((ours.w, ours.h), (w, h), "{name}");
            let worst = ours.px.iter().zip(&rgba.px).map(|(a, b)| a.abs_diff(*b)).max().unwrap_or(0);
            let differ = ours.px.iter().zip(&rgba.px).filter(|(a, b)| a != b).count();
            println!(
                "{name} ({} {}): decoded pixels differ by at most {worst}, {:.3}% of the values",
                im["format"].as_str().unwrap(),
                im["mode"].as_str().unwrap(),
                differ as f64 * 100.0 / rgba.px.len() as f64
            );
            if worst != 0 && im["format"] != "JPEG" {
                failures.push(format!("{name}: decoding differs"));
            }
        }
        let (tw, th) = size("out");
        if resize(&rgba, tw, th).px != read("out") {
            failures.push(format!("{name}: the resize to {tw} by {th} differs"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
