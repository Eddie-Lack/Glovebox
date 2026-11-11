//! The kit's ruins and barriers: a signpost, rubble, a scorch mark, thorns and two gates. A port of kit/ruins.py and
//! kit/barrier.py. Every random number is drawn in the same order as there.

use super::font;
use super::image::{Image, Rgba};
use super::meshkit::{self as mk, add, cross, dot3, mul, norm, sub, v3, Place, Soup, Tube, V3, BRIGHT};
use super::rng::{hypot, Random};

const PI: f64 = std::f64::consts::PI;

fn rgb(r: i64, g: i64, b: i64) -> Rgba {
    [r as u8, g as u8, b as u8, 255]
}

// ---------------------------------------------------------------------------------------------
// ruins.py

/// A painted board (64x32): the name in peeling white, a red word under it, and rot.
pub fn board_tex(top: &str, bottom: &str, size: u32, seed: u64) -> Image {
    let mut rnd = Random::new(seed);
    let mut im = mk::wood(&mut rnd, 64, 32, [82, 60, 42]);
    im.rectangle(1, 1, 62, 30, None, Some(rgb(40, 28, 20)));
    let w = font::text_length(top, size);
    font::draw_text(&mut im, (64.0 - w) / 2.0, 4.0, top, rgb(214, 206, 188), size);
    let w2 = font::text_length(bottom, 8);
    font::draw_text(&mut im, (64.0 - w2) / 2.0, 17.0, bottom, rgb(176, 24, 20), 8);
    for _ in 0..40 {
        let x = rnd.randrange(3, 61);
        let y = rnd.randrange(3, 29);
        im.put(x as i32, y as i32, rgb(82, 60, 42));
    }
    for _ in 0..3 {
        let x0 = rnd.randrange(8, 56);
        for y in 22..22 + rnd.randrange(4, 9) {
            im.put(x0 as i32, y as i32, rgb(120, 14, 10));
        }
    }
    im
}

/// A leaning post with a board nailed on at a slant.
pub fn build_sign(top: &str, bottom: &str, size: u32, seed: u64) -> Soup {
    let mut rnd = Random::new(seed);
    let mut s = Soup::new();
    let wd = s.material(mk::wood(&mut rnd, 16, 32, [58, 42, 30]), false, BRIGHT);
    let face = s.material(board_tex(top, bottom, size, seed), true, 1.0);
    let mut post = Soup::new();
    mk::box1(&mut post, [-3.0, -6.0, -3.0], [3.0, 58.0, 3.0], wd);
    s.add(&post, Place { tilt: (0.0, 0.10), ..Place::default() }, true);
    let mut board = Soup::new();
    let w = Some(wd);
    mk::box_(&mut board, [-30.0, -15.0, -1.5], [30.0, 15.0, 1.5], [w, w, w, w, Some(face), w], Some(face));
    s.add(&board, Place { offset: [1.0, 38.0, 4.0], tilt: (0.0, -0.22), ..Place::default() }, true);
    s
}

/// Soot with cracks and embers (32x32). The corners are clear.
pub fn soot(seed: u64) -> Image {
    let mut rnd = Random::new(seed);
    let mut im = mk::noise(32, 32, &mut rnd, -8, 10, [26, 22, 26]);
    for _ in 0..6 {
        let mut a = rnd.uniform(0.0, 2.0 * PI);
        let (mut x, mut y) = (16.0, 16.0);
        for _ in 0..rnd.randint(5, 9) {
            a += rnd.uniform(-0.5, 0.5);
            let (nx, ny) = (x + 2.2 * a.cos(), y + 2.2 * a.sin());
            im.line(&[(x, y), (nx, ny)], rgb(8, 6, 8), 1);
            x = nx;
            y = ny;
        }
    }
    for _ in 0..14 {
        let x = rnd.randrange(4, 28) as i32;
        let y = rnd.randrange(4, 28) as i32;
        im.put(x, y, rgb(255, 90, 28));
        im.put(x + 1, y, rgb(160, 40, 18));
    }
    for y in 0..32 {
        for x in 0..32 {
            let (fx, fy) = (x as f64 - 15.5, y as f64 - 15.5);
            let r = hypot(fx, fy);
            let lim = 14.5 + 1.4 * (fy.atan2(fx) * 5.0 + seed as f64).sin();
            if r > lim {
                im.put(x, y, [0, 0, 0, 0]);
            }
        }
    }
    im
}

