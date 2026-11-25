//! Compares the kit with the reference dumps in `$GLOVEBOX_REFERENCE/kit/dump`. Skipped when `GLOVEBOX_REFERENCE`
//! isn't set.
//!
//! Geometry must be equal bit for bit, as float64 in the soup and as float32 in the GLB. Textures without words must
//! be equal pixel for pixel. Textures with words differ in the letters only, because the kit draws its own font.
//!
//! The pumpkin's texturing was corrected after the recordings were made. The recorded pumpkin wraps the face
//! texture three times around the body and relies on a clamp that the game doesn't have. The pumpkin now has the
//! face on its four front columns and a third texture, plain skin, on the other eight. For the pumpkin cases the
//! test compares what didn't change: the settings, the label, every position and normal, the stem's and the face
//! columns' texture coordinates, and the face and stem textures. It skips the skin triangles' texture and
//! coordinates, the texture count, the GLB and the preview. Every other item is compared in full.

use glovebox_core::kit::{self, image::Image, Params};
use serde_json::Value;
use std::path::PathBuf;

fn dump() -> Option<PathBuf> {
    match std::env::var("GLOVEBOX_REFERENCE") {
        Ok(p) => Some(PathBuf::from(p).join("kit").join("dump")),
        Err(_) => {
            println!("skipped: GLOVEBOX_REFERENCE isn't set");
            None
        }
    }
}

fn index(dir: &PathBuf) -> Value {
    serde_json::from_slice(&std::fs::read(dir.join("cases.json")).expect("cases.json")).expect("json")
}

fn params(v: &Value) -> Params {
    v.as_object().cloned().unwrap_or_default()
}

struct Reader<'a>(&'a [u8], usize);

impl Reader<'_> {
    fn bytes(&mut self, n: usize) -> &[u8] {
        let s = &self.0[self.1..self.1 + n];
        self.1 += n;
        s
    }
    fn u32(&mut self) -> u32 {
        u32::from_le_bytes(self.bytes(4).try_into().unwrap())
    }
    fn f64(&mut self) -> f64 {
        f64::from_le_bytes(self.bytes(8).try_into().unwrap())
    }
}

fn decode_png(data: &[u8]) -> Image {
    let mut dec = png::Decoder::new(std::io::Cursor::new(data));
    dec.set_transformations(png::Transformations::EXPAND);
    let mut reader = dec.read_info().expect("png header");
    let mut buf = vec![0; reader.output_buffer_size().expect("png size")];
    let info = reader.next_frame(&mut buf).expect("png data");
    let n = info.color_type.samples();
    let px = buf[..info.buffer_size()]
        .chunks(n)
        .map(|c| match n {
            4 => [c[0], c[1], c[2], c[3]],
            3 => [c[0], c[1], c[2], 255],
            _ => [c[0], c[0], c[0], 255],
        })
        .collect();
    Image { w: info.width as usize, h: info.height as usize, px }
}

// The share of pixels that differ, in percent.
fn differ(a: &Image, b: &Image) -> f64 {
    assert_eq!((a.w, a.h), (b.w, b.h), "picture size");
    let n = a.px.iter().zip(&b.px).filter(|(x, y)| x != y).count();
    100.0 * n as f64 / a.px.len().max(1) as f64
}

fn split_glb(g: &[u8]) -> (Value, Vec<u8>) {
    assert_eq!(&g[..4], b"glTF");
    assert_eq!(u32::from_le_bytes(g[8..12].try_into().unwrap()) as usize, g.len());
    let n = u32::from_le_bytes(g[12..16].try_into().unwrap()) as usize;
    assert_eq!(&g[16..20], b"JSON");
    let json = serde_json::from_slice(&g[20..20 + n]).expect("glb json");
    let m = u32::from_le_bytes(g[20 + n..24 + n].try_into().unwrap()) as usize;
    assert_eq!(&g[24 + n..28 + n], b"BIN\0");
    assert_eq!(28 + n + m, g.len());
    (json, g[28 + n..].to_vec())
}

fn view<'a>(json: &Value, bin: &'a [u8], i: u64) -> &'a [u8] {
    let v = &json["bufferViews"][i as usize];
    let (o, l) = (v["byteOffset"].as_u64().unwrap() as usize, v["byteLength"].as_u64().unwrap() as usize);
    &bin[o..o + l]
}

