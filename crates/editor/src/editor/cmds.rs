//! Edits inside elements: commands (the inspector's Add command, remove, up and down), enemy scripts, puzzles,
//! camera zones. Also the layers.

use super::{format_of, look_name, Editor, Event, GROUPS, LOOK_KEEP, MOD_NOTE};
use crate::commands::{self, missing_need, platform_rules, refusal, PLATFORMS};
use crate::entities::{camera_zone, set_camera_zone, Zone, MOD_PARENTS};
use crate::exits;
use crate::puzzles;
use crate::scripts;
use crate::vec::V3;
use crate::xmlutil::{body, fmt, inum, num0};
use glovebox_core::xml::NodeId;

impl Editor {
    // Commands inside elements

    /// Values for a new command that depend on where it goes: positions near the platform, a free tag.
    pub fn command_attrs(&self, parent: NodeId, tag: &str) -> Vec<(String, String)> {
        let doc = &self.doc;
        let start = doc.first(parent, "PlatSetInitialPos").or_else(|| doc.first(parent, "PlatPathPoint"));
        let at: V3 = match start {
            Some(s) => [num0(doc, s, "x"), num0(doc, s, "y"), num0(doc, s, "z")],
            None => [0.0; 3],
        };
        let xyz = |q: V3| vec![("x".to_string(), fmt(q[0])), ("y".to_string(), fmt(q[1])), ("z".to_string(), fmt(q[2]))];
        match tag {
            "PlatPathPoint" => {
                let q = match doc.kids(parent, "PlatPathPoint").last() {
                    Some(p) => [num0(doc, *p, "x") + 100.0, num0(doc, *p, "y"), num0(doc, *p, "z")],
                    None => at,
                };
                xyz(q)
            }
            "PlatOrbitAroundPoint" => xyz([at[0] + 100.0, at[1], at[2]]),
            "PlatSetInitialPos" => xyz(at),
            "PlatSetTag" => vec![("tag".to_string(), self.free_tag().to_string())],
            "PlatDestructible" => match doc.get(parent, "object_id").filter(|o| !o.is_empty()) {
                Some(o) => vec![("fragment_object_id".to_string(), o.to_string())],
                None => Vec::new(),
            },
            _ => Vec::new(),
        }
    }

    /// Adds a command to an element, where the game reads it. Returns the new element, or None with the reason
    /// in the status line.
    pub fn add_command(&mut self, parent: NodeId, tag: &str, attrs: &[(&str, &str)]) -> Option<NodeId> {
        let be = self.backend.clone();
        let fmt_ = format_of(be.decomp());
        let ptag = self.doc.tag(parent).to_string();
        if let Some(why) = refusal(&self.doc, parent, tag) {
            self.set_status(&format!("Can't add {} to <{}>: {}", fmt_.label(tag), ptag, why), "err");
            return None;
        }
        let mut a = self.command_attrs(parent, tag);
        for (k, v) in attrs {
            match a.iter_mut().find(|(n, _)| n == k) {
                Some(e) => e.1 = v.to_string(),
                None => a.push((k.to_string(), v.to_string())),
            }
        }
        let mut made = None;
        self.edit(|ed| {
            let pairs: Vec<(&str, &str)> = a.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
            let n = fmt_.make(&mut ed.doc, tag, &pairs);
            made = Some(commands::insert(&mut ed.doc, parent, n));
        });
        let mut msg = format!("Added {} to <{}>.", fmt_.label(tag), ptag);
        if matches!(tag, "SetActorRotation" | "SetActorScale") && MOD_PARENTS.contains(&ptag.as_str()) {
            msg = msg + " " + MOD_NOTE;
        }
        self.set_status(&msg, "ok");
        made
    }

