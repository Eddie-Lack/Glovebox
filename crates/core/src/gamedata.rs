//! The game's data, read from the player's ROM and kept in memory: the levels, the object and texture banks, the
//! catalog of every model, and what each level has loaded. This is what `core.py` read from `data/` and `cache/`.
//! Nothing here writes a file.
//!
//! An object id or a texture id is a `u32`. As text it is `0x` and 8 upper-case hex digits (`hex`, `parse_id`).

use crate::fla2;
use crate::level;
use crate::model::Model;
use crate::objbank::{self, Bank, Single};
use crate::rom::{Key, Rom};
use crate::tables;
use crate::texbank::{self, TexBank};
use crate::xml::Doc;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

/// Why the game's data can't be read or used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameError(pub String);

impl std::fmt::Display for GameError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for GameError {}

fn err<T>(text: impl Into<String>) -> Result<T, GameError> {
    Err(GameError(text.into()))
}

/// An id as the editor writes it: `0x73DB3DE3`.
pub fn hex(id: u32) -> String {
    format!("0x{id:08X}")
}

/// The id that `text` names. Only `0x` and 8 hex digits in upper case is an id.
pub fn parse_id(text: &str) -> Option<u32> {
    let digits = text.strip_prefix("0x")?;
    let ok = digits.len() == 8 && digits.bytes().all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(&b));
    if ok {
        u32::from_str_radix(digits, 16).ok()
    } else {
        None
    }
}

/// The part of a bank's file name before the first full stop: `HUB_PART1` for `HUB_PART1.obj.fla`.
pub fn stem(file: &str) -> &str {
    file.split('.').next().unwrap_or(file)
}

/// One level of the scene table (`scenes.json`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scene {
    /// Two hex digits, `00` to `2f`
    pub id: String,
    /// The level's number in the game
    pub index: usize,
    pub name: String,
    pub group: String,
    /// The level file's name in noclip's data set
    pub landscape: String,
    /// The banks noclip loads, as file names (`GENERIC.obj.fla`)
    pub object_banks: Vec<String>,
    pub texture_banks: Vec<String>,
    /// The banks the game loads (LoadLevel), as names without an ending
    pub game_object_banks: Vec<String>,
    pub game_texture_banks: Vec<String>,
    /// The banks both lists have, as file names
    pub sure_object_banks: Vec<String>,
    pub sure_texture_banks: Vec<String>,
    /// The banks a build copies imported models and their textures into
    pub import_object_bank: String,
    pub import_texture_bank: String,
}

impl Scene {
    /// The scene's texture banks joined as the Python app named its model folders: `GENERIC+HUB`.
    pub fn texture_tag(&self) -> String {
        let worlds: Vec<&str> = self.texture_banks.iter().map(|t| world_of(t)).collect();
        worlds.join("+")
    }
}

/// `HUB` for `HUB_TEX_BANK.tex.fla`.
fn world_of(tex_bank: &str) -> &str {
    tex_bank.split("_TEX").next().unwrap_or(tex_bank)
}

/// The scene table, in the game's order.
pub fn scenes() -> &'static [Scene] {
    static T: OnceLock<Vec<Scene>> = OnceLock::new();
    T.get_or_init(|| {
        let text = |v: &Value| v.as_str().unwrap_or("").to_string();
        let list = |v: &Value| v.as_array().map(|a| a.iter().map(text).collect()).unwrap_or_default();
        let all = tables::scenes().as_array().map(|a| a.as_slice()).unwrap_or(&[]);
        all.iter()
            .map(|s| Scene {
                id: text(&s["id"]),
                index: s["index"].as_u64().unwrap_or(0) as usize,
                name: text(&s["name"]),
                group: text(&s["group"]),
                landscape: text(&s["landscape"]),
                object_banks: list(&s["objectBanks"]),
                texture_banks: list(&s["textureBanks"]),
                game_object_banks: list(&s["gameObjectBanks"]),
                game_texture_banks: list(&s["gameTextureBanks"]),
                sure_object_banks: list(&s["sureObjectBanks"]),
                sure_texture_banks: list(&s["sureTextureBanks"]),
                import_object_bank: text(&s["importObjectBank"]),
                import_texture_bank: text(&s["importTextureBank"]),
            })
            .collect()
    })
}

