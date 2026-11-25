//! The level's things as the editor shows and edits them, read from the level XML.
//!
//! This module has no drawing. An `Ent` says what a thing is, which XML it comes from and how to move, turn and
//! size it. `Look` says what the view draws for it. The renderer loads the models and builds the scene from that.
//!
//! Placement follows the gloveworld decompilation:
//!   rotation: GetQuaternionFromXYZRot (actor.c) builds q = qY·qZ·qX, and QuaternionToMatrix (maths.c) stores it
//!             transposed for N64 row vectors. R = Ry·Rz·Rx: Euler order 'YZX'.
//!   scale:    landscape matrices are T·R·S with guScaleF(scale.x, scale.y, scale.z) (object.c), no axis swap.

use crate::commands::{ensure_cmd, Format};
use crate::vec::{self, Quat, V3};
use crate::xmlutil::{fmt, hex_id, id_hex, num, num0, r1};
use glovebox_core::xml::{Doc, NodeId};
use serde_json::Value;
use std::f64::consts::PI;

/// An entity's place in the list `build` returns.
pub type EntId = usize;

/// The layers of the view: (kind, name, color).
pub const KINDS: [(&str, &str, &str); 12] = [
    ("land", "Landscape", "#8fa3b8"),
    ("platform", "Platforms", "#6fb7ff"),
    ("garib", "Garibs", "#f2c14e"),
    ("enemy", "Enemies", "#ff6b6b"),
    ("pickup", "Powerups", "#c58bff"),
    ("tip", "Mr. Tip", "#6bd88a"),
    ("spawn", "Spawns", "#ffffff"),
    ("water", "Water", "#3fa8e0"),
    ("wind", "Wind/Vents", "#9be7e0"),
    ("hazard", "Buzzers", "#ffd23f"),
    ("sound", "Sounds", "#b0b0b0"),
    ("trigger", "Puzzle triggers", "#ff8ad8"),
];

pub fn kind_color(kind: &str) -> &'static str {
    KINDS.iter().find(|k| k.0 == kind).map(|k| k.2).unwrap_or("#ffffff")
}

// Why some objects cannot rotate or scale: each reason is a fact about the level format or the game code.
pub const WHY_NO_ORIENTATION: &str = "No orientation in the level format. In a group rotation it moves around the pivot.";
pub const WHY_SPRITE: &str = "Garibs, Mr. Tip and the start markers are flat sprites or points in the game. They have no rotation or size to set. In a group rotation they move around the pivot.";
pub const WHY_NO_SCALE: &str = "No size in the level format.";
// Rotation and scale on these objects need the Expansion Pak game mod (mod/transforms.c), which Build ROM adds
pub const WHY_ROT_ENEMY: &str = "Turning about Y sets y_rotation, the facing its script starts from. Tilt about X or Z is a SetActorRotation the game mod applies when drawing.";
pub const WHY_ROT_PLATFORM: &str = "SetActorRotation, applied by the game mod before the platform\u{2019}s collision is built. For a spinning platform it adds to the spin angles.";
pub const WHY_ROT_PICKUP: &str = "SetActorRotation, applied by the game mod under the powerup\u{2019}s own spin about Y.";
pub const WHY_ROT_WATER: &str = "SetActorRotation, applied by the game mod to the water\u{2019}s model. Its swimming volume (left/top/front\u{2026}) stays an axis-aligned box.";
pub const WHY_SCALE_ENEMY: &str = "SetActorScale, a factor on the type\u{2019}s size. The game mod also scales the collision sphere by the mean of the three axes.";
pub const WHY_SCALE_PICKUP: &str = "SetActorScale, a factor on the powerup\u{2019}s size (0.4 in the game\u{2019}s code).";
pub const WHY_SCALE_WATER: &str = "SetActorScale on the water\u{2019}s model. Its swimming volume keeps its own size.";
pub const WHY_CUSTOM_PLATFORM: &str = "A platform must use a model from the game. A converted model\u{2019}s first rotation key is all zeros, and a console stops when it draws one as a platform (an emulator doesn\u{2019}t). Place it as Scenery or Background.";
pub const WHY_TRIGGER_ROT: &str = "Puzzle trigger volumes are axis-aligned: the game tests boxes with IsPointWithinBox, spheres and cylinders by distance.";
pub const WHY_TRIGGER_EDIT: &str = "This trigger volume is one of several conditions of its puzzle. It can\u{2019}t be duplicated or deleted on its own.";
pub const WHY_WIND_ROT: &str = "Wind volumes are axis-aligned boxes (left/top/front + width/height/depth).";
pub const WHY_START_ROT: &str = "Set by the first checkpoint platform\u{2019}s theta.";

pub const EULER_ORDER: &str = "YZX";

pub fn quat_from_xyz(e: V3) -> Quat {
    vec::quat_from_euler_yzx(e[0], e[1], e[2])
}
pub fn euler_of(q: Quat) -> V3 {
    vec::euler_yzx_from_quat(q)
}

/// SetActorRotation / SetActorScale inside an object. The game applies them to landscape and background actors.
/// For these parents the Expansion Pak game mod does (mod/transforms.c).
pub const MOD_PARENTS: [&str; 5] = ["Enemy", "Platform", "NullPlatform", "Powerup", "Water"];

fn trio(doc: &Doc, node: Option<NodeId>, ax: [&str; 3], d: f64) -> V3 {
    [num(doc, node, ax[0], d), num(doc, node, ax[1], d), num(doc, node, ax[2], d)]
}

const XYZ: [&str; 3] = ["x", "y", "z"];

pub fn child_rot(doc: &Doc, node: NodeId) -> V3 {
    trio(doc, doc.first(node, "SetActorRotation"), XYZ, 0.0)
}

pub fn child_scale(doc: &Doc, node: NodeId) -> V3 {
    trio(doc, doc.first(node, "SetActorScale"), XYZ, 1.0)
}

fn set_xyz(doc: &mut Doc, e: NodeId, v: V3) {
    for i in 0..3 {
        doc.set(e, XYZ[i], &fmt(v[i]));
    }
}

/// Sets a child's x, y and z. A missing child is not added for the value that changes nothing.
fn set_child_xyz(doc: &mut Doc, node: NodeId, tag: &str, v: V3, identity: f64) {
    let c = doc.first(node, tag);
    if c.is_none() && (0..3).all(|i| (v[i] - identity).abs() < 1e-6) {
        return;
    }
    let e = match c {
        Some(c) => c,
        None => ensure_cmd(doc, node, tag, &[]),
    };
    set_xyz(doc, e, v);
}

pub fn set_child_rot(doc: &mut Doc, node: NodeId, v: V3) {
    set_child_xyz(doc, node, "SetActorRotation", v, 0.0)
}

pub fn set_child_scale(doc: &mut Doc, node: NodeId, v: V3) {
    set_child_xyz(doc, node, "SetActorScale", v, 1.0)
}

/// A spin's axis as 0, 1 or 2. A value the format does not know reads as y (the checks report it).
pub fn axis_index(doc: &Doc, spin: NodeId) -> usize {
    match doc.get(spin, "axis") {
        Some("x") => 0,
        Some("z") => 2,
        _ => 1,
    }
}

/// True when the object needs the game mod for its rotation or scale.
pub fn uses_mod(doc: &Doc, node: NodeId) -> bool {
    MOD_PARENTS.contains(&doc.tag(node))
        && (doc.first(node, "SetActorRotation").is_some() || doc.first(node, "SetActorScale").is_some())
}

/// The angles a platform's spin starts from, on the spin's axis.
pub fn spin_angles(doc: &Doc, node: NodeId) -> V3 {
    let mut v = [0.0; 3];
    if let Some(s) = doc.first(node, "PlatConstantSpin") {
        v[axis_index(doc, s)] = num0(doc, s, "initial_theta");
    }
    v
}

