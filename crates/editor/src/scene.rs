//! A small scene graph for the 3D view: nodes with transforms, and the meshes, lines and sprites they draw.
//!
//! It's plain data. The renderer in the app crate draws it. This module also finds what a ray hits (picking, the ground under a
//! point) and the boxes around things, on the CPU. Transforms and picking use `f64`. Vertex data is `f32`.
//!
//! Its shape:
//! - Nodes live in a [`Scene`] and are named by a [`NodeId`].
//! - A node that leaves the tree stays alive until [`Scene::delete`] frees it.
//! - Geometry, materials and textures are shared with `Arc`. Two meshes use "the same material" when they hold the
//!   same `Arc`. [`make_batches`] groups by that.
//! - A hit names its item by the item's index in the node's `items`.

use crate::camera::Camera;
use crate::vec::{self, Box3, Mat4, Quat, V3};
use glovebox_core::model::{AlphaMode, Model, Wrap};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

static IDS: AtomicU64 = AtomicU64::new(1);

/// A new id for a geometry, a texture or a set of lines. The renderer keys its GPU copies by it.
pub fn next_id() -> u64 {
    IDS.fetch_add(1, Ordering::Relaxed)
}

pub const IDENTITY: Mat4 = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];

/// Vertex data: positions, and optionally triangle indices, normals, uvs and colors (one per vertex).
/// Without indices, every three vertices are a triangle.
#[derive(Debug, Clone)]
pub struct Geometry {
    pub id: u64,
    pub positions: Vec<[f32; 3]>,
    pub indices: Option<Vec<[u32; 3]>>,
    pub normals: Option<Vec<[f32; 3]>>,
    pub uvs: Option<Vec<[f32; 2]>>,
    pub colors: Option<Vec<[f32; 4]>>,
    bounds: Box3,
}

impl Geometry {
    /// Triangles that name a vertex the geometry doesn't have are dropped.
    pub fn new(positions: Vec<[f32; 3]>, indices: Option<Vec<[u32; 3]>>) -> Geometry {
        let mut bounds = Box3::new();
        for p in &positions {
            bounds.add_point(to64(*p));
        }
        let n = positions.len() as u64;
        let indices = indices.map(|mut t| {
            t.retain(|t| t.iter().all(|&i| u64::from(i) < n));
            t
        });
        Geometry { id: next_id(), positions, indices, normals: None, uvs: None, colors: None, bounds }
    }

    /// The same, from `f64` points and a flat index list (the shapes below).
    pub fn from_f64(points: &[V3], indices: Option<&[u32]>) -> Geometry {
        let pos = points.iter().map(|p| [p[0] as f32, p[1] as f32, p[2] as f32]).collect();
        Geometry::new(pos, indices.map(|i| i.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect()))
    }

    /// Attributes whose length isn't the vertex count are left out.
    pub fn with(mut self, normals: Option<Vec<[f32; 3]>>, uvs: Option<Vec<[f32; 2]>>, colors: Option<Vec<[f32; 4]>>) -> Geometry {
        let n = self.positions.len();
        self.normals = normals.filter(|v| v.len() == n);
        self.uvs = uvs.filter(|v| v.len() == n);
        self.colors = colors.filter(|v| v.len() == n);
        self
    }

    /// The box around the positions, in the geometry's own space.
    pub fn bounds(&self) -> &Box3 {
        &self.bounds
    }

    pub fn triangle_count(&self) -> usize {
        match &self.indices {
            Some(t) => t.len(),
            None => self.positions.len() / 3,
        }
    }

    /// The corners of triangle `i`.
    pub fn triangle(&self, i: usize) -> [V3; 3] {
        let t = match &self.indices {
            Some(t) => t[i],
            None => [3 * i as u32, 3 * i as u32 + 1, 3 * i as u32 + 2],
        };
        t.map(|k| to64(self.positions[k as usize]))
    }
}

fn to64(p: [f32; 3]) -> V3 {
    [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])]
}

/// RGBA pixels, 8 bits each, `width * height * 4` bytes in rows. Row 0 is at v = 0. For a model that is the top
/// of the picture. For a sprite it's the bottom of the square.
#[derive(Debug, Clone)]
pub struct Texture {
    pub id: u64,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// The pixels are sRGB colors. The renderer turns them linear when it samples.
    pub srgb: bool,
}

impl Texture {
    /// None when the pixel data doesn't fit the size or the size is 0.
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Option<Texture> {
        if width == 0 || height == 0 || rgba.len() as u64 != u64::from(width) * u64::from(height) * 4 {
            return None;
        }
        Some(Texture { id: next_id(), width, height, rgba, srgb: true })
    }
}

#[derive(Debug, Clone)]
pub struct Material {
    pub color: [f64; 3],
    pub texture: Option<Arc<Texture>>,
    pub vertex_colors: bool,
    pub lit: bool,
    /// Pixels with alpha under this aren't drawn.
    pub alpha_test: f64,
    pub transparent: bool,
    pub opacity: f64,
    pub depth_write: bool,
    pub depth_test: bool,
    /// Not used: the renderer draws every material double-sided.
    pub double_sided: bool,
    /// How the texture repeats along s and t.
    pub wrap: (Wrap, Wrap),
}

impl Default for Material {
    fn default() -> Material {
        Material {
            color: [1.0; 3],
            texture: None,
            vertex_colors: false,
            lit: true,
            alpha_test: 0.0,
            transparent: false,
            opacity: 1.0,
            depth_write: true,
            depth_test: true,
            double_sided: true,
            wrap: (Wrap::Repeat, Wrap::Repeat),
        }
    }
}

impl Material {
    pub fn color(color: [f64; 3]) -> Material {
        Material { color, ..Material::default() }
    }

    /// Drawn after the opaque meshes, back to front.
    pub fn blended(&self) -> bool {
        self.transparent || self.opacity < 1.0
    }
}

#[derive(Debug, Clone)]
pub struct Mesh {
    pub geometry: Arc<Geometry>,
    pub material: Arc<Material>,
    /// The mesh's place within its node.
    pub matrix: Mat4,
}

impl Mesh {
    pub fn new(geometry: Arc<Geometry>, material: Arc<Material>) -> Mesh {
        Mesh { geometry, material, matrix: IDENTITY }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineMode {
    /// Each point joins the next.
    Strip,
    /// A strip whose last point joins the first.
    Loop,
    /// Every two points are one segment.
    Segments,
}

#[derive(Debug, Clone)]
pub struct Lines {
    pub id: u64,
    /// Counts the changes of `points`. The renderer uploads again when it differs.
    pub version: u64,
    pub color: [f64; 3],
    pub mode: LineMode,
    /// (dash, gap) in world units, or none for a solid line.
    pub dash: Option<(f64, f64)>,
    pub depth_test: bool,
    pub opacity: f64,
    pub matrix: Mat4,
    points: Vec<[f32; 3]>,
}

impl Lines {
    pub fn new(points: Vec<[f32; 3]>, color: [f64; 3], mode: LineMode) -> Lines {
        Lines { id: next_id(), version: 1, color, mode, dash: None, depth_test: true, opacity: 1.0, matrix: IDENTITY, points }
    }

    pub fn from_f64(points: &[V3], color: [f64; 3], mode: LineMode) -> Lines {
        Lines::new(points.iter().map(|p| [p[0] as f32, p[1] as f32, p[2] as f32]).collect(), color, mode)
    }

    pub fn points(&self) -> &[[f32; 3]] {
        &self.points
    }

    pub fn set_points(&mut self, points: Vec<[f32; 3]>) {
        self.points = points;
        self.version += 1;
    }

    /// The segments the lines draw, as pairs of points.
    pub fn segments(&self) -> Vec<([f32; 3], [f32; 3])> {
        let p = &self.points;
        match self.mode {
            LineMode::Segments => p.chunks_exact(2).map(|s| (s[0], s[1])).collect(),
            LineMode::Loop if p.len() > 1 => (0..p.len()).map(|i| (p[i], p[(i + 1) % p.len()])).collect(),
            _ => p.windows(2).map(|s| (s[0], s[1])).collect(),
        }
    }
}

/// A square that faces the camera, `size` world units across (a three.js Sprite).
#[derive(Debug, Clone)]
pub struct Sprite {
    pub texture: Arc<Texture>,
    pub size: f64,
    pub opacity: f64,
    pub depth_test: bool,
    pub matrix: Mat4,
}

impl Sprite {
    pub fn new(texture: Arc<Texture>, size: f64) -> Sprite {
        Sprite { texture, size, opacity: 1.0, depth_test: true, matrix: IDENTITY }
    }
}

#[derive(Debug, Clone)]
pub enum Item {
    Mesh(Mesh),
    Lines(Lines),
    Sprite(Sprite),
}

/// The name of a node in its [`Scene`]. A name of a deleted node matches no node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId {
    index: u32,
    generation: u32,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub name: String,
    pub pos: V3,
    /// The rotation: x, y, z, w. `set_euler` and `euler` read it as the editor's Euler angles.
    pub quat: Quat,
    pub scale: V3,
    pub items: Vec<Item>,
    /// A hidden node hides everything under it. Hidden things aren't drawn and not picked.
    pub visible: bool,
    /// The editor's entity the node stands for (an index or id the editor chooses).
    pub entity: Option<u64>,
    /// Drawn after lower orders. Above 0 it draws over everything, without a depth test.
    pub order: i32,
    parent: Option<NodeId>,
    children: Vec<NodeId>,
}

impl Node {
    pub fn new(name: &str) -> Node {
        Node {
            name: name.to_string(),
            pos: [0.0; 3],
            quat: vec::quat_identity(),
            scale: [1.0; 3],
            items: Vec::new(),
            visible: true,
            entity: None,
            order: 0,
            parent: None,
            children: Vec::new(),
        }
    }

