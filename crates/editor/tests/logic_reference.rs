//! The logic modules against the reference dumps.
//!
//! The dumps hold level content. They stay outside the repository. Set `GLOVEBOX_REFERENCE` to the dump folder to
//! run these tests. Without it each test says that it was skipped, and passes.

use glovebox_core::tables;
use glovebox_core::xml::{Doc, NodeId};
use glovebox_editor::commands::{self, Format};
use glovebox_editor::entities::{self, Models};
use glovebox_editor::sanitize::{self, FormatModel};
use glovebox_editor::{camera, drafts, presets, puzzles, scripts, vec, xmlutil};
use serde_json::Value;
use std::collections::HashSet;

fn reference(name: &str) -> Option<Value> {
    let Ok(dir) = std::env::var("GLOVEBOX_REFERENCE") else {
        println!("skipped: GLOVEBOX_REFERENCE is not set");
        return None;
    };
    let path = std::path::Path::new(&dir).join("logic").join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {}", path.display(), e));
    Some(serde_json::from_str(&text).expect("the dump is JSON"))
}

fn f(v: &Value) -> f64 {
    v.as_f64().expect("a number")
}
fn fs(v: &Value) -> Vec<f64> {
    v.as_array().expect("a list").iter().map(f).collect()
}
fn v3(v: &Value) -> [f64; 3] {
    let a = fs(v);
    [a[0], a[1], a[2]]
}
fn q4(v: &Value) -> [f64; 4] {
    let a = fs(v);
    [a[0], a[1], a[2], a[3]]
}
/// The wording was changed to American English after the recordings were made. A recorded text goes through this
/// table (old words, new words) before it is compared.
const AMERICAN: [(&str, &str); 3] =
    [("Turns about its centre", "Turns about its center"), ("fly towards a point", "fly toward a point"), ("swings round to", "swings around to")];

fn american(text: &str) -> String {
    AMERICAN.iter().fold(text.to_string(), |t, (old, new)| t.replace(old, new))
}

fn s(v: &Value) -> &str {
    v.as_str().expect("text")
}
fn opt_s(v: &Value) -> Option<&str> {
    v.as_str()
}
fn list(v: &Value) -> &Vec<Value> {
    v.as_array().expect("a list")
}
fn path(v: &Value) -> Vec<usize> {
    list(v).iter().map(|x| x.as_u64().unwrap() as usize).collect()
}
fn flat3(m: &[[f64; 3]; 3]) -> Vec<f64> {
    m.iter().flatten().copied().collect()
}
fn flat4(m: &[[f64; 4]; 4]) -> Vec<f64> {
    m.iter().flatten().copied().collect()
}

/// The same doubles. -0.0 and 0.0 count as the same.
#[track_caller]
fn exact(got: &[f64], want: &Value, what: &str) {
    let want = fs(want);
    assert_eq!(got.len(), want.len(), "{}", what);
    for (g, w) in got.iter().zip(&want) {
        assert!(g == w || (g.is_nan() && w.is_nan()), "{}: {:?} is not {:?}", what, got, want);
    }
}

#[track_caller]
fn near(got: &[f64], want: &Value, tol: f64, what: &str) {
    let want = fs(want);
    assert_eq!(got.len(), want.len(), "{}", what);
    let size = want.iter().fold(1.0f64, |a, b| a.max(b.abs()));
    for (g, w) in got.iter().zip(&want) {
        assert!((g - w).abs() <= tol * size, "{}: {:?} is not {:?}", what, got, want);
    }
}

fn pairs(v: &[(String, String)]) -> Value {
    Value::Array(v.iter().map(|(k, x)| serde_json::json!([k, x])).collect())
}

fn format() -> &'static Format<'static> {
    Format::shared()
}