/// A scene by its id.
pub fn scene(sid: &str) -> Result<&'static Scene, GameError> {
    match scenes().iter().find(|s| s.id == sid) {
        Some(s) => Ok(s),
        None => err(format!("no level {sid}")),
    }
}

/// A number of the compiled-in bank number table: `kind` is `obj` or `tex`.
pub fn bank_number(kind: &str, name: &str) -> Option<i64> {
    tables::bank_numbers()[kind][name].as_i64()
}

/// One model of the catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameModel {
    /// The root mesh's name
    pub name: String,
    /// The object bank that holds it, without an ending
    pub bank: String,
    /// The size of its single-object bank
    pub bytes: usize,
    /// The world of the last texture bank of `first_scene`
    pub world: String,
    /// The first scene that loads the bank and can draw the model. `model(first_scene, id)` is the catalog's model.
    pub first_scene: String,
}

/// A texture's pixels: RGBA, 4 bytes for each pixel, rows from the top.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// The enemy template of one type (`enemy_templates.json`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnemyTemplate {
    pub enemy_type: String,
    /// The level the script is from
    pub level: String,
    /// True when only boss levels have the type
    pub boss: bool,
    pub cutscene_only: bool,
    /// The `<Enemy>` element on one line
    pub xml: String,
    /// Every level that has the type, sorted
    pub levels: Vec<String>,
}

struct ObjBank {
    /// The bank, not compressed
    data: Vec<u8>,
    singles: Vec<Single>,
    /// An index into `singles` for each object id. Of two objects with one id the later one is kept.
    by_id: HashMap<u32, usize>,
}

struct TexBankData {
    /// The file name in noclip's data set: `HUB_TEX_BANK.tex.fla`
    file: String,
    data: Vec<u8>,
    ids: BTreeSet<u32>,
    sizes: HashMap<u32, (u16, u16)>,
}

type Sizes = HashMap<u32, (u16, u16)>;

/// The game's data. Share it between threads behind an `Arc`.
pub struct GameData {
    rom_bytes: Vec<u8>,
    rom: Rom,
    /// By name without an ending. Every bank the scene table names.
    obj: BTreeMap<String, ObjBank>,
    /// Sorted by file name, as `core.ALL_TEX_BANKS`.
    tex: Vec<TexBankData>,
    /// The size of every texture of every bank. A later bank wins.
    all_sizes: Sizes,
    catalog: OnceLock<BTreeMap<u32, GameModel>>,
    object_textures: Mutex<HashMap<u32, Arc<BTreeSet<u32>>>>,
    /// By the scene's texture tag, the bank and the object id. None for an object that has no model.
    models: Mutex<HashMap<(String, String, u32), Option<Arc<Model>>>>,
    scene_sizes: Mutex<HashMap<String, Arc<Sizes>>>,
    backdrops: OnceLock<Vec<u32>>,
    templates: OnceLock<Vec<EnemyTemplate>>,
}

/// Runs `f` on every item, on up to 4 threads. The results are in the items' order.
fn par_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let threads = std::thread::available_parallelism().map_or(2, |n| n.get()).min(4).min(items.len().max(1));
    let mut parts: Vec<Vec<(usize, R)>> = std::thread::scope(|s| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                s.spawn(|| {
                    let mut out = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(item) = items.get(i) else { break };
                        out.push((i, f(item)));
                    }
                    out
                })
            })
            .collect();
        workers.into_iter().map(|w| w.join().unwrap_or_default()).collect()
    });
    let mut all: Vec<(usize, R)> = parts.drain(..).flatten().collect();
    all.sort_by_key(|(i, _)| *i);
    all.into_iter().map(|(_, r)| r).collect()
}

