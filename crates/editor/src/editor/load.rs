//! Loading a level, importing XML, and building. The slow parts are values with a `run` method that need
//! nothing from the editor. The window can run them on a thread.

use super::{format_of, sanitize, Editor, Event, ModelIndex, ENTRY_TAG, EXTRA_SLOTS};
use crate::entities::{self, Context, MOD_PARENTS};
use crate::sanitize::SanitizeResult;
use crate::scene::{Prefab, Texture};
use crate::xmlutil::hex_id;
use glovebox_core::backend::{Backend, CatalogEntry, ModelRef, SceneModels};
use glovebox_core::build::{BuildAllEntry, BuildAllRequest, BuildRequest, Info};
use glovebox_core::imports::Plan;
use glovebox_core::xml::Doc;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

/// Reads a model and its textures into scene data. None when the back end can't give the model.
pub(crate) fn load_prefab(backend: &dyn Backend, r: &ModelRef, textures: bool) -> Option<Arc<Prefab>> {
    let model = backend.model(r).ok()?;
    let mut tex = |id: u32| -> Option<Arc<Texture>> {
        if !textures {
            return None;
        }
        let img = backend.texture(r, id).ok()?;
        Texture::new(img.width, img.height, img.rgba.clone()).map(Arc::new)
    };
    Some(Arc::new(Prefab::from_model(&model, &mut tex)))
}

/// What a level needs from the back end before the editor can show it.
pub struct LevelData {
    pub sid: String,
    pub xml: String,
    pub models: SceneModels,
    pub orig_lev_size: Option<usize>,
    /// The models the level draws, read ahead. The editor loads the others when it needs them.
    pub prefabs: HashMap<ModelRef, Option<Arc<Prefab>>>,
}

/// The models the entities of a level draw.
fn wanted(backend: &dyn Backend, sid: &str, doc: &Doc, index: &ModelIndex) -> Vec<ModelRef> {
    let fmt_ = format_of(backend.decomp());
    let cx = Context { format: &fmt_, tables: backend.tables(), scene_id: sid, models: index };
    let mut out = Vec::new();
    for e in entities::build(doc, doc.root(), &cx).ents {
        let ids = [e.object_id.as_deref(), e.decor.map(|d| d.0)];
        for r in ids.into_iter().flatten().filter_map(|o| index.model_ref(o)) {
            if !out.contains(&r) {
                out.push(r);
            }
        }
    }
    out
}

/// Reads a level from the back end: its XML, its models and, read ahead, the models it draws. `draft` is the
/// saved draft's XML, whose models are read ahead too. This is the slow part of opening a level.
pub fn fetch_level(
    backend: &dyn Backend,
    catalog: &BTreeMap<String, CatalogEntry>,
    sid: &str,
    draft: Option<&str>,
    textures: bool,
) -> Result<LevelData, String> {
    let xml = backend.level_xml(sid).map_err(|e| e.0)?;
    let models = backend.models(sid).map_err(|e| e.0)?;
    let loaded: BTreeSet<String> = models.loaded.iter().cloned().collect();
    let index = ModelIndex { models: &models.models, loaded: &loaded, catalog };
    let mut prefabs = HashMap::new();
    for text in [Some(xml.as_str()), draft].into_iter().flatten() {
        let Ok(doc) = Doc::parse(text) else { continue };
        for r in wanted(backend, sid, &doc, &index) {
            if !prefabs.contains_key(&r) {
                let p = load_prefab(backend, &r, textures);
                prefabs.insert(r, p);
            }
        }
    }
    Ok(LevelData { sid: sid.to_string(), orig_lev_size: backend.orig_lev_size(sid), xml, models, prefabs })
}

/// `fetch_level` to run on a thread. Give the result to `Editor::apply_level`.
pub struct LevelJob {
    pub backend: Arc<dyn Backend>,
    pub catalog: Arc<BTreeMap<String, CatalogEntry>>,
    pub sid: String,
    pub draft: Option<String>,
    pub textures: bool,
}

impl LevelJob {
    pub fn run(self) -> Result<LevelData, String> {
        fetch_level(&*self.backend, &self.catalog, &self.sid, self.draft.as_deref(), self.textures)
    }
}

/// The import plan to work out on a thread. Give the result to `Editor::apply_plan`.
pub struct PlanJob {
    pub backend: Arc<dyn Backend>,
    pub sid: String,
    pub ids: Vec<String>,
    pub serial: u64,
}

