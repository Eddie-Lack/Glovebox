//! The model the editor draws for one object of an object bank: a tree of nodes, meshes and materials.
//!
//! It carries what libgarib's glTF export of the object carries, as far as the editor reads it.
//! `objbank::model` builds it. Node 0 is the root. It has the object's id as its name and no mesh. A node's
//! transform is its static pose: the first key of each of the mesh's animation tracks. Transforms are `f64`, as the
//! glTF file holds them. A sprite's scale is a third of its size and has no exact `f32`.

/// How a texture repeats outside 0..1, in one direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Wrap {
    #[default]
    Repeat,
    Mirror,
    Clamp,
}

impl Wrap {
    /// The OpenGL and glTF constant.
    pub fn gl(self) -> u32 {
        match self {
            Wrap::Repeat => 10497,
            Wrap::Mirror => 33648,
            Wrap::Clamp => 33071,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AlphaMode {
    #[default]
    Opaque,
    /// Pixels with alpha under `Material::alpha_cutoff` are not drawn.
    Mask,
    Blend,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Material {
    /// The texture's id, or none for an untextured material.
    pub texture: Option<u32>,
    pub base_color: [f32; 4],
    pub alpha_mode: AlphaMode,
    /// 0.5 for `AlphaMode::Mask`. Not used otherwise.
    pub alpha_cutoff: f32,
    /// Only billboards are marked double-sided. The Python editor drew every material double-sided.
    pub double_sided: bool,
    /// The game draws the mesh with its vertex colours and without lights.
    pub unlit: bool,
    pub wrap_s: Wrap,
    pub wrap_t: Wrap,
}

/// The triangles of a mesh that share a material. Three vertices for each triangle. No vertex is shared.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Primitive {
    /// An index into `Model::materials`
    pub material: u32,
    pub positions: Vec<[f32; 3]>,
    /// One for each vertex, or empty. The editor then uses `computed_normals`.
    pub normals: Vec<[f32; 3]>,
    /// One for each vertex, or empty.
    pub uvs: Vec<[f32; 2]>,
    /// One for each vertex, or empty.
    pub colors: Vec<[f32; 4]>,
    /// Three for each triangle. They count up from 0.
    pub indices: Vec<u32>,
}

impl Primitive {
    pub fn triangles(&self) -> usize {
        self.indices.len() / 3
    }

    /// The normals the editor uses when the primitive has none: for each vertex, the normalised sum of the
    /// cross products of its triangles (`desktop/gltf.py`, `vertex_normals`).
    pub fn computed_normals(&self) -> Vec<[f32; 3]> {
        let mut sum = vec![[0f32; 3]; self.positions.len()];
        for t in self.indices.chunks_exact(3) {
            let (Some(a), Some(b), Some(c)) = (
                self.positions.get(t[0] as usize),
                self.positions.get(t[1] as usize),
                self.positions.get(t[2] as usize),
            ) else {
                continue;
            };
            let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
            for &i in t {
                for k in 0..3 {
                    sum[i as usize][k] += n[k];
                }
            }
        }
        for n in &mut sum {
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-9);
            *n = [n[0] / len, n[1] / len, n[2] / len];
        }
        sum
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Mesh {
    pub name: String,
    pub primitives: Vec<Primitive>,
    /// A sprite that the game turns to the camera. Its mesh is a square in the YZ plane.
    pub billboard: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub name: String,
    pub parent: Option<u32>,
    pub children: Vec<u32>,
    pub translation: [f64; 3],
    /// A quaternion: x, y, z, w
    pub rotation: [f64; 4],
    pub scale: [f64; 3],
    /// An index into `Model::meshes`
    pub mesh: Option<u32>,
    /// False when the node ignores the scale of the nodes above it (libgarib's `EXT_node_tsr_inheritance`).
    /// The pieces directly under the root and the sprites have it false.
    pub inherit_scale: bool,
    /// False for sprites. The Python editor did not read it.
    pub inherit_rotation: bool,
}

impl Node {
    pub fn new(name: String) -> Node {
        Node {
            name,
            parent: None,
            children: Vec::new(),
            translation: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0; 3],
            mesh: None,
            inherit_scale: true,
            inherit_rotation: true,
        }
    }
}

/// A 4 by 4 matrix in rows. It acts on column vectors.
pub type Mat4 = [[f64; 4]; 4];

/// An axis-aligned box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Model {
    pub nodes: Vec<Node>,
    pub meshes: Vec<Mesh>,
    pub materials: Vec<Material>,
}

fn mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut m = [[0.0; 4]; 4];
    for (r, row) in m.iter_mut().enumerate() {
        for (c, cell) in row.iter_mut().enumerate() {
            *cell = (0..4).map(|k| a[r][k] * b[k][c]).sum();
        }
    }
    m
}

/// The matrix of a translation, a quaternion and a scale (`desktop/vec.py`, `compose`).
pub fn compose(pos: [f64; 3], q: [f64; 4], scale: [f64; 3]) -> Mat4 {
    let [x, y, z, w] = q;
    let (x2, y2, z2) = (x + x, y + y, z + z);
    let (xx, xy, xz) = (x * x2, x * y2, x * z2);
    let (yy, yz, zz) = (y * y2, y * z2, z * z2);
    let (wx, wy, wz) = (w * x2, w * y2, w * z2);
    let r = [
        [1.0 - (yy + zz), xy - wz, xz + wy],
        [xy + wz, 1.0 - (xx + zz), yz - wx],
        [xz - wy, yz + wx, 1.0 - (xx + yy)],
    ];
    let mut m = [[0.0; 4]; 4];
    for i in 0..3 {
        for j in 0..3 {
            m[i][j] = r[i][j] * scale[j];
        }
        m[i][3] = pos[i];
    }
    m[3][3] = 1.0;
    m
}

/// The lengths of a matrix's three axes.
fn axis_scale(m: &Mat4) -> [f64; 3] {
    let mut s = [0.0; 3];
    for (j, v) in s.iter_mut().enumerate() {
        *v = (m[0][j] * m[0][j] + m[1][j] * m[1][j] + m[2][j] * m[2][j]).sqrt();
    }
    s
}

impl Model {
    /// The number of nodes with a mesh. Each is one object the game draws.
    pub fn mesh_nodes(&self) -> usize {
        self.nodes.iter().filter(|n| n.mesh.is_some()).count()
    }

