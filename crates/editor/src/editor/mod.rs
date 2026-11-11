//! The editor's state and every edit, without the widgets: loading a level, selection, the transform engine, undo,
//! autosave, adding and removing things, the checks, imports and the memory budget, the path tool and building.
//!
//! A port of the former Python editor's `editor.py`. Every edit is a change to the level XML with an undo snapshot.
//! The entities and their scene nodes are then built again from the XML. The window draws this state and calls
//! these methods.
//! What replaces the Qt parts:
//! - Signals are a queue of [`Event`]s. The window drains it each frame with [`Editor::drain_events`].
//! - The two debounce timers (draft save, import plan) are deadlines. [`Editor::tick`] checks them. Without
//!   [`Editor::delays`] both run at once, as the Python editor did without a window.
//! - Background work is a value with a `run` method: [`BuildJob`], [`PlanJob`], [`LevelJob`]. The window runs it
//!   on a thread and hands the result back. The `Editor` itself is used from one thread.
//! - Entities are named by their index in [`Editor::entities`]. The indices change with every rebuild.

mod checks;
mod cmds;
mod edit;
mod exits;
mod load;
mod nodes;
mod path;
mod select;
mod transform;

pub use exits::ExitTrigger;
pub use checks::{Budget, BudgetRow, Check, Click, DrawLoad, SceneryBudget, Violation};
pub use load::{fetch_level, BuildJob, BuildOutput, LevelData, LevelJob, PlanJob, PlanResult};
pub use nodes::EntNodes;
pub use path::Scatter;
pub use select::{Button, Mods};

use crate::camera::{Camera, Orbit};
use crate::commands::Format;
use crate::drafts::Drafts;
use crate::entities::{self, Ent, EntId, Models};
use crate::gizmo::{Gizmo, Mode};
use crate::sanitize::FormatModel;
use crate::scene::{self, Geometry, Prefab, Scene, Texture};
use crate::vec::V3;
use glovebox_core::backend::{Backend, CatalogEntry, ModInfo, ModelRef};
use glovebox_core::imports::Plan;
use glovebox_core::xml::{Doc, NodeId};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

pub const GITHUB: &str = "https://github.com/Eddie-Lack/gloveworld/blob/";

// Draw load. The game draws every actor within the far plane of the camera (IsActorVisible in object.c: the actor's
// radius plus farPlaneDist against its distance, with no test against the view), behind the camera too. Each drawn
// object node takes one of the frame's 256 object matrices (object.c), and so do shadows and footprints. Past 256
// they overwrite the camera's matrices and the frame never finishes. Long before that the game slows down. The far
// plane is FogConfiguration's near_clip (parse.c TOKEN_FOG; 1000 without one, game.c).
pub const MATRICES: i64 = 256;
// Measured in the emulator: the shipped hub 1 asks for 228 at its fountain, and a hub
// built with the editor lagged on a console at 206 to 228. The estimate was within 35 of the game's count in four
// levels, and up to 63 over in hub 1.
pub const LOAD_SLOW: i64 = 190;
pub const LOAD_STOP: i64 = 240;
/// Glover, the ball, their shadows and footprints.
pub const LOAD_BASE: i64 = 24;
pub const LOAD_KINDS: [&str; 5] = ["land", "platform", "enemy", "pickup", "water"];
// Measured in the emulator
pub const LOAD_CAM_BACK: f64 = 130.0;
pub const LOAD_CAM_UP: f64 = 50.0;

pub const MOD_NOTE: &str =
    "Build ROM and Export patch add the Expansion Pak game mod for this, and the ROM then needs the Expansion Pak.";
pub const PATH_COPY: [&str; 7] = ["LandActor", "BackgroundActor", "AnimatedBackgroundActor", "Enemy", "Powerup", "MrTip", "Platform"];
pub const PATH_MAX: usize = 300;

