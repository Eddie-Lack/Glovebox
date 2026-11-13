//! The scene nodes of the entities.
//!
//! `entities::build` reads the XML into entities. This module makes the nodes each entity draws, puts them where
//! the XML says (`place_ent`), turns spinning platforms (`animate`) and hides layers (`apply_visibility`).
//!
//! The faint fill of a trigger volume is in the world. It is not pickable and changes no box.
//!
//! A platform with a SetExit also gets the sphere the game tests for the exit (`exit_nodes`). Like a trigger's fill
//! it is in the world next to the entity's node: a click goes through it, and it is in no box.

use super::{Editor, Event};
use crate::entities::{self, camera_zone, water_volume, zone_camera, Binding, EntId, Look, Xform, Zone};
use crate::exits;
use crate::gizmo::cone;
use crate::scene::{
    box_edges, box_geometry, cylinder_geometry, octahedron_geometry, ring_points, sphere_geometry, Geometry, Item, LineMode, Lines,
    Material, Mesh, NodeId, Sprite, Texture,
};
use crate::vec::{self, V3};
use crate::xmlutil::num0;
use std::collections::HashMap;
use std::sync::Arc;

/// The scene nodes of one entity.
#[derive(Clone, Debug)]
pub struct EntNodes {
    /// The node the entity draws and is picked by.
    pub obj: NodeId,
    /// More to draw that is not picked: a water volume, a platform's path, a trigger's fill.
    pub extra: Option<NodeId>,
    /// A camera zone's line to where its camera stands, and whether the zone has one.
    pub marks: Option<NodeId>,
    pub marks_on: bool,
    /// The sphere of a platform's exit trigger, and whether its radius is above 0.
    pub exit: Option<NodeId>,
    pub exit_on: bool,
    /// The object nodes the game draws for the model. None without a model.
    pub draw_nodes: Option<usize>,
    line_group: Option<NodeId>,
    mark_line: Option<NodeId>,
    mark_eye: Option<NodeId>,
    zone: Option<Zone>,
}

impl EntNodes {
    fn new(obj: NodeId) -> EntNodes {
        EntNodes { obj, extra: None, marks: None, marks_on: false, exit: None, exit_on: false, draw_nodes: None, line_group: None, mark_line: None, mark_eye: None, zone: None }
    }
}

const XYZ: [&str; 3] = ["x", "y", "z"];

impl Editor {
    fn shape(&mut self, name: &'static str) -> Arc<Geometry> {
        self.shapes
            .entry(name)
            .or_insert_with(|| {
                Arc::new(match name {
                    "box" => box_geometry(1.0, 1.0, 1.0),
                    "sphere" => sphere_geometry(1.0, 24, 16),
                    "cylinder" => cylinder_geometry(1.0, 1.0, 32),
                    "octa5" => octahedron_geometry(5.0),
                    "ball4" => sphere_geometry(4.0, 12, 8),
                    _ => sphere_geometry(5.0, 12, 8),
                })
            })
            .clone()
    }

    pub(crate) fn lines(&mut self, points: &[V3], color: [f64; 3], mode: LineMode, dash: Option<(f64, f64)>) -> NodeId {
        let mut l = Lines::from_f64(points, color, mode);
        l.dash = dash;
        self.graph.lines_node(l)
    }

    fn fill(&mut self, geometry: Arc<Geometry>, color: &str, opacity: f64) -> NodeId {
        let m = Material { color: entities::hex_rgb(color), lit: false, transparent: true, opacity, depth_write: false, ..Material::default() };
        self.graph.mesh_node(geometry, Arc::new(m))
    }

    fn solid(&mut self, geometry: Arc<Geometry>, color: &str) -> NodeId {
        let m = Material { color: entities::hex_rgb(color), lit: false, ..Material::default() };
        self.graph.mesh_node(geometry, Arc::new(m))
    }