/// A flat black disc of soot, 280 across.
pub fn build_scorch() -> Soup {
    let mut s = Soup::new();
    let m = s.material(soot(3), true, 1.2);
    mk::disc(&mut s, [0.0; 3], 140.0, 14, m);
    s
}

/// A heap of broken stone blocks.
pub fn build_rubble() -> Soup {
    let mut rnd = Random::new(5);
    let mut s = Soup::new();
    let st = s.material(mk::stone(&mut rnd, 32, 32, [88, 90, 88], true), false, BRIGHT);
    for k in 0..7 {
        let mut b = Soup::new();
        let w = rnd.uniform(14.0, 30.0);
        let h = rnd.uniform(9.0, 18.0);
        let d = rnd.uniform(12.0, 24.0);
        mk::box1(&mut b, [-w / 2.0, 0.0, -d / 2.0], [w / 2.0, h, d / 2.0], st);
        let a = 2.0 * PI * k as f64 / 7.0 + rnd.uniform(-0.4, 0.4);
        let r = if k == 0 { 0.0 } else { rnd.uniform(14.0, 36.0) };
        let offset = [r * a.cos(), if k < 5 { 0.0 } else { 12.0 }, r * a.sin()];
        let yaw = rnd.uniform(0.0, 6.28);
        let t0 = rnd.uniform(-0.4, 0.4);
        let t1 = rnd.uniform(-0.4, 0.4);
        s.add(&b, Place { offset, yaw, tilt: (t0, t1), ..Place::default() }, true);
    }
    s
}

// ---------------------------------------------------------------------------------------------
// barrier.py

/// Black thorn bark with dark red veins (16x16).
pub fn bark(seed: u64) -> Image {
    let mut rnd = Random::new(seed);
    let mut im = mk::noise(16, 16, &mut rnd, -10, 12, [40, 30, 42]);
    for _ in 0..4 {
        let mut x = rnd.randrange(0, 16);
        for y in 0..16 {
            x = (x + rnd.choice(&[-1, 0, 0, 1])).clamp(0, 15);
            im.put(x as i32, y, rgb(96, 20, 22));
        }
    }
    im
}

fn bone(seed: u64) -> Image {
    let mut rnd = Random::new(seed);
    let mut im = mk::noise(16, 32, &mut rnd, -22, 12, [196, 188, 160]);
    for _ in 0..5 {
        let x = rnd.randrange(0, 16);
        let y = rnd.randrange(0, 32);
        let x1 = x + rnd.randint(-2, 2);
        let y1 = y + rnd.randint(3, 9);
        im.line(&[(x as f64, y as f64), (x1 as f64, y1 as f64)], rgb(96, 78, 56), 1);
    }
    im
}

fn rust(seed: u64) -> Image {
    let mut rnd = Random::new(seed);
    let mut im = mk::noise(16, 16, &mut rnd, -24, 20, [116, 66, 40]);
    for _ in 0..10 {
        let x = rnd.randrange(0, 16);
        let y = rnd.randrange(0, 16);
        im.put(x as i32, y as i32, rgb(60, 30, 22));
    }
    im
}

