//! Draws a scene (`glovebox_editor::scene`) with wgpu: Lambert-shaded meshes with fog, lines (dashed or not) and
//! sprites.
//!
//! The renderer draws into its own colour and depth textures, never into the window. The window shows the colour
//! texture as an egui image (`Renderer::view`). `read_pixels`, `render_image` and `render_thumbnail` give the
//! picture as RGBA bytes.
//!
//! The order of drawing: the batches and the other opaque meshes; then blended meshes, lines and
//! sprites, far to near; then what ignores depth, by the node's order; then each overlay root (the gizmo) over a
//! cleared depth buffer. Every material is drawn double-sided. Colours blend by alpha, and alpha itself only
//! grows.
//!
//! GPU copies of geometry, lines and textures are made on first use and kept by the id of the scene object.
//! `collect` frees the copies of what is no longer drawn.

pub mod icons;

use eframe::egui_wgpu::wgpu;
use glovebox_core::model::Wrap;
use glovebox_editor::camera::Camera;
use glovebox_editor::scene::{self, Batch, Geometry, Item, Lines, LineMode, Material, NodeId, Prefab, Scene, Texture};
use glovebox_editor::vec::{self, Mat4, V3};
use std::collections::{HashMap, HashSet};

const SHADERS: &str = include_str!("shaders.wgsl");
/// The colour target. The shaders write sRGB values into it.
const COLOR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const GLOBALS_SIZE: u64 = 192;
const DRAW_SIZE: u64 = 176;
const MESH_STRIDE: u64 = 48;

/// The level's fog: its colour (sRGB), and where it starts and is full, as distances from the camera.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fog {
    pub color: [f64; 3],
    pub near: f64,
    pub far: f64,
}

/// What one frame draws.
pub struct Frame<'a> {
    pub scene: &'a Scene,
    pub roots: &'a [NodeId],
    /// Roots drawn after everything else, each over a cleared depth buffer (the gizmo).
    pub overlay: &'a [NodeId],
    pub camera: &'a Camera,
    /// Meshes already in world space, drawn first (`scene::make_batches`).
    pub batches: &'a [Batch],
    /// The (node, item) pairs the batches hold. The walk skips them.
    pub batched: Option<&'a HashSet<(NodeId, usize)>>,
    /// The alpha the picture is cleared to. 1 for the view, 0 for a thumbnail.
    pub clear_alpha: f64,
}

impl<'a> Frame<'a> {
    pub fn new(scene: &'a Scene, roots: &'a [NodeId], camera: &'a Camera) -> Frame<'a> {
        Frame { scene, roots, overlay: &[], camera, batches: &[], batched: None, clear_alpha: 1.0 }
    }
}

/// Counts of the last frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stats {
    pub draw_calls: u32,
    pub triangles: u64,
}

struct Target {
    width: u32,
    height: u32,
    /// The multisampled colour texture's view, when the renderer multisamples.
    msaa: Option<wgpu::TextureView>,
    color: wgpu::Texture,
    color_view: wgpu::TextureView,
    depth: wgpu::TextureView,
}

struct GpuGeometry {
    vertices: wgpu::Buffer,
    indices: Option<wgpu::Buffer>,
    /// The number of indices, or of vertices without an index buffer.
    count: u32,
}

