//! Platform presets: common platforms (rings, exits, lifts, spikes...) as one click each.
//!
//! Each preset makes whole elements with the children in the order the game reads them (`commands`). The values
//! were tested in the emulator and on a console. Platforms use the game's own models: a platform with an imported
//! custom model makes a console trap (`entities::WHY_CUSTOM_PLATFORM`).

use crate::xmlutil::{float_bits, fmt};
use glovebox_core::xml::{Doc, NodeId};
use std::f64::consts::PI;

/// A model of the game: (object id, name).
pub type Model<'a> = (&'a str, &'a str);

/// GENERIC, the checkpoint ring.
pub const HOOP: Model = ("0x70210547", "hoop.nd");
/// GENERIC, the exit marker of the Space levels.
pub const HEART: Model = ("0x7FDADB91", "heart");
/// GENERIC, radius 43.8 at scale 1.
pub const BALLBIT: Model = ("0x2859DA5E", "ballbit");
/// GENERIC, 48 x 48, collision top 6 above its origin, 21 thick.
pub const LIFT: Model = ("0x090BCA4B", "lift");
/// GENERIC, the slam button, 40 x 40.
pub const GLOVEBU: Model = ("0x11C4C2A2", "glovebu");
/// OOTW_L2A, 210 x 210, collision top 46 above its origin.
pub const STEPY: Model = ("0xF382F9F3", "stepy.n");
/// OOTW_L3, the teleport pad.
pub const TELEPAD: Model = ("0xA8B34B43", "ootelep");
/// A ring's height over the floor. At 12 the glove restarted inside the floor.
pub const RING_START: f64 = 40.0;
pub const RING_MID: f64 = 26.0;
/// The lift scaled into a 192 x 192 deck.
pub const DECK: [f64; 3] = [4.0, 1.0, 4.0];

/// An attribute value: a float is written as the level XML writes floats, the rest as it is.
enum Val<'a> {
    F(f64),
    I(i64),
    S(&'a str),
    /// A platform tag. It is written "None" when there is none.
    Tag(Option<i64>),
}

fn el(doc: &mut Doc, tag: &str, attrs: &[(&str, Val)]) -> NodeId {
    let e = doc.new_element(tag);
    for (k, v) in attrs {
        let text = match v {
            Val::F(f) => fmt(*f),
            Val::I(i) => i.to_string(),
            Val::S(s) => s.to_string(),
            Val::Tag(Some(t)) => t.to_string(),
            Val::Tag(None) => "None".to_string(),
        };
        doc.set(e, k, &text);
    }
    e
}

fn add(doc: &mut Doc, parent: NodeId, tag: &str, attrs: &[(&str, Val)]) {
    let c = el(doc, tag, attrs);
    doc.append(parent, c);
}

fn xyz<'a>(p: [f64; 3]) -> [(&'static str, Val<'a>); 3] {
    [("x", Val::F(p[0])), ("y", Val::F(p[1])), ("z", Val::F(p[2]))]
}

fn platform(doc: &mut Doc, model: Model, p: [f64; 3], scale: Option<[f64; 3]>, place: bool) -> NodeId {
    let name: String = model.1.chars().take(7).collect();
    let n = el(doc, "Platform", &[("object_id", Val::S(model.0)), ("name", Val::S(&name))]);
    if let Some(s) = scale {
        if s != [1.0, 1.0, 1.0] {
            add(doc, n, "PlatScale", &xyz(s));
        }
    }
    if place {
        add(doc, n, "PlatSetInitialPos", &xyz(p));
    }
    n
}

/// Rounds to four decimals, by the exact value of the double. A half goes to the even neighbor.
fn round4(v: f64) -> f64 {
    if !v.is_finite() {
        return v;
    }
    format!("{:.4}", v).parse().unwrap_or(v)
}

fn ring(doc: &mut Doc, p: [f64; 3], facing: f64, height: f64) -> NodeId {
    let n = platform(doc, HOOP, [p[0], p[1] + height, p[2]], None, true);
    // theta is Glover's facing at a restart and the start camera's turn. The spin's speed 0.05 matters: the game
    // reads the spin when the ring is touched, and a ring with no spin crashes.
    add(doc, n, "PlatCheckpoint", &[("u16_0x17", Val::I(0)), ("theta", Val::F(facing))]);
    add(doc, n, "PlatConstantSpin", &[("axis", Val::S("y")), ("initial_theta", Val::F(0.0)), ("speed", Val::F(0.05))]);
    add(doc, n, "PlatNoClip", &[]);
    n
}

