"""Draws a scene (desktop/scene.py) with OpenGL 3.3 core: Lambert-shaded meshes with fog, lines (dashed or not) and
sprites. Lighting follows three.js's (physically based lights since three.js r155: Lambert's
albedo / π, an ambient and one directional light, sRGB output)."""
import numpy as np
import OpenGL
OpenGL.ERROR_CHECKING = False    # the checks cost more than the drawing; set before the first GL import
from OpenGL import GL
from . import vec
from .scene import Lines, Mesh, Sprite

MESH_VS = """#version 330 core
layout(location=0) in vec3 pos; layout(location=1) in vec3 nrm; layout(location=2) in vec2 uv; layout(location=3) in vec4 col;
uniform mat4 model, view, proj;
out vec3 vN; out vec2 vUV; out vec4 vCol; out float vDepth;
void main() {
  vec4 wv = view * model * vec4(pos, 1.0);
  vDepth = -wv.z; vN = normalize(mat3(view) * transpose(inverse(mat3(model))) * nrm); vUV = uv; vCol = col;
  gl_Position = proj * wv;
}"""
MESH_FS = """#version 330 core
in vec3 vN; in vec2 vUV; in vec4 vCol; in float vDepth;
uniform sampler2D tex; uniform bool hasTex, vcol, lit, fogOn; uniform vec3 color, ambient, sunDir, fogColor;
uniform float alphaTest, opacity, fogNear, fogFar;
out vec4 frag;
vec3 toSRGB(vec3 c) { return mix(c * 12.92, 1.055 * pow(c, vec3(1.0 / 2.4)) - 0.055, step(0.0031308, c)); }
void main() {
  vec4 c = vec4(color, opacity);
  if (hasTex) c *= texture(tex, vUV);
  if (vcol) c *= vCol;
  if (c.a < alphaTest) discard;
  vec3 rgb = c.rgb;
  if (lit) {
    vec3 n = normalize(vN); if (!gl_FrontFacing) n = -n;
    rgb = c.rgb / 3.14159265 * (ambient * 0.8 + vec3(1.4) * max(dot(n, sunDir), 0.0));
  }
  if (fogOn) rgb = mix(rgb, fogColor, smoothstep(fogNear, fogFar, vDepth));
  frag = vec4(toSRGB(rgb), c.a);
}"""
LINE_VS = """#version 330 core
layout(location=0) in vec3 pos; layout(location=1) in float dist;
uniform mat4 mvp; out float vDist;
void main() { vDist = dist; gl_Position = mvp * vec4(pos, 1.0); }"""
LINE_FS = """#version 330 core
in float vDist; uniform vec3 color; uniform float opacity, dash, gap; out vec4 frag;
vec3 toSRGB(vec3 c) { return mix(c * 12.92, 1.055 * pow(c, vec3(1.0 / 2.4)) - 0.055, step(0.0031308, c)); }
void main() { if (dash > 0.0 && mod(vDist, dash + gap) > dash) discard; frag = vec4(toSRGB(color), opacity); }"""
SPRITE_VS = """#version 330 core
layout(location=0) in vec2 corner;
uniform mat4 view, proj; uniform vec3 centre; uniform float size; out vec2 vUV;
void main() { vec4 c = view * vec4(centre, 1.0); c.xy += corner * size; vUV = corner + 0.5; gl_Position = proj * c; }"""
SPRITE_FS = """#version 330 core
in vec2 vUV; uniform sampler2D tex; uniform float opacity; out vec4 frag;
void main() { vec4 c = texture(tex, vUV); c.a *= opacity; if (c.a < 0.02) discard; frag = c; }"""


def srgb_to_linear(c):
    c = np.asarray(c, float)
    return np.where(c <= 0.04045, c / 12.92, ((c + 0.055) / 1.055) ** 2.4)


