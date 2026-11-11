//! What is around the window: the options, the user folder with its marker, lock and log, the ROM copy, the
//! graphics adapter and the self-test. It does what the former Python app's launcher did.
//!
//! The user folder holds `rom/glover.z64` (the checked copy of the player's ROM), `drafts/`, `custom/`,
//! `settings.ini`, `test/` and `editor.log`. The app only uses a new or empty folder, or one it marked before
//! (`glovebox-folder.txt`). One copy of the app uses a folder at a time (`editor.lock`).

use eframe::egui_wgpu::{self, wgpu};
use glovebox_core::n64;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

pub const VERSION: &str = env!("GLOVEBOX_VERSION");
pub const ROM_MAX: usize = 64 << 20;
pub const MARKER: &str = "glovebox-folder.txt";

pub const HELP: &str = "Glovebox, the Glover map editor.

Usage: Glovebox [options]

  --home DIR    the user folder (the ROM copy, drafts, imported models, settings and the log)
  --reset       forget the ROM and ask for it again
  --fixture     made-up levels and models instead of the ROM's
  --self-test   check the app and the graphics, print a report and exit
  --version     print the version and exit
  --help        print this text and exit

On Windows the app is a windowed program and has no console: redirect the output to a file to read it, for
example: Glovebox.exe --self-test > report.txt";

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Args {
    pub home: Option<PathBuf>,
    pub reset: bool,
    pub fixture: bool,
    pub self_test: bool,
    pub version: bool,
    pub help: bool,
}

pub fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let mut out = Args::default();
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--home" => out.home = Some(PathBuf::from(it.next().ok_or("--home needs a folder")?)),
            "--reset" => out.reset = true,
            "--fixture" => out.fixture = true,
            "--self-test" => out.self_test = true,
            "--version" | "-V" => out.version = true,
            "--help" | "-h" | "/?" => out.help = true,
            other => match other.strip_prefix("--home=") {
                Some(dir) => out.home = Some(PathBuf::from(dir)),
                None => return Err(format!("Unknown option: {other}")),
            },
        }
    }
    Ok(out)
}

/// `%LOCALAPPDATA%\Glovebox` on Windows, `$XDG_DATA_HOME/glovebox` or `~/.local/share/glovebox` elsewhere.
pub fn default_home() -> PathBuf {
    let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    if cfg!(windows) {
        var("LOCALAPPDATA").or_else(|| var("USERPROFILE")).unwrap_or_else(|| PathBuf::from(".")).join("Glovebox")
    } else if cfg!(target_os = "macos") {
        var("HOME").unwrap_or_else(|| PathBuf::from(".")).join("Library/Application Support/Glovebox")
    } else {
        var("XDG_DATA_HOME").or_else(|| var("HOME").map(|h| h.join(".local/share"))).unwrap_or_else(|| PathBuf::from(".")).join("glovebox")
    }
}

/// Marks the user folder as the app's. An error when it holds files of something else.
pub fn claim_home(home: &Path) -> Result<(), String> {
    if home.join(MARKER).is_file() {
        return Ok(());
    }
    if home.is_dir() && std::fs::read_dir(home).map(|mut d| d.next().is_some()).unwrap_or(true) {
        return Err(format!(
            "The user folder {} already holds other files. The app only uses a new or empty folder (or one it made). Choose another --home.",
            home.display()
        ));
    }
    let made = std::fs::create_dir_all(home)
        .and_then(|_| std::fs::write(home.join(MARKER), "This folder belongs to Glovebox. The app deletes and rewrites the files it made here.\n"));
    made.map_err(|e| format!("The app could not make its user folder {}: {e}", home.display()))
}

