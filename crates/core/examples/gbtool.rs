//! A small command-line tool over the back end, for developers and for the emulator tests.
//!
//!     gbtool xml ROM SID                          print a level's XML
//!     gbtool build ROM SID XML OUT [--quickboot] [--expansion] [--copy] [--bps] [--import 0xID]...
//!     gbtool hwcheck ROM                          check a built ROM against the console's rules
//!     gbtool pack ROM SID XML OUTDIR NAME [--home DIR]
//!                                                 write a level pack (docs/technical.md, "Level packs for g64coopdx")
//!     gbtool import ROM GLTF [--scale S] [--home DIR] import a glTF model into the user folder
//!     gbtool modelbanks GLTF OUT                  write a glTF model as a pack's models.bin for g64coopdx
//!
//!     gbtool --help                               print the usage line
//!
//! SID is a level's id: two hex digits, as the editor's level list shows them (`0a` is Atlantis Level 1).
//! OUT gets the built ROM (or the BPS patch with --bps).
//! `build` reads imported models from the user folder that the environment variable `GLOVEBOX_HOME` names. `pack`
//! and `import` use the folder that `--home` names, or else that variable.

use glovebox_core::backend::{Backend, GameBackend};
use glovebox_core::build::BuildRequest;
use std::path::Path;
use std::process::ExitCode;

/// The user folder: `--home DIR` among `opts`, else `GLOVEBOX_HOME`, else a folder in the temporary folder. The
/// other options are returned as pairs of a name and a value.
fn options(opts: &[String], allowed: &[&str]) -> Result<(std::path::PathBuf, Vec<(String, String)>), String> {
    let mut home = std::env::var("GLOVEBOX_HOME").map(Into::into).unwrap_or_else(|_| std::env::temp_dir().join("gbtool-home"));
    let mut rest = Vec::new();
    let mut i = 0;
    while i < opts.len() {
        let name = opts[i].as_str();
        let value = opts.get(i + 1).ok_or_else(|| format!("{name} needs a value"))?.clone();
        match name {
            "--home" => home = value.into(),
            _ if allowed.contains(&name) => rest.push((name.to_string(), value)),
            _ => return Err(format!("unknown option {name}")),
        }
        i += 2;
    }
    Ok((home, rest))
}

const USAGE: &str = "usage: gbtool xml ROM SID | build ROM SID XML OUT [options] | hwcheck ROM | pack ROM SID XML OUTDIR NAME [--home DIR] | import ROM GLTF [--scale S] [--home DIR] | modelbanks GLTF OUT";

