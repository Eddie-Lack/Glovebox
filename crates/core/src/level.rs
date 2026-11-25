//! Glover's level file (`.lev`) to and from the level XML.
//!
//! The XML is the one libgarib's level-tool writes and reads. `disassemble` and `assemble` give the same bytes as
//! libgarib's `landscapeToXML` and `xmlToLandscape`, including their quirks. `validate` replaces libgarib's RelaxNG
//! schema.
//!
//! The format is data: `data/level_format.json`, made by `tools/gen_formats.py` from `formats/glover.lev.ksy`.
//! `format()` gives it to other code.
//!
//! A level file is a 4-byte length, a NUL-terminated name, and commands to the end of the file. A command is a
//! 16-bit code and the fields of that code's type. All numbers are big-endian. The file is padded with zeros to a
//! multiple of 8 bytes.

use crate::pyfmt;
use crate::xml::{Doc, NodeId};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Level XML larger than this is refused.
pub const MAX_XML: usize = 8 << 20;
/// `validate` stops after this many messages.
pub const MAX_MESSAGES: usize = 20;
/// Objects whose SetActorRotation and SetActorScale only the game mod applies.
pub const MOD_PARENTS: [&str; 5] = ["Enemy", "Platform", "NullPlatform", "Powerup", "Water"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LevelError(pub String);

impl std::fmt::Display for LevelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for LevelError {}

fn err<T>(msg: impl Into<String>) -> Result<T, LevelError> {
    Err(LevelError(msg.into()))
}

// ---------------------------------------------------------------------------------------------------------------
// The format

/// What a field holds in the file.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldKind {
    U1,
    U2,
    U4,
    S1,
    S2,
    S4,
    F4,
    /// ASCII text in a fixed number of bytes, padded with NULs.
    Str(usize),
    /// Every byte to the end of the file. Only the type for an unknown command has one.
    BytesEos,
    /// Another type, by its index in `Format::types`.
    Struct(usize),
    /// One of several types, chosen by an earlier field.
    Switch(Switch),
}

