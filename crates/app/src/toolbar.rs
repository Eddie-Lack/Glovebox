//! The two rows of the toolbar: the level list, undo and redo, the tools, the files and builds, and the Options
//! menu (the theme, the info texts, the panels and the tour).

use crate::app::Window;
use crate::style;
use crate::tour::Target;
use egui::RichText;
use glovebox_editor::exits;
use glovebox_editor::gizmo::Mode;

fn s<'a>(v: &'a serde_json::Value, key: &str) -> &'a str {
    v.get(key).and_then(|x| x.as_str()).unwrap_or("")
}

fn sep(ui: &mut egui::Ui) {
    ui.add(egui::Separator::default().vertical().spacing(6.0));
}

pub fn ui(w: &mut Window, ui: &mut egui::Ui) {
    let frame = egui::Frame::new().fill(style::theme(ui.ctx()).deep).inner_margin(egui::Margin::symmetric(6, 4));
    let bar = egui::Panel::top("toolbar").frame(frame).show(ui, |ui| {
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
        ui.horizontal_wrapped(|ui| edit_row(w, ui));
        ui.horizontal_wrapped(|ui| files_row(w, ui));
    });
    w.tour.mark(Target::Toolbar, bar.response.rect);
}

fn edit_row(w: &mut Window, ui: &mut egui::Ui) {
    style::logo(ui, "Glovebox", 16.0);
    ui.add_space(4.0);

    // The level list
    let current = w.ed.scene.as_ref().map(|x| w.scene_label(x).trim_start().to_string()).unwrap_or_else(|| "Loading level list…".to_string());
    let mut pick = None;
    let combo = egui::ComboBox::from_id_salt("level").icon(style::combo_icon).selected_text(current).width(250.0).height(520.0).show_ui(ui, |ui| {
        let mut group = "";
        for sc in &w.ed.scenes {
            if s(sc, "group") != group {
                group = s(sc, "group");
                ui.label(RichText::new(group).size(11.0).color(style::theme(ui.ctx()).muted));
            }
            let on = s(sc, "id") == w.ed.scene_id();
            if ui.selectable_label(on, w.scene_label(sc)).clicked() {
                pick = Some(s(sc, "id").to_string());
            }
        }
    });
    combo.response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, "Level"));
    w.tour.mark(Target::LevelList, combo.response.rect);
    combo.response.on_hover_text("Pick the level to edit");
    if let Some(sid) = pick {
        w.load_scene(&sid);
    }
    sep(ui);

    let edited = w.ed.has_level && w.ed.is_edited();
    let r = ui.add_enabled(edited, egui::Button::new("Reset level")).on_hover_text("Discard all edits to this level and its autosave. Undo brings them back");
    w.tour.mark(Target::Undo, r.rect);
    if r.clicked() {
        w.reset_level();
    }
    let r = ui.add_enabled(!w.ed.undo_stack.is_empty(), egui::Button::new("Undo")).on_hover_text("Undo (Ctrl+Z)");
    w.tour.mark(Target::Undo, r.rect);
    if r.clicked() {
        w.ed.undo();
    }
    let r = ui.add_enabled(!w.ed.redo_stack.is_empty(), egui::Button::new("Redo")).on_hover_text("Redo (Ctrl+Shift+Z)");
    w.tour.mark(Target::Undo, r.rect);
    if r.clicked() {
        w.ed.redo();
    }
    sep(ui);
    let add = if w.palette_open { style::toggle(ui, true, "+ Add", "") } else { ui.add(style::accent("+ Add")) };
    w.tour.mark(Target::Add, add.rect);
    if add.on_hover_text("Add collectables, enemies and models from any level (A)").clicked() {
        w.toggle_palette(None);
    }
    for (m, text, key) in [(Mode::Translate, "Move", "W"), (Mode::Rotate, "Rotate", "E"), (Mode::Scale, "Scale", "R")] {
        let r = style::toggle(ui, w.ed.mode == m, text, &format!("{text} ({key})"));
        w.tour.mark(Target::Tools, r.rect);
        if r.clicked() {
            w.ed.set_mode(m);
        }
    }
    // Each label stays on the row of its list when the toolbar wraps
    ui.horizontal(|ui| {
        ui.label(RichText::new(" Pivot").color(style::theme(ui.ctx()).muted));
        let pivots = [("group", "group center"), ("individual", "each origin")];
        let shown = pivots.iter().find(|p| p.0 == w.ed.pivot_mode).map_or("each origin", |p| p.1);
        let r = egui::ComboBox::from_id_salt("pivot").icon(style::combo_icon).selected_text(shown).show_ui(ui, |ui| {
            for (v, text) in pivots {
                if ui.selectable_label(w.ed.pivot_mode == v, text).clicked() {
                    w.ed.pivot_mode = v.to_string();
                }
            }
        });
        r.response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, "Pivot"));
        r.response.on_hover_text("Pick the center for rotating or scaling several objects");
    });
    ui.horizontal(|ui| {
        ui.label(RichText::new(" Snap").color(style::theme(ui.ctx()).muted));
        let snaps = [("off", 0.0), ("1", 1.0), ("10", 10.0), ("50", 50.0)];
        let shown = snaps.iter().find(|x| x.1 == w.ed.snap).map_or("off", |x| x.0);
        let r = egui::ComboBox::from_id_salt("snap").icon(style::combo_icon).selected_text(shown).width(48.0).show_ui(ui, |ui| {
            for (text, v) in snaps {
                if ui.selectable_label(w.ed.snap == v, text).clicked() {
                    w.ed.snap = v;
                    w.ed.update_pivot();
                }
            }
        });
        r.response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, "Snap"));
        r.response.on_hover_text("Snap: moves snap to the grid, rotation to 15°, scale to 0.05");
    });
    if ui.button("Duplicate").on_hover_text("Duplicate selection (Ctrl+D)").clicked() {
        w.ed.duplicate();
    }
    if ui.button("Delete").on_hover_text("Delete selection (Del)").clicked() {
        w.ed.remove();
    }
    if ui.button("Focus").on_hover_text("Frame selection (F)").clicked() {
        w.focus_selected();
    }
}