    /// Why an element can't be removed, or None.
    pub fn removal_refused(&self, node: NodeId) -> Option<String> {
        let doc = &self.doc;
        let (root, tag) = (doc.root(), doc.tag(node));
        let parent = doc.parent(node);
        let top = parent == Some(root);
        if top && tag == "EndLevelData" {
            return Some("EndLevelData ends the level: the game parses until it.".into());
        }
        if top && LOOK_KEEP.contains(&tag) {
            let what = look_name(tag).unwrap_or(tag).to_lowercase();
            return Some(format!("Every level keeps its {}. Without one the game keeps the last screen's.", what));
        }
        if top && tag == "DiffuseLight" && doc.kids(root, "DiffuseLight").len() == 1 {
            return Some("Every level keeps one directional light.".into());
        }
        if tag == "EnemyFinalize" {
            return Some("EnemyFinalize ends the enemy: the game reads everything after it as the next object.".into());
        }
        let enemy = if tag == "NormalInstructions" {
            parent
        } else {
            parent.filter(|p| doc.tag(*p) == "NormalInstructions").and_then(|p| doc.parent(p))
        };
        if let Some(enemy) = enemy.filter(|e| doc.tag(*e) == "Enemy") {
            let other = doc
                .kids(enemy, "NormalInstructions")
                .into_iter()
                .filter(|g| *g != node)
                .any(|g| doc.children(g).iter().any(|c| *c != node));
            if !other {
                return Some("An enemy needs at least one normal instruction (NMEGetNewInstruction reads the first).".into());
            }
        }
        None
    }

    /// Removes an element, with what is left with nothing to act on.
    pub fn remove_node(&mut self, node: NodeId) {
        let Some(parent) = self.doc.parent(node) else { return };
        if let Some(why) = self.removal_refused(node) {
            return self.set_status(&why, "err");
        }
        let root = self.doc.root();
        let tag = self.doc.tag(node).to_string();
        // Backdrops are counted from 1 by decal_parent_idx, and path points from 0 by PlatPathSetStartingPoint
        let nth = self.doc.kids(parent, &tag).iter().position(|n| *n == node).unwrap_or(0) as i64;
        let orphans: Vec<NodeId> = self
            .doc
            .children(parent)
            .iter()
            .enumerate()
            .filter(|(j, c)| missing_need(&self.doc, parent, self.doc.tag(**c), Some(*j)).is_some())
            .map(|(_, c)| *c)
            .collect();
        let mut gone: Vec<String> = Vec::new();
        self.mutate(|ed| {
            let doc = &mut ed.doc;
            doc.detach(node);
            if tag == "Backdrop" && parent == root {
                // The later ones move down, and one that named this one loses its parent
                for b in doc.kids(root, "Backdrop") {
                    let v = inum(doc, Some(b), "decal_parent_idx", 0);
                    if v == nth + 1 {
                        doc.set(b, "decal_parent_idx", "0");
                    } else if v > nth + 1 {
                        doc.set(b, "decal_parent_idx", &(v - 1).to_string());
                    }
                }
            }
            if tag == "PlatPathPoint" {
                for sp in doc.kids(parent, "PlatPathSetStartingPoint") {
                    let v = inum(doc, Some(sp), "point_idx", 0);
                    if v > nth {
                        doc.set(sp, "point_idx", &(v - 1).to_string());
                    }
                }
            }
            // Children left with nothing to act on go with it (not ones that had nothing before)
            loop {
                let lost = doc
                    .children(parent)
                    .iter()
                    .enumerate()
                    .find(|(j, c)| !orphans.contains(c) && missing_need(doc, parent, doc.tag(**c), Some(*j)).is_some())
                    .map(|(_, c)| *c);
                let Some(c) = lost else { break };
                gone.push(doc.tag(c).to_string());
                doc.detach(c);
            }
            // An empty instruction group breaks the build (libgarib reads its last child)
            if GROUPS.contains(&doc.tag(parent)) && doc.children(parent).is_empty() && doc.parent(parent).is_some() {
                doc.detach(parent);
            }
            (parent == root).then(Vec::new)
        });
        if !gone.is_empty() {
            let fmt_ = format_of(self.backend.decomp());
            let names: Vec<&str> = gone.iter().map(|t| fmt_.label(t)).collect();
            let msg = format!(
                "Also removed {}: {} had nothing to act on without it.",
                names.join(", "),
                if gone.len() == 1 { "it" } else { "they" }
            );
            drop(fmt_);
            self.set_status(&msg, "");
        }
    }

