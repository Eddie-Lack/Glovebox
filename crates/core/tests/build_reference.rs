//! The back end (`gamedata`, `imports`, `build`, `backend`) against the reference dumps, on the whole game.
//!
//! `GLOVEBOX_ROM` names the ROM, `GLOVEBOX_REFERENCE` the folder of reference dumps and `GLOVEBOX_REFHOME` a folder
//! with the split ROM and exported models. Without the three variables every test here prints that it was skipped
//! and passes.

use glovebox_core::backend::{Backend, GameBackend, ModelRef};
use glovebox_core::build::{BuildAllEntry, BuildAllRequest, BuildRequest, Built, Info};
use glovebox_core::gamedata::{self, hex, parse_id, GameData};
use glovebox_core::imports::{self, Customs, Plan};
use glovebox_core::{bps, n64};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

struct Env {
    game: Arc<GameData>,
    reference: PathBuf,
    refhome: PathBuf,
}

/// The game's data, opened once for all the tests.
fn env(test: &str) -> Option<&'static Env> {
    static ENV: OnceLock<Option<Env>> = OnceLock::new();
    let e = ENV.get_or_init(|| {
        let var = |name: &str| std::env::var_os(name).map(PathBuf::from);
        let (rom, reference, refhome) = (var("GLOVEBOX_ROM")?, var("GLOVEBOX_REFERENCE")?, var("GLOVEBOX_REFHOME")?);
        let game = GameData::open(read(&rom)).unwrap();
        Some(Env { game: Arc::new(game), reference, refhome })
    });
    if e.is_none() {
        println!("{test}: skipped, GLOVEBOX_ROM, GLOVEBOX_REFERENCE and GLOVEBOX_REFHOME aren't all set");
    }
    e.as_ref()
}

fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn json_file(path: &Path) -> Value {
    serde_json::from_slice(&read(path)).unwrap()
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array().map(|a| a.iter().map(|s| s.as_str().unwrap().to_string()).collect()).unwrap_or_default()
}

fn hex_set(ids: impl IntoIterator<Item = u32>) -> BTreeSet<String> {
    ids.into_iter().map(hex).collect()
}

fn string_set(v: &Value) -> BTreeSet<String> {
    strings(v).into_iter().collect()
}

fn backend(e: &Env) -> Arc<dyn Backend> {
    // No folder of imported models: the reference data was made without one
    Arc::new(GameBackend::new(e.game.clone(), e.reference.join("gamedata/no-home"), Some(PathBuf::from("rom.z64"))))
}

#[test]
fn levels_match_the_reference() {
    let Some(e) = env("levels_match_the_reference") else { return };
    let misc = json_file(&e.reference.join("gamedata/misc.json"));
    for s in gamedata::scenes() {
        let want = read(&e.reference.join(format!("levels/{}.xml", s.id)));
        assert!(e.game.level_xml(&s.id).unwrap().as_bytes() == want, "level {}: the XML differs", s.id);
        assert_eq!(e.game.level_file(&s.id).unwrap(), read(&e.reference.join(format!("levels/{}.lev", s.id))), "{}", s.id);
        assert_eq!(e.game.orig_lev_size(&s.id).map(|n| n as u64), misc["origLevSize"][&s.id].as_u64(), "{}", s.id);
    }
    assert_eq!(e.game.orig_lev_size("zz"), None);
    assert!(e.game.level_xml("30").is_err());
}