/// The Options menu: the look of the window (Theme), how the notes show (Info texts), where the panels are (Panels)
/// and `Take the tour`. A choice works at once and is kept in the settings.
fn options_menu(w: &mut Window, ui: &mut egui::Ui) {
    let active = style::theme(ui.ctx());
    let info = style::info(ui.ctx());
    let (mut theme, mut notes, mut tour, mut emulator) = (None, None, false, false);
    let (r, _) = egui::containers::menu::MenuButton::new("Options").ui(ui, |ui| {
        ui.set_width(238.0);
        style::h4(ui, "Theme");
        // Two columns: the default, the plain and the styled themes, then the palette themes
        use crate::theme::{PALETTES, THEMES};
        let palette = |th: &crate::theme::Theme| PALETTES.iter().any(|p| p.id == th.id);
        ui.horizontal_top(|ui| {
            for second in [false, true] {
                ui.allocate_ui_with_layout(egui::vec2(116.0, 0.0), egui::Layout::top_down_justified(egui::Align::Min), |ui| {
                    ui.set_width(116.0);
                    for th in THEMES.iter().filter(|th| palette(th) == second) {
                        if ui.selectable_label(th.id == active.id, th.name).clicked() {
                            theme = Some(th.id);
                        }
                    }
                });
            }
        });
        ui.add_space(4.0);
        style::h4(ui, "Info texts").on_hover_text("Info texts explain the game and the editor. They belong to titles, fields and rows");
        for i in style::Info::ALL {
            if ui.selectable_label(i == info, i.name()).clicked() {
                notes = Some(i);
            }
        }
        ui.add_space(4.0);
        ui.separator();
        // The Panels menu stays open while the player arranges the panels
        let stay = egui::containers::menu::MenuConfig::new().close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
        egui::containers::menu::SubMenuButton::new("Panels").config(stay).ui(ui, |ui| crate::layout::menu(w, ui));
        if ui.add_enabled(w.ed.has_level && !w.busy, egui::Button::selectable(false, "Take the tour")).on_hover_text("Take a quick tour of the window").clicked() {
            tour = true;
            ui.close();
        }
        ui.separator();
        if ui.add(egui::Button::selectable(false, "Choose your emulator…")).clicked() {
            emulator = true;
            ui.close();
        }
        if ui.add(egui::Button::selectable(false, "Open the user folder")).on_hover_text(w.home.display().to_string()).clicked() {
            let url = format!("file://{}{}", if cfg!(windows) { "/" } else { "" }, w.home.to_string_lossy().replace('\\', "/"));
            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
            ui.close();
        }
        if ui.add(egui::Button::selectable(false, "Open the guide ↗")).on_hover_text(crate::tour::GUIDE).clicked() {
            ui.ctx().open_url(egui::OpenUrl::new_tab(crate::tour::GUIDE));
            ui.close();
        }
        ui.label(RichText::new(format!("Glovebox {}", crate::shell::VERSION)).color(style::theme(ui.ctx()).muted));
    });
    w.tour.mark(Target::Options, r.rect);
    r.on_hover_text("Change the theme, the info texts and the panels, or take the tour");
    // After the frame: the panels of this frame are drawn in one theme and one state
    if let Some(id) = theme {
        w.acts.push(Box::new(move |w: &mut Window| w.set_theme(id)));
    }
    if let Some(i) = notes {
        w.acts.push(Box::new(move |w: &mut Window| w.set_info(i)));
    }
    if tour {
        w.acts.push(Box::new(|w: &mut Window| w.start_tour()));
    }
    if emulator {
        w.acts.push(Box::new(|w: &mut Window| w.choose_emulator()));
    }
}