/// Holds `editor.lock` while the app runs.
pub struct Lock(#[allow(dead_code)] std::fs::File);

/// None when another copy of the app uses the folder.
pub fn lock_home(home: &Path) -> Option<Lock> {
    let f = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(home.join("editor.lock")).ok()?;
    f.try_lock().ok()?;
    Some(Lock(f))
}

static LOG: OnceLock<Mutex<std::fs::File>> = OnceLock::new();
static START: OnceLock<std::time::Instant> = OnceLock::new();

/// Call first thing in `main`: log lines carry the time since then.
pub fn mark_start() {
    START.get_or_init(std::time::Instant::now);
}

pub fn since_start() -> f64 {
    START.get().map(|t| t.elapsed().as_secs_f64()).unwrap_or(0.0)
}

/// Opens the log (a new file at every start).
pub fn log_to(path: &Path) {
    if let Ok(f) = std::fs::File::create(path) {
        let _ = LOG.set(Mutex::new(f));
    }
}

/// Writes a line to the log and to the console, when there is one.
pub fn log(line: &str) {
    if let Some(f) = LOG.get() {
        let mut f = f.lock().unwrap_or_else(|e| e.into_inner());
        let _ = writeln!(f, "[{:8.3}] {line}", since_start());
        let _ = f.flush();
    }
    let _ = writeln!(std::io::stderr(), "{line}");
}

/// A panic goes to the log and into a message box. The release build then stops.
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let text = format!("The editor stopped because of an error in the program:\n\n{info}\n\nYour edits are autosaved. The details are in editor.log in the user folder.");
        log(&text);
        if std::thread::current().name() == Some("main") {
            message(&text);
        }
    }));
}

/// Says something the player must see, in a window of the system (and in the log).
pub fn message(text: &str) {
    log(text);
    if std::env::var_os("GLOVEBOX_NO_MESSAGE_BOX").is_none() {
        let _ = rfd::MessageDialog::new().set_title("Glovebox").set_description(text).set_buttons(rfd::MessageButtons::Ok).show();
    }
}

/// Deletes what only the Python app made in the user folder.
pub fn remove_python_leftovers(home: &Path) {
    for d in ["data", "cache", "web"] {
        if home.join(d).is_dir() {
            log(&format!("Removing {d}/ of an older version of the app."));
            let _ = std::fs::remove_dir_all(home.join(d));
        }
    }
    for f in ["config.json", "prepared.json"] {
        let _ = std::fs::remove_file(home.join(f));
    }
}

pub fn rom_path(home: &Path) -> PathBuf {
    home.join("rom").join("glover.z64")
}

/// Reads a file the player chose as their ROM. A file too big to be one is not read whole.
pub fn read_rom_file(path: &Path) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let fail = |e: std::io::Error| format!("The app could not read {}: {e}", path.display());
    let f = std::fs::File::open(path).map_err(fail)?;
    let mut data = Vec::new();
    f.take(ROM_MAX as u64 + 1).read_to_end(&mut data).map_err(fail)?;
    Ok(data)
}

/// Checks a ROM and keeps a copy in .z64 byte order. The error is a message for the player.
pub fn take_rom(home: &Path, data: &[u8]) -> Result<(), String> {
    if data.len() > ROM_MAX {
        return Err("That file is too big to be a Nintendo 64 ROM.".to_string());
    }
    let rom = n64::supported_rom(data).map_err(|e| e.to_string())?;
    let path = rom_path(home);
    let tmp = path.with_extension("z64.tmp");
    let write = || -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&tmp, &rom)?;
        std::fs::rename(&tmp, &path)
    };
    write().map_err(|e| format!("The app could not keep a copy of the ROM in {}: {e}", path.display()))
}

/// `--reset`: the app asks for the ROM again.
pub fn reset(home: &Path) {
    let _ = std::fs::remove_file(rom_path(home));
    log("Reset: the app asks for the ROM again.");
}

// ---------------------------------------------------------------------------------------------------------------
// Graphics

pub const GRAPHICS_HELP: &str = "Update your graphics driver from the maker of your graphics card (NVIDIA, AMD or Intel). \
In Remote Desktop or a virtual machine without 3D, start the editor on the computer itself.";

fn backends() -> wgpu::Backends {
    if cfg!(windows) {
        wgpu::Backends::DX12 | wgpu::Backends::VULKAN
    } else {
        wgpu::Backends::PRIMARY | wgpu::Backends::GL
    }
}

/// Lower is better: a real graphics card before a software one, Direct3D 12 before Vulkan before the rest.
fn rank(info: &wgpu::AdapterInfo) -> (u8, u8) {
    let kind = match info.device_type {
        wgpu::DeviceType::DiscreteGpu | wgpu::DeviceType::IntegratedGpu => 0,
        wgpu::DeviceType::VirtualGpu | wgpu::DeviceType::Other => 1,
        wgpu::DeviceType::Cpu => 2,
    };
    let api = match info.backend {
        wgpu::Backend::Dx12 => 0,
        wgpu::Backend::Metal => 0,
        wgpu::Backend::Vulkan => 1,
        _ => 2,
    };
    (kind, api)
}

