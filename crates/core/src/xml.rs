//! The level XML as a tree.
//!
//! A `Doc` owns every node. A `NodeId` names one node of its `Doc`. A node that is removed from its parent stays in
//! the `Doc` and can be inserted again. Attributes keep the order they were set in.
//!
//! The parser reads only what a level file needs: elements, attributes and text. It refuses a DOCTYPE and entity
//! declarations, and it knows only the five predefined entities and numeric character references. Comments and
//! processing instructions are dropped. Whitespace-only text is dropped.
//!
//! `serialize` and `pretty` write the bytes that lxml writes for the same tree (`ET.tostring`, and `ET.indent` with
//! two spaces). Undo snapshots, drafts and the comparison with the vanilla level depend on that.

use std::fmt::Write;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct NodeId(pub u32);

#[derive(Clone, Debug, Default)]
struct Node {
    tag: String,
    attrs: Vec<(String, String)>,
    text: String,
    parent: Option<NodeId>,
    children: Vec<NodeId>,
}

#[derive(Clone, Debug)]
pub struct Doc {
    nodes: Vec<Node>,
    root: NodeId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmlError(pub String);

impl std::fmt::Display for XmlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for XmlError {}

/// Limits for `parse_with`. A file past a limit is refused.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_bytes: usize,
    pub max_elements: usize,
    pub max_depth: usize,
    pub max_attr_len: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits { max_bytes: 8 << 20, max_elements: 400_000, max_depth: 64, max_attr_len: 1 << 16 }
    }
}

impl Doc {
    /// A document with one empty root element.
    pub fn new(root_tag: &str) -> Doc {
        Doc { nodes: vec![Node { tag: root_tag.to_string(), ..Node::default() }], root: NodeId(0) }
    }

    pub fn root(&self) -> NodeId {
        self.root
    }

    fn n(&self, id: NodeId) -> &Node {
        &self.nodes[id.0 as usize]
    }
    fn nm(&mut self, id: NodeId) -> &mut Node {
        &mut self.nodes[id.0 as usize]
    }

    /// A new element that has no parent yet.
    pub fn new_element(&mut self, tag: &str) -> NodeId {
        self.nodes.push(Node { tag: tag.to_string(), ..Node::default() });
        NodeId(self.nodes.len() as u32 - 1)
    }

    /// A new element with attributes, without a parent.
    pub fn make(&mut self, tag: &str, attrs: &[(&str, &str)]) -> NodeId {
        let id = self.new_element(tag);
        for (k, v) in attrs {
            self.set(id, k, v);
        }
        id
    }

    pub fn tag(&self, id: NodeId) -> &str {
        &self.n(id).tag
    }
    pub fn set_tag(&mut self, id: NodeId, tag: &str) {
        self.nm(id).tag = tag.to_string();
    }
    pub fn text(&self, id: NodeId) -> &str {
        &self.n(id).text
    }
    pub fn set_text(&mut self, id: NodeId, text: &str) {
        self.nm(id).text = text.to_string();
    }