/// The point a `Shift` binding reads.
#[derive(Clone, Debug, PartialEq)]
pub enum Anchor {
    /// Three attributes.
    Xyz([&'static str; 3]),
    /// The middle of a box stored as a corner and a size.
    BoxCenter { corner: [&'static str; 3], size: [&'static str; 3] },
    /// The middle of a trigger cylinder: x, the mean of min_y and max_y, z.
    Cylinder,
}

/// A point stored in the XML, with how to read and write it.
#[derive(Clone, Debug, PartialEq)]
pub enum Binding {
    /// Three attributes of a node. `parent`: a platform's parent platform, whose place is added.
    Xyz { node: NodeId, ax: [&'static str; 3], parent: Option<EntId> },
    /// A box stored as a corner and a size, read and moved by its center.
    BoxCenter { node: NodeId, corner: [&'static str; 3], size: [&'static str; 3] },
    /// Attributes that shift together by the movement of an anchor point. Each triple names the x, y and z
    /// attributes of one point.
    Shift { node: NodeId, triples: Vec<[Option<&'static str>; 3]>, anchor: Anchor },
    /// The middle of a buzzer's two ends.
    Mid { node: NodeId },
    /// A platform with no position yet. It reads as its parent's place. Writing adds a PlatSetInitialPos.
    NewPlatformPos { node: NodeId, parent: Option<EntId> },
}

const END_1: [&str; 3] = ["end_1_x", "end_1_y", "end_1_z"];
const END_2: [&str; 3] = ["end_2_x", "end_2_y", "end_2_z"];

fn parent_offset(doc: &Doc, ents: &[Ent], parent: Option<EntId>) -> V3 {
    match parent.and_then(|p| ents.get(p)) {
        Some(e) => e.world_pos(doc, ents),
        None => [0.0; 3],
    }
}

impl Binding {
    /// The XML node the point is stored in.
    pub fn node(&self) -> NodeId {
        match self {
            Binding::Xyz { node, .. }
            | Binding::BoxCenter { node, .. }
            | Binding::Shift { node, .. }
            | Binding::Mid { node }
            | Binding::NewPlatformPos { node, .. } => *node,
        }
    }

    /// The point. `ents` is the list this binding's entity is in.
    pub fn get(&self, doc: &Doc, ents: &[Ent]) -> V3 {
        match self {
            Binding::Xyz { node, ax, parent } => vec::add(trio(doc, Some(*node), *ax, 0.0), parent_offset(doc, ents, *parent)),
            Binding::BoxCenter { node, corner, size } => box_center(doc, *node, *corner, *size),
            Binding::Shift { node, anchor, .. } => match anchor {
                Anchor::Xyz(ax) => vec::add(trio(doc, Some(*node), *ax, 0.0), [0.0; 3]),
                Anchor::BoxCenter { corner, size } => {
                    vec::add(trio(doc, Some(*node), *corner, 0.0), vec::scale(trio(doc, Some(*node), *size, 0.0), 0.5))
                }
                Anchor::Cylinder => cylinder_center(doc, *node),
            },
            Binding::Mid { node } => {
                let (a, b) = (trio(doc, Some(*node), END_1, 0.0), trio(doc, Some(*node), END_2, 0.0));
                vec::scale(vec::add(vec::add(a, [0.0; 3]), vec::add(b, [0.0; 3])), 0.5)
            }
            Binding::NewPlatformPos { parent, .. } => parent_offset(doc, ents, *parent),
        }
    }

    /// Moves the point to `v`.
    pub fn set(&self, doc: &mut Doc, ents: &[Ent], v: V3) {
        match self {
            Binding::Xyz { node, ax, parent } => {
                let o = parent_offset(doc, ents, *parent);
                for i in 0..3 {
                    doc.set(*node, ax[i], &fmt(v[i] - o[i]));
                }
            }
            Binding::BoxCenter { node, corner, size } => {
                for i in 0..3 {
                    let c = v[i] - num0(doc, *node, size[i]) / 2.0;
                    doc.set(*node, corner[i], &fmt(c));
                }
            }
            Binding::Shift { node, triples, .. } => {
                let d = vec::sub(v, self.get(doc, ents));
                for t in triples {
                    for i in 0..3 {
                        if let Some(a) = t[i] {
                            let n = num0(doc, *node, a) + d[i];
                            doc.set(*node, a, &fmt(n));
                        }
                    }
                }
            }
            Binding::Mid { node } => {
                let d = vec::sub(v, self.get(doc, ents));
                for ax in [END_1, END_2] {
                    let p = vec::add(vec::add(trio(doc, Some(*node), ax, 0.0), [0.0; 3]), d);
                    for i in 0..3 {
                        doc.set(*node, ax[i], &fmt(p[i] - 0.0));
                    }
                }
            }
            Binding::NewPlatformPos { node, parent } => {
                let o = parent_offset(doc, ents, *parent);
                let p = ensure_cmd(doc, *node, "PlatSetInitialPos", &[]);
                set_xyz(doc, p, vec::sub(v, o));
            }
        }
    }
}

fn box_center(doc: &Doc, node: NodeId, corner: [&str; 3], size: [&str; 3]) -> V3 {
    let mut out = [0.0; 3];
    for i in 0..3 {
        out[i] = num0(doc, node, corner[i]) + num0(doc, node, size[i]) / 2.0;
    }
    out
}

fn cylinder_center(doc: &Doc, vol: NodeId) -> V3 {
    [num0(doc, vol, "x"), (num0(doc, vol, "min_y") + num0(doc, vol, "max_y")) / 2.0, num0(doc, vol, "z")]
}

fn static_axes(pre: &str) -> Option<[&'static str; 3]> {
    Some(match pre {
        "home" => ["home_x", "home_y", "home_z"],
        "destination" => ["destination_x", "destination_y", "destination_z"],
        "lookat" => ["lookat_x", "lookat_y", "lookat_z"],
        _ => return None,
    })
}

/// Absolute points that belong to an object and must follow it: enemy patrol and target points (their scripts use
/// world coordinates), enemy attention boxes, platform start and path points and orbit centers (relative to a
/// parent platform, if any). Boxes stored as corner and size follow by their center.
pub fn attached_points(doc: &Doc, node: NodeId, parent: Option<EntId>) -> Vec<Binding> {
    let mut out = Vec::new();
    for e in doc.iter(node).into_iter().skip(1) {
        match doc.tag(e) {
            "PlatSetInitialPos" | "PlatPathPoint" | "PlatOrbitAroundPoint" => {
                out.push(Binding::Xyz { node: e, ax: XYZ, parent });
                continue;
            }
            "EnemySetAttentionBbox" => {
                out.push(Binding::BoxCenter { node: e, corner: ["left", "top", "front"], size: ["width", "height", "depth"] });
                continue;
            }
            _ => {}
        }
        for pre in ["home", "destination", "lookat"] {
            let Some(ax) = static_axes(pre) else { continue };
            if !doc.has(e, ax[0]) {
                continue;
            }
            if pre == "home" && doc.has(e, "extent_x") {
                out.push(Binding::BoxCenter { node: e, corner: ax, size: ["extent_x", "extent_y", "extent_z"] });
            } else {
                out.push(Binding::Xyz { node: e, ax, parent: None });
            }
        }
    }
    out
}

/// How an object's rotation and scale are stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Xform {
    /// Landscape and background actors: SetActorRotation and SetActorScale, which the game itself applies.
    Actor,
    /// Water and powerups: the same children, applied by the game mod. A value that changes nothing adds no child.
    Child,
    /// Platforms: SetActorRotation plus the spin's first angle, and PlatScale.
    Platform,
    /// Enemies: y_rotation for the turn about Y, SetActorRotation for the tilt, SetActorScale.
    Enemy,
    /// Volumes: no rotation, and a scale of 1 that `Volume::set_box` turns into a size.
    Volume,
}

/// The shape of a volume the author can size.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Shape {
    /// A min corner plus a size.
    Box { corner: [&'static str; 3], size: [&'static str; 3] },
    /// x, y, z and range.
    Sphere,
    /// x, z, range, min_y and max_y.
    Cylinder,
}

/// An axis-aligned volume stored in one node (wind volumes, puzzle trigger volumes). Moving shifts it. Scaling
/// resizes it about its center.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Volume {
    pub node: NodeId,
    pub shape: Shape,
}

/// The farthest of these scale factors from 1: a drag on any one gizmo axis resizes a radius.
pub fn dominant(f: &[f64]) -> f64 {
    let mut out = 1.0;
    for b in f {
        if (b - 1.0).abs() > (out - 1.0f64).abs() {
            out = *b;
        }
    }
    out
}

impl Volume {
    fn sz(&self, doc: &Doc, size: [&str; 3]) -> V3 {
        trio(doc, Some(self.node), size, 0.0)
    }

    fn set_size(&self, doc: &mut Doc, corner: [&str; 3], size: [&str; 3], s: V3) {
        let c = vec::add(trio(doc, Some(self.node), corner, 0.0), vec::scale(self.sz(doc, size), 0.5));
        for i in 0..3 {
            doc.set(self.node, size[i], &fmt(s[i]));
            doc.set(self.node, corner[i], &fmt(c[i] - s[i] / 2.0));
        }
    }

    fn set_height(&self, doc: &mut Doc, h: f64) {
        let c = cylinder_center(doc, self.node)[1];
        doc.set(self.node, "min_y", &fmt(c - h / 2.0));
        doc.set(self.node, "max_y", &fmt(c + h / 2.0));
    }

    fn range(&self, doc: &Doc) -> f64 {
        num0(doc, self.node, "range")
    }

    fn height(&self, doc: &Doc) -> f64 {
        num0(doc, self.node, "max_y") - num0(doc, self.node, "min_y")
    }

    /// The size of the box around the volume, as a scale drag starts from it.
    pub fn box_size0(&self, doc: &Doc) -> V3 {
        match &self.shape {
            Shape::Box { size, .. } => self.sz(doc, *size),
            Shape::Sphere => [2.0 * self.range(doc); 3],
            Shape::Cylinder => [2.0 * self.range(doc), self.height(doc), 2.0 * self.range(doc)],
        }
    }

    /// Resizes the volume to the size `s0` times the factors `f`.
    pub fn set_box(&self, doc: &mut Doc, s0: V3, f: V3) {
        match &self.shape {
            Shape::Box { corner, size } => self.set_size(doc, *corner, *size, [s0[0] * f[0], s0[1] * f[1], s0[2] * f[2]]),
            Shape::Sphere => doc.set(self.node, "range", &fmt((s0[0] / 2.0) * dominant(&f))),
            Shape::Cylinder => {
                doc.set(self.node, "range", &fmt((s0[0] / 2.0) * dominant(&[f[0], f[2]])));
                self.set_height(doc, s0[1] * f[1]);
            }
        }
    }

    /// The names of the sizes the inspector shows.
    pub fn size_fields(&self) -> Vec<&'static str> {
        match &self.shape {
            Shape::Box { size, .. } => size.to_vec(),
            Shape::Sphere => vec!["range"],
            Shape::Cylinder => vec!["range", "height"],
        }
    }

    /// One of `size_fields`.
    pub fn size_get(&self, doc: &Doc, i: usize) -> f64 {
        match &self.shape {
            Shape::Box { size, .. } => size.get(i).map(|a| num0(doc, self.node, a)).unwrap_or(0.0),
            Shape::Cylinder if i == 1 => self.height(doc),
            _ => self.range(doc),
        }
    }

    pub fn size_set(&self, doc: &mut Doc, i: usize, v: f64) {
        match &self.shape {
            Shape::Box { corner, size } => {
                let mut s = self.sz(doc, *size);
                if i < 3 {
                    s[i] = v;
                    self.set_size(doc, *corner, *size, s);
                }
            }
            Shape::Cylinder if i == 1 => self.set_height(doc, v),
            _ => doc.set(self.node, "range", &fmt(v)),
        }
    }

    /// The scale of the marker: a unit box, a unit sphere or a unit cylinder. A trigger's is at least 0.5.
    pub fn dims(&self, doc: &Doc) -> V3 {
        match &self.shape {
            Shape::Box { size, .. } => self.sz(doc, *size),
            Shape::Sphere => [self.range(doc); 3],
            Shape::Cylinder => [self.range(doc), self.height(doc), self.range(doc)],
        }
    }
}

/// What the view draws for an entity.
#[derive(Clone, Debug, PartialEq)]
pub enum Look {
    /// The model `object_id`, scaled by `scale`. `water`: drawn see-through. `decorative`: nothing is drawn when
    /// the model is missing (the others get a placeholder box).
    Model { scale: f64, water: bool, decorative: bool },
    /// A sprite with one glyph.
    Icon { glyph: &'static str, color: &'static str, size: u32, opacity: f64 },
    /// A wind volume's box of lines.
    WindBox,
    /// A buzzer's line between its ends.
    BuzzerLine,
    /// A small ball: a buzzer end.
    Ball,
    /// A small octahedron: a path point.
    PathPoint,
    /// A trigger volume: lines, a faint fill and an icon. `who`: "Glover", "ball" or "camera".
    Trigger { who: &'static str, color: &'static str, glyph: &'static str },
    /// The marker where Glover starts: a hand and an arrow, turned by the gate's theta.
    Start,
}

/// One thing in the view.
#[derive(Clone, Debug, PartialEq)]
pub struct Ent {
    /// The layer: one of `KINDS`.
    pub kind: &'static str,
    /// The top-level element it comes from, and that element's tag.
    pub node: NodeId,
    pub tag: String,
    /// The element inside `node` this entity stands for: a garib, a path point, a trigger volume.
    pub sub: Option<NodeId>,
    pub label: String,
    /// Its position.
    pub bind: Option<Binding>,
    /// Points that follow every move, rotation and scale of it.
    pub points: Vec<Binding>,
    /// How it turns and scales: "xyz", "box" or nothing.
    pub rot: Option<&'static str>,
    pub scl: Option<&'static str>,
    /// Why it cannot turn or scale, or what the rotation and the scale are stored as.
    pub why_rot: Option<&'static str>,
    pub why_scl: Option<&'static str>,
    pub xform: Option<Xform>,
    pub volume: Option<Volume>,
    pub look: Look,
    /// More sprites on it: (glyph, color, size, offset).
    pub icons: Vec<(&'static str, &'static str, u32, V3)>,
    pub object_id: Option<String>,
    /// The model is in the catalog and not in the banks the level loads.
    pub imported: bool,
    /// The level names a model the editor does not have. The renderer also sets it when a model fails to load.
    pub missing_model: bool,
    /// A handle on a part of another entity (a path point, a buzzer end), and that entity.
    pub is_handle: bool,
    pub owner: Option<EntId>,
    /// Made from another entity and not selectable itself. A click selects `selects`.
    pub derived: bool,
    pub selects: Option<EntId>,
    pub plat_tag: Option<f64>,
    pub parent_tag: Option<f64>,
    pub parent_ent: Option<EntId>,
    /// A platform's PlatCheckpoint.
    pub gate: Option<NodeId>,
    /// A trigger volume of a camera zone.
    pub zone: bool,
    /// A platform's constant spin: (axis, angle per frame). The view turns it at 30 frames a second.
    pub spin: Option<(usize, f64)>,
    /// A platform's path points, when the view draws its path.
    pub path: Vec<NodeId>,
    /// A model drawn with the marker when the editor has it: (object id, scale). The ball at its spawn point.
    pub decor: Option<(&'static str, f64)>,
}

impl Ent {
    fn new(kind: &'static str, node: NodeId, tag: &str, look: Look) -> Ent {
        Ent {
            kind,
            node,
            tag: tag.to_string(),
            sub: None,
            label: String::new(),
            bind: None,
            points: Vec::new(),
            rot: None,
            scl: None,
            why_rot: None,
            why_scl: None,
            xform: None,
            volume: None,
            look,
            icons: Vec::new(),
            object_id: None,
            imported: false,
            missing_model: false,
            is_handle: false,
            owner: None,
            derived: false,
            selects: None,
            plat_tag: None,
            parent_tag: None,
            parent_ent: None,
            gate: None,
            zone: false,
            spin: None,
            path: Vec::new(),
            decor: None,
        }
    }

    fn why(mut self, rot: &'static str, scl: &'static str) -> Ent {
        self.why_rot = Some(rot);
        self.why_scl = Some(scl);
        self
    }

    /// Where the entity is. A derived entity is where the entity it selects is.
    pub fn world_pos(&self, doc: &Doc, ents: &[Ent]) -> V3 {
        match (&self.bind, self.selects) {
            (Some(b), _) => b.get(doc, ents),
            (None, Some(s)) if self.derived => ents.get(s).map(|e| e.world_pos(doc, ents)).unwrap_or([0.0; 3]),
            _ => [0.0; 3],
        }
    }

    /// The rotation, for the entities that have one.
    pub fn get_quat(&self, doc: &Doc) -> Option<Quat> {
        let n = self.node;
        Some(match self.xform? {
            Xform::Actor | Xform::Child => quat_from_xyz(child_rot(doc, n)),
            Xform::Platform => quat_from_xyz(vec::add(child_rot(doc, n), spin_angles(doc, n))),
            Xform::Enemy => {
                let b = child_rot(doc, n);
                quat_from_xyz([b[0], num0(doc, n, "y_rotation") + b[1], b[2]])
            }
            Xform::Volume => return None,
        })
    }

    pub fn set_quat(&self, doc: &mut Doc, q: Quat) {
        let n = self.node;
        let e = euler_of(q);
        match self.xform {
            Some(Xform::Actor) => {
                let r = ensure_cmd(doc, n, "SetActorRotation", &[]);
                set_xyz(doc, r, e);
            }
            Some(Xform::Child) => set_child_rot(doc, n, e),
            Some(Xform::Platform) => {
                let sa = spin_angles(doc, n);
                set_child_rot(doc, n, vec::sub(e, sa));
            }
            Some(Xform::Enemy) => {
                doc.set(n, "y_rotation", &fmt(e[1]));
                set_child_rot(doc, n, [e[0], 0.0, e[2]]);
            }
            _ => {}
        }
    }

    /// The scale. A volume's is 1: its size is in `volume`.
    pub fn get_scale(&self, doc: &Doc) -> Option<V3> {
        Some(match self.xform? {
            Xform::Actor | Xform::Child | Xform::Enemy => child_scale(doc, self.node),
            Xform::Platform => trio(doc, doc.first(self.node, "PlatScale"), XYZ, 1.0),
            Xform::Volume => [1.0; 3],
        })
    }

    pub fn set_scale(&self, doc: &mut Doc, v: V3) {
        let n = self.node;
        match self.xform {
            Some(Xform::Actor) => {
                let s = ensure_cmd(doc, n, "SetActorScale", &[]);
                set_xyz(doc, s, v);
            }
            Some(Xform::Child) | Some(Xform::Enemy) => set_child_scale(doc, n, v),
            Some(Xform::Platform) => {
                let s = ensure_cmd(doc, n, "PlatScale", &[]);
                set_xyz(doc, s, v);
            }
            _ => {}
        }
    }

    /// The angles the view turns a spinning platform to after `t` seconds.
    pub fn spin_euler(&self, doc: &Doc, t: f64) -> Option<V3> {
        let (axis, speed) = self.spin?;
        let mut e = vec::add(child_rot(doc, self.node), spin_angles(doc, self.node));
        e[axis] += speed * t * 30.0;
        Some(e)
    }

    /// The turn of the start marker: about Y by the gate's theta.
    pub fn start_quat(&self, doc: &Doc, ents: &[Ent]) -> Option<Quat> {
        let gate = ents.get(self.selects?)?.gate?;
        Some(vec::quat_axis_angle([0.0, 1.0, 0.0], num0(doc, gate, "theta")))
    }
}

/// The swimming volume of a Water element, as the center and the size of the box the view draws.
pub fn water_volume(doc: &Doc, node: NodeId) -> (V3, V3) {
    let n = |a: &str| num0(doc, node, a);
    let sum = n("top") + n("bottom");
    let surface = n("surface_y");
    // The lower of the two. A NaN `surface` gives `sum`
    let top = if surface < sum { surface } else { sum };
    let h = top - n("top");
    (
        [n("left") + n("width") / 2.0, (n("top") + top) / 2.0, n("front") + n("depth") / 2.0],
        [n("width"), if h > 1.0 { h } else { 1.0 }, n("depth")],
    )
}

// Puzzle conditions that test where Glover, the ball or the camera is. parse.c (TOKEN_CONDITION) reads condition
// ids 35-41 into a PuzzleCamCond, and PuzzleEvaluateCondition (puzzle.c) tests them: a box is a min corner plus a
// size (IsPointWithinBox), a sphere is a center and a radius, and the cylinder is a center x/z and a radius between
// min_y and max_y. Id 34 (PuzzleCond0x22, CONDITION_HAND_CROSSES_LINE) is always true and has nothing to draw.

/// The shape and the subject of a trigger volume element: ("box" | "sphere" | "cylinder", who).
pub fn trigger_spec(tag: &str) -> Option<(&'static str, &'static str)> {
    Some(match tag {
        "PuzzleCondGloverWithinVolume" => ("box", "Glover"),
        "PuzzleCondBallWithinVolume" => ("box", "ball"),
        "PuzzleCondCameraWithinVolume" => ("box", "camera"),
        "PuzzleCondGloverWithinRangeOfPoint" => ("sphere", "Glover"),
        "PuzzleCondBallWithinRangeOfPoint" => ("sphere", "ball"),
        "PuzzleCondCameraWithinRangeOfPoint" => ("sphere", "camera"),
        "PuzzleCondGloverWithinRangeOfPoint2" => ("cylinder", "Glover"),
        _ => return None,
    })
}

fn trigger_look(who: &str) -> (&'static str, &'static str) {
    match who {
        "Glover" => ("#ff8ad8", "\u{270b}"),
        "ball" => ("#ffb36b", "\u{25cf}"),
        _ => ("#b28dff", "\u{25c9}"),
    }
}

/// The trigger volumes of a puzzle.
pub fn trigger_volumes(doc: &Doc, puzzle: NodeId) -> Vec<NodeId> {
    doc.kids(puzzle, "PuzzleCond")
        .iter()
        .flat_map(|c| doc.children(*c).to_vec())
        .filter(|v| trigger_spec(doc.tag(*v)).is_some())
        .collect()
}

/// A puzzle with one trigger volume (a camera zone, for one) is duplicated and deleted whole with its volume.
pub fn lone_trigger(doc: &Doc, ent: &Ent) -> bool {
    ent.kind == "trigger" && trigger_volumes(doc, ent.node).len() == 1
}

/// A color "#rrggbb" as three numbers from 0 to 1.
pub fn hex_rgb(h: &str) -> [f64; 3] {
    let h = h.trim_start_matches('#');
    let part = |i: usize| h.get(i..i + 2).and_then(|s| u8::from_str_radix(s, 16).ok()).unwrap_or(0) as f64 / 255.0;
    [part(0), part(2), part(4)]
}

/// The ball's model.
pub const BALL_MODEL: &str = "0x8E4DDE49";

/// What `build` asks about the models. The renderer or the editor's catalog answers.
pub trait Models {
    /// True when the editor has a model with this id: one of the level's own or one of the catalog.
    fn has_model(&self, object_id: &str) -> bool;
    /// True when the model is in the catalog and not in the banks the level loads. It is imported at ROM build.
    fn is_import(&self, object_id: &str) -> bool;
}

/// The value of an enum name of a field: the number the format gives the name, or the name as a decimal number.
pub fn enum_value(fmt_: &Format, tag: &str, field: &str, name: Option<&str>) -> Option<i64> {
    if let Some(en) = fmt_.enum_for(tag, field).filter(|e| !e.is_empty()) {
        if let Some((k, _)) = en.iter().find(|(_, v)| name.is_some() && v.as_str() == name) {
            return int10(k);
        }
    }
    int10(name?)
}

/// A decimal whole number within the i64 range, as `pyfmt::parse_int` reads one without a prefix.
fn int10(s: &str) -> Option<i64> {
    let t = s.trim();
    let (neg, digits) = match t.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    let b = digits.as_bytes();
    let ok = !b.is_empty()
        && b.iter().enumerate().all(|(i, c)| {
            c.is_ascii_digit() || (*c == b'_' && i > 0 && i + 1 < b.len() && b[i - 1].is_ascii_digit() && b[i + 1].is_ascii_digit())
        });
    if !ok {
        return None;
    }
    let v: i64 = digits.replace('_', "").parse().ok()?;
    Some(if neg { -v } else { v })
}

/// The decompilation's name for a value of one of its enums.
pub fn decomp_enum_name<'a>(decomp: &'a Value, enum_: &str, value: Option<i64>) -> Option<&'a str> {
    let value = value?;
    decomp.get("decompEnums")?.get(enum_)?.as_object()?.iter().find(|(_, v)| v.as_i64() == Some(value)).map(|(k, _)| k.as_str())
}

fn table_id(tables: &Value, name: &str, i: Option<i64>) -> Option<String> {
    let i = usize::try_from(i?).ok()?;
    let v = tables.get(name)?.as_array()?.get(i)?.get("value")?;
    let n = v.as_i64().or_else(|| v.as_f64().map(|f| f as i64))?;
    if n == 0 {
        None
    } else {
        Some(id_hex(n))
    }
}

/// The model of an enemy type. The hub's chicken differs by level.
pub fn enemy_model_id(fmt_: &Format, tables: &Value, scene_id: &str, type_name: Option<&str>) -> Option<String> {
    let tv = enum_value(fmt_, "Enemy", "type", type_name);
    if type_name.map(|t| t.starts_with("hubchicken")).unwrap_or(false) {
        return Some(id_hex(match scene_id {
            "00" | "01" | "02" => 0x11CD1E6C,
            "03" | "04" => 0x58C079E1,
            _ => 0x83D7D176,
        }));
    }
    table_id(tables, "enemyObjects", tv)
}

/// The model of a powerup type.
pub fn powerup_model_id(tables: &Value, t: i64) -> Option<String> {
    table_id(tables, "powerupObjects", Some(t))
}

/// What `build` reads besides the XML.
pub struct Context<'a> {
    pub format: &'a Format<'a>,
    /// The table of enemy and powerup models (`tables::noclip_tables()`).
    pub tables: &'a Value,
    /// The level's id, two hex digits.
    pub scene_id: &'a str,
    pub models: &'a dyn Models,
}

/// The entities of a level and the elements that set its fog and ambient light.
#[derive(Clone, Debug, PartialEq)]
pub struct Level {
    pub ents: Vec<Ent>,
    pub fog: Option<NodeId>,
    pub ambient: Option<NodeId>,
}

fn attach_model(cx: &Context, ent: &mut Ent, scale: f64, water: bool, decorative: bool) {
    let oid = ent.object_id.as_deref();
    ent.imported = oid.map(|o| cx.models.is_import(o)).unwrap_or(false);
    let have = oid.map(|o| cx.models.has_model(o)).unwrap_or(false);
    if !have && !decorative {
        ent.missing_model = oid.is_some();
    }
    ent.look = Look::Model { scale, water, decorative };
}

fn trigger_entity(node: NodeId, vol: NodeId, spec: (&'static str, &'static str), label: String) -> Ent {
    let (shape, who) = spec;
    let (color, glyph) = trigger_look(who);
    let mut ent = Ent::new("trigger", node, "Puzzle", Look::Trigger { who, color, glyph });
    ent.sub = Some(vol);
    ent.label = label;
    ent.why_rot = Some(WHY_TRIGGER_ROT);
    ent.scl = Some("box");
    ent.xform = Some(Xform::Volume);
    let (bind, shape) = match shape {
        "box" => {
            let (corner, size) = (XYZ, ["l", "w", "h"]);
            (
                Binding::Shift { node: vol, triples: vec![corner.map(Some)], anchor: Anchor::BoxCenter { corner, size } },
                Shape::Box { corner, size },
            )
        }
        "sphere" => (Binding::Xyz { node: vol, ax: XYZ, parent: None }, Shape::Sphere),
        _ => (
            Binding::Shift {
                node: vol,
                triples: vec![[Some("x"), None, Some("z")], [None, Some("min_y"), None], [None, Some("max_y"), None]],
                anchor: Anchor::Cylinder,
            },
            Shape::Cylinder,
        ),
    };
    ent.bind = Some(bind);
    ent.volume = Some(Volume { node: vol, shape });
    ent
}

/// Reads the level into entities, in the XML's order.
pub fn build(doc: &Doc, root: NodeId, cx: &Context) -> Level {
    let mut ents: Vec<Ent> = Vec::new();
    let mut platforms_by_tag: Vec<(f64, EntId)> = Vec::new();
    let (mut fog, mut ambient) = (None, None);
    let mut first_gate: Option<EntId> = None;
    let mut puzzle_index = 0;
    for node in doc.children(root).to_vec() {
        let tag = doc.tag(node);
        match tag {
            "FogConfiguration" => fog = Some(node),
            "AmbientLight" => ambient = Some(node),
            "LandActor" | "BackgroundActor" | "AnimatedBackgroundActor" | "Water" => {
                let water = tag == "Water";
                let mut ent = Ent::new(if water { "water" } else { "land" }, node, tag, Look::Start);
                ent.object_id = hex_id(doc.get(node, "object_id"));
                ent.label = nonempty(doc.get(node, "name")).or(ent.object_id.as_deref()).unwrap_or(tag).to_string();
                ent.rot = Some("xyz");
                ent.scl = Some("xyz");
                if water {
                    ent.bind = Some(Binding::Shift {
                        node,
                        triples: vec![XYZ.map(Some), ["left", "top", "front"].map(Some), [None, Some("surface_y"), None]],
                        anchor: Anchor::Xyz(XYZ),
                    });
                    ent.why_rot = Some(WHY_ROT_WATER);
                    ent.why_scl = Some(WHY_SCALE_WATER);
                    ent.xform = Some(Xform::Child);
                } else {
                    ent.bind = Some(Binding::Xyz { node, ax: XYZ, parent: None });
                    ent.xform = Some(Xform::Actor);
                }
                attach_model(cx, &mut ent, 1.0, water, false);
                ents.push(ent);
            }
            "Platform" | "NullPlatform" => {
                let id = ents.len();
                let mut ent = Ent::new("platform", node, tag, Look::Start);
                ent.object_id = hex_id(doc.get(node, "object_id"));
                ent.label = match nonempty(doc.get(node, "name")) {
                    Some(n) => n.to_string(),
                    None if tag == "NullPlatform" => "null platform".to_string(),
                    None => ent.object_id.clone().unwrap_or_else(|| tag.to_string()),
                };
                if let Some(t) = doc.first(node, "PlatSetTag") {
                    let v = num0(doc, t, "tag");
                    ent.plat_tag = Some(v);
                    // A NaN never finds its entry again
                    match platforms_by_tag.iter_mut().find(|(k, _)| *k == v) {
                        Some(e) => e.1 = id,
                        None => platforms_by_tag.push((v, id)),
                    }
                }
                if let Some(p) = doc.first(node, "PlatSetParent") {
                    let v = num0(doc, p, "tag");
                    ent.parent_tag = Some(v);
                    // A platform that names itself as its parent has no parent here.
                    ent.parent_ent = platforms_by_tag.iter().find(|(k, _)| *k == v).map(|(_, e)| *e).filter(|e| *e != id);
                }
                let parent = ent.parent_ent;
                let pos_node = doc.first(node, "PlatSetInitialPos");
                let path_pts = doc.kids(node, "PlatPathPoint");
                // The platform's position is its start (PlatSetInitialPos, else the first path point). The rest of
                // its path and its orbit center are attached points that follow every move, rotation and scale.
                let anchor = pos_node.or(path_pts.first().copied());
                let mut pts = attached_points(doc, node, parent);
                let at = anchor.and_then(|a| pts.iter().position(|b| b.node() == a));
                ent.bind = Some(match at {
                    Some(i) => pts.remove(i),
                    None => Binding::NewPlatformPos { node, parent },
                });
                ent.points = pts;
                ent.rot = Some("xyz");
                ent.scl = Some("xyz");
                ent.why_rot = Some(WHY_ROT_PLATFORM);
                ent.xform = Some(Xform::Platform);
                if let Some(sp) = doc.first(node, "PlatConstantSpin") {
                    let speed = num0(doc, sp, "speed");
                    if speed.abs() > 1e-6 {
                        ent.spin = Some((axis_index(doc, sp), speed));
                    }
                }
                if tag == "Platform" {
                    let scale = if ent.object_id.as_deref() == Some("0x7FDADB91") { 1.5 } else { 1.0 };
                    attach_model(cx, &mut ent, scale, false, false);
                } else {
                    ent.look = Look::Icon { glyph: "\u{25c7}", color: kind_color("platform"), size: 12, opacity: 1.0 };
                }
                let with_path = path_pts.len() > 1 || (!path_pts.is_empty() && pos_node.is_some());
                if with_path {
                    ent.path = path_pts.clone();
                }
                if let Some(gate) = doc.first(node, "PlatCheckpoint") {
                    ent.gate = Some(gate);
                    if first_gate.is_none() {
                        first_gate = Some(id);
                    }
                }
                if doc.first(node, "SetExit").is_some() {
                    ent.icons.push(("\u{21e5}", "#ffffff", 16, [0.0, 30.0, 0.0]));
                }
                let label = ent.label.clone();
                ents.push(ent);
                if with_path {
                    for (i, p) in path_pts.iter().enumerate() {
                        let mut sub = Ent::new("platform", node, tag, Look::PathPoint).why(WHY_NO_ORIENTATION, WHY_NO_SCALE);
                        sub.bind = Some(Binding::Xyz { node: *p, ax: XYZ, parent });
                        sub.sub = Some(*p);
                        sub.label = format!("{} \u{b7} point {}", label, i);
                        sub.is_handle = true;
                        sub.owner = Some(id);
                        ents.push(sub);
                    }
                }
            }
            "GaribGroup" => {
                for g in doc.kids(node, "Garib") {
                    let t = doc.get(g, "type");
                    let (color, glyph) = match t {
                        Some("bang_500pt") => ("#ff9f43", "!"),
                        Some("extra_life") => ("#6bd88a", "\u{2665}"),
                        Some("mad_garib") => ("#ff6bd5", "\u{2738}"),
                        _ => ("#f2c14e", "\u{2726}"),
                    };
                    let opacity = if num0(doc, node, "initial_state") == 0.0 { 0.45 } else { 1.0 };
                    let mut ent =
                        Ent::new("garib", node, tag, Look::Icon { glyph, color, size: 12, opacity }).why(WHY_SPRITE, WHY_SPRITE);
                    ent.bind = Some(Binding::Xyz { node: g, ax: XYZ, parent: None });
                    ent.sub = Some(g);
                    ent.label = format!("{} \u{b7} group {}", t.unwrap_or("None"), doc.get(node, "group_id").unwrap_or("None"));
                    ents.push(ent);
                }
            }
            "Enemy" => {
                let mut ent = Ent::new("enemy", node, tag, Look::Start).why(WHY_ROT_ENEMY, WHY_SCALE_ENEMY);
                ent.bind = Some(Binding::Xyz { node, ax: XYZ, parent: None });
                ent.points = attached_points(doc, node, None);
                let type_name = doc.get(node, "type");
                let tv = enum_value(cx.format, "Enemy", "type", type_name);
                let actor = decomp_enum_name(cx.format.decomp, "ActorType", tv);
                ent.label = format!(
                    "{}{}",
                    nonempty(type_name).unwrap_or("?"),
                    actor.filter(|a| !a.is_empty()).map(|a| format!(" ({})", a)).unwrap_or_default()
                );
                ent.object_id = enemy_model_id(cx.format, cx.tables, cx.scene_id, type_name);
                ent.rot = Some("xyz");
                ent.scl = Some("xyz");
                ent.xform = Some(Xform::Enemy);
                let sc = tv
                    .and_then(|t| usize::try_from(t).ok())
                    .and_then(|t| cx.tables.get("enemyScales")?.as_array()?.get(t))
                    .map(|s| s.get("value").and_then(|v| v.as_f64()).unwrap_or(0.0))
                    .unwrap_or(0.05);
                attach_model(cx, &mut ent, if sc != 0.0 { sc } else { 0.05 }, false, false);
                ents.push(ent);
            }
            "Powerup" => {
                let v = num0(doc, node, "type");
                // A type that is not a number reads as type 0.
                let t = if v.is_finite() { v.trunc() as i64 } else { 0 };
                let name = decomp_enum_name(cx.format.decomp, "Pickup", Some(t)).filter(|n| !n.is_empty());
                let mut ent = Ent::new("pickup", node, tag, Look::Start).why(WHY_ROT_PICKUP, WHY_SCALE_PICKUP);
                ent.bind = Some(Binding::Xyz { node, ax: XYZ, parent: None });
                ent.label = name.map(|n| n.to_string()).unwrap_or_else(|| format!("type {}", t)).to_lowercase();
                // Ball-change powerups (bowling ball, ball bearing) have no model of their own in any bank
                ent.object_id = powerup_model_id(cx.tables, t).filter(|p| cx.models.has_model(p));
                ent.rot = Some("xyz");
                ent.scl = Some("xyz");
                ent.xform = Some(Xform::Child);
                if ent.object_id.is_some() {
                    attach_model(cx, &mut ent, 0.4, false, false);
                } else {
                    ent.look = Look::Icon { glyph: "\u{2605}", color: kind_color("pickup"), size: 14, opacity: 1.0 };
                }
                ents.push(ent);
            }
            "MrTip" => {
                let look = Look::Icon { glyph: "?", color: kind_color("tip"), size: 16, opacity: 1.0 };
                let mut ent = Ent::new("tip", node, tag, look).why(WHY_SPRITE, WHY_SPRITE);
                ent.bind = Some(Binding::Xyz { node, ax: XYZ, parent: None });
                ent.label = format!("message {}", doc.get(node, "message_id").unwrap_or("None"));
                ents.push(ent);
            }
            "GloverSpawnPoint" | "BallSpawnPoint" | "CameraSpawnPoint" => {
                let glyph = match tag {
                    "GloverSpawnPoint" => "\u{270b}",
                    "BallSpawnPoint" => "\u{25cf}",
                    _ => "\u{25c9}",
                };
                let look = Look::Icon { glyph, color: "#ffffff", size: 18, opacity: 1.0 };
                let mut ent = Ent::new("spawn", node, tag, look).why(WHY_SPRITE, WHY_SPRITE);
                ent.bind = Some(Binding::Xyz { node, ax: XYZ, parent: None });
                ent.label = tag.replace("SpawnPoint", " start");
                if tag == "BallSpawnPoint" {
                    ent.decor = Some((BALL_MODEL, 0.05));
                    ent.imported = cx.models.is_import(BALL_MODEL);
                }
                ents.push(ent);
            }
            "Vent" => {
                let look = Look::Icon { glyph: "\u{224b}", color: kind_color("wind"), size: 14, opacity: 1.0 };
                let mut ent = Ent::new("wind", node, tag, look).why(WHY_NO_ORIENTATION, WHY_NO_SCALE);
                ent.bind = Some(Binding::Xyz { node, ax: ["origin_x", "origin_y", "origin_z"], parent: None });
                ent.label = format!("vent type {}", doc.get(node, "type").unwrap_or("None"));
                ents.push(ent);
            }
            "Wind" => {
                let mut ent = Ent::new("wind", node, tag, Look::WindBox);
                ent.label = "wind volume".to_string();
                ent.why_rot = Some(WHY_WIND_ROT);
                let (corner, size) = (["left", "top", "front"], ["width", "height", "depth"]);
                ent.bind = Some(Binding::Shift { node, triples: vec![corner.map(Some)], anchor: Anchor::BoxCenter { corner, size } });
                ent.scl = Some("box");
                ent.xform = Some(Xform::Volume);
                ent.volume = Some(Volume { node, shape: Shape::Box { corner, size } });
                ents.push(ent);
            }
            "Buzzer" => {
                let id = ents.len();
                let mut ent = Ent::new("hazard", node, tag, Look::BuzzerLine).why(WHY_NO_ORIENTATION, WHY_NO_SCALE);
                ent.bind = Some(Binding::Mid { node });
                ent.label = "buzzer".to_string();
                ents.push(ent);
                for (ax, i) in [(END_1, 1), (END_2, 2)] {
                    let mut h = Ent::new("hazard", node, tag, Look::Ball).why(WHY_NO_ORIENTATION, WHY_NO_SCALE);
                    h.bind = Some(Binding::Xyz { node, ax, parent: None });
                    h.label = format!("buzzer end {}", i);
                    h.is_handle = true;
                    h.owner = Some(id);
                    ents.push(h);
                }
            }
            "Puzzle" => {
                puzzle_index += 1;
                let zone = camera_zone(doc, node).is_some();
                for cond in doc.kids(node, "PuzzleCond") {
                    for vol in doc.children(cond).to_vec() {
                        let Some(spec) = trigger_spec(doc.tag(vol)) else { continue };
                        let shape_name = if spec.0 == "sphere" { "range" } else { spec.0 };
                        let label = if zone {
                            format!("camera zone \u{b7} puzzle {}", puzzle_index)
                        } else {
                            format!("{} in {} \u{b7} puzzle {}", spec.1, shape_name, puzzle_index)
                        };
                        let mut ent = trigger_entity(node, vol, spec, label);
                        ent.zone = zone;
                        ents.push(ent);
                    }
                }
            }
            "AmbientSoundAtPoint" => {
                let look = Look::Icon { glyph: "\u{266a}", color: kind_color("sound"), size: 10, opacity: 1.0 };
                let mut ent = Ent::new("sound", node, tag, look).why(WHY_NO_ORIENTATION, WHY_NO_SCALE);
                ent.bind = Some(Binding::Xyz { node, ax: XYZ, parent: None });
                ent.label = format!("sound {}", doc.get(node, "sound_id").unwrap_or(""));
                ents.push(ent);
            }
            _ => {}
        }
    }
    // The start marker sits on the first restart gate (PlatCheckpoint). It is left out when the level has a
    // GloverSpawnPoint. parse.c TOKEN_RESTART: the first gate is the start, its theta Glover's facing (hub levels
    // override it by world).
    if let Some(g) = first_gate {
        if !ents.iter().any(|e| e.tag == "GloverSpawnPoint") {
            let mut ent = Ent::new("spawn", ents[g].node, "Platform", Look::Start);
            ent.label = "Glover start (first checkpoint)".to_string();
            ent.derived = true;
            ent.selects = Some(g);
            ent.why_rot = Some(WHY_START_ROT);
            ent.why_scl = Some(WHY_NO_SCALE);
            ent.icons.push(("\u{270b}", "#ffffff", 18, [0.0, 22.0, 0.0]));
            ents.push(ent);
        }
    }
    Level { ents, fog, ambient }
}

fn nonempty(s: Option<&str>) -> Option<&str> {
    s.filter(|s| !s.is_empty())
}

// Camera zones: a puzzle whose one condition is Glover inside a box (cond_type 35) and whose actions only set the
// camera. camera.c puts the level's own camera back every frame Glover is in no zone. LookAtGlover's angle is the
// direction from the camera to Glover (atan2(dx, dz)); its distance is how fast the camera turns there.
// TweenYAdjust raises the camera over Glover, TweenDistance sets how far back it stands, and FlyTowardsPoint sends
// it to a fixed point.

/// The body element of a camera action: 70, 71, 73 or 74.
pub fn cam_action(t: i64) -> Option<&'static str> {
    Some(match t {
        70 => "PuzzleActionCameraTweenYAdjust",
        71 => "PuzzleActionCameraTweenDistance",
        73 => "PuzzleActionCameraFlyTowardsPoint",
        74 => "PuzzleActionCameraLookAtGlover",
        _ => return None,
    })
}