    pub fn parent(&self) -> Option<NodeId> {
        self.parent
    }

    pub fn children(&self) -> &[NodeId] {
        &self.children
    }

    pub fn local_matrix(&self) -> Mat4 {
        vec::compose(self.pos, self.quat, self.scale)
    }

    /// Sets the rotation from Euler angles in radians, order 'YZX' (the level format's).
    pub fn set_euler(&mut self, x: f64, y: f64, z: f64) {
        self.quat = vec::quat_from_euler_yzx(x, y, z);
    }

    pub fn euler(&self) -> V3 {
        vec::euler_yzx_from_quat(self.quat)
    }
}

/// What a ray met.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    pub distance: f64,
    pub point: V3,
    pub node: NodeId,
    /// The index of the mesh, sprite or lines in the node's `items`.
    pub item: usize,
}

/// The options of [`Scene::raycast`]. The default is every mesh at any distance, and no sprite or line.
#[derive(Debug, Clone, Copy)]
pub struct RayOptions<'a> {
    pub near: f64,
    pub far: f64,
    pub meshes_only: bool,
    /// Sprites and lines are tested only with a camera.
    pub camera: Option<&'a Camera>,
    /// Lines count when they pass within this many pixels on screen. None or 0 leaves lines out.
    pub line_px: Option<f64>,
}

impl Default for RayOptions<'_> {
    fn default() -> Self {
        RayOptions { near: 0.0, far: f64::INFINITY, meshes_only: false, camera: None, line_px: None }
    }
}

struct Slot {
    generation: u32,
    node: Option<Node>,
}

/// The store of nodes. A node is in a tree (it has a parent, or other nodes hold it as a root) or detached.
#[derive(Default)]
pub struct Scene {
    slots: Vec<Slot>,
    free: Vec<u32>,
}

impl std::ops::Index<NodeId> for Scene {
    type Output = Node;
    /// Panics for a deleted node. Use `get` when the id may be stale.
    fn index(&self, id: NodeId) -> &Node {
        self.get(id).expect("the node was deleted")
    }
}

impl std::ops::IndexMut<NodeId> for Scene {
    fn index_mut(&mut self, id: NodeId) -> &mut Node {
        self.get_mut(id).expect("the node was deleted")
    }
}

impl Scene {
    pub fn new() -> Scene {
        Scene::default()
    }

    /// The number of live nodes.
    pub fn len(&self) -> usize {
        self.slots.len() - self.free.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Adds a detached node.
    pub fn insert(&mut self, node: Node) -> NodeId {
        let mut node = node;
        node.parent = None;
        node.children.clear();
        if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index as usize];
            slot.node = Some(node);
            return NodeId { index, generation: slot.generation };
        }
        self.slots.push(Slot { generation: 0, node: Some(node) });
        NodeId { index: self.slots.len() as u32 - 1, generation: 0 }
    }

    /// Adds a detached, empty node.
    pub fn node(&mut self, name: &str) -> NodeId {
        self.insert(Node::new(name))
    }

    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.slots.get(id.index as usize).filter(|s| s.generation == id.generation)?.node.as_ref()
    }

    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.slots.get_mut(id.index as usize).filter(|s| s.generation == id.generation)?.node.as_mut()
    }

    fn is_under(&self, id: NodeId, top: NodeId) -> bool {
        let mut n = Some(id);
        while let Some(i) = n {
            if i == top {
                return true;
            }
            n = self.get(i).and_then(|x| x.parent);
        }
        false
    }

    /// Puts `child` under `parent`, at the end. It leaves its old parent first. Nothing happens when either is
    /// deleted or when `parent` is `child` or under it.
    pub fn add(&mut self, parent: NodeId, child: NodeId) {
        if self.get(parent).is_none() || self.get(child).is_none() || self.is_under(parent, child) {
            return;
        }
        self.detach(child);
        self[child].parent = Some(parent);
        self[parent].children.push(child);
    }

    /// Takes the node out of its parent. It and everything under it stay alive.
    pub fn detach(&mut self, child: NodeId) {
        let Some(p) = self.get(child).and_then(|n| n.parent) else { return };
        if let Some(pn) = self.get_mut(p) {
            pn.children.retain(|c| *c != child);
        }
        self[child].parent = None;
    }

    /// Detaches `child` when it's a child of `parent`.
    pub fn remove(&mut self, parent: NodeId, child: NodeId) {
        if self.get(child).and_then(|n| n.parent) == Some(parent) {
            self.detach(child);
        }
    }

    /// Frees the node and everything under it.
    pub fn delete(&mut self, id: NodeId) {
        if self.get(id).is_none() {
            return;
        }
        self.detach(id);
        let mut stack = vec![id];
        while let Some(i) = stack.pop() {
            let Some(slot) = self.slots.get_mut(i.index as usize).filter(|s| s.generation == i.generation) else { continue };
            if let Some(n) = slot.node.take() {
                stack.extend(n.children);
                slot.generation = slot.generation.wrapping_add(1);
                self.free.push(i.index);
            }
        }
    }

    /// Frees everything under the node.
    pub fn clear(&mut self, id: NodeId) {
        let Some(n) = self.get(id) else { return };
        for c in n.children.clone() {
            self.delete(c);
        }
    }

    /// A deep copy of the node and everything under it, detached. Meshes share their geometry and material.
    /// Lines get a new id. `entity` isn't copied.
    pub fn copy_tree(&mut self, id: NodeId) -> Option<NodeId> {
        let src = self.get(id)?.clone();
        let kids = src.children.clone();
        let mut n = src;
        n.entity = None;
        for it in &mut n.items {
            if let Item::Lines(l) = it {
                l.id = next_id();
            }
        }
        let copy = self.insert(n);
        for k in kids {
            if let Some(c) = self.copy_tree(k) {
                self.add(copy, c);
            }
        }
        Some(copy)
    }

    pub fn local_matrix(&self, id: NodeId) -> Mat4 {
        self[id].local_matrix()
    }

    /// The node's matrix in the world: its parents' matrices times its own.
    pub fn world_matrix(&self, id: NodeId) -> Mat4 {
        let mut m = self[id].local_matrix();
        let mut p = self[id].parent;
        while let Some(i) = p {
            let n = &self[i];
            m = vec::mat4_mul(&n.local_matrix(), &m);
            p = n.parent;
        }
        m
    }

    pub fn world_pos(&self, id: NodeId) -> V3 {
        let m = self.world_matrix(id);
        [m[0][3], m[1][3], m[2][3]]
    }

    /// False when the node or a node above it is hidden.
    pub fn shown(&self, id: NodeId) -> bool {
        let mut n = Some(id);
        while let Some(i) = n {
            let node = &self[i];
            if !node.visible {
                return false;
            }
            n = node.parent;
        }
        true
    }

    /// The first node at or above `id` that stands for an entity, and that entity.
    pub fn entity_of(&self, id: NodeId) -> Option<(NodeId, u64)> {
        let mut n = Some(id);
        while let Some(i) = n {
            let node = self.get(i)?;
            if let Some(e) = node.entity {
                return Some((i, e));
            }
            n = node.parent;
        }
        None
    }

    fn walk_into(&self, id: NodeId, m: Mat4, visible_only: bool, out: &mut Vec<(NodeId, Mat4)>) {
        let Some(n) = self.get(id) else { return };
        if visible_only && !n.visible {
            return;
        }
        out.push((id, m));
        for &c in &n.children {
            if let Some(cn) = self.get(c) {
                self.walk_into(c, vec::mat4_mul(&m, &cn.local_matrix()), visible_only, out);
            }
        }
    }

    /// (node, matrix) for this node and every node under it, parents first. The matrices start from `base`
    /// (None: the node's own local matrix, as if it had no parent). With `visible_only`, hidden nodes and what is
    /// under them are left out.
    pub fn walk(&self, id: NodeId, base: Option<Mat4>, visible_only: bool) -> Vec<(NodeId, Mat4)> {
        let mut out = Vec::new();
        if let Some(n) = self.get(id) {
            self.walk_into(id, base.unwrap_or_else(|| n.local_matrix()), visible_only, &mut out);
        }
        out
    }

    /// `walk` with world matrices. A hidden parent above `id` doesn't hide it here.
    pub fn world_walk(&self, id: NodeId, visible_only: bool) -> Vec<(NodeId, Mat4)> {
        match self.get(id) {
            Some(_) => self.walk(id, Some(self.world_matrix(id)), visible_only),
            None => Vec::new(),
        }
    }

    /// The world box around everything under the node, hidden or not (three.js `Box3.setFromObject`).
    pub fn bounds(&self, id: NodeId) -> Box3 {
        let mut b = Box3::new();
        for (n, m) in self.world_walk(id, false) {
            for it in &self[n].items {
                match it {
                    Item::Mesh(mesh) => {
                        let gb = mesh.geometry.bounds();
                        if !gb.empty() {
                            let mm = vec::mat4_mul(&m, &mesh.matrix);
                            for c in gb.corners() {
                                b.add_point(transform_point(&mm, c));
                            }
                        }
                    }
                    Item::Lines(l) => {
                        let mm = vec::mat4_mul(&m, &l.matrix);
                        for p in l.points() {
                            b.add_point(transform_point(&mm, to64(*p)));
                        }
                    }
                    Item::Sprite(s) => {
                        let mm = vec::mat4_mul(&m, &s.matrix);
                        let c = [mm[0][3], mm[1][3], mm[2][3]];
                        let h = vec::scale(vec::decompose_scale(&mm), s.size / 2.0);
                        b.add_point(vec::sub(c, h));
                        b.add_point(vec::add(c, h));
                    }
                }
            }
        }
        b
    }

    /// A new detached node with one mesh.
    pub fn mesh_node(&mut self, geometry: Arc<Geometry>, material: Arc<Material>) -> NodeId {
        let mut n = Node::new("");
        n.items.push(Item::Mesh(Mesh::new(geometry, material)));
        self.insert(n)
    }

    /// A new detached node with one set of lines.
    pub fn lines_node(&mut self, lines: Lines) -> NodeId {
        let mut n = Node::new("");
        n.items.push(Item::Lines(lines));
        self.insert(n)
    }

    /// A new detached node with one sprite.
    pub fn sprite_node(&mut self, sprite: Sprite) -> NodeId {
        let mut n = Node::new("");
        n.items.push(Item::Sprite(sprite));
        self.insert(n)
    }

    /// Every mesh triangle and sprite the ray meets under `roots`, nearest first. Hidden nodes are left out.
    /// A mesh gives its nearest triangle only, and both faces count. Its material doesn't matter: a translucent
    /// fill is hit like any mesh. Sprites count as squares that face the camera. Lines count when
    /// they pass within `line_px` pixels on screen.
    pub fn raycast(&self, roots: &[NodeId], origin: V3, direction: V3, o: &RayOptions) -> Vec<Hit> {
        let direction = vec::div(direction, vec::norm(direction));
        let mut hits = Vec::new();
        for &root in roots {
            for (id, m) in self.world_walk(root, true) {
                for (index, it) in self[id].items.iter().enumerate() {
                    let hit = match it {
                        Item::Mesh(mesh) => ray_mesh(mesh, &m, origin, direction),
                        _ if o.meshes_only => None,
                        Item::Sprite(s) => o.camera.and_then(|cam| ray_sprite(s, &m, origin, direction, cam)),
                        Item::Lines(l) => match (o.camera, o.line_px) {
                            (Some(cam), Some(px)) if px != 0.0 => {
                                let mm = vec::mat4_mul(&m, &l.matrix);
                                let segs: Vec<(V3, V3)> = l
                                    .segments()
                                    .iter()
                                    .map(|(a, b)| (transform_point(&mm, to64(*a)), transform_point(&mm, to64(*b))))
                                    .collect();
                                if segs.is_empty() {
                                    None
                                } else {
                                    cam.segment_hit(&segs, origin, direction, px)
                                }
                            }
                            _ => None,
                        },
                    };
                    if let Some((distance, point)) = hit {
                        if o.near <= distance && distance <= o.far {
                            hits.push(Hit { distance, point, node: id, item: index });
                        }
                    }
                }
            }
        }
        // A stable sort: hits at the same distance keep their order
        hits.sort_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap_or(std::cmp::Ordering::Equal));
        hits
    }
}

