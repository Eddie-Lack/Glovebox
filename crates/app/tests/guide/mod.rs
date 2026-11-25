//! The pictures of the guide in `docs/guide/`.
//!
//! `GLOVEBOX_ROM=PATH GLOVEBOX_GUIDE=docs/guide/img cargo test -p glovebox --test ui guide_shots` drives the real
//! window on the real game, in the Sage theme, into each state the guide shows and saves a PNG of it. Without both
//! variables the test does nothing.

use super::*;
use std::collections::HashMap;

/// The window's size for the pictures, and its height for a picture of a whole panel.
const WIDE: f32 = 1280.0;
const HIGH: f32 = 800.0;
const TALL: f32 = 1060.0;

/// A picture: RGB bytes, row after row.
#[derive(Clone)]
pub struct Pic {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u8>,
}

/// The digits 1 to 9, 5 wide and 7 high, one bit per pixel.
const DIGITS: [[u8; 7]; 9] = [
    [0x04, 0x0c, 0x04, 0x04, 0x04, 0x04, 0x0e],
    [0x0e, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1f],
    [0x1f, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0e],
    [0x02, 0x06, 0x0a, 0x12, 0x1f, 0x02, 0x02],
    [0x1f, 0x10, 0x1e, 0x01, 0x01, 0x11, 0x0e],
    [0x06, 0x08, 0x10, 0x1e, 0x11, 0x11, 0x0e],
    [0x1f, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
    [0x0e, 0x11, 0x11, 0x0e, 0x11, 0x11, 0x0e],
    [0x0e, 0x11, 0x11, 0x0f, 0x01, 0x02, 0x0c],
];

impl Pic {
    fn blend(&mut self, x: i64, y: i64, c: [u8; 3], a: f32) {
        if x < 0 || y < 0 || x >= self.w as i64 || y >= self.h as i64 || a <= 0.0 {
            return;
        }
        let i = (y as usize * self.w + x as usize) * 3;
        for k in 0..3 {
            self.px[i + k] = (f32::from(self.px[i + k]) * (1.0 - a) + f32::from(c[k]) * a).round() as u8;
        }
    }

    /// A part of the picture. The rectangle is cut to the picture.
    pub fn crop(&self, r: egui::Rect) -> Pic {
        let x0 = (r.min.x.floor().max(0.0) as usize).min(self.w);
        let y0 = (r.min.y.floor().max(0.0) as usize).min(self.h);
        let x1 = (r.max.x.ceil().max(0.0) as usize).clamp(x0, self.w);
        let y1 = (r.max.y.ceil().max(0.0) as usize).clamp(y0, self.h);
        let mut px = Vec::with_capacity((x1 - x0) * (y1 - y0) * 3);
        for y in y0..y1 {
            px.extend_from_slice(&self.px[(y * self.w + x0) * 3..(y * self.w + x1) * 3]);
        }
        Pic { w: x1 - x0, h: y1 - y0, px }
    }

    /// The picture `n` times smaller: each pixel is the mean of a square of `n` by `n`.
    pub fn smaller(&self, n: usize) -> Pic {
        let (w, h) = (self.w / n, self.h / n);
        let mut px = Vec::with_capacity(w * h * 3);
        for y in 0..h {
            for x in 0..w {
                for k in 0..3 {
                    let mut sum = 0u32;
                    for dy in 0..n {
                        for dx in 0..n {
                            sum += u32::from(self.px[((y * n + dy) * self.w + x * n + dx) * 3 + k]);
                        }
                    }
                    px.push((sum / (n * n) as u32) as u8);
                }
            }
        }
        Pic { w, h, px }
    }

    /// The picture twice as large: each pixel becomes a square of 2 by 2.
    pub fn doubled(&self) -> Pic {
        let mut px = Vec::with_capacity(self.px.len() * 4);
        for y in 0..self.h * 2 {
            for x in 0..self.w * 2 {
                let at = ((y / 2) * self.w + x / 2) * 3;
                px.extend_from_slice(&self.px[at..at + 3]);
            }
        }
        Pic { w: self.w * 2, h: self.h * 2, px }
    }

    /// A round marker with a number from 1 to 9: a disk of the theme's accent with a light ring and digit.
    pub fn marker(&mut self, at: egui::Pos2, n: usize, disk: [u8; 3], ink: [u8; 3]) {
        let r = 13.0f32;
        for y in (at.y - r - 3.0) as i64..=(at.y + r + 3.0) as i64 {
            for x in (at.x - r - 3.0) as i64..=(at.x + r + 3.0) as i64 {
                let d = ((x as f32 + 0.5 - at.x).powi(2) + (y as f32 + 0.5 - at.y).powi(2)).sqrt();
                self.blend(x, y, ink, (r + 2.0 - d).clamp(0.0, 1.0));
                self.blend(x, y, disk, (r - d).clamp(0.0, 1.0));
            }
        }
        let Some(rows) = DIGITS.get(n.wrapping_sub(1)) else { return };
        let s = 2i64;
        let (x0, y0) = (at.x.round() as i64 - 5 * s / 2, at.y.round() as i64 - 7 * s / 2);
        for (j, row) in rows.iter().enumerate() {
            for i in 0..5 {
                if row & (0x10 >> i) != 0 {
                    for dy in 0..s {
                        for dx in 0..s {
                            self.blend(x0 + i * s + dx, y0 + j as i64 * s + dy, ink, 1.0);
                        }
                    }
                }
            }
        }
    }

    /// Several pictures side by side on a ground color, with a gap between them.
    pub fn row(pics: &[Pic], gap: usize, ground: [u8; 3]) -> Pic {
        let w = pics.iter().map(|p| p.w).sum::<usize>() + gap * pics.len().saturating_sub(1);
        let h = pics.iter().map(|p| p.h).max().unwrap_or(0);
        let mut out = Pic { w, h, px: ground.iter().copied().cycle().take(w * h * 3).collect() };
        let mut x0 = 0;
        for p in pics {
            for y in 0..p.h {
                let at = (y * w + x0) * 3;
                out.px[at..at + p.w * 3].copy_from_slice(&p.px[y * p.w * 3..(y + 1) * p.w * 3]);
            }
            x0 += p.w + gap;
        }
        out
    }

    /// Saves the picture as a PNG with a palette of at most 256 colors. The colors the window uses most (its
    /// flat grounds and its text) keep their exact values. The rest, the shades of the 3D view, share what's left.
    pub fn save(&self, path: &std::path::Path) {
        let mut count: HashMap<[u8; 3], u32> = HashMap::new();
        for p in self.px.chunks_exact(3) {
            *count.entry([p[0], p[1], p[2]]).or_default() += 1;
        }
        let mut colors: Vec<([u8; 3], u32)> = count.into_iter().collect();
        colors.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let kept = if colors.len() <= 256 { colors.len() } else { 64 };
        let mut palette: Vec<[u8; 3]> = colors[..kept].iter().map(|c| c.0).collect();
        // Median cut: the box with the longest side is cut in two at the middle of its pixels
        let mut boxes = vec![colors[kept..].to_vec()];
        boxes.retain(|b| !b.is_empty());
        while !boxes.is_empty() && boxes.len() < 256 - kept {
            let span = |b: &[([u8; 3], u32)], k: usize| b.iter().map(|c| c.0[k]).max().unwrap() - b.iter().map(|c| c.0[k]).min().unwrap();
            let Some((at, k)) = (0..boxes.len()).filter(|&i| boxes[i].len() > 1).map(|i| (i, (0..3).max_by_key(|&k| span(&boxes[i], k)).unwrap())).max_by_key(|&(i, k)| span(&boxes[i], k)) else { break };
            let mut b = boxes.swap_remove(at);
            b.sort_by_key(|c| (c.0[k], c.0));
            let total: u64 = b.iter().map(|c| u64::from(c.1)).sum();
            let (mut seen, mut cut) = (0u64, 1);
            for (i, c) in b.iter().enumerate() {
                seen += u64::from(c.1);
                if seen * 2 >= total {
                    cut = (i + 1).clamp(1, b.len() - 1);
                    break;
                }
            }
            let rest = b.split_off(cut);
            boxes.push(b);
            boxes.push(rest);
        }
        for b in &boxes {
            let total: u64 = b.iter().map(|c| u64::from(c.1)).sum();
            let mean = |k: usize| ((b.iter().map(|c| u64::from(c.0[k]) * u64::from(c.1)).sum::<u64>() + total / 2) / total) as u8;
            palette.push([mean(0), mean(1), mean(2)]);
        }
        let near = |c: [u8; 3]| -> u8 {
            let d = |p: &[u8; 3]| (0..3).map(|k| (i32::from(p[k]) - i32::from(c[k])).pow(2)).sum::<i32>();
            (0..palette.len()).min_by_key(|&i| d(&palette[i])).unwrap_or(0) as u8
        };
        let index: HashMap<[u8; 3], u8> = colors.iter().map(|c| (c.0, near(c.0))).collect();
        let data: Vec<u8> = self.px.chunks_exact(3).map(|p| index[&[p[0], p[1], p[2]]]).collect();
        let file = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
        let mut e = png::Encoder::new(file, self.w as u32, self.h as u32);
        e.set_color(png::ColorType::Indexed);
        e.set_depth(png::BitDepth::Eight);
        e.set_palette(palette.concat());
        e.set_compression(png::Compression::High);
        let mut w = e.write_header().unwrap();
        w.write_image_data(&data).unwrap();
        w.finish().unwrap();
    }
}

pub struct G {
    pub t: T,
    pub dir: PathBuf,
    pub saved: Vec<(String, usize, usize, u64)>,
    pub missed: Vec<String>,
}

impl G {
    /// The window as a picture. The pointer leaves the window first: no row is lit and no tooltip is open.
    pub fn pic(&mut self) -> Pic {
        self.t.h.event(egui::Event::PointerGone);
        self.t.settle();
        self.raw()
    }

    /// The window as it is, tooltips and open menus included.
    pub fn raw(&mut self) -> Pic {
        let img = self.t.h.render().expect("a picture of the window");
        let (w, h) = (img.width() as usize, img.height() as usize);
        let px = img.as_raw().chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
        Pic { w, h, px }
    }

    pub fn save(&mut self, name: &str, pic: &Pic) {
        let path = self.dir.join(format!("{name}.png"));
        pic.save(&path);
        let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        println!("picture {name}.png  {}x{}  {} KB", pic.w, pic.h, bytes / 1024);
        self.saved.push((name.to_string(), pic.w, pic.h, bytes));
    }

    /// Saves the whole window.
    pub fn shot(&mut self, name: &str) -> Pic {
        let pic = self.pic();
        self.save(name, &pic);
        pic
    }

    /// Opens a level and waits for it.
    pub fn level(&mut self, sid: &str) {
        self.t.w().load_scene(sid);
        let want = sid.to_string();
        let ok = self.t.until(300, move |w| !w.busy && w.ed.scene_id() == want && w.jobs.running == 0);
        assert!(ok, "level {sid} did not open");
        self.t.idle();
    }

    /// Looks at a point from a distance: `yaw` turns about the point (degrees, 0 looks along -z), `pitch` is how
    /// far the camera is above it.
    pub fn look(&mut self, target: vec::V3, yaw: f64, pitch: f64, dist: f64) {
        let (y, p) = (yaw.to_radians(), pitch.to_radians());
        let dir = [p.cos() * y.sin(), p.sin(), p.cos() * y.cos()];
        let ed = self.t.ed();
        ed.orbit.target = target;
        ed.camera.pos = vec::add(target, vec::scale(dir, dist));
        ed.camera.look_at(target);
        self.t.w().view.dirty = true;
        self.t.settle();
    }

    /// Waits for the pictures of the Add palette.
    pub fn thumbs(&mut self) {
        for _ in 0..4 {
            self.t.settle();
            self.t.until(60, |w| w.side_jobs.running == 0 && w.palette.thumbs_wanted.is_empty());
        }
    }

    /// The rectangle of the first widget with this label.
    pub fn rect_of(&self, label: &str) -> Option<egui::Rect> {
        self.t.find(label, true).first().map(|n| n.rect())
    }
}


impl G {
    fn size(&self) -> egui::Vec2 {
        self.t.h.ctx.content_rect().size()
    }

    fn view(&mut self) -> egui::Rect {
        self.t.w().view.rect
    }

    /// The panel on the right of the view: the inspector, the Add palette or the Path panel, over the checks.
    fn right(&mut self) -> egui::Rect {
        let (v, s) = (self.view(), self.size());
        egui::Rect::from_min_max(egui::pos2(v.max.x, v.min.y), egui::pos2(s.x, v.max.y))
    }

    /// Where the Checks panel begins.
    fn checks_top(&mut self) -> f32 {
        let v = self.view();
        self.rect_of("CHECKS").map_or(v.max.y, |r| r.min.y - 8.0)
    }

    /// The right panel without the checks.
    fn inspector(&mut self) -> egui::Rect {
        let (mut r, top) = (self.right(), self.checks_top());
        r.max.y = top;
        r
    }

    fn checks(&mut self) -> egui::Rect {
        let (mut r, top) = (self.right(), self.checks_top());
        r.min.y = top;
        r
    }

    /// The open dialog: it's in the middle of the window, from over its first line to under its buttons.
    fn dialog(&mut self, first: &str, last: &str) -> egui::Rect {
        let s = self.size();
        let top = self.t.find(first, false).iter().map(|n| n.rect().min.y).fold(f32::INFINITY, f32::min);
        let bottom = self.t.find(last, true).iter().map(|n| n.rect().max.y).fold(0.0, f32::max);
        egui::Rect::from_min_max(egui::pos2(s.x / 2.0 - 290.0, top - 52.0), egui::pos2(s.x / 2.0 + 290.0, bottom + 26.0))
    }

    /// Brings a title of the right panel to the top of what shows.
    fn scroll_to(&mut self, label: &str) {
        if let Some(n) = self.t.find(label, false).first() {
            n.scroll_to_me();
        }
        for _ in 0..6 {
            self.t.h.step();
        }
    }

    /// A taller window, for a picture of a whole panel.
    fn tall(&mut self, on: bool) {
        let size = egui::vec2(WIDE, if on { TALL } else { HIGH });
        if self.size() != size {
            self.t.h.set_size(size);
            self.t.settle();
            self.t.w().view.dirty = true;
            self.t.settle();
        }
    }

    /// Turns the wheel over a place in the window.
    fn wheel(&mut self, at: egui::Pos2, dy: f32) {
        self.t.h.event(egui::Event::PointerMoved(at));
        self.t.h.step();
        self.t.h.event(egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: egui::vec2(0.0, dy), phase: egui::TouchPhase::Move, source: egui::MouseWheelSource::Wheel, modifiers: Modifiers::default() });
        for _ in 0..12 {
            self.t.h.step();
        }
    }

    fn click(&mut self, label: &str) {
        if !self.t.click(label) {
            self.missed.push(label.to_string());
        }
    }

    fn find_ent(&mut self, what: &str, pred: impl Fn(&Editor, EntId) -> bool) -> EntId {
        let ed = self.t.ed();
        (0..ed.entities.len()).find(|&i| pred(ed, i)).unwrap_or_else(|| panic!("no {what} in the level"))
    }

    fn select(&mut self, i: Option<EntId>) {
        self.t.ed().select(i, false);
        self.t.w().view.dirty = true;
        self.t.settle();
    }

    /// A small picture for the guide's contents list: a part of a picture, 240 by 150.
    fn thumb(&mut self, name: &str, pic: &Pic, at: egui::Pos2, n: usize) {
        let r = egui::Rect::from_min_size(at, egui::vec2(240.0 * n as f32, 150.0 * n as f32));
        let small = pic.crop(r).smaller(n);
        self.save(&format!("thumb-{name}"), &small);
    }
}

