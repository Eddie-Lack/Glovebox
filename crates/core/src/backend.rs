//! What the editor's window asks its back end for: the tables, the levels, the models and textures, plans, builds
//! and level packs, the author's imported models, the kit, and where the ROM is. `Backend` is the list of those
//! methods. `GameBackend` answers from the player's ROM (`GameData`) and the user folder.
//!
//! A model is named by a `ModelRef`. `model` and `texture` give the model and the pixels.

use crate::build::{self, BuildAllRequest, BuildRequest, Built};
use crate::custom;
use crate::gamedata::{self, hex, parse_id, GameData, Image};
use crate::imports::{self, Customs, Plan};
use crate::kit;
use crate::level;
use crate::model::Model;
use crate::objbank::{self, Bank};
use crate::pack;
use crate::tables;
use crate::texbank::TexBank;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// A message for the author.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendError(pub String);

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for BackendError {}

impl From<String> for BackendError {
    fn from(text: String) -> Self {
        BackendError(text)
    }
}

impl From<gamedata::GameError> for BackendError {
    fn from(e: gamedata::GameError) -> Self {
        BackendError(e.0)
    }
}

pub type Result<T> = std::result::Result<T, BackendError>;

/// The name of a model. Ids are text: `0x73DB3DE3`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ModelRef {
    /// A model of the game, drawn with the texture banks of a scene
    Scene { scene: String, id: String },
    /// A piece of an imported model, drawn with the textures of its set
    Custom { set: String, id: String },
}

/// What only an imported model's catalog entry has.
#[derive(Debug, Clone, PartialEq)]
pub struct CustomEntry {
    /// The set's id
    pub set: String,
    /// The piece's name in the game
    pub piece: String,
    pub offset: [f64; 3],
    /// The ids of the piece's textures
    pub textures: Vec<String>,
    /// `custom:<set>`
    pub tex_bank: String,
    /// The retail model a reskinned model was made from
    pub reskin_from: Option<String>,
}

/// One model of the catalog.
#[derive(Debug, Clone, PartialEq)]
pub struct CatalogEntry {
    pub name: String,
    /// The object bank's name, or `Custom: ` and the model's name
    pub bank: String,
    /// The size of the model's single-object bank
    pub bytes: u64,
    /// The world whose textures the model is drawn with. None for an imported model.
    pub world: Option<String>,
    /// The first scene that can draw it. None for an imported model.
    pub first_scene: Option<String>,
    /// None when there's no model to draw
    pub model: Option<ModelRef>,
    pub custom: Option<CustomEntry>,
}

/// The models of a scene.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SceneModels {
    /// Every object the scene can draw, by id
    pub models: BTreeMap<String, ModelRef>,
    /// The ids of the objects the game is sure to have loaded, sorted
    pub loaded: Vec<String>,
}

/// Facts about the Expansion Pak game mod.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ModInfo {
    pub heap_extra_bytes: u64,
    pub mod_bytes: u64,
}

/// The editor's back end. A build runs on a worker thread, so an implementation is shared between threads.
pub trait Backend: Send + Sync {
    // -- Tables

    /// The scene table: a list of objects as in `scenes.json`.
    fn scenes(&self) -> &Value;
    /// The level format joined with the decompilation's names (`decomp.json`).
    fn decomp(&self) -> &Value;
    /// Which model each enemy and powerup type uses (`noclip_tables.json`).
    fn tables(&self) -> &Value;
    /// A script for each enemy type (`enemy_templates.json`), or an empty object.
    fn templates(&self) -> &Value;
    /// Free heap for each level (`heap.json`), or an empty object.
    fn heap(&self) -> &Value;
    fn mod_info(&self) -> ModInfo;

    // -- Levels and models

