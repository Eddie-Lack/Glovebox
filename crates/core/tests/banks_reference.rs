//! The object bank and texture bank code against the reference dumps, on all the game's banks.
//!
//! `GLOVEBOX_REFHOME` names a folder: its `data/Glover` holds the split ROM, its `cache` the files libgarib's tools
//! wrote. `GLOVEBOX_REFERENCE` names the folder of reference dumps. These tests read its `banks/` folder. Without
//! the two variables every test here prints that it was skipped and passes.

use glovebox_core::model::{AlphaMode, Model, Wrap};
use glovebox_core::objbank::{self, Bank};
use glovebox_core::texbank::{self, TexBank};
use glovebox_core::fla2;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::Instant;

fn dirs(test: &str) -> Option<(PathBuf, PathBuf)> {
    match (std::env::var_os("GLOVEBOX_REFHOME"), std::env::var_os("GLOVEBOX_REFERENCE")) {
        (Some(home), Some(reference)) => Some((PathBuf::from(home), PathBuf::from(reference))),
        _ => {
            println!("{test}: skipped, GLOVEBOX_REFHOME and GLOVEBOX_REFERENCE are not set");
            None
        }
    }
}

fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn json(path: &Path) -> Value {
    serde_json::from_slice(&read(path)).unwrap()
}

/// A bank of the split ROM, decompressed.
fn bank(home: &Path, file: &str) -> Vec<u8> {
    fla2::data(&read(&home.join("data/Glover").join(file))).unwrap()
}

fn listing(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

fn banks_with(home: &Path, ending: &str) -> Vec<String> {
    listing(&home.join("data/Glover")).into_iter().filter(|f| f.ends_with(ending)).collect()
}

fn hex(id: u32) -> String {
    format!("0x{id:08X}")
}

fn unhex(text: &str) -> u32 {
    u32::from_str_radix(&text[2..10], 16).unwrap()
}

/// A PNG file as RGBA pixels.
fn png_rgba(path: &Path) -> (u32, u32, Vec<u8>) {
    let mut decoder = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).unwrap()));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    buf.truncate(info.buffer_size());
    assert_eq!(info.bit_depth, png::BitDepth::Eight, "{}", path.display());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf.chunks_exact(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&p| [p, p, p, 255]).collect(),
        other => panic!("{}: {other:?}", path.display()),
    };
    (info.width, info.height, rgba)
}

#[test]
fn texture_ids_and_pixels() {
    let Some((home, reference)) = dirs("texture_ids_and_pixels") else { return };
    let want_ids = json(&reference.join("texture_ids.json"));
    let banks = banks_with(&home, ".tex.fla");
    assert_eq!(banks.len(), want_ids.as_object().unwrap().len());
    let (mut textures, mut formats) = (0, BTreeMap::new());
    for file in &banks {
        let data = bank(&home, file);
        let parsed = TexBank::parse(&data).unwrap();
        let ids: BTreeSet<String> = texbank::ids(&data).unwrap().into_iter().map(hex).collect();
        let want: BTreeSet<String> =
            want_ids[file].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
        assert_eq!(ids, want, "{file}");
        let dir = home.join("cache/textures").join(file.split('.').next().unwrap());
        let pngs: Vec<String> = listing(&dir).into_iter().filter(|f| f.ends_with(".png")).collect();
        assert_eq!(pngs.len(), ids.len(), "{file}");
        for (i, t) in parsed.textures.iter().enumerate() {
            assert!(parsed.filenames[i].is_empty(), "{file}");
            // A later texture with the same id overwrote the earlier one's PNG.
            if parsed.textures[i + 1..].iter().any(|o| o.id == t.id) {
                continue;
            }
            let (w, h, rgba) = png_rgba(&dir.join(format!("{}.png", hex(t.id))));
            assert_eq!((w, h), (t.width as u32, t.height as u32), "{file} {}", hex(t.id));
            assert!(t.decode().unwrap() == rgba, "{file} {} pixels", hex(t.id));
            *formats.entry((t.color_format, t.compression_format)).or_insert(0) += 1;
            textures += 1;
        }
    }
    println!("texture_ids_and_pixels: {} banks, {textures} textures equal their PNG; (color, size) counts {formats:?}", banks.len());
}

