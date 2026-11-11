//! The kit's creatures: shade, giant hand, cage, lantern, watching eye, pale face, rag-doll glove, hanging glove and
//! gallows. A port of kit/stalker.py, watcher.py, effigy.py and glove_rope.py. Every random number is drawn in the
//! same order as there.

use super::image::{Image, Rgba};
use super::meshkit::{self as mk, v3, Lathe, Place, Soup, Tube, V3, BRIGHT};
use super::rng::Random;

const PI: f64 = std::f64::consts::PI;

fn rgb(r: i64, g: i64, b: i64) -> Rgba {
    [r as u8, g as u8, b as u8, 255]
}

fn l(im: &mut Image, x0: i64, y0: i64, x1: i64, y1: i64, c: Rgba) {
    im.line(&[(x0 as f64, y0 as f64), (x1 as f64, y1 as f64)], c, 1);
}

fn tube_end(s: &mut Soup, pts: &[V3], radii: &[f64], sides: usize, mat: usize) {
    mk::tube(s, pts, radii, sides, mat, Tube::default());
}

// ---------------------------------------------------------------------------------------------
// stalker.py

/// A dark figure, 115 high, with two pale points for eyes.
pub fn build_shade() -> Soup {
    let mut s = Soup::new();
    let mut tex = mk::noise(32, 32, &mut Random::new(61), -4, 6, [10, 8, 12]);
    tex.rectangle(24, 0, 31, 7, Some(rgb(236, 232, 214)), None);
    let m = s.material(tex, true, 1.0);
    let body = [[0.0, 0.0, 0.0], [0.0, 40.0, 0.0], [0.0, 80.0, 0.0], [0.0, 98.0, 0.0]];
    mk::tube(&mut s, &body, &[3.0, 6.0, 7.5, 3.0], 6, m, Tube { cap_end: false, u_repeat: 0.5, ..Tube::default() });
    let head = [[0.0, 98.0, 0.0], [0.0, 106.0, 3.0], [0.0, 114.0, 5.0]];
    mk::tube(&mut s, &head, &[5.5, 6.0, 3.5], 6, m, Tube { u_repeat: 0.5, ..Tube::default() });
    for sx in [-1.0, 1.0] {
        let arm = [[sx * 7.0, 94.0, 0.0], [sx * 10.0, 70.0, 1.0], [sx * 11.0, 40.0, 2.0], [sx * 10.0, 30.0, 3.0]];
        mk::tube(&mut s, &arm, &[2.0, 1.8, 1.6, 1.0], 4, m, Tube { u_repeat: 0.25, ..Tube::default() });
    }
    let pale = [[0.78, 0.22], [0.97, 0.22], [0.97, 0.03], [0.78, 0.03]];
    for sx in [-1.0, 1.0] {
        let (x0, x1) = (sx * 1.2 - 0.8, sx * 1.2 + 0.8);
        let z = 11.2;
        s.quad([v3(x0, 106.5, z), v3(x1, 106.5, z), v3(x1, 108.0, z), v3(x0, 108.0, z)], pale, m, [None; 4]);
    }
    s
}

fn glove_tex(seed: u64) -> Image {
    let mut rnd = Random::new(seed);
    let mut im = mk::noise(32, 32, &mut rnd, -16, 6, [214, 210, 200]);
    for _ in 0..40 {
        let x = rnd.randrange(0, 32);
        let y = rnd.randrange(0, 32);
        im.put(x as i32, y as i32, rgb(76, 60, 44));
    }
    for y in 24..32 {
        for x in 0..32 {
            if rnd.random() < 0.6 {
                let r = 60 + rnd.randint(0, 20);
                im.put(x, y, rgb(r, 46, 32));
            }
        }
    }
    im
}

