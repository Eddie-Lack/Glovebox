//! Strict validation of level XML from outside the editor (Import XML, drag and drop, autosaved drafts).
//!
//! The file is checked as text, parsed by the hardened parser (no DTD, no entities), validated against the level
//! format (the decomp table: every element is a known command or type, every attribute a field of it, every value
//! within its field's type), and copied element by element into a fresh tree. Only that copy reaches the editor.
//! Comments, processing instructions, foreign namespaces and anything the checks do not know are dropped.

use crate::commands::{as_list, truthy};
use glovebox_core::pyfmt;
use glovebox_core::xml::{Doc, Limits, NodeId, Piece};
use serde_json::Value;
use std::collections::{BTreeSet, HashMap};

/// The text may have this many characters.
pub const MAX_CHARS: usize = 4 << 20;
pub const MAX_ELEMENTS: usize = 50000;
pub const MAX_DEPTH: usize = 8;
pub const MAX_ATTRS: usize = 64;
pub const MAX_ATTR_LENGTH: usize = 64;
pub const MAX_ROOT_ATTR_LENGTH: usize = 64;
/// At most this many errors are reported.
pub const MAX_ERRORS: usize = 50;
const ROOT_ATTRS: [&str; 3] = ["name", "libgarib-version", "data-format-version"];
const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";

fn int_range(t: &str) -> Option<(i64, i64)> {
    Some(match t {
        "u1" => (0, 0xFF),
        "u2" => (0, 0xFFFF),
        "u4" => (0, 0xFFFF_FFFF),
        "s1" => (-0x80, 0x7F),
        "s2" => (-0x8000, 0x7FFF),
        "s4" => (-0x8000_0000, 0x7FFF_FFFF),
        _ => return None,
    })
}

/// An element or attribute name of the level format: `^[A-Za-z_][A-Za-z0-9_]*$`.
fn is_name(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && (b[0].is_ascii_alphabetic() || b[0] == b'_')
        && b.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'_')
}

/// Printable ASCII. One line feed at the end is let through.
fn is_printable(s: &str) -> bool {
    let s = s.strip_suffix('\n').unwrap_or(s);
    s.bytes().all(|b| (0x20..=0x7E).contains(&b))
}

/// `^[-+]?(\d+\.?\d*|\.\d+)([eE][-+]?\d+)?$` on ASCII text.
fn is_float_text(s: &str) -> bool {
    let b = s.strip_suffix('\n').unwrap_or(s).as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'-' || b[i] == b'+') {
        i += 1;
    }
    let digits = |i: &mut usize| {
        let start = *i;
        while *i < b.len() && b[*i].is_ascii_digit() {
            *i += 1;
        }
        *i - start
    };
    if digits(&mut i) > 0 {
        if i < b.len() && b[i] == b'.' {
            i += 1;
            digits(&mut i);
        }
    } else if i < b.len() && b[i] == b'.' {
        i += 1;
        if digits(&mut i) == 0 {
            return false;
        }
    } else {
        return false;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        i += 1;
        if i < b.len() && (b[i] == b'-' || b[i] == b'+') {
            i += 1;
        }
        if digits(&mut i) == 0 {
            return false;
        }
    }
    i == b.len()
}

/// The level format as the checks need it.
pub struct FormatModel<'a> {
    pub decomp: &'a Value,
    /// Group element -> the command that groups into it.
    groups: HashMap<String, String>,
    /// Group element -> the only element it may hold.
    group_child: HashMap<String, String>,
    /// Kind -> the commands that declare it.
    declarers: HashMap<String, BTreeSet<String>>,
}

fn camel(s: &str) -> String {
    s.split(|c: char| c == '_' || c == '-' || c.is_whitespace())
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut ch = w.chars();
            match ch.next() {
                Some(c) => c.to_uppercase().chain(ch).collect::<String>(),
                None => String::new(),
            }
        })
        .collect()
}

impl FormatModel<'static> {
    /// The model of the compiled-in decomp table.
    pub fn shared() -> &'static FormatModel<'static> {
        static M: std::sync::OnceLock<FormatModel<'static>> = std::sync::OnceLock::new();
        M.get_or_init(|| FormatModel::new(glovebox_core::tables::decomp()))
    }
}