impl GameData {
    /// Reads the ROM (in .z64 byte order): splits it, decompresses the banks and splits each object bank into
    /// its objects.
    pub fn open(rom_bytes: Vec<u8>) -> Result<GameData, GameError> {
        let rom = Rom::parse(&rom_bytes).map_err(|e| GameError(e.0))?;
        let mut obj_names: BTreeSet<String> = BTreeSet::new();
        for s in scenes() {
            obj_names.extend(s.object_banks.iter().map(|b| stem(b).to_string()));
            obj_names.extend(s.game_object_banks.iter().cloned());
            obj_names.insert(s.import_object_bank.clone());
        }
        let obj_names: Vec<String> = obj_names.into_iter().collect();
        let mut tex_names: BTreeSet<String> = BTreeSet::new();
        for r in rom.regions() {
            if let (true, Some(name)) = (r.data_type() == "texbank", r.name()) {
                tex_names.insert(format!("{name}.tex.fla"));
            }
        }
        let tex_names: Vec<String> = tex_names.into_iter().collect();

        enum Job<'a> {
            Obj(&'a str),
            Tex(&'a str),
        }
        enum Done {
            Obj(ObjBank),
            Tex(TexBankData),
        }
        let jobs: Vec<Job> =
            obj_names.iter().map(|n| Job::Obj(n)).chain(tex_names.iter().map(|n| Job::Tex(n))).collect();
        let done = par_map(&jobs, |job| -> Result<Done, GameError> {
            match job {
                Job::Obj(name) => {
                    let Some(packed) = rom.object_bank(name) else {
                        return err(format!("the ROM has no object bank {name}"));
                    };
                    let data = fla2::data(packed).map_err(|e| GameError(format!("object bank {name}: {e}")))?;
                    let singles = objbank::split(&data).map_err(|e| GameError(format!("object bank {name}: {e}")))?;
                    let by_id = singles.iter().enumerate().map(|(i, s)| (s.id, i)).collect();
                    Ok(Done::Obj(ObjBank { data, singles, by_id }))
                }
                Job::Tex(file) => {
                    let name = stem(file);
                    let Some(packed) = rom.texture_bank(name) else {
                        return err(format!("the ROM has no texture bank {name}"));
                    };
                    let data = fla2::data(packed).map_err(|e| GameError(format!("texture bank {name}: {e}")))?;
                    let bank = TexBank::parse(&data).map_err(|e| GameError(format!("texture bank {name}: {e}")))?;
                    let ids = bank.textures.iter().map(|t| t.id).collect();
                    let sizes = bank.textures.iter().map(|t| (t.id, (t.width, t.height))).collect();
                    Ok(Done::Tex(TexBankData { file: file.to_string(), data, ids, sizes }))
                }
            }
        });
        let mut obj = BTreeMap::new();
        let mut tex = Vec::new();
        for (job, d) in jobs.iter().zip(done) {
            match (job, d?) {
                (Job::Obj(name), Done::Obj(b)) => {
                    obj.insert(name.to_string(), b);
                }
                (_, Done::Tex(b)) => tex.push(b),
                _ => {}
            }
        }
        let mut all_sizes = HashMap::new();
        for b in &tex {
            all_sizes.extend(b.sizes.iter().map(|(k, v)| (*k, *v)));
        }
        Ok(GameData {
            rom_bytes,
            rom,
            obj,
            tex,
            all_sizes,
            catalog: OnceLock::new(),
            object_textures: Mutex::new(HashMap::new()),
            models: Mutex::new(HashMap::new()),
            scene_sizes: Mutex::new(HashMap::new()),
            backdrops: OnceLock::new(),
            templates: OnceLock::new(),
        })
    }

    /// The ROM that was opened.
    pub fn rom_bytes(&self) -> &[u8] {
        &self.rom_bytes
    }

    /// The ROM split into regions. A build clones it and replaces regions.
    pub fn rom(&self) -> &Rom {
        &self.rom
    }

    // -- Levels

    /// A level file as the ROM has it.
    pub fn level_file(&self, sid: &str) -> Result<&[u8], GameError> {
        let s = scene(sid)?;
        match self.rom.level(s.index) {
            Some(lev) => Ok(lev),
            None => err(format!("the ROM has no level {sid}")),
        }
    }

    fn level_doc(&self, sid: &str) -> Result<Doc, GameError> {
        level::disassemble(self.level_file(sid)?).map_err(|e| GameError(format!("level {sid}: {}", e.0)))
    }

    /// A shipped level as XML text (`core.level_xml`).
    pub fn level_xml(&self, sid: &str) -> Result<String, GameError> {
        Ok(self.level_doc(sid)?.pretty_no_newline())
    }

    /// The size of a shipped level's file.
    pub fn orig_lev_size(&self, sid: &str) -> Option<usize> {
        self.level_file(sid).ok().map(|l| l.len())
    }

    // -- Banks

    fn obj_bank(&self, name: &str) -> Result<&ObjBank, GameError> {
        match self.obj.get(name) {
            Some(b) => Ok(b),
            None => err(format!("no object bank {name}")),
        }
    }

    fn tex_bank(&self, file: &str) -> Option<&TexBankData> {
        self.tex.iter().find(|b| b.file == file)
    }

    /// The file names of every texture bank, sorted (`core.ALL_TEX_BANKS`).
    pub fn all_texture_banks(&self) -> impl Iterator<Item = &str> {
        self.tex.iter().map(|b| b.file.as_str())
    }

    /// A bank's data, not compressed, by its file name in noclip's data set (`HUB_PART1.obj.fla`,
    /// `HUB_TEX_BANK.tex.fla`). `core.raw_bank` without the `custom:` banks.
    pub fn raw_bank(&self, file: &str) -> Result<&[u8], GameError> {
        if let Some(name) = file.strip_suffix(".tex.fla") {
            if let Some(b) = self.tex_bank(file) {
                return Ok(&b.data);
            }
            return err(format!("no texture bank {name}"));
        }
        match file.strip_suffix(".obj.fla") {
            Some(name) => Ok(&self.obj_bank(name)?.data),
            None => err(format!("{file} is not a bank's file name")),
        }
    }

    /// The region a build replaces for an object bank, by its name.
    pub fn object_bank_key(&self, name: &str) -> Option<Key> {
        self.rom.bank("objbank", name).map(|r| r.key().clone())
    }

    /// The region a build replaces for a texture bank, by its name. For LEVELEND_TEX_BANK it is bank 1, as in the
    /// Python app, while `raw_bank` gives bank 14.
    pub fn texture_bank_key(&self, name: &str) -> Option<Key> {
        self.rom.bank("texbank", name).map(|r| r.key().clone())
    }

    /// The ids of a texture bank's textures, by its file name. Empty for a bank the ROM does not have.
    pub fn texture_ids(&self, file: &str) -> &BTreeSet<u32> {
        static NONE: BTreeSet<u32> = BTreeSet::new();
        self.tex_bank(file).map_or(&NONE, |b| &b.ids)
    }

    /// The single-object banks of an object bank, by its name: each object's id, root mesh name and bank
    /// (`core.split_bank`).
    pub fn split_bank(&self, name: &str) -> Result<&[Single], GameError> {
        Ok(&self.obj_bank(name)?.singles)
    }

    fn single(&self, bank: &str, id: u32) -> Option<&Single> {
        let b = self.obj.get(bank)?;
        b.by_id.get(&id).map(|i| &b.singles[*i])
    }

    /// A catalog model's single-object bank (`core.model_bank` for the game's models).
    pub fn model_bank(&self, id: u32) -> Option<&[u8]> {
        let entry = self.catalog().get(&id)?;
        self.single(&entry.bank, id).map(|s| s.bytes.as_slice())
    }

    /// The number of collision faces of a catalog model, or 0.
    pub fn faces(&self, id: u32) -> u32 {
        self.model_bank(id).map_or(0, objbank::faces)
    }

    /// The objects the game is sure to have loaded for a level: those of the banks that both the game and noclip
    /// list.
    pub fn loaded_ids(&self, sid: &str) -> Result<BTreeSet<u32>, GameError> {
        let mut ids = BTreeSet::new();
        for b in &scene(sid)?.sure_object_banks {
            ids.extend(self.obj_bank(stem(b))?.by_id.keys().copied());
        }
        Ok(ids)
    }

    // -- Models and textures

    /// The sizes of the textures of a scene's texture banks. A later bank wins.
    fn sizes_of(&self, s: &Scene) -> Arc<Sizes> {
        let tag = s.texture_tag();
        let mut cache = self.scene_sizes.lock().unwrap_or_else(|e| e.into_inner());
        cache
            .entry(tag)
            .or_insert_with(|| {
                let mut sizes = HashMap::new();
                for t in &s.texture_banks {
                    if let Some(b) = self.tex_bank(t) {
                        sizes.extend(b.sizes.iter().map(|(k, v)| (*k, *v)));
                    }
                }
                Arc::new(sizes)
            })
            .clone()
    }

    /// The model of one object of one bank with a scene's textures. None when the Python app had no model for it.
    fn bank_model(&self, s: &Scene, bank: &str, id: u32, sizes: &Sizes) -> Option<Arc<Model>> {
        let key = (s.texture_tag(), bank.to_string(), id);
        if let Some(m) = self.models.lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
            return m.clone();
        }
        let built = self.single(bank, id).and_then(|single| {
            let parsed = Bank::parse(&single.bytes).ok()?;
            objbank::model(&parsed, id, &|t| sizes.get(&t).copied()).ok().map(Arc::new)
        });
        self.models.lock().unwrap_or_else(|e| e.into_inner()).insert(key, built.clone());
        built
    }

