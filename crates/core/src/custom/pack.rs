//! Builds an object bank from triangles, as libgarib's `objbank-tool pack` builds one from a glTF file.
//!
//! This follows libgarib's Python step by step (`objects.packActor`, `gltf_helper.MeshData`,
//! `vertex_cache_optimizer`, `display_lists.gltfNodeToDisplayList`), with the fixes Glovebox made to libgarib's
//! packer (docs/technical.md, "Notes and limits"). The bytes must equal libgarib's, so its oddities are kept and marked.

use std::collections::HashMap;

use crate::objbank::{layout, GeometryParts, Kind, Linker, Target};

use super::{err, ModelError};

/// The triangles of one material of a piece: three vertices for each triangle, as 32-bit floats.
pub struct Prim {
    pub texture: u32,
    /// The texture's width and height in texels
    pub size: (u32, u32),
    pub pos: Vec<[f32; 3]>,
    pub nrm: Vec<[f32; 3]>,
    pub uv: Vec<[f32; 2]>,
}

/// libgarib's CRC of a name. An object's ID is the hash of its root node's name.
pub fn hash_str(text: &str) -> u32 {
    let mut value = 0u32;
    for c in text.chars() {
        let index = ((value >> 24) ^ c as u32) & 0xFF;
        let mut entry = index << 24;
        for _ in 0..8 {
            entry = if entry & 0x8000_0000 != 0 { (entry << 1) ^ 0x04C1_1DB7 } else { entry << 1 };
        }
        value = (value << 8) ^ entry;
    }
    value
}

/// One primitive after libgarib's `MeshData.optimize`. An index is a vertex number times 4, plus the number of
/// the vertex's variant: one of up to four sets of texture coordinates at that position.
struct Mesh {
    /// libgarib counts every pair of position and texture coordinates, though it stores one vertex for each
    /// position. The vertices past the stored ones stay zero, and go into the bank.
    vertex_count: usize,
    pos: Vec<[f64; 3]>,
    nrm: Vec<[f64; 3]>,
    uv_scaled: Vec<[f64; 2]>,
    indices: Vec<u32>,
    variants: HashMap<u32, Vec<[f32; 2]>>,
}

/// A float as Python's dict key: 0.0 and -0.0 are one key.
fn key(v: f32) -> u32 {
    if v == 0.0 {
        0
    } else {
        v.to_bits()
    }
}

fn optimize(p: &Prim) -> Result<Mesh, ModelError> {
    let n = p.pos.len();
    if p.nrm.len() != n || p.uv.len() != n || n % 3 != 0 {
        return err("A piece's vertex lists differ in length");
    }
    let pos_key = |i: usize| p.pos[i].map(key);
    let uv_key = |i: usize| p.uv[i].map(key);
    // MeshData.duplicates: the texture coordinates used at each position
    let mut shared: HashMap<[u32; 3], Vec<[u32; 2]>> = HashMap::new();
    for i in 0..n {
        let uvs = shared.entry(pos_key(i)).or_default();
        if !uvs.contains(&uv_key(i)) {
            uvs.push(uv_key(i));
        }
    }
    let vertex_count: usize = shared.values().map(Vec::len).sum();
    let mut m = Mesh {
        vertex_count,
        pos: vec![[0.0; 3]; vertex_count],
        nrm: vec![[0.0; 3]; vertex_count],
        uv_scaled: vec![[0.0; 2]; vertex_count],
        indices: Vec::with_capacity(n),
        variants: HashMap::new(),
    };
    let mut cursor = 0usize;
    let mut write = |m: &mut Mesh, i: usize| -> Result<u32, ModelError> {
        if cursor >= vertex_count {
            return err("A piece has too many texture coordinates at one point");
        }
        m.pos[cursor] = p.pos[i].map(f64::from);
        m.nrm[cursor] = p.nrm[i].map(f64::from);
        m.uv_scaled[cursor] = [p.uv[i][0] as f64 * p.size.0 as f64, p.uv[i][1] as f64 * p.size.1 as f64];
        cursor += 1;
        Ok(((cursor - 1) as u32) << 2)
    };
    let mut mapping: HashMap<[u32; 3], u32> = HashMap::new();
    for i in 0..n {
        let pk = pos_key(i);
        let mut new_idx = match mapping.get(&pk) {
            Some(v) => *v,
            None => {
                let v = write(&mut m, i)?;
                mapping.insert(pk, v);
                v
            }
        };
        if shared[&pk].len() > 1 {
            let uv = p.uv[i];
            let list = m.variants.entry(new_idx).or_default();
            let variant = match list.iter().position(|v| v[0] == uv[0] && v[1] == uv[1]) {
                Some(v) => v,
                None => {
                    if list.len() == 4 {
                        // Two index bits pick a variant. Another vertex starts at this position.
                        new_idx = write(&mut m, i)?;
                        mapping.insert(pk, new_idx);
                        m.variants.insert(new_idx, vec![uv]);
                        0
                    } else {
                        list.push(uv);
                        list.len() - 1
                    }
                }
            };
            new_idx |= variant as u32 & 3;
        }
        m.indices.push(new_idx);
    }
    // MeshData.remove_degenerate_triangles
    let mut kept = Vec::with_capacity(n);
    for t in m.indices.chunks_exact(3) {
        let (a, b, c) = (t[0] >> 2, t[1] >> 2, t[2] >> 2);
        if a != b && a != c && b != c {
            kept.extend_from_slice(t);
        }
    }
    m.indices = kept;
    cache_order(&mut m);
    Ok(m)
}

