//! The app's icon: a garib (a gold four-pointed star) on a dark round tile.
//! The window uses the picture, and `build.rs` writes it into the `.exe` as an `.ico`.

pub const SIZES: [u32; 7] = [16, 24, 32, 48, 64, 128, 256];

const GOLD: [f64; 3] = [242.0, 193.0, 78.0];
const TILE: [f64; 3] = [29.0, 31.0, 38.0];

fn star() -> Vec<[f64; 2]> {
    (0..8)
        .map(|i| {
            let r = if i % 2 == 0 { 0.36 } else { 0.11 };
            let a = std::f64::consts::FRAC_PI_2 + f64::from(i) * std::f64::consts::FRAC_PI_4;
            [0.5 + r * a.cos(), 0.5 - r * a.sin()]
        })
        .collect()
}

fn inside(p: [f64; 2], poly: &[[f64; 2]]) -> bool {
    let mut on = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[j]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0] {
            on = !on;
        }
        j = i;
    }
    on
}

/// The color and cover of a point of the unit square.
fn sample(p: [f64; 2], star: &[[f64; 2]]) -> Option<[f64; 3]> {
    let d = (p[0] - 0.5).hypot(p[1] - 0.5);
    if d > 0.46 {
        None
    } else if d > 0.46 - 0.035 || inside(p, star) {
        Some(GOLD)
    } else {
        Some(TILE)
    }
}

/// The icon as RGBA bytes with straight alpha, rows from the top. Each pixel is the mean of 8 by 8 samples.
pub fn picture(size: u32) -> Vec<u8> {
    let star = star();
    let n = 8;
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let (mut rgb, mut cover) = ([0.0; 3], 0.0);
            for sy in 0..n {
                for sx in 0..n {
                    let p = [
                        (f64::from(x) + (f64::from(sx) + 0.5) / f64::from(n)) / f64::from(size),
                        (f64::from(y) + (f64::from(sy) + 0.5) / f64::from(n)) / f64::from(size),
                    ];
                    if let Some(c) = sample(p, &star) {
                        for k in 0..3 {
                            rgb[k] += c[k];
                        }
                        cover += 1.0;
                    }
                }
            }
            if cover > 0.0 {
                out.extend(rgb.iter().map(|c| (c / cover).round() as u8));
                out.push((cover * 255.0 / f64::from(n * n)).round() as u8);
            } else {
                out.extend([0, 0, 0, 0]);
            }
        }
    }
    out
}

fn png_bytes(size: u32, rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, size, size);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        if let Ok(mut w) = enc.write_header() {
            let _ = w.write_image_data(rgba);
        }
    }
    out
}

/// One picture as an icon's bitmap: a 32-bit image with its rows from the bottom, then an empty mask.
fn bmp_bytes(size: u32, rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend(40u32.to_le_bytes());
    out.extend((size as i32).to_le_bytes());
    out.extend((size as i32 * 2).to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(32u16.to_le_bytes());
    out.extend([0u8; 24]);
    for y in (0..size as usize).rev() {
        for px in rgba[y * size as usize * 4..][..size as usize * 4].chunks_exact(4) {
            out.extend([px[2], px[1], px[0], px[3]]);
        }
    }
    let mask_row = (size as usize).div_ceil(32) * 4;
    out.resize(out.len() + mask_row * size as usize, 0);
    out
}

/// The icon as an `.ico` file with every size of `SIZES`.
pub fn ico() -> Vec<u8> {
    let images: Vec<(u32, Vec<u8>)> = SIZES
        .iter()
        .map(|&n| {
            let rgba = picture(n);
            (n, if n >= 256 { png_bytes(n, &rgba) } else { bmp_bytes(n, &rgba) })
        })
        .collect();
    let mut out = Vec::new();
    out.extend([0u8, 0, 1, 0]);
    out.extend((images.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * images.len();
    for (n, data) in &images {
        let side = if *n >= 256 { 0 } else { *n as u8 };
        out.extend([side, side, 0, 0]);
        out.extend(1u16.to_le_bytes());
        out.extend(32u16.to_le_bytes());
        out.extend((data.len() as u32).to_le_bytes());
        out.extend((offset as u32).to_le_bytes());
        offset += data.len();
    }
    for (_, data) in images {
        out.extend(data);
    }
    out
}
