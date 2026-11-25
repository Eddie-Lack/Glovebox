//! Edits. Every edit is a change to the XML with an undo snapshot. Also the autosave, and adding, copying and
//! removing things.

use super::nodes::fog_of;
use super::{format_of, look_name, Editor, Event, ModelIndex, ENTRY_TAG, EXTRA_SLOTS, LIGHTS_MAX, LOOKS, LOOK_ONE};
use crate::drafts::LevelInfo;
use crate::entities::{
    self, attached_points, lone_trigger, preset_zone, set_camera_zone, trigger_volumes, Context, EntId, CAM_PRESETS,
    WHY_TRIGGER_EDIT,
};
use crate::presets;
use crate::scene::{self, RayOptions};
use crate::vec::{self, V3};
use crate::xmlutil::{fmt, hex_id, num0, r1};
use glovebox_core::xml::{Doc, NodeId};
use std::time::{SystemTime, UNIX_EPOCH};

/// The script a new enemy gets when no vanilla one exists for its type: rest forever. An enemy with no normal
/// instruction crashes the parser (NMEGetNewInstruction reads normalList[0]).
const REST: [(&str, &str); 6] = [
    ("instr_type", "4"),
    ("lifetime", "0"),
    ("execution_condition_param_a", "0.0"),
    ("execution_condition_param_b", "0.0"),
    ("flags", "0"),
    ("execution_condition", "always"),
];

/// What names an entity across a rebuild.
pub(crate) struct SelKey {
    node: NodeId,
    path: Vec<usize>,
    handle: Option<String>,
}

fn wall_clock() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

/// The text with its first underscore as a space.
fn spaced(s: &str) -> String {
    s.replacen('_', " ", 1)
}

impl Editor {
    // -----------------------------------------------------------------------------------------------------------
    // Entities

    /// Builds the entities and their scene nodes again from the XML.
    pub fn build_entities(&mut self) {
        self.gizmo.detach();
        let be = self.backend.clone();
        let fmt_ = format_of(be.decomp());
        let level = {
            let index = ModelIndex { models: &self.models, loaded: &self.loaded, catalog: &self.catalog };
            let sid = self.scene.as_ref().and_then(|s| s.get("id")).and_then(|v| v.as_str()).unwrap_or("");
            let cx = Context { format: &fmt_, tables: be.tables(), scene_id: sid, models: &index };
            entities::build(&self.doc, self.doc.root(), &cx)
        };
        self.entities = level.ents;
        self.selection.clear();
        self.build_nodes();
        (self.background, self.level_fog) = fog_of(&self.doc, level.fog);
        self.ambient = match level.ambient {
            Some(a) => ["r", "g", "b"].map(|c| num0(&self.doc, a, c) / 255.0 + 0.35),
            None => [1.0, 1.0, 1.0],
        };
        self.apply_visibility();
    }

    // -----------------------------------------------------------------------------------------------------------
    // Undo