    /// A shipped level as XML text.
    fn level_xml(&self, sid: &str) -> Result<String>;
    /// The backdrop textures a level can show, most used first.
    fn backdrops(&self, sid: &str) -> Result<Vec<String>>;
    /// The number of collision faces of a model, or 0.
    fn faces(&self, oid: &str) -> u32;
    /// The vertices that the faces of a model's root mesh use, in the model's own units. The game measures a
    /// platform's radius on them. None when the back end can't tell.
    fn root_vertices(&self, oid: &str) -> Option<Vec<[f32; 3]>>;
    fn models(&self, sid: &str) -> Result<SceneModels>;
    /// The size of a shipped level's file.
    fn orig_lev_size(&self, sid: &str) -> Option<usize>;
    /// Every model of the game's banks and the author's imported models, by id.
    fn catalog(&self) -> Result<BTreeMap<String, CatalogEntry>>;
    fn model(&self, model: &ModelRef) -> Result<Arc<Model>>;
    /// The pixels of a texture that `model` uses.
    fn texture(&self, model: &ModelRef, texture_id: u32) -> Result<Arc<Image>>;
    /// The problems of a level's XML (at most 5), or none.
    fn validate(&self, xml: &str) -> Vec<String>;
    /// What a build adds for these imports.
    fn plan(&self, sid: &str, ids: &[String]) -> Result<Plan>;
    fn loaded_ids(&self, sid: &str) -> Result<BTreeSet<String>>;

    // -- Building

    fn build(&self, request: &BuildRequest) -> Result<Built>;
    fn build_all(&self, request: &BuildAllRequest) -> Result<Built>;
    /// A level pack for g64coopdx of the level `sid` with this XML, named `name`, with the models it imports
    /// (`pack::export`).
    fn level_pack(&self, sid: &str, xml: &str, name: &str) -> Result<pack::LevelPack>;

    // -- Imported models and the kit

    /// Converts a glTF model into a new set of imported models. Returns the set's `meta.json` with its id as `set`.
    fn import_custom(&self, data: &[u8], name: &str, scale: f64) -> Result<Value>;
    fn delete_custom(&self, set: &str) -> Result<()>;
    /// A copy of a retail model with textures from other worlds. Returns the copy's object id.
    fn reskin(&self, oid: &str, worlds: &[String], seed: i64) -> Result<String>;
    fn kit_items(&self) -> Result<Value>;
    /// A PNG picture of a kit model.
    fn kit_preview(&self, item: &str, params: &Value) -> Result<Vec<u8>>;
    fn make_kit(&self, item: &str, params: &Value) -> Result<Value>;
    /// Where the player's ROM is.
    fn rom_path(&self) -> Option<PathBuf>;
}

/// The back end over the player's ROM and the user folder.
pub struct GameBackend {
    game: Arc<GameData>,
    home: PathBuf,
    rom_path: Option<PathBuf>,
    templates: OnceLock<Value>,
    /// Decoded textures by the bank's name and the texture id
    textures: Mutex<BTreeMap<(String, u32), Arc<Image>>>,
    /// Kit pictures by the item and its settings
    kit_previews: Mutex<HashMap<String, Vec<u8>>>,
    /// One model is imported at a time
    custom_lock: Mutex<()>,
}

impl GameBackend {
    /// `home` is the user folder. Imported models are in its `custom` folder.
    pub fn new(game: Arc<GameData>, home: impl Into<PathBuf>, rom_path: Option<PathBuf>) -> GameBackend {
        GameBackend {
            game,
            home: home.into(),
            rom_path,
            templates: OnceLock::new(),
            textures: Mutex::new(BTreeMap::new()),
            kit_previews: Mutex::new(HashMap::new()),
            custom_lock: Mutex::new(()),
        }
    }

    /// Reads the ROM file and opens the back end on it.
    pub fn open(rom_path: &Path, home: impl Into<PathBuf>) -> Result<GameBackend> {
        let file = std::fs::read(rom_path).map_err(|e| BackendError(format!("{}: {e}", rom_path.display())))?;
        let rom = crate::n64::supported_rom(&file).map_err(|e| BackendError(e.to_string()))?;
        Ok(GameBackend::new(Arc::new(GameData::open(rom)?), home, Some(rom_path.to_path_buf())))
    }

    pub fn game(&self) -> &Arc<GameData> {
        &self.game
    }

    /// The author's imported models, read from the user folder.
    pub fn customs(&self) -> Customs {
        imports::load_customs(&self.home)
    }

