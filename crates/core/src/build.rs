//! ROM, patch and level file builds. Errors are messages for the author.

use crate::bps;
use crate::gamedata::{parse_id, scene, GameData};
use crate::imports::{build_import_banks, plan_imports, plan_runtime, Customs, Plan};
use crate::level;
use crate::modpatch::{self, ExtraBanks, LevelInfo, SlotLevel, Slots};
use crate::n64;
use crate::rom::Qol;
use std::collections::BTreeMap;

/// A level's row of the level table. None keeps the shipped value.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Info {
    pub name: Option<String>,
    pub song: Option<i64>,
    /// The bonus clock in seconds
    pub bonus_time: Option<i64>,
}

/// One level of a build.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LevelEntry {
    pub sid: String,
    /// The level file
    pub lev: Vec<u8>,
    /// Object ids the level uses from other banks
    pub imports: Vec<String>,
    pub info: Option<Info>,
    /// An extra level: it is stored behind the game mod in its Space slot (35 to 41) and loads when the player
    /// comes in through the entry zone (mod/slots.c).
    pub extra: bool,
}

/// The level a built ROM boots into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Quickboot {
    Level(String),
    /// The extra level in this level's slot
    Extra(String),
}

/// The way into the extra levels: the hub (0 to 7, or 255 for any hub) and the tag of the entry platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    pub hub: u8,
    pub tag: u16,
}

impl Default for Entry {
    fn default() -> Self {
        Entry { hub: 255, tag: 99 }
    }
}

/// The level as .lev bytes, and whether its rotations and scales need the Expansion Pak game mod.
pub fn assemble(xml: &[u8]) -> Result<(Vec<u8>, bool), String> {
    level::assemble_xml(xml).map_err(|e| e.0)
}

/// A build's plans: one for each level, keyed by its id, or `x` and its id for an extra level.
pub type Plans = Vec<(String, Plan)>;

/// One level: see `build_levels`.
pub fn build_rom(
    gd: &GameData,
    customs: &Customs,
    level: LevelEntry,
    quickboot: bool,
    with_mod: bool,
    copy: bool,
) -> Result<(Vec<u8>, Plan), String> {
    let boot = quickboot.then(|| Quickboot::Level(level.sid.clone()));
    let (rom, mut plans) = build_levels(gd, customs, &[level], boot.as_ref(), with_mod, copy, None)?;
    Ok((rom, plans.remove(0).1))
}

