//! The path tool (P): click the ground to lay a path, then place garibs or copies of the selection along it. The
//! path is only a guide in the editor. The level keeps what's placed. Each placed thing is dropped onto the
//! ground. Also: placing the pieces of an imported model.

use super::{Editor, Event, PATH_COPY, PATH_MAX};
use crate::commands::ensure_cmd;
use crate::entities::{attached_points, preset_zone, EntId, CAM_PRESETS};
use crate::scene::{sphere_geometry, LineMode, Lines, Material, RayOptions};
use crate::vec::{self, V3};
use crate::xmlutil::{fmt, num0, r1};
use glovebox_core::backend::CatalogEntry;
use glovebox_core::kit::rng::Random;
use glovebox_core::pyfmt::py_round;
use glovebox_core::xml::{Doc, NodeId};
use std::f64::consts::PI;
use std::sync::Arc;

const YELLOW: [f64; 3] = [1.0, 0.82, 0.25];

/// The settings of `scatter_along_path`.
#[derive(Clone, Debug, PartialEq)]
pub struct Scatter {
    /// The least and the most distance between two copies along the path.
    pub spacing: (f64, f64),
    /// The sides of the path: 1 is the right of the way on, -1 the left.
    pub sides: Vec<f64>,
    /// How far from the path.
    pub dist: (f64, f64),
    /// How far along the path a copy may shift.
    pub jitter: f64,
    /// Up or down from the ground.
    pub dy: (f64, f64),
    pub scale: (f64, f64),
    /// "free" (any way), "path" (the way on) or "player" (toward the path).
    pub turn: String,
    /// The most tilt about x and z, in radians.
    pub tilt: f64,
    /// Copies are LandActor (with collision), not BackgroundActor.
    pub solid: bool,
    pub seed: i64,
    /// One for each selected model. None gives every model the same weight.
    pub weights: Option<Vec<f64>>,
}

impl Default for Scatter {
    fn default() -> Scatter {
        Scatter {
            spacing: (80.0, 170.0),
            sides: vec![1.0, -1.0],
            dist: (180.0, 420.0),
            jitter: 30.0,
            dy: (-25.0, 15.0),
            scale: (0.8, 1.2),
            turn: "free".to_string(),
            tilt: 0.0,
            solid: false,
            seed: 0,
            weights: None,
        }
    }
}

/// Rounds to `digits` decimals, by the exact value of the double. A half goes to the even neighbor.
fn round_to(x: f64, digits: usize) -> f64 {
    if !x.is_finite() {
        return x;
    }
    format!("{:.*}", digits, x).parse().unwrap_or(x)
}

/// The remainder of `a / b` with the sign of `b`.
fn py_mod(a: f64, b: f64) -> f64 {
    let r = a % b;
    if r != 0.0 && (r < 0.0) != (b < 0.0) {
        r + b
    } else {
        r
    }
}

/// A random index, each with its weight (CPython's `random.choices(range(n), weights)[0]`).
fn choice(rnd: &mut Random, weights: &[f64]) -> usize {
    let mut cum = Vec::with_capacity(weights.len());
    let mut total = 0.0;
    for w in weights {
        total += w;
        cum.push(total);
    }
    let x = rnd.random() * (total + 0.0);
    let hi = weights.len().saturating_sub(1);
    // bisect_right within the first `hi` entries
    cum[..hi].partition_point(|c| *c <= x)
}