/// The level-wide commands the Level settings list shows, and their names in its Add menu.
pub const LOOK: [(&str, &str); 6] = [
    ("FogConfiguration", "Fog"),
    ("AmbientLight", "Ambient light"),
    ("DiffuseLight", "Directional light"),
    ("Backdrop", "Backdrop"),
    ("SetGravity", "Gravity"),
    ("AmbientSound", "Ambient sound"),
];
pub const LOOK_ONE: [&str; 3] = ["FogConfiguration", "SetGravity", "AmbientLight"];
/// Settings every level keeps one of: the game keeps the last screen's fog without one, and every shipped level
/// has both.
pub const LOOK_KEEP: [&str; 2] = ["FogConfiguration", "AmbientLight"];
/// AddBackgroundLight ignores the 8th and later (light.c).
pub const LIGHTS_MAX: usize = 7;
/// The enemies[] array (0x5A00 bytes of 0x300), and LoadEnemy doesn't check.
pub const ENEMIES_MAX: usize = 30;
/// The Atlantis boss: the game reads enemies[0], [1] and [2] as these.
pub const TRIO: [&str; 3] = ["joff", "cancer", "kirk"];
pub const GROUPS: [&str; 3] = ["NormalInstructions", "ConditionalInstructions", "AttackInstructions"];
/// The Space slots mod/slots.c can swap for extra levels.
pub const EXTRA_SLOTS: [i64; 5] = [35, 36, 37, 38, 41];
/// The platform tag of a hub's entry zone to the extra levels.
pub const ENTRY_TAG: i64 = 99;

/// A look preset: ambient light, directional lights (r, g, b, theta_x, theta_y) and fog (r, g, b, fog start, far
/// plane).
pub struct LookPreset {
    pub name: &'static str,
    pub ambient: [i64; 3],
    pub lights: &'static [(i64, i64, i64, f64, f64)],
    pub fog: [i64; 5],
}

/// Look presets. The far plane is 1150 in the course looks: levels built with 1600 lagged on a console.
pub const LOOKS: [LookPreset; 6] = [
    LookPreset { name: "Graveyard violet", ambient: [140, 120, 150], lights: &[(210, 235, 190, 0.0, 3.7), (230, 140, 90, 4.28, 0.0)], fog: [26, 10, 36, 860, 1150] },
    LookPreset { name: "Frozen green", ambient: [120, 140, 140], lights: &[(190, 235, 240, 0.0, 3.7), (220, 90, 90, 4.28, 0.0)], fog: [10, 24, 20, 860, 1150] },
    LookPreset { name: "Blood red", ambient: [150, 110, 120], lights: &[(240, 120, 110, 0.0, 3.7), (150, 110, 200, 4.28, 0.0)], fog: [40, 6, 8, 860, 1150] },
    LookPreset { name: "Rotting green arena", ambient: [160, 175, 150], lights: &[(200, 235, 170, 0.0, 3.7), (235, 110, 90, 4.28, 0.0)], fog: [10, 30, 16, 980, 1900] },
    LookPreset { name: "Burning dark", ambient: [150, 120, 110], lights: &[(250, 160, 80, 0.0, 3.7), (150, 90, 200, 4.28, 0.0)], fog: [34, 16, 6, 860, 1150] },
    LookPreset { name: "Daylight (shipped levels)", ambient: [110, 110, 150], lights: &[(160, 160, 160, 0.0, 3.7)], fog: [40, 40, 60, 996, 1500] },
];

/// The name the Level settings list shows for a level-wide command.
pub fn look_name(tag: &str) -> Option<&'static str> {
    LOOK.iter().find(|(t, _)| *t == tag).map(|(_, n)| *n)
}

/// What the Python editor sent as Qt signals.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// A message for the status line. `class` is "ok", "err" or "".
    Status { message: String, class: String },
    SelectionChanged,
    /// The entities were built again (after a load or an edit).
    Rebuilt,
    /// The level changed (undo state, autosave chip).
    Edited,
    /// "" hides the loading note.
    Loading(String),
    /// Something in the 3D view moved.
    ViewChanged,
    ImportsChanged,
    SceneLoaded,
    ModeChanged,
}

/// The app's settings store (Python's QSettings). The editor uses the keys "scene", "triggers" and "exits". The window
/// also keeps "fog", "saveDir" and "emulator" there.
pub trait Settings {
    fn value(&self, key: &str) -> Option<String>;
    fn set_value(&mut self, key: &str, value: &str);
}

/// Settings kept in memory.
#[derive(Default, Clone, Debug)]
pub struct MemorySettings(pub BTreeMap<String, String>);

impl Settings for MemorySettings {
    fn value(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }
    fn set_value(&mut self, key: &str, value: &str) {
        self.0.insert(key.to_string(), value.to_string());
    }
}

/// How long the editor waits after an edit, in seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Delays {
    pub draft: f64,
    pub plan: f64,
}

impl Default for Delays {
    /// The Python window's timers: 400 ms and 250 ms.
    fn default() -> Delays {
        Delays { draft: 0.4, plan: 0.25 }
    }
}

