//! The tables compiled into the program. None of them holds game data.
//!
//! - `decomp()`: the level format (commands, types, fields, enums) joined with the gloveworld decompilation's names,
//!   parser locations and diagnostics.
//! - `scenes()`: the 48 levels with their names and the banks each one loads.
//! - `noclip_tables()`: which model each enemy and powerup type uses.
//! - `heap()`: free heap per level, measured in the emulator.
//! - `bank_numbers()`: the game's bank numbers, from the switch statements of LoadObjectBank and LoadTextureBank in
//!   the decompilation.
//! - `mod_info()` and `MOD_BIN`: the Expansion Pak game mod (`mod/`).

use serde_json::Value;
use std::sync::OnceLock;

fn table(cell: &'static OnceLock<Value>, text: &'static str) -> &'static Value {
    cell.get_or_init(|| serde_json::from_str(text).expect("a compiled-in table is valid JSON"))
}

pub fn decomp() -> &'static Value {
    static T: OnceLock<Value> = OnceLock::new();
    table(&T, include_str!("../data/decomp.json"))
}

pub fn scenes() -> &'static Value {
    static T: OnceLock<Value> = OnceLock::new();
    table(&T, include_str!("../data/scenes.json"))
}

pub fn noclip_tables() -> &'static Value {
    static T: OnceLock<Value> = OnceLock::new();
    table(&T, include_str!("../data/noclip_tables.json"))
}

pub fn heap() -> &'static Value {
    static T: OnceLock<Value> = OnceLock::new();
    table(&T, include_str!("../data/heap.json"))
}

pub fn mod_info() -> &'static Value {
    static T: OnceLock<Value> = OnceLock::new();
    table(&T, include_str!("../../../mod/mod.json"))
}

pub static MOD_BIN: &[u8] = include_bytes!("../../../mod/mod.bin");

/// The game's bank numbers, from the switch statements of LoadObjectBank and LoadTextureBank in the decompilation:
/// `{"obj": {name: number}, "tex": {name: number}}`. LEVELEND_TEX_BANK is 1 and 14; the table has 1.
pub fn bank_numbers() -> &'static Value {
    static T: OnceLock<Value> = OnceLock::new();
    table(&T, include_str!("../data/bank_numbers.json"))
}