    pub fn get(&self, id: NodeId, name: &str) -> Option<&str> {
        self.n(id).attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }
    /// The attribute, or "" when it's missing.
    pub fn attr(&self, id: NodeId, name: &str) -> &str {
        self.get(id, name).unwrap_or("")
    }
    pub fn has(&self, id: NodeId, name: &str) -> bool {
        self.get(id, name).is_some()
    }
    /// Sets an attribute. A new attribute goes last.
    pub fn set(&mut self, id: NodeId, name: &str, value: &str) {
        let attrs = &mut self.nm(id).attrs;
        match attrs.iter_mut().find(|(k, _)| k == name) {
            Some(a) => a.1 = value.to_string(),
            None => attrs.push((name.to_string(), value.to_string())),
        }
    }
    pub fn remove_attr(&mut self, id: NodeId, name: &str) {
        self.nm(id).attrs.retain(|(k, _)| k != name);
    }
    pub fn attrs(&self, id: NodeId) -> &[(String, String)] {
        &self.n(id).attrs
    }
    pub fn clear_attrs(&mut self, id: NodeId) {
        self.nm(id).attrs.clear();
    }

    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.n(id).parent
    }
    pub fn children(&self, id: NodeId) -> &[NodeId] {
        &self.n(id).children
    }
    /// The children with this tag.
    pub fn kids(&self, id: NodeId, tag: &str) -> Vec<NodeId> {
        self.n(id).children.iter().copied().filter(|c| self.n(*c).tag == tag).collect()
    }
    /// The first child with this tag.
    pub fn first(&self, id: NodeId, tag: &str) -> Option<NodeId> {
        self.n(id).children.iter().copied().find(|c| self.n(*c).tag == tag)
    }
    pub fn len(&self, id: NodeId) -> usize {
        self.n(id).children.len()
    }
    /// The node's place among its parent's children.
    pub fn index_of(&self, id: NodeId) -> Option<usize> {
        let p = self.n(id).parent?;
        self.n(p).children.iter().position(|c| *c == id)
    }
    pub fn next(&self, id: NodeId) -> Option<NodeId> {
        let p = self.n(id).parent?;
        let i = self.index_of(id)?;
        self.n(p).children.get(i + 1).copied()
    }
    pub fn prev(&self, id: NodeId) -> Option<NodeId> {
        let p = self.n(id).parent?;
        let i = self.index_of(id)?;
        if i == 0 {
            None
        } else {
            Some(self.n(p).children[i - 1])
        }
    }
    /// True while the node is the root or has the root as an ancestor.
    pub fn attached(&self, id: NodeId) -> bool {
        let mut n = id;
        loop {
            if n == self.root {
                return true;
            }
            match self.n(n).parent {
                Some(p) => n = p,
                None => return false,
            }
        }
    }
    /// The nearest of the node and its ancestors with this tag.
    pub fn closest(&self, id: NodeId, tag: &str) -> Option<NodeId> {
        let mut n = Some(id);
        while let Some(i) = n {
            if self.n(i).tag == tag {
                return Some(i);
            }
            n = self.n(i).parent;
        }
        None
    }

    /// Takes the node out of its parent. Its own children stay with it.
    pub fn detach(&mut self, id: NodeId) {
        if let Some(p) = self.n(id).parent {
            self.nm(p).children.retain(|c| *c != id);
            self.nm(id).parent = None;
        }
    }
    /// Makes the node the last child of `parent`. A node that has a parent is moved.
    pub fn append(&mut self, parent: NodeId, id: NodeId) {
        self.detach(id);
        self.nm(parent).children.push(id);
        self.nm(id).parent = Some(parent);
    }
    /// Makes the node child number `index` of `parent` (the end, when `index` is past it).
    pub fn insert(&mut self, parent: NodeId, index: usize, id: NodeId) {
        self.detach(id);
        let ch = &mut self.nm(parent).children;
        let i = index.min(ch.len());
        ch.insert(i, id);
        self.nm(id).parent = Some(parent);
    }
    /// Puts `id` right after `reference`, under the same parent.
    pub fn add_next(&mut self, reference: NodeId, id: NodeId) {
        self.detach(id);
        if let (Some(p), Some(i)) = (self.n(reference).parent, self.index_of(reference)) {
            self.insert(p, i + 1, id);
        }
    }
    /// Puts `id` right before `reference`, under the same parent.
    pub fn add_prev(&mut self, reference: NodeId, id: NodeId) {
        self.detach(id);
        if let (Some(p), Some(i)) = (self.n(reference).parent, self.index_of(reference)) {
            self.insert(p, i, id);
        }
    }

    /// The node and everything under it, in document order.
    pub fn iter(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack = vec![id];
        while let Some(n) = stack.pop() {
            out.push(n);
            stack.extend(self.n(n).children.iter().rev());
        }
        out
    }
    /// Like `iter`, with only the nodes that have one of these tags.
    pub fn iter_tags(&self, id: NodeId, tags: &[&str]) -> Vec<NodeId> {
        self.iter(id).into_iter().filter(|n| tags.contains(&self.n(*n).tag.as_str())).collect()
    }

    /// A copy of the node and everything under it, without a parent.
    pub fn deep_copy(&mut self, id: NodeId) -> NodeId {
        let src = self.n(id).clone();
        let copy = self.new_element(&src.tag);
        self.nm(copy).attrs = src.attrs;
        self.nm(copy).text = src.text;
        for c in src.children {
            let cc = self.deep_copy(c);
            self.append(copy, cc);
        }
        copy
    }
    /// Copies a node of another document into this one, without a parent.
    pub fn import(&mut self, other: &Doc, id: NodeId) -> NodeId {
        let copy = self.new_element(other.tag(id));
        self.nm(copy).attrs = other.n(id).attrs.clone();
        self.nm(copy).text = other.n(id).text.clone();
        for c in other.children(id).to_vec() {
            let cc = self.import(other, c);
            self.append(copy, cc);
        }
        copy
    }

    /// The child indices that lead from the root to the node.
    pub fn path_of(&self, id: NodeId) -> Vec<usize> {
        let mut p = Vec::new();
        let mut n = id;
        while let Some(i) = self.index_of(n) {
            p.insert(0, i);
            n = self.n(n).parent.unwrap();
        }
        p
    }
    pub fn node_at(&self, path: &[usize]) -> Option<NodeId> {
        let mut n = self.root;
        for i in path {
            n = *self.n(n).children.get(*i)?;
        }
        Some(n)
    }

    /// A new document with only what the root still reaches. Node ids change.
    pub fn compact(&self) -> Doc {
        let mut d = Doc::new(self.tag(self.root));
        d.nodes[0].attrs = self.n(self.root).attrs.clone();
        d.nodes[0].text = self.n(self.root).text.clone();
        for c in self.children(self.root).to_vec() {
            let cc = d.import(self, c);
            d.append(d.root, cc);
        }
        d
    }

    // -- Reading

    pub fn parse(xml: &str) -> Result<Doc, XmlError> {
        Doc::parse_with(xml, Limits::default())
    }

    pub fn parse_bytes(xml: &[u8]) -> Result<Doc, XmlError> {
        let s = std::str::from_utf8(xml).map_err(|_| XmlError("the XML isn't valid UTF-8".into()))?;
        Doc::parse(s)
    }

    pub fn parse_with(xml: &str, lim: Limits) -> Result<Doc, XmlError> {
        if xml.len() > lim.max_bytes {
            return Err(XmlError(format!("the XML is larger than {} MB", lim.max_bytes >> 20)));
        }
        Parser { s: xml.as_bytes(), src: xml, i: 0, lim, count: 0, pieces: None }.document().map(|(doc, _)| doc)
    }

    /// Like `parse_with`, and also gives what each element holds, in order: every run of text (also text made
    /// of whitespace only, and text between child elements), every child, and a mark for every comment or
    /// processing instruction. The list is indexed by `NodeId.0`. The strict check of imported files reports
    /// stray text and dropped comments from it.
    pub fn parse_pieces(xml: &str, lim: Limits) -> Result<(Doc, Vec<Vec<Piece>>), XmlError> {
        if xml.len() > lim.max_bytes {
            return Err(XmlError(format!("the XML is larger than {} MB", lim.max_bytes >> 20)));
        }
        let p = Parser { s: xml.as_bytes(), src: xml, i: 0, lim, count: 0, pieces: Some(Vec::new()) };
        p.document().map(|(doc, pieces)| (doc, pieces.unwrap_or_default()))
    }

    // -- Writing

    /// The tree on one line, as lxml's `ET.tostring(root, encoding="unicode")` writes it.
    pub fn serialize(&self) -> String {
        let mut out = String::new();
        self.write(self.root, &mut out, None, 0);
        out
    }
    /// One subtree on one line.
    pub fn serialize_node(&self, id: NodeId) -> String {
        let mut out = String::new();
        self.write(id, &mut out, None, 0);
        out
    }
    /// The tree with an XML declaration, indented by two spaces, with a newline at the end.
    pub fn pretty(&self) -> String {
        let mut out = String::from("<?xml version='1.0' encoding='utf-8'?>\n");
        self.write(self.root, &mut out, Some("  "), 0);
        out.push('\n');
        out
    }
    /// Like `pretty` without the newline at the end: libgarib's level-tool output.
    pub fn pretty_no_newline(&self) -> String {
        let mut out = String::from("<?xml version='1.0' encoding='utf-8'?>\n");
        self.write(self.root, &mut out, Some("  "), 0);
        out
    }

    fn write(&self, id: NodeId, out: &mut String, indent: Option<&str>, depth: usize) {
        let n = self.n(id);
        out.push('<');
        out.push_str(&n.tag);
        for (k, v) in &n.attrs {
            let _ = write!(out, " {}=\"", k);
            escape(v, true, out);
            out.push('"');
        }
        if n.children.is_empty() && n.text.is_empty() {
            out.push_str("/>");
            return;
        }
        out.push('>');
        escape(&n.text, false, out);
        for c in &n.children {
            if let Some(sp) = indent {
                out.push('\n');
                for _ in 0..=depth {
                    out.push_str(sp);
                }
            }
            self.write(*c, out, indent, depth + 1);
        }
        if let (Some(sp), false) = (indent, n.children.is_empty()) {
            out.push('\n');
            for _ in 0..depth {
                out.push_str(sp);
            }
        }
        out.push_str("</");
        out.push_str(&n.tag);
        out.push('>');
    }
}