impl FieldKind {
    /// Bytes of a number field.
    pub fn int_size(&self) -> Option<usize> {
        match self {
            FieldKind::U1 | FieldKind::S1 => Some(1),
            FieldKind::U2 | FieldKind::S2 => Some(2),
            FieldKind::U4 | FieldKind::S4 => Some(4),
            _ => None,
        }
    }
    fn signed(&self) -> bool {
        matches!(self, FieldKind::S1 | FieldKind::S2 | FieldKind::S4)
    }
    /// True when the field is an attribute in the XML. Struct and switch fields are child elements.
    pub fn is_attribute(&self) -> bool {
        !matches!(self, FieldKind::Struct(_) | FieldKind::Switch(_))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Switch {
    /// The id of the field that chooses the type.
    pub on: String,
    /// That field's index in the type's fields.
    pub on_index: usize,
    /// Code and type index, in the format's order. A type can have several codes.
    pub cases: Vec<(u32, usize)>,
    /// The type for every other code.
    pub default: Option<usize>,
}

impl Switch {
    pub fn type_for(&self, code: i128) -> Option<usize> {
        self.cases.iter().find(|(c, _)| *c as i128 == code).map(|(_, t)| *t).or(self.default)
    }
}

#[derive(Debug, Clone)]
pub struct Field {
    pub id: String,
    pub kind: FieldKind,
    /// Index in `Format::enums` when the number has names.
    pub enum_index: Option<usize>,
    /// "object" or "texture" when the field is the hashed name of a model or a texture.
    pub hash_namespace: Option<String>,
    pub tag_namespace: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EnumDef {
    /// "type::enum", for example "puzzle_action::camera_flags".
    pub name: String,
    /// Name and value, in the format's order.
    pub members: Vec<(String, i64)>,
}

impl EnumDef {
    pub fn value(&self, name: &str) -> Option<i64> {
        self.members.iter().find(|(n, _)| n == name).map(|(_, v)| *v)
    }
    pub fn name_of(&self, value: i64) -> Option<&str> {
        self.members.iter().find(|(_, v)| *v == value).map(|(n, _)| n.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct TypeDef {
    /// The name in the format, for example "plat_set_initial_pos".
    pub id: String,
    /// The element name, for example "PlatSetInitialPos".
    pub tag: String,
    pub fields: Vec<Field>,
    /// The kind of object this command starts ("PLATFORM"). Commands that modify that kind may sit inside it.
    pub declares: Option<String>,
    /// The kinds of object this command may sit inside.
    pub modifies: Vec<String>,
    /// The element that collects runs of this command ("NormalInstructions"). Such a type has one struct field,
    /// and the XML holds that struct's element in place of the command's own.
    pub group_tag: Option<String>,
    /// The command's code when it's a top-level command.
    pub opcode: Option<u32>,
    // The nesting rules libgarib's reader was made with. They differ from `declares` and `modifies`: see
    // tools/gen_formats.py.
    disasm_declares: Option<String>,
    disasm_modifies: Vec<String>,
    /// Element names that may sit inside this type's element after its switch child. Empty without `declares`.
    children: Vec<String>,
}

impl TypeDef {
    pub fn field(&self, id: &str) -> Option<&Field> {
        self.fields.iter().find(|f| f.id == id)
    }
    /// The switch field, when the type has one.
    pub fn switch(&self) -> Option<(&Field, &Switch)> {
        self.fields.iter().find_map(|f| match &f.kind {
            FieldKind::Switch(s) => Some((f, s)),
            _ => None,
        })
    }
    /// The ids of the fields that are attributes of the element, in order.
    pub fn attributes(&self) -> impl Iterator<Item = &Field> {
        self.fields.iter().filter(|f| f.kind.is_attribute())
    }
}

/// The level format.
#[derive(Debug)]
pub struct Format {
    /// What libgarib writes as `libgarib-version` on `<Level>`.
    pub libgarib_version: String,
    /// What libgarib writes as `data-format-version` on `<Level>`.
    pub data_format_version: String,
    /// Integers above this are written in hex.
    pub hex_cutoff: i64,
    pub types: Vec<TypeDef>,
    pub enums: Vec<EnumDef>,
    /// Code and type index of every top-level command, in the format's order.
    pub opcodes: Vec<(u32, usize)>,
    by_id: HashMap<String, usize>,
    by_tag: HashMap<String, usize>,
    /// Group element name and the type index of the command it collects.
    groups: Vec<(String, usize)>,
    top_level: Vec<String>,
    cmd: usize,
}

impl Format {
    pub fn type_by_id(&self, id: &str) -> Option<&TypeDef> {
        self.by_id.get(id).map(|i| &self.types[*i])
    }
    /// The type an element name stands for. A group element has no type of its own: see `group`.
    pub fn type_by_tag(&self, tag: &str) -> Option<&TypeDef> {
        self.by_tag.get(tag).map(|i| &self.types[*i])
    }
    pub fn enum_by_name(&self, name: &str) -> Option<&EnumDef> {
        self.enums.iter().find(|e| e.name == name)
    }
    /// The names and values of a field's numbers, when it has them.
    pub fn field_enum(&self, field: &Field) -> Option<&EnumDef> {
        field.enum_index.map(|i| &self.enums[i])
    }
    /// The element names that may sit directly in `<Level>`: top-level commands and group elements.
    pub fn top_level_tags(&self) -> &[String] {
        &self.top_level
    }
    /// The element names of the group elements ("NormalInstructions", ...).
    pub fn group_tags(&self) -> impl Iterator<Item = &str> {
        self.groups.iter().map(|(t, _)| t.as_str())
    }
    /// For a group element: the command it collects, and the type of the elements inside it.
    pub fn group(&self, tag: &str) -> Option<(&TypeDef, &TypeDef)> {
        let (_, ti) = self.groups.iter().find(|(t, _)| t == tag)?;
        let wrapper = &self.types[*ti];
        match wrapper.fields.first().map(|f| &f.kind) {
            Some(FieldKind::Struct(inner)) => Some((wrapper, &self.types[*inner])),
            _ => None,
        }
    }
    /// The element names that may sit inside an element after its switch child, in any order and number.
    /// For `Level` these are the top-level tags. For a group element it's the one element it collects.
    pub fn valid_children(&self, tag: &str) -> Vec<&str> {
        if tag == "Level" {
            return self.top_level.iter().map(|s| s.as_str()).collect();
        }
        if let Some((_, inner)) = self.group(tag) {
            return vec![inner.tag.as_str()];
        }
        match self.type_by_tag(tag) {
            Some(t) => t.children.iter().map(|s| s.as_str()).collect(),
            None => Vec::new(),
        }
    }
    /// The element names a type's switch field can be: one child of these comes first in the element.
    pub fn switch_child_tags(&self, tag: &str) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        if let Some((_, sw)) = self.type_by_tag(tag).and_then(|t| t.switch()) {
            for ti in sw.cases.iter().map(|(_, t)| *t).chain(sw.default) {
                let name = self.element_name(ti);
                if !out.contains(&name) {
                    out.push(name);
                }
            }
        }
        out
    }
    /// The top-level command's code for an element name.
    pub fn opcode(&self, tag: &str) -> Option<u32> {
        self.type_by_tag(tag).and_then(|t| t.opcode)
    }
    /// The element the schema uses for a type: the group element for a grouped command.
    fn element_name(&self, ti: usize) -> &str {
        let t = &self.types[ti];
        t.group_tag.as_deref().unwrap_or(&t.tag)
    }
}

#[derive(Deserialize)]
struct RawFormat {
    libgarib_version: String,
    data_format_version: String,
    hex_cutoff: i64,
    opcodes: Vec<(u32, String)>,
    enums: serde_json::Map<String, serde_json::Value>,
    types: Vec<RawType>,
}

#[derive(Deserialize)]
struct RawType {
    id: String,
    tag: String,
    fields: Vec<RawField>,
    declares: Option<String>,
    modifies: Vec<String>,
    group_tag: Option<String>,
    disasm_declares: Option<String>,
    disasm_modifies: Vec<String>,
}

#[derive(Deserialize)]
struct RawField {
    id: String,
    #[serde(rename = "type")]
    ty: String,
    size: Option<usize>,
    #[serde(rename = "enum")]
    en: Option<String>,
    on: Option<String>,
    cases: Option<Vec<(u32, String)>>,
    default: Option<String>,
    #[serde(rename = "struct")]
    st: Option<String>,
    hash_namespace: Option<String>,
    tag_namespace: Option<String>,
}

fn load(text: &str) -> Result<Format, String> {
    let raw: RawFormat = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let by_id: HashMap<String, usize> = raw.types.iter().enumerate().map(|(i, t)| (t.id.clone(), i)).collect();
    let by_tag: HashMap<String, usize> = raw.types.iter().enumerate().map(|(i, t)| (t.tag.clone(), i)).collect();
    let type_index = |id: &str| by_id.get(id).copied().ok_or_else(|| format!("unknown type {}", id));
    let mut enums = Vec::new();
    for (name, members) in &raw.enums {
        let members: Vec<(String, i64)> = serde_json::from_value(members.clone()).map_err(|e| e.to_string())?;
        enums.push(EnumDef { name: name.clone(), members });
    }
    let mut types = Vec::new();
    for t in &raw.types {
        let mut fields: Vec<Field> = Vec::new();
        for f in &t.fields {
            let kind = match f.ty.as_str() {
                "u1" => FieldKind::U1,
                "u2" => FieldKind::U2,
                "u4" => FieldKind::U4,
                "s1" => FieldKind::S1,
                "s2" => FieldKind::S2,
                "s4" => FieldKind::S4,
                "f4" => FieldKind::F4,
                "str" => FieldKind::Str(f.size.ok_or("a string without a size")?),
                "bytes_eos" => FieldKind::BytesEos,
                "struct" => FieldKind::Struct(type_index(f.st.as_deref().ok_or("a struct without a type")?)?),
                "switch" => {
                    let on = f.on.clone().ok_or("a switch without a field")?;
                    let on_index = fields.iter().position(|x| x.id == on).ok_or("a switch before its field")?;
                    let mut cases = Vec::new();
                    for (code, id) in f.cases.as_ref().ok_or("a switch without cases")? {
                        cases.push((*code, type_index(id)?));
                    }
                    let default = match &f.default {
                        Some(id) => Some(type_index(id)?),
                        None => None,
                    };
                    FieldKind::Switch(Switch { on, on_index, cases, default })
                }
                other => return Err(format!("unknown field type {}", other)),
            };
            let enum_index = match &f.en {
                Some(name) => Some(enums.iter().position(|e| &e.name == name).ok_or("unknown enum")?),
                None => None,
            };
            fields.push(Field {
                id: f.id.clone(),
                kind,
                enum_index,
                hash_namespace: f.hash_namespace.clone(),
                tag_namespace: f.tag_namespace.clone(),
            });
        }
        types.push(TypeDef {
            id: t.id.clone(),
            tag: t.tag.clone(),
            fields,
            declares: t.declares.clone(),
            modifies: t.modifies.clone(),
            group_tag: t.group_tag.clone(),
            opcode: None,
            disasm_declares: t.disasm_declares.clone(),
            disasm_modifies: t.disasm_modifies.clone(),
            children: Vec::new(),
        });
    }
    let mut opcodes = Vec::new();
    for (code, id) in &raw.opcodes {
        let ti = type_index(id)?;
        types[ti].opcode = Some(*code);
        opcodes.push((*code, ti));
    }
    let cmd = type_index("cmd")?;
    let cmd_default = match types[cmd].switch() {
        Some((_, sw)) => sw.default,
        None => return Err("cmd has no switch".into()),
    };
    // What may sit inside what, as libgarib's schema generator works it out: a command that modifies a kind of
    // object may sit in every command that declares that kind. A grouped command appears as its group element.
    for ci in 0..types.len() {
        let name = types[ci].group_tag.clone().unwrap_or_else(|| types[ci].tag.clone());
        for kind in types[ci].modifies.clone() {
            for pi in 0..types.len() {
                if types[pi].declares.as_deref() == Some(kind.as_str()) && !types[pi].children.contains(&name) {
                    types[pi].children.push(name.clone());
                }
            }
        }
    }
    let mut groups = Vec::new();
    let mut top_level = Vec::new();
    for (i, t) in types.iter().enumerate() {
        if let Some(g) = &t.group_tag {
            groups.push((g.clone(), i));
            top_level.push(g.clone());
        } else if i != cmd && (t.opcode.is_some() || cmd_default == Some(i)) {
            top_level.push(t.tag.clone());
        }
    }
    Ok(Format {
        libgarib_version: raw.libgarib_version,
        data_format_version: raw.data_format_version,
        hex_cutoff: raw.hex_cutoff,
        types,
        enums,
        opcodes,
        by_id,
        by_tag,
        groups,
        top_level,
        cmd,
    })
}

/// The level format that is compiled into the program.
pub fn format() -> &'static Format {
    static F: OnceLock<Format> = OnceLock::new();
    F.get_or_init(|| load(include_str!("../data/level_format.json")).expect("the compiled-in level format is valid"))
}

// ---------------------------------------------------------------------------------------------------------------
// .lev to XML

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], LevelError> {
        let end = self.pos.checked_add(n).filter(|e| *e <= self.data.len());
        match end {
            Some(end) => {
                let s = &self.data[self.pos..end];
                self.pos = end;
                Ok(s)
            }
            None => err(format!("the level file ends inside a command (at byte {})", self.pos)),
        }
    }
    fn uint(&mut self, n: usize) -> Result<u32, LevelError> {
        Ok(self.take(n)?.iter().fold(0u32, |a, b| (a << 8) | *b as u32))
    }
}

/// One struct read from the file: its type and one value per field.
struct Obj {
    ty: usize,
    vals: Vec<Val>,
}

enum Val {
    Attr(String),
    Child(Obj),
}

/// The text lxml takes for an attribute: ASCII without control characters other than tab, newline and return.
fn xml_text(bytes: &[u8], what: &str) -> Result<String, LevelError> {
    for b in bytes {
        if *b >= 0x80 {
            return err(format!("{} is not ASCII", what));
        }
        if *b < 0x20 && !matches!(*b, 9 | 10 | 13) {
            return err(format!("{} holds a control character", what));
        }
    }
    Ok(bytes.iter().map(|b| *b as char).collect())
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

fn read_obj(f: &Format, r: &mut Reader, ty: usize, depth: usize) -> Result<Obj, LevelError> {
    if depth > 16 {
        return err("the level format nests too deeply");
    }
    let t = &f.types[ty];
    let mut vals = Vec::with_capacity(t.fields.len());
    // The numbers read so far, for the switch
    let mut ints: Vec<i64> = Vec::with_capacity(t.fields.len());
    for field in &t.fields {
        let mut int = 0i64;
        let val = match &field.kind {
            FieldKind::F4 => {
                let b = r.take(4)?;
                Val::Attr(pyfmt::repr_f32(f32::from_be_bytes([b[0], b[1], b[2], b[3]])))
            }
            FieldKind::Str(n) => {
                let b = r.take(*n)?;
                let end = b.iter().rposition(|c| *c != 0).map_or(0, |p| p + 1);
                Val::Attr(xml_text(&b[..end], &format!("the text of {}.{}", t.tag, field.id))?)
            }
            FieldKind::BytesEos => {
                let rest = &r.data[r.pos..];
                r.pos = r.data.len();
                Val::Attr(base64(rest))
            }
            FieldKind::Struct(inner) => Val::Child(read_obj(f, r, *inner, depth + 1)?),
            FieldKind::Switch(sw) => {
                let code = ints.get(sw.on_index).copied().unwrap_or(0);
                match sw.type_for(code as i128) {
                    Some(inner) => Val::Child(read_obj(f, r, inner, depth + 1)?),
                    None => return err(format!("{}: no type for {} {}", t.tag, sw.on, code)),
                }
            }
            kind => {
                let size = kind.int_size().unwrap_or(1);
                let u = r.uint(size)?;
                int = if kind.signed() {
                    match size {
                        1 => u as u8 as i8 as i64,
                        2 => u as u16 as i16 as i64,
                        _ => u as i32 as i64,
                    }
                } else {
                    u as i64
                };
                let name = field.enum_index.and_then(|e| f.enums[e].name_of(int));
                Val::Attr(match name {
                    Some(name) => name.to_string(),
                    None if int > f.hex_cutoff => format!("0x{:0width$X}", int, width = size * 2),
                    None => int.to_string(),
                })
            }
        };
        ints.push(int);
        vals.push(val);
    }
    Ok(Obj { ty, vals })
}

/// Adds the element for a struct: its number and text fields as attributes, its struct fields as children.
fn add_element(f: &Format, doc: &mut Doc, parent: NodeId, obj: &Obj) -> NodeId {
    let t = &f.types[obj.ty];
    let node = doc.new_element(&t.tag);
    for (field, val) in t.fields.iter().zip(&obj.vals) {
        if let Val::Attr(s) = val {
            doc.set(node, &field.id, s);
        }
    }
    doc.append(parent, node);
    for val in &obj.vals {
        if let Val::Child(c) = val {
            add_element(f, doc, node, c);
        }
    }
    node
}

/// A level file as the XML tree that libgarib's level-tool writes for it. Empty commands at the end are left out.
pub fn disassemble(lev: &[u8]) -> Result<Doc, LevelError> {
    let f = format();
    let mut r = Reader { data: lev, pos: 0 };
    r.take(4)?; // The length. libgarib reads to the end of the file whatever it says.
    let name_len = match lev[r.pos..].iter().position(|b| *b == 0) {
        Some(n) => n,
        None => return err("the level's name has no end"),
    };
    let name = xml_text(r.take(name_len)?, "the level's name")?;
    r.take(1)?;

    let mut doc = Doc::new("Level");
    let root = doc.root();
    doc.set(root, "name", &name);
    doc.set(root, "libgarib-version", &f.libgarib_version);
    doc.set(root, "data-format-version", &f.data_format_version);

    // The last element of each kind of object, and the kind that takes modifying commands now
    let mut cursors: HashMap<&str, NodeId> = HashMap::new();
    let mut active: Option<&str> = None;
    while r.pos < lev.len() {
        let cmd = read_obj(f, &mut r, f.cmd, 0)?;
        let mut body = match cmd.vals.get(1) {
            Some(Val::Child(body)) => body,
            _ => return err("a command has no body"),
        };
        let t = &f.types[body.ty];
        let mut parent = root;
        if !t.disasm_modifies.is_empty() {
            match t.disasm_modifies.iter().find(|m| Some(m.as_str()) == active) {
                Some(kind) => parent = cursors.get(kind.as_str()).copied().unwrap_or(root),
                None => active = None,
            }
        }
        if let Some(group) = &t.group_tag {
            let last = doc.children(parent).last().copied();
            parent = match last {
                Some(last) if doc.tag(last) == group => last,
                _ => {
                    let g = doc.new_element(group);
                    doc.append(parent, g);
                    g
                }
            };
            body = match body.vals.first() {
                Some(Val::Child(inner)) => inner,
                _ => return err("a grouped command has no body"),
            };
        }
        let node = add_element(f, &mut doc, parent, body);
        if let Some(kind) = &t.disasm_declares {
            cursors.insert(kind.as_str(), node);
            active = Some(kind.as_str());
        }
    }
    loop {
        match doc.children(root).last().copied() {
            Some(last) if doc.tag(last) == "Noop" => doc.detach(last),
            Some(_) => break,
            // libgarib fails here too
            None => return err("the level file has no commands"),
        }
    }
    Ok(doc.compact())
}

/// Each command of a level file: its type code and the offset of its fields, which follow the 2 bytes of the code.
/// The commands are read as `disassemble` reads them, up to and including EndLevelData. `disassemble` reads on to
/// the end of the file and drops the Noop elements at the end.
pub fn commands(lev: &[u8]) -> Result<Vec<(u32, usize)>, LevelError> {
    let f = format();
    let mut r = Reader { data: lev, pos: 0 };
    r.take(4)?;
    let name_len = match lev[r.pos..].iter().position(|b| *b == 0) {
        Some(n) => n,
        None => return err("the level's name has no end"),
    };
    r.take(name_len + 1)?;
    let mut out = Vec::new();
    while r.pos < lev.len() {
        let start = r.pos;
        read_obj(f, &mut r, f.cmd, 0)?;
        let code = u16::from_be_bytes([lev[start], lev[start + 1]]) as u32;
        out.push((code, start + 2));
        // The game stops at EndLevelData. The bytes after it pad the file to a multiple of 8, and their count can be
        // odd, so they aren't read as commands.
        if Some(code) == f.opcode("EndLevelData") {
            break;
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------------------------
// XML to .lev

/// A value on its way into the file, as libgarib holds it between reading the XML and writing bytes.
#[derive(Debug, Clone)]
enum Value {
    Int(i128),
    Float(f64),
    Str(String),
    /// The fields of an element, by field id.
    Dict(Vec<(String, Value)>),
}

/// libgarib's `hash_str`: CRC-32 with polynomial 0x04C11DB7, most significant bit first, starting from 0.
/// None when the text has a character above U+00FF, where libgarib fails.
pub fn hash_str(text: &str) -> Option<u32> {
    let mut value = 0u32;
    for c in text.chars() {
        let c = c as u32;
        if c > 0xFF {
            return None;
        }
        let mut entry = ((value >> 24) ^ c) << 24;
        for _ in 0..8 {
            entry = if entry & 0x8000_0000 != 0 { (entry << 1) ^ 0x04C1_1DB7 } else { entry << 1 };
        }
        value = (value << 8) ^ entry;
    }
    Some(value)
}

/// libgarib's `canonicalize_reference`: the id that a model or texture reference stands for. The reference is a
/// file name that starts with "0x" and 8 hex digits, or a name to hash. Folders before the last "/" are ignored.
pub fn canonicalize_reference(raw: &str) -> Option<u32> {
    let base = raw.rsplit('/').next().unwrap_or(raw);
    let b = base.as_bytes();
    if b.len() >= 10 && b[0] == b'0' && b[1] == b'x' && b[2..10].iter().all(|c| c.is_ascii_hexdigit()) {
        return u32::from_str_radix(&base[2..10], 16).ok();
    }
    hash_str(base)
}

struct Assembler<'a> {
    f: &'static Format,
    doc: &'a Doc,
    /// Every element in document order
    order: Vec<NodeId>,
    next: usize,
}

impl<'a> Assembler<'a> {
    fn next(&mut self) -> Result<NodeId, LevelError> {
        match self.order.get(self.next) {
            Some(n) => {
                self.next += 1;
                Ok(*n)
            }
            None => err("the level ends where a command needs a child element"),
        }
    }

    /// An attribute's text as the value libgarib makes of it for this field.
    fn coerce(&self, tag: &str, field: &Field, text: &str) -> Result<Value, LevelError> {
        let bad = |what: &str| err(format!("{} {}=\"{}\" is not {}", tag, field.id, text, what));
        match &field.kind {
            FieldKind::F4 => match pyfmt::parse_float(text) {
                Some(v) => Ok(Value::Float(v)),
                None => bad("a number"),
            },
            FieldKind::Str(size) => {
                let n = text.chars().count();
                let mut s: String = text.chars().take(*size).collect();
                for _ in n..*size {
                    s.push('\0');
                }
                Ok(Value::Str(s))
            }
            FieldKind::BytesEos | FieldKind::Struct(_) | FieldKind::Switch(_) => Ok(Value::Str(text.to_string())),
            _ => {
                if let Some(v) = field.enum_index.and_then(|e| self.f.enums[e].value(text)) {
                    return Ok(Value::Int(v as i128));
                }
                match pyfmt::parse_int(text) {
                    Some(v) => Ok(Value::Int(v)),
                    None => bad("an integer"),
                }
            }
        }
    }

    /// libgarib's `prepareConstructDict`: the element's fields as values. A field that isn't an attribute takes
    /// the next element of the document, whatever it is.
    fn prepare(&mut self, node: NodeId, depth: usize) -> Result<(usize, Value), LevelError> {
        if depth > 64 {
            return err("the level XML nests too deeply");
        }
        let doc = self.doc;
        let tag = doc.tag(node);
        let ti = match self.f.by_tag.get(tag) {
            Some(ti) => *ti,
            None => return err(format!("<{}> is not a command of the level format", tag)),
        };
        let t = &self.f.types[ti];
        let mut hashed: Vec<(usize, String)> = Vec::new();
        for (i, field) in t.fields.iter().enumerate() {
            if field.hash_namespace.is_some() {
                let raw = match doc.get(node, &field.id) {
                    Some(raw) => raw,
                    None => return err(format!("<{}> has no {}", tag, field.id)),
                };
                match canonicalize_reference(raw) {
                    Some(id) => hashed.push((i, format!("0x{:08X}", id))),
                    None => return err(format!("{} {}=\"{}\" cannot be hashed", tag, field.id, raw)),
                }
            }
        }
        let switch = t.switch();
        let mut values: Vec<Option<Value>> = vec![None; t.fields.len()];
        let mut child_types: Vec<Option<usize>> = vec![None; t.fields.len()];
        for (i, field) in t.fields.iter().enumerate() {
            let chooses = switch.is_some_and(|(_, sw)| sw.on == field.id);
            if !doc.has(node, &field.id) && !chooses {
                let child = self.next()?;
                let (cti, value) = self.prepare(child, depth + 1)?;
                values[i] = Some(value);
                child_types[i] = Some(cti);
            }
        }
        for (i, field) in t.fields.iter().enumerate() {
            if values[i].is_some() {
                continue;
            }
            if let Some(text) = doc.get(node, &field.id) {
                let text = hashed.iter().find(|(h, _)| *h == i).map_or(text, |(_, s)| s.as_str());
                values[i] = Some(self.coerce(tag, field, text)?);
            }
        }
        if let Some((body, sw)) = switch {
            if values[sw.on_index].is_none() {
                // The code comes from the child's type. That works when the type has exactly one code.
                let body_index = t.fields.iter().position(|x| x.id == body.id).unwrap_or(0);
                let cti = match child_types[body_index] {
                    Some(cti) => cti,
                    None => return err(format!("<{}> has no {}", tag, sw.on)),
                };
                let mut codes = sw.cases.iter().filter(|(_, c)| *c == cti).map(|(code, _)| *code);
                match (codes.next(), codes.next(), sw.default == Some(cti)) {
                    (Some(code), None, false) => values[sw.on_index] = Some(Value::Int(code as i128)),
                    _ => {
                        return err(format!(
                            "<{}> needs {}: <{}> does not stand for one code",
                            tag, sw.on, self.f.types[cti].tag
                        ))
                    }
                }
            }
        }
        let dict = t.fields.iter().zip(values).filter_map(|(field, v)| v.map(|v| (field.id.clone(), v))).collect();
        Ok((ti, Value::Dict(dict)))
    }

    /// Writes a struct from its values, as the construct library does: fields are found by name, other names
    /// are ignored, and a value of the wrong kind is an error.
    fn build(&self, ti: usize, value: &Value, out: &mut Vec<u8>, depth: usize) -> Result<(), LevelError> {
        if depth > 64 {
            return err("the level format nests too deeply");
        }
        let t = &self.f.types[ti];
        let dict = match value {
            Value::Dict(d) => d,
            // construct takes an empty string for a struct without fields
            Value::Str(s) if s.is_empty() && t.fields.is_empty() => return Ok(()),
            _ => return err(format!("<{}> is given as an attribute", t.tag)),
        };
        let mut ints: Vec<i128> = Vec::with_capacity(t.fields.len());
        for field in &t.fields {
            let what = || format!("{}.{}", t.tag, field.id);
            let v = match dict.iter().find(|(k, _)| *k == field.id) {
                Some((_, v)) => v,
                None => return err(format!("{} is missing", what())),
            };
            let mut int = 0i128;
            match &field.kind {
                FieldKind::F4 => {
                    let x = match v {
                        Value::Float(x) => *x,
                        Value::Int(i) => *i as f64,
                        _ => return err(format!("{} is not a number", what())),
                    };
                    let bits = if x.is_nan() {
                        // libgarib keeps the sign of a NaN
                        0x7FC0_0000 | if x.is_sign_negative() { 0x8000_0000u32 } else { 0 }
                    } else {
                        let y = x as f32;
                        if y.is_infinite() && x.is_finite() {
                            return err(format!("{} is too large for a 32-bit float", what()));
                        }
                        y.to_bits()
                    };
                    out.extend_from_slice(&bits.to_be_bytes());
                }
                FieldKind::Str(size) => {
                    let s = match v {
                        Value::Str(s) => s,
                        _ => return err(format!("{} is not text", what())),
                    };
                    if !s.is_ascii() {
                        return err(format!("{} is not ASCII", what()));
                    }
                    if s.len() > *size {
                        return err(format!("{} is longer than {} bytes", what(), size));
                    }
                    out.extend_from_slice(s.as_bytes());
                    out.resize(out.len() + (size - s.len()), 0);
                }
                // Only the type for an unknown command has such a field, and no element assembles to it.
                FieldKind::BytesEos => return err(format!("{} cannot be assembled", what())),
                FieldKind::Struct(inner) => self.build(*inner, v, out, depth + 1)?,
                FieldKind::Switch(sw) => {
                    let code = ints.get(sw.on_index).copied().unwrap_or(0);
                    match sw.type_for(code) {
                        Some(inner) => self.build(inner, v, out, depth + 1)?,
                        None => return err(format!("{}: no type for {} {}", t.tag, sw.on, code)),
                    }
                }
                kind => {
                    int = match v {
                        Value::Int(i) => *i,
                        // construct takes an enum's name here too
                        Value::Str(s) => match field.enum_index.and_then(|e| self.f.enums[e].value(s)) {
                            Some(i) => i as i128,
                            None => return err(format!("{} is not an integer", what())),
                        },
                        _ => return err(format!("{} is not an integer", what())),
                    };
                    let size = kind.int_size().unwrap_or(1);
                    let bits = 8 * size as u32;
                    let (lo, hi) = if kind.signed() {
                        (-(1i128 << (bits - 1)), (1i128 << (bits - 1)) - 1)
                    } else {
                        (0, (1i128 << bits) - 1)
                    };
                    if int < lo || int > hi {
                        return err(format!("{} = {} does not fit ({} to {})", what(), int, lo, hi));
                    }
                    out.extend_from_slice(&(int as i64 as u32).to_be_bytes()[4 - size..]);
                }
            }
            ints.push(int);
        }
        Ok(())
    }

    fn command(&self, ti: usize, params: Value, out: &mut Vec<u8>) -> Result<(), LevelError> {
        let t = &self.f.types[ti];
        let code = match t.opcode {
            Some(code) => code,
            None => return err(format!("<{}> is not a top-level command", t.tag)),
        };
        let cmd = Value::Dict(vec![("type_code".into(), Value::Int(code as i128)), ("params".into(), params)]);
        self.build(self.f.cmd, &cmd, out, 0)
    }
}

/// The level XML as a level file: the bytes libgarib's `xmlToLandscape` gives for the same tree.
///
/// The elements are read in document order, whatever their nesting. `validate` is a separate step: this function
/// takes what libgarib's assembler takes and refuses what makes it fail.
pub fn assemble(doc: &Doc) -> Result<Vec<u8>, LevelError> {
    let f = format();
    let root = doc.root();
    let order = doc.iter(root);
    // lxml puts an element with a default namespace under another name, and libgarib then fails on it.
    for n in &order {
        if doc.get(*n, "xmlns").is_some_and(|v| !v.is_empty()) {
            return err("the level XML may not use XML namespaces");
        }
    }
    let name = match doc.get(root, "name") {
        Some(name) => name,
        None => return err("<Level> has no name"),
    };
    if !name.is_ascii() {
        return err("the level's name is not ASCII");
    }
    let mut out = vec![0u8; 4];
    out.extend_from_slice(name.as_bytes());
    out.push(0);

    let mut a = Assembler { f, doc, order, next: 0 };
    while a.next < a.order.len() {
        let cmd = a.next()?;
        let tag = doc.tag(cmd);
        if tag == "Level" {
            continue;
        }
        if let Some((_, wrapper)) = f.groups.iter().find(|(g, _)| g == tag) {
            let field = match f.types[*wrapper].fields.first() {
                Some(field) => field.id.clone(),
                None => return err("a group type has no field"),
            };
            let last = match doc.children(cmd).last() {
                Some(last) => *last,
                None => return err(format!("<{}> is empty", tag)),
            };
            // Every following element is one grouped command, up to the group's last child. When a command
            // before it takes that child as its own, libgarib runs on past the group; so does this.
            loop {
                let grouped = a.next()?;
                let (_, value) = a.prepare(grouped, 0)?;
                a.command(*wrapper, Value::Dict(vec![(field.clone(), value)]), &mut out)?;
                if grouped == last {
                    break;
                }
            }
        } else {
            let (ti, value) = a.prepare(cmd, 0)?;
            a.command(ti, value, &mut out)?;
        }
    }
    out.resize(out.len() + (8 - out.len() % 8) % 8, 0);
    let len = match u32::try_from(out.len()) {
        Ok(len) => len,
        Err(_) => return err("the level is too large"),
    };
    out[..4].copy_from_slice(&len.to_be_bytes());
    Ok(out)
}

// ---------------------------------------------------------------------------------------------------------------
// The schema check

struct Checker<'a> {
    f: &'static Format,
    doc: &'a Doc,
    out: Vec<String>,
}

/// True when the text is the code as libgarib's schema allows it: decimal, or "0x", any zeros, and the hex digits.
fn matches_code(text: &str, code: u32) -> bool {
    if text == code.to_string() {
        return true;
    }
    let hex = format!("{:X}", code);
    match text.strip_prefix("0x") {
        Some(rest) if rest.len() >= hex.len() && rest.is_char_boundary(rest.len() - hex.len()) => {
            let (zeros, digits) = rest.split_at(rest.len() - hex.len());
            zeros.bytes().all(|b| b == b'0') && digits.eq_ignore_ascii_case(&hex)
        }
        _ => false,
    }
}

impl<'a> Checker<'a> {
    fn full(&self) -> bool {
        self.out.len() >= MAX_MESSAGES
    }
    fn say(&mut self, node: NodeId, msg: String) {
        if !self.full() {
            let path: Vec<String> = self.doc.path_of(node).iter().map(|i| (i + 1).to_string()).collect();
            let at = if path.is_empty() { String::new() } else { format!(" (element {})", path.join(".")) };
            self.out.push(format!("<{}>{}: {}", self.doc.tag(node), at, msg));
        }
    }

    /// Attributes: all of `required`, and nothing else. Text: none. Namespace declarations with a prefix aren't
    /// attributes for a schema.
    fn attributes(&mut self, node: NodeId, required: &[&str]) {
        let doc = self.doc;
        for name in required {
            if !doc.has(node, name) {
                self.say(node, format!("the attribute {} is missing", name));
            }
        }
        for (name, value) in doc.attrs(node) {
            if name.starts_with("xmlns:") || (name == "xmlns" && value.is_empty()) {
                continue;
            }
            if !required.contains(&name.as_str()) {
                self.say(node, format!("the attribute {} is not allowed", name));
            }
        }
        if !doc.text(node).trim().is_empty() {
            self.say(node, "text is not allowed".into());
        }
    }

    /// Checks an element that stands for the type `ti`.
    fn element(&mut self, node: NodeId, ti: usize) {
        if self.full() {
            return;
        }
        let f = self.f;
        let doc = self.doc;
        let t = &f.types[ti];
        let switch = t.switch();
        let mut required: Vec<&str> = t
            .fields
            .iter()
            .filter(|x| x.kind.is_attribute() && x.kind != FieldKind::BytesEos)
            .filter(|x| !switch.is_some_and(|(_, sw)| sw.on == x.id))
            .map(|x| x.id.as_str())
            .collect();
        if let Some((_, sw)) = switch {
            required.push(&sw.on);
        }
        self.attributes(node, &required);

        let mut children = doc.children(node);
        if let Some((_, sw)) = switch {
            // The first child is the switch's type, and the attribute names a code of that type.
            let possible: Vec<usize> = sw.cases.iter().map(|(_, c)| *c).chain(sw.default).collect();
            match children.first().copied() {
                Some(first) if possible.iter().any(|c| f.element_name(*c) == doc.tag(first)) => {
                    let tag = doc.tag(first);
                    let code = doc.get(node, &sw.on);
                    let by_code = sw
                        .cases
                        .iter()
                        .any(|(c, cti)| f.element_name(*cti) == tag && code.is_some_and(|text| matches_code(text, *c)));
                    let by_default = sw.default.is_some_and(|d| f.element_name(d) == tag);
                    if code.is_some() && !by_code && !by_default {
                        self.say(node, format!("{}=\"{}\" does not go with <{}>", sw.on, code.unwrap_or(""), tag));
                    }
                    self.child(first);
                    children = &children[1..];
                }
                _ => self.say(node, format!("the first child must be the element for its {}", sw.on)),
            }
        }
        for child in children {
            let tag = doc.tag(*child);
            if t.children.iter().any(|c| c == tag) {
                self.child(*child);
            } else {
                self.say(*child, format!("is not allowed in <{}>", t.tag));
            }
        }
    }

    /// Checks an element whose place is already known to be right.
    fn child(&mut self, node: NodeId) {
        if self.full() {
            return;
        }
        let f = self.f;
        let doc = self.doc;
        let tag = doc.tag(node);
        if let Some((_, inner)) = f.group(tag) {
            self.attributes(node, &[]);
            let inner_index = f.by_tag.get(&inner.tag).copied();
            for c in doc.children(node) {
                match inner_index {
                    Some(ii) if doc.tag(*c) == inner.tag => self.element(*c, ii),
                    _ => self.say(*c, format!("is not allowed in <{}>", tag)),
                }
            }
        } else if let Some(ti) = f.by_tag.get(tag) {
            self.element(node, *ti);
        } else {
            self.say(node, "is not an element of the level format".into());
        }
    }
}

/// Checks the tree's structure as libgarib's RelaxNG schema does: which element sits where, and which attributes
/// each one has. Values aren't checked, apart from the code of a switch. Returns at most `MAX_MESSAGES` messages;
/// an empty list means the tree is valid.
pub fn validate(doc: &Doc) -> Vec<String> {
    let f = format();
    let mut c = Checker { f, doc, out: Vec::new() };
    let root = doc.root();
    if doc.tag(root) != "Level" {
        c.say(root, "the root element must be <Level>".into());
        return c.out;
    }
    c.attributes(root, &["name", "libgarib-version", "data-format-version"]);
    for child in doc.children(root) {
        if f.top_level.iter().any(|t| t == doc.tag(*child)) {
            c.child(*child);
        } else {
            c.say(*child, "is not allowed in <Level>".into());
        }
    }
    c.out
}

/// True when the level has a SetActorRotation or SetActorScale that only the Expansion Pak game mod applies.
pub fn needs_mod(doc: &Doc) -> bool {
    doc.iter_tags(doc.root(), &["SetActorRotation", "SetActorScale"])
        .into_iter()
        .any(|n| doc.parent(n).is_some_and(|p| MOD_PARENTS.contains(&doc.tag(p))))
}

/// True when the bytes hold `<!DOCTYPE`, `<!ENTITY`, `<?xml-stylesheet`, `SYSTEM` before a quote, `<xi:` or
/// `xmlns:xi`, in any letter case.
fn forbidden(xml: &[u8]) -> bool {
    const WORDS: [&[u8]; 5] = [b"<!doctype", b"<!entity", b"<?xml-stylesheet", b"<xi:", b"xmlns:xi"];
    let starts = |i: usize, w: &[u8]| xml.len() - i >= w.len() && xml[i..i + w.len()].eq_ignore_ascii_case(w);
    for i in 0..xml.len() {
        match xml[i] {
            b'<' | b'x' | b'X' => {
                if WORDS.iter().any(|w| starts(i, w)) {
                    return true;
                }
            }
            b's' | b'S' => {
                if starts(i, b"system") {
                    let mut j = i + 6;
                    while j < xml.len() && matches!(xml[j], b' ' | b'\t' | b'\n' | b'\r' | 0x0B | 0x0C) {
                        j += 1;
                    }
                    if matches!(xml.get(j), Some(b'"') | Some(b'\'')) {
                        return true;
                    }
                }
            }
            _ => {}
        }
    }
    false
}

/// Reads level XML from an untrusted source. Refuses more than 8 MB, a DOCTYPE, entities, stylesheets and
/// XInclude, XML that isn't well-formed, and a root that isn't `<Level>`.
pub fn parse_level_xml(xml: &[u8]) -> Result<Doc, LevelError> {
    if xml.len() > MAX_XML {
        return err(format!("Level XML is larger than {} MB", MAX_XML >> 20));
    }
    if forbidden(xml) {
        return err("Level XML may not contain a DOCTYPE, entities, stylesheets or XInclude");
    }
    let doc = match Doc::parse_bytes(xml) {
        Ok(doc) => doc,
        Err(e) => return err(format!("Level XML is not well-formed: {}", e)),
    };
    if doc.tag(doc.root()) != "Level" {
        return err("Root element must be <Level>");
    }
    Ok(doc)
}

/// `parse_level_xml`, the schema check and `assemble` in one step. It gives the level file,
/// and whether it needs the game mod.
pub fn assemble_xml(xml: &[u8]) -> Result<(Vec<u8>, bool), LevelError> {
    let doc = parse_level_xml(xml)?;
    let problems = validate(&doc);
    if !problems.is_empty() {
        return err(format!("Level XML does not match the level format:\n{}", problems.join("\n")));
    }
    Ok((assemble(&doc)?, needs_mod(&doc)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn be(v: f32) -> [u8; 4] {
        v.to_be_bytes()
    }

    /// A made-up level: a platform with two modifiers, a land actor with a rotation, an enemy with instructions.
    fn sample() -> Vec<u8> {
        let mut b = vec![0, 0, 0, 0];
        b.extend_from_slice(b"TEST.ln\0");
        // Platform: object_id, name
        b.extend_from_slice(&[0x00, 0x62, 0x12, 0x34, 0x56, 0x78]);
        b.extend_from_slice(b"box\0\0\0\0\0");
        // PlatSetInitialPos
        b.extend_from_slice(&[0x00, 0xA6]);
        for v in [1.5f32, -2.0, 0.05] {
            b.extend_from_slice(&be(v));
        }
        // PlatNoClip
        b.extend_from_slice(&[0x00, 0x64]);
        // GloverSpawnPoint, which doesn't close the platform
        b.extend_from_slice(&[0x00, 0x01]);
        for v in [0.0f32, 1e-5, 3e10] {
            b.extend_from_slice(&be(v));
        }
        // PlatConstantSpin: axis (an enum), initial_theta, speed
        b.extend_from_slice(&[0x00, 0x7F, 0x00, 0x01]);
        b.extend_from_slice(&be(0.0));
        b.extend_from_slice(&be(0.25));
        // EndLevelData and padding
        b.extend_from_slice(&[0x7D, 0x00]);
        while b.len() % 8 != 0 {
            b.push(0);
        }
        let len = b.len() as u32;
        b[..4].copy_from_slice(&len.to_be_bytes());
        b
    }

    #[test]
    fn format_loads() {
        let f = format();
        assert_eq!(f.data_format_version.len(), 7);
        assert_eq!(f.opcode("Platform"), Some(0x62));
        assert_eq!(f.opcode("EndLevelData"), Some(0x7D00));
        assert_eq!(f.opcode("EnemyInstruction"), None);
        assert!(f.top_level_tags().iter().any(|t| t == "NormalInstructions"));
        assert!(!f.top_level_tags().iter().any(|t| t == "EnemyNormalInstruction"));
        assert!(!f.top_level_tags().iter().any(|t| t == "Cmd"));
        assert!(f.valid_children("Platform").contains(&"PlatNoClip"));
        assert!(f.valid_children("Platform").contains(&"SetActorScale"));
        assert!(f.valid_children("Enemy").contains(&"AttackInstructions"));
        assert!(f.valid_children("PlatNoClip").is_empty());
        assert_eq!(f.valid_children("ConditionalInstructions"), vec!["EnemyInstruction"]);
        assert!(f.switch_child_tags("EnemyInstruction").contains(&"EnemyInstructionMove"));
        assert_eq!(f.group_tags().count(), 3);
        let spin = f.type_by_tag("PlatConstantSpin").unwrap();
        let axis = f.field_enum(spin.field("axis").unwrap()).unwrap();
        assert_eq!(axis.value("y"), Some(1));
        assert_eq!(axis.name_of(2), Some("z"));
        assert_eq!(f.type_by_id("platform").unwrap().field("object_id").unwrap().hash_namespace.as_deref(), Some("object"));
    }

    #[test]
    fn disassembles_and_assembles_a_made_up_level() {
        let lev = sample();
        let doc = disassemble(&lev).unwrap();
        let f = format();
        let expected = format!(
            concat!(
                "<Level name=\"TEST.ln\" libgarib-version=\"{}\" data-format-version=\"{}\">",
                "<Platform object_id=\"0x12345678\" name=\"box\">",
                "<PlatSetInitialPos x=\"1.5\" y=\"-2.0\" z=\"0.05000000074505806\"/>",
                "<PlatNoClip/>",
                "<PlatConstantSpin axis=\"y\" initial_theta=\"0.0\" speed=\"0.25\"/>",
                "</Platform>",
                "<GloverSpawnPoint x=\"0.0\" y=\"9.999999747378752e-06\" z=\"30000001024.0\"/>",
                "<EndLevelData/>",
                "</Level>"
            ),
            f.libgarib_version, f.data_format_version
        );
        assert_eq!(doc.serialize(), expected);
        assert!(validate(&doc).is_empty(), "{:?}", validate(&doc));
        // The platform's commands come before the spawn point in the XML, so the bytes differ in order only.
        let again = assemble(&doc).unwrap();
        assert_eq!(again.len(), lev.len());
        assert_eq!(disassemble(&again).unwrap().serialize(), expected);
    }

    #[test]
    fn a_cut_file_is_an_error() {
        let lev = sample();
        for n in 0..lev.len() {
            // Must not panic. Most cuts end inside a command.
            let _ = disassemble(&lev[..n]);
        }
        assert!(disassemble(&lev[..20]).is_err());
        assert!(disassemble(b"\0\0\0\x08NAME").is_err());
        // A level without commands: libgarib fails on it
        assert!(disassemble(b"\0\0\0\x08N\0\0\0").is_err());
        // Text that isn't ASCII
        let mut bad = lev.clone();
        bad[18] = 0xE9;
        assert!(disassemble(&bad).is_err());
    }

    #[test]
    fn an_unknown_command_takes_the_rest_of_the_file() {
        let mut b = vec![0, 0, 0, 0, b'N', 0, 0x00, 0x64, 0x12, 0x34, 1, 2, 3, 4];
        let doc = disassemble(&b).unwrap();
        assert_eq!(doc.serialize_node(doc.children(doc.root())[1]), "<Unknown body=\"AQIDBA==\"/>");
        assert!(assemble(&doc).is_err());
        b.truncate(10);
        let doc = disassemble(&b).unwrap();
        assert_eq!(doc.serialize_node(doc.children(doc.root())[1]), "<Unknown body=\"\"/>");
    }

    fn level(body: &str) -> Doc {
        let xml = format!("<Level name=\"T\" libgarib-version=\"a\" data-format-version=\"b\">{}</Level>", body);
        parse_level_xml(xml.as_bytes()).unwrap()
    }

    #[test]
    fn values_are_coerced_like_libgarib() {
        let one = |body: &str| assemble(&level(body)).map(|b| b[6..].to_vec());
        // Hex, decimal, octal and binary integers; enum names and numbers
        assert_eq!(one("<AmbientLight r=\"0x10\" g=\"7\" b=\"0b11\"/>").unwrap()[..8], [0, 0x98, 0, 16, 0, 7, 0, 3]);
        let spin = |axis: &str| one(&format!("<PlatConstantSpin axis=\"{}\" initial_theta=\"1\" speed=\"-1e0\"/>", axis));
        assert_eq!(spin("z").unwrap()[..4], [0, 0x7F, 0, 2]);
        assert_eq!(spin("0x2").unwrap(), spin("z").unwrap());
        assert_eq!(spin(" 2 ").unwrap(), spin("z").unwrap());
        assert!(spin("w").is_err());
        assert!(spin("65536").is_err());
        assert!(spin("-1").is_err());
        // Out of range, and a number that isn't an integer
        assert!(one("<AmbientLight r=\"65536\" g=\"0\" b=\"0\"/>").is_err());
        assert!(one("<AmbientLight r=\"1.0\" g=\"0\" b=\"0\"/>").is_err());
        assert!(one("<AmbientLight r=\"\" g=\"0\" b=\"0\"/>").is_err());
        // Floats: an integer in hex isn't a float; too large for 32 bits is an error; infinity isn't
        assert!(one("<SetGravity strength=\"0x10\"/>").is_err());
        assert!(one("<SetGravity strength=\"1e39\"/>").is_err());
        assert_eq!(one("<SetGravity strength=\"inf\"/>").unwrap()[2..6], [0x7F, 0x80, 0, 0]);
        assert_eq!(one("<SetGravity strength=\"1e400\"/>").unwrap()[2..6], [0x7F, 0x80, 0, 0]);
        assert_eq!(one("<SetGravity strength=\"nan\"/>").unwrap()[2..6], [0x7F, 0xC0, 0, 0]);
        assert_eq!(one("<SetGravity strength=\"-2.5E-1\"/>").unwrap()[2..6], (-0.25f32).to_be_bytes());
        // Names: cut at 8 characters, padded with NULs, ASCII only; references are hashed
        let land = |id: &str, name: &str| {
            one(&format!("<LandActor object_id=\"{}\" name=\"{}\" x=\"0\" y=\"0\" z=\"0\"/>", id, name))
        };
        assert_eq!(land("0xAABBCCDD", "abcdefghij").unwrap()[2..14], *b"\xAA\xBB\xCC\xDDabcdefgh");
        assert_eq!(land("dir/0xaabbccdd.name", "ab").unwrap()[2..14], *b"\xAA\xBB\xCC\xDDab\0\0\0\0\0\0");
        assert_eq!(land("", "ab").unwrap()[2..6], [0, 0, 0, 0]);
        assert_eq!(land("a", "ab").unwrap()[2..6], 0xA864DB20u32.to_be_bytes());
        assert!(land("0x00000000", "\u{e9}").is_err());
        assert!(land("\u{100}", "a").is_err());
        // A missing attribute takes the next element as its value, which then fails
        assert!(one("<AmbientLight r=\"1\" g=\"1\"/><PlatNoClip/>").is_err());
        assert!(one("<AmbientLight r=\"1\" g=\"1\"/>").is_err());
        // Not a command, not a top-level command, an empty group
        assert!(one("<Nothing/>").is_err());
        assert!(one("<EnemyInstructionRest/>").is_err());
        assert!(one("<NormalInstructions/>").is_err());
        assert!(one("<Unknown/>").is_err());
    }

    #[test]
    fn hashes_names() {
        assert_eq!(hash_str(""), Some(0));
        assert_eq!(hash_str("a"), Some(0xA864DB20));
        assert_eq!(hash_str("\u{1}"), Some(0x04C11DB7));
        assert_eq!(hash_str("\u{100}"), None);
        assert_eq!(canonicalize_reference("x/y/0x0000002Atail"), Some(42));
        assert_eq!(canonicalize_reference("0X0000002A"), hash_str("0X0000002A"));
    }

    #[test]
    fn switches_and_groups() {
        let body = concat!(
            "<Enemy type=\"1\" u1=\"0\" x=\"0\" y=\"0\" z=\"0\" y_rotation=\"0\">",
            "<NormalInstructions>",
            "<EnemyInstruction instr_type=\"4\" lifetime=\"-1\" execution_condition_param_a=\"0\" ",
            "execution_condition_param_b=\"0\" flags=\"face_player\" execution_condition=\"always\">",
            "<EnemyInstructionRest flags=\"0\" anim_start_playing=\"0\"/>",
            "</EnemyInstruction>",
            "</NormalInstructions>",
            "</Enemy>"
        );
        let doc = level(body);
        let f = format();
        // Skip the test's exact field names when the format changes; the reference tests cover real levels.
        let rest = f.type_by_tag("EnemyInstructionRest").unwrap();
        let enemy = f.type_by_tag("Enemy").unwrap();
        let ids: Vec<&str> = rest.fields.iter().map(|x| x.id.as_str()).collect();
        let enemy_ids: Vec<&str> = enemy.fields.iter().map(|x| x.id.as_str()).collect();
        if ids != ["flags", "anim_start_playing"] || enemy_ids != ["type", "u1", "x", "y", "z", "y_rotation"] {
            return;
        }
        assert!(validate(&doc).is_empty(), "{:?}", validate(&doc));
        let lev = assemble(&doc).unwrap();
        let back = disassemble(&lev).unwrap();
        assert!(back.serialize().contains("<NormalInstructions><EnemyInstruction instr_type=\"4\" lifetime=\"-1\""));
        assert!(back.serialize().contains("flags=\"face_player\" execution_condition=\"always\""));
        // The code may be left out when the child stands for one code; libgarib takes that, the schema doesn't.
        let without = level(&body.replace("instr_type=\"4\" ", ""));
        assert_eq!(assemble(&without).unwrap(), lev);
        assert!(!validate(&without).is_empty());
        // A code that doesn't go with the child
        let wrong = level(&body.replace("instr_type=\"4\"", "instr_type=\"5\""));
        assert!(!validate(&wrong).is_empty());
        let hex = level(&body.replace("instr_type=\"4\"", "instr_type=\"0x004\""));
        assert!(validate(&hex).is_empty());
        assert_eq!(assemble(&hex).unwrap(), lev);
    }

    #[test]
    fn code_patterns() {
        assert!(matches_code("75", 75));
        assert!(matches_code("0x4b", 75));
        assert!(matches_code("0x004B", 75));
        assert!(!matches_code("0X4B", 75));
        assert!(!matches_code(" 75", 75));
        assert!(!matches_code("075", 75));
        assert!(matches_code("0x0", 0));
        assert!(matches_code("0x000", 0));
        assert!(!matches_code("0x", 0));
        assert!(!matches_code("0x14b", 75));
        assert!(!matches_code("0x\u{e9}b", 75));
    }

    #[test]
    fn validate_refuses_broken_structure() {
        let ok = level("<Platform object_id=\"0x00000001\" name=\"a\"><PlatNoClip/><SetActorScale x=\"1\" y=\"1\" z=\"1\"/></Platform>");
        assert!(validate(&ok).is_empty(), "{:?}", validate(&ok));
        let cases = [
            "<Nothing/>",
            "<PlatNoClip extra=\"1\"/>",
            "<PlatNoClip>text</PlatNoClip>",
            "<Platform object_id=\"0x00000001\"/>",
            "<Platform object_id=\"0x00000001\" name=\"a\"><Platform object_id=\"0x00000001\" name=\"a\"/></Platform>",
            "<EnemyInstruction/>",
            "<EnemyNormalInstruction/>",
            "<Cmd type_code=\"1\"/>",
            "<Unknown body=\"\"/>",
            "<NormalInstructions><PlatNoClip/></NormalInstructions>",
            "<NormalInstructions a=\"1\"/>",
            "<PlatNoClip xmlns=\"urn:x\"/>",
            "<a:PlatNoClip xmlns:a=\"urn:x\"/>",
        ];
        for body in cases {
            assert!(!validate(&level(body)).is_empty(), "{}", body);
        }
        // What the schema takes although the assembler doesn't, and an unused namespace prefix
        for body in ["<Unknown/>", "<NormalInstructions/>", "<PlatNoClip xmlns:a=\"urn:x\"/>", "<PlatNoClip/><Noop/>"] {
            assert!(validate(&level(body)).is_empty(), "{}", body);
        }
        let root = Doc::parse("<Level name=\"a\" libgarib-version=\"a\"/>").unwrap();
        assert_eq!(validate(&root).len(), 1);
        let root = Doc::parse("<Levels/>").unwrap();
        assert_eq!(validate(&root).len(), 1);
        let many = level(&"<Nothing/>".repeat(50));
        assert_eq!(validate(&many).len(), MAX_MESSAGES);
    }

    #[test]
    fn parse_refusals() {
        assert!(parse_level_xml(b"<Level/>").is_ok());
        assert!(parse_level_xml(b"<Levels/>").is_err());
        assert!(parse_level_xml(b"<Level>").is_err());
        assert!(parse_level_xml(b"<!doctype x><Level/>").is_err());
        assert!(parse_level_xml(b"<Level a='system \t\"'/>").is_err());
        assert!(parse_level_xml(b"<Level a='system x'/>").is_ok());
        assert!(parse_level_xml(b"<Level XMLNS:XI='a'/>").is_err());
        assert!(parse_level_xml(b"<?XML-stylesheet a?><Level/>").is_err());
        assert!(parse_level_xml(b"<Level><!ENTITY></Level>").is_err());
        assert!(parse_level_xml(b"<Level><xi:include/></Level>").is_err());
        let big = vec![b' '; MAX_XML + 1];
        assert!(parse_level_xml(&big).is_err());
    }

    #[test]
    fn needs_mod_looks_at_the_parent() {
        assert!(!needs_mod(&level("<LandActor><SetActorScale/></LandActor><SetActorRotation/>")));
        assert!(needs_mod(&level("<Water><SetActorScale/></Water>")));
        assert!(needs_mod(&level("<NullPlatform><SetActorRotation/></NullPlatform>")));
    }

    #[test]
    fn commands_stop_at_the_end_of_the_level() {
        // A name of 6 letters leaves 3 bytes of padding after EndLevelData.
        let mut b = vec![0u8; 4];
        b.extend_from_slice(b"ISLAND\0");
        b.extend_from_slice(&[0x7D, 0x00]);
        while b.len() % 8 != 0 {
            b.push(0);
        }
        assert_eq!(b.len() - 13, 3);
        assert_eq!(commands(&b).unwrap(), vec![(0x7D00, 13)]);
    }
}
