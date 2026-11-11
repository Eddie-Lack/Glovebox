//! The editor's kit: props made by code, some with settings such as a gravestone's words. A port of the former
//! Python kit (`kit.py` and its model files).
//!
//! `items` lists the kit, `clean` checks an item's settings, `build` makes the model as a `Soup`, `Soup::glb` writes
//! the binary glTF that goes through the same converter as an imported model, and `preview_png` draws the small
//! picture for the list. The same settings always give the same model.
//!
//! Settings are a JSON object, as the Python kit took a dict: the window sends every value as text.

pub mod creatures;
pub mod font;
pub mod graveyard;
pub mod image;
pub mod meshkit;
pub mod preview;
pub mod rng;
pub mod ruins;

pub use meshkit::{Soup, Tri};

use crate::pyfmt;
use serde_json::{json, Map, Value};

/// An item's settings: key to value.
pub type Params = Map<String, Value>;

/// What each number of the pumpkin's `face` setting looks like.
pub const PUMPKIN_FACES: [&str; 8] = ["grins", "howls", "one round eye", "weeps", "screams", "eyes sewn shut", "a glove print", "six eyes"];

// id, name, group, note
const ITEMS: [(&str, &str, &str, &str); 23] = [
    ("grave", "Gravestone", "Graveyard", "A crooked stone with your words, about 60 triangles"),
    ("sign", "Signpost", "Graveyard", "A leaning post with a painted board"),
    ("pumpkin", "Pumpkin", "Graveyard", "A jack-o'-lantern with a glowing face, 46 across"),
    ("candles", "Candles", "Graveyard", "Three melted candles with flames"),
    ("coffin", "Coffin", "Graveyard", "A coffin with its lid pushed aside, red eyes inside"),
    ("fence", "Broken fence", "Graveyard", "Leaning posts and hanging planks, 150 long"),
    ("tree1", "Dead tree", "Graveyard", "A tall bent trunk with bare branches"),
    ("tree2", "Crooked tree", "Graveyard", "A short twisted trunk"),
    ("web", "Cobweb", "Graveyard", "A flat web, 40 across, both sides drawn"),
    ("rubble", "Rubble", "Ruins", "A heap of broken stone"),
    ("scorch", "Scorch mark", "Ruins", "A burnt patch for the ground"),
    ("thorns", "Thorns", "Ruins", "A clump of thorny vines (solid when placed as Scenery)"),
    ("gate_atl", "Bone gate", "Ruins", "Bone bars and iron crossbars over a wall of glowing runes; place it as Scenery to bar a way"),
    ("gate_cave", "Flesh gate", "Ruins", "A stitched curtain with eyes behind thorn vines, 150 wide; place it as Scenery to bar a way"),
    ("shade", "Shade", "Creatures", "A dark figure standing on the path"),
    ("hand", "Giant hand", "Creatures", "A hand reaching out of the ground"),
    ("eye", "Watching eye", "Creatures", "A bloodshot eye; reads right only from the front"),
    ("face", "Pale face", "Creatures", "A face with red points for eyes; reads right only from the front"),
    ("effigy", "Rag-doll glove", "Creatures", "A giant stitched glove on a cross, 330 tall"),
    ("glove", "Hanging glove", "Creatures", "A limp glove on a rope"),
    ("gallows", "Gallows", "Creatures", "A beam with gloves hanging from it"),
    ("cage", "Hanging cage", "Creatures", "An iron cage on a chain"),
    ("lantern", "Lantern", "Creatures", "A lantern on a post"),
];

// An item's settings: key, label, kind ("text", "number" or "choice"), default, and a hint or the choices' labels.
fn item_params(id: &str) -> Vec<(&'static str, &'static str, &'static str, Value, Value)> {
    match id {
        "grave" => vec![
            ("text", "Words (/ starts a line)", "text", json!("RIP/YOU"), json!("up to 5 lines of 6 letters")),
            ("lean", "Lean \u{b0}", "number", json!(6), json!("-15 to 15")),
        ],
        "sign" => vec![
            ("top", "Name", "text", json!("KEEP OUT"), json!("up to 11 letters")),
            ("bottom", "Red word", "text", json!("GONE"), json!("up to 10 letters")),
        ],
        "pumpkin" => vec![("face", "Face", "choice", json!(1), json!(PUMPKIN_FACES))],
        _ => Vec::new(),
    }
}

