//! Enemy scripts: an enemy's normal, conditional and attack instructions (gloveworld src/nmeprocess.c,
//! src/enemy.c).
//!
//! The game keeps one flat list per kind: 30 normal, 10 conditional and 30 attack instructions. The level XML
//! groups consecutive instructions of a kind into `<NormalInstructions>` and the like, and a level may hold several
//! groups of a kind. Indices (a Goto's instr_idx, a Loop's target) count the instructions of a kind across all its
//! groups, from 0.
//!
//! An enemy runs its normal list from the start. An instruction ends when its lifetime runs out (lifetime 0 or -1:
//! no limit) or when its action is done, and the next one runs. The game reads past the list's end when the last
//! one ends. A list must end with a Goto, an End attack, or an instruction that never ends.

use crate::commands::Format;
use crate::xmlutil::{attrs_of, body, float_bits, float_from_bits, int0, inum, make_el, num, num0, put};
use glovebox_core::pyfmt;
use glovebox_core::xml::{Doc, NodeId};
use serde_json::{Map, Value};

pub const GROUPS: [&str; 3] = ["NormalInstructions", "ConditionalInstructions", "AttackInstructions"];

/// The place of a group tag in `GROUPS`.
pub fn group_index(kind: &str) -> Option<usize> {
    GROUPS.iter().position(|g| *g == kind)
}

/// How many instructions of a kind the game has room for.
pub fn limit(kind: &str) -> usize {
    match kind {
        "ConditionalInstructions" => 10,
        _ => 30,
    }
}

pub fn kind_name(kind: &str) -> &'static str {
    match kind {
        "NormalInstructions" => "Normal",
        "ConditionalInstructions" => "Conditional",
        _ => "Attack",
    }
}

/// The plain name of an instr_type.
pub fn type_name(t: i128) -> Option<&'static str> {
    Some(match t {
        0 => "Move to a point",
        1 => "Walk to a point",
        2 => "Turn to a point",
        3 => "Wander in a box",
        4 => "Rest",
        5 => "Shoot (1)",
        6 => "Shoot (2)",
        7 => "Play an animation",
        8 => "Face the player",
        9 => "Follow the player",
        10 => "Grab the player",
        11 => "Throw the player",
        12 => "Flee the player",
        13 => "Circle (steer)",
        14 => "Attack cooldown",
        15 => "Start the attack list",
        16 => "End the attack",
        17 => "Charge",
        18 => "Go to instruction",
        19 => "Do nothing",
        20 => "Loop back",
        21 => "Set the loop counter",
        22 => "Move along a vector",
        23 => "Jump",
        24 => "Die",
        _ => return None,
    })
}

/// The number of instruction types.
pub const TYPE_COUNT: i128 = 25;

/// The targets an instruction's flags can pick: (bit, name).
pub const TARGETS: [(i128, &str); 3] = [(0x40, "the glove"), (0x80, "the ball"), (0x100, "the closer of the two")];

/// These read the target the flags pick (enemy.c: without a target, a garbage pointer).
pub fn needs_target(t: i128) -> bool {
    matches!(t, 8 | 9 | 12 | 17)
}
/// These end only by their lifetime (Wander, Flee, Circle, Charge, Move along a vector).
pub fn never_ends(t: i128) -> bool {
    matches!(t, 3 | 12 | 13 | 17 | 22)
}
/// With lifetime 0 these never end. With -1 they end with their animation.
pub fn ends_by_anim(t: i128) -> bool {
    matches!(t, 4 | 5 | 7 | 8)
}
/// A Goto or a Loop names an index in its list.
pub fn is_jump(t: i128) -> bool {
    matches!(t, 18 | 20)
}

/// The instr_type of an instruction. -1 when it isn't a number.
pub fn code(doc: &Doc, node: NodeId) -> i128 {
    int0(doc.get(node, "instr_type")).unwrap_or(-1)
}

/// Flags as the level XML holds them: a number (decimal or hex), a name, or names joined by |. `enum_` maps a
/// value's text to its name.
pub fn flags_value(s: Option<&str>, enum_: Option<&Map<String, Value>>) -> i128 {
    let s = match s {
        Some(v) if !v.is_empty() => v,
        _ => "0",
    };
    let s = s.trim();
    if let Some(v) = pyfmt::parse_int(s) {
        return v;
    }
    let mut out = 0;
    for part in s.split('|') {
        let part = part.trim();
        if let Some(en) = enum_ {
            // The last value with this name counts
            if let Some((k, _)) = en.iter().rev().find(|(_, v)| v.as_str() == Some(part)) {
                out |= k.trim().parse::<i128>().unwrap_or(0);
            }
        }
    }
    out
}