fn heart(doc: &mut Doc, p: [f64; 3], facing: f64, behavior: i64, scale: Option<[f64; 3]>) -> NodeId {
    let n = platform(doc, HEART, p, scale, true);
    // A spin of speed 0 only turns the heart. The heart is updated every frame, which makes it safe.
    add(
        doc,
        n,
        "PlatConstantSpin",
        &[("axis", Val::S("y")), ("initial_theta", Val::F(round4(facing + PI / 2.0))), ("speed", Val::F(0.0))],
    );
    add(doc, n, "SetExit", &[("behavior", Val::I(behavior)), ("type", Val::S("solid_platform"))]);
    n
}

/// A platform held at its first path point until a puzzle nudges its tag (duration -100: held).
#[allow(clippy::too_many_arguments)]
fn held_path(
    doc: &mut Doc,
    model: Model,
    scale: [f64; 3],
    points: &[[f64; 3]],
    speed: f64,
    accel: f64,
    tag: Option<i64>,
    sound: Option<i64>,
) -> NodeId {
    let n = platform(doc, model, points[0], Some(scale), false);
    for q in points {
        add(
            doc,
            n,
            "PlatPathPoint",
            &[("duration", Val::I(-100)), ("x", Val::F(q[0])), ("y", Val::F(q[1])), ("z", Val::F(q[2]))],
        );
    }
    add(doc, n, "PlatMaxVelocity", &[("velocity", Val::F(speed))]);
    add(doc, n, "PlatPathAcceleration", &[("acceleration", Val::F(accel))]);
    if let Some(s) = sound {
        add(doc, n, "PlatPathSoundAtPointHit", &[("sound_id", Val::I(s)), ("volume", Val::I(100)), ("pitch", Val::I(128))]);
    }
    add(doc, n, "PlatSetTag", &[("tag", Val::Tag(tag))]);
    n
}

/// A puzzle that nudges a held platform once, when Glover walks into a box around p.
fn release_puzzle(doc: &mut Doc, p: [f64; 3], size: f64, tag: Option<i64>) -> NodeId {
    let z = el(doc, "Puzzle", &[]);
    add(doc, z, "PuzzleNumtimes", &[("n", Val::I(1))]);
    let c = el(doc, "PuzzleCond", &[("cond_type", Val::I(35))]);
    add(
        doc,
        c,
        "PuzzleCondGloverWithinVolume",
        &[
            ("x", Val::F(p[0] - size / 2.0)),
            ("y", Val::F(p[1] - 20.0)),
            ("z", Val::F(p[2] - size / 2.0)),
            ("l", Val::F(size)),
            ("w", Val::F(size)),
            ("h", Val::F(size)),
        ],
    );
    doc.append(z, c);
    let a = el(doc, "PuzzleAction", &[("action_type", Val::I(45))]);
    add(
        doc,
        a,
        "PuzzleActionPlatformNudge",
        &[("velocity", Val::F(10.0)), ("platform_tag", Val::Tag(tag)), ("activation_delay", Val::I(0)), ("flags", Val::I(4))],
    );
    doc.append(z, a);
    z
}

fn ahead(p: [f64; 3], facing: f64, d: f64) -> [f64; 3] {
    [p[0] + facing.sin() * d, p[1], p[2] + facing.cos() * d]
}

/// One preset of the menu.
pub struct Preset {
    pub key: &'static str,
    pub name: &'static str,
    /// What it does.
    pub about: &'static str,
    /// The model it uses.
    pub model: Model<'static>,
}