fn escape(s: &str, attr: bool, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if attr => out.push_str("&quot;"),
            '\n' if attr => out.push_str("&#10;"),
            '\t' if attr => out.push_str("&#9;"),
            '\r' => out.push_str("&#13;"),
            c => out.push(c),
        }
    }
}

/// One thing an element holds, as `Doc::parse_pieces` reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Piece {
    /// A run of text between two pieces of markup, with its entities resolved.
    Text(String),
    Child(NodeId),
    /// A comment or a processing instruction.
    Dropped,
}

struct Parser<'a> {
    s: &'a [u8],
    src: &'a str,
    i: usize,
    lim: Limits,
    count: usize,
    /// What each element holds, when the caller asked for it.
    pieces: Option<Vec<Vec<Piece>>>,
}

impl<'a> Parser<'a> {
    fn err<T>(&self, msg: &str) -> Result<T, XmlError> {
        let line = self.s[..self.i.min(self.s.len())].iter().filter(|b| **b == b'\n').count() + 1;
        Err(XmlError(format!("{} (line {})", msg, line)))
    }
    fn starts(&self, p: &str) -> bool {
        self.s[self.i..].starts_with(p.as_bytes())
    }
    fn skip_ws(&mut self) {
        while self.i < self.s.len() && matches!(self.s[self.i], b' ' | b'\t' | b'\r' | b'\n') {
            self.i += 1;
        }
    }
    fn skip_past(&mut self, end: &str) -> Result<(), XmlError> {
        match self.src[self.i..].find(end) {
            Some(p) => {
                self.i += p + end.len();
                Ok(())
            }
            None => self.err("the XML ends inside a comment or an instruction"),
        }
    }
    /// Skips a comment. XML doesn't allow "--" inside one, and lxml refuses it.
    fn skip_comment(&mut self) -> Result<(), XmlError> {
        self.i += 4;
        match self.src[self.i..].find("--") {
            Some(p) if self.src[self.i + p..].starts_with("-->") => {
                self.i += p + 3;
                Ok(())
            }
            Some(_) => self.err("a comment may not contain --"),
            None => self.err("the XML ends inside a comment or an instruction"),
        }
    }

