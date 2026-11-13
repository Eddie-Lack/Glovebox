//! What the level format allows inside each element, and the order the game needs.
//!
//! The format comes from the decomp table (libgarib's format with the decompilation's names). A command that
//! "modifies" a kind (PLATFORM, ENEMY...) goes inside an element that "declares" that kind. The game reads the level
//! as a stream of tokens and each child acts on the last declared object. A child's place among its siblings is its
//! place in the stream, and the order matters (gloveworld src/parse.c):
//!   - A platform builds its collision when it is placed (PlatSetInitialPos or the first PlatPathPoint), from its
//!     scale and rotation at that moment. A platform that never moves never rebuilds it.
//!   - PlatScale after the first point is refused and its data is then read as tokens.
//!   - Spin, orbit, crumble and breakable modifiers write through the structure their main command makes, with no
//!     check that it exists.
//!   - EnemyFinalize ends an enemy: everything after it belongs to the next object.

use crate::xmlutil::{int0, make_el, put, Attrs};
use glovebox_core::xml::{Doc, NodeId};
use serde_json::Value;
use std::collections::HashMap;

/// Commands that have their own tools (the enemy script editor and the puzzle editor) or that the editor writes
/// for itself (an enemy's end).
pub const OWN_TOOLS: &[&str] = &[
    "EnemyNormalInstruction",
    "EnemyConditionalInstruction",
    "EnemyAttackInstruction",
    "PuzzleCond",
    "PuzzleAction",
    "EnemyFinalize",
    "Garib",
];
pub const FIRST: &[&str] = &["SetActorRotation", "SetActorScale"];
pub const LAST: &[&str] = &["EnemyFinalize"];
pub const PLATFORMS: &[&str] = &["Platform", "NullPlatform"];
/// Commands the menus leave out: unused in the shipped levels, or they crash on a wrong value (Actor0xbf).
pub const HIDDEN: &[&str] = &["PlatStrobe", "Rope", "PlatSine", "PlatOrbit", "Actor0xbf"];

pub const MOTION: &[&str] = &[
    "PlatConstantSpin",
    "PlatSpin0x80",
    "PlatTopple0x81",
    "Plat0x9f",
    "PlatRocking",
    "PlatTurnTowardsPathPoint",
    "PlatGoForwards0x5f",
    "LookAtHand0x60",
    "LookAtBall0x61",
    "PlatCat0x69",
];
pub const MOTION_MODS: &[&str] =
    &["PlatSpinPause0x7c", "PlatSpinFlip", "Plat0x7e", "Plat0xc3", "PlatSpinSound0xc5", "Plat0x78"];
pub const ORBIT_MODS: &[&str] = &["PlatOrbitPause", "PlatOrbitFlip0x77", "PlatOrbitSound0xc4", "Plat0xc6"];
/// These free the platform's collision faces, as does a SetExit of type loading_zone.
pub const FREES_FACES: &[&str] = &["PlatNoClip", "PlatSpike"];
/// The phase of the children that no phase names: features.
pub const FEATURES: usize = 4;
/// The set-up commands that change the collision the platform builds when it is placed.
pub const SETUP: &[&str] = &["SetActorRotation", "SetActorScale", "PlatScale"];
/// Pairs that write the same field: the game refuses the second one and reads its data as tokens.
pub const EXCLUSIVE: &[&[&str]] = &[
    &["PlatSpinPause0x7c", "PlatSpinFlip"],
    &["PlatOrbitPause", "PlatOrbitFlip0x77"],
    &["PuzzleAnd", "PuzzleOr", "PuzzleAny"],
];
/// Commands of which an element may hold one. A second one replaces the first in the game or confuses it.
pub const ONE_OF: &[&str] = &[
    "SetActorRotation",
    "SetActorScale",
    "PlatScale",
    "PlatSetParent",
    "PlatSetInitialPos",
    "PlatSetTag",
    "PlatCheckpoint",
    "SetExit",
    "SetTeleport",
    "PlatOrbitAroundPoint",
    "PlatDestructible",
    "PlatCrumb0x67",
    "PlatMaxVelocity",
    "PlatPathAcceleration",
    "PlatformConveyor",
    "PlatActorSurfaceType",
    "PlatSpecial0xb9",
    "PlatNoClip",
    "PuzzleNumtimes",
    "EnemySetAttentionBbox",
];
const NULL_OK_BASE: &[&str] = &[
    "SetActorRotation",
    "SetActorScale",
    "PlatSetTag",
    "PlatSetInitialPos",
    "PlatOrbitAroundPoint",
    "PlatPathPoint",
    "PlatScale",
    "PlatConstantSpin",
    "PlatMaxVelocity",
    "PlatPathAcceleration",
    "PlatSetParent",
    "PlatReverseAtEndsOfPath",
    "PlatPathSetStartingPoint",
    "PlatCopySpinFromParent",
];
const MOVERS_BASE: &[&str] = &[
    "PlatPathPoint",
    "PlatOrbitAroundPoint",
    "PlatPush0x5b",
    "PlatCrumb0x67",
    "PlatSetParent",
    "PlatMvspn0x58",
    "PlatMvspn0x59",
    "PlatMvspn0x5a",
    "PlatMvspn0x73",
    "PlatMvspn0x74",
    "LookAtHand0x60",
    "LookAtBall0x61",
];