    /// Converts a glTF model and returns the set's `meta.json` with its id as `set`.
    fn import(&self, data: &[u8], name: &str, scale: f64) -> Result<Value> {
        let taken_ids: HashSet<u32> = self.game.catalog().keys().copied().collect();
        let mut taken_textures: HashSet<u32> = HashSet::new();
        for bank in self.game.all_texture_banks() {
            taken_textures.extend(self.game.texture_ids(bank));
        }
        let _guard = self.custom_lock.lock().unwrap_or_else(|e| e.into_inner());
        let set = custom::import_custom(&self.home, data, name, scale, &taken_ids, &taken_textures)
            .map_err(|e| BackendError(e.0))?;
        let meta = custom::custom_sets(&self.home).remove(&set);
        let Some(Value::Object(meta)) = meta else {
            return Err(BackendError("The imported model couldn't be read back".into()));
        };
        let mut out = serde_json::Map::new();
        out.insert("set".into(), Value::String(set));
        out.extend(meta);
        Ok(Value::Object(out))
    }

    fn cached_texture(&self, bank: String, id: u32, load: impl FnOnce() -> Result<Option<Image>>) -> Result<Arc<Image>> {
        let key = (bank, id);
        if let Some(image) = self.textures.lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
            return Ok(image.clone());
        }
        let Some(image) = load()? else {
            return Err(BackendError(format!("texture not in this level's banks: {}", hex(id))));
        };
        let image = Arc::new(image);
        self.textures.lock().unwrap_or_else(|e| e.into_inner()).insert(key, image.clone());
        Ok(image)
    }
}

impl Backend for GameBackend {
    fn scenes(&self) -> &Value {
        tables::scenes()
    }

    fn decomp(&self) -> &Value {
        tables::decomp()
    }

    fn tables(&self) -> &Value {
        tables::noclip_tables()
    }

    fn templates(&self) -> &Value {
        self.templates.get_or_init(|| self.game.enemy_templates_json())
    }

    fn heap(&self) -> &Value {
        tables::heap()
    }

    fn mod_info(&self) -> ModInfo {
        let info = tables::mod_info();
        ModInfo {
            heap_extra_bytes: info["heap_extra_bytes"].as_u64().unwrap_or(0),
            mod_bytes: info["mod_bytes"].as_u64().unwrap_or(0),
        }
    }

    fn level_xml(&self, sid: &str) -> Result<String> {
        Ok(self.game.level_xml(sid)?)
    }

    fn backdrops(&self, sid: &str) -> Result<Vec<String>> {
        Ok(self.game.backdrops(sid)?.into_iter().map(hex).collect())
    }

    fn faces(&self, oid: &str) -> u32 {
        match self.customs().get(oid) {
            Some(c) => objbank::faces(&c.bank),
            None => parse_id(oid).map_or(0, |id| self.game.faces(id)),
        }
    }

    fn root_vertices(&self, oid: &str) -> Option<Vec<[f32; 3]>> {
        match self.customs().get(oid) {
            Some(c) => objbank::root_face_vertices(&c.bank),
            None => parse_id(oid).and_then(|id| self.game.root_vertices(id)),
        }
    }

    fn models(&self, sid: &str) -> Result<SceneModels> {
        let index = self.game.model_index(sid)?;
        let model = |id: &u32| (hex(*id), ModelRef::Scene { scene: sid.to_string(), id: hex(*id) });
        Ok(SceneModels {
            models: index.keys().map(model).collect(),
            loaded: self.game.loaded_ids(sid)?.into_iter().map(hex).collect(),
        })
    }

    fn orig_lev_size(&self, sid: &str) -> Option<usize> {
        self.game.orig_lev_size(sid)
    }

    fn catalog(&self) -> Result<BTreeMap<String, CatalogEntry>> {
        let mut out = BTreeMap::new();
        for (id, m) in self.game.catalog() {
            let entry = CatalogEntry {
                name: m.name.clone(),
                bank: m.bank.clone(),
                bytes: m.bytes as u64,
                world: Some(m.world.clone()),
                first_scene: Some(m.first_scene.clone()),
                model: Some(ModelRef::Scene { scene: m.first_scene.clone(), id: hex(*id) }),
                custom: None,
            };
            out.insert(hex(*id), entry);
        }
        for (id, c) in self.customs().models {
            let entry = CatalogEntry {
                name: c.name.clone(),
                bank: format!("Custom: {}", c.set_name),
                bytes: c.bytes,
                world: None,
                first_scene: None,
                model: Some(ModelRef::Custom { set: c.set.clone(), id: id.clone() }),
                custom: Some(CustomEntry {
                    tex_bank: c.tex_bank(),
                    set: c.set,
                    piece: c.piece,
                    offset: c.offset,
                    textures: c.textures,
                    reskin_from: c.reskin_from,
                }),
            };
            out.insert(id, entry);
        }
        Ok(out)
    }

