//! The system's file dialogs behind a trait. The app uses `rfd`'s native dialogs. Tests give the paths
//! themselves.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

/// A file filter: a name and its extensions, without dots. No extensions means every file.
pub type Filter<'a> = (&'a str, &'a [&'a str]);

pub trait FileDialogs {
    /// Asks for a file to open. None when the player cancels.
    fn open_file(&mut self, title: &str, folder: Option<&Path>, filters: &[Filter]) -> Option<PathBuf>;
    /// Asks where to save a file. None when the player cancels.
    fn save_file(&mut self, title: &str, folder: &Path, name: &str) -> Option<PathBuf>;
}

/// The system's dialogs.
pub struct Native;

impl FileDialogs for Native {
    fn open_file(&mut self, title: &str, folder: Option<&Path>, filters: &[Filter]) -> Option<PathBuf> {
        let mut d = rfd::FileDialog::new().set_title(title);
        if let Some(f) = folder.filter(|f| f.is_dir()) {
            d = d.set_directory(f);
        }
        for (name, exts) in filters {
            d = if exts.is_empty() { d.add_filter(*name, &["*"]) } else { d.add_filter(*name, exts) };
        }
        d.pick_file()
    }

    fn save_file(&mut self, title: &str, folder: &Path, name: &str) -> Option<PathBuf> {
        let mut d = rfd::FileDialog::new().set_title(title).set_file_name(name);
        if folder.is_dir() {
            d = d.set_directory(folder);
        }
        d.save_file()
    }
}

/// Dialogs for tests: each call takes the next path of its queue, and an empty queue cancels. `asked` records
/// the titles.
#[derive(Default, Clone)]
pub struct Scripted(pub std::rc::Rc<std::cell::RefCell<ScriptedState>>);

#[derive(Default)]
pub struct ScriptedState {
    pub open: VecDeque<PathBuf>,
    pub save: VecDeque<PathBuf>,
    pub asked: Vec<String>,
}

impl FileDialogs for Scripted {
    fn open_file(&mut self, title: &str, _folder: Option<&Path>, _filters: &[Filter]) -> Option<PathBuf> {
        let mut s = self.0.borrow_mut();
        s.asked.push(title.to_string());
        s.open.pop_front()
    }

    fn save_file(&mut self, title: &str, _folder: &Path, name: &str) -> Option<PathBuf> {
        let mut s = self.0.borrow_mut();
        s.asked.push(format!("{title}: {name}"));
        s.save.pop_front()
    }
}