/// A black wall with glowing red signs in rows (64x32), a ring with a slit eye in the middle.
pub fn runes(seed: u64) -> Image {
    let mut rnd = Random::new(seed);
    let mut im = Image::new(64, 32, rgb(12, 8, 16));
    for _ in 0..90 {
        let x = rnd.randrange(0, 64);
        let y = rnd.randrange(0, 32);
        let c = rnd.randint(0, 22);
        im.put(x as i32, y as i32, rgb(12 + c, 8 + c / 2, 16 + c));
    }
    let (red, glow) = (rgb(255, 48, 36), rgb(120, 16, 18));
    for row in 0..3 {
        for col in 0..8 {
            let (x0, y0) = (2.0 + col as f64 * 7.8, 2.0 + row as f64 * 10.5);
            if 22.0 < x0 && x0 < 40.0 && row == 1 {
                continue;
            }
            let n = rnd.randint(3, 5);
            let mut pts = Vec::new();
            for _ in 0..n {
                let x = x0 + rnd.randrange(0, 4) as f64 * 1.4;
                let y = y0 + rnd.randrange(0, 4) as f64 * 1.6;
                pts.push((x, y));
            }
            im.line(&pts, glow, 2);
            im.line(&pts, red, 1);
        }
    }
    im.ellipse(22, 9, 41, 23, None, Some(glow), 2);
    im.ellipse(23, 10, 40, 22, None, Some(red), 1);
    im.ellipse(27, 12, 36, 20, Some(rgb(224, 214, 160)), None, 1);
    im.ellipse(30, 11, 33, 21, Some(rgb(8, 2, 2)), None, 1);
    im.rectangle(0, 0, 63, 0, Some(red), None);
    im.rectangle(0, 31, 63, 31, Some(red), None);
    im
}

/// Stitched patches of dark flesh with eyes (64x32).
pub fn curtain(seed: u64) -> Image {
    let mut rnd = Random::new(seed);
    let mut im = Image::new(64, 32, rgb(50, 20, 26));
    for _ in 0..5 {
        let x = rnd.randrange(0, 56) as i32;
        let y = rnd.randrange(0, 24) as i32;
        let c = rnd.choice(&[[64, 28, 34], [38, 22, 34], [70, 44, 40], [30, 24, 28]]);
        let x1 = x + rnd.randint(10, 24) as i32;
        let y1 = y + rnd.randint(8, 16) as i32;
        im.rectangle(x, y, x1, y1, Some(rgb(c[0], c[1], c[2])), None);
    }
    let thread = rgb(10, 6, 8);
    for x in [16.0, 33.0, 49.0] {
        let x1 = x + rnd.randint(-1, 1) as f64;
        im.line(&[(x, 0.0), (x1, 31.0)], thread, 1);
        for y in (1..31).step_by(3) {
            im.line(&[(x - 2.0, y as f64), (x + 2.0, y as f64 + 2.0)], thread, 1);
        }
    }
    for y in [10.0, 22.0] {
        im.line(&[(0.0, y), (63.0, y)], thread, 1);
    }
    for (cx, cy) in [(9, 6), (25, 16), (41, 5), (56, 14), (8, 26), (33, 27), (51, 26)] {
        im.ellipse(cx - 5, cy - 3, cx + 5, cy + 3, Some(rgb(226, 218, 190)), Some(rgb(60, 10, 12)), 1);
        im.ellipse(cx - 2, cy - 2, cx + 2, cy + 2, Some(rgb(190, 40, 24)), None, 1);
        im.line(&[(cx as f64, cy as f64 - 3.0), (cx as f64, cy as f64 + 3.0)], rgb(6, 2, 2), 1);
        im.put(cx - 3, cy - 1, rgb(160, 20, 20));
    }
    im
}