def compile_program(vs, fs):
    def shader(kind, src):
        s = GL.glCreateShader(kind)
        GL.glShaderSource(s, src)
        GL.glCompileShader(s)
        if not GL.glGetShaderiv(s, GL.GL_COMPILE_STATUS):
            raise RuntimeError(GL.glGetShaderInfoLog(s).decode())
        return s
    p = GL.glCreateProgram()
    for s in (shader(GL.GL_VERTEX_SHADER, vs), shader(GL.GL_FRAGMENT_SHADER, fs)):
        GL.glAttachShader(p, s)
    GL.glLinkProgram(p)
    if not GL.glGetProgramiv(p, GL.GL_LINK_STATUS):
        raise RuntimeError(GL.glGetProgramInfoLog(p).decode())
    return p


class Program:
    def __init__(self, vs, fs):
        self.id = compile_program(vs, fs)
        self.locs = {}

    def use(self):
        GL.glUseProgram(self.id)

    def loc(self, name):
        if name not in self.locs:
            self.locs[name] = GL.glGetUniformLocation(self.id, name)
        return self.locs[name]

    def mat4(self, name, m):
        GL.glUniformMatrix4fv(self.loc(name), 1, GL.GL_TRUE, np.asarray(m, np.float32))

    def mat3(self, name, m):
        GL.glUniformMatrix3fv(self.loc(name), 1, GL.GL_TRUE, np.asarray(m, np.float32))

    def vec3(self, name, v):
        GL.glUniform3f(self.loc(name), *[float(x) for x in v])

    def f(self, name, v):
        GL.glUniform1f(self.loc(name), float(v))

    def i(self, name, v):
        GL.glUniform1i(self.loc(name), int(v))