#[test]
fn texture_banks_written() {
    let Some((home, reference)) = dirs("texture_banks_written") else { return };
    let dir = reference.join("banks/tex");
    let cases = json(&dir.join("cases.json"));
    let mut datas: HashMap<String, Vec<u8>> = HashMap::new();
    for file in banks_with(&home, ".tex.fla") {
        datas.insert(file.clone(), bank(&home, &file));
    }
    let mut n = 0;
    for case in cases.as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let want = read(&dir.join(format!("{name}.bin")));
        let mut extra: Vec<(u32, Vec<u8>)> = Vec::new();
        for pair in case["textures"].as_array().unwrap() {
            let (id, source) = (unhex(pair[0].as_str().unwrap()), pair[1].as_str().unwrap());
            let records = texbank::records(&datas[source]).unwrap();
            // The last record of an id wins.
            let rec = records.records.iter().rev().find(|(i, _)| *i == id).unwrap();
            extra.push((id, rec.1.to_vec()));
        }
        let extra: Vec<(u32, &[u8])> = extra.iter().map(|(i, r)| (*i, r.as_slice())).collect();
        let got = match case["target"].as_str() {
            Some(target) => texbank::import_bank(&datas[target], &extra).unwrap(),
            None => texbank::bank_of(&extra),
        };
        assert!(got == want, "{name}");
        n += 1;
    }
    println!("texture_banks_written: {n} banks equal the reference's");
}

#[test]
fn split_all_banks() {
    let Some((home, reference)) = dirs("split_all_banks") else { return };
    let faces = json(&reference.join("banks/faces.json"));
    let (mut objects, mut same_as_cache) = (0, 0);
    let mut uncached: Vec<String> = Vec::new();
    for file in banks_with(&home, ".obj.fla") {
        let stem = file.strip_suffix(".obj.fla").unwrap();
        let cached = home.join("cache/split").join(stem);
        if !cached.is_dir() {
            // No scene loads the bank, so the reference has no split of it. It must still split.
            uncached.push(format!("{stem} ({})", objbank::split(&bank(&home, &file)).unwrap().len()));
            continue;
        }
        let relinked = reference.join("banks/relink").join(stem);
        // The compressed file and its data give the same result.
        let raw = read(&home.join("data/Glover").join(&file));
        let singles = objbank::split(&raw).unwrap();
        assert_eq!(singles.len(), listing(&cached).len(), "{stem}");
        for s in &singles {
            let name = format!("0x{:08x}-{}.obj.bin", s.id, s.name);
            let theirs = read(&cached.join(&name));
            assert_eq!(s.bytes.len(), theirs.len(), "{stem}/{name}");
            assert!(s.bytes == read(&relinked.join(&name)), "{stem}/{name} differs from libgarib's relink");
            // The cached file has the same segments in another order: relinked here it gives the same bytes.
            assert!(objbank::pack(&theirs, &[] as &[&[u8]]).unwrap() == s.bytes, "{stem}/{name}");
            same_as_cache += (s.bytes == theirs) as usize;
            let want = faces[format!("{stem}/{name}")].as_u64().unwrap() as u32;
            assert_eq!(objbank::faces(&s.bytes), want, "{stem}/{name}");
            assert_eq!(objbank::faces(&theirs), want, "{stem}/{name}");
            objects += 1;
        }
    }
    println!(
        "split_all_banks: {objects} objects equal libgarib's relink of the cached file; {same_as_cache} also equal \
         the cached file byte for byte (libgarib's segment order changes from run to run); banks the reference has no \
         split of (objects): {uncached:?}"
    );
}