    /// The model of an object in a scene, built with the scene's texture banks. When several of the scene's object
    /// banks have the object, the last one that gives a model wins.
    pub fn model(&self, sid: &str, id: u32) -> Result<Option<Arc<Model>>, GameError> {
        let s = scene(sid)?;
        let sizes = self.sizes_of(s);
        Ok(s.object_banks.iter().rev().find_map(|b| self.bank_model(s, stem(b), id, &sizes)))
    }

    /// Every object a scene can draw, with its model (`core.model_index`).
    pub fn model_index(&self, sid: &str) -> Result<BTreeMap<u32, Arc<Model>>, GameError> {
        let s = scene(sid)?;
        let sizes = self.sizes_of(s);
        let mut jobs: Vec<(&str, u32)> = Vec::new();
        for b in &s.object_banks {
            jobs.extend(self.obj_bank(stem(b))?.singles.iter().map(|single| (stem(b), single.id)));
        }
        let built = par_map(&jobs, |(bank, id)| self.bank_model(s, bank, *id, &sizes));
        let mut index = BTreeMap::new();
        for ((_, id), m) in jobs.iter().zip(built) {
            if let Some(m) = m {
                index.insert(*id, m);
            }
        }
        Ok(index)
    }

    /// A texture of a scene's texture banks. A later bank wins (`core.find_texture`).
    pub fn texture(&self, sid: &str, id: u32) -> Result<Option<Image>, GameError> {
        let s = scene(sid)?;
        for file in s.texture_banks.iter().rev() {
            let Some(b) = self.tex_bank(file) else { continue };
            if b.ids.contains(&id) {
                return decode(&b.data, id).map(Some);
            }
        }
        Ok(None)
    }

