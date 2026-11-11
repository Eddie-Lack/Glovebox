//! Puzzles: what each condition and action does, in plain words, the order a puzzle's children go in, and the
//! checks the format cannot make (gloveworld src/puzzle.c and src/parse.c).
//!
//! How the game runs a puzzle: every frame, CheckPuzzles tests each puzzle's conditions. When they hold and the
//! puzzle has firings left (PuzzleNumtimes; none means every frame), each action starts its wait (activation_delay,
//! in game frames) and then runs once. A puzzle with no conditions fires on the first frame.

use crate::commands::Format;
use crate::xmlutil::{body, float_from_bits, fmt_g, int0, inum, num, num0};
use glovebox_core::xml::{Doc, NodeId};

/// The plain name of a condition, by cond_type. The game's own type is cond_type - 9.
pub fn cond_name(code: i128) -> Option<&'static str> {
    Some(match code {
        9 => "Platform rests at a path point",
        12 => "Platform just arrived at a path point (-1: is moving)",
        10 => "Spin paused at a stop",
        13 => "Spin just reached a stop",
        11 => "Orbit paused at a stop",
        14 => "Orbit just reached a stop",
        15 => "Glover is on a platform",
        16 => "Glover steps onto or off a platform",
        17 => "The ball is on a platform",
        18 => "The ball rolls onto or off a platform",
        19 => "An enemy is on a platform",
        20 => "An enemy steps onto or off a platform",
        21 => "Glover touches a platform",
        22 => "Glover starts or stops touching a platform",
        23 => "The ball touches a platform",
        24 => "The ball hits a platform",
        25 => "An enemy touches a platform",
        26 => "An enemy starts or stops touching a platform",
        27 => "Counter = value",
        28 => "Counter \u{2260} value",
        29 => "Counter > value",
        30 => "Counter < value",
        31 => "Pushed block is at an edge",
        32 => "Pushed block just reached an edge",
        33 => "An enemy is gone (or there)",
        34 => "Glover crosses a line",
        35 => "Glover is inside a box",
        36 => "Glover is near a point",
        37 => "The ball is inside a box",
        38 => "The ball is near a point",
        39 => "The camera is inside a box",
        40 => "The camera is near a point",
        41 => "Glover is inside a cylinder",
        42 => "A platform is broken or gone",
        43 => "A pickup or garib group is collected",
        44 => "A pickup is on",
        _ => return None,
    })
}

/// The condition codes that have a plain name, in the order the menus show them.
pub const COND_CODES: &[i128] = &[
    9, 12, 10, 13, 11, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38,
    39, 40, 41, 42, 43, 44,
];

/// The conditions true for only one frame at a time (an edge). The rest stay true while the state lasts.
pub fn is_edge(code: i128) -> bool {
    matches!(code, 13 | 14 | 16 | 18 | 20 | 22 | 24 | 26 | 32)
}

/// The plain name of an action, by action_type.
pub fn action_name(code: i128) -> Option<&'static str> {
    Some(match code {
        45 => "Start a path platform (release it)",
        46 => "Set a platform's spin speed",
        47 => "Set a platform's orbit speed",
        48 => "Send a path platform to a point",
        51 => "Spin a platform about an axis",
        52 => "Turn vents, buzzers, fans or teleports on or off",
        53 => "Set a conveyor's push",
        54 => "Hide or show a platform",
        55 => "Make a platform fall (physics)",
        56 => "Set a counter",
        57 => "Add to a counter",
        58 => "Subtract from a counter",
        59 => "Drop a pickup",
        60 => "Bring in an enemy",
        61 => "Set a platform's velocity",
        62 => "Camera: look at a platform",
        63 => "Camera: look at a point",
        64 => "Camera: move to a point",
        65 => "Camera: set its distance",
        66 => "Camera: raise or lower",
        67 => "Jump an enemy's script",
        68 => "Turn wind on or off",
        69 => "Show a garib group",
        70 => "Zone camera: raise or lower",
        71 => "Zone camera: distance",
        72 => "Zone camera: turn to the focus",
        73 => "Zone camera: fly towards a point",
        74 => "Zone camera: look at Glover",
        75 => "Zone camera (retail 0x4b)",
        76 => "Zone camera (retail 0x4c)",
        77 => "Zone camera: fly to a point near Glover",
        78 => "Change the water height",
        79 => "Earthquake",
        80 => "Set the gravity",
        81 => "Start a cameo",
        82 => "Set a path platform's direction",
        83 => "Make the ball usable",
        84 => "Play or stop a sound",
        85 => "Change the backdrop",
        86 => "Change the fog",
        87 => "Score",
        _ => return None,
    })
}