#[test]
fn pack_cases() {
    let Some((home, reference)) = dirs("pack_cases") else { return };
    let dir = reference.join("banks/pack");
    let cases = json(&dir.join("cases.json"));
    let (mut same, mut tool_failed) = (0, 0);
    for case in cases.as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let base = read(&home.join("data/Glover").join(format!("{}.obj.fla", case["target"].as_str().unwrap())));
        let mut cached: Vec<Vec<u8>> = Vec::new();
        let mut own: Vec<Vec<u8>> = Vec::new();
        for pair in case["singles"].as_array().unwrap() {
            let (stem, file) = (pair[0].as_str().unwrap(), pair[1].as_str().unwrap());
            cached.push(read(&home.join("cache/split").join(stem).join(file)));
            let id = u32::from_str_radix(&file[2..10], 16).unwrap();
            let singles = objbank::split(&bank(&home, &format!("{stem}.obj.fla"))).unwrap();
            own.push(singles.into_iter().find(|s| s.id == id).unwrap().bytes);
        }
        let got = objbank::pack(&base, &cached).unwrap();
        assert!(got == objbank::pack(&base, &own).unwrap(), "{name}: singles of this split");
        if case["ok"].as_bool().unwrap() {
            assert!(got == read(&dir.join(format!("{name}.obj.bin"))), "{name}");
            same += 1;
        } else {
            // objbank-tool reads a file that starts with '{' as glTF and fails. An object id may start with 0x7B.
            assert!(cached.iter().any(|c| c[0] == b'{'), "{name}");
            tool_failed += 1;
        }
    }
    println!("pack_cases: {same} packed banks equal objbank-tool's; {tool_failed} more that objbank-tool fails on");
}

/// The sizes of the textures of these banks. A later bank wins, as in libgarib's texture database.
fn sizes(home: &Path, banks: &[String]) -> HashMap<u32, (u16, u16)> {
    let mut out = HashMap::new();
    for file in banks {
        let data = bank(home, file);
        for t in TexBank::parse(&data).unwrap().textures {
            out.insert(t.id, (t.width, t.height));
        }
    }
    out
}

#[test]
fn object_texture_ids() {
    let Some((home, reference)) = dirs("object_texture_ids") else { return };
    let want = json(&reference.join("object_texture_ids.json"));
    let catalog = json(&reference.join("catalog.json"));
    let known = sizes(&home, &banks_with(&home, ".tex.fla"));
    let lookup = |id: u32| known.get(&id).copied();
    let mut by_bank: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (oid, entry) in catalog.as_object().unwrap() {
        by_bank.entry(entry["bank"].as_str().unwrap().to_string()).or_default().push(oid.clone());
    }
    assert_eq!(want.as_object().unwrap().len(), catalog.as_object().unwrap().len());
    let (mut n, mut empty, mut unknown) = (0, 0, 0);
    for (stem, oids) in &by_bank {
        let data = bank(&home, &format!("{stem}.obj.fla"));
        let full = Bank::parse(&data).unwrap();
        let singles = objbank::split(&data).unwrap();
        for oid in oids {
            let id = unhex(oid);
            let single = &singles.iter().find(|s| s.id == id).unwrap().bytes;
            let got = objbank::texture_ids(&Bank::parse(single).unwrap(), id, &lookup);
            let theirs: BTreeSet<u32> =
                want[oid].as_array().unwrap().iter().map(|v| unhex(v.as_str().unwrap())).collect();
            assert_eq!(got, theirs, "{stem} {oid}");
            assert_eq!(objbank::texture_ids(&full, id, &lookup), theirs, "{stem} {oid} in the full bank");
            empty += got.is_empty() as usize;
            unknown += got.iter().filter(|t| !known.contains_key(t)).count();
            n += 1;
        }
    }
    println!("object_texture_ids: {n} objects equal; {empty} have none; {unknown} ids are in no texture bank");
}

// ---------------------------------------------------------------------------------------------------------------
// A minimal reader of the .glb files libgarib wrote

struct Glb {
    json: Value,
    bin: Vec<u8>,
}

impl Glb {
    fn read(path: &Path) -> Glb {
        let d = read(path);
        assert_eq!(&d[..4], b"glTF");
        let n = u32::from_le_bytes(d[12..16].try_into().unwrap()) as usize;
        let json = serde_json::from_slice(&d[20..20 + n]).unwrap();
        let rest = &d[20 + n..];
        let bin = if rest.len() >= 8 {
            let m = u32::from_le_bytes(rest[0..4].try_into().unwrap()) as usize;
            rest[8..8 + m].to_vec()
        } else {
            Vec::new()
        };
        Glb { json, bin }
    }