// The JSON without what the PNG encoder changes: offsets, the pictures' lengths and the generator's name.
fn structure(json: &Value) -> Value {
    let mut j = json.clone();
    j["asset"].as_object_mut().unwrap().remove("generator");
    j["buffers"][0].as_object_mut().unwrap().remove("byteLength");
    for v in j["bufferViews"].as_array_mut().unwrap() {
        let o = v.as_object_mut().unwrap();
        o.remove("byteOffset");
        if !o.contains_key("target") {
            o.remove("byteLength");
        }
    }
    j
}

#[test]
fn list_numbers_and_refusals() {
    let Some(dir) = dump() else { return };
    let ix = index(&dir);
    // The wording was changed to American English after the recordings were made: one recorded note has the old word.
    let mut items = ix["items"].clone();
    for item in items.as_array_mut().unwrap() {
        if item["note"] == "A burnt patch for the ground" {
            item["note"] = "A burned patch for the ground".into();
        }
    }
    assert_eq!(kit::items(), items);
    for r in ix["rng"].as_array().unwrap() {
        let mut g = kit::rng::Random::new(r["seed"].as_u64().unwrap());
        for (i, want) in r["values"].as_array().unwrap().iter().enumerate() {
            let got = match i % 6 {
                0 => g.random(),
                1 => g.uniform(-3.5, 7.25),
                2 => g.randint(-22, 12) as f64,
                3 => g.randrange(0, 1 + i as i64 % 97) as f64,
                4 => g.randrange(3, 61) as f64,
                _ => g.choice(&[5, 6, 7]) as f64,
            };
            assert_eq!(got.to_bits(), want.as_str().unwrap().parse::<u64>().unwrap(), "seed {} value {i}", r["seed"]);
        }
    }
    for h in ix["hypot"].as_array().unwrap() {
        let bits = |v: &Value| f64::from_bits(v.as_str().unwrap().parse::<u64>().unwrap());
        let (x, y, want) = (bits(&h[0]), bits(&h[1]), bits(&h[2]));
        assert_eq!(kit::rng::hypot(x, y).to_bits(), want.to_bits(), "hypot({x}, {y})");
    }
    let refusals = ix["refusals"].as_array().unwrap();
    for r in refusals {
        let item = r["item"].as_str().unwrap();
        let got = kit::clean(item, &params(&r["params"]));
        assert_eq!(got.err().as_deref(), r["error"].as_str(), "clean({item}, {})", r["params"]);
        assert_eq!(kit::build(item, &params(&r["params"])).err().as_deref(), r["error"].as_str(), "build({item}, {})", r["params"]);
    }
    println!("items, {} random sequences, {} hypot values and {} refusals equal", ix["rng"].as_array().unwrap().len(), ix["hypot"].as_array().unwrap().len(), refusals.len());
}