    /// Skips whitespace, comments and processing instructions between elements.
    fn skip_misc(&mut self) -> Result<(), XmlError> {
        loop {
            self.skip_ws();
            if self.starts("<!--") {
                self.skip_comment()?;
            } else if self.starts("<?") {
                self.skip_past("?>")?;
            } else {
                return Ok(());
            }
        }
    }

    fn document(mut self) -> Result<(Doc, Option<Vec<Vec<Piece>>>), XmlError> {
        if self.starts("\u{feff}") {
            self.i += 3;
        }
        self.skip_misc()?;
        if self.starts("<!") {
            return self.err("the XML may not contain a DOCTYPE or declarations");
        }
        let mut doc = Doc { nodes: Vec::new(), root: NodeId(0) };
        self.element(&mut doc, None, 0)?;
        self.skip_misc()?;
        if self.i < self.s.len() {
            return self.err("there's content after the root element");
        }
        Ok((doc, self.pieces))
    }

    fn name(&mut self) -> Result<&'a str, XmlError> {
        let start = self.i;
        while self.i < self.s.len() {
            let b = self.s[self.i];
            if b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b':') || b >= 0x80 {
                self.i += 1;
            } else {
                break;
            }
        }
        if self.i == start || self.s[start].is_ascii_digit() || matches!(self.s[start], b'-' | b'.') {
            return self.err("a name is expected");
        }
        Ok(&self.src[start..self.i])
    }

    fn element(&mut self, doc: &mut Doc, parent: Option<NodeId>, depth: usize) -> Result<NodeId, XmlError> {
        if self.i >= self.s.len() || self.s[self.i] != b'<' {
            return self.err("an element is expected");
        }
        if depth > self.lim.max_depth {
            return self.err("the XML is nested too deeply");
        }
        self.count += 1;
        if self.count > self.lim.max_elements {
            return self.err("the XML has too many elements");
        }
        self.i += 1;
        let tag = self.name()?;
        doc.nodes.push(Node { tag: tag.to_string(), parent, ..Node::default() });
        let id = NodeId(doc.nodes.len() as u32 - 1);
        if let Some(p) = parent {
            doc.nodes[p.0 as usize].children.push(id);
        }
        if let Some(pieces) = self.pieces.as_mut() {
            pieces.push(Vec::new());
            if let Some(p) = parent {
                pieces[p.0 as usize].push(Piece::Child(id));
            }
        }
        loop {
            let before = self.i;
            self.skip_ws();
            if self.starts("/>") {
                self.i += 2;
                return Ok(id);
            }
            if self.starts(">") {
                self.i += 1;
                break;
            }
            if self.i >= self.s.len() {
                return self.err("the XML ends inside a tag");
            }
            if before == self.i {
                return self.err("a space is expected between attributes");
            }
            let key = self.name()?;
            self.skip_ws();
            if !self.starts("=") {
                return self.err("an attribute needs a value");
            }
            self.i += 1;
            self.skip_ws();
            let q = match self.s.get(self.i) {
                Some(b'"') => b'"',
                Some(b'\'') => b'\'',
                _ => return self.err("an attribute value needs quotes"),
            };
            self.i += 1;
            let start = self.i;
            while self.i < self.s.len() && self.s[self.i] != q {
                if self.s[self.i] == b'<' {
                    return self.err("an attribute value may not contain <");
                }
                self.i += 1;
            }
            if self.i >= self.s.len() {
                return self.err("the XML ends inside an attribute");
            }
            if self.i - start > self.lim.max_attr_len {
                return self.err("an attribute is too long");
            }
            let raw = &self.src[start..self.i];
            self.i += 1;
            let value = self.unescape(raw, true)?;
            let node = &mut doc.nodes[id.0 as usize];
            if node.attrs.iter().any(|(k, _)| k == key) {
                return self.err("an attribute is given twice");
            }
            node.attrs.push((key.to_string(), value));
        }
        // Content
        let mut text = String::new();
        loop {
            let start = self.i;
            while self.i < self.s.len() && self.s[self.i] != b'<' {
                self.i += 1;
            }
            if self.i > start {
                let t = self.unescape(&self.src[start..self.i], false)?;
                text.push_str(&t);
                if let Some(pieces) = self.pieces.as_mut() {
                    pieces[id.0 as usize].push(Piece::Text(t));
                }
            }
            if self.i >= self.s.len() {
                return self.err("the XML ends inside an element");
            }
            if self.starts("<!--") || self.starts("<?") {
                if self.starts("<?") {
                    self.skip_past("?>")?;
                } else {
                    self.skip_comment()?;
                }
                if let Some(pieces) = self.pieces.as_mut() {
                    pieces[id.0 as usize].push(Piece::Dropped);
                }
            } else if self.starts("<![CDATA[") {
                return self.err("the XML may not contain CDATA");
            } else if self.starts("<!") {
                return self.err("the XML may not contain declarations");
            } else if self.starts("</") {
                self.i += 2;
                let close = self.name()?;
                if close != tag {
                    return self.err(&format!("</{}> closes <{}>", close, tag));
                }
                self.skip_ws();
                if !self.starts(">") {
                    return self.err("a closing tag isn't finished");
                }
                self.i += 1;
                break;
            } else {
                self.element(doc, Some(id), depth + 1)?;
            }
        }
        // Text is kept only on an element without children: lxml's text and tail between elements are whitespace
        // in every level file, and the editor drops whitespace-only text.
        if doc.nodes[id.0 as usize].children.is_empty() && !text.trim().is_empty() {
            doc.nodes[id.0 as usize].text = text;
        }
        Ok(id)
    }

    fn unescape(&self, raw: &str, attr: bool) -> Result<String, XmlError> {
        if !raw.contains('&') && !(attr && raw.contains(['\n', '\t', '\r'])) {
            return Ok(raw.to_string());
        }
        let mut out = String::with_capacity(raw.len());
        let mut rest = raw;
        while let Some(c) = rest.chars().next() {
            if c == '&' {
                let end = match rest.find(';') {
                    Some(e) => e,
                    None => return self.err("an entity isn't finished"),
                };
                let ent = &rest[1..end];
                let ch = match ent {
                    "amp" => '&',
                    "lt" => '<',
                    "gt" => '>',
                    "quot" => '"',
                    "apos" => '\'',
                    _ => {
                        let code = if let Some(h) = ent.strip_prefix("#x") {
                            u32::from_str_radix(h, 16).ok()
                        } else if let Some(d) = ent.strip_prefix('#') {
                            d.parse::<u32>().ok()
                        } else {
                            return self.err("the XML may not use entities of its own");
                        };
                        match code.and_then(char::from_u32) {
                            Some(c) if c == '\t' || c == '\n' || c == '\r' || c >= ' ' => c,
                            _ => return self.err("a character reference isn't valid"),
                        }
                    }
                };
                out.push(ch);
                rest = &rest[end + 1..];
            } else {
                // An XML reader turns a literal tab or line break inside an attribute into a space.
                out.push(if attr && matches!(c, '\n' | '\t' | '\r') { ' ' } else { c });
                rest = &rest[c.len_utf8()..];
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let src = "<?xml version='1.0' encoding='utf-8'?>\n<Level name=\"A &amp; B\">\n  <!-- c -->\n  <Platform object_id=\"0x1\">\n    <PlatNoClip/>\n  </Platform>\n  <Garib x=\"1.0\"/>\n</Level>\n";
        let d = Doc::parse(src).unwrap();
        assert_eq!(d.serialize(), "<Level name=\"A &amp; B\"><Platform object_id=\"0x1\"><PlatNoClip/></Platform><Garib x=\"1.0\"/></Level>");
        assert_eq!(d.pretty(), src.replace("  <!-- c -->\n", ""));
    }

    #[test]
    fn refuses_doctype_and_entities() {
        assert!(Doc::parse("<!DOCTYPE a [<!ENTITY x \"y\">]><a/>").is_err());
        assert!(Doc::parse("<a b=\"&x;\"/>").is_err());
        assert!(Doc::parse("<a><![CDATA[x]]></a>").is_err());
        assert!(Doc::parse("<a></b>").is_err());
        assert!(Doc::parse("<a/><b/>").is_err());
        assert!(Doc::parse("<a><!-- b -- c --></a>").is_err());
        assert!(Doc::parse("<!-- b ---><a/>").is_err());
        assert!(Doc::parse("<!----><a><!-- - --></a>").is_ok());
    }

    #[test]
    fn pieces() {
        let (d, p) = Doc::parse_pieces("<a> x<!-- c --><b>&amp;</b><?pi?>y </a>", Limits::default()).unwrap();
        let b = d.children(d.root())[0];
        assert_eq!(
            p[0],
            vec![Piece::Text(" x".into()), Piece::Dropped, Piece::Child(b), Piece::Dropped, Piece::Text("y ".into())]
        );
        assert_eq!(p[b.0 as usize], vec![Piece::Text("&".into())]);
    }

    #[test]
    fn editing() {
        let mut d = Doc::parse("<L><A/><B/><C/></L>").unwrap();
        let r = d.root();
        let b = d.children(r)[1];
        let x = d.make("X", &[("k", "v\"\n")]);
        d.add_next(b, x);
        assert_eq!(d.serialize(), "<L><A/><B/><X k=\"v&quot;&#10;\"/><C/></L>");
        d.detach(b);
        assert!(!d.attached(b));
        assert_eq!(d.path_of(x), vec![1]);
        let c = d.deep_copy(x);
        d.insert(r, 0, c);
        assert_eq!(d.compact().serialize(), "<L><X k=\"v&quot;&#10;\"/><A/><X k=\"v&quot;&#10;\"/><C/></L>");
    }
}