#[test]
fn vec_math() {
    let Some(o) = reference("vec.json") else { return };
    for r in list(&o["euler"]) {
        let e = v3(&r["e"]);
        let q = vec::quat_from_euler_yzx(e[0], e[1], e[2]);
        let q2 = vec::quat_from_euler_yxz(e[0], e[1], e[2]);
        exact(&q, &r["yzx"], "yzx");
        exact(&q2, &r["yxz"], "yxz");
        exact(&vec::euler_yzx_from_quat(q), &r["back_yzx"], "back yzx");
        exact(&vec::euler_yxz_from_quat(q2), &r["back_yxz"], "back yxz");
        exact(&flat3(&vec::quat_to_mat3(q)), &r["m3"], "mat3");
        exact(&vec::mat3_to_quat(&vec::quat_to_mat3(q)), &r["m3q"], "mat3 to quat");
    }
    for r in list(&o["quat"]) {
        if r.get("zero_norm").is_some() {
            exact(&vec::quat_normalize([0.0; 4]), &r["zero_norm"], "zero norm");
            exact(&vec::quat_axis_angle([0.0; 3], 1.0), &r["zero_axis"], "zero axis");
            continue;
        }
        let (a, b, v, sc, p, ang) = (q4(&r["a"]), q4(&r["b"]), v3(&r["v"]), v3(&r["s"]), v3(&r["p"]), f(&r["ang"]));
        exact(&vec::quat_mul(a, b), &r["mul"], "mul");
        exact(&vec::quat_inv(a), &r["inv"], "inv");
        let n = vec::quat_normalize(a);
        exact(&n, &r["norm"], "normalize");
        exact(&vec::quat_axis_angle(v, ang), &r["axis"], "axis angle");
        exact(&vec::quat_rotate(n, v), &r["rot"], "rotate");
        let m = vec::compose(p, n, sc);
        exact(&flat4(&m), &r["compose"], "compose");
        exact(&vec::decompose_scale(&m), &r["scale"], "decompose");
        exact(&vec::translate(&m, v), &r["translate"], "translate");
        exact(&vec::euler_yzx_from_quat(a), &r["euler"], "euler");
        near(&flat4(&vec::mat4_inv(&m).unwrap()), &r["m4inv"], 1e-9, "inverse");
    }
    for r in list(&o["look"]) {
        exact(&vec::look_rotation(v3(&r["eye"]), v3(&r["target"]), vec::UP), &r["q"], "look");
    }
    for r in list(&o["persp"]) {
        let a = fs(&r["a"]);
        exact(&flat4(&vec::perspective(a[0], a[1], a[2], a[3])), &r["m"], "perspective");
    }
    for r in list(&o["box"]) {
        let pts: Vec<[f64; 3]> = fs(&r["pts"]).chunks(3).map(|c| [c[0], c[1], c[2]]).collect();
        let mut b = vec::Box3::new();
        b.add_points(&pts);
        exact(&b.min, &r["min"], "min");
        exact(&b.max, &r["max"], "max");
        exact(&b.center(), &r["center"], "center");
        exact(&b.size(), &r["size"], "size");
        exact(&[b.sphere_radius()], &serde_json::json!([r["radius"]]), "radius");
        exact(&b.corners().concat(), &r["corners"], "corners");
        let other: Vec<[f64; 3]> = fs(&r["other"]).chunks(3).map(|c| [c[0], c[1], c[2]]).collect();
        let mut b2 = vec::Box3::new();
        b2.add_points(&other);
        b.add_box(&b2).add_box(&vec::Box3::new());
        exact(&b.min, &r["min2"], "min2");
        exact(&b.max, &r["max2"], "max2");
    }
}

#[test]
fn commands_tables_and_platforms() {
    let Some(o) = reference("commands.json") else { return };
    let fm = format();
    for (tag, want) in o["children_of"].as_object().unwrap() {
        let got: Vec<Value> = fm.children_of(tag).into_iter().map(Value::String).collect();
        assert_eq!(&Value::Array(got), want, "children_of {}", tag);
        assert_eq!(pairs(&fm.defaults(tag)), o["defaults"][tag], "defaults {}", tag);
        assert_eq!(fm.label(tag), s(&o["labels"][tag]));
    }
    let mut d = Doc::new("x");
    let n = fm.make(&mut d, "PlatConstantSpin", &[("speed", "9.0"), ("new", "1")]);
    assert_eq!(d.serialize_node(n), s(&o["make"]));
    let all: Vec<&str> = list(&o["all"]).iter().map(s).collect();
    let (mut with_rules, mut moved) = (0, 0);
    for c in list(&o["cases"]) {
        let what = format!("{} {} {}", s(&c["level"]), s(&c["kind"]), s(&c["xml"]));
        let mut doc = Doc::parse(s(&c["xml"])).unwrap();
        let node = doc.root();
        assert_eq!(doc.serialize(), s(&c["xml"]));
        if !c["rules"].is_null() {
            let got: Vec<Value> = commands::platform_rules(&doc, node)
                .into_iter()
                .map(|r| serde_json::json!([r.key, r.message, r.game_text]))
                .collect();
            with_rules += !got.is_empty() as usize;
            assert_eq!(Value::Array(got), c["rules"], "rules of {}", what);
            let m = commands::misplaced(&doc, node).map(|(n, j)| serde_json::json!([doc.index_of(n), j]));
            assert_eq!(m.unwrap_or(Value::Null), c["misplaced"], "misplaced in {}", what);
        }
        if let Some(want) = c.get("place_for") {
            for (i, t) in all.iter().enumerate() {
                assert_eq!(commands::place_for(&doc, node, t) as u64, want[i].as_u64().unwrap(), "place_for {} in {}", t, what);
                assert_eq!(commands::refusal(&doc, node, t).as_deref(), opt_s(&c["refusal"][i]), "refusal {} in {}", t, what);
            }
        }
        let m = commands::tidy(&mut doc, node);
        moved += m as usize;
        assert_eq!(m, c["moved"].as_bool().unwrap(), "{}", what);
        assert_eq!(doc.serialize(), s(&c["tidied"]), "tidy of {}", what);
    }
    println!("{} cases, {} with rules, {} moved by tidy", list(&o["cases"]).len(), with_rules, moved);
    assert!(with_rules > 1000 && moved > 300);
    for e in list(&o["ensure"]) {
        let mut doc = Doc::parse(s(&e["xml"])).unwrap();
        let root = doc.root();
        for (t, want) in list(&e["tags"]).iter().zip(list(&e["steps"])) {
            commands::ensure_cmd(&mut doc, root, s(t), &[("a", "1")]);
            let c = doc.new_element(s(t));
            commands::insert(&mut doc, root, c);
            assert_eq!(doc.serialize(), s(want));
        }
    }
}