struct GpuLines {
    vertices: Option<wgpu::Buffer>,
    count: u32,
    version: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct TexKey(u64, u32, u32);

#[derive(Clone, Copy, PartialEq)]
enum What {
    Mesh(u64),
    Lines(u64),
    Sprite,
}

struct Cmd {
    pipeline: usize,
    /// The offset of the draw's uniforms.
    offset: u32,
    texture: Option<TexKey>,
    what: What,
}

/// The pipelines, by what they draw and how they use depth.
const MESH: usize = 0; // depth test and write
const MESH_NO_WRITE: usize = 1;
const MESH_NO_TEST: usize = 2;
const LINE: usize = 3;
const LINE_NO_TEST: usize = 4;
const SPRITE: usize = 5;
const SPRITE_NO_TEST: usize = 6;

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    samples: u32,
    pipelines: Vec<wgpu::RenderPipeline>,
    globals: wgpu::Buffer,
    globals_group: wgpu::BindGroup,
    draw_layout: wgpu::BindGroupLayout,
    draws: wgpu::Buffer,
    draws_group: wgpu::BindGroup,
    draws_capacity: u64,
    /// The spacing of the draws' uniforms in their buffer.
    slot: u64,
    texture_layout: wgpu::BindGroupLayout,
    samplers: HashMap<(u32, u32), wgpu::Sampler>,
    white: wgpu::BindGroup,
    geoms: HashMap<u64, GpuGeometry>,
    lines: HashMap<u64, GpuLines>,
    textures: HashMap<TexKey, wgpu::BindGroup>,
    target: Option<Target>,
    /// The colour the view is cleared to (sRGB).
    pub background: [f64; 3],
    pub fog: Option<Fog>,
    /// The level's ambient light (sRGB).
    pub ambient: [f64; 3],
    /// The direction to the sun in the world, a unit vector.
    pub sun: V3,
    pub stats: Stats,
}