/// A ROM with every level of `levels` replaced.
///
/// With the game mod, imported models are loaded from the player's ROM at run time (`plan_runtime`), unless `copy`
/// is set. Otherwise they are copied into the level's object and texture banks (`build_import_banks`). When the ROM
/// also has the mod, the copied models still get the mod's larger heap. Two levels can't both copy models into the
/// same bank.
pub fn build_levels(
    gd: &GameData,
    customs: &Customs,
    levels: &[LevelEntry],
    quickboot: Option<&Quickboot>,
    with_mod: bool,
    copy: bool,
    entry: Option<Entry>,
) -> Result<(Vec<u8>, Plans), String> {
    for (i, e) in levels.iter().enumerate() {
        if levels[..i].iter().any(|o| o.sid == e.sid && o.extra == e.extra) {
            return Err("a level is in the build twice".into());
        }
    }
    let with_mod = with_mod || levels.iter().any(|e| e.extra);
    let mut rom = gd.rom().clone();
    let mut slots: BTreeMap<i64, SlotLevel> = BTreeMap::new();
    let mut plans: Plans = Vec::new();
    let mut extra: BTreeMap<i64, ExtraBanks> = BTreeMap::new();
    // Each bank that models are copied into, with the level that does it
    let mut banks: Vec<(String, &str)> = Vec::new();
    let mut replaced = Vec::new();
    for e in levels {
        let s = scene(&e.sid).map_err(|err| err.0)?;
        if e.extra {
            if !(35..=41).contains(&s.index) || s.index == 39 || s.index == 40 {
                return Err(format!(
                    "{} can't be an extra slot: only Space 1, 2, 3, its boss and its bonus can.",
                    s.name
                ));
            }
            let info = e.info.clone().unwrap_or_default();
            let default_name: String = s.name.to_uppercase().chars().take(39).collect();
            let song = match info.song {
                Some(n) => Some(u8::try_from(n).map_err(|_| format!("Song {n} is not a number from 0 to 255"))?),
                None => None,
            };
            let slot = SlotLevel {
                lev: e.lev.clone(),
                name: Some(info.name.filter(|n| !n.is_empty()).unwrap_or(default_name)),
                song,
                bonus_time: bonus(info.bonus_time)?,
            };
            slots.insert(s.index as i64, slot);
        } else {
            rom.replace_level(s.index, &e.lev).map_err(|err| format!("Could not build the ROM: {}", err.0))?;
        }
        let (mut plan, runtime) = if with_mod && !copy {
            (plan_runtime(gd, customs, &e.sid, &e.imports)?, true)
        } else {
            (plan_imports(gd, customs, &e.sid, &e.imports)?, false)
        };
        if let (true, Some(rt)) = (runtime, &plan.runtime) {
            if !rt.object_numbers.is_empty() || !rt.texture_numbers.is_empty() {
                // An extra level's banks are keyed with its slot + 0x40 (mod/slots.c Mod_SlotLevel)
                let key = rt.level as i64 + if e.extra { modpatch::SLOT_KEY } else { 0 };
                let list = ExtraBanks { textures: rt.texture_numbers.clone(), objects: rt.object_numbers.clone(), first: false };
                extra.insert(key, list);
            }
        }
        // The part of the plan that is copied into the level's banks
        let (objects, textures) = match &plan.copy {
            Some(c) if runtime => (c.objects.clone(), c.textures.clone()),
            _ => (plan.objects.clone(), plan.textures.clone()),
        };
        let mut tex_bytes = 0;
        if !objects.is_empty() {
            for b in [&plan.target_bank, &plan.target_tex_bank] {
                if let Some((_, other)) = banks.iter().find(|(name, _)| name == b) {
                    let other = scene(other).map_or("", |o| o.name.as_str());
                    return Err(format!(
                        "{} and {} both copy models into {}. Build them into separate ROMs, or let the Expansion Pak \
                         game mod load the models from the ROM.",
                        other, s.name, b
                    ));
                }
                banks.push((b.clone(), &e.sid));
            }
            let built = build_import_banks(gd, customs, &objects, &textures, &plan.target_bank, &plan.target_tex_bank)?;
            tex_bytes = built.tex_bytes;
            replaced.extend(built.banks);
        }
        match &mut plan.copy {
            Some(c) if runtime => c.tex_bytes = tex_bytes,
            _ => plan.tex_bytes = Some(tex_bytes),
        }
        plans.push((if e.extra { format!("x{}", e.sid) } else { e.sid.clone() }, plan));
    }
    let failed = |err: crate::rom::RomError| format!("Could not build the ROM: {}", err.0);
    let boot_extra = matches!(quickboot, Some(Quickboot::Extra(_)));
    if let Some(Quickboot::Level(q) | Quickboot::Extra(q)) = quickboot {
        let index = scene(q).map_err(|err| err.0)?.index;
        rom.apply_qol(&[Qol::CopySkip, Qol::QuickBoot(index as u16)]).map_err(failed)?;
    }
    for (key, data) in &replaced {
        rom.replace(key, data).map_err(failed)?;
    }
    let mut out = rom.finalize().map_err(failed)?;
    if with_mod {
        let entry = entry.unwrap_or_default();
        let slots = Slots { levels: slots, boot: boot_extra, entry_hub: entry.hub, entry_tag: entry.tag };
        let slots = (!slots.levels.is_empty()).then_some(&slots);
        out = modpatch::apply_mod(&out, Some(&extra), None, slots)
            .map_err(|err| format!("Could not add the Expansion Pak game mod: {}", err.0))?;
    }
    let mut rows: Vec<(i64, LevelInfo)> = Vec::new();
    for e in levels {
        if let (Some(info), false) = (&e.info, e.extra) {
            let row = LevelInfo { name: info.name.clone(), song: info.song, bonus_time: bonus(info.bonus_time)? };
            rows.push((scene(&e.sid).map_err(|err| err.0)?.index as i64, row));
        }
    }
    if !rows.is_empty() {
        out = modpatch::apply_level_info(&out, &rows)
            .map_err(|err| format!("Could not set the level's name, music or clock: {}", err.0))?;
    }
    // Every built ROM, and so every patch, walks the ball with Easy's controls (not inverted). This also recomputes
    // the header checksum.
    let out = modpatch::apply_easy_controls(&out).map_err(|err| format!("Could not set Easy's controls: {}", err.0))?;
    Ok((out, plans))
}