impl<'a> FormatModel<'a> {
    pub fn new(decomp: &'a Value) -> FormatModel<'a> {
        let mut m = FormatModel { decomp, groups: HashMap::new(), group_child: HashMap::new(), declarers: HashMap::new() };
        if let Some(commands) = decomp.get("commands").and_then(|c| c.as_object()) {
            for (name, c) in commands {
                if let Some(g) = c.get("groupsInto").and_then(|g| g.as_str()).filter(|g| !g.is_empty()) {
                    m.groups.insert(g.to_string(), name.clone());
                    let ty = c.get("fields").and_then(|f| f.get(0)).and_then(|f| f.get("type")).and_then(|t| t.as_str());
                    m.group_child.insert(g.to_string(), camel(ty.unwrap_or("")));
                }
                for d in as_list(c.get("declares")) {
                    m.declarers.entry(d.to_string()).or_default().insert(name.clone());
                }
            }
        }
        m
    }

    fn command(&self, tag: &str) -> Option<&'a Value> {
        self.decomp.get("commands").and_then(|c| c.get(tag))
    }
    fn type_(&self, tag: &str) -> Option<&'a Value> {
        self.decomp.get("types").and_then(|c| c.get(tag))
    }
    fn any(&self, tag: &str) -> Option<&'a Value> {
        self.command(tag).filter(|c| truthy(Some(c))).or_else(|| self.type_(tag))
    }

    pub fn fields_of(&self, tag: &str) -> &'a [Value] {
        self.any(tag).and_then(|c| c.get("fields")).and_then(|f| f.as_array()).map(|f| f.as_slice()).unwrap_or(&[])
    }

    /// The names a field's enum allows. `None` when the field has no enum.
    pub fn enum_values(&self, tag: &str, f: &Value) -> Option<Vec<&'a str>> {
        let name = f.get("enum").and_then(|e| e.as_str()).filter(|e| !e.is_empty())?;
        let key = if name.contains("::") {
            name.to_string()
        } else {
            format!("{}::{}", self.any(tag).and_then(|c| c.get("ksy")).and_then(|k| k.as_str()).unwrap_or("None"), name)
        };
        let e = self.decomp.get("ksyEnums")?.get(&key)?.as_object()?;
        if e.is_empty() {
            return None;
        }
        Some(e.values().filter_map(|v| v.as_str()).collect())
    }

    fn declares(&self, kind: &str, tag: &str) -> bool {
        self.declarers.get(kind).map(|s| s.contains(tag)).unwrap_or(false)
    }
}

/// What is wrong with an attribute's value. `None` when it is fine.
pub fn check_value(model: &FormatModel, tag: &str, field: &Value, value: &str) -> Option<String> {
    if value.chars().count() > MAX_ATTR_LENGTH {
        return Some(format!("is longer than {} characters", MAX_ATTR_LENGTH));
    }
    if !is_printable(value) {
        return Some("contains non-printable characters".into());
    }
    let enums = model.enum_values(tag, field).filter(|e| !e.is_empty());
    if let Some(e) = &enums {
        if e.contains(&value) {
            return None;
        }
    }
    let t = field.get("type").and_then(|t| t.as_str()).unwrap_or("");
    if t == "str" {
        return if value.len() <= 8 { None } else { Some("is longer than the 8 bytes the format stores".into()) };
    }
    if t == "f4" || t == "f8" {
        if !is_float_text(value) {
            return Some("is not a number".into());
        }
        let n = pyfmt::parse_float(value).unwrap_or(f64::NAN);
        if n.is_nan() || n.abs() > 3.4e38 {
            return Some("is out of range for a 32-bit float".into());
        }
        return None;
    }
    if let Some((lo, hi)) = int_range(t) {
        let b = value.as_bytes();
        let hex = b.len() >= 3 && b.len() <= 10 && b.starts_with(b"0x") && b[2..].iter().all(|c| c.is_ascii_hexdigit());
        let body = value.strip_prefix(['-', '+']).unwrap_or(value);
        let dec = !body.is_empty() && body.len() <= 11 && body.bytes().all(|c| c.is_ascii_digit());
        let n = if hex {
            i64::from_str_radix(&value[2..], 16).ok()
        } else if dec {
            value.strip_prefix('+').unwrap_or(value).parse::<i64>().ok()
        } else {
            None
        };
        let Some(n) = n else {
            if let Some(e) = &enums {
                return Some(format!(
                    "is not one of the allowed values ({}{})",
                    e.iter().take(8).copied().collect::<Vec<_>>().join(", "),
                    if e.len() > 8 { ", \u{2026}" } else { "" }
                ));
            }
            return Some("is not an integer".into());
        };
        if n < lo || n > hi {
            return Some(format!("is out of range for {} ({}\u{2026}{})", t, lo, hi));
        }
        return None;
    }
    Some(format!("has a type ({}) the editor can't validate", t))
}

