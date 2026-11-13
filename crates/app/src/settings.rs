//! The app's settings file, `settings.ini` in the user folder. It holds `key=value` lines: `scene`, `fog`,
//! `triggers`, `saveDir`, `emulator`, `exits`, `theme`, `info` and `tour`.

use glovebox_editor::editor::Settings;
use std::collections::BTreeMap;
use std::path::PathBuf;

pub struct IniSettings {
    path: Option<PathBuf>,
    values: BTreeMap<String, String>,
}

/// Reads a value `escape` wrote: a backslash is written as two, and a value with spaces or commas is in quotes.
fn unescape(v: &str) -> String {
    let v = v.trim();
    let v = v.strip_prefix('"').and_then(|x| x.strip_suffix('"')).unwrap_or(v);
    let mut out = String::new();
    let mut chars = v.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some(d) => out.push(d),
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn escape(v: &str) -> String {
    let body = v.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n");
    if body.contains([' ', ',', ';', '=']) {
        format!("\"{body}\"")
    } else {
        body
    }
}

impl IniSettings {
    /// Reads the file. A missing or unreadable file gives empty settings.
    pub fn open(path: impl Into<PathBuf>) -> IniSettings {
        let path = path.into();
        let mut values = BTreeMap::new();
        if let Ok(text) = std::fs::read_to_string(&path) {
            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with(['[', ';', '#']) {
                    continue;
                }
                if let Some((k, v)) = line.split_once('=') {
                    values.insert(k.trim().to_string(), unescape(v));
                }
            }
        }
        IniSettings { path: Some(path), values }
    }

    /// Settings that are not written anywhere.
    pub fn in_memory() -> IniSettings {
        IniSettings { path: None, values: BTreeMap::new() }
    }

    pub fn remove(&mut self, key: &str) {
        if self.values.remove(key).is_some() {
            self.write();
        }
    }

    fn write(&self) {
        let Some(path) = &self.path else { return };
        let mut text = String::from("[General]\n");
        for (k, v) in &self.values {
            text.push_str(&format!("{k}={}\n", escape(v)));
        }
        let tmp = path.with_extension("ini.tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }
}

impl Settings for IniSettings {
    fn value(&self, key: &str) -> Option<String> {
        self.values.get(key).cloned()
    }

    fn set_value(&mut self, key: &str, value: &str) {
        if self.values.get(key).map(String::as_str) != Some(value) {
            self.values.insert(key.to_string(), value.to_string());
            self.write();
        }
    }
}

/// The settings the window and the editor share.
#[derive(Clone)]
pub struct Shared(pub std::rc::Rc<std::cell::RefCell<IniSettings>>);

impl Shared {
    pub fn new(s: IniSettings) -> Shared {
        Shared(std::rc::Rc::new(std::cell::RefCell::new(s)))
    }
    pub fn get(&self, key: &str) -> Option<String> {
        self.0.borrow().value(key)
    }
    pub fn set(&self, key: &str, value: &str) {
        self.0.borrow_mut().set_value(key, value);
    }
    pub fn remove(&self, key: &str) {
        self.0.borrow_mut().remove(key);
    }
}

impl Settings for Shared {
    fn value(&self, key: &str) -> Option<String> {
        self.get(key)
    }
    fn set_value(&mut self, key: &str, value: &str) {
        self.set(key, value);
    }
}