fn node_path(doc: &Doc, n: NodeId) -> Value {
    Value::Array(doc.path_of(n).into_iter().map(|i| Value::from(i as u64)).collect())
}

#[test]
fn puzzles_and_scripts() {
    let Some(o) = reference("puzzles.json") else { return };
    let fm = format();
    let flags = tables::decomp()["ksyEnums"]["enemy_instruction::instruction_flags"].as_object();
    assert!(flags.is_some());
    let mut d = Doc::new("x");
    for (code, want) in o["make_cond"].as_object().unwrap() {
        let code: i128 = code.parse().unwrap();
        let c = puzzles::make_cond(fm, &mut d, code, &[]);
        assert_eq!(d.serialize_node(c), s(want));
        let a = puzzles::make_action(fm, &mut d, code, &[("activation_delay", "5")]);
        assert_eq!(d.serialize_node(a), s(&o["make_action"][code.to_string()]));
    }
    for (t, want) in o["make_instruction"].as_object().unwrap() {
        let t: i128 = t.parse().unwrap();
        let a = scripts::make_instruction(fm, &mut d, t, &[], &[]).unwrap();
        assert_eq!(d.serialize_node(a), s(&want[0]));
        let b = scripts::make_instruction(
            fm,
            &mut d,
            t,
            &[("flags", "2"), ("lifetime", "9")],
            &[("vel_magnitude", "1.0"), ("anim_start_playing", "0")],
        )
        .unwrap();
        assert_eq!(d.serialize_node(b), s(&want[1]));
    }
    for r in list(&o["flags"]) {
        assert_eq!(scripts::flags_value(opt_s(&r[0]), flags) as i64, r[1].as_i64().unwrap(), "flags {:?}", r[0]);
        assert_eq!(scripts::flags_value(opt_s(&r[0]), None) as i64, r[2].as_i64().unwrap(), "flags {:?}", r[0]);
    }
    for r in list(&o["bits"]) {
        if r[0].is_number() {
            assert_eq!(xmlutil::float_bits(f(&r[0])), s(&r[1]));
        } else {
            let v = xmlutil::float_from_bits(opt_s(&r[0]));
            match r[1].as_f64() {
                Some(w) => assert_eq!(v, w, "bits {:?}", r[0]),
                None => assert!(v.is_nan()),
            }
        }
    }
    let (mut n_puzzle, mut n_script) = (0, 0);
    for l in list(&o["levels"]) {
        let what = format!("{} {}", s(&l["level"]), s(&l["kind"]));
        let doc = Doc::parse(s(&l["xml"])).unwrap();
        assert_eq!(doc.serialize(), s(&l["xml"]), "{}", what);
        let root = doc.root();
        let got: Vec<Value> = puzzles::rules(&doc, root, fm)
            .into_iter()
            .map(|r| serde_json::json!([node_path(&doc, r.node), r.key, r.message, r.level]))
            .collect();
        n_puzzle += got.len();
        let want = list(&l["rules"]);
        for (g, w) in got.iter().zip(want) {
            assert_eq!(g, w, "{}", what);
        }
        assert_eq!(got.len(), want.len(), "{}", what);
        for p in list(&l["puzzles"]) {
            let n = doc.node_at(&path(&p["path"])).unwrap();
            assert_eq!(puzzles::summary(&doc, n), american(s(&p["summary"])), "{}", what);
            assert_eq!(puzzles::fires(&doc, n), p["fires"].as_i64(), "{}", what);
            let lg = puzzles::logic(&doc, n);
            assert_eq!(serde_json::json!([lg.name(), lg.count()]), p["logic"], "{}", what);
            let ranks: Vec<Value> = doc.children(n).iter().map(|c| Value::from(puzzles::rank(&doc, *c))).collect();
            assert_eq!(Value::Array(ranks), p["ranks"]);
            let codes: Vec<Value> = doc
                .children(n)
                .iter()
                .map(|c| {
                    let a = if doc.tag(*c) == "PuzzleCond" { "cond_type" } else { "action_type" };
                    Value::from(puzzles::code(&doc, *c, a) as i64)
                })
                .collect();
            assert_eq!(Value::Array(codes), p["codes"]);
        }
        for e in list(&l["enemies"]) {
            let n = doc.node_at(&path(&e["path"])).unwrap();
            for (key, en) in [("rules", flags), ("rules_noenum", None)] {
                let got: Vec<Value> =
                    scripts::rules(&doc, n, en).into_iter().map(|r| serde_json::json!([r.key, r.message, r.level])).collect();
                n_script += got.len();
                assert_eq!(Value::Array(got), e[key], "{} enemy {:?}", what, e["path"]);
            }
            let ls = scripts::lists(&doc, n);
            for g in 0..3 {
                let paths: Vec<Value> = ls[g].iter().map(|i| node_path(&doc, *i)).collect();
                assert_eq!(Value::Array(paths), e["lists"][g]);
                let codes: Vec<Value> = ls[g].iter().map(|i| Value::from(scripts::code(&doc, *i) as i64)).collect();
                assert_eq!(Value::Array(codes), e["codes"][g]);
            }
        }
    }
    println!("{} puzzle findings and {} script findings are the same", n_puzzle, n_script);
    assert!(n_puzzle > 1000 && n_script > 500);
    for e in list(&o["edits"]) {
        if let Some(puzzle) = e.get("puzzle") {
            let mut doc = Doc::parse(s(puzzle)).unwrap();
            let p = doc.root();
            let makes: [(u8, i128, &str); 6] =
                [(1, 56, ""), (0, 35, ""), (2, 0, "PuzzleNumtimes"), (2, 0, "PuzzleOr"), (0, 27, ""), (1, 45, "")];
            for ((kind, code, tag), want) in makes.iter().zip(list(&e["inserts"])) {
                let c = match kind {
                    0 => puzzles::make_cond(fm, &mut doc, *code, &[]),
                    1 => puzzles::make_action(fm, &mut doc, *code, &[]),
                    _ => doc.new_element(tag),
                };
                puzzles::insert(&mut doc, p, c);
                assert_eq!(doc.serialize(), s(want));
            }
            continue;
        }
        let mut doc = Doc::parse(s(&e["xml"])).unwrap();
        let enemy = doc.children(doc.root())[e["enemy"].as_u64().unwrap() as usize];
        for (name, rows) in e["templates"].as_object().unwrap() {
            let made = scripts::template(fm, &mut doc, enemy, name).unwrap();
            let got: Vec<Value> = made.iter().map(|n| Value::String(doc.serialize_node(*n))).collect();
            assert_eq!(&Value::Array(got), rows, "template {}", name);
        }
        assert_eq!(e["templates"].as_object().unwrap().len(), scripts::TEMPLATES.len());
        let inc = |k: i64| Some(k + 1);
        let swap = |k: i64| match k {
            0 => Some(1),
            1 => Some(0),
            _ => None,
        };
        for step in list(&e["steps"]) {
            let kind = s(&step[0]);
            match s(&step[1]) {
                "inc" => {
                    scripts::retarget(&mut doc, enemy, kind, &inc);
                    scripts::retarget_puzzles(&mut doc, enemy, kind, &inc);
                    assert_eq!(doc.serialize(), s(&step[2]), "retarget");
                }
                "swap" => {
                    scripts::retarget(&mut doc, enemy, kind, &swap);
                    scripts::retarget_puzzles(&mut doc, enemy, kind, &swap);
                    assert_eq!(doc.serialize(), s(&step[2]), "retarget");
                    if kind == "ConditionalInstructions" {
                        // The reference step now takes the groups out
                        for g in doc.children(enemy).to_vec() {
                            if scripts::GROUPS.contains(&doc.tag(g)) {
                                doc.detach(g);
                            }
                        }
                    }
                }
                _ => {
                    let g = scripts::group_for(&mut doc, enemy, kind, true).unwrap();
                    let i = scripts::make_instruction(fm, &mut doc, 9, &[], &[]).unwrap();
                    doc.append(g, i);
                    assert_eq!(doc.serialize_node(enemy), s(&step[2]), "group_for");
                }
            }
        }
    }
}