    /// A texture of one bank, by the bank's file name.
    pub fn bank_texture(&self, file: &str, id: u32) -> Result<Option<Image>, GameError> {
        match self.tex_bank(file) {
            Some(b) if b.ids.contains(&id) => decode(&b.data, id).map(Some),
            _ => Ok(None),
        }
    }

    /// Every model of the game's banks, by object id (`core.catalog`). A model is listed with the first scene
    /// that loads its bank and can draw it.
    pub fn catalog(&self) -> &BTreeMap<u32, GameModel> {
        self.catalog.get_or_init(|| {
            let mut cat: BTreeMap<u32, GameModel> = BTreeMap::new();
            for s in scenes() {
                let sizes = self.sizes_of(s);
                let world = s.texture_banks.last().map_or("", |t| world_of(t));
                // Objects the catalog lacks, with the bank that lists them first
                let mut jobs: Vec<(&str, &Single)> = Vec::new();
                for b in &s.object_banks {
                    let Some(bank) = self.obj.get(stem(b)) else { continue };
                    for (i, single) in bank.singles.iter().enumerate() {
                        let new = !cat.contains_key(&single.id) && !jobs.iter().any(|(_, j)| j.id == single.id);
                        if new && bank.by_id.get(&single.id) == Some(&i) {
                            jobs.push((stem(b), single));
                        }
                    }
                }
                let drawn = par_map(&jobs, |(_, single)| {
                    s.object_banks.iter().any(|b| self.bank_model(s, stem(b), single.id, &sizes).is_some())
                });
                for ((bank, single), ok) in jobs.iter().zip(drawn) {
                    if ok {
                        cat.insert(
                            single.id,
                            GameModel {
                                name: single.name.clone(),
                                bank: bank.to_string(),
                                bytes: single.bytes.len(),
                                world: world.to_string(),
                                first_scene: s.id.clone(),
                            },
                        );
                    }
                }
            }
            cat
        })
    }