fn has(set: &[&str], tag: &str) -> bool {
    set.contains(&tag)
}

/// PlatCopySpinFromParent makes a motion too, but does not reset one that is there.
pub fn is_motion_maker(tag: &str) -> bool {
    has(MOTION, tag) || tag == "PlatCopySpinFromParent"
}

/// What a NullPlatform (a platform with no model: a parent or orbit centre) may hold. The rest use the model or
/// the collision, which it does not have (parse.c TOKEN_DUMMY).
pub fn null_ok(tag: &str) -> bool {
    has(NULL_OK_BASE, tag) || has(ORBIT_MODS, tag)
}

/// Commands with which a platform moves. The game never rebuilds the collision of a platform without one (parse.c,
/// the flag 0x20000).
pub fn is_mover(tag: &str) -> bool {
    has(MOVERS_BASE, tag) || has(MOTION, tag)
}

/// How many of a child the game has room for.
pub fn limit(tag: &str) -> Option<usize> {
    match tag {
        "PlatPathPoint" | "CameoInst" => Some(20),
        "BuzzerDutyCycle" | "VentDutyCycle" => Some(6),
        _ => None,
    }
}

/// A platform's children go in this order of phases (parse.c): set-up, placement, path settings, motion, features,
/// the commands that free the collision, and the head start last.
pub fn phase_of(tag: &str) -> usize {
    match tag {
        "SetActorRotation" | "SetActorScale" | "PlatScale" | "PlatSetParent" | "PlatSetCollisionYOffset" => 0,
        "PlatSetInitialPos" | "PlatPathPoint" | "PlatPathSoundAtPointHit" => 1,
        "PlatMaxVelocity" | "PlatPathAcceleration" | "PlatReverseAtEndsOfPath" | "PlatPathSetStartingPoint" => 2,
        "PlatOrbitAroundPoint" | "PlatCopySpinFromParent" => 3,
        "PlatNoClip" | "PlatSpike" => 5,
        "PlatVentAdvanceFrames" => 6,
        t if has(MOTION, t) || has(MOTION_MODS, t) || has(ORBIT_MODS, t) => 3,
        _ => FEATURES,
    }
}

/// True for the children that stay right after their anchor, and move with it.
pub fn in_glue(tag: &str) -> bool {
    matches!(tag, "PlatPathSoundAtPointHit" | "PlatDestructibleSound" | "PlatSpecial0xc7")
        || has(MOTION_MODS, tag)
        || has(ORBIT_MODS, tag)
}

/// True when `tag` is glued and `anchor` is one of the commands it stays after.
pub fn glue_anchor(tag: &str, anchor: &str) -> bool {
    match tag {
        "PlatPathSoundAtPointHit" => anchor == "PlatPathPoint",
        "PlatDestructibleSound" => anchor == "PlatDestructible",
        "PlatSpecial0xc7" => anchor == "PlatCrumb0x67",
        t if has(MOTION_MODS, t) => is_motion_maker(anchor) || has(MOTION_MODS, anchor),
        t if has(ORBIT_MODS, t) => anchor == "PlatOrbitAroundPoint" || has(ORBIT_MODS, anchor),
        _ => false,
    }
}

/// A child that needs another one earlier in the same platform.
pub struct Need {
    pub anchor: fn(&str) -> bool,
    pub count: usize,
    /// What to tell the author.
    pub message: &'static str,
}

fn is_path_point(t: &str) -> bool {
    t == "PlatPathPoint"
}
fn is_orbit(t: &str) -> bool {
    t == "PlatOrbitAroundPoint"
}
fn is_destructible(t: &str) -> bool {
    t == "PlatDestructible"
}
fn is_crumb(t: &str) -> bool {
    t == "PlatCrumb0x67"
}

