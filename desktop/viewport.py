"""The 3D view: draws the level, and turns mouse and keys into camera moves, picks, box selections and gizmo drags.

Click: select · Ctrl+click: add/remove · B or Ctrl+Alt+drag: box select · drag: orbit · right drag: pan · wheel: zoom.
The fly camera (C): WASD moves along the view, Space and Shift move straight up and down, dragging turns the view,
and the wheel sets the speed.
"""
import math, time
import numpy as np
from PySide6.QtCore import QPoint, QRect, QSize, Qt, QTimer, Signal
from PySide6.QtGui import QColor, QFont, QImage, QPainter, QPen, QSurfaceFormat
from PySide6.QtOpenGLWidgets import QOpenGLWidget
from PySide6.QtWidgets import QLabel, QRubberBand
from . import vec
from .scene import Lines, Node, Sprite, Texture, raycast

FLY_KEYS = {Qt.Key_W: (0, 0, -1), Qt.Key_S: (0, 0, 1), Qt.Key_A: (-1, 0, 0), Qt.Key_D: (1, 0, 0), Qt.Key_Space: (0, 1, 0),
            Qt.Key_Shift: (0, -1, 0)}
HINTS = {
    "orbit": "Click: select · Ctrl+click: add/remove · B: box select · Ctrl+A: all · W/E/R: move/rotate/scale · Shift+click: drop garib · A: add · C: fly · P: path",
    "fly": "Flying · WASD: move · Space/Shift: up/down · drag: look · wheel: speed · click: select · C or Esc: stop",
}


def surface_format(samples=0):
    """OpenGL 3.3 core. Only the 3D view asks for multisampling: QOpenGLWidget draws into its own multisampled
    buffer, and a multisampled default format would also go to the window that shows it."""
    f = QSurfaceFormat()
    f.setVersion(3, 3)
    f.setProfile(QSurfaceFormat.CoreProfile)
    f.setDepthBufferSize(24)
    f.setSamples(samples)
    return f


def prepare_qt():
    """Call before the QApplication is made. Qt and PyOpenGL must use the same OpenGL: the graphics driver's
    (opengl32.dll from System32). Without this, Qt on Windows can switch to its own software OpenGL on a driver it
    distrusts, while PyOpenGL keeps calling the system's, and the view stays empty."""
    from PySide6.QtCore import QCoreApplication
    QCoreApplication.setAttribute(Qt.AA_UseDesktopOpenGL)
    QSurfaceFormat.setDefaultFormat(surface_format())


def gl_problem():
    """None when this computer gives the view OpenGL 3.3 core, or what is wrong, for the player. Qt gives a lower
    version without failing when the driver can't do 3.3, so the version is checked here, before any window opens."""
    from PySide6.QtGui import QOffscreenSurface, QOpenGLContext
    ctx = QOpenGLContext()
    ctx.setFormat(surface_format())
    surface = QOffscreenSurface()
    if not ctx.create():
        return "Windows could not start OpenGL."
    surface.setFormat(ctx.format())
    surface.create()
    if not ctx.makeCurrent(surface):
        return "Windows could not start OpenGL."
    name = version = "?"
    try:
        from . import render  # noqa: F401  (turns off PyOpenGL's error checks before OpenGL.GL is imported)
        from OpenGL import GL
        name = (GL.glGetString(GL.GL_RENDERER) or b"?").decode(errors="replace")
        version = (GL.glGetString(GL.GL_VERSION) or b"?").decode(errors="replace")
        f = ctx.format()
        if (f.majorVersion(), f.minorVersion()) < (3, 3) or f.profile() != QSurfaceFormat.CoreProfile:
            return "The graphics driver gives OpenGL %s (%s). The editor needs OpenGL 3.3." % (version, name)
        render.Renderer()   # compiles the shaders
    except Exception as e:
        return "OpenGL failed (%s, %s): %s" % (name, version, e)
    finally:
        ctx.doneCurrent()
    return None


