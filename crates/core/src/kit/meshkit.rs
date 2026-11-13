//! The kit's mesh builder: a triangle soup with textures, the shapes the models are made of, noise textures, and
//! the binary glTF writer.
//!
//! Units are game units and y is up. The same settings must give the same model bit for bit, and the reference
//! tests hold the expected models. Every sum is written in a fixed order. The products of 3 by 3 matrices and the
//! dot products use fused multiply-add, as the reference results were computed, and start each sum from +0.0,
//! which turns a -0.0 into 0.0. `dot3`, `mat_mul` and `vec_mat_t` keep both.

use super::font;
use super::image::{Image, Rgba};
use super::rng::Random;
use crate::pyfmt;

pub type V3 = [f64; 3];
pub type Uv = [f64; 2];

/// How much lighter pictures are made, because the game lights models dark.
pub const BRIGHT: f64 = 1.4;
const PI: f64 = std::f64::consts::PI;

pub fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
pub fn mul(a: V3, k: f64) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}
pub fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
/// The dot product of two vectors, with fused multiply-add.
pub fn dot3(a: V3, b: V3) -> f64 {
    a[2].mul_add(b[2], a[1].mul_add(b[1], a[0].mul_add(b[0], 0.0)))
}
/// The length of a vector.
pub fn len(a: V3) -> f64 {
    dot3(a, a).sqrt()
}
/// The vector with length 1. A vector shorter than 1e-12 stays as it is.
pub fn norm(v: V3) -> V3 {
    let n = len(v);
    if n > 1e-12 { [v[0] / n, v[1] / n, v[2] / n] } else { v }
}
/// The product `a b` of two 3 by 3 matrices.
pub fn mat_mul(a: &[V3; 3], b: &[V3; 3]) -> [V3; 3] {
    let mut c = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            c[i][j] = a[i][2].mul_add(b[2][j], a[i][1].mul_add(b[1][j], a[i][0].mul_add(b[0][j], 0.0)));
        }
    }
    c
}
/// Three row vectors, each multiplied by the transpose of `m`.
pub fn rows_mat_t(rows: &[V3; 3], m: &[V3; 3]) -> [V3; 3] {
    let mut c = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            c[i][j] = dot3(rows[i], m[j]);
        }
    }
    c
}
/// One row vector multiplied by the transpose of `m`. The order of the sum differs from the matrix product's.
pub fn vec_mat_t(v: V3, m: &[V3; 3]) -> V3 {
    let mut c = [0.0; 3];
    for j in 0..3 {
        c[j] = v[2].mul_add(m[j][2], v[0].mul_add(m[j][0], v[1].mul_add(m[j][1], 0.0)));
    }
    c
}

/// One triangle: a position, a normal and a texture coordinate at each corner, and its texture.
#[derive(Clone, Debug, PartialEq)]
pub struct Tri {
    pub p: [V3; 3],
    pub n: [V3; 3],
    pub uv: [Uv; 3],
    pub mat: usize,
}

/// A list of triangles with their textures.
#[derive(Clone, Debug, Default)]
pub struct Soup {
    pub tris: Vec<Tri>,
    /// The textures, in order of creation.
    pub mats: Vec<Image>,
    /// Per texture: true when it must not repeat.
    pub clamp: Vec<bool>,
}

/// How `Soup::add` places another soup.
#[derive(Clone, Copy)]
pub struct Place {
    pub offset: V3,
    pub yaw: f64,
    pub scale: f64,
    pub tilt: (f64, f64),
}

impl Default for Place {
    fn default() -> Place {
        Place { offset: [0.0; 3], yaw: 0.0, scale: 1.0, tilt: (0.0, 0.0) }
    }
}

impl Soup {
    pub fn new() -> Soup {
        Soup::default()
    }

    /// A texture, made `bright` times lighter and clipped at white. Returns its number.
    pub fn material(&mut self, image: Image, clamp: bool, bright: f64) -> usize {
        let mut im = image;
        if bright != 1.0 {
            for p in im.px.iter_mut() {
                for c in p.iter_mut().take(3) {
                    *c = (*c as f64 * bright).clamp(0.0, 255.0) as u8;
                }
            }
        }
        self.mats.push(im);
        self.clamp.push(clamp);
        self.mats.len() - 1
    }