fn find(item: &str) -> Result<&'static (&'static str, &'static str, &'static str, &'static str), String> {
    ITEMS.iter().find(|i| i.0 == item).ok_or_else(|| format!("No kit model {}", py_repr(item)))
}

/// The kit's list: an array of `{id, name, group, note, params: [{key, label, kind, default, extra}]}`.
/// `kind` is "text", "number" (`extra` is a hint) or "choice" (`extra` is the labels of 1, 2, ...).
pub fn items() -> Value {
    Value::Array(
        ITEMS
            .iter()
            .map(|(id, name, group, note)| {
                let params: Vec<Value> = item_params(id)
                    .into_iter()
                    .map(|(key, label, kind, default, extra)| json!({"key": key, "label": label, "kind": kind, "default": default, "extra": extra}))
                    .collect();
                json!({"id": id, "name": name, "group": group, "note": note, "params": params})
            })
            .collect(),
    )
}

// Python's repr() of a string, for the usual cases.
fn py_repr(s: &str) -> String {
    let q = if s.contains('\'') && !s.contains('"') { '"' } else { '\'' };
    let mut out = String::from(q);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == q => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || c as u32 == 0x7f => out.push_str(&format!("\\x{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push(q);
    out
}

// Python's str() of a setting's value.
fn py_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => match (n.as_i64(), n.as_u64(), n.as_f64()) {
            (Some(i), _, _) => i.to_string(),
            (_, Some(u), _) => u.to_string(),
            (_, _, Some(f)) => pyfmt::repr(f),
            _ => n.to_string(),
        },
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::Null => "None".into(),
        other => other.to_string(),
    }
}

// Python's float() of a setting's value.
fn py_float(v: &Value) -> Option<f64> {
    match v {
        Value::String(s) => pyfmt::parse_float(s),
        Value::Number(n) => n.as_f64(),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    }
}

fn py_space(c: char) -> bool {
    c.is_whitespace() || ('\x1c'..='\x1f').contains(&c)
}

fn text(v: &Value, lines: usize, width: usize, name: &str) -> Result<Vec<String>, String> {
    let v = py_str(v).to_uppercase();
    let out: Vec<String> = v.split('/').map(|x| x.trim_matches(py_space).to_string()).filter(|x| !x.is_empty()).collect();
    if out.is_empty() {
        return Err(format!("{name}: write at least one word"));
    }
    if out.len() > lines {
        return Err(format!("{name}: at most {lines} lines"));
    }
    for x in &out {
        if x.chars().count() > width {
            return Err(format!("{name}: \"{x}\" is longer than {width} letters"));
        }
        if !x.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || " !?.,'-".contains(c)) {
            return Err(format!("{name}: only letters, digits, spaces and ! ? . , ' -"));
        }
    }
    Ok(out)
}

fn num(v: &Value, lo: f64, hi: f64, name: &str) -> Result<f64, String> {
    let x = py_float(v).ok_or_else(|| format!("{name} must be a number"))?;
    if !(lo <= x && x <= hi) || x.is_nan() {
        return Err(format!("{name} must be from {lo} to {hi}"));
    }
    Ok(x)
}

// The checked settings of the items that have some.
enum Checked {
    Grave(Vec<String>, f64, u64),
    Sign(String, String, u32, u64),
    Pumpkin(usize),
    Fixed,
}

fn ord_sum(s: &str) -> u64 {
    s.chars().map(|c| c as u64).sum()
}

