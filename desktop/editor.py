"""The editor's state and every edit, without the widgets: loading a level, selection, the transform engine, undo,
autosave, adding and removing things, the checks, imports and the memory budget, the path tool and building.

Every edit is a change to the level XML with an undo snapshot; the entities are then built again from the XML.
Widgets (desktop/window.py) listen to the signals and call these methods.
"""
import math, random, time
import numpy as np
from PySide6.QtCore import QObject, Signal
from . import vec, xmlutil
from .camera import Camera, Orbit
from .entities import (AXES, CAM_PRESETS, MOD_PARENTS, TRIGGERS, WHY, Builder, attached_points, camera_zone, lone_trigger,
                       preset_zone, set_camera_zone, trigger_volumes, uses_mod)
from .gizmo import Gizmo
from .sanitize import sanitize_level_xml
from .scene import Lines, Node, mesh_node, raycast, ring_points, sphere_geometry, Material
from .commands import ensure_cmd
from .xmlutil import closest, first, fmt, hex_id, id_hex, kids, make_el, num, path_of, node_at, r1, inum

GITHUB = "https://github.com/Eddie-Lack/gloveworld/blob/"
DEG = 180 / math.pi

# Draw load. The game draws every actor within the far plane of the camera (IsActorVisible in object.c: the actor's
# radius plus farPlaneDist against its distance, with no test against the view), behind the camera too. Each drawn
# object node takes one of the frame's 256 object matrices (object.c), and so do shadows and footprints. Past 256 they
# overwrite the camera's matrices and the frame never finishes; long before that the game slows down. The far plane is
# FogConfiguration's near_clip (parse.c TOKEN_FOG; 1000 without one, game.c).
MATRICES = 256
# Measured in the emulator (tools/draw_load_check.py): the shipped hub 1 asks for 228 at its fountain, and a hub built
# with the editor lagged on a console at 206 to 228. The estimate was within 35 of the game's count in four levels, and
# up to 63 over in hub 1.
LOAD_SLOW, LOAD_STOP = 190, 240
LOAD_BASE = 24                      # Glover, the ball, their shadows and footprints
LOAD_KINDS = {"land", "platform", "enemy", "pickup", "water"}
LOAD_CAM_BACK, LOAD_CAM_UP = 130, 50   # measured in the emulator (tools/draw_load_check.py)

MOD_NOTE = "Build ROM and Export patch add the Expansion Pak game mod for this, and the ROM then needs the Expansion Pak."
PATH_COPY = {"LandActor", "BackgroundActor", "AnimatedBackgroundActor", "Enemy", "Powerup", "MrTip", "Platform"}
PATH_MAX = 300


# The level-wide commands the Level settings list shows, and their names in its Add menu
LOOK = {"FogConfiguration": "Fog", "AmbientLight": "Ambient light", "DiffuseLight": "Directional light",
        "Backdrop": "Backdrop", "SetGravity": "Gravity", "AmbientSound": "Ambient sound"}
LOOK_ONE = {"FogConfiguration", "SetGravity", "AmbientLight"}
# Settings every level keeps one of: the game keeps the last screen's fog without one, and every shipped level has
# both
LOOK_KEEP = {"FogConfiguration", "AmbientLight"}
LIGHTS_MAX = 7      # AddBackgroundLight ignores the 8th and later (light.c)
ENEMIES_MAX = 30    # the enemies[] array (0x5A00 bytes of 0x300), and LoadEnemy doesn't check
TRIO = ("joff", "cancer", "kirk")   # the Atlantis boss: the game reads enemies[0], [1] and [2] as these
# The script a new enemy gets when no shipped one exists for its type: rest forever. An enemy with no normal
# instruction crashes the parser (NMEGetNewInstruction reads normalList[0]).
REST = ("EnemyInstruction", {"instr_type": "4", "lifetime": "0", "execution_condition_param_a": "0.0",
                             "execution_condition_param_b": "0.0", "flags": "0", "execution_condition": "always"},
        "EnemyInstructionRest", {"flags": "0", "anim_start_playing": "1"})
GROUPS = ("NormalInstructions", "ConditionalInstructions", "AttackInstructions")
EXTRA_SLOTS = (35, 36, 37, 38, 41)   # the Space slots mod/slots.c can swap for extra levels
ENTRY_TAG = 99                        # the platform tag of a hub's entry zone to the extra levels
# Look presets: ambient light, two directional lights and fog (r, g, b, fog start, far plane). The far
# plane is 1150 in the course looks: levels built with 1600 lagged on a console.
LOOKS = {
    "Graveyard violet": ((140, 120, 150), [(210, 235, 190, 0.0, 3.7), (230, 140, 90, 4.28, 0.0)], (26, 10, 36, 860, 1150)),
    "Frozen green": ((120, 140, 140), [(190, 235, 240, 0.0, 3.7), (220, 90, 90, 4.28, 0.0)], (10, 24, 20, 860, 1150)),
    "Blood red": ((150, 110, 120), [(240, 120, 110, 0.0, 3.7), (150, 110, 200, 4.28, 0.0)], (40, 6, 8, 860, 1150)),
    "Rotting green arena": ((160, 175, 150), [(200, 235, 170, 0.0, 3.7), (235, 110, 90, 4.28, 0.0)], (10, 30, 16, 980, 1900)),
    "Burning dark": ((150, 120, 110), [(250, 160, 80, 0.0, 3.7), (150, 90, 200, 4.28, 0.0)], (34, 16, 6, 860, 1150)),
    "Daylight (shipped levels)": ((110, 110, 150), [(160, 160, 160, 0.0, 3.7)], (40, 40, 60, 996, 1500)),
}


class Check:
    """A line in the Checks panel: level (ok, note, warn, err, imp), text, what clicking it does, a source link"""

    def __init__(self, level, text, on_click=None, link=None, sub=None, title=None):
        self.level, self.text, self.on_click, self.link, self.sub, self.title = level, text, on_click, link, sub, title