#[test]
fn presets_xml() {
    let Some(o) = reference("presets.json") else { return };
    for (key, want) in o["presets"].as_object().unwrap() {
        let p = presets::preset(key).unwrap();
        assert_eq!(serde_json::json!([p.name, p.about, [p.model.0, p.model.1]]), serde_json::json!([want[0], american(s(&want[1])), want[2]]));
    }
    assert_eq!(o["presets"].as_object().unwrap().len(), presets::PRESETS.len());
    let ground = |q: [f64; 3]| q[0] * 0.01 + q[2] * 0.02 + 5.0;
    let mut doc = Doc::new("x");
    for c in list(&o["cases"]) {
        let model = c["model"].as_array().map(|m| (s(&m[0]), s(&m[1])));
        let g: Option<&dyn Fn([f64; 3]) -> f64> = if c["ground"].as_bool().unwrap() { Some(&ground) } else { None };
        let made = presets::build(&mut doc, s(&c["key"]), v3(&c["p"]), f(&c["facing"]), c["tag"].as_i64(), model, g).unwrap();
        let got: Vec<Value> = made.iter().map(|n| Value::String(doc.serialize_node(*n))).collect();
        assert_eq!(Value::Array(got), c["xml"], "{} {:?} {:?}", s(&c["key"]), c["p"], c["facing"]);
    }
    println!("{} presets are the same", list(&o["cases"]).len());
}

