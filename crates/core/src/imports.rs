//! Which banks an edited level needs for the models it imports, and the enlarged banks a build writes.
//!
//! Ids are text here, as the editor keeps them: `0x73DB3DE3`. A texture bank is named by its file name in noclip's
//! data set (`HUB_TEX_BANK.tex.fla`), or `custom:<set>` for the texture bank of an imported model.

use crate::fla2;
use crate::gamedata::{bank_number, hex, parse_id, scene, stem, GameData};
use crate::objbank;
use crate::rom::Key;
use crate::texbank;
use serde_json::{json, Map, Value};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

/// One piece of an imported model.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CustomModel {
    /// The name the catalog shows: the model's name, with `2/3` after it when the model has several pieces
    pub name: String,
    /// The model's name alone
    pub set_name: String,
    /// The set's id: 10 hex digits
    pub set: String,
    /// The piece's name in the game (8 characters at most)
    pub piece: String,
    pub offset: [f64; 3],
    /// The ids of the textures the piece uses
    pub textures: Vec<String>,
    /// The piece's single-object bank
    pub bank: Vec<u8>,
    /// The size `meta.json` gives for the bank
    pub bytes: u64,
    /// The retail model a reskinned model was made from
    pub reskin_from: Option<String>,
}

impl CustomModel {
    /// The name of the set's texture bank: `custom:<set>`.
    pub fn tex_bank(&self) -> String {
        format!("custom:{}", self.set)
    }
}

/// The author's imported models.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Customs {
    /// By object id, in the order of the sets and of their pieces
    pub models: Vec<(String, CustomModel)>,
    /// Each set's texture bank (`textures.tex.bin`), by set
    pub tex_banks: BTreeMap<String, Vec<u8>>,
}

impl Customs {
    pub fn get(&self, id: &str) -> Option<&CustomModel> {
        self.models.iter().find(|(k, _)| k == id).map(|(_, m)| m)
    }
}

/// Reads the imported models of `<home>/custom` (`custom::custom_catalog`), with the bank of each piece
/// (`<id>.obj.bin`) and each set's `textures.tex.bin`. A piece whose bank can't be read is left out.
pub fn load_customs(home: &Path) -> Customs {
    let mut out = Customs::default();
    for entry in crate::custom::custom_catalog(home) {
        // The id is part of a file name here. Only a real id is safe to use as one.
        if parse_id(&entry.id).is_none() || !crate::custom::is_set_id(&entry.custom) {
            continue;
        }
        let Ok(bank) = std::fs::read(&entry.path) else { continue };
        if !out.tex_banks.contains_key(&entry.custom) {
            let dir = entry.path.parent().unwrap_or(home);
            if let Ok(tex) = std::fs::read(dir.join("textures.tex.bin")) {
                out.tex_banks.insert(entry.custom.clone(), tex);
            }
        }
        let set_name = entry.bank.strip_prefix("Custom: ").unwrap_or(&entry.bank).to_string();
        let model = CustomModel {
            name: entry.name,
            set_name,
            set: entry.custom,
            piece: entry.piece,
            offset: entry.offset,
            textures: entry.textures,
            bank,
            bytes: entry.bytes,
            reskin_from: entry.reskin_from,
        };
        out.models.push((entry.id, model));
    }
    out
}

/// The part of a plan that a build copies into the level's own banks.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CopyPlan {
    pub objects: Vec<String>,
    /// Each texture to copy and the bank it comes from, in the order they are added
    pub textures: Vec<(String, String)>,
    pub target_bank: String,
    pub target_tex_bank: String,
    pub object_bytes: u64,
    pub tex_bytes: u64,
}

/// The banks the game mod loads from the player's ROM when the level starts.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Runtime {
    /// The level's number
    pub level: usize,
    pub object_banks: Vec<String>,
    pub texture_banks: Vec<String>,
    pub object_numbers: Vec<i64>,
    pub texture_numbers: Vec<i64>,
    /// The size of those banks, not compressed
    pub bytes: u64,
}