impl G {
    /// A small picture of a part of a side panel, 240 by 150. The panel is narrower than two thumbs, so the part is
    /// 360 by 225 and the picture is two thirds of its size.
    fn thumb_panel(&mut self, name: &str, pic: &Pic, at: egui::Pos2) {
        let r = egui::Rect::from_min_size(at, egui::vec2(360.0, 225.0));
        let small = pic.crop(r).doubled().smaller(3);
        self.save(&format!("thumb-{name}"), &small);
    }

    /// A small picture of the key hints, 240 by 150: a part of 600 by 375 at two fifths of its size. The hints are
    /// wider than two thumbs.
    fn thumb_hints(&mut self, name: &str, pic: &Pic, at: egui::Pos2) {
        let r = egui::Rect::from_min_size(at, egui::vec2(600.0, 375.0));
        let small = pic.crop(r).doubled().smaller(5);
        self.save(&format!("thumb-{name}"), &small);
    }
}

const SAGE: [u8; 3] = [0x46, 0x70, 0x52];
const PAPER: [u8; 3] = [0xff, 0xff, 0xff];

pub fn run(rom: PathBuf, dir: PathBuf) {
    std::fs::create_dir_all(&dir).unwrap();
    let home = std::env::temp_dir().join("glovebox-guide").join("Glovebox");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    // The view shows the level without its fog. The notes are on hover, as on a first start, and the settings do
    // not say that the welcome was answered. The panels are in the default layout, and the Add panel shows its list.
    let layout = glovebox::layout::Layout::default();
    std::fs::write(home.join("settings.ini"), format!("[General]\ntheme=sage\nfog=off\ninfo=hover\npanels=\"{}\"\nadd_view=list\n", layout.to_text())).unwrap();
    let mut g = G { t: T::open(home, None, [WIDE, HIGH]), dir, saved: Vec::new(), missed: Vec::new() };
    g.t.settle();

    // Getting started: the setup screen
    let p = g.pic();
    let low = g.rect_of("Choose the ROM…").map_or(340.0, |r| r.max.y + 26.0);
    g.save("setup-screen", &p.crop(egui::Rect::from_min_max(egui::pos2(300.0, 92.0), egui::pos2(980.0, low))));
    g.t.files.0.borrow_mut().open.push_back(rom);
    g.t.h.get_by_label("Choose the ROM…").click();
    g.t.settle();
    assert!(g.t.h.state_mut().window().is_some(), "the setup screen took the ROM");
    g.t.idle();
    assert_eq!(g.t.theme(), "sage");
    assert_eq!(g.t.w().layout, layout, "the guide's pictures start from the default layout");
    welcome(&mut g);
    let only = std::env::var("GLOVEBOX_GUIDE_ONLY").unwrap_or_default();
    let steps: [(&str, fn(&mut G)); 13] = [
        ("start", start),
        ("panels", panels),
        ("view", moving),
        ("select", selecting),
        ("add", adding),
        ("platform", platforms),
        ("enemy", enemies),
        ("puzzle", puzzles_and_zones),
        ("exit", exit_triggers),
        ("path", path_tool),
        ("settings", level_settings),
        ("build", checks_and_builds),
        ("options", options),
    ];
    for (name, step) in steps {
        if only.is_empty() || only.split(',').any(|o| o == name) {
            atlantis(&mut g);
            step(&mut g);
            g.t.key(Key::Escape);
        }
    }
    let total: u64 = g.saved.iter().map(|s| s.3).sum();
    let big: Vec<&str> = g.saved.iter().filter(|s| s.3 > 300 * 1024).map(|s| s.0.as_str()).collect();
    println!("{} pictures, {} KB. Over 300 KB: {big:?}", g.saved.len(), total / 1024);
    assert!(g.missed.is_empty(), "the guide's pictures clicked widgets that are not there: {:?}", g.missed);
    g.t.finish();
}