pub fn needs(tag: &str) -> Option<Need> {
    let n = |anchor: fn(&str) -> bool, count, message| Some(Need { anchor, count, message });
    match tag {
        "PlatPathSoundAtPointHit" | "PlatPathSetStartingPoint" => n(is_path_point, 1, "Add a path point first"),
        "PlatTurnTowardsPathPoint" | "PlatGoForwards0x5f" => n(is_path_point, 2, "Add two path points first"),
        "PlatDestructibleSound" => n(is_destructible, 1, "Make it breakable first"),
        "PlatSpecial0xc7" => n(is_crumb, 1, "Make it crumble first"),
        t if has(MOTION_MODS, t) => n(is_motion_maker, 1, "Add a spin or other motion first"),
        t if has(ORBIT_MODS, t) => n(is_orbit, 1, "Add an orbit first"),
        _ => None,
    }
}

/// Values for a new command, where the type-neutral 0 would be wrong (the shipped levels' usual values, or tested
/// ones).
pub fn special_defaults(tag: &str) -> &'static [(&'static str, &'static str)] {
    match tag {
        "SetActorScale" | "PlatScale" => &[("x", "1.0"), ("y", "1.0"), ("z", "1.0")],
        "PlatPathPoint" => &[("duration", "-100")],
        "PlatMaxVelocity" => &[("velocity", "10.0")],
        "PlatPathAcceleration" => &[("acceleration", "0.8")],
        "PlatConstantSpin" => &[("axis", "y"), ("initial_theta", "0.0"), ("speed", "0.05")],
        "SetExit" => &[("behavior", "1"), ("type", "solid_platform")],
        "PlatOrbitAroundPoint" => &[("axis", "1"), ("speed", "0.02")],
        "PlatActorSurfaceType" => &[("value", "0")],
        "SetObjectSparkle" => &[("period", "10")],
        "BuzzerDutyCycle" => &[("frames_off", "40"), ("frames_on", "40")],
        "VentDutyCycle" => &[("frames_off", "1"), ("frames_on", "1")],
        "PlatMovementSound" => &[("sound_id", "167"), ("volume", "100"), ("pitch", "128")],
        "PlatPathSoundAtPointHit" => &[("sound_id", "191"), ("volume", "100"), ("pitch", "128")],
        "PlatDestructibleSound" => &[("sound_id", "138"), ("volume", "100"), ("pitch", "128")],
        "PlatCrumb0x67" => &[("u16_0x02", "70"), ("u16_0x04", "10"), ("u16_0x08", "0x3FCCCCCD")],
        "PlatDestructible" => &[("flags", "3"), ("num_fragments", "3")],
        "Backdrop" => &[("scale_x", "4096"), ("scale_y", "4096"), ("scroll_speed_x", "1")],
        "SetGravity" => &[("strength", "1.2")],
        "PuzzleNumtimes" => &[("n", "1")],
        "FogConfiguration" => {
            &[("fog_enabled", "1"), ("r", "40"), ("g", "40"), ("b", "60"), ("fog_distance", "990"), ("near_clip", "1500")]
        }
        "AmbientLight" => &[("r", "110"), ("g", "110"), ("b", "110")],
        "DiffuseLight" => &[("r", "160"), ("g", "160"), ("b", "160"), ("theta_x", "0.0"), ("theta_y", "3.7")],
        _ => &[],
    }
}

/// Plain names for the commands an author is likely to want, shown first in the Add command menu.
pub fn plain_name(tag: &str) -> Option<&'static str> {
    Some(match tag {
        "PlatConstantSpin" => "Spin (constant)",
        "PlatPathPoint" => "Path point",
        "PlatMaxVelocity" => "Path speed",
        "PlatPathAcceleration" => "Path acceleration",
        "PlatReverseAtEndsOfPath" => "Reverse at the path's ends",
        "PlatCheckpoint" => "Checkpoint",
        "SetExit" => "Exit",
        "SetTeleport" => "Teleport",
        "PlatformConveyor" => "Conveyor",
        "PlatActorSurfaceType" => "Surface type (ice...)",
        "PlatSpecial0xb9" => "Camera passes through",
        "PlatNoClip" => "No collision",
        "PlatSetTag" => "Tag (for puzzles)",
        "PlatSetParent" => "Parent platform",
        "PlatScale" => "Scale",
        "SetActorRotation" => "Rotation",
        "SetActorScale" => "Scale",
        "PlatPathSoundAtPointHit" => "Sound at path point",
        "PlatSpike" => "Spikes (hurts)",
        "PlatCauseDamage" => "Hurts",
        "PlatOrbitAroundPoint" => "Orbit a point",
        "PlatRocking" => "Rocking",
        "PlatDestructible" => "Breakable",
        "PlatMovementSound" => "Movement sound",
        "PlatSetInitialPos" => "Start position",
        "PuzzleNumtimes" => "Times it can fire",
        "PuzzleAnd" => "Conditions: all",
        "PuzzleOr" => "Conditions: any",
        "PuzzleAny" => "Conditions: any (2)",
        "EnemySetAttentionBbox" => "Attention box",
        "PlatSetCollisionYOffset" => "Collision height offset",
        "PlatInteractsWithWater" => "Floats on water",
        "SetObjectSparkle" => "Sparkle",
        _ => return None,
    })
}

