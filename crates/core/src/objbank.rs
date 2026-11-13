//! Glover's object banks: a hardened parser, the linker that writes a bank, splitting a bank into single-object
//! banks, packing objects into a bank, and the model the editor draws for an object.
//! Ported from libgarib's `objects.py`, `linkable.py`, `display_lists.py`, `gbi.py`, `gltf_helper.py` and
//! `glover.objbank.ksy` (see THIRD_PARTY_NOTICES.md).
//!
//! A bank starts with a directory of object ids and pointers, ended by an id of 0. Every pointer in a bank is an
//! offset from the start of the bank. An object has an animation block and a tree of meshes. A mesh has key
//! frames for its translation, rotation and scale, and may have geometry, a display list and sprites.
//!
//! The parts are in three layers. `Bank::parse` reads a bank and borrows from its bytes. `Linker` holds segments
//! with pointers between them and writes a bank; it does not care where the segments come from. `split`, `pack`,
//! `model`, `texture_ids` and `faces` are what the editor calls.

use crate::fla2;
use crate::model::{AlphaMode, Material, Mesh as ModelMesh, Model, Node, Primitive, Wrap};
use std::borrow::Cow;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjError(pub String);

impl std::fmt::Display for ObjError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ObjError {}

fn err<T>(text: impl Into<String>) -> Result<T, ObjError> {
    Err(ObjError(text.into()))
}

/// Sizes of the bank's structures and offsets of their fields (`glover.objbank.ksy`).
pub mod layout {
    pub const OBJECT: usize = 28;
    pub const OBJECT_MESH_PTR: u32 = 12;
    pub const OBJECT_ANIMATION_PTR: u32 = 24;

    pub const MESH: usize = 64;
    pub const MESH_GEOMETRY_PTR: u32 = 20;
    pub const MESH_DISPLAY_LIST_PTR: u32 = 24;
    pub const MESH_SCALE_PTR: u32 = 28;
    pub const MESH_TRANSLATION_PTR: u32 = 32;
    pub const MESH_ROTATION_PTR: u32 = 36;
    pub const MESH_SPRITES_PTR: u32 = 44;
    pub const MESH_CHILD_PTR: u32 = 52;
    pub const MESH_SIBLING_PTR: u32 = 56;

    pub const GEOMETRY: usize = 36;
    pub const GEOMETRY_VERTICES_PTR: u32 = 4;
    pub const GEOMETRY_FACES_PTR: u32 = 8;
    pub const GEOMETRY_FACE_CN_PTR: u32 = 12;
    pub const GEOMETRY_UVS_PTR: u32 = 16;
    pub const GEOMETRY_UVS_UNMODIFIED_PTR: u32 = 20;
    pub const GEOMETRY_VERTEX_CN_PTR: u32 = 24;
    pub const GEOMETRY_FLAGS_PTR: u32 = 28;
    pub const GEOMETRY_TEXTURE_IDS_PTR: u32 = 32;

    pub const ANIMATION: usize = 80;
    pub const ANIMATION_DEFINITIONS_PTR: u32 = 72;
    pub const ANIMATION_DEFINITION: usize = 12;

    pub const VERTEX: usize = 12;
    pub const FACE: usize = 6;
    pub const UV: usize = 12;
    pub const SPRITE: usize = 24;
    /// A key frame: four floats and a time
    pub const KEY: usize = 20;
    /// A vertex of a display list
    pub const DL_VERTEX: usize = 16;
}
use layout::*;

/// Bits of a mesh's render mode.
pub mod render_mode {
    pub const PER_VERTEX_CN: u16 = 0x1;
    /// Drawn blended
    pub const XLU: u16 = 0x2;
    /// Drawn with an alpha cut-out
    pub const MASKED: u16 = 0x4;
    pub const UNLIT: u16 = 0x8;
    pub const RIPPLE: u16 = 0x20;
    pub const SYNC_TO_GLOBAL_CLOCK: u16 = 0x40;
    pub const CLOUD: u16 = 0x80;
}

/// The most meshes one object may have. A mesh tree that loops back on itself stops here.
const MAX_MESHES: usize = 16384;

fn u16_at(d: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([d[at], d[at + 1]])
}

fn u32_at(d: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

fn f32_at(d: &[u8], at: usize) -> f32 {
    f32::from_bits(u32_at(d, at))
}

/// `len` bytes at `at`, or an error. A read of no bytes succeeds anywhere, as it does in libgarib's parser.
fn rd<'a>(data: &'a [u8], at: u32, len: usize, what: &str) -> Result<&'a [u8], ObjError> {
    if len == 0 {
        return Ok(&[]);
    }
    let at = at as usize;
    match at.checked_add(len).and_then(|end| data.get(at..end)) {
        Some(s) => Ok(s),
        None => err(format!("the object bank ends inside {what} at 0x{at:X}")),
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The parser

/// One key frame of a mesh's translation, rotation or scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Key {
    /// x, y, z and, for a rotation, w
    pub v: [f32; 4],
    pub t: u32,
}

/// A sprite of a mesh: a textured square that faces the camera.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sprite {
    pub texture_id: u32,
    pub x: u16,
    pub y: u16,
    pub z: u16,
    pub width: u16,
    pub height: u16,
    pub flags: u16,
}

/// A mesh's geometry. Each array is the raw big-endian data, or none when its pointer is 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Geometry<'a> {
    pub num_faces: u16,
    pub num_vertices: u16,
    /// The 36-byte structure
    pub raw: &'a [u8],
    /// Three floats for each vertex
    pub vertices: Option<&'a [u8]>,
    /// Three 16-bit vertex indices for each face
    pub faces: Option<&'a [u8]>,
    /// Four bytes for each face: a normal of three signed bytes, or a colour
    pub face_cn: Option<&'a [u8]>,
    /// Six 16-bit numbers for each face: u and v of each corner, in 1/32 of a texel
    pub uvs: Option<&'a [u8]>,
    /// Four bytes for each vertex: a colour, or a normal
    pub vertex_cn: Option<&'a [u8]>,
    /// One byte for each face
    pub flags: Option<&'a [u8]>,
    /// One texture id for each face
    pub texture_ids: Option<&'a [u8]>,
}

/// One mesh of an object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mesh<'a> {
    pub id: u32,
    /// The name without the zero bytes at its ends
    pub name: &'a str,
    pub mesh_alpha: u8,
    pub sprite_alpha: u8,
    pub render_mode: u16,
    /// The 64-byte structure
    pub raw: &'a [u8],
    pub geometry: Option<Geometry<'a>>,
    /// The display list's commands, 8 bytes each, to the end command
    pub display_list: Option<&'a [u8]>,
    /// 24 bytes for each sprite
    pub sprites: Option<&'a [u8]>,
    /// 20 bytes for each key
    pub scale: Option<&'a [u8]>,
    pub translation: Option<&'a [u8]>,
    pub rotation: Option<&'a [u8]>,
    /// An index into `Object::meshes`
    pub child: Option<u32>,
    pub sibling: Option<u32>,
}

fn key_at(raw: &[u8], i: usize) -> Option<Key> {
    let k = raw.get(i * KEY..(i + 1) * KEY)?;
    Some(Key { v: [f32_at(k, 0), f32_at(k, 4), f32_at(k, 8), f32_at(k, 12)], t: u32_at(k, 16) })
}

impl<'a> Mesh<'a> {
    /// The keys of a track (`scale`, `translation` or `rotation`).
    pub fn keys(track: Option<&'a [u8]>) -> impl Iterator<Item = Key> + 'a {
        let raw = track.unwrap_or(&[]);
        (0..raw.len() / KEY).filter_map(move |i| key_at(raw, i))
    }

    pub fn sprite_list(&self) -> impl Iterator<Item = Sprite> + 'a {
        self.sprites.unwrap_or(&[]).chunks_exact(SPRITE).map(|s| Sprite {
            texture_id: u32_at(s, 0),
            x: u16_at(s, 8),
            y: u16_at(s, 10),
            z: u16_at(s, 12),
            width: u16_at(s, 14),
            height: u16_at(s, 16),
            flags: u16_at(s, 22),
        })
    }
}

/// An object's animation block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Animation<'a> {
    /// The 80-byte structure
    pub raw: &'a [u8],
    /// 12 bytes for each definition: start time, end time, playback speed and an unused word
    pub definitions: Option<&'a [u8]>,
}

/// One object of a bank.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Object<'a> {
    /// The id in the bank's directory
    pub dir_id: u32,
    /// The id in the object itself. The game's banks have the same id in both places.
    pub id: u32,
    pub offset: u32,
    /// The 28-byte structure
    pub raw: &'a [u8],
    pub animation: Option<Animation<'a>>,
    /// The meshes, the root first. A mesh comes before its children, and its children before its siblings.
    pub meshes: Vec<Mesh<'a>>,
}

impl<'a> Object<'a> {
    pub fn root(&self) -> Option<&Mesh<'a>> {
        self.meshes.first()
    }

    /// The root mesh's name. `objbank-tool` puts it in the names of the files it writes.
    pub fn name(&self) -> Option<&'a str> {
        self.root().map(|m| m.name)
    }
}