    pub fn push_undo(&mut self, before: String) {
        if self.scrub.is_some() {
            return;
        }
        self.undo_stack.push(before);
        if self.undo_stack.len() > 200 {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    /// Runs an edit. When it changed the XML, that's one undo step and the entities are built again. `f` returns
    /// the elements to select afterward, or None to keep the selection. True when the level changed.
    pub fn mutate(&mut self, f: impl FnOnce(&mut Editor) -> Option<Vec<NodeId>>) -> bool {
        let before = self.serialize();
        let select = f(self);
        if self.serialize() == before {
            return false;
        }
        self.push_undo(before);
        self.after_edit(select.as_deref(), false);
        true
    }

    /// `mutate` that keeps the selection.
    pub fn edit(&mut self, f: impl FnOnce(&mut Editor)) -> bool {
        self.mutate(|ed| {
            f(ed);
            None
        })
    }

    pub(crate) fn selection_keys(&self) -> Vec<SelKey> {
        self.selection
            .iter()
            .map(|&i| {
                let e = &self.entities[i];
                let node = self.key_node(i);
                SelKey { node, path: self.doc.path_of(node), handle: e.is_handle.then(|| e.label.clone()) }
            })
            .collect()
    }

    fn find_by_key(&self, k: &SelKey, by_path: bool) -> Option<EntId> {
        let n = if by_path { self.doc.node_at(&k.path)? } else { k.node };
        (0..self.entities.len()).find(|&i| {
            let e = &self.entities[i];
            let same = match &k.handle {
                Some(h) => e.label == *h,
                None => !e.is_handle,
            };
            !e.derived && same && self.key_node(i) == n
        })
    }

    /// Builds the entities again after a change to the XML and puts the selection back.
    pub fn after_edit(&mut self, select: Option<&[NodeId]>, by_path: bool) {
        let keys = self.selection_keys();
        self.finish_edit(keys, select, by_path);
    }

    /// `after_edit` with the selection's keys taken before the document was replaced.
    pub(crate) fn finish_edit(&mut self, keys: Vec<SelKey>, select: Option<&[NodeId]>, by_path: bool) {
        self.build_entities();
        let sel: Vec<EntId> = match select {
            Some(nodes) => nodes
                .iter()
                .filter_map(|n| (0..self.entities.len()).find(|&i| self.whole(i) && self.key_node(i) == *n))
                .collect(),
            None => keys.iter().filter_map(|k| self.find_by_key(k, by_path)).collect(),
        };
        self.emit(Event::Rebuilt);
        self.set_selection(sel);
        self.schedule_import_plan();
        self.emit(Event::Edited);
        self.schedule_draft_save();
    }

    /// A number field's drag began: every edit until `end_scrub` belongs to one undo step.
    pub fn begin_scrub(&mut self) {
        if self.scrub.is_none() && self.has_level {
            self.scrub = Some(self.serialize());
        }
    }

    /// True while a number field is being dragged.
    pub fn scrubbing(&self) -> bool {
        self.scrub.is_some()
    }

    /// The drag ended: one undo step when it changed the level.
    pub fn end_scrub(&mut self) {
        let Some(before) = self.scrub.take() else { return };
        if before != self.serialize() {
            self.push_undo(before);
            self.emit(Event::Edited);
        }
    }

    /// The drag was canceled: the level is as it was before, with no undo step.
    pub fn cancel_scrub(&mut self) {
        let Some(before) = self.scrub.take() else { return };
        if before == self.serialize() {
            return;
        }
        let keys = self.selection_keys();
        self.restore_keeping_puzzle(&before);
        self.finish_edit(keys, None, true);
    }

    pub fn undo(&mut self) {
        self.end_scrub();
        let Some(xml) = self.undo_stack.pop() else { return };
        self.redo_stack.push(self.serialize());
        let keys = self.selection_keys();
        self.restore_keeping_puzzle(&xml);
        self.finish_edit(keys, None, true);
    }

    pub fn redo(&mut self) {
        self.end_scrub();
        let Some(xml) = self.redo_stack.pop() else { return };
        self.undo_stack.push(self.serialize());
        let keys = self.selection_keys();
        self.restore_keeping_puzzle(&xml);
        self.finish_edit(keys, None, true);
    }

    /// Restores a snapshot. The open puzzle stays open only when it's still the same one.
    fn restore_keeping_puzzle(&mut self, xml: &str) {
        let before = self.puzzle_shown().map(|p| self.doc.serialize_node(p));
        let count = self.doc.kids(self.doc.root(), "Puzzle").len();
        self.restore(xml);
        let puzzles = self.doc.kids(self.doc.root(), "Puzzle");
        if let (Some(before), true) = (before, puzzles.len() != count) {
            // A puzzle came or went: find the open one by its content, or close it
            self.puzzle_index = puzzles.iter().position(|p| self.doc.serialize_node(*p) == before);
        }
    }

    pub fn is_edited(&self) -> bool {
        self.has_level && Some(self.serialize()) != self.orig_xml
    }

    // -----------------------------------------------------------------------------------------------------------
    // Autosave: the current level's XML is kept as a draft in the user folder after every edit, and restored when
    // the level is opened again. A level that matches the vanilla one has no draft.

    pub(crate) fn schedule_draft_save(&mut self) {
        self.draft_pending = Some((self.scene_id().to_string(), self.serialize()));
        match self.delays {
            Some(d) => self.draft_due = Some(self.now + d.draft),
            None => self.flush_draft(),
        }
    }

    /// Saves the waiting draft now.
    pub fn flush_draft(&mut self) {
        self.draft_due = None;
        let Some((sid, xml)) = self.draft_pending.take() else { return };
        let open = self.scene.is_some() && sid == self.scene_id();
        if self.orig_xml_for.get(&sid) == Some(&xml) {
            self.drafts.delete(&sid);
            if open {
                self.draft_saved_at = None;
            }
        } else if self.drafts.save(&sid, &xml) {
            if open {
                self.draft_saved_at = Some(wall_clock());
            }
        } else {
            self.set_status("Couldn't autosave: the user folder can't be written. Export XML to keep your edits.", "err");
        }
        self.events.push(Event::Edited);
    }

    /// Back to the vanilla level (undoable). The window asks first.
    pub fn reset_level(&mut self) {
        let Some(orig) = self.orig_xml.clone() else { return };
        self.push_undo(self.serialize());
        self.selection.clear();
        self.restore(&orig);
        self.after_edit(Some(&[]), false);
        self.flush_draft();
        let msg = format!(
            "Reset {} to the vanilla level and deleted its autosave. Undo (Ctrl+Z) brings your edits back.",
            self.scene_str("name")
        );
        self.set_status(&msg, "ok");
    }

    // -----------------------------------------------------------------------------------------------------------
    // Adding things

    /// The nodes on which a ray meets the ground. `solid_only`: only what the game gives collision (LandActor and
    /// platforms), without `skip`.
    pub fn ground_objects(&self, skip: Option<EntId>, solid_only: bool) -> Vec<scene::NodeId> {
        let mut out = Vec::new();
        for (i, e) in self.entities.iter().enumerate() {
            if solid_only {
                if e.is_handle || e.derived || Some(i) == skip || !(e.tag == "LandActor" || e.kind == "platform") {
                    continue;
                }
            } else if !matches!(e.kind, "land" | "platform") {
                continue;
            }
            out.push(self.nodes[i].obj);
        }
        out
    }

    /// Where the view's center meets the ground (or the orbit's target).
    pub fn placement_point(&self) -> V3 {
        let Some((origin, d)) = self.camera.ray_ndc(0.0, 0.0) else { return self.orbit.target };
        let o = RayOptions { camera: Some(&self.camera), ..RayOptions::default() };
        let hits = self.graph.raycast(&self.ground_objects(None, false), origin, d, &o);
        hits.first().map(|h| h.point).unwrap_or(self.orbit.target)
    }

    /// The heading the editor's camera looks along: new zones are made for a player who walks away from the viewer.
    pub fn view_facing(&self) -> f64 {
        let d = self.camera.forward();
        d[0].atan2(d[2])
    }

    /// The group a new garib goes in: a new one after the level's last.
    fn new_garib_group(&mut self) -> NodeId {
        let root = self.doc.root();
        let mut top = -1.0;
        for g in self.doc.kids(root, "GaribGroup") {
            let v = num0(&self.doc, g, "group_id");
            if v > top {
                top = v;
            }
        }
        let id = (top.trunc() as i64).saturating_add(1).to_string();
        let g = self.doc.make("GaribGroup", &[("group_id", &id), ("initial_state", "1")]);
        self.insert_top(g)
    }

    pub(crate) fn make_garib(&mut self, p: V3, type_: &str) -> NodeId {
        self.doc.make("Garib", &[("x", &r1(p[0])), ("y", &r1(p[1])), ("z", &r1(p[2])), ("type", type_), ("dynamic_shadow", "0")])
    }

    pub(crate) fn garib_group(&mut self) -> NodeId {
        self.new_garib_group()
    }

    /// Adds a garib to the selected garib's group, or to a new group.
    pub fn add_garib(&mut self, pos: V3, type_: &str) {
        self.mutate(|ed| {
            let group = ed.selected().map(|s| &ed.entities[s]).filter(|e| e.tag == "GaribGroup").map(|e| e.node);
            let group = match group {
                Some(g) => g,
                None => ed.new_garib_group(),
            };
            let g = ed.make_garib(pos, type_);
            ed.doc.append(group, g);
            Some(vec![g])
        });
        let msg = format!("Added a {}. The game counts garibs as it parses the level. The total updates by itself.", spaced(type_));
        self.set_status(&msg, "ok");
    }

    /// Adds the element `make` returns at the end of the level and selects it.
    pub fn add_element(&mut self, make: impl FnOnce(&mut Editor) -> NodeId, msg: Option<&str>) {
        self.mutate(|ed| {
            let n = make(ed);
            Some(vec![ed.insert_top(n)])
        });
        if let Some(m) = msg {
            self.set_status(m, "ok");
        }
    }

    /// Adds a model of the catalog as `as_`: LandActor, BackgroundActor, AnimatedBackgroundActor or Platform.
    pub fn add_model(&mut self, oid: &str, as_: &str) {
        let info = self.catalog.get(oid).cloned();
        let p = self.placement_point();
        let full = info.as_ref().map(|i| i.name.as_str()).filter(|n| !n.is_empty());
        let name: String = full.unwrap_or("object").chars().take(7).collect();
        let imp = self.is_import(oid);
        let (x, y, z) = (r1(p[0]), r1(p[1]), r1(p[2]));
        let mut msg = format!("Added {} as {}.", full.unwrap_or(oid), as_);
        if let (true, Some(i)) = (imp, &info) {
            msg += &format!(" It comes from {} and is imported into {} when you build the ROM.", i.bank, self.scene_str("importObjectBank"));
        }
        self.add_element(
            |ed| {
                if as_ == "Platform" {
                    let n = ed.doc.make("Platform", &[("object_id", oid), ("name", &name)]);
                    let pos = ed.doc.make("PlatSetInitialPos", &[("x", &x), ("y", &y), ("z", &z)]);
                    ed.doc.append(n, pos);
                    n
                } else {
                    ed.doc.make(as_, &[("object_id", oid), ("name", &name), ("x", &x), ("y", &y), ("z", &z)])
                }
            },
            Some(&msg),
        );
    }

    /// New enemies copy a vanilla script for their type, shifted so its absolute coordinates follow the enemy.
    pub fn add_enemy(&mut self, type_: &str) {
        let be = self.backend.clone();
        let t = be.templates().get(type_).filter(|t| t.is_object());
        let template = t.and_then(|t| t.get("xml")).and_then(|x| x.as_str()).and_then(|x| Doc::parse(x).ok());
        let p = vec::add(self.placement_point(), [0.0, 4.0, 0.0]);
        let level = t.and_then(|t| t.get("level")).and_then(|l| l.as_str());
        let lvl = level.map(|l| self.scene_row(l).and_then(|s| s.get("name")).and_then(|n| n.as_str()).unwrap_or(l).to_string());
        let mid = self.enemy_model_id(Some(type_));
        let mut msg = format!("Added {}", type_);
        msg += &match (&template, &lvl) {
            (Some(_), Some(l)) => format!(" with the script it has in {}, moved to the new spot.", l),
            _ => ".".to_string(),
        };
        if let Some(c) = mid.as_deref().filter(|m| self.is_import(m)).and_then(|m| self.catalog.get(m)) {
            msg += &format!(" Its model comes from {} and is imported at ROM build.", c.bank);
        }
        self.add_element(
            |ed| {
                let n = match &template {
                    Some(src) => {
                        let n = ed.doc.import(src, src.root());
                        let d = [p[0] - num0(&ed.doc, n, "x"), p[1] - num0(&ed.doc, n, "y"), p[2] - num0(&ed.doc, n, "z")];
                        for b in attached_points(&ed.doc, n, None) {
                            let v = vec::add(b.get(&ed.doc, &[]), d);
                            b.set(&mut ed.doc, &[], v);
                        }
                        ed.doc.set(n, "puzzle_tag", "0");
                        n
                    }
                    None => {
                        let n = ed.doc.make(
                            "Enemy",
                            &[("type", type_), ("puzzle_tag", "0"), ("x", "0.0"), ("y", "0.0"), ("z", "0.0"), ("y_rotation", "0.0")],
                        );
                        let ins = ed.doc.make("EnemyInstruction", &REST);
                        let body = ed.doc.make("EnemyInstructionRest", &[("flags", "0"), ("anim_start_playing", "1")]);
                        ed.doc.append(ins, body);
                        let grp = ed.doc.make("NormalInstructions", &[]);
                        ed.doc.append(grp, ins);
                        ed.doc.append(n, grp);
                        let end = ed.doc.make("EnemyFinalize", &[]);
                        ed.doc.append(n, end);
                        n
                    }
                };
                for (i, a) in ["x", "y", "z"].iter().enumerate() {
                    ed.doc.set(n, a, &r1(p[i]));
                }
                n
            },
            Some(&msg),
        );
    }

    /// The ground height under a point, or the point's own height.
    fn ground_y(&self, q: V3) -> f64 {
        match self.ground_below([q[0], q[1] + 200.0, q[2]], 600.0, None, None) {
            Some(g) => g[1],
            None => q[1],
        }
    }

    /// A platform preset from `presets` at the view's ground point.
    pub fn add_preset(&mut self, key: &str) {
        let Some(preset) = presets::preset(key) else { return };
        let (p, f) = (self.placement_point(), self.view_facing());
        let root = self.doc.root();
        let mut model: Option<(String, String)> = None;
        if matches!(key, "mover" | "spinner" | "conveyor" | "ice") {
            if let Some(e) = self.selected().map(|s| &self.entities[s]).filter(|e| !e.is_handle) {
                if let Some(raw) = self.doc.get(e.node, "object_id").filter(|o| !o.is_empty()) {
                    let oid = hex_id(Some(raw)).unwrap_or_default();
                    let name = self.doc.get(e.node, "name").filter(|n| !n.is_empty()).unwrap_or("deck");
                    model = Some((oid, name.to_string()));
                }
            }
        }
        if key == "start" {
            // Glover starts at the first checkpoint in the level. A new one put before the level's own platforms
            // froze the game, so the first one moves instead.
            let first_ring = self.doc.kids(root, "Platform").into_iter().find(|n| self.doc.first(*n, "PlatCheckpoint").is_some());
            if let Some(ring) = first_ring {
                if self.doc.first(ring, "PlatSetInitialPos").is_none() {
                    return self.set_status("The level's start ring follows a path, so it can't be moved here. Move its path points.", "err");
                }
                self.mutate(|ed| {
                    if let Some(pos) = ed.doc.first(ring, "PlatSetInitialPos") {
                        for (i, a) in ["x", "y", "z"].iter().enumerate() {
                            ed.doc.set(pos, a, &fmt(p[i] + if *a == "y" { presets::RING_START } else { 0.0 }));
                        }
                    }
                    if let Some(c) = ed.doc.first(ring, "PlatCheckpoint") {
                        ed.doc.set(c, "theta", &fmt(f));
                    }
                    Some(vec![ring])
                });
                return self.set_status("Moved the level's start ring here, facing the way the view looks.", "ok");
            }
        }
        if key == "entry" && self.scene_index() > 7 {
            return self.set_status("The entry zone goes in a hub: the game's portals only work there.", "err");
        }
        let mut tag = self.free_tag();
        // The preset is built in a document of its own: its ground probe reads the editor
        let mut tmp = Doc::new("Level");
        let ground = |q: V3| self.ground_y(q);
        let m = model.as_ref().map(|(a, b)| (a.as_str(), b.as_str()));
        let Some(made) = presets::build(&mut tmp, key, p, f, Some(tag), m, Some(&ground)) else { return };
        if key == "ring" && self.level_info().extra {
            // In an extra level the mod lets a rolling ball touch a ring with tag 77 (mod/slots.c
            // Mod_CheckRestartGate)
            if let Some(first) = made.first() {
                let t = tmp.make("PlatSetTag", &[("tag", "77")]);
                tmp.append(*first, t);
            }
            tag = 77;
        }
        let tagged = made.iter().any(|n| tmp.first(*n, "PlatSetTag").is_some());
        let imp = made.iter().any(|n| tmp.get(*n, "object_id").filter(|o| !o.is_empty()).and_then(|o| hex_id(Some(o))).is_some_and(|o| self.is_import(&o)));
        self.mutate(|ed| {
            let mut first = None;
            for n in &made {
                let c = ed.doc.import(&tmp, *n);
                ed.insert_top(c);
                if first.is_none() && ed.doc.tag(c) == "Platform" {
                    first = Some(c);
                }
            }
            Some(first.into_iter().collect())
        });
        let mut msg = format!("Added {}.", preset.name.to_lowercase());
        if tagged {
            msg += &format!(" Its platform tag is {}.", tag);
        }
        if imp {
            msg += " Its model is imported at ROM build.";
        }
        self.set_status(&msg, "ok");
    }

    /// `name` is the powerup's name in the decompilation's Pickup enum.
    pub fn add_powerup(&mut self, v: i64, name: &str) {
        let p = vec::add(self.placement_point(), [0.0, 10.0, 0.0]);
        let msg = format!("Added a {} powerup (Pickup::{}).", name.to_lowercase(), name);
        self.add_element(
            |ed| {
                ed.doc.make(
                    "Powerup",
                    &[("type", &v.to_string()), ("u16_0x02", "300"), ("u16_0x04", "300"), ("x", &r1(p[0])), ("y", &r1(p[1])), ("z", &r1(p[2]))],
                )
            },
            Some(&msg),
        );
    }

    pub fn add_tip(&mut self) {
        let p = vec::add(self.placement_point(), [0.0, 12.0, 0.0]);
        self.add_element(
            |ed| ed.doc.make("MrTip", &[("x", &r1(p[0])), ("y", &r1(p[1])), ("z", &r1(p[2])), ("message_id", "1")]),
            Some("Added Mr. Tip. Set message_id in the inspector."),
        );
    }

    /// A new puzzle with one box condition and the camera actions of `zone`. Returns the box.
    pub(crate) fn make_zone(&mut self, corner: V3, size: V3, zone: &entities::Zone) -> NodeId {
        let puzzle = self.doc.make("Puzzle", &[]);
        self.insert_top(puzzle);
        let c = self.doc.make("PuzzleCond", &[("cond_type", "35")]);
        let vol = self.doc.make(
            "PuzzleCondGloverWithinVolume",
            &[("x", &r1(corner[0])), ("y", &r1(corner[1])), ("z", &r1(corner[2])), ("l", &r1(size[0])), ("w", &r1(size[1])), ("h", &r1(size[2]))],
        );
        self.doc.append(c, vol);
        self.doc.append(puzzle, c);
        set_camera_zone(&mut self.doc, puzzle, zone);
        vol
    }

    /// A camera zone of one of `CAM_PRESETS` at the view's ground point.
    pub fn add_camera_zone(&mut self, key: &str) {
        let (p, f) = (self.placement_point(), self.view_facing());
        let Some(zone) = preset_zone(key, f, vec::add(p, [0.0, 100.0, 0.0])) else { return };
        let Some((_, label, note)) = CAM_PRESETS.iter().find(|c| c.0 == key) else { return };
        self.mutate(|ed| {
            let vol = ed.make_zone([p[0] - 150.0, p[1] - 100.0, p[2] - 150.0], [300.0, 400.0, 300.0], &zone);
            Some(vec![vol])
        });
        let msg = format!(
            "Added a {} camera zone. While Glover is inside the box the camera {}; it goes back when he leaves.",
            label.to_lowercase(),
            note
        );
        self.set_status(&msg, "ok");
    }

    /// Copies the selection, 40 units to the side.
    pub fn duplicate(&mut self) {
        if self.selection.is_empty() {
            return;
        }
        self.mutate(|ed| {
            let mut clones = Vec::new();
            let mut done = Vec::new();
            for &i in &ed.selection.clone() {
                let ent = ed.entities[i].clone();
                let trigger = ent.kind == "trigger";
                if ent.derived || (trigger && !lone_trigger(&ed.doc, &ent)) {
                    continue;
                }
                let src = if ent.tag == "GaribGroup" {
                    ent.sub
                } else if ent.is_handle {
                    None
                } else {
                    Some(ent.node)
                };
                let Some(src) = src.filter(|s| !done.contains(s)) else { continue };
                done.push(src);
                let c = ed.doc.deep_copy(src);
                ed.doc.add_next(src, c);
                clones.push(c);
                // Nudge the copy (and its script and path points) 40 units along x, off the original
                for a in ["x", "origin_x", "left", "end_1_x", "end_2_x"] {
                    if ed.doc.has(c, a) {
                        let v = fmt(num0(&ed.doc, c, a) + 40.0);
                        ed.doc.set(c, a, &v);
                    }
                }
                for b in attached_points(&ed.doc, c, None) {
                    let v = vec::add(b.get(&ed.doc, &[]), [40.0, 0.0, 0.0]);
                    b.set(&mut ed.doc, &[], v);
                }
                if trigger {
                    let vols = trigger_volumes(&ed.doc, c);
                    for v in vols.iter().copied().chain(ed.doc.iter_tags(c, &["PuzzleActionCameraFlyTowardsPoint"])) {
                        let x = fmt(num0(&ed.doc, v, "x") + 40.0);
                        ed.doc.set(v, "x", &x);
                    }
                    if let (Some(v), Some(last)) = (vols.first(), clones.last_mut()) {
                        *last = *v;
                    }
                }
                if ent.tag == "Platform" {
                    // A copy shouldn't answer to the original's puzzle tag
                    if let Some(t) = ed.doc.first(c, "PlatSetTag") {
                        ed.doc.detach(t);
                    }
                }
            }
            Some(clones)
        });
    }

    /// Deletes the selection.
    pub fn remove(&mut self) {
        if self.selection.is_empty() {
            return;
        }
        let stays = |ed: &Editor, i: EntId| ed.entities[i].kind == "trigger" && !lone_trigger(&ed.doc, &ed.entities[i]);
        let kept = self.selection.iter().filter(|&&i| stays(self, i)).count();
        let n = self.selection.len() - kept;
        self.mutate(|ed| {
            for &i in &ed.selection.clone() {
                let ent = ed.entities[i].clone();
                if ent.derived || stays(ed, i) {
                    continue;
                }
                if ent.tag == "GaribGroup" {
                    let grp = ent.node;
                    if let Some(s) = ent.sub.filter(|s| ed.doc.parent(*s).is_some()) {
                        ed.doc.detach(s);
                    }
                    if ed.doc.kids(grp, "Garib").is_empty() && ed.doc.parent(grp).is_some() {
                        ed.doc.detach(grp);
                    }
                } else if ent.is_handle {
                    if let Some(s) = ent.sub.filter(|s| ed.doc.parent(*s).is_some()) {
                        ed.doc.detach(s);
                    }
                } else if ed.doc.parent(ent.node).is_some() {
                    ed.doc.detach(ent.node);
                }
            }
            Some(Vec::new())
        });
        let mut parts = Vec::new();
        if n > 0 || kept == 0 {
            parts.push(format!("Deleted {} object{}.", n, if n == 1 { "" } else { "s" }));
        }
        if kept > 0 {
            parts.push(WHY_TRIGGER_EDIT.to_string());
        }
        self.set_status(&parts.join(" "), "");
    }

    pub fn set_attr(&mut self, node: NodeId, name: &str, value: &str) {
        self.edit(|ed| ed.doc.set(node, name, value));
    }

    // The level's row in the game's level table: its name in the level select, its music and its bonus clock

    pub fn level_info(&self) -> LevelInfo {
        if self.scene.is_some() {
            self.drafts.info(self.scene_id())
        } else {
            LevelInfo::default()
        }
    }

    /// Extra levels go in the Space slots 35, 36, 37 (levels), 38 (boss) and 41 (bonus). 39 and 40 have fixed
    /// game code.
    pub fn can_be_extra(&self) -> bool {
        self.scene.is_some() && EXTRA_SLOTS.contains(&self.scene_index())
    }

    /// Changes the level's name, music, clock or "extra level" setting.
    pub fn set_level_info(&mut self, change: impl FnOnce(&mut LevelInfo)) {
        let mut info = self.level_info();
        change(&mut info);
        if !self.drafts.save_info(self.scene_id(), &info) {
            return self.set_status("Couldn't save the level's settings in the user folder.", "err");
        }
        self.set_status("Saved. Build ROM and Export patch write it into the game's level table.", "ok");
        self.events.push(Event::Edited);
    }

    /// Puts new level-wide commands at the start of the level, in the order given.
    fn insert_first(&mut self, new: &[NodeId]) {
        let root = self.doc.root();
        match self.doc.children(root).first().copied() {
            Some(first) => {
                for n in new {
                    self.doc.add_prev(first, *n);
                }
            }
            None => {
                for n in new {
                    self.doc.append(root, *n);
                }
            }
        }
    }

    /// Writes a look preset: the level's ambient light, directional lights and fog, in one undo step.
    pub fn apply_look(&mut self, name: &str) {
        let Some(look) = LOOKS.iter().find(|l| l.name == name) else { return };
        self.edit(|ed| {
            let root = ed.doc.root();
            for t in ["AmbientLight", "DiffuseLight", "FogConfiguration"] {
                for n in ed.doc.kids(root, t) {
                    ed.doc.detach(n);
                }
            }
            let s = |v: i64| v.to_string();
            let a = look.ambient;
            let mut new = vec![ed.doc.make("AmbientLight", &[("r", &s(a[0])), ("g", &s(a[1])), ("b", &s(a[2]))])];
            for (r, g, b, tx, ty) in look.lights {
                new.push(ed.doc.make("DiffuseLight", &[("r", &s(*r)), ("g", &s(*g)), ("b", &s(*b)), ("theta_x", &fmt(*tx)), ("theta_y", &fmt(*ty))]));
            }
            let g = look.fog;
            new.push(ed.doc.make(
                "FogConfiguration",
                &[("fog_enabled", "1"), ("r", &s(g[0])), ("g", &s(g[1])), ("b", &s(g[2])), ("fog_distance", &s(g[3])), ("near_clip", &s(g[4]))],
            ));
            ed.insert_first(&new);
        });
        self.set_status(&format!("The level now looks \u{201c}{}\u{201d}.", name), "ok");
    }

    /// Adds a level-wide command (fog, a light and so on) at the start of the level.
    pub fn add_look(&mut self, tag: &str) {
        let root = self.doc.root();
        let what = look_name(tag).unwrap_or(tag).to_lowercase();
        if LOOK_ONE.contains(&tag) && self.doc.first(root, tag).is_some() {
            return self.set_status(&format!("The level already has {}.", what), "err");
        }
        if tag == "DiffuseLight" && self.doc.kids(root, tag).len() >= LIGHTS_MAX {
            return self.set_status(&format!("The game uses {} directional lights at most (AddBackgroundLight).", LIGHTS_MAX), "err");
        }
        let mut texture = None;
        if tag == "Backdrop" {
            // The game stops with LEVEL UNPLAYABLE when the texture isn't in the level's banks (FindTexture, vital)
            let have = self.backend.backdrops(self.scene_id()).unwrap_or_default();
            match have.into_iter().next() {
                Some(t) => texture = Some(t),
                None => return self.set_status("None of the vanilla backdrop textures is in this level's texture banks.", "err"),
            }
        }
        let be = self.backend.clone();
        self.edit(|ed| {
            let attrs: Vec<(&str, &str)> = texture.iter().map(|t| ("texture_id", t.as_str())).collect();
            let n = format_of(be.decomp()).make(&mut ed.doc, tag, &attrs);
            ed.insert_first(&[n]);
        });
        self.set_status(&format!("Added {}. Its fields are in the Level settings list (deselect everything to see it).", what), "ok");
    }

    /// A platform tag no platform has. The mod reads 77 (rings) and 99 (the entry zone).
    pub fn free_tag(&self) -> i64 {
        let used: Vec<f64> = self.doc.iter_tags(self.doc.root(), &["PlatSetTag"]).into_iter().map(|t| num0(&self.doc, t, "tag")).collect();
        let mut t = 1;
        while used.contains(&(t as f64)) || t == 77 || t == ENTRY_TAG {
            t += 1;
        }
        t
    }
}