/// A table value that is one name, a list of names, or nothing, as a list.
pub fn as_list(v: Option<&Value>) -> Vec<&str> {
    match v {
        Some(Value::Array(a)) => a.iter().filter_map(|x| x.as_str()).collect(),
        Some(Value::String(s)) => vec![s.as_str()],
        _ => Vec::new(),
    }
}

/// Whether a JSON value counts as true: not null, false, zero or empty.
pub fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().map(|f| f != 0.0).unwrap_or(true),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

/// The level format: the commands, the types inside them and their enums.
pub struct Format<'a> {
    pub decomp: &'a Value,
    /// Element -> the kinds it declares.
    declares: HashMap<String, Vec<String>>,
    /// Kind -> the commands that modify it, in the table's order.
    by_kind: HashMap<String, Vec<String>>,
}

impl Format<'static> {
    /// The format of the compiled-in decomp table.
    pub fn shared() -> &'static Format<'static> {
        static F: std::sync::OnceLock<Format<'static>> = std::sync::OnceLock::new();
        F.get_or_init(|| Format::new(glovebox_core::tables::decomp()))
    }
}

impl<'a> Format<'a> {
    pub fn new(decomp: &'a Value) -> Format<'a> {
        let mut declares: HashMap<String, Vec<String>> = HashMap::new();
        let mut by_kind: HashMap<String, Vec<String>> = HashMap::new();
        if let Some(commands) = decomp.get("commands").and_then(|c| c.as_object()) {
            for (name, c) in commands {
                for k in as_list(c.get("declares")) {
                    declares.entry(name.clone()).or_default().push(k.to_string());
                }
                for k in as_list(c.get("modifies")) {
                    by_kind.entry(k.to_string()).or_default().push(name.clone());
                }
            }
        }
        Format { decomp, declares, by_kind }
    }

    pub fn command(&self, tag: &str) -> Option<&'a Value> {
        self.decomp.get("commands").and_then(|c| c.get(tag))
    }

    /// The command with this name, or else the nested type with this name.
    pub fn command_or_type(&self, tag: &str) -> Option<&'a Value> {
        match self.command(tag) {
            Some(c) if truthy(Some(c)) => Some(c),
            _ => self.decomp.get("types").and_then(|c| c.get(tag)),
        }
    }

    /// The body element of a code of a switch (`PuzzleCond`, `PuzzleAction`, `EnemyInstruction`).
    pub fn switch_case(&self, kind: &str, code: i128) -> Option<&'a str> {
        self.decomp.get("subSwitches")?.get(kind)?.get("cases")?.get(code.to_string())?.as_str()
    }

    /// Commands that may go inside an element, the common ones first.
    pub fn children_of(&self, tag: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for k in self.declares.get(tag).map(|v| v.as_slice()).unwrap_or(&[]) {
            for name in self.by_kind.get(k).map(|v| v.as_slice()).unwrap_or(&[]) {
                let groups = truthy(self.command(name).and_then(|c| c.get("groupsInto")));
                if !out.contains(name) && !has(OWN_TOOLS, name) && !has(HIDDEN, name) && !groups {
                    out.push(name.clone());
                }
            }
        }
        if tag == "NullPlatform" {
            out.retain(|n| null_ok(n));
        }
        out.sort_by_cached_key(|n| (plain_name(n).is_none(), plain_name(n).unwrap_or(n).to_lowercase()));
        out
    }

    /// The fields of a command or type that are attributes (a switch is a child element).
    pub fn fields(&self, tag: &str) -> Vec<&'a Value> {
        self.command_or_type(tag)
            .and_then(|c| c.get("fields"))
            .and_then(|f| f.as_array())
            .map(|f| f.iter().filter(|f| f.get("type").and_then(|t| t.as_str()) != Some("switch")).collect())
            .unwrap_or_default()
    }

    /// The enum of a field: value -> name. `None` when the field has none.
    pub fn enum_of(&self, tag: &str, field: &Value) -> Option<&'a serde_json::Map<String, Value>> {
        let c = self.command_or_type(tag)?;
        let name = field.get("enum").and_then(|e| e.as_str()).filter(|e| !e.is_empty())?;
        let key = if name.contains("::") {
            name.to_string()
        } else {
            format!("{}::{}", c.get("ksy").and_then(|k| k.as_str()).unwrap_or(""), name)
        };
        self.decomp.get("ksyEnums")?.get(&key)?.as_object()
    }

    /// The enum of the field with this name.
    pub fn enum_for(&self, tag: &str, field: &str) -> Option<&'a serde_json::Map<String, Value>> {
        let c = self.command_or_type(tag)?;
        let f = c.get("fields")?.as_array()?.iter().find(|f| f.get("id").and_then(|i| i.as_str()) == Some(field))?;
        self.enum_of(tag, f)
    }

    /// Every field with a neutral value: 0 (0.0 for floats), the first enum value, an empty name. The values of
    /// `special_defaults` replace them.
    pub fn defaults(&self, tag: &str) -> Attrs {
        let mut out = Attrs::new();
        for f in self.fields(tag) {
            let id = f.get("id").and_then(|i| i.as_str()).unwrap_or("");
            let ty = f.get("type").and_then(|t| t.as_str()).unwrap_or("");
            let en = self.enum_of(tag, f).filter(|e| !e.is_empty());
            let v = if let Some(en) = en {
                en.values().next().and_then(|v| v.as_str()).unwrap_or("")
            } else if ty == "str" {
                ""
            } else if ty.starts_with('f') {
                "0.0"
            } else {
                "0"
            };
            put(&mut out, id, v);
        }
        for (k, v) in special_defaults(tag) {
            put(&mut out, k, v);
        }
        out
    }

    pub fn label<'t>(&self, tag: &'t str) -> &'t str
    where
        'static: 't,
    {
        plain_name(tag).unwrap_or(tag)
    }

    /// The attributes of a new element: the defaults, with `attrs` over them.
    pub fn make_attrs(&self, tag: &str, attrs: &[(&str, &str)]) -> Attrs {
        let mut a = self.defaults(tag);
        for (k, v) in attrs {
            put(&mut a, k, v);
        }
        a
    }

    /// A new element with its defaults, without a parent.
    pub fn make(&self, doc: &mut Doc, tag: &str, attrs: &[(&str, &str)]) -> NodeId {
        let a = self.make_attrs(tag, attrs);
        make_el(doc, tag, &a)
    }
}