fn check_sanitize(r: &Value, what: &str) {
    let got = sanitize::sanitize_level_xml(s(&r["text"]), FormatModel::shared(), s(&r["source"]));
    let want_errors: Vec<&str> = list(&r["errors"]).iter().map(s).collect();
    if want_errors.len() == 1 && want_errors[0].starts_with("Not well-formed XML: ") {
        // The text after the colon is the parser's own
        assert!(got.errors.len() == 1 && got.errors[0].starts_with("Not well-formed XML: "), "{}: {:?}", what, got.errors);
    } else {
        assert_eq!(got.errors, want_errors, "{}", what);
        assert_eq!(got.info.elements as u64, r["info"]["elements"].as_u64().unwrap(), "{}", what);
        assert_eq!(got.info.name.as_deref(), opt_s(&r["info"]["name"]), "{}", what);
    }
    assert_eq!(got.ok, r["ok"].as_bool().unwrap(), "{}", what);
    let want_warnings: Vec<&str> = list(&r["warnings"]).iter().map(s).collect();
    assert_eq!(got.warnings, want_warnings, "{}", what);
    assert_eq!(got.root.map(|d| d.serialize()).as_deref(), opt_s(&r["xml"]), "{}", what);
}

#[test]
fn sanitize_levels_and_hostile_files() {
    let Some(o) = reference("sanitize.json") else { return };
    let scenes = tables::scenes();
    let scenes: Vec<&Value> = match scenes {
        Value::Object(m) => m.values().collect(),
        Value::Array(a) => a.iter().collect(),
        _ => Vec::new(),
    };
    let ids = |name: Option<&str>| -> Vec<Value> {
        sanitize::match_scenes(name, scenes.iter().copied()).iter().map(|x| x["id"].clone()).collect()
    };
    for l in list(&o["levels"]) {
        check_sanitize(l, s(&l["level"]));
        assert!(l["ok"].as_bool().unwrap() && l["same"].as_bool().unwrap());
        let doc = Doc::parse(s(&l["text"])).unwrap();
        assert_eq!(doc.serialize(), s(&l["xml"]), "{} comes back the same", s(&l["level"]));
        assert_eq!(Value::Array(ids(opt_s(&l["info"]["name"]))), l["scenes"]);
    }
    for r in list(&o["scenes"]) {
        assert_eq!(Value::Array(ids(opt_s(&r[0]))), r[1], "{:?}", r[0]);
    }
    let mut malformed = 0;
    for (i, h) in list(&o["hostile"]).iter().enumerate() {
        let text = s(&h["text"]);
        let short: String = text.chars().take(200).collect();
        check_sanitize(h, &format!("hostile file {}: {}", i, short));
        malformed += list(&h["errors"]).first().map(|e| s(e).starts_with("Not well-formed")).unwrap_or(false) as usize;
    }
    println!("{} hostile files give the same result; for {} of them only the start of the message is compared", list(&o["hostile"]).len(), malformed);
    let model = FormatModel::shared();
    for v in list(&o["values"]) {
        let (tag, fid) = (s(&v[0]), s(&v[1]));
        let field = model.fields_of(tag).iter().find(|x| x["id"] == fid).unwrap();
        assert_eq!(sanitize::check_value(model, tag, field, s(&v[2])).as_deref(), opt_s(&v[3]), "{} {} {:?}", tag, fid, v[2]);
    }
}

struct Catalog {
    all: HashSet<String>,
    loaded: HashSet<String>,
}

impl Models for Catalog {
    fn has_model(&self, oid: &str) -> bool {
        self.all.contains(oid)
    }
    fn is_import(&self, oid: &str) -> bool {
        !self.loaded.contains(oid) && self.all.contains(oid)
    }
}

fn opt_path(doc: &Doc, n: Option<NodeId>) -> Value {
    n.map(|n| node_path(doc, n)).unwrap_or(Value::Null)
}

fn build_level(doc: &Doc, models: &Catalog, id: &str) -> entities::Level {
    let cx = entities::Context { format: format(), tables: tables::noclip_tables(), scene_id: id, models };
    entities::build(doc, doc.root(), &cx)
}

fn zone_json(doc: &Doc, z: &entities::Zone) -> Value {
    serde_json::json!({"vol": opt_path(doc, z.vol), "look": z.look, "speed": z.speed, "distance": z.distance,
        "height": z.height, "fly": z.fly})
}

