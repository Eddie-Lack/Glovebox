//! What the editor's window asks its back end for: the tables, the levels, the models and textures, plans and
//! builds, and the author's imported models. `Backend` is the interface of `desktop/backend.py`'s `Backend` class.
//! `GameBackend` answers from the player's ROM (`GameData`) and the user folder.
//!
//! The Python app named a model by a path and read its file with `fetch`. Here a model is named by a `ModelRef`,
//! and `model` and `texture` give the model and the pixels.

use crate::build::{self, BuildAllRequest, BuildRequest, Built};
use crate::gamedata::{self, hex, parse_id, GameData, Image};
use crate::imports::{self, Customs, Plan};
use crate::level;
use crate::model::Model;
use crate::objbank::{self, Bank};
use crate::tables;
use crate::texbank::TexBank;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
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
    /// None when there is no model to draw
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

    // -- Imported models and the kit

    /// Converts a glTF model into a new set of imported models. Returns the set's `meta.json` with its id as `set`.
    fn import_custom(&self, data: &[u8], name: &str, scale: f64) -> Result<Value>;
    fn delete_custom(&self, set: &str) -> Result<()>;
    /// A copy of a retail model with textures from other worlds. Returns the copy's object id.
    fn reskin(&self, oid: &str, worlds: &[String], seed: u64) -> Result<String>;
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
}

/// The longest model file `import_custom` takes.
pub const MAX_MODEL: usize = 64 << 20;

fn not_yet<T>(what: &str) -> Result<T> {
    // todo: call crate::custom and crate::kit when they exist
    Err(BackendError(format!("{what} is not available yet")))
}

impl GameBackend {
    /// `home` is the user folder. Imported models are in its `custom` folder.
    pub fn new(game: Arc<GameData>, home: impl Into<PathBuf>, rom_path: Option<PathBuf>) -> GameBackend {
        GameBackend { game, home: home.into(), rom_path, templates: OnceLock::new(), textures: Mutex::new(BTreeMap::new()) }
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

    fn custom_dir(&self) -> PathBuf {
        self.home.join("custom")
    }

    /// The author's imported models, read from the user folder.
    pub fn customs(&self) -> Customs {
        imports::load_customs(&self.custom_dir())
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

    fn import_custom(&self, data: &[u8], _name: &str, _scale: f64) -> Result<Value> {
        if data.len() > MAX_MODEL {
            return Err(BackendError(format!("The model is larger than {} MB", MAX_MODEL >> 20)));
        }
        not_yet("Importing a model")
    }

    fn delete_custom(&self, set: &str) -> Result<()> {
        if !imports::is_set_id(set) {
            return Err(BackendError("no such model".into()));
        }
        // A folder that is not there is not an error, as in the Python app
        let _ = std::fs::remove_dir_all(self.custom_dir().join(set));
        self.textures.lock().unwrap_or_else(|e| e.into_inner()).retain(|(bank, _), _| *bank != format!("custom:{set}"));
        Ok(())
    }

    fn reskin(&self, _oid: &str, _worlds: &[String], _seed: u64) -> Result<String> {
        not_yet("Reskinning a model")
    }

    fn kit_items(&self) -> Result<Value> {
        not_yet("The kit")
    }

    fn kit_preview(&self, _item: &str, _params: &Value) -> Result<Vec<u8>> {
        not_yet("The kit")
    }

    fn make_kit(&self, _item: &str, _params: &Value) -> Result<Value> {
        not_yet("The kit")
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