/// `vertex_cache_optimizer.POSITION_SCORE`, as the bits of Python's floats.
const POSITION_SCORE: [u64; 32] = [
    0x3fe8000000000000, 0x3fe8000000000000, 0x3fe8000000000000, 0x3ff0000000000000, 0x3fee5bf32e542a2d,
    0x3fecbf55d93149de, 0x3feb2a4af178a2cc, 0x3fe99cf7678ea25b, 0x3fe817825e0e1cd9, 0x3fe69a1563cfa1e8,
    0x3fe524dcb6b0ba4e, 0x3fe3b80790e108ab, 0x3fe253c882f11719, 0x3fe0f855dd7a5d97, 0x3fdf4bd45c113017,
    0x3fdcb989a80f41b4, 0x3fda3a5567fe99f7, 0x3fd7cece2313e6e1, 0x3fd577993700035d, 0x3fd3356d748f24dd,
    0x3fd109166dbe64aa, 0x3fcde6f16dcc962e, 0x3fc9eb2ef9ecd37f, 0x3fc621380fbed1e7, 0x3fc28bc0368eed16,
    0x3fbe5bf32e542a2e, 0x3fb817825e0e1cd7, 0x3fb253c882f11719, 0x3faa3a5567fe99fc, 0x3fa109166dbe64a8,
    0x3f928bc0368eed1a, 0x3f7a3a5567fe99ea,
];

/// `vertex_cache_optimizer.VALENCE_SCORE`.
const VALENCE_SCORE: [u64; 31] = [
    0x0000000000000000, 0x4000000000000000, 0x3ff6a09e667f3bcd, 0x3ff279a74590331c, 0x3ff0000000000000,
    0x3fec9f25c5bfedd9, 0x3fea20bd700c2c3e, 0x3fe83091e6a7f7e7, 0x3fe6a09e667f3bcd, 0x3fe5555555555555,
    0x3fe43d136248490f, 0x3fe34bf63d156826, 0x3fe279a74590331c, 0x3fe1c01aa03be896, 0x3fe11acee560242a,
    0x3fe08654a2d4f6db, 0x3fe0000000000000, 0x3fdf0b6848d2af1c, 0x3fde2b7dddfefa66, 0x3fdd5d7ea914b937,
    0x3fdc9f25c5bfedd9, 0x3fdbee9056fb9c39, 0x3fdb4a293c1d9550, 0x3fdab099ae8f539a, 0x3fda20bd700c2c3e,
    0x3fd999999999999a, 0x3fd91a556151761c, 0x3fd8a2345cc04426, 0x3fd83091e6a7f7e7, 0x3fd7c4dd663ebb88,
    0x3fd75e9746a0b098,
];

fn vertex_score(active: usize, cache_position: i32) -> f64 {
    if active == 0 {
        return 0.0;
    }
    let mut score = if cache_position > 0 { f64::from_bits(POSITION_SCORE[cache_position as usize]) } else { 0.0 };
    if active < VALENCE_SCORE.len() {
        score += f64::from_bits(VALENCE_SCORE[active]);
    }
    score
}