/// The instructions of each kind in game order, in the order of `GROUPS`.
pub fn lists(doc: &Doc, enemy: NodeId) -> [Vec<NodeId>; 3] {
    GROUPS.map(|g| doc.kids(enemy, g).iter().flat_map(|grp| doc.children(*grp).to_vec()).collect())
}

/// The instructions of one kind in game order.
pub fn list_of(doc: &Doc, enemy: NodeId, kind: &str) -> Vec<NodeId> {
    doc.kids(enemy, kind).iter().flat_map(|grp| doc.children(*grp).to_vec()).collect()
}

/// A new instruction of type `t` with its body, without a parent. `None` when the format has no such type.
pub fn make_instruction(
    fmt_: &Format,
    doc: &mut Doc,
    t: i128,
    attrs: &[(&str, &str)],
    body_attrs: &[(&str, &str)],
) -> Option<NodeId> {
    let tag = fmt_.switch_case("EnemyInstruction", t)?;
    let mut a = attrs_of(&[
        ("instr_type", &t.to_string()),
        ("lifetime", "0"),
        ("execution_condition_param_a", "0.0"),
        ("execution_condition_param_b", "0.0"),
        ("flags", "0"),
        ("execution_condition", "always"),
    ]);
    for (k, v) in attrs {
        put(&mut a, k, v);
    }
    let n = make_el(doc, "EnemyInstruction", &a);
    let b = fmt_.make(doc, tag, body_attrs);
    let given = |k: &str| body_attrs.iter().any(|(n, _)| *n == k);
    if matches!(t, 0 | 1 | 9 | 22) && doc.has(b, "vel_magnitude") && !given("vel_magnitude") {
        doc.set(b, "vel_magnitude", "3.0");
    }
    if t == 4 && !given("anim_start_playing") {
        doc.set(b, "anim_start_playing", "1");
    }
    if needs_target(t) && !attrs.iter().any(|(n, _)| *n == "flags") {
        // The glove, turning as it moves
        doc.set(n, "flags", "65");
    }
    doc.append(n, b);
    Some(n)
}

/// The last group of a kind. With `create`, a missing group is added in the order Normal, Conditional, Attack,
/// before EnemyFinalize.
pub fn group_for(doc: &mut Doc, enemy: NodeId, kind: &str, create: bool) -> Option<NodeId> {
    if let Some(g) = doc.kids(enemy, kind).last() {
        return Some(*g);
    }
    if !create {
        return None;
    }
    let after = group_index(kind)? + 1;
    let later = doc.children(enemy).iter().copied().find(|c| {
        let t = doc.tag(*c);
        GROUPS[after..].contains(&t) || t == "EnemyFinalize"
    });
    let g = doc.new_element(kind);
    match later {
        Some(l) => doc.add_prev(l, g),
        None => doc.append(enemy, g),
    }
    Some(g)
}

/// Rewrites every Goto and Loop index into this kind's list with `fix(old index)` (`None` leaves it). A
/// conditional instruction's Goto jumps in the normal list.
pub fn retarget(doc: &mut Doc, enemy: NodeId, kind: &str, fix: &dyn Fn(i64) -> Option<i64>) {
    if kind == "ConditionalInstructions" || group_index(kind).is_none() {
        return;
    }
    let mut all = list_of(doc, enemy, kind);
    if kind == "NormalInstructions" {
        all.extend(list_of(doc, enemy, "ConditionalInstructions"));
    }
    for ins in all {
        let Some(b) = body(doc, ins) else { continue };
        let field = match code(doc, ins) {
            18 => "instr_idx",
            20 => "u32_0x02",
            _ => continue,
        };
        if doc.has(b, field) {
            if let Some(new) = fix(inum(doc, Some(b), field, 0)) {
                doc.set(b, field, &new.to_string());
            }
        }
    }
}

/// Puzzle action 67 (Jump an enemy's script) names an index in the normal list of every enemy with its puzzle
/// tag. Rewrites it with `fix(old index)` when this enemy is the only one with the tag.
pub fn retarget_puzzles(doc: &mut Doc, enemy: NodeId, kind: &str, fix: &dyn Fn(i64) -> Option<i64>) {
    let mut root = enemy;
    while let Some(p) = doc.parent(root) {
        root = p;
    }
    let tag = num0(doc, enemy, "puzzle_tag");
    let same = doc.kids(root, "Enemy").iter().filter(|e| num0(doc, **e, "puzzle_tag") == tag).count();
    if kind != "NormalInstructions" || same != 1 {
        return;
    }
    for a in doc.iter_tags(root, &["PuzzleAction"]) {
        let Some(b) = body(doc, a) else { continue };
        if inum(doc, Some(a), "action_type", 0) == 67 && num0(doc, b, "puzzle_tag") == tag && doc.has(b, "instruction_idx") {
            let old = float_from_bits(doc.get(b, "instruction_idx")).trunc() as i64;
            if let Some(new) = fix(old) {
                doc.set(b, "instruction_idx", &float_bits(new as f64));
            }
        }
    }
}

