//! The inspector: what is selected, with its number fields, its commands and their fields, an enemy's script, a
//! puzzle's conditions and actions, a camera zone's settings, and the level's settings when nothing is selected.

use crate::app::{with_format, Act, Window};
use crate::numfield::{self, Spec};
use crate::style::{self, card, check_line, commit_text, fieldname, h2, h4, mini, mini_menu, muted, note, num_input};
use egui::RichText;
use glovebox_core::pyfmt;
use glovebox_core::xml::NodeId;
use glovebox_editor::commands::{refusal, PLATFORMS};
use glovebox_editor::editor::{look_name, Editor, GITHUB, LOOK, LOOKS};
use glovebox_editor::entities::{self, camera_zone, preset_zone, set_camera_zone, zone_camera, zone_centre, EntId, CAM_PRESETS, KINDS};
use glovebox_editor::exits::Who;
use glovebox_editor::puzzles;
use glovebox_editor::scripts;
use glovebox_editor::xmlutil::{body, float_bits, float_from_bits, hex_id, inum};

/// The drag steps of the number fields: one unit or one degree, and a hundredth for a scale.
const UNIT: Spec = Spec::float(1.0);
const FINE: Spec = Spec::float(0.01);
const DEG: f64 = 180.0 / std::f64::consts::PI;
const WORLDS: [&str; 7] = ["ATLANTIS", "CARNIVAL", "PIRATES", "PREHISTORIC", "FORTRESS", "OOTW", "HUB"];

/// The game's songs (gloveworld include/enums.h, enum Track): PrepareSong plays levelInfo's song when a level loads.
pub fn songs() -> Vec<(i64, String)> {
    let worlds = ["Atlantis", "Carnival", "Pirates", "Prehistoric", "Fear", "Space"];
    let mut out = vec![(0, "Silence".to_string())];
    for (w, name) in worlds.iter().enumerate() {
        for (i, part) in ["1", "2", "3", "boss"].iter().enumerate() {
            out.push((1 + 4 * w as i64 + i as i64, format!("{name} {part}")));
        }
    }
    out.extend([(25, "Title".to_string()), (26, "Intro".to_string()), (27, "Outro".to_string())]);
    out.extend((0..6).map(|i| (28 + i, format!("Hub {}", i + 1))));
    out.extend([(58, "Bonus".to_string()), (59, "Hasbro".to_string()), (60, "Title 2".to_string())]);
    out.extend((34..58).map(|k| (k, format!("Jingle {k} (short)"))));
    out
}

pub struct State {
    /// What the panel showed last: a new selection scrolls back to the top.
    shown: Vec<(NodeId, Option<NodeId>)>,
    /// The enemy instruction whose fields are open: the enemy's place in the level, the list and the row.
    pub script_open: Option<(Vec<usize>, usize, usize)>,
    /// The camera zone panel's "player walks" box. None shows the way the view looks.
    facing: Option<String>,
    /// The fields of "Transform selection".
    mv: [String; 3],
    axis: usize,
    deg: String,
    sc: String,
    /// New look: the worlds ticked and the seed.
    worlds: Option<[bool; 7]>,
    pub reskin_seed: i64,
    /// The spin boxes of the bonus clock and of a puzzle, while they differ from the level.
    secs: i64,
    /// A colour being picked: the element's place and the colour.
    colour: Option<(Vec<usize>, [u8; 3])>,
}

impl Default for State {
    fn default() -> State {
        State {
            shown: Vec::new(),
            script_open: None,
            facing: None,
            mv: ["0".to_string(), "0".to_string(), "0".to_string()],
            axis: 0,
            deg: "90".to_string(),
            sc: "1.5".to_string(),
            worlds: None,
            reskin_seed: 0,
            secs: 60,
            colour: None,
        }
    }
}

struct Cx<'a> {
    ed: &'a Editor,
    st: &'a mut State,
    acts: Vec<Act>,
}

impl Cx<'_> {
    fn act(&mut self, f: impl FnOnce(&mut Window) + 'static) {
        self.acts.push(Box::new(f));
    }
    fn ed_act(&mut self, f: impl FnOnce(&mut Editor) + 'static) {
        self.acts.push(Box::new(move |w: &mut Window| f(&mut w.ed)));
    }
}

/// `^-?(0x[0-9a-f]+|\d+(\.\d*)?(e-?\d+)?)$`, without regard to case.
fn looks_like_number(v: &str) -> bool {
    let v = v.strip_prefix('-').unwrap_or(v).to_ascii_lowercase();
    if let Some(h) = v.strip_prefix("0x") {
        return !h.is_empty() && h.bytes().all(|b| b.is_ascii_hexdigit());
    }
    let digits = |s: &str| s.bytes().take_while(|b| b.is_ascii_digit()).count();
    let n = digits(&v);
    if n == 0 {
        return false;
    }
    let mut rest = &v[n..];
    if let Some(r) = rest.strip_prefix('.') {
        rest = &r[digits(r)..];
    }
    if let Some(r) = rest.strip_prefix('e') {
        let r = r.strip_prefix('-').unwrap_or(r);
        let n = digits(r);
        if n == 0 {
            return false;
        }
        rest = &r[n..];
    }
    rest.is_empty()
}

fn kind_name(kind: &str) -> &'static str {
    KINDS.iter().find(|k| k.0 == kind).map_or("", |k| k.1)
}

/// A row of a field grid: the name, then the widget.
fn row<R>(ui: &mut egui::Ui, name: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let r = ui
        .horizontal(|ui| {
            let label = fieldname(ui, name);
            let pad = (96.0 - label.rect.width()).max(0.0);
            ui.add_space(pad);
            add(ui)
        })
        .inner;
    r
}

/// A combo box over (value, text) pairs. Gives the chosen value.
fn combo<T: Clone + PartialEq>(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, label: &str, current: &T, items: &[(T, String)]) -> Option<T> {
    let shown = items.iter().find(|i| i.0 == *current).map(|i| i.1.clone()).unwrap_or_default();
    let mut pick = None;
    let width = ui.available_width().clamp(60.0, 240.0);
    let r = egui::ComboBox::from_id_salt(id).icon(style::combo_icon).selected_text(shown).width(width).truncate().height(360.0).show_ui(ui, |ui| {
        for (v, text) in items {
            if ui.selectable_label(v == current, text).clicked() {
                pick = Some(v.clone());
            }
        }
    });
    if !label.is_empty() {
        let label = label.to_string();
        r.response.widget_info(move || egui::WidgetInfo::labeled(egui::Role::ComboBox, true, &label));
    }
    pick
}

/// A whole-number box that gives its value when the player is done with it, or drags on it.
fn spin(ui: &mut egui::Ui, id: egui::Id, value: i64, range: std::ops::RangeInclusive<i64>, label: &str) -> Option<i64> {
    numfield::int(ui, id, value, range, 70.0, label)
}

pub fn ui(w: &mut Window, ui: &mut egui::Ui) {
    if !w.ed.has_level {
        return;
    }
    let key: Vec<(NodeId, Option<NodeId>)> = w.ed.selection.iter().map(|&i| (w.ed.entities[i].node, w.ed.entities[i].sub)).collect();
    let mut area = egui::ScrollArea::vertical().id_salt("inspector").auto_shrink([false, false]);
    if key != w.inspector.shown {
        w.inspector.shown = key;
        w.inspector.facing = None;
        w.inspector.worlds = None;
        area = area.vertical_scroll_offset(0.0);
    }
    let mut cx = Cx { ed: &w.ed, st: &mut w.inspector, acts: Vec::new() };
    area.show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        egui::Frame::new().inner_margin(egui::Margin { left: 4, right: 8, top: 4, bottom: 8 }).show(ui, |ui| fill(&mut cx, ui));
    });
    let acts = cx.acts;
    w.acts.extend(acts);
}

