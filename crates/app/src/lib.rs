//! Glovebox's window: the egui panels and the wgpu view. See docs/technical.md.
//!
//! `shell` is what's around the window (the user folder, the options, the self-test). `app` is the application:
//! the setup screen (`setup`) or the editor's window, whose parts are `toolbar`, `viewport`, `outliner`,
//! `inspector`, `checks`, `palette`, `path_panel` and `dialogs`. `tour` is the welcome and the tour of the window.
//! `jobs` runs slow work on threads.

pub mod app;
pub mod checks;
pub mod dialogs;
pub mod icon;
pub mod inspector;
pub mod jobs;
pub mod layout;
pub mod numfield;
pub mod outliner;
pub mod palette;
pub mod path_panel;
pub mod render;
pub mod settings;
pub mod setup;
pub mod shell;
pub mod style;
pub mod theme;
pub mod toolbar;
pub mod tour;
pub mod viewport;

use std::sync::Arc;

/// Runs the app with the options of the command line. Returns the exit code.
pub fn run(args: shell::Args) -> i32 {
    if args.help {
        println!("{}", shell::HELP);
        return 0;
    }
    if args.version {
        println!("Glovebox {}", shell::VERSION);
        return 0;
    }
    let fixture_home = args.fixture && args.home.is_none();
    let home = match &args.home {
        Some(h) => std::path::absolute(h).unwrap_or_else(|_| h.clone()),
        None if args.fixture => std::env::temp_dir().join(format!("glovebox-fixture-{}", std::process::id())),
        None => shell::default_home(),
    };
    if args.self_test {
        return shell::self_test(&home);
    }
    if let Err(e) = shell::claim_home(&home) {
        shell::message(&e);
        return 1;
    }
    let Some(_lock) = shell::lock_home(&home) else {
        // Before the log: a second copy must not empty the running copy's editor.log
        let more = if args.reset { " Close it first, then start the app with --reset again." } else { "" };
        shell::message(&format!("The editor is already running with this user folder.{more}"));
        return i32::from(args.reset);
    };
    shell::log_to(&home.join("editor.log"));
    shell::install_panic_hook();
    shell::log(&format!("Glovebox {}", shell::VERSION));
    shell::log(&format!("User folder: {}", home.display()));
    if args.reset {
        shell::reset(&home);
    }

    let backend: Option<Arc<dyn glovebox_core::backend::Backend>> = args.fixture.then(|| Arc::new(glovebox_editor::fixture::FixtureBackend::new()) as _);
    let (w, h) = (256, 256);
    let icon = egui::IconData { rgba: icon::picture(w), width: w, height: h };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("Glovebox").with_inner_size([1500.0, 900.0]).with_min_inner_size([960.0, 600.0]).with_icon(icon).with_drag_and_drop(true),
        wgpu_options: shell::wgpu_options(),
        ..Default::default()
    };
    let start_home = home.clone();
    let result = eframe::run_native(
        "Glovebox",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, app::Start { home: start_home, backend, dialogs: Box::new(dialogs::Native) })))),
    );
    if fixture_home {
        let _ = std::fs::remove_dir_all(&home);
    }
    match result {
        Ok(()) => 0,
        Err(e) => {
            shell::message(&format!("The editor could not start its window: {e}\n\n{}", shell::GRAPHICS_HELP));
            1
        }
    }
}