/// The names of the ready-made normal scripts.
pub const TEMPLATES: [&str; 4] = ["Chase the glove", "Wander in a box", "Patrol between two points", "Stand still"];

/// A ready-made normal script, as new instructions without a parent. Points are around the enemy.
pub fn template(fmt_: &Format, doc: &mut Doc, enemy: NodeId, name: &str) -> Option<Vec<NodeId>> {
    let (x, y, z) = (num0(doc, enemy, "x"), num0(doc, enemy, "y"), num0(doc, enemy, "z"));
    let r = pyfmt::repr;
    let mut mk = |t: i128, attrs: &[(&str, &str)], body_attrs: &[(&str, &str)]| make_instruction(fmt_, doc, t, attrs, body_attrs);
    let point = |dx: f64, dz: f64| [r(x + dx), r(y), r(z + dz)];
    Some(match name {
        "Chase the glove" => vec![
            mk(4, &[("lifetime", "10")], &[])?,
            mk(9, &[("flags", "65")], &[])?,
            mk(18, &[("lifetime", "-1")], &[("instr_idx", "1")])?,
        ],
        "Wander in a box" => {
            let (hx, hy, hz) = (r(x - 150.0), r(y), r(z - 150.0));
            vec![mk(
                3,
                &[("lifetime", "0")],
                &[
                    ("home_x", &hx),
                    ("home_y", &hy),
                    ("home_z", &hz),
                    ("extent_x", "300.0"),
                    ("extent_y", "0.0"),
                    ("extent_z", "300.0"),
                    // An int the game reads through a float field: 60 as float bits
                    ("min_travel_distance", "8.407790785948902e-44"),
                ],
            )?]
        }
        "Patrol between two points" => {
            let a = point(200.0, 0.0);
            let b = point(-200.0, 0.0);
            let dest = |p: &[String; 3]| -> Vec<(&'static str, String)> {
                vec![("destination_x", p[0].clone()), ("destination_y", p[1].clone()), ("destination_z", p[2].clone())]
            };
            let (da, db) = (dest(&a), dest(&b));
            let da: Vec<(&str, &str)> = da.iter().map(|(k, v)| (*k, v.as_str())).collect();
            let db: Vec<(&str, &str)> = db.iter().map(|(k, v)| (*k, v.as_str())).collect();
            vec![
                mk(1, &[], &da)?,
                mk(4, &[("lifetime", "20")], &[])?,
                mk(1, &[], &db)?,
                mk(4, &[("lifetime", "20")], &[])?,
                mk(18, &[("lifetime", "-1")], &[("instr_idx", "0")])?,
            ]
        }
        "Stand still" => vec![mk(4, &[("lifetime", "0")], &[])?],
        _ => return None,
    })
}

/// One finding of the script checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    pub key: String,
    pub message: String,
    /// "err" or "warn".
    pub level: &'static str,
}