fn fill(cx: &mut Cx, ui: &mut egui::Ui) {
    let ed = cx.ed;
    let sel = &ed.selection;
    if sel.is_empty() {
        if let Some(p) = ed.puzzle_shown() {
            return puzzle_view(cx, ui, p);
        }
        h2(ui, "No selection");
        ui.label("Click something in the level, drag a box (B), or pick from the list on the left. Ctrl+click adds to the selection.");
        stats(cx, ui);
        return level_settings(cx, ui);
    }
    if sel.len() > 1 {
        return multi_panel(cx, ui);
    }
    let i = sel[0];
    let ent = &ed.entities[i];
    h2(ui, &ent.label);
    let mut sub = format!("{} · <{}>", kind_name(ent.kind), ent.tag);
    if let Some(oid) = &ent.object_id {
        sub.push_str(&format!(" · model {oid}"));
        if let (true, Some(c)) = (ent.imported, ed.catalog.get(oid)) {
            sub.push_str(&format!(" · imported from {}", c.bank));
        }
    }
    muted(ui, &sub);
    for (level, msg) in ed.issues_for(i) {
        check_line(ui, level, &msg, false, None, None, None);
    }
    if ent.bind.is_some() {
        transform_panel(cx, ui, i);
    }
    if ent.zone {
        camera_zone_panel(cx, ui, i);
    }
    exit_panel(cx, ui, i);
    if ent.kind == "trigger" {
        let node = ent.node;
        if ui.button("Edit its puzzle (conditions and actions)").clicked() {
            cx.ed_act(move |ed| ed.open_puzzle(Some(node)));
        }
    }
    let node = if ent.tag == "GaribGroup" || ent.is_handle { ent.sub.unwrap_or(ent.node) } else { ent.node };
    if ed.doc.tag(node) == "Enemy" {
        enemy_panel(cx, ui, node);
    } else {
        cmd_card(cx, ui, node, true, "", true);
    }
    if matches!(ed.doc.tag(node), "LandActor" | "BackgroundActor") {
        if let Some(oid) = ent.object_id.clone() {
            new_look(cx, ui, node, &oid);
        }
    }
    if ent.tag == "GaribGroup" {
        let n = ed.doc.kids(ent.node, "Garib").len();
        cmd_card(cx, ui, ent.node, false, &format!(" · parent of {n} garib{}", if n == 1 { "" } else { "s" }), true);
    } else if let (true, Some(owner)) = (ent.is_handle, ent.owner) {
        if ui.button(format!("Select {}", ed.entities[owner].label)).clicked() {
            cx.ed_act(move |ed| ed.select(Some(owner), false));
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The level's settings

fn stats(cx: &mut Cx, ui: &mut egui::Ui) {
    let ed = cx.ed;
    let all = ed.doc.iter(ed.doc.root());
    let count = |t: &str| all.iter().filter(|n| ed.doc.tag(**n) == t).count();
    let rows = [
        ("Garibs", count("Garib")),
        ("Enemies", count("Enemy")),
        ("Platforms", count("Platform") + count("NullPlatform")),
        ("Puzzles", count("Puzzle")),
        ("Mr. Tip messages", count("MrTip")),
        ("Landscape pieces", count("LandActor")),
        ("Commands", all.len().saturating_sub(1)),
    ];
    ui.add_space(4.0);
    egui::Grid::new("stats").num_columns(2).spacing([24.0, 3.0]).show(ui, |ui| {
        for (a, b) in rows {
            muted(ui, a);
            ui.label(b.to_string());
            ui.end_row();
        }
    });
}

fn level_settings(cx: &mut Cx, ui: &mut egui::Ui) {
    let ed = cx.ed;
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            mini_menu(ui, "+ Add", "+ Add level setting", "", |ui| {
                for (tag, text) in LOOK {
                    if ui.button(text).clicked() {
                        cx.ed_act(move |ed| ed.add_look(tag));
                    }
                }
            });
            mini_menu(ui, "Look", "", "Set the lights and fog to a preset (dark, coloured moods, or the shipped levels' daylight)", |ui| {
                for l in &LOOKS {
                    if ui.button(l.name).clicked() {
                        let name = l.name;
                        cx.ed_act(move |ed| ed.apply_look(name));
                    }
                }
            });
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| h4(ui, "LEVEL SETTINGS"));
        });
    });
    note(ui, "The game reads these once when the level starts. Fog's near_clip is the far plane: the game draws everything nearer than it, so a shorter one draws less.");
    let root = ed.doc.root();
    let name = ed.doc.attr(root, "name").to_string();
    row(ui, "File name", |ui| {
        let (text, r) = commit_text(ui, egui::Id::unique("level file name"), &name, |t| t.char_limit(7).desired_width(120.0));
        style::name(&r, "File name");
        if let Some(v) = text {
            cx.ed_act(move |ed| ed.set_attr(root, "name", &v));
        }
    });
    note(ui, "The name inside the level file, 7 letters like the shipped ones (an even length crashes a console). It isn't the title the game shows.");
    slot_card(cx, ui);
    for node in ed.doc.children(root).to_vec() {
        if look_name(ed.doc.tag(node)).is_some() {
            cmd_card(cx, ui, node, true, "", true);
        }
    }
    puzzle_list(cx, ui);
}

/// The level's row in the game's level table: title, music and bonus clock (not in the level file).
fn slot_card(cx: &mut Cx, ui: &mut egui::Ui) {
    let ed = cx.ed;
    let info = ed.level_info();
    card(ui, "Title, music and clock", None, |_| {}, |ui| {
        let shown = info.name.clone().unwrap_or_default();
        row(ui, "title", |ui| {
            let (text, r) = commit_text(ui, egui::Id::unique("level title"), &shown, |t| t.char_limit(39).hint_text("the shipped title").desired_width(f32::INFINITY));
            style::name(&r, "title");
            if let Some(v) = text {
                let v = v.to_uppercase().trim().to_string();
                let ok = v.chars().all(|c| c.is_ascii_alphanumeric() || " .-!?':".contains(c));
                cx.ed_act(move |ed| {
                    if !ok {
                        return ed.set_status("Use letters, numbers, spaces and . - ! ? ' : in a title.", "err");
                    }
                    let v = (!v.is_empty()).then_some(v);
                    if v != ed.level_info().name {
                        ed.set_level_info(|i| i.name = v);
                    }
                });
            }
        });
        row(ui, "music", |ui| {
            let mut items: Vec<(Option<i64>, String)> = vec![(None, "Keep the shipped music".to_string())];
            items.extend(songs().into_iter().map(|(k, text)| (Some(k), format!("{k}  {text}"))));
            if let Some(v) = combo(ui, "song", "music", &info.song, &items) {
                cx.ed_act(move |ed| ed.set_level_info(|i| i.song = v));
            }
        });
        let bt = info.bonus_time;
        let mode = match bt {
            None => 0,
            Some(-1) => 1,
            Some(_) => 2,
        };
        if let Some(t) = bt.filter(|t| *t > 0) {
            cx.st.secs = t;
        }
        row(ui, "clock", |ui| {
            let items = [(0, "Keep the shipped clock".to_string()), (1, "No clock".to_string()), (2, "Seconds:".to_string())];
            if let Some(k) = combo(ui, "clock", "clock", &mode, &items) {
                let secs = cx.st.secs;
                cx.ed_act(move |ed| ed.set_level_info(|i| i.bonus_time = [None, Some(-1), Some(secs)][k]));
            }
        });
        if mode == 2 {
            row(ui, "", |ui| {
                if let Some(v) = spin(ui, egui::Id::unique("clock seconds"), cx.st.secs, 1..=32766, "seconds") {
                    cx.st.secs = v;
                    cx.ed_act(move |ed| ed.set_level_info(|i| i.bonus_time = Some(v)));
                }
            });
        }
        note(ui, "The game's level table, written into the ROM at build. The title shows in the level select. At 0 the clock says TIME UP and sends the player to the way room, as in a bonus level.");
        if ed.can_be_extra() {
            let mut on = info.extra;
            if style::checkbox(ui, &mut on, "Extra level (Expansion Pak)").changed() {
                cx.ed_act(move |ed| ed.set_level_info(|i| i.extra = on));
            }
            note(ui, "An extra level is stored behind the game mod. The shipped level stays in its slot. Players reach the extra levels through an Extra levels entry zone in a hub (Add, Presets), then the Space way room. Progress in them is kept apart from the shipped Space levels.");
        }
    });
}