    /// An accessor's numbers as `f32` bit patterns (for floats) or values (for 16-bit indices), and its width.
    fn accessor(&self, index: u64) -> (Vec<u32>, usize) {
        let a = &self.json["accessors"][index as usize];
        let view = &self.json["bufferViews"][a["bufferView"].as_u64().unwrap() as usize];
        let width = match a["type"].as_str().unwrap() {
            "SCALAR" => 1,
            "VEC2" => 2,
            "VEC3" => 3,
            "VEC4" => 4,
            other => panic!("{other}"),
        };
        let size = match a["componentType"].as_u64().unwrap() {
            5126 => 4,
            5123 => 2,
            other => panic!("component type {other}"),
        };
        let start = (view["byteOffset"].as_u64().unwrap_or(0) + a["byteOffset"].as_u64().unwrap_or(0)) as usize;
        let stride = view["byteStride"].as_u64().map_or(size * width, |s| s as usize);
        let mut out = Vec::new();
        for i in 0..a["count"].as_u64().unwrap() as usize {
            for k in 0..width {
                let at = start + i * stride + k * size;
                out.push(if size == 4 {
                    u32::from_le_bytes(self.bin[at..at + 4].try_into().unwrap())
                } else {
                    u16::from_le_bytes(self.bin[at..at + 2].try_into().unwrap()) as u32
                });
            }
        }
        (out, width)
    }
}

fn floats(v: &Value, default: &[f64]) -> Vec<f64> {
    match v.as_array() {
        Some(a) => a.iter().map(|x| x.as_f64().unwrap()).collect(),
        None => default.to_vec(),
    }
}

/// Equal to within one unit in the last place of a double.
fn close(a: &[f64], b: &[f64]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() <= x.abs() * f64::EPSILON)
}

fn bits<const N: usize>(values: &[[f32; N]]) -> Vec<u32> {
    values.iter().flatten().map(|v| v.to_bits()).collect()
}

/// Same bit patterns, or both not a number.
fn same_floats(a: &[u32], b: &[u32]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| x == y || (f32::from_bits(*x).is_nan() && f32::from_bits(*y).is_nan()))
}

#[derive(Default)]
struct Tally {
    models: usize,
    nodes: usize,
    mesh_nodes: usize,
    primitives: usize,
    vertices: usize,
    triangles: usize,
    with_normals: usize,
    with_colors: usize,
    with_uvs: usize,
    sprites: usize,
    animated: usize,
    animations: usize,
    skipped: usize,
    glb_without_indices: usize,
    texcoord_1: usize,
    zero_uv_geometry: usize,
}