    fn icon_texture(&mut self, glyph: &str, color: &str) -> Arc<Texture> {
        let key = (glyph.to_string(), color.to_string());
        if let Some(t) = self.icon_cache.get(&key) {
            return t.clone();
        }
        let made = self.icon_maker.as_mut().and_then(|f| f(glyph, color));
        let t = match made {
            Some(t) => t,
            // One clear pixel: the sprite keeps its size for picking and boxes
            None => match Texture::new(1, 1, vec![0, 0, 0, 0]) {
                Some(t) => Arc::new(t),
                None => unreachable!("a 1 by 1 texture has 4 bytes"),
            },
        };
        self.icon_cache.insert(key, t.clone());
        t
    }

    /// A sprite node with a glyph.
    pub fn icon(&mut self, glyph: &str, color: &str, size: u32, opacity: f64) -> NodeId {
        let mut s = Sprite::new(self.icon_texture(glyph, color), f64::from(size));
        s.opacity = opacity;
        self.graph.sprite_node(s)
    }

    /// Puts the model of `oid` under `obj`, or a placeholder box when there is none.
    fn attach_model(&mut self, i: EntId, obj: NodeId, oid: Option<&str>, scale: f64, water: bool, decorative: bool) -> Option<usize> {
        let model = oid.and_then(|o| self.load_model(o));
        let Some(model) = model else {
            if !decorative {
                let ph = self.lines(&box_edges(20.0, 20.0, 20.0), entities::hex_rgb("#ff00ff"), LineMode::Segments, None);
                self.graph.add(obj, ph);
                self.entities[i].missing_model = oid.is_some();
            }
            return None;
        };
        let m = model.instance(&mut self.graph);
        self.graph[m].scale = [scale; 3];
        if water {
            // The same see-through material for every mesh that shared one
            let mut seen: HashMap<*const Material, Arc<Material>> = HashMap::new();
            for (n, _) in self.graph.walk(m, None, false) {
                for it in self.graph[n].items.iter_mut() {
                    if let Item::Mesh(mesh) = it {
                        let key = Arc::as_ptr(&mesh.material);
                        let mat = seen.entry(key).or_insert_with(|| {
                            Arc::new(Material { transparent: true, opacity: 0.6, depth_write: false, ..(*mesh.material).clone() })
                        });
                        *mesh = Mesh { geometry: mesh.geometry.clone(), material: mat.clone(), matrix: mesh.matrix };
                    }
                }
            }
        }
        self.graph.add(obj, m);
        Some(model.draw_nodes)
    }

    /// Builds the world again from the entities.
    pub(crate) fn build_nodes(&mut self) {
        self.graph.delete(self.world);
        self.world = self.graph.node("world");
        self.nodes = Vec::with_capacity(self.entities.len());
        for i in 0..self.entities.len() {
            let n = self.make_nodes(i);
            self.nodes.push(n);
        }
        for i in 0..self.entities.len() {
            let e = &self.entities[i];
            let who = if e.derived { e.selects } else { Some(i) };
            let n = &self.nodes[i];
            self.graph[n.obj].entity = who.map(|w| w as u64);
            if let Some(x) = n.extra {
                self.graph[x].entity = Some(i as u64);
            }
            if let Some(x) = n.exit {
                self.graph[x].entity = Some(i as u64);
            }
        }
    }