/// Reskin: the same model with textures from other worlds.
fn new_look(cx: &mut Cx, ui: &mut egui::Ui, node: NodeId, oid: &str) {
    let ed = cx.ed;
    let info = ed.catalog.get(oid);
    let custom = info.and_then(|c| c.custom.as_ref());
    let from = custom.and_then(|c| c.reskin_from.clone());
    let Some(src) = from.clone().or_else(|| custom.is_none().then(|| oid.to_string())) else { return };
    let own = ed.catalog.get(&src).and_then(|c| c.world.clone());
    card(ui, "New look", None, |_| {}, |ui| {
        note(ui, "A copy of the model with textures from other worlds. The copy is kept with your imported models and copied into the level's banks at ROM build.");
        let ticks = cx.st.worlds.get_or_insert_with(|| std::array::from_fn(|i| WORLDS[i] == "PIRATES" && own.as_deref() != Some("PIRATES")));
        egui::Grid::new("worlds").num_columns(2).show(ui, |ui| {
            for (i, world) in WORLDS.iter().enumerate() {
                let name = if *world == "OOTW" { "Space".to_string() } else { crate::app::title_case(world) };
                ui.add_enabled_ui(own.as_deref() != Some(*world), |ui| style::checkbox(ui, &mut ticks[i], name));
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });
        row(ui, "seed", |ui| {
            if let Some(v) = spin(ui, egui::Id::unique("reskin seed"), cx.st.reskin_seed, 0..=99999, "seed") {
                cx.st.reskin_seed = v;
            }
        });
        let ticks = *ticks;
        let go = |cx: &mut Cx, next: bool| {
            let worlds: Vec<String> = WORLDS.iter().zip(ticks).filter(|(w_, on)| *on && own.as_deref() != Some(**w_)).map(|(w_, _)| w_.to_string()).collect();
            if worlds.is_empty() {
                return cx.ed_act(|ed| ed.set_status("Pick at least one world.", "err"));
            }
            if next {
                cx.st.reskin_seed += 1;
            }
            let seed = cx.st.reskin_seed;
            cx.act(move |w| w.reskin(node, worlds, seed));
        };
        ui.horizontal(|ui| {
            if ui.button("Apply").clicked() {
                go(cx, false);
            }
            if ui.button("Try another").clicked() {
                go(cx, true);
            }
            if from.is_some() && ui.button("Original").on_hover_text("Back to the shipped model").clicked() {
                let name: String = ed.catalog.get(&src).map(|c| c.name.clone()).unwrap_or_default().chars().take(7).collect();
                let src = src.clone();
                cx.ed_act(move |ed| {
                    ed.edit(|ed| {
                        ed.doc.set(node, "object_id", &src);
                        ed.doc.set(node, "name", &name);
                    });
                });
            }
        });
    });
}

// ---------------------------------------------------------------------------------------------------------------
// Enemy scripts

/// An enemy's own fields and commands, then its script as three lists.
fn enemy_panel(cx: &mut Cx, ui: &mut egui::Ui, enemy: NodeId) {
    let ed = cx.ed;
    cmd_card(cx, ui, enemy, false, "", true);
    for c in ed.doc.children(enemy).to_vec() {
        let tag = ed.doc.tag(c);
        if !scripts::GROUPS.contains(&tag) && tag != "EnemyFinalize" {
            cmd_card(cx, ui, c, true, "", true);
        }
    }
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            mini_menu(ui, "Ready-made", "", "Replace the normal list with a ready-made script", |ui| {
                for name in scripts::TEMPLATES {
                    if ui.button(name).clicked() {
                        cx.ed_act(move |ed| ed.add_script_template(enemy, name));
                    }
                }
            });
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| h4(ui, "ENEMY SCRIPT"));
        });
    });
    note(ui, "The enemy runs its normal list from #0. Each instruction ends when its lifetime runs out (0 or -1: no limit) or its action is done. Conditional ones interrupt it when their condition holds; Start the attack list runs the attack list.");
    let lists = scripts::lists(&ed.doc, enemy);
    let enemy_path = ed.doc.path_of(enemy);
    let types: Vec<(i128, String)> = (0..scripts::TYPE_COUNT).filter_map(|t| scripts::type_name(t).map(|n| (t, format!("{n}   ({t})")))).collect();
    for (gi, kind) in scripts::GROUPS.iter().enumerate() {
        let items = &lists[gi];
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.push_id(("add instruction", gi), |ui| {
                    mini_menu(ui, "+ Add", "+ Add instruction", "", |ui| {
                        for (t, text) in &types {
                            if ui.button(text).clicked() {
                                let t = *t;
                                cx.ed_act(move |ed| ed.add_instruction(enemy, kind, t));
                            }
                        }
                    });
                });
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    h4(ui, &format!("{}  {}/{}", scripts::kind_name(kind), items.len(), scripts::limit(kind)));
                });
            });
        });
        for (i, &ins) in items.iter().enumerate() {
            let t = scripts::code(&ed.doc, ins);
            let bd = body(&ed.doc, ins);
            let extra = match bd {
                Some(b) if t == 18 => format!(" → #{}", inum(&ed.doc, Some(b), "instr_idx", 0)),
                _ => String::new(),
            };
            let life = inum(&ed.doc, Some(ins), "lifetime", 0);
            let key = (enemy_path.clone(), gi, i);
            let open = cx.st.script_open.as_ref() == Some(&key);
            let title = format!("#{i}  {}{extra}", scripts::type_name(t).map(str::to_string).unwrap_or_else(|| format!("type {t}")));
            let op = (life > 0).then(|| format!("{life} frames"));
            let removable = ed.removal_refused(ins).is_none();
            let mut head_acts: Vec<Act> = Vec::new();
            let mut toggle = false;
            ui.push_id(("instruction", gi, i), |ui| {
                card(
                    ui,
                    &title,
                    op.as_deref(),
                    |ui| {
                        if mini(ui, "…", "Show or hide its fields").clicked() {
                            toggle = true;
                        }
                        if mini(ui, "↑", "Move earlier").clicked() {
                            head_acts.push(Box::new(move |w: &mut Window| w.ed.move_instruction(ins, -1)));
                        }
                        if mini(ui, "↓", "Move later").clicked() {
                            head_acts.push(Box::new(move |w: &mut Window| w.ed.move_instruction(ins, 1)));
                        }
                        if removable && mini(ui, "×", "Remove").clicked() {
                            head_acts.push(Box::new(move |w: &mut Window| w.ed.remove_instruction(ins)));
                        }
                    },
                    |ui| {
                        let cond = ed.doc.get(ins, "execution_condition").filter(|c| !c.is_empty()).unwrap_or("always");
                        if cond != "always" {
                            note(ui, &format!("when {}", cond.replace('_', " ")));
                        }
                        if scripts::needs_target(t) {
                            let fl = scripts::flags_value(ed.doc.get(ins, "flags"), ed.flag_enum().as_ref());
                            let mut items: Vec<(i128, String)> = scripts::TARGETS.iter().map(|(b, text)| (*b, format!("Target: {text}"))).collect();
                            let cur = scripts::TARGETS.iter().map(|x| x.0).find(|b| fl & b != 0).unwrap_or(0);
                            if cur == 0 {
                                items.push((0, "Target: none (pick one)".to_string()));
                            }
                            if let Some(bit) = combo(ui, "target", "target", &cur, &items) {
                                cx.ed_act(move |ed| ed.set_instruction_target(ins, bit));
                            }
                        }
                        if open {
                            if let Some(k) = combo(ui, "type", "instruction type", &t, &types) {
                                cx.ed_act(move |ed| ed.set_instruction_type(ins, k));
                            }
                            cmd_card(cx, ui, ins, false, "", false);
                            if let Some(b) = bd {
                                cmd_card(cx, ui, b, false, "", false);
                            }
                        }
                    },
                );
            });
            if toggle {
                cx.st.script_open = if open { None } else { Some(key) };
            }
            cx.acts.extend(head_acts);
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Puzzles

/// Every puzzle in the level, also those with no trigger volume in the view.
fn puzzle_list(cx: &mut Cx, ui: &mut egui::Ui) {
    let ed = cx.ed;
    let ps = ed.doc.kids(ed.doc.root(), "Puzzle");
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if mini(ui, "+ New puzzle", "").clicked() {
                cx.ed_act(|ed| ed.add_puzzle());
            }
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| h4(ui, &format!("PUZZLES ({})", ps.len())));
        });
    });
    note(ui, "A puzzle runs its actions when its conditions hold. The game tests every puzzle each frame.");
    for (i, p) in ps.into_iter().enumerate() {
        let text = format!("{}. {}", i + 1, puzzles::summary(&ed.doc, p));
        let r = ui.add(egui::Label::new(RichText::new(text).color(style::theme(ui.ctx()).text)).sense(egui::Sense::click())).on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(format!("Open puzzle {}", i + 1));
        if r.clicked() {
            cx.ed_act(move |ed| ed.open_puzzle(Some(p)));
        }
        ui.separator();
    }
}