fn run(args: &[String]) -> Result<(), String> {
    let cmd = args.first().map(String::as_str).unwrap_or("");
    match cmd {
        "xml" if args.len() == 3 => {
            let home = std::env::temp_dir().join("gbtool-home");
            let be = GameBackend::open(Path::new(&args[1]), home).map_err(|e| e.to_string())?;
            print!("{}", be.level_xml(&args[2]).map_err(|e| e.to_string())?);
            Ok(())
        }
        "build" if args.len() >= 5 => {
            let home = std::env::var("GLOVEBOX_HOME").map(Into::into).unwrap_or_else(|_| std::env::temp_dir().join("gbtool-home"));
            let be = GameBackend::open(Path::new(&args[1]), home).map_err(|e| e.to_string())?;
            let xml = std::fs::read_to_string(&args[3]).map_err(|e| e.to_string())?;
            let mut req = BuildRequest {
                sid: args[2].clone(),
                xml,
                format: "rom".into(),
                quickboot: false,
                imports: Vec::new(),
                expansion: false,
                copy: false,
                info: None,
                created: None,
            };
            let mut i = 5;
            while i < args.len() {
                match args[i].as_str() {
                    "--quickboot" => req.quickboot = true,
                    "--expansion" => req.expansion = true,
                    "--copy" => req.copy = true,
                    "--bps" => req.format = "bps".into(),
                    "--import" => {
                        i += 1;
                        req.imports.push(args.get(i).ok_or("--import needs an object id")?.clone());
                    }
                    other => return Err(format!("unknown option {}", other)),
                }
                i += 1;
            }
            let built = be.build(&req).map_err(|e| e.to_string())?;
            std::fs::write(&args[4], &built.data).map_err(|e| e.to_string())?;
            eprintln!("{}: {} bytes, game mod {}", args[4], built.data.len(), if built.needs_mod { "yes" } else { "no" });
            Ok(())
        }
        "pack" if args.len() >= 6 => {
            // The level's imports come from the ROM and from the imported models in the user folder.
            let (home, _) = options(&args[6..], &[])?;
            let be = GameBackend::open(Path::new(&args[1]), home).map_err(|e| e.to_string())?;
            let scene = glovebox_core::gamedata::scene(&args[2]).map_err(|e| e.0)?;
            let xml = std::fs::read_to_string(&args[3]).map_err(|e| e.to_string())?;
            let pack = be.level_pack(&scene.id, &xml, &args[5]).map_err(|e| e.to_string())?;
            // The folder's id counts a script.lua that is already there
            let id = pack.write_to(Path::new(&args[4])).map_err(|e| e.to_string())?;
            let script = if id != pack.id { ", with the folder's script.lua" } else { "" };
            let models = match &pack.models {
                Some(m) => format!("models.bin {} bytes", m.len()),
                None => "no models.bin".to_string(),
            };
            eprintln!(
                "{}: level {} ({}), level.lev {} bytes, {}{}, id {}",
                args[4],
                scene.index,
                scene.name,
                pack.level.len(),
                models,
                script,
                glovebox_core::pack::id_text(&id)
            );
            let (tex, obj) = pack.game_banks();
            if tex + obj > 0 {
                eprintln!("{}", glovebox_core::pack::GAME_BANKS_TEXT);
            }
            Ok(())
        }
        "import" if args.len() >= 3 => {
            let (home, rest) = options(&args[3..], &["--scale"])?;
            let scale = match rest.iter().find(|(k, _)| k == "--scale") {
                Some((_, v)) => v.parse::<f64>().map_err(|_| format!("--scale {v} isn't a number"))?,
                None => 1.0,
            };
            let be = GameBackend::open(Path::new(&args[1]), home).map_err(|e| e.to_string())?;
            let data = std::fs::read(&args[2]).map_err(|e| e.to_string())?;
            let name = Path::new(&args[2]).file_stem().and_then(|s| s.to_str()).unwrap_or("model");
            let meta = be.import_custom(&data, name, scale).map_err(|e| e.to_string())?;
            let ids: Vec<&str> = meta["pieces"].as_array().into_iter().flatten().filter_map(|p| p["id"].as_str()).collect();
            println!("{}", ids.join(" "));
            eprintln!("set {}: {} piece(s)", meta["set"].as_str().unwrap_or("?"), ids.len());
            Ok(())
        }
        "modelbanks" if args.len() == 3 => {
            // The converter needs no ROM. With no ROM it can't avoid the game's ids. g64coopdx refuses a pack
            // with the game's ids when it has the ROM.
            let data = std::fs::read(&args[1]).map_err(|e| e.to_string())?;
            let name = Path::new(&args[1]).file_stem().and_then(|s| s.to_str()).unwrap_or("model");
            let none = std::collections::HashSet::new();
            let set = glovebox_core::custom::convert(&data, name, 1.0, &none, &none).map_err(|e| e.to_string())?;
            let pieces: Vec<_> = set.pieces.iter().collect();
            let (textures, objects) =
                glovebox_core::custom::pack::model_banks(&pieces, &[&set.texture_bank]).map_err(|e| e.to_string())?;
            let file = glovebox_core::pack::models_bin(false, &[], &[], &textures, &objects)?;
            std::fs::write(&args[2], &file).map_err(|e| e.to_string())?;
            let ids: Vec<String> = set.pieces.iter().map(|p| format!("0x{:08X}", p.id)).collect();
            let tex: Vec<String> = set.textures.iter().map(|t| format!("0x{:08X}", t.0)).collect();
            eprintln!(
                "{}: {} bytes, texture bank {} bytes ({}), object bank {} bytes ({})",
                args[2],
                file.len(),
                textures.len(),
                tex.join(" "),
                objects.len(),
                ids.join(" ")
            );
            Ok(())
        }
        "hwcheck" if args.len() == 2 => {
            let rom = std::fs::read(&args[1]).map_err(|e| e.to_string())?;
            let found = glovebox_core::hwcheck::check_rom(&rom);
            if found.is_empty() {
                println!("ok");
                Ok(())
            } else {
                Err(found.join("\n"))
            }
        }
        "--help" | "-h" => {
            println!("{USAGE}");
            Ok(())
        }
        _ => Err(USAGE.into()),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{}", e);
            ExitCode::FAILURE
        }
    }
}