    fn make_nodes(&mut self, i: EntId) -> EntNodes {
        let e = self.entities[i].clone();
        let world = self.world;
        let out = match &e.look {
            Look::Trigger { color, glyph, .. } => self.trigger_nodes(i, color, glyph),
            Look::WindBox => {
                let obj = self.lines(&box_edges(1.0, 1.0, 1.0), entities::hex_rgb("#9be7e0"), LineMode::Segments, None);
                self.graph.add(world, obj);
                EntNodes::new(obj)
            }
            Look::BuzzerLine => {
                let obj = self.lines(&[[0.0; 3], [0.0; 3]], entities::hex_rgb("#ffd23f"), LineMode::Strip, None);
                self.graph.add(world, obj);
                EntNodes::new(obj)
            }
            Look::Ball | Look::PathPoint => {
                let (shape, color) = if e.look == Look::Ball { ("ball4", "#ffd23f") } else { ("octa5", "#6fb7ff") };
                let g = self.shape(shape);
                let obj = self.solid(g, color);
                self.graph.add(world, obj);
                EntNodes::new(obj)
            }
            Look::Start => {
                // Where Glover starts: a hand and an arrow the way he faces
                let obj = self.graph.node("");
                let arrow = self.graph.node("");
                let shaft = self.lines(&[[0.0, 6.0, 0.0], [0.0, 6.0, 22.0]], entities::hex_rgb("#ffffff"), LineMode::Strip, None);
                self.graph.add(arrow, shaft);
                let tip = cone([0.0, 6.0, 22.0], [0.0, 6.0, 30.0], 3.0, 12);
                let mat = Material { color: [1.0, 1.0, 1.0], lit: false, ..Material::default() };
                self.graph[arrow].items.push(Item::Mesh(Mesh::new(Arc::new(tip), Arc::new(mat))));
                for (glyph, color, size, at) in &e.icons {
                    let s = self.icon(glyph, color, *size, 1.0);
                    self.graph[s].pos = *at;
                    self.graph.add(obj, s);
                }
                self.graph.add(obj, arrow);
                if let Some(g) = e.selects.filter(|g| *g < self.entities.len()) {
                    self.graph[obj].pos = self.entities[g].world_pos(&self.doc, &self.entities);
                }
                if let Some(q) = e.start_quat(&self.doc, &self.entities) {
                    self.graph[obj].quat = q;
                }
                self.graph.add(world, obj);
                EntNodes::new(obj)
            }
            Look::Icon { glyph, color, size, opacity } => {
                let icon = self.icon(glyph, color, *size, *opacity);
                // A garib, Mr. Tip, a vent and a sound are the sprite itself. The others hold it as a child.
                let obj = if matches!(e.kind, "platform" | "pickup" | "spawn") {
                    let obj = self.graph.node("");
                    self.graph.add(obj, icon);
                    obj
                } else {
                    icon
                };
                self.graph.add(world, obj);
                let mut n = EntNodes::new(obj);
                if let Some((oid, scale)) = e.decor {
                    n.draw_nodes = self.attach_model(i, obj, Some(oid), scale, false, true);
                }
                self.platform_extras(&e, &mut n);
                n
            }
            Look::Model { scale, water, decorative } => {
                let obj = self.graph.node("");
                self.graph.add(world, obj);
                let mut n = EntNodes::new(obj);
                n.draw_nodes = self.attach_model(i, obj, e.object_id.as_deref(), *scale, *water, *decorative);
                if *water {
                    let vol = self.graph.node("");
                    let b = self.shape("box");
                    let f = self.fill(b, "#2a8fd0", 0.12);
                    let l = self.lines(&box_edges(1.0, 1.0, 1.0), entities::hex_rgb("#3fa8e0"), LineMode::Segments, None);
                    self.graph.add(vol, f);
                    self.graph.add(vol, l);
                    self.graph.add(world, vol);
                    let (centre, size) = water_volume(&self.doc, e.node);
                    self.graph[vol].pos = centre;
                    self.graph[vol].scale = size;
                    n.extra = Some(vol);
                }
                self.platform_extras(&e, &mut n);
                n
            }
        };
        let mut out = out;
        if let Some(on) = self.nodes_place(i, &out) {
            out.marks_on = on;
        }
        if self.exit_trigger(i).is_some() {
            out.exit = Some(self.exit_nodes());
            out.exit_on = self.place_exit(i, &out);
        }
        out
    }

    /// The sphere of an exit trigger: a faint fill and three rings, with a radius of 1. The rings are drawn a
    /// second time, faint and through everything: many exits of the game sit inside a wall or a door.
    fn exit_nodes(&mut self) -> NodeId {
        let group = self.graph.node("");
        let rgb = entities::hex_rgb(exits::COLOR);
        for ax in ['x', 'y', 'z'] {
            let points = ring_points(ax, 0.0, 48);
            let l = self.lines(&points, rgb, LineMode::Loop, None);
            self.graph.add(group, l);
            let mut through = Lines::from_f64(&points, rgb, LineMode::Loop);
            through.depth_test = false;
            through.opacity = 0.35;
            let l = self.graph.lines_node(through);
            self.graph.add(group, l);
        }
        let g = self.shape("sphere");
        let fill = self.fill(g, exits::COLOR, 0.1);
        self.graph.add(group, fill);
        let world = self.world;
        self.graph.add(world, group);
        group
    }