/// The path tool's state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PathTool {
    pub on: bool,
    pub pts: Vec<V3>,
}

/// Makes the picture of an icon: a glyph on a round badge of a colour like "#f2c14e".
pub type IconMaker = Box<dyn FnMut(&str, &str) -> Option<Arc<Texture>>>;

/// A `Format` that is either the shared one or built for another table.
pub(crate) enum Held<'a> {
    Shared(&'a Format<'a>),
    Own(Box<Format<'a>>),
}

impl<'a> std::ops::Deref for Held<'a> {
    type Target = Format<'a>;
    fn deref(&self) -> &Format<'a> {
        match self {
            Held::Shared(f) => f,
            Held::Own(f) => f,
        }
    }
}

/// The format of a decomp table. The compiled-in table's format is built once.
pub(crate) fn format_of(decomp: &Value) -> Held<'_> {
    if std::ptr::eq(decomp, glovebox_core::tables::decomp()) {
        Held::Shared(Format::shared())
    } else {
        Held::Own(Box::new(Format::new(decomp)))
    }
}

/// `sanitize_level_xml` with the model of a decomp table.
pub(crate) fn sanitize(decomp: &Value, xml: &str, source: &str) -> crate::sanitize::SanitizeResult {
    if std::ptr::eq(decomp, glovebox_core::tables::decomp()) {
        crate::sanitize::sanitize_level_xml(xml, FormatModel::shared(), source)
    } else {
        crate::sanitize::sanitize_level_xml(xml, &FormatModel::new(decomp), source)
    }
}

/// Answers `entities::build`'s questions about models.
pub(crate) struct ModelIndex<'a> {
    pub models: &'a BTreeMap<String, ModelRef>,
    pub loaded: &'a BTreeSet<String>,
    pub catalog: &'a BTreeMap<String, CatalogEntry>,
}

impl ModelIndex<'_> {
    pub fn model_ref(&self, oid: &str) -> Option<ModelRef> {
        self.models.get(oid).cloned().or_else(|| self.catalog.get(oid).and_then(|c| c.model.clone()))
    }
}

impl Models for ModelIndex<'_> {
    fn has_model(&self, object_id: &str) -> bool {
        self.model_ref(object_id).is_some()
    }
    fn is_import(&self, object_id: &str) -> bool {
        !object_id.is_empty() && !self.loaded.contains(object_id) && self.catalog.contains_key(object_id)
    }
}

/// A drag of the gizmo in progress.
pub(crate) struct Drag {
    pub before: String,
    pub set: Vec<EntId>,
    pub p0: V3,
    pub q0: crate::vec::Quat,
    pub items: Vec<DragItem>,
}

pub(crate) struct DragItem {
    pub e: EntId,
    pub p: V3,
    pub pts: Vec<V3>,
    pub q: Option<crate::vec::Quat>,
    pub s: Option<V3>,
    pub size: Option<V3>,
}