#[test]
fn entities_of_every_level() {
    let Some(o) = reference("entities.json") else { return };
    let dir = std::env::var("GLOVEBOX_REFERENCE").unwrap();
    let read = |name: &str| -> Value {
        serde_json::from_str(&std::fs::read_to_string(std::path::Path::new(&dir).join(name)).unwrap()).unwrap()
    };
    let catalog: HashSet<String> = read("catalog.json").as_object().unwrap().keys().cloned().collect();
    let loaded = read("loaded_ids.json");
    let mut count = 0;
    for l in list(&o["levels"]) {
        let id = s(&l["level"]);
        let models = Catalog { all: catalog.clone(), loaded: list(&loaded[id]).iter().map(|x| s(x).to_string()).collect() };
        let doc = Doc::parse(s(&l["xml"])).unwrap();
        let level = build_level(&doc, &models, id);
        let ents = &level.ents;
        assert_eq!(opt_path(&doc, level.fog), l["fog"]);
        assert_eq!(opt_path(&doc, level.ambient), l["amb"]);
        let want = list(&l["ents"]);
        assert_eq!(ents.len(), want.len(), "level {}", id);
        for (i, (e, w)) in ents.iter().zip(want).enumerate() {
            let what = format!("level {} entity {} ({})", id, i, e.label);
            count += 1;
            let facts = serde_json::json!({
                "kind": e.kind, "tag": e.tag, "label": e.label, "node": node_path(&doc, e.node), "sub": opt_path(&doc, e.sub),
                "rot": e.rot, "scl": e.scl, "object_id": e.object_id, "imported": e.imported,
                "missing_model": e.missing_model, "is_handle": e.is_handle, "derived": e.derived, "owner": e.owner,
                "plat_tag": e.plat_tag, "parent_tag": e.parent_tag, "parent": e.parent_ent, "zone": e.zone,
                "gate": opt_path(&doc, e.gate), "animate": e.spin.is_some(), "lone": entities::lone_trigger(&doc, e),
                "bind_node": opt_path(&doc, e.bind.as_ref().map(|b| b.node())),
            });
            for (k, v) in facts.as_object().unwrap() {
                assert_eq!(v, &w[k], "{}: {}", what, k);
            }
            let mut why = serde_json::Map::new();
            if let Some(r) = e.why_rot {
                why.insert("rot".into(), r.into());
            }
            if let Some(r) = e.why_scl {
                why.insert("scl".into(), r.into());
            }
            assert_eq!(Value::Object(why), w["why"], "{}", what);
            match &e.bind {
                Some(b) => exact(&b.get(&doc, ents), &w["pos"], &what),
                None => assert!(w["pos"].is_null(), "{}", what),
            }
            if !(e.kind == "hazard" && !e.is_handle) {
                exact(&e.world_pos(&doc, ents), &w["obj_pos"], &what);
            }
            let wp = list(&w["points"]);
            assert_eq!(e.points.len(), wp.len(), "{}", what);
            for (b, p) in e.points.iter().zip(wp) {
                assert_eq!(node_path(&doc, b.node()), p[0], "{}", what);
                exact(&b.get(&doc, ents), &p[1], &what);
            }
            match e.get_quat(&doc) {
                Some(q) => exact(&q, &w["quat"], &what),
                None => assert!(w["quat"].is_null(), "{}", what),
            }
            match e.get_scale(&doc) {
                Some(v) => exact(&v, &w["scale"], &what),
                None => assert!(w["scale"].is_null(), "{}", what),
            }
            match &e.volume {
                Some(v) => {
                    exact(&v.box_size0(&doc), &w["box"], &what);
                    let sizes: Vec<Value> =
                        v.size_fields().iter().enumerate().map(|(k, n)| serde_json::json!([n, v.size_get(&doc, k)])).collect();
                    assert_eq!(Value::Array(sizes), w["sizes"], "{}", what);
                }
                None => assert!(w["box"].is_null() && w["sizes"].is_null(), "{}", what),
            }
            if let Some(eu) = e.spin_euler(&doc, 1.25) {
                exact(&entities::quat_from_xyz(eu), &w["spun"], &what);
            }
        }
        for (n, z) in doc.children(doc.root()).iter().zip(list(&l["zones"])) {
            let zone = entities::camera_zone(&doc, *n);
            assert_eq!(zone.as_ref().map(|z| zone_json(&doc, z)).unwrap_or(Value::Null), z["zone"], "level {}", id);
            let cam = zone.as_ref().and_then(|z| entities::zone_camera(&doc, z));
            assert_eq!(cam.map(|(a, b)| serde_json::json!([a, b])).unwrap_or(Value::Null), z["camera"], "level {}", id);
            if doc.tag(*n) == "Puzzle" {
                let vols: Vec<Value> = entities::trigger_volumes(&doc, *n).iter().map(|v| node_path(&doc, *v)).collect();
                assert_eq!(Value::Array(vols), z["volumes"]);
            }
        }
        let uses: Vec<Value> = doc.children(doc.root()).iter().map(|n| Value::Bool(entities::uses_mod(&doc, *n))).collect();
        assert_eq!(Value::Array(uses), l["uses_mod"]);

        // Every entity moved, on one copy, in order
        let mut d2 = doc.clone();
        for (i, e) in ents.iter().enumerate() {
            if let (Some(b), false) = (&e.bind, e.derived) {
                let p = b.get(&d2, ents);
                b.set(&mut d2, ents, vec::add(p, [1.5 + (i % 3) as f64, -2.25, 3.0001 * (i % 5) as f64]));
            }
        }
        assert_eq!(d2.serialize(), s(&l["moved"]), "level {} moved", id);
        for (i, e) in ents.iter().enumerate() {
            for b in &e.points {
                let p = b.get(&d2, ents);
                b.set(&mut d2, ents, vec::add(p, [-7.0, 0.125 * i as f64, 9.0]));
            }
        }
        assert_eq!(d2.serialize(), s(&l["moved_points"]), "level {} points moved", id);
        // Every entity turned and sized
        let mut d3 = doc.clone();
        for (i, e) in ents.iter().enumerate() {
            if let (Some(q0), true) = (e.get_quat(&d3), e.rot.is_some()) {
                let turn = vec::quat_from_euler_yzx(0.1 * (i % 7) as f64, 0.3 + 0.01 * i as f64, -0.2 * (i % 3) as f64);
                e.set_quat(&mut d3, vec::quat_mul(turn, q0));
            }
        }
        assert_eq!(d3.serialize(), s(&l["turned"]), "level {} turned", id);
        for (i, e) in ents.iter().enumerate() {
            let fac = [1.0 + 0.25 * (i % 4) as f64, 0.5 + 0.5 * (i % 3) as f64, 1.0 + 0.1 * (i % 2) as f64];
            if e.scl == Some("xyz") {
                if let Some(sc) = e.get_scale(&d3) {
                    e.set_scale(&mut d3, [sc[0] * fac[0], sc[1] * fac[1], sc[2] * fac[2]]);
                }
            } else if let Some(v) = &e.volume {
                let s0 = v.box_size0(&d3);
                v.set_box(&mut d3, s0, fac);
            }
        }
        assert_eq!(d3.serialize(), s(&l["sized"]), "level {} sized", id);
        for e in ents {
            if let Some(v) = &e.volume {
                for k in 0..v.size_fields().len() {
                    let g = v.size_get(&d3, k);
                    v.size_set(&mut d3, k, g * 1.5 + k as f64 + 0.3333);
                }
            }
        }
        assert_eq!(d3.serialize(), s(&l["fields"]), "level {} size fields", id);
        // Camera zones rewritten
        let mut d4 = doc.clone();
        let root = d4.root();
        for (i, p) in d4.kids(root, "Puzzle").into_iter().enumerate() {
            if let Some(z) = entities::camera_zone(&d4, p) {
                let center = entities::zone_center(&d4, z.vol.unwrap());
                let key = entities::CAM_PRESETS[i % entities::CAM_PRESETS.len()].0;
                let mut new = entities::preset_zone(key, 0.37 * i as f64 - 4.0, center).unwrap();
                if i % 3 == 0 {
                    new = z.clone();
                    if i % 2 == 1 {
                        new.speed = None;
                    }
                }
                entities::set_camera_zone(&mut d4, p, &new);
            }
        }
        assert_eq!(d4.serialize(), s(&l["zoned"]), "level {} zones", id);
    }
    println!("{} entities of {} levels are the same", count, list(&o["levels"]).len());
    for (r, p) in list(&o["presets"]).iter().zip(entities::CAM_PRESETS.iter().flat_map(|p| [p; 4])) {
        assert_eq!((s(&r["key"]), s(&r["name"]), american(s(&r["note"])).as_str()), *p);
        let z = entities::preset_zone(p.0, f(&r["facing"]), [10.0, 20.0, -30.0]).unwrap();
        let got = serde_json::json!({"look": z.look, "speed": z.speed, "distance": z.distance, "height": z.height, "fly": z.fly});
        for (k, v) in got.as_object().unwrap() {
            assert_eq!(v.as_f64(), r["zone"][k].as_f64(), "{} {}", p.0, k);
            assert_eq!(v.as_array().map(|a| a.len()), r["zone"][k].as_array().map(|a| a.len()));
            if v.is_array() {
                exact(&z.fly.unwrap(), &r["zone"][k], "fly");
            }
        }
    }
    for r in list(&o["dominant"]) {
        assert_eq!(entities::dominant(&fs(&r[0])), f(&r[1]));
    }
    for r in list(&o["rgb"]) {
        exact(&entities::hex_rgb(s(&r[0])), &r[1], "rgb");
    }
}