    fn model(&self, model: &ModelRef) -> Result<Arc<Model>> {
        let missing = |id: &str| BackendError(format!("no model {id}"));
        match model {
            ModelRef::Scene { scene, id } => {
                let oid = parse_id(id).ok_or_else(|| missing(id))?;
                self.game.model(scene, oid)?.ok_or_else(|| missing(id))
            }
            ModelRef::Custom { set, id } => {
                let customs = self.customs();
                let c = customs.get(id).filter(|c| c.set == *set).ok_or_else(|| missing(id))?;
                let oid = parse_id(id).ok_or_else(|| missing(id))?;
                let plain = |d: &[u8]| crate::fla2::data(d).map_err(|e| BackendError(e.to_string()));
                let tex = match customs.tex_banks.get(set) {
                    Some(t) => plain(t)?,
                    None => Vec::new(),
                };
                let sizes: BTreeMap<u32, (u16, u16)> = match TexBank::parse(&tex) {
                    Ok(bank) => bank.textures.iter().map(|t| (t.id, (t.width, t.height))).collect(),
                    Err(_) => BTreeMap::new(),
                };
                let data = plain(&c.bank)?;
                let bank = Bank::parse(&data).map_err(|e| BackendError(e.0))?;
                let built = objbank::model(&bank, oid, &|t| sizes.get(&t).copied()).map_err(|e| BackendError(e.0))?;
                Ok(Arc::new(built))
            }
        }
    }

    fn texture(&self, model: &ModelRef, texture_id: u32) -> Result<Arc<Image>> {
        match model {
            ModelRef::Scene { scene, .. } => {
                let tag = gamedata::scene(scene)?.texture_tag();
                self.cached_texture(tag, texture_id, || Ok(self.game.texture(scene, texture_id)?))
            }
            ModelRef::Custom { set, .. } => self.cached_texture(format!("custom:{set}"), texture_id, || {
                match self.customs().tex_banks.get(set) {
                    Some(bank) => Ok(Some(gamedata::texture_of(bank, texture_id)?)),
                    None => Ok(None),
                }
            }),
        }
    }

    fn validate(&self, xml: &str) -> Vec<String> {
        match level::parse_level_xml(xml.as_bytes()) {
            Ok(doc) => level::validate(&doc).into_iter().take(5).collect(),
            Err(e) => vec![e.0],
        }
    }

    fn plan(&self, sid: &str, ids: &[String]) -> Result<Plan> {
        let customs = self.customs();
        let mut plan = match ids.is_empty() {
            true => imports::plan_imports(&self.game, &customs, sid, ids)?,
            false => imports::plan_runtime(&self.game, &customs, sid, ids)?,
        };
        plan.tex_bytes = Some(imports::tex_bytes(&self.game, &customs, &plan.textures)?);
        Ok(plan)
    }

    fn loaded_ids(&self, sid: &str) -> Result<BTreeSet<String>> {
        Ok(self.game.loaded_ids(sid)?.into_iter().map(hex).collect())
    }

    fn build(&self, request: &BuildRequest) -> Result<Built> {
        Ok(build::build(&self.game, &self.customs(), request)?)
    }

    fn build_all(&self, request: &BuildAllRequest) -> Result<Built> {
        Ok(build::build_all(&self.game, &self.customs(), request)?)
    }

    fn level_pack(&self, sid: &str, xml: &str, name: &str) -> Result<pack::LevelPack> {
        Ok(pack::export(&self.game, &self.customs(), sid, name, xml.as_bytes())?)
    }

