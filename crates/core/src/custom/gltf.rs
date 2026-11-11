//! Reads a glTF 2.0 model for Import model: a `.glb` file, or a `.gltf` file with its data in data URIs.
//!
//! This is the reader of `tools/custom_model.py`. It takes the triangles of the default scene in world space, with
//! normals, texture coordinates and the material of each. The file is untrusted. Every read is checked.

use serde_json::Value;

use super::{err, ModelError, MAX_INPUT_TRIANGLES};

/// The most elements one accessor may hold.
const MAX_ACCESSOR: usize = 4_000_000;
/// The most nodes a walk of the scene may visit. A file can name one node as a child many times.
const MAX_VISITS: usize = 1_000_000;

pub const REPEAT: i64 = 10497;
pub const MIRRORED_REPEAT: i64 = 33648;

/// A parsed file: the JSON and the bytes of each buffer.
pub struct Gltf {
    pub json: Value,
    bufs: Vec<Vec<u8>>,
}

/// Every triangle of the scene. `m` is the material's index, or -1.
#[derive(Default)]
pub struct Triangles {
    pub p: Vec<[[f64; 3]; 3]>,
    pub n: Vec<[[f64; 3]; 3]>,
    pub uv: Vec<[[f64; 2]; 3]>,
    pub m: Vec<i64>,
}

fn not_gltf<T>(why: impl std::fmt::Display) -> Result<T, ModelError> {
    err(format!("Not a glTF model: {why}"))
}

/// Python's `base64.b64decode`: characters outside the alphabet are skipped, and the padding must be right.
pub fn base64(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let (mut acc, mut bits, mut pad, mut count) = (0u32, 0u32, 0usize, 0usize);
    for c in text.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => {
                if count % 4 >= 2 {
                    pad += 1;
                    if (count + pad) % 4 == 0 {
                        break;
                    }
                }
                continue;
            }
            _ => continue,
        };
        pad = 0;
        count += 1;
        acc = (acc << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    ((count + pad) % 4 == 0).then_some(out)
}

fn data_uri(uri: &str) -> Option<Vec<u8>> {
    base64(uri.split_once(',')?.1)
}

/// A whole number field. JSON numbers with a fraction do not count.
fn int(v: &Value, key: &str) -> Option<i64> {
    let n = v.get(key)?;
    n.as_i64().or_else(|| n.as_f64().filter(|f| f.fract() == 0.0 && f.abs() < 9e15).map(|f| f as i64))
}

fn list<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key).and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])
}

fn floats(v: &Value, key: &str) -> Option<Vec<f64>> {
    let a = v.get(key)?.as_array()?;
    if a.is_empty() {
        return None;
    }
    a.iter().map(Value::as_f64).collect()
}

fn le32(d: &[u8], at: usize) -> Option<usize> {
    Some(u32::from_le_bytes(d.get(at..at + 4)?.try_into().ok()?) as usize)
}