/// The puzzle card: when it fires, its condition logic, its conditions and its actions.
fn puzzle_view(cx: &mut Cx, ui: &mut egui::Ui, p: NodeId) {
    let ed = cx.ed;
    if ui.button("← Level settings and all puzzles").clicked() {
        cx.ed_act(|ed| ed.open_puzzle(None));
    }
    let i = ed.doc.kids(ed.doc.root(), "Puzzle").iter().position(|x| *x == p).unwrap_or(0);
    h2(ui, &format!("Puzzle {}", i + 1));
    muted(ui, &puzzles::summary(&ed.doc, p));
    for v in &ed.violations {
        if v.top == p {
            check_line(ui, if v.shipped { "note" } else { v.level }, &v.msg, false, None, None, None);
        }
    }
    let n = puzzles::fires(&ed.doc, p);
    let logic = puzzles::logic(&ed.doc, p);
    let conds = ed.doc.kids(p, "PuzzleCond").len() as i64;
    let mut delete = false;
    card(
        ui,
        "Puzzle",
        None,
        |ui| {
            if mini(ui, "×", "Delete this puzzle").clicked() {
                delete = true;
            }
        },
        |ui| {
            let mode = match n {
                None => 0,
                Some(1) => 1,
                Some(_) => 2,
            };
            let times = n.unwrap_or(1);
            row(ui, "Fires", |ui| {
                let items = [(0, "Every frame while true".to_string()), (1, "Once".to_string()), (2, "A number of times".to_string())];
                if let Some(k) = combo(ui, "fires", "Fires", &mode, &items) {
                    cx.ed_act(move |ed| ed.set_fires(p, [None, Some(1), Some(times.max(2))][k]));
                }
            });
            if mode == 2 {
                row(ui, "", |ui| {
                    if let Some(v) = spin(ui, egui::Id::unique("puzzle times"), times, 1..=32767, "times") {
                        cx.ed_act(move |ed| ed.set_fires(p, Some(v)));
                    }
                });
            }
            let (how, m) = (logic.name(), logic.count());
            let least = m.max(1);
            row(ui, "Needs", |ui| {
                let items = [("all", "All conditions".to_string()), ("any", "Any condition".to_string()), ("least", "At least N conditions".to_string())];
                if let Some(k) = combo(ui, "needs", "Needs", &how, &items) {
                    cx.ed_act(move |ed| ed.set_logic(p, k, least));
                }
            });
            if how == "least" {
                row(ui, "", |ui| {
                    if let Some(v) = spin(ui, egui::Id::unique("puzzle least"), least, 1..=m.max(conds).max(1), "conditions") {
                        cx.ed_act(move |ed| ed.set_logic(p, "least", v));
                    }
                });
            }
            note(ui, "Without a limit, actions run again on every frame the conditions hold. Camera zones need that; most other puzzles want Once.");
        },
    );
    if delete {
        cx.ed_act(move |ed| ed.remove_puzzle(p));
    }
    type Names = fn(i128) -> Option<&'static str>;
    let kinds: [(&'static str, &[i128], Names, &str, &str); 2] = [
        ("PuzzleCond", puzzles::COND_CODES, puzzles::cond_name, "cond_type", "CONDITIONS"),
        ("PuzzleAction", puzzles::ACTION_CODES, puzzles::action_name, "action_type", "ACTIONS"),
    ];
    for (kind, codes, name_of, attr, title) in kinds {
        let names: Vec<(i128, String)> = codes.iter().filter_map(|k| name_of(*k).map(|t| (*k, format!("{t}   ({k})")))).collect();
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.push_id(("add part", kind), |ui| {
                    mini_menu(ui, "+ Add", if kind == "PuzzleCond" { "+ Add condition" } else { "+ Add action" }, "", |ui| {
                        for (k, text) in &names {
                            if ui.button(text).clicked() {
                                let k = *k;
                                cx.ed_act(move |ed| {
                                    ed.add_puzzle_part(p, kind, k);
                                });
                            }
                        }
                    });
                });
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| h4(ui, title));
            });
        });
        let parts = ed.doc.kids(p, kind);
        if parts.is_empty() {
            note(ui, if kind == "PuzzleCond" { "None: it fires when the level starts." } else { "None yet." });
        }
        for (pi, part) in parts.into_iter().enumerate() {
            let t = puzzles::code(&ed.doc, part, attr);
            let title = name_of(t).map(str::to_string).unwrap_or_else(|| format!("{kind} {t}"));
            let mut head_acts: Vec<Act> = Vec::new();
            ui.push_id((kind, pi), |ui| {
                card(
                    ui,
                    &title,
                    Some(&t.to_string()),
                    |ui| {
                        if mini(ui, "↑", "Move earlier").clicked() {
                            head_acts.push(Box::new(move |w: &mut Window| w.ed.move_node(part, -1)));
                        }
                        if mini(ui, "↓", "Move later").clicked() {
                            head_acts.push(Box::new(move |w: &mut Window| w.ed.move_node(part, 1)));
                        }
                        if mini(ui, "×", "Remove").clicked() {
                            head_acts.push(Box::new(move |w: &mut Window| w.ed.remove_node(part)));
                        }
                    },
                    |ui| {
                        let mut items = names.clone();
                        if !items.iter().any(|x| x.0 == t) {
                            items.push((t, t.to_string()));
                        }
                        if let Some(k) = combo(ui, "type", "type", &t, &items) {
                            cx.ed_act(move |ed| ed.set_part_type(part, k));
                        }
                        if let Some(b) = body(&ed.doc, part) {
                            cmd_card(cx, ui, b, false, "", false);
                        }
                    },
                );
            });
            cx.acts.extend(head_acts);
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// One object

/// Three number boxes in a row. Gives the index and value of the one that changed.
/// A drag on a box changes its number by `specs[i]`, or by the last of `specs`.
fn trio(ui: &mut egui::Ui, id: egui::Id, values: &[f64], names: &[&str], enabled: bool, specs: &[Spec]) -> Option<(usize, f64)> {
    let mut out = None;
    ui.add_enabled_ui(enabled, |ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let width = ((ui.available_width() - 4.0 * (values.len() as f32 - 1.0)) / values.len().max(1) as f32 - 10.0).clamp(36.0, 70.0);
        for (i, v) in values.iter().enumerate() {
            let spec = specs.get(i).or(specs.last()).copied().unwrap_or(UNIT);
            if let Some(n) = num_input(ui, id.with(i), *v, width, names.get(i).copied().unwrap_or(""), spec) {
                out = Some((i, n));
            }
        }
    });
    out
}