/// What a build has to add for a level's imports.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Plan {
    /// The imports the level hasn't loaded, each once, in the order given
    pub objects: Vec<String>,
    pub target_bank: String,
    /// Each texture the level lacks and the bank it comes from
    pub textures: Vec<(String, String)>,
    pub target_tex_bank: String,
    pub object_bytes: u64,
    /// None until a build or `Backend::plan` has set it
    pub tex_bytes: Option<u64>,
    /// With the game mod: the imported models, which aren't in the ROM and are copied
    pub copy: Option<CopyPlan>,
    /// With the game mod: the banks it loads
    pub runtime: Option<Runtime>,
}

fn pairs(list: &[(String, String)]) -> Value {
    Value::Object(list.iter().map(|(k, v)| (k.clone(), Value::from(v.as_str()))).collect::<Map<_, _>>())
}

impl Plan {
    /// The plan as a JSON object.
    pub fn to_json(&self) -> Value {
        let mut v = json!({"objects": self.objects, "targetBank": self.target_bank, "textures": pairs(&self.textures),
                           "targetTexBank": self.target_tex_bank, "objectBytes": self.object_bytes});
        if let Some(c) = &self.copy {
            v["copy"] = json!({"objects": c.objects, "textures": pairs(&c.textures), "targetBank": c.target_bank,
                               "targetTexBank": c.target_tex_bank, "objectBytes": c.object_bytes,
                               "texBytes": c.tex_bytes});
        }
        if let Some(r) = &self.runtime {
            v["runtime"] = json!({"level": r.level, "objectBanks": r.object_banks, "textureBanks": r.texture_banks,
                                  "objectNumbers": r.object_numbers, "textureNumbers": r.texture_numbers,
                                  "bytes": r.bytes});
        }
        if let Some(n) = self.tex_bytes {
            v["texBytes"] = json!(n);
        }
        v
    }
}

/// A texture bank's data, not compressed. A `custom:` bank is an imported set's.
fn raw_bank<'a>(gd: &'a GameData, customs: &'a Customs, bank: &str) -> Result<Cow<'a, [u8]>, String> {
    match bank.strip_prefix("custom:") {
        Some(set) => custom_tex_bank(customs, set),
        None => gd.raw_bank(bank).map(Cow::Borrowed).map_err(|e| e.0),
    }
}

/// The texture bank of the imported set `set`, not compressed.
pub fn custom_tex_bank<'a>(customs: &'a Customs, set: &str) -> Result<Cow<'a, [u8]>, String> {
    match customs.tex_banks.get(set) {
        Some(data) if fla2::is_fla2(data) => fla2::decompress(data).map(Cow::Owned).map_err(|e| e.to_string()),
        Some(data) => Ok(Cow::Borrowed(data.as_slice())),
        None => Err(format!("The imported model {set} has no texture bank")),
    }
}

/// The records of texture banks, split once for each bank.
struct RecordCache<'a> {
    gd: &'a GameData,
    customs: &'a Customs,
    banks: HashMap<String, HashMap<u32, Vec<u8>>>,
}

impl<'a> RecordCache<'a> {
    fn new(gd: &'a GameData, customs: &'a Customs) -> Self {
        RecordCache { gd, customs, banks: HashMap::new() }
    }

    /// The record of texture `tid` in `bank`, padded to a multiple of 8.
    fn get(&mut self, tid: &str, bank: &str) -> Result<&[u8], String> {
        if !self.banks.contains_key(bank) {
            let data = raw_bank(self.gd, self.customs, bank)?;
            let recs = texbank::records(&data).map_err(|e| format!("{bank}: {}", e.0))?;
            let mut by_id = HashMap::new();
            for (id, rec) in recs.records {
                by_id.insert(id, rec.into_owned());
            }
            self.banks.insert(bank.to_string(), by_id);
        }
        let rec = parse_id(tid).and_then(|id| self.banks.get(bank)?.get(&id));
        rec.map(|r| r.as_slice()).ok_or_else(|| format!("Texture {tid} isn't in {bank}"))
    }