fn ray_mesh(mesh: &Mesh, m: &Mat4, origin: V3, direction: V3) -> Option<(f64, V3)> {
    let mm = vec::mat4_mul(m, &mesh.matrix);
    let inv = vec::mat4_inv(&mm)?;
    let lo = transform_point(&inv, origin);
    let ld = transform_dir(&inv, direction);
    let g = &mesh.geometry;
    if !ray_box(lo, ld, g.bounds()) {
        return None;
    }
    let mut best = f64::INFINITY;
    for i in 0..g.triangle_count() {
        let d = ray_triangle(lo, ld, &g.triangle(i));
        if d < best {
            best = d;
        }
    }
    if !best.is_finite() {
        return None;
    }
    let pw = transform_point(&mm, vec::add(lo, vec::scale(ld, best)));
    Some((vec::norm(vec::sub(pw, origin)), pw))
}

fn ray_sprite(s: &Sprite, m: &Mat4, origin: V3, direction: V3, cam: &Camera) -> Option<(f64, V3)> {
    let mm = vec::mat4_mul(m, &s.matrix);
    let c = [mm[0][3], mm[1][3], mm[2][3]];
    let sx = vec::decompose_scale(&mm)[0];
    let half = (if sx != 0.0 { sx } else { 1.0 }) * s.size / 2.0;
    let to = vec::sub(c, origin);
    if vec::dot(to, direction) <= 0.0 {
        return None;
    }
    let plane_n = vec::scale(cam.forward(), -1.0);
    let denom = vec::dot(direction, plane_n);
    if denom.abs() < 1e-9 {
        return None;
    }
    let tt = vec::dot(to, plane_n) / denom;
    let p = vec::add(origin, vec::scale(direction, tt));
    let d = vec::sub(p, c);
    (vec::dot(d, cam.right()).abs() <= half && vec::dot(d, cam.up()).abs() <= half).then_some((tt, p))
}

/// The point `p` moved by the matrix.
pub fn transform_point(m: &Mat4, p: V3) -> V3 {
    [
        p[0] * m[0][0] + p[1] * m[0][1] + p[2] * m[0][2] + m[0][3],
        p[0] * m[1][0] + p[1] * m[1][1] + p[2] * m[1][2] + m[1][3],
        p[0] * m[2][0] + p[1] * m[2][1] + p[2] * m[2][2] + m[2][3],
    ]
}

fn transform_dir(m: &Mat4, p: V3) -> V3 {
    [
        p[0] * m[0][0] + p[1] * m[0][1] + p[2] * m[0][2],
        p[0] * m[1][0] + p[1] * m[1][1] + p[2] * m[1][2],
        p[0] * m[2][0] + p[1] * m[2][1] + p[2] * m[2][2],
    ]
}

/// The distance along the ray to the triangle, or infinity when the ray misses it. Both faces count
/// (Möller–Trumbore).
pub fn ray_triangle(origin: V3, direction: V3, tri: &[V3; 3]) -> f64 {
    let e1 = vec::sub(tri[1], tri[0]);
    let e2 = vec::sub(tri[2], tri[0]);
    let p = vec::cross(direction, e2);
    let det = vec::dot(e1, p);
    if det.abs() <= 1e-12 {
        return f64::INFINITY;
    }
    let inv = 1.0 / det;
    let t = vec::sub(origin, tri[0]);
    let u = vec::dot(t, p) * inv;
    let q = vec::cross(t, e1);
    let v = vec::dot(q, direction) * inv;
    let d = vec::dot(e2, q) * inv;
    if u >= 0.0 && v >= 0.0 && u + v <= 1.0 && d > 0.0 {
        d
    } else {
        f64::INFINITY
    }
}

/// True when the ray meets the box (the slab test). When the ray runs along a face, that face's axis is skipped.
pub fn ray_box(origin: V3, direction: V3, b: &Box3) -> bool {
    let (mut tmin, mut tmax) = (f64::NAN, f64::NAN);
    for i in 0..3 {
        let inv = 1.0 / direction[i];
        let t1 = (b.min[i] - origin[i]) * inv;
        let t2 = (b.max[i] - origin[i]) * inv;
        // 0 times infinity gives NaN for a ray along a face. That axis is skipped.
        if t1.is_nan() || t2.is_nan() {
            continue;
        }
        let (lo, hi) = (t1.min(t2), t1.max(t2));
        tmin = if tmin.is_nan() { lo } else { tmin.max(lo) };
        tmax = if tmax.is_nan() { hi } else { tmax.min(hi) };
    }
    tmax >= tmin.max(0.0)
}

