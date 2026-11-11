//! The outliner (the left panel): the filter, the layers and the list of the level's entities in groups.

use crate::app::{Act, Window};
use crate::style;
use egui::RichText;
use glovebox_editor::entities::{EntId, KINDS};
use std::collections::BTreeMap;

#[derive(Default)]
pub struct State {
    /// Which groups are open, by kind. A missing kind is open, except the landscape and the sounds.
    pub open: BTreeMap<&'static str, bool>,
    /// The entities were built again: `bad` is out of date.
    pub dirty: bool,
    /// For each entity, whether a check reports an error on it.
    bad: Vec<bool>,
    last_selection: Vec<EntId>,
}

enum Row {
    Head { kind: &'static str, name: &'static str, count: usize, selected: usize, open: bool },
    Ent(EntId, usize),
}

pub fn ui(w: &mut Window, ui: &mut egui::Ui) {
    let Window { ed, outliner: st, acts, .. } = w;
    let r = ui.add(egui::TextEdit::singleline(&mut ed.filter).hint_text("Filter entities…").desired_width(f32::INFINITY));
    style::name(&r, "Filter entities");
    ui.checkbox(&mut ed.include_land, RichText::new("box select includes landscape").color(style::MUTED));
    if !ed.has_level {
        return;
    }
    if st.dirty || st.bad.len() != ed.entities.len() {
        st.bad = (0..ed.entities.len()).map(|i| ed.issues_for(i).iter().any(|l| l.0 == "err")).collect();
        st.dirty = false;
    }
    let whole = |i: usize| !ed.entities[i].is_handle && !ed.entities[i].derived;

    // Layers
    egui::Grid::new("layers").num_columns(2).spacing([8.0, 2.0]).show(ui, |ui| {
        let mut col = 0;
        for (k, name, color) in KINDS {
            let n = (0..ed.entities.len()).filter(|&i| ed.entities[i].kind == k && whole(i)).count();
            if n == 0 {
                continue;
            }
            let mut on = !ed.hidden.contains(k);
            ui.scope(|ui| {
                let c = style::hex(color);
                ui.visuals_mut().selection.bg_fill = c;
                ui.visuals_mut().widgets.inactive.fg_stroke.color = c;
                ui.visuals_mut().widgets.hovered.fg_stroke.color = c;
                if ui.checkbox(&mut on, RichText::new(format!("{name} {n}")).color(style::TEXT)).changed() {
                    acts.push(Box::new(move |w: &mut Window| w.ed.set_layer(k, on)));
                }
            });
            col += 1;
            if col % 2 == 0 {
                ui.end_row();
            }
        }
    });
    ui.add_space(2.0);

    // The groups and their rows
    let f = ed.filter.to_lowercase();
    let selected: std::collections::HashSet<EntId> = ed.selection.iter().copied().collect();
    let owners: std::collections::HashSet<EntId> = ed.selection.iter().filter_map(|&i| ed.entities[i].owner).collect();
    let sel_changed = st.last_selection != ed.selection;
    if sel_changed {
        st.last_selection = ed.selection.clone();
    }
    let mut rows = Vec::new();
    let mut lists: Vec<Vec<EntId>> = Vec::new();
    for (k, name, _) in KINDS {
        let lst: Vec<EntId> = (0..ed.entities.len())
            .filter(|&i| {
                let e = &ed.entities[i];
                e.kind == k && whole(i) && (f.is_empty() || format!("{} {}", e.label, e.tag).to_lowercase().contains(&f))
            })
            .collect();
        if lst.is_empty() {
            continue;
        }
        let n = lst.iter().filter(|i| selected.contains(i)).count();
        if n > 0 && sel_changed {
            st.open.insert(k, true);
        }
        let open = st.open.get(k).copied().unwrap_or(!matches!(k, "land" | "sound") || !f.is_empty());
        rows.push(Row::Head { kind: k, name, count: lst.len(), selected: n, open });
        if open {
            rows.extend(lst.iter().map(|&i| Row::Ent(i, lists.len())));
        }
        lists.push(lst);
    }
    let mut act: Option<Act> = None;
    let row_height = 20.0;
    ui.spacing_mut().item_spacing.y = 0.0;
    egui::ScrollArea::vertical().id_salt("outliner").auto_shrink([false, false]).show_rows(ui, row_height, rows.len(), |ui, range| {
        let mut list_of_head = 0;
        // The index of the list a header stands for: count the headers before the first shown row
        for r in &rows[..range.start] {
            if matches!(r, Row::Head { .. }) {
                list_of_head += 1;
            }
        }
        for row in &rows[range] {
            match row {
                Row::Head { kind, name, count, selected: n, open } => {
                    let li = list_of_head;
                    list_of_head += 1;
                    ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), row_height), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let all = style::mini(ui, "all", &format!("Select all {} (Ctrl adds to the selection)", name.to_lowercase()));
                        all.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("all {}", name.to_lowercase())));
                        if all.clicked() {
                            let lst = lists[li].clone();
                            let add = ui.input(|i| i.modifiers.ctrl || i.modifiers.command);
                            act = Some(Box::new(move |w: &mut Window| {
                                let sel = if add { w.ed.selection.iter().copied().chain(lst).collect() } else { lst };
                                w.ed.set_selection(sel);
                            }));
                        }
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            let mut text = format!("{} {}  {}", if *open { "▾" } else { "▸" }, name.to_uppercase(), count);
                            if *n > 0 {
                                text.push_str(&format!("  · {n} selected"));
                            }
                            let head = ui.add(egui::Label::new(RichText::new(text).size(12.0).color(style::MUTED)).truncate().sense(egui::Sense::click()));
                            if head.clicked() {
                                st.open.insert(kind, !open);
                            }
                        });
                    });
                }
                Row::Ent(i, li) => {
                    let e = &ed.entities[*i];
                    let on = selected.contains(i) || owners.contains(i);
                    let color = if on {
                        style::GOLD
                    } else if st.bad.get(*i).copied().unwrap_or(false) {
                        style::ERR
                    } else {
                        style::TEXT
                    };
                    let tag = if e.tag == "GaribGroup" { "" } else { e.tag.as_str() };
                    ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), row_height), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.add_space(14.0);
                        let mut text = RichText::new(format!("{}   {}", e.label, tag)).color(color);
                        if on {
                            text = text.strong();
                        }
                        let mut r = ui.add(egui::Label::new(text).truncate().sense(egui::Sense::click()));
                        if r.hovered() {
                            ui.painter().rect_filled(r.rect.expand2(egui::vec2(3.0, 1.0)), 3.0, egui::Color32::from_white_alpha(10));
                        }
                        if e.imported {
                            r = r.on_hover_text("Imported at ROM build");
                        }
                        if r.clicked() {
                            let (i, lst) = (*i, lists[*li].clone());
                            let m = ui.input(|x| x.modifiers);
                            act = Some(Box::new(move |w: &mut Window| {
                                if m.ctrl || m.command {
                                    w.ed.toggle_select(Some(i));
                                } else if m.shift {
                                    w.ed.range_select(&lst, i);
                                } else {
                                    w.ed.select(Some(i), true);
                                }
                            }));
                        }
                    });
                }
            }
        }
    });
    acts.extend(act);
}
