//! A stand-in for the editor's back end that needs no ROM: two made-up levels and a few made-up models.
//!
//! A port of `tools/desktop_fixture.py`. The tests run the editor against it, and the app can show it with
//! `--fixture`. It holds no game data. The ground is a made-up hilly square, the blocks are cubes, and the grave,
//! the pumpkin and the sign come from the kit. Building a .lev works. Building a ROM or a patch needs the ROM and
//! says so.

use glovebox_core::backend::{Backend, BackendError, CatalogEntry, CustomEntry, ModInfo, ModelRef, Result, SceneModels};
use glovebox_core::build::{self, BuildAllRequest, BuildRequest, Built};
use glovebox_core::custom;
use glovebox_core::gamedata::Image;
use glovebox_core::imports::{CopyPlan, Plan, Runtime};
use glovebox_core::kit;
use glovebox_core::level;
use glovebox_core::model::{Material, Mesh, Model, Node, Primitive, Wrap};
use glovebox_core::tables;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub const GROUND: &str = "0x1E5700A1";
pub const BLOCK: &str = "0x1E5700A2";
pub const GRAVE: &str = "0x1E5700A3";
pub const PUMPKIN: &str = "0x1E5700A4";
pub const SIGN: &str = "0x1E5700A5";
pub const BALL: &str = "0x8E4DDE49";
pub const ENEMY: &str = "0x1E5700A6";

const SCENE: &str = "FIX";

fn scene(sid: &str, name: &str, group: &str) -> Value {
    json!({
        "id": sid, "index": i64::from_str_radix(sid, 16).unwrap_or(0), "name": name, "group": group,
        "landscape": format!("{sid}.FIX{sid}lnd.n64.lev"),
        "objectBanks": ["FIXTURE_BANK.obj.fla"], "textureBanks": ["FIXTURE_TEX_BANK.tex.fla"],
        "gameObjectBanks": ["FIXTURE_BANK"], "gameTextureBanks": ["FIXTURE_TEX_BANK"],
        "sureObjectBanks": ["FIXTURE_BANK.obj.fla"], "sureTextureBanks": ["FIXTURE_TEX_BANK.tex.fla"],
        "importObjectBank": "FIXTURE_BANK", "importTextureBank": "FIXTURE_TEX_BANK",
    })
}