#[test]
fn every_case_matches_the_reference() {
    let Some(dir) = dump() else { return };
    let ix = index(&dir);
    let cases = ix["cases"].as_array().unwrap();
    let (mut worst_text, mut worst_preview, mut text_textures, mut exact_textures, mut tris) = (0.0f64, 0.0f64, 0, 0, 0);
    let mut corrected_cases = 0;
    for c in cases {
        let (name, item) = (c["name"].as_str().unwrap(), c["item"].as_str().unwrap());
        let p = params(&c["params"]);
        let cleaned = kit::clean(item, &p).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(Value::Object(cleaned.clone()), c["clean"], "{name}: clean");
        assert_eq!(kit::label(item, &cleaned).unwrap(), c["label"].as_str().unwrap(), "{name}: label");
        let soup = kit::build(item, &p).unwrap();
        // See the note at the top of the file.
        let corrected = item == "pumpkin";
        const PUMPKIN_SKIN: usize = 2;

        // The soup: every float64 equal.
        let raw = std::fs::read(dir.join(format!("{name}.soup"))).unwrap();
        let mut r = Reader(&raw, 0);
        let n = r.u32() as usize;
        assert_eq!(soup.tris.len(), n, "{name}: triangle count");
        for (i, t) in soup.tris.iter().enumerate() {
            let got: Vec<f64> = t.p.iter().flatten().chain(t.n.iter().flatten()).chain(t.uv.iter().flatten()).copied().collect();
            // A skin triangle of the pumpkin has new texture coordinates (values 18 to 23) and a new texture.
            let reskinned = corrected && t.mat == PUMPKIN_SKIN;
            for (k, g) in got.iter().enumerate() {
                let want = r.f64();
                assert!(g.to_bits() == want.to_bits() || (reskinned && k >= 18), "{name}: triangle {i} value {k}: {g:?} != {want:?}");
            }
            let want = r.u32();
            if reskinned {
                assert_eq!(want, 0, "{name}: triangle {i} wasn't on the face texture");
            } else {
                assert_eq!(t.mat as u32, want, "{name}: triangle {i} material");
            }
        }
        tris += n;
        let nm = r.u32() as usize;
        assert_eq!(soup.mats.len(), if corrected { nm + 1 } else { nm }, "{name}: texture count");
        let has_words = |m: usize| (item == "grave" && m == 0) || (item == "sign" && m == 1);
        for m in 0..nm {
            let (w, h) = (r.u32() as usize, r.u32() as usize);
            let clamp = r.bytes(1)[0] != 0;
            let px = r.bytes(w * h * 4).chunks(4).map(|c| [c[0], c[1], c[2], c[3]]).collect();
            let want = Image { w, h, px };
            assert_eq!(soup.clamp[m], clamp, "{name}: texture {m} clamp");
            let d = differ(&soup.mats[m], &want);
            if has_words(m) {
                text_textures += 1;
                worst_text = worst_text.max(d);
                assert!(d <= 6.0, "{name}: texture {m} with words differs in {d:.2}% of its pixels");
                // Outside the rows of the words, the picture is equal.
                for (i, (a, b)) in soup.mats[m].px.iter().zip(&want.px).enumerate() {
                    let y = i / w;
                    let in_words = if item == "grave" { (9..62).contains(&y) } else { (4..28).contains(&y) };
                    assert!(a == b || in_words, "{name}: texture {m} differs outside the words at row {y}");
                }
            } else {
                exact_textures += 1;
                assert!(d == 0.0, "{name}: texture {m} differs in {d:.2}% of its pixels");
            }
        }

        if corrected {
            // The GLB has a third primitive and the preview has the skin's colors: neither is compared with the
            // recording. The GLB's pictures must still be the soup's textures.
            let (rj, rbin) = split_glb(&soup.glb());
            let images = rj["images"].as_array().unwrap();
            assert_eq!(images.len(), soup.mats.len(), "{name}: glb pictures");
            for (k, im) in images.iter().enumerate() {
                assert!(decode_png(view(&rj, &rbin, im["bufferView"].as_u64().unwrap())) == soup.mats[k], "{name}: glb picture {k}");
            }
            assert_eq!(decode_png(&kit::preview_png(item, &cleaned, 128).unwrap()).w, 128);
            corrected_cases += 1;
            continue;
        }

        // The GLB: the same structure, the same accessor bytes, the same pictures as this soup's.
        let (pj, pbin) = split_glb(&std::fs::read(dir.join(format!("{name}.glb"))).unwrap());
        let (rj, rbin) = split_glb(&soup.glb());
        assert_eq!(structure(&rj), structure(&pj), "{name}: glb json");
        for (i, a) in pj["accessors"].as_array().unwrap().iter().enumerate() {
            let bv = a["bufferView"].as_u64().unwrap();
            assert!(view(&rj, &rbin, bv) == view(&pj, &pbin, bv), "{name}: accessor {i} bytes");
        }
        let mut used: Vec<usize> = soup.tris.iter().map(|t| t.mat).collect();
        used.sort_unstable();
        used.dedup();
        for (k, im) in pj["images"].as_array().unwrap().iter().enumerate() {
            let bv = im["bufferView"].as_u64().unwrap();
            let mine = decode_png(view(&rj, &rbin, bv));
            assert!(mine == soup.mats[used[k]], "{name}: glb picture {k} isn't the soup's texture");
            let d = differ(&mine, &decode_png(view(&pj, &pbin, bv)));
            assert!(if has_words(used[k]) { d <= 6.0 } else { d == 0.0 }, "{name}: glb picture {k} differs in {d:.2}%");
        }

        // The preview.
        let want = decode_png(&std::fs::read(dir.join(format!("{name}.preview.png"))).unwrap());
        let got = decode_png(&kit::preview_png(item, &cleaned, 128).unwrap());
        let d = differ(&got, &want);
        worst_preview = worst_preview.max(d);
        let limit = if item == "grave" || item == "sign" { 1.0 } else { 0.0 };
        assert!(d <= limit, "{name}: preview differs in {d:.3}% of its pixels");
    }
    println!(
        "{} cases, {tris} triangles equal; {exact_textures} textures equal; {text_textures} textures with words, at most {worst_text:.2}% of pixels differ; previews differ by at most {worst_preview:.3}%; {corrected_cases} pumpkin cases compared without their skin triangles' texture, GLB and preview",
        cases.len()
    );
}