#[test]
fn camera_and_drafts() {
    let Some(o) = reference("camera.json") else { return };
    for r in list(&o["cams"]) {
        let mut c = camera::Camera::new(f(&r["fov"]), 1.0, 60000.0);
        c.pos = v3(&r["pos"]);
        c.width = f(&r["size"][0]);
        c.height = f(&r["size"][1]);
        let mut orbit = camera::Orbit { target: v3(&r["target"]), enabled: true };
        orbit.update(&mut c);
        exact(&c.quat, &r["quat"], "look");
        exact(&[c.aspect()], &serde_json::json!([r["aspect"]]), "aspect");
        exact(&c.forward(), &r["forward"], "forward");
        exact(&c.right(), &r["right"], "right");
        exact(&c.up(), &r["up"], "up");
        near(&flat4(&c.view()), &r["view"], 1e-12, "view");
        exact(&flat4(&c.proj()), &r["proj"], "proj");
        exact(&flat4(&c.proj_for(640.0, 480.0)), &r["proj2"], "proj for a size");
        let p = v3(&r["p"]);
        near(&c.project(p), &r["project"], 1e-9, "project");
        near(&c.to_pixels(p), &r["pixels"], 1e-9, "pixels");
        let (x, y) = (f(&r["xy"][0]), f(&r["xy"][1]));
        let (ro, rd) = c.ray(x, y).unwrap();
        exact(&ro, &r["ray"][0], "ray origin");
        near(&rd, &r["ray"][1], 1e-7, "ray direction");
        let segs: Vec<([f64; 3], [f64; 3])> = list(&r["segs"]).iter().map(|x| (v3(&x[0]), v3(&x[1]))).collect();
        // The reference ray: a hit at the edge of the reach must not depend on the last bits of the inverse
        let hit = c.segment_hit(&segs, v3(&r["ray"][0]), v3(&r["ray"][1]), 6.0);
        match (hit, r["hit"].as_array()) {
            (Some((d, w)), Some(h)) => {
                near(&[d], &serde_json::json!([h[0]]), 1e-9, "hit distance");
                near(&w, &h[1], 1e-9, "hit point");
            }
            (None, None) => {}
            (g, w) => panic!("hit {:?} is not {:?}", g, w),
        }
        let (dx, dy, steps) = (f(&r["move"][0]), f(&r["move"][1]), f(&r["move"][2]));
        orbit.rotate(&mut c, dx, dy);
        near(&c.pos, &r["rotated"][0], 1e-12, "rotate");
        near(&c.quat, &r["rotated"][1], 1e-12, "rotate");
        orbit.pan(&mut c, dx, dy);
        near(&c.pos, &r["panned"][0], 1e-12, "pan");
        near(&orbit.target, &r["panned"][1], 1e-12, "pan");
        orbit.dolly(&mut c, steps);
        near(&c.pos, &r["dollied"][0], 1e-12, "dolly");
        near(&c.quat, &r["dollied"][1], 1e-12, "dolly");
    }
    // The reference drafts open here, and the files written here are the same bytes
    let dir = std::path::Path::new(&std::env::var("GLOVEBOX_REFERENCE").unwrap()).join("logic").join("drafts_py");
    let recorded = drafts::Drafts::new(&dir);
    let tmp = std::env::temp_dir().join(format!("glovebox-reference-drafts-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let mine = drafts::Drafts::new(&tmp);
    for d in list(&o["drafts"]) {
        let got = recorded.load(s(&d["id"])).unwrap();
        assert_eq!(got.xml, s(&d["xml"]));
        assert!(mine.save_at(s(&d["id"]), &got.xml, got.saved_at));
        assert_eq!(std::fs::read_to_string(tmp.join(format!("{}.json", s(&d["id"])))).unwrap(), s(&d["file"]));
    }
    let info = recorded.all_info();
    let as_json: serde_json::Map<String, Value> = info
        .iter()
        .map(|(k, v)| (k.clone(), serde_json::json!({"name": v.name, "song": v.song, "bonus_time": v.bonus_time, "extra": v.extra})))
        .collect();
    assert_eq!(Value::Object(as_json), o["all_info"]);
    for (k, v) in &info {
        assert!(mine.save_info(k, v));
    }
    assert!(mine.save_info("1f", &drafts::LevelInfo::default()));
    assert_eq!(std::fs::read_to_string(tmp.join("info.json")).unwrap(), s(&o["info_file"]));
    let want: Vec<(&String, f64)> = o["list2"].as_object().unwrap().iter().map(|(k, v)| (k, f(v))).collect();
    let mut want = want;
    want.sort_by(|a, b| a.0.cmp(b.0));
    let got = recorded.list();
    assert_eq!(got.iter().map(|(k, v)| (k, *v)).collect::<Vec<_>>(), want);
    let _ = std::fs::remove_dir_all(&tmp);
}
