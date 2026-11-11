//! Import model: a player's glTF file becomes Glover objects, a texture bank and the facts in `meta.json`.
//!
//! It does what the former Python converter (`custom_model.py`) and the parts of libgarib it ran did, with the
//! storage of the former Python back end
//! (`import_custom`, `custom_sets`, `custom_catalog`, `reskin`). For the same file, name and scale it gives the
//! same pieces, IDs and bank bytes as the Python app. See docs/technical.md, "Imported models".
//!
//! A set is stored in `<home>/custom/<set>/`: `meta.json`, one single-object bank `<id>.obj.bin` for each piece, and
//! `textures.tex.bin`.

pub mod gltf;
pub mod image;
mod jpeg;
pub mod pack;

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};
use sha1::{Digest, Sha1};

use crate::pyfmt;
use crate::pyrandom::PyRandom;
use crate::texbank;
use image::Rgba;

/// The most triangles one piece holds. A larger model is cut into pieces.
pub const MAX_TRIANGLES: usize = 1500;
/// The most texels one `G_LOADBLOCK` loads.
pub const MAX_TEXELS: usize = 2048;
pub const MAX_SIDE: usize = 256;
pub const MAX_INPUT_TRIANGLES: usize = 200_000;
/// The largest model file the importer takes, in bytes.
pub const MAX_MODEL: usize = 64 << 20;
/// Texture coordinates are signed 10.5 fixed point in texels. The converter keeps them within this many texels.
const MAX_TEXELS_COORD: f64 = 1000.0;

/// Why a model could not be imported, in words for the player.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelError(pub String);

impl std::fmt::Display for ModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ModelError {}

pub(crate) fn err<T>(text: impl Into<String>) -> Result<T, ModelError> {
    Err(ModelError(text.into()))
}

/// One piece of a converted model: an object of its own.
#[derive(Debug, Clone, PartialEq)]
pub struct Piece {
    pub id: u32,
    /// The mesh's name, at most 8 characters
    pub name: String,
    /// Where the piece's middle goes, in the model's coordinates
    pub offset: [f64; 3],
    pub triangles: usize,
    pub size: [f64; 3],
    /// The IDs of the textures the piece uses, in rising order
    pub textures: Vec<u32>,
    /// The single-object bank
    pub bank: Vec<u8>,
}

/// A converted model.
#[derive(Debug, Clone, PartialEq)]
pub struct CustomSet {
    pub name: String,
    pub scale: f64,
    pub triangles: usize,
    pub size: [f64; 3],
    pub pieces: Vec<Piece>,
    /// Each texture's ID and the length of its record, in the order the materials first use them
    pub textures: Vec<(u32, usize)>,
    /// The texture bank: the records in rising order of ID
    pub texture_bank: Vec<u8>,
}

fn hex_id(id: u32) -> String {
    format!("0x{id:08X}")
}

