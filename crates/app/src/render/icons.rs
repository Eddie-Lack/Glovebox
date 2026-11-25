//! The round icons that stand for things without a model: a disk in the kind's color with a black rim and a dark
//! glyph, as a 64 by 64 texture for a sprite.
//!
//! The glyphs are shapes drawn by this module (stars, a heart, a hand, a note and so on). The icons look the same
//! on every computer. Letters, digits, '?' and '!' use the kit's stroke font. A glyph this module doesn't know is
//! drawn as a dot.

use glovebox_core::kit::font;
use glovebox_editor::scene::Texture;
use std::collections::HashMap;
use std::f32::consts::PI;
use std::sync::Arc;

pub const SIZE: usize = 64;

/// A shape in the icon's 64 by 64 square, y down.
enum Shape {
    /// A filled polygon
    Poly(Vec<(f32, f32)>),
    /// A line through the points, of the given width, with round ends
    Stroke(Vec<(f32, f32)>, f32),
    Disk(f32, f32, f32),
    /// A circle line: center, radius, width
    Ring(f32, f32, f32, f32),
}

fn seg_dist(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let d = (b.0 - a.0, b.1 - a.1);
    let l = d.0 * d.0 + d.1 * d.1;
    let t = if l == 0.0 { 0.0 } else { (((p.0 - a.0) * d.0 + (p.1 - a.1) * d.1) / l).clamp(0.0, 1.0) };
    (a.0 + d.0 * t - p.0).hypot(a.1 + d.1 * t - p.1)
}

impl Shape {
    fn covers(&self, p: (f32, f32)) -> bool {
        match self {
            Shape::Poly(pts) => {
                let mut inside = false;
                for i in 0..pts.len() {
                    let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
                    if (a.1 > p.1) != (b.1 > p.1) && p.0 < (b.0 - a.0) * (p.1 - a.1) / (b.1 - a.1) + a.0 {
                        inside = !inside;
                    }
                }
                inside
            }
            Shape::Stroke(pts, w) => pts.windows(2).any(|s| seg_dist(p, s[0], s[1]) <= w / 2.0),
            Shape::Disk(x, y, r) => (p.0 - x).hypot(p.1 - y) <= *r,
            Shape::Ring(x, y, r, w) => ((p.0 - x).hypot(p.1 - y) - r).abs() <= w / 2.0,
        }
    }
}

/// A star around the icon's center: `points` tips at `outer`, the notches between them at `inner`.
fn star(points: usize, outer: f32, inner: f32) -> Shape {
    Shape::Poly(
        (0..points * 2)
            .map(|i| {
                let a = PI * i as f32 / points as f32 - PI / 2.0;
                let r = if i % 2 == 0 { outer } else { inner };
                (32.0 + r * a.cos(), 32.0 + r * a.sin())
            })
            .collect(),
    )
}

fn wave(y: f32) -> Shape {
    Shape::Stroke((0..=24).map(|i| (19.0 + i as f32 * 26.0 / 24.0, y + 2.5 * (i as f32 * PI / 6.0).sin())).collect(), 3.0)
}

