//! Tests of the 3D view's renderer, without a window.
//!
//! The first tests draw made-up scenes and check pixels. The others replay scenes the Python editor drew
//! (the reference script `render/shots.py`, not in this repository) and compare the pictures. They need
//! `GLOVEBOX_ORACLE` (the folder of dumps); the level also needs `GLOVEBOX_PYHOME` (the Python app's folder with the
//! split ROM). Without them they print that they were skipped. Each comparison saves its pictures next to Python's,
//! as `<name>_rs.png` and `<name>_diff.png`.

use eframe::egui_wgpu::wgpu;
use glovebox::render::{icons::Icons, Fog, Frame, Renderer};
use glovebox_core::model::{self as gm, AlphaMode, Wrap};
use glovebox_core::{fla2, objbank, texbank};
use glovebox_editor::camera::Camera;
use glovebox_editor::scene::{self, Geometry, Item, LineMode, Lines, Material, Mesh, NodeId, Prefab, Scene, Sprite, Texture};
use glovebox_editor::vec;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

/// One device for all tests: making one takes a while on a software renderer.
fn gpu() -> &'static (wgpu::Device, wgpu::Queue) {
    static GPU: OnceLock<(wgpu::Device, wgpu::Queue)> = OnceLock::new();
    GPU.get_or_init(|| {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).expect("no graphics adapter");
        let info = adapter.get_info();
        println!("adapter: {} ({:?})", info.name, info.backend);
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).expect("no device")
    })
}

fn renderer() -> Renderer {
    let (device, queue) = gpu();
    Renderer::new(device, queue)
}

fn save_png(path: &Path, w: u32, h: u32, rgba: &[u8]) {
    let mut enc = png::Encoder::new(std::io::BufWriter::new(std::fs::File::create(path).unwrap()), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(rgba).unwrap();
}

/// A PNG file as RGBA bytes.
fn load_png(path: &Path) -> (u32, u32, Vec<u8>) {
    let mut dec = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).unwrap()));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = dec.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    buf.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf.chunks(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        other => panic!("{other:?} in {path:?}"),
    };
    (info.width, info.height, rgba)
}

fn px(img: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
    let o = ((y * w + x) * 4) as usize;
    [img[o], img[o + 1], img[o + 2], img[o + 3]]
}

fn near(a: [u8; 4], b: [u8; 4], tol: i32) -> bool {
    (0..4).all(|i| (i32::from(a[i]) - i32::from(b[i])).abs() <= tol)
}

fn srgb(c: f64) -> u8 {
    let v = if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
    (v * 255.0).round().clamp(0.0, 255.0) as u8
}

/// What a lit, untextured face shows: Lambert's albedo / pi under the ambient light and the sun.
fn lit(color: [f64; 3], ambient: [f64; 3], n_dot_sun: f64) -> [u8; 4] {
    let lin = |c: f64| glovebox::render::srgb_to_linear(c);
    let f = |i: usize| srgb(color[i] / std::f64::consts::PI * (lin(ambient[i]) * 0.8 + 1.4 * n_dot_sun.max(0.0)));
    [f(0), f(1), f(2), 255]
}

fn camera(w: u32, h: u32, pos: [f64; 3], target: [f64; 3]) -> Camera {
    let mut c = Camera::new(55.0, 1.0, 60000.0);
    (c.width, c.height, c.pos) = (f64::from(w), f64::from(h), pos);
    c.look_at(target);
    c
}

/// A 2 by 2 picture: red and green on top, blue and white below.
fn four_colours() -> Arc<Texture> {
    let px = [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255], [255, 255, 255, 255]];
    let mut rgba = Vec::new();
    for y in 0..32 {
        for x in 0..32 {
            rgba.extend_from_slice(&px[(y / 16) * 2 + x / 16]);
        }
    }
    Arc::new(Texture::new(32, 32, rgba).unwrap())
}