// ---------------------------------------------------------------------------------------------------------------
// Shapes

fn flat_normals(pts: &[V3], idx: &[u32]) -> Vec<[f32; 3]> {
    let mut n = vec![[0.0f64; 3]; pts.len()];
    for t in idx.chunks_exact(3) {
        let (a, b, c) = (pts[t[0] as usize], pts[t[1] as usize], pts[t[2] as usize]);
        let f = vec::cross(vec::sub(b, a), vec::sub(c, a));
        for &k in t {
            n[k as usize] = vec::add(n[k as usize], f);
        }
    }
    n.iter()
        .map(|v| {
            let l = vec::norm(*v).max(1e-9);
            [(v[0] / l) as f32, (v[1] / l) as f32, (v[2] / l) as f32]
        })
        .collect()
}

/// A box around the origin, with a normal per vertex.
pub fn box_geometry(w: f64, h: f64, d: f64) -> Geometry {
    let eye = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    let (mut p, mut idx): (Vec<V3>, Vec<u32>) = (Vec::new(), Vec::new());
    for axis in 0..3 {
        for sign in [-1.0, 1.0] {
            let n = vec::scale(eye[axis], sign);
            // np.roll(np.eye(3), 1, 0)[axis] is row axis - 1. Rolling by 2 gives row axis - 2.
            let (u, v) = (eye[(axis + 2) % 3], eye[(axis + 1) % 3]);
            let base = p.len() as u32;
            for (du, dv) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                let c = vec::add(n, vec::add(vec::scale(u, du), vec::scale(v, dv)));
                p.push([c[0] * 0.5 * w, c[1] * 0.5 * h, c[2] * 0.5 * d]);
            }
            let q = [base, base + 1, base + 2, base, base + 2, base + 3];
            if sign > 0.0 {
                idx.extend(q);
            } else {
                idx.extend(q.iter().rev());
            }
        }
    }
    let normals = flat_normals(&p, &idx);
    Geometry::from_f64(&p, Some(&idx)).with(Some(normals), None, None)
}

/// The twelve edges of a box around the origin, as 24 points for `LineMode::Segments`.
pub fn box_edges(w: f64, h: f64, d: f64) -> Vec<V3> {
    let mut c = Vec::new();
    for x in [-0.5, 0.5] {
        for y in [-0.5, 0.5] {
            for z in [-0.5, 0.5] {
                c.push([x * w, y * h, z * d]);
            }
        }
    }
    BOX_EDGES.iter().flat_map(|&(i, j)| [c[i], c[j]]).collect()
}

/// The corner pairs of a box's edges, for `Box3::corners` and `box_edges`.
pub const BOX_EDGES: [(usize, usize); 12] =
    [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)];

/// The edges of world boxes as points for `LineMode::Segments` (the selection's highlight).
pub fn boxes_edges(boxes: &[Box3]) -> Vec<[f32; 3]> {
    let mut out = Vec::new();
    for b in boxes.iter().filter(|b| !b.empty()) {
        let c = b.corners();
        for (i, j) in BOX_EDGES {
            for p in [c[i], c[j]] {
                out.push([p[0] as f32, p[1] as f32, p[2] as f32]);
            }
        }
    }
    out
}

/// A sphere. The usual one is `sphere_geometry(1, 24, 16)`.
pub fn sphere_geometry(r: f64, ws: u32, hs: u32) -> Geometry {
    use std::f64::consts::PI;
    let (mut pts, mut idx): (Vec<V3>, Vec<u32>) = (Vec::new(), Vec::new());
    for j in 0..=hs {
        let th = PI * f64::from(j) / f64::from(hs);
        for i in 0..=ws {
            let ph = 2.0 * PI * f64::from(i) / f64::from(ws);
            pts.push([-r * ph.cos() * th.sin(), r * th.cos(), r * ph.sin() * th.sin()]);
        }
    }
    for j in 0..hs {
        for i in 0..ws {
            let (a, b) = (j * (ws + 1) + i, (j + 1) * (ws + 1) + i);
            idx.extend([a, b, a + 1, b, b + 1, a + 1]);
        }
    }
    let k = r.max(1e-9);
    let normals = pts.iter().map(|p| [(p[0] / k) as f32, (p[1] / k) as f32, (p[2] / k) as f32]).collect();
    Geometry::from_f64(&pts, Some(&idx)).with(Some(normals), None, None)
}

/// A cylinder along Y with both ends closed. The usual one is `cylinder_geometry(1, 1, 32)`.
pub fn cylinder_geometry(r: f64, h: f64, n: u32) -> Geometry {
    use std::f64::consts::PI;
    let (mut pts, mut idx): (Vec<V3>, Vec<u32>) = (Vec::new(), Vec::new());
    for i in 0..=n {
        let a = 2.0 * PI * f64::from(i) / f64::from(n);
        pts.push([r * a.sin(), h / 2.0, r * a.cos()]);
        pts.push([r * a.sin(), -h / 2.0, r * a.cos()]);
    }
    for i in 0..n {
        let a = 2 * i;
        idx.extend([a, a + 1, a + 2, a + 1, a + 3, a + 2]);
    }
    let (top, bot) = (pts.len() as u32, pts.len() as u32 + 1);
    pts.push([0.0, h / 2.0, 0.0]);
    pts.push([0.0, -h / 2.0, 0.0]);
    for i in 0..n {
        idx.extend([top, 2 * i, 2 * i + 2, bot, 2 * i + 3, 2 * i + 1]);
    }
    let normals = flat_normals(&pts, &idx);
    Geometry::from_f64(&pts, Some(&idx)).with(Some(normals), None, None)
}

pub fn octahedron_geometry(r: f64) -> Geometry {
    let p = [[r, 0.0, 0.0], [-r, 0.0, 0.0], [0.0, r, 0.0], [0.0, -r, 0.0], [0.0, 0.0, r], [0.0, 0.0, -r]];
    let idx = [0, 2, 4, 0, 4, 3, 0, 3, 5, 0, 5, 2, 1, 2, 5, 1, 5, 3, 1, 3, 4, 1, 4, 2];
    let normals = flat_normals(&p, &idx);
    Geometry::from_f64(&p, Some(&idx)).with(Some(normals), None, None)
}