/// What the check counted.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Info {
    pub elements: usize,
    /// The `name` of `<Level>`.
    pub name: Option<String>,
}

/// The outcome of `sanitize_level_xml`.
#[derive(Clone, Debug)]
pub struct SanitizeResult {
    pub ok: bool,
    /// A fresh tree of only validated content. `None` when there are errors.
    pub root: Option<Doc>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub info: Info,
}

/// The text with its letters folded for a search that ignores case.
fn folded(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '\u{17f}' => 's',
            '\u{212a}' => 'k',
            '\u{130}' | '\u{131}' => 'i',
            c => c.to_ascii_lowercase(),
        })
        .collect()
}

fn py_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

/// What the text holds that level XML never needs.
fn forbidden(text: &str) -> Option<&'static str> {
    let t = folded(text);
    if t.contains("<!doctype") {
        return Some("a DOCTYPE");
    }
    if t.contains("<!entity") {
        return Some("an entity declaration");
    }
    if t.contains("<?xml-stylesheet") {
        return Some("a stylesheet instruction");
    }
    let decl = t.match_indices("xmlns:xi").any(|(i, m)| t[i + m.len()..].trim_start_matches(py_space).starts_with('='));
    if decl || t.contains("<xi:include") {
        return Some("XInclude");
    }
    if t.contains("<![cdata[") {
        return Some("a CDATA section");
    }
    None
}

/// The namespace prefixes in scope: (prefix, URI). The empty prefix is the default namespace.
type Scope = Vec<(String, String)>;

fn scope_of(doc: &Doc, node: NodeId, outer: &Scope) -> Scope {
    let mut s = outer.clone();
    for (k, v) in doc.attrs(node) {
        let prefix = if k == "xmlns" {
            ""
        } else if let Some(p) = k.strip_prefix("xmlns:") {
            p
        } else {
            continue;
        };
        s.retain(|(p, _)| p != prefix);
        s.push((prefix.to_string(), v.clone()));
    }
    s
}

fn uri_of<'s>(scope: &'s Scope, prefix: &str) -> Option<&'s str> {
    if prefix == "xml" {
        return Some(XML_NS);
    }
    scope.iter().find(|(p, _)| p == prefix).map(|(_, u)| u.as_str()).filter(|u| !u.is_empty())
}

/// An element's name as (namespace, local name). A prefix that is not declared stays in the local name.
fn element_name<'s>(scope: &'s Scope, tag: &'s str) -> (Option<&'s str>, &'s str) {
    match tag.split_once(':') {
        Some((p, local)) => match uri_of(scope, p) {
            Some(u) => (Some(u), local),
            None => (None, tag),
        },
        None => (uri_of(scope, ""), tag),
    }
}

/// The attributes of an element as lxml lists them: no namespace declarations, and `{URI}name` for a name with
/// a declared prefix.
fn attributes(doc: &Doc, node: NodeId, scope: &Scope) -> Vec<(String, String)> {
    doc.attrs(node)
        .iter()
        .filter(|(k, _)| k != "xmlns" && !k.starts_with("xmlns:"))
        .map(|(k, v)| {
            let name = match k.split_once(':') {
                Some((p, local)) => match uri_of(scope, p) {
                    Some(u) => format!("{{{}}}{}", u, local),
                    None => k.clone(),
                },
                None => k.clone(),
            };
            (name, v.clone())
        })
        .collect()
}