/// A parsed bank. It borrows from the bank's bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bank<'a> {
    pub data: &'a [u8],
    /// The objects in the directory's order. Directory entries without a pointer are left out.
    pub objects: Vec<Object<'a>>,
}

impl<'a> Bank<'a> {
    /// Reads a bank. `data` is not compressed (see `fla2::data`). Every pointer and count is checked; a bank with
    /// one bad structure is refused whole.
    pub fn parse(data: &'a [u8]) -> Result<Bank<'a>, ObjError> {
        let mut objects = Vec::new();
        let mut pos = 0usize;
        loop {
            let Some(entry) = data.get(pos..pos + 8) else {
                return err("the object bank's directory has no end");
            };
            pos += 8;
            let (id, ptr) = (u32_at(entry, 0), u32_at(entry, 4));
            if ptr != 0 {
                objects.push(parse_object(data, id, ptr)?);
            }
            if id == 0 {
                break;
            }
        }
        Ok(Bank { data, objects })
    }

    /// The first object with this id.
    pub fn object(&self, id: u32) -> Option<&Object<'a>> {
        self.objects.iter().find(|o| o.id == id)
    }
}

fn parse_object<'a>(data: &'a [u8], dir_id: u32, ptr: u32) -> Result<Object<'a>, ObjError> {
    let raw = rd(data, ptr, OBJECT, "an object")?;
    let mesh_ptr = u32_at(raw, OBJECT_MESH_PTR as usize);
    let anim_ptr = u32_at(raw, OBJECT_ANIMATION_PTR as usize);
    let animation = if anim_ptr != 0 {
        let a = rd(data, anim_ptr, ANIMATION, "an animation block")?;
        let count = (u16_at(a, 0) as i16).max(0) as usize;
        let defs_ptr = u32_at(a, ANIMATION_DEFINITIONS_PTR as usize);
        let definitions = if defs_ptr != 0 {
            Some(rd(data, defs_ptr, count * ANIMATION_DEFINITION, "animation definitions")?)
        } else {
            None
        };
        Some(Animation { raw: a, definitions })
    } else {
        None
    };
    let mut meshes: Vec<Mesh<'a>> = Vec::new();
    // What is left to read: a mesh's pointer, the mesh that points to it, and whether it is that mesh's child.
    let mut todo: Vec<(u32, Option<(u32, bool)>)> = Vec::new();
    if mesh_ptr != 0 {
        todo.push((mesh_ptr, None));
    }
    while let Some((ptr, from)) = todo.pop() {
        if meshes.len() >= MAX_MESHES {
            return err(format!("object 0x{dir_id:08X} has too many meshes"));
        }
        let index = meshes.len() as u32;
        let (mesh, child_ptr, sibling_ptr) = parse_mesh(data, ptr)?;
        meshes.push(mesh);
        match from {
            Some((parent, true)) => meshes[parent as usize].child = Some(index),
            Some((parent, false)) => meshes[parent as usize].sibling = Some(index),
            None => {}
        }
        // The child is read first: it is pushed last.
        if sibling_ptr != 0 {
            todo.push((sibling_ptr, Some((index, false))));
        }
        if child_ptr != 0 {
            todo.push((child_ptr, Some((index, true))));
        }
    }
    Ok(Object { dir_id, id: u32_at(raw, 0), offset: ptr, raw, animation, meshes })
}

fn parse_mesh<'a>(data: &'a [u8], ptr: u32) -> Result<(Mesh<'a>, u32, u32), ObjError> {
    let raw = rd(data, ptr, MESH, "a mesh")?;
    let name = &raw[4..12];
    if !name.is_ascii() {
        return err(format!("the mesh at 0x{ptr:X} has a name that is not ASCII"));
    }
    let name = std::str::from_utf8(name).unwrap_or("").trim_matches('\0');
    let at = |field: u32| u32_at(raw, field as usize);
    let array = |field: u32, count: usize, size: usize, what: &str| -> Result<Option<&'a [u8]>, ObjError> {
        match at(field) {
            0 => Ok(None),
            p => rd(data, p, count * size, what).map(Some),
        }
    };
    let geometry = match at(MESH_GEOMETRY_PTR) {
        0 => None,
        p => {
            let g = rd(data, p, GEOMETRY, "a geometry")?;
            let (faces, verts) = (u16_at(g, 0) as usize, u16_at(g, 2) as usize);
            let part = |field: u32, count: usize, size: usize, what: &str| match u32_at(g, field as usize) {
                0 => Ok(None),
                p => rd(data, p, count * size, what).map(Some),
            };
            Some(Geometry {
                num_faces: faces as u16,
                num_vertices: verts as u16,
                raw: g,
                vertices: part(GEOMETRY_VERTICES_PTR, verts, VERTEX, "vertices")?,
                faces: part(GEOMETRY_FACES_PTR, faces, FACE, "faces")?,
                face_cn: part(GEOMETRY_FACE_CN_PTR, faces, 4, "face normals")?,
                uvs: part(GEOMETRY_UVS_PTR, faces, UV, "texture coordinates")?,
                vertex_cn: part(GEOMETRY_VERTEX_CN_PTR, verts, 4, "vertex colours")?,
                flags: part(GEOMETRY_FLAGS_PTR, faces, 1, "face flags")?,
                texture_ids: part(GEOMETRY_TEXTURE_IDS_PTR, faces, 4, "texture ids")?,
            })
        }
    };
    let display_list = match at(MESH_DISPLAY_LIST_PTR) {
        0 => None,
        p => {
            let rest = data.get(p as usize..).unwrap_or(&[]);
            let Some(n) = rest.chunks_exact(8).position(|c| c[0] == 0xB8) else {
                return err(format!("the display list at 0x{p:X} has no end"));
            };
            Some(&rest[..(n + 1) * 8])
        }
    };
    let mesh = Mesh {
        id: at(0),
        name,
        mesh_alpha: raw[12],
        sprite_alpha: raw[13],
        render_mode: u16_at(raw, 50),
        raw,
        geometry,
        display_list,
        sprites: array(MESH_SPRITES_PTR, at(40) as usize, SPRITE, "sprites")
            .or_else(|_| err(format!("the object bank ends inside the sprites of the mesh at 0x{ptr:X}")))?,
        scale: array(MESH_SCALE_PTR, u16_at(raw, 14) as usize, KEY, "scale keys")?,
        translation: array(MESH_TRANSLATION_PTR, u16_at(raw, 16) as usize, KEY, "translation keys")?,
        rotation: array(MESH_ROTATION_PTR, u16_at(raw, 18) as usize, KEY, "rotation keys")?,
        child: None,
        sibling: None,
    };
    Ok((mesh, at(MESH_CHILD_PTR), at(MESH_SIBLING_PTR)))
}

// ---------------------------------------------------------------------------------------------------------------
// The linker

/// The kinds of segment, in the order a bank holds them after its directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    DisplayList,
    Geometry,
    Keyframes,
    Sprites,
    Mesh,
    AnimDefs,
    AnimProps,
    Actor,
}

/// A segment of a `Linker`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SegId(u32);

/// Where a pointer points: `offset` bytes into a block of a segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    pub seg: SegId,
    pub block: u32,
    pub offset: u32,
}

impl Target {
    /// The start of a segment
    pub fn seg(seg: SegId) -> Target {
        Target { seg, block: 0, offset: 0 }
    }

    /// The start of one block of a segment
    pub fn block(seg: SegId, block: u32) -> Target {
        Target { seg, block, offset: 0 }
    }
}

/// A 32-bit pointer at `offset` in a block. The linker writes the target's place in the bank there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pointer {
    pub offset: u32,
    pub target: Target,
}

/// Bytes with the pointers in them. In the bank a block takes its size rounded up to a multiple of 8.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block<'a> {
    pub data: Cow<'a, [u8]>,
    pub pointers: Vec<Pointer>,
}

impl<'a> Block<'a> {
    pub fn new(data: impl Into<Cow<'a, [u8]>>) -> Block<'a> {
        Block { data: data.into(), pointers: Vec::new() }
    }

    fn size(&self) -> usize {
        self.data.len().next_multiple_of(8)
    }
}

#[derive(Debug, Clone)]
struct Segment<'a> {
    blocks: Vec<Block<'a>>,
    alive: bool,
}

/// The parts of a geometry. `root` is the 36-byte structure; the linker fills in the pointer of each part given.
#[derive(Debug, Clone, Default)]
pub struct GeometryParts<'a> {
    pub root: Cow<'a, [u8]>,
    pub verts: Option<Cow<'a, [u8]>>,
    pub faces: Option<Cow<'a, [u8]>>,
    pub vertex_cn: Option<Cow<'a, [u8]>>,
    pub face_cn: Option<Cow<'a, [u8]>>,
    pub uvs: Option<Cow<'a, [u8]>>,
    pub flags: Option<Cow<'a, [u8]>>,
    pub texture_ids: Option<Cow<'a, [u8]>>,
}