pub struct PlanResult {
    pub sid: String,
    pub serial: u64,
    /// None when the back end could not plan.
    pub plan: Option<Plan>,
}

impl PlanJob {
    pub fn run(self) -> PlanResult {
        PlanResult { plan: self.backend.plan(&self.sid, &self.ids).ok(), sid: self.sid, serial: self.serial }
    }
}

/// A built file: its bytes, its file name and the message for the status line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildOutput {
    pub data: Vec<u8>,
    pub name: String,
    pub message: String,
}

/// A build to run on a thread. Everything it needs from the editor was read when it was made, so a level switch
/// or an edit while it runs changes nothing.
pub enum BuildJob {
    /// Nothing left to do (Export XML).
    Ready(BuildOutput),
    One { backend: Arc<dyn Backend>, request: BuildRequest, notes: Vec<String>, scene_name: String, orig_size: i64 },
    All { backend: Arc<dyn Backend>, request: BuildAllRequest, names: Vec<String> },
}

const WITH_MOD: &str = " Includes the Expansion Pak game mod and needs the Expansion Pak.";

impl BuildJob {
    /// Builds. The error is the back end's message for the author.
    pub fn run(self) -> Result<BuildOutput, String> {
        match self {
            BuildJob::Ready(out) => Ok(out),
            BuildJob::One { backend, request, notes, scene_name, orig_size } => {
                let b = backend.build(&request).map_err(|e| e.0)?;
                let size = b.lev_size.unwrap_or(0) as i64;
                let imp = b.imports.as_ref();
                let with_mod = if b.needs_mod { WITH_MOD } else { "" };
                let name = b.filename.clone();
                let message = if request.format == "bps" {
                    let ps = b.patch_size.unwrap_or(0) as f64;
                    let boot = if request.quickboot { "boots straight into the level" } else { "starts the game normally" };
                    let what = match imp {
                        Some(i) if i.runtime > 0 => format!(
                            " The game mod loads its {} imported model(s) from the player's ROM; the patch contains no game data.",
                            i.objects
                        ),
                        Some(i) if i.objects > 0 => format!(
                            " It includes the rebuilt {} and {} banks for {} imported model(s), game data included.",
                            i.bank, i.tex_bank, i.objects
                        ),
                        _ => " It contains only what your edits add.".to_string(),
                    };
                    format!(
                        "Exported {} ({:.1} KB BPS patch; {}). Anyone with the original Glover (USA) ROM can apply it with patcher.html, in their browser.{}{}",
                        name,
                        ps / 1024.0,
                        boot,
                        what,
                        with_mod
                    )
                } else {
                    let delta = size - orig_size;
                    let rom = request.format == "rom";
                    let what = match imp {
                        Some(i) if i.runtime > 0 => {
                            format!("; {} imported model(s) load from {} bank(s) of the ROM at run time", i.objects, i.runtime)
                        }
                        Some(i) if i.objects > 0 => {
                            format!("; imported {} model(s) into {} and {} texture(s) into {}", i.objects, i.bank, i.textures, i.tex_bank)
                        }
                        _ => String::new(),
                    };
                    let end = if rom && request.quickboot { format!("; the ROM boots straight into {}.", scene_name) } else { ".".to_string() };
                    let msg = format!(
                        "Built {}: level is {} bytes ({}{} vs. original){}{}{}",
                        name,
                        size,
                        if delta >= 0 { "+" } else { "" },
                        delta,
                        what,
                        end,
                        if rom { with_mod } else { "" }
                    );
                    if notes.is_empty() {
                        msg
                    } else {
                        format!("{} {}", notes.join(" "), msg)
                    }
                };
                Ok(BuildOutput { data: b.data, name, message })
            }
            BuildJob::All { backend, request, names } => {
                let b = backend.build_all(&request).map_err(|e| e.0)?;
                let n = request.entries.len();
                let mut message = format!(
                    "{} {} with {} level{}: {}.",
                    if request.format == "rom" { "Built" } else { "Exported" },
                    b.filename,
                    n,
                    if n == 1 { "" } else { "s" },
                    names.join(", ")
                );
                if !b.extra.is_empty() {
                    message += &format!(
                        " {} extra level{}, entered through a hub zone with platform tag {} (exit 11).",
                        b.extra.len(),
                        if b.extra.len() == 1 { "" } else { "s" },
                        ENTRY_TAG
                    );
                }
                if b.needs_mod {
                    message += WITH_MOD;
                }
                Ok(BuildOutput { name: b.filename.clone(), data: b.data, message })
            }
        }
    }
}