fn sha1_hex(parts: &[&[u8]]) -> String {
    let mut h = Sha1::new();
    for p in parts {
        h.update(p);
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// The folder name of the set a file and a scale give: the start of a hash of both.
pub fn set_id(data: &[u8], scale: f64) -> String {
    sha1_hex(&[data, pyfmt::repr(scale).as_bytes()])[..10].to_string()
}

// ---------------------------------------------------------------------------------------------
// Textures

fn pow2_at_most(n: usize, cap: usize) -> usize {
    let mut p = 1;
    while p * 2 <= n.min(cap) {
        p *= 2;
    }
    p
}

/// A picture no larger than `MAX_TEXELS` texels with power-of-two sides, tinted by the material's colour.
fn n64_texture(im: &Rgba, factor: &[f64]) -> Result<Rgba, ModelError> {
    let (mut w, mut h) = (pow2_at_most(im.w, MAX_SIDE), pow2_at_most(im.h, MAX_SIDE));
    while w * h > MAX_TEXELS {
        if w >= h {
            w /= 2;
        } else {
            h /= 2;
        }
    }
    let mut out = image::resize(im, w, h);
    if factor.iter().any(|f| (f - 1.0).abs() > 1e-3) {
        if factor.len() != 4 && factor.len() != 1 {
            return err("A material of the model has a colour that is not four numbers");
        }
        for p in out.px.chunks_exact_mut(4) {
            for (c, v) in p.iter_mut().enumerate() {
                *v = (*v as f64 * factor[c % factor.len()] + 0.5).clamp(0.0, 255.0) as u8;
            }
        }
    }
    Ok(out)
}

/// libgarib's `encodeRGBA16`.
fn rgba16(p: &[u8]) -> u16 {
    let c = |v: u8| ((v as f64 / 255.0) * 31.0) as u16 & 0x1f;
    c(p[0]) << 11 | c(p[1]) << 6 | c(p[2]) << 1 | (p[3] > 0) as u16
}

/// A texture record as `texbank-tool pack` writes it: the header, then 16-bit RGBA pixels.
fn texture_record(id: u32, im: &Rgba) -> Vec<u8> {
    let data: Vec<u8> = im.px.chunks_exact(4).flat_map(|p| rgba16(p).to_be_bytes()).collect();
    let header = texbank::Texture {
        id,
        palette_anim_idx_min: 0,
        palette_anim_idx_max: 0,
        flags: 0,
        frame_increment: 0,
        frame_counter: 0,
        width: im.w as u16,
        height: im.h as u16,
        masks: im.w.ilog2() as u16,
        maskt: im.h.ilog2() as u16,
        length: (texbank::HEADER + data.len()) as u32,
        color_format: texbank::color::RGBA,
        compression_format: texbank::compression::UNCOMPRESSED_16B,
        data_ptr: texbank::HEADER as u32,
        palette_offset: 0,
        data: &data,
    }
    .header();
    [&header[..], &data].concat()
}

/// What the converter knows of a glTF material.
struct Material {
    texture: u32,
    /// The mesh flags for the material's alpha mode
    render_mode: u16,
    wrap: (i64, i64),
    size: (usize, usize),
}

/// Moves a piece's texture coordinates near zero. A repeating texture can move coordinates by whole repeats (two
/// for a mirrored one). The coordinates of one material move as one, or each triangle's on its own if that is not
/// enough. A triangle that still spans too much is clamped.
fn near_zero(uv: &mut [[[f64; 2]; 3]], m: &[i64], materials: &BTreeMap<i64, Material>) {
    let used: HashSet<i64> = m.iter().copied().collect();
    for mat in used {
        let info = &materials[&mat];
        for axis in 0..2 {
            let size = if axis == 0 { info.size.0 } else { info.size.1 };
            let limit = MAX_TEXELS_COORD / size as f64;
            let step = match if axis == 0 { info.wrap.0 } else { info.wrap.1 } {
                gltf::REPEAT => Some(1.0),
                gltf::MIRRORED_REPEAT => Some(2.0),
                _ => None,
            };
            let count = uv.len();
            let rows = || (0..count).filter(|i| m[*i] == mat);
            if let Some(step) = step {
                let min = rows().flat_map(|i| uv[i].map(|v| v[axis])).fold(f64::INFINITY, f64::min);
                let shift = (min / step).floor() * step;
                let mut max = 0.0f64;
                for i in rows() {
                    for v in &mut uv[i] {
                        v[axis] -= shift;
                        max = max.max(v[axis].abs());
                    }
                }
                if max > limit {
                    for i in rows() {
                        let min = uv[i].iter().map(|v| v[axis]).fold(f64::INFINITY, f64::min);
                        let shift = (min / step).floor() * step;
                        for v in &mut uv[i] {
                            v[axis] -= shift;
                        }
                    }
                }
            }
            for i in rows() {
                for v in &mut uv[i] {
                    v[axis] = v[axis].clamp(-limit, limit);
                }
            }
        }
    }
}

/// Cuts a list of triangles into groups of at most `limit`, across the longest side of their centres.
fn split(centroids: &[[f64; 3]], order: Vec<usize>, limit: usize, out: &mut Vec<Vec<usize>>) {
    if order.len() <= limit {
        out.push(order);
        return;
    }
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for i in &order {
        for a in 0..3 {
            lo[a] = lo[a].min(centroids[*i][a]);
            hi[a] = hi[a].max(centroids[*i][a]);
        }
    }
    let mut axis = 0;
    for a in 1..3 {
        if hi[a] - lo[a] > hi[axis] - lo[axis] {
            axis = a;
        }
    }
    let mut order = order;
    order.sort_by(|a, b| centroids[*a][axis].partial_cmp(&centroids[*b][axis]).unwrap_or(std::cmp::Ordering::Equal));
    let second = order.split_off(order.len() / 2);
    split(centroids, order, limit, out);
    split(centroids, second, limit, out);
}

/// numpy's `round(v, 1)`.
fn round1(v: f64) -> f64 {
    (v * 10.0).round_ties_even() / 10.0
}

fn bounds<'a>(p: impl Iterator<Item = &'a [[f64; 3]; 3]>) -> ([f64; 3], [f64; 3]) {
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for v in p.flatten() {
        for a in 0..3 {
            lo[a] = lo[a].min(v[a]);
            hi[a] = hi[a].max(v[a]);
        }
    }
    (lo, hi)
}

/// Mesh names hold 8 characters.
fn short_name(name: &str) -> String {
    let s: String = name.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').take(8).collect();
    if s.is_empty() {
        "custom".into()
    } else {
        s
    }
}

/// Converts the glTF bytes `data` into Glover objects. `taken_ids` and `taken_textures` hold the IDs the game and
/// the other imported models use already.
pub fn convert(
    data: &[u8],
    name: &str,
    scale: f64,
    taken_ids: &HashSet<u32>,
    taken_textures: &HashSet<u32>,
) -> Result<CustomSet, ModelError> {
    if !(0.0 < scale && scale < 1e6) {
        return err("The scale must be a positive number");
    }
    let g = gltf::Gltf::parse(data)?;
    let (mut tris, count) = g.triangles()?;
    if count > MAX_INPUT_TRIANGLES {
        return err(format!("The model has {count} triangles. The most this importer takes is {MAX_INPUT_TRIANGLES}."));
    }
    // The scale, then drop triangles without an area
    let mut keep = Vec::with_capacity(count);
    for (i, p) in tris.p.iter_mut().enumerate() {
        for v in p.iter_mut() {
            *v = v.map(|c| c * scale);
        }
        if gltf::cross_length(p) > 1e-6 {
            keep.push(i);
        }
    }
    let p: Vec<[[f64; 3]; 3]> = keep.iter().map(|i| tris.p[*i]).collect();
    let n: Vec<[[f64; 3]; 3]> = keep.iter().map(|i| tris.n[*i]).collect();
    let uv: Vec<[[f64; 2]; 3]> = keep.iter().map(|i| tris.uv[*i]).collect();
    let m: Vec<i64> = keep.iter().map(|i| tris.m[*i]).collect();
    drop(tris);
    if p.is_empty() {
        return err("Every triangle of the model is too small at this scale");
    }
    if n.iter().flatten().flatten().any(|c| !c.is_finite()) || uv.iter().flatten().flatten().any(|c| !c.is_finite()) {
        return err("The model has normals or texture coordinates that are not numbers");
    }
    let digest = &sha1_hex(&[data, pyfmt::repr(scale).as_bytes()])[..8];

    // Materials: texture ID, alpha mode, wrap modes
    let mut materials: BTreeMap<i64, Material> = BTreeMap::new();
    // Each picture by the hash of its pixels, in the order of first use
    let mut textures: Vec<([u8; 20], u32, Rgba)> = Vec::new();
    for mat in m.iter().copied().collect::<std::collections::BTreeSet<i64>>() {
        let (factor, tex_index, alpha_mode) = g.material(mat);
        let src = match g.image(tex_index)? {
            Some(bytes) => match image::decode(&bytes) {
                Ok(im) => im,
                Err(e) => return err(format!("Could not read an image in the model: {e}")),
            },
            None => Rgba::filled(8, 8, [255; 4]),
        };
        let im = n64_texture(&src, factor.as_deref().unwrap_or(&[1.0; 4]))?;
        let render_mode = match alpha_mode {
            Some("BLEND") => crate::objbank::render_mode::XLU,
            Some("MASK") => crate::objbank::render_mode::MASKED,
            _ if im.px.chunks_exact(4).any(|p| p[3] < 128) => crate::objbank::render_mode::MASKED,
            _ => 0,
        };
        let wrap = g.wrap(tex_index)?;
        let key: [u8; 20] = Sha1::new().chain_update(&im.px).chain_update(format!("({}, {})", im.w, im.h)).finalize().into();
        let texture = match textures.iter().find(|t| t.0 == key) {
            Some(t) => t.1,
            None => {
                let hash: [u8; 20] = Sha1::new().chain_update(key).chain_update(digest).finalize().into();
                let mut tid = u32::from_be_bytes([hash[0], hash[1], hash[2], hash[3]]);
                while taken_textures.contains(&tid) || textures.iter().any(|t| t.1 == tid) || tid == 0 {
                    tid = tid.wrapping_add(1);
                }
                textures.push((key, tid, im.clone()));
                tid
            }
        };
        materials.insert(mat, Material { texture, render_mode, wrap, size: (im.w, im.h) });
    }

    // Pieces
    let centroids: Vec<[f64; 3]> =
        p.iter().map(|t| [0, 1, 2].map(|a| (t[0][a] + t[1][a] + t[2][a]) / 3.0)).collect();
    let mut groups = Vec::new();
    split(&centroids, (0..p.len()).collect(), MAX_TRIANGLES, &mut groups);
    let base = short_name(name);
    let mut used: HashSet<u32> = taken_ids.clone();
    let mut pieces = Vec::with_capacity(groups.len());
    let (lo, hi) = bounds(p.iter());
    for (i, sel) in groups.iter().enumerate() {
        let (plo, phi) = bounds(sel.iter().map(|t| &p[*t]));
        let centre = [0, 1, 2].map(|a| round1((plo[a] + phi[a]) / 2.0));
        if sel.iter().flat_map(|t| p[*t]).any(|v| (0..3).any(|a| (v[a] - centre[a]).abs() > 32000.0)) {
            return err("Part of the model is more than 64,000 units across at this scale, more than a Glover model \
                        holds. Use a smaller scale.");
        }
        let pname: String = if groups.len() == 1 {
            base.clone()
        } else {
            base.chars().take(5).chain(i.to_string().chars()).take(8).collect()
        };
        // The mesh keeps the first 8 characters; the padding ends its name there
        let (mut node_name, mut oid) = (String::new(), 0);
        for k in 0..1000 {
            let suffix = if k == 0 { String::new() } else { format!("-{k}") };
            node_name = format!("{pname:\0<8}{digest}{i}{suffix}");
            oid = pack::hash_str(&node_name);
            // A bank whose first byte is "{" would be read as glTF JSON by libgarib's tool
            if !used.contains(&oid) && oid != 0 && (oid >> 24) != 0x7B {
                break;
            }
        }
        used.insert(oid);
        let mut piece_uv: Vec<[[f64; 2]; 3]> = sel.iter().map(|t| uv[*t]).collect();
        let piece_m: Vec<i64> = sel.iter().map(|t| m[*t]).collect();
        near_zero(&mut piece_uv, &piece_m, &materials);
        // One primitive for each material, in the order of the material numbers
        let mut prims: BTreeMap<i64, pack::Prim> = BTreeMap::new();
        for (k, t) in sel.iter().enumerate() {
            let info = &materials[&piece_m[k]];
            let prim = prims.entry(piece_m[k]).or_insert_with(|| pack::Prim {
                texture: info.texture,
                size: (info.size.0 as u32, info.size.1 as u32),
                pos: Vec::new(),
                nrm: Vec::new(),
                uv: Vec::new(),
            });
            for c in 0..3 {
                prim.pos.push([0, 1, 2].map(|a| (p[*t][c][a] - centre[a]) as f32));
                prim.nrm.push(n[*t][c].map(|v| v as f32));
                prim.uv.push(piece_uv[k][c].map(|v| v as f32));
            }
        }
        // libgarib takes the mesh's flags from its first primitive
        let render_mode = prims.keys().next().map(|k| materials[k].render_mode).unwrap_or(0);
        let prims: Vec<pack::Prim> = prims.into_values().collect();
        let bank = pack::pack_object(&node_name, oid, render_mode, &prims)?;
        let mut piece_textures: Vec<u32> = prims.iter().map(|p| p.texture).collect();
        piece_textures.sort_unstable();
        piece_textures.dedup();
        pieces.push(Piece {
            id: oid,
            name: pname,
            offset: centre,
            triangles: sel.len(),
            size: [0, 1, 2].map(|a| round1(phi[a] - plo[a])),
            textures: piece_textures,
            bank,
        });
    }
    // libgarib's unpacker divides by a texture's size less one. The Python app stops at the first piece it
    // could not read back.
    for piece in &pieces {
        let thin = |t: &u32| textures.iter().any(|x| x.1 == *t && (x.2.w == 1 || x.2.h == 1));
        if piece.textures.iter().any(thin) {
            return err(format!("libgarib could not read back piece {}", hex_id(piece.id)));
        }
    }

    let records: BTreeMap<u32, Vec<u8>> = textures.iter().map(|(_, id, im)| (*id, texture_record(*id, im))).collect();
    let names = vec![""; records.len()];
    let texture_bank = texbank::write_bank(&records.values().collect::<Vec<_>>(), &names);
    Ok(CustomSet {
        name: name.to_string(),
        scale,
        triangles: p.len(),
        size: [0, 1, 2].map(|a| round1(hi[a] - lo[a])),
        pieces,
        textures: textures.iter().map(|(_, id, _)| (*id, records[id].len())).collect(),
        texture_bank,
    })
}

// ---------------------------------------------------------------------------------------------
// meta.json

/// JSON text as Python's `json.dump(value, indent=1)` writes it.
pub fn py_json(value: &Value) -> String {
    py_dumps(value, true)
}

/// JSON text as Python's `json.dumps` writes it: on one line, or with an indent of 1.
fn py_dumps(value: &Value, indent: bool) -> String {
    fn string(s: &str, out: &mut String) {
        out.push('"');
        for c in s.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                '\u{8}' => out.push_str("\\b"),
                '\u{c}' => out.push_str("\\f"),
                c if (c as u32) < 0x20 || (c as u32) > 0x7E && !c.is_ascii() => {
                    let mut units = [0u16; 2];
                    for u in c.encode_utf16(&mut units) {
                        out.push_str(&format!("\\u{u:04x}"));
                    }
                }
                c => out.push(c),
            }
        }
        out.push('"');
    }
    fn write(v: &Value, depth: usize, indent: bool, out: &mut String) {
        let pad = |out: &mut String, depth: usize| {
            if indent {
                out.push('\n');
                out.extend(std::iter::repeat(' ').take(depth));
            }
        };
        let comma = if indent { "," } else { ", " };
        match v {
            Value::Null => out.push_str("null"),
            Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Value::Number(n) => match (n.as_i64(), n.as_u64(), n.as_f64()) {
                (Some(i), _, _) => out.push_str(&i.to_string()),
                (_, Some(u), _) => out.push_str(&u.to_string()),
                (_, _, Some(f)) => out.push_str(&pyfmt::repr(f)),
                _ => out.push_str("null"),
            },
            Value::String(s) => string(s, out),
            Value::Array(a) if a.is_empty() => out.push_str("[]"),
            Value::Array(a) => {
                out.push('[');
                for (i, item) in a.iter().enumerate() {
                    if i > 0 {
                        out.push_str(comma);
                    }
                    pad(out, depth + 1);
                    write(item, depth + 1, indent, out);
                }
                pad(out, depth);
                out.push(']');
            }
            Value::Object(o) if o.is_empty() => out.push_str("{}"),
            Value::Object(o) => {
                out.push('{');
                for (i, (k, item)) in o.iter().enumerate() {
                    if i > 0 {
                        out.push_str(comma);
                    }
                    pad(out, depth + 1);
                    string(k, out);
                    out.push_str(": ");
                    write(item, depth + 1, indent, out);
                }
                pad(out, depth);
                out.push('}');
            }
        }
    }
    let mut out = String::new();
    write(value, 0, indent, &mut out);
    out
}