GL_HELP = ("Update your graphics driver from the maker of your graphics card (NVIDIA, AMD or Intel). "
           "\"GDI Generic\" or \"Microsoft Basic Render Driver\" means Windows has no driver for it. "
           "In Remote Desktop or a virtual machine without 3D, start the editor on the computer itself.")


class Icons:
    """Round icons with a glyph, as textures (one per glyph and colour)"""

    def __init__(self):
        self.cache = {}

    def texture(self, glyph, color):
        key = glyph + color
        if key not in self.cache:
            img = QImage(64, 64, QImage.Format_RGBA8888)
            img.fill(Qt.transparent)
            p = QPainter(img)
            p.setRenderHint(QPainter.Antialiasing)
            p.setBrush(QColor(color))
            p.setPen(QPen(QColor("#000"), 4))
            p.drawEllipse(QPoint(32, 32), 26, 26)
            f = QFont()
            f.setBold(True)
            f.setPixelSize(30)
            p.setFont(f)
            p.setPen(QColor("#111"))
            p.drawText(QRect(0, 2, 64, 64), Qt.AlignCenter, glyph)
            p.end()
            ptr = img.constBits()
            arr = np.frombuffer(ptr, np.uint8, 64 * 64 * 4).reshape(64, 64, 4)[::-1].copy()  # GL rows run bottom up
            self.cache[key] = Texture(arr)
        return self.cache[key]

    def node(self, glyph, color, size=14, opacity=1.0):
        n = Node()
        n.items.append(Sprite(self.texture(glyph, color), size, opacity))
        return n