impl Gltf {
    pub fn parse(data: &[u8]) -> Result<Gltf, ModelError> {
        let text = data.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(data);
        let (json, blob) = if text.iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'{') {
            (text, None)
        } else {
            if data.len() < 4 {
                return not_gltf("unpack requires a buffer of 4 bytes");
            }
            if data.len() < 12 {
                return not_gltf("unpack requires a buffer of 8 bytes");
            }
            if &data[..4] != b"glTF" {
                return not_gltf("Unable to load binary gltf file. Header does not appear to be valid glb format.");
            }
            let length = le32(data, 8).unwrap_or(0);
            let (mut at, mut json, mut blob) = (12usize, None, None);
            while at < length {
                let Some(size) = le32(data, at) else { return not_gltf("a chunk is cut short") };
                let Some(kind) = data.get(at + 4..at + 8) else { return not_gltf("a chunk is cut short") };
                at += 8;
                // Python's slices stop at the end of the data
                let body = &data[at.min(data.len())..at.saturating_add(size).min(data.len())];
                match kind {
                    b"JSON" => json = Some(body),
                    b"BIN\0" => {
                        if json.is_none() {
                            return not_gltf("the binary chunk comes before the JSON");
                        }
                        blob = Some(body)
                    }
                    _ => {}
                }
                at = at.saturating_add(size);
            }
            match json {
                Some(j) => (j, blob),
                None => return not_gltf("the file has no JSON chunk"),
            }
        };
        let json: Value = match serde_json::from_slice(json) {
            Ok(v @ Value::Object(_)) => v,
            Ok(_) => return not_gltf("the JSON is not an object"),
            Err(e) => return not_gltf(e),
        };
        let mut bufs = Vec::new();
        for b in list(&json, "buffers") {
            match b.get("uri").and_then(Value::as_str) {
                None => bufs.push(blob.unwrap_or(&[]).to_vec()),
                Some(uri) if uri.starts_with("data:") => match data_uri(uri) {
                    Some(d) => bufs.push(d),
                    None => return err("A buffer of the model is not valid base64"),
                },
                Some(uri) => {
                    return err(format!("The model refers to a separate file ({uri}). Export it as a single .glb file."))
                }
            }
        }
        Ok(Gltf { json, bufs })
    }

    fn item(&self, key: &str, index: i64) -> Result<&Value, ModelError> {
        match usize::try_from(index).ok().and_then(|i| list(&self.json, key).get(i)) {
            Some(v) => Ok(v),
            None => err(format!("The model refers to item {index} of {key}, which it does not have")),
        }
    }

    /// The bytes of a buffer view and its stride.
    fn view(&self, index: i64) -> Result<(&[u8], Option<i64>), ModelError> {
        let v = self.item("bufferViews", index)?;
        let buf = match int(v, "buffer").and_then(|b| usize::try_from(b).ok()).and_then(|b| self.bufs.get(b)) {
            Some(b) => b.as_slice(),
            None => return err("A buffer view of the model has no buffer"),
        };
        let (Ok(start), Some(Ok(len))) =
            (usize::try_from(int(v, "byteOffset").unwrap_or(0)), int(v, "byteLength").map(usize::try_from))
        else {
            return err("A buffer view of the model has a bad size");
        };
        let start = start.min(buf.len());
        Ok((&buf[start..start.saturating_add(len).min(buf.len())], int(v, "byteStride").filter(|s| *s != 0)))
    }

    /// An accessor's values as rows of `width` numbers.
    fn accessor(&self, index: i64) -> Result<(Vec<f64>, usize), ModelError> {
        let a = self.item("accessors", index)?;
        if a.get("sparse").is_some_and(|s| !s.is_null()) {
            return err("Sparse accessors are not supported. Export the model without them.");
        }
        let ctype = int(a, "componentType").unwrap_or(0);
        let size = match ctype {
            5120 | 5121 => 1usize,
            5122 | 5123 => 2,
            5125 | 5126 => 4,
            _ => return err(format!("Unsupported accessor component type {ctype}")),
        };
        let kind = a.get("type").and_then(Value::as_str).unwrap_or("None");
        let width = match kind {
            "SCALAR" => 1usize,
            "VEC2" => 2,
            "VEC3" => 3,
            "VEC4" => 4,
            _ => return err(format!("Unsupported accessor type {kind}")),
        };
        let count = match int(a, "count").map(usize::try_from) {
            Some(Ok(c)) if c <= MAX_ACCESSOR => c,
            Some(Ok(_)) => return err("An accessor of the model is too large"),
            _ => return err("An accessor of the model has no count"),
        };
        let Some(view) = int(a, "bufferView") else {
            return Ok((vec![0.0; count * width], width));
        };
        let (data, stride) = self.view(view)?;
        let item = width * size;
        let past = || err("An accessor reads past the end of its buffer");
        let (Ok(stride), Ok(start)) =
            (usize::try_from(stride.unwrap_or(item as i64)), usize::try_from(int(a, "byteOffset").unwrap_or(0)))
        else {
            return past();
        };
        if count == 0 {
            return Ok((Vec::new(), width));
        }
        match stride.checked_mul(count - 1).and_then(|n| n.checked_add(start)).and_then(|n| n.checked_add(item)) {
            Some(end) if end <= data.len() => {}
            _ => return past(),
        }
        let normalised = a.get("normalized").and_then(Value::as_bool) == Some(true);
        let divisor = match ctype {
            5120 => 127.0,
            5121 => 255.0,
            5122 => 32767.0,
            5123 => 65535.0,
            _ => 0.0,
        };
        let mut out = Vec::with_capacity(count * width);
        for i in 0..count {
            let row = &data[start + i * stride..start + i * stride + item];
            for c in row.chunks_exact(size) {
                let v = match ctype {
                    5120 => c[0] as i8 as f64,
                    5121 => c[0] as f64,
                    5122 => i16::from_le_bytes([c[0], c[1]]) as f64,
                    5123 => u16::from_le_bytes([c[0], c[1]]) as f64,
                    5125 => u32::from_le_bytes([c[0], c[1], c[2], c[3]]) as f64,
                    _ => f32::from_le_bytes([c[0], c[1], c[2], c[3]]) as f64,
                };
                out.push(if normalised && divisor != 0.0 { (v / divisor).max(-1.0) } else { v });
            }
        }
        Ok((out, width))
    }

    /// The bytes of the image behind a texture index, or None when the texture has no image.
    pub fn image(&self, texture: Option<i64>) -> Result<Option<Vec<u8>>, ModelError> {
        let textures = list(&self.json, "textures");
        let Some(tex) = texture.and_then(|t| usize::try_from(t).ok()).and_then(|t| textures.get(t)) else {
            return Ok(None);
        };
        let images = list(&self.json, "images");
        let Some(im) = int(tex, "source").and_then(|s| usize::try_from(s).ok()).and_then(|s| images.get(s)) else {
            return Ok(None);
        };
        if let Some(view) = int(im, "bufferView") {
            return Ok(Some(self.view(view)?.0.to_vec()));
        }
        let uri = im.get("uri").and_then(Value::as_str);
        match uri {
            Some(u) if u.starts_with("data:") => match data_uri(u) {
                Some(d) => Ok(Some(d)),
                None => err("Could not read an image in the model: its data is not valid base64"),
            },
            _ => err(format!(
                "The model refers to a separate image ({}). Export it as a single .glb file.",
                uri.unwrap_or("None")
            )),
        }
    }

    /// A material's colour factor, its texture index and its alpha mode. A material the file lacks has none of them.
    pub fn material(&self, index: i64) -> (Option<Vec<f64>>, Option<i64>, Option<&str>) {
        let Some(mat) = usize::try_from(index).ok().and_then(|i| list(&self.json, "materials").get(i)) else {
            return (None, None, None);
        };
        let pbr = mat.get("pbrMetallicRoughness").filter(|p| p.is_object());
        let factor = pbr.and_then(|p| floats(p, "baseColorFactor"));
        let texture = pbr.and_then(|p| p.get("baseColorTexture")).filter(|t| t.is_object()).map(|t| int(t, "index"));
        // A texture entry without an index is an index of None in Python, which is no texture
        (factor, texture.flatten(), mat.get("alphaMode").and_then(Value::as_str))
    }

    /// The wrap modes of a texture's sampler.
    pub fn wrap(&self, texture: Option<i64>) -> Result<(i64, i64), ModelError> {
        let Some(t) = texture else { return Ok((REPEAT, REPEAT)) };
        let tex = self.item("textures", t)?;
        let Some(s) = int(tex, "sampler") else { return Ok((REPEAT, REPEAT)) };
        let s = self.item("samplers", s)?;
        let get = |key| int(s, key).filter(|w| *w != 0).unwrap_or(REPEAT);
        Ok((get("wrapS"), get("wrapT")))
    }

    /// Every triangle in the default scene, in world space. The count is the number the file has; when it is more
    /// than the importer takes, the lists stop growing.
    pub fn triangles(&self) -> Result<(Triangles, usize), ModelError> {
        let nodes = list(&self.json, "nodes");
        let scenes = list(&self.json, "scenes");
        let roots: Vec<i64> = if scenes.is_empty() {
            (0..nodes.len() as i64).collect()
        } else {
            let scene = self.item("scenes", int(&self.json, "scene").unwrap_or(0))?;
            list(scene, "nodes").iter().map(|n| n.as_i64().unwrap_or(-1)).collect()
        };
        let mut walk = Walk { g: self, out: Triangles::default(), count: 0, found: false, visits: 0 };
        for r in roots {
            walk.node(r, &IDENTITY, 0)?;
        }
        if !walk.found {
            return err("The model has no triangles");
        }
        Ok((walk.out, walk.count))
    }
}