/// The editor. See the module's comment.
pub struct Editor {
    pub backend: Arc<dyn Backend>,
    pub drafts: Drafts,
    pub settings: Option<Box<dyn Settings>>,
    /// The level list (`scenes.json` rows).
    pub scenes: Vec<Value>,
    /// The open level's row of `scenes`.
    pub scene: Option<Value>,
    /// The puzzle the inspector shows when nothing is selected: its place among the level's puzzles.
    pub puzzle_index: Option<usize>,
    /// Every model of the game and the author's imported models. Set it with `set_catalog`.
    pub catalog: Arc<BTreeMap<String, CatalogEntry>>,
    pub mod_info: ModInfo,
    /// The level XML. Empty until a level is loaded (`has_level`).
    pub doc: Doc,
    pub has_level: bool,
    pub models: BTreeMap<String, ModelRef>,
    pub loaded: BTreeSet<String>,
    pub entities: Vec<Ent>,
    /// The scene nodes of each entity, in the order of `entities`.
    pub nodes: Vec<EntNodes>,
    pub selection: Vec<EntId>,
    pub undo_stack: Vec<String>,
    pub redo_stack: Vec<String>,
    /// The kinds of entity the view hides.
    pub hidden: BTreeSet<String>,
    pub orig_lev_size: Option<usize>,
    pub filter: String,
    /// "group": a group turns and scales about its middle. Anything else: each object about itself.
    pub pivot_mode: String,
    /// The motion preview is on.
    pub preview: bool,
    pub show_fog: bool,
    /// (colour, start, end) of the level's fog as the view draws it.
    pub level_fog: Option<([f64; 3], f64, f64)>,
    pub background: [f64; 3],
    pub ambient: [f64; 3],
    pub orig_xml: Option<String>,
    pub orig_xml_for: HashMap<String, String>,
    pub orig_violations: Vec<Violation>,
    pub violations: Vec<Violation>,
    /// When the open level's draft was saved, in seconds since 1970.
    pub draft_saved_at: Option<f64>,
    pub copy_imports: bool,
    pub plan: Option<Plan>,
    pub mode: Mode,
    /// The move snap in units. 0 turns every snap off.
    pub snap: f64,
    pub box_mode: bool,
    /// The scene graph. Its roots are `world`, `path_node` and `far_sphere`.
    pub graph: Scene,
    pub world: scene::NodeId,
    pub far_sphere: Option<scene::NodeId>,
    pub path: PathTool,
    pub path_node: scene::NodeId,
    pub camera: Camera,
    pub orbit: Orbit,
    pub gizmo: Gizmo,
    /// Set by the window. Without one, icons are sprites with an empty picture.
    pub icon_maker: Option<IconMaker>,
    /// False loads models without their textures (tests).
    pub load_textures: bool,
    /// The debounce times. None saves drafts and plans imports at once.
    pub delays: Option<Delays>,
    /// Formats a draft's time (seconds since 1970) for the status line. The default writes UTC.
    pub format_time: fn(f64) -> String,
    /// The last status message and its class.
    pub last_status: (String, String),
    /// The box-select setting "include the landscape".
    pub include_land: bool,
    pub(crate) model_cache: HashMap<ModelRef, Option<Arc<Prefab>>>,
    /// The root mesh vertices of the models of exit platforms, by object id (`root_vertices`).
    pub(crate) exit_verts: std::cell::RefCell<HashMap<String, Option<Arc<Vec<[f32; 3]>>>>>,
    pub(crate) icon_cache: HashMap<(String, String), Arc<Texture>>,
    pub(crate) shapes: HashMap<&'static str, Arc<Geometry>>,
    pub(crate) drag: Option<Drag>,
    pub(crate) events: Vec<Event>,
    pub(crate) draft_pending: Option<(String, String)>,
    pub(crate) draft_due: Option<f64>,
    pub(crate) plan_pending: Option<Vec<String>>,
    pub(crate) plan_due: Option<f64>,
    pub(crate) plan_serial: u64,
    pub(crate) now: f64,
    pub(crate) orig_cost: Option<(String, (i64, i64))>,
    pub(crate) mouse: select::Mouse,
    pub(crate) preview_t0: f64,
}

impl Editor {
    pub fn new(backend: Arc<dyn Backend>, drafts: Drafts, settings: Option<Box<dyn Settings>>) -> Editor {
        let mut graph = Scene::new();
        let world = graph.node("world");
        let path_node = graph.node("path");
        Editor {
            backend,
            drafts,
            settings,
            scenes: Vec::new(),
            scene: None,
            puzzle_index: None,
            catalog: Arc::new(BTreeMap::new()),
            mod_info: ModInfo::default(),
            doc: Doc::new("Level"),
            has_level: false,
            models: BTreeMap::new(),
            loaded: BTreeSet::new(),
            entities: Vec::new(),
            nodes: Vec::new(),
            selection: Vec::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            hidden: BTreeSet::new(),
            orig_lev_size: None,
            filter: String::new(),
            pivot_mode: "group".to_string(),
            preview: false,
            show_fog: true,
            level_fog: None,
            background: [0.051, 0.055, 0.071],
            ambient: [1.0, 1.0, 1.0],
            orig_xml: None,
            orig_xml_for: HashMap::new(),
            orig_violations: Vec::new(),
            violations: Vec::new(),
            draft_saved_at: None,
            copy_imports: false,
            plan: None,
            mode: Mode::Translate,
            snap: 1.0,
            box_mode: false,
            graph,
            world,
            far_sphere: None,
            path: PathTool::default(),
            path_node,
            camera: Camera::default(),
            orbit: Orbit::default(),
            gizmo: Gizmo::new(),
            icon_maker: None,
            load_textures: true,
            delays: None,
            format_time: utc_time,
            last_status: (String::new(), String::new()),
            include_land: false,
            model_cache: HashMap::new(),
            exit_verts: Default::default(),
            icon_cache: HashMap::new(),
            shapes: HashMap::new(),
            drag: None,
            events: Vec::new(),
            draft_pending: None,
            draft_due: None,
            plan_pending: None,
            plan_due: None,
            plan_serial: 0,
            now: 0.0,
            orig_cost: None,
            mouse: select::Mouse::default(),
            preview_t0: 0.0,
        }
    }