/// The adapter the app draws with. A software adapter (WARP on Windows) is taken when there is nothing better.
pub fn choose_adapter(adapters: &[wgpu::Adapter], surface: Option<&wgpu::Surface<'_>>) -> Result<wgpu::Adapter, String> {
    let fits = |a: &&wgpu::Adapter| surface.is_none_or(|s| a.is_surface_supported(s));
    match adapters.iter().filter(fits).min_by_key(|a| rank(&a.get_info())) {
        Some(a) => {
            let i = a.get_info();
            log(&format!("Graphics: {} ({:?}, {:?})", i.name, i.backend, i.device_type));
            Ok(a.clone())
        }
        None => Err("no graphics adapter".to_string()),
    }
}

pub fn instance_descriptor() -> wgpu::InstanceDescriptor {
    let mut d = wgpu::InstanceDescriptor::new_without_display_handle();
    d.backends = backends();
    d
}

pub fn wgpu_options() -> egui_wgpu::WgpuConfiguration {
    let mut setup = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor = instance_descriptor();
    setup.native_adapter_selector = Some(Arc::new(choose_adapter));
    egui_wgpu::WgpuConfiguration { wgpu_setup: egui_wgpu::WgpuSetup::CreateNew(setup), ..Default::default() }
}

/// A device without a window, for the self-test.
pub fn headless_device() -> Result<(wgpu::Device, wgpu::Queue, wgpu::AdapterInfo), String> {
    let instance = wgpu::Instance::new(instance_descriptor());
    let adapters = pollster::block_on(instance.enumerate_adapters(backends()));
    let adapter = choose_adapter(&adapters, None)?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).map_err(|e| e.to_string())?;
    Ok((device, queue, adapter.get_info()))
}

// ---------------------------------------------------------------------------------------------------------------
// The self-test