/// Position, rotation and scale as the game stores them for this kind of object.
fn transform_panel(cx: &mut Cx, ui: &mut egui::Ui, i: EntId) {
    let ed = cx.ed;
    let ent = &ed.entities[i];
    let Some(bind) = ent.bind.clone() else { return };
    card(ui, "Transform", None, |_| {}, |ui| {
        let id = egui::Id::unique(("transform", ed.doc.path_of(ent.sub.unwrap_or(ent.node)), ent.is_handle.then(|| ent.label.clone())));
        let p = bind.get(&ed.doc, &ed.entities);
        if let Some((k, v)) = row(ui, "position", |ui| trio(ui, id.with("p"), &p, &["position x", "position y", "position z"], true, &[UNIT])) {
            let points = ent.points.clone();
            cx.ed_act(move |ed| {
                ed.edit(|ed| {
                    let p = bind.get(&ed.doc, &ed.entities);
                    let mut d = [0.0; 3];
                    d[k] = v - p[k];
                    bind.set(&mut ed.doc, &ed.entities, glovebox_editor::vec::add(p, d));
                    for b in &points {
                        let q = b.get(&ed.doc, &ed.entities);
                        b.set(&mut ed.doc, &ed.entities, glovebox_editor::vec::add(q, d));
                    }
                });
            });
        }
        match (ent.rot, ent.get_quat(&ed.doc)) {
            (Some("xyz"), Some(q)) => {
                let e = entities::euler_of(q);
                let deg = [e[0] * DEG, e[1] * DEG, e[2] * DEG];
                if let Some((k, v)) = row(ui, "rotation °", |ui| trio(ui, id.with("r"), &deg, &["rotation x", "rotation y", "rotation z"], true, &[UNIT])) {
                    cx.ed_act(move |ed| {
                        ed.edit(|ed| {
                            let Some(q) = ed.entities.get(i).and_then(|e| e.get_quat(&ed.doc)) else { return };
                            let mut cur = entities::euler_of(q);
                            cur[k] = v.to_radians();
                            let ent = ed.entities[i].clone();
                            ent.set_quat(&mut ed.doc, entities::quat_from_xyz(cur));
                        });
                    });
                }
                note(ui, ent.why_rot.unwrap_or("SetActorRotation, applied Y·Z·X like GetQuaternionFromXYZRot in the decomp."));
            }
            _ => {
                fieldname(ui, "rotation");
                note(ui, ent.why_rot.unwrap_or(entities::WHY_NO_ORIENTATION));
            }
        }
        match (ent.scl, ent.get_scale(&ed.doc), &ent.volume) {
            (Some("xyz"), Some(s), _) => {
                if let Some((k, v)) = row(ui, "scale", |ui| trio(ui, id.with("s"), &s, &["scale x", "scale y", "scale z"], true, &[FINE])) {
                    cx.ed_act(move |ed| {
                        ed.edit(|ed| {
                            let Some(mut s) = ed.entities.get(i).and_then(|e| e.get_scale(&ed.doc)) else { return };
                            s[k] = v;
                            let ent = ed.entities[i].clone();
                            ent.set_scale(&mut ed.doc, s);
                        });
                    });
                }
                let why = if ent.tag == "Platform" {
                    "PlatScale. The game ignores it after the first path point. The editor keeps it before them."
                } else {
                    ent.why_scl.unwrap_or("SetActorScale: x, y, z in the model’s own axes (guScaleF in object.c).")
                };
                note(ui, why);
            }
            (Some("box"), _, Some(vol)) => {
                let names = vol.size_fields();
                let values: Vec<f64> = (0..names.len()).map(|k| vol.size_get(&ed.doc, k)).collect();
                if let Some((k, v)) = row(ui, "size", |ui| trio(ui, id.with("b"), &values, &names, true, &[UNIT])) {
                    let vol = vol.clone();
                    cx.ed_act(move |ed| {
                        ed.edit(|ed| vol.size_set(&mut ed.doc, k, v));
                    });
                }
                note(ui, &format!("{}{}", names.join(", "), if ent.kind == "trigger" { ". Scaling keeps the centre." } else { "" }));
            }
            _ => {
                fieldname(ui, "scale");
                note(ui, ent.why_scl.unwrap_or(entities::WHY_NO_SCALE));
            }
        }
    });
}

/// A platform's exit as the game tests it: the radius, who must come within it and what else the game asks.
fn exit_panel(cx: &mut Cx, ui: &mut egui::Ui, i: EntId) {
    let Some(t) = cx.ed.exit_trigger(i) else { return };
    card(ui, "Exit trigger", None, |_| {}, |ui| {
        let who = match t.rule.who {
            Who::Glover => "Glover (the hand\u{2019}s centre)",
            Who::Ball => "the ball (its centre)",
            Who::OnPlatform => "the ball, resting on the platform",
        };
        row(ui, "exit type", |ui| ui.label(t.exit_type.to_string()));
        row(ui, "radius", |ui| match t.radius {
            Some(r) => ui.label(format!("{r:.1}")),
            None => ui.label("unknown"),
        });
        row(ui, "who", |ui| ui.label(who));
        match (t.rule.who, t.radius) {
            (Who::OnPlatform, _) => {
                note(ui, "The game tests no distance for this exit type. The view draws no sphere.");
            }
            (_, None) => {
                note(ui, "The editor does not have this model\u{2019}s vertices. It cannot tell the size, and draws no sphere.");
            }
            (_, Some(r)) if r > 0.0 => {
                note(ui, "The exit fires when the centre comes closer to the platform\u{2019}s position than the radius: the sphere in the view. The game measures the radius once, on the model at the scale it has when the position is read.");
            }
            _ => {
                let why = t.zero.as_ref().map(|z| z.text()).unwrap_or_default();
                check_line(ui, "err", &format!("The radius is 0. This exit can never fire. {why}"), false, None, None, None);
            }
        }
        for n in &t.rule.notes {
            note(ui, n);
        }
    });
}