class Editor(QObject):
    status = Signal(str, str)            # message, class (ok, err or "")
    selection_changed = Signal()
    rebuilt = Signal()                   # the entities were built again (after a load or an edit)
    edited = Signal()                    # the level changed (undo state, autosave chip)
    loading = Signal(str)                # "" hides the loading note
    view_changed = Signal()              # something in the 3D view moved
    imports_changed = Signal()
    scene_loaded = Signal()
    mode_changed = Signal()

    def __init__(self, backend, drafts, settings=None):
        super().__init__()
        self.backend, self.drafts, self.settings = backend, drafts, settings
        self.scenes, self.decomp, self.tables, self.templates, self.heap = [], None, {}, {}, {}
        self.puzzle_index = None          # the puzzle the inspector shows when nothing is selected (its place among
                                          # the level's puzzles: undo rebuilds the XML)
        self.catalog, self.mod_info = {}, None
        self.scene = None
        self.root = None                  # the level XML (lxml root)
        self.models, self.loaded = {}, set()
        self.entities, self.selection = [], []
        self.undo_stack, self.redo_stack = [], []
        self.hidden = set()
        self.orig_lev_size = None
        self.filter = ""
        self.pivot_mode = "group"
        self.preview = False
        self.show_fog = True
        self.level_fog = None
        self.background = (0.051, 0.055, 0.071)
        self.ambient = (1.0, 1.0, 1.0)
        self.orig_xml = None
        self.orig_xml_for = {}
        self.orig_violations = []
        self.violations = []
        self.draft_saved_at = None
        self.copy_imports = False
        self.plan = None
        self.mode = "translate"
        self.snap = 1.0
        self.box_mode = False
        self.world = Node("world")
        self.sel_boxes = []
        self.far_sphere = None
        self.path = {"on": False, "pts": []}
        self.path_node = Node("path")
        self.camera = Camera()
        self.orbit = Orbit(self.camera)
        self.gizmo = Gizmo()
        self.model_cache = {}
        self.icon_maker = None            # set by the window: (glyph, color, size, opacity) -> Node
        self._drag = None
        self.kit = None

    # -----------------------------------------------------------------------------------------------------------
    # Tables

    def start(self):
        """Loads the level list and the tables (call before load_scene)"""
        self.scenes = self.backend.scenes()
        self.decomp = self.backend.decomp()
        self.tables = self.backend.tables()
        self.templates = self.backend.templates()
        self.heap = self.backend.heap()
        self.mod_info = self.backend.mod_info()
        from .commands import Format
        self.format = Format(self.decomp)

    def type_for(self, tag):
        return self.decomp["commands"].get(tag) or self.decomp["types"].get(tag)

    def command_for(self, node):
        c = self.decomp["commands"].get(node.tag)
        if c:
            return node.tag, c
        parent = node.getparent()
        ptag = parent.tag if parent is not None else None
        for name, c in self.decomp["commands"].items():
            if c.get("groupsInto") and c["groupsInto"] == ptag:
                return name, c
        return None, None

    def ksy_enum_for(self, tag, field):
        c = self.type_for(tag)
        f = next((x for x in (c or {}).get("fields", []) if x["id"] == field), None)
        if not f or not f.get("enum"):
            return None
        key = f["enum"] if "::" in f["enum"] else c["ksy"] + "::" + f["enum"]
        return self.decomp["ksyEnums"].get(key)

    def enum_value(self, tag, field, name):
        en = self.ksy_enum_for(tag, field)
        if en:
            for k, v in en.items():
                if v == name:
                    return int(k)
        try:
            return int(name, 10)
        except (TypeError, ValueError):
            return None

    def decomp_enum_name(self, enum, value):
        for k, v in (self.decomp["decompEnums"].get(enum) or {}).items():
            if v == value:
                return k
        return None

    def enemy_model_id(self, type_name):
        tv = self.enum_value("Enemy", "type", type_name)
        if type_name and type_name.startswith("hubchicken"):
            return id_hex({"00": 0x11CD1E6C, "01": 0x11CD1E6C, "02": 0x11CD1E6C, "03": 0x58C079E1,
                           "04": 0x58C079E1}.get(self.scene["id"], 0x83D7D176))
        objs = self.tables.get("enemyObjects", [])
        v = objs[tv]["value"] if tv is not None and 0 <= tv < len(objs) else None
        return id_hex(v) if v else None

    def powerup_model_id(self, t):
        objs = self.tables.get("powerupObjects", [])
        v = objs[t]["value"] if 0 <= t < len(objs) else None
        return id_hex(v) if v else None

    # -----------------------------------------------------------------------------------------------------------
    # Models. Models outside this level's banks come from the catalog of every bank in the game and are imported
    # into the level's bank at ROM build.

    def model_url(self, oid):
        if not oid:
            return None
        return self.models.get(oid) or (self.catalog.get(oid) or {}).get("url")

    def is_import(self, oid):
        """'Loaded' = in a bank both the game's LoadLevel and noclip's scene table load for this level"""
        return bool(oid) and oid not in self.loaded and oid in self.catalog

    def load_model(self, oid):
        from . import gltf
        url = self.model_url(oid)
        if not url:
            return None
        if url not in self.model_cache:
            try:
                self.model_cache[url] = gltf.load(self.backend.fetch(url), self.backend.fetch, url)
            except Exception as e:  # a model that won't load shows as a placeholder
                print("model", oid, e)
                self.model_cache[url] = None
        return self.model_cache[url]

    def icon(self, glyph, color, size=14, opacity=1.0):
        return self.icon_maker(glyph, color, size, opacity)

    # -----------------------------------------------------------------------------------------------------------
    # The document

    def serialize(self):
        return xmlutil.serialize(self.root)

    def top_level(self):
        return kids(self.root)

    def end_marker(self):
        return first(self.root, "EndLevelData")

    def insert_top(self, node):
        end = self.end_marker()
        if end is None:
            self.root.append(node)
        else:
            end.addprevious(node)
        return node

    def restore(self, xml):
        self.root = xmlutil.parse(xml)

    # -----------------------------------------------------------------------------------------------------------
    # Entities

    def build_entities(self):
        self.gizmo.detach()
        world, ents, fog, amb = Builder(self).build(self.root)
        self.world, self.entities = world, ents
        if fog is not None:
            c = (num(fog, "r") / 255, num(fog, "g") / 255, num(fog, "b") / 255)
            self.background = c
            self.level_fog = (c, 800, max(2500, num(fog, "fog_distance") * 4)) if num(fog, "fog_enabled") else None
        else:
            self.background, self.level_fog = (0.051, 0.055, 0.071), None
        self.ambient = (num(amb, "r") / 255 + 0.35, num(amb, "g") / 255 + 0.35, num(amb, "b") / 255 + 0.35) if amb is not None else (1, 1, 1)
        self.apply_visibility()

    def fog(self):
        """The level's FogConfiguration is drawn by default; the toolbar switch hides it in the editor view only
        (the level data, and the fog in the game, are unchanged)"""
        return self.level_fog if self.show_fog else None

    def apply_visibility(self):
        for e in self.entities:
            v = e.kind not in self.hidden
            e.obj.visible = v
            if e.extra is not None:
                e.extra.visible = v
            if e.marks is not None:
                e.marks.visible = v and getattr(e.marks, "on", False)
        self.view_changed.emit()

    def pickable(self):
        return [x for x in self.entities if x.kind not in self.hidden and not x.derived]

    @staticmethod
    def entity_of(node):
        while node is not None:
            if node.entity is not None:
                return node.entity
            node = node.parent
        return None

    def place_ent(self, e):
        if e.place:
            e.place()
        else:
            e.obj.pos = e.bind.get()
        if e.owner is not None and e.owner.place:
            e.owner.place()

    # -----------------------------------------------------------------------------------------------------------
    # Selection

    def select(self, ent, focus=False):
        self.set_selection([ent] if ent else [])
        if focus and ent:
            self.focus_on(ent)

    def toggle_select(self, ent):
        if not ent:
            return
        self.set_selection([x for x in self.selection if x is not ent] if ent in self.selection else self.selection + [ent])

    def set_selection(self, items):
        seen, out = set(), []
        for e in items:
            if e is not None and id(e) not in seen:
                seen.add(id(e))
                out.append(e)
        self.selection = out
        if out:
            self.puzzle_index = None
        if not self.box_mode:
            self.update_pivot()
        self.selection_changed.emit()
        self.view_changed.emit()

    @property
    def selected(self):
        return self.selection[-1] if self.selection else None

    def range_select(self, items, e):
        a = items.index(self.selected) if self.selected in items else -1
        if a < 0:
            return self.select(e)
        b = items.index(e)
        self.set_selection(self.selection + items[min(a, b):max(a, b) + 1])

    def transform_set(self):
        """Objects the transform applies to: path handles whose platform is also selected and child platforms whose
        parent is selected are left out (they're stored relative to it and would move twice)"""
        sel = set(map(id, self.selection))
        return [e for e in self.selection if e.bind and not e.derived and not (e.is_handle and id(e.owner) in sel)
                and not (e.parent_ent is not None and id(e.parent_ent) in sel)]

    def update_pivot(self):
        self.gizmo.detach()
        tset = self.transform_set()
        if not tset:
            self.view_changed.emit()
            return
        single = tset[0] if len(tset) == 1 else None
        if single and self.mode == "rotate" and not single.rot:
            self.set_status("%s: %s" % (single.label, single.why.get("rot") or WHY["noOrientation"]))
            return
        if single and self.mode == "scale" and not single.scl:
            self.set_status("%s: %s" % (single.label, single.why.get("scl") or WHY["noScale"]))
            return
        pos = single.bind.get() if single else sum(e.bind.get() for e in tset) / len(tset)
        # A single object rotates and scales about its own axes
        quat = single.get_quat() if single and single.get_quat else vec.quat_identity()
        self.gizmo.mode = self.mode
        self.gizmo.space = "world" if self.mode == "translate" else (
            "local" if single and single.rot == "xyz" else ("local" if self.mode == "scale" else "world"))
        self.gizmo.snap = {"translate": self.snap or None, "rotate": math.radians(15) if self.snap else None,
                           "scale": 0.05 if self.snap else None}
        self.gizmo.attach(pos, quat)
        self.view_changed.emit()

    def set_mode(self, m):
        self.mode = m
        self.update_pivot()
        self.mode_changed.emit()

    def set_box_mode(self, on):
        self.box_mode = on
        # The gizmo is detached while box-selecting. It would otherwise grab a drag that starts on it.
        if on:
            self.gizmo.detach()
        else:
            self.update_pivot()
        self.mode_changed.emit()

    def focus_on(self, ent):
        box = vec.Box()
        for e in (self.selection if len(self.selection) > 1 else [ent]):
            box.add_box(e.obj.box())
        c = ent.obj.world_pos() if box.empty() else box.center()
        r = 60 if box.empty() else max(60, float(np.linalg.norm(box.size())))
        d = self.camera.pos - self.orbit.target
        d = d / (np.linalg.norm(d) or 1)
        self.orbit.target = c
        self.camera.pos = c + d * r * 1.4
        self.camera.look_at(c)
        self.view_changed.emit()

    def frame_level(self):
        box = vec.Box()
        for e in self.entities:
            if e.kind in ("land", "platform"):
                box.add_box(e.obj.box())
        if box.empty():
            for e in self.entities:
                box.add_box(e.obj.box())
        if box.empty():
            return
        c, r = box.center(), float(np.linalg.norm(box.size()))
        self.orbit.target = c
        d = vec.v3(0.55, 0.6, 0.75)
        self.camera.pos = c + d / np.linalg.norm(d) * r * 0.7
        self.camera.look_at(c)

    # -----------------------------------------------------------------------------------------------------------
    # Transform engine: the gizmo moves the pivot; every change re-applies the total delta since the drag began.

    def begin_drag(self):
        tset = self.transform_set()
        self._drag = {
            "before": self.serialize(), "set": tset, "p0": self.gizmo.pos.copy(), "q0": self.gizmo.quat.copy(),
            "items": [{"e": e, "p": e.bind.get(), "pts": [b.get() for b in e.points], "q": e.get_quat() if e.get_quat else None,
                       "s": e.get_scale() if e.get_scale else None, "box": e.box_size0() if e.box_size0 else None} for e in tset],
        }

    def on_drag(self):
        dr = self._drag
        if not dr:
            return
        mode = self.gizmo.mode
        group = len(dr["set"]) > 1 and self.pivot_mode == "group"
        p0 = dr["p0"]

        def move_item(it, T):
            it["e"].bind.set(T(it["p"]))
            for b, p in zip(it["e"].points, it["pts"]):
                b.set(T(p))
        if mode == "translate":
            d = self.gizmo.pos - p0
            for it in dr["items"]:
                move_item(it, lambda p: p + d)
        elif mode == "rotate":
            dq = vec.quat_mul(self.gizmo.quat, vec.quat_inv(dr["q0"]))
            axis = self.gizmo.axis.lower() if self.gizmo.axis in ("X", "Y", "Z") else None
            for it in dr["items"]:
                if group:
                    move_item(it, lambda p: p0 + vec.quat_rotate(dq, p - p0))
                if it["q"] is not None and it["e"].set_quat:
                    it["e"].set_quat(vec.quat_mul(dq, it["q"]), axis)
        elif mode == "scale":
            f = self.gizmo.scale.copy()
            for it in dr["items"]:
                if group:
                    move_item(it, lambda p: p0 + (p - p0) * f)
                if it["e"].scl == "xyz" and it["s"] is not None:
                    it["e"].set_scale(it["s"] * f)
                elif it["e"].scl == "box" and it["box"] is not None:
                    it["e"].set_box(it["box"], f)
        for it in dr["items"]:
            self.place_ent(it["e"])
        self.selection_changed.emit()   # the inspector's values
        self.view_changed.emit()

    def end_drag(self):
        dr, self._drag = self._drag, None
        if dr and dr["before"] != self.serialize():
            self.push_undo(dr["before"])
            n, mode = len(dr["set"]), self.gizmo.mode
            unsupported = len([e for e in dr["set"] if (mode == "rotate" and not e.rot) or (mode == "scale" and not e.scl)])
            self.after_edit()
            if unsupported:
                self.set_status("%s %d objects; %d can’t store %s and were only moved around the pivot." % (
                    "Rotated" if mode == "rotate" else "Scaled", n, unsupported, "a rotation" if mode == "rotate" else "a size"))

    def apply_transform(self, move=None, rot_deg=None, axis="y", scale=None):
        """Numeric transforms (inspector), applied the same way as a gizmo drag"""
        tset = self.transform_set()
        if not tset:
            return

        def fn():
            c = sum(e.bind.get() for e in tset) / len(tset)
            group = len(tset) > 1 and self.pivot_mode == "group"
            for e in tset:
                def T(f, e=e):
                    e.bind.set(f(e.bind.get()))
                    for b in e.points:
                        b.set(f(b.get()))
                if move is not None:
                    T(lambda p: p + move)
                if rot_deg:
                    dq = vec.quat_axis_angle(AXES[axis], math.radians(rot_deg))
                    if group:
                        T(lambda p: c + vec.quat_rotate(dq, p - c))
                    if e.set_quat:
                        e.set_quat(vec.quat_mul(dq, e.get_quat()), axis)
                if scale:
                    f = np.full(3, float(scale))
                    if group:
                        T(lambda p: c + (p - c) * f)
                    if e.scl == "xyz":
                        e.set_scale(e.get_scale() * f)
                    elif e.scl == "box":
                        e.set_box(e.box_size0(), f)
        self.mutate(fn)

    # -----------------------------------------------------------------------------------------------------------
    # Edits (every edit is an XML mutation with an undo snapshot)

    def push_undo(self, before):
        self.undo_stack.append(before)
        if len(self.undo_stack) > 200:
            self.undo_stack.pop(0)
        self.redo_stack = []

    def mutate(self, fn, select=None):
        before = self.serialize()
        res = fn()
        if self.serialize() != before:
            self.push_undo(before)
            self.after_edit(select=select)
        return res

    def selection_keys(self):
        return [{"node": e.sub if e.sub is not None else e.node, "path": path_of(e.sub if e.sub is not None else e.node),
                 "handle": e.label if e.is_handle else None} for e in self.selection]

    def find_by_key(self, k, by_path):
        n = node_at(self.root, k["path"]) if by_path else k["node"]
        for e in self.entities:
            if not e.derived and ((e.label == k["handle"]) if k["handle"] else not e.is_handle) and (
                    e.sub if e.sub is not None else e.node) is n:
                return e
        return None

    def after_edit(self, select=None, by_path=False):
        keys = self.selection_keys()
        self.build_entities()
        if select is not None:
            sel = []
            for n in select:
                sel += [e for e in self.entities if not e.is_handle and not e.derived and (e.sub if e.sub is not None else e.node) is n][:1]
        else:
            sel = [x for x in (self.find_by_key(k, by_path) for k in keys) if x]
        self.rebuilt.emit()
        self.set_selection(sel)
        self.schedule_import_plan()
        self.edited.emit()
        self.schedule_draft_save()

    def undo(self):
        if not self.undo_stack:
            return
        self.redo_stack.append(self.serialize())
        self.restore_keeping_puzzle(self.undo_stack.pop())
        self.after_edit(by_path=True)

    def redo(self):
        if not self.redo_stack:
            return
        self.undo_stack.append(self.serialize())
        self.restore_keeping_puzzle(self.redo_stack.pop())
        self.after_edit(by_path=True)

    def restore_keeping_puzzle(self, xml):
        """Restores a snapshot; the open puzzle stays open only when it is still the same one"""
        before = xmlutil.serialize(self.puzzle_shown) if self.puzzle_shown is not None else None
        count = len(kids(self.root, "Puzzle"))
        self.restore(xml)
        if before is not None and len(kids(self.root, "Puzzle")) != count:
            # A puzzle came or went: find the open one by its content, or close it
            same = [i for i, p in enumerate(kids(self.root, "Puzzle")) if xmlutil.serialize(p) == before]
            self.puzzle_index = same[0] if same else None

    def is_edited(self):
        return self.root is not None and self.serialize() != self.orig_xml

    # -----------------------------------------------------------------------------------------------------------
    # Autosave: the current level's XML is kept as a draft in the user folder after every edit, and restored when
    # the level is opened again. A level that matches the shipped one has no draft.

    def schedule_draft_save(self):
        self._draft_pending = (self.scene["id"], self.serialize())
        self.draft_timer_hook()

    def draft_timer_hook(self):
        """The window replaces this with a short timer; without one the draft is saved at once"""
        self.flush_draft()

    def flush_draft(self):
        job, self._draft_pending = getattr(self, "_draft_pending", None), None
        if not job:
            return
        sid, xml = job
        if xml == self.orig_xml_for.get(sid):
            self.drafts.delete(sid)
            if self.scene and sid == self.scene["id"]:
                self.draft_saved_at = None
        else:
            if self.drafts.save(sid, xml):
                if self.scene and sid == self.scene["id"]:
                    self.draft_saved_at = time.time()
            else:
                self.set_status("Couldn’t autosave: the user folder can’t be written. Export XML to keep your edits.", "err")
        self.edited.emit()

    def reset_level(self):
        """Back to the shipped level (undoable); the window asks first"""
        self.push_undo(self.serialize())
        self.restore(self.orig_xml)
        self.after_edit(select=[])
        self.flush_draft()
        self.set_status("Reset %s to the shipped level and deleted its autosave. Undo (Ctrl+Z) brings your edits back." % self.scene["name"], "ok")

    # -----------------------------------------------------------------------------------------------------------
    # Adding things

    def ground_objects(self, skip=None, solid_only=False):
        out = []
        for e in self.entities:
            if solid_only:
                if e.is_handle or e.derived or e is skip or not (e.tag == "LandActor" or e.kind == "platform"):
                    continue
            elif e.kind not in ("land", "platform"):
                continue
            out.append(e.obj)
        return out

    def placement_point(self):
        """Where the view's centre meets the ground (or the orbit's target)"""
        origin, d = self.camera.ray_ndc(0, 0)
        hits = raycast(self.ground_objects(), origin, d, camera=self.camera)
        return hits[0].point.copy() if hits else self.orbit.target.copy()

    def view_facing(self):
        """The heading the editor's camera looks along: new zones expect the player to walk away from the viewer"""
        d = self.camera.forward()
        return math.atan2(d[0], d[2])

    def add_garib(self, pos, type_="garib"):
        made = []

        def fn():
            sel = self.selected
            group = sel.node if sel is not None and sel.tag == "GaribGroup" else None
            if group is None:
                ids = [num(x, "group_id") for x in kids(self.root, "GaribGroup")]
                group = self.insert_top(make_el("GaribGroup", {"group_id": str(int(max([-1] + ids)) + 1), "initial_state": "1"}))
            g = make_el("Garib", {"x": r1(pos[0]), "y": r1(pos[1]), "z": r1(pos[2]), "type": type_, "dynamic_shadow": "0"})
            group.append(g)
            made.append(g)
        self.mutate(fn, select=made)
        self.set_status("Added a %s. The game counts garibs as it parses the level. The total updates by itself." % type_.replace("_", " ", 1), "ok")

    def add_element(self, make, msg=None):
        made = []
        self.mutate(lambda: made.append(self.insert_top(make())), select=made)
        if msg:
            self.set_status(msg, "ok")

    def add_model(self, oid, as_):
        info = self.catalog.get(oid) or {}
        p = self.placement_point()
        name = (info.get("name") or "object")[:7]
        imp = self.is_import(oid)

        def make():
            if as_ == "Platform":
                n = make_el("Platform", {"object_id": oid, "name": name})
                n.append(make_el("PlatSetInitialPos", {"x": r1(p[0]), "y": r1(p[1]), "z": r1(p[2])}))
                return n
            return make_el(as_, {"object_id": oid, "name": name, "x": r1(p[0]), "y": r1(p[1]), "z": r1(p[2])})
        self.add_element(make, "Added %s as %s." % (info.get("name") or oid, as_) + (
            " It comes from %s and is imported into %s when you build the ROM." % (info["bank"], self.scene["importObjectBank"]) if imp else ""))

    def add_enemy(self, type_):
        """New enemies copy a shipped script for their type, shifted so its absolute coordinates follow the enemy"""
        t = self.templates.get(type_)
        p = self.placement_point() + vec.v3(0, 4, 0)

        def make():
            if t:
                n = xmlutil.parse(t["xml"])
                d = vec.v3(p[0] - num(n, "x"), p[1] - num(n, "y"), p[2] - num(n, "z"))
                for b in attached_points(n):
                    b.set(b.get() + d)
                n.set("puzzle_tag", "0")
            else:
                n = make_el("Enemy", {"type": type_, "puzzle_tag": "0", "x": "0.0", "y": "0.0", "z": "0.0", "y_rotation": "0.0"})
                ins = make_el(REST[0], REST[1])
                ins.append(make_el(REST[2], REST[3]))
                grp = make_el("NormalInstructions")
                grp.append(ins)
                n.append(grp)
                n.append(make_el("EnemyFinalize"))
            for i, a in enumerate("xyz"):
                n.set(a, r1(p[i]))
            return n
        lvl = next((s["name"] for s in self.scenes if t and s["id"] == t["level"]), t["level"] if t else None)
        mid = self.enemy_model_id(type_)
        self.add_element(make, "Added %s" % type_ + (" with the script it has in %s, moved to the new spot." % lvl if t else ".") + (
            " Its model comes from %s and is imported at ROM build." % self.catalog[mid]["bank"] if self.is_import(mid) else ""))

    def add_preset(self, key):
        """A platform preset from desktop/presets.py at the view's ground point"""
        from . import presets
        name = presets.PRESETS[key][0]
        p, f = self.placement_point(), self.view_facing()
        sel = self.selected
        model = None
        if key in ("mover", "spinner", "conveyor", "ice") and sel is not None and sel.node.get("object_id") and not sel.is_handle:
            oid = hex_id(sel.node.get("object_id"))
            if (self.catalog.get(oid) or {}).get("custom"):
                return self.set_status(WHY["customPlatform"], "err")
            model = (oid, sel.node.get("name") or "deck")

        def ground(q):
            g = self.ground_below(vec.v3(q[0], q[1] + 200, q[2]), reach=600)
            return float(g[1]) if g is not None else q[1]
        if key == "start":
            # Glover starts at the first checkpoint in the level. A new one put before the level's own platforms
            # froze the game, so the first one moves instead.
            first_ring = next((n for n in kids(self.root, "Platform") if first(n, "PlatCheckpoint") is not None), None)
            if first_ring is not None and first(first_ring, "PlatSetInitialPos") is None:
                return self.set_status("The level's start ring follows a path, so it can't be moved here. Move its path points.", "err")
            if first_ring is not None:
                def move():
                    pos = first(first_ring, "PlatSetInitialPos")
                    if pos is not None:
                        for i, a in enumerate("xyz"):
                            pos.set(a, fmt(p[i] + (presets.RING_START if a == "y" else 0)))
                    first(first_ring, "PlatCheckpoint").set("theta", fmt(f))
                self.mutate(move, select=[first_ring])
                return self.set_status("Moved the level's start ring here, facing the way the view looks.", "ok")
        if key == "entry" and self.scene["index"] > 7:
            return self.set_status("The entry zone goes in a hub: the game's portals only work there.", "err")
        tag = self.free_tag()
        made = presets.build(key, p, f, tag=tag, model=model, ground=ground)
        if key == "ring" and self.level_info().get("extra"):
            # In an extra level the mod lets a rolling ball touch a ring with tag 77 (mod/slots.c Mod_CheckRestartGate)
            made[0].append(make_el("PlatSetTag", {"tag": "77"}))
            tag = 77
        added = []

        def fn():
            for n in made:
                added.append(self.insert_top(n))
        self.mutate(fn, select=[n for n in made if n.tag == "Platform"][:1])
        ids = {n.get("object_id") for n in made if n.get("object_id")}
        imp = [i for i in ids if self.is_import(hex_id(i))]
        self.set_status("Added %s." % name.lower() + (" Its platform tag is %d." % tag if any(
            c.tag == "PlatSetTag" for n in made for c in kids(n)) else "") + (
            " Its model is imported at ROM build." if imp else ""), "ok")

    def add_powerup(self, v, name):
        p = self.placement_point() + vec.v3(0, 10, 0)
        self.add_element(lambda: make_el("Powerup", {"type": str(v), "u16_0x02": "300", "u16_0x04": "300", "x": r1(p[0]),
                                                     "y": r1(p[1]), "z": r1(p[2])}),
                         "Added a %s powerup (Pickup::%s)." % (name.lower(), name))

    def add_tip(self):
        p = self.placement_point() + vec.v3(0, 12, 0)
        self.add_element(lambda: make_el("MrTip", {"x": r1(p[0]), "y": r1(p[1]), "z": r1(p[2]), "message_id": "1"}),
                         "Added Mr. Tip. Set message_id in the inspector.")

    def add_camera_zone(self, key):
        p, f = self.placement_point(), self.view_facing()
        made = []

        def fn():
            puzzle = self.insert_top(make_el("Puzzle"))
            c = make_el("PuzzleCond", {"cond_type": "35"})
            vol = make_el("PuzzleCondGloverWithinVolume", {"x": r1(p[0] - 150), "y": r1(p[1] - 100), "z": r1(p[2] - 150),
                                                           "l": "300.0", "w": "400.0", "h": "300.0"})
            c.append(vol)
            puzzle.append(c)
            set_camera_zone(puzzle, preset_zone(key, f, p + vec.v3(0, 100, 0)))
            made.append(vol)
        self.mutate(fn, select=made)
        label, note = CAM_PRESETS[key][0], CAM_PRESETS[key][1]
        self.set_status("Added a %s camera zone. While Glover is inside the box the camera %s; it goes back when he leaves." % (label.lower(), note), "ok")

    def duplicate(self):
        if not self.selection:
            return
        clones = []

        def fn():
            done = set()
            for ent in self.selection:
                if ent.derived or (ent.kind == "trigger" and not lone_trigger(ent)):
                    continue
                src = ent.sub if ent.tag == "GaribGroup" else (None if ent.is_handle else ent.node)
                if src is None or id(src) in done:
                    continue
                done.add(id(src))
                c = xmlutil.clone(src)
                xmlutil.insert_after(src, c)
                clones.append(c)
                # Nudge the copy (and its script/path points) 40 units along x, off the original
                for a in ("x", "origin_x", "left", "end_1_x", "end_2_x"):
                    if c.get(a) is not None:
                        c.set(a, fmt(num(c, a) + 40))
                for b in attached_points(c):
                    b.set(b.get() + vec.v3(40, 0, 0))
                if ent.kind == "trigger":
                    for v in trigger_volumes(c):
                        v.set("x", fmt(num(v, "x") + 40))
                    for f in c.iter("PuzzleActionCameraFlyTowardsPoint"):
                        f.set("x", fmt(num(f, "x") + 40))
                    clones[-1] = trigger_volumes(c)[0]
                if ent.tag == "Platform":
                    t = first(c, "PlatSetTag")   # a copy shouldn't answer to the original's puzzle tag
                    if t is not None:
                        c.remove(t)
        self.mutate(fn, select=clones)

    def remove(self):
        if not self.selection:
            return
        kept = len([e for e in self.selection if e.kind == "trigger" and not lone_trigger(e)])
        n = len(self.selection) - kept

        def fn():
            for ent in self.selection:
                if ent.derived or (ent.kind == "trigger" and not lone_trigger(ent)):
                    continue
                if ent.tag == "GaribGroup":
                    grp = ent.node
                    if ent.sub.getparent() is not None:
                        grp.remove(ent.sub)
                    if not kids(grp, "Garib") and grp.getparent() is not None:
                        grp.getparent().remove(grp)
                elif ent.is_handle and ent.sub is not None:
                    if ent.sub.getparent() is not None:
                        ent.sub.getparent().remove(ent.sub)
                elif not ent.is_handle and ent.node.getparent() is not None:
                    ent.node.getparent().remove(ent.node)
        self.mutate(fn, select=[])
        parts = []
        if n or not kept:
            parts.append("Deleted %d object%s." % (n, "" if n == 1 else "s"))
        if kept:
            parts.append(WHY["triggerEdit"])
        self.set_status(" ".join(parts))

    def set_attr(self, node, name, value):
        self.mutate(lambda: node.set(name, value))

    # The level's row in the game's level table: its name in the level select, its music and its bonus clock

    def level_info(self):
        return self.drafts.info(self.scene["id"]) if self.scene else {"name": None, "song": None, "bonus_time": None, "extra": False}

    def can_be_extra(self):
        """Extra levels go in the Space slots 35, 36, 37 (levels), 38 (boss) and 41 (bonus). 39 and 40 have fixed game code."""
        return bool(self.scene) and self.scene["index"] in EXTRA_SLOTS

    def set_level_info(self, key, value):
        info = self.level_info()
        info[key] = value
        if not self.drafts.save_info(self.scene["id"], info):
            return self.set_status("Couldn't save the level's settings in the user folder.", "err")
        self.set_status("Saved. Build ROM and Export patch write it into the game's level table.", "ok")
        self.edited.emit()

    def apply_look(self, name):
        """Writes a look preset: the level's ambient light, directional lights and fog, in one undo step"""
        amb, lights, fog = LOOKS[name]

        def fn():
            for t in ("AmbientLight", "DiffuseLight", "FogConfiguration"):
                for n in kids(self.root, t):
                    self.root.remove(n)
            new = [make_el("AmbientLight", {"r": str(amb[0]), "g": str(amb[1]), "b": str(amb[2])})]
            for r, g, b, tx, ty in lights:
                new.append(make_el("DiffuseLight", {"r": str(r), "g": str(g), "b": str(b), "theta_x": fmt(tx), "theta_y": fmt(ty)}))
            new.append(make_el("FogConfiguration", {"fog_enabled": "1", "r": str(fog[0]), "g": str(fog[1]), "b": str(fog[2]),
                                                    "fog_distance": str(fog[3]), "near_clip": str(fog[4])}))
            ch = kids(self.root)
            for n in new:
                if ch:
                    ch[0].addprevious(n)
                else:
                    self.root.append(n)
        self.mutate(fn)
        self.set_status("The level now looks “%s”." % name, "ok")

    def add_look(self, tag):
        """Adds a level-wide command (fog, a light...) at the start of the level"""
        if tag in LOOK_ONE and first(self.root, tag) is not None:
            return self.set_status("The level already has %s." % LOOK[tag].lower(), "err")
        if tag == "DiffuseLight" and len(kids(self.root, tag)) >= LIGHTS_MAX:
            return self.set_status("The game uses %d directional lights at most (AddBackgroundLight)." % LIGHTS_MAX, "err")
        attrs = {}
        if tag == "Backdrop":
            # The game stops with LEVEL UNPLAYABLE when the texture isn't in the level's banks (FindTexture, vital)
            have = self.backend.backdrops(self.scene["id"])
            if not have:
                return self.set_status("None of the shipped backdrop textures is in this level's texture banks.", "err")
            attrs["texture_id"] = have[0]
        made = []

        def fn():
            n = self.format.make(tag, attrs)
            ch = kids(self.root)
            if ch:
                ch[0].addprevious(n)
            else:
                self.root.append(n)
            made.append(n)
        self.mutate(fn)
        self.set_status("Added %s. Its fields are in the Level settings list (with nothing selected)." % LOOK[tag].lower(), "ok")

    # Commands inside elements (the inspector's Add command, ×, ↑ and ↓)

    def command_attrs(self, parent, tag):
        """Values for a new command that depend on where it goes: positions near the platform, a free tag"""
        a = {}
        start = first(parent, "PlatSetInitialPos")
        if start is None:
            start = first(parent, "PlatPathPoint")
        at = vec.v3(num(start, "x"), num(start, "y"), num(start, "z")) if start is not None else vec.v3()
        if tag == "PlatPathPoint":
            pts = kids(parent, "PlatPathPoint")
            q = vec.v3(num(pts[-1], "x") + 100, num(pts[-1], "y"), num(pts[-1], "z")) if pts else at
            a = {"x": fmt(q[0]), "y": fmt(q[1]), "z": fmt(q[2])}
        elif tag == "PlatOrbitAroundPoint":
            a = {"x": fmt(at[0] + 100), "y": fmt(at[1]), "z": fmt(at[2])}
        elif tag == "PlatSetInitialPos":
            a = {"x": fmt(at[0]), "y": fmt(at[1]), "z": fmt(at[2])}
        elif tag == "PlatSetTag":
            a = {"tag": str(self.free_tag())}
        elif tag == "PlatDestructible" and parent.get("object_id"):
            a = {"fragment_object_id": parent.get("object_id")}
        return a

    def free_tag(self):
        used = {num(t, "tag") for t in self.root.iter("PlatSetTag")} | {77, ENTRY_TAG}   # the mod reads 77 (rings) and 99
        t = 1
        while t in used:
            t += 1
        return t

    def add_command(self, parent, tag, attrs=None):
        from .commands import refusal, insert
        why = refusal(parent, tag)
        if why:
            return self.set_status("Can't add %s to <%s>: %s" % (self.format.label(tag), parent.tag, why), "err")
        a = self.command_attrs(parent, tag)
        a.update(attrs or {})
        made = []
        self.mutate(lambda: made.append(insert(parent, self.format.make(tag, a))))
        self.set_status("Added %s to <%s>." % (self.format.label(tag), parent.tag) + (
            " " + MOD_NOTE if tag in ("SetActorRotation", "SetActorScale") and parent.tag in MOD_PARENTS else ""), "ok")
        return made[0] if made else None

    def removal_refused(self, node):
        parent = node.getparent()
        if parent is self.root and node.tag == "EndLevelData":
            return "EndLevelData ends the level: the game parses until it."
        if parent is self.root and node.tag in LOOK_KEEP:
            return "Every level keeps its %s. Without one the game keeps the last screen's." % LOOK[node.tag].lower()
        if parent is self.root and node.tag == "DiffuseLight" and len(kids(self.root, "DiffuseLight")) == 1:
            return "Every level keeps one directional light."
        if node.tag == "EnemyFinalize":
            return "EnemyFinalize ends the enemy: the game reads everything after it as the next object."
        enemy = node.getparent() if node.tag == "NormalInstructions" else parent.getparent() if parent is not None and parent.tag == "NormalInstructions" else None
        if enemy is not None and enemy.tag == "Enemy" and not any(
                c is not node for g in kids(enemy, "NormalInstructions") if g is not node for c in kids(g)):
            return "An enemy needs at least one normal instruction (NMEGetNewInstruction reads the first)."
        return None

    def remove_node(self, node):
        from .commands import missing_need
        parent = node.getparent()
        if parent is None:
            return
        why = self.removal_refused(node)
        if why:
            return self.set_status(why, "err")
        gone = []
        # Backdrops are counted from 1 by decal_parent_idx, and path points from 0 by PlatPathSetStartingPoint
        nth = kids(parent, node.tag).index(node)
        orphans = {id(c) for j, c in enumerate(kids(parent)) if missing_need(parent, c.tag, j)}

        def fn():
            parent.remove(node)
            if node.tag == "Backdrop" and parent is self.root:
                # The later ones move down, and one that named this one loses its parent
                for b in kids(self.root, "Backdrop"):
                    v = inum(b, "decal_parent_idx")
                    if v == nth + 1:
                        b.set("decal_parent_idx", "0")
                    elif v > nth + 1:
                        b.set("decal_parent_idx", str(v - 1))
            if node.tag == "PlatPathPoint":
                for sp in kids(parent, "PlatPathSetStartingPoint"):
                    if inum(sp, "point_idx") > nth:
                        sp.set("point_idx", str(inum(sp, "point_idx") - 1))
            # Children left with nothing to act on go with it (not ones that had nothing before)
            changed = True
            while changed:
                changed = False
                for j, c in enumerate(kids(parent)):
                    if id(c) not in orphans and missing_need(parent, c.tag, j):
                        parent.remove(c)
                        gone.append(c.tag)
                        changed = True
                        break
            # An empty instruction group breaks the build (libgarib reads its last child)
            if parent.tag in GROUPS and not kids(parent) and parent.getparent() is not None:
                parent.getparent().remove(parent)
        self.mutate(fn, select=[] if parent is self.root else None)
        if gone:
            self.set_status("Also removed %s: %s had nothing to act on without it." % (
                ", ".join(self.format.label(t) for t in gone), "it" if len(gone) == 1 else "they"))

    def move_node(self, node, delta):
        """Moves an element one place earlier (-1) or later (+1) among its siblings. The top level keeps
        EndLevelData last, an enemy keeps EnemyFinalize last, and a platform refuses an order the game reads
        wrong."""
        from .commands import PLATFORMS, platform_rules
        parent = node.getparent()
        if parent is None:
            return
        sib = kids(parent)
        i = sib.index(node)
        j = i + delta
        if j < 0 or j >= len(sib) or (parent is self.root and sib[j].tag == "EndLevelData"):
            return
        if "EnemyFinalize" in (node.tag, sib[j].tag):
            return self.set_status("EnemyFinalize stays last: it ends the enemy.", "err")
        if parent.tag == "Puzzle":
            from .puzzles import rank
            if rank(sib[j]) != rank(node):
                return   # conditions stay among conditions, actions among actions

        def swap():
            if delta < 0:
                sib[j].addprevious(node)
            else:
                sib[j].addnext(node)
            node.tail = None
        if parent.tag in PLATFORMS:
            # Try the move on a copy
            trial = xmlutil.clone(parent)
            tk = kids(trial)
            (tk[j].addprevious if delta < 0 else tk[j].addnext)(tk[i])
            before = {k for k, _, _ in platform_rules(parent)}
            new = [m for k, m, _ in platform_rules(trial) if k not in before]
            if new:
                return self.set_status("Can't move it there: " + new[0], "err")
        self.mutate(swap)

    def tidy_node(self, node):
        from .commands import tidy
        moved = []
        self.mutate(lambda: moved.append(tidy(node)))
        self.set_status("Moved the commands the game would read in the wrong order." if moved and moved[0]
                        else "The order is already right.", "ok")

    # Enemy scripts (the script panel in the inspector)

    def add_instruction(self, enemy, kind, t):
        from . import scripts
        if len(scripts.lists(enemy)[kind]) >= scripts.LIMITS[kind]:
            return self.set_status("The game has room for %d %s instructions." % (scripts.LIMITS[kind], scripts.KIND_NAMES[kind].lower()), "err")
        self.mutate(lambda: scripts.group_for(enemy, kind).append(scripts.make_instruction(self.format, t)))

    def add_script_template(self, enemy, name):
        """Replaces the enemy's normal list with a ready-made script"""
        from . import scripts
        rows = scripts.templates(self.format, enemy)[name]

        def fn():
            for g in kids(enemy, "NormalInstructions"):
                enemy.remove(g)
            g = scripts.group_for(enemy, "NormalInstructions")
            for r in rows:
                g.append(r)
        self.mutate(fn)
        self.set_status("The enemy's normal script is now “%s”." % name, "ok")

    def set_instruction_type(self, ins, t):
        """A new body for the type; fields both bodies have keep their values"""
        from . import scripts
        old = scripts.body(ins)

        def fn():
            new = scripts.body(scripts.make_instruction(self.format, t))
            if old is not None:
                for k, v in old.attrib.items():
                    if new.get(k) is not None:
                        new.set(k, v)
                ins.remove(old)
            ins.set("instr_type", str(t))
            if t in scripts.NEEDS_TARGET and not scripts.flags_value(ins.get("flags"), self.flag_enum()) & 0x1C0:
                ins.set("flags", str(scripts.flags_value(ins.get("flags"), self.flag_enum()) | 0x40))
            ins.append(new)
        self.mutate(fn)

    def flag_enum(self):
        return self.decomp["ksyEnums"].get("enemy_instruction::instruction_flags", {})

    def move_instruction(self, ins, delta):
        """Moves an instruction within its list. Goto and Loop targets follow it."""
        from . import scripts
        grp = ins.getparent()
        enemy, kind = grp.getparent(), grp.tag
        items = scripts.lists(enemy)[kind]
        i = items.index(ins)
        j = i + delta
        if not 0 <= j < len(items):
            return

        def fn():
            (items[j].addprevious if delta < 0 else items[j].addnext)(ins)
            ins.tail = None
            if not kids(grp):
                enemy.remove(grp)
            scripts.retarget(enemy, kind, lambda k: j if k == i else i if k == j else None)
            scripts.retarget_puzzles(enemy, kind, lambda k: j if k == i else i if k == j else None)
        self.mutate(fn)

    def remove_instruction(self, ins):
        from . import scripts
        why = self.removal_refused(ins)
        if why:
            return self.set_status(why, "err")
        grp = ins.getparent()
        enemy, kind = grp.getparent(), grp.tag
        i = scripts.lists(enemy)[kind].index(ins)

        def fn():
            grp.remove(ins)
            if not kids(grp):
                enemy.remove(grp)
            scripts.retarget(enemy, kind, lambda k: k - 1 if k > i else None)
            scripts.retarget_puzzles(enemy, kind, lambda k: k - 1 if k > i else None)
        self.mutate(fn)

    def set_instruction_target(self, ins, bit):
        """Which actor Follow, Flee and Face act on: 0x40 the glove, 0x80 the ball, 0x100 the closer one"""
        from . import scripts
        v = scripts.flags_value(ins.get("flags"), self.flag_enum()) & ~0x1C0 | bit
        self.mutate(lambda: ins.set("flags", str(v)))

    # Puzzles (the puzzle card in the inspector)

    @property
    def puzzle_shown(self):
        ps = kids(self.root, "Puzzle") if self.root is not None else []
        return ps[self.puzzle_index] if self.puzzle_index is not None and self.puzzle_index < len(ps) else None

    def open_puzzle(self, puzzle):
        """Shows a puzzle in the inspector. Puzzles with no trigger volume have no place in the view."""
        self.puzzle_index = None if puzzle is None else kids(self.root, "Puzzle").index(puzzle)
        self.set_selection([])

    def add_puzzle(self):
        made = []

        def fn():
            p = self.insert_top(make_el("Puzzle"))
            p.append(make_el("PuzzleNumtimes", {"n": "1"}))
            made.append(p)
        self.mutate(fn, select=[])
        self.open_puzzle(made[0])
        self.set_status("Added a puzzle that fires once. With no conditions it fires when the level starts; add conditions and actions below.", "ok")

    def set_fires(self, puzzle, n):
        """n: None (every frame while its conditions hold) or how many times it fires"""
        from .puzzles import insert

        def fn():
            for c in kids(puzzle, "PuzzleNumtimes"):
                puzzle.remove(c)
            if n is not None:
                insert(puzzle, make_el("PuzzleNumtimes", {"n": str(int(n))}))
        self.mutate(fn)

    def set_logic(self, puzzle, how, n=0):
        """how: all (no element), any (PuzzleOr) or least (PuzzleAny op=n)"""
        from .puzzles import insert

        def fn():
            for c in kids(puzzle):
                if c.tag in ("PuzzleAnd", "PuzzleOr", "PuzzleAny"):
                    puzzle.remove(c)
            if how == "any":
                insert(puzzle, make_el("PuzzleOr"))
            elif how == "least":
                insert(puzzle, make_el("PuzzleAny", {"op": str(int(n))}))
        self.mutate(fn)

    def add_puzzle_part(self, puzzle, kind, code):
        """Adds a condition (kind PuzzleCond) or an action (PuzzleAction) with this type code"""
        from . import puzzles
        made = []
        make = puzzles.make_cond if kind == "PuzzleCond" else puzzles.make_action
        self.mutate(lambda: made.append(puzzles.insert(puzzle, make(self.format, code))))
        return made[0] if made else None

    def set_part_type(self, part, code):
        """Changes a condition's or action's type: a new body with that type's fields. The fields both bodies have
        keep their values."""
        from . import puzzles
        kind = part.tag
        attr = "cond_type" if kind == "PuzzleCond" else "action_type"
        old = puzzles.body(part)

        def fn():
            new = (puzzles.make_cond if kind == "PuzzleCond" else puzzles.make_action)(self.format, code)
            b = puzzles.body(new)
            if old is not None:
                for k, v in old.attrib.items():
                    if b.get(k) is not None:
                        b.set(k, v)
                part.remove(old)
            part.set(attr, str(code))
            part.append(b)
        self.mutate(fn)

    def remove_puzzle(self, puzzle):
        self.puzzle_index = None
        self.mutate(lambda: puzzle.getparent().remove(puzzle), select=[])

    # -----------------------------------------------------------------------------------------------------------
    # Layers

    def set_layer(self, kind, on):
        """Shows or hides a kind of entity in the view. Hidden entities can't be picked, and leave the selection."""
        (self.hidden.discard if on else self.hidden.add)(kind)
        if kind == "trigger" and self.settings is not None:
            self.settings.setValue("triggers", "on" if on else "off")
        if not on and any(e.kind == kind for e in self.selection):
            self.set_selection([e for e in self.selection if e.kind != kind])
        self.apply_visibility()
        self.selection_changed.emit()

    # -----------------------------------------------------------------------------------------------------------
    # Checks. Most are the game's own level-parser diagnostics, found in the gloveworld decomp
    # (PRINTF("WARNING!! ...") and Crash() in src/parse.c and src/nmeprocess.c), re-run on the XML.

    def tag_of(self, token):
        for name, c in self.decomp["commands"].items():
            if c.get("token") == token:
                return name
        return None

    def diag_for(self, text):
        return next((d for d in self.decomp["diagnostics"] if text in d["text"]), None)

    def run_rules(self, root):
        from .commands import LIMITS, platform_rules
        out = []
        tags_so_far = set()
        plat_index = 0

        def add(node, key, msg, diag_text=None, sub=None, level="err"):
            out.append({"top": node, "sub": sub, "key": key, "msg": msg, "level": level,
                        "diag": self.diag_for(diag_text) if diag_text else None})

        def read_tag(node, key, value, what, level):
            if value and value not in tags_so_far:
                add(node, key, "%s tag %g isn't on an earlier platform. The game looks it up while it reads the level." % (what, value),
                    "No such parent tag" if what == "PlatSetParent" else None, level=level)
        top = kids(root)
        enemies = [n for n in top if n.tag == "Enemy"]
        for node in top:
            if node.tag in ("Platform", "NullPlatform"):
                plat_index += 1
                pid = "platform %d" % plat_index
                par = first(node, "PlatSetParent")
                if par is not None:
                    read_tag(node, pid + ":parent", num(par, "tag"), "PlatSetParent", "err")
                for t in kids(node, "PlatSetTag"):
                    tags_so_far.add(num(t, "tag"))
                tele = first(node, "SetTeleport")
                if tele is not None:
                    # An unknown target makes the game skip the rest of the command and read its numbers as commands
                    read_tag(node, pid + ":teleport", num(tele, "out_framecount"), "The teleport's target", "err")
                for key, msg, diag in platform_rules(node):
                    add(node, pid + ":" + key, msg, diag, level="warn" if key == "2tag" else "err")
                if (self.catalog.get(hex_id(node.get("object_id"))) or {}).get("custom"):
                    add(node, pid + ":custom", WHY["customPlatform"])
            elif node.tag == "Buzzer":
                for f in ("platform_1_tag", "platform_2_tag"):
                    read_tag(node, "buzzer:%s:%s" % (path_of(node), f), num(node, f), "The buzzer's " + f.replace("_", " "), "warn")
            elif node.tag == "Vent":
                read_tag(node, "vent:%s:parent" % path_of(node), num(node, "parent_tag"), "The vent's parent", "warn")
            elif node.tag == "Enemy":
                for grp, mx, crash in (("NormalInstructions", 30, "NMEADDNORMAL"), ("ConditionalInstructions", 10, "NMEADDCONDITIONAL"),
                                       ("AttackInstructions", 30, "NMEADDATTACK")):
                    n = sum(len(kids(g)) for g in kids(node, grp))
                    if n > mx:
                        add(node, "enemy:%s:%s" % (path_of(node), grp), '%d %s instructions; more than %d makes the game call Crash("%s").' % (
                            n, grp.replace("Instructions", "").lower(), mx, crash), crash)
                    if any(not kids(g) for g in kids(node, grp)):
                        add(node, "enemy:%s:empty" % path_of(node), "An empty %s group: the build can't write it." % grp)
                if not any(kids(g) for g in kids(node, "NormalInstructions")):
                    add(node, "enemy:%s:none" % path_of(node), "No normal instruction. The game reads the first one when the enemy ends, and crashes.")
                from . import scripts
                for key, msg, level in scripts.rules(node, self.flag_enum()):
                    add(node, "enemy:%s:%s" % (path_of(node), key), msg, level=level)
                ch = kids(node)
                if not ch or ch[-1].tag != "EnemyFinalize" or len(kids(node, "EnemyFinalize")) != 1:
                    add(node, "enemy:%s:final" % path_of(node), "EnemyFinalize must be the enemy's last command, once.")
            for c in kids(node):
                if c.tag in LIMITS and c.tag != "PlatPathPoint" and len(kids(node, c.tag)) > LIMITS[c.tag]:
                    add(node, "limit:%s:%s" % (path_of(node), c.tag), "%d %s; the game has room for %d and writes past the end." % (
                        len(kids(node, c.tag)), c.tag, LIMITS[c.tag]))
                    break
        if not top or top[-1].tag != "EndLevelData" or len(kids(root, "EndLevelData")) != 1:
            add(root, "level:end", "EndLevelData must be the level's last element, once: the game reads until it.")
        fog = first(root, "FogConfiguration")
        if fog is None:
            add(root, "level:fog", "The level has no fog settings: the game keeps the last screen's fog and far plane.")
        elif num(fog, "fog_distance") >= 1000:
            add(root, "level:fogdist", "The fog distance is %g. It must be below 1000: the game divides by 1000 minus it." % num(fog, "fog_distance"))
        if len(enemies) > ENEMIES_MAX:
            add(root, "level:enemies", "%d enemies; the game has room for %d and writes past the end." % (len(enemies), ENEMIES_MAX))
        types = [e.get("type") for e in enemies]
        if any(t in types for t in TRIO) and types[:3] != list(TRIO):
            add(root, "level:trio", "The whale, crab and fish (joff, cancer, kirk) must be the first three enemies, in that order: the game reads them as enemies[0], [1] and [2].")
        name = root.get("name") or ""
        if name and (len(name) % 2 == 0 or not name.isascii()):
            add(root, "level:name", "The level's file name “%s” must be ASCII with an odd length (shipped levels use 7). Otherwise every command sits at an odd address and a console crashes." % name)
        for t in ("AmbientLight", "DiffuseLight", "FogConfiguration"):
            for n in kids(root, t):
                if any(num(n, c) > 255 for c in "rgb"):
                    add(n, "level:colour:%s" % path_of(n), "%s: a colour channel is over 255; the game keeps the low byte." % LOOK.get(t, t))
        if not kids(root, "DiffuseLight"):
            add(root, "level:nolight", "The level has no directional light. Every shipped level has at least one.", level="warn")
        if first(root, "AmbientLight") is None:
            add(root, "level:noambient", "The level has no ambient light: the game keeps the last screen's.", level="warn")
        for bd in kids(root, "Backdrop"):
            if num(bd, "scale_x") == 0 or num(bd, "scale_y") == 0:
                add(bd, "backdrop:scale:%s" % path_of(bd), "A backdrop scale of 0: the game divides by it.")
        entry = [p for p in kids(root, "Platform") if any(num(t, "tag") == ENTRY_TAG for t in kids(p, "PlatSetTag"))
                 and any(e.get("behavior") == "11" for e in kids(p, "SetExit"))]
        if len(entry) > 1:
            add(root, "level:entry", "%d entry zones to the extra levels (exit 11, tag %d): give the level one." % (len(entry), ENTRY_TAG), level="warn")
        g = first(root, "SetGravity")
        if g is not None and abs(num(g, "strength") - 1.2) > 1e-6:
            add(g, "level:gravity", "Gravity is %g (the game's own is 1.2): jumps and falls differ from the shipped levels." % num(g, "strength"), level="warn")
        lights = len(kids(root, "DiffuseLight"))
        if lights > LIGHTS_MAX:
            add(root, "level:lights", "%d directional lights; the game uses the first %d." % (lights, LIGHTS_MAX), level="warn")
        drops = kids(root, "Backdrop")
        for i, bd in enumerate(drops):
            if num(bd, "decal_parent_idx") > len(drops):
                add(bd, "backdrop:%d" % i, "decal_parent_idx %g names a backdrop the level doesn't have." % num(bd, "decal_parent_idx"), level="warn")
        # Trigger volumes PuzzleEvaluateCondition can never satisfy: IsPointWithinBox needs a positive size, and a
        # range test (distance < radius) needs a positive radius; the cylinder also needs min_y <= max_y.
        for p in kids(root, "Puzzle"):
            for v in [v for c in kids(p, "PuzzleCond") for v in kids(c)]:
                t = TRIGGERS.get(v.tag)
                if not t:
                    continue
                if t[0] == "box":
                    bad = ["%s ≤ 0" % a for a in ("l", "w", "h") if not num(v, a) > 0]
                elif t[0] == "sphere":
                    bad = ["range = 0"] if num(v, "range") == 0 else []   # the test squares both sides
                else:
                    bad = ([] if num(v, "range") > 0 else ["range ≤ 0"]) + (["min_y > max_y"] if num(v, "min_y") > num(v, "max_y") else [])
                if bad:
                    add(p, "trigger:%s" % path_of(v), "%s: %s. The volume is flat or empty, and this condition is practically never true." % (
                        v.tag, ", ".join(bad)), None, v)
        from . import puzzles
        for n, key, msg, level in puzzles.rules(root, self.format):
            add(n, key, msg, level=level)
        groups = {num(g, "group_id") for g in kids(root, "GaribGroup")}
        for n in root.iter("PuzzleActionSpawnGaribGroup"):
            if num(n, "group_id") not in groups:
                add(closest(n, "Puzzle") if closest(n, "Puzzle") is not None else n, "spawn:%g" % num(n, "group_id"),
                    "A puzzle spawns garib group %g, which doesn't exist." % num(n, "group_id"))
        return out

    def mod_users(self):
        """Objects whose rotation or scale only the Expansion Pak game mod applies (mod/transforms.c)"""
        out = []
        for n in self.root.iter("SetActorRotation", "SetActorScale"):
            p = n.getparent()
            if p is not None and p.tag in MOD_PARENTS and p not in out:
                out.append(p)
        return out

    def runtime_imports(self):
        """Imports are loaded from the player's ROM at run time by the game mod (mod/banks.c), unless the author
        chooses to copy them into the level's banks, which works without the Expansion Pak but puts game data in
        patches. Imported custom models are always copied: they are not in the player's ROM."""
        return any(not (self.catalog.get(i) or {}).get("custom") for i in self.current_imports()) and not self.copy_imports

    def needs_pak_heap(self):
        """Whether copied imports or added scenery need the mod's larger heap (mod/heap.c)"""
        sb = self.scenery_budget()
        if sb and sb["free"] < 0:
            return True
        if self.runtime_imports():
            return False
        b = self.import_budget()
        return bool(b) and len(b["over"]) > 0

    # Memory for scenery. Each landscape actor takes its Actor (0xE4 bytes, 240 with jalloc's rounding), its
    # collision planes (0x78 bytes a face of the model's root mesh, parse.c), and its static matrix (64): three heap
    # blocks of the game's 1,280 (include/jalloc.h). A background actor frees its planes after the parse, but the
    # parse needs the room.

    def faces(self, node):
        oid = hex_id(node.get("object_id")) if node.get("object_id") else None
        f = getattr(self.backend, "faces", None)
        return f(oid) if oid and f else 0

    def actor_cost(self, node):
        """(heap bytes, heap blocks) a top-level element takes when the level loads (scenery only)"""
        if node.tag in ("LandActor", "BackgroundActor"):
            return 240 + (120 * self.faces(node) + 15) // 16 * 16 + 64, 3
        if node.tag == "AnimatedBackgroundActor":
            return 240, 1
        return 0, 0

    def scenery_cost(self, root):
        b = k = 0
        for n in kids(root):
            x, y = self.actor_cost(n)
            b, k = b + x, k + y
        return b, k

    def scenery_budget(self):
        """How the scenery added or removed since the shipped level changes the heap: {delta, blocks, free,
        freePak}, free being what is left after the 16 KB the game needs during play. None for an unchanged level
        or one with no measured heap."""
        h = self.heap.get((self.scene or {}).get("id"))
        if not h or self.root is None or not self.orig_xml:
            return None
        if getattr(self, "_orig_cost_for", None) != self.orig_xml:
            self._orig_cost = self.scenery_cost(xmlutil.parse(self.orig_xml))
            self._orig_cost_for = self.orig_xml
        cur = self.scenery_cost(self.root)
        delta, blocks = cur[0] - self._orig_cost[0], cur[1] - self._orig_cost[1]
        if delta <= 0:
            return None
        free = h["free"] - 16 * 1024 - delta
        return {"delta": delta, "blocks": blocks, "free": free, "freePak": free + (self.mod_info or {}).get("heapExtraBytes", 0)}

    def use_mod(self):
        return bool(self.mod_users()) or self.runtime_imports() or self.needs_pak_heap()

    def issues_for(self, ent):
        out = []
        if ent.is_handle or ent.derived:
            return out
        if uses_mod(ent.node):
            out.append(("note", "Its rotation or scale uses the Expansion Pak game mod. " + MOD_NOTE))
        if ent.missing_model:
            out.append(("err", "Model %s isn't in any object bank in the game." % ent.object_id))
        elif ent.imported:
            c = self.catalog[ent.object_id]
            out.append(("imp", "Model %s comes from %s; it is imported into %s at ROM build (%.1f KB)." % (
                ent.object_id, c["bank"], self.scene["importObjectBank"], c["bytes"] / 1024)))
        if ent.kind == "enemy" and not ent.object_id:
            out.append(("err", "No model is known for this enemy type."))
        if ent.kind == "enemy" and ent.imported:
            out.append(("note", "Enemy from another world: its own model is imported, but projectiles it fires come from the game’s bullet table and stay invisible if their model isn’t loaded (FindObject non-vital)."))
        for v in self.violations:
            if v["top"] is ent.node and (v["sub"] is None or v["sub"] is ent.sub):
                out.append(("note" if v["shipped"] else v.get("level", "err"), v["msg"] + (" (the shipped level does this too)" if v["shipped"] else "")))
        return out

    def source_link(self, file, line):
        return "%s%s/%s#L%s" % (GITHUB, self.decomp["source"].get("gloveworld") or "main", file, line)

    def checks(self):
        """The Checks panel's lines"""
        out = []
        shipped = {v["key"] for v in self.orig_violations}
        self.violations = [dict(v, shipped=v["key"] in shipped) for v in self.run_rules(self.root)]
        missing = [e for e in self.entities if not e.is_handle and not e.derived and (e.missing_model or (e.kind == "enemy" and not e.object_id))]
        for e in missing:
            out.append(Check("err", "%s: %s" % (e.label, self.issues_for(e)[0][1]), lambda e=e: self.select(e, focus=True)))
        for v in sorted(self.violations, key=lambda v: v["shipped"]):
            ent = next((e for e in self.entities if e.node is v["top"] and (v["sub"] is None or e.sub is v["sub"]) and not e.is_handle and not e.derived), None)
            link = (self.source_link(v["diag"]["file"], v["diag"]["line"]), "%s:%s" % (v["diag"]["file"], v["diag"]["line"])) if v["diag"] else None
            out.append(Check("note" if v["shipped"] else v.get("level", "err"), ("%s: " % ent.label if ent else "") + v["msg"] + (" Also in the shipped level." if v["shipped"] else ""),
                             (lambda ent=ent: ent and self.select(ent, focus=True)), link))
        mods = self.mod_users()
        if mods:
            out.append(Check("note", "%d object%s a rotation or scale that only the Expansion Pak game mod applies (enemies, platforms, powerups or water). %s" % (
                len(mods), " has" if len(mods) == 1 else "s have", MOD_NOTE)))
        fresh = len([v for v in self.violations if not v["shipped"]])
        if not missing and not fresh:
            out.append(Check("ok", "No new problems. %d entities checked against the level-parser rules from the decomp; every model is in a bank." % len(
                [e for e in self.entities if not e.is_handle and not e.derived])))
        # libgarib's schema (structure), then the import checks (every value against its type and enum: the schema
        # takes any text for an enum)
        xml = self.serialize()
        try:
            bad = self.backend.validate(xml)
        except Exception:
            bad = []
        if not bad:
            res = sanitize_level_xml(xml, self.decomp, "level")
            bad = [] if res.ok else res.errors
        if bad:
            out.insert(0, Check("err", "The level doesn't match libgarib's format, so it won't build: %s" % bad[0],
                                sub="\n".join(bad[1:]) or None))
        sb = self.scenery_budget()
        if sb:
            text = "Added scenery takes about %d KB more heap than the shipped level and %d more of the game's 1,280 heap blocks. " % (
                round(sb["delta"] / 1024), sb["blocks"])
            if sb["free"] >= 0:
                out.append(Check("ok", text + "About %d KB stays free." % round(sb["free"] / 1024)))
            elif sb["freePak"] >= 256 * 1024:
                out.append(Check("note", text + "That is more than the game's own heap has, so Build ROM and Export patch add the Expansion Pak game mod: "
                                 "about %d KB stays free with it. The ROM then needs the Expansion Pak." % round(sb["freePak"] / 1024)))
            else:
                out.append(Check("err" if sb["freePak"] < 0 else "warn", text + "Even with the Expansion Pak only about %d KB stays free." % round(sb["freePak"] / 1024)))
        out += self.ground_checks()
        out += self.load_checks()
        return out

    # Draw load

    def far_plane(self):
        fog = first(self.root, "FogConfiguration")
        return num(fog, "near_clip") if fog is not None else 1000

    def load_items(self):
        """The actors as the game culls them: centre, radius and matrices (object nodes, plus a shadow for enemies)"""
        out = []
        for e in self.entities:
            if e.is_handle or e.derived or e.kind not in LOAD_KINDS:
                continue
            b = e.obj.box()
            if b.empty():
                continue
            out.append((e.obj.world_pos(), b.sphere_radius(), (e.nodes or 1) + (1 if e.kind == "enemy" else 0)))
        return out

    def load_spots(self):
        """Where the player can be: garibs, powerups, checkpoints, the start, enemies and camera zones. A level with
        few of them is also sampled at its landscape pieces."""
        spots = [e.obj.world_pos() for e in self.entities
                 if (not e.is_handle and e.kind in ("garib", "pickup", "tip", "spawn", "enemy")) or e.gate is not None or e.zone]
        if len(spots) < 30:
            spots += [e.obj.world_pos() for e in self.entities if e.kind == "land" and not e.derived]
        return spots

    def load_at(self, p, far=None, items=None):
        """The camera stands behind and over Glover; it can face any way, so each spot counts from four cameras round it"""
        far = self.far_plane() if far is None else far
        items = self.load_items() if items is None else items
        n, at = 0, None
        if items:
            cs = np.array([c for c, _, _ in items])
            rs = np.array([r for _, r, _ in items])
            ns = np.array([k for _, _, k in items])
        for k in range(4):
            a = k * math.pi / 2
            eye = p + vec.v3(math.sin(a) * LOAD_CAM_BACK, LOAD_CAM_UP, math.cos(a) * LOAD_CAM_BACK)
            m = LOAD_BASE
            if items:
                d = far + rs
                m += int(ns[np.sum((cs - eye) ** 2, 1) <= d * d].sum())
            if m > n:
                n, at = m, eye
        return n, at

    def draw_load(self):
        far, items = self.far_plane(), self.load_items()
        worst = (0, None)
        spots = []
        for p in self.load_spots():
            r = self.load_at(p, far, items)
            if r[0] > worst[0]:
                worst = r
            spots.append((p, r[0]))
        return {"far": far, "worst": worst, "spots": spots, "total": sum(k for _, _, k in items) + LOAD_BASE}

    def show_far_plane(self, at, far):
        self.far_sphere = None
        if at is not None:
            n = Node("far plane")
            for ax in "xyz":
                ln = Node()
                ln.items.append(Lines(ring_points(ax, 0, 96), (1.0, 0.7, 0.42), "loop", opacity=0.7))
                n.add(ln)
            n.scale = np.full(3, float(far))
            n.pos = np.array(at, float)
            self.far_sphere = n
            self.orbit.target = n.pos.copy()
            self.camera.pos = n.pos + vec.v3(0, far * 0.8, far * 1.2)
            self.orbit.update()
        self.view_changed.emit()

    def load_checks(self):
        out = []
        fog = first(self.root, "FogConfiguration")
        if fog is not None and num(fog, "fog_distance") >= 1000:
            out.append(Check("err", "Fog: fog_distance is %g. It is where the fog starts on the game's 0 to 1000 scale (gSPFogPosition) and must stay under 1000." % num(fog, "fog_distance"),
                             lambda: self.select(None)))
        dl = self.draw_load()
        n, at = dl["worst"]
        if at is None:
            return out
        level = "err" if n >= LOAD_STOP else "warn" if n >= LOAD_SLOW else "ok"
        what = ("That's at or near the limit: past %d the game stops, and before that it crawls." % MATRICES if level == "err"
                else "The game slows down here. Fewer pieces in reach or a shorter far plane fix it." if level == "warn" else "Fine.")
        out.append(Check(level, "Draw load: about %d of %d object matrices at the busiest spot (far plane %g). %s" % (n, MATRICES, dl["far"], what),
                         lambda: self.show_far_plane(at, dl["far"]),
                         sub="The game draws everything within %g of the camera, behind it too. %d in the whole level. Click to show the spot." % (dl["far"], dl["total"]),
                         title="Click to see the spot and the far plane"))
        return out

    def ground_below(self, p, reach=400, skip=None, objs=None):
        """Solid ground under a point: the first LandActor or platform face below it, within reach. Background
        actors have no collision. Returns the point hit, or None."""
        objs = self.ground_objects(skip, solid_only=True) if objs is None else objs
        hits = raycast(objs, np.asarray(p, float) + vec.v3(0, 30, 0), vec.v3(0, -1, 0), 0, reach + 30, meshes_only=True)
        return hits[0].point if hits else None

    def ground_checks(self):
        """Glover starts at the first checkpoint (or the GloverSpawnPoint) and restarts at the last one he touched:
        one with no solid ground under it drops him into a pit every time"""
        out = []
        if self.scene and self.scene.get("group") == "System":   # the cutscenes and the flythrus aren't played
            return out
        for e in self.entities:
            what = "checkpoint" if e.gate is not None else "Glover’s start" if e.tag == "GloverSpawnPoint" else None
            if not what or e.is_handle or e.derived:
                continue
            if self.ground_below(e.obj.world_pos(), 400, e) is None:
                out.append(Check("warn", "%s: no solid ground within 400 under this %s. Glover starts or restarts here and falls." % (e.label, what),
                                 lambda e=e: self.select(e, focus=True)))
        return out

    # -----------------------------------------------------------------------------------------------------------
    # Imports and the memory budget. Imports are derived from the XML and never stored separately. Undo, redo and
    # XML export always match what the ROM build adds.

    def current_imports(self):
        ids = []
        for e in self.entities:
            if not e.is_handle and not e.derived and e.object_id and self.is_import(e.object_id) and e.object_id not in ids:
                ids.append(e.object_id)
        return ids

    def schedule_import_plan(self):
        """The window runs the plan in the background (plan_hook); without one it runs now"""
        ids = self.current_imports()
        if not ids:
            self.plan = None
            self.imports_changed.emit()
            return
        self.plan_hook(ids)

    def plan_hook(self, ids):
        try:
            self.plan = self.backend.plan(self.scene["id"], ids)
        except Exception:
            self.plan = None
        self.imports_changed.emit()

    def import_budget(self):
        """Every level whose LoadLevel loads the enlarged object or texture bank pays for the imports, not just this
        one. over: over the game's own heap; overWithPak: over even with the mod's larger heap (mod/heap.c)."""
        p = self.plan
        if not p or not self.current_imports():
            return None
        extra = (self.mod_info or {}).get("heapExtraBytes", 0)
        rows = []
        for s in self.scenes:
            h = self.heap.get(s["id"])
            if not h:
                continue
            cost = (p["objectBytes"] if p["targetBank"] in s.get("gameObjectBanks", []) else 0) + (
                p.get("texBytes", 0) if p["targetTexBank"] in s.get("gameTextureBanks", []) else 0)
            room = max(0, h["free"] - 16 * 1024)
            if cost > 0:
                rows.append({"s": s, "cost": cost, "room": room, "ratio": cost / max(1, room)})
        if not rows:
            return None
        rows.sort(key=lambda r: -r["ratio"])
        least = min(({"s": r["s"], "left": r["room"] + extra - r["cost"]} for r in rows), key=lambda r: r["left"])
        return {"rows": rows, "extra": extra, "leastWithPak": least, "over": [r for r in rows if r["cost"] > r["room"]],
                "overWithPak": [r for r in rows if r["cost"] > r["room"] + extra]}

    # -----------------------------------------------------------------------------------------------------------
    # Path tool (P): click the ground to lay a path, then place garibs or copies of the selection along it. The path
    # is only a guide in the editor; the level keeps what is placed. Each placed thing is dropped onto the ground.

    def set_path_mode(self, on):
        self.path["on"] = on
        if on and self.box_mode:
            self.set_box_mode(False)
        self.draw_path()
        self.mode_changed.emit()

    def path_length(self):
        p = self.path["pts"]
        return sum(float(np.linalg.norm(p[i] - p[i - 1])) for i in range(1, len(p)))

    def draw_path(self):
        n = Node("path")
        n.order = 10
        pts = self.path["pts"]
        n.visible = self.path["on"] and bool(pts)
        if pts:
            lift = [p + vec.v3(0, 3, 0) for p in pts]
            ln = Node()
            ln.order = 10
            ln.items.append(Lines(lift, (1.0, 0.82, 0.25), depth_test=False))
            n.add(ln)
            ball = sphere_geometry(5, 12, 8)
            for p in lift:
                m = mesh_node(ball, Material(color=(1.0, 0.82, 0.25), lit=False, depth_test=False))
                m.order = 10
                m.pos = p
                n.add(m)
        self.path_node = n
        self.view_changed.emit()

    def path_info(self):
        pts = self.path["pts"]
        return "%d point%s · %d long" % (len(pts), "" if len(pts) == 1 else "s", round(self.path_length())) if pts else "no points yet"

    def path_click(self, x, y):
        origin, d = self.camera.ray(x, y)
        hits = raycast(self.ground_objects(), origin, d, meshes_only=True)
        if not hits:
            return self.set_status("The path goes on the ground: click a landscape piece or a platform.", "err")
        self.path["pts"].append(hits[0].point.copy())
        self.draw_path()

    def path_samples(self, step):
        """Points every step along the path, with the way on at each (a unit vector in x/z)"""
        pts = self.path["pts"]
        out = []
        if len(pts) < 2:
            return [(pts[0].copy(), vec.v3(0, 0, 1))] if pts else out
        nxt, run = 0.0, 0.0
        for i in range(1, len(pts)):
            a, b = pts[i - 1], pts[i]
            ln = float(np.linalg.norm(b - a))
            d = vec.v3(b[0] - a[0], 0, b[2] - a[2])
            d = d / (np.linalg.norm(d) or 1)
            while nxt <= run + ln + 1e-6:
                out.append((a + (b - a) * ((nxt - run) / ln if ln else 0), d))
                nxt += step
            run += ln
        return out

    def place_along_path(self, what, garib_type, step, side, off, up, turn):
        if not self.path["pts"]:
            return self.set_status("Click the ground to lay the path first.", "err")
        step = max(10, step or 60)
        spots = []
        for p, d in self.path_samples(step):
            right = vec.v3(-d[2], 0, d[0])   # the right of the way on
            for s in ([1, -1] if side == 2 else [side]):
                spots.append(p + right * s * off)
        if len(spots) > PATH_MAX:
            return self.set_status("That makes %d objects. Place at most %d at a time: space them further apart." % (len(spots), PATH_MAX), "err")
        ground = self.ground_objects()

        def drop(q):
            g = self.ground_below(q + vec.v3(0, 120, 0), 600, None, ground)
            return g if g is not None else q
        if what == "garib":
            added = []

            def fn():
                ids = [num(x, "group_id") for x in kids(self.root, "GaribGroup")]
                group = self.insert_top(make_el("GaribGroup", {"group_id": str(int(max([-1] + ids)) + 1), "initial_state": "1"}))
                for q in spots:
                    g = drop(q)
                    e = make_el("Garib", {"x": r1(g[0]), "y": r1(g[1] + up), "z": r1(g[2]), "type": garib_type, "dynamic_shadow": "0"})
                    group.append(e)
                    added.append(e)
            self.mutate(fn, select=added)
            return self.set_status("Placed %d %s%s along the path in a new garib group." % (
                len(added), garib_type.replace("_", " ", 1), "" if len(added) == 1 else "s"), "ok")
        src = []
        for e in self.selection:
            if not e.derived and not e.is_handle and e.tag in PATH_COPY and e.node not in src:
                src.append(e.node)
        if not src:
            return self.set_status("Select what to copy first: scenery, a kit model, an enemy, a powerup or Mr. Tip.", "err")
        ents = [next(e for e in self.entities if e.node is n and not e.is_handle and not e.derived) for n in src]
        centre = sum(e.bind.get() for e in ents) / len(ents)
        under = self.ground_below(centre + vec.v3(0, 120, 0), 600, None, ground)
        anchor = under if under is not None else vec.v3(centre[0], min(e.bind.get()[1] for e in ents), centre[2])
        do_turn = turn and len(src) == 1 and ents[0].rot == "xyz"
        added = []

        def fn():
            for q in spots:
                g = drop(q)
                d = vec.v3(g[0] - anchor[0], g[1] + up - anchor[1], g[2] - anchor[2])
                for n in src:
                    c = xmlutil.clone(n)
                    if c.tag == "Platform":
                        t = first(c, "PlatSetTag")
                        if t is not None:
                            c.remove(t)
                    shift_node(c, d)
                    if do_turn:
                        r = ensure_cmd(c, "SetActorRotation", {})
                        r.set("x", r.get("x") or "0.0")
                        r.set("z", r.get("z") or "0.0")
                        r.set("y", fmt(random.random() * math.pi * 2))
                    added.append(self.insert_top(c))
        self.mutate(fn, select=added)
        self.set_status("Placed %d cop%s of the selection along the path (%d objects)." % (len(spots), "y" if len(spots) == 1 else "ies", len(added)) + (
            " Random turn needs one selected model that can turn." if turn and not do_turn else ""), "ok")

    def path_zones(self, key, start=0.0, end=1.0, margin=140.0, below=300.0, above=400.0, split=600.0):
        """Camera zones over a stretch of the path (start and end are fractions of its length): boxes around the
        path, each split off at most split long. The box is level with the axes, so a
        diagonal stretch gets a bigger box: split it."""
        samples = self.path_samples(20)
        if len(samples) < 2:
            return self.set_status("Lay a path of at least two points first.", "err")
        total = 20.0 * (len(samples) - 1)
        part = [q for i, q in enumerate(samples) if start * total - 1e-6 <= i * 20.0 <= end * total + 1e-6]
        if len(part) < 2:
            return self.set_status("That stretch of the path is too short.", "err")
        n = max(1, int(math.ceil(20.0 * (len(part) - 1) / split))) if split and split > 0 else 1
        size = int(math.ceil(len(part) / n))
        chunks = [part[i:i + size + 1] for i in range(0, len(part) - 1, size)]
        made = []

        def fn():
            for ch in chunks:
                xs, ys, zs = [q[0][0] for q in ch], [q[0][1] for q in ch], [q[0][2] for q in ch]
                x0, x1 = min(xs) - margin, max(xs) + margin
                y0, y1 = min(ys) - below, max(ys) + above
                z0, z1 = min(zs) - margin, max(zs) + margin
                mid = ch[len(ch) // 2]
                facing = math.atan2(mid[1][0], mid[1][2])
                centre = ch[-1][0] if key == "fixed" else mid[0] + vec.v3(0, 100, 0)
                puzzle = self.insert_top(make_el("Puzzle"))
                c = make_el("PuzzleCond", {"cond_type": "35"})
                vol = make_el("PuzzleCondGloverWithinVolume", {"x": r1(x0), "y": r1(y0), "z": r1(z0),
                                                               "l": r1(x1 - x0), "w": r1(y1 - y0), "h": r1(z1 - z0)})
                c.append(vol)
                puzzle.append(c)
                set_camera_zone(puzzle, preset_zone(key, facing, centre))
                made.append(vol)
        self.mutate(fn, select=made)   # filled by fn, read after it
        label, note = CAM_PRESETS[key][0], CAM_PRESETS[key][1]
        self.set_status("Added %d %s camera zone%s over %d of the path. While Glover is inside a box the camera %s; it goes back when he leaves." % (
            len(made), label.lower(), "" if len(made) == 1 else "s", round(20.0 * (len(part) - 1)), note), "ok")

    def scatter_along_path(self, spacing=(80.0, 170.0), sides=(1, -1), dist=(180.0, 420.0), jitter=30.0, dy=(-25.0, 15.0),
                           scale=(0.8, 1.2), turn="free", tilt=0.0, solid=False, seed=0, weights=None):
        """Scatters copies of the selected scenery beside the path at random spacing, distance, height, scale and
        turn. weights: one per selected model. The same seed gives the same
        result."""
        if len(self.path["pts"]) < 2:
            return self.set_status("Lay a path of at least two points first.", "err")
        src = []
        for e in self.selection:
            if not e.derived and not e.is_handle and e.tag in ("LandActor", "BackgroundActor") and e.node not in src:
                src.append(e.node)
        if not src:
            return self.set_status("Select the scenery to scatter first: landscape pieces or kit models.", "err")
        if not 5 <= spacing[0] <= spacing[1]:
            return self.set_status("The spacing must be at least 5, with the first number no bigger than the second.", "err")
        weights = list(weights or [1.0] * len(src))
        rnd = random.Random(seed)
        samples = self.path_samples(5)
        spots = []
        for side in sides:
            at = rnd.uniform(*spacing) / 2
            while at / 5 < len(samples):
                p, d = samples[min(len(samples) - 1, int(at / 5))]
                right = vec.v3(-d[2], 0, d[0])
                q = p + right * side * rnd.uniform(*dist) + d * rnd.uniform(-jitter, jitter)
                pick = rnd.choices(range(len(src)), weights)[0]
                if turn == "path":
                    yaw = math.atan2(d[0], d[2])
                elif turn == "player":
                    yaw = math.atan2(-right[0] * side, -right[2] * side)
                else:
                    yaw = rnd.uniform(0, 2 * math.pi)
                spots.append((q, pick, yaw, rnd.uniform(*scale), rnd.uniform(*dy), rnd.uniform(-tilt, tilt), rnd.uniform(-tilt, tilt)))
                at += rnd.uniform(*spacing)
                if len(spots) > PATH_MAX:
                    break
        if len(spots) > PATH_MAX:
            return self.set_status("That makes %d objects. Place at most %d at a time: space them further apart." % (len(spots), PATH_MAX), "err")
        ground = self.ground_objects()
        added = []

        def fn():
            for q, pick, yaw, sc, up, tx, tz in spots:
                g = self.ground_below(q + vec.v3(0, 120, 0), 600, None, ground)
                g = g if g is not None else q
                n = xmlutil.clone(src[pick])
                n.tag = "LandActor" if solid else "BackgroundActor"
                for i, a in enumerate("xyz"):
                    n.set(a, r1(g[i] + (up if a == "y" else 0)))
                for c in kids(n, "SetActorRotation") + kids(n, "SetActorScale"):
                    n.remove(c)
                r = ensure_cmd(n, "SetActorRotation", {})
                r.set("x", fmt(round(tx, 3)))
                r.set("y", fmt(round(yaw % (2 * math.pi), 3)))
                r.set("z", fmt(round(tz, 3)))
                if abs(sc - 1) > 0.005:
                    s_ = ensure_cmd(n, "SetActorScale", {})
                    for a in "xyz":
                        s_.set(a, fmt(round(sc, 2)))
                added.append(self.insert_top(n))
        self.mutate(fn, select=added)
        self.set_status("Scattered %d copies of %d model%s beside the path (seed %d)." % (
            len(added), len(src), "" if len(src) == 1 else "s", seed) + self.copy_cost(added), "ok")

    def copy_cost(self, nodes):
        """' It adds about N triangles and N KB of heap.' for new scenery"""
        b = sum(self.actor_cost(n)[0] for n in nodes)
        f = sum(self.faces(n) for n in nodes)
        return " About %d triangles and %d KB of heap." % (f, round(b / 1024)) if nodes else ""

    # -----------------------------------------------------------------------------------------------------------
    # Imported models: a .glb from another game or a 3D program, which the backend converts into Glover models
    # (tools/custom_model.py) and keeps in the user folder. A large model comes as several pieces, each with its
    # offset.

    def custom_pieces(self, set_):
        return [(i, c) for i, c in self.catalog.items() if c.get("custom") == set_]

    def place_custom(self, set_, as_="LandActor", p=None):
        """Adds every piece of an imported model at the placement point, each at its offset, in one undo step"""
        pieces = self.custom_pieces(set_)
        if not pieces:
            return
        p = self.placement_point() if p is None else p
        nodes = []

        def fn():
            for i, c in pieces:
                o = c.get("offset") or [0, 0, 0]
                nodes.append(self.insert_top(make_el(as_, {"object_id": i, "name": c["piece"][:7], "x": r1(p[0] + o[0]),
                                                          "y": r1(p[1] + o[1]), "z": r1(p[2] + o[2])})))
        self.mutate(fn, select=nodes)
        kb = sum(c["bytes"] for _, c in pieces) / 1024
        self.set_status("Added %s as %d %s piece%s (%.0f KB). Build ROM copies %s into %s." % (
            pieces[0][1]["bank"].replace("Custom: ", ""), len(pieces), as_, "" if len(pieces) == 1 else "s", kb,
            "it" if len(pieces) == 1 else "them", self.scene["importObjectBank"]), "ok")

    # -----------------------------------------------------------------------------------------------------------
    # Camera zone panel edits

    def write_zone(self, puzzle, fn):
        def go():
            cur = camera_zone(puzzle)
            fn(cur)
            set_camera_zone(puzzle, cur)
        self.mutate(go)

    # -----------------------------------------------------------------------------------------------------------
    # Loading a level

    def load_scene(self, sid):
        """Loads a level (call from the UI thread; the backend's first calls can take a while)"""
        self.flush_draft()   # finish saving the level we're leaving
        self.scene = next(s for s in self.scenes if s["id"] == sid)
        self.puzzle_index = None
        self.show_far_plane(None, 0)
        self.path["pts"] = []
        self.draw_path()
        xml = self.backend.level_xml(sid)
        m = self.backend.models(sid)
        self.models, self.loaded = m["models"], set(m["loaded"])
        self.undo_stack, self.redo_stack, self.selection = [], [], []
        self.orig_lev_size = self.backend.orig_lev_size(sid)
        self.restore(xml)
        self.orig_xml = self.serialize()
        self.orig_xml_for[sid] = self.orig_xml
        self.orig_violations = self.run_rules(self.root)
        self.draft_saved_at = None
        restored = ""
        draft = self.drafts.load(sid)
        if draft and draft["xml"] != self.orig_xml:
            res = sanitize_level_xml(draft["xml"], self.decomp, "saved draft")
            if res.ok:
                self.undo_stack = [self.orig_xml]
                self.root = xmlutil.normalize(res.root)
                self.draft_saved_at = draft["savedAt"]
                restored = "Restored your edits from %s (autosave). Undo or Reset level to go back to the shipped level." % time.strftime(
                    "%Y-%m-%d %H:%M", time.localtime(draft["savedAt"]))
            else:
                restored = "An autosave for this level failed validation and was not loaded (%s)." % res.errors[0]
        self.model_cache.clear()
        self.build_entities()
        self.frame_level()
        if self.settings is not None:
            self.settings.setValue("scene", sid)
        self.rebuilt.emit()
        self.set_selection([])
        self.schedule_import_plan()
        self.scene_loaded.emit()
        self.edited.emit()
        if restored:
            self.set_status(restored, "ok" if restored.startswith("Restored") else "err")
        else:
            self.set_status("Loaded %s from the noclip data set: %d entities, %d models from %s." % (
                self.scene["landscape"], len([e for e in self.entities if not e.is_handle and not e.derived]), len(self.models),
                ", ".join(self.scene["objectBanks"])))

    # -----------------------------------------------------------------------------------------------------------
    # Import XML: the file is read and validated here (desktop/sanitize.py). Only after it passes does it replace the
    # level, as an undoable edit that is then autosaved.

    def confirm_import(self, res):
        self.push_undo(self.serialize())
        self.root = xmlutil.normalize(xmlutil.parse(xmlutil.serialize(res.root)))
        self.after_edit(select=[])
        imp = len(self.current_imports())
        self.set_status("Imported %s into %s (%d commands). Undo (Ctrl+Z) reverts it." % (res.info["name"] or "XML", self.scene["name"], res.info["elements"]) + (
            " It uses %d model(s) from other levels, imported at ROM build." % imp if imp else ""), "ok")

    # -----------------------------------------------------------------------------------------------------------
    # Build / export

    def build(self, fmt_, quickboot=False):
        """Builds the level: returns (bytes, file name, message). Raises ValueError with the backend's error."""
        return self.build_job(fmt_, quickboot)()

    def build_job(self, fmt_, quickboot=False):
        """The build as a function to call later, on a worker thread: everything it needs from the editor is read
        now, so a level switch or an edit while it runs changes nothing"""
        xml = self.serialize().encode("utf-8")
        imports = self.current_imports()
        notes = []
        if fmt_ not in ("rom", "bps") and imports:
            notes.append("Note: %d imported model%s only exist in a ROM built here; this %s alone references models the game won’t find." % (
                len(imports), "" if len(imports) == 1 else "s", ".lev" if fmt_ == "lev" else "XML"))
        if fmt_ == "lev" and self.mod_users():
            notes.append("Note: rotations and scales on enemies, platforms, powerups or water only work in a ROM built here, which adds the transforms game mod. In an unmodded game this .lev turns or scales the previous landscape piece instead.")
        if fmt_ == "xml":
            data, name = xmlutil.pretty(self.root).encode("utf-8"), self.scene["landscape"] + ".xml"
            return lambda: (data, name, " ".join(notes))
        info = self.level_info()
        if info.get("extra") and self.can_be_extra() and fmt_ in ("rom", "bps"):
            # An extra level is built behind the game mod, with the retail level left in its slot
            return self.build_all_job([self.scene["id"]], fmt_, self.scene["id"] if quickboot else None)
        info = {k: v for k, v in info.items() if k != "extra"}
        info = info if any(v is not None for v in info.values()) else None
        sid, scene_name, orig_size = self.scene["id"], self.scene["name"], self.orig_lev_size or 0
        use_mod, copy = self.use_mod(), self.copy_imports
        backend = self.backend

        def run():
            data, h = backend.build(sid, xml, fmt_, quickboot, imports, use_mod, copy, info)
            size = int(h["X-Lev-Size"])
            imp = h.get("X-Imports")
            mod = " Includes the Expansion Pak game mod and needs the Expansion Pak." if h.get("X-Needs-Mod") == "1" else ""
            name = h["filename"]
            if fmt_ == "bps":
                ps = int(h["X-Patch-Size"])
                msg = ("Exported %s (%.1f KB BPS patch; %s). Anyone with the original Glover (USA) ROM can apply it with patcher.html, in their browser." % (
                    name, ps / 1024, "boots straight into the level" if quickboot else "starts the game normally") +
                    (" The game mod loads its %d imported model(s) from the player’s ROM; the patch contains no game data." % imp["objects"] if imp and imp.get("runtime")
                     else " It includes the rebuilt %s and %s banks for %d imported model(s), game data included." % (imp["bank"], imp["texBank"], imp["objects"]) if imp and imp.get("objects")
                     else " It contains only what your edits add.") + mod)
                return data, name, msg
            delta = size - orig_size
            msg = ("Built %s: level is %d bytes (%s%d vs. original)" % (name, size, "+" if delta >= 0 else "", delta) +
                   ("; %d imported model(s) load from %d bank(s) of the ROM at run time" % (imp["objects"], imp["runtime"]) if imp and imp.get("runtime")
                    else "; imported %d model(s) into %s and %d texture(s) into %s" % (imp["objects"], imp["bank"], imp["textures"], imp["texBank"]) if imp and imp.get("objects")
                    else "") +
                   ("; the ROM boots straight into %s." % scene_name if fmt_ == "rom" and quickboot else ".") + (mod if fmt_ == "rom" else ""))
            if notes:
                msg = " ".join(notes) + " " + msg
            return data, name, msg
        return run

    # Every edited level in one ROM or patch

    def project_levels(self):
        """The levels with edits: a draft, a title, music or clock, or the open level's unsaved edits. [(sid, name,
        xml or None)] with xml None for a level whose file is the shipped one."""
        self.flush_draft()
        sids = set(self.drafts.list()) | set(self.drafts.all_info())
        if self.scene and self.is_edited():
            sids.add(self.scene["id"])
        out = []
        for s_ in self.scenes:
            if s_["id"] not in sids:
                continue
            if self.scene and s_["id"] == self.scene["id"]:
                xml = self.serialize() if self.is_edited() else None
            else:
                d = self.drafts.load(s_["id"])
                xml = None
                if d:
                    res = sanitize_level_xml(d["xml"], self.decomp, "saved draft")
                    xml = d["xml"] if res.ok else None
            out.append((s_["id"], s_["name"], xml))
        return out

    def level_entry(self, sid, xml):
        """What build_all needs for one level: its XML, the models it imports, its table row; and whether it needs
        the game mod (rotations, imports from the ROM, an extra level, or more scenery than the game's heap holds)"""
        root = xmlutil.parse(xml) if xml is not None else xmlutil.parse(self.backend.level_xml(sid))
        is_open = bool(self.scene) and sid == self.scene["id"]
        loaded = self.loaded if is_open else self.backend.loaded_ids(sid)
        ids = sorted({hex_id(n.get("object_id")) for n in root.iter() if isinstance(n.tag, str) and n.get("object_id")})
        imports = [i for i in ids if i not in loaded and i in self.catalog]
        info = dict(self.drafts.info(sid))
        extra = info.pop("extra", False) and next((s_["index"] for s_ in self.scenes if s_["id"] == sid), -1) in EXTRA_SLOTS
        rot = any(n.getparent() is not None and n.getparent().tag in MOD_PARENTS for n in root.iter("SetActorRotation", "SetActorScale"))
        runtime = any(not (self.catalog.get(i) or {}).get("custom") for i in imports) and not self.copy_imports
        heap = is_open and self.use_mod()
        h = self.heap.get(sid)
        if not heap and h and xml is not None:
            # Added scenery (scenery_budget) and copied imports against the level's measured free heap
            shipped = xmlutil.parse(self.backend.level_xml(sid))
            delta = self.scenery_cost(root)[0] - self.scenery_cost(shipped)[0]
            copied = 0 if runtime else sum((self.catalog.get(i) or {}).get("bytes", 0) for i in imports)
            heap = h["free"] - 16 * 1024 - max(0, delta) - copied < 0
        return ({"sid": sid, "xml": xmlutil.serialize(root).encode("utf-8") if xml is None else xml.encode("utf-8"), "imports": imports,
                 "info": info if any(v is not None for v in info.values()) else None, "extra": extra}, rot or runtime or extra or heap)

    def build_all(self, sids, fmt_, quickboot=None):
        """(bytes, file name, message) for the levels sids in one ROM or BPS patch"""
        return self.build_all_job(sids, fmt_, quickboot)()

    def build_all_job(self, sids, fmt_, quickboot=None):
        """build_all as a function to call later on a worker thread; the editor's state is read now"""
        entries, mod = [], False
        for sid, name, xml in self.project_levels():
            if sid in sids:
                e, m = self.level_entry(sid, xml)
                entries.append(e)
                mod = mod or m
        if not entries:
            raise ValueError("No levels to build.")
        names = [next(s_["name"] for s_ in self.scenes if s_["id"] == e["sid"]) for e in entries]
        copy, backend = self.copy_imports, self.backend

        def run():
            data, h = backend.build_all(entries, fmt_, quickboot, bool(mod), copy)
            msg = "%s %s with %d level%s: %s." % ("Built" if fmt_ == "rom" else "Exported", h["filename"], len(entries),
                                                  "" if len(entries) == 1 else "s", ", ".join(names))
            if h.get("extra"):
                msg += " %d extra level%s, entered through a hub zone with platform tag %d (exit 11)." % (
                    len(h["extra"]), "" if len(h["extra"]) == 1 else "s", ENTRY_TAG)
            if h.get("X-Needs-Mod") == "1":
                msg += " Includes the Expansion Pak game mod and needs the Expansion Pak."
            return data, h["filename"], msg
        return run

    def set_status(self, msg, cls=""):
        self.status.emit(msg, cls)


def shift_node(n, d):
    """Moves a copied object, with its script and path points, by d"""
    if n.get("x") is not None:
        for i, a in enumerate("xyz"):
            n.set(a, fmt(num(n, a) + d[i]))
    for b in attached_points(n):
        b.set(b.get() + d)