fn s(v: Option<&serde_json::Value>) -> &str {
    v.and_then(|v| v.as_str()).unwrap_or("")
}

impl Editor {
    // -----------------------------------------------------------------------------------------------------------
    // Loading a level

    /// What to run on a thread to open a level. Saves the waiting draft first.
    pub fn level_job(&mut self, sid: &str) -> LevelJob {
        self.flush_draft();
        LevelJob {
            backend: self.backend.clone(),
            catalog: self.catalog.clone(),
            sid: sid.to_string(),
            draft: self.drafts.load(sid).map(|d| d.xml),
            textures: self.load_textures,
        }
    }

    /// Loads a level at once.
    pub fn load_scene(&mut self, sid: &str) -> Result<(), String> {
        let data = self.level_job(sid).run()?;
        self.apply_level(data)
    }

    /// Opens a level that a `LevelJob` read.
    pub fn apply_level(&mut self, data: LevelData) -> Result<(), String> {
        self.flush_draft(); // finish saving the level we're leaving
        let sid = data.sid.as_str();
        let row = self.scene_row(sid).cloned().ok_or_else(|| format!("There is no level {}", sid))?;
        let doc = Doc::parse(&data.xml).map_err(|e| e.0)?;
        self.scene = Some(row);
        self.puzzle_index = None;
        self.show_far_plane(None, 0.0);
        self.path.pts.clear();
        self.draw_path();
        self.models = data.models.models;
        self.loaded = data.models.loaded.into_iter().collect();
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.selection.clear();
        self.drag = None;
        self.orig_lev_size = data.orig_lev_size;
        self.doc = doc;
        self.has_level = true;
        let orig = self.serialize();
        self.orig_xml = Some(orig.clone());
        self.orig_xml_for.insert(sid.to_string(), orig.clone());
        self.orig_violations = self.run_rules(&self.doc);
        self.prime_orig_cost();
        self.draft_saved_at = None;
        let mut restored: Option<(String, &str)> = None;
        if let Some(draft) = self.drafts.load(sid).filter(|d| d.xml != orig) {
            let res = sanitize(self.backend.decomp(), &draft.xml, "saved draft");
            match res.root.filter(|_| res.ok) {
                Some(root) => {
                    self.undo_stack = vec![orig];
                    self.doc = root;
                    self.draft_saved_at = Some(draft.saved_at);
                    let msg = format!(
                        "Restored your edits from {} (autosave). Undo or Reset level to go back to the shipped level.",
                        (self.format_time)(draft.saved_at)
                    );
                    restored = Some((msg, "ok"));
                }
                None => {
                    let why = res.errors.first().cloned().unwrap_or_default();
                    restored = Some((format!("An autosave for this level failed validation and was not loaded ({}).", why), "err"));
                }
            }
        }
        self.model_cache = data.prefabs;
        self.exit_verts.borrow_mut().clear();
        self.build_entities();
        self.frame_level();
        if let Some(st) = self.settings.as_mut() {
            st.set_value("scene", sid);
        }
        self.emit(Event::Rebuilt);
        self.set_selection(Vec::new());
        self.schedule_import_plan();
        self.emit(Event::SceneLoaded);
        self.emit(Event::Edited);
        match restored {
            Some((msg, cls)) => self.set_status(&msg, cls),
            None => {
                let banks: Vec<&str> = self
                    .scene
                    .as_ref()
                    .and_then(|r| r.get("objectBanks"))
                    .and_then(|b| b.as_array())
                    .map(|b| b.iter().filter_map(|x| x.as_str()).collect())
                    .unwrap_or_default();
                let msg = format!(
                    "Loaded {} from the noclip data set: {} entities, {} models from {}.",
                    self.scene_str("landscape"),
                    (0..self.entities.len()).filter(|&i| self.whole(i)).count(),
                    self.models.len(),
                    banks.join(", ")
                );
                self.set_status(&msg, "");
            }
        }
        Ok(())
    }