fn glyph(g: &str) -> Vec<Shape> {
    match g {
        "★" => vec![star(5, 15.0, 6.3)],
        "✦" => vec![star(4, 16.0, 5.0)],
        "✸" => vec![star(8, 15.0, 7.5)],
        "●" => vec![Shape::Disk(32.0, 32.0, 10.0)],
        "◉" => vec![Shape::Ring(32.0, 32.0, 11.0, 3.0), Shape::Disk(32.0, 32.0, 5.5)],
        "◇" => vec![Shape::Stroke(vec![(32.0, 18.0), (46.0, 32.0), (32.0, 46.0), (18.0, 32.0), (32.0, 18.0)], 3.0)],
        "♥" => vec![Shape::Poly(
            (0..48)
                .map(|i| {
                    let t = 2.0 * PI * i as f32 / 48.0;
                    let x = 16.0 * t.sin().powi(3);
                    let y = 13.0 * t.cos() - 5.0 * (2.0 * t).cos() - 2.0 * (3.0 * t).cos() - (4.0 * t).cos();
                    (32.0 + x * 0.85, 31.0 - y * 0.85)
                })
                .collect(),
        )],
        "♪" => vec![
            Shape::Disk(27.0, 42.0, 5.5),
            Shape::Stroke(vec![(31.0, 42.0), (31.0, 19.0)], 3.0),
            Shape::Stroke(vec![(31.0, 20.0), (40.0, 26.0)], 4.0),
        ],
        "≋" => vec![wave(24.0), wave(32.0), wave(40.0)],
        "⇥" => vec![
            Shape::Stroke(vec![(17.0, 32.0), (41.0, 32.0)], 3.5),
            Shape::Stroke(vec![(33.0, 24.0), (41.0, 32.0), (33.0, 40.0)], 3.5),
            Shape::Stroke(vec![(46.0, 21.0), (46.0, 43.0)], 3.5),
        ],
        // A raised hand: four fingers, a thumb and the palm
        "✋" => vec![
            Shape::Stroke(vec![(25.0, 36.0), (24.0, 22.0)], 4.6),
            Shape::Stroke(vec![(30.0, 34.0), (30.0, 17.0)], 4.6),
            Shape::Stroke(vec![(35.0, 34.0), (36.0, 18.0)], 4.6),
            Shape::Stroke(vec![(40.0, 36.0), (42.0, 23.0)], 4.6),
            Shape::Stroke(vec![(24.0, 40.0), (17.0, 32.0)], 4.6),
            Shape::Poly(vec![(22.7, 34.0), (42.3, 34.0), (41.0, 43.0), (37.0, 47.5), (28.0, 47.5), (23.0, 43.0)]),
        ],
        _ => {
            let c = g.chars().next().unwrap_or(' ').to_ascii_uppercase();
            let bits = font::glyph(c, 5, 7);
            let mut out = Vec::new();
            for (y, row) in bits.iter().enumerate() {
                for (x, on) in row.iter().enumerate() {
                    if *on {
                        let (x0, y0) = (22.0 + 4.0 * x as f32, 18.0 + 4.0 * y as f32);
                        out.push(Shape::Poly(vec![(x0, y0), (x0 + 4.0, y0), (x0 + 4.0, y0 + 4.0), (x0, y0 + 4.0)]));
                    }
                }
            }
            if out.is_empty() {
                out.push(Shape::Disk(32.0, 32.0, 6.0));
            }
            out
        }
    }
}

/// The icon's pixels: 64 rows of 64 RGBA pixels, the bottom row first (the order a sprite's texture has).
/// `color` is the disk's color.
pub fn icon_rgba(glyph_: &str, color: [u8; 3]) -> Vec<u8> {
    let shapes = glyph(glyph_);
    let mut out = vec![0u8; SIZE * SIZE * 4];
    const SUB: usize = 4;
    for y in 0..SIZE {
        for x in 0..SIZE {
            // Straight color and alpha, averaged over the pixel's samples
            let (mut rgb, mut alpha) = ([0.0f32; 3], 0.0f32);
            for sy in 0..SUB {
                for sx in 0..SUB {
                    let p = (x as f32 + (sx as f32 + 0.5) / SUB as f32, y as f32 + (sy as f32 + 0.5) / SUB as f32);
                    let r = (p.0 - 32.0).hypot(p.1 - 32.0);
                    // A rim of width 4 on a circle of radius 26, and a glyph in #111
                    let c = if r > 28.0 {
                        continue;
                    } else if r >= 24.0 {
                        [0.0; 3]
                    } else if shapes.iter().any(|s| s.covers(p)) {
                        [17.0; 3]
                    } else {
                        [f32::from(color[0]), f32::from(color[1]), f32::from(color[2])]
                    };
                    for k in 0..3 {
                        rgb[k] += c[k];
                    }
                    alpha += 1.0;
                }
            }
            let o = ((SIZE - 1 - y) * SIZE + x) * 4;
            if alpha > 0.0 {
                for k in 0..3 {
                    out[o + k] = (rgb[k] / alpha).round() as u8;
                }
                out[o + 3] = (alpha * 255.0 / (SUB * SUB) as f32).round() as u8;
            }
        }
    }
    out
}