fn compare(model: &Model, glb: &Glb, what: &str, tally: &mut Tally) {
    let j = &glb.json;
    let nodes = j["nodes"].as_array().unwrap();
    assert_eq!(model.nodes.len(), nodes.len(), "{what}: node count");
    assert_eq!(j["scenes"][0]["nodes"], serde_json::json!([0]), "{what}");
    let empty = Vec::new();
    let glb_meshes = j["meshes"].as_array().unwrap_or(&empty);
    let glb_materials = j["materials"].as_array().unwrap_or(&empty);
    assert_eq!(model.meshes.len(), glb_meshes.len(), "{what}: mesh count");
    assert_eq!(model.materials.len(), glb_materials.len(), "{what}: material count");
    let mut parents = vec![None; nodes.len()];
    for (i, (n, g)) in model.nodes.iter().zip(nodes).enumerate() {
        let at = format!("{what} node {i}");
        assert_eq!(n.name, g["name"].as_str().unwrap_or(""), "{at}: name");
        let children: Vec<u32> = floats(&g["children"], &[]).iter().map(|c| *c as u32).collect();
        assert_eq!(n.children, children, "{at}: children");
        for c in &children {
            parents[*c as usize] = Some(i as u32);
        }
        assert_eq!(n.mesh.map(u64::from), g["mesh"].as_u64(), "{at}: mesh");
        // The numbers in the file are doubles. serde_json reads a number to within one unit in the last
        // place unless its float_roundtrip feature is on, so that much is allowed here.
        assert!(close(&n.translation, &floats(&g["translation"], &[0.0; 3])), "{at}: translation");
        assert!(close(&n.rotation, &floats(&g["rotation"], &[0.0, 0.0, 0.0, 1.0])), "{at}: rotation");
        assert!(close(&n.scale, &floats(&g["scale"], &[1.0; 3])), "{at}: scale");
        assert!(g["matrix"].is_null(), "{at}");
        let ext = &g["extensions"]["EXT_node_tsr_inheritance"];
        assert_eq!(n.inherit_scale, ext["scale"].as_bool().unwrap_or(true), "{at}: scale inheritance");
        assert_eq!(n.inherit_rotation, ext["rotation"].as_bool().unwrap_or(true), "{at}: rotation inheritance");
    }
    for (i, n) in model.nodes.iter().enumerate() {
        assert_eq!(n.parent, parents[i], "{what} node {i}: parent");
    }
    for (i, (m, g)) in model.meshes.iter().zip(glb_meshes).enumerate() {
        let at = format!("{what} mesh {i}");
        assert_eq!(m.name, g["name"].as_str().unwrap_or(""), "{at}: name");
        assert_eq!(m.billboard, g["extras"]["billboard"].as_bool().unwrap_or(false), "{at}: billboard");
        tally.sprites += m.billboard as usize;
        let prims = g["primitives"].as_array().unwrap();
        assert_eq!(m.primitives.len(), prims.len(), "{at}: primitive count");
        for (k, (p, gp)) in m.primitives.iter().zip(prims).enumerate() {
            let at = format!("{at} primitive {k}");
            assert_eq!(gp["mode"].as_u64().unwrap_or(4), 4, "{at}");
            assert_eq!(u64::from(p.material), gp["material"].as_u64().unwrap(), "{at}: material");
            let attrs = gp["attributes"].as_object().unwrap();
            let (pos, w) = glb.accessor(attrs["POSITION"].as_u64().unwrap());
            assert_eq!(w, 3);
            assert_eq!(p.positions.len() * 3, pos.len(), "{at}: vertex count");
            assert!(same_floats(&bits(&p.positions), &pos), "{at}: positions");
            for (name, mine, width) in [
                ("NORMAL", bits(&p.normals), 3),
                ("TEXCOORD_0", bits(&p.uvs), 2),
                ("COLOR_0", bits(&p.colors), 4),
            ] {
                match attrs.get(name).and_then(Value::as_u64) {
                    Some(a) => {
                        let (theirs, w) = glb.accessor(a);
                        assert_eq!(w, width, "{at}: {name}");
                        assert!(same_floats(&mine, &theirs), "{at}: {name}");
                    }
                    None => assert!(mine.is_empty(), "{at}: {name} is not in the file"),
                }
            }
            for name in attrs.keys() {
                assert!(
                    ["POSITION", "NORMAL", "TEXCOORD_0", "COLOR_0", "TEXCOORD_1"].contains(&name.as_str()),
                    "{at}: {name}"
                );
            }
            tally.texcoord_1 += attrs.contains_key("TEXCOORD_1") as usize;
            match gp["indices"].as_u64() {
                Some(a) => assert_eq!(p.indices, glb.accessor(a).0, "{at}: indices"),
                None => {
                    assert_eq!(p.indices, (0..p.positions.len() as u32).collect::<Vec<_>>(), "{at}: indices");
                    tally.glb_without_indices += 1;
                }
            }
            tally.primitives += 1;
            tally.vertices += p.positions.len();
            tally.triangles += p.triangles();
            tally.with_normals += !p.normals.is_empty() as usize;
            tally.with_colors += !p.colors.is_empty() as usize;
            tally.with_uvs += !p.uvs.is_empty() as usize;
            let display_list = g["extras"]["display_list"].is_string();
            if !m.billboard && !display_list && !p.uvs.is_empty() {
                assert!(p.uvs.iter().all(|uv| uv[0] == 0.0 && uv[1] == 0.0), "{at}");
                tally.zero_uv_geometry += 1;
            }
        }
    }
    for (i, (m, g)) in model.materials.iter().zip(glb_materials).enumerate() {
        let at = format!("{what} material {i}");
        let pbr = &g["pbrMetallicRoughness"];
        let (texture, wrap) = match pbr["baseColorTexture"]["index"].as_u64() {
            Some(t) => {
                let t = &j["textures"][t as usize];
                let uri = j["images"][t["source"].as_u64().unwrap() as usize]["uri"].as_str().unwrap();
                assert!(uri.starts_with("textures/0x") && uri.ends_with(".png") && uri.len() == 23, "{at}: {uri}");
                let s = &j["samplers"][t["sampler"].as_u64().unwrap() as usize];
                (Some(unhex(&uri[9..])), (s["wrapS"].as_u64().unwrap_or(10497), s["wrapT"].as_u64().unwrap_or(10497)))
            }
            None => (None, (10497, 10497)),
        };
        assert_eq!(m.texture, texture, "{at}: texture");
        assert_eq!((u64::from(m.wrap_s.gl()), u64::from(m.wrap_t.gl())), wrap, "{at}: wrap");
        let _ = Wrap::Repeat;
        let color: Vec<f64> = m.base_color.iter().map(|c| f64::from(*c)).collect();
        assert_eq!(color, floats(&pbr["baseColorFactor"], &[1.0; 4]), "{at}: base color");
        let mode = match g["alphaMode"].as_str().unwrap_or("OPAQUE") {
            "OPAQUE" => AlphaMode::Opaque,
            "MASK" => AlphaMode::Mask,
            "BLEND" => AlphaMode::Blend,
            other => panic!("{other}"),
        };
        assert_eq!(m.alpha_mode, mode, "{at}: alpha mode");
        if mode == AlphaMode::Mask {
            assert_eq!(f64::from(m.alpha_cutoff), g["alphaCutoff"].as_f64().unwrap_or(0.5), "{at}: cutoff");
        }
        assert_eq!(m.double_sided, g["doubleSided"].as_bool().unwrap_or(false), "{at}: double-sided");
        assert_eq!(m.unlit, g["extensions"]["KHR_materials_unlit"].is_object(), "{at}: unlit");
    }
    let mesh_nodes = nodes.iter().filter(|n| n["mesh"].is_u64()).count();
    assert_eq!(model.mesh_nodes(), mesh_nodes, "{what}");
    assert_eq!(model.draw_nodes(), mesh_nodes.max(1), "{what}");
    let animations = j["animations"].as_array().map_or(0, |a| a.len());
    tally.animated += (animations > 0) as usize;
    tally.animations += animations;
    tally.models += 1;
    tally.nodes += nodes.len();
    tally.mesh_nodes += mesh_nodes;
}