type Mat4 = [[f64; 4]; 4];
const IDENTITY: Mat4 = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];

fn node_matrix(n: &Value) -> Result<Mat4, ModelError> {
    let bad = || err("A node of the model has a transform of the wrong size");
    if let Some(m) = floats(n, "matrix") {
        if m.len() != 16 {
            return bad();
        }
        // glTF stores columns first
        let mut out = IDENTITY;
        for (i, row) in out.iter_mut().enumerate() {
            for (j, v) in row.iter_mut().enumerate() {
                *v = m[j * 4 + i];
            }
        }
        return Ok(out);
    }
    let t = floats(n, "translation").unwrap_or(vec![0.0; 3]);
    let q = floats(n, "rotation").unwrap_or(vec![0.0, 0.0, 0.0, 1.0]);
    let s = floats(n, "scale").unwrap_or(vec![1.0; 3]);
    if t.len() != 3 || q.len() != 4 || s.len() != 3 {
        return bad();
    }
    let (x, y, z, w) = (q[0], q[1], q[2], q[3]);
    let r = [
        [1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y - z * w), 2.0 * (x * z + y * w)],
        [2.0 * (x * y + z * w), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - x * w)],
        [2.0 * (x * z - y * w), 2.0 * (y * z + x * w), 1.0 - 2.0 * (x * x + y * y)],
    ];
    let mut m = IDENTITY;
    for i in 0..3 {
        for j in 0..3 {
            m[i][j] = r[i][j] * s[j];
        }
        m[i][3] = t[i];
    }
    Ok(m)
}