#[test]
fn the_catalog_matches_the_reference() {
    let Some(e) = env("the_catalog_matches_the_reference") else { return };
    let want = json_file(&e.reference.join("catalog.json"));
    let want = want.as_object().unwrap();
    let got = e.game.catalog();
    assert_eq!(hex_set(got.keys().copied()), want.keys().cloned().collect::<BTreeSet<_>>());
    for (id, m) in got {
        let w = &want[&hex(*id)];
        let mine = json!({"name": m.name, "bank": m.bank, "bytes": m.bytes, "world": m.world, "firstScene": m.first_scene});
        for key in ["name", "bank", "bytes", "world", "firstScene"] {
            assert_eq!(mine[key], w[key], "{} {key}", hex(*id));
        }
        assert_eq!(e.game.model_bank(*id).unwrap().len(), m.bytes);
        // The catalog's model is the model of the first scene
        assert!(e.game.model(&m.first_scene, *id).unwrap().is_some(), "{}", hex(*id));
    }
    // The same through the back end, by model reference
    let b = backend(e);
    let cat = b.catalog().unwrap();
    assert_eq!(cat.len(), want.len());
    for (id, c) in cat.iter().step_by(40) {
        let w = &want[id];
        assert_eq!(json!([c.name, c.bank, c.bytes, c.world, c.first_scene]), json!([w["name"], w["bank"], w["bytes"], w["world"], w["firstScene"]]));
        let model = c.model.as_ref().unwrap();
        assert_eq!(*model, ModelRef::Scene { scene: w["firstScene"].as_str().unwrap().into(), id: id.clone() });
        let built = b.model(model).unwrap();
        for t in built.texture_ids() {
            let image = b.texture(model, t).unwrap();
            assert_eq!(image.rgba.len(), (image.width * image.height * 4) as usize);
        }
    }
    println!("the_catalog_matches_the_reference: {} models", want.len());
}

#[test]
fn ids_match_the_reference() {
    let Some(e) = env("ids_match_the_reference") else { return };
    let loaded = json_file(&e.reference.join("loaded_ids.json"));
    assert_eq!(loaded.as_object().unwrap().len(), 48);
    for s in gamedata::scenes() {
        assert_eq!(hex_set(e.game.loaded_ids(&s.id).unwrap()), string_set(&loaded[&s.id]), "loaded {}", s.id);
    }
    let tex = json_file(&e.reference.join("texture_ids.json"));
    let banks: Vec<&str> = e.game.all_texture_banks().collect();
    assert_eq!(banks, tex.as_object().unwrap().keys().map(|k| k.as_str()).collect::<BTreeSet<_>>().into_iter().collect::<Vec<_>>());
    for b in &banks {
        assert_eq!(hex_set(e.game.texture_ids(b).iter().copied()), string_set(&tex[*b]), "{b}");
    }
    let objtex = json_file(&e.reference.join("object_texture_ids.json"));
    let objtex = objtex.as_object().unwrap();
    assert_eq!(objtex.len(), e.game.catalog().len());
    for (id, want) in objtex {
        let got = e.game.object_texture_ids(parse_id(id).unwrap()).unwrap();
        assert_eq!(hex_set(got.iter().copied()), string_set(want), "{id}");
    }
    assert!(e.game.object_texture_ids(0xDEADBEEF).is_none());
    // The bank numbers are compiled in
    assert_eq!(*glovebox_core::tables::bank_numbers(), json_file(&e.reference.join("bank_numbers.json")));
    println!("ids_match_the_reference: 48 levels, {} texture banks, {} models", banks.len(), objtex.len());
}

#[test]
fn enemy_templates_match_the_reference() {
    let Some(e) = env("enemy_templates_match_the_reference") else { return };
    let want = json_file(&e.refhome.join("web/enemy_templates.json"));
    let got = e.game.enemy_templates_json();
    assert_eq!(got, want);
    // The same order of types as in the file
    assert_eq!(got.as_object().unwrap().keys().collect::<Vec<_>>(), want.as_object().unwrap().keys().collect::<Vec<_>>());
    assert_eq!(backend(e).templates(), &want);
    println!("enemy_templates_match_the_reference: {} types", want.as_object().unwrap().len());
}