/// Builds a bank from segments. A segment is one or more blocks that stay together. Pointers between segments
/// are kept as references until `link` lays the bank out: the directory, then the segments by kind, each kind in
/// the order its segments were added. This is libgarib's `LinkableObjectBank`.
#[derive(Debug, Clone, Default)]
pub struct Linker<'a> {
    segs: Vec<Segment<'a>>,
    order: [Vec<SegId>; 8],
    /// The directory: object ids and their actor segments, in the order they were added
    actors: Vec<(u32, SegId)>,
}

impl<'a> Linker<'a> {
    pub fn new() -> Linker<'a> {
        Linker::default()
    }

    /// Adds a segment of several blocks.
    pub fn add(&mut self, kind: Kind, blocks: Vec<Block<'a>>) -> SegId {
        let id = SegId(self.segs.len() as u32);
        self.segs.push(Segment { blocks, alive: true });
        self.order[kind as usize].push(id);
        id
    }

    /// Adds a segment of one block.
    pub fn bytes(&mut self, kind: Kind, data: impl Into<Cow<'a, [u8]>>) -> SegId {
        self.add(kind, vec![Block::new(data)])
    }

    /// Adds a pointer at `offset` of a block of `from`.
    pub fn point(&mut self, from: SegId, block: u32, offset: u32, target: Target) {
        if let Some(b) = self.segs.get_mut(from.0 as usize).and_then(|s| s.blocks.get_mut(block as usize)) {
            b.pointers.push(Pointer { offset, target });
        }
    }

    /// Adds a geometry segment: the parts given, then the root. Returns where a mesh's geometry pointer points.
    pub fn add_geometry(&mut self, parts: GeometryParts<'a>) -> Target {
        let mut blocks = Vec::with_capacity(8);
        let mut pointers = Vec::new();
        let id = SegId(self.segs.len() as u32);
        for (part, field) in [
            (parts.verts, GEOMETRY_VERTICES_PTR),
            (parts.faces, GEOMETRY_FACES_PTR),
            (parts.vertex_cn, GEOMETRY_VERTEX_CN_PTR),
            (parts.face_cn, GEOMETRY_FACE_CN_PTR),
            (parts.uvs, GEOMETRY_UVS_PTR),
            (parts.flags, GEOMETRY_FLAGS_PTR),
            (parts.texture_ids, GEOMETRY_TEXTURE_IDS_PTR),
        ] {
            if let Some(data) = part {
                pointers.push(Pointer { offset: field, target: Target::block(id, blocks.len() as u32) });
                blocks.push(Block::new(data));
            }
        }
        let root = blocks.len() as u32;
        blocks.push(Block { data: parts.root, pointers });
        self.add(Kind::Geometry, blocks);
        Target::block(id, root)
    }

    /// Puts an object in the directory. An id that is there already keeps its place and gets the new actor.
    pub fn set_actor(&mut self, id: u32, actor: SegId) {
        match self.actors.iter_mut().find(|(i, _)| *i == id) {
            Some(entry) => entry.1 = actor,
            None => self.actors.push((id, actor)),
        }
    }

    pub fn has_actor(&self, id: u32) -> bool {
        self.actors.iter().any(|(i, _)| *i == id)
    }

    /// The number of objects in the directory.
    pub fn len(&self) -> usize {
        self.actors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.actors.is_empty()
    }

    /// Removes an object: its directory entry and every segment its actor reaches through pointers.
    pub fn delete(&mut self, id: u32) {
        let Some(at) = self.actors.iter().position(|(i, _)| *i == id) else { return };
        let (_, actor) = self.actors.remove(at);
        let mut todo = vec![actor];
        while let Some(seg) = todo.pop() {
            let Some(s) = self.segs.get_mut(seg.0 as usize) else { continue };
            if !s.alive {
                continue;
            }
            s.alive = false;
            for b in &s.blocks {
                todo.extend(b.pointers.iter().map(|p| p.target.seg).filter(|t| *t != seg));
            }
        }
        let segs = &self.segs;
        for list in &mut self.order {
            list.retain(|s| segs[s.0 as usize].alive);
        }
    }

    /// Adds one parsed object's segments and returns its actor. `data` is the bank the object was parsed from.
    /// The directory is not changed.
    pub fn add_object(&mut self, data: &'a [u8], obj: &Object<'a>) -> Result<SegId, ObjError> {
        let actor = self.bytes(Kind::Actor, obj.raw);
        if let Some(anim) = &obj.animation {
            let props = self.bytes(Kind::AnimProps, anim.raw);
            if let Some(defs) = anim.definitions {
                let defs = self.bytes(Kind::AnimDefs, defs);
                self.point(props, 0, ANIMATION_DEFINITIONS_PTR, Target::seg(defs));
            }
            self.point(actor, 0, OBJECT_ANIMATION_PTR, Target::seg(props));
        }
        let mut ids = Vec::with_capacity(obj.meshes.len());
        for mesh in &obj.meshes {
            let m = self.bytes(Kind::Mesh, mesh.raw);
            ids.push(m);
            if let Some(g) = &mesh.geometry {
                let b = |part: Option<&'a [u8]>| part.map(Cow::Borrowed);
                let root = self.add_geometry(GeometryParts {
                    root: Cow::Borrowed(g.raw),
                    verts: b(g.vertices),
                    faces: b(g.faces),
                    vertex_cn: b(g.vertex_cn),
                    face_cn: b(g.face_cn),
                    uvs: b(g.uvs),
                    flags: b(g.flags),
                    texture_ids: b(g.texture_ids),
                });
                self.point(m, 0, MESH_GEOMETRY_PTR, root);
            }
            if let Some(dl) = mesh.display_list {
                let (blob, pointers) = relocatable_display_list(data, dl)?;
                let seg = SegId(self.segs.len() as u32);
                let pointers = pointers
                    .into_iter()
                    .map(|(offset, to)| Pointer { offset, target: Target { seg, block: 0, offset: to } })
                    .collect();
                self.add(Kind::DisplayList, vec![Block { data: Cow::Owned(blob), pointers }]);
                self.point(m, 0, MESH_DISPLAY_LIST_PTR, Target::seg(seg));
            }
            for (part, kind, field) in [
                (mesh.sprites, Kind::Sprites, MESH_SPRITES_PTR),
                (mesh.translation, Kind::Keyframes, MESH_TRANSLATION_PTR),
                (mesh.rotation, Kind::Keyframes, MESH_ROTATION_PTR),
                (mesh.scale, Kind::Keyframes, MESH_SCALE_PTR),
            ] {
                if let Some(data) = part {
                    let seg = self.bytes(kind, data);
                    self.point(m, 0, field, Target::seg(seg));
                }
            }
        }
        for (mesh, &m) in obj.meshes.iter().zip(&ids) {
            for (link, field) in [(mesh.child, MESH_CHILD_PTR), (mesh.sibling, MESH_SIBLING_PTR)] {
                if let Some(&to) = link.and_then(|i| ids.get(i as usize)) {
                    self.point(m, 0, field, Target::seg(to));
                }
            }
        }
        if let Some(&root) = ids.first() {
            self.point(actor, 0, OBJECT_MESH_PTR, Target::seg(root));
        }
        Ok(actor)
    }

    /// Adds every object of a parsed bank. An object whose id is in the directory already replaces the old one
    /// and goes to the end of the directory. This is `LinkableObjectBank.merge`.
    pub fn add_bank(&mut self, bank: &Bank<'a>) -> Result<(), ObjError> {
        let mut seen: Vec<u32> = Vec::with_capacity(bank.objects.len());
        for o in &bank.objects {
            if !seen.contains(&o.dir_id) {
                seen.push(o.dir_id);
                self.delete(o.dir_id);
            }
        }
        for o in &bank.objects {
            let actor = self.add_object(bank.data, o)?;
            self.set_actor(o.dir_id, actor);
        }
        Ok(())
    }

    /// Lays the bank out and writes it.
    pub fn link(&self) -> Result<Vec<u8>, ObjError> {
        let mut start = vec![usize::MAX; self.segs.len()];
        let mut size = 8 * (self.actors.len() + 1);
        for id in self.order.iter().flatten() {
            start[id.0 as usize] = size;
            size += self.segs[id.0 as usize].blocks.iter().map(Block::size).sum::<usize>();
        }
        if size > u32::MAX as usize {
            return err("the object bank is too large");
        }
        let place = |t: Target| -> Result<u32, ObjError> {
            let seg = self.segs.get(t.seg.0 as usize).filter(|s| s.alive);
            let Some(seg) = seg.filter(|s| (t.block as usize) < s.blocks.len()) else {
                return err("a pointer in the object bank has no target");
            };
            let before: usize = seg.blocks[..t.block as usize].iter().map(Block::size).sum();
            match u32::try_from(start[t.seg.0 as usize] + before + t.offset as usize) {
                Ok(v) => Ok(v),
                Err(_) => err("the object bank is too large"),
            }
        };
        let mut out = vec![0u8; size];
        for (i, (id, actor)) in self.actors.iter().enumerate() {
            out[i * 8..i * 8 + 4].copy_from_slice(&id.to_be_bytes());
            out[i * 8 + 4..i * 8 + 8].copy_from_slice(&place(Target::seg(*actor))?.to_be_bytes());
        }
        for id in self.order.iter().flatten() {
            let mut pos = start[id.0 as usize];
            for b in &self.segs[id.0 as usize].blocks {
                out[pos..pos + b.data.len()].copy_from_slice(&b.data);
                for p in &b.pointers {
                    let at = p.offset as usize;
                    if at + 4 > b.data.len() {
                        return err("a pointer lies outside its block");
                    }
                    out[pos + at..pos + at + 4].copy_from_slice(&place(p.target)?.to_be_bytes());
                }
                pos += b.size();
            }
        }
        Ok(out)
    }
}