/// `vertex_cache_optimizer.optimize` with a cache of 32. It is kept as libgarib has it: a vertex starts with
/// cache position 0, which reads as "in the cache already", so in practice the cache stays empty. The result is
/// the best first triangle, then the others in their order.
fn cache_order(m: &mut Mesh) {
    const CACHE: usize = 32;
    let faces = m.indices.len() / 3;
    let mut tris: Vec<Vec<usize>> = vec![Vec::new(); m.vertex_count];
    for (i, idx) in m.indices.iter().enumerate() {
        tris[(idx >> 2) as usize].push(i / 3);
    }
    let mut cache_tag = vec![0i32; m.vertex_count];
    let mut added = vec![false; faces];
    let mut tri_score = vec![0f64; faces];
    let last_score: Vec<f64> = (0..m.vertex_count).map(|v| vertex_score(tris[v].len(), cache_tag[v])).collect();
    for v in 0..m.vertex_count {
        for f in &tris[v] {
            tri_score[*f] += last_score[v];
        }
    }
    let mut best: Option<usize> = None;
    let mut best_score = -1.0;
    for f in 0..faces {
        if tri_score[f] > best_score {
            best = Some(f);
            best_score = tri_score[f];
        }
    }
    let mut out = Vec::with_capacity(m.indices.len());
    let mut cache = [-1i64; CACHE + 3];
    let mut scan = 0usize;
    while let Some(tri) = best {
        added[tri] = true;
        let face = [m.indices[tri * 3], m.indices[tri * 3 + 1], m.indices[tri * 3 + 2]];
        out.extend_from_slice(&face);
        for (fv, idx) in face.iter().enumerate() {
            let v = (idx >> 2) as usize;
            let mut end = cache_tag[v];
            if end < 0 {
                end = (CACHE + fv) as i32;
            }
            if end > fv as i32 {
                for s in (fv + 1..=end as usize).rev() {
                    cache[s] = cache[s - 1];
                    if cache[s] >= 0 {
                        cache_tag[cache[s] as usize] += 1;
                    }
                }
                cache[fv] = v as i64;
                cache_tag[v] = fv as i32;
            }
            if let Some(at) = tris[v].iter().position(|t| *t == tri) {
                tris[v].remove(at);
            }
        }
        for c in 0..CACHE + 3 {
            if cache[c] < 0 {
                break;
            }
            let v = cache[c] as usize;
            if c >= CACHE {
                cache_tag[v] = -1;
                cache[c] = -1;
            }
            let delta = vertex_score(tris[v].len(), cache_tag[v]) - last_score[v];
            for f in &tris[v] {
                tri_score[*f] += delta;
            }
        }
        best = None;
        best_score = -1.0;
        for c in cache.iter().take(CACHE) {
            if *c < 0 {
                break;
            }
            for f in &tris[*c as usize] {
                if tri_score[*f] > best_score {
                    best = Some(*f);
                    best_score = tri_score[*f];
                }
            }
        }
        if best.is_none() {
            while scan < faces {
                if !added[scan] {
                    best = Some(scan);
                    break;
                }
                scan += 1;
            }
        }
    }
    m.indices = out;
}

/// Every primitive of a piece as one mesh (`MeshData.flatten`), with the texture of each face.
struct Flat {
    mesh: Mesh,
    face_texture: Vec<u32>,
    sizes: HashMap<u32, (u32, u32)>,
}

fn flatten(prims: &[Prim]) -> Result<Flat, ModelError> {
    let mut mesh =
        Mesh { vertex_count: 0, pos: vec![], nrm: vec![], uv_scaled: vec![], indices: vec![], variants: HashMap::new() };
    let (mut face_texture, mut sizes) = (Vec::new(), HashMap::new());
    for p in prims {
        if p.size.0 == 0 || p.size.1 == 0 {
            return err("A texture has no size");
        }
        let m = optimize(p)?;
        let base = (mesh.vertex_count as u32) << 2;
        mesh.indices.extend(m.indices.iter().map(|i| i + base));
        for (k, v) in m.variants {
            mesh.variants.insert(k + base, v);
        }
        face_texture.extend(std::iter::repeat(p.texture).take(m.indices.len() / 3));
        mesh.pos.extend(m.pos);
        mesh.nrm.extend(m.nrm);
        mesh.uv_scaled.extend(m.uv_scaled);
        mesh.vertex_count += m.vertex_count;
        sizes.insert(p.texture, p.size);
    }
    // A display list's face holds indices as 16 bits, variant bits included
    if mesh.vertex_count >= 1 << 14 || face_texture.len() > 0xFFFF {
        return err("A piece of the model has too many vertices");
    }
    Ok(Flat { mesh, face_texture, sizes })
}

/// Python's `int(x)` packed as a signed number of `bits` bits. libgarib stops when it does not fit.
fn fixed(v: f64, bits: u32) -> Result<i64, ModelError> {
    let limit = 1i64 << (bits - 1);
    if !v.is_finite() || v.trunc() < -(limit as f64) || v.trunc() >= limit as f64 {
        return err("A coordinate of the model does not fit a Glover model. Use a smaller scale.");
    }
    Ok(v.trunc() as i64)
}