/// A camera zone's settings. A missing one is `None`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Zone {
    /// The zone's box. `None` only in settings that were not read from a puzzle.
    pub vol: Option<NodeId>,
    pub look: Option<f64>,
    pub speed: Option<f64>,
    pub distance: Option<f64>,
    pub height: Option<f64>,
    pub fly: Option<V3>,
}

/// The zone's settings, or `None` when the puzzle is anything else.
pub fn camera_zone(doc: &Doc, puzzle: NodeId) -> Option<Zone> {
    if doc.tag(puzzle) != "Puzzle" {
        return None;
    }
    let (conds, acts) = (doc.kids(puzzle, "PuzzleCond"), doc.kids(puzzle, "PuzzleAction"));
    if conds.len() != 1 || acts.is_empty() || doc.len(puzzle) != conds.len() + acts.len() {
        return None;
    }
    let vol = doc.first(conds[0], "PuzzleCondGloverWithinVolume")?;
    let mut z = Zone { vol: Some(vol), ..Zone::default() };
    for a in acts {
        let b = *doc.children(a).first()?;
        match doc.tag(b) {
            "PuzzleActionCameraLookAtGlover" => {
                z.look = Some(num0(doc, b, "angle"));
                z.speed = Some(num0(doc, b, "distance"));
            }
            "PuzzleActionCameraTweenDistance" => z.distance = Some(num0(doc, b, "distance")),
            "PuzzleActionCameraTweenYAdjust" => z.height = Some(num0(doc, b, "y_adjust")),
            "PuzzleActionCameraFlyTowardsPoint" => z.fly = Some(trio(doc, Some(b), XYZ, 0.0)),
            _ => return None,
        }
    }
    Some(z)
}