/// The action codes that have a plain name, in the order the menus show them.
pub const ACTION_CODES: &[i128] = &[
    45, 46, 47, 48, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76,
    77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87,
];

/// Actions that only set the camera, a sound, the backdrop or the fog.
pub fn is_camera_action(code: i128) -> bool {
    (62..78).contains(&code) || matches!(code, 84 | 85 | 86)
}

/// Fields the game reads as a float but the format names u4: their XML holds the float's bits (1.0 = 0x3F800000).
pub const FLOAT_BITS: &[&str] =
    &["hide_enabled", "physics_enabled", "point_idx", "axis_idx", "instruction_idx", "enabled", "direction"];
/// Fields that name a platform by its PlatSetTag, in conditions and actions.
pub const PLAT_TAG: &[&str] = &["plat_tag", "platform_tag"];
/// These actions call the field puzzle_tag but mean a platform.
pub const PLAT_TAG_ACTIONS: &[i128] = &[53, 61, 82];
/// For these codes puzzle_tag is an Enemy's puzzle_tag.
pub const ENEMY_TAG: &[i128] = &[33, 60, 67];
/// activation_delay is an s16 in the game.
pub const DELAY_MAX: i64 = 32767;

/// The body element a cond_type or action_type takes (the Default body for codes the format does not name).
/// `kind` is "PuzzleCond" or "PuzzleAction".
pub fn body_tag(fmt_: &Format, kind: &str, code: i128) -> String {
    match fmt_.switch_case(kind, code) {
        Some(t) => t.to_string(),
        None => format!("{}Default", kind),
    }
}

pub use crate::xmlutil::float_bits;

/// A new condition with its body, without a parent.
pub fn make_cond(fmt_: &Format, doc: &mut Doc, code: i128, attrs: &[(&str, &str)]) -> NodeId {
    let c = doc.make("PuzzleCond", &[("cond_type", &code.to_string())]);
    let b = fmt_.make(doc, &body_tag(fmt_, "PuzzleCond", code), attrs);
    if matches!(code, 35 | 37 | 39) {
        for k in ["l", "w", "h"] {
            doc.set(b, k, "200.0");
        }
    } else if matches!(code, 36 | 38 | 40 | 41) {
        doc.set(b, "range", "100.0");
        if code == 41 {
            doc.set(b, "max_y", "100.0");
        }
    } else if (15..27).contains(&code) || code == 33 {
        for k in ["invert_result", "started_or_stopped"] {
            if doc.has(b, k) {
                doc.set(b, k, "1");
            }
        }
    }
    doc.append(c, b);
    c
}

/// A new action with its body, without a parent.
pub fn make_action(fmt_: &Format, doc: &mut Doc, code: i128, attrs: &[(&str, &str)]) -> NodeId {
    let a = doc.make("PuzzleAction", &[("action_type", &code.to_string())]);
    let b = fmt_.make(doc, &body_tag(fmt_, "PuzzleAction", code), attrs);
    for k in FLOAT_BITS {
        if doc.has(b, k) {
            doc.set(b, k, &float_bits(1.0));
        }
    }
    if code == 45 {
        // -1 keeps the platform's own speed (PlatMaxVelocity)
        doc.set(b, "velocity", "-1.0");
        doc.set(b, "flags", "4");
    }
    doc.append(a, b);
    a
}

/// A puzzle's children go: PuzzleNumtimes, the logic (And/Or/Any), the conditions, the actions.
pub fn rank(doc: &Doc, node: NodeId) -> u8 {
    match doc.tag(node) {
        "PuzzleNumtimes" => 0,
        "PuzzleAnd" | "PuzzleOr" | "PuzzleAny" => 1,
        "PuzzleCond" => 2,
        _ => 3,
    }
}

/// Puts a new child of a puzzle after the children of its rank.
pub fn insert(doc: &mut Doc, puzzle: NodeId, child: NodeId) -> NodeId {
    let r = rank(doc, child);
    let ch = doc.children(puzzle);
    let i = ch.iter().position(|c| rank(doc, *c) > r).unwrap_or(ch.len());
    doc.insert(puzzle, i, child);
    child
}