    fn import_custom(&self, data: &[u8], name: &str, scale: f64) -> Result<Value> {
        if data.len() > custom::MAX_MODEL {
            return Err(BackendError(format!("The model is larger than {} MB", custom::MAX_MODEL >> 20)));
        }
        let words: Vec<&str> = name.split_whitespace().collect();
        let name: String = words.join(" ").chars().take(40).collect();
        self.import(data, if name.is_empty() { "Model" } else { &name }, scale)
    }

    fn delete_custom(&self, set: &str) -> Result<()> {
        let _guard = self.custom_lock.lock().unwrap_or_else(|e| e.into_inner());
        if !custom::delete_custom(&self.home, set) {
            return Err(BackendError("no such model".into()));
        }
        self.textures.lock().unwrap_or_else(|e| e.into_inner()).retain(|(bank, _), _| *bank != format!("custom:{set}"));
        Ok(())
    }

    fn reskin(&self, oid: &str, worlds: &[String], seed: i64) -> Result<String> {
        let unknown = || BackendError(format!("no model {oid}"));
        let id = parse_id(oid).ok_or_else(unknown)?;
        let entry = self.game.catalog().get(&id).ok_or_else(unknown)?;
        let bank = self.game.model_bank(id).ok_or_else(unknown)?;
        let mut worlds: Vec<&str> = worlds.iter().map(|w| w.as_str()).collect();
        worlds.sort_unstable();
        let textures: Vec<String> =
            self.game.object_texture_ids(id).iter().flat_map(|s| s.iter()).map(|t| hex(*t)).collect();
        let pool: Vec<String> = self.game.reskin_pool(&worlds).into_iter().map(hex).collect();
        let taken_ids: HashSet<u32> = self.game.catalog().keys().copied().collect();
        let model = custom::Reskin {
            oid,
            name: &entry.name,
            world: &entry.world,
            bank,
            textures: &textures,
            pool: &pool,
            taken_ids: &taken_ids,
        };
        let record = |t: &str| parse_id(t).and_then(|t| self.game.texture_record(t));
        let _guard = self.custom_lock.lock().unwrap_or_else(|e| e.into_inner());
        let set = custom::reskin(&self.home, &model, &worlds, seed, &record).map_err(|e| BackendError(e.0))?;
        let made = custom::custom_catalog(&self.home).into_iter().find(|c| c.custom == set);
        made.map(|c| c.id).ok_or_else(|| BackendError("The reskinned model couldn't be read back".into()))
    }

    fn kit_items(&self) -> Result<Value> {
        Ok(kit::items())
    }

    fn kit_preview(&self, item: &str, params: &Value) -> Result<Vec<u8>> {
        // The kit takes every setting as text
        let mut texts = kit::Params::new();
        for (k, v) in params.as_object().into_iter().flatten() {
            let text = match v {
                Value::String(s) => s.clone(),
                Value::Bool(b) => if *b { "True" } else { "False" }.to_string(),
                Value::Number(n) if n.is_f64() => crate::pyfmt::repr(n.as_f64().unwrap_or(0.0)),
                other => other.to_string(),
            };
            texts.insert(k.clone(), Value::String(text));
        }
        let clean = kit::clean(item, &texts)?;
        let key = format!("{item} {}", Value::Object(clean.clone()));
        if let Some(png) = self.kit_previews.lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
            return Ok(png.clone());
        }
        let png = kit::preview_png(item, &clean, 128)?;
        let mut cache = self.kit_previews.lock().unwrap_or_else(|e| e.into_inner());
        if cache.len() > 200 {
            cache.clear();
        }
        cache.insert(key, png.clone());
        Ok(png)
    }

    fn make_kit(&self, item: &str, params: &Value) -> Result<Value> {
        let empty = kit::Params::new();
        let clean = kit::clean(item, params.as_object().unwrap_or(&empty))?;
        let glb = kit::build(item, &clean)?.glb();
        self.import(&glb, &kit::label(item, &clean)?, 1.0)
    }

    fn rom_path(&self) -> Option<PathBuf> {
        self.rom_path.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_trait_is_object_safe_and_shared_between_threads() {
        fn shared<T: Send + Sync + ?Sized>() {}
        shared::<dyn Backend>();
        shared::<GameBackend>();
        shared::<Arc<dyn Backend>>();
    }
}
