//! Import model against the reference dumps.
//!
//! The reference sets are in `convert/` in the folder `GLOVEBOX_REFERENCE` names. Each has the glTF file, its name
//! and scale, and the expected `meta.json`, object banks and texture bank.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use glovebox_core::custom::{self, CustomSet};
use glovebox_core::{objbank, texbank};
use serde_json::Value;

fn reference(test: &str) -> Option<PathBuf> {
    match std::env::var_os("GLOVEBOX_REFERENCE") {
        Some(dir) if Path::new(&dir).join("convert").is_dir() => Some(Path::new(&dir).join("convert")),
        _ => {
            println!("{test}: skipped, GLOVEBOX_REFERENCE is not set or has no convert folder");
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
fn sets_match_the_reference() {
    let Some(dir) = reference("sets_match_the_reference") else { return };
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
                Err(_) => problems.push(format!("the reference has no bank 0x{:08X}", p.id)),
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
            failures.push(format!("{name}: {}", problems.join("; ")));
        }
    }
    println!("{identical} of {} sets are byte-identical", cases.len());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn errors_match_the_reference() {
    let Some(dir) = reference("errors_match_the_reference") else { return };
    let errors = json(&dir.join("errors.json"));
    for (name, e) in errors.as_object().unwrap() {
        let data = std::fs::read(dir.join("errors").join(format!("{name}.glb"))).unwrap();
        let got = custom::convert(&data, "model", e["scale"].as_f64().unwrap(), &HashSet::new(), &HashSet::new());
        let want = e["error"].as_str().unwrap_or_else(|| panic!("{name}: the reference has no error"));
        let got = got.err().unwrap_or_else(|| panic!("{name}: no error, the reference says {want}")).0;
        // One reference text goes on with the name of an object in memory. That part is cut off
        let want = want.split(" <_io.BytesIO").next().unwrap();
        assert_eq!(got, want, "{name}");
    }
}

/// Pictures against Pillow: the decoded pixels of each file, and the Lanczos resize of Pillow's own pixels.
#[test]
fn images_match_pillow() {
    use glovebox_core::custom::image::{decode, resize, Rgba};
    let Some(dir) = reference("images_match_pillow") else { return };
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
            if worst != 0 {
                failures.push(format!("{name}: decoding differs"));
            }
        }
        let (tw, th) = size("out");
        let (ours, theirs) = (resize(&rgba, tw, th).px, read("out"));
        if ours != theirs {
            let at: Vec<usize> = (0..ours.len()).filter(|i| ours[*i] != theirs[*i]).collect();
            let worst = at.iter().map(|i| ours[*i].abs_diff(theirs[*i])).max().unwrap_or(0);
            failures.push(format!(
                "{name}: the resize to {tw} by {th} differs in {} values by at most {worst}, first at x {} y {} channel {}",
                at.len(), at[0] / 4 % tw, at[0] / 4 / tw, at[0] % 4
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn temp_home(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("glovebox-custom-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// `import_custom` stores what `convert` makes, under the expected set name, and the reference
/// sets load through `custom_sets` and `custom_catalog`.
#[test]
fn storage_matches_the_reference() {
    let Some(dir) = reference("storage_matches_the_reference") else { return };
    let home = temp_home("storage");
    let case = dir.join("testmodel");
    let data = std::fs::read(case.join("input.glb")).unwrap();
    let none = HashSet::new();
    let set = custom::import_custom(&home, &data, "model", 1.0, &none, &none).unwrap();
    // The set's name: the first 10 hex digits of the SHA-1 of the file and the scale's text
    let mut hashed = data.clone();
    hashed.extend_from_slice(b"1.0");
    assert_eq!(set, glovebox_core::n64::sha1_hex(&hashed)[..10]);
    let stored = home.join("custom").join(&set);
    let out = case.join("out");
    let mut names: Vec<String> = std::fs::read_dir(&stored).unwrap().flatten().map(|e| e.file_name().into_string().unwrap()).collect();
    names.sort();
    let theirs = json(&out.join("meta.json"));
    let mut expect: Vec<String> = theirs["pieces"].as_array().unwrap().iter().map(|p| format!("{}.obj.bin", p["id"].as_str().unwrap())).collect();
    expect.extend(["meta.json".to_string(), "textures.tex.bin".to_string()]);
    expect.sort();
    assert_eq!(names, expect);
    for name in names.iter().filter(|n| n.ends_with(".bin")) {
        assert!(std::fs::read(stored.join(name)).unwrap() == std::fs::read(out.join(name)).unwrap(), "{name}");
    }
    // The text of meta.json is the reference's, but for the preview's file name and the PNG sizes
    let ours = std::fs::read_to_string(stored.join("meta.json")).unwrap();
    let mut theirs_same = theirs.clone();
    for p in theirs_same["pieces"].as_array_mut().unwrap() {
        p["preview"] = "".into();
    }
    theirs_same["textures"] = json(&stored.join("meta.json"))["textures"].clone();
    assert_eq!(ours, custom::py_json(&theirs_same));
    // py_json writes the reference file back byte for byte
    assert_eq!(custom::py_json(&theirs), std::fs::read_to_string(out.join("meta.json")).unwrap());
    // A second import of the same file gives the same set and writes nothing
    let before = std::fs::metadata(stored.join("meta.json")).unwrap().modified().unwrap();
    assert_eq!(custom::import_custom(&home, &data, "other name", 1.0, &none, &none).unwrap(), set);
    assert_eq!(std::fs::metadata(stored.join("meta.json")).unwrap().modified().unwrap(), before);
    // The same file at another scale is another set, and its IDs avoid the first set's
    let second = custom::import_custom(&home, &data, "model", 2.0, &none, &none).unwrap();
    assert_ne!(second, set);
    let catalog = custom::custom_catalog(&home);
    let ids: HashSet<&str> = catalog.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids.len(), catalog.len());
    assert_eq!(catalog.len(), 8);
    assert!(!home.join("custom").join(format!("{set}.tmp")).exists());

    // A reference set, copied into the user folder
    let copied_set = home.join("custom").join("0123456789");
    std::fs::create_dir_all(&copied_set).unwrap();
    for entry in std::fs::read_dir(dir.join("big").join("out")).unwrap().flatten() {
        if entry.path().is_file() {
            std::fs::copy(entry.path(), copied_set.join(entry.file_name())).unwrap();
        }
    }
    let sets = custom::custom_sets(&home);
    assert_eq!(sets.len(), 3);
    assert_eq!(sets["0123456789"]["name"], "big sphere");
    let catalog = custom::custom_catalog(&home);
    let big: Vec<_> = catalog.iter().filter(|c| c.custom == "0123456789").collect();
    assert_eq!(big.len(), 8);
    assert_eq!(big[2].name, "big sphere 3/8");
    assert_eq!(big[2].bank, "Custom: big sphere");
    assert_eq!(big[2].tex_bank, "custom:0123456789");
    assert_eq!(big[2].bytes, std::fs::metadata(&big[2].path).unwrap().len());
    assert!(custom::delete_custom(&home, "0123456789"));
    assert!(!custom::delete_custom(&home, "../escape"));
    assert_eq!(custom::custom_sets(&home).len(), 2);
    let _ = std::fs::remove_dir_all(&home);
}

/// `reskin` against the reference: the same set, new ID, bank, texture bank and `meta.json`.
#[test]
fn reskin_matches_the_reference() {
    let Some(dir) = reference("reskin_matches_the_reference") else { return };
    let dir = dir.join("reskin");
    let cases = json(&dir.join("index.json"));
    assert!(cases.as_array().unwrap().len() >= 3);
    for case in cases.as_array().unwrap() {
        let d = dir.join(case.as_str().unwrap());
        let c = json(&d.join("case.json"));
        let strings = |v: &Value| v.as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect::<Vec<_>>();
        let bank = std::fs::read(d.join("bank.bin")).unwrap();
        let (textures, pool, taken) = (strings(&c["textures"]), strings(&c["pool"]), ids(&c["taken"]));
        let model = custom::Reskin {
            oid: c["oid"].as_str().unwrap(),
            name: c["name"].as_str().unwrap(),
            world: c["world"].as_str().unwrap(),
            bank: &bank,
            textures: &textures,
            pool: &pool,
            taken_ids: &taken,
        };
        let worlds = strings(&c["worlds"]);
        let worlds: Vec<&str> = worlds.iter().map(String::as_str).collect();
        let record = |t: &str| {
            let hex = c["records"][t].as_str()?;
            Some((0..hex.len() / 2).map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap()).collect::<Vec<u8>>())
        };
        let home = temp_home(&format!("reskin{}", case.as_str().unwrap()));
        let set = custom::reskin(&home, &model, &worlds, c["seed"].as_i64().unwrap(), &record).unwrap();
        assert_eq!(set, c["set"].as_str().unwrap());
        let (ours, theirs) = (home.join("custom").join(&set), d.join("custom").join(&set));
        let mut meta = json(&theirs.join("meta.json"));
        meta["pieces"][0]["preview"] = "".into();
        assert_eq!(std::fs::read_to_string(ours.join("meta.json")).unwrap(), custom::py_json(&meta));
        let id = meta["pieces"][0]["id"].as_str().unwrap();
        for name in [format!("{id}.obj.bin"), "textures.tex.bin".to_string()] {
            assert!(std::fs::read(ours.join(&name)).unwrap() == std::fs::read(theirs.join(&name)).unwrap(), "{name}");
        }
        // The copy loads back, with the new textures
        let data = std::fs::read(ours.join(format!("{id}.obj.bin"))).unwrap();
        let parsed = objbank::Bank::parse(&data).unwrap();
        assert_eq!(format!("0x{:08X}", parsed.objects[0].id), id);
        let tex = std::fs::read(ours.join("textures.tex.bin")).unwrap();
        assert_eq!(texbank::TexBank::parse(&tex).unwrap().textures.len(), meta["textures"].as_object().unwrap().len());
        assert_eq!(custom::reskin(&home, &model, &worlds, c["seed"].as_i64().unwrap(), &record).unwrap(), set);
        let _ = std::fs::remove_dir_all(&home);
    }
}

/// Damaged files give an error or a model, and never a panic. Each reference model and picture is cut short and
/// has bytes changed, by a fixed rule.
#[test]
fn damaged_files_do_not_panic() {
    let Some(dir) = reference("damaged_files_do_not_panic") else { return };
    let none = HashSet::new();
    let mut state = 0x2545F491u32;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        state
    };
    let (mut tried, mut failed) = (0, 0);
    for case in ["testmodel_small", "formats", "jpeg", "variants", "kit_sign"] {
        let data = std::fs::read(dir.join(case).join("input.glb")).unwrap();
        // The JSON chunk starts at byte 20: damage there, and anywhere
        let json_len = u32::from_le_bytes(data[12..16].try_into().unwrap()) as usize;
        for round in 0..300 {
            let mut d = data.clone();
            match round % 4 {
                0 => d.truncate(next() as usize % d.len()),
                1 => {
                    for _ in 0..1 + next() % 4 {
                        let at = 20 + next() as usize % json_len;
                        d[at] = b"0123456789-,:[]{}\"e."[next() as usize % 20];
                    }
                }
                _ => {
                    for _ in 0..1 + next() % 8 {
                        let at = next() as usize % d.len();
                        d[at] = next() as u8;
                    }
                }
            }
            tried += 1;
            match custom::convert(&d, "damaged", 1.0, &none, &none) {
                Ok(set) => loads_back(&set),
                Err(_) => failed += 1,
            }
        }
    }
    let images = dir.join("images");
    for im in json(&images.join("index.json")).as_array().unwrap() {
        if im["format"].is_null() {
            continue;
        }
        let data = std::fs::read(images.join(format!("{}.src", im["name"].as_str().unwrap()))).unwrap();
        for round in 0..60 {
            let mut d = data.clone();
            if round % 3 == 0 {
                d.truncate(next() as usize % d.len());
            } else {
                for _ in 0..1 + next() % 6 {
                    let at = next() as usize % d.len();
                    d[at] = next() as u8;
                }
            }
            tried += 1;
            failed += custom::image::decode(&d).is_err() as usize;
        }
    }
    println!("{tried} damaged files: {failed} refused, {} read", tried - failed);
}

/// A made-up model, without the reference sets: two triangles with a colour, as `.gltf` text with a data URI and
/// as `.glb`. Both convert, load back, and differ only in what comes from the hash of the file.
#[test]
fn made_up_model_converts() {
    let mut bin = Vec::new();
    for v in [[0.0f32, 0.0, 0.0], [10.0, 0.0, 0.0], [10.0, 0.0, 10.0], [0.0, 0.0, 10.0]] {
        bin.extend(v.iter().flat_map(|c| c.to_le_bytes()));
    }
    bin.extend([0u16, 2, 1, 0, 3, 2].iter().flat_map(|i| i.to_le_bytes()));
    const B64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut uri = String::from("data:application/octet-stream;base64,");
    for c in bin.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for k in 0..4 {
            uri.push(if k <= c.len() { B64[(n >> (18 - 6 * k)) as usize & 63] as char } else { '=' });
        }
    }
    let doc = |buffer: Value| {
        serde_json::json!({
            "asset": {"version": "2.0"}, "scene": 0, "scenes": [{"nodes": [0]}],
            "nodes": [{"mesh": 0, "translation": [5.0, 1.0, 0.0]}],
            "meshes": [{"primitives": [{"attributes": {"POSITION": 0}, "indices": 1, "material": 0}]}],
            "materials": [{"pbrMetallicRoughness": {"baseColorFactor": [1.0, 0.5, 0.25, 1.0]}}],
            "accessors": [
                {"bufferView": 0, "componentType": 5126, "count": 4, "type": "VEC3"},
                {"bufferView": 1, "componentType": 5123, "count": 6, "type": "SCALAR"},
            ],
            "bufferViews": [{"buffer": 0, "byteLength": 48}, {"buffer": 0, "byteOffset": 48, "byteLength": 12}],
            "buffers": [buffer],
        })
        .to_string()
    };
    let text = doc(serde_json::json!({"byteLength": 60, "uri": uri}));
    let mut json = doc(serde_json::json!({"byteLength": 60})).into_bytes();
    json.resize(json.len().next_multiple_of(4), b' ');
    let mut glb = b"glTF".to_vec();
    glb.extend(2u32.to_le_bytes());
    glb.extend((12 + 8 + json.len() as u32 + 8 + 60).to_le_bytes());
    glb.extend((json.len() as u32).to_le_bytes());
    glb.extend(b"JSON");
    glb.extend(&json);
    glb.extend(60u32.to_le_bytes());
    glb.extend(b"BIN\0");
    glb.extend(&bin);

    let none = HashSet::new();
    let a = custom::convert(text.as_bytes(), "Flat tile!", 2.0, &none, &none).unwrap();
    let b = custom::convert(&glb, "Flat tile!", 2.0, &none, &none).unwrap();
    for set in [&a, &b] {
        loads_back(set);
        assert_eq!(set.triangles, 2);
        assert_eq!(set.size, [20.0, 0.0, 20.0]);
        assert_eq!(set.pieces.len(), 1);
        assert_eq!(set.pieces[0].name, "Flattile");
        assert_eq!(set.pieces[0].offset, [20.0, 2.0, 10.0]);
        // The colour becomes an 8 by 8 texture of 16-bit pixels
        assert_eq!(set.textures.len(), 1);
        assert_eq!(set.textures[0].1, texbank::HEADER + 8 * 8 * 2);
        let bank = texbank::TexBank::parse(&set.texture_bank).unwrap();
        assert_eq!(&bank.textures[0].data[..2], &(31u16 << 11 | 15 << 6 | 7 << 1 | 1).to_be_bytes());
    }
    assert_eq!(a.pieces[0].bank.len(), b.pieces[0].bank.len());
    assert_ne!(a.pieces[0].id, b.pieces[0].id);
    // An ID that is taken is stepped over
    let taken: HashSet<u32> = [a.pieces[0].id].into();
    let taken_tex: HashSet<u32> = [a.textures[0].0].into();
    let c = custom::convert(text.as_bytes(), "Flat tile!", 2.0, &taken, &taken_tex).unwrap();
    assert_ne!(c.pieces[0].id, a.pieces[0].id);
    assert_eq!(c.textures[0].0, a.textures[0].0 + 1);
    assert_eq!(custom::convert(text.as_bytes(), "x", 0.0, &none, &none).unwrap_err().0, "The scale must be a positive number");
    assert_eq!(custom::set_id(b"abc", 1.0).len(), 10);
}