pub const PRESETS: &[Preset] = &[
    Preset { key: "start", name: "Start ring", about: "Glover starts here. Moves the level's first checkpoint ring if it has one", model: HOOP },
    Preset { key: "ring", name: "Checkpoint ring", about: "Glover restarts here once he has touched it. A rolling ball may pass under it: it counts when the ball comes down through it", model: HOOP },
    Preset { key: "exit", name: "Level exit (heart)", about: "Ends the level when Glover brings the ball to it", model: HEART },
    Preset { key: "bonus", name: "Bonus finish line", about: "A row of five big hearts across the way: the end of a bonus level", model: HEART },
    Preset { key: "world", name: "World exit lift", about: "Carries Glover and the ball up and out of the world when he stands on it holding the ball", model: STEPY },
    Preset { key: "lift", name: "Lift", about: "Rises once when Glover walks onto it. A puzzle with a box around it nudges it", model: LIFT },
    Preset { key: "button", name: "Slam button", about: "Goes down when Glover slams it with the ball. Use its tag in a puzzle condition", model: GLOVEBU },
    Preset { key: "mover", name: "Moving platform", about: "Goes back and forth along two points. Uses the selected object's model, or a deck", model: LIFT },
    Preset { key: "spinner", name: "Spinner", about: "Turns about its center all the time. Uses the selected object's model, or a deck", model: LIFT },
    Preset { key: "conveyor", name: "Conveyor", about: "Pushes what stands on it along the view's direction. 7.5 a frame stops a walking ball", model: LIFT },
    Preset { key: "ice", name: "Ice floor", about: "The ball keeps its speed on it", model: LIFT },
    Preset { key: "spike", name: "Spike", about: "An invisible point that hurts Glover and bursts the ball. Put any model on it as the visible part", model: BALLBIT },
    Preset { key: "zone", name: "Exit zone", about: "An invisible exit: the level ends when the glove holding the ball reaches it", model: BALLBIT },
    Preset { key: "entry", name: "Extra levels entry", about: "In a hub: the glove going in here leads to the way room of the extra levels (Expansion Pak). Without one, the Space world's own levels load", model: BALLBIT },
    Preset { key: "teleport", name: "Teleport pad", about: "Sends the ball 300 ahead along the view's direction. Drag the place in the inspector", model: TELEPAD },
];

pub fn preset(key: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.key == key)
}