/// A square in the XY plane that faces +z, 2 units across, with the texture's top-left corner at its top left.
fn square() -> Geometry {
    let pos = vec![[-1.0, -1.0, 0.0], [1.0, -1.0, 0.0], [1.0, 1.0, 0.0], [-1.0, 1.0, 0.0]];
    let uv = vec![[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
    Geometry::new(pos, Some(vec![[0, 1, 2], [0, 2, 3]])).with(Some(vec![[0.0, 0.0, 1.0]; 4]), Some(uv), None)
}

#[test]
fn a_made_up_scene() {
    let (w, h) = (400u32, 300u32);
    let mut s = Scene::new();
    let world = s.node("world");
    // A lit, textured square in the middle, facing the camera
    let tex = four_colours();
    let textured = Arc::new(Material { texture: Some(tex), ..Material::default() });
    let sq = s.mesh_node(Arc::new(square()), textured);
    s[sq].scale = [40.0; 3];
    s.add(world, sq);
    // A lit grey box to its right, turned so that three faces show
    let grey = Arc::new(Material::color([0.5, 0.5, 0.5]));
    let bx = s.mesh_node(Arc::new(scene::box_geometry(50.0, 50.0, 50.0)), grey);
    s[bx].pos = [110.0, 0.0, 0.0];
    s.add(world, bx);
    // A translucent, unlit volume in front of the square's lower half, with its outline
    let glass = Arc::new(Material { color: [1.0, 1.0, 1.0], lit: false, transparent: true, opacity: 0.5, depth_write: false, ..Material::default() });
    let vol = s.mesh_node(Arc::new(scene::box_geometry(120.0, 30.0, 10.0)), glass);
    s[vol].pos = [0.0, -20.0, 30.0];
    s.add(world, vol);
    // A sprite to the left, and one hidden behind the square
    let mut icons = Icons::new();
    let icon = icons.texture("●", "#ff0000");
    let sp = s.sprite_node(Sprite::new(icon.clone(), 60.0));
    s[sp].pos = [-110.0, 0.0, 0.0];
    s.add(world, sp);
    let behind = s.sprite_node(Sprite::new(icon, 30.0));
    s[behind].pos = [20.0, 20.0, -50.0];
    s.add(world, behind);
    // A line across the top, a dashed line below it, and a line behind the square that ignores depth
    let top = s.lines_node(Lines::from_f64(&[[-150.0, 80.0, 0.0], [150.0, 80.0, 0.0]], [1.0, 1.0, 0.0], LineMode::Strip));
    s.add(world, top);
    let mut dashed = Lines::from_f64(&[[-150.0, 65.0, 0.0], [150.0, 65.0, 0.0]], [0.0, 1.0, 1.0], LineMode::Strip);
    dashed.dash = Some((20.0, 20.0));
    let dashed = s.lines_node(dashed);
    s.add(world, dashed);
    let mut through = Lines::from_f64(&[[-30.0, 20.0, -20.0], [-10.0, 20.0, -20.0]], [1.0, 0.0, 1.0], LineMode::Strip);
    through.depth_test = false;
    let through = s.lines_node(through);
    s.add(world, through);

    let cam = camera(w, h, [0.0, 0.0, 300.0], [0.0, 0.0, 0.0]);
    let mut r = renderer();
    r.background = [0.2, 0.3, 0.4];
    r.resize(w, h);
    let roots = [world];
    r.draw(&Frame::new(&s, &roots, &cam));
    let img = r.read_pixels().unwrap();
    assert_eq!(img.len(), (w * h * 4) as usize);
    if let Ok(dir) = std::env::var("GLOVEBOX_ORACLE") {
        save_png(&Path::new(&dir).join("render/out/unit_rs.png"), w, h, &img);
    }
    let at = |p: [f64; 3], dx: i32, dy: i32| {
        let q = cam.to_pixels(p);
        px(&img, w, (q[0].round() as i32 + dx) as u32, (q[1].round() as i32 + dy) as u32)
    };
    // The background is written as it is
    assert!(near(px(&img, w, 5, 5), [51, 77, 102, 255], 1));
    // The textured square: red top left, green top right; lit from the sun (0.4, 1, 0.3) on a face along +z
    let n_dot = 0.3 / (0.4f64 * 0.4 + 1.0 + 0.3 * 0.3).sqrt();
    assert!(near(at([-20.0, 20.0, 0.0], -8, -8), lit([1.0, 0.0, 0.0], [1.0; 3], n_dot), 2), "{:?}", at([-20.0, 20.0, 0.0], -8, -8));
    assert!(near(at([20.0, 20.0, 0.0], 8, -8), lit([0.0, 1.0, 0.0], [1.0; 3], n_dot), 2));
    // Its lower half is behind the white volume at half opacity: blue on the left, white on the right
    let blend = |c: [u8; 4]| [(c[0] / 2 + 128), (c[1] / 2 + 128), (c[2] / 2 + 128), 255];
    assert!(near(at([-20.0, -20.0, 0.0], 0, 0), blend(lit([0.0, 0.0, 1.0], [1.0; 3], n_dot)), 3), "{:?}", at([-20.0, -20.0, 0.0], 0, 0));
    assert!(near(at([20.0, -20.0, 0.0], 0, 0), blend(lit([1.0, 1.0, 1.0], [1.0; 3], n_dot)), 3));
    // The grey box shows its front face
    assert!(near(at([110.0, 0.0, 25.0], 0, 0), lit([0.5; 3], [1.0; 3], n_dot), 2), "{:?}", at([110.0, 0.0, 25.0], 0, 0));
    // The sprite: the icon's red disc with the dark dot in the middle and a black rim. The sprite's shader writes
    // the texel as sampled (linear), as Python's did: 255 stays 255, the glyph's 17 becomes 1.
    assert!(near(at([-110.0, 18.0, 0.0], 0, 0), [255, 0, 0, 255], 2), "{:?}", at([-110.0, 18.0, 0.0], 0, 0));
    assert!(near(at([-110.0, 0.0, 0.0], 0, 0), [1, 1, 1, 255], 2));
    assert!(near(at([-110.0, 24.5, 0.0], 0, 0), [0, 0, 0, 255], 2));
    // Its corner is clear: the background shows
    assert!(near(at([-110.0, 0.0, 0.0], -24, -24), [51, 77, 102, 255], 1));
    // The sprite behind the square is hidden
    assert!(near(at([20.0, 20.0, 0.0], 0, 0), lit([0.0, 1.0, 0.0], [1.0; 3], n_dot), 2));
    // The lines: some pixel of the row is the line's colour (multisampling may split it over two rows)
    let row_has = |p: [f64; 3], c: [u8; 3]| (-1..=1).any(|dy| (0..3).all(|k| (i32::from(at(p, 0, dy)[k]) - i32::from(c[k])).abs() <= 140) && at(p, 0, dy)[..3] != [51, 77, 102]);
    assert!(row_has([100.0, 80.0, 0.0], [255, 255, 0]));
    // The dashed line is drawn for 20 units and left out for 20
    assert!(row_has([-140.0, 65.0, 0.0], [0, 255, 255]));
    assert!((-2..=2).all(|dy| near(at([-120.0, 65.0, 0.0], 0, dy), [51, 77, 102, 255], 1)));
    // The line behind the square shows through it
    assert!((-1..=1).any(|dy| { let c = at([-20.0, 20.0, -20.0], 0, dy); c[2] > 100 && c[0] > 150 }));
    assert_eq!(r.stats.draw_calls, 8);

    // Batches draw the same picture with fewer calls
    let (batches, held) = scene::make_batches(&s, &roots, &|_, _| false);
    assert_eq!((batches.len(), held.len()), (2, 2));
    r.draw(&Frame { batches: &batches, batched: Some(&held), ..Frame::new(&s, &roots, &cam) });
    assert_eq!(r.read_pixels().unwrap(), img);
    // Fog takes a far mesh and leaves a near one
    r.fog = Some(Fog { color: [0.2, 0.3, 0.4], near: 100.0, far: 250.0 });
    r.draw(&Frame::new(&s, &roots, &cam));
    let fogged = r.read_pixels().unwrap();
    assert!(near(px(&fogged, w, 200 - 20, 150 - 20), [51, 77, 102, 255], 1));
    // A hidden root draws nothing, and the picture can be drawn at another size
    s[world].visible = false;
    let small = r.render_image(&Frame::new(&s, &roots, &cam), 64, 48).unwrap();
    assert!(small.chunks(4).all(|p| near([p[0], p[1], p[2], p[3]], [51, 77, 102, 255], 1)));
    assert_eq!(r.size(), Some((w, h)));
    // GPU copies are freed when nothing holds them
    assert!(r.gpu_copies().0 >= 3);
    r.collect(&s, &[], &[]);
    assert_eq!((r.gpu_copies().0, r.gpu_copies().1), (0, 0));
}

#[test]
fn textures_wrap_and_cut_out() {
    let (w, h) = (300u32, 100u32);
    let mut s = Scene::new();
    let world = s.node("world");
    let tex = four_colours();
    // Three squares whose texture coordinates run from -1 to 2: repeated, mirrored and clamped
    for (i, wrap) in [Wrap::Repeat, Wrap::Mirror, Wrap::Clamp].into_iter().enumerate() {
        let mut g = square();
        g.uvs = Some(vec![[-1.0, 2.0], [2.0, 2.0], [2.0, -1.0], [-1.0, -1.0]]);
        let mat = Arc::new(Material { texture: Some(tex.clone()), lit: false, wrap: (wrap, wrap), ..Material::default() });
        let n = s.mesh_node(Arc::new(g), mat);
        s[n].pos = [-100.0 + 100.0 * i as f64, 0.0, 0.0];
        s[n].scale = [45.0; 3];
        s.add(world, n);
    }
    let mut cam = Camera::new(30.0, 1.0, 5000.0);
    (cam.width, cam.height, cam.pos) = (f64::from(w), f64::from(h), [0.0, 0.0, 187.0]);
    cam.look_at([0.0; 3]);
    let mut r = renderer();
    r.resize(w, h);
    let roots = [world];
    r.draw(&Frame::new(&s, &roots, &cam));
    let img = r.read_pixels().unwrap();
    // The texel at the top left of each square (u, v just above -1): red is the picture's top left
    let at = |i: usize, fx: f64, fy: f64| {
        let q = cam.to_pixels([-100.0 + 100.0 * i as f64 + 45.0 * fx, 45.0 * fy, 0.0]);
        px(&img, w, q[0].round() as u32, q[1].round() as u32)
    };
    let (red, green, blue, white) = ([255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255], [255, 255, 255, 255]);
    // u = -0.75, v = -0.75. Repeat reads (0.25, 0.25): red. Mirror reads (0.75, 0.75): white. Clamp reads (0, 0): red.
    assert_eq!([at(0, -0.83, 0.83), at(1, -0.83, 0.83), at(2, -0.83, 0.83)], [red, white, red]);
    // u = 1.75, v = -0.75. Repeat: (0.75, 0.25) green. Mirror: (0.25, 0.75) blue. Clamp: (1, 0) green.
    assert_eq!([at(0, 0.83, 0.83), at(1, 0.83, 0.83), at(2, 0.83, 0.83)], [green, blue, green]);
    // The middle of each square reads the picture as it is: the bottom right quarter is white
    assert_eq!([at(0, 0.17, -0.17), at(1, 0.17, -0.17), at(2, 0.17, -0.17)], [white, white, white]);
    assert_eq!(at(2, -0.83, -0.83), blue);

    // A cut-out: texels under the alpha test are not drawn, and vertex colours tint the rest
    let mut s = Scene::new();
    let world = s.node("world");
    let mut rgba = Vec::new();
    for _y in 0..8 {
        for x in 0..8 {
            rgba.extend_from_slice(&[255, 255, 255, if x < 4 { 255 } else { 40 }]);
        }
    }
    let holes = Arc::new(Texture::new(8, 8, rgba).unwrap());
    let g = square().with(None, Some(vec![[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]), Some(vec![[1.0, 0.5, 0.0, 1.0]; 4]));
    let mat = Arc::new(Material { texture: Some(holes), lit: false, alpha_test: 0.3, vertex_colors: true, wrap: (Wrap::Clamp, Wrap::Clamp), ..Material::default() });
    let n = s.mesh_node(Arc::new(g), mat);
    s[n].scale = [45.0; 3];
    s.add(world, n);
    let roots = [world];
    r.background = [0.0, 0.0, 1.0];
    r.draw(&Frame::new(&s, &roots, &cam));
    let img = r.read_pixels().unwrap();
    assert!(near(px(&img, w, 150 - 20, 50), [255, srgb(0.5), 0, 255], 1), "{:?}", px(&img, w, 130, 50));
    assert!(near(px(&img, w, 150 + 20, 50), [0, 0, 255, 255], 1));
}

fn triangle_model() -> gm::Model {
    let prim = gm::Primitive {
        material: 0,
        positions: vec![[-10.0, 0.0, -10.0], [10.0, 0.0, 10.0], [10.0, 0.0, -10.0], [-10.0, 0.0, -10.0], [-10.0, 0.0, 10.0], [10.0, 0.0, 10.0]],
        indices: vec![0, 1, 2, 3, 4, 5],
        ..gm::Primitive::default()
    };
    let material = gm::Material {
        texture: None,
        base_color: [1.0, 0.5, 0.25, 1.0],
        alpha_mode: AlphaMode::Opaque,
        alpha_cutoff: 0.5,
        double_sided: false,
        unlit: false,
        wrap_s: Wrap::Repeat,
        wrap_t: Wrap::Repeat,
    };
    let mut root = gm::Node::new("root".into());
    root.children = vec![1];
    let mut a = gm::Node::new("a".into());
    (a.parent, a.mesh) = (Some(0), Some(0));
    gm::Model { nodes: vec![root, a], meshes: vec![gm::Mesh { name: "m".into(), primitives: vec![prim], billboard: false }], materials: vec![material] }
}

#[test]
fn a_thumbnail_keeps_its_shape() {
    let model = triangle_model();
    let prefab = Prefab::from_model(&model, &mut |_| None);
    let mut r = renderer();
    r.background = [1.0, 0.0, 0.0];
    r.fog = Some(Fog { color: [1.0; 3], near: 0.0, far: 1.0 });
    let img = r.render_thumbnail(&prefab, 64).unwrap();
    assert_eq!(img.len(), 64 * 64 * 4);
    // Clear around the model, the lit colour in the middle: the flat square faces up, towards the sun
    assert_eq!(px(&img, 64, 1, 1), [0, 0, 0, 0]);
    let d = [0.4f64, 1.0, 0.3];
    let n_dot = 1.0 / (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    assert!(near(px(&img, 64, 32, 32), lit([1.0, 0.5, 0.25], [1.0; 3], n_dot), 2), "{:?}", px(&img, 64, 32, 32));
    // The model fills the picture and leaves a margin
    let cover = img.chunks(4).filter(|p| p[3] == 255).count();
    assert!((400..3000).contains(&cover), "{cover}");
    // The view's own settings are put back
    assert_eq!((r.background, r.fog.is_some()), ([1.0, 0.0, 0.0], true));
    assert!(r.render_thumbnail(&Prefab::from_model(&gm::Model::default(), &mut |_| None), 64).is_none());
}

// ---------------------------------------------------------------------------------------------------------------
// Against the Python renderer

fn oracle() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("GLOVEBOX_ORACLE")?).join("render/out");
    dir.is_dir().then_some(dir)
}

fn f(v: &serde_json::Value) -> f64 {
    v.as_f64().unwrap()
}

fn v3(v: &serde_json::Value) -> [f64; 3] {
    [f(&v[0]), f(&v[1]), f(&v[2])]
}

fn dump_camera(c: &serde_json::Value) -> Camera {
    let mut cam = Camera::new(f(&c["fov"]), f(&c["near"]), f(&c["far"]));
    (cam.width, cam.height, cam.pos) = (f(&c["width"]), f(&c["height"]), v3(&c["pos"]));
    cam.quat = [f(&c["quat"][0]), f(&c["quat"][1]), f(&c["quat"][2]), f(&c["quat"][3])];
    cam
}

fn settings(r: &mut Renderer, json: &serde_json::Value) {
    let s = &json["render"];
    r.background = v3(&s["background"]);
    r.ambient = v3(&s["ambient"]);
    r.fog = s["fog"].as_array().map(|fog| Fog { color: v3(&fog[0]), near: f(&fog[1]), far: f(&fog[2]) });
}

/// The mean difference of the colours of two pictures, of 255, and the share of pixels that differ by more than
/// 40 in some channel. Saves the differences as a picture (white where they are the same).
fn compare(dir: &Path, name: &str, w: u32, h: u32, ours: &[u8], theirs: &[u8]) -> (f64, f64) {
    assert_eq!(ours.len(), theirs.len());
    let (mut sum, mut far) = (0u64, 0u64);
    let mut diff = Vec::with_capacity(ours.len());
    for (a, b) in ours.chunks(4).zip(theirs.chunks(4)) {
        let d = [0, 1, 2].map(|k| (i32::from(a[k]) - i32::from(b[k])).unsigned_abs());
        sum += u64::from(d[0] + d[1] + d[2]);
        far += u64::from(d.iter().any(|v| *v > 40));
        let m = d.iter().max().unwrap().saturating_mul(4).min(255) as u8;
        diff.extend_from_slice(&[255, 255 - m, 255 - m, 255]);
    }
    save_png(&dir.join(format!("{name}_rs.png")), w, h, ours);
    save_png(&dir.join(format!("{name}_diff.png")), w, h, &diff);
    let n = (ours.len() / 4) as f64;
    (sum as f64 / (3.0 * n), far as f64 / n)
}

#[test]
fn the_made_up_scene_matches_python() {
    let Some(dir) = oracle() else {
        println!("the_made_up_scene_matches_python: skipped, GLOVEBOX_ORACLE is not set");
        return;
    };
    let read = |name: &str| std::fs::read(dir.join(name)).unwrap();
    let d = scene::load_dump(&read("made.json"), &read("made.bin"), &|_| true).unwrap();
    let cam = dump_camera(&d.json["camera"]);
    let (w, h) = (cam.width as u32, cam.height as u32);
    let mut r = renderer();
    settings(&mut r, &d.json);
    let (roots, overlay) = (&d.roots[..1], &d.roots[1..]);
    let (batches, held) = scene::make_batches(&d.scene, roots, &|_, _| false);
    assert_eq!(batches.len() as u64, d.json["batches"].as_u64().unwrap());
    let frame = Frame { overlay, batches: &batches, batched: Some(&held), ..Frame::new(&d.scene, roots, &cam) };
    let ours = r.render_image(&frame, w, h).unwrap();
    let batched_calls = r.stats.draw_calls;
    let (pw, ph, theirs) = load_png(&dir.join("made_py.png"));
    assert_eq!((pw, ph), (w, h));
    let (mean, far) = compare(&dir, "made", w, h, &ours, &theirs);
    println!("made: mean colour difference {mean:.3} of 255, {:.2}% of pixels differ by more than 40; {batched_calls} draw calls", far * 100.0);
    assert!(mean < 1.0 && far < 0.01, "{mean} {far}");
    // Without batches the picture is the same
    let plain = r.render_image(&Frame { overlay, ..Frame::new(&d.scene, roots, &cam) }, w, h).unwrap();
    let same = plain.chunks(4).zip(ours.chunks(4)).filter(|(a, b)| a == b).count() as f64 / (plain.len() / 4) as f64;
    assert!(same > 0.999, "{same}");
    assert!(r.stats.draw_calls > batched_calls);
    // And without fog, on the default background
    (r.background, r.fog, r.ambient) = ([0.051, 0.055, 0.071], None, [1.0; 3]);
    let ours = r.render_image(&Frame { overlay, ..Frame::new(&d.scene, roots, &cam) }, w, h).unwrap();
    let (mean, far) = compare(&dir, "made_nofog", w, h, &ours, &load_png(&dir.join("made_py_nofog.png")).2);
    println!("made, no fog: mean colour difference {mean:.3} of 255, {:.2}% of pixels differ by more than 40", far * 100.0);
    assert!(mean < 1.0 && far < 0.01, "{mean} {far}");
}

/// The models of a level, built by this crate's own code from the game's banks.
struct Models {
    home: PathBuf,
    banks: HashMap<String, Vec<u8>>,
    prefabs: HashMap<String, Arc<Prefab>>,
    textures: HashMap<u32, Option<Arc<Texture>>>,
}

impl Models {
    fn bank(&mut self, file: &str) -> &Vec<u8> {
        let home = &self.home;
        self.banks.entry(file.to_string()).or_insert_with(|| fla2::data(&std::fs::read(home.join("data/Glover").join(file)).unwrap()).unwrap())
    }

    /// The model of a URL of the Python back end: /models/BANK@WORLD+WORLD/0xID-name.glb
    fn prefab(&mut self, url: &str) -> Arc<Prefab> {
        if let Some(p) = self.prefabs.get(url) {
            return p.clone();
        }
        let mut parts = url.trim_start_matches("/models/").split('/');
        let (folder, file) = (parts.next().unwrap(), parts.next().unwrap());
        let (bank, worlds) = folder.split_once('@').unwrap();
        let id = u32::from_str_radix(&file[2..10], 16).unwrap();
        let tex_files: Vec<String> = worlds.split('+').map(|w| format!("{w}_TEX_BANK.tex.fla")).collect();
        let tex_data: Vec<Vec<u8>> = tex_files.iter().map(|t| self.bank(t).clone()).collect();
        let tex_banks: Vec<texbank::TexBank> = tex_data.iter().map(|d| texbank::TexBank::parse(d).unwrap()).collect();
        let find = |id: u32| tex_banks.iter().find_map(|b| b.get(id));
        let obj_data = self.bank(&format!("{bank}.obj.fla")).clone();
        let parsed = objbank::Bank::parse(&obj_data).unwrap();
        let model = objbank::model(&parsed, id, &|id| find(id).map(|t| (t.width, t.height))).unwrap();
        let textures = &mut self.textures;
        let prefab = Arc::new(Prefab::from_model(&model, &mut |id| {
            textures
                .entry(id)
                .or_insert_with(|| {
                    let t = find(id)?;
                    Texture::new(u32::from(t.width), u32::from(t.height), t.decode().ok()?).map(Arc::new)
                })
                .clone()
        }));
        self.prefabs.insert(url.to_string(), prefab.clone());
        prefab
    }
}

#[test]
fn a_level_matches_python() {
    let (Some(dir), Some(home)) = (oracle(), std::env::var_os("GLOVEBOX_PYHOME")) else {
        println!("a_level_matches_python: skipped, GLOVEBOX_ORACLE and GLOVEBOX_PYHOME are not both set");
        return;
    };
    let read = |name: &str| std::fs::read(dir.join(name)).unwrap();
    let json: serde_json::Value = serde_json::from_slice(&read("level_0a.json")).unwrap();
    let nodes = json["nodes"].as_array().unwrap();
    // The nodes of each model copy: the tagged node and everything under it
    let mut owner: Vec<Option<usize>> = vec![None; nodes.len()];
    for (i, n) in nodes.iter().enumerate() {
        owner[i] = if n["tag"].is_string() { Some(i) } else { n["parent"].as_u64().and_then(|p| owner[p as usize]) };
    }
    let in_model: HashSet<String> = (0..nodes.len()).filter(|i| owner[*i].is_some()).map(|i| nodes[i]["world"].to_string() + &nodes[i]["name"].to_string()).collect();
    // The scene without the models' meshes: lines, icons, volumes and markers come from the dump
    let keep = |n: &serde_json::Value| !in_model.contains(&(n["world"].to_string() + &n["name"].to_string()));
    let mut d = scene::load_dump(&read("level_0a.json"), &read("level_0a.bin"), &keep).unwrap();
    let mut models = Models { home: PathBuf::from(home), banks: HashMap::new(), prefabs: HashMap::new(), textures: HashMap::new() };
    let t = Instant::now();
    let (mut copies, mut model_nodes, mut worst) = (0, 0, 0.0f64);
    let mut water_materials: HashMap<*const Material, Arc<Material>> = HashMap::new();
    for (i, n) in nodes.iter().enumerate() {
        let Some(tag) = n["tag"].as_str() else { continue };
        let (water, url) = match tag.strip_prefix("water:") {
            Some(u) => (true, u),
            None => (false, tag),
        };
        let prefab = models.prefab(url);
        let s = &mut d.scene;
        let (old, parent) = (d.nodes[i], s[d.nodes[i]].parent().unwrap());
        let copy = prefab.instance(s);
        (s[copy].pos, s[copy].quat, s[copy].scale) = (s[old].pos, s[old].quat, s[old].scale);
        s.add(parent, copy);
        // This crate's model gives the nodes, the matrices and the box that Python's glTF loader gave
        let walk = s.world_walk(copy, false);
        let theirs: Vec<usize> = (i..nodes.len()).take_while(|k| owner[*k] == Some(i)).collect();
        assert_eq!(walk.len(), theirs.len(), "{url}");
        for ((id, m), k) in walk.iter().zip(&theirs) {
            for (e, v) in nodes[*k]["world"].as_array().unwrap().iter().enumerate() {
                let dv = (m[e / 4][e % 4] - f(v)).abs() / (1.0 + f(v).abs());
                worst = worst.max(dv);
                assert!(dv < 1e-9, "{url}: matrix of {}", nodes[*k]["name"]);
            }
            assert_eq!(s[*id].items.len(), nodes[*k]["items"].as_array().unwrap().len(), "{url}");
            if water {
                // entities.attach_model makes a water model's materials translucent
                for it in &mut s[*id].items {
                    let Item::Mesh(mesh) = it else { continue };
                    let m = water_materials.entry(Arc::as_ptr(&mesh.material)).or_insert_with(|| {
                        Arc::new(Material { transparent: true, opacity: 0.6, depth_write: false, ..(*mesh.material).clone() })
                    });
                    *it = Item::Mesh(Mesh { geometry: mesh.geometry.clone(), material: m.clone(), matrix: mesh.matrix });
                }
            }
            model_nodes += 1;
        }
        let b = s.bounds(copy);
        let pb = &n["box"];
        for k in 0..3 {
            assert!((b.min[k] - f(&pb[0][k])).abs() < 1e-6 * (1.0 + b.min[k].abs()), "{url}: box");
            assert!((b.max[k] - f(&pb[1][k])).abs() < 1e-6 * (1.0 + b.max[k].abs()), "{url}: box");
        }
        s.delete(old);
        copies += 1;
    }
    let build = t.elapsed();
    let cam = dump_camera(&d.json["camera"]);
    let (w, h) = (cam.width as u32, cam.height as u32);
    let (device, _) = gpu();
    let mut r = renderer();
    settings(&mut r, &d.json);
    let roots = &d.roots[..1];
    let t = Instant::now();
    let (batches, held) = scene::make_batches(&d.scene, roots, &|_, _| false);
    let batching = t.elapsed();
    assert_eq!(batches.len() as u64, d.json["batches"].as_u64().unwrap());
    let frame = Frame { batches: &batches, batched: Some(&held), ..Frame::new(&d.scene, roots, &cam) };
    let ours = r.render_image(&frame, w, h).unwrap();
    let (mean, far) = compare(&dir, "level_0a", w, h, &ours, &load_png(&dir.join("level_0a_py.png")).2);
    println!(
        "{}: {copies} model copies of {} models ({model_nodes} nodes, worst matrix difference {worst:e}), built in {build:?}; mean colour \
         difference {mean:.3} of 255, {:.2}% of pixels differ by more than 40",
        d.json["info"]["level"],
        models.prefabs.len(),
        far * 100.0
    );
    assert!(mean < 1.5 && far < 0.01, "{mean} {far}");
    // A second view, close to the ground, where the fog is thin
    if let Some(c) = d.json.get("camera_near") {
        let near_cam = dump_camera(c);
        let ours = r.render_image(&Frame { camera: &near_cam, ..Frame::new(&d.scene, roots, &near_cam) }, w, h).unwrap();
        let (mean, far) = compare(&dir, "level_0a_near", w, h, &ours, &load_png(&dir.join("level_0a_near_py.png")).2);
        println!("near view: mean colour difference {mean:.3} of 255, {:.2}% of pixels differ by more than 40", far * 100.0);
        assert!(mean < 2.5 && far < 0.03, "{mean} {far}");
    }

    // Frame times at the view's size, on this adapter. A frame ends when the GPU has drawn it.
    r.resize(w, h);
    let mut time = |frame: &Frame, r: &mut Renderer| {
        let mut ms = Vec::new();
        for _ in 0..12 {
            let t = Instant::now();
            r.draw(frame);
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            ms.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
        (ms[ms.len() / 2], r.stats)
    };
    let (ms, stats) = time(&frame, &mut r);
    println!("frame, batched: {ms:.1} ms (median of 12), {} draw calls, {} triangles, {w}x{h}, 4 samples; batching took {batching:?}", stats.draw_calls, stats.triangles);
    let (ms, stats) = time(&Frame::new(&d.scene, roots, &cam), &mut r);
    println!("frame, unbatched: {ms:.1} ms, {} draw calls, {} triangles", stats.draw_calls, stats.triangles);
    let mut plain = Renderer::with_samples(device, &gpu().1, 1);
    settings(&mut plain, &d.json);
    plain.resize(w, h);
    let (ms, _) = time(&frame, &mut plain);
    println!("frame, batched, 1 sample: {ms:.1} ms");
    // A thumbnail of each model, as the palette draws them
    let t = Instant::now();
    let mut drawn = 0;
    let mut sheet = vec![0u8; 64 * 64 * 4 * 16];
    let mut urls: Vec<&String> = models.prefabs.keys().collect();
    urls.sort();
    for (i, url) in urls.iter().enumerate() {
        if let Some(img) = r.render_thumbnail(&models.prefabs[*url], 64) {
            if drawn < 16 {
                for y in 0..64 {
                    sheet[(y * 16 * 64 + drawn * 64) * 4..][..256].copy_from_slice(&img[y * 256..][..256]);
                }
            }
            drawn += 1;
        }
        let _ = i;
    }
    save_png(&dir.join("level_0a_thumbnails_rs.png"), 16 * 64, 64, &sheet);
    println!("{drawn} thumbnails of 64 pixels in {:?}", t.elapsed());
    let _ = vec::quat_identity();
    let _: Option<NodeId> = None;
}
