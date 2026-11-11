//! The two rows of the toolbar: the level list, undo and redo, the tools, and the files and builds.

use crate::app::Window;
use crate::style;
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
    let frame = egui::Frame::new().fill(style::BG).inner_margin(egui::Margin::symmetric(6, 4));
    egui::Panel::top("toolbar").frame(frame).show(ui, |ui| {
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
        ui.horizontal_wrapped(|ui| edit_row(w, ui));
        ui.horizontal_wrapped(|ui| files_row(w, ui));
    });
}

fn edit_row(w: &mut Window, ui: &mut egui::Ui) {
    let mut brand = egui::text::LayoutJob::default();
    let font = egui::FontId::proportional(14.0);
    brand.append("Glove", 0.0, egui::TextFormat { font_id: font.clone(), color: style::GOLD, ..Default::default() });
    brand.append("box", 0.0, egui::TextFormat { font_id: font, color: style::TEXT, ..Default::default() });
    ui.label(brand);
    ui.add_space(6.0);

    // The level list
    let current = w.ed.scene.as_ref().map(|x| w.scene_label(x).trim_start().to_string()).unwrap_or_else(|| "Loading level list…".to_string());
    let mut pick = None;
    let combo = egui::ComboBox::from_id_salt("level").selected_text(current).width(250.0).height(520.0).show_ui(ui, |ui| {
        let mut group = "";
        for sc in &w.ed.scenes {
            if s(sc, "group") != group {
                group = s(sc, "group");
                ui.add_enabled(false, egui::Label::new(RichText::new(group).color(style::MUTED)));
            }
            let on = s(sc, "id") == w.ed.scene_id();
            if ui.selectable_label(on, w.scene_label(sc)).clicked() {
                pick = Some(s(sc, "id").to_string());
            }
        }
    });
    combo.response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, "Level"));
    combo.response.on_hover_text("Level (from noclip.website's scene table)");
    if let Some(sid) = pick {
        w.load_scene(&sid);
    }
    sep(ui);

    let edited = w.ed.has_level && w.ed.is_edited();
    if ui.add_enabled(edited, egui::Button::new("Reset level")).on_hover_text("Discard all edits to this level and its autosave (undoable)").clicked() {
        w.reset_level();
    }
    if ui.add_enabled(!w.ed.undo_stack.is_empty(), egui::Button::new("Undo")).on_hover_text("Undo (Ctrl+Z)").clicked() {
        w.ed.undo();
    }
    if ui.add_enabled(!w.ed.redo_stack.is_empty(), egui::Button::new("Redo")).on_hover_text("Redo (Ctrl+Shift+Z)").clicked() {
        w.ed.redo();
    }
    sep(ui);
    let add = if w.palette_open { style::toggle(ui, true, "+ Add", "") } else { ui.add(style::accent("+ Add")) };
    if add.on_hover_text("Add objects, enemies, collectables and models from any level (A)").clicked() {
        w.toggle_palette(None);
    }
    for (m, text, key) in [(Mode::Translate, "Move", "W"), (Mode::Rotate, "Rotate", "E"), (Mode::Scale, "Scale", "R")] {
        if style::toggle(ui, w.ed.mode == m, text, &format!("{text} ({key})")).clicked() {
            w.ed.set_mode(m);
        }
    }
    ui.label(RichText::new(" Pivot").color(style::MUTED));
    let pivots = [("group", "group centre"), ("individual", "each origin")];
    let shown = pivots.iter().find(|p| p.0 == w.ed.pivot_mode).map_or("each origin", |p| p.1);
    let r = egui::ComboBox::from_id_salt("pivot").selected_text(shown).show_ui(ui, |ui| {
        for (v, text) in pivots {
            if ui.selectable_label(w.ed.pivot_mode == v, text).clicked() {
                w.ed.pivot_mode = v.to_string();
            }
        }
    });
    r.response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, "Pivot"));
    r.response.on_hover_text("Where a multi-object rotation or scale is centred");
    ui.label(RichText::new(" Snap").color(style::MUTED));
    let snaps = [("off", 0.0), ("1", 1.0), ("10", 10.0), ("50", 50.0)];
    let shown = snaps.iter().find(|x| x.1 == w.ed.snap).map_or("off", |x| x.0);
    let r = egui::ComboBox::from_id_salt("snap").selected_text(shown).width(48.0).show_ui(ui, |ui| {
        for (text, v) in snaps {
            if ui.selectable_label(w.ed.snap == v, text).clicked() {
                w.ed.snap = v;
                w.ed.update_pivot();
            }
        }
    });
    r.response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, "Snap"));
    r.response.on_hover_text("Snap: grid for moves, 15° for rotation, 0.05 for scale");
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