/// A white glove as big as a tree, reaching up out of the ground.
pub fn build_hand() -> Soup {
    let mut s = Soup::new();
    let g = s.material(glove_tex(71), false, BRIGHT);
    let soil = s.material(mk::noise(16, 16, &mut Random::new(72), -10, 10, [48, 36, 28]), false, BRIGHT);
    mk::lathe(&mut s, &[(30.0, -6.0), (27.0, 20.0), (24.0, 50.0), (26.0, 70.0)], 8, g, Lathe::default());
    mk::box1(&mut s, [-26.0, 70.0, -10.0], [26.0, 130.0, 10.0], g);
    for (k, x) in [-19.0, -6.5, 6.5, 19.0].into_iter().enumerate() {
        let h = if k == 1 || k == 2 { 70.0 } else { 58.0 };
        let pts = [[x, 128.0, 0.0], [x * 1.08, 128.0 + h * 0.6, 4.0], [x * 1.12, 128.0 + h * 0.9, 18.0], [x * 1.12, 128.0 + h * 0.85, 34.0]];
        tube_end(&mut s, &pts, &[7.0, 6.5, 6.0, 5.0], 6, g);
    }
    tube_end(&mut s, &[[26.0, 92.0, 0.0], [40.0, 112.0, 6.0], [46.0, 130.0, 14.0]], &[7.0, 6.0, 5.0], 6, g);
    mk::lathe(&mut s, &[(48.0, -4.0), (34.0, 6.0), (24.0, 10.0)], 8, soil, Lathe::default());
    s
}

/// A post with an arm and a round iron cage on a chain, a limp glove inside.
pub fn build_cage() -> Soup {
    let mut rnd = Random::new(81);
    let mut s = Soup::new();
    let wd = s.material(mk::wood(&mut rnd, 16, 32, [50, 38, 28]), false, BRIGHT);
    let iron = s.material(mk::noise(16, 16, &mut rnd, -14, 10, [58, 52, 50]), false, BRIGHT);
    let glove = s.material(glove_tex(73), false, BRIGHT);
    mk::box1(&mut s, [-6.0, -4.0, -6.0], [6.0, 220.0, 6.0], wd);
    mk::box1(&mut s, [-6.0, 210.0, -5.0], [100.0, 220.0, 5.0], wd);
    let (cx, top) = (90.0, 210.0);
    mk::box1(&mut s, [cx - 1.2, 150.0, -1.2], [cx + 1.2, top, 1.2], iron);
    let (r, y0, y1) = (24.0, 70.0, 150.0);
    for k in 0..8 {
        let a = 2.0 * PI * k as f64 / 8.0;
        let (x, z) = (cx + r * a.cos(), r * a.sin());
        mk::box1(&mut s, [x - 1.3, y0, z - 1.3], [x + 1.3, y1, z + 1.3], iron);
    }
    let mut ends = Soup::new();
    mk::lathe(&mut ends, &[(r + 1.5, y1 - 2.0), (r + 1.5, y1 + 2.0), (0.0, y1 + 14.0)], 8, iron, Lathe::default());
    mk::lathe(&mut ends, &[(0.0, y0 - 3.0), (r + 1.5, y0 - 3.0), (r + 1.5, y0 + 2.0)], 8, iron, Lathe::default());
    s.add(&ends, Place { offset: [cx, 0.0, 0.0], ..Place::default() }, true);
    let mut limp = Soup::new();
    mk::box1(&mut limp, [-11.0, 0.0, -5.0], [11.0, 16.0, 5.0], glove);
    for x in [-8.0, -2.5, 2.5, 8.0] {
        tube_end(&mut limp, &[[x, 14.0, 0.0], [x, 22.0, 6.0], [x * 1.2, 20.0, 14.0]], &[3.0, 2.8, 2.4], 4, glove);
    }
    s.add(&limp, Place { offset: [cx, y0 + 1.0, 2.0], yaw: 0.6, tilt: (0.3, 0.4), ..Place::default() }, true);
    s
}