/// A three sided thorn: its base middle, its direction, length and base radius.
pub fn thorn(s: &mut Soup, mat: usize, base: V3, direction: V3, length: f64, radius: f64) {
    let dn = norm(direction);
    let a = if dn[1].abs() < 0.95 { norm(cross(dn, v3(0.0, 1.0, 0.0))) } else { v3(1.0, 0.0, 0.0) };
    let b = cross(dn, a);
    let ring: Vec<V3> = (0..3)
        .map(|k| {
            let ang = 2.0 * PI * k as f64 / 3.0;
            add(base, mul(add(mul(a, ang.cos()), mul(b, ang.sin())), radius))
        })
        .collect();
    let apex = add(base, mul(dn, length));
    for k in 0..3 {
        let (mut p, mut q) = (ring[k], ring[(k + 1) % 3]);
        let sum = add(add(p, q), apex);
        let mid = [sum[0] / 3.0, sum[1] / 3.0, sum[2] / 3.0];
        let out = sub(mid, base);
        if dot3(cross(sub(q, p), sub(apex, p)), out) < 0.0 {
            std::mem::swap(&mut p, &mut q);
        }
        s.tri([p, q, apex], [[0.0, 1.0], [1.0, 1.0], [0.5, 0.0]], mat, [None; 3]);
    }
}

// A thorny vine along a line of points, its radius going from r0 to r1, with thorns on the way.
#[allow(clippy::too_many_arguments)]
fn vine(s: &mut Soup, mat: usize, tmat: usize, rnd: &mut Random, pts: &[V3], r0: f64, r1: f64, thorns: usize, thorn_len: f64) {
    let n = pts.len();
    let radii: Vec<f64> = (0..n).map(|i| r0 + (r1 - r0) * i as f64 / (n - 1) as f64).collect();
    mk::tube(s, pts, &radii, 4, mat, Tube { v_scale: 2.0, cap_end: true, cap_start: true, u_repeat: 1.0 });
    for i in 0..thorns {
        let t = (i as f64 + rnd.uniform(0.2, 0.8)) / thorns as f64 * (n - 1) as f64;
        let k = (t as usize).min(n - 2);
        let f = t - k as f64;
        let p = add(mul(pts[k], 1.0 - f), mul(pts[k + 1], f));
        let side = rnd.choice(&[-1.0, 1.0]);
        let jx = rnd.uniform(-0.7, 0.7);
        let jy = rnd.uniform(-0.7, 0.7);
        let d = add(mul([0.0, 0.0, 1.0], 0.7), mul([jx, jy, 0.0], side));
        let l = thorn_len * rnd.uniform(0.7, 1.2);
        thorn(s, tmat, p, d, l, 2.2);
    }
}

// The closed box behind a gate: the front picture on +z, a plain texture on the other faces.
fn core(s: &mut Soup, lo: V3, hi: V3, front: usize, side: usize) {
    let sd = Some(side);
    mk::box_(s, lo, hi, [sd, sd, sd, sd, Some(front), sd], Some(front));
}

/// Bone bars and iron crossbars over a wall of glowing runes.
pub fn build_gate_atl() -> Soup {
    let seed = 11;
    let mut rnd = Random::new(seed);
    let mut s = Soup::new();
    let (w, h, t) = (70.0, 112.0, 10.0);
    let front = s.material(runes(5), true, 1.0);
    let side = s.material(bark(seed), false, BRIGHT);
    let bn = s.material(bone(seed + 1), false, BRIGHT);
    let ir = s.material(rust(seed + 2), false, BRIGHT);
    core(&mut s, [-w, 0.0, -t], [w, h, t], front, side);
    let zf = t + 4.0;
    for i in 0..7 {
        let x = -60.0 + 20.0 * i as f64 + rnd.uniform(-1.5, 1.5);
        let x1 = x + rnd.uniform(-1.0, 1.0);
        let z1 = zf + rnd.uniform(-1.0, 1.0);
        let x2 = x + rnd.uniform(-1.5, 1.5);
        let pts = [[x, -2.0, zf], [x1, h * 0.5, z1], [x2, h + 2.0, zf]];
        mk::tube(&mut s, &pts, &[3.6, 5.0, 3.4], 5, bn, Tube { v_scale: 2.0, cap_end: true, cap_start: true, u_repeat: 1.0 });
        mk::tube(&mut s, &[[x, h, zf], [x, h + 3.0, zf]], &[6.0, 4.0], 5, bn, Tube::default());
    }
    for y in [14.0, 56.0, 98.0] {
        mk::box1(&mut s, [-w - 3.0, y - 3.0, t], [w + 3.0, y + 3.0, t + 4.0], ir);
    }
    for (k, y0) in [22.0, 62.0, 90.0].into_iter().enumerate() {
        let pts: Vec<V3> = (0..9)
            .map(|j| {
                let x = -w + 2.0 + (2.0 * w - 4.0) * j as f64 / 8.0;
                [x, y0 + 14.0 * (j as f64 * 1.3 + k as f64 * 2.0).sin(), zf + if j % 2 == 0 { 9.0 } else { -2.0 }]
            })
            .collect();
        vine(&mut s, side, ir, &mut rnd, &pts, 2.8, 2.0, 8, 9.0);
    }
    s
}