/// The remainder of `a / b` with the sign of `b`.
fn py_mod(a: f64, b: f64) -> f64 {
    let r = a % b;
    if r != 0.0 && (r < 0.0) != (b < 0.0) {
        r + b
    } else if r == 0.0 {
        0.0f64.copysign(b)
    } else {
        r
    }
}

/// Replaces the zone's actions with these settings (`None` leaves an action out), in the game's order: look,
/// distance, height, fly.
pub fn set_camera_zone(doc: &mut Doc, puzzle: NodeId, z: &Zone) {
    for a in doc.kids(puzzle, "PuzzleAction") {
        doc.detach(a);
    }
    let mut act = |t: i64, attrs: &[(&str, String)]| {
        let Some(tag) = cam_action(t) else { return };
        let a = doc.make("PuzzleAction", &[("action_type", &t.to_string())]);
        let b = doc.new_element(tag);
        for (k, v) in attrs {
            doc.set(b, k, v);
        }
        doc.set(b, "activation_delay", "0");
        doc.append(a, b);
        doc.append(puzzle, a);
    };
    if let Some(look) = z.look {
        act(74, &[("angle", fmt(py_mod(look, 2.0 * PI))), ("distance", fmt(z.speed.unwrap_or(2.0)))]);
    }
    if let Some(d) = z.distance {
        act(71, &[("distance", fmt(d))]);
    }
    if let Some(h) = z.height {
        act(70, &[("y_adjust", fmt(h))]);
    }
    if let Some(f) = z.fly {
        act(73, &[("x", r1(f[0])), ("y", r1(f[1])), ("z", r1(f[2]))]);
    }
}

