//! Autosaved level drafts, kept as files in the user folder (`drafts/<level id>.json`).
//!
//! A draft is the edited level XML for one level id plus when it was saved. The files have the layout the Python
//! editor wrote: `{"id": "0a", "xml": "...", "savedAt": 1700000000.5}`. The caller validates a draft with the same
//! checks as an imported file before it is used.
//!
//! `info.json` in the same folder holds each level's name, music and bonus clock for the game's level table. They
//! are not in the level file.

use glovebox_core::pyfmt;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// A level id: two lower-case hex digits.
pub fn is_id(sid: &str) -> bool {
    sid.len() == 2 && sid.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// A saved draft.
#[derive(Clone, Debug, PartialEq)]
pub struct Draft {
    pub xml: String,
    /// Seconds since 1970.
    pub saved_at: f64,
}

/// What `info.json` holds for one level. `extra`: the level is built as an extra level behind the game mod.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LevelInfo {
    pub name: Option<String>,
    pub song: Option<i64>,
    pub bonus_time: Option<i64>,
    pub extra: bool,
}

impl LevelInfo {
    fn is_empty(&self) -> bool {
        self.name.is_none() && self.song.is_none() && self.bonus_time.is_none() && !self.extra
    }
}

pub struct Drafts {
    pub folder: PathBuf,
}

/// A string as Python's `json.dump` writes it: ASCII only, with `\uXXXX` for the rest.
fn json_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 || (c as u32) > 0x7E => {
                let mut buf = [0u16; 2];
                for u in c.encode_utf16(&mut buf) {
                    out.push_str(&format!("\\u{:04x}", u));
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn opt_int(v: Option<i64>) -> String {
    match v {
        Some(n) => n.to_string(),
        None => "null".into(),
    }
}

/// Writes a file through a temporary file next to it. The file is whole or absent.
fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, text)?;
    fs::rename(&tmp, path)
}

fn now() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

impl Drafts {
    pub fn new(folder: impl Into<PathBuf>) -> Drafts {
        Drafts { folder: folder.into() }
    }

    fn path(&self, sid: &str) -> Option<PathBuf> {
        if is_id(sid) {
            Some(self.folder.join(format!("{}.json", sid)))
        } else {
            None
        }
    }

    /// Saves a draft. False when the id is not a level id or the folder cannot be written.
    pub fn save(&self, sid: &str, xml: &str) -> bool {
        self.save_at(sid, xml, now())
    }

    /// `save` with a given time.
    pub fn save_at(&self, sid: &str, xml: &str, saved_at: f64) -> bool {
        let Some(path) = self.path(sid) else { return false };
        if !saved_at.is_finite() || fs::create_dir_all(&self.folder).is_err() {
            return false;
        }
        let mut text = String::with_capacity(xml.len() + 64);
        text.push_str("{\"id\": ");
        json_string(sid, &mut text);
        text.push_str(", \"xml\": ");
        json_string(xml, &mut text);
        text.push_str(", \"savedAt\": ");
        text.push_str(&pyfmt::repr(saved_at));
        text.push('}');
        write_atomic(&path, &text).is_ok()
    }

    /// The draft of a level. `None` when there is none or the file is damaged.
    pub fn load(&self, sid: &str) -> Option<Draft> {
        let text = fs::read_to_string(self.path(sid)?).ok()?;
        let r: Value = serde_json::from_str(&text).ok()?;
        let r = r.as_object()?;
        let xml = r.get("xml")?.as_str()?.to_string();
        let saved_at = match r.get("savedAt")? {
            Value::Number(n) => n.as_f64()?,
            // Python counts a boolean as a number
            Value::Bool(b) => *b as u8 as f64,
            _ => return None,
        };
        Some(Draft { xml, saved_at })
    }

    pub fn delete(&self, sid: &str) {
        if let Some(p) = self.path(sid) {
            let _ = fs::remove_file(p);
        }
    }

    fn info_path(&self) -> PathBuf {
        self.folder.join("info.json")
    }

    /// The info of every level that has some, in the file's order. Values outside their range read as none.
    pub fn all_info(&self) -> Vec<(String, LevelInfo)> {
        let Ok(text) = fs::read_to_string(self.info_path()) else { return Vec::new() };
        let Ok(Value::Object(r)) = serde_json::from_str::<Value>(&text) else { return Vec::new() };
        let mut out = Vec::new();
        for (sid, v) in &r {
            let Some(v) = v.as_object() else { continue };
            if !is_id(sid) {
                continue;
            }
            let int_in = |key: &str, lo: i64, hi: i64| {
                v.get(key).filter(|n| n.is_i64() || n.is_u64()).and_then(|n| n.as_i64()).filter(|n| (lo..=hi).contains(n))
            };
            out.push((
                sid.clone(),
                LevelInfo {
                    name: v
                        .get("name")
                        .and_then(|n| n.as_str())
                        .filter(|n| n.is_ascii() && n.len() <= 39)
                        .map(|n| n.to_string()),
                    song: int_in("song", 0, 60),
                    bonus_time: int_in("bonus_time", -1, 32766),
                    extra: v.get("extra") == Some(&Value::Bool(true)),
                },
            ));
        }
        out
    }