/// Getting started: the welcome of the first start, and a step of the tour. The other pictures of the guide show
/// the notes in the panels, so the Info texts are `Shown` from here on.
fn welcome(g: &mut G) {
    assert!(g.t.w().tour.welcome, "the first start has the welcome");
    let r = g.dialog("first time", "Show me around").shrink2(egui::vec2(60.0, 0.0));
    let p = g.pic();
    g.save("welcome", &p.crop(r));
    g.click("Show me around");
    g.click("Next");
    g.click("Next");
    assert_eq!(g.t.w().tour.run.as_ref().map(|r| r.step), Some(2));
    g.shot("tour-step");
    g.t.key(Key::Escape);
    assert!(g.t.w().tour.run.is_none());
    g.t.w().set_info(glovebox::style::Info::Shown);
    g.t.settle();
}

/// Atlantis Level 1 as shipped, with nothing selected and every tool off.
fn atlantis(g: &mut G) {
    g.tall(false);
    if g.t.ed().scene_id() != "0a" {
        g.level("0a");
    }
    let w = g.t.w();
    w.dialog = None;
    w.toggle_palette(Some(false));
    w.set_path_mode(false);
    w.set_fly(false);
    w.inspector = glovebox::inspector::State::default();
    let ed = g.t.ed();
    ed.set_box_mode(false);
    ed.set_mode(Mode::Translate);
    if ed.is_edited() {
        ed.reset_level();
    }
    ed.show_far_plane(None, 0.0);
    ed.set_selection(Vec::new());
    g.t.idle();
    g.t.w().set_status("", "");
    beach(g);
}