class Renderer:
    """GPU copies of geometry and textures, made on first use. Call with the GL context current."""

    def __init__(self):
        self.mesh = Program(MESH_VS, MESH_FS)
        self.line = Program(LINE_VS, LINE_FS)
        self.sprite = Program(SPRITE_VS, SPRITE_FS)
        self.geoms, self.textures, self.lines = {}, {}, {}
        self.background = (0.051, 0.055, 0.071)
        self.fog = None            # (rgb, near, far)
        self.ambient = (1.0, 1.0, 1.0)
        self.sun = vec.v3(0.4, 1, 0.3) / np.linalg.norm([0.4, 1, 0.3])
        quad = np.array([-.5, -.5, .5, -.5, .5, .5, -.5, -.5, .5, .5, -.5, .5], np.float32)
        self.quad_vao = GL.glGenVertexArrays(1)
        GL.glBindVertexArray(self.quad_vao)
        vbo = GL.glGenBuffers(1)
        GL.glBindBuffer(GL.GL_ARRAY_BUFFER, vbo)
        GL.glBufferData(GL.GL_ARRAY_BUFFER, quad.nbytes, quad, GL.GL_STATIC_DRAW)
        GL.glEnableVertexAttribArray(0)
        GL.glVertexAttribPointer(0, 2, GL.GL_FLOAT, False, 0, None)
        GL.glBindVertexArray(0)
        self.white = None
        from .scene import Node
        self.batch_node = Node("batches")

    # -- GPU resources

    def geometry(self, g):
        e = self.geoms.get(g.id)
        if e is not None and e[3] == g.version:
            return e
        vao = e[0] if e else GL.glGenVertexArrays(1)
        GL.glBindVertexArray(vao)
        bufs = e[1] if e else GL.glGenBuffers(5)
        n = len(g.positions)
        attrs = [(g.positions, 3), (g.normals if g.normals is not None else np.tile([0, 1, 0], (n, 1)), 3),
                 (g.uvs if g.uvs is not None else np.zeros((n, 2)), 2),
                 (g.colors if g.colors is not None else np.ones((n, 4)), 4)]
        for i, (data, width) in enumerate(attrs):
            data = np.ascontiguousarray(data, np.float32)
            GL.glBindBuffer(GL.GL_ARRAY_BUFFER, bufs[i])
            GL.glBufferData(GL.GL_ARRAY_BUFFER, data.nbytes, data, GL.GL_STATIC_DRAW)
            GL.glEnableVertexAttribArray(i)
            GL.glVertexAttribPointer(i, width, GL.GL_FLOAT, False, 0, None)
        count = 0
        if g.indices is not None:
            idx = np.ascontiguousarray(g.indices, np.uint32)
            GL.glBindBuffer(GL.GL_ELEMENT_ARRAY_BUFFER, bufs[4])
            GL.glBufferData(GL.GL_ELEMENT_ARRAY_BUFFER, idx.nbytes, idx, GL.GL_STATIC_DRAW)
            count = idx.size
        GL.glBindVertexArray(0)
        e = (vao, bufs, count if g.indices is not None else -n, g.version)
        self.geoms[g.id] = e
        return e

    def texture(self, t, wrap=(10497, 10497)):
        key = (t.id, wrap)
        if key not in self.textures:
            tid = GL.glGenTextures(1)
            GL.glBindTexture(GL.GL_TEXTURE_2D, tid)
            h, w = t.rgba.shape[:2]
            GL.glPixelStorei(GL.GL_UNPACK_ALIGNMENT, 1)
            GL.glTexImage2D(GL.GL_TEXTURE_2D, 0, GL.GL_SRGB8_ALPHA8 if t.srgb else GL.GL_RGBA8, w, h, 0, GL.GL_RGBA,
                            GL.GL_UNSIGNED_BYTE, t.rgba)
            GL.glGenerateMipmap(GL.GL_TEXTURE_2D)
            GL.glTexParameteri(GL.GL_TEXTURE_2D, GL.GL_TEXTURE_MIN_FILTER, GL.GL_LINEAR_MIPMAP_LINEAR)
            GL.glTexParameteri(GL.GL_TEXTURE_2D, GL.GL_TEXTURE_MAG_FILTER, GL.GL_LINEAR)
            GL.glTexParameteri(GL.GL_TEXTURE_2D, GL.GL_TEXTURE_WRAP_S, wrap[0])
            GL.glTexParameteri(GL.GL_TEXTURE_2D, GL.GL_TEXTURE_WRAP_T, wrap[1])
            self.textures[key] = tid
        return self.textures[key]

    def line_buffers(self, ln):
        e = self.lines.get(ln.id)
        if e is not None and e[3] == ln.version:
            return e
        a, b = ln.segments()
        pts = np.empty((len(a) * 2, 3), np.float32)
        pts[0::2], pts[1::2] = a, b
        # Distance along the line, for dashes: continuous along strips and loops
        seg = np.linalg.norm(b - a, axis=1)
        start = np.concatenate([[0.0], np.cumsum(seg)[:-1]]) if ln.mode != "segments" else np.zeros(len(a))
        dist = np.empty(len(a) * 2, np.float32)
        dist[0::2], dist[1::2] = start, start + seg
        vao = e[0] if e else GL.glGenVertexArrays(1)
        bufs = e[1] if e else GL.glGenBuffers(2)
        GL.glBindVertexArray(vao)
        for i, (data, width) in enumerate(((pts, 3), (dist, 1))):
            data = np.ascontiguousarray(data, np.float32)
            GL.glBindBuffer(GL.GL_ARRAY_BUFFER, bufs[i])
            GL.glBufferData(GL.GL_ARRAY_BUFFER, max(4, data.nbytes), data if data.size else None, GL.GL_DYNAMIC_DRAW)
            GL.glEnableVertexAttribArray(i)
            GL.glVertexAttribPointer(i, width, GL.GL_FLOAT, False, 0, None)
        GL.glBindVertexArray(0)
        e = (vao, bufs, len(pts), ln.version)
        self.lines[ln.id] = e
        return e

    def forget(self, keep_textures=()):
        """Frees every GPU copy (a new level)"""
        for vao, bufs, _, _ in list(self.geoms.values()) + list(self.lines.values()):
            GL.glDeleteVertexArrays(1, [vao])
            GL.glDeleteBuffers(len(bufs), bufs)
        self.geoms, self.lines = {}, {}
        drop = [k for k in self.textures if k[0] not in keep_textures]
        if drop:
            GL.glDeleteTextures([self.textures[k] for k in drop])
        for k in drop:
            del self.textures[k]

    # -- Drawing

    def draw(self, roots, camera, width, height, overlay=(), clear_alpha=1.0, batches=(), batched=frozenset()):
        """batches: (geometry, material) pairs already in world space, drawn first; batched: ids of the meshes
        they hold, which the walk skips"""
        GL.glViewport(0, 0, width, height)
        GL.glClearColor(*self.background, clear_alpha)   # sRGB, as three.js clears
        GL.glClear(GL.GL_COLOR_BUFFER_BIT | GL.GL_DEPTH_BUFFER_BIT)
        GL.glEnable(GL.GL_DEPTH_TEST)
        GL.glEnable(GL.GL_BLEND)
        # Colour blends by alpha; alpha itself only grows: the view the window composes stays opaque, and a
        # thumbnail drawn on a clear background keeps its shape
        GL.glBlendFuncSeparate(GL.GL_SRC_ALPHA, GL.GL_ONE_MINUS_SRC_ALPHA, GL.GL_ONE, GL.GL_ONE_MINUS_SRC_ALPHA)
        GL.glDisable(GL.GL_CULL_FACE)
        view, proj = camera.view(), camera.proj(width, height)
        opaque, clear, late = [], [], []
        ident = np.eye(4)
        for g, mat in batches:
            opaque.append((self.batch_node, ident, Mesh(g, mat)))
        for root in roots:
            for node, m in root.world_walk():
                for it in node.items:
                    if (id(node), id(it)) in batched:
                        continue
                    entry = (node, m @ it.matrix, it)
                    if node.order > 0 or getattr(it, "depth_test", True) is False or (
                            isinstance(it, Mesh) and not it.material.depth_test):
                        late.append(entry)
                    elif isinstance(it, Mesh) and not (it.material.transparent or it.material.opacity < 1):
                        opaque.append(entry)
                    else:
                        clear.append(entry)
        # Transparent things back to front, as three.js sorts them
        eye = camera.pos
        clear.sort(key=lambda e: -float(np.sum((e[1][:3, 3] - eye) ** 2)))
        late.sort(key=lambda e: e[0].order)
        self.draw_list(opaque, view, proj)
        self.draw_list(clear, view, proj)
        self.draw_list(late, view, proj)
        for root in overlay:
            GL.glClear(GL.GL_DEPTH_BUFFER_BIT)
            self.draw_list([(n, m @ it.matrix, it) for n, m in root.world_walk() for it in n.items], view, proj)

    def draw_list(self, entries, view, proj):
        current = None
        fog = self.fog
        for node, m, it in entries:
            if isinstance(it, Mesh):
                p = self.mesh
                if current is not p:
                    p.use()
                    p.mat4("view", view)
                    p.mat4("proj", proj)
                    p.vec3("ambient", srgb_to_linear(self.ambient))
                    p.vec3("sunDir", (view[:3, :3] @ self.sun))
                    p.i("fogOn", fog is not None)
                    if fog is not None:
                        p.vec3("fogColor", srgb_to_linear(fog[0]))
                        p.f("fogNear", fog[1])
                        p.f("fogFar", fog[2])
                    p.i("tex", 0)
                    current = p
                mat = it.material
                vao, _, count, _ = self.geometry(it.geometry)
                p.mat4("model", m)
                p.vec3("color", srgb_to_linear(mat.color) if not mat.lit else mat.color)
                p.f("opacity", mat.opacity)
                p.f("alphaTest", mat.alpha_test)
                p.i("vcol", mat.vertex_colors)
                p.i("lit", mat.lit)
                p.i("hasTex", mat.texture is not None)
                if mat.texture is not None:
                    GL.glActiveTexture(GL.GL_TEXTURE0)
                    GL.glBindTexture(GL.GL_TEXTURE_2D, self.texture(mat.texture, mat.wrap))
                GL.glDepthMask(bool(mat.depth_write))
                (GL.glEnable if mat.depth_test and node.order <= 0 else GL.glDisable)(GL.GL_DEPTH_TEST)
                GL.glBindVertexArray(vao)
                if count >= 0:
                    GL.glDrawElements(GL.GL_TRIANGLES, count, GL.GL_UNSIGNED_INT, None)
                else:
                    GL.glDrawArrays(GL.GL_TRIANGLES, 0, -count)
            elif isinstance(it, Lines):
                p = self.line
                if current is not p:
                    p.use()
                    current = p
                vao, _, n, _ = self.line_buffers(it)
                if not n:
                    continue
                p.mat4("mvp", proj @ view @ m)
                p.vec3("color", srgb_to_linear(it.color))
                p.f("opacity", it.opacity)
                p.f("dash", it.dash[0] if it.dash else 0.0)
                p.f("gap", it.dash[1] if it.dash else 0.0)
                GL.glDepthMask(True)
                (GL.glEnable if it.depth_test and node.order <= 0 else GL.glDisable)(GL.GL_DEPTH_TEST)
                GL.glBindVertexArray(vao)
                GL.glDrawArrays(GL.GL_LINES, 0, n)
            elif isinstance(it, Sprite):
                p = self.sprite
                if current is not p:
                    p.use()
                    p.mat4("view", view)
                    p.mat4("proj", proj)
                    p.i("tex", 0)
                    current = p
                p.vec3("centre", m[:3, 3])
                p.f("size", it.size * (vec.decompose_scale(m)[0] or 1.0))
                p.f("opacity", it.opacity)
                GL.glActiveTexture(GL.GL_TEXTURE0)
                GL.glBindTexture(GL.GL_TEXTURE_2D, self.texture(it.texture, (GL.GL_CLAMP_TO_EDGE, GL.GL_CLAMP_TO_EDGE)))
                GL.glDepthMask(True)
                (GL.glEnable if it.depth_test and node.order <= 0 else GL.glDisable)(GL.GL_DEPTH_TEST)
                GL.glBindVertexArray(self.quad_vao)
                GL.glDrawArrays(GL.GL_TRIANGLES, 0, 6)
        GL.glBindVertexArray(0)
        GL.glDepthMask(True)
        GL.glEnable(GL.GL_DEPTH_TEST)