#[test]
fn models_equal_gltf_export() {
    let Some((home, _)) = dirs("models_equal_gltf_export") else { return };
    let root = home.join("cache/models");
    let mut tally = Tally::default();
    let mut dirs_done = 0;
    for dir in listing(&root) {
        let Some((stem, worlds)) = dir.split_once('@') else { continue };
        let tex: Vec<String> = worlds.split('+').map(|w| format!("{w}_TEX_BANK.tex.fla")).collect();
        let known = sizes(&home, &tex);
        let lookup = |id: u32| known.get(&id).copied();
        let data = bank(&home, &format!("{stem}.obj.fla"));
        let parsed = Bank::parse(&data).unwrap();
        let mut files: BTreeSet<String> = listing(&root.join(&dir)).into_iter().filter(|f| f.ends_with(".glb")).collect();
        for o in &parsed.objects {
            let name = format!("0x{:08x}-{}.glb", o.id, o.name().unwrap());
            let what = format!("{dir}/{name}");
            match objbank::object_model(parsed.data, o, &lookup) {
                Ok(model) => {
                    assert!(files.remove(&name), "{what}: libgarib did not export it");
                    assert_eq!(model, objbank::model(&parsed, o.id, &lookup).unwrap(), "{what}");
                    compare(&model, &Glb::read(&root.join(&dir).join(&name)), &what, &mut tally);
                }
                Err(e) => {
                    assert!(!files.contains(&name), "{what}: {e}");
                    tally.skipped += 1;
                }
            }
        }
        assert!(files.is_empty(), "{dir}: no model for {files:?}");
        dirs_done += 1;
    }
    println!(
        "models_equal_gltf_export: {dirs_done} folders, {} models equal their .glb ({} nodes, {} with a mesh, {} \
         primitives, {} vertices, {} triangles; primitives with normals {}, colors {}, texture coordinates {}; \
         sprites {}); {} objects have no .glb and no model here; {} models have animations in the .glb ({} in all), \
         which the model does not carry; {} primitives have no index list in the .glb (sprites); {} have a second \
         texture coordinate set (face flags), not carried; {} primitives of meshes without a display list have all \
         texture coordinates at 0",
        tally.models, tally.nodes, tally.mesh_nodes, tally.primitives, tally.vertices, tally.triangles,
        tally.with_normals, tally.with_colors, tally.with_uvs, tally.sprites, tally.skipped, tally.animated,
        tally.animations, tally.glb_without_indices, tally.texcoord_1, tally.zero_uv_geometry
    );
}