    // -----------------------------------------------------------------------------------------------------------
    // Import XML: the window reads the file and checks it with `sanitize_level_xml`. Only after it passes does it
    // replace the level, as an undoable edit that is then autosaved.

    pub fn confirm_import(&mut self, res: &SanitizeResult) {
        let Some(root) = res.root.as_ref().filter(|_| res.ok) else { return };
        let Ok(doc) = Doc::parse(&root.serialize()) else { return };
        self.push_undo(self.serialize());
        self.selection.clear();
        self.doc = doc;
        self.after_edit(Some(&[]), false);
        let imp = self.current_imports().len();
        let mut msg = format!(
            "Imported {} into {} ({} commands). Undo (Ctrl+Z) reverts it.",
            res.info.name.as_deref().filter(|n| !n.is_empty()).unwrap_or("XML"),
            self.scene_str("name"),
            res.info.elements
        );
        if imp > 0 {
            msg += &format!(" It uses {} model(s) from other levels, imported at ROM build.", imp);
        }
        self.set_status(&msg, "ok");
    }

    // -----------------------------------------------------------------------------------------------------------
    // Build and export

    /// Builds the level now: `fmt_` is "lev", "xml", "rom" or "bps". The error is the back end's.
    pub fn build(&mut self, fmt_: &str, quickboot: bool) -> Result<BuildOutput, String> {
        self.build_job(fmt_, quickboot)?.run()
    }

    /// The build as a job to run later, on a worker thread.
    pub fn build_job(&mut self, fmt_: &str, quickboot: bool) -> Result<BuildJob, String> {
        let xml = self.serialize();
        let imports = self.current_imports();
        let mut notes = Vec::new();
        if !matches!(fmt_, "rom" | "bps") && !imports.is_empty() {
            notes.push(format!(
                "Note: {} imported model{} only exist in a ROM built here; this {} alone references models the game won't find.",
                imports.len(),
                if imports.len() == 1 { "" } else { "s" },
                if fmt_ == "lev" { ".lev" } else { "XML" }
            ));
        }
        if fmt_ == "lev" && !self.mod_users().is_empty() {
            notes.push("Note: rotations and scales on enemies, platforms, powerups or water only work in a ROM built here, which adds the transforms game mod. In an unmodded game this .lev turns or scales the previous landscape piece instead.".to_string());
        }
        if fmt_ == "xml" {
            let name = format!("{}.xml", self.scene_str("landscape"));
            return Ok(BuildJob::Ready(BuildOutput { data: self.doc.pretty().into_bytes(), name, message: notes.join(" ") }));
        }
        let info = self.level_info();
        let sid = self.scene_id().to_string();
        if info.extra && self.can_be_extra() && matches!(fmt_, "rom" | "bps") {
            // An extra level is built behind the game mod, with the retail level left in its slot
            return self.build_all_job(&[sid.clone()], fmt_, quickboot.then_some(sid));
        }
        let info = Info { name: info.name, song: info.song, bonus_time: info.bonus_time };
        let any = info.name.is_some() || info.song.is_some() || info.bonus_time.is_some();
        let request = BuildRequest {
            sid,
            xml,
            format: fmt_.to_string(),
            quickboot,
            imports,
            expansion: self.use_mod(),
            copy: self.copy_imports,
            info: any.then_some(info),
            created: None,
        };
        Ok(BuildJob::One {
            backend: self.backend.clone(),
            request,
            notes,
            scene_name: self.scene_str("name").to_string(),
            orig_size: self.orig_lev_size.unwrap_or(0) as i64,
        })
    }

    // Every edited level in one ROM or patch

    /// The levels with edits: a draft, a title, music or clock, or the open level's unsaved edits, as (id, name,
    /// XML). The XML is None for a level whose file is the shipped one.
    pub fn project_levels(&mut self) -> Vec<(String, String, Option<String>)> {
        self.flush_draft();
        let mut sids: BTreeSet<String> = self.drafts.list().into_keys().collect();
        sids.extend(self.drafts.all_info().into_iter().map(|(k, _)| k));
        let open = self.scene.is_some().then(|| self.scene_id().to_string());
        let edited = self.is_edited();
        if let (Some(o), true) = (&open, edited) {
            sids.insert(o.clone());
        }
        let mut out = Vec::new();
        for row in &self.scenes {
            let sid = s(row.get("id"));
            if !sids.contains(sid) {
                continue;
            }
            let xml = if open.as_deref() == Some(sid) {
                edited.then(|| self.serialize())
            } else {
                self.drafts.load(sid).filter(|d| sanitize(self.backend.decomp(), &d.xml, "saved draft").ok).map(|d| d.xml)
            };
            out.push((sid.to_string(), s(row.get("name")).to_string(), xml));
        }
        out
    }