/// The first made-up level: one of every kind of entity.
pub fn level_1() -> String {
    let text = r#"<Level name="FIX10.lnd" libgarib-version="0.0" data-format-version="1">
  <FogConfiguration fog_enabled="1" r="70" g="110" b="150" fog_distance="900" near_clip="1500"/>
  <AmbientLight r="120" g="120" b="130"/>
  <LandActor object_id="{ground}" name="ground" x="0.0" y="0.0" z="0.0"/>
  <LandActor object_id="{grave}" name="grave" x="150.0" y="40.0" z="-120.0">
    <SetActorRotation x="0.0" y="0.6" z="0.0"/>
  </LandActor>
  <BackgroundActor object_id="{pumpkin}" name="pumpkin" x="-200.0" y="45.0" z="80.0"/>
  <Platform object_id="{block}" name="lift">
    <PlatSetTag tag="7"/>
    <PlatScale x="0.2" y="0.1" z="0.2"/>
    <PlatPathPoint duration="60" x="300.0" y="80.0" z="200.0"/>
    <PlatPathPoint duration="60" x="300.0" y="260.0" z="200.0"/>
    <PlatCheckpoint u16_0x17="0" theta="1.2"/>
  </Platform>
  <Platform object_id="{block}" name="spinner">
    <PlatSetInitialPos x="-320.0" y="90.0" z="-260.0"/>
    <PlatScale x="0.3" y="0.05" z="0.3"/>
    <PlatConstantSpin axis="y" initial_theta="0.0" speed="0.05"/>
  </Platform>
  <GaribGroup group_id="0" initial_state="1">
    <Garib x="0.0" y="80.0" z="0.0" type="garib" dynamic_shadow="0"/>
    <Garib x="60.0" y="80.0" z="0.0" type="garib" dynamic_shadow="0"/>
    <Garib x="120.0" y="80.0" z="0.0" type="extra_life" dynamic_shadow="0"/>
  </GaribGroup>
  <Enemy type="dennis" puzzle_tag="0" x="-120.0" y="60.0" z="-80.0" y_rotation="0.0">
    <EnemyFinalize/>
  </Enemy>
  <Powerup type="7" u16_0x02="300" u16_0x04="300" x="200.0" y="70.0" z="40.0"/>
  <MrTip x="-60.0" y="90.0" z="160.0" message_id="1"/>
  <GloverSpawnPoint x="0.0" y="70.0" z="250.0"/>
  <BallSpawnPoint type="0" x="30.0" y="70.0" z="250.0"/>
  <Wind left="-400.0" top="40.0" front="300.0" width="150.0" height="120.0" depth="150.0" vel_x="0.0" vel_y="2.0" vel_z="0.0" turbulence="0.0" unknown_0x2c="0" active="1" tag="0"/>
  <Buzzer tag="0" platform_1_tag="0" platform_2_tag="0" draw_flags="0" r="255" g="255" b="0" color_jitter="0" end_1_x="-450.0" end_1_y="60.0" end_1_z="-100.0" end_2_x="-450.0" end_2_y="60.0" end_2_z="100.0" draw_diameter="10.0" draw_thickness="2.0"/>
  <Puzzle>
    <PuzzleCond cond_type="35">
      <PuzzleCondGloverWithinVolume x="-100.0" y="0.0" z="350.0" l="300.0" w="400.0" h="300.0"/>
    </PuzzleCond>
    <PuzzleAction action_type="74">
      <PuzzleActionCameraLookAtGlover angle="3.1416" distance="2.0" activation_delay="0"/>
    </PuzzleAction>
  </Puzzle>
  <Puzzle>
    <PuzzleCond cond_type="38">
      <PuzzleCondBallWithinRangeOfPoint x="400.0" y="60.0" z="-300.0" range="80.0"/>
    </PuzzleCond>
    <PuzzleCond cond_type="41">
      <PuzzleCondGloverWithinRangeOfPoint2 x="420.0" z="-150.0" range="60.0" max_y="160.0" min_y="20.0"/>
    </PuzzleCond>
  </Puzzle>
  <AmbientSoundAtPoint sound_id="12" volume="100" flags="0" h_0x06="0" h_0x08="0" h_0x0a="0" platform_tag="0" puzzle_tag="0" x="250.0" y="60.0" z="-250.0" radius="300.0"/>
  <EndLevelData/>
</Level>
"#;
    text.replace("{ground}", GROUND).replace("{grave}", GRAVE).replace("{pumpkin}", PUMPKIN).replace("{block}", BLOCK)
}

/// The second made-up level: a floor and a garib.
pub fn level_2() -> String {
    let text = r#"<Level name="FIX11.lnd" libgarib-version="0.0" data-format-version="1">
  <LandActor object_id="{ground}" name="ground" x="0.0" y="0.0" z="0.0"/>
  <GaribGroup group_id="0" initial_state="1">
    <Garib x="0.0" y="80.0" z="0.0" type="garib" dynamic_shadow="0"/>
  </GaribGroup>
  <EndLevelData/>
</Level>
"#;
    text.replace("{ground}", GROUND)
}

/// A model and the pictures of its textures. Texture ids count from 1.
#[derive(Clone)]
struct Made {
    model: Arc<Model>,
    textures: Vec<Arc<Image>>,
}

fn one_mesh(positions: Vec<[f32; 3]>, uvs: Vec<[f32; 2]>, material: Material, scale: f64, textures: Vec<Arc<Image>>) -> Made {
    let indices = (0..positions.len() as u32).collect();
    let prim = Primitive { material: 0, positions, uvs, indices, ..Primitive::default() };
    let mut node = Node::new("model".to_string());
    node.mesh = Some(0);
    node.scale = [scale; 3];
    let mesh = Mesh { name: "model".to_string(), primitives: vec![prim], billboard: false };
    Made { model: Arc::new(Model { nodes: vec![node], meshes: vec![mesh], materials: vec![material] }), textures }
}