    /// A triangle. A corner without a normal gets the face's. A triangle without area is dropped.
    pub fn tri(&mut self, p: [V3; 3], uv: [Uv; 3], mat: usize, n: [Option<V3>; 3]) {
        let f = cross(sub(p[1], p[0]), sub(p[2], p[0]));
        if len(f) < 1e-9 {
            return;
        }
        let f = norm(f);
        let n = [n[0].map_or(f, norm), n[1].map_or(f, norm), n[2].map_or(f, norm)];
        self.tris.push(Tri { p, n, uv, mat });
    }

    /// Four corners, counter-clockwise seen from outside.
    pub fn quad(&mut self, p: [V3; 4], uv: [Uv; 4], mat: usize, n: [Option<V3>; 4]) {
        self.tri([p[0], p[1], p[2]], [uv[0], uv[1], uv[2]], mat, [n[0], n[1], n[2]]);
        self.tri([p[0], p[2], p[3]], [uv[0], uv[2], uv[3]], mat, [n[0], n[2], n[3]]);
    }

    /// Adds another soup: tilted about x then z, turned about y, scaled and moved. With `shared` the other soup was
    /// built with this soup's textures. Without it, its textures join this soup's.
    pub fn add(&mut self, other: &Soup, place: Place, shared: bool) {
        let base = if shared { 0 } else { self.mats.len() };
        if !shared {
            self.mats.extend(other.mats.iter().cloned());
            self.clamp.extend(other.clamp.iter().cloned());
        }
        let (cy, sy) = (place.yaw.cos(), place.yaw.sin());
        let ry = [[cy, 0.0, sy], [0.0, 1.0, 0.0], [-sy, 0.0, cy]];
        let (cx, sx) = (place.tilt.0.cos(), place.tilt.0.sin());
        let rx = [[1.0, 0.0, 0.0], [0.0, cx, -sx], [0.0, sx, cx]];
        let (cz, sz) = (place.tilt.1.cos(), place.tilt.1.sin());
        let rz = [[cz, -sz, 0.0], [sz, cz, 0.0], [0.0, 0.0, 1.0]];
        let rot = mat_mul(&mat_mul(&ry, &rz), &rx);
        for t in &other.tris {
            let scaled = [mul(t.p[0], place.scale), mul(t.p[1], place.scale), mul(t.p[2], place.scale)];
            let mut p = rows_mat_t(&scaled, &rot);
            for row in p.iter_mut() {
                *row = add(*row, place.offset);
            }
            self.tris.push(Tri { p, n: rows_mat_t(&t.n, &rot), uv: t.uv, mat: t.mat + base });
        }
    }

    /// The number of triangles.
    pub fn count(&self) -> usize {
        self.tris.len()
    }