/// The first element or attribute name whose prefix no element around it declares. An XML reader with
/// namespaces refuses such a file.
fn unbound_prefix(doc: &Doc, node: NodeId, outer: &Scope) -> Option<String> {
    let scope = scope_of(doc, node, outer);
    let bad = |name: &str| match name.split_once(':') {
        Some((p, _)) => p != "xmlns" && uri_of(&scope, p).is_none(),
        None => false,
    };
    if bad(doc.tag(node)) {
        return Some(doc.tag(node).to_string());
    }
    if let Some((k, _)) = doc.attrs(node).iter().find(|(k, _)| bad(k)) {
        return Some(k.clone());
    }
    doc.children(node).iter().find_map(|c| unbound_prefix(doc, *c, &scope))
}

fn clip(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

struct Walk<'a, 'm> {
    model: &'a FormatModel<'m>,
    src: &'a Doc,
    pieces: &'a [Vec<Piece>],
    out: Doc,
    errors: Vec<String>,
    info: Info,
    dropped: usize,
}

impl Walk<'_, '_> {
    fn text_error(&mut self, stack: &[String], text: &str) {
        let t = text.trim_matches(py_space);
        if !t.is_empty() {
            self.errors.push(format!("{}: unexpected text \u{201c}{}\u{201d}.", stack.join(" \u{203a} "), clip(t, 30)));
        }
    }

    fn walk(&mut self, src: NodeId, dst: NodeId, depth: usize, stack: &[String], scope: &Scope, parent_tag: &str) {
        let pieces = self.pieces.get(src.0 as usize).map(|p| p.as_slice()).unwrap_or(&[]);
        let mut k = 0;
        // The text before the first child
        while let Some(Piece::Text(t)) = pieces.get(k) {
            self.text_error(stack, t);
            k += 1;
        }
        let mut index = 0;
        while k < pieces.len() {
            let node = &pieces[k];
            k += 1;
            // The text after a child is reported before the child itself
            while let Some(Piece::Text(t)) = pieces.get(k) {
                self.text_error(stack, t);
                k += 1;
            }
            let node = match node {
                Piece::Child(n) => *n,
                _ => {
                    self.dropped += 1;
                    continue;
                }
            };
            index += 1;
            if self.errors.len() >= MAX_ERRORS {
                return;
            }
            let inner = scope_of(self.src, node, scope);
            let (namespace, tag) = element_name(&inner, self.src.tag(node));
            let mut here = stack.to_vec();
            here.push(format!("{}[{}]", tag, index));
            let path = here.join(" \u{203a} ");
            if namespace.is_some() {
                self.errors.push(format!("{}: namespaced elements aren't part of level XML.", path));
                continue;
            }
            if !is_name(tag) {
                self.errors.push(format!("{}: invalid element name.", path));
                continue;
            }
            self.info.elements += 1;
            if self.info.elements > MAX_ELEMENTS {
                self.errors.push(format!("More than {} elements.", MAX_ELEMENTS));
                return;
            }
            if depth > MAX_DEPTH {
                self.errors.push(format!("{}: nested deeper than {} levels.", path, MAX_DEPTH));
                continue;
            }
            let m = self.model;
            let command = m.command(tag);
            let is_type = m.type_(tag).is_some();
            let is_group = m.groups.contains_key(tag);
            if command.is_none() && !is_type && !is_group {
                self.errors.push(format!("{}: <{}> is not a level command libgarib knows.", path, tag));
                continue;
            }
            let inside = |c: &Value| as_list(c.get("modifies")).iter().any(|k| m.declares(k, parent_tag));
            if let Some(child) = m.group_child.get(parent_tag) {
                if tag != child {
                    self.errors.push(format!("{}: <{}> may only contain <{}>.", path, parent_tag, child));
                }
            } else if is_group {
                let owner = m.groups.get(tag).and_then(|o| m.command(o));
                if !owner.map(inside).unwrap_or(false) {
                    self.errors.push(format!("{}: <{}> can't appear inside <{}>.", path, tag, parent_tag));
                }
            } else if let Some(c) = command {
                if parent_tag == "Level" {
                    let modifies = as_list(c.get("modifies"));
                    if let Some(g) = c.get("groupsInto").and_then(|g| g.as_str()).filter(|g| !g.is_empty()) {
                        self.errors.push(format!("{}: <{}> belongs inside <{}>.", path, tag, g));
                    } else if !modifies.is_empty() {
                        let owners: Vec<String> = modifies
                            .iter()
                            .flat_map(|k| m.declarers.get(*k).into_iter().flatten())
                            .map(|o| format!("<{}>", o))
                            .collect();
                        self.errors.push(format!("{}: <{}> belongs inside {}.", path, tag, owners.join(" or ")));
                    }
                } else if !inside(c) {
                    self.errors.push(format!("{}: <{}> can't appear inside <{}>.", path, tag, parent_tag));
                }
            } else if parent_tag == "Level" {
                self.errors.push(format!("{}: <{}> is part of another command and can't stand alone.", path, tag));
            }

            let fields = m.fields_of(tag);
            let el = self.out.new_element(tag);
            self.out.append(dst, el);
            let attrs = attributes(self.src, node, &inner);
            if attrs.len() > MAX_ATTRS {
                self.errors.push(format!("{}: too many attributes.", path));
                continue;
            }
            for (name, value) in &attrs {
                if name.starts_with('{') {
                    self.errors.push(format!("{}: namespaced attribute {}.", path, name));
                    continue;
                }
                // The last field with this name counts
                let f = fields.iter().rev().find(|f| f.get("id").and_then(|i| i.as_str()) == Some(name.as_str()));
                let f = match f {
                    Some(f) if f.get("type").and_then(|t| t.as_str()) != Some("switch") => f,
                    _ => {
                        self.errors.push(format!("{}: <{}> has no field \u{201c}{}\u{201d}.", path, tag, name));
                        continue;
                    }
                };
                if let Some(problem) = check_value(m, tag, f, value) {
                    self.errors.push(format!("{}: {}=\"{}\" {}.", path, name, clip(value, 24), problem));
                    continue;
                }
                self.out.set(el, name, value);
            }
            let tag = tag.to_string();
            self.walk(node, el, depth + 1, &here, &inner, &tag);
        }
    }
}

/// Validates level XML text. `source` names where it came from in the messages ("file", "level", "saved draft").
pub fn sanitize_level_xml(text: &str, model: &FormatModel, source: &str) -> SanitizeResult {
    let fail = |msg: String| SanitizeResult {
        ok: false,
        root: None,
        errors: vec![msg],
        warnings: Vec::new(),
        info: Info::default(),
    };
    let chars = text.chars().count();
    if chars > MAX_CHARS {
        return fail(format!(
            "The {} is {:.1} MB; level XML is at most {} MB.",
            source,
            chars as f64 / 1048576.0,
            MAX_CHARS >> 20
        ));
    }
    if let Some(what) = forbidden(text) {
        return fail(format!(
            "The {} contains {}, which level XML never needs; it was refused before parsing.",
            source, what
        ));
    }
    // The limits of this module are checked on the tree; the parser only needs room for what they let through
    let lim = Limits { max_bytes: 4 * MAX_CHARS, max_elements: usize::MAX, max_depth: 255, max_attr_len: 4 * MAX_CHARS };
    let (src, pieces) = match Doc::parse_pieces(text, lim) {
        Ok(r) => r,
        Err(e) => return fail(format!("Not well-formed XML: {}", clip(e.0.lines().next().unwrap_or(""), 200))),
    };
    let root = src.root();
    if let Some(name) = unbound_prefix(&src, root, &Scope::new()) {
        return fail(format!("Not well-formed XML: the namespace prefix of {} is not defined", clip(&name, 60)));
    }
    let scope = scope_of(&src, root, &Scope::new());
    if src.tag(root) != "Level" || uri_of(&scope, "").is_some() {
        return fail("The root element must be <Level> (libgarib level XML).".into());
    }

    let mut w =
        Walk { model, src: &src, pieces: &pieces, out: Doc::new("Level"), errors: Vec::new(), info: Info::default(), dropped: 0 };
    let mut warnings = Vec::new();
    let out_root = w.out.root();
    for (name, value) in attributes(&src, root, &scope) {
        if !ROOT_ATTRS.contains(&name.as_str()) {
            warnings.push(format!("Dropped unknown attribute {} on <Level>.", name));
            continue;
        }
        if value.chars().count() > MAX_ROOT_ATTR_LENGTH || !is_printable(&value) {
            w.errors.push(format!("<Level {}> is not short printable text.", name));
            continue;
        }
        w.out.set(out_root, &name, &value);
    }
    w.info.name = w.out.get(out_root, "name").map(|s| s.to_string());
    w.walk(root, out_root, 1, &["Level".to_string()], &scope, "Level");
    let Walk { mut out, mut errors, info, dropped, .. } = w;
    if dropped > 0 {
        let s = if dropped == 1 { "" } else { "s" };
        warnings.push(format!("Dropped {} comment{} or processing instruction{}.", dropped, s, s));
    }
    let ends = out.kids(out_root, "EndLevelData");
    if ends.len() > 1 {
        errors.push("More than one <EndLevelData/>; the game stops parsing at the first.".into());
    } else if ends.len() == 1 && out.children(out_root).last() != ends.first() {
        errors.push("<EndLevelData/> must be the last command; the game ignores everything after it.".into());
    } else if ends.is_empty() && errors.is_empty() {
        let e = out.new_element("EndLevelData");
        out.append(out_root, e);
        warnings.push("Added the missing <EndLevelData/> terminator the game parses until.".into());
    }
    if out.len(out_root) <= 1 {
        errors.push("The level has no commands.".into());
    }
    if !errors.is_empty() {
        errors.truncate(MAX_ERRORS);
        return SanitizeResult { ok: false, root: None, errors, warnings, info };
    }
    SanitizeResult { ok: true, root: Some(out), errors, warnings, info }
}

/// Which scenes an XML's `<Level name>` matches. The level files are named after it: "AT1.lnd" is in
/// "10.AT1lnd.n64.lev".
pub fn match_scenes<'s>(name: Option<&str>, scenes: impl IntoIterator<Item = &'s Value>) -> Vec<&'s Value> {
    let key: String = name.unwrap_or("").chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_lowercase();
    if key.is_empty() {
        return Vec::new();
    }
    scenes
        .into_iter()
        .filter(|s| {
            let land = s.get("landscape").and_then(|l| l.as_str()).unwrap_or("");
            land.split('.').nth(1).unwrap_or("").to_lowercase() == key
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str) -> SanitizeResult {
        sanitize_level_xml(text, FormatModel::shared(), "file")
    }

    #[test]
    fn a_good_level_comes_back_the_same() {
        let text = r#"<Level name="FIX.lnd"><Platform object_id="0x12345678" name="lift"><PlatSetTag tag="7"/><PlatPathPoint duration="60" x="1.5" y="-2e1" z=".5"/></Platform><Enemy type="0" puzzle_tag="0" x="0.0" y="0.0" z="0.0" y_rotation="0.0"><EnemyFinalize/></Enemy><EndLevelData/></Level>"#;
        let r = run(text);
        assert!(r.ok, "{:?}", r.errors);
        assert_eq!(r.root.unwrap().serialize(), text);
        assert_eq!(r.info, Info { elements: 6, name: Some("FIX.lnd".into()) });
        assert!(r.warnings.is_empty());
    }

    #[test]
    fn comments_and_the_missing_end() {
        let r = run("<?xml version='1.0'?><!-- a --><Level odd=\"1\"><!-- b --><PlatNoClip/><?pi?></Level>");
        assert!(!r.ok);
        assert_eq!(
            r.errors,
            [
                "Level \u{203a} PlatNoClip[1]: <PlatNoClip> belongs inside <NullPlatform> or <Platform>.",
                "The level has no commands."
            ]
        );
        let r = run("<Level odd=\"1\"><!-- b --><Platform object_id=\"0x1\" name=\"a\"/><?pi?></Level>");
        assert!(r.ok, "{:?}", r.errors);
        assert_eq!(
            r.warnings,
            [
                "Dropped unknown attribute odd on <Level>.",
                "Dropped 2 comments or processing instructions.",
                "Added the missing <EndLevelData/> terminator the game parses until."
            ]
        );
        assert_eq!(r.root.unwrap().serialize(), r#"<Level><Platform object_id="0x1" name="a"/><EndLevelData/></Level>"#);
    }

    #[test]
    fn refusals_before_parsing() {
        for (text, what) in [
            ("<!DocType a><Level/>", "a DOCTYPE"),
            ("<Level><!entity x></Level>", "an entity declaration"),
            ("<?XML-StyleSheet href='x'?><Level/>", "a stylesheet instruction"),
            ("<Level xmlns:XI \n= 'u'/>", "XInclude"),
            ("<Level><xi:include/></Level>", "XInclude"),
            ("<Level><![CDATA[x]]></Level>", "a CDATA section"),
        ] {
            let r = run(text);
            assert_eq!(
                r.errors,
                [format!("The file contains {}, which level XML never needs; it was refused before parsing.", what)]
            );
        }
        assert!(run("<Level").errors[0].starts_with("Not well-formed XML: "));
        assert_eq!(run("<Other/>").errors, ["The root element must be <Level> (libgarib level XML)."]);
        assert_eq!(run("<Level xmlns='u'/>").errors, ["The root element must be <Level> (libgarib level XML)."]);
        assert_eq!(run("<Level/>").errors, ["The level has no commands."]);
    }

    #[test]
    fn bad_content() {
        let r = run(
            r#"<Level>text<Platform object_id="0x123456789" name="waytoolong" extra="1"/>tail<a:b xmlns:a="u"/><Nope/><PlatSetTag tag="-1"/><Enemy type="zz" x="1e39" xml:lang="en"><Puzzle/></Enemy><EndLevelData/><EndLevelData/></Level>"#,
        );
        assert_eq!(
            r.errors,
            [
                "Level: unexpected text \u{201c}text\u{201d}.",
                "Level: unexpected text \u{201c}tail\u{201d}.",
                "Level \u{203a} Platform[1]: object_id=\"0x123456789\" is not an integer.",
                "Level \u{203a} Platform[1]: name=\"waytoolong\" is longer than the 8 bytes the format stores.",
                "Level \u{203a} Platform[1]: <Platform> has no field \u{201c}extra\u{201d}.",
                "Level \u{203a} b[2]: namespaced elements aren't part of level XML.",
                "Level \u{203a} Nope[3]: <Nope> is not a level command libgarib knows.",
                "Level \u{203a} PlatSetTag[4]: <PlatSetTag> belongs inside <NullPlatform> or <Platform>.",
                "Level \u{203a} PlatSetTag[4]: tag=\"-1\" is out of range for u2 (0\u{2026}65535).",
                "Level \u{203a} Enemy[5]: type=\"zz\" is not one of the allowed values (bovva, cannon, samtex, mallet, generalw, lionfish, chester, keg, \u{2026}).",
                "Level \u{203a} Enemy[5]: x=\"1e39\" is out of range for a 32-bit float.",
                "Level \u{203a} Enemy[5]: namespaced attribute {http://www.w3.org/XML/1998/namespace}lang.",
                "Level \u{203a} Enemy[5] \u{203a} Puzzle[1]: <Puzzle> can't appear inside <Enemy>.",
                "More than one <EndLevelData/>; the game stops parsing at the first.",
            ]
        );
    }

    #[test]
    fn scenes() {
        let scenes: Vec<Value> = serde_json::from_str(
            r#"[{"id": "0a", "landscape": "10.AT1lnd.n64.lev"}, {"id": "0b", "landscape": "nodot"}, {"id": "0c", "landscape": "11.at1LND.n64.lev"}]"#,
        )
        .unwrap();
        let m = match_scenes(Some("AT1.lnd"), &scenes);
        assert_eq!(m.len(), 2);
        assert!(match_scenes(Some("..."), &scenes).is_empty());
        assert!(match_scenes(None, &scenes).is_empty());
    }
}