/// A stitched curtain with eyes behind thorn vines.
pub fn build_gate_cave() -> Soup {
    let seed = 21;
    let mut rnd = Random::new(seed);
    let mut s = Soup::new();
    let (w, h, t) = (75.0, 114.0, 10.0);
    let front = s.material(curtain(7), true, 1.0);
    let side = s.material(bark(seed), false, BRIGHT);
    let ir = s.material(rust(seed + 2), false, BRIGHT);
    core(&mut s, [-w, 0.0, -t], [w, h, t], front, side);
    let zf = t + 3.0;
    for k in 0..6 {
        let x0 = -w + 12.0 + k as f64 * 26.0;
        let sgn = if k % 2 == 0 { 1.0 } else { -1.0 };
        let pts: Vec<V3> = (0..8)
            .map(|j| {
                let f = j as f64 / 7.0;
                [
                    x0 + sgn * 34.0 * (f - 0.5) + 6.0 * (j as f64 * 1.7 + k as f64).sin(),
                    4.0 + (h - 8.0) * f,
                    zf + if j % 2 == 0 { 5.0 } else { -1.5 },
                ]
            })
            .collect();
        vine(&mut s, side, ir, &mut rnd, &pts, 3.0, 2.2, 7, 9.0);
    }
    for (k, y) in [18.0, 60.0, 100.0].into_iter().enumerate() {
        let pts: Vec<V3> = (0..7)
            .map(|j| {
                [
                    -w + 2.0 + (2.0 * w - 4.0) * j as f64 / 6.0,
                    y + 8.0 * (j as f64 * 1.9 + k as f64 * 2.3).sin(),
                    zf + if j % 2 != 0 { 6.0 } else { 0.0 },
                ]
            })
            .collect();
        vine(&mut s, side, ir, &mut rnd, &pts, 3.2, 2.4, 8, 10.0);
    }
    s
}

/// A clump of black thorns, 40 across and 34 high.
pub fn build_thorns() -> Soup {
    let seed = 31;
    let mut rnd = Random::new(seed);
    let mut s = Soup::new();
    let bk = s.material(bark(seed), false, BRIGHT);
    for k in 0..8 {
        let a = 2.0 * PI * k as f64 / 8.0 + rnd.uniform(-0.3, 0.3);
        let r = if k == 0 { 0.0 } else { rnd.uniform(6.0, 15.0) };
        let lean = if k == 0 { 0.0 } else { rnd.uniform(0.25, 0.7) };
        let h = if k == 0 { 34.0 } else { rnd.uniform(18.0, 30.0) };
        let d = [a.cos() * lean, 1.0, a.sin() * lean];
        let rad = rnd.uniform(3.5, 5.0);
        thorn(&mut s, bk, [r * a.cos(), 0.0, r * a.sin()], d, h, rad);
    }
    s
}