pub fn phase(doc: &Doc, node: NodeId) -> usize {
    phase_of(doc.tag(node))
}

pub fn frees_faces(doc: &Doc, node: NodeId) -> bool {
    let tag = doc.tag(node);
    has(FREES_FACES, tag) || (tag == "SetExit" && matches!(doc.get(node, "type"), Some("loading_zone") | Some("0")))
}

fn tags_of(doc: &Doc, parent: NodeId) -> Vec<&str> {
    doc.children(parent).iter().map(|c| doc.tag(*c)).collect()
}

/// The index after child i and the children glued to it.
fn glued_end(tags: &[&str], i: usize) -> usize {
    let mut j = i + 1;
    while j < tags.len() && glue_anchor(tags[j], tags[i]) {
        j += 1;
    }
    j
}

/// The index a new child with this tag goes at.
pub fn place_for(doc: &Doc, parent: NodeId, tag: &str) -> usize {
    let tags = tags_of(doc, parent);
    if has(PLATFORMS, doc.tag(parent)) {
        if in_glue(tag) {
            if let Some(at) = tags.iter().rposition(|t| glue_anchor(tag, t)) {
                return glued_end(&tags, at);
            }
        }
        if tag == "PlatPathPoint" {
            if let Some(at) = tags.iter().rposition(|t| *t == "PlatPathPoint") {
                return glued_end(&tags, at);
            }
        }
        // At the end of its phase, and never after a child of a later phase
        let p = phase_of(tag);
        let same = tags.iter().rposition(|t| phase_of(t) == p && !in_glue(t));
        let later = tags.iter().position(|t| phase_of(t) > p);
        let i = match same {
            Some(s) => glued_end(&tags, s),
            None => tags.len(),
        };
        return match later {
            Some(l) => i.min(l),
            None => i,
        };
    }
    if let Some(rank) = FIRST.iter().position(|t| *t == tag) {
        let mut i = 0;
        while i < tags.len() && FIRST.iter().position(|t| *t == tags[i]).map(|r| r < rank).unwrap_or(false) {
            i += 1;
        }
        return i;
    }
    let mut i = tags.len();
    // Nothing goes after an enemy's EnemyFinalize
    if let Some(last) = tags.last() {
        if has(LAST, last) && !has(LAST, tag) {
            i -= 1;
        }
    }
    i
}

/// Puts a new child command where the game reads it right.
pub fn insert(doc: &mut Doc, parent: NodeId, child: NodeId) -> NodeId {
    let i = place_for(doc, parent, doc.tag(child));
    doc.insert(parent, i, child);
    child
}

