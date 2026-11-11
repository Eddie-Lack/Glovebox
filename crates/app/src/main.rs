//! Glovebox, the Glover level editor: the program. `glovebox::run` does the work.
//!
//! The release build for Windows is a windowed program: it has no console of its own. It prints into the console
//! it was started from when there is one. Otherwise redirect its output to a file (`Glovebox.exe --self-test >
//! report.txt`).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() {
    glovebox::shell::mark_start();
    let args = match glovebox::shell::parse_args(std::env::args().skip(1)) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}\n\n{}", glovebox::shell::HELP);
            std::process::exit(2);
        }
    };
    std::process::exit(glovebox::run(args));
}