/// The elements of a preset at ground point `p`, facing the view's direction, as new elements without a parent, in
/// level order. `tag` is a free platform tag. `model` is the selected object's (id, name), for the presets that
/// take any model. `ground(q)` gives the ground height under a point. `None` for a key that is not a preset.
pub fn build(
    doc: &mut Doc,
    key: &str,
    p: [f64; 3],
    facing: f64,
    tag: Option<i64>,
    model: Option<Model>,
    ground: Option<&dyn Fn([f64; 3]) -> f64>,
) -> Option<Vec<NodeId>> {
    let (deck_model, deck_scale) = match model {
        Some(m) => (m, None),
        None => (LIFT, Some(DECK)),
    };
    Some(match key {
        "start" | "ring" => vec![ring(doc, p, facing, if key == "start" { RING_START } else { RING_MID })],
        "exit" => vec![heart(doc, p, facing, 1, None)],
        "bonus" => {
            let mut out = Vec::new();
            for k in -2..3 {
                let k = k as f64;
                let mut q = [p[0] - k * 120.0 * facing.cos(), p[1], p[2] + k * 120.0 * facing.sin()];
                if let Some(g) = ground {
                    q = [q[0], g(q), q[2]];
                }
                out.push(heart(doc, q, facing, 5, Some([2.5, 2.5, 2.5])));
            }
            out
        }
        "world" => {
            // The lift waits at the floor; exit 3 starts its path when Glover stands on it with the ball. At speed
            // 5 and acceleration 0.5 it stops exactly (at 10 and 1 it overshot by 190). Its top is 46 over its
            // origin.
            let low = [p[0], p[1] - 46.0 + 1.0, p[2]];
            let n = held_path(
                doc,
                STEPY,
                [170.0 / 210.0, 1.0, 170.0 / 210.0],
                &[low, [low[0], low[1] + 700.0, low[2]]],
                5.0,
                0.5,
                tag,
                None,
            );
            add(doc, n, "SetExit", &[("behavior", Val::I(3)), ("type", Val::S("solid_platform"))]);
            vec![n]
        }
        "lift" => {
            // The lift's top is 6 x 1.2 over its origin
            let low = [p[0], p[1] - 7.0, p[2]];
            let n = held_path(doc, LIFT, [1.2, 1.2, 1.2], &[low, [low[0], low[1] + 200.0, low[2]]], 10.0, 0.5, tag, Some(210));
            let z = release_puzzle(doc, p, 60.0, tag);
            vec![n, z]
        }
        "button" => {
            // Duration -2 waits for a slam; it then sinks 6 and stays down until a puzzle sends it back
            let top = [p[0], p[1] - 2.5, p[2]];
            let n = platform(doc, GLOVEBU, top, Some([1.25, 1.25, 1.25]), false);
            add(
                doc,
                n,
                "PlatPathPoint",
                &[("duration", Val::I(-2)), ("x", Val::F(top[0])), ("y", Val::F(top[1])), ("z", Val::F(top[2]))],
            );
            add(
                doc,
                n,
                "PlatPathPoint",
                &[("duration", Val::I(-100)), ("x", Val::F(top[0])), ("y", Val::F(top[1] - 6.0)), ("z", Val::F(top[2]))],
            );
            add(doc, n, "PlatMaxVelocity", &[("velocity", Val::F(1.0))]);
            add(doc, n, "PlatPathAcceleration", &[("acceleration", Val::F(0.8))]);
            add(doc, n, "PlatPathSoundAtPointHit", &[("sound_id", Val::I(210)), ("volume", Val::I(100)), ("pitch", Val::I(128))]);
            add(doc, n, "PlatSetTag", &[("tag", Val::Tag(tag))]);
            vec![n]
        }
        "mover" => {
            let a = [p[0], p[1] + 30.0, p[2]];
            let b = ahead(a, facing, 300.0);
            let n = platform(doc, deck_model, a, deck_scale, true);
            add(doc, n, "PlatMaxVelocity", &[("velocity", Val::F(3.0))]);
            add(doc, n, "PlatPathAcceleration", &[("acceleration", Val::F(0.2))]);
            for q in [a, b] {
                add(
                    doc,
                    n,
                    "PlatPathPoint",
                    &[("duration", Val::I(60)), ("x", Val::F(q[0])), ("y", Val::F(q[1])), ("z", Val::F(q[2]))],
                );
            }
            add(doc, n, "PlatReverseAtEndsOfPath", &[]);
            vec![n]
        }
        "spinner" => {
            let scale = if model.is_none() { Some([6.0, 0.5, 1.0]) } else { None };
            let n = platform(doc, deck_model, [p[0], p[1] + 30.0, p[2]], scale, true);
            add(doc, n, "PlatConstantSpin", &[("axis", Val::S("y")), ("initial_theta", Val::F(0.0)), ("speed", Val::F(0.03))]);
            vec![n]
        }
        "conveyor" => {
            let n = platform(doc, deck_model, [p[0], p[1] - 6.0, p[2]], deck_scale, true);
            add(
                doc,
                n,
                "PlatformConveyor",
                &[
                    ("vel_x", Val::F(round4(facing.sin() * 3.0))),
                    ("vel_y", Val::F(0.0)),
                    ("vel_z", Val::F(round4(facing.cos() * 3.0))),
                ],
            );
            vec![n]
        }
        "ice" => {
            let n = platform(doc, deck_model, [p[0], p[1] - 6.0, p[2]], deck_scale, true);
            add(doc, n, "PlatActorSurfaceType", &[("value", Val::I(2))]);
            vec![n]
        }
        "spike" => {
            // A small invisible ballbit: the spike is a point with a radius of 20. SetExit 0 hides it, and it then
            // takes none of the frame's object matrices. No spin: a speed-0 spin on a platform that is never
            // updated traps a console.
            let n = platform(doc, BALLBIT, [p[0], p[1] + 18.0, p[2]], Some([0.2, 0.2, 0.2]), true);
            add(doc, n, "PlatSpike", &[]);
            add(doc, n, "SetExit", &[("behavior", Val::I(0)), ("type", Val::S("loading_zone"))]);
            vec![n]
        }
        "zone" => {
            let n = platform(doc, BALLBIT, [p[0], p[1] + 30.0, p[2]], None, true);
            add(doc, n, "SetExit", &[("behavior", Val::I(1)), ("type", Val::S("loading_zone"))]);
            vec![n]
        }
        "entry" => {
            // mod/slots.c: a SPACE_EXIT (11) loading zone with the entry tag sets the flag for the extra levels.
            // The game tests the glove's center against the ballbit's radius, 43.8.
            let n = platform(doc, BALLBIT, [p[0], p[1] + 30.0, p[2]], None, true);
            add(doc, n, "SetExit", &[("behavior", Val::I(11)), ("type", Val::S("loading_zone"))]);
            add(doc, n, "PlatSetTag", &[("tag", Val::I(99))]);
            vec![n]
        }
        "teleport" => {
            let n = platform(doc, TELEPAD, [p[0], p[1] - 13.0 * 0.4, p[2]], Some([0.4, 0.4, 0.4]), true);
            let mut to = ahead(p, facing, 300.0);
            if let Some(g) = ground {
                to = [to[0], g(to), to[2]];
            }
            // The format's names are shifted from the game's fields (parse.c TOKEN_TELEPORT): target_tag holds the
            // flags (1 the ball, 2 the glove without the ball, 0x10 stop on arrival), out_framecount the target
            // platform's tag (0: a fixed place), in_framecount and u16_0x12 the shrink and grow frames,
            // u32_0x00..08 the place
            let (bx, by, bz) = (float_bits(to[0]), float_bits(to[1]), float_bits(to[2]));
            add(
                doc,
                n,
                "SetTeleport",
                &[
                    ("target_tag", Val::I(0x13)),
                    ("out_framecount", Val::I(0)),
                    ("in_framecount", Val::I(10)),
                    ("u16_0x12", Val::I(10)),
                    ("u32_0x00", Val::S(&bx)),
                    ("u32_0x04", Val::S(&by)),
                    ("u32_0x08", Val::S(&bz)),
                ],
            );
            vec![n]
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xml(doc: &Doc, nodes: &[NodeId]) -> Vec<String> {
        nodes.iter().map(|n| doc.serialize_node(*n)).collect()
    }

    #[test]
    fn rounds_to_four_decimals() {
        assert_eq!(round4(0.03125), 0.0312);
        assert_eq!(round4(2.71828), 2.7183);
        assert_eq!(round4(-0.00004), -0.0);
        assert_eq!(fmt(round4(-0.00004)), "0.0");
    }

    #[test]
    fn ring_and_lift() {
        let mut doc = Doc::new("Level");
        let made = build(&mut doc, "start", [10.0, 0.0, -5.5], 0.0, Some(7), None, None).unwrap();
        assert_eq!(
            xml(&doc, &made),
            [r#"<Platform object_id="0x70210547" name="hoop.nd"><PlatSetInitialPos x="10.0" y="40.0" z="-5.5"/><PlatCheckpoint u16_0x17="0" theta="0.0"/><PlatConstantSpin axis="y" initial_theta="0.0" speed="0.05"/><PlatNoClip/></Platform>"#]
        );
        let made = build(&mut doc, "lift", [0.0, 100.0, 0.0], 0.0, Some(7), None, None).unwrap();
        let out = xml(&doc, &made);
        assert_eq!(out.len(), 2);
        assert!(out[0].contains(r#"<PlatScale x="1.2" y="1.2" z="1.2"/><PlatPathPoint duration="-100" x="0.0" y="93.0" z="0.0"/>"#));
        assert!(out[0].ends_with(r#"<PlatSetTag tag="7"/></Platform>"#));
        assert!(out[1].contains(r#"<PuzzleCondGloverWithinVolume x="-30.0" y="80.0" z="-30.0" l="60.0" w="60.0" h="60.0"/>"#));
        assert!(build(&mut doc, "nothing", [0.0; 3], 0.0, None, None, None).is_none());
    }

    #[test]
    fn decks_and_ground() {
        let mut doc = Doc::new("Level");
        let made = build(&mut doc, "spinner", [0.0; 3], 0.0, None, Some(("0x00000001", "averylongname")), None).unwrap();
        assert_eq!(
            xml(&doc, &made),
            [r#"<Platform object_id="0x00000001" name="averylo"><PlatSetInitialPos x="0.0" y="30.0" z="0.0"/><PlatConstantSpin axis="y" initial_theta="0.0" speed="0.03"/></Platform>"#]
        );
        let ground = |q: [f64; 3]| q[0] / 10.0;
        let made = build(&mut doc, "bonus", [0.0; 3], 0.0, None, None, Some(&ground)).unwrap();
        assert_eq!(made.len(), 5);
        assert!(xml(&doc, &made)[0].contains(r#"<PlatSetInitialPos x="240.0" y="24.0" z="0.0"/>"#));
        let made = build(&mut doc, "teleport", [0.0; 3], 0.0, None, None, None).unwrap();
        assert!(xml(&doc, &made)[0].contains(r#"u32_0x00="0x00000000" u32_0x04="0x00000000" u32_0x08="0x43960000""#));
    }
}
