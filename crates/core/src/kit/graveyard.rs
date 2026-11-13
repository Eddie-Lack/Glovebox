//! The kit's graveyard props: gravestone, pumpkin, candles, coffin, fence, dead trees and cobweb. The order in
//! which the random numbers are drawn is fixed: it decides each model's shape.

use super::image::{Image, Rgba};
use super::meshkit::{self as mk, add, mul, norm, v3, Lathe, Place, Soup, Tube, Uv, V3, BRIGHT};
use super::rng::Random;

const PI: f64 = std::f64::consts::PI;

fn rgb(r: i64, g: i64, b: i64) -> Rgba {
    [r as u8, g as u8, b as u8, 255]
}

fn pts(p: &[(i32, i32)]) -> Vec<(f64, f64)> {
    p.iter().map(|&(x, y)| (x as f64, y as f64)).collect()
}

// ---------------------------------------------------------------------------------------------
// Gravestone

/// The front of a gravestone (32x64): stone, a border, a crack, the words and blood drips.
pub fn grave_front(lines: &[String], seed: u64) -> Image {
    let mut rnd = Random::new(seed);
    let mut im = mk::stone(&mut rnd, 32, 64, [96, 104, 98], true);
    im.rectangle(2, 2, 29, 61, None, Some(rgb(70, 76, 72)));
    let mut x = 4 + (seed % 6) as i64;
    for y in 40..62 {
        x += rnd.choice(&[-1, 0, 1]);
        im.put(x as i32, y, rgb(30, 32, 30));
    }
    let y0 = if lines.len() > 3 { 9.0 } else { 14.0 };
    let line_h = if lines.len() <= 4 { 10.0 } else { 9.0 };
    mk::text_centered(&mut im, lines, y0, line_h, rgb(30, 32, 30), 7);
    for _ in 0..3 {
        let x0 = rnd.randrange(4, 28);
        let y0 = rnd.randrange(46, 52);
        for y in y0..y0 + rnd.randrange(5, 14) {
            im.put(x0 as i32, y as i32, rgb(120, 14, 10));
        }
    }
    im
}