#[test]
fn backdrops_models_and_faces_match_the_reference() {
    let Some(e) = env("backdrops_models_and_faces_match_the_reference") else { return };
    let misc = json_file(&e.reference.join("gamedata/misc.json"));
    let b = backend(e);
    let mut models = 0;
    for s in gamedata::scenes() {
        assert_eq!(b.backdrops(&s.id).unwrap(), strings(&misc["backdrops"][&s.id]), "backdrops {}", s.id);
        let got = b.models(&s.id).unwrap();
        let ids: Vec<String> = got.models.keys().cloned().collect();
        assert_eq!(ids, strings(&misc["models"][&s.id]), "models {}", s.id);
        assert_eq!(got.loaded, b.loaded_ids(&s.id).unwrap().into_iter().collect::<Vec<_>>());
        models += ids.len();
        // Every model is there, and every texture it names that the level has
        for (id, r) in got.models.iter().step_by(15) {
            let model = b.model(r).unwrap_or_else(|err| panic!("{} {id}: {err}", s.id));
            for t in model.texture_ids() {
                assert!(b.texture(r, t).is_ok(), "{} {id} texture {}", s.id, hex(t));
            }
        }
    }
    let faces = misc["faces"].as_object().unwrap();
    for (id, n) in faces {
        assert_eq!(b.faces(id) as u64, n.as_u64().unwrap(), "{id}");
    }
    assert_eq!(b.faces("0xDEADBEEF"), 0);
    assert_eq!(b.faces("nonsense"), 0);
    let info = b.mod_info();
    assert_eq!(json!({"heapExtraBytes": info.heap_extra_bytes, "modBytes": info.mod_bytes}), misc["modInfo"]);
    assert_eq!(b.scenes().as_array().unwrap().len(), 48);
    println!("backdrops_models_and_faces_match_the_reference: {models} models in 48 levels, {} face counts", faces.len());
}

/// Compares one outcome of the reference data with a plan or an error.
fn same_plan(what: &str, got: Result<Plan, String>, want: &Value) {
    match (got, want.get("plan")) {
        (Ok(plan), Some(theirs)) => {
            assert_eq!(&plan.to_json(), theirs, "{what}");
            let order = |list: &[(String, String)]| json!(list.iter().map(|(t, b)| json!([t, b])).collect::<Vec<_>>());
            assert_eq!(order(&plan.textures), want["order"], "{what}: the order of the textures");
            match &plan.copy {
                Some(c) => assert_eq!(order(&c.textures), want["copyOrder"], "{what}"),
                None => assert!(want["copyOrder"].is_null(), "{what}"),
            }
        }
        (Err(message), None) => match want.get("error") {
            Some(theirs) => assert_eq!(json!(message), *theirs, "{what}"),
            None => assert!(want.get("crash").is_some(), "{what}"),
        },
        (Ok(_), None) => panic!("{what}: the reference refuses with {want}"),
        (Err(message), Some(_)) => panic!("{what}: refused with {message:?}, the reference has a plan"),
    }
}

#[test]
fn plans_match_the_reference() {
    let Some(e) = env("plans_match_the_reference") else { return };
    let cases = json_file(&e.reference.join("gamedata/plans.json"));
    let cases = cases.as_array().unwrap();
    assert!(cases.len() >= 480);
    let none = Customs::default();
    let b = backend(e);
    let (mut plans, mut refused) = (0, 0);
    for c in cases {
        let sid = c["sid"].as_str().unwrap();
        let ids = strings(&c["imports"]);
        let what = format!("{sid} {ids:?}");
        same_plan(&format!("plan_imports {what}"), imports::plan_imports(&e.game, &none, sid, &ids), &c["plan_imports"]);
        same_plan(&format!("plan_runtime {what}"), imports::plan_runtime(&e.game, &none, sid, &ids), &c["plan_runtime"]);
        same_plan(&format!("plan {what}"), b.plan(sid, &ids).map_err(|err| err.0), &c["backend"]);
        match c["plan_runtime"].get("plan") {
            Some(_) => plans += 1,
            None => refused += 1,
        }
    }
    assert!(plans > 300 && refused > 40, "{plans} {refused}");
    println!("plans_match_the_reference: {} import lists, {plans} planned, {refused} refused", cases.len());
}