fn files_row(w: &mut Window, ui: &mut egui::Ui) {
    if style::toggle(ui, w.ed.box_mode, "Box select", "Box select (B). Ctrl+click toggles, Ctrl+A selects all").clicked() {
        w.ed.set_box_mode(!w.ed.box_mode);
    }
    if style::toggle(ui, w.view.fly.on, "Fly", "Fly camera (C): WASD moves, Space and Shift go up and down, drag to look, the wheel sets the speed").clicked() {
        w.set_fly(!w.view.fly.on);
    }
    if style::toggle(ui, w.ed.path.on, "Path", "Path (P): click the ground to lay a path, then place garibs or copies of the selection along it").clicked() {
        w.set_path_mode(!w.ed.path.on);
    }
    if style::toggle(ui, w.ed.preview, "Motion", "Animate spinning platforms").clicked() {
        let now = w.now();
        w.ed.set_preview(!w.ed.preview, now);
        w.view.dirty = true;
    }
    if style::toggle(ui, w.ed.show_fog, "Fog", "Show the level's fog in the editor view. Turning it off only affects the editor; to change the fog in the game, edit the level's FogConfiguration (fog_enabled).").clicked() {
        w.toggle_fog(!w.ed.show_fog);
    }
    let triggers = !w.ed.hidden.contains("trigger");
    if style::toggle(ui, triggers, "Triggers", "Show the puzzle trigger volumes (the areas a puzzle condition tests). Hiding them only affects the editor view; the level keeps them.").clicked() {
        w.ed.set_layer("trigger", !triggers);
    }
    let exits = !w.ed.hidden.contains(exits::LAYER);
    if style::toggle(ui, exits, "Exits", "Show the exit triggers: the sphere round each platform with an exit, at the size the game tests. Glover or the ball must come inside it. Hiding them only affects the editor view.").clicked() {
        w.ed.set_layer(exits::LAYER, !exits);
    }
    sep(ui);
    let ready = w.ed.has_level && !w.busy;
    ui.add_enabled_ui(ready, |ui| {
        if ui.button("Import XML").on_hover_text("Import a libgarib level XML (checked here first; dropping a file on the window also works)").clicked() {
            crate::dialogs::open_import_dialog(w, None);
        }
        if ui.button("Export XML").on_hover_text("Save the level as libgarib XML").clicked() {
            w.export("xml", None);
        }
        if ui.button(".lev").on_hover_text("Assemble with libgarib level-tool and save the .lev").clicked() {
            w.export("lev", None);
        }
        ui.checkbox(&mut w.quickboot, "boot here").on_hover_text("Build ROM: the ROM boots straight into this level (libgarib's copyskip and quickboot patches)");
        if ui.button("Export patch").on_hover_text("Export a BPS patch: share it, and players apply it to their own ROM with the patcher").clicked() {
            crate::dialogs::open_patch_dialog(w);
        }
        if ui.button("Test level").on_hover_text("Build the level into a ROM and open it in your emulator (T)").clicked() {
            w.test_level();
        }
        if ui.add(style::primary("Build ROM")).on_hover_text("Patch the edited level (and any imported models) into the ROM with libgarib").clicked() {
            w.export("rom", None);
        }
        if ui.button("Build all…").on_hover_text("Build every edited level into one ROM or patch").clicked() {
            crate::dialogs::open_all_dialog(w);
        }
        if ui.button("Patcher ↗").on_hover_text("Open the patcher page for players (works offline; the ROM never leaves the page)").clicked() {
            w.open_patcher();
        }
    });
}