    /// The editor's draw-load count of the model: the nodes with a mesh, and at least 1.
    pub fn draw_nodes(&self) -> usize {
        self.mesh_nodes().max(1)
    }

    pub fn triangles(&self) -> usize {
        self.nodes
            .iter()
            .filter_map(|n| self.meshes.get(n.mesh? as usize))
            .flat_map(|m| &m.primitives)
            .map(|p| p.triangles())
            .sum()
    }

    /// The ids of the textures the materials use, each once, sorted.
    pub fn texture_ids(&self) -> Vec<u32> {
        let mut ids: Vec<u32> = self.materials.iter().filter_map(|m| m.texture).collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    fn world(&self, scales: &[[f64; 3]]) -> Vec<Mat4> {
        let mut out: Vec<Mat4> = vec![[[0.0; 4]; 4]; self.nodes.len()];
        let mut done = vec![false; self.nodes.len()];
        // A parent comes before its children in `nodes`. A tree that breaks this is walked again until it settles.
        for _ in 0..self.nodes.len() {
            let mut all = true;
            for (i, n) in self.nodes.iter().enumerate() {
                if done[i] {
                    continue;
                }
                let local = compose(n.translation, n.rotation, scales[i]);
                match n.parent.map(|p| p as usize) {
                    None => out[i] = local,
                    Some(p) if p < done.len() && done[p] => out[i] = mul(&out[p], &local),
                    Some(_) => {
                        all = false;
                        continue;
                    }
                }
                done[i] = true;
            }
            if all {
                break;
            }
        }
        out
    }

    /// Each node's scale as the editor poses it. A node that does not inherit scale has its scale divided by the
    /// scale of its parent's world matrix (`desktop/gltf.py`, `load`).
    pub fn posed_scales(&self) -> Vec<[f64; 3]> {
        let raw: Vec<[f64; 3]> = self.nodes.iter().map(|n| n.scale).collect();
        if self.nodes.iter().all(|n| n.inherit_scale || n.parent.is_none()) {
            return raw;
        }
        let world = self.world(&raw);
        let mut out = raw;
        for (i, n) in self.nodes.iter().enumerate() {
            let Some(p) = n.parent else { continue };
            if n.inherit_scale || p as usize >= world.len() {
                continue;
            }
            let ps = axis_scale(&world[p as usize]);
            for k in 0..3 {
                out[i][k] /= if ps[k] == 0.0 { 1.0 } else { ps[k] };
            }
        }
        out
    }

    /// Each node's matrix in the model's space, in the editor's static pose.
    pub fn world_matrices(&self) -> Vec<Mat4> {
        self.world(&self.posed_scales())
    }

    /// The box around the model in its own space, in the editor's static pose. It is the box around the corners of
    /// each mesh's own box, as the editor measures a model. None when the model has no vertex.
    pub fn bounds(&self) -> Option<Bounds> {
        let world = self.world_matrices();
        let mut b = Bounds { min: [f64::INFINITY; 3], max: [f64::NEG_INFINITY; 3] };
        let mut any = false;
        for (i, n) in self.nodes.iter().enumerate() {
            let Some(mesh) = n.mesh.and_then(|m| self.meshes.get(m as usize)) else { continue };
            for p in &mesh.primitives {
                if p.positions.is_empty() {
                    continue;
                }
                let (mut lo, mut hi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
                for v in &p.positions {
                    for k in 0..3 {
                        lo[k] = lo[k].min(v[k]);
                        hi[k] = hi[k].max(v[k]);
                    }
                }
                let m = &world[i];
                for corner in 0..8 {
                    let c = [
                        f64::from(if corner & 4 == 0 { lo[0] } else { hi[0] }),
                        f64::from(if corner & 2 == 0 { lo[1] } else { hi[1] }),
                        f64::from(if corner & 1 == 0 { lo[2] } else { hi[2] }),
                    ];
                    for k in 0..3 {
                        let v = c[0] * m[k][0] + c[1] * m[k][1] + c[2] * m[k][2] + m[k][3];
                        b.min[k] = b.min[k].min(v);
                        b.max[k] = b.max[k].max(v);
                    }
                }
                any = true;
            }
        }
        any.then_some(b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(material: u32) -> Primitive {
        Primitive {
            material,
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            indices: vec![0, 1, 2],
            ..Primitive::default()
        }
    }

    fn model() -> Model {
        let mut root = Node::new("root".into());
        root.children = vec![1];
        let mut piece = Node::new("piece".into());
        piece.parent = Some(0);
        piece.children = vec![2];
        piece.mesh = Some(0);
        piece.translation = [10.0, 0.0, 0.0];
        piece.scale = [2.0, 2.0, 2.0];
        piece.inherit_scale = false;
        let mut sprite = Node::new("piece_sprite_0".into());
        sprite.parent = Some(1);
        sprite.mesh = Some(1);
        sprite.translation = [0.0, 1.0, 0.0];
        sprite.scale = [1.0, 4.0, 4.0];
        sprite.inherit_scale = false;
        sprite.inherit_rotation = false;
        Model {
            nodes: vec![root, piece, sprite],
            meshes: vec![
                Mesh { name: "piece".into(), primitives: vec![square(0)], billboard: false },
                Mesh { name: "piece_sprite_0".into(), primitives: vec![square(1)], billboard: true },
            ],
            materials: vec![],
        }
    }

    #[test]
    fn counts() {
        let m = model();
        assert_eq!(m.mesh_nodes(), 2);
        assert_eq!(m.draw_nodes(), 2);
        assert_eq!(m.triangles(), 2);
        assert_eq!(Model::default().draw_nodes(), 1);
        assert_eq!(Model::default().bounds(), None);
    }

    #[test]
    fn pose_and_bounds() {
        let m = model();
        // The sprite ignores the piece's scale of 2: its own scale of 4 is posed as 2, and is 4 in the world.
        assert_eq!(m.posed_scales()[2], [0.5, 2.0, 2.0]);
        let w = m.world_matrices();
        assert_eq!(w[1][0], [2.0, 0.0, 0.0, 10.0]);
        assert_eq!(w[2][1], [0.0, 4.0, 0.0, 2.0]);
        let b = m.bounds().unwrap();
        assert_eq!(b.min, [10.0, 0.0, 0.0]);
        assert_eq!(b.max, [12.0, 6.0, 0.0]);
    }

    #[test]
    fn rotation() {
        // A quarter turn about Z takes X to Y.
        let h = std::f64::consts::FRAC_1_SQRT_2;
        let m = compose([0.0; 3], [0.0, 0.0, h, h], [1.0; 3]);
        assert!((m[1][0] - 1.0).abs() < 1e-12 && m[0][0].abs() < 1e-12);
    }

    #[test]
    fn normals() {
        let n = square(0).computed_normals();
        assert_eq!(n, vec![[0.0, 0.0, 1.0]; 3]);
        let bad = Primitive { indices: vec![0, 1, 9], positions: vec![[0.0; 3]; 2], ..Primitive::default() };
        assert_eq!(bad.computed_normals().len(), 2);
    }
}