fn check(item: &str, p: &Params) -> Result<Checked, String> {
    find(item)?;
    let get = |key: &str, default: Value| p.get(key).cloned().unwrap_or(default);
    Ok(match item {
        "grave" => {
            let lines = text(&get("text", json!("RIP")), 5, 6, "Words")?;
            let lean = num(&get("lean", json!(6)), -15.0, 15.0, "Lean")?;
            let seed = ord_sum(&lines.join("/")) % 97;
            Checked::Grave(lines, lean, 3 + seed)
        }
        "sign" => {
            let top = text(&get("top", json!("KEEP OUT")), 1, 11, "Top")?.remove(0);
            let bottom = text(&get("bottom", json!("GONE")), 1, 10, "Bottom")?.remove(0);
            let n = top.chars().count();
            let size = if n <= 4 { 10 } else if n <= 6 { 9 } else if n <= 9 { 8 } else { 7 };
            let seed = 20 + ord_sum(&top) % 50;
            Checked::Sign(top, bottom, size, seed)
        }
        "pumpkin" => Checked::Pumpkin(num(&get("face", json!(1)), 1.0, 8.0, "Face")? as usize - 1),
        _ => Checked::Fixed,
    })
}

/// The item's settings with every value checked, defaults filled in and unknown keys dropped. The error is the
/// sentence to show the player.
pub fn clean(item: &str, params: &Params) -> Result<Params, String> {
    find(item)?;
    let mut out = Params::new();
    for (key, _, _, default, _) in item_params(item) {
        out.insert(key.to_string(), params.get(key).cloned().unwrap_or(default));
    }
    check(item, &out)?;
    Ok(out)
}

/// The model. Settings that are missing take the builder's defaults. A bad setting gives the same error as `clean`.
pub fn build(item: &str, params: &Params) -> Result<Soup, String> {
    Ok(match check(item, params)? {
        Checked::Grave(lines, lean, seed) => graveyard::stone_soup(&lines, lean, seed),
        Checked::Sign(top, bottom, size, seed) => ruins::build_sign(&top, &bottom, size, seed),
        Checked::Pumpkin(kind) => graveyard::build_pumpkin(kind),
        Checked::Fixed => match item {
            "candles" => graveyard::build_candles(),
            "coffin" => graveyard::build_coffin(),
            "fence" => graveyard::build_fence(),
            "tree1" => graveyard::build_tree(1, 150.0, 0.25),
            "tree2" => graveyard::build_tree(5, 110.0, 0.55),
            "web" => graveyard::build_web(),
            "rubble" => ruins::build_rubble(),
            "scorch" => ruins::build_scorch(),
            "thorns" => ruins::build_thorns(),
            "gate_atl" => ruins::build_gate_atl(),
            "gate_cave" => ruins::build_gate_cave(),
            "shade" => creatures::build_shade(),
            "hand" => creatures::build_hand(),
            "eye" => creatures::build_eye(),
            "face" => creatures::build_face(),
            "effigy" => creatures::build_effigy(),
            "glove" => creatures::build_single(1, 70.0),
            "gallows" => creatures::build_gallows(3),
            "cage" => creatures::build_cage(),
            "lantern" => creatures::build_lantern(),
            _ => return Err(format!("No kit model {}", py_repr(item))),
        },
    })
}

/// A short name for the converted model: the item's name and its words, at most 40 letters. `params` are the
/// settings `clean` returned.
pub fn label(item: &str, params: &Params) -> Result<String, String> {
    let name = find(item)?.1;
    let mut words: Vec<String> = Vec::new();
    for (key, ..) in item_params(item) {
        if key == "text" || key == "top" {
            let v = params.get(key).ok_or_else(|| format!("The setting {key} is missing"))?;
            words.push(py_str(v).replace('/', " "));
        }
    }
    let mut words = words.join(" ");
    if item == "pumpkin" {
        let face = params.get("face").and_then(py_float).filter(|f| f.is_finite()).ok_or("The setting face is missing")?;
        // Python counts a list from the end for a negative number.
        let i = (face as i64 - 1).rem_euclid(8) as usize;
        if !(-7.0..9.0).contains(&face) {
            return Err("Face must be from 1 to 8".into());
        }
        words = PUMPKIN_FACES[i].to_string();
    }
    let full = if words.is_empty() { name.to_string() } else { format!("{name}: {words}") };
    Ok(full.chars().take(40).collect())
}