    pub fn info(&self, sid: &str) -> LevelInfo {
        self.all_info().into_iter().find(|(k, _)| k == sid).map(|(_, v)| v).unwrap_or_default()
    }

    /// Stores a level's info. Info with nothing set removes the level from the file. False when the folder cannot
    /// be written.
    pub fn save_info(&self, sid: &str, info: &LevelInfo) -> bool {
        let mut all = self.all_info();
        let at = all.iter().position(|(k, _)| k == sid);
        match (at, info.is_empty()) {
            (Some(i), true) => {
                all.remove(i);
            }
            (Some(i), false) => all[i].1 = info.clone(),
            (None, false) => all.push((sid.to_string(), info.clone())),
            (None, true) => {}
        }
        let mut text = String::from("{");
        for (i, (k, v)) in all.iter().enumerate() {
            if i > 0 {
                text.push_str(", ");
            }
            json_string(k, &mut text);
            text.push_str(": {\"name\": ");
            match &v.name {
                Some(n) => json_string(n, &mut text),
                None => text.push_str("null"),
            }
            text.push_str(&format!(
                ", \"song\": {}, \"bonus_time\": {}, \"extra\": {}}}",
                opt_int(v.song),
                opt_int(v.bonus_time),
                v.extra
            ));
        }
        text.push('}');
        fs::create_dir_all(&self.folder).is_ok() && write_atomic(&self.info_path(), &text).is_ok()
    }

    /// Every level that has a draft, with when it was saved.
    pub fn list(&self) -> BTreeMap<String, f64> {
        let mut out = BTreeMap::new();
        let Ok(dir) = fs::read_dir(&self.folder) else { return out };
        for entry in dir.flatten() {
            let name = entry.file_name();
            let Some(sid) = name.to_str().and_then(|n| n.strip_suffix(".json")) else { continue };
            if is_id(sid) {
                if let Some(d) = self.load(sid) {
                    out.insert(sid.to_string(), d.saved_at);
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("glovebox-drafts-{}-{}", std::process::id(), name));
        let _ = fs::remove_dir_all(&p);
        p
    }

    #[test]
    fn save_load_delete() {
        let dir = temp("a");
        let d = Drafts::new(dir.join("drafts"));
        assert_eq!(d.load("0a"), None);
        assert!(d.save_at("0a", "<Level name=\"\u{e9}\u{1f600}\"/>\n", 1700000000.5));
        assert_eq!(
            fs::read_to_string(dir.join("drafts/0a.json")).unwrap(),
            "{\"id\": \"0a\", \"xml\": \"<Level name=\\\"\\u00e9\\ud83d\\ude00\\\"/>\\n\", \"savedAt\": 1700000000.5}"
        );
        assert_eq!(d.load("0a"), Some(Draft { xml: "<Level name=\"\u{e9}\u{1f600}\"/>\n".into(), saved_at: 1700000000.5 }));
        assert!(!d.save("../x", "<Level/>"));
        assert!(!d.save("0A", "<Level/>"));
        assert!(d.save("1f", "<Level/>"));
        fs::write(dir.join("drafts/2b.json"), "{\"xml\": 5, \"savedAt\": 1}").unwrap();
        fs::write(dir.join("drafts/2c.json"), "not json").unwrap();
        let l = d.list();
        assert_eq!(l.keys().collect::<Vec<_>>(), ["0a", "1f"]);
        d.delete("0a");
        d.delete("zz");
        assert_eq!(d.load("0a"), None);
        assert!(!dir.join("drafts/0a.json.tmp").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn level_info() {
        let dir = temp("b");
        let d = Drafts::new(&dir);
        assert_eq!(d.info("0a"), LevelInfo::default());
        let a = LevelInfo { name: Some("My \"level\"".into()), song: Some(3), bonus_time: None, extra: true };
        assert!(d.save_info("0a", &a));
        assert!(d.save_info("0b", &LevelInfo { song: Some(60), ..Default::default() }));
        assert_eq!(
            fs::read_to_string(dir.join("info.json")).unwrap(),
            "{\"0a\": {\"name\": \"My \\\"level\\\"\", \"song\": 3, \"bonus_time\": null, \"extra\": true}, \"0b\": {\"name\": null, \"song\": 60, \"bonus_time\": null, \"extra\": false}}"
        );
        assert_eq!(d.info("0a"), a);
        assert!(d.save_info("0a", &LevelInfo::default()));
        assert_eq!(d.all_info().len(), 1);
        fs::write(
            dir.join("info.json"),
            r#"{"0a": {"name": "x", "song": 61, "bonus_time": 1.0, "extra": 1}, "zz": {}, "0b": 5}"#,
        )
        .unwrap();
        assert_eq!(d.all_info(), [("0a".to_string(), LevelInfo { name: Some("x".into()), ..Default::default() })]);
        let _ = fs::remove_dir_all(&dir);
    }
}
