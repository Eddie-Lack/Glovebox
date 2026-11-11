//! Exit triggers: where and how big the game's test for a platform's `SetExit` is.
//!
//! The game tests an exit in `CheckForExit` (gloveworld, src/game.c): Glover or the ball must come closer to the
//! platform's position than `actor.radius`. The radius is measured once, when the level file gives the platform
//! its position (`InitPlatformInfo`, src/platform.c). It depends on the order of the platform's commands, which
//! `radius` follows. See docs/technical.md, "Exits".

use crate::xmlutil::{hex_id, num};
use glovebox_core::xml::{Doc, NodeId};

/// The object id of `exitpost.ndo`. The game starts this model with a radius of 20 and a scale of 1.5, 2, 1.5.
pub const EXITPOST: &str = "0x7FDADB91";
/// The colour of the spheres in the view.
pub const COLOR: &str = "#9dff3c";
/// The key of the layer in `Editor::hidden`, and its name in the settings.
pub const LAYER: &str = "exit";
pub const SETTING: &str = "exits";

const CAVE: i64 = 8;
const ACOURSE: i64 = 9;
const ATLANTIS3A: i64 = 12;
const ATLANTISBONUS: i64 = 14;
const CARNIVAL1: i64 = 15;
const SPACEBOSS1: i64 = 38;
const WAYROOM: i64 = 42;

/// Why the game leaves a platform's radius at 0.
#[derive(Clone, Debug, PartialEq)]
pub enum Zero {
    /// A NullPlatform has no model. The game never measures it.
    Null,
    /// This command comes before the position. It sets the flag that makes the game skip the measuring.
    Before(&'static str),
    /// No PlatSetInitialPos and no PlatPathPoint: nothing makes the game measure the platform.
    NoPosition,
    /// The model's root mesh has no faces.
    NoFaces,
    /// The scale in force when the position is read makes every vertex 0.
    NoSize,
}

impl Zero {
    pub fn text(&self) -> String {
        match self {
            Zero::Null => "A NullPlatform has no model, and the game measures the radius on the model.".to_string(),
            Zero::Before(what) => format!(
                "{what} comes before the platform\u{2019}s position. The game then skips the measuring. Move it after the position."
            ),
            Zero::NoPosition => "The platform has no PlatSetInitialPos and no PlatPathPoint. The game measures the radius when it reads one.".to_string(),
            Zero::NoFaces => "The model\u{2019}s root mesh has no faces to measure.".to_string(),
            Zero::NoSize => "The scale in force when the game reads the position is 0.".to_string(),
        }
    }
}

/// The exit type of a platform: the `behavior` of its last SetExit. None without a SetExit, or when the type is 0
/// (the game tests no exit then).
pub fn exit_type(doc: &Doc, node: NodeId) -> Option<i64> {
    let last = doc.kids(node, "SetExit").last().copied()?;
    let t = num(doc, Some(last), "behavior", 0.0);
    (t.is_finite() && t as i64 != 0).then_some(t as i64)
}

fn loading_zone(doc: &Doc, set_exit: NodeId) -> bool {
    matches!(doc.get(set_exit, "type"), Some("loading_zone") | Some("0"))
}

/// What a child command is called in a message.
fn flag_name(doc: &Doc, child: NodeId) -> Option<&'static str> {
    match doc.tag(child) {
        "SetExit" if loading_zone(doc, child) => Some("The loading-zone SetExit"),
        "PlatNoClip" => Some("PlatNoClip"),
        "PlatSpike" => Some("PlatSpike"),
        _ => None,
    }
}