/// The start of Atlantis 1: the beach, the first enemy and the way on.
fn beach(g: &mut G) {
    g.look([10.0, 60.0, -260.0], 12.0, 24.0, 720.0);
}

/// An empty part of the beach in the middle of the view: new objects land there.
fn sand(g: &mut G) {
    g.look([-70.0, 20.0, 30.0], 18.0, 27.0, 400.0);
}

fn general(g: &mut G) -> EntId {
    g.find_ent("first enemy", |ed, i| ed.entities[i].kind == "enemy" && ed.entities[i].label.starts_with("generalw"))
}

fn start(g: &mut G) {
    // The level list, open
    g.click("Level");
    let p = g.raw();
    g.save("level-list", &p.crop(egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(520.0, 440.0))));
    g.t.key(Key::Escape);
    g.t.settle();
    // The level from above, as a first start shows it: with the notes on hover
    g.look([0.0, 60.0, -1250.0], 8.0, 36.0, 2500.0);
    g.t.w().set_info(glovebox::style::Info::Hover);
    let p = g.shot("first-level");
    let v = g.view();
    g.thumb("getting-started", &p, v.min + egui::vec2(-15.0, 150.0), 2);
    // The numbered window and the key hints, with the notes shown
    g.t.w().set_info(glovebox::style::Info::Shown);
    // The window, with a number on each part
    beach(g);
    let e = general(g);
    g.select(Some(e));
    g.t.w().set_status("Selected generalw (GENERALWU).", "");
    let mut p = g.pic();
    let (v, s) = (g.view(), g.size());
    let (level, ins, checks) = (g.rect_of("Level").map_or(egui::pos2(200.0, 14.0), |r| r.center()), g.inspector(), g.checks());
    let spots = [
        egui::pos2(s.x - 40.0, v.min.y / 2.0),
        egui::pos2(level.x + 100.0, level.y),
        egui::pos2(v.min.x - 60.0, v.min.y + 330.0),
        egui::pos2(v.center().x, v.min.y + 90.0),
        egui::pos2(ins.max.x - 60.0, ins.min.y + 11.0),
        egui::pos2(checks.max.x - 100.0, checks.min.y + 18.0),
        egui::pos2(v.center().x, (v.max.y + s.y) / 2.0),
    ];
    for (i, at) in spots.into_iter().enumerate() {
        p.marker(at, i + 1, SAGE, PAPER);
    }
    g.save("window-tour", &p);
    g.thumb("the-window", &p, egui::pos2(40.0, 25.0), 5);
    // The key hints at the bottom of the view
    g.thumb_hints("keys", &p, egui::pos2(v.min.x + 8.0, v.max.y - 372.0));
}