fn info(v: &Value) -> Option<Info> {
    v.as_object()?;
    Some(Info {
        name: v["name"].as_str().map(String::from),
        song: v["song"].as_i64(),
        bonus_time: v["bonus_time"].as_i64(),
    })
}

/// The `created` time in a patch's metadata.
fn created_of(patch: &[u8]) -> Option<String> {
    let meta: Value = serde_json::from_slice(bps::metadata_of(patch)?).ok()?;
    meta["created"].as_str().map(String::from)
}

fn run(b: &dyn Backend, e: &Env, c: &Value, created: Option<String>) -> Result<Built, String> {
    let xml = |name: &Value| String::from_utf8(read(&e.reference.join("gamedata/xml").join(name.as_str().unwrap()))).unwrap();
    let kw = &c["kw"];
    let flag = |key: &str| kw[key].as_bool().unwrap_or(false);
    let format = c["fmt"].as_str().unwrap().to_string();
    let built = if c["call"] == "build" {
        b.build(&BuildRequest {
            sid: c["sid"].as_str().unwrap().into(),
            xml: xml(&c["xml"]),
            format,
            quickboot: flag("quickboot"),
            imports: strings(&kw["imports"]),
            expansion: flag("expansion"),
            copy: flag("copy"),
            info: info(&kw["info"]),
            created,
        })
    } else {
        let entry = |v: &Value| BuildAllEntry {
            sid: v["sid"].as_str().unwrap().into(),
            xml: xml(&v["xml"]),
            imports: strings(&v["imports"]),
            info: info(&v["info"]),
            extra: v["extra"].as_bool().unwrap_or(false),
        };
        b.build_all(&BuildAllRequest {
            entries: c["entries"].as_array().unwrap().iter().map(entry).collect(),
            format,
            quickboot: kw["quickboot"].as_str().map(String::from),
            expansion: flag("expansion"),
            copy: flag("copy"),
            created,
        })
    };
    built.map_err(|err| err.0)
}

/// The facts of a build as the reference dumps hold them: a JSON object of headers.
fn headers(built: &Built, several: bool) -> Value {
    let mut h = json!({"X-Needs-Mod": if built.needs_mod { "1" } else { "0" }, "filename": built.filename});
    if let Some(n) = built.lev_size {
        h["X-Lev-Size"] = json!(n.to_string());
    }
    if let Some(i) = &built.imports {
        h["X-Imports"] = json!({"objects": i.objects, "textures": i.textures, "objectBytes": i.object_bytes,
                                "texBytes": i.tex_bytes, "bank": i.bank, "texBank": i.tex_bank, "runtime": i.runtime});
    }
    if let Some(n) = built.patch_size {
        h["X-Patch-Size"] = json!(n.to_string());
    }
    if several {
        h["levels"] = json!(built.levels);
        h["extra"] = json!(built.extra);
        h["objects"] = json!(built.objects);
    }
    h
}