    /// The lowest and the highest corner. Zeros for an empty soup.
    pub fn bounds(&self) -> (V3, V3) {
        let mut it = self.tris.iter().flat_map(|t| t.p.iter());
        let Some(first) = it.next() else { return ([0.0; 3], [0.0; 3]) };
        let (mut lo, mut hi) = (*first, *first);
        for p in it {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        (lo, hi)
    }

    /// The soup as a binary glTF file: one mesh with one primitive per texture in use, each with its picture as PNG.
    pub fn glb(&self) -> Vec<u8> {
        let mut blob: Vec<u8> = Vec::new();
        let mut views: Vec<String> = Vec::new();
        let mut accessors: Vec<String> = Vec::new();
        let (mut images, mut materials, mut samplers, mut textures, mut prims) = (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let mut view = |blob: &mut Vec<u8>, data: &[u8], target: Option<u32>| -> usize {
            while blob.len() % 4 != 0 {
                blob.push(0);
            }
            let t = target.map_or(String::new(), |t| format!(",\"target\":{t}"));
            views.push(format!("{{\"buffer\":0,\"byteOffset\":{},\"byteLength\":{}{}}}", blob.len(), data.len(), t));
            blob.extend_from_slice(data);
            views.len() - 1
        };
        let list = |v: &[f64]| v.iter().map(|x| pyfmt::repr(*x)).collect::<Vec<_>>().join(",");
        let mut used: Vec<usize> = self.tris.iter().map(|t| t.mat).collect();
        used.sort_unstable();
        used.dedup();
        for (k, &m) in used.iter().enumerate() {
            let sel: Vec<&Tri> = self.tris.iter().filter(|t| t.mat == m).collect();
            let count = sel.len() * 3;
            let png = self.mats.get(m).map(|im| im.png()).unwrap_or_default();
            let iv = view(&mut blob, &png, None);
            images.push(format!("{{\"mimeType\":\"image/png\",\"bufferView\":{iv}}}"));
            let wrap = if self.clamp.get(m).copied().unwrap_or(false) { 33071 } else { 10497 };
            samplers.push(format!("{{\"wrapS\":{wrap},\"wrapT\":{wrap}}}"));
            textures.push(format!("{{\"sampler\":{k},\"source\":{k}}}"));
            materials.push(format!(
                "{{\"pbrMetallicRoughness\":{{\"baseColorFactor\":[1.0,1.0,1.0,1.0],\"metallicFactor\":1.0,\"roughnessFactor\":1.0,\"baseColorTexture\":{{\"index\":{k},\"texCoord\":0}}}},\"emissiveFactor\":[0.0,0.0,0.0],\"alphaMode\":\"OPAQUE\",\"doubleSided\":false}}"
            ));
            let first = accessors.len();
            for which in 0..2 {
                let mut data = Vec::with_capacity(count * 12);
                let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
                for t in &sel {
                    for v in if which == 0 { &t.p } else { &t.n } {
                        for c in 0..3 {
                            data.extend_from_slice(&(v[c] as f32).to_le_bytes());
                            lo[c] = lo[c].min(v[c]);
                            hi[c] = hi[c].max(v[c]);
                        }
                    }
                }
                let bv = view(&mut blob, &data, Some(34962));
                accessors.push(format!(
                    "{{\"bufferView\":{bv},\"byteOffset\":0,\"componentType\":5126,\"normalized\":false,\"count\":{count},\"type\":\"VEC3\",\"max\":[{}],\"min\":[{}]}}",
                    list(&hi),
                    list(&lo)
                ));
            }
            let mut data = Vec::with_capacity(count * 8);
            for t in &sel {
                for uv in &t.uv {
                    data.extend_from_slice(&(uv[0] as f32).to_le_bytes());
                    data.extend_from_slice(&(uv[1] as f32).to_le_bytes());
                }
            }
            let bv = view(&mut blob, &data, Some(34962));
            accessors.push(format!("{{\"bufferView\":{bv},\"byteOffset\":0,\"componentType\":5126,\"normalized\":false,\"count\":{count},\"type\":\"VEC2\"}}"));
            let data: Vec<u8> = (0..count as u32).flat_map(|i| i.to_le_bytes()).collect();
            let bv = view(&mut blob, &data, Some(34963));
            accessors.push(format!("{{\"bufferView\":{bv},\"byteOffset\":0,\"componentType\":5125,\"normalized\":false,\"count\":{count},\"type\":\"SCALAR\"}}"));
            prims.push(format!(
                "{{\"attributes\":{{\"POSITION\":{},\"NORMAL\":{},\"TEXCOORD_0\":{}}},\"indices\":{},\"mode\":4,\"material\":{k}}}",
                first,
                first + 1,
                first + 2,
                first + 3
            ));
        }
        let mut json = format!(
            "{{\"accessors\":[{}],\"asset\":{{\"generator\":\"Glovebox kit\",\"version\":\"2.0\"}},\"bufferViews\":[{}],\"buffers\":[{{\"byteLength\":{}}}],\"images\":[{}],\"materials\":[{}],\"meshes\":[{{\"primitives\":[{}]}}],\"nodes\":[{{\"mesh\":0}}],\"samplers\":[{}],\"scene\":0,\"scenes\":[{{\"nodes\":[0]}}],\"textures\":[{}]}}",
            accessors.join(","),
            views.join(","),
            blob.len(),
            images.join(","),
            materials.join(","),
            prims.join(","),
            samplers.join(","),
            textures.join(",")
        )
        .into_bytes();
        while json.len() % 4 != 0 {
            json.push(b' ');
        }
        let mut bin = blob;
        while bin.len() % 4 != 0 {
            bin.push(0);
        }
        let total = 12 + 8 + json.len() + 8 + bin.len();
        let mut out = Vec::with_capacity(total);
        out.extend_from_slice(b"glTF");
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&(total as u32).to_le_bytes());
        out.extend_from_slice(&(json.len() as u32).to_le_bytes());
        out.extend_from_slice(b"JSON");
        out.extend_from_slice(&json);
        out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        out.extend_from_slice(b"BIN\0");
        out.extend_from_slice(&bin);
        out
    }
}

pub fn v3(x: f64, y: f64, z: f64) -> V3 {
    [x, y, z]
}

// ---------------------------------------------------------------------------------------------
// Shapes

/// Options of `lathe`.
#[derive(Clone, Copy)]
pub struct Lathe {
    pub v0: f64,
    pub v1: f64,
    pub u_repeat: f64,
    pub smooth: bool,
    pub phase: f64,
}

impl Default for Lathe {
    fn default() -> Lathe {
        Lathe { v0: 0.0, v1: 1.0, u_repeat: 1.0, smooth: true, phase: 0.0 }
    }
}

/// A surface of revolution about y. `profile` is (radius, y) from the bottom up. The first and last radius may be 0.
pub fn lathe(s: &mut Soup, profile: &[(f64, f64)], segs: usize, mat: usize, o: Lathe) {
    let n = profile.len();
    for i in 0..n.saturating_sub(1) {
        let ((r0, y0), (r1, y1)) = (profile[i], profile[i + 1]);
        let va = o.v0 + (o.v1 - o.v0) * i as f64 / (n - 1) as f64;
        let vb = o.v0 + (o.v1 - o.v0) * (i + 1) as f64 / (n - 1) as f64;
        let (dr, dy) = (r1 - r0, y1 - y0);
        let sn = if dr != 0.0 || dy != 0.0 { norm(v3(dy, -dr, 0.0)) } else { v3(1.0, 0.0, 0.0) };
        for k in 0..segs {
            let a0 = o.phase + 2.0 * PI * k as f64 / segs as f64;
            let a1 = o.phase + 2.0 * PI * (k + 1) as f64 / segs as f64;
            let (ua, ub) = (o.u_repeat * k as f64 / segs as f64, o.u_repeat * (k + 1) as f64 / segs as f64);
            let pt = |r: f64, y: f64, a: f64| v3(r * a.cos(), y, r * a.sin());
            let nm = |a: f64| if o.smooth { Some(v3(sn[0] * a.cos(), sn[1], sn[0] * a.sin())) } else { None };
            let (p00, p10) = (pt(r0, y0, a0), pt(r0, y0, a1));
            let (p01, p11) = (pt(r1, y1, a0), pt(r1, y1, a1));
            if r0 > 1e-9 && r1 > 1e-9 {
                s.quad([p00, p01, p11, p10], [[ua, va], [ua, vb], [ub, vb], [ub, va]], mat, [nm(a0), nm(a0), nm(a1), nm(a1)]);
            } else if r0 <= 1e-9 {
                s.tri([p01, p11, p00], [[ua, vb], [ub, vb], [ua, va]], mat, [nm(a0), nm(a1), nm(a0)]);
            } else {
                s.tri([p00, p01, p10], [[ua, va], [ua, vb], [ub, va]], mat, [nm(a0), nm(a0), nm(a1)]);
            }
        }
    }
}

/// A flat disc that faces up, with the picture's middle at its middle.
pub fn disc(s: &mut Soup, centre: V3, radius: f64, segs: usize, mat: usize) {
    let [cx, cy, cz] = centre;
    let (uvc, uvr) = ([0.5, 0.5], 0.5);
    for k in 0..segs {
        let (a0, a1) = (2.0 * PI * k as f64 / segs as f64, 2.0 * PI * (k + 1) as f64 / segs as f64);
        let p0 = v3(cx + radius * a0.cos(), cy, cz + radius * a0.sin());
        let p1 = v3(cx + radius * a1.cos(), cy, cz + radius * a1.sin());
        let u0 = [uvc[0] + uvr * a0.cos(), uvc[1] + uvr * a0.sin()];
        let u1 = [uvc[0] + uvr * a1.cos(), uvc[1] + uvr * a1.sin()];
        s.tri([v3(cx, cy, cz), p1, p0], [uvc, u1, u0], mat, [None; 3]);
    }
}

/// An axis-aligned box. `mats` are the textures of +x, -x, +y, -y, +z, -z; None leaves a face out. Each face gets
/// the whole picture. The texture `upright`, if any, is turned to stand upright on a side face.
pub fn box_(s: &mut Soup, lo: V3, hi: V3, mats: [Option<usize>; 6], upright: Option<usize>) {
    let [x0, y0, z0] = lo;
    let [x1, y1, z1] = hi;
    let full = [[0.0, 1.0], [0.0, 0.0], [1.0, 0.0], [1.0, 1.0]];
    let up = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
    let faces = [
        [[x1, y0, z1], [x1, y0, z0], [x1, y1, z0], [x1, y1, z1]],
        [[x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0]],
        [[x0, y1, z1], [x1, y1, z1], [x1, y1, z0], [x0, y1, z0]],
        [[x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]],
        [[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]],
        [[x1, y0, z0], [x0, y0, z0], [x0, y1, z0], [x1, y1, z0]],
    ];
    for (f, m) in faces.iter().zip(mats) {
        if let Some(m) = m {
            s.quad(*f, if upright == Some(m) { up } else { full }, m, [None; 4]);
        }
    }
}

/// A box with one texture on all faces.
pub fn box1(s: &mut Soup, lo: V3, hi: V3, mat: usize) {
    box_(s, lo, hi, [Some(mat); 6], None);
}

/// Options of `tube`.
#[derive(Clone, Copy)]
pub struct Tube {
    pub v_scale: f64,
    pub cap_end: bool,
    pub cap_start: bool,
    pub u_repeat: f64,
}

impl Default for Tube {
    fn default() -> Tube {
        Tube { v_scale: 1.0, cap_end: true, cap_start: false, u_repeat: 1.0 }
    }
}

/// A tube along a line of points, with one radius per point.
pub fn tube(s: &mut Soup, pts: &[V3], radii: &[f64], sides: usize, mat: usize, o: Tube) {
    let n = pts.len();
    if n == 0 || radii.len() < n || sides == 0 {
        return;
    }
    let up = v3(0.0, 1.0, 0.0);
    let mut rings: Vec<(Vec<V3>, Vec<V3>, V3, V3)> = Vec::new();
    for (i, &p) in pts.iter().enumerate() {
        let t = norm(sub(pts[(i + 1).min(n - 1)], pts[i.saturating_sub(1)]));
        let a = if t[1].abs() < 0.95 { norm(cross(t, up)) } else { v3(1.0, 0.0, 0.0) };
        let b = cross(t, a);
        let ring: Vec<V3> = (0..sides)
            .map(|k| {
                let ang = 2.0 * PI * k as f64 / sides as f64;
                add(p, mul(add(mul(a, ang.cos()), mul(b, ang.sin())), radii[i]))
            })
            .collect();
        let nrm: Vec<V3> = ring.iter().map(|r| norm(sub(*r, p))).collect();
        rings.push((ring, nrm, p, t));
    }
    let mut length = 0.0;
    let mut vs = vec![0.0];
    for i in 1..n {
        length += len(sub(pts[i], pts[i - 1]));
        vs.push(length);
    }
    for i in 0..n - 1 {
        let (r0, n0) = (&rings[i].0, &rings[i].1);
        let (r1, n1) = (&rings[i + 1].0, &rings[i + 1].1);
        for k in 0..sides {
            let k2 = (k + 1) % sides;
            let (ua, ub) = (o.u_repeat * k as f64 / sides as f64, o.u_repeat * (k + 1) as f64 / sides as f64);
            let (va, vb) = (vs[i] * o.v_scale / length.max(1e-9), vs[i + 1] * o.v_scale / length.max(1e-9));
            s.quad(
                [r0[k], r0[k2], r1[k2], r1[k]],
                [[ua, va], [ub, va], [ub, vb], [ua, vb]],
                mat,
                [Some(n0[k]), Some(n0[k2]), Some(n1[k2]), Some(n1[k])],
            );
        }
    }
    let mid = [[0.5, 0.5]; 3];
    if o.cap_end {
        let (ring, _, p, t) = &rings[n - 1];
        for k in 0..sides {
            s.tri([add(*p, mul(*t, 0.001)), ring[k], ring[(k + 1) % sides]], mid, mat, [None; 3]);
        }
    }
    if o.cap_start {
        let (ring, _, p, _) = &rings[0];
        for k in 0..sides {
            s.tri([*p, ring[(k + 1) % sides], ring[k]], mid, mat, [None; 3]);
        }
    }
}

/// The front half (+z) of an ellipsoid at the origin, its picture projected from the front, closed by a flat back.
pub fn dome(s: &mut Soup, radius: f64, rings: usize, segs: usize, mat: usize, squash: V3, back: usize) {
    let [sx, sy, sz] = squash;
    let pt = |t: f64, a: f64| {
        let x = t.sin() * a.cos();
        let y = t.sin() * a.sin();
        let z = t.cos();
        (v3(radius * sx * x, radius * sy * y, radius * sz * z), norm(v3(x / sx, y / sy, z / sz)), [0.5 + 0.5 * x, 0.5 - 0.5 * y])
    };
    for i in 0..rings {
        let (t0, t1) = ((PI / 2.0) * i as f64 / rings as f64, (PI / 2.0) * (i + 1) as f64 / rings as f64);
        for k in 0..segs {
            let (a0, a1) = (2.0 * PI * k as f64 / segs as f64, 2.0 * PI * (k + 1) as f64 / segs as f64);
            let ((p00, n00, u00), (p10, n10, u10)) = (pt(t0, a0), pt(t0, a1));
            let ((p01, n01, u01), (p11, n11, u11)) = (pt(t1, a0), pt(t1, a1));
            if i == 0 {
                s.tri([p00, p01, p11], [u00, u01, u11], mat, [Some(n00), Some(n01), Some(n11)]);
            } else {
                s.quad([p00, p01, p11, p10], [u00, u01, u11, u10], mat, [Some(n00), Some(n01), Some(n11), Some(n10)]);
            }
        }
    }
    let bn = Some(v3(0.0, 0.0, -1.0));
    for k in 0..segs {
        let (a0, a1) = (2.0 * PI * k as f64 / segs as f64, 2.0 * PI * (k + 1) as f64 / segs as f64);
        let p0 = v3(radius * sx * a0.cos(), radius * sy * a0.sin(), 0.0);
        let p1 = v3(radius * sx * a1.cos(), radius * sy * a1.sin(), 0.0);
        s.tri([v3(0.0, 0.0, 0.0), p1, p0], [[0.5, 0.5]; 3], back, [bn; 3]);
    }
}

/// A surface from a function of u and v in 0 to 1, where u wraps round. Normals come from the surface's slopes.
/// `flip` reverses the winding.
pub fn param(s: &mut Soup, f: &dyn Fn(f64, f64) -> V3, nu: usize, nv: usize, mat: usize, uv: &dyn Fn(f64, f64) -> Uv, flip: bool) {
    param_cols(s, f, nu, nv, &|_, u, v| (mat, uv(u, v)), flip);
}

/// `param` with a texture per column. A column is the strip between two steps of u, numbered from 0. `skin` takes
/// the column, u and v, and gives the column's texture and the texture coordinate of that corner. Two columns can
/// give the corner they share different coordinates.
pub fn param_cols(s: &mut Soup, f: &dyn Fn(f64, f64) -> V3, nu: usize, nv: usize, skin: &dyn Fn(usize, f64, f64) -> (usize, Uv), flip: bool) {
    let eps = 1e-3;
    let mut grid: Vec<Vec<(V3, V3, Uv)>> = Vec::new();
    for i in 0..=nu {
        let mut col = Vec::new();
        for j in 0..=nv {
            let (u, v) = (i as f64 / nu as f64, j as f64 / nv as f64);
            let p = f(u, v);
            let ua = if u + eps <= 1.0 + eps { u + eps } else { 1.0 + eps };
            let ub = if u - eps >= -eps { u - eps } else { -eps };
            let du = sub(f(ua, v), f(ub, v));
            let va = if v + eps <= 1.0 { v + eps } else { 1.0 };
            let vb = if v - eps >= 0.0 { v - eps } else { 0.0 };
            let dv = sub(f(u, va), f(u, vb));
            let mut n = cross(du, dv);
            if len(n) < 1e-12 {
                n = [0.0; 3];
            }
            col.push((p, n, [u, v]));
        }
        grid.push(col);
    }
    // A pole has no slope. It gets the sum of the normals of the ring next to it.
    for j in 0..=nv {
        for i in 0..=nu {
            if len(grid[i][j].1) < 1e-12 {
                let jj = if j == 0 { 1.min(nv) } else { j - 1 };
                let mut avg = [0.0; 3];
                for col in grid.iter() {
                    avg = add(avg, col[jj].1);
                }
                grid[i][j].1 = avg;
            }
        }
    }
    let sgn = if flip { -1.0 } else { 1.0 };
    for i in 0..nu {
        for j in 0..nv {
            let (a, b, c, d) = (grid[i][j], grid[i + 1][j], grid[i + 1][j + 1], grid[i][j + 1]);
            let q = if flip { [a, d, c, b] } else { [a, b, c, d] };
            let na: Vec<Option<V3>> = q.iter().map(|x| if len(x.1) > 0.0 { Some(mul(norm(x.1), sgn)) } else { None }).collect();
            let mat = skin(i, q[0].2[0], q[0].2[1]).0;
            let t = [0, 1, 2, 3].map(|k| skin(i, q[k].2[0], q[k].2[1]).1);
            s.tri([q[0].0, q[1].0, q[2].0], [t[0], t[1], t[2]], mat, [na[0], na[1], na[2]]);
            s.tri([q[0].0, q[2].0, q[3].0], [t[0], t[2], t[3]], mat, [na[0], na[2], na[3]]);
        }
    }
}

/// A convex polygon of (x, z) points, counter-clockwise seen from above, pulled up from y0 to y1. The top is a fan
/// with texture coordinates from `top_uv`, or from the default when None. `bottom` closes the underside.
pub fn prism(s: &mut Soup, poly: &[(f64, f64)], y0: f64, y1: f64, side: usize, top: usize, bottom: Option<usize>, top_uv: Option<&dyn Fn(f64, f64) -> Uv>) {
    let n = poly.len();
    if n == 0 {
        return;
    }
    let (u_scale, v_scale) = (1.0, 1.0);
    for i in 0..n {
        let ((x0, z0), (x1, z1)) = (poly[i], poly[(i + 1) % n]);
        let l = super::rng::hypot(x1 - x0, z1 - z0);
        let nrm = Some(norm(v3(z1 - z0, 0.0, -(x1 - x0))));
        s.quad(
            [v3(x0, y0, z0), v3(x0, y1, z0), v3(x1, y1, z1), v3(x1, y0, z1)],
            [[0.0, v_scale], [0.0, 0.0], [u_scale * l / 40.0, 0.0], [u_scale * l / 40.0, v_scale]],
            side,
            [nrm; 4],
        );
    }
    let default_uv = |x: f64, z: f64| [x / 40.0 + 0.5, z / 40.0 + 0.5];
    let tuv: &dyn Fn(f64, f64) -> Uv = match top_uv {
        Some(f) => f,
        None => &default_uv,
    };
    let cx = poly.iter().fold(0.0, |a, p| a + p.0) / n as f64;
    let cz = poly.iter().fold(0.0, |a, p| a + p.1) / n as f64;
    let (upn, dn) = (Some(v3(0.0, 1.0, 0.0)), Some(v3(0.0, -1.0, 0.0)));
    for i in 0..n {
        let ((x0, z0), (x1, z1)) = (poly[i], poly[(i + 1) % n]);
        s.tri([v3(cx, y1, cz), v3(x1, y1, z1), v3(x0, y1, z0)], [tuv(cx, cz), tuv(x1, z1), tuv(x0, z0)], top, [upn; 3]);
        if let Some(b) = bottom {
            s.tri([v3(cx, y0, cz), v3(x0, y0, z0), v3(x1, y0, z1)], [tuv(cx, cz), tuv(x0, z0), tuv(x1, z1)], b, [dn; 3]);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Textures

fn shade(base: [i64; 3], d: i64) -> Rgba {
    [(base[0] + d).clamp(0, 255) as u8, (base[1] + d).clamp(0, 255) as u8, (base[2] + d).clamp(0, 255) as u8, 255]
}

/// A noisy picture of one colour: each texel moves by a random amount from `lo` to `hi`.
pub fn noise(w: usize, h: usize, rnd: &mut Random, lo: i64, hi: i64, base: [i64; 3]) -> Image {
    let mut im = Image::new(w, h, [0; 4]);
    for p in im.px.iter_mut() {
        *p = shade(base, rnd.randint(lo, hi));
    }
    im
}

/// Grey stone, with specks of moss when asked.
pub fn stone(rnd: &mut Random, w: usize, h: usize, base: [i64; 3], moss: bool) -> Image {
    let mut im = noise(w, h, rnd, -18, 14, base);
    if moss {
        for _ in 0..w * h / 40 {
            let (x, y) = (rnd.randrange(0, w as i64), rnd.randrange(0, h as i64));
            let r = 52 + rnd.randint(0, 20);
            let g = 80 + rnd.randint(0, 24);
            im.put(x as i32, y as i32, [r as u8, g as u8, 48, 255]);
        }
    }
    im
}

/// Dark wood with vertical grain.
pub fn wood(rnd: &mut Random, w: usize, h: usize, base: [i64; 3]) -> Image {
    let cols: Vec<i64> = (0..w).map(|_| rnd.randint(-14, 12)).collect();
    let mut im = Image::new(w, h, [0; 4]);
    for y in 0..h {
        for x in 0..w {
            im.px[y * w + x] = shade(base, cols[x] + rnd.randint(-5, 5));
        }
    }
    im
}

/// Lines of text, each centred across the picture.
pub fn text_centered(im: &mut Image, lines: &[String], y0: f64, line_h: f64, colour: Rgba, size: u32) {
    for (i, t) in lines.iter().enumerate() {
        let w = font::text_length(t, size);
        font::draw_text(im, (im.w as f64 - w) / 2.0, y0 + i as f64 * line_h, t, colour, size);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain() -> (Soup, usize) {
        let mut s = Soup::new();
        let m = s.material(Image::new(2, 2, [100, 200, 10, 255]), false, BRIGHT);
        (s, m)
    }

    #[test]
    fn material_is_made_lighter_and_clipped() {
        let (s, _) = plain();
        assert_eq!(s.mats[0].px[0], [140, 255, 14, 255]);
    }

    #[test]
    fn a_box_has_twelve_triangles_that_face_out() {
        let (mut s, m) = plain();
        box1(&mut s, [-1.0, 0.0, -2.0], [1.0, 3.0, 2.0], m);
        assert_eq!(s.count(), 12);
        for t in &s.tris {
            let mid = mul(add(add(t.p[0], t.p[1]), t.p[2]), 1.0 / 3.0);
            assert!(dot3(t.n[0], sub(mid, [0.0, 1.5, 0.0])) > 0.0);
        }
        assert_eq!(s.bounds(), ([-1.0, 0.0, -2.0], [1.0, 3.0, 2.0]));
    }

    #[test]
    fn a_triangle_without_area_is_dropped() {
        let (mut s, m) = plain();
        s.tri([[0.0; 3], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]], [[0.0; 2]; 3], m, [None; 3]);
        assert_eq!(s.count(), 0);
    }

    #[test]
    fn shapes_have_the_expected_counts() {
        let (mut s, m) = plain();
        lathe(&mut s, &[(0.0, 0.0), (2.0, 1.0), (2.0, 3.0), (0.0, 4.0)], 6, m, Lathe::default());
        assert_eq!(s.count(), 6 + 12 + 6);
        let (mut s, m) = plain();
        tube(&mut s, &[[0.0; 3], [0.0, 5.0, 0.0], [1.0, 9.0, 0.0]], &[1.0, 1.0, 0.5], 4, m, Tube::default());
        assert_eq!(s.count(), 2 * 8 + 4);
        let (mut s, m) = plain();
        disc(&mut s, [0.0; 3], 3.0, 7, m);
        assert_eq!(s.count(), 7);
    }

    #[test]
    fn glb_has_its_chunks() {
        let (mut s, m) = plain();
        box1(&mut s, [0.0; 3], [1.0; 3], m);
        let g = s.glb();
        assert_eq!(&g[..4], b"glTF");
        assert_eq!(u32::from_le_bytes([g[8], g[9], g[10], g[11]]) as usize, g.len());
        let n = u32::from_le_bytes([g[12], g[13], g[14], g[15]]) as usize;
        let v: serde_json::Value = serde_json::from_slice(&g[20..20 + n]).unwrap();
        assert_eq!(v["accessors"][0]["count"], 36);
        assert_eq!(v["samplers"][0]["wrapS"], 10497);
        assert_eq!(&g[24 + n..28 + n], b"BIN\0");
    }
}