    /// The size of these textures' records (`texBytes`).
    fn size(&mut self, textures: &[(String, String)]) -> Result<u64, String> {
        let mut sum = 0;
        for (tid, bank) in textures {
            sum += self.get(tid, bank)?.len() as u64;
        }
        Ok(sum)
    }
}

/// The size of the records of a plan's textures (`texBytes` in a plan).
pub fn tex_bytes(gd: &GameData, customs: &Customs, textures: &[(String, String)]) -> Result<u64, String> {
    RecordCache::new(gd, customs).size(textures)
}

/// The size a plan counts for a model's bank.
fn model_bytes(gd: &GameData, customs: &Customs, id: &str) -> u64 {
    match customs.get(id) {
        Some(c) => c.bytes,
        None => parse_id(id).and_then(|i| gd.model_bank(i)).map_or(0, |b| b.len() as u64),
    }
}

/// What a ROM build has to add for the level `sid` to draw `imports` (object ids from other banks). The targets are
/// banks the game loads for the level: its own object bank, loaded last, and its last texture bank.
pub fn plan_imports(gd: &GameData, customs: &Customs, sid: &str, imports: &[String]) -> Result<Plan, String> {
    let s = scene(sid).map_err(|e| e.0)?;
    let loaded = gd.loaded_ids(sid).map_err(|e| e.0)?;
    let cat = gd.catalog();
    let mut objs: Vec<String> = Vec::new();
    for o in imports {
        let is_loaded = parse_id(o).is_some_and(|id| loaded.contains(&id));
        if !is_loaded && !objs.contains(o) {
            objs.push(o.clone());
        }
    }
    for o in &objs {
        let known = customs.get(o).is_some() || parse_id(o).is_some_and(|id| cat.contains_key(&id));
        if !known {
            return Err(format!("Object {o} isn't in any of the game's object banks or the imported models"));
        }
    }
    let mut have: BTreeSet<u32> = BTreeSet::new();
    for t in &s.sure_texture_banks {
        have.extend(gd.texture_ids(t));
    }
    let mut need: BTreeSet<String> = BTreeSet::new();
    for o in &objs {
        match customs.get(o) {
            Some(c) => need.extend(c.textures.iter().cloned()),
            None => {
                let ids = parse_id(o).and_then(|id| gd.object_texture_ids(id));
                need.extend(ids.iter().flat_map(|set| set.iter()).map(|t| hex(*t)));
            }
        }
    }
    need.retain(|t| !parse_id(t).is_some_and(|id| have.contains(&id)));
    // An imported model's textures come from its own texture bank
    let mut textures: Vec<(String, String)> = Vec::new();
    for o in &objs {
        let Some(c) = customs.get(o) else { continue };
        for t in &c.textures {
            if need.contains(t) {
                match textures.iter_mut().find(|(k, _)| k == t) {
                    Some(pair) => pair.1 = c.tex_bank(),
                    None => textures.push((t.clone(), c.tex_bank())),
                }
            }
        }
    }
    for t in &need {
        if textures.iter().any(|(k, _)| k == t) {
            continue;
        }
        let Some(id) = parse_id(t) else { continue };
        if let Some(bank) = gd.all_texture_banks().find(|b| gd.texture_ids(b).contains(&id)) {
            textures.push((t.clone(), bank.to_string()));
        }
    }
    Ok(Plan {
        object_bytes: objs.iter().map(|o| model_bytes(gd, customs, o)).sum(),
        objects: objs,
        target_bank: s.import_object_bank.clone(),
        textures,
        target_tex_bank: s.import_texture_bank.clone(),
        tex_bytes: None,
        copy: None,
        runtime: None,
    })
}