    /// Puts an exit's sphere where the game tests it, at the radius the game gives it. Returns whether there is a
    /// sphere to draw: a radius of 0, a model the editor cannot measure, and an exit that tests no distance (type
    /// 3) draw nothing.
    fn place_exit(&mut self, i: EntId, n: &EntNodes) -> bool {
        let Some(node) = n.exit else { return false };
        let t = self.exit_trigger(i).filter(|t| t.rule.who != exits::Who::OnPlatform);
        let r = t.as_ref().and_then(|t| t.radius).filter(|r| *r > 0.0);
        let shown = !self.hidden.contains(exits::LAYER) && !self.hidden.contains("platform");
        let g = &mut self.graph[node];
        g.visible = r.is_some() && shown;
        if let (Some(t), Some(r)) = (t, r) {
            g.pos = t.centre;
            g.scale = [r; 3];
        }
        r.is_some()
    }

    /// A platform's path line and the sprites on it.
    fn platform_extras(&mut self, e: &entities::Ent, n: &mut EntNodes) {
        if e.kind != "platform" {
            return;
        }
        if !e.path.is_empty() {
            let mut pts: Vec<V3> = e
                .path
                .iter()
                .map(|p| Binding::Xyz { node: *p, ax: XYZ, parent: e.parent_ent }.get(&self.doc, &self.entities))
                .collect();
            pts.push(pts[0]);
            let line = self.lines(&pts, entities::hex_rgb("#6fb7ff"), LineMode::Strip, Some((8.0, 5.0)));
            self.graph.add(self.world, line);
            n.extra = Some(line);
        }
        for (glyph, color, size, at) in &e.icons {
            let s = self.icon(glyph, color, *size, 1.0);
            self.graph[s].pos = *at;
            self.graph.add(n.obj, s);
        }
    }