/// Reads "#rrggbb". Anything else is white.
pub fn hex_rgb(s: &str) -> [u8; 3] {
    let h = s.trim_start_matches('#');
    let byte = |i: usize| h.get(i..i + 2).and_then(|b| u8::from_str_radix(b, 16).ok());
    match (h.len(), byte(0), byte(2), byte(4)) {
        (6, Some(r), Some(g), Some(b)) => [r, g, b],
        _ => [255; 3],
    }
}

/// The icons' textures, one for each glyph and color.
#[derive(Default)]
pub struct Icons {
    cache: HashMap<(String, [u8; 3]), Arc<Texture>>,
}

impl Icons {
    pub fn new() -> Icons {
        Icons::default()
    }

    /// The texture of a glyph on a disk of the color "#rrggbb". Make a sprite of it with
    /// `scene::Sprite::new(texture, size)`; the editor's sizes are 10 to 18 world units.
    pub fn texture(&mut self, glyph: &str, color: &str) -> Arc<Texture> {
        let rgb = hex_rgb(color);
        self.cache
            .entry((glyph.to_string(), rgb))
            .or_insert_with(|| {
                let t = Texture::new(SIZE as u32, SIZE as u32, icon_rgba(glyph, rgb));
                Arc::new(t.expect("the icon has 64 by 64 pixels"))
            })
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_icon_is_a_rimmed_disc_with_a_glyph() {
        let px = icon_rgba("●", [200, 100, 50]);
        let at = |x: usize, y: usize| &px[((SIZE - 1 - y) * SIZE + x) * 4..][..4];
        assert_eq!(at(0, 0), [0, 0, 0, 0]);
        assert_eq!(at(32, 6), [0, 0, 0, 255]);
        assert_eq!(at(32, 14), [200, 100, 50, 255]);
        assert_eq!(at(32, 32), [17, 17, 17, 255]);
        // the edge is smooth
        assert!(px.chunks(4).filter(|c| (1..255).contains(&c[3])).count() > 100);
        // every glyph the editor uses draws something, and no two are the same picture
        let all = ["★", "✦", "✸", "●", "◉", "◇", "♥", "♪", "≋", "⇥", "✋", "?", "!"];
        let pics: Vec<Vec<u8>> = all.iter().map(|g| icon_rgba(g, [255; 3])).collect();
        let blank = icon_rgba(" ", [255; 3]);
        for (i, p) in pics.iter().enumerate() {
            assert!(p.chunks(4).filter(|c| c[..3] == [17, 17, 17]).count() > 40, "{}", all[i]);
            assert!(pics[..i].iter().all(|q| q != p) && *p != blank, "{}", all[i]);
        }
        // the hand is upright: its top (the fingers) is wider than its bottom
        let hand = icon_rgba("✋", [255; 3]);
        let dark = |y: usize| (0..SIZE).filter(|x| hand[((SIZE - 1 - y) * SIZE + x) * 4] == 17).count();
        assert!(dark(26) > dark(46));
    }

    #[test]
    fn colors_and_the_cache() {
        assert_eq!(hex_rgb("#ff8ad8"), [255, 138, 216]);
        assert_eq!(hex_rgb("nonsense"), [255; 3]);
        assert_eq!(hex_rgb("#é8ad8"), [255; 3]);
        let mut icons = Icons::new();
        let (a, b, c) = (icons.texture("★", "#c58bff"), icons.texture("★", "#c58bff"), icons.texture("★", "#ffffff"));
        assert!(Arc::ptr_eq(&a, &b) && !Arc::ptr_eq(&a, &c));
    }
}