/// True when libgarib knows the opcode and can read the command's arguments, and the command does not point to
/// data that `objbank-tool` cannot move (a matrix, a light, another display list).
fn movable_command(hi: u32, lo: u32) -> bool {
    let op = hi >> 24;
    let known = matches!(op, 0..=9 | 175..=192 | 200..=207 | 228..=240 | 242..=255);
    known
        && match op {
            0x01 | 0x03 | 0x06 | 0xB0 => false,
            0xBC => hi & 0xFF <= 0x0E && hi & 1 == 0,
            0xED => matches!(lo >> 24 & 0xF, 0 | 2 | 3),
            0xFD | 0xF5 => hi >> 21 & 7 <= 4,
            0xB2 => matches!(hi >> 16 & 0xFF, 0x10 | 0x14 | 0x18 | 0x1C),
            _ => true,
        }
}

/// A display list with the vertex data its commands load copied behind it, and each vertex pointer as (its offset
/// in the result, the offset of the data it points to). A range of vertices that two commands load is copied twice.
fn relocatable_display_list(data: &[u8], dl: &[u8]) -> Result<(Vec<u8>, Vec<(u32, u32)>), ObjError> {
    let mut out = dl.to_vec();
    let mut pointers = Vec::new();
    for (i, c) in dl.chunks_exact(8).enumerate() {
        let (hi, lo) = (u32_at(c, 0), u32_at(c, 4));
        if !movable_command(hi, lo) {
            return err(format!("a display list has a command (0x{:02X}) that cannot be moved", hi >> 24));
        }
        if hi >> 24 == 0x04 {
            let region = rd(data, lo, (hi & 0x3FF) as usize + 1, "the vertices of a display list")?;
            pointers.push((i as u32 * 8 + 4, out.len() as u32));
            out.extend_from_slice(region);
        }
    }
    Ok((out, pointers))
}

// ---------------------------------------------------------------------------------------------------------------
// Splitting and packing

/// A bank with one object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Single {
    /// The object's id
    pub id: u32,
    /// The root mesh's name. `objbank-tool split` names the file `0x<id, 8 hex digits, lower case>-<name>.obj.bin`.
    pub name: String,
    pub bytes: Vec<u8>,
}

fn plain(data: &[u8]) -> Result<Cow<'_, [u8]>, ObjError> {
    if fla2::is_fla2(data) {
        fla2::decompress(data).map(Cow::Owned).map_err(|e| ObjError(e.to_string()))
    } else {
        Ok(Cow::Borrowed(data))
    }
}

/// One bank for each object of `bank`, in the directory's order (`objbank-tool.py split`). `bank` may be
/// compressed.
///
/// `objbank-tool` writes an object's segments of one kind in the order of a Python set, which changes from run to
/// run. This writes them in the order `objbank-tool pack` writes them. Packing a file of `objbank-tool split`
/// alone gives these bytes.
pub fn split(bank: &[u8]) -> Result<Vec<Single>, ObjError> {
    let data = plain(bank)?;
    let bank = Bank::parse(&data)?;
    let mut out = Vec::with_capacity(bank.objects.len());
    for o in &bank.objects {
        let Some(name) = o.name() else {
            return err(format!("object 0x{:08X} has no mesh", o.dir_id));
        };
        let mut linker = Linker::new();
        let actor = linker.add_object(bank.data, o)?;
        linker.set_actor(o.dir_id, actor);
        out.push(Single { id: o.id, name: name.to_string(), bytes: linker.link()? });
    }
    Ok(out)
}

/// `base` with the objects of each bank of `singles` added, in order (`objbank-tool.py pack --output-file OUT BASE
/// single...`). An object that is in the result already is replaced and moves to the end. Every input may be
/// compressed. The result is not compressed.
pub fn pack<S: AsRef<[u8]>>(base: &[u8], singles: &[S]) -> Result<Vec<u8>, ObjError> {
    let mut datas = Vec::with_capacity(singles.len() + 1);
    datas.push(plain(base)?);
    for s in singles {
        datas.push(plain(s.as_ref())?);
    }
    let mut linker = Linker::new();
    for d in &datas {
        linker.add_bank(&Bank::parse(d)?)?;
    }
    linker.link()
}

/// The number of collision faces of the model in a single-object bank: the faces of its root mesh, which the game
/// turns into planes for a landscape actor. 0 when the bank does not have them (`backend.faces`).
pub fn faces(single: &[u8]) -> u32 {
    let u4 = |at: u32| single.get(at as usize..).and_then(|d| d.get(..4)).map(|d| u32_at(d, 0));
    let geometry = (|| {
        let obj = u4(4).filter(|p| *p != 0)?;
        let mesh = u4(obj.checked_add(OBJECT_MESH_PTR)?).filter(|p| *p != 0)?;
        u4(mesh.checked_add(MESH_GEOMETRY_PTR)?).filter(|p| *p != 0)
    })();
    geometry
        .and_then(|g| single.get(g as usize..)?.get(..2))
        .map_or(0, |d| u16_at(d, 0) as u32)
}

/// The vertices that the faces of the root mesh use, in the model's own units. The root mesh is the first mesh of
/// the bank's first object, the one the game calls `obj->mesh`. The vertices are the mesh's own: the game puts no
/// key frame of the mesh on them when it builds a platform's collision (`InitPlatformInfo`). A vertex that several
/// faces use is given once. None when the bank cannot be read. Empty when the root mesh has no faces.
pub fn root_face_vertices(single: &[u8]) -> Option<Vec<[f32; 3]>> {
    let bank = Bank::parse(single).ok()?;
    let root = bank.objects.first()?.root()?;
    let Some(geo) = &root.geometry else { return Some(Vec::new()) };
    if geo.num_faces == 0 {
        return Some(Vec::new());
    }
    let (faces, vertices) = (geo.faces?, geo.vertices?);
    let mut used = vec![false; geo.num_vertices as usize];
    for f in 0..geo.num_faces as usize * 3 {
        let v = faces.get(f * 2..f * 2 + 2).map(|d| u16_at(d, 0) as usize)?;
        *used.get_mut(v)? = true;
    }
    let vertex = |v: usize| vertices.get(v * VERTEX..(v + 1) * VERTEX).map(|d| [0, 4, 8].map(|k| f32_at(d, k)));
    (0..used.len()).filter(|v| used[*v]).map(vertex).collect()
}

// ---------------------------------------------------------------------------------------------------------------
// The model

/// A material of libgarib's glTF export, before it becomes a `model::Material`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct MatKey {
    texture: Option<u32>,
    clamp_s: bool,
    clamp_t: bool,
    mirror_s: bool,
    mirror_t: bool,
}

impl MatKey {
    fn wrap(clamp: bool, mirror: bool) -> Wrap {
        if clamp {
            Wrap::Clamp
        } else if mirror {
            Wrap::Mirror
        } else {
            Wrap::Repeat
        }
    }
}

/// The primitive of `key` in `prims`, added at the end when it is new.
fn prim_of(prims: &mut Vec<(MatKey, Primitive)>, key: MatKey) -> &mut Primitive {
    let at = match prims.iter().position(|(k, _)| *k == key) {
        Some(at) => at,
        None => {
            prims.push((key, Primitive::default()));
            prims.len() - 1
        }
    };
    &mut prims[at].1
}

/// A unit vector along `n`, or straight up when `n` has no length. The division is done in `f64`, as libgarib does.
fn unit(n: [f64; 3]) -> [f32; 3] {
    let mag = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if mag == 0.0 {
        [0.0, 1.0, 0.0]
    } else {
        [(n[0] / mag) as f32, (n[1] / mag) as f32, (n[2] / mag) as f32]
    }
}

/// A vertex in the display list's vertex cache: position, texture coordinates in 1/32 of a texel, and four bytes
/// that are a normal or a colour.
#[derive(Clone, Copy, Default)]
struct DlVertex {
    pos: [i16; 3],
    uv: [i16; 2],
    cn: [u8; 4],
}