/// A post with a lantern on a hook, its panes glowing.
pub fn build_lantern() -> Soup {
    let mut rnd = Random::new(91);
    let mut s = Soup::new();
    let wd = s.material(mk::wood(&mut rnd, 16, 32, [54, 40, 30]), false, BRIGHT);
    let iron = s.material(mk::noise(16, 16, &mut rnd, -10, 10, [40, 36, 34]), false, BRIGHT);
    let mut pane = mk::noise(16, 16, &mut rnd, -20, 0, [255, 190, 80]);
    l(&mut pane, 8, 0, 8, 15, rgb(40, 30, 20));
    l(&mut pane, 0, 8, 15, 8, rgb(40, 30, 20));
    let glow = s.material(pane, false, 1.8);
    mk::box1(&mut s, [-4.0, -4.0, -4.0], [4.0, 150.0, 4.0], wd);
    mk::box1(&mut s, [-4.0, 140.0, -3.0], [34.0, 146.0, 3.0], wd);
    mk::box1(&mut s, [29.0, 118.0, -1.0], [31.0, 140.0, 1.0], iron);
    let (g, i) = (Some(glow), Some(iron));
    mk::box_(&mut s, [21.0, 96.0, -9.0], [39.0, 118.0, 9.0], [g, g, i, i, g, g], None);
    let mut roof = Soup::new();
    mk::lathe(&mut roof, &[(14.0, 118.0), (14.0, 120.0), (0.0, 130.0)], 4, iron, Lathe { phase: PI / 4.0, ..Lathe::default() });
    s.add(&roof, Place { offset: [30.0, 0.0, 0.0], ..Place::default() }, true);
    s
}

// ---------------------------------------------------------------------------------------------
// watcher.py

/// A bloodshot eye, painted square and stored 64x32.
pub fn eye_tex() -> Image {
    let mut rnd = Random::new(41);
    let mut im = Image::new(64, 64, rgb(24, 6, 8));
    im.ellipse(0, 0, 63, 63, Some(rgb(214, 204, 172)), None, 1);
    for _ in 0..46 {
        let mut a = rnd.uniform(0.0, 2.0 * PI);
        let r0 = 31.5;
        let (mut x, mut y) = (32.0 + r0 * a.cos(), 32.0 + r0 * a.sin());
        for _ in 0..rnd.randrange(8, 20) {
            a += rnd.uniform(-0.35, 0.35);
            let (nx, ny) = (x - a.cos() * 1.7, y - a.sin() * 1.7);
            let c = rgb(150 + rnd.randrange(0, 30), 20, 22);
            im.line(&[(x, y), (nx, ny)], c, 1);
            x = nx;
            y = ny;
        }
    }
    im.ellipse(16, 16, 48, 48, Some(rgb(120, 150, 24)), None, 1);
    im.ellipse(19, 19, 45, 45, Some(rgb(170, 190, 40)), None, 1);
    for k in 0..18 {
        let a = 2.0 * PI * k as f64 / 18.0;
        im.line(&[(32.0 + 5.0 * a.cos(), 32.0 + 5.0 * a.sin()), (32.0 + 14.0 * a.cos(), 32.0 + 14.0 * a.sin())], rgb(90, 110, 20), 1);
    }
    im.ellipse(27, 14, 37, 50, Some(rgb(6, 2, 2)), None, 1);
    im.ellipse(22, 20, 25, 23, Some(rgb(255, 255, 230)), None, 1);
    im.resize_height_lanczos(32)
}

/// A pale face with red points for pupils, painted square and stored 64x32.
pub fn face_tex() -> Image {
    let mut rnd = Random::new(43);
    let mut im = Image::new(64, 64, rgb(24, 22, 24));
    im.ellipse(0, 0, 63, 63, Some(rgb(196, 192, 180)), None, 1);
    for _ in 0..300 {
        let x = rnd.randrange(0, 64);
        let y = rnd.randrange(0, 64);
        if (x - 32) * (x - 32) + (y - 32) * (y - 32) < 31 * 31 {
            let c = rnd.randint(-26, 12);
            im.put(x as i32, y as i32, rgb(196 + c, 192 + c, 180 + c));
        }
    }
    for cx in [20, 44] {
        im.ellipse(cx - 9, 18, cx + 9, 40, Some(rgb(4, 2, 4)), None, 1);
        im.ellipse(cx - 1, 27, cx + 1, 29, Some(rgb(255, 30, 20)), None, 1);
    }
    im.polygon(&[(32.0, 32.0), (28.0, 44.0), (36.0, 44.0)], rgb(60, 50, 50));
    let mouth = [(14.0, 50.0), (22.0, 54.0), (28.0, 50.0), (34.0, 55.0), (40.0, 50.0), (46.0, 54.0), (52.0, 50.0)];
    im.line(&mouth, rgb(10, 4, 4), 2);
    for (x, y) in mouth {
        im.line(&[(x, y - 4.0), (x, y + 4.0)], rgb(10, 4, 4), 1);
    }
    im.resize_height_lanczos(32)
}