    /// Moves an element one place earlier (-1) or later (+1) among its siblings. The top level keeps EndLevelData
    /// last, an enemy keeps EnemyFinalize last, and a platform refuses an order the game reads wrong.
    pub fn move_node(&mut self, node: NodeId, delta: i32) {
        let Some(parent) = self.doc.parent(node) else { return };
        let sib = self.doc.children(parent).to_vec();
        let Some(i) = sib.iter().position(|n| *n == node) else { return };
        let j = i as i64 + i64::from(delta);
        if j < 0 || j >= sib.len() as i64 {
            return;
        }
        let j = j as usize;
        let other = sib[j];
        if parent == self.doc.root() && self.doc.tag(other) == "EndLevelData" {
            return;
        }
        if self.doc.tag(node) == "EnemyFinalize" || self.doc.tag(other) == "EnemyFinalize" {
            return self.set_status("EnemyFinalize stays last: it ends the enemy.", "err");
        }
        if self.doc.tag(parent) == "Puzzle" && puzzles::rank(&self.doc, other) != puzzles::rank(&self.doc, node) {
            return; // conditions stay among conditions, actions among actions
        }
        if PLATFORMS.contains(&self.doc.tag(parent)) {
            // Try the move on a copy
            let trial = self.doc.deep_copy(parent);
            let tk = self.doc.children(trial).to_vec();
            if delta < 0 {
                self.doc.add_prev(tk[j], tk[i]);
            } else {
                self.doc.add_next(tk[j], tk[i]);
            }
            let before: Vec<String> = platform_rules(&self.doc, parent).into_iter().map(|r| r.key).collect();
            let new = platform_rules(&self.doc, trial).into_iter().find(|r| !before.contains(&r.key));
            if let Some(r) = new {
                return self.set_status(&format!("Can't move it there: {}", r.message), "err");
            }
        }
        self.edit(|ed| {
            if delta < 0 {
                ed.doc.add_prev(other, node);
            } else {
                ed.doc.add_next(other, node);
            }
        });
    }

    /// Puts an element's commands in the order the game reads them right.
    pub fn tidy_node(&mut self, node: NodeId) {
        let mut moved = false;
        self.edit(|ed| moved = commands::tidy(&mut ed.doc, node));
        let msg = if moved { "Moved the commands the game would read in the wrong order." } else { "The order is already right." };
        self.set_status(msg, "ok");
    }

    // Enemy scripts (the script panel in the inspector)

    /// Adds an instruction of type `t` to the enemy's list `kind` (one of `GROUPS`).
    pub fn add_instruction(&mut self, enemy: NodeId, kind: &str, t: i128) {
        if scripts::list_of(&self.doc, enemy, kind).len() >= scripts::limit(kind) {
            let msg = format!("The game has room for {} {} instructions.", scripts::limit(kind), scripts::kind_name(kind).to_lowercase());
            return self.set_status(&msg, "err");
        }
        let be = self.backend.clone();
        self.edit(|ed| {
            let fmt_ = format_of(be.decomp());
            let ins = scripts::make_instruction(&fmt_, &mut ed.doc, t, &[], &[]);
            let grp = scripts::group_for(&mut ed.doc, enemy, kind, true);
            if let (Some(i), Some(g)) = (ins, grp) {
                ed.doc.append(g, i);
            }
        });
    }

    /// Replaces the enemy's normal list with a ready-made script (one of `scripts::TEMPLATES`).
    pub fn add_script_template(&mut self, enemy: NodeId, name: &str) {
        let be = self.backend.clone();
        let Some(rows) = scripts::template(&format_of(be.decomp()), &mut self.doc, enemy, name) else { return };
        self.edit(|ed| {
            for g in ed.doc.kids(enemy, "NormalInstructions") {
                ed.doc.detach(g);
            }
            if let Some(g) = scripts::group_for(&mut ed.doc, enemy, "NormalInstructions", true) {
                for r in &rows {
                    ed.doc.append(g, *r);
                }
            }
        });
        self.set_status(&format!("The enemy's normal script is now \u{201c}{}\u{201d}.", name), "ok");
    }

    /// A new body for the type. Fields both bodies have keep their values.
    pub fn set_instruction_type(&mut self, ins: NodeId, t: i128) {
        let old = body(&self.doc, ins);
        let be = self.backend.clone();
        let flags = self.flag_enum();
        self.edit(|ed| {
            let fmt_ = format_of(be.decomp());
            let Some(new) = scripts::make_instruction(&fmt_, &mut ed.doc, t, &[], &[]).and_then(|n| body(&ed.doc, n)) else { return };
            if let Some(old) = old {
                for (k, v) in ed.doc.attrs(old).to_vec() {
                    if ed.doc.has(new, &k) {
                        ed.doc.set(new, &k, &v);
                    }
                }
                ed.doc.detach(old);
            }
            ed.doc.set(ins, "instr_type", &t.to_string());
            let v = scripts::flags_value(ed.doc.get(ins, "flags"), flags.as_ref());
            if scripts::needs_target(t) && v & 0x1C0 == 0 {
                ed.doc.set(ins, "flags", &(v | 0x40).to_string());
            }
            ed.doc.append(ins, new);
        });
    }