/// Runs the builds of a reference file and compares each with the recorded output. Returns the numbers of ROMs,
/// patches, level files and refused builds, and the slowest build.
fn check_builds(b: &dyn Backend, e: &Env, cases: &[Value], prefix: &str) -> (usize, usize, usize, usize, (f64, String)) {
    let (mut roms, mut patches, mut levs, mut refused) = (0, 0, 0, 0);
    let mut slowest = (0.0, String::new());
    for c in cases {
        let name = c["name"].as_str().unwrap();
        assert!(c.get("crash").is_none(), "{name}: the reference build crashed");
        let Some(sha1) = c["sha1"].as_str() else {
            let message = run(b, e, c, None).err().unwrap_or_else(|| panic!("{name}: built, the reference refuses"));
            assert_eq!(json!(message), c["error"], "{name}");
            refused += 1;
            continue;
        };
        let theirs = read(&e.reference.join("gamedata/out").join(format!("{prefix}{name}")));
        assert_eq!(n64::sha1_hex(&theirs), sha1);
        let format = c["fmt"].as_str().unwrap();
        let created = if format == "bps" { created_of(&theirs) } else { None };
        assert_eq!(created.is_some(), format == "bps");
        let t = Instant::now();
        let built = run(b, e, c, created).unwrap_or_else(|err| panic!("{name}: {err}"));
        let took = t.elapsed().as_secs_f64();
        if took > slowest.0 {
            slowest = (took, name.to_string());
        }
        if built.data != theirs {
            let at = built.data.iter().zip(&theirs).position(|(a, b)| a != b);
            panic!("{name}: {} bytes, the reference {} bytes, first difference at {:x?}", built.data.len(), theirs.len(), at);
        }
        assert_eq!(headers(&built, c["call"] == "build_all"), c["headers"], "{name}");
        match format {
            "rom" => {
                assert_eq!(n64::header_crc(&built.data).unwrap(), n64::stored_crc(&built.data).unwrap(), "{name}");
                if built.needs_mod {
                    assert_eq!(glovebox_core::hwcheck::check_rom(&built.data), Vec::<String>::new(), "{name}");
                }
                roms += 1
            }
            "bps" => {
                // The patch makes a ROM from the clean one
                let rom = bps::apply(e.game.rom_bytes(), &built.data).unwrap();
                let meta: Value = serde_json::from_slice(bps::metadata_of(&built.data).unwrap()).unwrap();
                assert_eq!(json!(n64::sha1_hex(&rom)), meta["targetSha1"], "{name}");
                patches += 1
            }
            _ => levs += 1,
        }
    }
    (roms, patches, levs, refused, slowest)
}

#[test]
fn builds_match_the_reference() {
    let Some(e) = env("builds_match_the_reference") else { return };
    let cases = json_file(&e.reference.join("gamedata/builds.json"));
    let cases = cases.as_array().unwrap();
    let b = backend(e);
    let (roms, patches, levs, refused, slowest) = check_builds(b.as_ref(), e, cases, "");
    assert!(roms + patches >= 25 && patches >= 8 && refused >= 10, "{roms} {patches} {refused}");
    // A patch made now carries the time of now
    let first = cases.iter().find(|c| c["fmt"] == "bps" && c.get("sha1").is_some()).unwrap();
    let now = created_of(&run(b.as_ref(), e, first, None).unwrap().data).unwrap();
    assert!(now.len() == 20 && now.ends_with('Z') && now.as_str() > "2026-01-01", "{now}");
    println!(
        "builds_match_the_reference: {roms} ROMs, {patches} patches and {levs} level files equal; {refused} refused with the \
         same message; slowest {:.2}s ({})",
        slowest.0, slowest.1
    );
}