/// The plan with the game mod. Imports from the game aren't copied into the ROM: the mod loads the banks they come
/// from, from the player's own ROM, when the level starts (mod/banks.c). This picks those banks: each model's object
/// bank, and texture banks for the textures the level's own banks lack, the model's own world first. Imported custom
/// models aren't in the ROM: `copy` is the part of the plan that copies them into the level's banks.
pub fn plan_runtime(gd: &GameData, customs: &Customs, sid: &str, imports: &[String]) -> Result<Plan, String> {
    let mut plan = plan_imports(gd, customs, sid, imports)?;
    let s = scene(sid).map_err(|e| e.0)?;
    let cat = gd.catalog();
    let (mut objs, mut copied) = (Vec::new(), Vec::new());
    for o in &plan.objects {
        match customs.get(o) {
            Some(_) => copied.push(o.clone()),
            None => objs.push(o.clone()),
        }
    }
    let custom_tex: BTreeSet<&String> =
        copied.iter().filter_map(|o| customs.get(o)).flat_map(|c| c.textures.iter()).collect();
    let copy_textures: Vec<(String, String)> =
        plan.textures.iter().filter(|(t, _)| custom_tex.contains(t)).cloned().collect();
    let copy = CopyPlan {
        object_bytes: copied.iter().map(|o| model_bytes(gd, customs, o)).sum(),
        objects: copied,
        tex_bytes: tex_bytes(gd, customs, &copy_textures)?,
        textures: copy_textures,
        target_bank: plan.target_bank.clone(),
        target_tex_bank: plan.target_tex_bank.clone(),
    };
    let entries: Vec<_> = objs.iter().filter_map(|o| parse_id(o).and_then(|id| cat.get(&id))).collect();
    let mut obj_banks: Vec<String> = Vec::new();
    for e in &entries {
        if !obj_banks.contains(&e.bank) && !s.game_object_banks.contains(&e.bank) {
            obj_banks.push(e.bank.clone());
        }
    }
    let loaded_tex: Vec<&str> = s.game_texture_banks.iter().map(|t| stem(t)).collect();
    let mut need: BTreeSet<&String> = plan.textures.iter().map(|(t, _)| t).filter(|t| !custom_tex.contains(t)).collect();
    let mut candidates: Vec<String> = Vec::new();
    let world_banks = entries.iter().map(|e| format!("{}_TEX_BANK", e.world));
    let source_banks = plan.textures.iter().filter(|(t, _)| need.contains(t)).map(|(_, b)| stem(b).to_string());
    for b in world_banks.chain(source_banks) {
        if !candidates.contains(&b) {
            candidates.push(b);
        }
    }
    let mut tex_banks: Vec<String> = Vec::new();
    for b in candidates {
        // The font bank always goes to slot 0. It can't be loaded as an extra bank.
        if loaded_tex.contains(&b.as_str()) || bank_number("tex", &b).is_none() || b == "FONT_TEX_BANK" {
            continue;
        }
        let ids = gd.texture_ids(&format!("{b}.tex.fla"));
        let before = need.len();
        need.retain(|t| !parse_id(t).is_some_and(|id| ids.contains(&id)));
        if need.len() < before {
            tex_banks.push(b);
        }
    }
    if !need.is_empty() {
        let list: Vec<&str> = need.iter().map(|t| t.as_str()).collect();
        return Err(format!("No texture bank has textures {}", list.join(", ")));
    }
    let number = |kind: &str, b: &String| bank_number(kind, b).ok_or_else(|| format!("Bank {b} has no number in the game"));
    let mut bytes = 0;
    for b in &obj_banks {
        bytes += gd.raw_bank(&format!("{b}.obj.fla")).map_err(|e| e.0)?.len() as u64;
    }
    for b in &tex_banks {
        bytes += gd.raw_bank(&format!("{b}.tex.fla")).map_err(|e| e.0)?.len() as u64;
    }
    plan.runtime = Some(Runtime {
        level: s.index,
        object_numbers: obj_banks.iter().map(|b| number("obj", b)).collect::<Result<_, _>>()?,
        texture_numbers: tex_banks.iter().map(|b| number("tex", b)).collect::<Result<_, _>>()?,
        object_banks: obj_banks,
        texture_banks: tex_banks,
        bytes,
    });
    plan.copy = Some(copy);
    plan.tex_bytes = Some(0);
    Ok(plan)
}