fn bonus(time: Option<i64>) -> Result<Option<i16>, String> {
    match time {
        Some(t) => i16::try_from(t).map(Some).map_err(|_| format!("A bonus clock of {t} seconds does not fit the level table")),
        None => Ok(None),
    }
}

// ---------------------------------------------------------------------------------------------------------------
// What the editor's window asks for

/// A request for one level (`Backend.build`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BuildRequest {
    pub sid: String,
    pub xml: String,
    /// `lev`, `rom` or `bps`
    pub format: String,
    pub quickboot: bool,
    pub imports: Vec<String>,
    /// Build with the Expansion Pak game mod even when the level does not need it
    pub expansion: bool,
    /// Copy imported models into the level's banks even with the mod
    pub copy: bool,
    pub info: Option<Info>,
    /// The time a patch says it was made at, as `2026-01-31T12:00:00Z`. None is now.
    pub created: Option<String>,
}

/// One level of a request for several levels.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BuildAllEntry {
    pub sid: String,
    pub xml: String,
    pub imports: Vec<String>,
    pub info: Option<Info>,
    pub extra: bool,
}

/// A request for several levels in one ROM or patch (`Backend.build_all`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BuildAllRequest {
    pub entries: Vec<BuildAllEntry>,
    /// `rom` or `bps`
    pub format: String,
    /// The id of the level to boot into
    pub quickboot: Option<String>,
    pub expansion: bool,
    pub copy: bool,
    pub created: Option<String>,
}

/// What a build did for a level's imports.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImportFacts {
    pub objects: usize,
    pub textures: usize,
    pub object_bytes: u64,
    pub tex_bytes: u64,
    pub bank: String,
    pub tex_bank: String,
    /// The number of banks the game mod loads at run time
    pub runtime: usize,
}

/// A build's result: the file's bytes and facts about it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Built {
    pub data: Vec<u8>,
    pub filename: String,
    /// The size of the level file. None for a build of several levels.
    pub lev_size: Option<usize>,
    pub needs_mod: bool,
    /// None for a level file and for a build of several levels
    pub imports: Option<ImportFacts>,
    /// The size of the patch, for a patch
    pub patch_size: Option<usize>,
    /// A build of several levels: the ids of its levels
    pub levels: Vec<String>,
    /// A build of several levels: the ids of its extra levels
    pub extra: Vec<String>,
    /// A build of several levels: the number of models its plans import
    pub objects: Option<usize>,
}

const BAD_IMPORTS: &str = "imports must be a list of object IDs like 0x73DB3DE3";

fn check_imports(imports: &[String]) -> Result<(), String> {
    if imports.len() > 256 || !imports.iter().all(|i| parse_id(i).is_some()) {
        return Err(BAD_IMPORTS.into());
    }
    Ok(())
}

/// A string as JSON text. A character outside ASCII is written as `\u` escapes.
fn py_json(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            ' '..='~' | '\u{7f}' => out.push(c),
            _ => {
                let mut units = [0u16; 2];
                for u in c.encode_utf16(&mut units) {
                    out.push_str(&format!("\\u{u:04x}"));
                }
            }
        }
    }
    out.push('"');
    out
}