/// The small picture for the kit list, as a PNG file. The Python kit drew it 128 pixels wide.
pub fn preview_png(item: &str, params: &Params, size: usize) -> Result<Vec<u8>, String> {
    Ok(preview::render(&build(item, params)?, size).png_rgb())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(v: Value) -> Params {
        v.as_object().cloned().unwrap_or_default()
    }

    #[test]
    fn the_list_has_every_item_once() {
        let list = items();
        let list = list.as_array().unwrap();
        assert_eq!(list.len(), 23);
        assert_eq!(list[0]["params"][1]["default"], 6);
        assert_eq!(list[2]["params"][0]["extra"][7], "six eyes");
        for it in list {
            assert!(build(it["id"].as_str().unwrap(), &Params::new()).unwrap().count() > 0);
        }
    }

    #[test]
    fn clean_fills_defaults_and_drops_unknown_keys() {
        let c = clean("grave", &p(json!({"text": "boo", "colour": "red"}))).unwrap();
        assert_eq!(Value::Object(c), json!({"text": "boo", "lean": 6}));
        assert!(clean("web", &p(json!({"x": 1}))).unwrap().is_empty());
    }

    #[test]
    fn clean_refuses_bad_settings() {
        let e = |item: &str, v: Value| clean(item, &p(v)).unwrap_err();
        assert_eq!(e("nothing", json!({})), "No kit model 'nothing'");
        assert_eq!(e("grave", json!({"text": " / "})), "Words: write at least one word");
        assert_eq!(e("grave", json!({"text": "a/b/c/d/e/f"})), "Words: at most 5 lines");
        assert_eq!(e("grave", json!({"text": "toolong"})), "Words: \"TOOLONG\" is longer than 6 letters");
        assert_eq!(e("grave", json!({"text": "a_b"})), "Words: only letters, digits, spaces and ! ? . , ' -");
        assert_eq!(e("grave", json!({"lean": "x"})), "Lean must be a number");
        assert_eq!(e("grave", json!({"lean": "16"})), "Lean must be from -15 to 15");
        assert_eq!(e("pumpkin", json!({"face": "nan"})), "Face must be from 1 to 8");
        assert_eq!(e("sign", json!({"top": "a/b"})), "Top: at most 1 lines");
    }

    #[test]
    fn labels() {
        assert_eq!(label("grave", &p(json!({"text": "rip/you", "lean": 6}))).unwrap(), "Gravestone: rip you");
        assert_eq!(label("pumpkin", &p(json!({"face": "3"}))).unwrap(), "Pumpkin: one round eye");
        assert_eq!(label("web", &Params::new()).unwrap(), "Cobweb");
        assert_eq!(label("sign", &p(json!({"top": "a very long name that goes on and on", "bottom": "x"}))).unwrap().chars().count(), 40);
    }

    #[test]
    fn default_models_have_the_expected_sizes() {
        let n = |item: &str| build(item, &Params::new()).unwrap();
        assert_eq!(n("web").count(), 4);
        assert_eq!(n("scorch").count(), 14);
        assert_eq!(n("thorns").count(), 24);
        assert_eq!(n("eye").count(), 16 + 4 * 32 + 16);
        let g = n("grave");
        assert_eq!(g.mats.len(), 3);
        assert_eq!((g.mats[0].w, g.mats[0].h), (32, 64));
        assert!(g.clamp[0] && !g.clamp[1]);
    }

    #[test]
    fn the_same_settings_give_the_same_file() {
        let a = build("sign", &p(json!({"top": "HERE"}))).unwrap().glb();
        let b = build("sign", &p(json!({"top": "here"}))).unwrap().glb();
        assert_eq!(a, b);
        assert_eq!(&preview_png("candles", &Params::new(), 64).unwrap()[..4], b"\x89PNG");
    }
}