#[test]
fn timings() {
    let Some((home, reference)) = dirs("timings") else { return };
    let obj: Vec<(String, Vec<u8>)> =
        banks_with(&home, ".obj.fla").into_iter().map(|f| (f.clone(), read(&home.join("data/Glover").join(f)))).collect();
    let tex: Vec<Vec<u8>> =
        banks_with(&home, ".tex.fla").into_iter().map(|f| read(&home.join("data/Glover").join(f))).collect();
    let t = Instant::now();
    let obj_data: Vec<(String, Vec<u8>)> = obj.iter().map(|(f, d)| (f.clone(), fla2::data(d).unwrap())).collect();
    let tex_data: Vec<Vec<u8>> = tex.iter().map(|d| fla2::data(d).unwrap()).collect();
    let unpack = t.elapsed();
    let t = Instant::now();
    let banks: Vec<Bank> = obj_data.iter().map(|(_, d)| Bank::parse(d).unwrap()).collect();
    let tex_banks: Vec<TexBank> = tex_data.iter().map(|d| TexBank::parse(d).unwrap()).collect();
    let parse = t.elapsed();
    let mut known = HashMap::new();
    for b in &tex_banks {
        for t in &b.textures {
            known.insert(t.id, (t.width, t.height));
        }
    }
    let lookup = |id: u32| known.get(&id).copied();
    let t = Instant::now();
    let (mut models, mut triangles) = (0, 0);
    for b in &banks {
        for o in &b.objects {
            if let Ok(m) = objbank::object_model(b.data, o, &lookup) {
                models += 1;
                triangles += m.triangles();
                std::hint::black_box(m.bounds());
            }
        }
    }
    let build = t.elapsed();
    let t = Instant::now();
    let pixels: usize = tex_banks.iter().flat_map(|b| &b.textures).map(|t| t.decode().unwrap().len() / 4).sum();
    let decode = t.elapsed();
    let t = Instant::now();
    let singles: usize = obj_data.iter().map(|(_, d)| objbank::split(d).unwrap().len()).sum();
    let split = t.elapsed();
    // One level: the banks of the first scene of the dump's catalog that uses the largest shared bank.
    let scene: Vec<&Bank> = obj_data
        .iter()
        .zip(&banks)
        .filter(|((f, _), _)| ["GENERIC.obj.fla", "ATLANTIS_SHARED.obj.fla", "ATLANTIS_L1.obj.fla"].contains(&f.as_str()))
        .map(|(_, b)| b)
        .collect();
    let t = Instant::now();
    let mut level_models = 0;
    for b in &scene {
        for o in &b.objects {
            level_models += objbank::object_model(b.data, o, &lookup).is_ok() as usize;
        }
    }
    let level = t.elapsed();
    let _ = reference;
    println!(
        "timings: decompress all banks {unpack:?}; parse {} object banks and {} texture banks {parse:?}; build {models} \
         models ({triangles} triangles) with bounds {build:?}; decode every texture ({pixels} pixels) {decode:?}; split \
         every bank ({singles} singles) {split:?}; the {level_models} models of one level (GENERIC, ATLANTIS_SHARED, \
         ATLANTIS_L1) {level:?}",
        banks.len(),
        tex_banks.len()
    );
}