/// Imported models. The reference data has three in `gamedata/pyhome/custom`: a kit model and two reskinned models.
#[test]
fn imported_models_match_the_reference() {
    let Some(e) = env("imported_models_match_the_reference") else { return };
    let want = json_file(&e.reference.join("gamedata/custom.json"));
    let home = e.reference.join("gamedata/pyhome");
    let b: Arc<dyn Backend> = Arc::new(GameBackend::new(e.game.clone(), &home, None));
    // The catalog's entries
    let cat = b.catalog().unwrap();
    let theirs = want["catalog"].as_object().unwrap();
    assert_eq!(cat.len(), e.game.catalog().len() + theirs.len());
    for (id, w) in theirs {
        let c = &cat[id];
        let x = c.custom.as_ref().unwrap();
        let mine = json!({"name": c.name, "bank": c.bank, "bytes": c.bytes, "custom": x.set, "piece": x.piece,
                          "offset": x.offset, "textures": x.textures, "texBank": x.tex_bank, "reskinFrom": x.reskin_from});
        // The reference's reskinned models have whole numbers as offsets
        let mut w = w.clone();
        w["offset"] = json!(w["offset"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect::<Vec<_>>());
        assert_eq!(mine, w, "{id}");
        assert_eq!(json!(b.faces(id)), want["faces"][id], "{id}");
        // The model and its textures are there to draw
        let model = b.model(c.model.as_ref().unwrap()).unwrap();
        assert!(model.triangles() > 0, "{id}");
        for t in model.texture_ids() {
            assert!(x.textures.contains(&hex(t)), "{id} {}", hex(t));
            assert!(b.texture(c.model.as_ref().unwrap(), t).is_ok(), "{id} {}", hex(t));
        }
    }
    // Plans and builds that import them
    let customs = imports::load_customs(&home);
    assert_eq!(customs.models.len(), theirs.len());
    let plans = want["plans"].as_array().unwrap();
    for c in plans {
        let sid = c["sid"].as_str().unwrap();
        let ids = strings(&c["imports"]);
        let what = format!("{sid} {ids:?}");
        same_plan(&format!("plan_imports {what}"), imports::plan_imports(&e.game, &customs, sid, &ids), &c["plan_imports"]);
        same_plan(&format!("plan_runtime {what}"), imports::plan_runtime(&e.game, &customs, sid, &ids), &c["plan_runtime"]);
        same_plan(&format!("plan {what}"), b.plan(sid, &ids).map_err(|err| err.0), &c["backend"]);
    }
    let (roms, patches, _, refused, _) = check_builds(b.as_ref(), e, want["builds"].as_array().unwrap(), "custom-");
    assert!(roms >= 4 && patches >= 2 && refused >= 1);

    // Making the same models again in an empty folder gives the reference's files
    let fresh = Path::new(env!("CARGO_TARGET_TMPDIR")).join("gamedata-custom-home");
    let _ = std::fs::remove_dir_all(&fresh);
    let maker = GameBackend::new(e.game.clone(), &fresh, None);
    // The reference's bank of a model was cut out with libgarib's `objbank-tool split`, which orders the parts of a
    // bank differently from run to run. Packed again, it's the bank made here.
    let same_files = |set: &str, id: &str| {
        let file = |home: &Path, name: &str| read(&home.join("custom").join(set).join(name));
        let (mine, theirs) = (file(&fresh, &format!("{id}.obj.bin")), file(&home, &format!("{id}.obj.bin")));
        let none: [&[u8]; 0] = [];
        assert_eq!(mine.len(), theirs.len(), "{set}/{id}");
        assert!(mine == glovebox_core::objbank::pack(&theirs, &none).unwrap(), "{set}/{id}.obj.bin differs");
        assert!(file(&fresh, "textures.tex.bin") == file(&home, "textures.tex.bin"), "{set}/textures.tex.bin differs");
    };
    // A kit model is converted by `custom` from the glTF file `kit` writes. Those bytes aren't the reference's, so
    // the set and its ids differ. The model itself is the same.
    let made = maker.make_kit(want["kit"]["item"].as_str().unwrap(), &json!({})).unwrap();
    for key in ["name", "scale", "triangles", "size"] {
        assert_eq!(made[key], want["kit"]["made"][key], "{key}");
    }
    let kit_ids: Vec<String> = made["pieces"].as_array().unwrap().iter().map(|p| p["id"].as_str().unwrap().to_string()).collect();
    assert_eq!(kit_ids.len(), strings(&want["kit"]["ids"]).len());
    let request = BuildRequest {
        sid: "0a".into(),
        xml: maker.level_xml("0a").unwrap(),
        format: "rom".into(),
        imports: kit_ids.clone(),
        expansion: true,
        ..BuildRequest::default()
    };
    let built = maker.build(&request).unwrap();
    assert_eq!(built.imports.as_ref().unwrap().objects, kit_ids.len());
    // The console's rules hold for a ROM with the game mod (`hwcheck`)
    assert_eq!(glovebox_core::hwcheck::check_rom(&built.data), Vec::<String>::new());
    for id in strings(&want["reskins"]) {
        let x = cat[&id].custom.as_ref().unwrap();
        let meta = json_file(&home.join("custom").join(&x.set).join("meta.json"));
        let r = &meta["reskin"];
        let got = maker.reskin(r["from"].as_str().unwrap(), &strings(&r["worlds"]), r["seed"].as_i64().unwrap()).unwrap();
        assert_eq!(got, id);
        same_files(&x.set, &id);
    }
    assert_eq!(maker.catalog().unwrap().len(), cat.len());
    let set = made["set"].as_str().unwrap();
    maker.delete_custom(set).unwrap();
    assert!(!fresh.join("custom").join(set).exists());
    assert_eq!(maker.catalog().unwrap().len(), cat.len() - strings(&want["kit"]["ids"]).len());
    assert!(!maker.kit_items().unwrap().as_array().unwrap().is_empty());
    let png = maker.kit_preview(want["kit"]["item"].as_str().unwrap(), &json!({})).unwrap();
    assert_eq!(&png[..4], b"\x89PNG");
    let _ = std::fs::remove_dir_all(&fresh);
    println!(
        "imported_models_match_the_reference: {} pieces, {} plans, {roms} ROMs and {patches} patches equal, {refused} refused; \
         {} reskinned models made again are the same files; a kit model made here builds into a ROM",
        theirs.len(),
        plans.len() * 3,
        strings(&want["reskins"]).len()
    );
}

#[test]
fn the_back_end_refuses_what_it_can_not_do() {
    let Some(e) = env("the_back_end_refuses_what_it_can_not_do") else { return };
    let b = backend(e);
    assert_eq!(b.validate("<Lvl/>"), vec!["Root element must be <Level>".to_string()]);
    assert!(b.validate(&b.level_xml("0a").unwrap()).is_empty());
    assert_eq!(b.delete_custom("../../etc").unwrap_err().0, "no such model");
    assert!(b.delete_custom("0123456789").is_ok());
    assert!(b.model(&ModelRef::Scene { scene: "0a".into(), id: "0xDEADBEEF".into() }).is_err());
    assert!(b.model(&ModelRef::Custom { set: "0123456789".into(), id: "0x00000001".into() }).is_err());
    assert!(b.texture(&ModelRef::Scene { scene: "0a".into(), id: "0x00000001".into() }, 0xDEADBEEF).is_err());
    assert!(b.plan("zz", &[]).is_err());
    assert_eq!(b.rom_path(), Some(PathBuf::from("rom.z64")));
}

#[test]
fn timings() {
    let Some(e) = env("timings") else { return };
    let rom = e.game.rom_bytes().to_vec();
    let t = Instant::now();
    let game = GameData::open(rom).unwrap();
    let open = t.elapsed();
    let t = Instant::now();
    let models = game.catalog().len();
    let catalog = t.elapsed();
    let t = Instant::now();
    let ids = game.catalog().keys().filter_map(|id| game.object_texture_ids(*id)).map(|s| s.len()).sum::<usize>();
    let object_textures = t.elapsed();
    let t = Instant::now();
    let templates = game.enemy_templates().len();
    let enemy_templates = t.elapsed();
    let t = Instant::now();
    let drawn = game.model_index("0a").unwrap().len();
    let model_index = t.elapsed();
    println!(
        "timings: open {open:?}; catalog {catalog:?} ({models} models); object_texture_ids of all {object_textures:?} \
         ({ids} ids); enemy_templates {enemy_templates:?} ({templates} types); model_index of one level \
         {model_index:?} ({drawn} models)"
    );
}