class Viewport(QOpenGLWidget):
    fly_changed = Signal(bool)

    def __init__(self, editor, parent=None):
        super().__init__(parent)
        self.ed = editor
        self.setFormat(surface_format(samples=4))
        self.setFocusPolicy(Qt.StrongFocus)
        self.setMouseTracking(True)
        self.setMinimumSize(200, 200)
        self.renderer = None
        self.icons = Icons()
        editor.icon_maker = self.icons.node
        self.sel_lines = Lines(np.zeros((0, 3)), (0.95, 0.76, 0.31), "segments")
        self.sel_node = Node("selection")
        self.sel_node.items.append(self.sel_lines)
        self.down = None
        self.button = None
        self.last = None
        self.box_start = None
        self.dragging_gizmo = False
        self.rubber = QRubberBand(QRubberBand.Rectangle, self)
        self.fly = {"on": False, "keys": set(), "speed": 800.0, "look": None, "last": time.time()}
        self.fly_timer = QTimer(self)
        self.fly_timer.setInterval(16)
        self.fly_timer.timeout.connect(self.fly_step)
        self.anim_timer = QTimer(self)
        self.anim_timer.setInterval(33)
        self.anim_timer.timeout.connect(self.animate)
        self.t0 = time.time()
        self.paused = False
        self._batch_key = None
        self.batches, self.batched = [], set()
        self.ctrl_held = False
        # The HUD: the level's name, the autosave chip and the hint line
        self.hud_level = QLabel(self)
        self.hud_level.setObjectName("hudLevel")
        self.hud_chip = QLabel("Original level", self)
        self.hud_chip.setObjectName("chip")
        self.hud_hint = QLabel(HINTS["orbit"], self)
        self.hud_hint.setObjectName("hudHint")
        self.loading = QLabel(self)
        self.loading.setObjectName("loading")
        self.loading.setAlignment(Qt.AlignCenter)
        self.loading.hide()
        for w in (self.hud_level, self.hud_chip, self.hud_hint):
            w.setAttribute(Qt.WA_TransparentForMouseEvents)
        editor.view_changed.connect(self.update)

    # -- GL

    def initializeGL(self):
        from .render import Renderer
        try:
            self.renderer = Renderer()
        except Exception as e:  # an error in a Qt override reaches no one: say it in the view, and log it
            import traceback
            traceback.print_exc()
            self.gl_error = "The 3D view could not start: %s" % e
            self.set_loading(self.gl_error)

    def resizeGL(self, w, h):
        self.ed.camera.width, self.ed.camera.height = max(1, self.width()), max(1, self.height())
        self.layout_hud()

    def layout_hud(self):
        self.hud_level.adjustSize()
        self.hud_level.move(10, 8)
        self.hud_chip.adjustSize()
        self.hud_chip.move(10, 10 + self.hud_level.height())
        self.hud_hint.setFixedWidth(max(100, self.width() - 20))
        self.hud_hint.setWordWrap(True)
        self.hud_hint.adjustSize()
        self.hud_hint.move(10, self.height() - self.hud_hint.height() - 8)
        self.loading.adjustSize()
        self.loading.resize(max(260, self.loading.width() + 40), 48)
        self.loading.move((self.width() - self.loading.width()) // 2, (self.height() - self.loading.height()) // 2)

    def paintGL(self):
        ed = self.ed
        if self.renderer is None or self.paused:
            return
        cam = ed.camera
        cam.width, cam.height = max(1, self.width()), max(1, self.height())
        r = self.renderer
        r.background = ed.background
        r.fog = ed.fog()
        r.ambient = ed.ambient
        self.update_sel_boxes()
        roots = [ed.world, self.sel_node, ed.path_node] + ([ed.far_sphere] if ed.far_sphere is not None else [])
        # Opaque meshes are merged per material once; what moves without a rebuild (a drag, the motion preview) is
        # left out and drawn on its own
        moving = set()
        if ed._drag:
            for it in ed._drag["items"]:
                moving.add(id(it["e"]))
                if it["e"].owner is not None:
                    moving.add(id(it["e"].owner))
        if ed.preview:
            moving |= {id(e) for e in ed.entities if e.animate}
        key = (id(ed.world), tuple(sorted(ed.hidden)), frozenset(moving))
        if key != self._batch_key:
            from .render import make_batches
            self._batch_key = key
            self.batches, self.batched = make_batches([ed.world], lambda n: n.entity is not None and id(n.entity) in moving)
            self.collect(roots)
        dpr = self.devicePixelRatioF()
        r.draw(roots, cam, round(self.width() * dpr), round(self.height() * dpr), overlay=[ed.gizmo.node(cam)],
               batches=self.batches, batched=self.batched)

    def collect(self, roots):
        """Frees the GPU copies of geometry no longer drawn (after the entities are built again)"""
        r = self.renderer
        live = {g.id for g, _ in self.batches}
        for root in roots:
            for n, _ in root.walk(visible_only=False):
                for it in n.items:
                    if not isinstance(it, Sprite):
                        live.add(getattr(it, "geometry", it).id)
        from OpenGL import GL
        for store in (r.geoms, r.lines):
            for k in [k for k in store if k not in live]:
                vao, bufs, _, _ = store.pop(k)
                GL.glDeleteVertexArrays(1, [vao])
                GL.glDeleteBuffers(len(bufs), bufs)

    def update_sel_boxes(self):
        pts = []
        for e in self.ed.selection[:400]:
            b = e.obj.box()
            if b.empty():
                continue
            c = b.corners()
            for i, j in ((0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)):
                pts += [c[i], c[j]]
        self.sel_lines.set_points(np.array(pts) if pts else np.zeros((0, 3)))

    def thumbnail(self, model, size=128):
        """A picture of a model, drawn into an offscreen buffer of this view's context"""
        from PySide6.QtOpenGL import QOpenGLFramebufferObject, QOpenGLFramebufferObjectFormat
        from .camera import Camera
        if self.renderer is None:
            return None
        root = Node()
        root.add(model.instance())
        box = root.box()
        if box.empty():
            return None
        self.makeCurrent()
        try:
            fmt = QOpenGLFramebufferObjectFormat()
            fmt.setAttachment(QOpenGLFramebufferObject.Depth)
            fbo = QOpenGLFramebufferObject(QSize(size, size), fmt)
            fbo.bind()
            rad = max(box.sphere_radius(), 1e-3)
            cam = Camera(30, rad / 50, rad * 50)
            cam.width = cam.height = size
            d = vec.v3(1, 0.7, 1.2)
            cam.pos = box.center() + d / np.linalg.norm(d) * rad / math.sin(math.pi / 12)
            cam.look_at(box.center())
            r = self.renderer
            saved = (r.background, r.fog, r.ambient)
            r.background, r.fog, r.ambient = (0, 0, 0), None, (1, 1, 1)
            r.draw([root], cam, size, size, clear_alpha=0.0)
            img = fbo.toImage(True)
            fbo.release()
            r.background, r.fog, r.ambient = saved
            img = img.convertToFormat(QImage.Format_ARGB32)
            return img.scaled(size // 2, size // 2, Qt.KeepAspectRatio, Qt.SmoothTransformation)
        finally:
            self.doneCurrent()

    # -- Mouse

    def mousePressEvent(self, e):
        ed = self.ed
        x, y = e.position().x(), e.position().y()
        self.down = (x, y)
        self.last = (x, y)
        self.button = e.button()
        self.setFocus()
        mods = e.modifiers()
        ctrl = bool(mods & (Qt.ControlModifier | Qt.MetaModifier))
        if e.button() == Qt.LeftButton and (ed.box_mode or (ctrl and mods & Qt.AltModifier)) and not ed.gizmo.axis:
            self.box_start = (x, y)
            return
        if e.button() == Qt.LeftButton and not ctrl and ed.gizmo.attached and ed.gizmo.begin(ed.camera, x, y):
            self.dragging_gizmo = True
            ed.begin_drag()
            return
        if self.fly["on"] and e.button() in (Qt.LeftButton, Qt.RightButton):
            self.fly["look"] = (x, y, vec.euler_yxz_from_quat(ed.camera.quat))

    def mouseMoveEvent(self, e):
        ed = self.ed
        x, y = e.position().x(), e.position().y()
        if self.dragging_gizmo:
            if ed.gizmo.move(ed.camera, x, y):
                ed.on_drag()
            return
        if self.box_start:
            x0, y0 = self.box_start
            self.rubber.setGeometry(QRect(QPoint(int(min(x0, x)), int(min(y0, y))), QPoint(int(max(x0, x)), int(max(y0, y)))))
            self.rubber.show()
            return
        if self.fly["look"] is not None:
            lx, ly, r = self.fly["look"]
            k = 0.004
            ed.camera.quat = vec.quat_from_euler_yxz(max(-1.55, min(1.55, r[0] - (y - ly) * k)), r[1] - (x - lx) * k, 0)
            self.update()
            return
        if self.down is not None and self.last is not None and not self.fly["on"]:
            dx, dy = x - self.last[0], y - self.last[1]
            if self.button == Qt.LeftButton:
                ed.orbit.rotate(dx, dy)
            elif self.button == Qt.RightButton:
                ed.orbit.pan(dx, dy)
            elif self.button == Qt.MiddleButton:
                ed.orbit.dolly(-dy / 10)
            self.last = (x, y)
            self.update()
            return
        if ed.gizmo.attached and ed.gizmo.hover(ed.camera, x, y):
            self.update()

    def mouseReleaseEvent(self, e):
        ed = self.ed
        x, y = e.position().x(), e.position().y()
        self.fly["look"] = None
        down, self.down = self.down, None
        self.last = None
        if self.dragging_gizmo:
            self.dragging_gizmo = False
            ed.gizmo.end()
            ed.end_drag()
            ed.update_pivot()
            return
        mods = e.modifiers()
        ctrl = bool(mods & (Qt.ControlModifier | Qt.MetaModifier))
        if self.box_start:
            x0, y0 = self.box_start
            self.box_start = None
            self.rubber.hide()
            if math.hypot(x - x0, y - y0) > 4:
                a, b = (min(x0, x), min(y0, y)), (max(x0, x), max(y0, y))
                hits = []
                for ent in ed.pickable():
                    if ent.kind == "land" and not (mods & Qt.AltModifier) and not self.box_includes_land():
                        continue
                    p = ed.camera.to_pixels(ent.obj.world_pos())
                    if p[2] < 1 and a[0] <= p[0] <= b[0] and a[1] <= p[1] <= b[1]:
                        hits.append(ent)
                ed.set_selection(ed.selection + hits if mods & Qt.ShiftModifier else hits)
                ed.set_box_mode(False)
                return
        if down is None or math.hypot(x - down[0], y - down[1]) > 4 or e.button() != Qt.LeftButton:
            return
        if ed.path["on"] and not self.fly["on"]:
            return ed.path_click(x, y)
        origin, d = ed.camera.ray(x, y)
        if mods & Qt.ShiftModifier and not ctrl and not self.fly["on"]:
            hits = raycast(ed.ground_objects(), origin, d, camera=ed.camera)
            if hits:
                ed.add_garib(hits[0].point + vec.v3(0, 12, 0))
            return
        objs = [x_.obj for x_ in ed.pickable()]
        hits = raycast(objs, origin, d, camera=ed.camera, line_px=5)
        picks = [ed.entity_of(h.node) for h in hits]
        picks = [p for p in picks if p is not None]
        picks.sort(key=lambda p: p.kind == "land")   # stable: nearest non-landscape first
        pick = picks[0] if picks else None
        if ctrl:
            ed.toggle_select(pick)
        else:
            ed.select(pick)

    def box_includes_land(self):
        return "land" not in self.ed.hidden and getattr(self, "include_land", lambda: False)()

    def wheelEvent(self, e):
        steps = e.angleDelta().y() / 120
        if self.fly["on"]:
            self.fly["speed"] = max(50, min(20000, self.fly["speed"] * (1.25 if steps > 0 else 0.8)))
            self.ed.set_status("Fly speed %d units per second" % round(self.fly["speed"]))
            return
        self.ed.orbit.dolly(steps)
        self.update()

    def contextMenuEvent(self, e):
        e.accept()

    # -- Fly camera

    def set_fly(self, on):
        ed = self.ed
        self.fly.update(on=on, look=None, last=time.time())
        self.fly["keys"].clear()
        if not on:
            # Orbit about a point ahead of the camera, as far away as the old target was
            d = max(100, min(3000, float(np.linalg.norm(ed.camera.pos - ed.orbit.target))))
            ed.orbit.target = ed.camera.pos + ed.camera.forward() * d
        self.hud_hint.setText(HINTS["fly" if on else "orbit"])
        self.layout_hud()
        self.fly_changed.emit(on)

    def fly_key(self, key, down):
        if key not in FLY_KEYS:
            return False
        if down:
            self.fly["keys"].add(key)
            self.fly["last"] = time.time()
            if not self.fly_timer.isActive():
                self.fly_timer.start()
        else:
            self.fly["keys"].discard(key)
        return True

    def fly_step(self):
        if not self.fly["keys"] or not self.fly["on"]:
            self.fly_timer.stop()
            return
        now = time.time()
        dt = min(0.1, now - self.fly["last"])
        self.fly["last"] = now
        v = np.zeros(3)
        for k in self.fly["keys"]:
            v += FLY_KEYS[k]
        if not v.any():
            return
        up = v[1]
        v[1] = 0
        v = vec.quat_rotate(self.ed.camera.quat, v)
        v[1] += up
        self.ed.camera.pos = self.ed.camera.pos + v / np.linalg.norm(v) * self.fly["speed"] * dt
        self.update()

    def focusOutEvent(self, e):
        self.fly["keys"].clear()
        super().focusOutEvent(e)

    # -- Motion preview

    def set_preview(self, on):
        self.ed.preview = on
        if on:
            self.t0 = time.time()
            self.anim_timer.start()
        else:
            self.anim_timer.stop()
            for x in self.ed.entities:
                if x.place:
                    x.place()
            self.update()

    def animate(self):
        if self.ed.gizmo.dragging:
            return
        t = time.time() - self.t0
        for e in self.ed.entities:
            if e.animate:
                e.animate(t)
        self.update()

    def set_loading(self, text):
        text = getattr(self, "gl_error", None) or text   # an OpenGL error stays in view
        if text:
            self.loading.setText(text)
            self.loading.show()
            self.layout_hud()
        else:
            self.loading.hide()