fn camera_zone_panel(cx: &mut Cx, ui: &mut egui::Ui, i: EntId) {
    let ed = cx.ed;
    let puzzle = ed.entities[i].node;
    let Some(z) = camera_zone(&ed.doc, puzzle) else { return };
    card(ui, "Camera zone", None, |_| {}, |ui| {
        let facing = cx.st.facing.get_or_insert_with(|| format!("{}", pyfmt::py_round(ed.view_facing() * DEG) as i64)).clone();
        row(ui, "preset", |ui| {
            let mut items = vec![("", "Choose…".to_string())];
            items.extend(CAM_PRESETS.iter().map(|(k, lab, about)| (*k, format!("{lab}: {about}"))));
            if let Some(k) = combo(ui, "zone preset", "preset", &"", &items) {
                let fv = pyfmt::parse_float(facing.trim()).unwrap_or(0.0);
                if let (false, Some(vol)) = (k.is_empty(), z.vol) {
                    cx.ed_act(move |ed| {
                        let c = zone_centre(&ed.doc, vol);
                        if let Some(zone) = preset_zone(k, fv / DEG, c) {
                            ed.edit(|ed| set_camera_zone(&mut ed.doc, puzzle, &zone));
                        }
                    });
                }
            }
        });
        row(ui, "player walks °", |ui| {
            if let Some(f) = cx.st.facing.as_mut() {
                let (r, _) = numfield::bound(ui, egui::Id::unique("zone facing"), f, UNIT, |t| t.desired_width(60.0));
                style::name(&r, "player walks");
            }
        });
        note(ui, "A preset replaces the actions below. “Player walks” is the heading the player goes along in the zone (0 is +z); it starts as the way the view looks.");

        let facing_now = ed.view_facing();
        let orbit = ed.orbit.target;
        // look
        let mut on = z.look.is_some();
        ui.horizontal(|ui| {
            if style::checkbox(ui, &mut on, "look °").changed() {
                cx.ed_act(move |ed| {
                    ed.write_zone(puzzle, |cur| {
                        if on {
                            (cur.look, cur.speed) = (Some(facing_now), Some(2.0));
                        } else {
                            cur.look = None;
                        }
                    })
                });
            }
            let vals = [z.look.unwrap_or(0.0) * DEG, z.speed.unwrap_or(2.0)];
            if let Some((k, v)) = trio(ui, egui::Id::unique("zone look"), &vals, &["look", "look speed"], z.look.is_some(), &[UNIT, Spec::float(0.1)]) {
                cx.ed_act(move |ed| ed.write_zone(puzzle, |cur| if k == 0 { cur.look = Some(v / DEG) } else { cur.speed = Some(v) }));
            }
        });
        note(ui, "LookAtGlover: the heading from the camera to Glover, and how fast it turns there (the shipped levels use 1 to 4).");
        // distance and height
        for (name, value, start, why, which) in [
            ("distance", z.distance, 250.0, "TweenDistance: how far behind Glover the camera stands.", 0),
            ("height", z.height, 100.0, "TweenYAdjust: how far over Glover the camera rises (negative goes under).", 1),
        ] {
            let mut on = value.is_some();
            ui.horizontal(|ui| {
                let set = move |cur: &mut entities::Zone, v: Option<f64>| if which == 0 { cur.distance = v } else { cur.height = v };
                if style::checkbox(ui, &mut on, name).changed() {
                    cx.ed_act(move |ed| ed.write_zone(puzzle, |cur| set(cur, on.then_some(start))));
                }
                if let Some((_, v)) = trio(ui, egui::Id::unique(("zone", name)), &[value.unwrap_or(0.0)], &[name], value.is_some(), &[UNIT]) {
                    cx.ed_act(move |ed| ed.write_zone(puzzle, |cur| set(cur, Some(v))));
                }
            });
            note(ui, why);
        }
        // fly to
        let mut on = z.fly.is_some();
        ui.horizontal(|ui| {
            if style::checkbox(ui, &mut on, "fly to").changed() {
                cx.ed_act(move |ed| {
                    ed.edit(|ed| {
                        let Some(mut cur) = camera_zone(&ed.doc, puzzle) else { return };
                        if on {
                            let mut probe = cur.clone();
                            probe.look = probe.look.or(Some(facing_now));
                            cur.fly = Some(zone_camera(&ed.doc, &probe).map_or(orbit, |v| v.1));
                        } else {
                            cur.fly = None;
                        }
                        set_camera_zone(&mut ed.doc, puzzle, &cur);
                    });
                });
            }
            let fly = z.fly.unwrap_or([0.0; 3]);
            if let Some((k, v)) = trio(ui, egui::Id::unique("zone fly"), &fly, &["fly to x", "fly to y", "fly to z"], z.fly.is_some(), &[UNIT]) {
                cx.ed_act(move |ed| {
                    ed.write_zone(puzzle, |cur| {
                        if let Some(f) = cur.fly.as_mut() {
                            f[k] = v;
                        }
                    })
                });
            }
        });
        note(ui, "FlyTowardsPoint: the camera flies to this point and watches Glover from there.");
        if let Some(view) = zone_camera(&ed.doc, &z) {
            if ui.button("Look from the zone’s camera").clicked() {
                cx.act(move |w| {
                    w.ed.camera.pos = view.1;
                    w.ed.orbit.target = view.0;
                    let orbit = w.ed.orbit.clone();
                    orbit.update(&mut w.ed.camera);
                    w.view.dirty = true;
                });
            }
            note(ui, "Shows about what the player sees with Glover in the middle of the box.");
        }
    });
}

// ---------------------------------------------------------------------------------------------------------------
// Commands

/// + (add a command inside), Tidy on a platform, ↑ ↓ (move) and × (remove) for an element of the level. Menu
/// entries the element can't take are greyed out, with the reason as the tooltip.
fn command_buttons(cx: &mut Cx, ui: &mut egui::Ui, node: NodeId) {
    let ed = cx.ed;
    let tag = ed.doc.tag(node).to_string();
    let allowed = with_format(ed, |f| f.children_of(&tag).into_iter().map(|t| (f.label(&t).to_string(), t)).collect::<Vec<_>>());
    if !allowed.is_empty() {
        mini_menu(ui, "+", "", &format!("Add a command inside <{tag}>"), |ui| {
            for (label, t) in &allowed {
                let text = if label == t { t.clone() } else { format!("{label}   <{t}>") };
                let why = refusal(&ed.doc, node, t);
                let r = ui.add_enabled(why.is_none(), egui::Button::new(text));
                if let Some(why) = why {
                    r.on_disabled_hover_text(why);
                } else if r.clicked() {
                    let t = t.clone();
                    cx.ed_act(move |ed| {
                        ed.add_command(node, &t, &[]);
                    });
                }
            }
        });
    }
    if PLATFORMS.contains(&tag.as_str()) && ed.doc.len(node) > 1 && mini(ui, "Tidy", "Tidy order: move the commands the game would read in the wrong order").clicked() {
        cx.ed_act(move |ed| ed.tidy_node(node));
    }
    if ed.doc.parent(node).is_some() && tag != "EnemyFinalize" {
        if mini(ui, "↑", "Move earlier (the game reads the level in order)").clicked() {
            cx.ed_act(move |ed| ed.move_node(node, -1));
        }
        if mini(ui, "↓", "Move later").clicked() {
            cx.ed_act(move |ed| ed.move_node(node, 1));
        }
        if ed.removal_refused(node).is_none() && mini(ui, "×", &format!("Remove <{tag}>")).clicked() {
            cx.ed_act(move |ed| ed.remove_node(node));
        }
    }
}