fn mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut out = [[0.0; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            out[i][j] = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j] + a[i][3] * b[3][j];
        }
    }
    out
}

type Mat3 = [[f64; 3]; 3];

fn det3(m: &Mat3) -> f64 {
    m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
}

/// The inverse, by the steps LAPACK takes (`getrf`, then `getri`), which is what `numpy.linalg.inv` runs.
fn inverse(m: &Mat3) -> Option<Mat3> {
    let mut a = *m;
    let mut piv = [0usize; 3];
    for j in 0..3 {
        let mut p = j;
        for i in j + 1..3 {
            if a[i][j].abs() > a[p][j].abs() {
                p = i;
            }
        }
        piv[j] = p;
        if a[p][j] == 0.0 || !a[p][j].is_finite() {
            return None;
        }
        a.swap(j, p);
        let r = 1.0 / a[j][j];
        for i in j + 1..3 {
            a[i][j] *= r;
        }
        for i in j + 1..3 {
            for k in j + 1..3 {
                a[i][k] -= a[i][j] * a[j][k];
            }
        }
    }
    // The inverse of the upper triangle, in place
    for j in 0..3 {
        a[j][j] = 1.0 / a[j][j];
        let ajj = -a[j][j];
        for k in 0..j {
            let temp = a[k][j];
            if temp != 0.0 {
                for i in 0..k {
                    a[i][j] += temp * a[i][k];
                }
                a[k][j] = temp * a[k][k];
            }
        }
        for i in 0..j {
            a[i][j] *= ajj;
        }
    }
    // Solve inverse * L = inverse(U)
    for j in (0..3).rev() {
        let mut work = [0.0; 3];
        for i in j + 1..3 {
            work[i] = a[i][j];
            a[i][j] = 0.0;
        }
        for k in j + 1..3 {
            let temp = -work[k];
            for row in a.iter_mut() {
                row[j] += temp * row[k];
            }
        }
    }
    for j in (0..2).rev() {
        if piv[j] != j {
            for row in a.iter_mut() {
                row.swap(j, piv[j]);
            }
        }
    }
    Some(a)
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// A vector divided by its length, as numpy does it: the length is never taken as less than 1e-9.
fn unit(v: [f64; 3]) -> [f64; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-9);
    // numpy.maximum gives NaN when the length is NaN
    let len = if v.iter().any(|c| c.is_nan()) { f64::NAN } else { len };
    [v[0] / len, v[1] / len, v[2] / len]
}

/// The area vector's length test of the converter uses this too.
pub fn cross_length(p: &[[f64; 3]; 3]) -> f64 {
    let c = cross(sub(p[1], p[0]), sub(p[2], p[0]));
    (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt()
}

struct Walk<'a> {
    g: &'a Gltf,
    out: Triangles,
    count: usize,
    found: bool,
    visits: usize,
}