impl CustomSet {
    /// The set's `meta.json`, with the fields the Python app writes. `preview` is empty: there are no preview
    /// files. A texture's size is the length of its record. The Python app wrote the size of a PNG file there,
    /// and nothing reads it.
    pub fn meta(&self) -> Value {
        let pieces: Vec<Value> = self
            .pieces
            .iter()
            .map(|p| {
                json!({
                    "id": hex_id(p.id), "name": p.name, "offset": p.offset, "triangles": p.triangles, "size": p.size,
                    "textures": p.textures.iter().map(|t| hex_id(*t)).collect::<Vec<_>>(),
                    "bytes": p.bank.len(), "preview": "",
                })
            })
            .collect();
        let textures: Map<String, Value> = self.textures.iter().map(|(id, len)| (hex_id(*id), json!(len))).collect();
        json!({
            "name": self.name, "scale": self.scale, "triangles": self.triangles, "size": self.size,
            "pieces": pieces, "textures": textures,
        })
    }
}

// ---------------------------------------------------------------------------------------------
// Storage

fn custom_dir(home: &Path) -> PathBuf {
    home.join("custom")
}

/// True for the name of a set's folder: ten hexadecimal digits in lower case.
pub fn is_set_id(text: &str) -> bool {
    text.len() == 10 && text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// `{set: meta.json}` for every imported model, in the order of the set names. A set whose `meta.json` cannot be
/// read is left out.
pub fn custom_sets(home: &Path) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    let Ok(dir) = std::fs::read_dir(custom_dir(home)) else { return out };
    for entry in dir.flatten() {
        let Ok(name) = entry.file_name().into_string() else { continue };
        if !is_set_id(&name) {
            continue;
        }
        let Ok(text) = std::fs::read(entry.path().join("meta.json")) else { continue };
        if let Ok(meta @ Value::Object(_)) = serde_json::from_slice::<Value>(&text) {
            out.insert(name, meta);
        }
    }
    out
}