/// The radius the game gives a platform, and why when it is 0. `verts` are the vertices the faces of the model's
/// root mesh use (`Backend::root_vertices`).
///
/// It walks the platform's commands in the order of the level file, as the game's parser does:
/// - `exitpost.ndo` starts with a radius of 20 and a scale of 1.5, 2, 1.5. Every other platform starts with 0 and 1.
/// - PlatScale sets the scale until the first path point is read. SetActorScale (the game mod) always sets it.
/// - A loading-zone SetExit, PlatNoClip and PlatSpike set flag 4. A NullPlatform has flag 0x80.
/// - PlatSetInitialPos and the first PlatPathPoint measure the platform, unless one of those flags is set: the
///   radius grows to the longest vertex, each axis multiplied by the scale. It never shrinks.
pub fn radius(doc: &Doc, node: NodeId, verts: &[[f32; 3]]) -> (f64, Option<Zero>) {
    if doc.tag(node) == "NullPlatform" {
        return (0.0, Some(Zero::Null));
    }
    let exitpost = hex_id(doc.get(node, "object_id")).as_deref() == Some(EXITPOST);
    let (mut r, mut scale) = if exitpost { (20.0f32, [1.5f32, 2.0, 1.5]) } else { (0.0, [1.0; 3]) };
    let xyz = |c: NodeId| ["x", "y", "z"].map(|a| num(doc, Some(c), a, 0.0) as f32);
    let (mut points, mut orbit, mut measured) = (0, false, false);
    let mut flag: Option<&'static str> = None;
    let mut why = None;
    for &c in doc.children(node) {
        let tag = doc.tag(c);
        let measure = match tag {
            "PlatScale" if points == 0 => {
                scale = xyz(c);
                false
            }
            "SetActorScale" => {
                scale = xyz(c);
                false
            }
            "PlatOrbitAroundPoint" => {
                orbit = true;
                false
            }
            "PlatSetInitialPos" => true,
            // An orbiting platform takes no path point, and the game keeps at most 20
            "PlatPathPoint" if !orbit && points < 20 => {
                points += 1;
                points == 1
            }
            _ => {
                flag = flag.or(flag_name(doc, c));
                false
            }
        };
        if !measure {
            continue;
        }
        if let Some(f) = flag {
            why = why.or(Some(Zero::Before(f)));
            continue;
        }
        measured = true;
        for v in verts {
            let p = [v[0] * scale[0], v[1] * scale[1], v[2] * scale[2]];
            let d = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            if r < d {
                r = d;
            }
        }
    }
    if r > 0.0 {
        return (f64::from(r), None);
    }
    let why = match why {
        Some(w) if !measured => w,
        _ if !measured => Zero::NoPosition,
        _ if verts.is_empty() => Zero::NoFaces,
        _ => Zero::NoSize,
    };
    (0.0, Some(why))
}

/// Who the game measures the distance to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Who {
    Glover,
    Ball,
    /// Exit type 3: no distance. The ball must be on the platform.
    OnPlatform,
}

/// How `CheckForExit` decides for an exit type in a level (the level's number in the game).
#[derive(Clone, Debug, PartialEq)]
pub struct Rule {
    pub who: Who,
    /// The other conditions and what happens, one sentence each.
    pub notes: Vec<&'static str>,
}