/// A swatch of r, g, b that opens a colour picker. Fog and lights use 0 to 255: the game keeps the low byte of a
/// bigger value.
fn colour_button(cx: &mut Cx, ui: &mut egui::Ui, node: NodeId) {
    let ed = cx.ed;
    let clamp = |a: &str| inum(&ed.doc, Some(node), a, 0).clamp(0, 255) as u8;
    let stored = [clamp("r"), clamp("g"), clamp("b")];
    let path = ed.doc.path_of(node);
    let mut rgb = match &cx.st.colour {
        Some((p, c)) if *p == path => *c,
        _ => stored,
    };
    ui.horizontal(|ui| {
        let r = ui.color_edit_button_srgb(&mut rgb);
        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ColorWell, true, "Colour…"));
        muted(ui, "Colour…");
        if r.changed() {
            cx.st.colour = Some((path.clone(), rgb));
        }
    });
    // Written once the picker is let go, as one undo step
    let picking = ui.input(|i| i.pointer.any_down());
    if !picking && matches!(&cx.st.colour, Some((p, _)) if *p == path) {
        cx.st.colour = None;
        if rgb != stored {
            cx.ed_act(move |ed| {
                ed.edit(|ed| {
                    for (a, v) in ["r", "g", "b"].iter().zip(rgb) {
                        ed.doc.set(node, a, &v.to_string());
                    }
                });
            });
        }
    }
}

/// What the decompilation says about a command: its token, where the game reads it, and its enum names.
fn decomp_block(cx: &mut Cx, ui: &mut egui::Ui, tag: &str, node: NodeId) {
    let ed = cx.ed;
    let decomp = ed.backend.decomp();
    let cname = ed.command_for(node);
    let c = cname.as_ref().and_then(|n| decomp.get("commands")?.get(n));
    let sw = decomp.get("subSwitches").and_then(|s| s.get(tag));
    if c.is_none() && sw.is_none() {
        return;
    }
    let st = |v: &serde_json::Value, k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    let th = style::theme(ui.ctx());
    style::boxed(th, th.deep).inner_margin(egui::Margin::symmetric(6, 4)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 2.0;
        if let Some(c) = c {
            let token = st(c, "token");
            let ksy = st(c, "ksy");
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label("decomp:");
                style::tok(ui, if token.is_empty() { "(unnamed)" } else { &token });
                if cname.as_deref() != Some(tag) {
                    ui.label(format!(" · via <{}>", cname.clone().unwrap_or_default()));
                }
                let lower = ksy.to_lowercase();
                let placeholder = lower.contains("todo") || lower.match_indices("0x").any(|(i, _)| lower[i + 2..].starts_with(|ch: char| ch.is_ascii_hexdigit()));
                if placeholder && !token.is_empty() {
                    ui.label(" · names libgarib’s placeholder");
                    ui.label(RichText::new(&ksy).monospace());
                }
            });
            let branch = decomp.get("source").and_then(|x| x.get("gloveworld")).and_then(|x| x.as_str()).filter(|x| !x.is_empty()).unwrap_or("main");
            for (hi, h) in c.get("handlers").and_then(|h| h.as_array()).into_iter().flatten().take(2).enumerate() {
                let (file, func, snippet) = (st(h, "file"), st(h, "func"), st(h, "snippet"));
                let line = h.get("line").map(|l| l.as_i64().map(|n| n.to_string()).unwrap_or_else(|| l.as_str().unwrap_or("").to_string())).unwrap_or_default();
                let id = ui.scope_id().with(("snippet", hi));
                let mut open = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if mini(ui, if open { "▾" } else { "▸" }, "").clicked() {
                        open = !open;
                        ui.data_mut(|d| d.insert_temp(id, open));
                    }
                    ui.hyperlink_to(format!("{file}:{line}"), format!("{GITHUB}{branch}/{file}#L{line}"));
                    ui.label(format!("in {func}()"));
                });
                if open {
                    egui::ScrollArea::both().id_salt(id).max_height(220.0).show(ui, |ui| {
                        ui.add(egui::Label::new(RichText::new(snippet).monospace().size(11.0)).wrap_mode(egui::TextWrapMode::Extend));
                    });
                }
            }
        }
        if let Some(sw) = sw {
            let field = st(sw, "field");
            if let Some(raw) = ed.doc.get(node, &field).and_then(|v| v.trim().parse::<i64>().ok()) {
                let en = st(sw, "enum");
                let offset = sw.get("offset").and_then(|o| o.as_i64()).unwrap_or(0);
                if let Some(name) = ed.decomp_enum_name(&en, Some(raw - offset)) {
                    style::tok(ui, &format!("{field} {raw} → {en}::{name}"));
                }
            }
        }
        if tag == "Enemy" {
            let v = ed.enum_value("Enemy", "type", ed.doc.get(node, "type"));
            if let Some(n) = ed.decomp_enum_name("ActorType", v) {
                style::tok(ui, &format!("type {} → ActorType::{n}", v.map(|x| x.to_string()).unwrap_or_else(|| "None".to_string())));
            }
        }
        if tag == "Powerup" {
            let t = inum(&ed.doc, Some(node), "type", 0);
            if let Some(n) = ed.decomp_enum_name("Pickup", Some(t)) {
                style::tok(ui, &format!("type {t} → Pickup::{n}"));
            }
        }
        if tag == "EnemyInstruction" {
            let v = ed.enum_value("EnemyInstruction", "execution_condition", ed.doc.get(node, "execution_condition"));
            if let Some(n) = ed.decomp_enum_name("NmeCondition", v) {
                style::tok(ui, &format!("execution_condition → NmeCondition::{n}"));
            }
        }
    });
}