    /// Moves an instruction within its list. Goto and Loop targets follow it.
    pub fn move_instruction(&mut self, ins: NodeId, delta: i32) {
        let Some(grp) = self.doc.parent(ins) else { return };
        let Some(enemy) = self.doc.parent(grp) else { return };
        let kind = self.doc.tag(grp).to_string();
        let items = scripts::list_of(&self.doc, enemy, &kind);
        let Some(i) = items.iter().position(|n| *n == ins) else { return };
        let j = i as i64 + i64::from(delta);
        if j < 0 || j >= items.len() as i64 {
            return;
        }
        let (i, j) = (i as i64, j);
        let other = items[j as usize];
        self.edit(|ed| {
            if delta < 0 {
                ed.doc.add_prev(other, ins);
            } else {
                ed.doc.add_next(other, ins);
            }
            if ed.doc.children(grp).is_empty() {
                ed.doc.detach(grp);
            }
            let fix = |k: i64| if k == i { Some(j) } else if k == j { Some(i) } else { None };
            scripts::retarget(&mut ed.doc, enemy, &kind, &fix);
            scripts::retarget_puzzles(&mut ed.doc, enemy, &kind, &fix);
        });
    }

    pub fn remove_instruction(&mut self, ins: NodeId) {
        if let Some(why) = self.removal_refused(ins) {
            return self.set_status(&why, "err");
        }
        let Some(grp) = self.doc.parent(ins) else { return };
        let Some(enemy) = self.doc.parent(grp) else { return };
        let kind = self.doc.tag(grp).to_string();
        let Some(i) = scripts::list_of(&self.doc, enemy, &kind).iter().position(|n| *n == ins) else { return };
        let i = i as i64;
        self.edit(|ed| {
            ed.doc.detach(ins);
            if ed.doc.children(grp).is_empty() {
                ed.doc.detach(grp);
            }
            let fix = |k: i64| (k > i).then_some(k - 1);
            scripts::retarget(&mut ed.doc, enemy, &kind, &fix);
            scripts::retarget_puzzles(&mut ed.doc, enemy, &kind, &fix);
        });
    }

    /// Which actor Follow, Flee and Face act on: 0x40 the glove, 0x80 the ball, 0x100 the closer one.
    pub fn set_instruction_target(&mut self, ins: NodeId, bit: i128) {
        let flags = self.flag_enum();
        let v = scripts::flags_value(self.doc.get(ins, "flags"), flags.as_ref()) & !0x1C0 | bit;
        self.edit(|ed| ed.doc.set(ins, "flags", &v.to_string()));
    }

    // Puzzles (the puzzle card in the inspector)

    /// The puzzle the inspector shows with nothing selected.
    pub fn puzzle_shown(&self) -> Option<NodeId> {
        if !self.has_level {
            return None;
        }
        self.doc.kids(self.doc.root(), "Puzzle").get(self.puzzle_index?).copied()
    }

    /// Shows a puzzle in the inspector. Puzzles with no trigger volume have no place in the view.
    pub fn open_puzzle(&mut self, puzzle: Option<NodeId>) {
        self.puzzle_index = puzzle.and_then(|p| self.doc.kids(self.doc.root(), "Puzzle").iter().position(|n| *n == p));
        self.set_selection(Vec::new());
    }

    pub fn add_puzzle(&mut self) {
        let mut made = None;
        self.mutate(|ed| {
            let p = ed.doc.make("Puzzle", &[]);
            ed.insert_top(p);
            let n = ed.doc.make("PuzzleNumtimes", &[("n", "1")]);
            ed.doc.append(p, n);
            made = Some(p);
            Some(Vec::new())
        });
        self.open_puzzle(made);
        self.set_status(
            "Added a puzzle that fires once. With no conditions it fires when the level starts; add conditions and actions below.",
            "ok",
        );
    }