/// One piece of an imported model, as the model list shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct CatalogEntry {
    /// The object ID, as `0x%08X`
    pub id: String,
    /// The model's name, with the piece's number when the model has several
    pub name: String,
    /// `Custom: <model name>`
    pub bank: String,
    /// The size of the piece's bank in bytes
    pub bytes: u64,
    /// The set
    pub custom: String,
    /// The mesh's name
    pub piece: String,
    pub offset: [f64; 3],
    pub textures: Vec<String>,
    /// The piece's single-object bank
    pub path: PathBuf,
    /// `custom:<set>`
    pub tex_bank: String,
    /// The retail model a reskinned model was made from
    pub reskin_from: Option<String>,
}

/// An entry for every piece of every imported model: the catalog's entries, without the preview's
/// URL. An ID that two sets hold keeps its first place and takes the later set's entry, as a Python dict does.
pub fn custom_catalog(home: &Path) -> Vec<CatalogEntry> {
    let mut out: Vec<CatalogEntry> = Vec::new();
    for (set, meta) in custom_sets(home) {
        let model = meta["name"].as_str().unwrap_or("");
        let Some(pieces) = meta["pieces"].as_array() else { continue };
        for (i, p) in pieces.iter().enumerate() {
            let Some(id) = p["id"].as_str() else { continue };
            let entry = CatalogEntry {
                id: id.to_string(),
                name: if pieces.len() == 1 { model.to_string() } else { format!("{model} {}/{}", i + 1, pieces.len()) },
                bank: format!("Custom: {model}"),
                bytes: p["bytes"].as_u64().unwrap_or(0),
                custom: set.clone(),
                piece: p["name"].as_str().unwrap_or("").to_string(),
                offset: [0, 1, 2].map(|a| p["offset"][a].as_f64().unwrap_or(0.0)),
                textures: p["textures"]
                    .as_array()
                    .map(|t| t.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_default(),
                path: custom_dir(home).join(&set).join(format!("{id}.obj.bin")),
                tex_bank: format!("custom:{set}"),
                reskin_from: meta["reskin"]["from"].as_str().map(String::from),
            };
            match out.iter_mut().find(|e| e.id == entry.id) {
                Some(old) => *old = entry,
                None => out.push(entry),
            }
        }
    }
    out
}

fn parse_id(text: &str) -> Option<u32> {
    u32::from_str_radix(text.strip_prefix("0x").or_else(|| text.strip_prefix("0X"))?, 16).ok()
}

/// The object IDs and texture IDs the imported models use.
fn custom_ids(home: &Path) -> (HashSet<u32>, HashSet<u32>) {
    let catalog = custom_catalog(home);
    (
        catalog.iter().filter_map(|c| parse_id(&c.id)).collect(),
        catalog.iter().flat_map(|c| c.textures.iter().filter_map(|t| parse_id(t))).collect(),
    )
}

fn io_err(what: &str, e: std::io::Error) -> ModelError {
    ModelError(format!("Could not {what}: {e}"))
}

/// Writes a set's files into a temporary folder, then renames it to `<home>/custom/<set>`.
fn store(home: &Path, set: &str, files: &[(String, &[u8])]) -> Result<(), ModelError> {
    let dir = custom_dir(home);
    let (tmp, fin) = (dir.join(format!("{set}.tmp")), dir.join(set));
    let _ = std::fs::remove_dir_all(&tmp);
    let _ = std::fs::remove_dir_all(&fin);
    let write = || -> std::io::Result<()> {
        std::fs::create_dir_all(&tmp)?;
        for (name, data) in files {
            std::fs::write(tmp.join(name), data)?;
        }
        std::fs::rename(&tmp, &fin)
    };
    write().map_err(|e| {
        let _ = std::fs::remove_dir_all(&tmp);
        io_err("save the model", e)
    })
}

/// Converts a glTF model into a new set of custom models in `<home>/custom` and returns the set's ID. A file and
/// scale that were imported before give the set that is there. `taken_ids` and `taken_textures` hold the IDs of
/// the game's own objects and textures; the IDs of the other imported models are added here.
pub fn import_custom(
    home: &Path,
    data: &[u8],
    name: &str,
    scale: f64,
    taken_ids: &HashSet<u32>,
    taken_textures: &HashSet<u32>,
) -> Result<String, ModelError> {
    if data.len() > MAX_MODEL {
        return err(format!("The model is larger than {} MB", MAX_MODEL >> 20));
    }
    let set = set_id(data, scale);
    if custom_dir(home).join(&set).join("meta.json").is_file() {
        return Ok(set);
    }
    let (ids, textures) = custom_ids(home);
    let taken: HashSet<u32> = taken_ids.union(&ids).copied().collect();
    let taken_tex: HashSet<u32> = taken_textures.union(&textures).copied().collect();
    let model = convert(data, name, scale, &taken, &taken_tex)?;
    let meta = py_json(&model.meta());
    let mut files: Vec<(String, &[u8])> =
        model.pieces.iter().map(|p| (format!("{}.obj.bin", hex_id(p.id)), p.bank.as_slice())).collect();
    files.push(("textures.tex.bin".into(), &model.texture_bank));
    // meta.json is written last: a set counts as there when it has one
    files.push(("meta.json".into(), meta.as_bytes()));
    store(home, &set, &files)?;
    Ok(set)
}

/// Removes an imported model. False when `set` is not the name of a set.
pub fn delete_custom(home: &Path, set: &str) -> bool {
    if !is_set_id(set) {
        return false;
    }
    let _ = std::fs::remove_dir_all(custom_dir(home).join(set));
    true
}

// ---------------------------------------------------------------------------------------------
// Reskinned models

/// What `reskin` needs to know of the game.
pub struct Reskin<'a> {
    /// The retail model's ID, as `0x%08X`
    pub oid: &'a str,
    /// The model's name and world in the catalog
    pub name: &'a str,
    pub world: &'a str,
    /// The model's single-object bank
    pub bank: &'a [u8],
    /// The IDs of the textures the model uses (`object_texture_ids` of the former Python back end), as `0x%08X`
    pub textures: &'a [String],
    /// The textures to pick from (`reskin_pool` of the former Python back end for the sorted worlds), as `0x%08X`
    pub pool: &'a [String],
    /// The IDs of the game's own objects
    pub taken_ids: &'a HashSet<u32>,
}