/// The cond_type or action_type of a part. -1 when it is not a number.
pub fn code(doc: &Doc, node: NodeId, attr: &str) -> i128 {
    int0(doc.get(node, attr)).unwrap_or(-1)
}

/// `None` (every frame while true) or the number of firings.
pub fn fires(doc: &Doc, puzzle: NodeId) -> Option<i64> {
    doc.first(puzzle, "PuzzleNumtimes").map(|n| inum(doc, Some(n), "n", 0))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Logic {
    /// Every condition must hold.
    All,
    /// One condition is enough.
    Any,
    /// At least this many conditions.
    Least(i64),
}

impl Logic {
    pub fn name(self) -> &'static str {
        match self {
            Logic::All => "all",
            Logic::Any => "any",
            Logic::Least(_) => "least",
        }
    }
    /// The number that goes with the name: 0 for all, 1 for any.
    pub fn count(self) -> i64 {
        match self {
            Logic::All => 0,
            Logic::Any => 1,
            Logic::Least(n) => n,
        }
    }
}

/// How a puzzle's conditions combine. The last of And/Or/Any wins (they all set requiredCount).
pub fn logic(doc: &Doc, puzzle: NodeId) -> Logic {
    let mut out = Logic::All;
    for c in doc.children(puzzle) {
        match doc.tag(*c) {
            "PuzzleOr" => out = Logic::Any,
            "PuzzleAny" => {
                let n = inum(doc, Some(*c), "op", 0);
                out = if n == 0 { Logic::All } else { Logic::Least(n) };
            }
            "PuzzleAnd" => out = Logic::All,
            _ => {}
        }
    }
    out
}

fn low(t: &str) -> String {
    if t.starts_with("Glover") {
        return t.to_string();
    }
    let mut ch = t.chars();
    match ch.next() {
        Some(c) => c.to_lowercase().chain(ch).collect(),
        None => String::new(),
    }
}

fn attr_text(doc: &Doc, node: NodeId, attr: &str) -> String {
    doc.get(node, attr).unwrap_or("None").to_string()
}

/// A puzzle in one line: when it fires, what it does, how often.
pub fn summary(doc: &Doc, puzzle: NodeId) -> String {
    let conds: Vec<String> = doc
        .kids(puzzle, "PuzzleCond")
        .iter()
        .map(|c| match cond_name(code(doc, *c, "cond_type")) {
            Some(n) => n.to_string(),
            None => format!("condition {}", attr_text(doc, *c, "cond_type")),
        })
        .collect();
    let acts: Vec<String> = doc
        .kids(puzzle, "PuzzleAction")
        .iter()
        .map(|a| match action_name(code(doc, *a, "action_type")) {
            Some(n) => n.to_string(),
            None => format!("action {}", attr_text(doc, *a, "action_type")),
        })
        .collect();
    let how = logic(doc, puzzle);
    let join = match how {
        Logic::All => " and ",
        Logic::Any => " or ",
        Logic::Least(_) => " / ",
    };
    let when = if conds.is_empty() {
        "At level start".to_string()
    } else {
        let head = match how {
            Logic::Least(n) => format!("At least {} of: ", n),
            _ => "When ".to_string(),
        };
        head + &conds.iter().map(|c| low(c)).collect::<Vec<_>>().join(join)
    };
    let times = match fires(doc, puzzle) {
        None => "every frame while true".to_string(),
        Some(1) => "once".to_string(),
        Some(f) => format!("{} times", f),
    };
    let what = acts.iter().map(|a| low(a)).collect::<Vec<_>>().join("; ");
    format!("{} \u{2192} {} ({})", when, if what.is_empty() { "nothing" } else { &what }, times)
}

/// What a condition or action reads through a pointer of the platforms with its tag (puzzle.c, no NULL check):
/// their path, their motion (a spin) or their orbit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Path,
    Spin,
    Orbit,
}

impl Part {
    fn made_by(self, tag: &str) -> bool {
        match self {
            Part::Path => tag == "PlatPathPoint",
            Part::Spin => crate::commands::is_motion_maker(tag),
            Part::Orbit => tag == "PlatOrbitAroundPoint",
        }
    }
    fn word(self) -> &'static str {
        match self {
            Part::Path => "path",
            Part::Spin => "spin",
            Part::Orbit => "orbit",
        }
    }
}