pub fn srgb_to_linear(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(c: f64) -> f64 {
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

fn put(out: &mut Vec<u8>, v: &[f32]) {
    for x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
}

/// A matrix in the column order WGSL reads.
fn put_mat(out: &mut Vec<u8>, m: &Mat4) {
    for j in 0..4 {
        for row in m {
            out.extend_from_slice(&(row[j] as f32).to_le_bytes());
        }
    }
}

fn buffer(device: &wgpu::Device, data: &[u8], usage: wgpu::BufferUsages) -> wgpu::Buffer {
    let size = (data.len() as u64).div_ceil(4).max(1) * 4;
    let b = device.create_buffer(&wgpu::BufferDescriptor { label: None, size, usage, mapped_at_creation: true });
    // A buffer made mapped always gives its range
    if let Ok(mut view) = b.slice(..).get_mapped_range_mut() {
        let mut padded = data.to_vec();
        padded.resize(size as usize, 0);
        view.copy_from_slice(&padded);
    }
    b.unmap();
    b
}

/// The picture and its smaller copies, each half the size of the one before, down to one pixel.
fn mip_chain(t: &Texture) -> Vec<(u32, u32, Vec<u8>)> {
    let to_lin: Vec<f32> = (0..256).map(|i| if t.srgb { srgb_to_linear(f64::from(i) / 255.0) as f32 } else { i as f32 / 255.0 }).collect();
    let mut out = vec![(t.width, t.height, t.rgba.clone())];
    loop {
        let (sw, sh, src) = out.last().expect("the chain starts with one picture");
        let (sw, sh) = (*sw as usize, *sh as usize);
        if sw == 1 && sh == 1 {
            break;
        }
        let (dw, dh) = ((sw / 2).max(1), (sh / 2).max(1));
        let mut dst = Vec::with_capacity(dw * dh * 4);
        for y in 0..dh {
            let (y0, y1) = (y * sh / dh, ((y + 1) * sh / dh).max(y * sh / dh + 1));
            for x in 0..dw {
                let (x0, x1) = (x * sw / dw, ((x + 1) * sw / dw).max(x * sw / dw + 1));
                let mut sum = [0f32; 4];
                for yy in y0..y1 {
                    for xx in x0..x1 {
                        let p = &src[(yy * sw + xx) * 4..][..4];
                        for k in 0..3 {
                            sum[k] += to_lin[p[k] as usize];
                        }
                        sum[3] += f32::from(p[3]) / 255.0;
                    }
                }
                let n = ((y1 - y0) * (x1 - x0)) as f32;
                for (k, s) in sum.iter().enumerate() {
                    let v = f64::from(s / n);
                    let v = if k < 3 && t.srgb { linear_to_srgb(v) } else { v };
                    dst.push((v * 255.0).round().clamp(0.0, 255.0) as u8);
                }
            }
        }
        out.push((dw as u32, dh as u32, dst));
    }
    out
}

fn address(w: Wrap) -> wgpu::AddressMode {
    match w {
        Wrap::Repeat => wgpu::AddressMode::Repeat,
        Wrap::Mirror => wgpu::AddressMode::MirrorRepeat,
        Wrap::Clamp => wgpu::AddressMode::ClampToEdge,
    }
}

impl Renderer {
    /// A renderer with 4 samples per pixel. Every WebGPU device can multisample these targets 4 times.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Renderer {
        Renderer::with_samples(device, queue, 4)
    }

    /// `samples` is 1 (no multisampling) or 4.
    pub fn with_samples(device: &wgpu::Device, queue: &wgpu::Queue, samples: u32) -> Renderer {
        let samples = if samples >= 4 { 4 } else { 1 };
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("view"), source: wgpu::ShaderSource::Wgsl(SHADERS.into()) });
        let uniform = |dynamic: bool, size: u64| wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: dynamic, min_binding_size: wgpu::BufferSize::new(size) },
            count: None,
        };
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: None, entries: &[uniform(false, GLOBALS_SIZE)] });
        let draw_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: None, entries: &[uniform(true, DRAW_SIZE)] });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&globals_layout), Some(&draw_layout), Some(&texture_layout)],
            ..Default::default()
        });
        let mesh_attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x4];
        let line_attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32];
        // Colour blends by alpha; alpha itself only grows: the view stays opaque, and a thumbnail drawn on a clear
        // background keeps its shape
        let blend = wgpu::BlendState {
            color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::SrcAlpha, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add },
            alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add },
        };
        let pipeline = |vs: &str, fs: &str, buffers: &[Option<wgpu::VertexBufferLayout>], lines: bool, test: bool, write: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(fs),
                layout: Some(&layout),
                vertex: wgpu::VertexState { module: &module, entry_point: Some(vs), buffers, compilation_options: Default::default() },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(fs),
                    targets: &[Some(wgpu::ColorTargetState { format: COLOR, blend: Some(blend), write_mask: wgpu::ColorWrites::ALL })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: if lines { wgpu::PrimitiveTopology::LineList } else { wgpu::PrimitiveTopology::TriangleList },
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH,
                    depth_write_enabled: Some(test && write),
                    depth_compare: Some(if test { wgpu::CompareFunction::Less } else { wgpu::CompareFunction::Always }),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState { count: samples, ..Default::default() },
                multiview_mask: None,
                cache: None,
            })
        };
        let mesh_buffers = [Some(wgpu::VertexBufferLayout { array_stride: MESH_STRIDE, step_mode: wgpu::VertexStepMode::Vertex, attributes: &mesh_attrs })];
        let line_buffers = [Some(wgpu::VertexBufferLayout { array_stride: 16, step_mode: wgpu::VertexStepMode::Vertex, attributes: &line_attrs })];
        let pipelines = vec![
            pipeline("mesh_vs", "mesh_fs", &mesh_buffers, false, true, true),
            pipeline("mesh_vs", "mesh_fs", &mesh_buffers, false, true, false),
            pipeline("mesh_vs", "mesh_fs", &mesh_buffers, false, false, false),
            pipeline("line_vs", "line_fs", &line_buffers, true, true, true),
            pipeline("line_vs", "line_fs", &line_buffers, true, false, false),
            pipeline("sprite_vs", "sprite_fs", &[], false, true, true),
            pipeline("sprite_vs", "sprite_fs", &[], false, false, false),
        ];
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: GLOBALS_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() }],
        });
        let align = u64::from(device.limits().min_uniform_buffer_offset_alignment.max(1));
        let slot = DRAW_SIZE.div_ceil(align) * align;
        let (draws, draws_group) = Self::draw_buffer(device, &draw_layout, slot * 256);
        let mut samplers = HashMap::new();
        let white_pixel = Texture { id: 0, width: 1, height: 1, rgba: vec![255; 4], srgb: true };
        let white = Self::texture_group(device, queue, &texture_layout, &mut samplers, &white_pixel, (Wrap::Repeat, Wrap::Repeat));
        Renderer {
            device: device.clone(),
            queue: queue.clone(),
            samples,
            pipelines,
            globals,
            globals_group,
            draw_layout,
            draws,
            draws_group,
            draws_capacity: slot * 256,
            slot,
            texture_layout,
            samplers,
            white,
            geoms: HashMap::new(),
            lines: HashMap::new(),
            textures: HashMap::new(),
            target: None,
            background: [0.051, 0.055, 0.071],
            fog: None,
            ambient: [1.0; 3],
            sun: vec::div([0.4, 1.0, 0.3], vec::norm([0.4, 1.0, 0.3])),
            stats: Stats::default(),
        }
    }

    fn draw_buffer(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, size: u64) -> (wgpu::Buffer, wgpu::BindGroup) {
        let b = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("draws"),
            size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let g = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &b, offset: 0, size: wgpu::BufferSize::new(DRAW_SIZE) }),
            }],
        });
        (b, g)
    }

    // -- Targets

    fn make_target(&self, width: u32, height: u32) -> Target {
        let size = wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 };
        let tex = |label: &str, format, samples: u32, usage, view_formats: &[wgpu::TextureFormat]| {
            self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size,
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats,
            })
        };
        let attach = wgpu::TextureUsages::RENDER_ATTACHMENT;
        let color = tex("view", COLOR, 1, attach | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC, &[]);
        let color_view = color.create_view(&Default::default());
        let msaa = (self.samples > 1).then(|| tex("view samples", COLOR, self.samples, attach, &[]).create_view(&Default::default()));
        let depth = tex("view depth", DEPTH, self.samples, attach, &[]).create_view(&Default::default());
        Target { width: size.width, height: size.height, msaa, color, color_view, depth }
    }

    /// Sets the size of the view's picture, in pixels. The texture behind `view` changes when the size does:
    /// register the new one with egui.
    pub fn resize(&mut self, width: u32, height: u32) -> bool {
        let (w, h) = (width.max(1), height.max(1));
        if self.target.as_ref().is_some_and(|t| t.width == w && t.height == h) {
            return false;
        }
        self.target = Some(self.make_target(w, h));
        true
    }

    pub fn size(&self) -> Option<(u32, u32)> {
        self.target.as_ref().map(|t| (t.width, t.height))
    }

    /// The view's picture, for `egui_wgpu::Renderer::register_native_texture`. None before the first `resize`.
    /// It holds the sRGB values the shaders wrote, and egui-wgpu shows a texture's values as they are.
    pub fn view(&self) -> Option<&wgpu::TextureView> {
        self.target.as_ref().map(|t| &t.color_view)
    }

    // -- GPU resources

    fn ensure_geometry(&mut self, g: &Geometry) {
        if self.geoms.contains_key(&g.id) {
            return;
        }
        let n = g.positions.len();
        let mut data = Vec::with_capacity(n * MESH_STRIDE as usize);
        for i in 0..n {
            put(&mut data, &g.positions[i]);
            put(&mut data, g.normals.as_ref().map_or(&[0.0, 1.0, 0.0], |v| &v[i]));
            put(&mut data, g.uvs.as_ref().map_or(&[0.0, 0.0], |v| &v[i]));
            put(&mut data, g.colors.as_ref().map_or(&[1.0; 4], |v| &v[i]));
        }
        let vertices = buffer(&self.device, &data, wgpu::BufferUsages::VERTEX);
        let (indices, count) = match &g.indices {
            Some(t) => {
                let mut data = Vec::with_capacity(t.len() * 12);
                for i in t.iter().flatten() {
                    data.extend_from_slice(&i.to_le_bytes());
                }
                (Some(buffer(&self.device, &data, wgpu::BufferUsages::INDEX)), t.len() as u32 * 3)
            }
            None => (None, (n / 3 * 3) as u32),
        };
        self.geoms.insert(g.id, GpuGeometry { vertices, indices, count });
    }

    fn ensure_lines(&mut self, l: &Lines) {
        if self.lines.get(&l.id).is_some_and(|e| e.version == l.version) {
            return;
        }
        let segs = l.segments();
        let mut data = Vec::with_capacity(segs.len() * 32);
        // The distance along the line, for dashes: it runs on along strips and loops
        let mut start = 0.0f64;
        for (a, b) in &segs {
            let len = f64::from(((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2) + (b[2] - a[2]).powi(2)).sqrt());
            if l.mode == LineMode::Segments {
                start = 0.0;
            }
            put(&mut data, a);
            put(&mut data, &[start as f32]);
            put(&mut data, b);
            put(&mut data, &[(start + len) as f32]);
            start += len;
        }
        let vertices = (!segs.is_empty()).then(|| buffer(&self.device, &data, wgpu::BufferUsages::VERTEX));
        self.lines.insert(l.id, GpuLines { vertices, count: segs.len() as u32 * 2, version: l.version });
    }

    fn texture_group(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        samplers: &mut HashMap<(u32, u32), wgpu::Sampler>,
        t: &Texture,
        wrap: (Wrap, Wrap),
    ) -> wgpu::BindGroup {
        let mips = mip_chain(t);
        let format = if t.srgb { wgpu::TextureFormat::Rgba8UnormSrgb } else { wgpu::TextureFormat::Rgba8Unorm };
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width: t.width, height: t.height, depth_or_array_layers: 1 },
            mip_level_count: mips.len() as u32,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (level, (w, h, data)) in mips.iter().enumerate() {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: level as u32, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                data,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * w), rows_per_image: None },
                wgpu::Extent3d { width: *w, height: *h, depth_or_array_layers: 1 },
            );
        }
        let sampler = samplers.entry((wrap.0.gl(), wrap.1.gl())).or_insert_with(|| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                address_mode_u: address(wrap.0),
                address_mode_v: address(wrap.1),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Linear,
                ..Default::default()
            })
        });
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&tex.create_view(&Default::default())) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
            ],
        })
    }

    fn ensure_texture(&mut self, t: &Texture, wrap: (Wrap, Wrap)) -> TexKey {
        let key = TexKey(t.id, wrap.0.gl(), wrap.1.gl());
        if !self.textures.contains_key(&key) {
            let g = Self::texture_group(&self.device, &self.queue, &self.texture_layout, &mut self.samplers, t, wrap);
            self.textures.insert(key, g);
        }
        key
    }

    /// Frees every GPU copy (a new level). Textures whose id is in `keep_textures` stay.
    pub fn forget(&mut self, keep_textures: &HashSet<u64>) {
        self.geoms.clear();
        self.lines.clear();
        self.textures.retain(|k, _| keep_textures.contains(&k.0));
    }

    /// Frees the GPU copies of geometry and lines that neither the roots nor the batches hold any more (after the
    /// entities are built again). Hidden nodes count as held.
    pub fn collect(&mut self, scene: &Scene, roots: &[NodeId], batches: &[Batch]) {
        let mut live: HashSet<u64> = batches.iter().map(|b| b.geometry.id).collect();
        for &root in roots {
            for (n, _) in scene.walk(root, None, false) {
                for it in &scene[n].items {
                    match it {
                        Item::Mesh(m) => live.insert(m.geometry.id),
                        Item::Lines(l) => live.insert(l.id),
                        Item::Sprite(_) => false,
                    };
                }
            }
        }
        self.geoms.retain(|k, _| live.contains(k));
        self.lines.retain(|k, _| live.contains(k));
    }

    /// The numbers of geometries, lines and textures that have a GPU copy.
    pub fn gpu_copies(&self) -> (usize, usize, usize) {
        (self.geoms.len(), self.lines.len(), self.textures.len())
    }

    // -- Drawing

    /// Draws the frame into the view's picture. Call `resize` first.
    pub fn draw(&mut self, frame: &Frame) {
        if let Some(t) = self.target.take() {
            self.draw_to(&t, frame);
            self.target = Some(t);
        }
    }

    fn draw_to(&mut self, target: &Target, f: &Frame) {
        let (scene, camera) = (f.scene, f.camera);
        let view = camera.view();
        let proj = camera.proj_for(f64::from(target.width), f64::from(target.height));
        let view_proj = vec::mat4_mul(&proj, &view);

        // What to draw, in three lists
        struct Entry<'a> {
            order: i32,
            m: Mat4,
            item: EntryItem<'a>,
        }
        enum EntryItem<'a> {
            Mesh(&'a Geometry, &'a Material),
            Lines(&'a Lines),
            Sprite(&'a scene::Sprite),
        }
        fn entry<'a>(order: i32, m: &Mat4, it: &'a Item) -> Entry<'a> {
            match it {
                Item::Mesh(x) => Entry { order, m: vec::mat4_mul(m, &x.matrix), item: EntryItem::Mesh(&x.geometry, &x.material) },
                Item::Lines(x) => Entry { order, m: vec::mat4_mul(m, &x.matrix), item: EntryItem::Lines(x) },
                Item::Sprite(x) => Entry { order, m: vec::mat4_mul(m, &x.matrix), item: EntryItem::Sprite(x) },
            }
        }
        let (mut opaque, mut clear, mut late): (Vec<Entry>, Vec<Entry>, Vec<Entry>) = (Vec::new(), Vec::new(), Vec::new());
        for b in f.batches {
            opaque.push(Entry { order: 0, m: scene::IDENTITY, item: EntryItem::Mesh(&b.geometry, &b.material) });
        }
        for &root in f.roots {
            for (id, m) in scene.world_walk(root, true) {
                let node = &scene[id];
                for (index, it) in node.items.iter().enumerate() {
                    if f.batched.is_some_and(|b| b.contains(&(id, index))) {
                        continue;
                    }
                    let e = entry(node.order, &m, it);
                    let no_depth = match it {
                        Item::Mesh(x) => !x.material.depth_test,
                        Item::Lines(x) => !x.depth_test,
                        Item::Sprite(x) => !x.depth_test,
                    };
                    if node.order > 0 || no_depth {
                        late.push(e);
                    } else if matches!(it, Item::Mesh(x) if !x.material.blended()) {
                        opaque.push(e);
                    } else {
                        clear.push(e);
                    }
                }
            }
        }
        // Transparent things back to front, as three.js sorts them. Both sorts keep the order of equals.
        let eye = camera.pos;
        let far = |e: &Entry| -((e.m[0][3] - eye[0]).powi(2) + (e.m[1][3] - eye[1]).powi(2) + (e.m[2][3] - eye[2]).powi(2));
        clear.sort_by(|a, b| far(a).partial_cmp(&far(b)).unwrap_or(std::cmp::Ordering::Equal));
        late.sort_by_key(|e| e.order);
        let mut passes: Vec<Vec<Entry>> = vec![opaque.into_iter().chain(clear).chain(late).collect()];
        for &root in f.overlay {
            let mut list = Vec::new();
            for (id, m) in scene.world_walk(root, true) {
                let node = &scene[id];
                list.extend(node.items.iter().map(|it| entry(node.order, &m, it)));
            }
            passes.push(list);
        }

        // The uniforms and the GPU copies
        let mut uniforms: Vec<u8> = Vec::new();
        let mut cmds: Vec<Vec<Cmd>> = Vec::new();
        let mut stats = Stats::default();
        for list in &passes {
            let mut out = Vec::with_capacity(list.len());
            for e in list {
                let offset = uniforms.len() as u32;
                let test = e.order <= 0;
                let (mut model, mut normal) = (e.m, [[0.0; 3]; 3]);
                let (mut color, mut params, mut flags, mut centre) = ([1.0f32; 4], [0.0f32; 4], [0u32; 4], [0.0f32; 4]);
                let mut texture = None;
                let (pipeline, what) = match &e.item {
                    EntryItem::Mesh(g, mat) => {
                        self.ensure_geometry(g);
                        let count = self.geoms[&g.id].count;
                        if count == 0 {
                            continue;
                        }
                        stats.triangles += u64::from(count / 3);
                        let nm = scene::normal_matrix(&e.m);
                        for i in 0..3 {
                            for j in 0..3 {
                                normal[i][j] = (0..3).map(|k| view[i][k] * nm[k][j]).sum();
                            }
                        }
                        // A lit material's colour goes to the shader as it is; an unlit one's is made linear
                        let c = if mat.lit { mat.color } else { mat.color.map(srgb_to_linear) };
                        color = [c[0] as f32, c[1] as f32, c[2] as f32, mat.opacity as f32];
                        params[0] = mat.alpha_test as f32;
                        flags = [u32::from(mat.texture.is_some()), u32::from(mat.vertex_colors), u32::from(mat.lit), 0];
                        if let Some(t) = &mat.texture {
                            texture = Some(self.ensure_texture(t, mat.wrap));
                        }
                        let p = match (mat.depth_test && test, mat.depth_write) {
                            (true, true) => MESH,
                            (true, false) => MESH_NO_WRITE,
                            (false, _) => MESH_NO_TEST,
                        };
                        (p, What::Mesh(g.id))
                    }
                    EntryItem::Lines(l) => {
                        self.ensure_lines(l);
                        if self.lines[&l.id].count == 0 {
                            continue;
                        }
                        model = vec::mat4_mul(&view_proj, &e.m);
                        let c = l.color.map(srgb_to_linear);
                        color = [c[0] as f32, c[1] as f32, c[2] as f32, l.opacity as f32];
                        if let Some((dash, gap)) = l.dash {
                            (params[1], params[2]) = (dash as f32, gap as f32);
                        }
                        (if l.depth_test && test { LINE } else { LINE_NO_TEST }, What::Lines(l.id))
                    }
                    EntryItem::Sprite(s) => {
                        let sx = vec::decompose_scale(&e.m)[0];
                        params[3] = (s.size * if sx != 0.0 { sx } else { 1.0 }) as f32;
                        centre = [e.m[0][3] as f32, e.m[1][3] as f32, e.m[2][3] as f32, 1.0];
                        color[3] = s.opacity as f32;
                        texture = Some(self.ensure_texture(&s.texture, (Wrap::Clamp, Wrap::Clamp)));
                        stats.triangles += 2;
                        (if s.depth_test && test { SPRITE } else { SPRITE_NO_TEST }, What::Sprite)
                    }
                };
                put_mat(&mut uniforms, &model);
                for j in 0..3 {
                    put(&mut uniforms, &[normal[0][j] as f32, normal[1][j] as f32, normal[2][j] as f32, 0.0]);
                }
                put(&mut uniforms, &color);
                put(&mut uniforms, &params);
                for v in flags {
                    uniforms.extend_from_slice(&v.to_le_bytes());
                }
                put(&mut uniforms, &centre);
                uniforms.resize(offset as usize + self.slot as usize, 0);
                out.push(Cmd { pipeline, offset, texture, what });
                stats.draw_calls += 1;
            }
            cmds.push(out);
        }
        if uniforms.len() as u64 > self.draws_capacity {
            self.draws_capacity = (uniforms.len() as u64).next_power_of_two();
            (self.draws, self.draws_group) = Self::draw_buffer(&self.device, &self.draw_layout, self.draws_capacity);
        }
        self.queue.write_buffer(&self.draws, 0, &uniforms);

        let mut globals = Vec::with_capacity(GLOBALS_SIZE as usize);
        put_mat(&mut globals, &view);
        put_mat(&mut globals, &proj);
        let a = self.ambient.map(srgb_to_linear);
        put(&mut globals, &[a[0] as f32, a[1] as f32, a[2] as f32, 0.0]);
        let sun = [0, 1, 2].map(|i| (view[i][0] * self.sun[0] + view[i][1] * self.sun[1] + view[i][2] * self.sun[2]) as f32);
        put(&mut globals, &[sun[0], sun[1], sun[2], 0.0]);
        match self.fog {
            Some(fog) => {
                let c = fog.color.map(srgb_to_linear);
                put(&mut globals, &[c[0] as f32, c[1] as f32, c[2] as f32, 0.0]);
                put(&mut globals, &[fog.near as f32, fog.far as f32, 1.0, 0.0]);
            }
            None => put(&mut globals, &[0.0; 8]),
        }
        self.queue.write_buffer(&self.globals, 0, &globals);

        let mut enc = self.device.create_command_encoder(&Default::default());
        let last = cmds.len() - 1;
        for (i, list) in cmds.iter().enumerate() {
            let first = i == 0;
            // The background is an sRGB colour, written as it is
            let bg = wgpu::Color { r: self.background[0], g: self.background[1], b: self.background[2], a: f.clear_alpha };
            let load = if first { wgpu::LoadOp::Clear(bg) } else { wgpu::LoadOp::Load };
            let (view, resolve_target) = match &target.msaa {
                Some(m) => (m, (i == last).then_some(&target.color_view)),
                None => (&target.color_view, None),
            };
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target,
                    ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.globals_group, &[]);
            let (mut pipeline, mut texture, mut bound) = (usize::MAX, None, false);
            for c in list {
                if c.pipeline != pipeline {
                    pass.set_pipeline(&self.pipelines[c.pipeline]);
                    pipeline = c.pipeline;
                }
                if !bound || c.texture != texture {
                    let group = c.texture.and_then(|k| self.textures.get(&k)).unwrap_or(&self.white);
                    pass.set_bind_group(2, group, &[]);
                    (texture, bound) = (c.texture, true);
                }
                pass.set_bind_group(1, &self.draws_group, &[c.offset]);
                match c.what {
                    What::Mesh(id) => {
                        let Some(g) = self.geoms.get(&id) else { continue };
                        pass.set_vertex_buffer(0, g.vertices.slice(..));
                        match &g.indices {
                            Some(idx) => {
                                pass.set_index_buffer(idx.slice(..), wgpu::IndexFormat::Uint32);
                                pass.draw_indexed(0..g.count, 0, 0..1);
                            }
                            None => pass.draw(0..g.count, 0..1),
                        }
                    }
                    What::Lines(id) => {
                        let Some((l, v)) = self.lines.get(&id).and_then(|l| Some((l, l.vertices.as_ref()?))) else { continue };
                        pass.set_vertex_buffer(0, v.slice(..));
                        pass.draw(0..l.count, 0..1);
                    }
                    What::Sprite => pass.draw(0..6, 0..1),
                }
            }
        }
        self.queue.submit([enc.finish()]);
        self.stats = stats;
    }

    fn read_target(&self, t: &Target) -> Result<Vec<u8>, String> {
        let row = (4 * t.width).div_ceil(256) * 256;
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("read"),
            size: u64::from(row) * u64::from(t.height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = self.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: &t.color, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: None } },
            wgpu::Extent3d { width: t.width, height: t.height, depth_or_array_layers: 1 },
        );
        self.queue.submit([enc.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        buf.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| e.to_string())?;
        rx.recv().map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
        let data = buf.slice(..).get_mapped_range().map_err(|e| e.to_string())?;
        let mut out = Vec::with_capacity(4 * t.width as usize * t.height as usize);
        for y in 0..t.height as usize {
            out.extend_from_slice(&data[y * row as usize..][..4 * t.width as usize]);
        }
        Ok(out)
    }

    /// The view's picture as RGBA bytes, rows from the top. The colours are sRGB.
    pub fn read_pixels(&self) -> Result<Vec<u8>, String> {
        match &self.target {
            Some(t) => self.read_target(t),
            None => Err("the view has no size yet".to_string()),
        }
    }

    /// Draws the frame into a picture of its own size and returns it as RGBA bytes, rows from the top
    /// (screenshots). The view's picture is left as it is.
    pub fn render_image(&mut self, frame: &Frame, width: u32, height: u32) -> Result<Vec<u8>, String> {
        let t = self.make_target(width, height);
        self.draw_to(&t, frame);
        self.read_target(&t)
    }

    /// A picture of a model for the palette: `size` by `size` RGBA bytes, rows from the top, on a clear
    /// background. The colours are multiplied by the alpha (where the model covers half a pixel, the pixel holds
    /// half the colour and an alpha of a half). None for a model without a vertex.
    ///
    /// It is drawn at `size` pixels with the renderer's multisampling.
    pub fn render_thumbnail(&mut self, model: &Prefab, size: u32) -> Option<Vec<u8>> {
        let mut scene = Scene::new();
        let root = model.instance(&mut scene);
        let b = scene.bounds(root);
        if b.empty() {
            return None;
        }
        let rad = b.sphere_radius().max(1e-3);
        let mut cam = Camera::new(30.0, rad / 50.0, rad * 50.0);
        (cam.width, cam.height) = (f64::from(size), f64::from(size));
        let d = [1.0, 0.7, 1.2];
        cam.pos = vec::add(b.center(), vec::scale(d, rad / (std::f64::consts::PI / 12.0).sin() / vec::norm(d)));
        cam.look_at(b.center());
        let saved = (self.background, self.fog, self.ambient, self.stats);
        (self.background, self.fog, self.ambient) = ([0.0; 3], None, [1.0; 3]);
        let roots = [root];
        let out = self.render_image(&Frame { clear_alpha: 0.0, ..Frame::new(&scene, &roots, &cam) }, size, size);
        (self.background, self.fog, self.ambient, self.stats) = saved;
        out.ok()
    }
}