/// The window: the panels in other places, with the Panels menu open. The layout goes back to the default after.
fn panels(g: &mut G) {
    beach(g);
    let e = general(g);
    g.select(Some(e));
    g.click("Checks panel menu");
    g.click("Move to bottom");
    g.click("Level panel menu");
    g.click("Move to right");
    g.click("Inspector");
    g.click("Inspector panel menu");
    g.click("Move to left");
    g.t.w().set_status("Selected generalw (GENERALWU).", "");
    g.click("Options");
    g.click("Panels");
    let p = g.raw();
    g.save("window-panels", &p);
    let s = g.size();
    g.t.click_at(egui::pos2(s.x / 2.0, s.y - 5.0));
    // A panel's own menu
    g.click("Level panel menu");
    let p = g.raw();
    let b = g.rect_of("Level panel menu").unwrap_or(egui::Rect::NOTHING);
    g.save("panel-menu", &p.crop(egui::Rect::from_min_max(egui::pos2(b.max.x - 262.0, b.min.y - 8.0), egui::pos2(b.max.x + 8.0, b.max.y + 86.0))));
    g.t.click_at(egui::pos2(s.x / 2.0, s.y - 5.0));
    g.t.w().set_layout(glovebox::layout::Layout::default());
    g.select(None);
    g.t.settle();
    assert_eq!(g.t.w().layout, glovebox::layout::Layout::default());
}

fn moving(g: &mut G) {
    let v = g.view();
    let p = g.pic().crop(v);
    g.save("view-orbit", &p);
    g.thumb("moving-around", &p, egui::pos2(0.0, 120.0), 2);
    // Focus frames the selection
    let e = general(g);
    g.select(Some(e));
    g.click("Focus");
    let p = g.pic().crop(v);
    g.save("view-focus", &p);
    g.select(None);
    // The fly camera, low over the beach
    g.click("Fly");
    let ed = g.t.ed();
    ed.camera.pos = [-150.0, 95.0, 190.0];
    ed.camera.look_at([40.0, 70.0, -330.0]);
    g.t.w().view.dirty = true;
    let p = g.pic().crop(v);
    g.save("view-fly", &p);
    g.t.key(Key::Escape);
}