    // -----------------------------------------------------------------------------------------------------------
    // Events and time

    pub(crate) fn emit(&mut self, e: Event) {
        // The same notice twice in a row says nothing new
        if !matches!(e, Event::Status { .. }) && self.events.last() == Some(&e) {
            return;
        }
        self.events.push(e);
    }

    /// The events since the last call, oldest first.
    pub fn drain_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    pub fn set_status(&mut self, msg: &str, cls: &str) {
        self.last_status = (msg.to_string(), cls.to_string());
        self.events.push(Event::Status { message: msg.to_string(), class: cls.to_string() });
    }

    /// Call every frame with a clock in seconds. Saves the draft when its wait is over. Returns the import plan
    /// to work out when its wait is over: run it on a thread and give the result to `apply_plan`.
    pub fn tick(&mut self, now: f64) -> Option<PlanJob> {
        self.now = now;
        if self.draft_due.is_some_and(|t| now >= t) {
            self.flush_draft();
        }
        if self.plan_due.is_some_and(|t| now >= t) {
            self.plan_due = None;
            if let Some(ids) = self.plan_pending.take() {
                return Some(self.plan_job(ids));
            }
        }
        None
    }

    // -----------------------------------------------------------------------------------------------------------
    // Tables

    /// Loads the level list and the mod's facts (call before `load_scene`).
    pub fn start(&mut self) {
        self.scenes = self.backend.scenes().as_array().cloned().unwrap_or_default();
        self.mod_info = self.backend.mod_info();
    }

    /// Replaces the catalog (after the back end loaded it, or an imported model came or went).
    pub fn set_catalog(&mut self, catalog: BTreeMap<String, CatalogEntry>) {
        self.catalog = Arc::new(catalog);
        self.exit_verts.borrow_mut().clear();
    }

    /// The open level's id, or "".
    pub fn scene_id(&self) -> &str {
        self.scene_str("id")
    }

    pub(crate) fn scene_str(&self, key: &str) -> &str {
        self.scene.as_ref().and_then(|s| s.get(key)).and_then(|v| v.as_str()).unwrap_or("")
    }

    pub(crate) fn scene_index(&self) -> i64 {
        self.scene.as_ref().and_then(|s| s.get("index")).and_then(|v| v.as_i64()).unwrap_or(-1)
    }

    pub(crate) fn scene_row(&self, sid: &str) -> Option<&Value> {
        self.scenes.iter().find(|s| s.get("id").and_then(|v| v.as_str()) == Some(sid))
    }

    /// The command or type table of an element.
    pub fn type_for(&self, tag: &str) -> Option<Value> {
        format_of(self.backend.decomp()).command_or_type(tag).cloned()
    }

    /// The command an element is written by: its own, or the one that groups into its parent.
    pub fn command_for(&self, node: NodeId) -> Option<String> {
        let decomp = self.backend.decomp();
        let commands = decomp.get("commands")?.as_object()?;
        let tag = self.doc.tag(node);
        if commands.contains_key(tag) {
            return Some(tag.to_string());
        }
        let ptag = self.doc.parent(node).map(|p| self.doc.tag(p))?;
        commands
            .iter()
            .find(|(_, c)| c.get("groupsInto").and_then(|g| g.as_str()).is_some_and(|g| !g.is_empty() && g == ptag))
            .map(|(n, _)| n.clone())
    }

    /// The names of an enum field's values by their numbers.
    pub fn ksy_enum_for(&self, tag: &str, field: &str) -> Option<serde_json::Map<String, Value>> {
        format_of(self.backend.decomp()).enum_for(tag, field).cloned()
    }

    pub fn enum_value(&self, tag: &str, field: &str, name: Option<&str>) -> Option<i64> {
        entities::enum_value(&format_of(self.backend.decomp()), tag, field, name)
    }

    pub fn decomp_enum_name(&self, enum_: &str, value: Option<i64>) -> Option<String> {
        entities::decomp_enum_name(self.backend.decomp(), enum_, value).map(|s| s.to_string())
    }

    pub fn enemy_model_id(&self, type_name: Option<&str>) -> Option<String> {
        entities::enemy_model_id(&format_of(self.backend.decomp()), self.backend.tables(), self.scene_id(), type_name)
    }