/// The index of the child that places the platform, and of its first path point.
pub fn placement(tags: &[&str]) -> (Option<usize>, Option<usize>) {
    (
        tags.iter().position(|t| *t == "PlatSetInitialPos" || *t == "PlatPathPoint"),
        tags.iter().position(|t| *t == "PlatPathPoint"),
    )
}

/// A platform that never moves and has collision: the game builds the collision once, when it is placed.
pub fn keeps_collision(doc: &Doc, node: NodeId) -> bool {
    doc.tag(node) == "Platform"
        && !doc.children(node).iter().any(|c| is_mover(doc.tag(*c)))
        && !doc.children(node).iter().any(|c| frees_faces(doc, *c))
}

/// The first child in the wrong place and the index it should go before. Only what the game needs is checked: the
/// shipped levels put most children in any order.
pub fn misplaced(doc: &Doc, node: NodeId) -> Option<(NodeId, usize)> {
    let ch = doc.children(node);
    let tags = tags_of(doc, node);
    let (placed, first_point) = placement(&tags);
    let still = keeps_collision(doc, node);
    for (i, c) in ch.iter().enumerate() {
        let t = tags[i];
        if let Some(placed) = placed {
            if i > placed
                && ((has(SETUP, t) && still)
                    || (first_point.map(|f| i > f).unwrap_or(false) && (t == "PlatScale" || t == "PlatSetParent")))
            {
                return Some((*c, placed));
            }
        }
        if let Some(need) = needs(t) {
            if missing_need(doc, node, t, Some(i)).is_some() && missing_need(doc, node, t, None).is_none() {
                let at = tags.iter().enumerate().filter(|(_, x)| (need.anchor)(x)).map(|(j, _)| j).nth(need.count - 1)?;
                return Some((*c, at + 1));
            }
        }
        if t == "PlatActorSurfaceType" {
            if let Some(free) = ch[..i].iter().position(|x| frees_faces(doc, *x)) {
                return Some((*c, free));
            }
        }
    }
    None
}

/// Moves a platform's children that are in an order the game reads wrong. The rest keep their order. Returns
/// whether anything moved.
pub fn tidy(doc: &mut Doc, parent: NodeId) -> bool {
    if !has(PLATFORMS, doc.tag(parent)) {
        return false;
    }
    let mut moved = false;
    for _ in 0..4 * doc.len(parent) {
        let Some((c, j)) = misplaced(doc, parent) else { break };
        let target = doc.children(parent).get(j).copied();
        doc.detach(c);
        match target {
            Some(t) if t != c => doc.add_prev(t, c),
            _ => doc.append(parent, c),
        }
        moved = true;
    }
    moved
}

/// What a new child with this tag needs and the parent does not have yet. With `before`, only the children before
/// that index count.
pub fn missing_need(doc: &Doc, parent: NodeId, tag: &str, before: Option<usize>) -> Option<&'static str> {
    let need = needs(tag)?;
    let ch = doc.children(parent);
    let ch = match before {
        Some(b) => &ch[..b.min(ch.len())],
        None => ch,
    };
    if ch.iter().filter(|c| (need.anchor)(doc.tag(**c))).count() < need.count {
        Some(need.message)
    } else {
        None
    }
}

/// Why the parent cannot take another child with this tag. `None` when it can.
pub fn refusal(doc: &Doc, parent: NodeId, tag: &str) -> Option<String> {
    let have = tags_of(doc, parent);
    let ptag = doc.tag(parent);
    let platform = has(PLATFORMS, ptag);
    if has(ONE_OF, tag) && have.contains(&tag) {
        return Some("It already has one.".into());
    }
    if platform && has(MOTION, tag) && have.iter().any(|t| has(MOTION, t)) {
        return Some("It already has a motion; a second one replaces it (\u{201c}Plat already has spin\u{201d}).".into());
    }
    for pair in EXCLUSIVE {
        if has(pair, tag) {
            if let Some(other) = pair.iter().find(|t| have.contains(t) && **t != tag) {
                return Some(format!("It can't have this and {} together.", other));
            }
        }
    }
    if let Some(l) = limit(tag) {
        if have.iter().filter(|t| **t == tag).count() >= l {
            return Some(format!("The game has room for {}.", l));
        }
    }
    if ptag == "NullPlatform" && !null_ok(tag) {
        return Some("A platform with no model can't use it.".into());
    }
    if platform && tag == "PlatPathPoint" && have.contains(&"PlatOrbitAroundPoint") {
        return Some("An orbiting platform can't follow a path.".into());
    }
    if platform && tag == "PlatOrbitAroundPoint" && have.iter().filter(|t| **t == "PlatPathPoint").count() >= 2 {
        return Some("A platform with two or more path points can't orbit.".into());
    }
    missing_need(doc, parent, tag, None).map(|s| s.to_string())
}