/// A bloodshot eye: the front half of a ball, radius 100.
pub fn build_eye() -> Soup {
    let mut s = Soup::new();
    let m = s.material(eye_tex(), true, 1.0);
    let back = s.material(mk::noise(8, 8, &mut Random::new(2), -6, 6, [40, 8, 10]), false, BRIGHT);
    mk::dome(&mut s, 100.0, 5, 16, m, [1.0, 1.0, 0.9], back);
    s
}

/// A pale face: the front half of a flattened ball, radius 100.
pub fn build_face() -> Soup {
    let mut s = Soup::new();
    let m = s.material(face_tex(), true, 1.0);
    let back = s.material(mk::noise(8, 8, &mut Random::new(3), -6, 6, [50, 48, 50]), false, BRIGHT);
    mk::dome(&mut s, 100.0, 5, 16, m, [0.8, 1.1, 0.4], back);
    s
}

// ---------------------------------------------------------------------------------------------
// effigy.py

fn patch(seed: u64, base: [i64; 3]) -> Image {
    let mut rnd = Random::new(seed);
    let mut im = mk::noise(32, 32, &mut rnd, -14, 12, base);
    let cols = [rgb(120, 70, 60), rgb(90, 96, 110), rgb(140, 128, 90)];
    for col in cols {
        let x0 = rnd.randrange(0, 20) as i32;
        let y0 = rnd.randrange(0, 20) as i32;
        let x1 = x0 + rnd.randrange(8, 14) as i32;
        let y1 = y0 + rnd.randrange(8, 14) as i32;
        im.rectangle(x0, y0, x1, y1, Some(col), None);
        for k in (0..14).step_by(2) {
            im.put(x0 + k, y0, rgb(40, 30, 28));
        }
    }
    for y in (0..32).step_by(4) {
        l(&mut im, 0, y, 2, y + 1, rgb(40, 30, 28));
    }
    im
}

fn effigy_face() -> Image {
    let mut im = mk::noise(64, 32, &mut Random::new(31), -10, 10, [168, 158, 134]);
    let (dark, thread) = (rgb(24, 18, 18), rgb(50, 38, 34));
    im.rectangle(0, 0, 63, 31, None, Some(thread));
    for x in (1..63).step_by(3) {
        l(&mut im, x, 1, x + 1, 2, thread);
    }
    im.ellipse(11, 6, 25, 20, Some(dark), None, 1);
    l(&mut im, 13, 8, 23, 18, rgb(200, 190, 160));
    l(&mut im, 23, 8, 13, 18, rgb(200, 190, 160));
    im.ellipse(38, 9, 56, 27, Some(dark), None, 1);
    im.ellipse(44, 14, 50, 22, Some(rgb(190, 30, 20)), None, 1);
    l(&mut im, 36, 4, 38, 12, thread);
    let pts = [(6.0, 26.0), (14.0, 29.0), (24.0, 27.0), (34.0, 30.0), (44.0, 29.0), (54.0, 26.0), (60.0, 22.0)];
    im.line(&pts, dark, 1);
    for &(x, y) in &pts[1..pts.len() - 1] {
        im.line(&[(x, y - 3.0), (x, y + 2.0)], dark, 1);
    }
    for x in (8..58).step_by(6) {
        l(&mut im, x, 28, x + 2, 31, dark);
    }
    for y in 20..31 {
        im.put(14, y, rgb(60, 30, 26));
    }
    im
}