def batchable(node, it):
    return isinstance(it, Mesh) and node.order <= 0 and it.material.depth_test and not (it.material.transparent or it.material.opacity < 1)


def make_batches(roots, skip=lambda node: False):
    """Merges the opaque meshes under roots into one world-space geometry per material. Returns the batches and the
    (node, mesh) id pairs they hold. skip(node) leaves a subtree out (things that move without a rebuild)."""
    from .scene import Geometry
    groups = {}
    held = set()

    def walk(node, m):
        if not node.visible or skip(node):
            return
        for it in node.items:
            if batchable(node, it):
                mm = m @ it.matrix
                groups.setdefault(id(it.material), (it.material, []))[1].append((it.geometry, mm))
                held.add((id(node), id(it)))
        for c in node.children:
            walk(c, m @ c.local_matrix())
    for root in roots:
        base = root.parent.world_matrix() if root.parent is not None else np.eye(4)
        walk(root, base @ root.local_matrix())
    out = []
    for mat, parts in groups.values():
        pos, nrm, uv, col, idx = [], [], [], [], []
        n = 0
        for g, mm in parts:
            p = g.positions.astype(np.float64) @ mm[:3, :3].T + mm[:3, 3]
            nm = np.linalg.inv(mm[:3, :3]).T if abs(np.linalg.det(mm[:3, :3])) > 1e-12 else np.eye(3)
            q = (g.normals if g.normals is not None else np.tile([0, 1, 0], (len(p), 1))) @ nm.T
            q /= np.maximum(np.linalg.norm(q, axis=1, keepdims=True), 1e-9)
            pos.append(p)
            nrm.append(q)
            uv.append(g.uvs if g.uvs is not None else np.zeros((len(p), 2)))
            col.append(g.colors if g.colors is not None else np.ones((len(p), 4)))
            t = g.indices if g.indices is not None else np.arange(len(p) // 3 * 3).reshape(-1, 3)
            idx.append(t + n)
            n += len(p)
        out.append((Geometry(np.concatenate(pos), np.concatenate(idx), np.concatenate(nrm), np.concatenate(uv),
                             np.concatenate(col)), mat))
    return out, held