/// One order or combination rule a platform breaks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlatformRule {
    pub key: String,
    pub message: String,
    /// The game's own warning text, when it prints one.
    pub game_text: Option<&'static str>,
}

/// The order and combination rules a platform breaks.
pub fn platform_rules(doc: &Doc, node: NodeId) -> Vec<PlatformRule> {
    let mut out = Vec::new();
    let mut add = |key: String, message: String, game_text: Option<&'static str>| {
        out.push(PlatformRule { key, message, game_text })
    };
    let ch = doc.children(node);
    let tags = tags_of(doc, node);
    let (placed, first_point) = placement(&tags);
    let still = keeps_collision(doc, node);
    let count = |upto: usize, tag: &str| tags[..upto].iter().filter(|t| **t == tag).count();
    let points = count(tags.len(), "PlatPathPoint");
    if points > 20 {
        add(
            "points".into(),
            format!("{} path points; the game keeps the first 20 (PlatformAddPoint).", points),
            Some("too many POINTS"),
        );
    }
    let motions: Vec<&str> = tags.iter().copied().filter(|t| has(MOTION, t)).collect();
    if motions.len() > 1 {
        add(
            "motion".into(),
            format!(
                "Has {} motions ({}); each one resets the one before (\u{201c}Plat already has spin\u{201d}).",
                motions.len(),
                motions.join(", ")
            ),
            None,
        );
    }
    for (i, c) in ch.iter().enumerate() {
        let t = tags[i];
        if let Some(miss) = missing_need(doc, node, t, Some(i)) {
            add(
                format!("needs:{}", t),
                format!(
                    "{} has nothing to act on: {}. The game writes through an empty pointer.",
                    t,
                    miss.to_lowercase()
                ),
                None,
            );
        }
        if doc.tag(node) == "NullPlatform" && !null_ok(t) {
            add(
                format!("null:{}", t),
                format!("{} on a platform with no model: it uses a model or collision the platform doesn't have.", t),
                None,
            );
        }
        if t == "PlatScale" && first_point.map(|f| i > f).unwrap_or(false) {
            add(
                "scale-order".into(),
                "PlatScale comes after the first path point. The game refuses it and reads its numbers as commands.".into(),
                Some("scale after first point"),
            );
        } else if has(SETUP, t) && placed.map(|p| i > p).unwrap_or(false) && still {
            add(
                format!("setup-order:{}", t),
                format!(
                    "{} comes after the platform is placed. It doesn't move, so its collision keeps the old size and turn. Use Tidy order.",
                    t
                ),
                None,
            );
        }
        if t == "PlatSetParent" && first_point.map(|f| i > f).unwrap_or(false) {
            add(
                "parent-order".into(),
                "PlatSetParent comes after the first path point and is ignored.".into(),
                Some("parenttag after first point"),
            );
        }
        if t == "PlatOrbitAroundPoint" && count(i, "PlatPathPoint") >= 2 {
            add(
                "orbit".into(),
                "Orbit after two or more path points: the game refuses it and reads its numbers as commands.".into(),
                Some("cannot orbit - more than one point"),
            );
        }
        if t == "PlatPathPoint" && tags[..i].contains(&"PlatOrbitAroundPoint") {
            add("orbit-point".into(), "A path point after an orbit is ignored.".into(), Some("it is an orbiter"));
        }
        if t == "PlatPathSetStartingPoint" {
            let idx = int0(doc.get(*c, "point_idx")).unwrap_or(0);
            if idx >= count(i, "PlatPathPoint") as i128 {
                add(
                    "startpoint".into(),
                    format!("The starting point {} isn't one of the path points before it.", idx),
                    None,
                );
            }
        }
        if t == "PlatActorSurfaceType" && ch[..i].iter().any(|x| frees_faces(doc, *x)) {
            add(
                "surface".into(),
                "The surface type comes after the collision was freed (no collision, spikes or a loading zone).".into(),
                None,
            );
        }
    }
    for pair in &EXCLUSIVE[..2] {
        if pair.iter().all(|t| tags.contains(t)) {
            let later = pair.iter().max_by_key(|t| tags.iter().position(|x| x == *t)).copied().unwrap_or("");
            add(
                format!("excl:{}", later),
                format!(
                    "Has both {}; the game refuses the later one and reads its numbers as commands.",
                    pair.join(" and ")
                ),
                None,
            );
        }
    }
    let ntags = count(tags.len(), "PlatSetTag");
    if ntags > 1 {
        add("2tag".into(), format!("Has {} tags; puzzles find the platform by the last one.", ntags), None);
    }
    out
}