/// The triangles a display list draws, grouped by material in the order the materials first draw
/// (`display_lists.f3dex_to_prims`).
fn display_list_prims(data: &[u8], dl: &[u8], lit: bool) -> Result<Vec<(MatKey, Primitive)>, ObjError> {
    let mut cache = [DlVertex::default(); 32];
    let mut material = MatKey::default();
    // The last texel of the tile, not its size. libgarib divides by this.
    let mut tile = (1.0f64, 1.0f64);
    let mut prims: Vec<(MatKey, Primitive)> = Vec::new();
    let i16_at = |d: &[u8], at: usize| u16_at(d, at) as i16;
    for c in dl.chunks_exact(8) {
        let (hi, lo) = (u32_at(c, 0), u32_at(c, 4));
        let mut triangle = |corners: [u32; 3]| -> Result<(), ObjError> {
            let p = prim_of(&mut prims, material);
            for corner in corners {
                let Some(v) = cache.get(corner as usize) else {
                    return err("a display list draws a vertex outside its cache");
                };
                if tile.0 == 0.0 || tile.1 == 0.0 {
                    return err("a display list draws with a tile of no size");
                }
                p.indices.push(p.positions.len() as u32);
                p.positions.push([v.pos[0] as f32, v.pos[1] as f32, v.pos[2] as f32]);
                p.uvs.push([
                    (f64::from(v.uv[0]) / 32.0 / tile.0) as f32,
                    (f64::from(v.uv[1]) / 32.0 / tile.1) as f32,
                ]);
                if lit {
                    p.normals.push(unit([0, 1, 2].map(|k| f64::from(v.cn[k] as i8) / 128.0)));
                } else {
                    let ch = |k: usize| (f64::from(v.cn[k]) / 255.0) as f32;
                    p.colors.push([ch(0), ch(1), ch(2), 1.0]);
                }
            }
            Ok(())
        };
        match hi >> 24 {
            // Load vertices into the cache
            0x04 => {
                let first = (hi >> 16 & 0xFF) / 2;
                for i in 0..(hi >> 10 & 0x3F) {
                    let at = lo as u64 + u64::from(i) * DL_VERTEX as u64;
                    let raw = u32::try_from(at)
                        .ok()
                        .and_then(|at| rd(data, at, DL_VERTEX, "the vertices of a display list").ok());
                    let (Some(raw), Some(slot)) = (raw, cache.get_mut((first + i) as usize)) else {
                        return err("a display list loads vertices that are not there");
                    };
                    *slot = DlVertex {
                        pos: [i16_at(raw, 0), i16_at(raw, 2), i16_at(raw, 4)],
                        uv: [i16_at(raw, 8), i16_at(raw, 10)],
                        cn: [raw[12], raw[13], raw[14], raw[15]],
                    };
                }
            }
            // Change one vertex of the cache
            0xB2 => {
                let Some(v) = cache.get_mut(((hi & 0xFFFF) / 2) as usize) else {
                    return err("a display list changes a vertex outside its cache");
                };
                match hi >> 16 & 0xFF {
                    0x10 => v.cn = lo.to_be_bytes(),
                    0x14 => v.uv = [(lo >> 16) as i16, lo as i16],
                    0x18 => (v.pos[0], v.pos[1]) = ((lo >> 16) as i16, lo as i16),
                    0x1C => v.pos[2] = (lo >> 16) as i16,
                    _ => return err("a display list changes a vertex in an unknown way"),
                }
            }
            0xBF => triangle([lo >> 16 & 0xFF, lo >> 8 & 0xFF, lo & 0xFF].map(|v| v / 2))?,
            0xB1 => {
                triangle([hi >> 16 & 0xFF, hi >> 8 & 0xFF, hi & 0xFF].map(|v| v / 2))?;
                triangle([lo >> 16 & 0xFF, lo >> 8 & 0xFF, lo & 0xFF].map(|v| v / 2))?;
            }
            0xB8 => break,
            // Set the texture image. The address is the texture's id.
            0xFD => {
                if hi >> 21 & 7 > 4 {
                    return err("a display list sets an image of an unknown format");
                }
                material.texture = Some(lo);
            }
            // Set the tile: how the texture wraps
            0xF5 => {
                if hi >> 21 & 7 > 4 {
                    return err("a display list sets a tile of an unknown format");
                }
                material.clamp_t = lo >> 19 & 1 == 1;
                material.mirror_t = lo >> 18 & 1 == 1;
                material.clamp_s = lo >> 9 & 1 == 1;
                material.mirror_s = lo >> 8 & 1 == 1;
            }
            0xF2 => tile = (f64::from(lo >> 12 & 0xFFF) / 4.0, f64::from(lo & 0xFFF) / 4.0),
            // Geometry mode, culling, the sync commands, texture loads and the other mode: nothing to draw
            0xB7 | 0xB6 | 0xBE | 0xE6 | 0xE8 | 0xE7 | 0xF3 | 0xF0 | 0xBA => {}
            op => return err(format!("a display list has a command (0x{op:02X}) the editor cannot draw")),
        }
    }
    Ok(prims)
}

/// The triangles of a mesh without a display list, grouped by texture in the order the textures first appear
/// (`objects.mesh_geo_to_prims`). libgarib exports these meshes with every texture coordinate at 0; so does this.
fn geometry_prims(
    geo: &Geometry,
    textures: &dyn Fn(u32) -> Option<(u16, u16)>,
) -> Result<Vec<(MatKey, Primitive)>, ObjError> {
    let mut prims: Vec<(MatKey, Primitive)> = Vec::new();
    let mut sizes: Vec<Option<(u16, u16)>> = Vec::new();
    let (Some(faces), Some(vertices)) = (geo.faces, geo.vertices) else {
        return err("a mesh has faces but no face or vertex data");
    };
    for f in 0..geo.num_faces as usize {
        let key = MatKey { texture: geo.texture_ids.map(|t| u32_at(t, f * 4)), ..MatKey::default() };
        let at = match prims.iter().position(|(k, _)| *k == key) {
            Some(at) => at,
            None => {
                let size = match key.texture {
                    Some(id) => match textures(id) {
                        Some(size) => Some(size),
                        None => return err(format!("Need dimensions of texture 0x{id:08X}")),
                    },
                    None => None,
                };
                sizes.push(size);
                prims.push((key, Primitive::default()));
                prims.len() - 1
            }
        };
        let p = &mut prims[at].1;
        for corner in 0..3 {
            let v = u16_at(faces, f * FACE + corner * 2) as usize;
            if v >= geo.num_vertices as usize {
                return err("a face uses a vertex that is not there");
            }
            p.indices.push(p.positions.len() as u32);
            p.positions.push([0, 4, 8].map(|k| f32_at(vertices, v * VERTEX + k)));
            if let Some(cn) = geo.vertex_cn {
                let ch = |k: usize| (f64::from(cn[v * 4 + k]) / 255.0) as f32;
                p.colors.push([ch(0), ch(1), ch(2), 1.0]);
            }
        }
        if geo.uvs.is_some() {
            let Some((w, h)) = sizes[at] else {
                return err("a mesh has texture coordinates but no texture");
            };
            // 0 divided by the texture's size
            let zero = |size: u16| if size == 0 { f32::NAN } else { 0.0 };
            p.uvs.extend([[zero(w), zero(h)]; 3]);
        }
        if let Some(cn) = geo.face_cn {
            let n = unit([0, 1, 2].map(|k| f64::from(cn[f * 4 + k] as i8)));
            p.normals.extend([n; 3]);
        }
    }
    Ok(prims)
}

/// The model of one parsed object, as libgarib's glTF export with a scale of 1 gives it. `data` is the bank the
/// object was parsed from. `textures` gives the size of a texture the level's texture banks hold, or none.
///
/// An error means libgarib does not export the object, and the editor has no model for it: a mesh without
/// a display list uses a texture that `textures` does not know, or the object lacks a part the export needs.
pub fn object_model(
    data: &[u8],
    obj: &Object,
    textures: &dyn Fn(u32) -> Option<(u16, u16)>,
) -> Result<Model, ObjError> {
    let mut model = Model::default();
    let mut root = Node::new(format!("0x{:08X}", obj.id));
    root.inherit_scale = true;
    model.nodes.push(root);
    if obj.meshes.is_empty() {
        return err(format!("object 0x{:08X} has no mesh", obj.id));
    }
    if obj.animation.is_none() {
        return err(format!("object 0x{:08X} has no animation block", obj.id));
    }
    // libgarib visits a mesh, then its siblings, then its children.
    let mut todo: Vec<(u32, u32)> = vec![(0, 0)];
    while let Some((index, parent)) = todo.pop() {
        let Some(mesh) = obj.meshes.get(index as usize) else { continue };
        let node = mesh_node(data, mesh, parent, textures, &mut model)?;
        if let Some(child) = mesh.child {
            todo.push((child, node));
        }
        if let Some(sibling) = mesh.sibling {
            todo.push((sibling, parent));
        }
    }
    Ok(model)
}

fn first_key(track: Option<&[u8]>, mesh: &Mesh) -> Result<Key, ObjError> {
    match track.and_then(|t| key_at(t, 0)) {
        Some(k) => Ok(k),
        None => err(format!("the mesh {} has a track without a key", mesh.name)),
    }
}