fn geometry(f: &Flat) -> GeometryParts<'static> {
    let m = &f.mesh;
    let faces = m.indices.len() / 3;
    let mut root = vec![0u8; layout::GEOMETRY];
    root[0..2].copy_from_slice(&(faces as u16).to_be_bytes());
    root[2..4].copy_from_slice(&(m.vertex_count as u16).to_be_bytes());
    let face_bytes: Vec<u8> = m.indices.iter().flat_map(|i| ((i >> 2) as u16).to_be_bytes()).collect();
    let verts: Vec<u8> = m.pos.iter().flatten().flat_map(|v| (*v as f32).to_be_bytes()).collect();
    // One normal for each face, from its corners, turned to the side of the first corner's normal
    let mut norms = Vec::with_capacity(faces * 4);
    for t in m.indices.chunks_exact(3) {
        let v = [m.pos[(t[0] >> 2) as usize], m.pos[(t[1] >> 2) as usize], m.pos[(t[2] >> 2) as usize]];
        let a = [v[1][0] - v[0][0], v[1][1] - v[0][1], v[1][2] - v[0][2]];
        let b = [v[2][0] - v[0][0], v[2][1] - v[0][1], v[2][2] - v[0][2]];
        let mut n = [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
        let mag = (0.0 + n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        let vn = m.nrm[(t[0] >> 2) as usize];
        for c in &mut n {
            *c /= mag;
        }
        if n[0] * vn[0] + n[1] * vn[1] + n[2] * vn[2] < 0.0 {
            for c in &mut n {
                *c *= -1.0;
            }
        }
        for c in n {
            let c = c * 127.0;
            // libgarib packs a number that is not finite as 0
            norms.push(if c.is_finite() { c as i64 as i8 as u8 } else { 0 });
        }
        norms.push(0);
    }
    GeometryParts {
        root: root.into(),
        verts: Some(verts.into()),
        faces: Some(face_bytes.into()),
        face_cn: Some(norms.into()),
        ..Default::default()
    }
}

// F3DEX commands, as libgarib's `gbi.py` packs them

fn cmd(out: &mut Vec<u8>, bits: u64) {
    out.extend_from_slice(&bits.to_be_bytes());
}

const G_TX_LOADTILE: u64 = 7;

fn set_tile(out: &mut Vec<u8>, siz: u64, line: u64, tile: u64, size: (u32, u32)) {
    let (masks, maskt) = (size.0.ilog2() as u64, size.1.ilog2() as u64);
    cmd(out, 0xF5 << 56 | siz << 51 | (line & 0x1FF) << 41 | tile << 24 | (maskt & 15) << 14 | (masks & 15) << 4);
}

/// `writeTextureLoad` for a 16-bit RGBA texture.
fn texture_load(out: &mut Vec<u8>, texture: u32, size: (u32, u32)) {
    let (w, h) = (size.0 as u64, size.1 as u64);
    cmd(out, 0xE6 << 56);
    cmd(out, 0xBA << 56 | 14 << 40 | 2 << 32);
    cmd(out, 0xFD << 56 | 2 << 51 | texture as u64);
    set_tile(out, 2, 0, G_TX_LOADTILE, size);
    cmd(out, 0xE6 << 56);
    let row_words = (w * 2 / 8).max(1);
    let dxt = (2048 + row_words - 1) / row_words;
    cmd(out, 0xF3 << 56 | G_TX_LOADTILE << 24 | ((w * h - 1) & 0xFFF) << 12 | (dxt & 0xFFF));
    cmd(out, 0xE7 << 56);
    set_tile(out, 2, (w * 2 + 7) >> 3, 0, size);
    cmd(out, 0xF2 << 56 | (((w - 1) << 2) & 0xFFF) << 12 | (((h - 1) << 2) & 0xFFF));
}

/// The display list of a piece: the commands, and a block of vertices for each batch of up to 32.
fn display_list(f: &Flat) -> Result<DisplayList, ModelError> {
    let m = &f.mesh;
    let mut cmds = Vec::new();
    cmd(&mut cmds, 0xB6 << 56 | 0x20000);
    cmd(&mut cmds, 0xB7 << 56 | 0x20000);
    let mut blocks: Vec<Vec<u8>> = Vec::new();
    let mut pointers: Vec<(u32, usize)> = Vec::new();
    let mut loaded: Option<u32> = None;
    let mut cursor = 0usize;
    while cursor < m.indices.len() {
        // buildDLFaceBatch: faces until a 33rd vertex is needed
        let mut batch: Vec<u32> = Vec::new();
        let mut unique: Vec<u32> = Vec::new();
        let start = cursor;
        while cursor < m.indices.len() {
            let v = m.indices[cursor] >> 2;
            if !unique.contains(&v) {
                if unique.len() >= 32 {
                    break;
                }
                unique.push(v);
            }
            batch.push(v);
            cursor += 1;
        }
        let over = batch.len() % 3;
        if over > 0 {
            batch.truncate(batch.len() - over);
            cursor -= over;
            unique = batch.clone();
        }
        unique.sort_unstable();
        unique.dedup();
        if cursor == start {
            return err("A piece of the model could not be put in a display list");
        }
        let mut block = Vec::with_capacity(unique.len() * layout::DL_VERTEX);
        let mut slot: HashMap<u32, u8> = HashMap::new();
        for (i, v) in unique.iter().enumerate() {
            slot.insert(*v, i as u8);
            let (p, uv, n) = (m.pos[*v as usize], m.uv_scaled[*v as usize], m.nrm[*v as usize]);
            for c in p {
                block.extend_from_slice(&(fixed(c, 16)? as i16).to_be_bytes());
            }
            block.extend_from_slice(&[0, 0]);
            for c in uv {
                block.extend_from_slice(&(fixed(c * 32.0, 16)? as i16).to_be_bytes());
            }
            for c in n {
                block.push(fixed(c * 127.0, 8)? as i8 as u8);
            }
            block.push(0);
        }
        // The batch's faces: three indices and the face's number
        let mut faces: Vec<[u32; 4]> = (start..cursor)
            .step_by(3)
            .map(|i| [m.indices[i], m.indices[i + 1], m.indices[i + 2], (i / 3) as u32])
            .collect();
        // sort_faces_by_variant. libgarib's key also holds the original index. This sort is stable, which gives the
        // same order without the index.
        faces.sort_by_key(|face| {
            let t = f.face_texture[face[3] as usize];
            (Some(t) != loaded, t, face[0] & 3, face[1] & 3, face[2] & 3)
        });
        let mut current = vec![0u32; unique.len()];
        let mut vertices_loaded = false;
        let block_index = blocks.len();
        let mut at = 0usize;
        while at < faces.len() {
            let face = faces[at];
            let texture = f.face_texture[face[3] as usize];
            // iterate_batch_faces: two triangles go in one command when they share a material and no vertex
            // is used with two variants
            let mut second = faces.get(at + 1).copied().filter(|n| f.face_texture[n[3] as usize] == texture);
            if let Some(next) = second {
                let mut seen: Vec<(u32, u32)> = Vec::with_capacity(6);
                for idx in face[..3].iter().chain(&next[..3]) {
                    match seen.iter().find(|(v, _)| *v == idx >> 2) {
                        Some((_, variant)) if *variant != idx & 3 => {
                            second = None;
                            break;
                        }
                        Some(_) => {}
                        None => seen.push((idx >> 2, idx & 3)),
                    }
                }
            }
            at += if second.is_some() { 2 } else { 1 };
            let mut vtx_load = |cmds: &mut Vec<u8>| {
                let len = unique.len() as u64 * layout::DL_VERTEX as u64;
                cmd(cmds, 0x04 << 56 | (unique.len() as u64 & 63) << 42 | ((len - 1) & 0x3FF) << 32);
                pointers.push((cmds.len() as u32 - 4, block_index));
            };
            if Some(texture) != loaded {
                let size = f.sizes[&texture];
                texture_load(&mut cmds, texture, size);
                vtx_load(&mut cmds);
                vertices_loaded = true;
                // writeTileSettings
                cmd(&mut cmds, 0xE8 << 56);
                set_tile(&mut cmds, 2, (size.0 as u64 * 2) >> 3, 0, size);
                loaded = Some(texture);
            }
            if !vertices_loaded {
                vtx_load(&mut cmds);
                vertices_loaded = true;
            }
            // prepare_variants: change a loaded vertex's texture coordinates when the face wants another variant
            let mut prepare = |cmds: &mut Vec<u8>, tri: &[u32]| -> Result<[u64; 3], ModelError> {
                let mut out = [0u64; 3];
                for (k, full) in tri.iter().enumerate() {
                    let s = slot[&(full >> 2)];
                    let variant = full & 3;
                    if current[s as usize] != variant {
                        let list = m.variants.get(&(full & !3));
                        let (Some(old), Some(new)) = (
                            list.and_then(|l| l.get(current[s as usize] as usize)),
                            list.and_then(|l| l.get(variant as usize)),
                        ) else {
                            return err("A piece of the model has a broken vertex variant");
                        };
                        if old[0] != new[0] || old[1] != new[1] {
                            // libgarib packs the coordinates before they are scaled by the texture's size
                            let u = fixed(new[0] as f64 * 32.0, 16)? as i16 as u16 as u64;
                            let v = fixed(new[1] as f64 * 32.0, 16)? as i16 as u16 as u64;
                            cmd(cmds, 0xB2 << 56 | 0x14 << 48 | (s as u64 * 2) << 32 | u << 16 | v);
                        }
                        current[s as usize] = variant;
                    }
                    out[k] = s as u64 * 2;
                }
                Ok(out)
            };
            let a = prepare(&mut cmds, &face[..3])?;
            match second {
                Some(next) => {
                    let b = prepare(&mut cmds, &next[..3])?;
                    cmd(&mut cmds, 0xB1 << 56 | a[0] << 48 | a[1] << 40 | a[2] << 32 | b[0] << 16 | b[1] << 8 | b[2]);
                }
                None => cmd(&mut cmds, 0xBF << 56 | a[0] << 16 | a[1] << 8 | a[2]),
            }
        }
        blocks.push(block);
    }
    cmd(&mut cmds, 0xB8 << 56);
    Ok(DisplayList { cmds, pointers, blocks })
}

struct DisplayList {
    cmds: Vec<u8>,
    /// Where in `cmds` a pointer goes, and the vertex block it points to
    pointers: Vec<(u32, usize)>,
    blocks: Vec<Vec<u8>>,
}

fn key_frame(v: [f32; 4]) -> Vec<u8> {
    let mut out: Vec<u8> = v.iter().flat_map(|c| c.to_be_bytes()).collect();
    out.extend_from_slice(&0u32.to_be_bytes());
    out
}

/// A bank with one object made of one mesh. `node_name` is the name libgarib would read from the glTF node: the
/// mesh's ID is its hash and the mesh's name its first 8 characters. `render_mode` holds the mesh's flags.
pub fn pack_object(node_name: &str, id: u32, render_mode: u16, prims: &[Prim]) -> Result<Vec<u8>, ModelError> {
    let flat = flatten(prims)?;
    let dl = display_list(&flat)?;
    let mut linker = Linker::new();
    let props = linker.bytes(Kind::AnimProps, vec![0u8; layout::ANIMATION]);
    let translation = linker.bytes(Kind::Keyframes, key_frame([0.0; 4]));
    let rotation = linker.bytes(Kind::Keyframes, key_frame([0.0; 4]));
    let scale = linker.bytes(Kind::Keyframes, key_frame([1.0, 1.0, 1.0, 0.0]));
    let geo = linker.add_geometry(geometry(&flat));
    let mut blocks = vec![crate::objbank::Block::new(dl.cmds)];
    blocks.extend(dl.blocks.into_iter().map(crate::objbank::Block::new));
    let list = linker.add(Kind::DisplayList, blocks);
    for (at, block) in dl.pointers {
        linker.point(list, 0, at, Target::block(list, block as u32 + 1));
    }

    let mut mesh = vec![0u8; layout::MESH];
    mesh[0..4].copy_from_slice(&hash_str(node_name).to_be_bytes());
    for (dst, c) in mesh[4..12].iter_mut().zip(node_name.chars()) {
        *dst = c as u8;
    }
    mesh[13] = 0xFF;
    for at in [14, 16, 18] {
        mesh[at..at + 2].copy_from_slice(&1u16.to_be_bytes());
    }
    mesh[50..52].copy_from_slice(&render_mode.to_be_bytes());
    let mesh = linker.bytes(Kind::Mesh, mesh);
    linker.point(mesh, 0, layout::MESH_TRANSLATION_PTR, Target::seg(translation));
    linker.point(mesh, 0, layout::MESH_ROTATION_PTR, Target::seg(rotation));
    linker.point(mesh, 0, layout::MESH_SCALE_PTR, Target::seg(scale));
    linker.point(mesh, 0, layout::MESH_GEOMETRY_PTR, geo);
    linker.point(mesh, 0, layout::MESH_DISPLAY_LIST_PTR, Target::seg(list));

    let mut actor = vec![0u8; layout::OBJECT];
    actor[0..4].copy_from_slice(&id.to_be_bytes());
    let actor = linker.bytes(Kind::Actor, actor);
    linker.point(actor, 0, layout::OBJECT_ANIMATION_PTR, Target::seg(props));
    linker.point(actor, 0, layout::OBJECT_MESH_PTR, Target::seg(mesh));
    linker.set_actor(id, actor);
    linker.link().map_err(|e| ModelError(e.to_string()))
}

// ---------------------------------------------------------------------------------------------------------------
// The banks of a level pack for g64coopdx

/// The rotation key the game reads as no turn: x, y and z 0, w 1.
const IDENTITY_ROTATION: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

/// One texture bank and one object bank that hold `pieces`, for a level pack of g64coopdx (its
/// docs/discovery/model-banks.md). Returns the texture bank, then the object bank. Neither needs the ROM.
///
/// - The object bank holds the pieces alone, linked with no bank of the game's under them.
/// - The texture bank holds the records the pieces use, each padded to a multiple of 8 bytes with its size field
///   fixed, in rising order of id. No list of names follows them. The bank ends with zeros up to a multiple of 8.
/// - Each rotation key of all zeros becomes the key of no turn (0, 0, 0, 1). The game turns a zero key into a
///   matrix of values that are not numbers. The converter still writes zero keys; only this copy changes.
///
/// `texture_banks` are the banks that hold the pieces' textures, such as `CustomSet::texture_bank`. A texture in two
/// of them is taken from the first.
pub fn model_banks(pieces: &[&super::Piece], texture_banks: &[&[u8]]) -> Result<(Vec<u8>, Vec<u8>), ModelError> {
    if pieces.is_empty() {
        return err("A level pack's models need at least one piece");
    }
    // The linker replaces an object with a later one of the same id. The first one's textures would stay in the
    // texture bank with no object to use them.
    let mut ids: Vec<u32> = pieces.iter().map(|p| p.id).collect();
    ids.sort_unstable();
    if let Some(w) = ids.windows(2).find(|w| w[0] == w[1]) {
        return err(format!("Two pieces have the id 0x{:08X}", w[0]));
    }
    let mut wanted: Vec<u32> = pieces.iter().flat_map(|p| p.textures.iter().copied()).collect();
    wanted.sort_unstable();
    wanted.dedup();
    let parsed = texture_banks
        .iter()
        .map(|b| crate::texbank::records(b))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| ModelError(e.to_string()))?;
    let mut records: Vec<&[u8]> = Vec::with_capacity(wanted.len());
    for id in &wanted {
        let found = parsed.iter().flat_map(|r| r.records.iter()).find(|(rid, _)| rid == id);
        match found {
            Some((_, rec)) => records.push(rec.as_ref()),
            None => return err(format!("Texture 0x{id:08X} of the model is in none of the texture banks")),
        }
    }
    let mut textures = crate::texbank::write_bank(&records, &[] as &[&[u8]]);
    // The count takes 4 bytes and every record a multiple of 8, so the bank ends 4 bytes past a multiple of 8.
    // The object bank follows it in models.bin, and its display lists must start on a multiple of 8.
    textures.resize(textures.len().next_multiple_of(8), 0);

    let banks = pieces
        .iter()
        .map(|p| crate::objbank::Bank::parse(&p.bank))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| ModelError(e.to_string()))?;
    let mut linker = Linker::new();
    for b in &banks {
        linker.add_bank(b).map_err(|e| ModelError(e.to_string()))?;
    }
    let mut objects = linker.link().map_err(|e| ModelError(e.to_string()))?;
    identity_rotations(&mut objects)?;
    Ok((textures, objects))
}