/// A giant stitched glove on a cross, 330 tall.
pub fn build_effigy() -> Soup {
    let mut s = Soup::new();
    let mut rnd = Random::new(8);
    let wd = s.material(mk::wood(&mut rnd, 16, 32, [56, 42, 32]), false, BRIGHT);
    let fc = s.material(effigy_face(), true, BRIGHT);
    let pa = s.material(patch(1, [150, 142, 120]), false, BRIGHT);
    let pb = s.material(patch(2, [140, 132, 110]), false, BRIGHT);
    let rope = s.material(mk::noise(8, 16, &mut Random::new(2), -14, 10, [110, 90, 60]), false, BRIGHT);
    mk::box1(&mut s, [-9.0, -8.0, -9.0], [9.0, 330.0, 9.0], wd);
    mk::box1(&mut s, [-90.0, 196.0, -8.0], [90.0, 214.0, 8.0], wd);
    let (cx, cy, cz) = (0.0, 232.0, 14.0);
    let (w, hh, t) = (46.0, 52.0, 13.0);
    let (a, b) = (Some(pa), Some(pb));
    mk::box_(&mut s, [cx - w, cy - hh, cz - t], [cx + w, cy, cz + t], [a, a, None, a, b, a], None);
    mk::box_(&mut s, [cx - w, cy, cz - t], [cx + w, cy + hh, cz + t], [a, a, a, None, Some(fc), b], Some(fc));
    mk::lathe(&mut s, &[(0.0, 0.0), (48.0, 0.0), (50.0, 4.0), (44.0, 28.0), (40.0, 30.0)], 8, pb, Lathe::default());
    let top = cy + hh;
    for (x, len, bend) in [(-33.0, 62.0, -0.5), (-11.0, 74.0, 0.2), (11.0, 70.0, -0.1), (33.0, 56.0, 0.6)] {
        let pts = [
            [x, top - 2.0, cz],
            [x + bend * 14.0, top + len * 0.55, cz + 3.0],
            [x + bend * 36.0, top + len * 0.9, cz + 14.0],
            [x + bend * 46.0, top + len * 0.9 - 10.0, cz + 22.0],
        ];
        tube_end(&mut s, &pts, &[11.5, 10.5, 9.5, 8.0], 5, pa);
    }
    let thumb = [[w - 3.0, cy + 8.0, cz], [w + 18.0, cy + 18.0, cz + 3.0], [w + 36.0, cy - 4.0, cz + 8.0], [w + 38.0, cy - 28.0, cz + 12.0]];
    tube_end(&mut s, &thumb, &[11.0, 10.0, 9.0, 7.0], 5, pb);
    for x in [-60.0, 60.0] {
        tube_end(&mut s, &[[x, 218.0, cz + 12.0], [x * 0.8, 204.0, cz + 6.0], [x * 0.9, 196.0, cz + 2.0]], &[2.2, 2.2, 2.2], 4, rope);
    }
    tube_end(&mut s, &[[0.0, 280.0, 11.0], [4.0, 268.0, 12.0], [0.0, 256.0, 11.0]], &[2.2, 2.2, 2.2], 4, rope);
    for k in 0..6 {
        let a = 2.0 * PI * k as f64 / 6.0;
        let pts = [[a.cos() * 30.0, 2.0, cz + a.sin() * 6.0], [a.cos() * 36.0, -10.0, cz + a.sin() * 8.0]];
        tube_end(&mut s, &pts, &[1.5, 0.6], 3, pb);
    }
    s
}

// ---------------------------------------------------------------------------------------------
// glove_rope.py

fn cloth(seed: u64) -> Image {
    let mut rnd = Random::new(seed);
    let mut im = mk::noise(32, 32, &mut rnd, -16, 10, [176, 170, 158]);
    for _ in 0..5 {
        let x = rnd.randrange(0, 32) as i32;
        let y = rnd.randrange(0, 32) as i32;
        let r = rnd.randrange(2, 5) as i32;
        im.ellipse(x - r, y - r, x + r, y + r, Some(rgb(110, 76, 52)), None, 1);
    }
    l(&mut im, 16, 0, 16, 31, rgb(70, 60, 54));
    for y in (0..32).step_by(3) {
        l(&mut im, 13, y, 19, y + 1, rgb(60, 50, 46));
    }
    im
}