fn py_list(items: &[String]) -> String {
    let items: Vec<String> = items.iter().map(|s| py_json(s)).collect();
    format!("[{}]", items.join(","))
}

/// Days since 1970-01-01 as a year, month and day.
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let (d, m) = (doy - (153 * mp + 2) / 5 + 1, if mp < 10 { mp + 3 } else { mp - 9 });
    (yoe + era * 400 + (m <= 2) as i64, m, d)
}

/// A time in seconds since 1970 as `2026-01-31T12:00:00Z`.
pub fn timestamp(seconds: u64) -> String {
    let (y, m, d) = civil((seconds / 86400) as i64);
    let s = seconds % 86400;
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", s / 3600, s / 60 % 60, s % 60)
}

fn created(given: &Option<String>) -> String {
    given.clone().unwrap_or_else(|| {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH);
        timestamp(now.map_or(0, |d| d.as_secs()))
    })
}

/// A patch from the clean ROM to `rom`. `facts` is the middle of the metadata, already as JSON.
fn patch(gd: &GameData, rom: &[u8], facts: &str, with_mod: bool, created: &str) -> Vec<u8> {
    let clean = gd.rom_bytes();
    let meta = format!(
        "{{\"tool\":\"Glovebox\",\"game\":\"Glover (NGVE)\",\"sourceSha1\":\"{}\",\"targetSha1\":\"{}\",{},\
         \"needsExpansionPak\":{},\"created\":{}}}",
        n64::sha1_hex(clean),
        n64::sha1_hex(rom),
        facts,
        with_mod,
        py_json(created)
    );
    bps::create(clean, rom, meta.as_bytes())
}

/// The level as a .lev file, a ROM or a BPS patch.
///
/// A patch is made against the clean ROM. Plain edits come out as a true diff. With the mod, imports load from the
/// player's ROM at run time and the patch has no game data. Without it, imports rebuild the level's object and
/// texture banks without compression, and the patch holds those banks in full.
pub fn build(gd: &GameData, customs: &Customs, req: &BuildRequest) -> Result<Built, String> {
    let s = scene(&req.sid).map_err(|e| e.0)?;
    let (lev, needs) = assemble(req.xml.as_bytes())?;
    if req.format == "lev" {
        return Ok(Built { filename: s.landscape.clone(), lev_size: Some(lev.len()), needs_mod: needs, data: lev, ..Built::default() });
    }
    if req.format != "rom" && req.format != "bps" {
        return Err("format must be lev, rom or bps".into());
    }
    check_imports(&req.imports)?;
    let with_mod = needs || req.expansion;
    let lev_size = lev.len();
    let entry = LevelEntry { sid: req.sid.clone(), lev, imports: req.imports.clone(), info: req.info.clone(), extra: false };
    let (rom, plan) = build_rom(gd, customs, entry, req.quickboot, with_mod, req.copy)?;
    let facts = ImportFacts {
        objects: plan.objects.len(),
        textures: plan.textures.len(),
        object_bytes: plan.object_bytes,
        tex_bytes: plan.tex_bytes.unwrap_or(0),
        bank: plan.target_bank.clone(),
        tex_bank: plan.target_tex_bank.clone(),
        runtime: plan.runtime.as_ref().map_or(0, |r| r.object_banks.len() + r.texture_banks.len()),
    };
    let mut out = Built { lev_size: Some(lev_size), needs_mod: with_mod, imports: Some(facts), ..Built::default() };
    if req.format == "rom" {
        out.filename = format!("glover-{}-edited.z64", req.sid);
        out.data = rom;
        return Ok(out);
    }
    let meta = format!(
        "\"level\":{},\"levelId\":{},\"quickboot\":{},\"importedModels\":{}",
        py_json(&s.name),
        py_json(&req.sid),
        req.quickboot,
        py_list(&plan.objects)
    );
    out.data = patch(gd, &rom, &meta, with_mod, &created(&req.created));
    out.filename = format!("glover-{}-{}.bps", req.sid, slug(&s.name));
    out.patch_size = Some(out.data.len());
    Ok(out)
}