/// Puts the key of no turn in place of each rotation key of all zeros. The keys are found through each mesh's
/// pointer and count. A search for the bytes would also find the translation keys, which are zeros too.
fn identity_rotations(bank: &mut [u8]) -> Result<(), ModelError> {
    let mut keys: Vec<usize> = Vec::new();
    {
        let parsed = crate::objbank::Bank::parse(bank).map_err(|e| ModelError(e.to_string()))?;
        for mesh in parsed.objects.iter().flat_map(|o| o.meshes.iter()) {
            let f = layout::MESH_ROTATION_PTR as usize;
            let start = u32::from_be_bytes([mesh.raw[f], mesh.raw[f + 1], mesh.raw[f + 2], mesh.raw[f + 3]]) as usize;
            let count = mesh.rotation.map_or(0, |r| r.len() / layout::KEY);
            keys.extend((0..count).map(|i| start + i * layout::KEY));
        }
    }
    keys.sort_unstable();
    keys.dedup();
    for at in keys {
        let key = &mut bank[at..at + 16];
        let zero = key.chunks_exact(4).all(|c| f32::from_be_bytes([c[0], c[1], c[2], c[3]]) == 0.0);
        if zero {
            for (dst, v) in key.chunks_exact_mut(4).zip(IDENTITY_ROTATION) {
                dst.copy_from_slice(&v.to_be_bytes());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objbank::Bank;
    use std::collections::HashSet;

    /// A cube drawn from scratch: `tests/data/cube.gltf`, written by `tests/data/make_cube.py`.
    const CUBE: &[u8] = include_bytes!("../../tests/data/cube.gltf");

    fn u32_at(d: &[u8], at: usize) -> u32 {
        u32::from_be_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
    }

    fn cube() -> (super::super::CustomSet, Vec<u8>, Vec<u8>) {
        let set = super::super::convert(CUBE, "cube", 1.0, &HashSet::new(), &HashSet::new()).unwrap();
        let pieces: Vec<&super::super::Piece> = set.pieces.iter().collect();
        let (t, o) = model_banks(&pieces, &[&set.texture_bank]).unwrap();
        (set, t, o)
    }

    #[test]
    fn cube_banks() {
        let (set, tex, obj) = cube();
        assert_eq!(set.pieces.len(), 1);
        assert_eq!(set.triangles, 12);
        assert!(!tex.is_empty() && !obj.is_empty());
        assert_eq!(tex.len() % 8, 0);
        assert_eq!(obj.len() % 8, 0);
        for bank in [&tex, &obj] {
            assert_ne!(&bank[..4], b"FLA2");
        }

        // The texture bank: one 8 by 8 record of 16-bit RGBA, padded to 168 bytes, then only zeros
        assert_eq!(u32_at(&tex, 0), 1);
        let rec = crate::texbank::Texture::parse(&tex[4..]).unwrap();
        assert_eq!(rec.id, set.textures[0].0);
        assert_eq!((rec.width, rec.height, rec.masks, rec.maskt), (8, 8, 3, 3));
        assert_eq!((rec.color_format, rec.compression_format), (0, 2));
        assert_eq!((rec.flags, rec.data_ptr, rec.palette_offset), (0, 36, 0));
        assert_eq!(rec.length, 168);
        assert_eq!(rec.length as usize, (crate::texbank::HEADER + 2 * 8 * 8).next_multiple_of(8));
        let end = 4 + rec.length as usize;
        assert_eq!(tex.len(), end.next_multiple_of(8));
        assert!(tex[end..].iter().all(|b| *b == 0), "no names after the records");

        // The object bank: the cube alone, with the key of no turn
        let bank = Bank::parse(&obj).unwrap();
        assert_eq!(bank.objects.len(), 1);
        assert_eq!(bank.objects[0].id, set.pieces[0].id);
        // The game's loop stops at an entry whose instance offset (the word at 12) is 0.
        assert!(obj[8..16].iter().all(|&b| b == 0), "the directory ends after one entry");
        for mesh in bank.objects.iter().flat_map(|o| o.meshes.iter()) {
            let keys: Vec<_> = crate::objbank::Mesh::keys(mesh.rotation).collect();
            assert_eq!(keys.len(), 1);
            for k in keys {
                assert!(k.v.iter().any(|c| *c != 0.0), "a rotation key of all zeros");
                assert_eq!(k.v, IDENTITY_ROTATION);
            }
        }
        // The object reads back as a model that uses the bank's texture
        let size = |id: u32| (id == rec.id).then_some((8u16, 8u16));
        let used: Vec<u32> = crate::objbank::texture_ids(&bank, set.pieces[0].id, &size).into_iter().collect();
        assert_eq!(used, vec![rec.id]);
    }

    #[test]
    fn the_converter_keeps_zero_keys() {
        let (set, _, _) = cube();
        let bank = Bank::parse(&set.pieces[0].bank).unwrap();
        let key = crate::objbank::Mesh::keys(bank.objects[0].meshes[0].rotation).next().unwrap();
        assert_eq!(key.v, [0.0; 4]);
    }

    #[test]
    fn same_bytes_every_time() {
        let (_, t1, o1) = cube();
        let (_, t2, o2) = cube();
        assert_eq!((t1, o1), (t2, o2));
    }

    #[test]
    fn missing_texture_is_an_error() {
        let (set, _, _) = cube();
        let pieces: Vec<&super::super::Piece> = set.pieces.iter().collect();
        let empty = crate::texbank::write_bank(&[] as &[&[u8]], &[] as &[&[u8]]);
        assert!(model_banks(&pieces, &[&empty]).is_err());
        assert!(model_banks(&[], &[&set.texture_bank]).is_err());
    }

    #[test]
    fn two_pieces_with_one_id_are_an_error() {
        let (set, _, _) = cube();
        let piece = &set.pieces[0];
        let e = model_banks(&[piece, piece], &[&set.texture_bank]).unwrap_err();
        assert!(e.to_string().contains(&format!("0x{:08X}", piece.id)), "{e}");
    }
}