/// Gets a child command, or adds one where the game reads it right.
pub fn ensure_cmd(doc: &mut Doc, parent: NodeId, tag: &str, attrs: &[(&str, &str)]) -> NodeId {
    if let Some(c) = doc.first(parent, tag) {
        return c;
    }
    let c = doc.make(tag, attrs);
    insert(doc, parent, c)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt() -> &'static Format<'static> {
        Format::shared()
    }

    #[test]
    fn children_and_defaults() {
        let f = fmt();
        let kids = f.children_of("Platform");
        assert!(kids.contains(&"PlatPathPoint".to_string()));
        assert!(!kids.contains(&"PlatStrobe".to_string()));
        // Named commands come first, by their plain name
        let named = kids.iter().take_while(|k| plain_name(k).is_some()).count();
        assert!(named > 10);
        assert!(kids[named..].iter().all(|k| plain_name(k).is_none()));
        assert!(f.children_of("NullPlatform").iter().all(|k| null_ok(k)));
        let d = f.defaults("PlatConstantSpin");
        assert_eq!(d, crate::xmlutil::attrs_of(&[("axis", "y"), ("initial_theta", "0.0"), ("speed", "0.05")]));
        assert_eq!(f.label("PlatNoClip"), "No collision");
        assert_eq!(f.label("Whatever"), "Whatever");
    }

    #[test]
    fn insert_keeps_the_games_order() {
        let mut doc = Doc::parse(r#"<Level><Platform><PlatNoClip/></Platform><Enemy><EnemyFinalize/></Enemy></Level>"#).unwrap();
        let p = doc.children(doc.root())[0];
        let e = doc.children(doc.root())[1];
        for tag in ["PlatPathPoint", "PlatMaxVelocity", "PlatPathPoint", "PlatScale", "PlatPathSoundAtPointHit", "PlatSetTag"] {
            let c = doc.new_element(tag);
            insert(&mut doc, p, c);
        }
        assert_eq!(
            tags_of(&doc, p),
            ["PlatScale", "PlatPathPoint", "PlatPathPoint", "PlatPathSoundAtPointHit", "PlatMaxVelocity", "PlatSetTag", "PlatNoClip"]
        );
        for tag in ["EnemySetAttentionBbox", "SetActorScale", "SetActorRotation"] {
            ensure_cmd(&mut doc, e, tag, &[]);
        }
        assert_eq!(tags_of(&doc, e), ["SetActorRotation", "SetActorScale", "EnemySetAttentionBbox", "EnemyFinalize"]);
    }

    #[test]
    fn tidy_and_rules() {
        let mut doc = Doc::parse(
            r#"<Level><Platform><PlatSpinFlip/><PlatSetInitialPos/><PlatScale/><PlatConstantSpin/><PlatPathSetStartingPoint point_idx="3"/></Platform></Level>"#,
        )
        .unwrap();
        let p = doc.children(doc.root())[0];
        let rules = platform_rules(&doc, p);
        let keys: Vec<&str> = rules.iter().map(|r| r.key.as_str()).collect();
        assert_eq!(keys, ["needs:PlatSpinFlip", "needs:PlatPathSetStartingPoint", "startpoint"]);
        assert!(tidy(&mut doc, p));
        assert_eq!(
            tags_of(&doc, p),
            ["PlatSetInitialPos", "PlatScale", "PlatConstantSpin", "PlatSpinFlip", "PlatPathSetStartingPoint"]
        );
        assert!(!tidy(&mut doc, p));
        let root = doc.root();
        assert!(!tidy(&mut doc, root));
    }

    #[test]
    fn refusals() {
        let doc = Doc::parse(
            r#"<Level><Platform><PlatSetTag/><PlatConstantSpin/><PlatSpinFlip/><PlatOrbitAroundPoint/></Platform><NullPlatform/></Level>"#,
        )
        .unwrap();
        let p = doc.children(doc.root())[0];
        let n = doc.children(doc.root())[1];
        assert_eq!(refusal(&doc, p, "PlatSetTag").as_deref(), Some("It already has one."));
        assert!(refusal(&doc, p, "PlatRocking").unwrap().starts_with("It already has a motion"));
        assert_eq!(refusal(&doc, p, "PlatSpinPause0x7c").as_deref(), Some("It can't have this and PlatSpinFlip together."));
        assert_eq!(refusal(&doc, p, "PlatPathPoint").as_deref(), Some("An orbiting platform can't follow a path."));
        assert_eq!(refusal(&doc, n, "PlatSpike").as_deref(), Some("A platform with no model can't use it."));
        assert_eq!(refusal(&doc, n, "PlatOrbitPause").as_deref(), Some("Add an orbit first"));
        assert_eq!(refusal(&doc, p, "PlatSpike"), None);
    }
}