fn count(data: &[u8], word: &[u8; 4]) -> usize {
    let (mut n, mut at) = (0, 0);
    while at + 4 <= data.len() {
        if &data[at..at + 4] == word {
            n += 1;
            at += 4;
        } else {
            at += 1;
        }
    }
    n
}

/// A copy of a retail model with each texture swapped for one from the chosen worlds, kept as a custom model.
/// Returns the set. `record` gives a texture's record from the game's banks, padded as `texbank::records` pads it.
/// This is `reskin` of the former Python back end: the same model, worlds and seed give the same set and the same new ID.
pub fn reskin(
    home: &Path,
    model: &Reskin,
    worlds: &[&str],
    seed: i64,
    record: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> Result<String, ModelError> {
    let mut worlds: Vec<&str> = worlds.to_vec();
    worlds.sort_unstable();
    let list = worlds.iter().map(|w| format!("'{w}'")).collect::<Vec<_>>().join(", ");
    let mut rnd = PyRandom::from_str(&format!("{} [{list}] {seed}", model.oid));
    let mut olds: Vec<&String> = model.textures.iter().collect();
    olds.sort_unstable();
    olds.dedup();
    if model.pool.is_empty() || olds.is_empty() {
        return err("no textures to swap");
    }
    let swap: Vec<(&String, &String)> = olds.iter().map(|t| (*t, rnd.choice(model.pool).unwrap_or(*t))).collect();
    let key = Value::Array(vec![json!(model.oid), json!(swap.iter().map(|(a, b)| json!([a, b])).collect::<Vec<_>>())]);
    let key = py_dumps(&key, false);
    let sid = sha1_hex(&[b"reskin", key.as_bytes()])[..10].to_string();
    if custom_dir(home).join(&sid).join("meta.json").is_file() {
        return Ok(sid);
    }
    let (ids, _) = custom_ids(home);
    let mut new = rnd.next_u32();
    // A bank that starts with "{" would be read as glTF by libgarib's tool
    while model.taken_ids.contains(&new) || ids.contains(&new) || new >> 24 == 0x7B {
        new = rnd.next_u32();
    }
    let new_id = hex_id(new);
    let Some(old) = parse_id(model.oid) else { return err(format!("{} is not an object ID", model.oid)) };
    let data = model.bank;
    if count(data, &old.to_be_bytes()) != 2 {
        return err(format!("the object ID of {} isn't where it should be", model.oid));
    }
    let mut words: Vec<([u8; 4], [u8; 4])> = vec![(old.to_be_bytes(), new.to_be_bytes())];
    for (a, b) in &swap {
        let (Some(from), Some(to)) = (parse_id(a), parse_id(b)) else { return err("a texture ID is malformed") };
        if count(data, &from.to_be_bytes()) < 1 {
            return err(format!("texture {a} isn't in {}", model.oid));
        }
        match words.iter_mut().find(|w| w.0 == from.to_be_bytes()) {
            Some(w) => w.1 = to.to_be_bytes(),
            None => words.push((from.to_be_bytes(), to.to_be_bytes())),
        }
    }
    // One pass: a new ID that equals another old one must not be replaced again
    let mut out = Vec::with_capacity(data.len());
    let mut at = 0;
    while at < data.len() {
        match data.get(at..at + 4).and_then(|w| words.iter().find(|x| x.0 == w)) {
            Some(w) => {
                out.extend_from_slice(&w.1);
                at += 4;
            }
            None => {
                out.push(data[at]);
                at += 1;
            }
        }
    }
    let mut texs: Vec<&String> = swap.iter().map(|s| s.1).collect();
    texs.sort_unstable();
    texs.dedup();
    let mut recs: Vec<(u32, Vec<u8>)> = Vec::new();
    for t in &texs {
        match (parse_id(t), record(t)) {
            (Some(id), Some(r)) => recs.push((id, r)),
            _ => return err(format!("texture {t} is in no texture bank")),
        }
    }
    let tex_bank = texbank::bank_of(&recs.iter().map(|(id, r)| (*id, r.as_slice())).collect::<Vec<_>>());
    let textures: Map<String, Value> = texs.iter().zip(&recs).map(|(t, r)| ((*t).clone(), json!(r.1.len()))).collect();
    let meta = json!({
        "name": format!("{} (reskin)", model.name), "scale": 1.0, "triangles": 0, "size": [0, 0, 0],
        "reskin": {"from": model.oid, "worlds": worlds, "seed": seed, "world": model.world},
        "pieces": [{
            "id": new_id, "name": model.name.chars().take(7).collect::<String>(), "offset": [0, 0, 0], "triangles": 0,
            "size": [0, 0, 0], "textures": texs, "bytes": out.len(), "preview": "",
        }],
        "textures": textures,
    });
    let meta = py_json(&meta);
    store(
        home,
        &sid,
        &[
            (format!("{new_id}.obj.bin"), &out),
            ("textures.tex.bin".into(), &tex_bank),
            ("meta.json".into(), meta.as_bytes()),
        ],
    )?;
    Ok(sid)
}
