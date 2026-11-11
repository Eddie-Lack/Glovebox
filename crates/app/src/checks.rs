//! The Checks panel: the checks' lines, the imports a ROM build makes and the memory they take.

use crate::app::Window;
use crate::style;
use egui::RichText;
use glovebox_editor::editor::Click;

fn kb(n: f64) -> String {
    format!("{:.0}", n / 1024.0)
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

pub fn ui(w: &mut Window, ui: &mut egui::Ui) {
    if !w.ed.has_level {
        return;
    }
    egui::ScrollArea::vertical().id_salt("checks").auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        style::h4(ui, "CHECKS");
        let mut click: Option<Click> = None;
        for c in &w.checks {
            if style::check_line(ui, c.level, &c.text, c.click.is_some(), c.link.as_ref(), c.sub.as_deref(), c.title.as_deref()) {
                click = c.click.clone();
            }
        }
        if let Some(c) = click {
            w.acts.push(Box::new(move |w: &mut Window| w.ed.click_check(&c)));
        }
        imports(w, ui);
    });
}

/// A bar that fills with the share of the free memory the imports take.
fn budget(ui: &mut egui::Ui, text: &str, ratio: f64, small: &str, over: Option<bool>) {
    let over = over.unwrap_or(ratio > 1.0);
    egui::Frame::new().fill(style::PANEL).corner_radius(6).inner_margin(egui::Margin::symmetric(8, 6)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(text);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 6.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 3.0, style::INPUT);
        let mut fill = rect;
        fill.set_width(rect.width() * ratio.clamp(0.0, 1.0) as f32);
        ui.painter().rect_filled(fill, 3.0, if over { style::ERR } else { style::GOLD });
        style::src(ui, small);
    });
}

fn imports(w: &mut Window, ui: &mut egui::Ui) {
    let ed = &w.ed;
    if ed.scene.is_none() {
        return;
    }
    let ids = ed.current_imports();
    let heap_free = ed.backend.heap().get(ed.scene_id()).and_then(|h| h.get("free")).and_then(|f| f.as_f64());
    if ids.is_empty() {
        if let Some(free) = heap_free {
            ui.label(RichText::new(format!("Free heap after this level loads: {} KB (measured in the emulator).", kb(free))).color(style::MUTED));
        }
        return;
    }
    style::h4(ui, "IMPORTS AT ROM BUILD");
    for i in &ids {
        let Some(c) = ed.catalog.get(i) else { continue };
        if style::check_line(ui, "imp", &format!("{} ({}) from {}, {:.1} KB", c.name, i, c.bank, c.bytes as f64 / 1024.0), true, None, None, None) {
            let i = i.clone();
            w.acts.push(Box::new(move |w: &mut Window| {
                let sel = (0..w.ed.entities.len()).filter(|&k| w.ed.entities[k].object_id.as_deref() == Some(&i) && !w.ed.entities[k].is_handle).collect();
                w.ed.set_selection(sel);
            }));
        }
    }
    if ids.iter().any(|i| ed.catalog.get(i).is_none_or(|c| c.custom.is_none())) {
        let mut on = ed.copy_imports;
        let tip = "Works without the Expansion Pak, but patches then contain game data";
        if ui.checkbox(&mut on, "Copy the models into the ROM instead").on_hover_text(tip).changed() {
            w.acts.push(Box::new(move |w: &mut Window| {
                w.ed.copy_imports = on;
                w.checks = w.ed.checks();
            }));
        }
        style::note(ui, tip);
    }
    let Some(plan) = &ed.plan else { return };
    if let (true, Some(rt)) = (ed.runtime_imports(), &plan.runtime) {
        let copied = plan.copy.as_ref().filter(|c| !c.objects.is_empty()).map_or(0, |c| c.object_bytes + c.tex_bytes);
        let cost = rt.bytes + copied;
        if !rt.object_banks.is_empty() || !rt.texture_banks.is_empty() {
            let (no, nt) = (rt.object_banks.len(), rt.texture_banks.len());
            let text = format!(
                "Build ROM and Export patch add the Expansion Pak game mod. When this level starts, it loads {no} model bank{} and {nt} texture bank{} ({} KB) from the player’s own ROM. The ROM and patch contain no game data from those banks. The ROM then needs the Expansion Pak.",
                plural(no),
                plural(nt),
                kb(rt.bytes as f64)
            );
            style::check_line(ui, "imp", &text, false, None, None, None);
        }
        if let (true, Some(copy)) = (copied > 0, &plan.copy) {
            let (n, t) = (copy.objects.len(), copy.textures.len());
            let text = format!(
                "{n} piece{} of your imported models ({} KB with {t} texture{}) are copied into {} and {}. The ROM and patch contain them. The game mod gives them the Expansion Pak’s memory.",
                plural(n),
                kb(copied as f64),
                plural(t),
                copy.target_bank,
                copy.target_tex_bank
            );
            style::check_line(ui, "imp", &text, false, None, None, None);
        }
        if let Some(free) = heap_free {
            let room = (free - 16.0 * 1024.0).max(0.0) + ed.mod_info.heap_extra_bytes as f64;
            budget(
                ui,
                &format!("{} KB of {} KB free with the Expansion Pak", kb(cost as f64), kb(room)),
                cost as f64 / room.max(1.0),
                "Only this level loads these banks. Free heap is measured in the emulator, minus 16 KB for allocations during play.",
                None,
            );
        }
        return;
    }
    let n = plan.textures.len();
    let text = format!("{n} texture{} ({:.1} KB) appended to {}.", plural(n), plan.tex_bytes.unwrap_or(0) as f64 / 1024.0, plan.target_tex_bank);
    style::check_line(ui, "imp", &text, false, None, None, None);
    let Some(b) = ed.import_budget() else { return };
    let Some(t) = b.rows.first() else { return };
    let names = |rows: &[glovebox_editor::editor::BudgetRow]| rows.iter().map(|r| r.name.clone()).collect::<Vec<_>>().join(", ");
    let pak = if b.over.is_empty() {
        String::new()
    } else if !b.over_with_pak.is_empty() {
        format!("Over budget in {} even with the Expansion Pak's memory. ", names(&b.over_with_pak))
    } else {
        format!(
            "Over the game's own heap in {}. Build ROM and Export patch add the Expansion Pak game mod: its heap is {} KB larger, leaving at least {} KB free ({}). The ROM then needs the Expansion Pak. ",
            names(&b.over),
            kb(b.extra as f64),
            kb(b.least_with_pak.2 as f64),
            b.least_with_pak.1
        )
    };
    let n = b.rows.len();
    let mut head = format!("Tightest: {}, {:.1} KB of {} KB free", t.name, t.cost as f64 / 1024.0, kb(t.room as f64));
    if !b.over.is_empty() && b.over_with_pak.is_empty() {
        head.push_str(&format!(" ({} KB with the Expansion Pak)", kb((t.room + b.extra) as f64)));
    }
    let small = format!(
        "{n} level{} load {} or {} and grow by this much. {pak}Free heap is measured per level in the emulator, minus 16 KB for allocations during play (measured runs used 2–3 KB).",
        plural(n),
        plan.target_bank,
        plan.target_tex_bank
    );
    budget(ui, &head, t.ratio, &small, Some(!b.over_with_pak.is_empty()));
}
