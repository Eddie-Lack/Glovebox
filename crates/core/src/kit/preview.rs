//! The small picture of a kit model for the kit list: flat shaded triangles in painter's order, coloured by the
//! texture at each triangle's middle.

use super::image::Image;
use super::meshkit::{cross, len, mat_mul, rows_mat_t, sub, vec_mat_t, Soup};

/// Draws the soup on a square picture of `size` pixels, seen from the front right and a little above.
pub fn render(soup: &Soup, size: usize) -> Image {
    let (yaw, pitch) = (0.6f64, 0.25f64);
    let mut im = Image::new(size, size, [24, 22, 30, 255]);
    if soup.tris.is_empty() {
        return im;
    }
    let (lo, hi) = soup.bounds();
    let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
    let mut r = len(sub(hi, lo)) / 2.0;
    if r == 0.0 {
        r = 1.0;
    }
    let (cy, sy, cp, sp) = (yaw.cos(), yaw.sin(), pitch.cos(), pitch.sin());
    let ry = [[cy, 0.0, sy], [0.0, 1.0, 0.0], [-sy, 0.0, cy]];
    let rx = [[1.0, 0.0, 0.0], [0.0, cp, -sp], [0.0, sp, cp]];
    let rot = mat_mul(&rx, &ry);
    let mut items: Vec<(f64, [[f64; 3]; 3], [u8; 4])> = Vec::new();
    for t in &soup.tris {
        let q = rows_mat_t(&[sub(t.p[0], c), sub(t.p[1], c), sub(t.p[2], c)], &rot);
        let mean = |k: usize| (t.n[0][k] + t.n[1][k] + t.n[2][k]) / 3.0;
        let nn = vec_mat_t([mean(0), mean(1), mean(2)], &rot);
        let fnorm = vec_mat_t(cross(sub(t.p[1], t.p[0]), sub(t.p[2], t.p[0])), &rot);
        if fnorm[2] > 0.0 {
            continue;
        }
        let Some(tex) = soup.mats.get(t.mat) else { continue };
        let u = (t.uv[0][0] + t.uv[1][0] + t.uv[2][0]) / 3.0;
        let v = (t.uv[0][1] + t.uv[1][1] + t.uv[2][1]) / 3.0;
        let cut = |x: f64| {
            let x = if x > 0.0 { x } else { 0.0 };
            if x < 0.999 { x } else { 0.999 }
        };
        let px = tex.get((cut(u) * tex.w as f64) as usize, (cut(v) * tex.h as f64) as usize);
        let mut nl = len(nn);
        if nl == 0.0 {
            nl = 1.0;
        }
        let facing = -nn[2] / nl;
        let light = 0.35 + 0.65 * if facing > 0.0 { facing } else { 0.0 };
        if px[3] < 128 {
            continue;
        }
        let col = |i: usize| (px[i] as f64 * light).min(255.0) as u8;
        items.push(((q[0][2] + q[1][2] + q[2][2]) / 3.0, q, [col(0), col(1), col(2), 255]));
    }
    items.sort_by(|a, b| (-a.0).partial_cmp(&(-b.0)).unwrap_or(std::cmp::Ordering::Equal));
    let s = size as f64;
    for (_, q, col) in items {
        let pts: Vec<(f64, f64)> = q.iter().map(|p| (s / 2.0 + p[0] / r * s * 0.45, s / 2.0 - p[1] / r * s * 0.45)).collect();
        im.polygon(&pts, col);
    }
    im
}