    fn trigger_nodes(&mut self, i: EntId, color: &str, glyph: &str) -> EntNodes {
        let e = self.entities[i].clone();
        let rgb = entities::hex_rgb(color);
        let obj = self.graph.node("");
        let line_group = self.graph.node("");
        let icon = self.icon(glyph, color, 12, 1.0);
        self.graph.add(obj, line_group);
        self.graph.add(obj, icon);
        let shape = e.sub.and_then(|v| entities::trigger_spec(self.doc.tag(v))).map(|s| s.0).unwrap_or("box");
        let fill_geo = match shape {
            "box" => {
                let l = self.lines(&box_edges(1.0, 1.0, 1.0), rgb, LineMode::Segments, None);
                self.graph.add(line_group, l);
                self.shape("box")
            }
            "sphere" => {
                for ax in ['x', 'y', 'z'] {
                    let l = self.lines(&ring_points(ax, 0.0, 48), rgb, LineMode::Loop, None);
                    self.graph.add(line_group, l);
                }
                self.shape("sphere")
            }
            _ => {
                let mut ends = Vec::new();
                for y in [0.5, -0.5] {
                    let l = self.lines(&ring_points('y', y, 48), rgb, LineMode::Loop, None);
                    self.graph.add(line_group, l);
                    for (x, z) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                        ends.push([x, y, z]);
                    }
                }
                let mut sides = Vec::new();
                for k in 0..4 {
                    sides.push(ends[k]);
                    sides.push(ends[k + 4]);
                }
                let l = self.lines(&sides, rgb, LineMode::Segments, None);
                self.graph.add(line_group, l);
                self.shape("cylinder")
            }
        };
        let fill = self.fill(fill_geo, color, 0.08);
        let world = self.world;
        self.graph.add(world, obj);
        self.graph.add(world, fill);
        let mut n = EntNodes::new(obj);
        n.extra = Some(fill);
        n.line_group = Some(line_group);
        if e.zone {
            // A line from the middle of the zone to about where its camera stands
            let line = self.lines(&[[0.0; 3], [0.0; 3]], entities::hex_rgb("#b28dff"), LineMode::Strip, Some((12.0, 8.0)));
            let eye = self.icon("\u{25c9}", "#b28dff", 12, 1.0);
            let marks = self.graph.node("");
            self.graph.add(marks, line);
            self.graph.add(marks, eye);
            self.graph.add(world, marks);
            n.marks = Some(marks);
            n.mark_line = Some(line);
            n.mark_eye = Some(eye);
            n.zone = camera_zone(&self.doc, e.node);
        }
        n
    }

    /// Writes one entity's place, turn and size from the XML into its nodes.
    fn nodes_place(&mut self, i: EntId, n: &EntNodes) -> Option<bool> {
        let e = self.entities.get(i)?;
        let mut marks_on = None;
        let (doc, ents) = (&self.doc, &self.entities);
        match &e.look {
            Look::Start => {}
            Look::Trigger { .. } => {
                let (Some(b), Some(v)) = (&e.bind, &e.volume) else { return None };
                let c = b.get(doc, ents);
                let d = v.dims(doc);
                let s = d.map(|a| if a.abs() > 0.5 { a.abs() } else { 0.5 });
                let marks = if e.zone {
                    let z = camera_zone(doc, e.node).or_else(|| n.zone.clone());
                    Some(z.and_then(|z| zone_camera(doc, &z)))
                } else {
                    None
                };
                self.graph[n.obj].pos = c;
                if let Some(g) = n.line_group {
                    self.graph[g].scale = s;
                }
                if let Some(f) = n.extra {
                    self.graph[f].pos = c;
                    self.graph[f].scale = s;
                }
                if let (Some(v), Some(m)) = (marks, n.marks) {
                    self.graph[m].visible = v.is_some() && !self.hidden.contains("trigger");
                    marks_on = Some(v.is_some());
                    if let Some((a, b)) = v {
                        if let Some(line) = n.mark_line {
                            if let Some(Item::Lines(l)) = self.graph[line].items.get_mut(0) {
                                l.set_points(vec![f32s(a), f32s(b)]);
                            }
                        }
                        if let Some(eye) = n.mark_eye {
                            self.graph[eye].pos = b;
                        }
                    }
                }
            }
            Look::WindBox => {
                let (Some(b), Some(v)) = (&e.bind, &e.volume) else { return None };
                let (c, s) = (b.get(doc, ents), v.dims(doc));
                self.graph[n.obj].pos = c;
                self.graph[n.obj].scale = s;
            }
            Look::BuzzerLine => {
                let a = Binding::Xyz { node: e.node, ax: ["end_1_x", "end_1_y", "end_1_z"], parent: None }.get(doc, ents);
                let b = Binding::Xyz { node: e.node, ax: ["end_2_x", "end_2_y", "end_2_z"], parent: None }.get(doc, ents);
                if let Some(Item::Lines(l)) = self.graph[n.obj].items.get_mut(0) {
                    l.set_points(vec![f32s(a), f32s(b)]);
                }
            }
            _ => {
                let p = e.world_pos(doc, ents);
                let turns = matches!(e.xform, Some(Xform::Actor | Xform::Child | Xform::Platform | Xform::Enemy));
                let q = if turns { e.get_quat(doc) } else { None };
                let s = if turns { e.get_scale(doc) } else { None };
                let node = &mut self.graph[n.obj];
                node.pos = p;
                if let Some(q) = q {
                    node.quat = q;
                }
                if let Some(s) = s {
                    node.scale = s;
                }
            }
        }
        marks_on
    }

    /// Moves an entity's nodes to where the XML now has it, and the nodes of the entity it is a handle of.
    pub fn place_ent(&mut self, i: EntId) {
        self.place_one(i);
        if let Some(o) = self.entities[i].owner {
            self.place_one(o);
        }
    }

    fn place_one(&mut self, i: EntId) {
        let Some(n) = self.nodes.get(i).cloned() else { return };
        if let Some(on) = self.nodes_place(i, &n) {
            self.nodes[i].marks_on = on;
        }
        if n.exit.is_some() {
            self.nodes[i].exit_on = self.place_exit(i, &n);
        }
    }

    /// Puts every entity back where the XML has it (after the motion preview).
    pub fn place_all(&mut self) {
        for i in 0..self.entities.len() {
            self.place_one(i);
        }
        self.emit(Event::ViewChanged);
    }

    /// The motion preview: turns the spinning platforms to where they are after `t` seconds.
    pub fn animate(&mut self, t: f64) {
        if self.gizmo.dragging {
            return;
        }
        for i in 0..self.entities.len() {
            if let Some(e) = self.entities[i].spin_euler(&self.doc, t) {
                let obj = self.nodes[i].obj;
                self.graph[obj].quat = entities::quat_from_xyz(e);
            }
        }
        self.emit(Event::ViewChanged);
    }

    /// Turns the motion preview on or off. `now` is the clock `animate_at` is called with.
    pub fn set_preview(&mut self, on: bool, now: f64) {
        self.preview = on;
        if on {
            self.preview_t0 = now;
        } else {
            self.place_all();
        }
    }

    /// One frame of the motion preview, when it is on.
    pub fn animate_at(&mut self, now: f64) {
        if self.preview {
            self.animate(now - self.preview_t0);
        }
    }

    /// The entities that move without a rebuild: what a drag holds and, in the preview, what spins. The renderer
    /// leaves them out of its merged meshes.
    pub fn moving(&self) -> Vec<EntId> {
        let mut out = Vec::new();
        if let Some(d) = &self.drag {
            for it in &d.items {
                out.push(it.e);
                out.extend(self.entities[it.e].owner);
            }
        }
        if self.preview {
            out.extend((0..self.entities.len()).filter(|&i| self.entities[i].spin.is_some()));
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Shows the layers that are on and hides the others.
    pub fn apply_visibility(&mut self) {
        for i in 0..self.entities.len() {
            let v = !self.hidden.contains(self.entities[i].kind);
            let n = &self.nodes[i];
            self.graph[n.obj].visible = v;
            if let Some(x) = n.extra {
                self.graph[x].visible = v;
            }
            if let Some(m) = n.marks {
                self.graph[m].visible = v && n.marks_on;
            }
            if let Some(x) = n.exit {
                self.graph[x].visible = v && n.exit_on && !self.hidden.contains(exits::LAYER);
            }
        }
        self.emit(Event::ViewChanged);
    }

    /// Where an entity's node is in the world.
    pub fn obj_pos(&self, i: EntId) -> V3 {
        self.graph.world_pos(self.nodes[i].obj)
    }

    /// The world box around an entity's node.
    pub fn obj_box(&self, i: EntId) -> vec::Box3 {
        self.graph.bounds(self.nodes[i].obj)
    }

    /// The corners of the selected entities' boxes as line segments, for the selection outline (at most 400
    /// entities).
    pub fn selection_box_points(&self) -> Vec<[f32; 3]> {
        let boxes: Vec<vec::Box3> = self.selection.iter().take(400).map(|&i| self.obj_box(i)).filter(|b| !b.empty()).collect();
        crate::scene::boxes_edges(&boxes)
    }

    /// The roots the renderer draws, in order.
    pub fn roots(&self) -> Vec<NodeId> {
        let mut r = vec![self.world, self.path_node];
        r.extend(self.far_sphere);
        r
    }
}

fn f32s(p: V3) -> [f32; 3] {
    [p[0] as f32, p[1] as f32, p[2] as f32]
}

/// The level's fog and ambient light as the view draws them.
pub(crate) fn fog_of(doc: &glovebox_core::xml::Doc, fog: Option<glovebox_core::xml::NodeId>) -> ([f64; 3], Option<([f64; 3], f64, f64)>) {
    match fog {
        Some(f) => {
            let c = [num0(doc, f, "r") / 255.0, num0(doc, f, "g") / 255.0, num0(doc, f, "b") / 255.0];
            let far = num0(doc, f, "fog_distance") * 4.0;
            let on = num0(doc, f, "fog_enabled") != 0.0;
            (c, on.then_some((c, 800.0, if far > 2500.0 { far } else { 2500.0 })))
        }
        None => ([0.051, 0.055, 0.071], None),
    }
}