impl Walk<'_> {
    fn node(&mut self, index: i64, parent: &Mat4, depth: u32) -> Result<(), ModelError> {
        if depth > 64 {
            return err("The model's node tree is too deep");
        }
        self.visits += 1;
        if self.visits > MAX_VISITS {
            return err("The model has too many nodes");
        }
        let n = self.g.item("nodes", index)?;
        let m = mul(parent, &node_matrix(n)?);
        if let Some(mesh) = int(n, "mesh") {
            let mesh = self.g.item("meshes", mesh)?;
            for prim in list(mesh, "primitives") {
                if int(prim, "mode").unwrap_or(4) != 4 {
                    continue;
                }
                self.primitive(prim, &m)?;
            }
        }
        for c in list(n, "children") {
            self.node(c.as_i64().unwrap_or(-1), &m, depth + 1)?;
        }
        Ok(())
    }

    fn primitive(&mut self, prim: &Value, m: &Mat4) -> Result<(), ModelError> {
        let g = self.g;
        let attrs = prim.get("attributes").unwrap_or(&Value::Null);
        let Some(position) = int(attrs, "POSITION") else {
            return err("A mesh of the model has no positions");
        };
        let (pos, width) = g.accessor(position)?;
        if width != 3 {
            return err("A mesh of the model has positions that are not 3D");
        }
        let verts = pos.len() / 3;
        let mut idx: Vec<i64> = match int(prim, "indices") {
            Some(i) => {
                let (data, width) = g.accessor(i)?;
                let mut out = Vec::with_capacity(data.len() / width);
                for v in data.iter().step_by(width) {
                    if !v.is_finite() {
                        return err("A mesh has indices past its vertices");
                    }
                    out.push(*v as i64);
                }
                out
            }
            None => (0..verts as i64).collect(),
        };
        idx.truncate(idx.len() / 3 * 3);
        if idx.iter().any(|i| *i < 0 || *i >= verts as i64) {
            return err("A mesh has indices past its vertices");
        }
        let m3: Mat3 = [[m[0][0], m[0][1], m[0][2]], [m[1][0], m[1][1], m[1][2]], [m[2][0], m[2][1], m[2][2]]];
        let flip = det3(&m3) < 0.0;
        let wp: Vec<[f64; 3]> = pos
            .chunks_exact(3)
            .map(|p| {
                let row = |r: usize| p[0] * m3[r][0] + p[1] * m3[r][1] + p[2] * m3[r][2] + m[r][3];
                [row(0), row(1), row(2)]
            })
            .collect();
        let normals: Option<Vec<[f64; 3]>> = match int(attrs, "NORMAL") {
            Some(a) => {
                let (data, width) = g.accessor(a)?;
                if width != 3 {
                    return err("A mesh of the model has normals that are not 3D");
                }
                let Some(inv) = inverse(&m3) else {
                    return err("A node of the model has a transform that flattens it");
                };
                Some(
                    data.chunks_exact(3)
                        .map(|n| {
                            let col = |c: usize| n[0] * inv[0][c] + n[1] * inv[1][c] + n[2] * inv[2][c];
                            unit([col(0), col(1), col(2)])
                        })
                        .collect(),
                )
            }
            None => None,
        };
        let uvs: Option<(Vec<f64>, usize)> = match int(attrs, "TEXCOORD_0") {
            Some(a) => {
                let (data, width) = g.accessor(a)?;
                if width < 2 {
                    return err("A mesh of the model has texture coordinates that are not 2D");
                }
                Some((data, width))
            }
            None => None,
        };
        let short = |len: usize| idx.iter().any(|i| *i as usize >= len);
        if normals.as_ref().is_some_and(|n| short(n.len())) || uvs.as_ref().is_some_and(|(d, w)| short(d.len() / w)) {
            return err("A mesh of the model has fewer normals or texture coordinates than vertices");
        }
        let material = match prim.get("material") {
            None | Some(Value::Null) => -1,
            Some(v) => match v.as_i64() {
                Some(i) => i,
                None => return err("A mesh of the model has a bad material index"),
            },
        };
        self.found = true;
        for tri in idx.chunks_exact(3) {
            self.count += 1;
            if self.count > MAX_INPUT_TRIANGLES {
                continue;
            }
            let t = if flip { [tri[2], tri[1], tri[0]] } else { [tri[0], tri[1], tri[2]] }.map(|i| i as usize);
            let p = t.map(|i| wp[i]);
            let n = match &normals {
                Some(n) => t.map(|i| n[i]),
                None => [unit(cross(sub(p[1], p[0]), sub(p[2], p[0]))); 3],
            };
            let uv = match &uvs {
                Some((d, w)) => t.map(|i| [d[i * w], d[i * w + 1]]),
                None => [[0.0; 2]; 3],
            };
            self.out.p.push(p);
            self.out.n.push(n);
            self.out.uv.push(uv);
            self.out.m.push(material);
        }
        Ok(())
    }
}