/// The camera presets: (key, name, what the camera does).
pub const CAM_PRESETS: [(&str, &str, &str); 8] = [
    ("behind", "Behind", "turns to look the way the player goes"),
    ("high", "High", "rises and looks down"),
    ("side", "Side", "swings around to the right of the way on"),
    ("left", "Other side", "swings around to the left"),
    ("front", "Front", "faces Glover: you can\u{2019}t see where you go"),
    ("low", "Low", "drops to Glover\u{2019}s height and comes close"),
    ("far", "Far", "stands far back and high"),
    ("fixed", "Fixed", "flies to a point ahead and watches from there"),
];

/// The settings of a preset, for a player who walks the way `facing` points (radians about y, 0 along +z). Angles
/// are from the camera to Glover. `None` for a key that is not a preset.
pub fn preset_zone(key: &str, facing: f64, center: V3) -> Option<Zone> {
    let z = |look: Option<f64>, speed: Option<f64>, distance: Option<f64>, height: Option<f64>| Zone {
        vol: None,
        look,
        speed,
        distance,
        height,
        fly: None,
    };
    let f = facing;
    Some(match key {
        "behind" => z(Some(f), Some(2.0), Some(220.0), Some(60.0)),
        "high" => z(None, None, Some(140.0), Some(320.0)),
        "side" => z(Some(f + PI / 2.0), Some(1.5), Some(260.0), Some(60.0)),
        "left" => z(Some(f - PI / 2.0), Some(1.5), Some(260.0), Some(60.0)),
        "front" => z(Some(f + PI), Some(2.0), Some(160.0), Some(30.0)),
        "low" => z(None, None, Some(40.0), Some(-60.0)),
        "far" => z(None, None, Some(700.0), Some(260.0)),
        "fixed" => Zone { fly: Some(vec::add(center, [f.sin() * 250.0, 260.0, f.cos() * 250.0])), ..Zone::default() },
        _ => return None,
    })
}