fn selecting(g: &mut G) {
    let e = general(g);
    let at = g.t.ed().obj_pos(e);
    g.look(vec::add(at, [0.0, 15.0, 0.0]), 25.0, 24.0, 330.0);
    g.select(Some(e));
    let v = g.view();
    let p = g.shot("select-enemy");
    g.thumb("selecting", &p, v.min + egui::vec2(-15.0, 160.0), 2);
    // The three gizmos, side by side
    let mut three = Vec::new();
    for mode in ["Move", "Rotate", "Scale"] {
        g.click(mode);
        let c = px(g.t.ed(), at);
        let c = v.min + egui::vec2(c.0 as f32, c.1 as f32 - 20.0);
        three.push(g.pic().crop(egui::Rect::from_center_size(c, egui::vec2(260.0, 260.0))));
    }
    g.save("gizmo-move-rotate-scale", &Pic::row(&three, 8, PAPER));
    g.click("Move");
    // The edit row of the toolbar
    let p = g.pic().crop(egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(g.size().x, v.min.y / 2.0 + 1.0)));
    g.save("toolbar-edit-row", &p);
    // The Transform card of one object
    let ins = g.inspector();
    let p = g.pic().crop(egui::Rect::from_min_max(ins.min, egui::pos2(ins.max.x, ins.min.y + 262.0)));
    g.save("inspector-transform", &p);
    // A box select, half drawn
    g.select(None);
    g.look([20.0, 40.0, -60.0], 0.0, 40.0, 520.0);
    g.click("Box select");
    let (a, b) = ((f64::from(v.width()) * 0.16, f64::from(v.height()) * 0.30), (f64::from(v.width()) * 0.84, f64::from(v.height()) * 0.74));
    g.t.mouse_move(a);
    g.t.mouse_button(a, true, Modifiers::default());
    g.t.mouse_move(((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0));
    g.t.mouse_move(b);
    g.t.settle();
    let p = g.raw().crop(egui::Rect::from_min_max(v.min + egui::vec2(0.0, 110.0), v.max - egui::vec2(0.0, 110.0)));
    g.save("box-select", &p);
    g.t.mouse_button(b, false, Modifiers::default());
    g.t.settle();
    g.t.ed().set_box_mode(false);
    // Several objects: the inspector moves, turns and scales them together
    println!("the box selected {} objects", g.t.ed().selection.len());
    g.tall(true);
    let ins = g.inspector();
    let low = g.t.find("Pivot: ", false).first().map_or(ins.max.y, |n| n.rect().max.y + 16.0);
    let p = g.pic().crop(egui::Rect::from_min_max(ins.min, egui::pos2(ins.max.x, low.min(ins.max.y))));
    g.save("inspector-many", &p);
}

fn adding(g: &mut G) {
    sand(g);
    g.click("+ Add");
    g.tall(true);
    for tab in glovebox::palette::TABS {
        g.click(tab);
        g.thumbs();
        let r = g.inspector();
        let p = g.pic().crop(r);
        g.save(&format!("add-{}", tab.to_lowercase()), &p);
        if tab == "Enemies" {
            g.thumb_panel("adding", &p, egui::pos2(10.0, 30.0));
        }
    }
    // The grid view: pictures only
    g.click("Enemies");
    g.click("Grid");
    for (tab, name) in [("Enemies", "add-grid-enemies"), ("Models", "add-grid-models")] {
        g.click(tab);
        g.thumbs();
        let r = g.inspector();
        let p = g.pic().crop(egui::Rect::from_min_max(r.min, egui::pos2(r.max.x, r.min.y + 470.0)));
        g.save(name, &p);
    }
    g.click("List");
    g.tall(false);
    // A new enemy on the beach
    g.click("Enemies");
    g.thumbs();
    g.click("Add mallet");
    g.t.idle();
    g.thumbs();
    let e = g.t.ed().selected().expect("the new enemy is selected");
    let at = g.t.ed().obj_pos(e);
    g.look(vec::add(at, [0.0, 15.0, 0.0]), 18.0, 24.0, 280.0);
    g.shot("add-enemy-placed");
}

fn platforms(g: &mut G) {
    sand(g);
    g.click("+ Add");
    g.click("Presets");
    g.thumbs();
    g.click("Add Lift");
    g.t.idle();
    g.t.w().toggle_palette(Some(false));
    g.t.settle();
    let p = g.shot("platform-lift");
    let _ = p;
    // The cards of its commands
    g.tall(true);
    let ins = g.inspector();
    let p = g.pic().crop(ins);
    g.save("platform-commands", &p);
    g.thumb_panel("platforms", &p, egui::pos2(10.0, 90.0));
    // The + menu of the platform's card
    g.click("+");
    // The menu is wider than the panel: the picture starts where the menu does
    let left = g.t.find("<PlatDestructible>", false).iter().map(|n| n.rect().min.x - 14.0).fold(ins.min.x, f32::min);
    let p = g.raw().crop(egui::Rect::from_min_max(egui::pos2(left, ins.min.y), ins.max));
    g.save("platform-add-command", &p);
    g.t.key(Key::Escape);
}