pub fn cond_part(code: i128) -> Option<Part> {
    match code {
        9 | 12 => Some(Part::Path),
        10 | 13 => Some(Part::Spin),
        11 | 14 => Some(Part::Orbit),
        _ => None,
    }
}

pub fn action_part(code: i128) -> Option<Part> {
    match code {
        48 | 82 => Some(Part::Path),
        46 | 51 => Some(Part::Spin),
        47 => Some(Part::Orbit),
        _ => None,
    }
}

/// One finding of the puzzle checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    pub node: NodeId,
    pub key: String,
    pub message: String,
    /// "err" or "warn".
    pub level: &'static str,
}

fn name_or_code(name: Option<&'static str>, code: i128) -> String {
    match name {
        Some(n) => n.to_string(),
        None => code.to_string(),
    }
}

/// Python's `int(float)` for the message of action 67. Python stops on a NaN or an infinity; here they give the
/// nearest i64.
fn int_of(v: f64) -> i64 {
    v.trunc() as i64
}

/// The puzzle checks of a level.
pub fn rules(doc: &Doc, root: NodeId, fmt_: &Format) -> Vec<Rule> {
    let mut out: Vec<Rule> = Vec::new();
    let plat_tags: Vec<f64> = doc.iter_tags(root, &["PlatSetTag"]).iter().map(|t| num0(doc, *t, "tag")).collect();
    let enemies = doc.kids(root, "Enemy");
    let enemy_tags: Vec<f64> = enemies.iter().map(|e| num0(doc, *e, "puzzle_tag")).collect();
    // (tag, platform) for every PlatSetTag of every platform
    let mut by_tag: Vec<(f64, NodeId)> = Vec::new();
    for p in doc.iter_tags(root, &["Platform", "NullPlatform"]) {
        for t in doc.kids(p, "PlatSetTag") {
            by_tag.push((num0(doc, t, "tag"), p));
        }
    }
    // (puzzle tag, the number of normal instructions) for every enemy
    let normal: Vec<(f64, usize)> = enemies
        .iter()
        .map(|e| {
            (num0(doc, *e, "puzzle_tag"), doc.kids(*e, "NormalInstructions").iter().map(|g| doc.len(*g)).sum::<usize>())
        })
        .collect();
    // True when a platform with this tag does not have the part
    let lacks = |tag: f64, part: Part| {
        by_tag.iter().any(|(t, p)| *t == tag && !doc.children(*p).iter().any(|c| part.made_by(doc.tag(*c))))
    };
    let has_tag = |tags: &[f64], v: f64| tags.iter().any(|t| *t == v);

    for c in doc.children(root) {
        let t = doc.tag(*c);
        if t.starts_with("Puzzle") && t != "Puzzle" {
            out.push(Rule {
                node: *c,
                key: format!("stray:{}", t),
                message: format!(
                    "{} outside a puzzle: the game adds it to the previous level's last puzzle, which is gone.",
                    t
                ),
                level: "err",
            });
        }
    }
    for (i, p) in doc.kids(root, "Puzzle").into_iter().enumerate() {
        let key = format!("puzzle {}", i + 1);
        let mut add = |k: String, message: String, level: &'static str| {
            out.push(Rule { node: p, key: format!("{}{}", key, k), message, level })
        };
        let conds = doc.kids(p, "PuzzleCond");
        let acts = doc.kids(p, "PuzzleAction");
        let how = logic(doc, p);
        let n = how.count();
        if fires(doc, p) == Some(0) {
            add(":zero".into(), "It can fire 0 times, so it never runs.".into(), "warn");
        }
        if (how == Logic::Any && conds.is_empty()) || (matches!(how, Logic::Least(_)) && n > conds.len() as i64) {
            add(
                ":never".into(),
                format!("Needs {} of its {} conditions, so it never fires.", n.max(1), conds.len()),
                "err",
            );
        }
        for (ci, c) in conds.iter().enumerate() {
            let t = code(doc, *c, "cond_type");
            let want = body_tag(fmt_, "PuzzleCond", t);
            let b = match body(doc, *c) {
                Some(b) if doc.tag(b) == want => b,
                other => {
                    add(
                        format!(":cbody:{}", ci),
                        format!(
                            "Condition {} expects a <{}> body, not <{}>. The game would read the wrong number of bytes.",
                            t,
                            want,
                            other.map(|b| doc.tag(b)).unwrap_or("nothing")
                        ),
                        "err",
                    );
                    continue;
                }
            };
            let name = name_or_code(cond_name(t), t);
            for f in ["plat_tag", "puzzle_tag", "reg_a"] {
                if doc.has(b, f) && num0(doc, b, f) > 255.0 {
                    add(
                        ":c255".into(),
                        format!(
                            "{}: the tag {} is over 255. The game keeps conditions' tags in one byte.",
                            name,
                            fmt_g(num0(doc, b, f))
                        ),
                        "err",
                    );
                }
            }
            if doc.has(b, "plat_tag") {
                let tag = num0(doc, b, "plat_tag");
                if let Some(part) = cond_part(t) {
                    if lacks(tag, part) {
                        add(
                            format!(":cpart:{}", fmt_g(tag)),
                            format!(
                                "{}: a platform with the tag {} has no {}; the game reads it through an empty pointer.",
                                name,
                                fmt_g(tag),
                                part.word()
                            ),
                            "err",
                        );
                    }
                }
                if !has_tag(&plat_tags, tag) {
                    add(
                        format!(":cplat:{}", fmt_g(tag)),
                        format!(
                            "{}: no platform has the tag {}, and the game then takes the condition as true.",
                            name,
                            fmt_g(tag)
                        ),
                        "warn",
                    );
                }
            }
        }
        for (ai, a) in acts.iter().enumerate() {
            let t = code(doc, *a, "action_type");
            let want = body_tag(fmt_, "PuzzleAction", t);
            let b = match body(doc, *a) {
                Some(b) if doc.tag(b) == want => b,
                other => {
                    add(
                        format!(":abody:{}", ai),
                        format!(
                            "Action {} expects a <{}> body, not <{}>. The game would read the wrong number of bytes.",
                            t,
                            want,
                            other.map(|b| doc.tag(b)).unwrap_or("nothing")
                        ),
                        "err",
                    );
                    continue;
                }
            };
            let name = name_or_code(action_name(t), t);
            let delay = num0(doc, b, "activation_delay");
            if delay > DELAY_MAX as f64 {
                add(
                    ":delay".into(),
                    format!("A wait of {} frames: over {} it never runs.", fmt_g(delay), DELAY_MAX),
                    "err",
                );
            }
            let has_platform_tag = doc.has(b, "platform_tag");
            let plat_action = PLAT_TAG_ACTIONS.contains(&t);
            // The platform the action names: its text and its number
            let named: Option<(&str, f64)> = if has_platform_tag {
                Some((doc.attr(b, "platform_tag"), num0(doc, b, "platform_tag")))
            } else if plat_action {
                doc.get(b, "puzzle_tag").map(|s| (s, num0(doc, b, "puzzle_tag")))
            } else {
                None
            };
            if let Some((text, v)) = named {
                if !has_tag(&plat_tags, v) && v > 0.0 {
                    add(format!(":aplat:{}", text), format!("{}: no platform has the tag {}.", name, text), "warn");
                }
            }
            if t == 45 && has_platform_tag {
                let v = num0(doc, b, "platform_tag");
                if v < 0.0 && !has_tag(&plat_tags, -v) {
                    add(
                        ":aplatneg".into(),
                        format!(
                            "Start a path platform with the tag {}: the game moves the first platform with the tag {}, and crashes when there is none.",
                            fmt_g(v),
                            fmt_g(-v)
                        ),
                        "err",
                    );
                }
            }
            let ptag = if has_platform_tag {
                Some(num0(doc, b, "platform_tag"))
            } else if plat_action {
                Some(num0(doc, b, "puzzle_tag"))
            } else {
                None
            };
            if let (Some(part), Some(ptag)) = (action_part(t), ptag) {
                if lacks(ptag, part) {
                    add(
                        format!(":apart:{}", fmt_g(ptag)),
                        format!(
                            "{}: a platform with the tag {} has no {}; the game writes through an empty pointer.",
                            name,
                            fmt_g(ptag),
                            part.word()
                        ),
                        "err",
                    );
                }
            }
            let etag = num0(doc, b, "puzzle_tag");
            if t == 67 && doc.has(b, "instruction_idx") {
                let idx = int_of(float_from_bits(doc.get(b, "instruction_idx")));
                if normal.iter().any(|(tag, count)| *tag == etag && !(0 <= idx && idx < *count as i64)) {
                    add(
                        ":a67".into(),
                        format!(
                            "Jump an enemy's script to #{}: an enemy with the puzzle tag {} has no such normal instruction.",
                            idx,
                            fmt_g(etag)
                        ),
                        "err",
                    );
                }
            }
            if matches!(t, 60 | 67) && !has_tag(&enemy_tags, etag) {
                add(":aenemy".into(), format!("{}: no enemy has the puzzle tag {}.", name, fmt_g(etag)), "warn");
            }
        }
        let held = !conds.is_empty()
            && conds.iter().filter(|c| body(doc, **c).is_some()).all(|c| {
                let t = code(doc, *c, "cond_type");
                !is_edge(t) && !(t == 12 && num(doc, body(doc, *c), "arg2", 0.0) >= 0.0)
            });
        if fires(doc, p).is_none() && held && acts.iter().any(|a| !is_camera_action(code(doc, *a, "action_type"))) {
            add(
                ":repeat".into(),
                "It has no firing limit and its conditions can stay true, so its actions run again every frame they hold. Set Fires to Once if they should run once.".into(),
                "warn",
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f() -> &'static Format<'static> {
        Format::shared()
    }

    #[test]
    fn new_parts() {
        let mut doc = Doc::new("Level");
        let c = make_cond(f(), &mut doc, 35, &[]);
        let b = body(&doc, c).unwrap();
        assert_eq!(doc.tag(b), "PuzzleCondGloverWithinVolume");
        assert_eq!(doc.attr(b, "l"), "200.0");
        let a = make_action(f(), &mut doc, 45, &[]);
        let b = body(&doc, a).unwrap();
        assert_eq!(doc.attr(b, "velocity"), "-1.0");
        assert_eq!(doc.attr(b, "flags"), "4");
        assert_eq!(body_tag(f(), "PuzzleAction", 9999), "PuzzleActionDefault");
    }

    #[test]
    fn order_and_summary() {
        let mut doc = Doc::parse("<Level><Puzzle/></Level>").unwrap();
        let p = doc.children(doc.root())[0];
        let a = make_action(f(), &mut doc, 56, &[]);
        insert(&mut doc, p, a);
        let c = make_cond(f(), &mut doc, 15, &[]);
        insert(&mut doc, p, c);
        let n = doc.make("PuzzleNumtimes", &[("n", "1")]);
        insert(&mut doc, p, n);
        let o = doc.new_element("PuzzleOr");
        insert(&mut doc, p, o);
        let tags: Vec<&str> = doc.children(p).iter().map(|c| doc.tag(*c)).collect();
        assert_eq!(tags, ["PuzzleNumtimes", "PuzzleOr", "PuzzleCond", "PuzzleAction"]);
        assert_eq!(fires(&doc, p), Some(1));
        assert_eq!(logic(&doc, p), Logic::Any);
        assert_eq!(summary(&doc, p), "When Glover is on a platform \u{2192} set a counter (once)");
    }

    #[test]
    fn checks() {
        let doc = Doc::parse(
            r#"<Level><PuzzleCond cond_type="15"/><Puzzle><PuzzleOr/><PuzzleNumtimes n="0"/></Puzzle><Puzzle><PuzzleCond cond_type="9"><PuzzleCondGloverWithinVolume/></PuzzleCond><PuzzleAction action_type="45"><PuzzleActionPlatformNudge platform_tag="-3" activation_delay="40000"/></PuzzleAction></Puzzle></Level>"#,
        )
        .unwrap();
        let r = rules(&doc, doc.root(), f());
        let keys: Vec<&str> = r.iter().map(|x| x.key.as_str()).collect();
        assert_eq!(
            keys,
            ["stray:PuzzleCond", "puzzle 1:zero", "puzzle 1:never", "puzzle 2:cbody:0", "puzzle 2:delay", "puzzle 2:aplatneg", "puzzle 2:repeat"]
        );
        assert_eq!(r[4].message, "A wait of 40000 frames: over 32767 it never runs.");
    }
}