/// A level's name in a file name: runs of other characters than letters and digits become one hyphen.
fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

/// Several levels in one ROM or BPS patch.
pub fn build_all(gd: &GameData, customs: &Customs, req: &BuildAllRequest) -> Result<Built, String> {
    if req.format != "rom" && req.format != "bps" {
        return Err("format must be rom or bps".into());
    }
    let mut levels = Vec::new();
    let mut with_mod = req.expansion;
    for e in &req.entries {
        let (lev, needs) = assemble(e.xml.as_bytes())?;
        check_imports(&e.imports)?;
        with_mod = with_mod || needs || e.extra;
        levels.push(LevelEntry { sid: e.sid.clone(), lev, imports: e.imports.clone(), info: e.info.clone(), extra: e.extra });
    }
    let quickboot = req.quickboot.as_ref().filter(|q| !q.is_empty()).map(|q| {
        match levels.iter().any(|e| e.sid == *q && e.extra) {
            true => Quickboot::Extra(q.clone()),
            false => Quickboot::Level(q.clone()),
        }
    });
    let (rom, plans) = build_levels(gd, customs, &levels, quickboot.as_ref(), with_mod, req.copy, Some(Entry::default()))?;
    let ids: Vec<String> = levels.iter().map(|e| e.sid.clone()).collect();
    let mut out = Built {
        needs_mod: with_mod,
        levels: ids.clone(),
        extra: levels.iter().filter(|e| e.extra).map(|e| e.sid.clone()).collect(),
        objects: Some(plans.iter().map(|(_, p)| p.objects.len()).sum()),
        ..Built::default()
    };
    if req.format == "rom" {
        out.filename = format!("glover-{}-levels.z64", levels.len());
        out.data = rom;
        return Ok(out);
    }
    let names: Vec<String> = ids.iter().map(|sid| scene(sid).map_or(String::new(), |s| s.name.clone())).collect();
    let boot = match &quickboot {
        None => "null".to_string(),
        Some(Quickboot::Level(q)) => py_json(q),
        Some(Quickboot::Extra(q)) => format!("[\"x\",{}]", py_json(q)),
    };
    let meta = format!("\"levels\":{},\"levelIds\":{},\"quickboot\":{}", py_list(&names), py_list(&ids), boot);
    out.data = patch(gd, &rom, &meta, with_mod, &created(&req.created));
    out.filename = format!("glover-{}-levels.bps", levels.len());
    out.patch_size = Some(out.data.len());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_are_written_in_utc() {
        assert_eq!(timestamp(0), "1970-01-01T00:00:00Z");
        assert_eq!(timestamp(951_782_399), "2000-02-28T23:59:59Z");
        assert_eq!(timestamp(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(timestamp(1_794_355_200 + 3661), "2026-11-11T01:01:01Z");
    }

    #[test]
    fn json_text_is_ascii() {
        assert_eq!(py_json("Hub 1"), "\"Hub 1\"");
        assert_eq!(py_json("a\"b\\c\n\u{1}"), "\"a\\\"b\\\\c\\n\\u0001\"");
        assert_eq!(py_json("é\u{1F600}"), "\"\\u00e9\\ud83d\\ude00\"");
        assert_eq!(py_list(&["a".into(), "b".into()]), "[\"a\",\"b\"]");
    }

    #[test]
    fn slugs() {
        assert_eq!(slug("Atlantis 1"), "atlantis-1");
        assert_eq!(slug("  Out of This World: Boss! "), "out-of-this-world-boss");
    }

    #[test]
    fn import_lists_are_checked() {
        assert!(check_imports(&["0x73DB3DE3".into()]).is_ok());
        assert_eq!(check_imports(&["0x73db3de3".into()]), Err(BAD_IMPORTS.to_string()));
        assert!(check_imports(&vec!["0x73DB3DE3".to_string(); 257]).is_err());
    }
}