/// Moves a copied object, with its script and path points, by `d`.
pub fn shift_node(doc: &mut Doc, n: NodeId, d: V3) {
    if doc.has(n, "x") {
        for (i, a) in ["x", "y", "z"].iter().enumerate() {
            let v = fmt(num0(doc, n, a) + d[i]);
            doc.set(n, a, &v);
        }
    }
    for b in attached_points(doc, n, None) {
        let v = vec::add(b.get(doc, &[]), d);
        b.set(doc, &[], v);
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

impl Editor {
    pub fn set_path_mode(&mut self, on: bool) {
        self.path.on = on;
        if on && self.box_mode {
            self.set_box_mode(false);
        }
        self.draw_path();
        self.emit(Event::ModeChanged);
    }

    pub fn path_length(&self) -> f64 {
        let p = &self.path.pts;
        let mut total = 0.0;
        for i in 1..p.len() {
            total += vec::norm(vec::sub(p[i], p[i - 1]));
        }
        total
    }

    /// Builds the path's line and balls again.
    pub fn draw_path(&mut self) {
        self.graph.delete(self.path_node);
        let n = self.graph.node("path");
        self.graph[n].order = 10;
        self.graph[n].visible = self.path.on && !self.path.pts.is_empty();
        if !self.path.pts.is_empty() {
            let lift: Vec<V3> = self.path.pts.iter().map(|p| vec::add(*p, [0.0, 3.0, 0.0])).collect();
            let mut l = Lines::from_f64(&lift, YELLOW, LineMode::Strip);
            l.depth_test = false;
            let ln = self.graph.lines_node(l);
            self.graph[ln].order = 10;
            self.graph.add(n, ln);
            let ball = Arc::new(sphere_geometry(5.0, 12, 8));
            for p in lift {
                let mat = Material { color: YELLOW, lit: false, depth_test: false, ..Material::default() };
                let m = self.graph.mesh_node(ball.clone(), Arc::new(mat));
                self.graph[m].order = 10;
                self.graph[m].pos = p;
                self.graph.add(n, m);
            }
        }
        self.path_node = n;
        self.emit(Event::ViewChanged);
    }

    pub fn path_info(&self) -> String {
        let n = self.path.pts.len();
        if n == 0 {
            return "no points yet".to_string();
        }
        format!("{} point{} \u{b7} {} long", n, plural(n), py_round(self.path_length()) as i64)
    }

    /// A click in the view with the path tool on: adds the ground point under the pixel to the path.
    pub fn path_click(&mut self, x: f64, y: f64) {
        let hit = self.camera.ray(x, y).and_then(|(origin, d)| {
            let o = RayOptions { meshes_only: true, ..RayOptions::default() };
            self.graph.raycast(&self.ground_objects(None, false), origin, d, &o).first().map(|h| h.point)
        });
        match hit {
            Some(p) => {
                self.path.pts.push(p);
                self.draw_path();
            }
            None => self.set_status("The path goes on the ground: click a landscape piece or a platform.", "err"),
        }
    }

    /// Points every `step` along the path, with the way on at each (a unit vector in x/z).
    pub fn path_samples(&self, step: f64) -> Vec<(V3, V3)> {
        let pts = &self.path.pts;
        let mut out = Vec::new();
        if pts.len() < 2 {
            return pts.first().map(|p| vec![(*p, [0.0, 0.0, 1.0])]).unwrap_or_default();
        }
        if !(step > 0.0) {
            return out;
        }
        let (mut nxt, mut run) = (0.0, 0.0);
        for i in 1..pts.len() {
            let (a, b) = (pts[i - 1], pts[i]);
            let ab = vec::sub(b, a);
            let ln = vec::norm(ab);
            let d = [b[0] - a[0], 0.0, b[2] - a[2]];
            let dn = vec::norm(d);
            let d = vec::div(d, if dn != 0.0 { dn } else { 1.0 });
            while nxt <= run + ln + 1e-6 {
                let t = if ln != 0.0 { (nxt - run) / ln } else { 0.0 };
                out.push((vec::add(a, vec::scale(ab, t)), d));
                nxt += step;
            }
            run += ln;
        }
        out
    }

    /// Places garibs (`what` "garib") or copies of the selection along the path. `side`: 1 right of the way on,
    /// -1 left, 2 both, 0 on the path. `off`: how far to the side. `up`: over the ground. `turn`: a random turn
    /// about y for each copy.
    #[allow(clippy::too_many_arguments)]
    pub fn place_along_path(&mut self, what: &str, garib_type: &str, step: f64, side: f64, off: f64, up: f64, turn: bool) {
        if self.path.pts.is_empty() {
            return self.set_status("Click the ground to lay the path first.", "err");
        }
        let step = if step == 0.0 || step.is_nan() { 60.0 } else { step };
        let step = if step > 10.0 { step } else { 10.0 };
        let mut spots: Vec<V3> = Vec::new();
        for (p, d) in self.path_samples(step) {
            let right = [-d[2], 0.0, d[0]]; // the right of the way on
            let sides: &[f64] = if side == 2.0 { &[1.0, -1.0] } else { &[side] };
            for s in sides {
                spots.push(vec::add(p, vec::scale(vec::scale(right, *s), off)));
            }
        }
        if spots.len() > PATH_MAX {
            let msg = format!("That makes {} objects. Place at most {} at a time: space them further apart.", spots.len(), PATH_MAX);
            return self.set_status(&msg, "err");
        }
        let ground = self.ground_objects(None, false);
        let settle = |ed: &Editor, q: V3| ed.ground_below(vec::add(q, [0.0, 120.0, 0.0]), 600.0, None, Some(&ground)).unwrap_or(q);
        if what == "garib" {
            let mut count = 0;
            self.mutate(|ed| {
                let group = ed.garib_group();
                let mut added = Vec::new();
                for q in &spots {
                    let g = settle(ed, *q);
                    let e = ed.make_garib([g[0], g[1] + up, g[2]], garib_type);
                    ed.doc.append(group, e);
                    added.push(e);
                }
                count = added.len();
                Some(added)
            });
            let msg = format!("Placed {} {}{} along the path in a new garib group.", count, garib_type.replacen('_', " ", 1), plural(count));
            return self.set_status(&msg, "ok");
        }
        let mut src: Vec<NodeId> = Vec::new();
        for &i in &self.selection {
            let e = &self.entities[i];
            if self.whole(i) && PATH_COPY.contains(&e.tag.as_str()) && !src.contains(&e.node) {
                src.push(e.node);
            }
        }
        if src.is_empty() {
            return self.set_status("Select what to copy first: scenery, a kit model, an enemy, a powerup or Mr. Tip.", "err");
        }
        let ents: Vec<EntId> = src.iter().filter_map(|n| (0..self.entities.len()).find(|&i| self.entities[i].node == *n && self.whole(i))).collect();
        let center = self.center_of(&ents);
        let under = self.ground_below(vec::add(center, [0.0, 120.0, 0.0]), 600.0, None, Some(&ground));
        let anchor = under.unwrap_or_else(|| {
            let mut low = f64::INFINITY;
            for &i in &ents {
                let y = self.bind_get(i)[1];
                if y < low {
                    low = y;
                }
            }
            [center[0], low, center[2]]
        });
        let do_turn = turn && src.len() == 1 && ents.first().is_some_and(|&i| self.entities[i].rot == Some("xyz"));
        let mut rnd = Random::new(clock_seed());
        let mut count = 0;
        self.mutate(|ed| {
            let mut added = Vec::new();
            for q in &spots {
                let g = settle(ed, *q);
                let d = [g[0] - anchor[0], g[1] + up - anchor[1], g[2] - anchor[2]];
                for n in &src {
                    let c = ed.doc.deep_copy(*n);
                    if ed.doc.tag(c) == "Platform" {
                        if let Some(t) = ed.doc.first(c, "PlatSetTag") {
                            ed.doc.detach(t);
                        }
                    }
                    shift_node(&mut ed.doc, c, d);
                    if do_turn {
                        let r = ensure_cmd(&mut ed.doc, c, "SetActorRotation", &[]);
                        for a in ["x", "z"] {
                            if ed.doc.get(r, a).is_none_or(|v| v.is_empty()) {
                                ed.doc.set(r, a, "0.0");
                            }
                        }
                        ed.doc.set(r, "y", &fmt(rnd.random() * PI * 2.0));
                    }
                    added.push(ed.insert_top(c));
                }
            }
            count = added.len();
            Some(added)
        });
        let mut msg = format!(
            "Placed {} cop{} of the selection along the path ({} objects).",
            spots.len(),
            if spots.len() == 1 { "y" } else { "ies" },
            count
        );
        if turn && !do_turn {
            msg += " Random turn needs one selected model that can turn.";
        }
        self.set_status(&msg, "ok");
    }

    /// Camera zones over a stretch of the path (`start` and `end` are fractions of its length): boxes around the
    /// path, each split off at most `split` long. The box is level with the axes, so a diagonal stretch gets a
    /// bigger box: split it. The usual values are 0 and 1 for `start` and `end`, 140 for `margin`, 300 for `below`,
    /// 400 for `above` and 600 for `split`.
    #[allow(clippy::too_many_arguments)]
    pub fn path_zones(&mut self, key: &str, start: f64, end: f64, margin: f64, below: f64, above: f64, split: f64) {
        let samples = self.path_samples(20.0);
        if samples.len() < 2 {
            return self.set_status("Lay a path of at least two points first.", "err");
        }
        let total = 20.0 * (samples.len() - 1) as f64;
        let part: Vec<(V3, V3)> = samples
            .into_iter()
            .enumerate()
            .filter(|(i, _)| start * total - 1e-6 <= *i as f64 * 20.0 && *i as f64 * 20.0 <= end * total + 1e-6)
            .map(|(_, q)| q)
            .collect();
        if part.len() < 2 {
            return self.set_status("That stretch of the path is too short.", "err");
        }
        let Some((_, label, note)) = CAM_PRESETS.iter().find(|c| c.0 == key) else { return };
        let long = 20.0 * (part.len() - 1) as f64;
        let n = if split > 0.0 { ((long / split).ceil() as usize).max(1) } else { 1 };
        let size = (part.len() as f64 / n as f64).ceil() as usize;
        let chunks: Vec<&[(V3, V3)]> = (0..part.len() - 1).step_by(size.max(1)).map(|i| &part[i..(i + size + 1).min(part.len())]).collect();
        let mut count = 0;
        self.mutate(|ed| {
            let mut made = Vec::new();
            for ch in &chunks {
                let mut lo = [f64::INFINITY; 3];
                let mut hi = [f64::NEG_INFINITY; 3];
                for (p, _) in ch.iter() {
                    for k in 0..3 {
                        if p[k] < lo[k] {
                            lo[k] = p[k];
                        }
                        if p[k] > hi[k] {
                            hi[k] = p[k];
                        }
                    }
                }
                let (x0, x1) = (lo[0] - margin, hi[0] + margin);
                let (y0, y1) = (lo[1] - below, hi[1] + above);
                let (z0, z1) = (lo[2] - margin, hi[2] + margin);
                let mid = ch[ch.len() / 2];
                let facing = mid.1[0].atan2(mid.1[2]);
                let center = if key == "fixed" { ch[ch.len() - 1].0 } else { vec::add(mid.0, [0.0, 100.0, 0.0]) };
                let Some(zone) = preset_zone(key, facing, center) else { continue };
                made.push(ed.make_zone([x0, y0, z0], [x1 - x0, y1 - y0, z1 - z0], &zone));
            }
            count = made.len();
            Some(made)
        });
        let msg = format!(
            "Added {} {} camera zone{} over {} of the path. While Glover is inside a box the camera {}; it goes back when he leaves.",
            count,
            label.to_lowercase(),
            plural(count),
            py_round(long) as i64,
            note
        );
        self.set_status(&msg, "ok");
    }

    /// Scatters copies of the selected scenery beside the path at random spacing, distance, height, scale and
    /// turn. The same seed gives the same result.
    pub fn scatter_along_path(&mut self, o: &Scatter) {
        if self.path.pts.len() < 2 {
            return self.set_status("Lay a path of at least two points first.", "err");
        }
        let mut src: Vec<NodeId> = Vec::new();
        for &i in &self.selection {
            let e = &self.entities[i];
            if self.whole(i) && matches!(e.tag.as_str(), "LandActor" | "BackgroundActor") && !src.contains(&e.node) {
                src.push(e.node);
            }
        }
        if src.is_empty() {
            return self.set_status("Select the scenery to scatter first: landscape pieces or kit models.", "err");
        }
        if !(5.0 <= o.spacing.0 && o.spacing.0 <= o.spacing.1) {
            return self.set_status("The spacing must be at least 5, with the first number no bigger than the second.", "err");
        }
        let mut weights = o.weights.clone().filter(|w| !w.is_empty()).unwrap_or_else(|| vec![1.0; src.len()]);
        weights.resize(src.len(), 1.0);
        let mut rnd = Random::new(o.seed.unsigned_abs());
        let samples = self.path_samples(5.0);
        // (place, model, turn, scale, up, tilt about x, tilt about z)
        let mut spots: Vec<(V3, usize, f64, f64, f64, f64, f64)> = Vec::new();
        'sides: for &side in &o.sides {
            let mut at = rnd.uniform(o.spacing.0, o.spacing.1) / 2.0;
            while at / 5.0 < samples.len() as f64 {
                let (p, d) = samples[(samples.len() - 1).min((at / 5.0) as usize)];
                let right = [-d[2], 0.0, d[0]];
                let a = vec::scale(vec::scale(right, side), rnd.uniform(o.dist.0, o.dist.1));
                let b = vec::scale(d, rnd.uniform(-o.jitter, o.jitter));
                let q = vec::add(vec::add(p, a), b);
                let pick = choice(&mut rnd, &weights);
                let yaw = match o.turn.as_str() {
                    "path" => d[0].atan2(d[2]),
                    "player" => (-right[0] * side).atan2(-right[2] * side),
                    _ => rnd.uniform(0.0, 2.0 * PI),
                };
                let sc = rnd.uniform(o.scale.0, o.scale.1);
                let up = rnd.uniform(o.dy.0, o.dy.1);
                let tx = rnd.uniform(-o.tilt, o.tilt);
                let tz = rnd.uniform(-o.tilt, o.tilt);
                spots.push((q, pick, yaw, sc, up, tx, tz));
                at += rnd.uniform(o.spacing.0, o.spacing.1);
                if spots.len() > PATH_MAX {
                    break 'sides;
                }
            }
        }
        if spots.len() > PATH_MAX {
            let msg = format!("That makes {} objects. Place at most {} at a time: space them further apart.", spots.len(), PATH_MAX);
            return self.set_status(&msg, "err");
        }
        let ground = self.ground_objects(None, false);
        let mut added: Vec<NodeId> = Vec::new();
        self.mutate(|ed| {
            for (q, pick, yaw, sc, up, tx, tz) in &spots {
                let g = ed.ground_below(vec::add(*q, [0.0, 120.0, 0.0]), 600.0, None, Some(&ground)).unwrap_or(*q);
                let n = ed.doc.deep_copy(src[(*pick).min(src.len() - 1)]);
                ed.doc.set_tag(n, if o.solid { "LandActor" } else { "BackgroundActor" });
                for (i, a) in ["x", "y", "z"].iter().enumerate() {
                    ed.doc.set(n, a, &r1(g[i] + if *a == "y" { *up } else { 0.0 }));
                }
                for c in ed.doc.children(n).to_vec() {
                    if matches!(ed.doc.tag(c), "SetActorRotation" | "SetActorScale") {
                        ed.doc.detach(c);
                    }
                }
                let r = ensure_cmd(&mut ed.doc, n, "SetActorRotation", &[]);
                ed.doc.set(r, "x", &fmt(round_to(*tx, 3)));
                ed.doc.set(r, "y", &fmt(round_to(py_mod(*yaw, 2.0 * PI), 3)));
                ed.doc.set(r, "z", &fmt(round_to(*tz, 3)));
                if (sc - 1.0).abs() > 0.005 {
                    let s = ensure_cmd(&mut ed.doc, n, "SetActorScale", &[]);
                    for a in ["x", "y", "z"] {
                        ed.doc.set(s, a, &fmt(round_to(*sc, 2)));
                    }
                }
                added.push(ed.insert_top(n));
            }
            Some(added.clone())
        });
        let msg = format!(
            "Scattered {} copies of {} model{} beside the path (seed {}).{}",
            added.len(),
            src.len(),
            plural(src.len()),
            o.seed,
            self.copy_cost(&added)
        );
        self.set_status(&msg, "ok");
    }

    /// " About N triangles and N KB of heap." for new scenery.
    pub fn copy_cost(&self, nodes: &[NodeId]) -> String {
        if nodes.is_empty() {
            return String::new();
        }
        let b: i64 = nodes.iter().map(|n| self.actor_cost(&self.doc, *n).0).sum();
        let f: i64 = nodes.iter().map(|n| self.faces(&self.doc, *n)).sum();
        format!(" About {} triangles and {} KB of heap.", f, py_round(b as f64 / 1024.0) as i64)
    }

    // -----------------------------------------------------------------------------------------------------------
    // Imported models: a .glb from another game or a 3D program, which the back end converts into Glover models
    // and keeps in the user folder. A large model comes as several pieces, each with its offset.

    /// The pieces of an imported model: (object id, catalog entry).
    pub fn custom_pieces(&self, set: &str) -> Vec<(String, CatalogEntry)> {
        self.catalog.iter().filter(|(_, c)| c.custom.as_ref().is_some_and(|x| x.set == set)).map(|(i, c)| (i.clone(), c.clone())).collect()
    }

    /// Adds every piece of an imported model at the placement point (or `p`), each at its offset, in one undo
    /// step.
    pub fn place_custom(&mut self, set: &str, as_: &str, p: Option<V3>) {
        let pieces = self.custom_pieces(set);
        let Some(first) = pieces.first() else { return };
        let p = p.unwrap_or_else(|| self.placement_point());
        self.mutate(|ed| {
            let mut nodes = Vec::new();
            for (i, c) in &pieces {
                let Some(x) = &c.custom else { continue };
                let name: String = x.piece.chars().take(7).collect();
                let o = x.offset;
                let n = ed.doc.make(
                    as_,
                    &[("object_id", i), ("name", &name), ("x", &r1(p[0] + o[0])), ("y", &r1(p[1] + o[1])), ("z", &r1(p[2] + o[2]))],
                );
                nodes.push(ed.insert_top(n));
            }
            Some(nodes)
        });
        let kb = pieces.iter().map(|(_, c)| c.bytes).sum::<u64>() as f64 / 1024.0;
        let msg = format!(
            "Added {} as {} {} piece{} ({:.0} KB). Build ROM copies {} into {}.",
            first.1.bank.replace("Custom: ", ""),
            pieces.len(),
            as_,
            plural(pieces.len()),
            kb,
            if pieces.len() == 1 { "it" } else { "them" },
            self.scene_str("importObjectBank")
        );
        self.set_status(&msg, "ok");
    }
}

/// A seed that differs from call to call, for the turns that need not repeat.
fn clock_seed() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0)
}
