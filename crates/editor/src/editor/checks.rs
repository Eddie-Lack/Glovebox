//! The checks. Most are the game's own level-parser diagnostics, found in the gloveworld decomp
//! (PRINTF("WARNING!! ...") and Crash() in src/parse.c and src/nmeprocess.c), run again on the XML. Also the draw
//! load, the ground under the checkpoints, the imports and the memory budget.

use super::{
    format_of, look_name, sanitize, Editor, Event, PlanJob, PlanResult, ENEMIES_MAX, ENTRY_TAG, GITHUB, LIGHTS_MAX, LOAD_BASE, LOAD_CAM_BACK,
    LOAD_CAM_UP, LOAD_KINDS, LOAD_SLOW, LOAD_STOP, MATRICES, MOD_NOTE, TRIO,
};
use crate::commands::{self, platform_rules};
use crate::exits;
use crate::entities::{trigger_spec, uses_mod, EntId, MOD_PARENTS};
use crate::puzzles;
use crate::scene::{self, LineMode, Lines, RayOptions};
use crate::scripts;
use crate::vec::{self, V3};
use crate::xmlutil::{fmt_g, hex_id, num0};
use glovebox_core::pyfmt::py_round;
use glovebox_core::xml::{Doc, NodeId};
use serde_json::Value;

/// What clicking a line of the Checks panel does. Give it to `Editor::click_check`.
#[derive(Clone, Debug, PartialEq)]
pub enum Click {
    /// Selects an entity and looks at it. None clears the selection.
    Select(Option<EntId>),
    /// Selects these entities and looks at them.
    SelectAll(Vec<EntId>),
    /// Shows a puzzle that has no trigger box in the inspector.
    Puzzle(NodeId),
    /// Shows the far plane around the busiest spot.
    ShowFarPlane { at: V3, far: f64 },
}

/// The least distance from the landscape's box that `far_checks` reports.
const FAR: f64 = 10_000.0;

/// A line in the Checks panel.
#[derive(Clone, Debug, PartialEq)]
pub struct Check {
    /// "ok", "note", "warn", "err" or "imp".
    pub level: &'static str,
    pub text: String,
    pub click: Option<Click>,
    /// A link to the game's source: (address, text).
    pub link: Option<(String, String)>,
    /// A second, smaller line.
    pub sub: Option<String>,
    /// The tooltip.
    pub title: Option<String>,
}

impl Check {
    fn new(level: &'static str, text: String) -> Check {
        Check { level, text, click: None, link: None, sub: None, title: None }
    }
    fn click(mut self, c: Click) -> Check {
        self.click = Some(c);
        self
    }
}

/// A rule of the game's level parser that the level breaks.
#[derive(Clone, Debug, PartialEq)]
pub struct Violation {
    /// The top-level element (or the root) and the element inside it that the rule is about.
    pub top: NodeId,
    pub sub: Option<NodeId>,
    /// Names the finding: the same key in the vanilla level means the vanilla level does it too.
    pub key: String,
    pub msg: String,
    pub level: &'static str,
    /// Where the game prints its own warning: (file, line).
    pub diag: Option<(String, String)>,
    pub vanilla: bool,
}

/// How added scenery changes the heap. `free` is what is left after the 16 KB the game needs during play.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneryBudget {
    pub delta: i64,
    pub blocks: i64,
    pub free: i64,
    pub free_pak: i64,
}

/// One level that pays for the imports.
#[derive(Clone, Debug, PartialEq)]
pub struct BudgetRow {
    /// The level's id and name.
    pub sid: String,
    pub name: String,
    pub cost: i64,
    pub room: i64,
    pub ratio: f64,
}

/// What the imports cost in every level that loads the enlarged bank.
#[derive(Clone, Debug, PartialEq)]
pub struct Budget {
    /// Most filled first.
    pub rows: Vec<BudgetRow>,
    /// The bytes the game mod's larger heap adds.
    pub extra: i64,
    /// The level with the least room left with the mod: (id, name, bytes left).
    pub least_with_pak: (String, String, i64),
    /// The rows over the game's own heap.
    pub over: Vec<BudgetRow>,
    /// The rows over even the mod's larger heap.
    pub over_with_pak: Vec<BudgetRow>,
}

/// The draw load of the level.
#[derive(Clone, Debug, PartialEq)]
pub struct DrawLoad {
    pub far: f64,
    /// The most object matrices at one spot, and the camera there. None for a level with no spots.
    pub worst: (i64, Option<V3>),
    pub spots: Vec<(V3, i64)>,
    pub total: i64,
}

type LoadItem = (V3, f64, i64);

fn s(v: Option<&Value>) -> &str {
    v.and_then(|v| v.as_str()).unwrap_or("")
}

fn key_path(doc: &Doc, n: NodeId) -> String {
    format!("{:?}", doc.path_of(n))
}

impl Editor {
    /// The command with a token of the game's parser.
    pub fn tag_of(&self, token: &str) -> Option<String> {
        let commands = self.backend.decomp().get("commands")?.as_object()?;
        commands.iter().find(|(_, c)| c.get("token").and_then(|t| t.as_str()) == Some(token)).map(|(n, _)| n.clone())
    }

    fn diag_for(&self, text: &str) -> Option<(String, String)> {
        let list = self.backend.decomp().get("diagnostics")?.as_array()?;
        let d = list.iter().find(|d| s(d.get("text")).contains(text))?;
        let line = match d.get("line") {
            Some(Value::String(l)) => l.clone(),
            Some(v) => v.to_string(),
            None => String::new(),
        };
        Some((s(d.get("file")).to_string(), line))
    }