/// The enlarged banks of a build: each region to replace with its new data, not compressed. The game reads a bank
/// that isn't compressed as it reads a compressed one.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImportBanks {
    pub banks: Vec<(Key, Vec<u8>)>,
    /// The size of the texture records that were added
    pub tex_bytes: u64,
}

/// Builds the banks for the objects and textures to copy: the target object bank with
/// each model's bank packed into it, and the target texture bank with the textures' records added.
pub fn build_import_banks(
    gd: &GameData,
    customs: &Customs,
    objects: &[String],
    textures: &[(String, String)],
    target_bank: &str,
    target_tex_bank: &str,
) -> Result<ImportBanks, String> {
    let mut out = ImportBanks::default();
    if !objects.is_empty() {
        let mut singles: Vec<&[u8]> = Vec::new();
        for o in objects {
            let bank = match customs.get(o) {
                Some(c) => Some(c.bank.as_slice()),
                None => parse_id(o).and_then(|id| gd.model_bank(id)),
            };
            singles.push(bank.ok_or_else(|| format!("Object {o} has no bank"))?);
        }
        let base = gd.raw_bank(&format!("{target_bank}.obj.fla")).map_err(|e| e.0)?;
        let packed = objbank::pack(base, &singles).map_err(|e| format!("Packing the object bank failed:\n{}", e.0))?;
        let key = gd.object_bank_key(target_bank).ok_or_else(|| format!("The ROM has no object bank {target_bank}"))?;
        out.banks.push((key, packed));
    }
    let mut cache = RecordCache::new(gd, customs);
    if !textures.is_empty() {
        let mut extra: Vec<(u32, Vec<u8>)> = Vec::new();
        for (tid, bank) in textures {
            let rec = cache.get(tid, bank)?.to_vec();
            extra.push((parse_id(tid).unwrap_or(0), rec));
        }
        let extra: Vec<(u32, &[u8])> = extra.iter().map(|(id, r)| (*id, r.as_slice())).collect();
        let base = gd.raw_bank(&format!("{target_tex_bank}.tex.fla")).map_err(|e| e.0)?;
        let data = texbank::import_bank(base, &extra).map_err(|e| e.0)?;
        let key = gd
            .texture_bank_key(target_tex_bank)
            .ok_or_else(|| format!("The ROM has no texture bank {target_tex_bank}"))?;
        out.banks.push((key, data));
    }
    out.tex_bytes = cache.size(textures)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plan_prints_as_json() {
        let mut plan = Plan {
            objects: vec!["0x00000001".into()],
            target_bank: "A".into(),
            textures: vec![("0x00000002".into(), "B.tex.fla".into())],
            target_tex_bank: "C".into(),
            object_bytes: 8,
            ..Plan::default()
        };
        assert_eq!(
            plan.to_json(),
            json!({"objects": ["0x00000001"], "targetBank": "A", "textures": {"0x00000002": "B.tex.fla"},
                   "targetTexBank": "C", "objectBytes": 8})
        );
        plan.tex_bytes = Some(0);
        plan.runtime = Some(Runtime::default());
        plan.copy = Some(CopyPlan::default());
        let v = plan.to_json();
        assert_eq!(v["texBytes"], 0);
        assert_eq!(v["runtime"]["objectBanks"], json!([]));
        assert_eq!(v["copy"]["texBytes"], 0);
    }

    #[test]
    fn no_folder_means_no_imported_models() {
        assert_eq!(load_customs(Path::new("/nonexistent/home")), Customs::default());
    }
}