/// The script checks for one enemy. `enum_flags` is the enum of an instruction's flags.
pub fn rules(doc: &Doc, enemy: NodeId, enum_flags: Option<&Map<String, Value>>) -> Vec<Rule> {
    let mut out = Vec::new();
    let mut add = |key: String, message: String, level: &'static str| out.push(Rule { key, message, level });
    let ls = lists(doc, enemy);
    for (k, kind) in GROUPS.iter().enumerate() {
        let items = &ls[k];
        let name = kind_name(kind).to_lowercase();
        for (i, ins) in items.iter().enumerate() {
            let t = code(doc, *ins);
            let b = body(doc, *ins);
            let tname = match type_name(t) {
                Some(n) => n.to_string(),
                None => t.to_string(),
            };
            let label = format!("{} #{} ({})", kind_name(kind), i, tname);
            if type_name(t).is_none() {
                add(
                    format!("type:{}:{}", kind, i),
                    format!(
                        "{}: instruction type {} doesn't exist; the game reads the rest of the level out of step.",
                        label, t
                    ),
                    "err",
                );
                continue;
            }
            if let (true, Some(b)) = (is_jump(t), b) {
                let field = if t == 18 { "instr_idx" } else { "u32_0x02" };
                let target = inum(doc, Some(b), field, 0);
                let len = ls[if *kind == "ConditionalInstructions" { 0 } else { k }].len() as i64;
                if doc.has(b, field) && !(0 <= target && target < len) {
                    add(
                        format!("jump:{}:{}", kind, i),
                        format!("{} goes to #{}, which the list doesn't have.", label, target),
                        "err",
                    );
                }
            }
            if needs_target(t) && flags_value(doc.get(*ins, "flags"), enum_flags) & 0x1C0 == 0 {
                add(
                    format!("target:{}:{}", kind, i),
                    format!(
                        "{} has no target. Pick the glove, the ball or the closer one in its flags (0x40, 0x80, 0x100).",
                        label
                    ),
                    "err",
                );
            }
            if matches!(doc.get(*ins, "execution_condition"), Some("periodic") | Some("9"))
                && num0(doc, *ins, "execution_condition_param_a") == 0.0
            {
                add(
                    format!("periodic:{}:{}", kind, i),
                    format!("{}: 'periodic' every 0 frames divides by zero.", label),
                    "err",
                );
            }
            if t == 15 && ls[2].is_empty() {
                add(format!("attack:{}", i), format!("{} starts the attack list, which is empty.", label), "warn");
            }
        }
        if let (Some(last), true) = (items.last(), *kind != "ConditionalInstructions") {
            let t = code(doc, *last);
            let life = num(doc, Some(*last), "lifetime", 0.0);
            let stays = matches!(t, 18 | 16)
                || (never_ends(t) && life <= 0.0)
                || (ends_by_anim(t) && life == 0.0)
                || (t == 19 && life <= 0.0);
            if !stays {
                let tname = match type_name(t) {
                    Some(n) => n.to_string(),
                    None => t.to_string(),
                };
                add(
                    format!("end:{}", kind),
                    format!(
                        "The {} list's last instruction ({}) can end, and the game then reads past the list. End it with Go to instruction{}.",
                        name,
                        tname,
                        if *kind == "AttackInstructions" { " or End the attack" } else { "" }
                    ),
                    "err",
                );
            }
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
    fn flags() {
        let mut en = Map::new();
        en.insert("64".into(), Value::String("glove".into()));
        en.insert("1".into(), Value::String("turn".into()));
        assert_eq!(flags_value(Some("0x41"), None), 0x41);
        assert_eq!(flags_value(Some(" glove | turn "), Some(&en)), 65);
        assert_eq!(flags_value(Some("nothing"), Some(&en)), 0);
        assert_eq!(flags_value(None, None), 0);
        assert_eq!(flags_value(Some(""), None), 0);
    }

    #[test]
    fn groups_and_instructions() {
        let mut doc = Doc::parse(r#"<Level><Enemy x="10.0" y="0.0" z="5.0"><EnemyFinalize/></Enemy></Level>"#).unwrap();
        let e = doc.children(doc.root())[0];
        assert_eq!(group_for(&mut doc, e, "AttackInstructions", false), None);
        let a = group_for(&mut doc, e, "AttackInstructions", true).unwrap();
        let n = group_for(&mut doc, e, "NormalInstructions", true).unwrap();
        assert_eq!(doc.index_of(n), Some(0));
        assert_eq!(doc.index_of(a), Some(1));
        let follow = make_instruction(f(), &mut doc, 9, &[], &[]).unwrap();
        assert_eq!(doc.attr(follow, "flags"), "65");
        doc.append(n, follow);
        let rows = template(f(), &mut doc, e, "Patrol between two points").unwrap();
        assert_eq!(rows.len(), 5);
        let b = body(&doc, rows[0]).unwrap();
        assert_eq!(doc.attr(b, "destination_x"), "210.0");
        for r in rows {
            doc.append(n, r);
        }
        // The Goto at the end points at 0; shift everything by one
        retarget(&mut doc, e, "NormalInstructions", &|k| Some(k + 1));
        let last = *doc.children(n).last().unwrap();
        assert_eq!(doc.attr(body(&doc, last).unwrap(), "instr_idx"), "1");
        assert!(rules(&doc, e, None).is_empty());
        assert!(make_instruction(f(), &mut doc, 999, &[], &[]).is_none());
    }

    #[test]
    fn checks() {
        let doc = Doc::parse(
            r#"<Level><Enemy><NormalInstructions><EnemyInstruction instr_type="99"/><EnemyInstruction instr_type="9" flags="0" execution_condition="periodic"/><EnemyInstruction instr_type="15"/></NormalInstructions></Enemy></Level>"#,
        )
        .unwrap();
        let e = doc.children(doc.root())[0];
        let keys: Vec<String> = rules(&doc, e, None).into_iter().map(|r| r.key).collect();
        assert_eq!(
            keys,
            [
                "type:NormalInstructions:0",
                "target:NormalInstructions:1",
                "periodic:NormalInstructions:1",
                "attack:2",
                "end:NormalInstructions"
            ]
        );
    }
}