    /// The textures a catalog model uses, read from its model built with every texture bank
    /// (`core.object_texture_ids` for the game's models). None for an object the catalog lacks.
    pub fn object_texture_ids(&self, id: u32) -> Option<Arc<BTreeSet<u32>>> {
        if let Some(ids) = self.object_textures.lock().unwrap_or_else(|e| e.into_inner()).get(&id) {
            return Some(ids.clone());
        }
        let single = self.model_bank(id)?;
        let ids = match Bank::parse(single) {
            Ok(bank) => objbank::texture_ids(&bank, id, &|t| self.all_sizes.get(&t).copied()),
            Err(_) => BTreeSet::new(),
        };
        let ids = Arc::new(ids);
        self.object_textures.lock().unwrap_or_else(|e| e.into_inner()).insert(id, ids.clone());
        Some(ids)
    }

    // -- What the shipped levels use

    /// The backdrop textures a level can show: the ids the shipped levels use as backdrops and that the level's
    /// texture banks hold, most used first.
    pub fn backdrops(&self, sid: &str) -> Result<Vec<u32>, GameError> {
        let s = scene(sid)?;
        let used = self.backdrops.get_or_init(|| {
            let per_level = par_map(scenes(), |other| {
                let Ok(doc) = self.level_doc(&other.id) else { return Vec::new() };
                let nodes = doc.iter_tags(doc.root(), &["Backdrop"]);
                nodes.iter().map(|n| doc.attr(*n, "texture_id").to_uppercase().replace("0X", "0x")).collect()
            });
            let mut count: Vec<(String, usize)> = Vec::new();
            for id in per_level.into_iter().flatten() {
                match count.iter_mut().find(|(k, _)| *k == id) {
                    Some((_, n)) => *n += 1,
                    None => count.push((id, 1)),
                }
            }
            // A stable sort keeps ids with the same count in the order they were first seen
            count.sort_by(|a, b| b.1.cmp(&a.1));
            count.iter().filter_map(|(id, _)| parse_id(id)).collect()
        });
        let have = |id: &u32| s.texture_banks.iter().any(|t| self.texture_ids(t).contains(id));
        Ok(used.iter().copied().filter(have).collect())
    }