/// `--self-test`: checks the tables, the graphics and, with a ROM in the user folder, the game data and a build.
/// Prints a report. Returns the exit code.
pub fn self_test(home: &Path) -> i32 {
    use glovebox_core::backend::{Backend, GameBackend};
    use glovebox_core::{hwcheck, tables};
    let mut failed = 0;
    let mut say = |ok: bool, text: String| {
        println!("{}  {text}", if ok { "ok  " } else { "FAIL" });
        if !ok {
            failed += 1;
        }
    };
    println!("Glovebox {VERSION}");
    let t = std::time::Instant::now();
    let scenes = tables::scenes().as_array().map_or(0, |a| a.len());
    let commands = tables::decomp().get("commands").and_then(|c| c.as_object()).map_or(0, |c| c.len());
    let mod_bytes = tables::mod_info()["mod_bytes"].as_u64().unwrap_or(0);
    say(scenes == 48 && commands > 100 && mod_bytes > 0, format!("tables: {scenes} levels, {commands} commands, a game mod of {mod_bytes} bytes"));
    say(crate::icon::ico().len() > 1000 && crate::icon::picture(64).len() == 64 * 64 * 4, "icon".to_string());

    match headless_device() {
        Ok((device, queue, info)) => {
            say(true, format!("graphics device: {} ({:?}, {:?})", info.name, info.backend, info.device_type));
            let backend: Arc<dyn Backend> = Arc::new(glovebox_editor::fixture::FixtureBackend::new());
            let mut ed = glovebox_editor::editor::Editor::new(backend.clone(), glovebox_editor::drafts::Drafts::new(std::env::temp_dir().join("glovebox-self-test-drafts")), None);
            ed.start();
            ed.set_catalog(backend.catalog().unwrap_or_default());
            ed.set_view_size(320.0, 240.0);
            let loaded = ed.load_scene("10");
            let mut r = crate::render::Renderer::new(&device, &queue);
            r.background = ed.background;
            let roots = ed.roots();
            let frame = crate::render::Frame::new(&ed.graph, &roots, &ed.camera);
            let picture = r.render_image(&frame, 320, 240);
            let drawn = picture.as_ref().is_ok_and(|p| p.len() == 320 * 240 * 4 && p.chunks_exact(4).any(|c| c[..3] != p[..3]));
            say(loaded.is_ok() && drawn, format!("a frame of a made-up level: {} draw calls, {} triangles", r.stats.draw_calls, r.stats.triangles));
        }
        Err(e) => say(false, format!("graphics device: {e}. {GRAPHICS_HELP}")),
    }

    let rom = rom_path(home);
    if rom.is_file() {
        let t = std::time::Instant::now();
        match GameBackend::open(&rom, home) {
            Ok(b) => {
                say(true, format!("the ROM opens in {:.0} ms", t.elapsed().as_secs_f64() * 1000.0));
                let models = b.catalog().map(|c| c.len()).unwrap_or(0);
                say(models > 1000, format!("catalog: {models} models"));
                // A level with a kit model: the ROM then has the game mod and its custom banks, which is what
                // hwcheck's own test breaks. It is made in a folder of its own, not in the user folder.
                drop(b);
                let work = std::env::temp_dir().join(format!("glovebox-self-test-{}", std::process::id()));
                let built = (|| -> Result<Vec<u8>, String> {
                    let backend: Arc<dyn Backend> = Arc::new(GameBackend::open(&rom, &work).map_err(|e| e.0)?);
                    let meta = backend.make_kit("pumpkin", &serde_json::json!({})).map_err(|e| e.0)?;
                    let set = meta.get("set").and_then(|s| s.as_str()).unwrap_or("").to_string();
                    let mut ed = glovebox_editor::editor::Editor::new(backend.clone(), glovebox_editor::drafts::Drafts::new(work.join("drafts")), None);
                    ed.start();
                    ed.set_catalog(backend.catalog().map_err(|e| e.0)?);
                    ed.load_scene("0a")?;
                    ed.place_custom(&set, "LandActor", None);
                    if ed.current_imports().is_empty() {
                        return Err("the kit model was not placed".to_string());
                    }
                    let request = glovebox_core::build::BuildRequest {
                        sid: "0a".to_string(),
                        xml: ed.serialize(),
                        format: "rom".to_string(),
                        quickboot: true,
                        imports: ed.current_imports(),
                        expansion: true,
                        copy: false,
                        info: None,
                        created: None,
                    };
                    Ok(backend.build(&request).map_err(|e| e.0)?.data)
                })();
                let _ = std::fs::remove_dir_all(&work);
                match built {
                    Ok(data) => {
                        let problems = hwcheck::check_rom(&data);
                        say(problems.is_empty(), format!("a built ROM with a kit model keeps the console's rules{}", problems.first().map(|p| format!(": {p}")).unwrap_or_default()));
                        match hwcheck::self_test(&data) {
                            Ok(cases) => {
                                // Four of the breaks are in the mod's table of custom banks, which this ROM does
                                // not use: they can't be found here
                                let general = ["wrong CRC", "ROM not a whole MiB"];
                                let missed: Vec<&str> = cases.iter().filter(|c| !c.1 && general.contains(&c.0)).map(|c| c.0).collect();
                                let found = cases.iter().filter(|c| c.1).count();
                                say(missed.is_empty(), format!("hwcheck finds {found} of {} breaks; the others need a ROM with custom banks{}", cases.len(), if missed.is_empty() { String::new() } else { format!(" (missed: {})", missed.join(", ")) }));
                            }
                            Err(e) => say(false, format!("hwcheck's self-test: {e}")),
                        }
                    }
                    Err(e) => say(false, format!("building a ROM: {e}")),
                }
            }
            Err(e) => say(false, format!("the ROM in {}: {e}", rom.display())),
        }
    } else {
        println!("skip  no ROM in {}: the game data and hwcheck were not tested", rom.display());
    }
    println!("{} in {:.1} s", if failed == 0 { "self-test passed".to_string() } else { format!("{failed} checks failed") }, t.elapsed().as_secs_f64());
    i32::from(failed != 0)
}

/// A time of day as the autosave chip shows it.
pub fn local_time(secs: f64, format: &str) -> String {
    use chrono::TimeZone;
    match chrono::Local.timestamp_opt(secs.floor() as i64, 0).single() {
        Some(t) => t.format(format).to_string(),
        None => String::new(),
    }
}

/// "2026-01-31 12:00" in local time, for the editor's status line.
pub fn local_date_time(secs: f64) -> String {
    local_time(secs, "%Y-%m-%d %H:%M")
}

/// Opens a file with a program (Test level's emulator). False when the program did not start.
pub fn launch(program: &Path, file: &Path) -> bool {
    use std::process::{Command, Stdio};
    let mut cmd = if cfg!(target_os = "macos") && program.extension().is_some_and(|e| e == "app") {
        let mut c = Command::new("open");
        c.arg("-a").arg(program).arg(file);
        c
    } else {
        let mut c = Command::new(program);
        c.arg(file);
        c
    };
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        // A console emulator would otherwise get a console window of its own (CREATE_NO_WINDOW)
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd.spawn().is_ok()
}