pub fn rule(exit_type: i64, level: i64) -> Rule {
    let t = exit_type;
    let door = (6..=12).contains(&t);
    let hand_level = matches!(level, WAYROOM | ATLANTISBONUS | ATLANTIS3A | CARNIVAL1);
    let mut notes = Vec::new();
    // The hub stages are levels 0 to 7. The Castle Cave and the Assault Course are in the hub world too, and
    // the game leaves them out by name.
    if (0..=7).contains(&level) && t != 12 && t != 21 {
        notes.push("In a hub stage the exit is closed while the hub waits to change (hubTarget differs from hubLevel) and the ball is in play, unless worldComplete[type \u{2212} 5] is set.");
    }
    if t == 3 {
        notes.push("No distance is tested. The ball must rest on this platform while Glover holds it.");
        return Rule { who: Who::OnPlatform, notes };
    }
    let who = if door || t == 21 || (t == 13 && level != ACOURSE) || hand_level { Who::Glover } else { Who::Ball };
    if matches!(level, WAYROOM | CAVE | ATLANTIS3A | CARNIVAL1) || door {
        notes.push("Nothing else is needed.");
        if matches!(level, ATLANTIS3A | CARNIVAL1) {
            notes.push("The level ends 50 frames later.");
        }
    } else {
        if t == 21 || level == ATLANTISBONUS {
            notes.push("Nothing else is needed.");
        } else if t == 13 {
            notes.push("It works when the hub is at the stage the save has reached (hubLevel equals hubTarget), or when the ball is put away (ballStatic).");
        } else {
            notes.push("Glover must hold the ball.");
        }
        if (t == 1 || level == ACOURSE) && level != SPACEBOSS1 {
            if level == ACOURSE && (t == 21 || t == 13) {
                notes.push("In the Assault Course Glover must also hold the ball.");
            }
            notes.push("Glover then spins and the level ends (the goal animation).");
        }
    }
    if door {
        notes.push("A ball that Glover does not hold is popped when it comes within the radius. The game keeps it for his return.");
    }
    Rule { who, notes }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A made-up model: the vertices of a box 20 wide, 10 high and 40 deep.
    fn verts() -> Vec<[f32; 3]> {
        let mut v = Vec::new();
        for x in [-10.0f32, 10.0] {
            for y in [-5.0f32, 5.0] {
                for z in [-20.0f32, 20.0] {
                    v.push([x, y, z]);
                }
            }
        }
        v
    }

    fn of(inner: &str) -> (f64, Option<Zero>) {
        let doc = Doc::parse(&format!("<Level>{inner}</Level>")).unwrap();
        let p = doc.children(doc.root())[0];
        radius(&doc, p, &verts())
    }

    fn len(x: f32, y: f32, z: f32) -> f64 {
        f64::from((x * x + y * y + z * z).sqrt())
    }

    const POS: &str = r#"<PlatSetInitialPos x="1" y="2" z="3"/>"#;
    const ZONE: &str = r#"<SetExit behavior="1" type="loading_zone"/>"#;
    const SOLID: &str = r#"<SetExit behavior="1" type="solid_platform"/>"#;

    #[test]
    fn scale_before_and_after_the_position() {
        let scale = r#"<PlatScale x="2" y="1" z="0.5"/>"#;
        let plain = of(&format!(r#"<Platform object_id="0x1">{POS}{SOLID}</Platform>"#));
        assert_eq!(plain, (len(10.0, 5.0, 20.0), None));
        let before = of(&format!(r#"<Platform object_id="0x1">{scale}{POS}{SOLID}</Platform>"#));
        assert_eq!(before, (len(20.0, 5.0, 10.0), None));
        // A scale after the position is drawn, and the radius keeps the size of scale 1
        let after = of(&format!(r#"<Platform object_id="0x1">{POS}{scale}{SOLID}</Platform>"#));
        assert_eq!(after, plain);
        // The first path point measures; PlatScale after it is not read at all
        let path = r#"<PlatPathPoint duration="1" x="0" y="0" z="0"/>"#;
        assert_eq!(of(&format!(r#"<Platform object_id="0x1">{scale}{path}{path}</Platform>"#)), before);
        assert_eq!(of(&format!(r#"<Platform object_id="0x1">{path}{scale}{POS}</Platform>"#)), plain);
        // The radius only grows: a second measuring with a smaller scale keeps it
        let small = r#"<PlatScale x="0.1" y="0.1" z="0.1"/>"#;
        assert_eq!(of(&format!(r#"<Platform object_id="0x1">{scale}{POS}{small}{POS}</Platform>"#)), before);
        // The game mod's SetActorScale writes the same scale, also after a path point
        let actor = r#"<SetActorScale x="3" y="3" z="3"/>"#;
        assert_eq!(of(&format!(r#"<Platform object_id="0x1">{actor}{POS}</Platform>"#)).0, len(30.0, 15.0, 60.0));
        assert_eq!(of(&format!(r#"<Platform object_id="0x1">{path}{actor}{POS}</Platform>"#)).0, len(30.0, 15.0, 60.0));
        // An orbiting platform takes no path point
        let orbit = r#"<PlatOrbitAroundPoint/>"#;
        assert_eq!(of(&format!(r#"<Platform object_id="0x1">{orbit}{path}</Platform>"#)), (0.0, Some(Zero::NoPosition)));
        assert_eq!(of(&format!(r#"<Platform object_id="0x1">{}{POS}</Platform>"#, r#"<PlatScale x="0" y="0" z="0"/>"#)), (0.0, Some(Zero::NoSize)));
    }

    #[test]
    fn loading_zone_before_and_after_the_position() {
        let after = of(&format!(r#"<Platform object_id="0x1">{POS}{ZONE}</Platform>"#));
        assert_eq!(after, (len(10.0, 5.0, 20.0), None));
        let before = of(&format!(r#"<Platform object_id="0x1">{ZONE}{POS}</Platform>"#));
        assert_eq!(before, (0.0, Some(Zero::Before("The loading-zone SetExit"))));
        // A solid exit sets no flag
        assert_eq!(of(&format!(r#"<Platform object_id="0x1">{SOLID}{POS}</Platform>"#)), after);
        // The other commands that set flag 4
        assert_eq!(of(&format!(r#"<Platform object_id="0x1"><PlatNoClip/>{POS}{SOLID}</Platform>"#)), (0.0, Some(Zero::Before("PlatNoClip"))));
        assert_eq!(of(&format!(r#"<Platform object_id="0x1">{POS}<PlatSpike/>{SOLID}</Platform>"#)), after);
        assert_eq!(of(&format!(r#"<Platform object_id="0x1">{SOLID}</Platform>"#)), (0.0, Some(Zero::NoPosition)));
    }

    #[test]
    fn null_platform_and_exitpost() {
        assert_eq!(of(&format!(r#"<NullPlatform>{POS}{SOLID}</NullPlatform>"#)), (0.0, Some(Zero::Null)));
        // The exit post starts at 20 with a scale of 1.5, 2, 1.5
        let post = of(&format!(r#"<Platform object_id="0x7fdadb91">{POS}{SOLID}</Platform>"#));
        assert_eq!(post, (len(15.0, 10.0, 30.0), None));
        // Unmeasured, it keeps its 20
        assert_eq!(of(&format!(r#"<Platform object_id="0x7FDADB91">{ZONE}{POS}</Platform>"#)), (20.0, None));
        // PlatScale replaces its scale
        let small = r#"<PlatScale x="0.1" y="0.1" z="0.1"/>"#;
        assert_eq!(of(&format!(r#"<Platform object_id="0x7FDADB91">{small}{POS}</Platform>"#)), (20.0, None));
    }

    #[test]
    fn exit_types() {
        let doc = Doc::parse(&format!(r#"<Level><Platform>{ZONE}<SetExit behavior="6" type="1"/></Platform><Platform><SetExit behavior="0" type="loading_zone"/></Platform><Platform/></Level>"#)).unwrap();
        let k = doc.children(doc.root()).to_vec();
        assert_eq!([exit_type(&doc, k[0]), exit_type(&doc, k[1]), exit_type(&doc, k[2])], [Some(6), None, None]);
    }

    #[test]
    fn who_must_come() {
        // A level's goal: the ball, held
        let goal = rule(1, 10);
        assert_eq!(goal.who, Who::Ball);
        assert!(goal.notes.contains(&"Glover must hold the ball."));
        // A world entrance of a hub stage: Glover alone
        let door = rule(6, 1);
        assert_eq!(door.who, Who::Glover);
        assert!(door.notes.iter().any(|n| n.contains("worldComplete")) && door.notes.contains(&"Nothing else is needed."));
        // Atlantis 3 and Carnival 1 end on Glover alone, whatever the type
        for level in [12, 15] {
            let end = rule(1, level);
            assert_eq!(end.who, Who::Glover);
            assert!(!end.notes.contains(&"Glover must hold the ball."));
        }
        // Type 13 is Glover, and the ball in the Assault Course
        assert_eq!((rule(13, 0).who, rule(13, 9).who), (Who::Glover, Who::Ball));
        assert_eq!(rule(3, 10).who, Who::OnPlatform);
        assert_eq!(rule(21, 9).who, Who::Glover);
    }
}