/// `n` points of a unit circle about the axis ('x', 'y' or anything else for z), `offset` along it. The
/// usual number is 48 points.
pub fn ring_points(axis: char, offset: f64, n: usize) -> Vec<V3> {
    (0..n)
        .map(|i| {
            let a = 2.0 * std::f64::consts::PI * (i as f64) / (n as f64);
            let (c, s) = (a.cos(), a.sin());
            match axis {
                'x' => [offset, c, s],
                'y' => [c, offset, s],
                _ => [c, s, offset],
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------------------------
// Models

struct PrefabNode {
    name: String,
    pos: V3,
    quat: Quat,
    scale: V3,
    items: Vec<Mesh>,
    children: Vec<usize>,
}

/// A model ready to place: a node tree to copy into a scene, in the editor's static pose.
/// Every copy shares the geometry and the materials.
pub struct Prefab {
    nodes: Vec<PrefabNode>,
    roots: Vec<usize>,
    /// The number of object nodes the game draws (the nodes with a mesh, at least 1).
    pub draw_nodes: usize,
    /// The triangles of all meshes.
    pub triangles: usize,
}

impl Prefab {
    /// Builds the scene data of a model. `texture` gives the pixels of a texture id, or none when they're missing (the material is then drawn without a texture).
    ///
    /// The material: the base color and its alpha as opacity, `Blend` as transparent, the
    /// cutoff of `Mask` as the alpha test, and an alpha test of 0.3 for any other textured material. It's
    /// always lit and double-sided: `unlit`, `billboard` and `inherit_rotation` aren't read.
    pub fn from_model(model: &Model, texture: &mut dyn FnMut(u32) -> Option<Arc<Texture>>) -> Prefab {
        let mut textures: HashMap<u32, Option<Arc<Texture>>> = HashMap::new();
        let mut materials: HashMap<(u32, bool), Arc<Material>> = HashMap::new();
        let mut meshes: HashMap<u32, Vec<Mesh>> = HashMap::new();
        let scales = model.posed_scales();
        let mut nodes = Vec::with_capacity(model.nodes.len());
        let mut triangles = 0;
        for (i, n) in model.nodes.iter().enumerate() {
            let mut items = Vec::new();
            if let Some(mi) = n.mesh.filter(|m| (*m as usize) < model.meshes.len()) {
                let list = meshes.entry(mi).or_insert_with(|| {
                    let mut out = Vec::new();
                    for p in &model.meshes[mi as usize].primitives {
                        let has_colors = !p.colors.is_empty() && p.colors.len() == p.positions.len();
                        let mat = materials.entry((p.material, has_colors)).or_insert_with(|| {
                            let mut m = Material { vertex_colors: has_colors, ..Material::default() };
                            if let Some(src) = model.materials.get(p.material as usize) {
                                if let Some(id) = src.texture {
                                    m.texture = textures.entry(id).or_insert_with(|| texture(id)).clone();
                                    if m.texture.is_some() {
                                        m.wrap = (src.wrap_s, src.wrap_t);
                                    }
                                }
                                let c = src.base_color;
                                m.color = [f64::from(c[0]), f64::from(c[1]), f64::from(c[2])];
                                m.opacity = f64::from(c[3]);
                                m.transparent = src.alpha_mode == AlphaMode::Blend;
                                if src.alpha_mode == AlphaMode::Mask {
                                    m.alpha_test = f64::from(src.alpha_cutoff);
                                }
                            }
                            if m.alpha_test == 0.0 && m.texture.is_some() {
                                m.alpha_test = 0.3;
                            }
                            Arc::new(m)
                        });
                        let idx: Vec<[u32; 3]> = p.indices.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect();
                        let normals = if p.normals.len() == p.positions.len() && !p.normals.is_empty() {
                            p.normals.clone()
                        } else {
                            p.computed_normals()
                        };
                        let g = Geometry::new(p.positions.clone(), Some(idx)).with(
                            Some(normals),
                            Some(p.uvs.clone()),
                            has_colors.then(|| p.colors.clone()),
                        );
                        out.push(Mesh::new(Arc::new(g), mat.clone()));
                    }
                    out
                });
                triangles += list.iter().map(|m| m.geometry.triangle_count()).sum::<usize>();
                items = list.clone();
            }
            // A child that names a missing node, itself, or a node that isn't its child is dropped
            let children = n
                .children
                .iter()
                .map(|c| *c as usize)
                .filter(|&c| c != i && c < model.nodes.len() && model.nodes[c].parent == Some(i as u32))
                .collect();
            nodes.push(PrefabNode { name: n.name.clone(), pos: n.translation, quat: n.rotation, scale: scales[i], items, children });
        }
        let roots = (0..nodes.len()).filter(|&i| model.nodes[i].parent.is_none()).collect();
        Prefab { nodes, roots, draw_nodes: model.draw_nodes(), triangles }
    }

    /// Copies the model into the scene. Returns the copy's top node (named "scene"), detached.
    pub fn instance(&self, scene: &mut Scene) -> NodeId {
        let root = scene.node("scene");
        // (prefab node, parent in the scene, depth). The depth limit stops a tree that loops.
        let mut stack: Vec<(usize, NodeId, usize)> = self.roots.iter().rev().map(|&r| (r, root, 0)).collect();
        while let Some((i, parent, depth)) = stack.pop() {
            let p = &self.nodes[i];
            let mut n = Node::new(&p.name);
            (n.pos, n.quat, n.scale) = (p.pos, p.quat, p.scale);
            n.items = p.items.iter().cloned().map(Item::Mesh).collect();
            let id = scene.insert(n);
            scene.add(parent, id);
            if depth < self.nodes.len() {
                stack.extend(p.children.iter().rev().map(|&c| (c, id, depth + 1)));
            }
        }
        root
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Batches

/// Opaque meshes of one material, merged into one geometry in world space.
#[derive(Debug, Clone)]
pub struct Batch {
    pub geometry: Arc<Geometry>,
    pub material: Arc<Material>,
}

/// True for a mesh that `make_batches` merges: opaque, depth-tested, on a node of order 0 or less.
pub fn batchable(node: &Node, mesh: &Mesh) -> bool {
    node.order <= 0 && mesh.material.depth_test && !mesh.material.blended()
}

fn mat3_inv_t(m: &Mat4) -> Option<[[f64; 3]; 3]> {
    let a = |i: usize, j: usize| m[i][j];
    let c = [
        [a(1, 1) * a(2, 2) - a(1, 2) * a(2, 1), a(1, 2) * a(2, 0) - a(1, 0) * a(2, 2), a(1, 0) * a(2, 1) - a(1, 1) * a(2, 0)],
        [a(0, 2) * a(2, 1) - a(0, 1) * a(2, 2), a(0, 0) * a(2, 2) - a(0, 2) * a(2, 0), a(0, 1) * a(2, 0) - a(0, 0) * a(2, 1)],
        [a(0, 1) * a(1, 2) - a(0, 2) * a(1, 1), a(0, 2) * a(1, 0) - a(0, 0) * a(1, 2), a(0, 0) * a(1, 1) - a(0, 1) * a(1, 0)],
    ];
    let det = a(0, 0) * c[0][0] + a(0, 1) * c[0][1] + a(0, 2) * c[0][2];
    if det.abs() <= 1e-12 {
        return None;
    }
    // The cofactor matrix over the determinant is the inverse, transposed
    Some(c.map(|r| r.map(|v| v / det)))
}

/// The matrix that turns normals for a model matrix: the inverse of its 3 by 3 part, transposed. The identity
/// when it has no inverse.
pub fn normal_matrix(m: &Mat4) -> [[f64; 3]; 3] {
    mat3_inv_t(m).unwrap_or([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]])
}

/// Merges the opaque meshes under `roots` into one world-space geometry per material. Returns the batches and
/// the (node, item index) pairs they hold, which the renderer then skips. `skip` leaves a node and everything
/// under it out (things that move without a rebuild). Hidden nodes are left out.
pub fn make_batches(scene: &Scene, roots: &[NodeId], skip: &dyn Fn(NodeId, &Node) -> bool) -> (Vec<Batch>, HashSet<(NodeId, usize)>) {
    struct Group {
        material: Arc<Material>,
        parts: Vec<(Arc<Geometry>, Mat4)>,
    }
    let mut groups: Vec<Group> = Vec::new();
    let mut by_material: HashMap<*const Material, usize> = HashMap::new();
    let mut held = HashSet::new();
    for &root in roots {
        let Some(rn) = scene.get(root) else { continue };
        let base = match rn.parent {
            Some(p) => vec::mat4_mul(&scene.world_matrix(p), &rn.local_matrix()),
            None => rn.local_matrix(),
        };
        let mut stack = vec![(root, base)];
        while let Some((id, m)) = stack.pop() {
            let n = &scene[id];
            if !n.visible || skip(id, n) {
                continue;
            }
            for (index, it) in n.items.iter().enumerate() {
                let Item::Mesh(mesh) = it else { continue };
                if !batchable(n, mesh) {
                    continue;
                }
                let g = *by_material.entry(Arc::as_ptr(&mesh.material)).or_insert_with(|| {
                    groups.push(Group { material: mesh.material.clone(), parts: Vec::new() });
                    groups.len() - 1
                });
                groups[g].parts.push((mesh.geometry.clone(), vec::mat4_mul(&m, &mesh.matrix)));
                held.insert((id, index));
            }
            for &c in n.children.iter().rev() {
                if let Some(cn) = scene.get(c) {
                    stack.push((c, vec::mat4_mul(&m, &cn.local_matrix())));
                }
            }
        }
    }
    let mut out = Vec::with_capacity(groups.len());
    for group in groups {
        let total: usize = group.parts.iter().map(|(g, _)| g.positions.len()).sum();
        let (mut pos, mut nrm, mut uv, mut col) =
            (Vec::with_capacity(total), Vec::with_capacity(total), Vec::with_capacity(total), Vec::with_capacity(total));
        let mut idx = Vec::new();
        let mut n = 0u32;
        for (g, mm) in &group.parts {
            let nm = normal_matrix(mm);
            for (i, p) in g.positions.iter().enumerate() {
                let w = transform_point(mm, to64(*p));
                pos.push([w[0] as f32, w[1] as f32, w[2] as f32]);
                let q = match &g.normals {
                    Some(v) => to64(v[i]),
                    None => [0.0, 1.0, 0.0],
                };
                let q = vec::mat3_mul_v(&nm, q);
                let l = vec::norm(q).max(1e-9);
                nrm.push([(q[0] / l) as f32, (q[1] / l) as f32, (q[2] / l) as f32]);
            }
            match &g.uvs {
                Some(v) => uv.extend_from_slice(v),
                None => uv.resize(uv.len() + g.positions.len(), [0.0; 2]),
            }
            match &g.colors {
                Some(v) => col.extend_from_slice(v),
                None => col.resize(col.len() + g.positions.len(), [1.0; 4]),
            }
            match &g.indices {
                Some(t) => idx.extend(t.iter().map(|t| [t[0] + n, t[1] + n, t[2] + n])),
                None => idx.extend((0..g.positions.len() as u32 / 3).map(|t| [n + 3 * t, n + 3 * t + 1, n + 3 * t + 2])),
            }
            n += g.positions.len() as u32;
        }
        let geometry = Geometry::new(pos, Some(idx)).with(Some(nrm), Some(uv), Some(col));
        out.push(Batch { geometry: Arc::new(geometry), material: group.material });
    }
    (out, held)
}

// ---------------------------------------------------------------------------------------------------------------
// Dumps

/// A scene read from a dump file, with the dump's other fields.
pub struct Dump {
    pub scene: Scene,
    /// The scene's node for each entry of the dump's "nodes".
    pub nodes: Vec<NodeId>,
    pub roots: Vec<NodeId>,
    /// The whole JSON file (per-node world matrices and boxes, the camera, the rays).
    pub json: serde_json::Value,
}

/// Reads a scene from the reference dumps: a JSON file that names arrays in a file of raw little-endian numbers.
/// The tests replay the recorded scenes with it.
/// `keep(node)` says whether to take the node's items. A node without them is still made.
pub fn load_dump(json: &[u8], blob: &[u8], keep: &dyn Fn(&serde_json::Value) -> bool) -> Result<Dump, String> {
    use serde_json::Value;
    let json: Value = serde_json::from_slice(json).map_err(|e| e.to_string())?;
    let list = |v: &Value, k: &str| v.get(k).and_then(Value::as_array).cloned().ok_or(format!("the dump has no list '{k}'"));
    let num = |v: &Value, k: &str| v.get(k).and_then(Value::as_f64).ok_or(format!("the dump has no number '{k}'"));
    let flag = |v: &Value, k: &str| v.get(k).and_then(Value::as_bool).unwrap_or(false);
    fn floats<const N: usize>(v: Option<&Value>) -> Result<[f64; N], String> {
        let a = v.and_then(Value::as_array).filter(|a| a.len() == N).ok_or("a list of numbers has the wrong length")?;
        let mut out = [0.0; N];
        for (o, x) in out.iter_mut().zip(a) {
            *o = x.as_f64().ok_or("not a number")?;
        }
        Ok(out)
    }
    fn mat(v: Option<&Value>) -> Result<Mat4, String> {
        let f = floats::<16>(v)?;
        let mut m = [[0.0; 4]; 4];
        for i in 0..16 {
            m[i / 4][i % 4] = f[i];
        }
        Ok(m)
    }
    let bytes = |off: Option<u64>, len: usize| -> Result<&[u8], String> {
        let off = off.ok_or("an array has no offset")? as usize;
        off.checked_add(len).and_then(|end| blob.get(off..end)).ok_or("an array runs past the end of the file".to_string())
    };
    fn f32s<const N: usize>(b: &[u8]) -> Vec<[f32; N]> {
        b.chunks_exact(4 * N)
            .map(|c| {
                let mut o = [0.0; N];
                for (k, x) in o.iter_mut().enumerate() {
                    *x = f32::from_le_bytes([c[4 * k], c[4 * k + 1], c[4 * k + 2], c[4 * k + 3]]);
                }
                o
            })
            .collect()
    }
    let wrap = |v: Option<&Value>| match v.and_then(Value::as_u64) {
        Some(33071) => Wrap::Clamp,
        Some(33648) => Wrap::Mirror,
        _ => Wrap::Repeat,
    };

    let mut textures = HashMap::new();
    for t in list(&json, "textures")? {
        let (w, h) = (num(&t, "w")? as u32, num(&t, "h")? as u32);
        let px = bytes(t["off"].as_u64(), (w as usize).saturating_mul(h as usize).saturating_mul(4))?;
        let mut tex = Texture::new(w, h, px.to_vec()).ok_or("an empty texture")?;
        tex.srgb = flag(&t, "srgb");
        textures.insert(t["id"].as_u64(), Arc::new(tex));
    }
    let mut geometries = HashMap::new();
    for g in list(&json, "geometries")? {
        let n = num(&g, "n")? as usize;
        let idx = match g["idx"].as_array() {
            Some(a) => {
                let count = a.get(1).and_then(Value::as_u64).ok_or("bad indices")? as usize;
                let b = bytes(a[0].as_u64(), count.saturating_mul(4))?;
                let flat: Vec<u32> = b.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
                Some(flat.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect())
            }
            None => None,
        };
        let attr = |k: &str, width: usize| -> Result<Option<&[u8]>, String> {
            if g[k].is_null() {
                return Ok(None);
            }
            bytes(g[k].as_u64(), n.saturating_mul(4 * width)).map(Some)
        };
        let geo = Geometry::new(f32s::<3>(bytes(g["pos"].as_u64(), n.saturating_mul(12))?), idx).with(
            attr("nrm", 3)?.map(f32s::<3>),
            attr("uv", 2)?.map(f32s::<2>),
            attr("col", 4)?.map(f32s::<4>),
        );
        geometries.insert(g["id"].as_u64(), Arc::new(geo));
    }
    let mut materials = HashMap::new();
    for m in list(&json, "materials")? {
        let w = m["wrap"].as_array().cloned().unwrap_or_default();
        let mat = Material {
            color: floats::<3>(m.get("color"))?,
            texture: textures.get(&m["texture"].as_u64()).filter(|_| !m["texture"].is_null()).cloned(),
            vertex_colors: flag(&m, "vertex_colors"),
            lit: flag(&m, "lit"),
            alpha_test: num(&m, "alpha_test")?,
            transparent: flag(&m, "transparent"),
            opacity: num(&m, "opacity")?,
            depth_write: flag(&m, "depth_write"),
            depth_test: flag(&m, "depth_test"),
            double_sided: true,
            wrap: (wrap(w.first()), wrap(w.get(1))),
        };
        materials.insert(m["id"].as_u64(), Arc::new(mat));
    }
    let mut scene = Scene::new();
    let mut nodes: Vec<NodeId> = Vec::new();
    for n in list(&json, "nodes")? {
        let mut node = Node::new(n["name"].as_str().unwrap_or(""));
        node.pos = floats::<3>(n.get("pos"))?;
        node.quat = floats::<4>(n.get("quat"))?;
        node.scale = floats::<3>(n.get("scale"))?;
        node.visible = flag(&n, "visible");
        node.order = n["order"].as_i64().unwrap_or(0) as i32;
        node.entity = n["entity"].as_u64();
        for it in list(&n, "items")?.iter().filter(|_| keep(&n)) {
            let matrix = mat(it.get("matrix"))?;
            node.items.push(match it["t"].as_str() {
                Some("mesh") => {
                    let g = geometries.get(&it["g"].as_u64()).ok_or("a mesh names a missing geometry")?;
                    let m = materials.get(&it["m"].as_u64()).ok_or("a mesh names a missing material")?;
                    Item::Mesh(Mesh { geometry: g.clone(), material: m.clone(), matrix })
                }
                Some("lines") => {
                    let p = it["points"].as_array().filter(|a| a.len() == 2).ok_or("bad points")?;
                    let pts = f32s::<3>(bytes(p[0].as_u64(), (p[1].as_u64().unwrap_or(0) as usize).saturating_mul(12))?);
                    let mode = match it["mode"].as_str() {
                        Some("loop") => LineMode::Loop,
                        Some("segments") => LineMode::Segments,
                        _ => LineMode::Strip,
                    };
                    let mut l = Lines::new(pts, floats::<3>(it.get("color"))?, mode);
                    l.dash = floats::<2>(it.get("dash")).ok().map(|d| (d[0], d[1]));
                    (l.depth_test, l.opacity, l.matrix) = (flag(it, "depth_test"), num(it, "opacity")?, matrix);
                    Item::Lines(l)
                }
                _ => {
                    let t = textures.get(&it["tex"].as_u64()).ok_or("a sprite names a missing texture")?;
                    Item::Sprite(Sprite { texture: t.clone(), size: num(it, "size")?, opacity: num(it, "opacity")?, depth_test: flag(it, "depth_test"), matrix })
                }
            });
        }
        let id = scene.insert(node);
        if let Some(p) = n["parent"].as_u64() {
            scene.add(*nodes.get(p as usize).ok_or("a node comes before its parent")?, id);
        }
        nodes.push(id);
    }
    let mut roots = Vec::new();
    for r in list(&json, "roots")? {
        roots.push(*r.as_u64().and_then(|r| nodes.get(r as usize)).ok_or("a root names a missing node")?);
    }
    Ok(Dump { scene, nodes, roots, json })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: V3, b: V3) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-9)
    }

    fn unit_box(scene: &mut Scene, mat: &Arc<Material>) -> NodeId {
        scene.mesh_node(Arc::new(box_geometry(2.0, 2.0, 2.0)), mat.clone())
    }

    #[test]
    fn tree_and_matrices() {
        let mut s = Scene::new();
        let (a, b, c) = (s.node("a"), s.node("b"), s.node("c"));
        s.add(a, b);
        s.add(b, c);
        s[a].pos = [10.0, 0.0, 0.0];
        s[a].scale = [2.0; 3];
        s[b].set_euler(0.0, std::f64::consts::FRAC_PI_2, 0.0);
        s[c].pos = [0.0, 0.0, 1.0];
        // b turns +z to +x, a doubles it and moves it by 10
        assert!(close(s.world_pos(c), [12.0, 0.0, 0.0]));
        assert_eq!(s.walk(a, None, true).len(), 3);
        s[b].visible = false;
        assert_eq!(s.walk(a, None, true).len(), 1);
        assert!(!s.shown(c));
        // a node can't go under itself
        s.add(c, a);
        assert_eq!(s[a].parent(), None);
        s.remove(a, b);
        assert_eq!(s[b].parent(), None);
        assert!(s[a].children().is_empty());
        s.delete(b);
        assert!(s.get(c).is_none() && s.get(b).is_none());
        assert_eq!(s.len(), 1);
        // a reused slot doesn't answer to the old name
        let d = s.node("d");
        assert!(s.get(c).is_none() || s.get(b).is_none());
        assert_eq!(s[d].name, "d");
    }

    #[test]
    fn shapes() {
        let b = box_geometry(1.0, 2.0, 3.0);
        assert_eq!((b.positions.len(), b.triangle_count()), (24, 12));
        assert_eq!((b.bounds().min, b.bounds().max), ([-0.5, -1.0, -1.5], [0.5, 1.0, 1.5]));
        // every face winds the same way: inward (it's drawn double-sided)
        for i in 0..12 {
            let t = b.triangle(i);
            let n = vec::cross(vec::sub(t[1], t[0]), vec::sub(t[2], t[0]));
            assert!(vec::dot(n, t[0]) < 0.0);
        }
        assert_eq!(sphere_geometry(1.0, 24, 16).triangle_count(), 24 * 16 * 2);
        assert_eq!(cylinder_geometry(1.0, 1.0, 32).triangle_count(), 32 * 4);
        assert_eq!(octahedron_geometry(5.0).triangle_count(), 8);
        assert_eq!(box_edges(1.0, 1.0, 1.0).len(), 24);
        let r = ring_points('y', 0.5, 48);
        assert!(r.len() == 48 && close(r[0], [1.0, 0.5, 0.0]) && close(r[12], [0.0, 0.5, 1.0]));
        // a triangle that names a missing vertex is dropped
        assert_eq!(Geometry::new(vec![[0.0; 3]; 3], Some(vec![[0, 1, 2], [0, 1, 3]])).triangle_count(), 1);
    }

    #[test]
    fn bounds_cover_meshes_lines_and_sprites() {
        let mut s = Scene::new();
        let mat = Arc::new(Material::default());
        let root = s.node("root");
        let m = unit_box(&mut s, &mat);
        s[m].pos = [5.0, 0.0, 0.0];
        s.add(root, m);
        let l = s.lines_node(Lines::from_f64(&[[0.0, 9.0, 0.0], [0.0, -3.0, 0.0]], [1.0; 3], LineMode::Strip));
        s.add(root, l);
        let tex = Arc::new(Texture::new(1, 1, vec![255; 4]).unwrap());
        let sp = s.sprite_node(Sprite::new(tex, 4.0));
        s[sp].pos = [0.0, 0.0, -10.0];
        s[sp].visible = false;
        s.add(root, sp);
        let b = s.bounds(root);
        assert!(close(b.min, [-2.0, -3.0, -12.0]) && close(b.max, [6.0, 9.0, 1.0]));
        let empty = s.node("empty");
        assert!(s.bounds(empty).empty());
    }

    #[test]
    fn rays() {
        let mut s = Scene::new();
        let mat = Arc::new(Material::default());
        let root = s.node("root");
        let near = unit_box(&mut s, &mat);
        let far = unit_box(&mut s, &mat);
        s[far].pos = [0.0, 0.0, -10.0];
        s[far].scale = [3.0; 3];
        s.add(root, near);
        s.add(root, far);
        let o = RayOptions::default();
        let hits = s.raycast(&[root], [0.0, 0.0, 20.0], [0.0, 0.0, -2.0], &o);
        assert_eq!(hits.iter().map(|h| h.node).collect::<Vec<_>>(), [near, far]);
        assert!((hits[0].distance - 19.0).abs() < 1e-9 && close(hits[0].point, [0.0, 0.0, 1.0]));
        assert!((hits[1].distance - 27.0).abs() < 1e-9);
        // near and far limits
        let lim = RayOptions { near: 20.0, far: 30.0, ..o };
        assert_eq!(s.raycast(&[root], [0.0, 0.0, 20.0], [0.0, 0.0, -1.0], &lim).len(), 1);
        // a hidden node isn't hit, and a ray that points away hits nothing
        s[near].visible = false;
        assert_eq!(s.raycast(&[root], [0.0, 0.0, 20.0], [0.0, 0.0, -1.0], &o)[0].node, far);
        assert!(s.raycast(&[root], [0.0, 0.0, 20.0], [0.0, 0.0, 1.0], &o).is_empty());
        // a ray along an axis beside the box misses, and one in a face's plane doesn't divide by zero
        assert!(s.raycast(&[root], [5.0, 0.0, 20.0], [0.0, 0.0, -1.0], &o).is_empty());
        // a node squashed flat has no inverse and is skipped
        s[far].scale = [1.0, 0.0, 1.0];
        assert!(s.raycast(&[root], [0.0, 0.0, 20.0], [0.0, 0.0, -1.0], &o).is_empty());
    }

    #[test]
    fn sprites_and_lines_need_a_camera() {
        let mut s = Scene::new();
        let root = s.node("root");
        let tex = Arc::new(Texture::new(1, 1, vec![255; 4]).unwrap());
        let sp = s.sprite_node(Sprite::new(tex, 4.0));
        s.add(root, sp);
        let l = s.lines_node(Lines::from_f64(&[[-5.0, 6.0, 0.0], [5.0, 6.0, 0.0]], [1.0; 3], LineMode::Strip));
        s.add(root, l);
        let mut cam = Camera::new(55.0, 1.0, 1000.0);
        (cam.width, cam.height) = (800.0, 600.0);
        cam.pos = [0.0, 0.0, 50.0];
        cam.look_at([0.0, 0.0, 0.0]);
        let ray = |x: f64, y: f64| cam.ray(x, y).unwrap();
        let with = RayOptions { camera: Some(&cam), line_px: Some(5.0), ..RayOptions::default() };
        let center = cam.to_pixels([0.0, 0.0, 0.0]);
        let (o, d) = ray(center[0], center[1]);
        assert!(s.raycast(&[root], o, d, &RayOptions::default()).is_empty());
        let hits = s.raycast(&[root], o, d, &with);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].node == sp && (hits[0].distance - 50.0).abs() < 1e-6);
        // just inside and just outside the square's edge (2 units from the center)
        let edge = cam.to_pixels([1.9, 0.0, 0.0]);
        let (o, d) = ray(edge[0], edge[1]);
        assert_eq!(s.raycast(&[root], o, d, &with).len(), 1);
        let out = cam.to_pixels([2.1, 0.0, 0.0]);
        let (o, d) = ray(out[0], out[1]);
        assert!(s.raycast(&[root], o, d, &with).is_empty());
        // the line: 4 pixels off is a hit, 7 isn't, and meshes_only leaves it out
        let on = cam.to_pixels([3.0, 6.0, 0.0]);
        let (o, d) = ray(on[0], on[1] + 4.0);
        assert_eq!(s.raycast(&[root], o, d, &with)[0].node, l);
        assert!(s.raycast(&[root], o, d, &RayOptions { meshes_only: true, ..with }).is_empty());
        let (o, d) = ray(on[0], on[1] + 7.0);
        assert!(s.raycast(&[root], o, d, &with).is_empty());
    }

    #[test]
    fn line_segments() {
        let p = vec![[0.0; 3], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]];
        let mut l = Lines::new(p, [1.0; 3], LineMode::Strip);
        assert_eq!(l.segments().len(), 2);
        l.mode = LineMode::Loop;
        assert_eq!(l.segments().len(), 3);
        l.mode = LineMode::Segments;
        assert_eq!(l.segments().len(), 1);
        let v = l.version;
        l.set_points(Vec::new());
        assert!(l.version > v && l.segments().is_empty());
    }

    fn model() -> Model {
        use glovebox_core::model as gm;
        let prim = gm::Primitive {
            material: 0,
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            indices: vec![0, 1, 2],
            ..gm::Primitive::default()
        };
        let material = gm::Material {
            texture: Some(7),
            base_color: [1.0, 0.5, 0.25, 1.0],
            alpha_mode: AlphaMode::Opaque,
            alpha_cutoff: 0.5,
            double_sided: false,
            unlit: false,
            wrap_s: Wrap::Clamp,
            wrap_t: Wrap::Mirror,
        };
        let mut root = gm::Node::new("root".into());
        root.scale = [2.0, 2.0, 2.0];
        root.children = vec![1, 2];
        let mut a = gm::Node::new("a".into());
        (a.parent, a.mesh, a.translation) = (Some(0), Some(0), [1.0, 0.0, 0.0]);
        // b ignores its parent's scale
        let mut b = gm::Node::new("b".into());
        (b.parent, b.mesh, b.inherit_scale) = (Some(0), Some(0), false);
        Model { nodes: vec![root, a, b], meshes: vec![gm::Mesh { name: "m".into(), primitives: vec![prim], billboard: false }], materials: vec![material] }
    }

    #[test]
    fn a_model_becomes_nodes() {
        let m = model();
        let tex = Arc::new(Texture::new(1, 1, vec![255; 4]).unwrap());
        let mut asked = Vec::new();
        let prefab = Prefab::from_model(&m, &mut |id| {
            asked.push(id);
            Some(tex.clone())
        });
        assert_eq!(asked, [7]);
        assert_eq!((prefab.draw_nodes, prefab.triangles), (2, 2));
        let mut s = Scene::new();
        let top = s.instance_checked(&prefab);
        let walk = s.walk(top, None, true);
        assert_eq!(walk.len(), 4);
        // the scene's matrices and box are the model's own
        let world = m.world_matrices();
        for (k, (_, mat)) in walk[1..].iter().enumerate() {
            for i in 0..4 {
                for j in 0..4 {
                    assert!((mat[i][j] - world[k][i][j]).abs() < 1e-12);
                }
            }
        }
        let (b, mb) = (s.bounds(top), m.bounds().unwrap());
        assert!(close(b.min, mb.min) && close(b.max, mb.max));
        assert!(close(b.max, [4.0, 2.0, 0.0]));
        let Item::Mesh(mesh) = &s[walk[2].0].items[0] else { panic!() };
        let mat = &mesh.material;
        assert_eq!((mat.color, mat.alpha_test, mat.wrap, mat.lit), ([1.0, 0.5, 0.25], 0.3, (Wrap::Clamp, Wrap::Mirror), true));
        assert_eq!(mesh.geometry.normals.as_ref().unwrap()[0], [0.0, 0.0, 1.0]);
        // without the texture the material has no alpha test and the default wrap
        let bare = Prefab::from_model(&m, &mut |_| None);
        assert!(bare.nodes[1].items[0].material.texture.is_none());
        assert_eq!(bare.nodes[1].items[0].material.alpha_test, 0.0);
        assert_eq!(bare.nodes[1].items[0].material.wrap, (Wrap::Repeat, Wrap::Repeat));
    }

    impl Scene {
        fn instance_checked(&mut self, p: &Prefab) -> NodeId {
            let before = self.len();
            let top = p.instance(self);
            assert_eq!(self.len(), before + p.nodes.len() + 1);
            top
        }
    }

    #[test]
    fn batches_merge_by_material() {
        let mut s = Scene::new();
        let (red, blue) = (Arc::new(Material::color([1.0, 0.0, 0.0])), Arc::new(Material::color([0.0, 0.0, 1.0])));
        let glass = Arc::new(Material { transparent: true, ..Material::default() });
        let root = s.node("root");
        s[root].pos = [0.0, 100.0, 0.0];
        let mut made = Vec::new();
        for (i, mat) in [&red, &blue, &red, &glass, &red].iter().enumerate() {
            let n = unit_box(&mut s, mat);
            s[n].pos = [10.0 * i as f64, 0.0, 0.0];
            s.add(root, n);
            made.push(n);
        }
        s[made[4]].visible = false;
        let (batches, held) = make_batches(&s, &[root], &|_, _| false);
        assert_eq!(batches.len(), 2);
        assert!(Arc::ptr_eq(&batches[0].material, &red) && Arc::ptr_eq(&batches[1].material, &blue));
        let g = &batches[0].geometry;
        assert_eq!((g.positions.len(), g.triangle_count()), (48, 24));
        assert_eq!((g.bounds().min, g.bounds().max), ([-1.0, 99.0, -1.0], [21.0, 101.0, 1.0]));
        assert_eq!(g.indices.as_ref().unwrap()[12], [27, 26, 24]);
        assert_eq!(held, HashSet::from([(made[0], 0), (made[1], 0), (made[2], 0)]));
        // skipping a node leaves it to be drawn on its own
        let (batches, held) = make_batches(&s, &[root], &|id, _| id == made[0]);
        assert_eq!(batches[0].geometry.positions.len(), 24);
        assert!(!held.contains(&(made[0], 0)));
        // a turned node turns its normals
        s[made[1]].set_euler(0.0, std::f64::consts::FRAC_PI_2, 0.0);
        let (batches, _) = make_batches(&s, &[made[1]], &|_, _| false);
        let n = batches[0].geometry.normals.as_ref().unwrap();
        // the first face's normal is +x (the box winds inward). Turned a quarter about y it's -z.
        assert!((n[0][2] + 1.0).abs() < 1e-6, "{:?}", n[0]);
    }

    /// Replays the scenes of the reference dump `render/out/picks.json`: every node's world matrix and box, and
    /// every pick ray's hits.
    #[test]
    fn scenes_replay_the_reference() {
        let Ok(dir) = std::env::var("GLOVEBOX_REFERENCE") else {
            println!("skipped: GLOVEBOX_REFERENCE is not set");
            return;
        };
        let read = |name: &str| std::fs::read(format!("{dir}/render/out/{name}")).unwrap();
        let d = load_dump(&read("picks.json"), &read("picks.bin"), &|_| true).unwrap();
        let s = &d.scene;
        let near = |a: f64, b: f64, tol: f64| (a - b).abs() <= tol * (1.0 + b.abs());
        let nodes = d.json["nodes"].as_array().unwrap();
        let (mut boxes, mut worst) = (0, 0.0f64);
        for (n, id) in nodes.iter().zip(&d.nodes) {
            let w = s.world_matrix(*id);
            for (k, v) in n["world"].as_array().unwrap().iter().enumerate() {
                let v = v.as_f64().unwrap();
                worst = worst.max((w[k / 4][k % 4] - v).abs() / (1.0 + v.abs()));
                assert!(near(w[k / 4][k % 4], v, 1e-12), "world matrix of {}", n["name"]);
            }
            let b = s.bounds(*id);
            match n["box"].as_array() {
                None => assert!(b.empty()),
                Some(pb) => {
                    boxes += 1;
                    for k in 0..3 {
                        assert!(near(b.min[k], pb[0][k].as_f64().unwrap(), 1e-9), "box of {}", n["name"]);
                        assert!(near(b.max[k], pb[1][k].as_f64().unwrap(), 1e-9), "box of {}", n["name"]);
                    }
                }
            }
        }
        let c = &d.json["camera"];
        let f = |k: &str| c[k].as_f64().unwrap();
        let v = |k: &str, i: usize| c[k][i].as_f64().unwrap();
        let mut cam = Camera::new(f("fov"), f("near"), f("far"));
        (cam.width, cam.height) = (f("width"), f("height"));
        cam.pos = [v("pos", 0), v("pos", 1), v("pos", 2)];
        cam.quat = [v("quat", 0), v("quat", 1), v("quat", 2), v("quat", 3)];
        let rays = d.json["rays"].as_array().unwrap();
        let (mut hits_total, mut kinds) = (0, [0usize; 3]);
        for (ri, r) in rays.iter().enumerate() {
            let g = |k: &str| [r[k][0].as_f64().unwrap(), r[k][1].as_f64().unwrap(), r[k][2].as_f64().unwrap()];
            let o = &r["opts"];
            let opts = RayOptions {
                near: o["near"].as_f64().unwrap(),
                far: o["far"].as_f64().unwrap_or(f64::INFINITY),
                meshes_only: o["meshes_only"].as_bool().unwrap(),
                camera: o["camera"].as_bool().unwrap().then_some(&cam),
                line_px: o["line_px"].as_f64(),
            };
            let hits = s.raycast(&d.roots, g("origin"), g("dir"), &opts);
            let want = r["hits"].as_array().unwrap();
            assert_eq!(hits.len(), want.len(), "ray {ri}");
            for (h, w) in hits.iter().zip(want) {
                assert_eq!(h.node, d.nodes[w["node"].as_u64().unwrap() as usize], "ray {ri}");
                assert_eq!(h.item, w["item"].as_u64().unwrap() as usize, "ray {ri}");
                assert!(near(h.distance, w["d"].as_f64().unwrap(), 1e-9), "ray {ri}: {} {}", h.distance, w["d"]);
                for k in 0..3 {
                    assert!(near(h.point[k], w["p"][k].as_f64().unwrap(), 1e-9), "ray {ri}");
                }
                kinds[match s[h.node].items[h.item] {
                    Item::Mesh(_) => 0,
                    Item::Sprite(_) => 1,
                    Item::Lines(_) => 2,
                }] += 1;
                hits_total += 1;
            }
        }
        println!(
            "{} nodes, {boxes} boxes, worst matrix difference {worst:e}; {} rays, {hits_total} hits ({} mesh, {} sprite, {} line)",
            nodes.len(),
            rays.len(),
            kinds[0],
            kinds[1],
            kinds[2]
        );
        assert!(kinds.iter().all(|k| *k > 10));
    }
}