/// One element of the level as a card: its fields, what the decompilation says, and its children.
fn cmd_card(cx: &mut Cx, ui: &mut egui::Ui, node: NodeId, with_children: bool, extra: &str, buttons: bool) {
    let ed = cx.ed;
    let tag = ed.doc.tag(node).to_string();
    let c = ed.type_for(&tag);
    let op = c.as_ref().and_then(|c| c.get("opcode")).and_then(|o| o.as_i64()).map(|o| format!("0x{o:02x}"));
    let path = ed.doc.path_of(node);
    let bits = ed.doc.parent(node).is_some_and(|p| ed.doc.tag(p) == "PuzzleAction");
    let mut head_acts: Vec<Act> = Vec::new();
    ui.push_id(("card", &path), |ui| {
        let st = std::cell::RefCell::new(&mut *cx.st);
        let mut body_acts: Vec<Act> = Vec::new();
        card(
            ui,
            &format!("{tag}{extra}"),
            op.as_deref(),
            |ui| {
                if buttons {
                    let mut st = st.borrow_mut();
                    let mut head_cx = Cx { ed, st: &mut st, acts: Vec::new() };
                    command_buttons(&mut head_cx, ui, node);
                    head_acts.append(&mut head_cx.acts);
                }
            },
            |ui| {
                let mut st = st.borrow_mut();
                let mut cx = Cx { ed, st: &mut st, acts: Vec::new() };
                let attrs = ed.doc.attrs(node).to_vec();
                for (name, value) in &attrs {
                    let fd = c.as_ref().and_then(|c| c.get("fields")).and_then(|f| f.as_array()).and_then(|f| f.iter().find(|x| x.get("id").and_then(|i| i.as_str()) == Some(name)));
                    let mut ty = fd.and_then(|f| f.get("type")).and_then(|t| t.as_str()).map(str::to_string);
                    let is_bits = bits && puzzles::FLOAT_BITS.contains(&name.as_str());
                    if is_bits {
                        ty = Some("f4·bits".to_string());
                    }
                    let label = match &ty {
                        Some(t) => format!("{name}  {t}"),
                        None => name.clone(),
                    };
                    let id = egui::Id::unique(("field", &path, name));
                    row(ui, &label, |ui| {
                        if let Some(en) = ed.ksy_enum_for(&tag, name) {
                            let mut items: Vec<(String, String)> = en.values().filter_map(|v| v.as_str()).map(|v| (v.to_string(), v.to_string())).collect();
                            if !items.iter().any(|x| x.0 == *value) {
                                items.push((value.clone(), value.clone()));
                            }
                            if let Some(v) = combo(ui, id, &label, value, &items) {
                                let name = name.clone();
                                cx.ed_act(move |ed| ed.set_attr(node, &name, &v));
                            }
                        } else if is_bits {
                            // A float the game reads from a field libgarib names u4: show the number, write its bits
                            let shown = pyfmt::fmt(float_from_bits(Some(value)));
                            let spec = if name.contains("scale") { FINE } else { Spec::for_text(&shown) };
                            let (text, r) = style::commit_number(ui, id, &shown, Some(spec), |t| t.desired_width(f32::INFINITY));
                            style::name(&r, &label);
                            if let Some(text) = text.filter(|t| t.trim() != shown) {
                                let (name, value) = (name.clone(), value.clone());
                                cx.ed_act(move |ed| match pyfmt::parse_float(text.trim()) {
                                    Some(v) if float_bits(v) != value => ed.set_attr(node, &name, &float_bits(v)),
                                    Some(_) => {}
                                    None => ed.set_status(&format!("{name} expects a number"), "err"),
                                });
                            }
                        } else {
                            // A drag changes whole numbers and floats, inside the limits of the field's type. Names,
                            // free text and ids written in hex are typed only.
                            let plain = looks_like_number(value) && !value.to_ascii_lowercase().contains("0x");
                            let spec = ty.as_deref().filter(|_| plain).and_then(|t| Spec::for_type(t, name, value));
                            let (text, r) = style::commit_number(ui, id, value, spec, |t| t.desired_width(f32::INFINITY));
                            style::name(&r, &label);
                            if let Some(text) = text {
                                let v = text.trim().to_string();
                                let numeric = ty.as_deref().is_some_and(|t| t.len() > 1 && "fsu".contains(&t[..1]) && t[1..].bytes().all(|b| b.is_ascii_digit()));
                                let float = ty.as_deref().is_some_and(|t| t.starts_with('f'));
                                let (name, ty) = (name.clone(), ty.clone().unwrap_or_default());
                                if v != *value {
                                    cx.ed_act(move |ed| {
                                        if numeric && !looks_like_number(&v) {
                                            return ed.set_status(&format!("{name} expects a number ({ty})"), "err");
                                        }
                                        let val = match pyfmt::parse_float(&v) {
                                            Some(x) if float => pyfmt::fmt(x),
                                            _ => v,
                                        };
                                        ed.set_attr(node, &name, &val);
                                    });
                                }
                            }
                        }
                    });
                    if name == "object_id" {
                        let text = match hex_id(Some(value)) {
                            Some(oid) if ed.loaded.contains(&oid) => "model loaded from this level’s object banks".to_string(),
                            Some(oid) if ed.catalog.contains_key(&oid) => format!("imported from {} at ROM build", ed.catalog[&oid].bank),
                            _ => "⚠ not in any object bank".to_string(),
                        };
                        note(ui, &text);
                    }
                }
                if ["r", "g", "b"].iter().all(|k| ed.doc.has(node, k)) {
                    colour_button(&mut cx, ui, node);
                }
                decomp_block(&mut cx, ui, &tag, node);
                if with_children && ed.doc.len(node) > 0 {
                    egui::Frame::new().inner_margin(egui::Margin { left: 10, right: 0, top: 2, bottom: 0 }).show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 6.0;
                        for k in ed.doc.children(node).to_vec() {
                            cmd_card(&mut cx, ui, k, true, "", true);
                        }
                    });
                }
                body_acts.append(&mut cx.acts);
            },
        );
        head_acts.append(&mut body_acts);
    });
    cx.acts.extend(head_acts);
}

// ---------------------------------------------------------------------------------------------------------------
// Several objects

fn multi_panel(cx: &mut Cx, ui: &mut egui::Ui) {
    let ed = cx.ed;
    let sel = &ed.selection;
    let tset = ed.transform_set();
    let mut by: Vec<(&str, usize)> = Vec::new();
    for &i in sel {
        let k = ed.entities[i].kind;
        match by.iter_mut().find(|x| x.0 == k) {
            Some(x) => x.1 += 1,
            None => by.push((k, 1)),
        }
    }
    let rot = tset.iter().filter(|&&i| ed.entities[i].rot.is_some()).count();
    let scl = tset.iter().filter(|&&i| ed.entities[i].scl.is_some()).count();
    h2(ui, &format!("{} objects selected", sel.len()));
    muted(ui, &by.iter().map(|(k, n)| format!("{n} {}", kind_name(k).to_lowercase())).collect::<Vec<_>>().join(" · "));
    card(ui, "Transform selection", None, |_| {}, |ui| {
        let fl = |s: &str, d: f64| pyfmt::parse_float(s.trim()).unwrap_or(d);
        let st = &mut *cx.st;
        let mut go: Option<Act> = None;
        row(ui, "move by", |ui| {
            for (k, name) in ["move by x", "move by y", "move by z"].iter().enumerate() {
                let (r, _) = numfield::bound(ui, egui::Id::unique(("move by", k)), &mut st.mv[k], UNIT, |t| t.desired_width(40.0));
                style::name(&r, name);
            }
            let r = ui.button("Apply");
            r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Apply move"));
            if r.clicked() {
                let v = [fl(&st.mv[0], 0.0), fl(&st.mv[1], 0.0), fl(&st.mv[2], 0.0)];
                go = Some(Box::new(move |w: &mut Window| w.ed.apply_transform(Some(v), None, 1, None)));
            }
        });
        row(ui, "rotate by °", |ui| {
            let items = [(0usize, "y".to_string()), (1, "x".to_string()), (2, "z".to_string())];
            if let Some(a) = combo(ui, "axis", "axis", &st.axis, &items) {
                st.axis = a;
            }
            let (r, _) = numfield::bound(ui, egui::Id::unique("rotate by"), &mut st.deg, UNIT, |t| t.desired_width(40.0));
            style::name(&r, "rotate by");
            let r = ui.button("Apply");
            r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Apply rotation"));
            if r.clicked() {
                let (deg, axis) = (fl(&st.deg, 0.0), [1usize, 0, 2][st.axis]);
                go = Some(Box::new(move |w: &mut Window| w.ed.apply_transform(None, Some(deg), axis, None)));
            }
        });
        row(ui, "scale by", |ui| {
            let (r, _) = numfield::bound(ui, egui::Id::unique("scale by"), &mut st.sc, FINE, |t| t.desired_width(40.0));
            style::name(&r, "scale by");
            let r = ui.button("Apply");
            r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Apply scale"));
            if r.clicked() {
                let s = fl(&st.sc, 1.0);
                go = Some(Box::new(move |w: &mut Window| w.ed.apply_transform(None, None, 1, Some(s))));
            }
        });
        cx.acts.extend(go);
        let pivot = if ed.pivot_mode == "group" { "group centre (positions rotate and scale around it)" } else { "each object’s own origin" };
        note(ui, &format!("Pivot: {pivot}. {rot} of {} can store a rotation and {scl} a size; the rest only move.", tset.len()));
    });
    for (n, &i) in sel.iter().take(200).enumerate() {
        let e = &ed.entities[i];
        ui.push_id(("selected", n), |ui| {
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if mini(ui, "×", "Remove from selection").clicked() {
                        cx.ed_act(move |ed| ed.toggle_select(Some(i)));
                    }
                    muted(ui, &e.tag);
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.add(egui::Label::new(&e.label).truncate());
                    });
                });
            });
        });
    }
}