/// The middle of a zone's box.
pub fn zone_center(doc: &Doc, vol: NodeId) -> V3 {
    box_center(doc, vol, XYZ, ["l", "w", "h"])
}

/// About where the camera stands for a Glover at the middle of the zone, as (glove, eye). `None` when the zone
/// keeps the level's direction.
pub fn zone_camera(doc: &Doc, z: &Zone) -> Option<(V3, V3)> {
    let c = zone_center(doc, z.vol?);
    if let Some(f) = z.fly {
        return Some((c, f));
    }
    let look = z.look?;
    // camera.c: the camera stands distance / 3 + 30 + the height over Glover
    let d = z.distance.unwrap_or(130.0);
    let h = z.height.unwrap_or(0.0);
    Some((c, vec::add(c, [-look.sin() * d, d / 3.0 + 30.0 + h, -look.cos() * d])))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoModels;
    impl Models for NoModels {
        fn has_model(&self, _: &str) -> bool {
            false
        }
        fn is_import(&self, _: &str) -> bool {
            false
        }
    }

    fn level(xml: &str) -> (Doc, Level) {
        let doc = Doc::parse(xml).unwrap();
        let cx = Context {
            format: Format::shared(),
            tables: glovebox_core::tables::noclip_tables(),
            scene_id: "0a",
            models: &NoModels,
        };
        let l = build(&doc, doc.root(), &cx);
        (doc, l)
    }

    #[test]
    fn platforms_and_their_points() {
        let (mut doc, l) = level(
            r#"<Level><FogConfiguration/><NullPlatform><PlatSetTag tag="3"/><PlatSetInitialPos x="100.0" y="0.0" z="0.0"/></NullPlatform><Platform object_id="0x1" name="lift"><PlatSetParent tag="3"/><PlatPathPoint x="1.0" y="2.0" z="3.0"/><PlatPathPoint x="1.0" y="50.0" z="3.0"/><PlatCheckpoint theta="1.0"/><PlatConstantSpin axis="z" initial_theta="0.5" speed="0.1"/></Platform><Platform/></Level>"#,
        );
        let kinds: Vec<(&str, &str)> = l.ents.iter().map(|e| (e.kind, e.label.as_str())).collect();
        assert_eq!(
            kinds,
            [
                ("platform", "null platform"),
                ("platform", "lift"),
                ("platform", "lift \u{b7} point 0"),
                ("platform", "lift \u{b7} point 1"),
                ("platform", "Platform"),
                ("spawn", "Glover start (first checkpoint)")
            ]
        );
        assert!(l.fog.is_some() && l.ambient.is_none());
        let lift = &l.ents[1];
        assert_eq!(lift.parent_ent, Some(0));
        assert_eq!(lift.world_pos(&doc, &l.ents), [101.0, 2.0, 3.0]);
        assert_eq!(lift.points.len(), 1);
        assert_eq!(lift.spin, Some((2, 0.1)));
        assert!(lift.missing_model);
        assert_eq!(l.ents[5].world_pos(&doc, &l.ents), [101.0, 2.0, 3.0]);
        assert!(l.ents[5].start_quat(&doc, &l.ents).is_some());
        // Moving writes the place relative to the parent
        lift.bind.as_ref().unwrap().set(&mut doc, &l.ents, [150.0, 0.0, 0.0]);
        assert_eq!(doc.serialize_node(lift.bind.as_ref().unwrap().node()), r#"<PlatPathPoint x="50.0" y="0.0" z="0.0"/>"#);
        // Turning takes the spin's own angle off
        lift.set_quat(&mut doc, quat_from_xyz([0.0, 0.0, 0.75]));
        assert_eq!(doc.serialize_node(doc.first(lift.node, "SetActorRotation").unwrap()), r#"<SetActorRotation x="0.0" y="0.0" z="0.25"/>"#);
        // A platform with no position gets one
        let bare = &l.ents[4];
        bare.bind.as_ref().unwrap().set(&mut doc, &l.ents, [1.0, 2.0, 3.0]);
        bare.set_scale(&mut doc, [2.0, 2.0, 2.0]);
        assert_eq!(
            doc.serialize_node(bare.node),
            r#"<Platform><PlatScale x="2.0" y="2.0" z="2.0"/><PlatSetInitialPos x="1.0" y="2.0" z="3.0"/></Platform>"#
        );
    }

    #[test]
    fn volumes() {
        let (mut doc, l) = level(
            r#"<Level><Wind left="0.0" top="0.0" front="0.0" width="10.0" height="20.0" depth="30.0"/><Puzzle><PuzzleCond cond_type="41"><PuzzleCondGloverWithinRangeOfPoint2 x="0.0" z="0.0" range="50.0" min_y="0.0" max_y="100.0"/></PuzzleCond><PuzzleCond cond_type="36"><PuzzleCondGloverWithinRangeOfPoint x="1.0" y="2.0" z="3.0" range="5.0"/></PuzzleCond></Puzzle><Buzzer end_1_x="0.0" end_1_y="0.0" end_1_z="0.0" end_2_x="10.0" end_2_y="0.0" end_2_z="0.0"/></Level>"#,
        );
        let labels: Vec<&str> = l.ents.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(
            labels,
            ["wind volume", "Glover in cylinder \u{b7} puzzle 1", "Glover in range \u{b7} puzzle 1", "buzzer", "buzzer end 1", "buzzer end 2"]
        );
        let wind = &l.ents[0];
        assert_eq!(wind.world_pos(&doc, &l.ents), [5.0, 10.0, 15.0]);
        let v = wind.volume.as_ref().unwrap();
        let s0 = v.box_size0(&doc);
        v.set_box(&mut doc, s0, [2.0, 1.0, 1.0]);
        assert_eq!(doc.attr(wind.node, "left"), "-5.0");
        assert_eq!(doc.attr(wind.node, "width"), "20.0");
        let cyl = l.ents[1].volume.as_ref().unwrap();
        assert_eq!(cyl.size_fields(), ["range", "height"]);
        cyl.size_set(&mut doc, 1, 50.0);
        assert_eq!((doc.attr(cyl.node, "min_y"), doc.attr(cyl.node, "max_y")), ("25.0", "75.0"));
        l.ents[1].bind.as_ref().unwrap().set(&mut doc, &l.ents, [1.0, 60.0, 2.0]);
        assert_eq!((doc.attr(cyl.node, "x"), doc.attr(cyl.node, "min_y"), doc.attr(cyl.node, "max_y")), ("1.0", "35.0", "85.0"));
        assert!(!lone_trigger(&doc, &l.ents[1]));
        l.ents[3].bind.as_ref().unwrap().set(&mut doc, &l.ents, [0.0, 4.0, 0.0]);
        assert_eq!(doc.attr(l.ents[3].node, "end_2_x"), "5.0");
        assert_eq!(doc.attr(l.ents[3].node, "end_1_y"), "4.0");
    }

    #[test]
    fn camera_zones() {
        let mut doc = Doc::parse(
            r#"<Level><Puzzle><PuzzleCond cond_type="35"><PuzzleCondGloverWithinVolume x="0.0" y="0.0" z="0.0" l="100.0" w="50.0" h="100.0"/></PuzzleCond><PuzzleAction action_type="71"><PuzzleActionCameraTweenDistance distance="5.0"/></PuzzleAction></Puzzle></Level>"#,
        )
        .unwrap();
        let p = doc.children(doc.root())[0];
        let z = camera_zone(&doc, p).unwrap();
        assert_eq!(z.distance, Some(5.0));
        assert_eq!(zone_camera(&doc, &z), None);
        let mut new = preset_zone("behind", -0.5, [0.0; 3]).unwrap();
        new.vol = z.vol;
        set_camera_zone(&mut doc, p, &new);
        let acts: Vec<String> = doc.kids(p, "PuzzleAction").iter().map(|a| doc.serialize_node(*a)).collect();
        assert_eq!(
            acts,
            [
                r#"<PuzzleAction action_type="74"><PuzzleActionCameraLookAtGlover angle="5.7832" distance="2.0" activation_delay="0"/></PuzzleAction>"#,
                r#"<PuzzleAction action_type="71"><PuzzleActionCameraTweenDistance distance="220.0" activation_delay="0"/></PuzzleAction>"#,
                r#"<PuzzleAction action_type="70"><PuzzleActionCameraTweenYAdjust y_adjust="60.0" activation_delay="0"/></PuzzleAction>"#
            ]
        );
        let (c, eye) = zone_camera(&doc, &camera_zone(&doc, p).unwrap()).unwrap();
        assert_eq!(c, [50.0, 25.0, 50.0]);
        assert!(eye[1] > 180.0);
        let (_, l) = level(&doc.serialize());
        assert!(l.ents[0].zone && l.ents[0].label == "camera zone \u{b7} puzzle 1");
        assert!(preset_zone("nothing", 0.0, [0.0; 3]).is_none());
    }
}