    /// `n`: None (every frame while its conditions hold) or how many times it fires.
    pub fn set_fires(&mut self, puzzle: NodeId, n: Option<i64>) {
        self.edit(|ed| {
            for c in ed.doc.kids(puzzle, "PuzzleNumtimes") {
                ed.doc.detach(c);
            }
            if let Some(n) = n {
                let e = ed.doc.make("PuzzleNumtimes", &[("n", &n.to_string())]);
                puzzles::insert(&mut ed.doc, puzzle, e);
            }
        });
    }

    /// `how`: "all" (no element), "any" (PuzzleOr) or "least" (PuzzleAny with op `n`).
    pub fn set_logic(&mut self, puzzle: NodeId, how: &str, n: i64) {
        self.edit(|ed| {
            for c in ed.doc.children(puzzle).to_vec() {
                if matches!(ed.doc.tag(c), "PuzzleAnd" | "PuzzleOr" | "PuzzleAny") {
                    ed.doc.detach(c);
                }
            }
            let e = match how {
                "any" => ed.doc.make("PuzzleOr", &[]),
                "least" => ed.doc.make("PuzzleAny", &[("op", &n.to_string())]),
                _ => return,
            };
            puzzles::insert(&mut ed.doc, puzzle, e);
        });
    }

    fn make_part(&mut self, kind: &str, code: i128) -> NodeId {
        let be = self.backend.clone();
        let fmt_ = format_of(be.decomp());
        if kind == "PuzzleCond" {
            puzzles::make_cond(&fmt_, &mut self.doc, code, &[])
        } else {
            puzzles::make_action(&fmt_, &mut self.doc, code, &[])
        }
    }

    /// Adds a condition (kind PuzzleCond) or an action (PuzzleAction) with this type code.
    pub fn add_puzzle_part(&mut self, puzzle: NodeId, kind: &str, code: i128) -> Option<NodeId> {
        let mut made = None;
        self.edit(|ed| {
            let part = ed.make_part(kind, code);
            made = Some(puzzles::insert(&mut ed.doc, puzzle, part));
        });
        made
    }

    /// Changes a condition's or action's type: a new body with that type's fields. The fields both bodies have
    /// keep their values.
    pub fn set_part_type(&mut self, part: NodeId, code: i128) {
        let kind = self.doc.tag(part).to_string();
        let attr = if kind == "PuzzleCond" { "cond_type" } else { "action_type" };
        let old = body(&self.doc, part);
        self.edit(|ed| {
            let new = ed.make_part(&kind, code);
            let Some(b) = body(&ed.doc, new) else { return };
            if let Some(old) = old {
                for (k, v) in ed.doc.attrs(old).to_vec() {
                    if ed.doc.has(b, &k) {
                        ed.doc.set(b, &k, &v);
                    }
                }
                ed.doc.detach(old);
            }
            ed.doc.set(part, attr, &code.to_string());
            ed.doc.append(part, b);
        });
    }

    pub fn remove_puzzle(&mut self, puzzle: NodeId) {
        self.puzzle_index = None;
        self.mutate(|ed| {
            ed.doc.detach(puzzle);
            Some(Vec::new())
        });
    }

    // Layers

    /// Shows or hides a kind of entity in the view. Hidden entities can't be picked, and leave the selection.
    /// "exit" is the layer of the exit triggers: the spheres around the platforms with a SetExit.
    pub fn set_layer(&mut self, kind: &str, on: bool) {
        if on {
            self.hidden.remove(kind);
        } else {
            self.hidden.insert(kind.to_string());
        }
        let setting = match kind {
            "trigger" => Some("triggers"),
            exits::LAYER => Some(exits::SETTING),
            _ => None,
        };
        if let (Some(key), Some(s)) = (setting, self.settings.as_mut()) {
            s.set_value(key, if on { "on" } else { "off" });
        }
        if !on && self.selection.iter().any(|&i| self.entities[i].kind == kind) {
            let sel = self.selection.iter().copied().filter(|&i| self.entities[i].kind != kind).collect();
            self.set_selection(sel);
        }
        self.apply_visibility();
        self.events.push(Event::SelectionChanged);
    }

    // Camera zone panel edits

    /// Changes a camera zone's settings and writes its actions again.
    pub fn write_zone(&mut self, puzzle: NodeId, change: impl FnOnce(&mut Zone)) {
        self.edit(|ed| {
            let Some(mut cur) = camera_zone(&ed.doc, puzzle) else { return };
            change(&mut cur);
            set_camera_zone(&mut ed.doc, puzzle, &cur);
        });
    }
}