    /// The rules of the game's level parser that a level breaks.
    pub fn run_rules(&self, doc: &Doc) -> Vec<Violation> {
        let root = doc.root();
        let be = self.backend.clone();
        let fmt_ = format_of(be.decomp());
        let flags = self.flag_enum();
        let mut out: Vec<Violation> = Vec::new();
        let mut add = |top: NodeId, key: String, msg: String, diag: Option<&str>, sub: Option<NodeId>, level: &'static str| {
            out.push(Violation { top, sub, key, msg, level, diag: diag.and_then(|d| self.diag_for(d)), vanilla: false });
        };
        let mut tags_so_far: Vec<f64> = Vec::new();
        let mut plat_index = 0;
        let top = doc.children(root).to_vec();
        let enemies: Vec<NodeId> = top.iter().copied().filter(|n| doc.tag(*n) == "Enemy").collect();
        // A tag the game looks up while it reads the level
        let unknown = |tags: &[f64], value: f64| value != 0.0 && !tags.contains(&value);
        let tag_msg = |what: &str, value: f64| {
            format!("{} tag {} isn't on an earlier platform. The game looks it up while it reads the level.", what, fmt_g(value))
        };
        for &node in &top {
            let tag = doc.tag(node);
            match tag {
                "Platform" | "NullPlatform" => {
                    plat_index += 1;
                    let pid = format!("platform {}", plat_index);
                    // CheckForExit: the distance must be below the platform's radius. No distance is below a radius
                    // of 0. Exit type 3 tests no distance.
                    if let Some(t) = exits::exit_type(doc, node).filter(|t| *t != 3) {
                        let verts = match tag {
                            "NullPlatform" => Some(Default::default()),
                            _ => hex_id(doc.get(node, "object_id")).and_then(|o| self.root_vertices(&o)),
                        };
                        if let Some((_, Some(why))) = verts.map(|v| exits::radius(doc, node, &v)) {
                            let msg = format!("The exit (type {t}) can never fire: the game gives this platform a radius of 0. {}", why.text());
                            add(node, format!("{}:exit", pid), msg, None, None, "err");
                        }
                    }
                    if let Some(par) = doc.first(node, "PlatSetParent") {
                        let v = num0(doc, par, "tag");
                        if unknown(&tags_so_far, v) {
                            add(node, format!("{}:parent", pid), tag_msg("PlatSetParent", v), Some("No such parent tag"), None, "err");
                        }
                    }
                    for t in doc.kids(node, "PlatSetTag") {
                        tags_so_far.push(num0(doc, t, "tag"));
                    }
                    if let Some(tele) = doc.first(node, "SetTeleport") {
                        // An unknown target makes the game skip the rest of the command and read its numbers as
                        // commands
                        let v = num0(doc, tele, "out_framecount");
                        if unknown(&tags_so_far, v) {
                            add(node, format!("{}:teleport", pid), tag_msg("The teleport's target", v), None, None, "err");
                        }
                    }
                    for r in platform_rules(doc, node) {
                        let level = if r.key == "2tag" { "warn" } else { "err" };
                        add(node, format!("{}:{}", pid, r.key), r.message, r.game_text, None, level);
                    }
                }
                "Buzzer" => {
                    for f in ["platform_1_tag", "platform_2_tag"] {
                        let v = num0(doc, node, f);
                        if unknown(&tags_so_far, v) {
                            let what = format!("The buzzer's {}", f.replace('_', " "));
                            add(node, format!("buzzer:{}:{}", key_path(doc, node), f), tag_msg(&what, v), None, None, "warn");
                        }
                    }
                }
                "Vent" => {
                    let v = num0(doc, node, "parent_tag");
                    if unknown(&tags_so_far, v) {
                        add(node, format!("vent:{}:parent", key_path(doc, node)), tag_msg("The vent's parent", v), None, None, "warn");
                    }
                }
                "Enemy" => {
                    let path = key_path(doc, node);
                    for (grp, mx, crash) in [
                        ("NormalInstructions", 30, "NMEADDNORMAL"),
                        ("ConditionalInstructions", 10, "NMEADDCONDITIONAL"),
                        ("AttackInstructions", 30, "NMEADDATTACK"),
                    ] {
                        let groups = doc.kids(node, grp);
                        let n: usize = groups.iter().map(|g| doc.len(*g)).sum();
                        if n > mx {
                            let msg = format!(
                                "{} {} instructions; more than {} makes the game call Crash(\"{}\").",
                                n,
                                grp.replace("Instructions", "").to_lowercase(),
                                mx,
                                crash
                            );
                            add(node, format!("enemy:{}:{}", path, grp), msg, Some(crash), None, "err");
                        }
                        if groups.iter().any(|g| doc.len(*g) == 0) {
                            add(node, format!("enemy:{}:empty", path), format!("An empty {} group: the build can't write it.", grp), None, None, "err");
                        }
                    }
                    if !doc.kids(node, "NormalInstructions").iter().any(|g| doc.len(*g) > 0) {
                        let msg = "No normal instruction. The game reads the first one when the enemy ends, and crashes.";
                        add(node, format!("enemy:{}:none", path), msg.to_string(), None, None, "err");
                    }
                    for r in scripts::rules(doc, node, flags.as_ref()) {
                        add(node, format!("enemy:{}:{}", path, r.key), r.message, None, None, r.level);
                    }
                    let ch = doc.children(node);
                    if ch.last().map(|c| doc.tag(*c)) != Some("EnemyFinalize") || doc.kids(node, "EnemyFinalize").len() != 1 {
                        let msg = "EnemyFinalize must be the enemy's last command, once.";
                        add(node, format!("enemy:{}:final", path), msg.to_string(), None, None, "err");
                    }
                }
                _ => {}
            }
            for &c in doc.children(node) {
                let ct = doc.tag(c);
                let Some(limit) = commands::limit(ct).filter(|_| ct != "PlatPathPoint") else { continue };
                let n = doc.kids(node, ct).len();
                if n > limit {
                    let msg = format!("{} {}; the game has room for {} and writes past the end.", n, ct, limit);
                    add(node, format!("limit:{}:{}", key_path(doc, node), ct), msg, None, None, "err");
                    break;
                }
            }
        }
        if top.last().map(|c| doc.tag(*c)) != Some("EndLevelData") || doc.kids(root, "EndLevelData").len() != 1 {
            let msg = "EndLevelData must be the level's last element, once: the game reads until it.";
            add(root, "level:end".into(), msg.to_string(), None, None, "err");
        }
        match doc.first(root, "FogConfiguration") {
            None => {
                let msg = "The level has no fog settings: the game keeps the last screen's fog and far plane.";
                add(root, "level:fog".into(), msg.to_string(), None, None, "err");
            }
            Some(fog) => {
                let d = num0(doc, fog, "fog_distance");
                if d >= 1000.0 {
                    let msg = format!("The fog distance is {}. It must be below 1000: the game divides by 1000 minus it.", fmt_g(d));
                    add(root, "level:fogdist".into(), msg, None, None, "err");
                }
            }
        }
        if enemies.len() > ENEMIES_MAX {
            let msg = format!("{} enemies; the game has room for {} and writes past the end.", enemies.len(), ENEMIES_MAX);
            add(root, "level:enemies".into(), msg, None, None, "err");
        }
        let types: Vec<Option<&str>> = enemies.iter().map(|e| doc.get(*e, "type")).collect();
        let first3: Vec<Option<&str>> = types.iter().take(3).copied().collect();
        if TRIO.iter().any(|t| types.contains(&Some(*t))) && first3 != TRIO.map(Some) {
            let msg = "The whale, crab and fish (joff, cancer, kirk) must be the first three enemies, in that order: the game reads them as enemies[0], [1] and [2].";
            add(root, "level:trio".into(), msg.to_string(), None, None, "err");
        }
        let name = doc.get(root, "name").unwrap_or("");
        if !name.is_empty() && (name.chars().count() % 2 == 0 || !name.is_ascii()) {
            let msg = format!(
                "The level's file name \u{201c}{}\u{201d} must be ASCII with an odd length (vanilla levels use 7). Otherwise every command sits at an odd address and a console crashes.",
                name
            );
            add(root, "level:name".into(), msg, None, None, "err");
        }
        for t in ["AmbientLight", "DiffuseLight", "FogConfiguration"] {
            for n in doc.kids(root, t) {
                if ["r", "g", "b"].iter().any(|c| num0(doc, n, c) > 255.0) {
                    let msg = format!("{}: a color channel is over 255; the game keeps the low byte.", look_name(t).unwrap_or(t));
                    add(n, format!("level:color:{}", key_path(doc, n)), msg, None, None, "err");
                }
            }
        }
        let lights = doc.kids(root, "DiffuseLight").len();
        if lights == 0 {
            let msg = "The level has no directional light. Every vanilla level has at least one.";
            add(root, "level:nolight".into(), msg.to_string(), None, None, "warn");
        }
        if doc.first(root, "AmbientLight").is_none() {
            let msg = "The level has no ambient light: the game keeps the last screen's.";
            add(root, "level:noambient".into(), msg.to_string(), None, None, "warn");
        }
        let drops = doc.kids(root, "Backdrop");
        for &bd in &drops {
            if num0(doc, bd, "scale_x") == 0.0 || num0(doc, bd, "scale_y") == 0.0 {
                let msg = "A backdrop scale of 0: the game divides by it.";
                add(bd, format!("backdrop:scale:{}", key_path(doc, bd)), msg.to_string(), None, None, "err");
            }
        }
        let entry = doc
            .kids(root, "Platform")
            .into_iter()
            .filter(|p| {
                doc.kids(*p, "PlatSetTag").iter().any(|t| num0(doc, *t, "tag") == ENTRY_TAG as f64)
                    && doc.kids(*p, "SetExit").iter().any(|e| doc.get(*e, "behavior") == Some("11"))
            })
            .count();
        if entry > 1 {
            let msg = format!("{} entry zones to the extra levels (exit 11, tag {}): give the level one.", entry, ENTRY_TAG);
            add(root, "level:entry".into(), msg, None, None, "warn");
        }
        if let Some(g) = doc.first(root, "SetGravity") {
            let v = num0(doc, g, "strength");
            if (v - 1.2).abs() > 1e-6 {
                let msg = format!("Gravity is {} (the game's own is 1.2): jumps and falls differ from the vanilla levels.", fmt_g(v));
                add(g, "level:gravity".into(), msg, None, None, "warn");
            }
        }
        if lights > LIGHTS_MAX {
            let msg = format!("{} directional lights; the game uses the first {}.", lights, LIGHTS_MAX);
            add(root, "level:lights".into(), msg, None, None, "warn");
        }
        for (i, &bd) in drops.iter().enumerate() {
            let v = num0(doc, bd, "decal_parent_idx");
            if v > drops.len() as f64 {
                let msg = format!("decal_parent_idx {} names a backdrop the level doesn't have.", fmt_g(v));
                add(bd, format!("backdrop:{}", i), msg, None, None, "warn");
            }
        }
        // Trigger volumes that PuzzleEvaluateCondition can never satisfy: IsPointWithinBox needs a positive size, and
        // a range test (distance < radius) needs a positive radius. The cylinder also needs min_y <= max_y.
        for p in doc.kids(root, "Puzzle") {
            for c in doc.kids(p, "PuzzleCond") {
                for &v in doc.children(c) {
                    let Some((shape, _)) = trigger_spec(doc.tag(v)) else { continue };
                    let n = |a: &str| num0(doc, v, a);
                    let mut bad: Vec<String> = Vec::new();
                    match shape {
                        "box" => {
                            for a in ["l", "w", "h"] {
                                if !(n(a) > 0.0) {
                                    bad.push(format!("{} \u{2264} 0", a));
                                }
                            }
                        }
                        // The test squares both sides
                        "sphere" => {
                            if n("range") == 0.0 {
                                bad.push("range = 0".into());
                            }
                        }
                        _ => {
                            if !(n("range") > 0.0) {
                                bad.push("range \u{2264} 0".into());
                            }
                            if n("min_y") > n("max_y") {
                                bad.push("min_y > max_y".into());
                            }
                        }
                    }
                    if !bad.is_empty() {
                        let msg = format!(
                            "{}: {}. The volume is flat or empty, and this condition is practically never true.",
                            doc.tag(v),
                            bad.join(", ")
                        );
                        add(p, format!("trigger:{}", key_path(doc, v)), msg, None, Some(v), "err");
                    }
                }
            }
        }
        for r in puzzles::rules(doc, root, &fmt_) {
            add(r.node, r.key, r.message, None, None, r.level);
        }
        let groups: Vec<f64> = doc.kids(root, "GaribGroup").iter().map(|g| num0(doc, *g, "group_id")).collect();
        for n in doc.iter_tags(root, &["PuzzleActionSpawnGaribGroup"]) {
            let id = num0(doc, n, "group_id");
            if !groups.contains(&id) {
                let msg = format!("A puzzle spawns garib group {}, which doesn't exist.", fmt_g(id));
                add(doc.closest(n, "Puzzle").unwrap_or(n), format!("spawn:{}", fmt_g(id)), msg, None, None, "err");
            }
        }
        drop(add);
        out
    }

    /// Objects whose rotation or scale only the Expansion Pak game mod applies (mod/transforms.c).
    pub fn mod_users(&self) -> Vec<NodeId> {
        let mut out = Vec::new();
        for n in self.doc.iter_tags(self.doc.root(), &["SetActorRotation", "SetActorScale"]) {
            if let Some(p) = self.doc.parent(n) {
                if MOD_PARENTS.contains(&self.doc.tag(p)) && !out.contains(&p) {
                    out.push(p);
                }
            }
        }
        out
    }

    fn is_custom(&self, oid: &str) -> bool {
        self.catalog.get(oid).is_some_and(|c| c.custom.is_some())
    }

    /// Imports are loaded from the player's ROM at run time by the game mod (mod/banks.c), unless the author
    /// chooses to copy them into the level's banks. Copied imports put game data in patches. They work without the
    /// Expansion Pak when they fit in the game's own heap (`needs_pak_heap`). Imported custom models are always
    /// copied: they aren't in the player's ROM.
    pub fn runtime_imports(&self) -> bool {
        self.current_imports().iter().any(|i| !self.is_custom(i)) && !self.copy_imports
    }

    /// Whether copied imports or added scenery need the mod's larger heap (mod/heap.c).
    pub fn needs_pak_heap(&self) -> bool {
        if self.scenery_budget().is_some_and(|sb| sb.free < 0) {
            return true;
        }
        if self.runtime_imports() {
            return false;
        }
        self.import_budget().is_some_and(|b| !b.over.is_empty())
    }

    // Memory for scenery. Each landscape actor takes its Actor (0xE4 bytes, 240 with jalloc's rounding), its
    // collision planes (0x78 bytes a face of the model's root mesh, parse.c), and its static matrix (64): three
    // heap blocks of the game's 1,280 (include/jalloc.h). A background actor frees its planes after the parse, but
    // the parse needs the room.

    /// The collision faces of an element's model.
    pub fn faces(&self, doc: &Doc, node: NodeId) -> i64 {
        match doc.get(node, "object_id").filter(|o| !o.is_empty()).and_then(|o| hex_id(Some(o))) {
            Some(oid) => i64::from(self.backend.faces(&oid)),
            None => 0,
        }
    }

    /// (heap bytes, heap blocks) that a top-level element takes when the level loads (scenery only).
    pub fn actor_cost(&self, doc: &Doc, node: NodeId) -> (i64, i64) {
        match doc.tag(node) {
            "LandActor" | "BackgroundActor" => (240 + (120 * self.faces(doc, node) + 15) / 16 * 16 + 64, 3),
            "AnimatedBackgroundActor" => (240, 1),
            _ => (0, 0),
        }
    }

    pub fn scenery_cost(&self, doc: &Doc) -> (i64, i64) {
        let (mut b, mut k) = (0, 0);
        for &n in doc.children(doc.root()) {
            let (x, y) = self.actor_cost(doc, n);
            b += x;
            k += y;
        }
        (b, k)
    }

    pub(crate) fn heap_free(&self, sid: &str) -> Option<i64> {
        let h = self.backend.heap().get(sid)?;
        if h.as_object().is_some_and(|o| o.is_empty()) {
            return None;
        }
        h.get("free").and_then(|f| f.as_i64().or_else(|| f.as_f64().map(|v| v as i64)))
    }

    /// The scenery cost of the vanilla level, worked out once for each level.
    pub(crate) fn prime_orig_cost(&mut self) {
        let Some(orig) = self.orig_xml.clone() else { return };
        if self.orig_cost.as_ref().is_some_and(|(x, _)| *x == orig) {
            return;
        }
        if let Ok(d) = Doc::parse(&orig) {
            let cost = self.scenery_cost(&d);
            self.orig_cost = Some((orig, cost));
        }
    }

    /// The draw load of the vanilla level, worked out once for each level. A restored draft is set aside while it
    /// is measured: the entities are built for the vanilla level, then for the draft again.
    pub(crate) fn prime_orig_load(&mut self) {
        let Some(orig) = self.orig_xml.clone().filter(|_| self.has_level) else { return };
        if self.orig_load.as_ref().is_some_and(|(x, _)| *x == orig) {
            return;
        }
        let n = if self.serialize() == orig {
            self.draw_load().worst.0
        } else {
            let Ok(d) = Doc::parse(&orig) else { return };
            let draft = std::mem::replace(&mut self.doc, d);
            self.build_entities();
            let n = self.draw_load().worst.0;
            self.doc = draft;
            self.build_entities();
            n
        };
        self.orig_load = Some((orig, n));
    }

    /// How the scenery added or removed since the vanilla level changes the heap. None for an unchanged level or
    /// one with no measured heap.
    pub fn scenery_budget(&self) -> Option<SceneryBudget> {
        let free = self.heap_free(self.scene_id())?;
        let orig = self.orig_xml.as_ref().filter(|_| self.has_level)?;
        let was = match &self.orig_cost {
            Some((x, c)) if x == orig => *c,
            _ => self.scenery_cost(&Doc::parse(orig).ok()?),
        };
        let cur = self.scenery_cost(&self.doc);
        let (delta, blocks) = (cur.0 - was.0, cur.1 - was.1);
        if delta <= 0 {
            return None;
        }
        let free = free - 16 * 1024 - delta;
        Some(SceneryBudget { delta, blocks, free, free_pak: free + self.mod_info.heap_extra_bytes as i64 })
    }

    /// The build adds the Expansion Pak game mod.
    pub fn use_mod(&self) -> bool {
        !self.mod_users().is_empty() || self.runtime_imports() || self.needs_pak_heap()
    }

    /// What the inspector says about an entity: (level, text).
    pub fn issues_for(&self, i: EntId) -> Vec<(&'static str, String)> {
        let mut out = Vec::new();
        let Some(ent) = self.entities.get(i) else { return out };
        if ent.is_handle || ent.derived {
            return out;
        }
        if uses_mod(&self.doc, ent.node) {
            out.push(("note", format!("Its rotation or scale uses the Expansion Pak game mod. {}", MOD_NOTE)));
        }
        let oid = ent.object_id.as_deref();
        if ent.missing_model {
            out.push(("err", format!("Model {} isn't in any object bank in the game.", oid.unwrap_or("None"))));
        } else if let (true, Some(c)) = (ent.imported, oid.and_then(|o| self.catalog.get(o))) {
            out.push((
                "imp",
                format!(
                    "Model {} comes from {}; it's imported into {} at ROM build ({:.1} KB).",
                    oid.unwrap_or(""),
                    c.bank,
                    self.scene_str("importObjectBank"),
                    c.bytes as f64 / 1024.0
                ),
            ));
        }
        if ent.kind == "enemy" && oid.is_none() {
            out.push(("err", "No model is known for this enemy type.".to_string()));
        }
        if ent.kind == "enemy" && ent.imported {
            out.push(("note", "Enemy from another world: its own model is imported, but projectiles it fires come from the game's bullet table and stay invisible if their model isn't loaded (FindObject non-vital).".to_string()));
        }
        for v in &self.violations {
            if v.top == ent.node && (v.sub.is_none() || v.sub == ent.sub) {
                let tail = if v.vanilla { " (the vanilla level does this too)" } else { "" };
                out.push((if v.vanilla { "note" } else { v.level }, format!("{}{}", v.msg, tail)));
            }
        }
        out
    }

    /// The address of a line of the decompilation.
    pub fn source_link(&self, file: &str, line: &str) -> String {
        let rev = s(self.backend.decomp().get("source").and_then(|x| x.get("gloveworld")));
        format!("{}{}/{}#L{}", GITHUB, if rev.is_empty() { "main" } else { rev }, file, line)
    }

    /// The Checks panel's lines.
    pub fn checks(&mut self) -> Vec<Check> {
        let mut out = Vec::new();
        self.prime_orig_cost();
        let mut violations = self.run_rules(&self.doc);
        for v in violations.iter_mut() {
            v.vanilla = self.orig_violations.iter().any(|o| o.key == v.key);
        }
        self.violations = violations;
        let whole: Vec<EntId> = (0..self.entities.len()).filter(|&i| self.whole(i)).collect();
        let missing: Vec<EntId> = whole
            .iter()
            .copied()
            .filter(|&i| {
                let e = &self.entities[i];
                e.missing_model || (e.kind == "enemy" && e.object_id.is_none())
            })
            .collect();
        for &i in &missing {
            let first = self.issues_for(i).into_iter().next().map(|x| x.1).unwrap_or_default();
            out.push(Check::new("err", format!("{}: {}", self.entities[i].label, first)).click(Click::Select(Some(i))));
        }
        // Last come the findings that the vanilla level has too
        for vanilla in [false, true] {
            for v in self.violations.iter().filter(|v| v.vanilla == vanilla) {
                let ent = whole.iter().copied().find(|&i| {
                    let e = &self.entities[i];
                    e.node == v.top && (v.sub.is_none() || e.sub == v.sub)
                });
                // A puzzle with no trigger box has no entity: the line names it by its number and opens it
                let puzzle = (ent.is_none() && self.doc.tag(v.top) == "Puzzle").then_some(v.top);
                let number = puzzle.and_then(|p| self.doc.kids(self.doc.root(), "Puzzle").iter().position(|n| *n == p));
                let label = match (ent, number) {
                    (Some(i), _) => format!("{}: ", self.entities[i].label),
                    (None, Some(n)) => format!("Puzzle {}: ", n + 1),
                    _ => String::new(),
                };
                let tail = if v.vanilla { " Also in the vanilla level." } else { "" };
                let mut c = Check::new(if v.vanilla { "note" } else { v.level }, format!("{}{}{}", label, v.msg, tail));
                c.click = ent.map(|i| Click::Select(Some(i))).or(puzzle.map(Click::Puzzle));
                c.link = v.diag.as_ref().map(|(file, line)| (self.source_link(file, line), format!("{}:{}", file, line)));
                out.push(c);
            }
        }
        let users = self.mod_users();
        let mods = users.len();
        if mods > 0 {
            // The line names the objects, as the other lines do, and a click selects them all
            let ents: Vec<EntId> = users.iter().filter_map(|n| whole.iter().copied().find(|&i| self.entities[i].node == *n)).collect();
            let mut names: Vec<&str> = ents.iter().take(3).map(|&i| self.entities[i].label.as_str()).collect();
            if ents.len() > 3 {
                names.push("…");
            }
            let label = if names.is_empty() { String::new() } else { format!("{}: ", names.join(", ")) };
            let mut c = Check::new(
                "note",
                format!(
                    "{}{} object{} a rotation or scale that only the Expansion Pak game mod applies (enemies, platforms, powerups or water). {}",
                    label,
                    mods,
                    if mods == 1 { " has" } else { "s have" },
                    MOD_NOTE
                ),
            );
            c.click = (!ents.is_empty()).then_some(Click::SelectAll(ents));
            out.push(c);
        }
        let far = self.far_checks(&whole);
        let near = far.is_empty();
        out.extend(far);
        // libgarib's schema (structure), then the import checks (every value against its type and enum: the schema
        // takes any text for an enum)
        let xml = self.serialize();
        let mut bad = self.backend.validate(&xml);
        if bad.is_empty() {
            let res = sanitize(self.backend.decomp(), &xml, "level");
            if !res.ok {
                bad = res.errors;
            }
        }
        if let Some(first) = bad.first() {
            let mut c = Check::new("err", format!("The level doesn't match libgarib's format, so it won't build: {}", first));
            c.sub = Some(bad[1..].join("\n")).filter(|x| !x.is_empty());
            out.insert(0, c);
        }
        let fresh = self.violations.iter().filter(|v| !v.vanilla).count();
        if missing.is_empty() && fresh == 0 && bad.is_empty() && near {
            out.push(Check::new(
                "ok",
                format!(
                    "No new problems. {} entities checked against the level-parser rules from the decomp; every model is in a bank.",
                    whole.len()
                ),
            ));
        }
        if let Some(sb) = self.scenery_budget() {
            let kb = |v: i64| py_round(v as f64 / 1024.0) as i64;
            let text = format!(
                "Added scenery takes about {} KB more heap than the vanilla level and {} more of the game's 1,280 heap blocks. ",
                kb(sb.delta),
                sb.blocks
            );
            out.push(if sb.free >= 0 {
                Check::new("ok", format!("{}About {} KB stays free.", text, kb(sb.free)))
            } else if sb.free_pak >= 256 * 1024 {
                Check::new(
                    "note",
                    format!(
                        "{}That's more than the game's own heap has, so Build ROM and Export patch add the Expansion Pak game mod: about {} KB stays free with it. The ROM then needs the Expansion Pak.",
                        text,
                        kb(sb.free_pak)
                    ),
                )
            } else {
                Check::new(
                    if sb.free_pak < 0 { "err" } else { "warn" },
                    format!("{}Even with the Expansion Pak only about {} KB stays free.", text, kb(sb.free_pak)),
                )
            });
        }
        out.extend(self.ground_checks());
        out.extend(self.load_checks());
        out
    }

    /// How far the object farthest out of the landscape's box is, in sizes of the level (0 inside it), and its
    /// label. For the tests that keep `far_checks` silent on the vanilla levels.
    pub fn farthest(&self) -> (f64, String) {
        let whole: Vec<EntId> = (0..self.entities.len()).filter(|&i| self.whole(i)).collect();
        let mut land = vec::Box3::new();
        for &i in whole.iter().filter(|&&i| self.entities[i].kind == "land") {
            land.add_box(&self.obj_box(i));
        }
        if land.empty() {
            return (0.0, String::new());
        }
        let size = land.size().iter().fold(1.0f64, |a, b| a.max(*b));
        let mut best = (0.0, String::new());
        for &i in whole.iter().filter(|&&i| self.entities[i].kind != "land") {
            let p = self.obj_pos(i);
            let out = (0..3).map(|k| (land.min[k] - p[k]).max(p[k] - land.max[k]).max(0.0)).fold(0.0, f64::max) / size;
            if out > best.0 {
                best = (out, format!("{} ({})", self.entities[i].label, self.entities[i].kind));
            }
        }
        best
    }

    /// Objects far outside the level: farther from its landscape's box than 10 times the box's largest side, and at
    /// least `FAR` units. The vanilla levels have objects up to about 6 sizes out (a Ball start in the Wayroom), and
    /// none this far.
    fn far_checks(&self, whole: &[EntId]) -> Vec<Check> {
        let mut land = vec::Box3::new();
        for &i in whole.iter().filter(|&&i| self.entities[i].kind == "land") {
            land.add_box(&self.obj_box(i));
        }
        if land.empty() {
            return Vec::new();
        }
        let s = land.size();
        let margin = s.iter().fold(FAR, |a, b| a.max(10.0 * b));
        let mut out = Vec::new();
        for &i in whole.iter().filter(|&&i| !matches!(self.entities[i].kind, "land")) {
            let p = self.obj_pos(i);
            let outside = (0..3).any(|k| !p[k].is_finite() || p[k] < land.min[k] - margin || p[k] > land.max[k] + margin);
            if outside {
                let text = format!("{}: far outside the level.", self.entities[i].label);
                out.push(Check::new("warn", text).click(Click::Select(Some(i))));
            }
        }
        out
    }

    /// Does what a click on a line of the Checks panel does.
    pub fn click_check(&mut self, click: &Click) {
        match click {
            Click::Select(e) => self.select(*e, true),
            Click::Puzzle(p) => self.open_puzzle(Some(*p)),
            Click::SelectAll(ents) => {
                self.set_selection(ents.clone());
                if let Some(e) = self.selected() {
                    self.focus_on(e);
                }
            }
            Click::ShowFarPlane { at, far } => self.show_far_plane(Some(*at), *far),
        }
    }

    // Draw load

    pub fn far_plane(&self) -> f64 {
        match self.doc.first(self.doc.root(), "FogConfiguration") {
            Some(fog) => num0(&self.doc, fog, "near_clip"),
            None => 1000.0,
        }
    }

    /// The actors as the game culls them: center, radius and matrices (object nodes, plus a shadow for enemies).
    fn load_items(&self) -> Vec<LoadItem> {
        let mut out = Vec::new();
        for (i, e) in self.entities.iter().enumerate() {
            if e.is_handle || e.derived || !LOAD_KINDS.contains(&e.kind) {
                continue;
            }
            let b = self.obj_box(i);
            if b.empty() {
                continue;
            }
            let nodes = self.nodes[i].draw_nodes.filter(|n| *n > 0).unwrap_or(1) as i64;
            out.push((self.obj_pos(i), b.sphere_radius(), nodes + i64::from(e.kind == "enemy")));
        }
        out
    }

    /// Where the player can be: garibs, powerups, checkpoints, the start, enemies and camera zones. A level with
    /// few of them is also sampled at its landscape pieces.
    fn load_spots(&self) -> Vec<V3> {
        let ents = &self.entities;
        let mut spots: Vec<V3> = (0..ents.len())
            .filter(|&i| {
                let e = &ents[i];
                (!e.is_handle && matches!(e.kind, "garib" | "pickup" | "tip" | "spawn" | "enemy")) || e.gate.is_some() || e.zone
            })
            .map(|i| self.obj_pos(i))
            .collect();
        if spots.len() < 30 {
            spots.extend((0..ents.len()).filter(|&i| ents[i].kind == "land" && !ents[i].derived).map(|i| self.obj_pos(i)));
        }
        spots
    }

    /// The camera stands behind and over Glover. It can face any way, so each spot counts from four cameras around
    /// it.
    fn load_at(p: V3, far: f64, items: &[LoadItem]) -> (i64, Option<V3>) {
        let (mut n, mut at) = (0, None);
        for k in 0..4 {
            let a = f64::from(k) * std::f64::consts::PI / 2.0;
            let eye = vec::add(p, [a.sin() * LOAD_CAM_BACK, LOAD_CAM_UP, a.cos() * LOAD_CAM_BACK]);
            let mut m = LOAD_BASE;
            for (c, r, count) in items {
                let d = vec::sub(*c, eye);
                let d2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
                let reach = far + r;
                if d2 <= reach * reach {
                    m += count;
                }
            }
            if m > n {
                (n, at) = (m, Some(eye));
            }
        }
        (n, at)
    }

    pub fn draw_load(&self) -> DrawLoad {
        let (far, items) = (self.far_plane(), self.load_items());
        let mut worst = (0, None);
        let mut spots = Vec::new();
        for p in self.load_spots() {
            let r = Editor::load_at(p, far, &items);
            if r.0 > worst.0 {
                worst = r;
            }
            spots.push((p, r.0));
        }
        DrawLoad { far, worst, spots, total: items.iter().map(|i| i.2).sum::<i64>() + LOAD_BASE }
    }

    /// Draws the far plane as a ball of three rings around a point and looks at it. None takes it away.
    pub fn show_far_plane(&mut self, at: Option<V3>, far: f64) {
        if let Some(old) = self.far_sphere.take() {
            self.graph.delete(old);
        }
        if let Some(at) = at {
            let n = self.graph.node("far plane");
            for ax in ['x', 'y', 'z'] {
                let mut l = Lines::from_f64(&scene::ring_points(ax, 0.0, 96), [1.0, 0.7, 0.42], LineMode::Loop);
                l.opacity = 0.7;
                let ln = self.graph.lines_node(l);
                self.graph.add(n, ln);
            }
            self.graph[n].scale = [far; 3];
            self.graph[n].pos = at;
            self.far_sphere = Some(n);
            self.orbit.target = at;
            self.camera.pos = vec::add(at, [0.0, far * 0.8, far * 1.2]);
            self.orbit.update(&mut self.camera);
        }
        self.emit(Event::ViewChanged);
    }

    pub fn load_checks(&self) -> Vec<Check> {
        let mut out = Vec::new();
        if let Some(fog) = self.doc.first(self.doc.root(), "FogConfiguration") {
            let d = num0(&self.doc, fog, "fog_distance");
            if d >= 1000.0 {
                let msg = format!(
                    "Fog: fog_distance is {}. It's where the fog starts on the game's 0 to 1000 scale (gSPFogPosition) and must stay under 1000.",
                    fmt_g(d)
                );
                out.push(Check::new("err", msg).click(Click::Select(None)));
            }
        }
        let dl = self.draw_load();
        let (n, Some(at)) = dl.worst else { return out };
        // Vanilla levels reach these numbers too: the estimate says 261 for hub 1. A level warns or fails only when
        // it's busier than the vanilla level.
        let orig = self.orig_xml.as_ref().and_then(|x| self.orig_load.as_ref().filter(|(o, _)| o == x)).map(|(_, n)| *n);
        let busier = orig.is_none_or(|o| n > o);
        let level = if n >= LOAD_STOP && busier {
            "err"
        } else if n >= LOAD_SLOW && busier {
            "warn"
        } else if n >= LOAD_SLOW {
            "note"
        } else {
            "ok"
        };
        let what = match level {
            "err" => format!("That's at or near the limit: past {} the game stops, and before that it crawls.", MATRICES),
            "warn" => "The game slows down here. Fewer pieces in reach or a shorter far plane fix it.".to_string(),
            "note" => format!("The vanilla level reaches about {}, and the game runs it.", orig.unwrap_or(n)),
            _ => "Fine.".to_string(),
        };
        let far = fmt_g(dl.far);
        let mut c = Check::new(
            level,
            format!("Draw load: about {} of {} object matrices at the busiest spot (far plane {}). {}", n, MATRICES, far, what),
        )
        .click(Click::ShowFarPlane { at, far: dl.far });
        c.sub = Some(format!(
            "The game draws everything within {} of the camera, behind it too. {} in the whole level. Click to show the spot.",
            far, dl.total
        ));
        c.title = Some("Click to see the spot and the far plane".to_string());
        out.push(c);
        out
    }

    /// Solid ground under a point: the first LandActor or platform face below it, within reach. Background
    /// actors have no collision. `objs`: the nodes to test, when the caller has them already.
    pub fn ground_below(&self, p: V3, reach: f64, skip: Option<EntId>, objs: Option<&[scene::NodeId]>) -> Option<V3> {
        let own;
        let objs = match objs {
            Some(o) => o,
            None => {
                own = self.ground_objects(skip, true);
                &own
            }
        };
        let o = RayOptions { near: 0.0, far: reach + 30.0, meshes_only: true, ..RayOptions::default() };
        let hits = self.graph.raycast(objs, vec::add(p, [0.0, 30.0, 0.0]), [0.0, -1.0, 0.0], &o);
        hits.first().map(|h| h.point)
    }

    /// Glover starts at the first checkpoint (or the GloverSpawnPoint in a level with none) and restarts at the last
    /// one he touched: one with no solid ground under it drops him into a pit every time.
    pub fn ground_checks(&self) -> Vec<Check> {
        let mut out = Vec::new();
        if self.scene.is_some() && self.scene_str("group") == "System" {
            return out; // the cutscenes and the flythrus aren't played
        }
        let gates = self.entities.iter().any(|e| e.gate.is_some());
        for (i, e) in self.entities.iter().enumerate() {
            let what = if e.gate.is_some() {
                "checkpoint"
            } else if e.tag == "GloverSpawnPoint" && !gates {
                "Glover's start"
            } else {
                continue;
            };
            if e.is_handle || e.derived {
                continue;
            }
            if self.ground_below(self.obj_pos(i), 400.0, Some(i), None).is_none() {
                let msg = format!("{}: no solid ground within 400 under this {}. Glover starts or restarts here and falls.", e.label, what);
                out.push(Check::new("warn", msg).click(Click::Select(Some(i))));
            }
        }
        out
    }

    // -----------------------------------------------------------------------------------------------------------
    // Imports and the memory budget. Imports are derived from the XML and never stored separately. Undo, redo and
    // XML export always match what the ROM build adds.

    /// The models the level uses from banks it doesn't load, each once, in level order.
    pub fn current_imports(&self) -> Vec<String> {
        let mut ids: Vec<String> = Vec::new();
        for e in &self.entities {
            if e.is_handle || e.derived {
                continue;
            }
            if let Some(o) = e.object_id.as_ref().filter(|o| self.is_import(o) && !ids.contains(o)) {
                ids.push(o.clone());
            }
        }
        ids
    }

    pub(crate) fn plan_job(&mut self, ids: Vec<String>) -> PlanJob {
        self.plan_serial += 1;
        PlanJob { backend: self.backend.clone(), sid: self.scene_id().to_string(), ids, serial: self.plan_serial }
    }

    /// Asks for the import plan again. With `delays` it's worked out in the background, a moment later.
    pub fn schedule_import_plan(&mut self) {
        let ids = self.current_imports();
        self.plan_pending = None;
        self.plan_due = None;
        if ids.is_empty() {
            self.plan_serial += 1;
            self.plan = None;
            self.emit(Event::ImportsChanged);
            return;
        }
        match self.delays {
            Some(d) => {
                self.plan_pending = Some(ids);
                self.plan_due = Some(self.now + d.plan);
            }
            None => {
                let res = self.plan_job(ids).run();
                self.apply_plan(res);
            }
        }
    }

    /// Takes the result of a `PlanJob`. A result that an edit or another level has overtaken is dropped.
    pub fn apply_plan(&mut self, res: PlanResult) {
        if res.serial != self.plan_serial || res.sid != self.scene_id() {
            return;
        }
        self.plan = res.plan;
        self.events.push(Event::ImportsChanged);
    }

    /// Every level whose LoadLevel loads the enlarged object or texture bank pays for the imports, not just this
    /// one.
    pub fn import_budget(&self) -> Option<Budget> {
        let p = self.plan.as_ref()?;
        if self.current_imports().is_empty() {
            return None;
        }
        let extra = self.mod_info.heap_extra_bytes as i64;
        let has = |row: &Value, key: &str, bank: &str| {
            row.get(key).and_then(|b| b.as_array()).is_some_and(|b| b.iter().any(|x| x.as_str() == Some(bank)))
        };
        let mut rows = Vec::new();
        for row in &self.scenes {
            let sid = s(row.get("id"));
            let Some(free) = self.heap_free(sid) else { continue };
            let mut cost = 0;
            if has(row, "gameObjectBanks", &p.target_bank) {
                cost += p.object_bytes as i64;
            }
            if has(row, "gameTextureBanks", &p.target_tex_bank) {
                cost += p.tex_bytes.unwrap_or(0) as i64;
            }
            let room = (free - 16 * 1024).max(0);
            if cost > 0 {
                rows.push(BudgetRow { sid: sid.to_string(), name: s(row.get("name")).to_string(), cost, room, ratio: cost as f64 / room.max(1) as f64 });
            }
        }
        if rows.is_empty() {
            return None;
        }
        rows.sort_by(|a, b| b.ratio.partial_cmp(&a.ratio).unwrap_or(std::cmp::Ordering::Equal));
        let mut least = &rows[0];
        for r in &rows {
            if r.room + extra - r.cost < least.room + extra - least.cost {
                least = r;
            }
        }
        Some(Budget {
            least_with_pak: (least.sid.clone(), least.name.clone(), least.room + extra - least.cost),
            over: rows.iter().filter(|r| r.cost > r.room).cloned().collect(),
            over_with_pak: rows.iter().filter(|r| r.cost > r.room + extra).cloned().collect(),
            extra,
            rows,
        })
    }
}