fn enemies(g: &mut G) {
    let e = general(g);
    g.select(Some(e));
    g.tall(true);
    let ins = g.inspector();
    g.scroll_to("ATTACK");
    g.wheel(ins.center(), -400.0);
    let p = g.pic();
    let top = g.rect_of("ENEMY SCRIPT").map_or(ins.min.y, |r| r.min.y - 10.0);
    let part = egui::Rect::from_min_max(egui::pos2(ins.min.x, top.max(ins.min.y)), ins.max);
    g.save("enemy-script", &p.crop(part));
    g.thumb_panel("enemies", &p, part.min + egui::vec2(10.0, 60.0));
    g.scroll_to("ENEMY SCRIPT");
    g.click("Ready-made");
    let p = g.raw();
    let menu = g.t.find("Stand still", false).first().map_or(ins, |n| n.rect());
    let top = g.rect_of("Ready-made").map_or(ins.min.y, |r| r.min.y - 40.0);
    g.save("enemy-ready-made", &p.crop(egui::Rect::from_min_max(egui::pos2(ins.min.x, top), egui::pos2(ins.max.x, menu.max.y + 60.0))));
    g.t.key(Key::Escape);
}

fn puzzles_and_zones(g: &mut G) {
    // The Lift preset comes with a puzzle and its trigger volume
    sand(g);
    g.t.ed().add_preset("lift");
    g.t.idle();
    let t = g.find_ent("trigger volume", |ed, i| ed.entities[i].kind == "trigger" && !ed.entities[i].zone);
    g.select(Some(t));
    let p = g.shot("puzzle-trigger");
    let v = g.view();
    g.thumb("puzzles", &p, v.min + egui::vec2(-15.0, 230.0), 2);
    g.click("Edit its puzzle (conditions and actions)");
    g.tall(true);
    let ins = g.inspector();
    let p = g.pic().crop(ins);
    g.save("puzzle-inspector", &p);
    // The list of puzzles, with nothing selected
    g.click("← Level settings and all puzzles");
    g.select(None);
    g.scroll_to("PUZZLES (");
    g.wheel(ins.center(), -600.0);
    let top = g.t.find("PUZZLES (", false).first().map_or(ins.min.y, |n| n.rect().min.y - 10.0);
    let p = g.pic().crop(egui::Rect::from_min_max(egui::pos2(ins.min.x, top), ins.max));
    g.save("puzzle-list", &p);
    g.tall(false);
    // A new camera zone on the beach
    g.t.ed().undo();
    g.t.idle();
    sand(g);
    g.click("+ Add");
    g.click("Camera");
    g.click("Add High");
    g.t.idle();
    g.t.w().toggle_palette(Some(false));
    g.t.settle();
    let z = g.t.ed().selected().expect("the new zone is selected");
    let (c, size) = (g.t.ed().obj_pos(z), g.t.ed().obj_box(z).size());
    println!("the new camera zone is {size:?} at {c:?}");
    g.look(c, 50.0, 28.0, vec::norm(size) * 1.5);
    g.shot("camera-zone");
    g.tall(true);
    g.scroll_to("Edit its puzzle");
    let ins = g.inspector();
    let top = g.t.find("CAMERA ZONE", false).first().map_or(ins.min.y, |n| n.rect().min.y - 8.0);
    let low = g.t.find("Edit its puzzle", false).first().map_or(ins.max.y, |n| n.rect().min.y - 6.0);
    let p = g.pic().crop(egui::Rect::from_min_max(egui::pos2(ins.min.x, top.max(ins.min.y)), egui::pos2(ins.max.x, low.min(ins.max.y))));
    g.save("camera-zone-inspector", &p);
}

fn exit_triggers(g: &mut G) {
    let x = g.find_ent("exit", |ed, i| ed.exit_trigger(i).is_some());
    let c = g.t.ed().exit_trigger(x).unwrap().center;
    g.select(Some(x));
    g.look(vec::add(c, [0.0, 10.0, 0.0]), 14.0, 16.0, 330.0);
    g.scroll_to("Glover then spins");
    let p = g.shot("exit-trigger");
    let v = g.view();
    g.thumb("exits", &p, v.min + egui::vec2(-15.0, 200.0), 2);
    let ins = g.inspector();
    let top = g.rect_of("Exit trigger").map_or(ins.min.y, |r| r.min.y - 8.0);
    g.save("exit-trigger-inspector", &p.crop(egui::Rect::from_min_max(egui::pos2(ins.min.x, top.max(ins.min.y)), ins.max)));
}

fn path_tool(g: &mut G) {
    g.look([10.0, 20.0, -130.0], 8.0, 40.0, 620.0);
    g.click("Path");
    let v = g.view();
    let none = Modifiers::default();
    for (x, y) in [(0.26, 0.86), (0.66, 0.74), (0.30, 0.62), (0.68, 0.50), (0.46, 0.40)] {
        g.t.view_click(((f64::from(v.width()) * x).trunc(), (f64::from(v.height()) * y).trunc()), none);
    }
    println!("the path has {} points", g.t.ed().path.pts.len());
    let p = g.pic().crop(v);
    g.save("path-laid", &p);
    g.click("Place");
    g.t.idle();
    let p = g.shot("path-garibs");
    g.thumb("path", &p, v.min + egui::vec2(-15.0, 260.0), 2);
    g.tall(true);
    let r = g.inspector();
    g.wheel(r.center(), 2000.0);
    let p = g.pic().crop(r);
    g.save("path-panel", &p);
}