/// A crooked gravestone with words, leaning by `lean` degrees.
pub fn stone_soup(lines: &[String], lean: f64, seed: u64) -> Soup {
    let mut s = Soup::new();
    let fr = s.material(grave_front(lines, seed), true, BRIGHT);
    let bk = s.material(mk::stone(&mut Random::new(seed + 50), 32, 32, [96, 104, 98], true), false, BRIGHT);
    let (w, h, t) = (21.0, 40.0, 6.0);
    let mut prof: Vec<(f64, f64)> = vec![(-w, 0.0), (w, 0.0)];
    for k in 0..8 {
        prof.push((w * (PI * k as f64 / 7.0).cos(), h + w * 0.8 * (PI * k as f64 / 7.0).sin()));
    }
    let top = h + w * 0.8;
    let uv = |x: f64, y: f64| -> Uv { [(x + w) / (2.0 * w), 1.0 - y / top] };
    let (cx, cy) = (0.0, h * 0.5);
    let n = prof.len();
    let (fwd, bwd) = (Some(v3(0.0, 0.0, 1.0)), Some(v3(0.0, 0.0, -1.0)));
    for i in 0..n {
        let ((x0, y0), (x1, y1)) = (prof[i], prof[(i + 1) % n]);
        s.tri([v3(cx, cy, t / 2.0), v3(x0, y0, t / 2.0), v3(x1, y1, t / 2.0)], [uv(cx, cy), uv(x0, y0), uv(x1, y1)], fr, [fwd; 3]);
        s.tri([v3(cx, cy, -t / 2.0), v3(x1, y1, -t / 2.0), v3(x0, y0, -t / 2.0)], [[0.5, 0.5]; 3], bk, [bwd; 3]);
        let nrm = Some(norm(v3(y1 - y0, -(x1 - x0), 0.0)));
        s.quad(
            [v3(x0, y0, t / 2.0), v3(x1, y1, t / 2.0), v3(x1, y1, -t / 2.0), v3(x0, y0, -t / 2.0)],
            [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            bk,
            [nrm; 4],
        );
    }
    let soil = s.material(mk::noise(16, 16, &mut Random::new(seed + 9), -10, 10, [44, 34, 26]), false, BRIGHT);
    mk::lathe(&mut s, &[(30.0, -4.0), (14.0, 9.0), (0.0, 11.0)], 7, soil, Lathe::default());
    let mut out = Soup::new();
    out.add(&s, Place { tilt: (0.0, lean.to_radians()), ..Place::default() }, false);
    out
}

// ---------------------------------------------------------------------------------------------
// Pumpkin

/// The carved face (64x32), lit from inside. `kind` is 0 to 7.
pub fn pumpkin_face(kind: usize) -> Image {
    let (w, h) = (64usize, 32usize);
    let mut rnd = Random::new(11 + kind as u64);
    let mut im = mk::noise(w, h, &mut rnd, -14, 10, [205, 90, 12]);
    for x in (0..w).step_by(4) {
        im.line(&[(x as f64, 0.0), (x as f64, h as f64)], rgb(150, 60, 8), 1);
    }
    let (glow, hot, dark) = (rgb(255, 214, 60), rgb(255, 250, 170), rgb(60, 20, 4));
    match kind {
        0 => {
            for cx in [18, 46] {
                im.polygon(&pts(&[(cx - 6, 17), (cx + 6, 17), (cx, 7)]), dark);
                im.polygon(&pts(&[(cx - 4, 16), (cx + 4, 16), (cx, 9)]), glow);
            }
            im.polygon(&pts(&[(32, 17), (29, 21), (35, 21)]), glow);
            let mouth = [(10, 22), (16, 25), (20, 23), (26, 27), (32, 24), (38, 27), (44, 23), (48, 25), (54, 22), (50, 29), (42, 31), (22, 31), (14, 29)];
            im.polygon(&pts(&mouth), dark);
            let inner = [(13, 24), (17, 26), (21, 25), (26, 28), (32, 26), (38, 28), (43, 25), (47, 26), (51, 24), (48, 29), (41, 30), (23, 30), (16, 28)];
            im.polygon(&pts(&inner), glow);
            for x in [19, 27, 37, 45] {
                im.rectangle(x, 24, x + 2, 27, Some(dark), None);
            }
            im.rectangle(30, 26, 34, 29, Some(hot), None);
        }
        1 => {
            for cx in [17, 47] {
                im.ellipse(cx - 5, 6, cx + 5, 18, Some(dark), None, 1);
                im.ellipse(cx - 3, 8, cx + 3, 16, Some(glow), None, 1);
                im.ellipse(cx - 1, 10, cx + 1, 14, Some(hot), None, 1);
            }
            im.ellipse(21, 19, 43, 32, Some(dark), None, 1);
            im.ellipse(24, 21, 40, 31, Some(glow), None, 1);
            im.ellipse(28, 23, 36, 30, Some(hot), None, 1);
        }
        3 => {
            for cx in [19, 45] {
                let (a, b) = if cx < 32 { (6, 4) } else { (-6, -4) };
                im.polygon(&pts(&[(cx - 7, 9), (cx + 7, 9), (cx + a, 17)]), dark);
                im.polygon(&pts(&[(cx - 5, 10), (cx + 5, 10), (cx + b, 15)]), glow);
            }
            for cx in [17.0, 47.0] {
                im.line(&[(cx, 17.0), (cx - 1.0, 26.0)], glow, 1);
            }
            im.arc(20, 22, 44, 36, 200.0, 340.0, dark, 2);
            im.arc(22, 24, 42, 36, 210.0, 330.0, glow, 1);
        }
        4 => {
            for cx in [20, 44] {
                im.ellipse(cx - 4, 5, cx + 4, 13, Some(dark), None, 1);
                im.ellipse(cx - 2, 7, cx + 2, 11, Some(hot), None, 1);
            }
            im.ellipse(12, 14, 52, 32, Some(dark), None, 1);
            im.ellipse(15, 16, 49, 31, Some(glow), None, 1);
            for x in (15..50).step_by(4) {
                let x = x as f64;
                im.polygon(&[(x, 16.0), (x + 3.0, 16.0), (x + 1.5, 21.0)], dark);
                im.polygon(&[(x + 2.0, 31.0), (x + 5.0, 31.0), (x + 3.5, 26.0)], dark);
            }
        }
        5 => {
            for cx in [18, 46] {
                im.line(&pts(&[(cx - 7, 12), (cx + 7, 12)]), dark, 2);
                for x in (cx - 6..cx + 7).step_by(3) {
                    im.line(&pts(&[(x, 9), (x + 1, 15)]), dark, 1);
                }
            }
            im.polygon(&pts(&[(4, 18), (14, 26), (32, 29), (50, 26), (60, 18), (52, 29), (32, 32), (12, 29)]), dark);
            im.polygon(&pts(&[(8, 20), (16, 27), (32, 30), (48, 27), (56, 20), (50, 28), (32, 31), (14, 28)]), glow);
        }
        6 => {
            im.rectangle(22, 14, 42, 30, Some(dark), None);
            im.rectangle(24, 16, 40, 29, Some(glow), None);
            for x in [22, 28, 34, 40] {
                im.rectangle(x - 1, 3, x + 3, 15, Some(dark), None);
                im.rectangle(x, 5, x + 2, 15, Some(glow), None);
            }
            im.polygon(&pts(&[(42, 20), (52, 14), (54, 17), (44, 24)]), dark);
        }
        7 => {
            for (cx, cy) in [(14, 8), (24, 13), (20, 21), (44, 9), (40, 17), (50, 22)] {
                im.ellipse(cx - 3, cy - 3, cx + 3, cy + 3, Some(dark), None, 1);
                im.ellipse(cx - 1, cy - 1, cx + 1, cy + 1, Some(hot), None, 1);
            }
            im.line(&pts(&[(24, 28), (40, 28)]), dark, 2);
        }
        _ => {
            im.ellipse(22, 5, 42, 23, Some(dark), None, 1);
            im.ellipse(25, 8, 39, 20, Some(glow), None, 1);
            im.ellipse(30, 11, 34, 17, Some(rgb(120, 20, 0)), None, 1);
            for x in (8..58).step_by(5) {
                im.line(&pts(&[(x, 26), (x + 3, 29)]), dark, 1);
                im.line(&pts(&[(x + 3, 26), (x, 29)]), dark, 1);
            }
        }
    }
    im
}

/// A jack-o'-lantern, 46 across. `kind` is the face, 0 to 7.
pub fn build_pumpkin(kind: usize) -> Soup {
    let mut s = Soup::new();
    let face = s.material(pumpkin_face(kind), true, BRIGHT);
    let stem = s.material(mk::noise(16, 16, &mut Random::new(3), -12, 12, [70, 90, 40]), false, BRIGHT);
    let (r0, hh) = (23.0, 19.0);
    let ribs = 4.0;
    let body = |u: f64, v: f64| -> V3 {
        let a = 2.0 * PI * u;
        let t = PI * v;
        let mut r = r0 * t.sin() * (1.0 + 0.09 * (ribs * a).cos()) * (1.0 + 0.04 * (t.sin() * t.sin()));
        if v > 0.93 {
            r *= 1.0 - (v - 0.93) * 4.0;
        }
        let y = hh - hh * t.cos() - if v < 0.93 { 0.0 } else { (v - 0.93) * 12.0 * 1.0 };
        [r * a.cos(), y, r * a.sin()]
    };
    let uv = |u: f64, v: f64| -> Uv { [(u * 360.0 - 30.0) / 120.0, 1.0 - v] };
    mk::param(&mut s, &body, 12, 6, face, &uv, true);
    let stem_pts = [[0.0, hh * 2.0 - 6.0, 0.0], [0.0, hh * 2.0 + 2.0, 0.0], [2.0, hh * 2.0 + 8.0, 1.0], [6.0, hh * 2.0 + 12.0, 2.0]];
    mk::tube(&mut s, &stem_pts, &[4.5, 3.6, 3.0, 2.0], 6, stem, Tube::default());
    s
}

// ---------------------------------------------------------------------------------------------
// Candles

/// Three melted candles with flames and a pool of wax.
pub fn build_candles() -> Soup {
    let mut s = Soup::new();
    let w = s.material(mk::noise(8, 16, &mut Random::new(21), -12, 8, [214, 200, 160]), false, 1.0);
    let mut flame = Image::new(8, 16, rgb(255, 190, 40));
    flame.rectangle(2, 4, 5, 15, Some(rgb(255, 235, 110)), None);
    flame.rectangle(3, 9, 4, 15, Some(rgb(255, 255, 220)), None);
    let f = s.material(flame, false, 1.0);
    let pool = s.material(mk::noise(8, 8, &mut Random::new(5), -8, 6, [200, 188, 150]), false, BRIGHT);
    mk::lathe(&mut s, &[(0.0, 0.2), (13.0, 0.2), (0.0, 1.8)], 6, pool, Lathe::default());
    for (x, z, h, r) in [(-6.0, 2.0, 16.0, 3.2), (5.0, -3.0, 11.0, 3.0), (3.0, 7.0, 7.0, 2.6)] {
        let mut p = Soup::new();
        mk::lathe(&mut p, &[(r + 1.5, 0.0), (r, 5.0), (r + 0.6, h * 0.7), (r - 0.2, h), (0.0, h - 0.8)], 5, w, Lathe::default());
        s.add(&p, Place { offset: [x, 1.0, z], ..Place::default() }, true);
        let mut fl = Soup::new();
        mk::lathe(&mut fl, &[(0.0, 0.0), (1.5, 2.5), (0.0, 7.5)], 4, f, Lathe::default());
        s.add(&fl, Place { offset: [x, h + 0.6, z], ..Place::default() }, true);
    }
    s
}

// ---------------------------------------------------------------------------------------------
// Coffin

const COFFIN: [(f64, f64); 6] = [(-9.0, -6.0), (9.0, -6.0), (15.0, 14.0), (8.0, 62.0), (-8.0, 62.0), (-15.0, 14.0)];

/// A coffin with its lid pushed aside.
pub fn build_coffin() -> Soup {
    let ajar: f64 = 12.0;
    let mut rnd = Random::new(1);
    let mut s = Soup::new();
    let w = s.material(mk::wood(&mut rnd, 16, 32, [58, 40, 28]), false, BRIGHT);
    let mut lid_tex = mk::wood(&mut Random::new(13), 32, 32, [66, 44, 30]);
    lid_tex.rectangle(14, 6, 17, 26, Some(rgb(150, 130, 90)), None);
    lid_tex.rectangle(9, 11, 22, 14, Some(rgb(150, 130, 90)), None);
    let lt = s.material(lid_tex, false, BRIGHT);
    let mut inside = mk::noise(16, 16, &mut Random::new(12), -4, 3, [10, 8, 10]);
    inside.put(5, 7, rgb(255, 40, 20));
    inside.put(10, 7, rgb(255, 40, 20));
    let ins = s.material(inside, true, BRIGHT);
    let top_uv = |x: f64, z: f64| -> Uv { [(x + 15.0) / 30.0, (z + 6.0) / 68.0] };
    mk::prism(&mut s, &COFFIN, 0.0, 16.0, w, ins, Some(w), Some(&top_uv));
    let mut lid = Soup::new();
    let poly: Vec<(f64, f64)> = COFFIN.iter().map(|&(x, z)| (x * 1.04, z * 1.02 + 0.0)).collect();
    mk::prism(&mut lid, &poly, 0.0, 3.0, w, lt, Some(w), Some(&top_uv));
    s.add(&lid, Place { offset: [ajar * 0.6, 16.5, ajar * 1.2], yaw: -(ajar * 0.9).to_radians(), ..Place::default() }, true);
    s
}

// ---------------------------------------------------------------------------------------------
// Fence

/// A broken fence, 150 long along x.
pub fn build_fence() -> Soup {
    let length = 150.0;
    let mut rnd = Random::new(2);
    let mut s = Soup::new();
    let w = s.material(mk::wood(&mut rnd, 16, 32, [74, 52, 36]), false, BRIGHT);
    let posts = 4;
    let xs: Vec<f64> = (0..posts).map(|i| -length / 2.0 + length * i as f64 / (posts - 1) as f64).collect();
    for &x in &xs {
        let mut p = Soup::new();
        let h = rnd.uniform(40.0, 58.0);
        mk::box1(&mut p, [-3.0, 0.0, -3.0], [3.0, h, 3.0], w);
        let yaw = rnd.uniform(-0.3, 0.3);
        let t0 = rnd.uniform(-0.12, 0.12);
        let t1 = rnd.uniform(-0.18, 0.18);
        s.add(&p, Place { offset: [x, -4.0, 0.0], yaw, tilt: (t0, t1), ..Place::default() }, true);
    }
    for (y, wob) in [(18.0, 0.02), (36.0, -0.05)] {
        for i in 0..posts - 1 {
            if rnd.random() < 0.25 {
                continue;
            }
            let mut r = Soup::new();
            let l = xs[i + 1] - xs[i];
            mk::box1(&mut r, [-l / 2.0, -2.0, -1.2], [l / 2.0, 2.0, 1.2], w);
            let oy = y + rnd.uniform(-3.0, 3.0);
            let t1 = wob + rnd.uniform(-0.1, 0.1);
            s.add(&r, Place { offset: [(xs[i] + xs[i + 1]) / 2.0, oy, 4.0], tilt: (0.0, t1), ..Place::default() }, true);
        }
    }
    for i in 0..7 {
        let x = -length / 2.0 + 8.0 + i as f64 * (length - 16.0) / 6.0;
        if rnd.random() < 0.28 {
            continue;
        }
        let mut pk = Soup::new();
        let h = rnd.uniform(28.0, 46.0);
        mk::box1(&mut pk, [-3.0, 0.0, -0.8], [3.0, h, 0.8], w);
        mk::prism(&mut pk, &[(-3.0, 0.8), (3.0, 0.8), (0.0, 0.8)], h, h + 1.0, w, w, None, None);
        let oy = 4.0 + rnd.uniform(-4.0, 6.0);
        let t1 = rnd.uniform(-0.25, 0.25);
        s.add(&pk, Place { offset: [x, oy, 6.0], tilt: (0.0, t1), ..Place::default() }, true);
    }
    s
}

// ---------------------------------------------------------------------------------------------
// Dead trees

fn tree_bark(seed: u64) -> Image {
    let mut rnd = Random::new(seed);
    let mut im = mk::noise(16, 32, &mut rnd, -16, 12, [62, 54, 48]);
    for x in (0..16).step_by(3) {
        let x0 = x as i64 + rnd.randint(0, 1);
        let x1 = x as i64 + rnd.randint(0, 2);
        im.line(&[(x0 as f64, 0.0), (x1 as f64, 31.0)], rgb(30, 26, 24), 1);
    }
    im
}

// One branch as a tube of three parts that curls, then its children.
#[allow(clippy::too_many_arguments)]
fn branch(s: &mut Soup, mat: usize, start: V3, direction: V3, length: f64, radius: f64, depth: u32, rnd: &mut Random) {
    let mut d = norm(direction);
    let mut pts = vec![start];
    let mut p = start;
    let seg = length / 3.0;
    for _ in 0..3 {
        let jx = rnd.uniform(-0.35, 0.35);
        let jy = rnd.uniform(-0.1, 0.25);
        let jz = rnd.uniform(-0.35, 0.35);
        d = norm(add(d, [jx, jy, jz]));
        p = add(p, mul(d, seg));
        pts.push(p);
    }
    let rad = [radius, radius * 0.78, radius * 0.55, (radius * 0.25).max(0.6)];
    mk::tube(s, &pts, &rad, 3, mat, Tube { v_scale: (length / 40.0).max(0.5), ..Tube::default() });
    if depth > 0 {
        for _ in 0..2 {
            let i = rnd.choice(&[1usize, 2]);
            let ang = rnd.uniform(0.0, 2.0 * PI);
            let side = [ang.cos(), rnd.uniform(0.5, 1.4), ang.sin()];
            branch(s, mat, pts[i], norm(add(mul(d, 0.5), side)), length * 0.62, rad[i] * 0.7, depth - 1, rnd);
        }
    }
}

/// A dead tree: a bent trunk, two roots and bare branches.
pub fn build_tree(seed: u64, height: f64, crooked: f64) -> Soup {
    let mut rnd = Random::new(seed);
    let mut s = Soup::new();
    let m = s.material(tree_bark(seed), false, BRIGHT);
    let mut pts: Vec<V3> = vec![[0.0, -6.0, 0.0]];
    let mut p = [0.0, 0.0, 0.0];
    let lx = rnd.uniform(-1.0, 1.0);
    let lz = rnd.uniform(-1.0, 1.0);
    let lean = mul([lx, 0.0, lz], crooked);
    for _ in 1..5 {
        let step = mul(mul(lean, height / 4.0), rnd.uniform(0.4, 1.0));
        p = add(add(p, [0.0, height / 4.0, 0.0]), step);
        pts.push(p);
    }
    mk::tube(&mut s, &pts, &[13.0, 8.5, 6.5, 5.0, 3.2], 5, m, Tube { v_scale: 3.0, ..Tube::default() });
    for k in 0..2 {
        let a = rnd.uniform(0.0, 2.0 * PI) + k as f64 * 3.0;
        let root = [[0.0, 8.0, 0.0], [a.cos() * 14.0, 3.0, a.sin() * 14.0], [a.cos() * 30.0, -2.0, a.sin() * 30.0]];
        mk::tube(&mut s, &root, &[8.0, 5.0, 2.0], 4, m, Tube::default());
    }
    let top = pts[pts.len() - 1];
    for (k, (hf, depth)) in [(0.5, 0), (0.8, 1), (1.0, 1), (0.65, 0)].into_iter().enumerate() {
        let a = 2.0 * PI * (k as f64 / 5.0) + rnd.uniform(-0.3, 0.3);
        let base = if hf < 1.0 { pts[(1 + (hf * 3.0) as usize).min(4)] } else { top };
        let dy = rnd.uniform(0.3, 0.9);
        let l = height * rnd.uniform(0.38, 0.55);
        branch(&mut s, m, base, [a.cos(), dy, a.sin()], l, 4.0, depth, &mut rnd);
    }
    s
}

// ---------------------------------------------------------------------------------------------
// Cobweb

/// A cobweb: white thread on a clear picture (32x32).
pub fn web_tex() -> Image {
    let mut rnd = Random::new(1);
    let mut im = Image::new(32, 32, [0, 0, 0, 0]);
    let white = rgb(214, 214, 220);
    let spokes = 9;
    for k in 0..spokes {
        let a = 2.0 * PI * k as f64 / spokes as f64 + rnd.uniform(-0.1, 0.1);
        im.line(&[(15.5, 15.5), (15.5 + 15.5 * a.cos(), 15.5 + 15.5 * a.sin())], white, 1);
    }
    for r in [4.0, 7.0, 10.0, 13.0] {
        let mut p = Vec::new();
        for k in 0..=spokes {
            let a = 2.0 * PI * k as f64 / spokes as f64;
            let rr = r * rnd.uniform(0.85, 1.0);
            p.push((15.5 + rr * a.cos(), 15.5 + rr * a.sin()));
        }
        im.line(&p, white, 1);
    }
    im
}

/// A flat web, 40 across, drawn on both sides.
pub fn build_web() -> Soup {
    let mut s = Soup::new();
    let m = s.material(web_tex(), true, 1.0);
    let h = 20.0;
    let (a, b, c, d) = (v3(-h, -h, 0.0), v3(h, -h, 0.0), v3(h, h, 0.0), v3(-h, h, 0.0));
    s.quad([a, b, c, d], [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]], m, [None; 4]);
    s.quad([b, a, d, c], [[1.0, 1.0], [0.0, 1.0], [0.0, 0.0], [1.0, 0.0]], m, [Some(v3(0.0, 0.0, -1.0)); 4]);
    s
}
