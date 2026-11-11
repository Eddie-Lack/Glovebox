//! A small command-line tool over the back end, for developers and for the emulator tests.
//!
//!     gbtool xml ROM SID                          print a level's XML
//!     gbtool build ROM SID XML OUT [--quickboot] [--expansion] [--copy] [--bps] [--import 0xID]...
//!     gbtool hwcheck ROM                          check a built ROM against the console's rules
//!
//! OUT gets the built ROM (or the BPS patch with --bps).
//! `build` reads imported models from the user folder that the environment variable `GLOVEBOX_HOME` names.

use glovebox_core::backend::{Backend, GameBackend};
use glovebox_core::build::BuildRequest;
use std::path::Path;
use std::process::ExitCode;

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
        _ => Err("usage: gbtool xml ROM SID | build ROM SID XML OUT [options] | hwcheck ROM".into()),
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