fn rope_tex() -> Image {
    let mut im = mk::noise(8, 16, &mut Random::new(4), -14, 10, [120, 98, 64]);
    for y in (0..16).step_by(3) {
        l(&mut im, 0, y, 7, y + 2, rgb(70, 56, 36));
    }
    im
}

// A hanging glove, its origin at the top of the cuff, fingers pointing down.
fn glove(s: &mut Soup, mat: usize, seed: u64) {
    let limp = 1.0;
    let mut rnd = Random::new(seed);
    mk::lathe(s, &[(5.5, 0.0), (7.5, -1.0), (8.5, -9.0), (6.5, -10.0)], 6, mat, Lathe { v1: 0.4, ..Lathe::default() });
    mk::lathe(s, &[(0.0, 1.0), (5.5, 0.0)], 6, mat, Lathe::default());
    let mut palm = Soup::new();
    mk::box1(&mut palm, [-8.5, -20.0, -4.0], [8.5, -9.5, 4.0], mat);
    s.add(&palm, Place::default(), true);
    for (x, len) in [(-6.0, 10.5), (-2.0, 12.0), (2.0, 11.5), (6.0, 9.5)] {
        let sway = rnd.uniform(-0.25, 0.25) * limp;
        let pts = [[x, -19.5, 0.0], [x + sway * 3.0, -19.5 - len * 0.6, 2.0 * limp], [x + sway * 6.0, -19.5 - len - 8.0, 6.0 * limp]];
        tube_end(s, &pts, &[2.6, 2.3, 1.6], 3, mat);
    }
    tube_end(s, &[[8.0, -13.0, 0.0], [14.0, -19.0, 1.0], [16.0, -25.0, 3.0]], &[3.0, 2.6, 2.0], 3, mat);
}

fn rope_to(s: &mut Soup, mat: usize, x: f64, y_top: f64, y_bottom: f64) {
    let (z, r) = (0.0, 1.4);
    let pts = [[x, y_top, z], [x + 0.3, (y_top + y_bottom) / 2.0, z], [x, y_bottom, z]];
    mk::tube(s, &pts, &[r, r, r], 5, mat, Tube { v_scale: ((y_top - y_bottom) / 16.0).max(1.0), ..Tube::default() });
}

/// One limp glove on a rope. The cuff is at y = 0.
pub fn build_single(seed: u64, drop: f64) -> Soup {
    let mut s = Soup::new();
    let c = s.material(cloth(seed), false, BRIGHT);
    let r = s.material(rope_tex(), false, BRIGHT);
    glove(&mut s, c, seed);
    rope_to(&mut s, r, 0.0, drop + 12.0, 0.5);
    s
}

/// A post and a beam with gloves hanging from it.
pub fn build_gallows(seed: u64) -> Soup {
    let mut rnd = Random::new(seed);
    let mut s = Soup::new();
    let wd = s.material(mk::wood(&mut rnd, 16, 32, [58, 42, 30]), false, BRIGHT);
    let c = s.material(cloth(seed), false, BRIGHT);
    let r = s.material(rope_tex(), false, BRIGHT);
    mk::box1(&mut s, [-6.0, -4.0, -6.0], [6.0, 140.0, 6.0], wd);
    mk::box1(&mut s, [-6.0, 130.0, -5.0], [78.0, 140.0, 5.0], wd);
    let mut brace = Soup::new();
    mk::box1(&mut brace, [-2.0, -2.0, -3.0], [2.0, 34.0, 3.0], wd);
    s.add(&brace, Place { offset: [10.0, 100.0, 0.0], tilt: (0.0, -0.7), ..Place::default() }, true);
    for (x, drop) in [(30.0, 40.0), (60.0, 62.0)] {
        let mut g = Soup::new();
        glove(&mut g, c, rnd.randint(0, 99) as u64);
        let top = 130.0;
        let y = top - drop - 12.0;
        rope_to(&mut s, r, x, top, y + 0.5);
        let yaw = rnd.uniform(-1.0, 1.0);
        let t0 = rnd.uniform(-0.06, 0.06);
        let t1 = rnd.uniform(-0.1, 0.1);
        s.add(&g, Place { offset: [x, y, 0.0], yaw, tilt: (t0, t1), ..Place::default() }, true);
    }
    s
}