/// Adds the node of a mesh, its glTF mesh and its sprites to the model (`objects.mesh_to_gltf`).
fn mesh_node(
    data: &[u8],
    mesh: &Mesh,
    parent: u32,
    textures: &dyn Fn(u32) -> Option<(u16, u16)>,
    model: &mut Model,
) -> Result<u32, ObjError> {
    let mode = mesh.render_mode;
    let mut prims = if let Some(dl) = mesh.display_list {
        let prims = display_list_prims(data, dl, mode & render_mode::UNLIT == 0)?;
        // libgarib also copies the display list into the glTF file, and fails when it cannot.
        relocatable_display_list(data, dl)?;
        prims
    } else {
        match &mesh.geometry {
            None => return err(format!("the mesh {} has no display list and no geometry", mesh.name)),
            Some(g) if g.num_faces > 0 => geometry_prims(g, textures)?,
            Some(_) => Vec::new(),
        }
    };
    let mesh_index = if prims.is_empty() {
        None
    } else {
        // Untextured primitives first, then by texture id. The sort keeps the order of equal ids.
        prims.sort_by_key(|(k, _)| (k.texture.is_some(), k.texture.unwrap_or(0)));
        let mut out = ModelMesh { name: mesh.name.to_string(), primitives: Vec::with_capacity(prims.len()), billboard: false };
        for (key, mut p) in prims {
            p.material = model.materials.len() as u32;
            model.materials.push(Material {
                texture: key.texture,
                base_color: [1.0; 4],
                alpha_mode: if mode & render_mode::XLU != 0 {
                    AlphaMode::Blend
                } else if mode & render_mode::MASKED != 0 {
                    AlphaMode::Mask
                } else {
                    AlphaMode::Opaque
                },
                alpha_cutoff: 0.5,
                double_sided: false,
                unlit: mode & render_mode::UNLIT != 0,
                // An untextured material has no sampler in the glTF file.
                wrap_s: MatKey::wrap(key.clamp_s && key.texture.is_some(), key.mirror_s && key.texture.is_some()),
                wrap_t: MatKey::wrap(key.clamp_t && key.texture.is_some(), key.mirror_t && key.texture.is_some()),
            });
            out.primitives.push(p);
        }
        model.meshes.push(out);
        Some(model.meshes.len() as u32 - 1)
    };
    let (t, r, s) = (
        first_key(mesh.translation, mesh)?,
        first_key(mesh.rotation, mesh)?,
        first_key(mesh.scale, mesh)?,
    );
    let index = model.nodes.len() as u32;
    let mut node = Node::new(mesh.name.to_string());
    node.parent = Some(parent);
    node.mesh = mesh_index;
    node.translation = [t.v[0], t.v[1], t.v[2]].map(f64::from);
    node.rotation = r.v.map(f64::from);
    node.scale = [s.v[0], s.v[1], s.v[2]].map(f64::from);
    node.inherit_scale = parent != 0;
    model.nodes[parent as usize].children.push(index);
    model.nodes.push(node);
    for (i, sprite) in mesh.sprite_list().enumerate() {
        let name = format!("{}_sprite_{}", mesh.name, i);
        let sprite_index = model.nodes.len() as u32;
        let mut node = Node::new(name.clone());
        node.parent = Some(index);
        node.mesh = Some(model.meshes.len() as u32);
        node.translation = [sprite.x, sprite.y, sprite.z].map(f64::from);
        node.scale = [1.0, f64::from(sprite.height) / 3.0, f64::from(sprite.width) / 3.0];
        node.inherit_scale = false;
        node.inherit_rotation = false;
        model.nodes[index as usize].children.push(sprite_index);
        model.nodes.push(node);
        // A square in the YZ plane, two triangles
        let corners = [(-0.5, 0.5), (-0.5, -0.5), (0.5, 0.5), (-0.5, -0.5), (0.5, -0.5), (0.5, 0.5)];
        let uvs = [[1.0, 0.0], [0.0, 0.0], [1.0, 1.0], [0.0, 0.0], [0.0, 1.0], [1.0, 1.0]];
        model.meshes.push(ModelMesh {
            name,
            billboard: true,
            primitives: vec![Primitive {
                material: model.materials.len() as u32,
                positions: corners.iter().map(|&(y, z)| [0.0, y, z]).collect(),
                normals: Vec::new(),
                uvs: uvs.to_vec(),
                colors: Vec::new(),
                indices: (0..6).collect(),
            }],
        });
        model.materials.push(Material {
            texture: Some(sprite.texture_id),
            base_color: [1.0; 4],
            alpha_mode: AlphaMode::Mask,
            alpha_cutoff: 0.5,
            double_sided: true,
            unlit: true,
            wrap_s: Wrap::Clamp,
            wrap_t: Wrap::Clamp,
        });
    }
    Ok(index)
}

/// The model of the object `object_id` of a parsed bank. See `object_model`.
pub fn model(
    bank: &Bank,
    object_id: u32,
    textures: &dyn Fn(u32) -> Option<(u16, u16)>,
) -> Result<Model, ObjError> {
    match bank.object(object_id) {
        Some(obj) => object_model(bank.data, obj, textures),
        None => err(format!("the bank has no object 0x{object_id:08X}")),
    }
}