    pub fn powerup_model_id(&self, t: i64) -> Option<String> {
        entities::powerup_model_id(self.backend.tables(), t)
    }

    pub fn flag_enum(&self) -> Option<serde_json::Map<String, Value>> {
        self.backend.decomp().get("ksyEnums")?.get("enemy_instruction::instruction_flags")?.as_object().cloned()
    }

    // -----------------------------------------------------------------------------------------------------------
    // Models. Models outside this level's banks come from the catalog of every bank in the game and are imported
    // into the level's bank at ROM build.

    pub(crate) fn index(&self) -> ModelIndex<'_> {
        ModelIndex { models: &self.models, loaded: &self.loaded, catalog: &self.catalog }
    }

    /// Where the model of an object id comes from (Python's `model_url`).
    pub fn model_ref(&self, oid: &str) -> Option<ModelRef> {
        self.index().model_ref(oid)
    }

    /// "Loaded" means in a bank both the game's LoadLevel and noclip's scene table load for this level.
    pub fn is_import(&self, oid: &str) -> bool {
        self.index().is_import(oid)
    }

    /// The model, ready to place. A model that won't load is None and shows as a placeholder.
    pub fn load_model(&mut self, oid: &str) -> Option<Arc<Prefab>> {
        let r = self.model_ref(oid)?;
        if let Some(p) = self.model_cache.get(&r) {
            return p.clone();
        }
        let p = load::load_prefab(&*self.backend, &r, self.load_textures);
        self.model_cache.insert(r, p.clone());
        p
    }

    // -----------------------------------------------------------------------------------------------------------
    // The document

    pub fn root(&self) -> NodeId {
        self.doc.root()
    }

    pub fn serialize(&self) -> String {
        self.doc.serialize()
    }

    pub fn top_level(&self) -> Vec<NodeId> {
        self.doc.children(self.doc.root()).to_vec()
    }

    pub fn end_marker(&self) -> Option<NodeId> {
        self.doc.first(self.doc.root(), "EndLevelData")
    }

    /// Puts a new element at the end of the level, before EndLevelData.
    pub fn insert_top(&mut self, node: NodeId) -> NodeId {
        match self.end_marker() {
            Some(end) => self.doc.add_prev(end, node),
            None => {
                let root = self.doc.root();
                self.doc.append(root, node)
            }
        }
        node
    }

    /// Replaces the level with a snapshot. False when the text is not XML: the level then stays as it is.
    pub fn restore(&mut self, xml: &str) -> bool {
        match Doc::parse(xml) {
            Ok(d) => {
                self.doc = d;
                true
            }
            Err(_) => false,
        }
    }

    /// The level's FogConfiguration is drawn by default. The toolbar switch hides it in the editor view only.
    pub fn fog(&self) -> Option<([f64; 3], f64, f64)> {
        if self.show_fog {
            self.level_fog
        } else {
            None
        }
    }

    /// The entities a click can pick.
    pub fn pickable(&self) -> Vec<EntId> {
        (0..self.entities.len()).filter(|&i| !self.hidden.contains(self.entities[i].kind) && !self.entities[i].derived).collect()
    }

    /// The entity a scene node belongs to.
    pub fn entity_of(&self, node: scene::NodeId) -> Option<EntId> {
        self.graph.entity_of(node).map(|(_, e)| e as usize).filter(|e| *e < self.entities.len())
    }

    /// The first entity that passes a test.
    pub fn find(&self, pred: impl Fn(&Ent) -> bool) -> Option<EntId> {
        self.entities.iter().position(pred)
    }

    /// The entities that stand for whole objects: no handles and nothing derived.
    pub(crate) fn whole(&self, i: EntId) -> bool {
        let e = &self.entities[i];
        !e.is_handle && !e.derived
    }

    /// The XML node an entity stands for.
    pub(crate) fn key_node(&self, i: EntId) -> NodeId {
        let e = &self.entities[i];
        e.sub.unwrap_or(e.node)
    }
}

/// "2026-01-31 12:00" for a time in seconds since 1970, in UTC.
pub fn utc_time(t: f64) -> String {
    let secs = if t.is_finite() { t.floor() as i64 } else { 0 };
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    // Days to a date (Howard Hinnant's civil_from_days)
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{:04}-{:02}-{:02} {:02}:{:02}", y, m, d, rem / 3600, rem % 3600 / 60)
}