fn plain(color: [f32; 3], texture: Option<u32>) -> Material {
    Material {
        texture,
        base_color: [color[0], color[1], color[2], 1.0],
        alpha_mode: Default::default(),
        alpha_cutoff: 0.5,
        double_sided: false,
        unlit: false,
        wrap_s: Wrap::Repeat,
        wrap_t: Wrap::Repeat,
    }
}

/// A hilly square of `cells` by `cells` squares, 1200 across, with a chequered texture.
fn ground(cells: usize) -> Made {
    let size = 1200.0;
    let at = |r: usize, c: usize| {
        let x = -size / 2.0 + size * c as f64 / cells as f64;
        let z = -size / 2.0 + size * r as f64 / cells as f64;
        let y = 60.0 * (x / 150.0).sin() * (z / 190.0).cos();
        ([x as f32, y as f32, z as f32], [(8.0 * c as f64 / cells as f64) as f32, (8.0 * r as f64 / cells as f64) as f32])
    };
    let (mut pos, mut uv) = (Vec::new(), Vec::new());
    for r in 0..cells {
        for c in 0..cells {
            let (a, b, d, e) = (at(r, c), at(r, c + 1), at(r + 1, c), at(r + 1, c + 1));
            for v in [a, d, b, b, d, e] {
                pos.push(v.0);
                uv.push(v.1);
            }
        }
    }
    let mut rgba = Vec::with_capacity(256 * 256 * 4);
    for y in 0..256 {
        for x in 0..256 {
            let c: [u8; 3] = if (x / 32 + y / 32) % 2 == 1 { [230, 200, 90] } else { [60, 140, 70] };
            rgba.extend_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
    let checker = Arc::new(Image { width: 256, height: 256, rgba });
    one_mesh(pos, uv, plain([1.0, 1.0, 1.0], Some(1)), 1.0, vec![checker])
}

/// A cube 100 across in one colour.
fn cube(color: [f32; 3]) -> Made {
    let mut pos = Vec::new();
    for axis in 0..3 {
        for sign in [-1.0f32, 1.0] {
            let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
            let corner = |du: f32, dv: f32| {
                let mut p = [0.0f32; 3];
                p[axis] = sign * 0.5;
                p[u] = du * 0.5;
                p[v] = dv * 0.5;
                p
            };
            let q = [corner(-1.0, -1.0), corner(1.0, -1.0), corner(1.0, 1.0), corner(-1.0, 1.0)];
            let order: [usize; 6] = if sign > 0.0 { [0, 1, 2, 0, 2, 3] } else { [3, 2, 0, 2, 1, 0] };
            pos.extend(order.iter().map(|i| q[*i]));
        }
    }
    let uv = vec![[0.0, 0.0]; pos.len()];
    one_mesh(pos, uv, plain(color, None), 100.0, Vec::new())
}

/// A kit model as it is: one mesh part for each of its textures.
fn from_soup(soup: &kit::Soup) -> Made {
    let mut prims: Vec<Primitive> = (0..soup.mats.len()).map(|m| Primitive { material: m as u32, ..Primitive::default() }).collect();
    for t in &soup.tris {
        let Some(p) = prims.get_mut(t.mat) else { continue };
        for k in 0..3 {
            p.positions.push(t.p[k].map(|v| v as f32));
            p.normals.push(t.n[k].map(|v| v as f32));
            p.uvs.push([t.uv[k][0] as f32, t.uv[k][1] as f32]);
            p.indices.push(p.indices.len() as u32);
        }
    }
    let mut materials = Vec::new();
    let mut textures = Vec::new();
    for (i, im) in soup.mats.iter().enumerate() {
        let mut m = plain([1.0, 1.0, 1.0], Some(i as u32 + 1));
        if soup.clamp.get(i).copied().unwrap_or(false) {
            (m.wrap_s, m.wrap_t) = (Wrap::Clamp, Wrap::Clamp);
        }
        materials.push(m);
        let rgba = im.px.iter().flat_map(|p| p.iter().copied()).collect();
        textures.push(Arc::new(Image { width: im.w as u32, height: im.h as u32, rgba }));
    }
    let mut node = Node::new("model".to_string());
    node.mesh = Some(0);
    prims.retain(|p| !p.positions.is_empty());
    let mesh = Mesh { name: "model".to_string(), primitives: prims, billboard: false };
    Made { model: Arc::new(Model { nodes: vec![node], meshes: vec![mesh], materials }), textures }
}

fn kit_model(item: &str, params: &[(&str, &str)]) -> Made {
    let p: kit::Params = params.iter().map(|(k, v)| (k.to_string(), Value::from(*v))).collect();
    match kit::clean(item, &p).and_then(|p| kit::build(item, &p)) {
        Ok(soup) => from_soup(&soup),
        Err(_) => cube([0.6, 0.6, 0.6]),
    }
}

fn entry(name: &str, bank: &str, bytes: u64, world: &str, id: &str) -> CatalogEntry {
    CatalogEntry {
        name: name.to_string(),
        bank: bank.to_string(),
        bytes,
        world: Some(world.to_string()),
        first_scene: Some("10".to_string()),
        model: Some(ModelRef::Scene { scene: SCENE.to_string(), id: id.to_string() }),
        custom: None,
    }
}

/// The back end without a ROM.
pub struct FixtureBackend {
    scenes: Value,
    tables: Value,
    templates: Value,
    heap: Value,
    models: BTreeMap<String, Made>,
    cat: BTreeMap<String, CatalogEntry>,
    custom: Mutex<BTreeMap<String, (CatalogEntry, Made)>>,
}

impl Default for FixtureBackend {
    fn default() -> FixtureBackend {
        FixtureBackend::new()
    }
}

impl FixtureBackend {
    pub fn new() -> FixtureBackend {
        let mut models = BTreeMap::new();
        models.insert(GROUND.to_string(), ground(24));
        models.insert(BLOCK.to_string(), cube([0.3, 0.5, 0.9]));
        models.insert(ENEMY.to_string(), cube([0.9, 0.3, 0.3]));
        models.insert(BALL.to_string(), cube([0.8, 0.8, 0.8]));
        models.insert(GRAVE.to_string(), kit_model("grave", &[("text", "RIP/TEST")]));
        models.insert(PUMPKIN.to_string(), kit_model("pumpkin", &[]));
        models.insert(SIGN.to_string(), kit_model("sign", &[]));
        let mut cat = BTreeMap::new();
        for (id, name, bank, bytes, world) in [
            (GROUND, "ground", "FIXTURE_BANK", 40000, "FIXTURE"),
            (BLOCK, "block", "FIXTURE_BANK", 900, "FIXTURE"),
            (GRAVE, "grave", "FIXTURE_BANK", 3000, "FIXTURE"),
            (PUMPKIN, "pumpkin", "FIXTURE_BANK", 3000, "FIXTURE"),
            (ENEMY, "dennis", "FIXTURE_BANK", 5000, "FIXTURE"),
            (BALL, "ball", "FIXTURE_BANK", 800, "FIXTURE"),
            (SIGN, "sign", "OTHER_BANK", 2500, "OTHER"),
        ] {
            cat.insert(id.to_string(), entry(name, bank, bytes, world, id));
        }
        // The enemy "dennis" uses the fixture's enemy model
        let mut tables = tables::noclip_tables().clone();
        let types = tables::decomp().get("ksyEnums").and_then(|e| e.get("enemy::enemy_type")).and_then(|e| e.as_object());
        let dennis = types.and_then(|t| t.iter().find(|(_, v)| v.as_str() == Some("dennis"))).and_then(|(k, _)| k.parse::<usize>().ok());
        if let (Some(tv), Some(id)) = (dennis, u32::from_str_radix(&ENEMY[2..], 16).ok()) {
            if let Some(slot) = tables.get_mut("enemyObjects").and_then(|o| o.get_mut(tv)).and_then(|o| o.as_object_mut()) {
                slot.insert("value".to_string(), Value::from(id));
            }
        }
        FixtureBackend {
            scenes: Value::Array(vec![scene("10", "Fixture 1", "Atlantis"), scene("11", "Fixture 2", "Hub")]),
            tables,
            templates: json!({}),
            heap: json!({"10": {"free": 200 * 1024}, "11": {"free": 120 * 1024}}),
            models,
            cat,
            custom: Mutex::new(BTreeMap::new()),
        }
    }

    /// The ids of the models both levels load.
    fn level_ids(&self) -> Vec<String> {
        [GROUND, BLOCK, GRAVE, PUMPKIN, BALL, ENEMY].iter().map(|s| s.to_string()).collect()
    }

    fn customs(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, (CatalogEntry, Made)>> {
        self.custom.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn made(&self, model: &ModelRef) -> Option<Made> {
        match model {
            ModelRef::Scene { id, .. } => self.models.get(id).cloned(),
            ModelRef::Custom { id, .. } => self.customs().get(id).map(|c| c.1.clone()),
        }
    }

    fn add_custom(&self, set: &str, name: &str, bytes: u64, made: Made) -> Value {
        let n = u32::from_str_radix(set.get(..7).unwrap_or("0"), 16).unwrap_or(0);
        let oid = format!("0x7{:07X}", n & 0x0FFF_FFFF);
        let custom = CustomEntry {
            set: set.to_string(),
            piece: name.to_string(),
            offset: [0.0; 3],
            textures: Vec::new(),
            tex_bank: format!("custom:{set}"),
            reskin_from: None,
        };
        let e = CatalogEntry {
            name: name.to_string(),
            bank: format!("Custom: {name}"),
            bytes,
            world: None,
            first_scene: None,
            model: Some(ModelRef::Custom { set: set.to_string(), id: oid.clone() }),
            custom: Some(custom),
        };
        self.customs().insert(oid, (e, made));
        json!({"set": set, "name": name})
    }
}

fn no_rom<T>() -> Result<T> {
    Err(BackendError("The fixture has no ROM: only .lev builds work here.".to_string()))
}

impl Backend for FixtureBackend {
    fn scenes(&self) -> &Value {
        &self.scenes
    }

    fn decomp(&self) -> &Value {
        tables::decomp()
    }

    fn tables(&self) -> &Value {
        &self.tables
    }

    fn templates(&self) -> &Value {
        &self.templates
    }

    fn heap(&self) -> &Value {
        &self.heap
    }

    fn mod_info(&self) -> ModInfo {
        ModInfo { heap_extra_bytes: 4 << 20, mod_bytes: 5000 }
    }

    fn level_xml(&self, sid: &str) -> Result<String> {
        Ok(if sid == "10" { level_1() } else { level_2() })
    }

    fn backdrops(&self, _sid: &str) -> Result<Vec<String>> {
        Ok(Vec::new())
    }

    fn faces(&self, _oid: &str) -> u32 {
        0
    }

    fn models(&self, _sid: &str) -> Result<SceneModels> {
        let mut loaded = self.level_ids();
        loaded.sort();
        let models = loaded.iter().map(|i| (i.clone(), ModelRef::Scene { scene: SCENE.to_string(), id: i.clone() })).collect();
        Ok(SceneModels { models, loaded })
    }

    fn orig_lev_size(&self, _sid: &str) -> Option<usize> {
        Some(1234)
    }

    fn catalog(&self) -> Result<BTreeMap<String, CatalogEntry>> {
        let mut out = self.cat.clone();
        out.extend(self.customs().iter().map(|(k, v)| (k.clone(), v.0.clone())));
        Ok(out)
    }

    fn model(&self, model: &ModelRef) -> Result<Arc<Model>> {
        self.made(model).map(|m| m.model).ok_or_else(|| BackendError(format!("The fixture has no model {:?}", model)))
    }

    fn texture(&self, model: &ModelRef, texture_id: u32) -> Result<Arc<Image>> {
        let made = self.made(model);
        let image = made.and_then(|m| m.textures.get((texture_id as usize).wrapping_sub(1)).cloned());
        image.ok_or_else(|| BackendError(format!("The fixture has no texture {}", texture_id)))
    }

    fn validate(&self, xml: &str) -> Vec<String> {
        match level::parse_level_xml(xml.as_bytes()) {
            Ok(doc) => level::validate(&doc).into_iter().take(5).collect(),
            Err(e) => vec![e.0],
        }
    }

    fn plan(&self, sid: &str, ids: &[String]) -> Result<Plan> {
        let custom = self.customs();
        let bytes = |i: &String| self.cat.get(i).or(custom.get(i).map(|c| &c.0)).map(|c| c.bytes);
        let objects: Vec<String> = ids.iter().filter(|i| bytes(i).is_some()).cloned().collect();
        let (bank, tex_bank) = ("FIXTURE_BANK".to_string(), "FIXTURE_TEX_BANK".to_string());
        Ok(Plan {
            object_bytes: objects.iter().filter_map(bytes).sum(),
            objects,
            target_bank: bank.clone(),
            textures: Vec::new(),
            target_tex_bank: tex_bank.clone(),
            tex_bytes: Some(0),
            copy: Some(CopyPlan { target_bank: bank, target_tex_bank: tex_bank, ..CopyPlan::default() }),
            runtime: Some(Runtime {
                level: usize::from_str_radix(sid, 16).unwrap_or(0),
                object_banks: vec!["OTHER_BANK".to_string()],
                texture_banks: Vec::new(),
                object_numbers: vec![9],
                texture_numbers: Vec::new(),
                bytes: 30000,
            }),
        })
    }

    fn loaded_ids(&self, _sid: &str) -> Result<BTreeSet<String>> {
        Ok(self.level_ids().into_iter().collect())
    }

    fn build(&self, request: &BuildRequest) -> Result<Built> {
        if request.format != "lev" {
            return no_rom();
        }
        let (lev, _) = build::assemble(request.xml.as_bytes())?;
        let filename = format!("{0}.FIX{0}lnd.n64.lev", request.sid);
        Ok(Built { filename, lev_size: Some(lev.len()), needs_mod: false, data: lev, ..Built::default() })
    }

    fn build_all(&self, _request: &BuildAllRequest) -> Result<Built> {
        no_rom()
    }

    fn import_custom(&self, data: &[u8], name: &str, scale: f64) -> Result<Value> {
        let gltf = custom::gltf::Gltf::parse(data).map_err(|e| BackendError(e.0))?;
        let (tris, _) = gltf.triangles().map_err(|e| BackendError(e.0))?;
        let mut pos = Vec::with_capacity(tris.p.len() * 3);
        for t in &tris.p {
            pos.extend(t.iter().map(|p| p.map(|v| (v * scale) as f32)));
        }
        let uv = vec![[0.0, 0.0]; pos.len()];
        let made = one_mesh(pos, uv, plain([0.7, 0.7, 0.7], None), 1.0, Vec::new());
        Ok(self.add_custom(&custom::set_id(data, scale), name, data.len() as u64, made))
    }

    fn delete_custom(&self, set: &str) -> Result<()> {
        self.customs().retain(|_, c| c.0.custom.as_ref().is_none_or(|x| x.set != set));
        Ok(())
    }

    fn reskin(&self, _oid: &str, _worlds: &[String], _seed: i64) -> Result<String> {
        Err(BackendError("The fixture has no ROM: it can't make a model with other textures.".to_string()))
    }

    fn kit_items(&self) -> Result<Value> {
        Ok(kit::items())
    }

    fn kit_preview(&self, item: &str, params: &Value) -> Result<Vec<u8>> {
        let empty = kit::Params::new();
        let clean = kit::clean(item, params.as_object().unwrap_or(&empty))?;
        Ok(kit::preview_png(item, &clean, 256)?)
    }

    fn make_kit(&self, item: &str, params: &Value) -> Result<Value> {
        let empty = kit::Params::new();
        let clean = kit::clean(item, params.as_object().unwrap_or(&empty))?;
        let soup = kit::build(item, &clean)?;
        let name = kit::label(item, &clean)?;
        // A name for the set that is the same for the same model
        let key = format!("{item}:{}", Value::Object(clean));
        let set = custom::set_id(key.as_bytes(), 1.0);
        Ok(self.add_custom(&set, &name, soup.count() as u64 * 40, from_soup(&soup)))
    }

    fn rom_path(&self) -> Option<PathBuf> {
        None
    }
}