    /// For each enemy type, the shortest script a shipped level gives it, as `prepare.py` wrote
    /// `enemy_templates.json`. The types are in the order the levels first use them.
    pub fn enemy_templates(&self) -> &[EnemyTemplate] {
        const CUTSCENES: [&str; 5] = ["2b", "2c", "2d", "2e", "2f"];
        const BOSSES: [&str; 8] = ["0d", "12", "17", "1c", "21", "26", "27", "28"];
        struct Found {
            cutscene: bool,
            size: usize,
            sid: String,
            xml: String,
        }
        self.templates.get_or_init(|| {
            let per_level = par_map(scenes(), |s| {
                let Ok(doc) = self.level_doc(&s.id) else { return Vec::new() };
                let enemies = doc.iter_tags(doc.root(), &["Enemy"]);
                let one = |e: &crate::xml::NodeId| {
                    let found = Found {
                        cutscene: CUTSCENES.contains(&s.id.as_str()),
                        size: doc.iter(*e).len(),
                        sid: s.id.clone(),
                        xml: doc.serialize_node(*e),
                    };
                    (doc.attr(*e, "type").to_string(), found)
                };
                enemies.iter().map(one).collect()
            });
            let mut types: Vec<(String, Vec<Found>)> = Vec::new();
            for (t, found) in per_level.into_iter().flatten() {
                match types.iter_mut().find(|(k, _)| *k == t) {
                    Some((_, list)) => list.push(found),
                    None => types.push((t, vec![found])),
                }
            }
            let template = |(t, mut list): (String, Vec<Found>)| {
                list.sort_by_key(|f| (f.cutscene, f.size));
                let levels: BTreeSet<String> = list.iter().map(|f| f.sid.clone()).collect();
                let boss = list.iter().all(|f| BOSSES.contains(&f.sid.as_str()));
                let first = list.swap_remove(0);
                EnemyTemplate {
                    enemy_type: t,
                    level: first.sid,
                    boss,
                    cutscene_only: first.cutscene,
                    xml: first.xml,
                    levels: levels.into_iter().collect(),
                }
            };
            types.into_iter().map(template).collect()
        })
    }

    /// `enemy_templates` as the JSON of `enemy_templates.json`.
    pub fn enemy_templates_json(&self) -> Value {
        let mut out = Map::new();
        for t in self.enemy_templates() {
            let entry = json!({"level": t.level, "boss": t.boss, "cutsceneOnly": t.cutscene_only, "xml": t.xml,
                               "levels": t.levels});
            out.insert(t.enemy_type.clone(), entry);
        }
        Value::Object(out)
    }
}

/// The pixels of one texture of a bank.
fn decode(bank: &[u8], id: u32) -> Result<Image, GameError> {
    let parsed = TexBank::parse(bank).map_err(|e| GameError(e.0))?;
    let Some(t) = parsed.get(id) else {
        return err(format!("the bank has no texture {}", hex(id)));
    };
    let rgba = t.decode().map_err(|e| GameError(e.0))?;
    Ok(Image { width: t.width as u32, height: t.height as u32, rgba })
}

/// The pixels of one texture of a texture bank that is not the game's (an imported model's bank).
pub fn texture_of(bank: &[u8], id: u32) -> Result<Image, GameError> {
    let data = fla2::data(bank).map_err(|e| GameError(e.to_string()))?;
    decode(&data, id)
}

/// The records of a texture bank by id, each padded to a multiple of 8 (`core.texbank_records`).
pub fn records_of(bank: &[u8]) -> Result<HashMap<u32, Vec<u8>>, GameError> {
    let recs = texbank::records(bank).map_err(|e| GameError(e.0))?;
    Ok(recs.records.into_iter().map(|(id, r)| (id, r.into_owned())).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_strict() {
        assert_eq!(parse_id("0x73DB3DE3"), Some(0x73DB3DE3));
        assert_eq!(hex(0x0A), "0x0000000A");
        for bad in ["0x73db3de3", "73DB3DE3", "0X73DB3DE3", "0x73DB3DE", "0x73DB3DE3 ", "0x+3DB3DE3", ""] {
            assert_eq!(parse_id(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_scene_table_is_whole() {
        assert_eq!(scenes().len(), 48);
        for (i, s) in scenes().iter().enumerate() {
            assert_eq!(s.index, i);
            assert_eq!(s.id, format!("{i:02x}"));
            assert!(!s.object_banks.is_empty() && !s.texture_banks.is_empty());
            assert!(bank_number("obj", &s.import_object_bank).is_some());
            assert!(bank_number("tex", &s.import_texture_bank).is_some());
        }
        assert!(scene("30").is_err());
        assert_eq!(stem("HUB_PART1.obj.fla"), "HUB_PART1");
    }

    #[test]
    fn a_file_that_is_not_the_rom_is_refused() {
        assert!(GameData::open(vec![0; 4096]).is_err());
        assert!(GameData::open(Vec::new()).is_err());
    }
}