    /// What `build_all` needs for one level: its XML, the models it imports, its table row. And whether it needs
    /// the game mod (rotations, imports from the ROM, an extra level, or more scenery than the game's heap holds).
    pub fn level_entry(&self, sid: &str, xml: Option<&str>) -> Result<(BuildAllEntry, bool), String> {
        let shipped = |ed: &Editor| ed.backend.level_xml(sid).map_err(|e| e.0).and_then(|x| Doc::parse(&x).map_err(|e| e.0));
        let root = match xml {
            Some(x) => Doc::parse(x).map_err(|e| e.0)?,
            None => shipped(self)?,
        };
        let is_open = self.scene.is_some() && sid == self.scene_id();
        let loaded = if is_open { self.loaded.clone() } else { self.backend.loaded_ids(sid).map_err(|e| e.0)? };
        let ids: BTreeSet<String> = root.iter(root.root()).into_iter().filter_map(|n| hex_id(root.get(n, "object_id"))).collect();
        let imports: Vec<String> = ids.into_iter().filter(|i| !loaded.contains(i) && self.catalog.contains_key(i)).collect();
        let info = self.drafts.info(sid);
        let index = self.scene_row(sid).and_then(|r| r.get("index")).and_then(|i| i.as_i64()).unwrap_or(-1);
        let extra = info.extra && EXTRA_SLOTS.contains(&index);
        let rot = root
            .iter_tags(root.root(), &["SetActorRotation", "SetActorScale"])
            .into_iter()
            .any(|n| root.parent(n).is_some_and(|p| MOD_PARENTS.contains(&root.tag(p))));
        let custom = |i: &String| self.catalog.get(i).is_some_and(|c| c.custom.is_some());
        let runtime = imports.iter().any(|i| !custom(i)) && !self.copy_imports;
        let mut heap = is_open && self.use_mod();
        if let (false, Some(free), true) = (heap, self.heap_free(sid), xml.is_some()) {
            // Added scenery (scenery_budget) and copied imports against the level's measured free heap
            let delta = self.scenery_cost(&root).0 - self.scenery_cost(&shipped(self)?).0;
            let copied: i64 = if runtime { 0 } else { imports.iter().map(|i| self.catalog.get(i).map_or(0, |c| c.bytes as i64)).sum() };
            heap = free - 16 * 1024 - delta.max(0) - copied < 0;
        }
        let any = info.name.is_some() || info.song.is_some() || info.bonus_time.is_some();
        let entry = BuildAllEntry {
            sid: sid.to_string(),
            xml: xml.map(|x| x.to_string()).unwrap_or_else(|| root.serialize()),
            imports,
            info: any.then_some(Info { name: info.name, song: info.song, bonus_time: info.bonus_time }),
            extra,
        };
        Ok((entry, rot || runtime || extra || heap))
    }

    /// The levels `sids` in one ROM or BPS patch, now.
    pub fn build_all(&mut self, sids: &[String], fmt_: &str, quickboot: Option<String>) -> Result<BuildOutput, String> {
        self.build_all_job(sids, fmt_, quickboot)?.run()
    }

    /// `build_all` as a job to run later on a worker thread. The editor's state is read now.
    pub fn build_all_job(&mut self, sids: &[String], fmt_: &str, quickboot: Option<String>) -> Result<BuildJob, String> {
        let (mut entries, mut names, mut with_mod) = (Vec::new(), Vec::new(), false);
        for (sid, name, xml) in self.project_levels() {
            if sids.contains(&sid) {
                let (e, m) = self.level_entry(&sid, xml.as_deref())?;
                entries.push(e);
                names.push(name);
                with_mod = with_mod || m;
            }
        }
        if entries.is_empty() {
            return Err("No levels to build.".to_string());
        }
        let request = BuildAllRequest { entries, format: fmt_.to_string(), quickboot, expansion: with_mod, copy: self.copy_imports, created: None };
        Ok(BuildJob::All { backend: self.backend.clone(), request, names })
    }
}