fn level_settings(g: &mut G) {
    g.tall(true);
    let ins = g.inspector();
    let part = ins;
    let p = g.pic();
    g.save("level-settings", &p.crop(part));
    let top = g.rect_of("LEVEL SETTINGS").map_or(ins.min.y, |r| r.min.y - 10.0);
    g.thumb_panel("level-settings", &p, egui::pos2(ins.min.x + 10.0, top));
    g.click("Look");
    let p = g.raw();
    let low = g.t.find("Daylight", false).first().map_or(part.max.y, |n| n.rect().max.y + 40.0);
    g.save("level-settings-look", &p.crop(egui::Rect::from_min_max(part.min, egui::pos2(part.max.x, low))));
    g.t.key(Key::Escape);
}

fn checks_and_builds(g: &mut G) {
    // The draw load row shows the busiest spot and the far plane
    if !g.t.click_containing("Draw load") {
        g.missed.push("Draw load".to_string());
    }
    // A step back: the whole ball of the far plane is in the view
    let ed = g.t.ed();
    let off = vec::sub(ed.camera.pos, ed.orbit.target);
    ed.camera.pos = vec::add(ed.orbit.target, vec::scale(off, 2.6));
    g.t.w().view.dirty = true;
    let p = g.shot("checks-draw-load");
    let r = g.checks();
    let low = g.t.find("Free heap", false).first().map_or(r.max.y, |n| n.rect().max.y + 12.0);
    g.save("checks-panel", &p.crop(egui::Rect::from_min_max(r.min, egui::pos2(r.max.x, low))));
    g.thumb_panel("checks", &p, egui::pos2(r.min.x + 10.0, r.min.y - 60.0));
    g.t.ed().show_far_plane(None, 0.0);
    // The build row of the toolbar
    let v = g.view();
    let p = g.pic().crop(egui::Rect::from_min_max(egui::pos2(0.0, v.min.y / 2.0 - 1.0), egui::pos2(g.size().x, v.min.y)));
    g.save("toolbar-build-row", &p);
    // An edit, then the dialogs of the builds
    sand(g);
    g.t.ed().add_enemy("lionfish");
    g.t.idle();
    g.select(None);
    g.t.w().set_status("", "");
    g.click("Export patch");
    let r = g.dialog("A BPS patch for other players", "Export .bps");
    let p = g.pic();
    g.save("export-patch", &p.crop(r));
    g.thumb("test-build-share", &p, r.min + egui::vec2(50.0, -35.0), 2);
    g.click("Cancel");
    g.click("Build all…");
    let r = g.dialog("Every checked level goes into one ROM", "Cancel");
    let p = g.pic();
    g.save("build-all", &p.crop(r));
    g.click("Cancel");
    g.click("Test level");
    let r = g.dialog("Test level opens the built ROM", "OK");
    let p = g.pic();
    g.save("test-level", &p.crop(r));
    g.t.w().dialog = None;
    g.t.settle();
    // Import model: the dialog on a file with one triangle
    let glb = g.t.home.join("my-model.glb");
    std::fs::write(&glb, tiny_glb()).unwrap();
    g.t.files.0.borrow_mut().open.push_back(glb);
    g.click("+ Add");
    g.click("Models");
    g.click("Import model…");
    g.click("Choose .glb…");
    g.t.idle();
    let r = g.dialog("One .glb file", "Import and place");
    let p = g.pic();
    g.save("import-model", &p.crop(r));
    g.thumb("import-model", &p, r.min + egui::vec2(50.0, -10.0), 2);
    g.t.w().dialog = None;
}

fn options(g: &mut G) {
    g.click("Options");
    let p = g.raw();
    let b = g.rect_of("Options").unwrap_or(egui::Rect::NOTHING);
    let low = g.t.find("Take the tour", true).iter().map(|n| n.rect().max.y).fold(b.max.y, f32::max);
    let r = egui::Rect::from_min_max(egui::pos2(b.min.x - 200.0, 0.0), egui::pos2(g.size().x.min(b.max.x + 70.0), low + 18.0));
    g.save("options-menu", &p.crop(r));
    g.thumb("options", &p, egui::pos2(r.min.x - 60.0, 0.0), 2);
    g.t.key(Key::Escape);
    // The top of the inspector with its info texts, and with them on hover
    g.t.option("Shown");
    let e = general(g);
    g.select(Some(e));
    let ins = g.inspector();
    let part = egui::Rect::from_min_max(ins.min, egui::pos2(ins.max.x, ins.min.y + 262.0));
    let shown = g.pic().crop(part);
    g.save("info-texts-shown", &shown);
    g.t.option("On hover");
    let hover = g.pic().crop(part);
    g.save("info-texts-on-hover", &hover);
    g.t.option("Shown");
    g.select(None);
    // A few themes, small
    for name in ["1998", "Dark", "Sand", "Slate", "Blueprint", "Sage"] {
        g.t.set_theme(name);
        let id = g.t.theme();
        let p = g.pic().smaller(3);
        g.save(&format!("theme-{id}"), &p);
    }
}