/// The ids of the textures an object's meshes and sprites use. `textures` gives the
/// size of a texture that some texture bank of the game holds, or none.
///
/// The set is empty when libgarib does not export the object with those textures (see `object_model`): the
/// ids are those of the exported model. A display list or a sprite may name a texture that no
/// bank holds; that id is in the set.
pub fn texture_ids(
    bank: &Bank,
    object_id: u32,
    textures: &dyn Fn(u32) -> Option<(u16, u16)>,
) -> BTreeSet<u32> {
    match model(bank, object_id, textures) {
        Ok(m) => m.materials.iter().filter_map(|m| m.texture).collect(),
        Err(_) => BTreeSet::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes made-up banks through the linker.
    struct Maker {
        linker: Linker<'static>,
    }

    fn key(v: [f32; 4], t: u32) -> Vec<u8> {
        let mut k = Vec::new();
        for x in v {
            k.extend_from_slice(&x.to_be_bytes());
        }
        k.extend_from_slice(&t.to_be_bytes());
        k
    }

    fn mesh_raw(name: &str, render_mode: u16, sprites: u32) -> Vec<u8> {
        let mut m = vec![0u8; MESH];
        m[0..4].copy_from_slice(&0x1234u32.to_be_bytes());
        m[4..4 + name.len()].copy_from_slice(name.as_bytes());
        m[14..16].copy_from_slice(&1u16.to_be_bytes());
        m[16..18].copy_from_slice(&1u16.to_be_bytes());
        m[18..20].copy_from_slice(&1u16.to_be_bytes());
        m[40..44].copy_from_slice(&sprites.to_be_bytes());
        m[50..52].copy_from_slice(&render_mode.to_be_bytes());
        m
    }

    fn cmd(hi: u32, lo: u32) -> [u8; 8] {
        let mut c = [0u8; 8];
        c[..4].copy_from_slice(&hi.to_be_bytes());
        c[4..].copy_from_slice(&lo.to_be_bytes());
        c
    }

    fn dl_vertex(pos: [i16; 3], uv: [i16; 2], cn: [u8; 4]) -> Vec<u8> {
        let mut v = Vec::new();
        for p in pos {
            v.extend_from_slice(&p.to_be_bytes());
        }
        v.extend_from_slice(&[0, 0]);
        for p in uv {
            v.extend_from_slice(&p.to_be_bytes());
        }
        v.extend_from_slice(&cn);
        v
    }

    impl Maker {
        fn new() -> Maker {
            Maker { linker: Linker::new() }
        }

        fn keys(&mut self, mesh: SegId, t: [f32; 3], s: [f32; 3]) {
            let l = &mut self.linker;
            let tr = l.bytes(Kind::Keyframes, key([t[0], t[1], t[2], 0.0], 0));
            l.point(mesh, 0, MESH_TRANSLATION_PTR, Target::seg(tr));
            let ro = l.bytes(Kind::Keyframes, key([0.0, 0.0, 0.0, 1.0], 0));
            l.point(mesh, 0, MESH_ROTATION_PTR, Target::seg(ro));
            let sc = l.bytes(Kind::Keyframes, key([s[0], s[1], s[2], 0.0], 0));
            l.point(mesh, 0, MESH_SCALE_PTR, Target::seg(sc));
        }

        /// A mesh with a display list: one textured triangle, then two triangles of another texture.
        fn dl_mesh(&mut self, name: &str, render_mode: u16) -> SegId {
            let mesh = self.linker.bytes(Kind::Mesh, mesh_raw(name, render_mode, 0));
            let mut verts = Vec::new();
            for (i, p) in [[0, 0, 0], [10, 0, 0], [0, 10, 0], [0, 0, 10]].into_iter().enumerate() {
                verts.extend(dl_vertex(p, [32 * 31, 32 * 62], [0, 0x7F, 0, 0xFF + 0 * i as u8]));
            }
            let mut cmds = Vec::new();
            cmds.extend(cmd(0xFD10_0000, 0xBBBB_0002));
            cmds.extend(cmd(0xF510_0000, 1 << 19 | 1 << 8));
            cmds.extend(cmd(0xF200_0000, (31 * 4) << 12 | (31 * 4)));
            cmds.extend(cmd(0x0400_0000 | 4 << 10 | 63, 0));
            cmds.extend(cmd(0xBF00_0000, 0 << 16 | 2 << 8 | 4));
            cmds.extend(cmd(0xFD10_0000, 0xAAAA_0001));
            cmds.extend(cmd(0xB100_0204, 0x0002_0406));
            cmds.extend(cmd(0xB800_0000, 0));
            let seg = SegId(self.linker.segs.len() as u32);
            let mut cmd_block = Block::new(cmds);
            cmd_block.pointers.push(Pointer { offset: 3 * 8 + 4, target: Target::block(seg, 1) });
            self.linker.add(Kind::DisplayList, vec![cmd_block, Block::new(verts)]);
            self.linker.point(mesh, 0, MESH_DISPLAY_LIST_PTR, Target::seg(seg));
            self.keys(mesh, [1.0, 2.0, 3.0], [2.0, 2.0, 2.0]);
            mesh
        }

        /// A mesh without a display list: two faces, each with its own texture, and a sprite.
        fn geo_mesh(&mut self, name: &str) -> SegId {
            let mesh = self.linker.bytes(Kind::Mesh, mesh_raw(name, render_mode::MASKED, 1));
            let mut root = vec![0u8; GEOMETRY];
            root[0..2].copy_from_slice(&2u16.to_be_bytes());
            root[2..4].copy_from_slice(&4u16.to_be_bytes());
            let mut verts = Vec::new();
            for v in [[0.0f32, 0.0, 0.0], [1.5, 0.0, 0.0], [0.0, 1.5, 0.0], [0.0, 0.0, 1.5]] {
                for x in v {
                    verts.extend_from_slice(&x.to_be_bytes());
                }
            }
            let mut faces = Vec::new();
            for i in [0u16, 1, 2, 0, 2, 3] {
                faces.extend_from_slice(&i.to_be_bytes());
            }
            let mut ids = Vec::new();
            ids.extend_from_slice(&0xCCCC_0003u32.to_be_bytes());
            ids.extend_from_slice(&0xAAAA_0001u32.to_be_bytes());
            let geo = self.linker.add_geometry(GeometryParts {
                root: root.into(),
                verts: Some(verts.into()),
                faces: Some(faces.into()),
                face_cn: Some(vec![0, 0, 0x40, 0, 0, 0, 0, 0].into()),
                texture_ids: Some(ids.into()),
                ..GeometryParts::default()
            });
            self.linker.point(mesh, 0, MESH_GEOMETRY_PTR, geo);
            let mut sprite = vec![0u8; SPRITE];
            sprite[0..4].copy_from_slice(&0xDDDD_0004u32.to_be_bytes());
            sprite[8..10].copy_from_slice(&7u16.to_be_bytes());
            sprite[14..16].copy_from_slice(&30u16.to_be_bytes());
            sprite[16..18].copy_from_slice(&10u16.to_be_bytes());
            let sprites = self.linker.bytes(Kind::Sprites, sprite);
            self.linker.point(mesh, 0, MESH_SPRITES_PTR, Target::seg(sprites));
            self.keys(mesh, [0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
            mesh
        }

        fn actor(&mut self, id: u32, root: SegId) {
            let mut raw = vec![0u8; OBJECT];
            raw[0..4].copy_from_slice(&id.to_be_bytes());
            let actor = self.linker.bytes(Kind::Actor, raw);
            let mut props = vec![0u8; ANIMATION];
            props[0..2].copy_from_slice(&1u16.to_be_bytes());
            let props = self.linker.bytes(Kind::AnimProps, props);
            let defs = self.linker.bytes(Kind::AnimDefs, vec![0u8; ANIMATION_DEFINITION]);
            self.linker.point(props, 0, ANIMATION_DEFINITIONS_PTR, Target::seg(defs));
            self.linker.point(actor, 0, OBJECT_ANIMATION_PTR, Target::seg(props));
            self.linker.point(actor, 0, OBJECT_MESH_PTR, Target::seg(root));
            self.linker.set_actor(id, actor);
        }
    }

    /// Two objects: 0x11 has a display-list mesh with a geometry child and a display-list sibling; 0x22 has one mesh.
    fn two_objects() -> Vec<u8> {
        let mut m = Maker::new();
        let root = m.dl_mesh("root", 0);
        let child = m.geo_mesh("child");
        let sibling = m.dl_mesh("sib", render_mode::UNLIT | render_mode::XLU);
        m.linker.point(root, 0, MESH_CHILD_PTR, Target::seg(child));
        m.linker.point(root, 0, MESH_SIBLING_PTR, Target::seg(sibling));
        m.actor(0x11, root);
        let other = m.dl_mesh("other", 0);
        m.actor(0x22, other);
        m.linker.link().unwrap()
    }

    fn all(id: u32) -> Option<(u16, u16)> {
        (id != 0xCCCC_0003).then_some((32, 32))
    }

    #[test]
    fn parse_and_names() {
        let data = two_objects();
        assert_eq!(data.len() % 8, 0);
        let bank = Bank::parse(&data).unwrap();
        assert_eq!(bank.objects.len(), 2);
        let o = &bank.objects[0];
        assert_eq!((o.dir_id, o.id, o.name()), (0x11, 0x11, Some("root")));
        let names: Vec<&str> = o.meshes.iter().map(|m| m.name).collect();
        assert_eq!(names, ["root", "child", "sib"]);
        assert_eq!((o.meshes[0].child, o.meshes[0].sibling), (Some(1), Some(2)));
        assert_eq!(o.meshes[0].display_list.unwrap().len(), 8 * 8);
        assert_eq!(o.meshes[1].geometry.as_ref().unwrap().num_faces, 2);
        assert_eq!(o.meshes[1].sprite_list().next().unwrap().texture_id, 0xDDDD_0004);
        assert_eq!(Mesh::keys(o.meshes[0].translation).next().unwrap().v, [1.0, 2.0, 3.0, 0.0]);
        assert_eq!(bank.object(0x22).unwrap().name(), Some("other"));
    }

    #[test]
    fn split_pack_round_trip() {
        let data = two_objects();
        let singles = split(&data).unwrap();
        assert_eq!(singles.len(), 2);
        assert_eq!((singles[0].id, singles[0].name.as_str()), (0x11, "root"));
        // A single relinked alone does not change, and its model is the model in the full bank.
        for s in &singles {
            assert_eq!(pack(&s.bytes, &[] as &[&[u8]]).unwrap(), s.bytes);
            assert_eq!(split(&s.bytes).unwrap()[0].bytes, s.bytes);
            let (a, b) = (Bank::parse(&data).unwrap(), Bank::parse(&s.bytes).unwrap());
            assert_eq!(model(&a, s.id, &all).ok(), model(&b, s.id, &all).ok());
        }
        // Packing both singles into an empty bank gives the relinked full bank.
        let empty = [0u8; 8];
        let both = pack(&empty, &[&singles[0].bytes, &singles[1].bytes]).unwrap();
        assert_eq!(both, pack(&data, &[] as &[&[u8]]).unwrap());
        // An object that is in the bank already is replaced and moves to the end of the directory.
        let moved = pack(&data, &[&singles[0].bytes]).unwrap();
        assert_eq!(moved.len(), both.len());
        let bank = Bank::parse(&moved).unwrap();
        assert_eq!(bank.objects.iter().map(|o| o.id).collect::<Vec<_>>(), [0x22, 0x11]);
        assert_eq!(split(&moved).unwrap()[1].bytes, singles[0].bytes);
        // Compressed inputs are read too.
        assert_eq!(pack(&fla2::compress(&data), &[fla2::compress(&singles[0].bytes)]).unwrap(), moved);
    }

    #[test]
    fn faces_of_single() {
        let data = two_objects();
        let mut m = Maker::new();
        let root = m.geo_mesh("geo");
        m.actor(0x33, root);
        let geo = m.linker.link().unwrap();
        assert_eq!(faces(&geo), 2);
        assert_eq!(faces(&split(&data).unwrap()[0].bytes), 0);
        assert_eq!(faces(&[]), 0);
        assert_eq!(faces(&[0, 0, 0, 1, 0xFF, 0xFF, 0xFF, 0xF0]), 0);
        // Both faces use vertex 0 and vertex 2: each is given once
        assert_eq!(root_face_vertices(&geo).unwrap(), [[0.0, 0.0, 0.0], [1.5, 0.0, 0.0], [0.0, 1.5, 0.0], [0.0, 0.0, 1.5]]);
        assert_eq!(root_face_vertices(&split(&data).unwrap()[0].bytes), Some(Vec::new()));
        assert_eq!(root_face_vertices(&[]), None);
    }

    #[test]
    fn model_of_object() {
        let data = two_objects();
        let bank = Bank::parse(&data).unwrap();
        // The geometry mesh uses a texture that is not known: no model, and no texture ids.
        assert!(model(&bank, 0x11, &all).is_err());
        assert!(texture_ids(&bank, 0x11, &all).is_empty());
        assert!(model(&bank, 0x99, &all).is_err());
        let every = |_: u32| Some((32u16, 32u16));
        let m = model(&bank, 0x11, &every).unwrap();
        let names: Vec<&str> = m.nodes.iter().map(|n| n.name.as_str()).collect();
        // The sibling comes before the child and its sprite.
        assert_eq!(names, ["0x00000011", "root", "sib", "child", "child_sprite_0"]);
        assert_eq!(m.nodes[0].children, [1, 2]);
        assert_eq!(m.nodes[1].children, [3]);
        assert_eq!(m.nodes[3].children, [4]);
        assert_eq!(m.nodes[3].parent, Some(1));
        assert_eq!(m.nodes[1].translation, [1.0, 2.0, 3.0]);
        assert_eq!(m.nodes[1].scale, [2.0, 2.0, 2.0]);
        assert_eq!(m.nodes[1].rotation, [0.0, 0.0, 0.0, 1.0]);
        assert!(!m.nodes[1].inherit_scale && m.nodes[3].inherit_scale && !m.nodes[4].inherit_scale);
        assert_eq!(m.nodes[4].translation, [7.0, 0.0, 0.0]);
        assert_eq!(m.nodes[4].scale, [1.0, 10.0 / 3.0, 10.0]);
        assert_eq!(m.mesh_nodes(), 4);
        assert_eq!(m.triangles(), 3 + 3 + 2 + 2);
        // The root's primitives are sorted by texture id: the two triangles of 0xAAAA0001 first.
        let root = &m.meshes[m.nodes[1].mesh.unwrap() as usize];
        assert_eq!(root.primitives.len(), 2);
        let (a, b) = (&root.primitives[0], &root.primitives[1]);
        assert_eq!((a.triangles(), b.triangles()), (2, 1));
        let (ma, mb) = (&m.materials[a.material as usize], &m.materials[b.material as usize]);
        assert_eq!((ma.texture, mb.texture), (Some(0xAAAA_0001), Some(0xBBBB_0002)));
        assert_eq!((mb.wrap_s, mb.wrap_t, mb.alpha_mode), (Wrap::Mirror, Wrap::Clamp, AlphaMode::Opaque));
        assert_eq!(b.positions, [[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0]]);
        assert_eq!(b.uvs[0], [1.0, 2.0]);
        assert_eq!(b.normals[0], [0.0, 1.0, 0.0]);
        assert!(b.colors.is_empty());
        assert_eq!(a.indices, [0, 1, 2, 3, 4, 5]);
        assert_eq!(a.positions[3..], [[10.0, 0.0, 0.0], [0.0, 10.0, 0.0], [0.0, 0.0, 10.0]]);
        // The unlit sibling has colours and no normals.
        let sib = &m.meshes[m.nodes[2].mesh.unwrap() as usize];
        assert!(sib.primitives[0].normals.is_empty());
        assert_eq!(sib.primitives[0].colors[0], [0.0, 127.0 / 255.0, 0.0, 1.0]);
        let ms = &m.materials[sib.primitives[0].material as usize];
        assert!(ms.unlit && ms.alpha_mode == AlphaMode::Blend);
        // The geometry mesh: sorted by texture id, face normals, no texture coordinates.
        let child = &m.meshes[m.nodes[3].mesh.unwrap() as usize];
        let tex: Vec<_> = child.primitives.iter().map(|p| m.materials[p.material as usize].texture).collect();
        assert_eq!(tex, [Some(0xAAAA_0001), Some(0xCCCC_0003)]);
        assert_eq!(child.primitives[1].positions, [[0.0, 0.0, 0.0], [1.5, 0.0, 0.0], [0.0, 1.5, 0.0]]);
        assert_eq!(child.primitives[1].normals, [[0.0, 0.0, 1.0]; 3]);
        assert_eq!(child.primitives[0].normals, [[0.0, 1.0, 0.0]; 3]);
        assert!(child.primitives[0].uvs.is_empty());
        assert_eq!(m.materials[child.primitives[0].material as usize].alpha_mode, AlphaMode::Mask);
        let sprite = &m.meshes[m.nodes[4].mesh.unwrap() as usize];
        assert!(sprite.billboard && sprite.primitives[0].positions.len() == 6);
        let ids: Vec<u32> = texture_ids(&bank, 0x11, &every).into_iter().collect();
        assert_eq!(ids, [0xAAAA_0001, 0xBBBB_0002, 0xCCCC_0003, 0xDDDD_0004]);
        assert_eq!(m.texture_ids(), ids);
        let b = m.bounds().unwrap();
        assert_eq!(b.min[0], 1.0);
    }

    #[test]
    fn malformed_never_panics() {
        let data = two_objects();
        assert!(Bank::parse(&[]).is_err());
        assert!(Bank::parse(&[0, 0, 0, 1]).is_err());
        assert!(Bank::parse(&[0, 0, 0, 1, 0, 0, 0, 8]).is_err());
        assert!(split(b"FLA2\x10\0\0\0").is_err());
        // A mesh that is its own child
        let bank = Bank::parse(&data).unwrap();
        let mesh_at = u32_at(bank.objects[1].raw, OBJECT_MESH_PTR as usize);
        let mut looped = data.clone();
        let at = mesh_at as usize + MESH_CHILD_PTR as usize;
        looped[at..at + 4].copy_from_slice(&mesh_at.to_be_bytes());
        assert!(Bank::parse(&looped).is_err());
        // Every cut and a spread of changed bytes: an error or a result, never a panic
        for cut in (0..data.len()).step_by(7) {
            let _ = split(&data[..cut]);
        }
        let every = |_: u32| Some((32u16, 32u16));
        let mut state = 12345u32;
        for _ in 0..4000 {
            let mut bad = data.clone();
            for _ in 0..3 {
                state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                let at = (state >> 8) as usize % bad.len();
                state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                bad[at] = (state >> 16) as u8;
            }
            let _ = split(&bad);
            let _ = pack(&data, &[&bad]);
            let _ = faces(&bad);
            if let Ok(bank) = Bank::parse(&bad) {
                for o in &bank.objects {
                    if let Ok(m) = object_model(bank.data, o, &every) {
                        let _ = (m.bounds(), m.draw_nodes(), m.texture_ids());
                    }
                }
            }
        }
    }

    #[test]
    fn display_list_rules() {
        // A display list that calls another one cannot be moved or drawn.
        assert!(!movable_command(0x0600_0000, 0));
        assert!(movable_command(0xFC00_0000, 0));
        assert!(!movable_command(0xC100_0000, 0));
        assert!(!movable_command(0xFDE0_0000, 0));
        assert!(!movable_command(0xB211_0000, 0));
        assert!(movable_command(0xBC00_0006, 0) && !movable_command(0xBC00_0007, 0));
        let dl = [cmd(0xFC00_0000, 0), cmd(0xB800_0000, 0)].concat();
        assert!(display_list_prims(&[], &dl, true).is_err());
        assert!(relocatable_display_list(&[], &dl).is_ok());
        // A triangle drawn with a tile of no size
        let dl = [cmd(0xF200_0000, 0), cmd(0xBF00_0000, 0), cmd(0xB800_0000, 0)].concat();
        assert!(display_list_prims(&[], &dl, true).is_err());
        // Vertices past the end of the bank, and past the 32 places of the cache
        let dl = [cmd(0x0400_0000 | 1 << 10 | 15, 64), cmd(0xB800_0000, 0)].concat();
        assert!(display_list_prims(&[0; 64], &dl, true).is_err());
        assert!(relocatable_display_list(&[0; 64], &dl).is_err());
        let dl = [cmd(0x0400_0000 | 62 << 16 | 2 << 10 | 31, 0), cmd(0xB800_0000, 0)].concat();
        assert!(display_list_prims(&[0; 64], &dl, true).is_err());
        // A changed vertex: new texture coordinates and a new colour
        let dl = [
            cmd(0x0400_0000 | 3 << 10 | 47, 0),
            cmd(0xB214_0002, 64 << 16 | 32),
            cmd(0xB210_0004, 0xFF00_0000),
            cmd(0xBF00_0000, 0 << 16 | 2 << 8 | 4),
            cmd(0xB800_0000, 0),
        ]
        .concat();
        let prims = display_list_prims(&[0; 48], &dl, false).unwrap();
        assert_eq!(prims[0].0.texture, None);
        assert_eq!(prims[0].1.uvs, [[0.0, 0.0], [2.0, 1.0], [0.0, 0.0]]);
        assert_eq!(prims[0].1.colors[2], [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn linker_delete_and_missing_target() {
        let mut m = Maker::new();
        let a = m.dl_mesh("a", 0);
        m.actor(1, a);
        let b = m.dl_mesh("b", 0);
        m.actor(2, b);
        let both = m.linker.link().unwrap();
        m.linker.delete(1);
        assert!(!m.linker.has_actor(1) && m.linker.len() == 1);
        let one = m.linker.link().unwrap();
        assert!(one.len() < both.len());
        assert_eq!(Bank::parse(&one).unwrap().objects[0].name(), Some("b"));
        // A pointer to a deleted segment is an error.
        m.linker.point(b, 0, MESH_CHILD_PTR, Target::seg(a));
        assert!(m.linker.link().is_err());
    }
}
