//! The first-run screen: the player chooses their ROM. The app checks it and keeps a copy in the user folder. Then
//! the editor opens in the same window. A port of `desktop/setup.py`. Nothing has to be prepared any more: opening
//! the game data takes milliseconds.

use crate::dialogs::FileDialogs;
use crate::{shell, style};
use std::path::{Path, PathBuf};

pub struct Setup {
    home: PathBuf,
    dialogs: Option<Box<dyn FileDialogs>>,
    /// What went wrong with the last file. Empty for nothing.
    pub error: String,
}

impl Setup {
    pub fn new(home: &Path, dialogs: Box<dyn FileDialogs>, error: Option<String>) -> Setup {
        Setup { home: home.to_path_buf(), dialogs: Some(dialogs), error: error.unwrap_or_default() }
    }

    /// The file dialogs, for the editor's window that follows.
    pub fn take_dialogs(&mut self) -> Box<dyn FileDialogs> {
        self.dialogs.take().unwrap_or_else(|| Box::new(crate::dialogs::Native))
    }

    /// Checks a file the player chose and keeps the ROM. True when the ROM is in the user folder.
    pub fn take(&mut self, path: &Path) -> bool {
        let res = shell::read_rom_file(path).and_then(|data| shell::take_rom(&self.home, &data));
        match res {
            Ok(()) => {
                shell::log("The ROM was checked and copied into the user folder.");
                self.error.clear();
                true
            }
            Err(e) => {
                shell::log(&e);
                self.error = e;
                false
            }
        }
    }

    /// Draws the screen. True when the ROM is in place and the editor can open.
    pub fn ui(&mut self, ui: &mut egui::Ui) -> bool {
        let mut ready = false;
        egui::CentralPanel::default().show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.set_max_width(640.0);
                ui.add_space((ui.available_height() * 0.18).max(20.0));
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.spacing_mut().item_spacing.y = 10.0;
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.label(egui::RichText::new("Glove").size(22.0).strong().color(style::GOLD));
                        ui.label(egui::RichText::new("box").size(22.0).strong());
                    });
                    ui.label("Choose your own Glover (USA) ROM (.z64, .v64 or .n64).");
                    ui.label(format!("The app checks it and keeps a copy in its user folder:\n{}", self.home.display()));
                    ui.label("Then the editor opens.");
                    if ui.add(style::primary("Choose the ROM…")).clicked() {
                        let filters: &[crate::dialogs::Filter] = &[("Nintendo 64 ROM", &["z64", "v64", "n64"]), ("All files", &[])];
                        let path = self.dialogs.as_mut().and_then(|d| d.open_file("Choose your Glover (USA) ROM", None, filters));
                        if let Some(p) = path {
                            ready = self.take(&p);
                        }
                    }
                    if !self.error.is_empty() {
                        ui.label(egui::RichText::new(&self.error).color(egui::Color32::from_rgb(0xff, 0x7b, 0x72)));
                    }
                });
            });
        });
        let dropped: Option<PathBuf> = ui.ctx().input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).next());
        if let Some(p) = dropped {
            ready = self.take(&p);
        }
        ready
    }
}