fn files_row(w: &mut Window, ui: &mut egui::Ui) {
    if style::toggle(ui, w.ed.box_mode, "Box select", "Drag a box to select (B). Ctrl+click toggles one object, Ctrl+A selects all").clicked() {
        w.ed.set_box_mode(!w.ed.box_mode);
    }
    if style::toggle(ui, w.view.fly.on, "Fly", "Fly the camera (C). WASD moves, Space and Shift go up and down, drag looks around, and the wheel sets the speed").clicked() {
        w.set_fly(!w.view.fly.on);
    }
    if style::toggle(ui, w.ed.path.on, "Path", "Lay a path (P). Click the ground to add points, then place garibs or copies of the selection along it").clicked() {
        w.toggle_path();
    }
    if style::toggle(ui, w.ed.preview, "Motion", "Animate spinning platforms").clicked() {
        let now = w.now();
        w.ed.set_preview(!w.ed.preview, now);
        w.view.dirty = true;
    }
    if style::toggle(ui, w.ed.show_fog, "Fog", "Show the level's fog in the view. This only changes the editor. To change the fog in the game, edit the level's FogConfiguration (fog_enabled)").clicked() {
        w.toggle_fog(!w.ed.show_fog);
    }
    let triggers = !w.ed.hidden.contains("trigger");
    if style::toggle(ui, triggers, "Triggers", "Show the puzzle trigger volumes: the areas a puzzle condition tests. Hiding them only changes the view").clicked() {
        w.ed.set_layer("trigger", !triggers);
    }
    let shown = !w.ed.hidden.contains(exits::LAYER);
    if style::toggle(ui, shown, "Exits", "Show the exit triggers: the sphere around each exit platform, at the size the game tests. Glover or the ball must come inside it. Hiding them only changes the view").clicked() {
        w.ed.set_layer(exits::LAYER, !shown);
    }
    sep(ui);
    let ready = w.ed.has_level && !w.busy;
    ui.add_enabled_ui(ready, |ui| {
        if ui.button("Import XML").on_hover_text("Import a level from a libgarib XML file. The editor checks the file first. You can also drop a file on the window").clicked() {
            crate::dialogs::open_import_dialog(w, None);
        }
        if ui.button("Export XML").on_hover_text("Save the level as libgarib XML").clicked() {
            w.export("xml", None);
        }
        if ui.button(".lev").on_hover_text("Assemble the level and save the .lev").clicked() {
            w.export("lev", None);
        }
        style::checkbox(ui, &mut w.quickboot, "boot here").on_hover_text("Make Build ROM boot straight into this level");
        let r = ui.button("Export patch").on_hover_text("Export a BPS patch to share your level. Players apply it to their own ROM with the patcher");
        w.tour.mark(Target::Patch, r.rect);
        if r.clicked() {
            crate::dialogs::open_patch_dialog(w);
        }
        let r = ui.button("Test level").on_hover_text("Build the level into a ROM and open it in your emulator (T)");
        w.tour.mark(Target::Build, r.rect);
        if r.clicked() {
            w.test_level();
        }
        let r = ui.add(style::primary("Build ROM")).on_hover_text("Build a ROM with the edited level and any imported models");
        w.tour.mark(Target::Build, r.rect);
        if r.clicked() {
            w.export("rom", None);
        }
        if ui.button("Build all…").on_hover_text("Build every edited level into one ROM or patch").clicked() {
            crate::dialogs::open_all_dialog(w);
        }
        if ui.button("Patcher ↗").on_hover_text("Open the patcher page for players. It works offline, and the ROM isn't uploaded").clicked() {
            w.open_patcher();
        }
    });
    options_menu(w, ui);
}
