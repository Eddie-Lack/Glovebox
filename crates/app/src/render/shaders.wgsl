// The 3D view's shaders: meshes, lines and sprites. A port of the GLSL of the former Python editor's render.py.
//
// Lighting follows three.js r160: Lambert's albedo / pi, an ambient light of 0.8 and a sun of 1.4, sRGB output.
// The shaders write sRGB values themselves. The target is not an sRGB format, and blending happens on the sRGB
// values, as in the OpenGL view.

struct Globals {
    view: mat4x4f,
    proj: mat4x4f,
    ambient: vec4f,
    // The sun's direction in view space
    sun: vec4f,
    fog_color: vec4f,
    // near, far, 1 when the fog is on
    fog: vec4f,
}

struct Draw {
    // The model matrix. For lines it is projection * view * model.
    model: mat4x4f,
    // The normal matrix (view * inverse transpose of model), one column each
    n0: vec4f,
    n1: vec4f,
    n2: vec4f,
    // rgb and opacity
    color: vec4f,
    // alpha test, dash, gap, sprite size
    params: vec4f,
    // has a texture, uses vertex colours, is lit
    flags: vec4u,
    // A sprite's centre in the world
    centre: vec4f,
}

@group(0) @binding(0) var<uniform> g: Globals;
@group(1) @binding(0) var<uniform> d: Draw;
@group(2) @binding(0) var tex: texture_2d<f32>;
@group(2) @binding(1) var samp: sampler;

// The camera's projection is OpenGL's, with depth from -1 to 1. wgpu's depth runs from 0 to 1.
fn clip(p: vec4f) -> vec4f {
    return vec4f(p.x, p.y, (p.z + p.w) * 0.5, p.w);
}

fn to_srgb(c: vec3f) -> vec3f {
    return mix(c * 12.92, 1.055 * pow(c, vec3f(1.0 / 2.4)) - 0.055, step(vec3f(0.0031308), c));
}

struct MeshOut {
    @builtin(position) pos: vec4f,
    @location(0) n: vec3f,
    @location(1) uv: vec2f,
    @location(2) col: vec4f,
    @location(3) depth: f32,
}

@vertex
fn mesh_vs(@location(0) pos: vec3f, @location(1) nrm: vec3f, @location(2) uv: vec2f, @location(3) col: vec4f) -> MeshOut {
    var o: MeshOut;
    let wv = g.view * d.model * vec4f(pos, 1.0);
    o.depth = -wv.z;
    o.n = normalize(mat3x3f(d.n0.xyz, d.n1.xyz, d.n2.xyz) * nrm);
    o.uv = uv;
    o.col = col;
    o.pos = clip(g.proj * wv);
    return o;
}

@fragment
fn mesh_fs(in: MeshOut, @builtin(front_facing) front: bool) -> @location(0) vec4f {
    let texel = textureSample(tex, samp, in.uv);
    var c = d.color;
    if d.flags.x != 0u {
        c *= texel;
    }
    if d.flags.y != 0u {
        c *= in.col;
    }
    if c.a < d.params.x {
        discard;
    }
    var rgb = c.rgb;
    if d.flags.z != 0u {
        var n = normalize(in.n);
        if !front {
            n = -n;
        }
        rgb = c.rgb / 3.14159265 * (g.ambient.rgb * 0.8 + vec3f(1.4) * max(dot(n, g.sun.xyz), 0.0));
    }
    if g.fog.z != 0.0 {
        rgb = mix(rgb, g.fog_color.rgb, smoothstep(g.fog.x, g.fog.y, in.depth));
    }
    return vec4f(to_srgb(rgb), c.a);
}

struct LineOut {
    @builtin(position) pos: vec4f,
    @location(0) dist: f32,
}

@vertex
fn line_vs(@location(0) pos: vec3f, @location(1) dist: f32) -> LineOut {
    var o: LineOut;
    o.dist = dist;
    o.pos = clip(d.model * vec4f(pos, 1.0));
    return o;
}

@fragment
fn line_fs(in: LineOut) -> @location(0) vec4f {
    let dash = d.params.y;
    let period = dash + d.params.z;
    if dash > 0.0 && in.dist - period * floor(in.dist / period) > dash {
        discard;
    }
    return vec4f(to_srgb(d.color.rgb), d.color.a);
}

struct SpriteOut {
    @builtin(position) pos: vec4f,
    @location(0) uv: vec2f,
}

@vertex
fn sprite_vs(@builtin(vertex_index) i: u32) -> SpriteOut {
    var corners = array<vec2f, 6>(vec2f(-0.5, -0.5), vec2f(0.5, -0.5), vec2f(0.5, 0.5), vec2f(-0.5, -0.5), vec2f(0.5, 0.5), vec2f(-0.5, 0.5));
    let corner = corners[i];
    var o: SpriteOut;
    var c = g.view * vec4f(d.centre.xyz, 1.0);
    c = vec4f(c.xy + corner * d.params.w, c.zw);
    o.uv = corner + 0.5;
    o.pos = clip(g.proj * c);
    return o;
}

// The texel is written as it is sampled, without the sRGB step, as the OpenGL view did.
@fragment
fn sprite_fs(in: SpriteOut) -> @location(0) vec4f {
    var c = textureSample(tex, samp, in.uv);
    c.a *= d.color.a;
    if c.a < 0.02 {
        discard;
    }
    return c;
}
