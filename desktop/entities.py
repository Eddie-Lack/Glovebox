"""The level's things as the editor shows and edits them, built from the level XML.

Placement follows noclip's Glover/scenes.ts and the maths follows the gloveworld decomp:
  rotation: GetQuaternionFromXYZRot (actor.c) builds q = qY·qZ·qX (rotOrder = {1,2,0}); QuaternionToMatrix (maths.c)
            stores it transposed for N64 row vectors, so R = Ry·Rz·Rx: Euler order 'YZX'.
  scale:    landscape matrices are T·R·S with guScaleF(scale.x, scale.y, scale.z) (object.c), no axis swap.
noclip uses 'ZYX' and swaps X/Z; shipped levels only use Y rotation and uniform scale. The two agree on shipped data
and differ only for edits like these.
"""
import math
import numpy as np
from . import vec
from .scene import Lines, Material, Mesh, Node, box_edges, box_geometry, cylinder_geometry, octahedron_geometry, \
    ring_points, sphere_geometry
from .commands import ensure_cmd
from .xmlutil import first, fmt, hex_id, kids, num

KINDS = {
    "land": ("Landscape", "#8fa3b8"), "platform": ("Platforms", "#6fb7ff"), "garib": ("Garibs", "#f2c14e"),
    "enemy": ("Enemies", "#ff6b6b"), "pickup": ("Powerups", "#c58bff"), "tip": ("Mr. Tip", "#6bd88a"),
    "spawn": ("Spawns", "#ffffff"), "water": ("Water", "#3fa8e0"), "wind": ("Wind/Vents", "#9be7e0"),
    "hazard": ("Buzzers", "#ffd23f"), "sound": ("Sounds", "#b0b0b0"), "trigger": ("Puzzle triggers", "#ff8ad8"),
}

# Why some objects can't rotate or scale: each reason is a fact about the level format or the game code.
WHY = {
    "noOrientation": "No orientation in the level format. In a group rotation it moves around the pivot.",
    "sprite": "Garibs, Mr. Tip and the start markers are flat sprites or points in the game. They have no rotation or size to set. In a group rotation they move around the pivot.",
    "noScale": "No size in the level format.",
    # Rotation and scale on these objects need the Expansion Pak game mod (mod/transforms.c), which Build ROM adds
    "modRot": {
        "enemy": "Turning about Y sets y_rotation, the facing its script starts from. Tilt about X or Z is a SetActorRotation the game mod applies when drawing.",
        "platform": "SetActorRotation, applied by the game mod before the platform’s collision is built. For a spinning platform it adds to the spin angles.",
        "pickup": "SetActorRotation, applied by the game mod under the powerup’s own spin about Y.",
        "water": "SetActorRotation, applied by the game mod to the water’s model. Its swimming volume (left/top/front…) stays an axis-aligned box.",
    },
    "modScale": {
        "enemy": "SetActorScale, a factor on the type’s size. The game mod also scales the collision sphere by the mean of the three axes.",
        "pickup": "SetActorScale, a factor on the powerup’s size (0.4 in the game’s code).",
        "water": "SetActorScale on the water’s model. Its swimming volume keeps its own size.",
    },
    "customPlatform": "A platform must use a model from the game. A converted model’s first rotation key is all zeros, and a console stops when it draws one as a platform (an emulator doesn’t). Place it as Scenery or Background.",
    "triggerRot": "Puzzle trigger volumes are axis-aligned: the game tests boxes with IsPointWithinBox, spheres and cylinders by distance.",
    "triggerEdit": "This trigger volume is one of several conditions of its puzzle. It can’t be duplicated or deleted on its own.",
}

EULER_ORDER = "YZX"
AXES = {"x": vec.v3(1, 0, 0), "y": vec.v3(0, 1, 0), "z": vec.v3(0, 0, 1)}
quat_from_xyz = vec.quat_from_euler_yzx
euler_of = vec.euler_yzx_from_quat

# SetActorRotation / SetActorScale inside an object. The game applies them to landscape and background actors; for
# enemies, platforms, powerups and water the Expansion Pak game mod does (mod/transforms.c). No-op values add nothing.
MOD_PARENTS = {"Enemy", "Platform", "NullPlatform", "Powerup", "Water"}


def child_rot(node):
    r = first(node, "SetActorRotation")
    return vec.v3(num(r, "x"), num(r, "y"), num(r, "z"))


def child_scale(node):
    s = first(node, "SetActorScale")
    return vec.v3(num(s, "x", 1), num(s, "y", 1), num(s, "z", 1))


def set_child_xyz(node, tag, v, identity):
    c = first(node, tag)
    if c is None and all(abs(v[i] - identity) < 1e-6 for i in range(3)):
        return
    e = c if c is not None else ensure_cmd(node, tag, {})
    e.set("x", fmt(v[0]))
    e.set("y", fmt(v[1]))
    e.set("z", fmt(v[2]))


def set_child_rot(node, v):
    set_child_xyz(node, "SetActorRotation", v, 0)


def set_child_scale(node, v):
    set_child_xyz(node, "SetActorScale", v, 1)


def axis_index(spin):
    """A spin's axis as 0, 1 or 2; a value the format doesn't know reads as y (the checks report it)"""
    a = spin.get("axis") or "y"
    return "xyz".index(a) if a in ("x", "y", "z") else 1


def uses_mod(node):
    return node.tag in MOD_PARENTS and (first(node, "SetActorRotation") is not None or first(node, "SetActorScale") is not None)


class Binding:
    """A point stored in the XML: get() and set(v)"""

    def __init__(self, node, get, set_):
        self.node, self.get, self.set = node, get, set_


def zero():
    return vec.v3()


def xyz_binding(node, ax=("x", "y", "z"), offset=zero):
    def get():
        return vec.v3(num(node, ax[0]), num(node, ax[1]), num(node, ax[2])) + offset()

    def set_(v):
        o = offset()
        for i in range(3):
            node.set(ax[i], fmt(v[i] - o[i]))
    return Binding(node, get, set_)


def shift_binding(node, triples, anchor):
    def set_(v):
        d = np.asarray(v, float) - anchor()
        for trio in triples:
            for i, a in enumerate(trio):
                if a:
                    node.set(a, fmt(num(node, a) + d[i]))
    return Binding(node, anchor, set_)


def attached_points(node, offset=zero):
    """Absolute points that belong to an object and must follow it: enemy patrol/target points (their scripts use
    world coordinates), enemy attention boxes, platform start/path points and orbit centres (relative to a parent
    platform, if any). Boxes stored as corner + size follow by their centre."""
    out = []

    def box(e, c, sz):
        def get():
            return vec.v3(*[num(e, c[i]) + num(e, sz[i]) / 2 for i in range(3)])

        def set_(v):
            for i in range(3):
                e.set(c[i], fmt(v[i] - num(e, sz[i]) / 2))
        return Binding(e, get, set_)

    for e in node.iterdescendants():
        if not isinstance(e.tag, str):
            continue
        if e.tag in ("PlatSetInitialPos", "PlatPathPoint", "PlatOrbitAroundPoint"):
            out.append(xyz_binding(e, ("x", "y", "z"), offset))
            continue
        if e.tag == "EnemySetAttentionBbox":
            out.append(box(e, ("left", "top", "front"), ("width", "height", "depth")))
            continue
        for pre in ("home", "destination", "lookat"):
            if e.get(pre + "_x") is None:
                continue
            if pre == "home" and e.get("extent_x") is not None:
                out.append(box(e, ("home_x", "home_y", "home_z"), ("extent_x", "extent_y", "extent_z")))
            else:
                out.append(xyz_binding(e, (pre + "_x", pre + "_y", pre + "_z")))
    return out


class Ent:
    """One thing in the view: its kind, the node it draws, the XML it comes from and how to move, turn and size it"""

    def __init__(self, kind, obj, **kw):
        self.kind, self.obj = kind, obj
        self.node = self.tag = self.sub = None
        self.label = ""
        self.bind = None
        self.points = []
        self.rot = self.scl = None
        self.why = {}
        self.get_quat = self.set_quat = self.get_scale = self.set_scale = None
        self.place = None
        self.animate = None
        self.extra = None
        self.marks = None
        self.object_id = None
        self.imported = False
        self.missing_model = False
        self.nodes = None
        self.is_handle = False
        self.owner = None
        self.derived = False
        self.plat_tag = self.parent_tag = None
        self.parent_ent = None
        self.world_pos = None
        self.gate = None
        self.zone = False
        self.box_size0 = self.set_box = None
        self.size_fields = None
        self.__dict__.update(kw)


def box_volume(ent, node, corner, size):
    """An axis-aligned box stored as a min corner plus a size (wind volumes, puzzle trigger boxes). Moving shifts the
    corner; scaling resizes the box about its centre."""
    def sz():
        return vec.v3(*[num(node, size[i]) for i in range(3)])

    def centre():
        return vec.v3(*[num(node, corner[i]) for i in range(3)]) + sz() * 0.5

    def set_size(s):
        c = centre()
        for i in range(3):
            node.set(size[i], fmt(s[i]))
            node.set(corner[i], fmt(c[i] - s[i] / 2))

    def field(i):
        def set_(v):
            s = sz()
            s[i] = v
            set_size(s)
        return (size[i], lambda: num(node, size[i]), set_)

    ent.bind = shift_binding(node, [corner], centre)
    ent.scl = "box"
    ent.get_scale = lambda: np.ones(3)
    ent.box_size0 = sz
    ent.set_box = lambda s0, f: set_size(np.asarray(s0) * np.asarray(f))
    ent.size_fields = [field(i) for i in range(3)]
    return centre, sz


# Puzzle conditions that test where Glover, the ball or the camera is. parse.c (TOKEN_CONDITION) reads condition
# ids 35-41 into a PuzzleCamCond, and PuzzleEvaluateCondition (puzzle.c) tests them: a box is a min corner plus a
# size (IsPointWithinBox), a sphere is a centre and a radius, and the cylinder is a centre x/z and a radius between
# min_y and max_y. Id 34 (PuzzleCond0x22, CONDITION_HAND_CROSSES_LINE) is always true and has nothing to draw.
TRIGGERS = {
    "PuzzleCondGloverWithinVolume": ("box", "Glover"), "PuzzleCondBallWithinVolume": ("box", "ball"),
    "PuzzleCondCameraWithinVolume": ("box", "camera"), "PuzzleCondGloverWithinRangeOfPoint": ("sphere", "Glover"),
    "PuzzleCondBallWithinRangeOfPoint": ("sphere", "ball"), "PuzzleCondCameraWithinRangeOfPoint": ("sphere", "camera"),
    "PuzzleCondGloverWithinRangeOfPoint2": ("cylinder", "Glover"),
}
TRIGGER_LOOK = {"Glover": ("#ff8ad8", "✋"), "ball": ("#ffb36b", "●"), "camera": ("#b28dff", "◉")}
TRIGGER_NAME = {"box": "box", "sphere": "range", "cylinder": "cylinder"}


def trigger_volumes(puzzle):
    return [v for c in kids(puzzle, "PuzzleCond") for v in kids(c) if v.tag in TRIGGERS]


def lone_trigger(ent):
    """A puzzle with one trigger volume (a camera zone, for one) is duplicated and deleted whole with its volume"""
    return ent.kind == "trigger" and len(trigger_volumes(ent.node)) == 1


def dominant(*f):
    """The farthest of these scale factors from 1: a drag on any one gizmo axis resizes a radius"""
    out = 1.0
    for b in f:
        if abs(b - 1) > abs(out - 1):
            out = b
    return out


def hex_rgb(h):
    h = h.lstrip("#")
    return tuple(int(h[i:i + 2], 16) / 255 for i in (0, 2, 4))


def lines(points, color, mode="strip", dash=None, depth_test=True):
    n = Node()
    n.items.append(Lines(points, hex_rgb(color) if isinstance(color, str) else color, mode, dash, depth_test))
    return n


def fill(geometry, color, opacity):
    n = Node()
    n.items.append(Mesh(geometry, Material(color=hex_rgb(color), lit=False, transparent=True, opacity=opacity,
                                           depth_write=False)))
    return n


def solid(geometry, color):
    n = Node()
    n.items.append(Mesh(geometry, Material(color=hex_rgb(color), lit=False)))
    return n


_shapes = {}


def shape(name):
    """Shared geometry for markers"""
    if name not in _shapes:
        _shapes[name] = {"box": box_geometry, "sphere": lambda: sphere_geometry(1, 24, 16),
                         "cylinder": lambda: cylinder_geometry(1, 1, 32), "octa5": lambda: octahedron_geometry(5),
                         "ball4": lambda: sphere_geometry(4, 12, 8), "ball5": lambda: sphere_geometry(5, 12, 8)}[name]()
    return _shapes[name]


class Builder:
    """Builds the entities of a level. icon(glyph, color, size) makes a sprite node; model(id) a loaded glTF or None."""

    def __init__(self, ed):
        self.ed = ed

    def trigger_entity(self, vol, spec, label):
        shape_, who = spec
        color, glyph = TRIGGER_LOOK[who]
        obj, line_group = Node(), Node()
        icon = self.ed.icon(glyph, color, 12)
        obj.add(line_group, icon)
        ent = Ent("trigger", obj, sub=vol, label=label, why={"rot": WHY["triggerRot"]})
        if shape_ == "box":
            line_group.add(lines(box_edges(), color, "segments"))
            fill_geo = shape("box")
            _, dims = box_volume(ent, vol, ("x", "y", "z"), ("l", "w", "h"))
        elif shape_ == "sphere":
            for ax in "xyz":
                line_group.add(lines(ring_points(ax, 0), color, "loop"))
            fill_geo = shape("sphere")
            ent.bind = xyz_binding(vol)
            ent.scl = "box"
            ent.box_size0 = lambda: np.full(3, 2 * num(vol, "range"))
            ent.set_box = lambda s0, f: vol.set("range", fmt((s0[0] / 2) * dominant(*f)))
            ent.size_fields = [("range", lambda: num(vol, "range"), lambda v: vol.set("range", fmt(v)))]
            dims = lambda: np.full(3, num(vol, "range"))
        else:
            ends = []
            for y in (0.5, -0.5):
                line_group.add(lines(ring_points("y", y), color, "loop"))
                for x, z in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                    ends.append((x, y, z))
            sides = []
            for i in range(4):
                sides += [ends[i], ends[i + 4]]
            line_group.add(lines(sides, color, "segments"))
            fill_geo = shape("cylinder")

            def centre():
                return vec.v3(num(vol, "x"), (num(vol, "min_y") + num(vol, "max_y")) / 2, num(vol, "z"))

            def set_height(h):
                c = centre()[1]
                vol.set("min_y", fmt(c - h / 2))
                vol.set("max_y", fmt(c + h / 2))

            def set_box(s0, f):
                vol.set("range", fmt((s0[0] / 2) * dominant(f[0], f[2])))
                set_height(s0[1] * f[1])
            ent.bind = shift_binding(vol, [("x", None, "z"), (None, "min_y", None), (None, "max_y", None)], centre)
            ent.scl = "box"
            ent.box_size0 = lambda: vec.v3(2 * num(vol, "range"), num(vol, "max_y") - num(vol, "min_y"), 2 * num(vol, "range"))
            ent.set_box = set_box
            ent.size_fields = [("range", lambda: num(vol, "range"), lambda v: vol.set("range", fmt(v))),
                               ("height", lambda: num(vol, "max_y") - num(vol, "min_y"), set_height)]
            dims = lambda: vec.v3(num(vol, "range"), num(vol, "max_y") - num(vol, "min_y"), num(vol, "range"))
        fill_ = fill(fill_geo, color, 0.08)
        ent.extra = fill_   # not pickable: clicks reach the objects inside the volume
        ent.extra_pickable = False

        def place():
            c, d = ent.bind.get(), dims()
            s = np.maximum(0.5, np.abs(d))
            obj.pos = c
            line_group.scale = s
            fill_.pos = c.copy()
            fill_.scale = s.copy()
        ent.place = place
        place()
        return ent

    def build(self, root):
        ed = self.ed
        ents = []
        world = Node("world")
        platforms_by_tag = {}
        fog = amb = first_gate = None
        puzzle_index = 0
        for node in kids(root):
            tag = node.tag

            def add(ent, node=node, tag=tag):
                ent.node, ent.tag = node, tag
                ents.append(ent)
                world.add(ent.obj)
                return ent

            if tag == "FogConfiguration":
                fog = node
            elif tag == "AmbientLight":
                amb = node
            elif tag in ("LandActor", "BackgroundActor", "AnimatedBackgroundActor", "Water"):
                obj = Node()
                is_water = tag == "Water"
                ent = add(Ent("water" if is_water else "land", obj))
                ent.object_id = hex_id(node.get("object_id"))
                ent.label = node.get("name") or ent.object_id or tag
                pos = xyz_binding(node)
                ent.bind = (shift_binding(node, [("x", "y", "z"), ("left", "top", "front"), (None, "surface_y", None)], pos.get)
                            if is_water else pos)
                ent.rot, ent.scl = "xyz", "xyz"
                if not is_water:
                    def get_quat(node=node):
                        r = first(node, "SetActorRotation")
                        return quat_from_xyz(num(r, "x"), num(r, "y"), num(r, "z"))

                    def set_quat(q, axis=None, node=node):
                        e = euler_of(q)
                        r = ensure_cmd(node, "SetActorRotation", {})
                        for i, a in enumerate("xyz"):
                            r.set(a, fmt(e[i]))

                    def get_scale(node=node):
                        s = first(node, "SetActorScale")
                        return vec.v3(num(s, "x", 1), num(s, "y", 1), num(s, "z", 1))

                    def set_scale(v, node=node):
                        s = ensure_cmd(node, "SetActorScale", {})
                        for i, a in enumerate("xyz"):
                            s.set(a, fmt(v[i]))
                    ent.get_quat, ent.set_quat, ent.get_scale, ent.set_scale = get_quat, set_quat, get_scale, set_scale
                else:
                    ent.why = {"rot": WHY["modRot"]["water"], "scl": WHY["modScale"]["water"]}
                    ent.get_quat = lambda node=node: quat_from_xyz(*child_rot(node))
                    ent.set_quat = lambda q, axis=None, node=node: set_child_rot(node, euler_of(q))
                    ent.get_scale = lambda node=node: child_scale(node)
                    ent.set_scale = lambda v, node=node: set_child_scale(node, v)

                def place(ent=ent, obj=obj, pos=pos):
                    obj.pos = pos.get()
                    obj.quat = ent.get_quat()
                    obj.scale = ent.get_scale()
                ent.place = place
                place()
                self.attach_model(ent, obj, ent.object_id, 1, is_water)
                if is_water:
                    vol = Node()
                    vol.add(fill(shape("box"), "#2a8fd0", 0.12), lines(box_edges(), "#3fa8e0", "segments"))
                    ent.extra = vol
                    world.add(vol)
                    top_ = min(num(node, "top") + num(node, "bottom"), num(node, "surface_y"))
                    vol.pos = vec.v3(num(node, "left") + num(node, "width") / 2, (num(node, "top") + top_) / 2,
                                     num(node, "front") + num(node, "depth") / 2)
                    vol.scale = vec.v3(num(node, "width"), max(1, top_ - num(node, "top")), num(node, "depth"))
            elif tag in ("Platform", "NullPlatform"):
                obj = Node()
                ent = add(Ent("platform", obj))
                ent.object_id = hex_id(node.get("object_id"))
                ent.label = node.get("name") or ("null platform" if tag == "NullPlatform" else ent.object_id or tag)
                tag_node = first(node, "PlatSetTag")
                if tag_node is not None:
                    ent.plat_tag = num(tag_node, "tag")
                    platforms_by_tag[ent.plat_tag] = ent
                parent_node = first(node, "PlatSetParent")
                if parent_node is not None:
                    ent.parent_tag = num(parent_node, "tag")
                    ent.parent_ent = platforms_by_tag.get(ent.parent_tag)

                def parent_offset(ent=ent):
                    return ent.parent_ent.world_pos() if ent.parent_ent else vec.v3()
                pos_node = first(node, "PlatSetInitialPos")
                path_pts = kids(node, "PlatPathPoint")
                # The platform's position is its start (PlatSetInitialPos, else the first path point); the rest of
                # its path and its orbit centre are attached points that follow every move, rotation and scale.
                anchor = pos_node if pos_node is not None else (path_pts[0] if path_pts else None)
                pts = attached_points(node, parent_offset)
                ent.bind = next((b for b in pts if b.node is anchor), None) if anchor is not None else None
                ent.points = [b for b in pts if b is not ent.bind]
                ent.world_pos = lambda ent=ent: ent.bind.get() if ent.bind else vec.v3()
                if ent.bind is None:
                    # A platform with no position yet gets one
                    def set_(v, node=node, parent_offset=parent_offset):
                        p = ensure_cmd(node, "PlatSetInitialPos", {})
                        o = parent_offset()
                        for i, a in enumerate("xyz"):
                            p.set(a, fmt(v[i] - o[i]))
                    ent.bind = Binding(node, parent_offset, set_)
                # Rotation: SetActorRotation, the first child. The game sets it before SetPlatformPos builds the
                # collision. The game mod adds it to a spinning platform's angles. The angles the game turns are
                # SetActorRotation plus the spin's initial angle on its axis. Scale: PlatScale before the first path
                # point.

                def spin_angles(node=node):
                    s = first(node, "PlatConstantSpin")
                    v = vec.v3()
                    if s is not None:
                        v[axis_index(s)] = num(s, "initial_theta")
                    return v
                ent.rot, ent.scl = "xyz", "xyz"
                ent.why = {"rot": WHY["modRot"]["platform"]}
                ent.get_quat = lambda node=node, sa=spin_angles: quat_from_xyz(*(child_rot(node) + sa()))
                ent.set_quat = lambda q, axis=None, node=node, sa=spin_angles: set_child_rot(node, euler_of(q) - sa())

                def get_scale(node=node):
                    s = first(node, "PlatScale")
                    return vec.v3(num(s, "x", 1), num(s, "y", 1), num(s, "z", 1))

                def set_scale(v, node=node):
                    s = ensure_cmd(node, "PlatScale", {})
                    for i, a in enumerate("xyz"):
                        s.set(a, fmt(v[i]))
                ent.get_scale, ent.set_scale = get_scale, set_scale

                def place(ent=ent, obj=obj):
                    obj.pos = ent.world_pos()
                    obj.quat = ent.get_quat()
                    obj.scale = ent.get_scale()
                ent.place = place
                place()
                sp = first(node, "PlatConstantSpin")
                if sp is not None and abs(num(sp, "speed")) > 1e-6:
                    axis, speed = axis_index(sp), num(sp, "speed")

                    def animate(t, node=node, obj=obj, axis=axis, speed=speed, sa=spin_angles):
                        e = child_rot(node) + sa()
                        e[axis] += speed * t * 30
                        obj.quat = quat_from_xyz(*e)
                    ent.animate = animate
                if tag == "Platform":
                    self.attach_model(ent, obj, ent.object_id, 1.5 if ent.object_id == "0x7FDADB91" else 1)
                else:
                    obj.add(ed.icon("◇", KINDS["platform"][1], 12))
                if len(path_pts) > 1 or (path_pts and pos_node is not None):
                    bs = [xyz_binding(p, ("x", "y", "z"), parent_offset) for p in path_pts]
                    line = lines([b.get() for b in bs] + [bs[0].get()], "#6fb7ff", dash=(8, 5))
                    ent.extra = line
                    world.add(line)
                    for i, b in enumerate(bs):
                        h = solid(shape("octa5"), "#6fb7ff")
                        h.pos = b.get()
                        sub = Ent("platform", h, bind=b, node=node, tag=tag, sub=path_pts[i], label="%s · point %d" % (ent.label, i),
                                  is_handle=True, owner=ent, why={"rot": WHY["noOrientation"], "scl": WHY["noScale"]})
                        sub.place = lambda h=h, b=b: setattr(h, "pos", b.get())
                        ents.append(sub)
                        world.add(h)
                gate = first(node, "PlatCheckpoint")
                if gate is not None:
                    ent.gate = gate
                    if first_gate is None:
                        first_gate = (ent, gate)
                if first(node, "SetExit") is not None:
                    s = ed.icon("⇥", "#ffffff", 16)
                    s.pos = vec.v3(0, 30, 0)
                    obj.add(s)
            elif tag == "GaribGroup":
                for g in kids(node, "Garib"):
                    t = g.get("type")
                    color = {"garib": "#f2c14e", "bang_500pt": "#ff9f43", "extra_life": "#6bd88a", "mad_garib": "#ff6bd5"}.get(t, "#f2c14e")
                    glyph = {"garib": "✦", "bang_500pt": "!", "extra_life": "♥", "mad_garib": "✸"}.get(t, "✦")
                    obj = ed.icon(glyph, color, 12, opacity=0.45 if num(node, "initial_state") == 0 else 1.0)
                    ent = Ent("garib", obj, bind=xyz_binding(g), node=node, tag=tag, sub=g,
                              label="%s · group %s" % (t, node.get("group_id")), why={"rot": WHY["sprite"], "scl": WHY["sprite"]})
                    ent.place = lambda obj=obj, ent=ent: setattr(obj, "pos", ent.bind.get())
                    ent.place()
                    ents.append(ent)
                    world.add(obj)
            elif tag == "Enemy":
                obj = Node()
                ent = add(Ent("enemy", obj, bind=xyz_binding(node), points=attached_points(node)))
                type_name = node.get("type")
                tv = ed.enum_value("Enemy", "type", type_name)
                actor = ed.decomp_enum_name("ActorType", tv)
                ent.label = (type_name or "?") + (" (%s)" % actor if actor else "")
                ent.object_id = ed.enemy_model_id(type_name)
                # The game draws an enemy as Ry(facing)·R(SetActorRotation), and R = Ry·Rz·Rx (Euler YZX). The Y angle
                # goes into y_rotation (the facing its script starts from). Only the X and Z tilt needs the game mod.
                ent.rot, ent.scl = "xyz", "xyz"
                ent.why = {"rot": WHY["modRot"]["enemy"], "scl": WHY["modScale"]["enemy"]}

                def get_quat(node=node):
                    b = child_rot(node)
                    return quat_from_xyz(b[0], num(node, "y_rotation") + b[1], b[2])

                def set_quat(q, axis=None, node=node):
                    e = euler_of(q)
                    node.set("y_rotation", fmt(e[1]))
                    set_child_rot(node, vec.v3(e[0], 0, e[2]))
                ent.get_quat, ent.set_quat = get_quat, set_quat
                ent.get_scale = lambda node=node: child_scale(node)
                ent.set_scale = lambda v, node=node: set_child_scale(node, v)

                def place(ent=ent, obj=obj):
                    obj.pos, obj.quat, obj.scale = ent.bind.get(), ent.get_quat(), ent.get_scale()
                ent.place = place
                place()
                scales = ed.tables.get("enemyScales", [])
                sc = scales[tv]["value"] if tv is not None and 0 <= tv < len(scales) else 0.05
                self.attach_model(ent, obj, ent.object_id, sc or 0.05)
            elif tag == "Powerup":
                obj = Node()
                t = int(num(node, "type"))
                name = ed.decomp_enum_name("Pickup", t)
                ent = add(Ent("pickup", obj, bind=xyz_binding(node), label=(name or "type %d" % t).lower(),
                              why={"rot": WHY["modRot"]["pickup"], "scl": WHY["modScale"]["pickup"]}))
                # Ball-change powerups (bowling ball, ball bearing) have no standalone model in any bank; noclip skips
                # them too.
                pid = ed.powerup_model_id(t)
                ent.object_id = pid if ed.model_url(pid) else None
                ent.rot, ent.scl = "xyz", "xyz"
                ent.get_quat = lambda node=node: quat_from_xyz(*child_rot(node))
                ent.set_quat = lambda q, axis=None, node=node: set_child_rot(node, euler_of(q))
                ent.get_scale = lambda node=node: child_scale(node)
                ent.set_scale = lambda v, node=node: set_child_scale(node, v)

                def place(ent=ent, obj=obj):
                    obj.pos, obj.quat, obj.scale = ent.bind.get(), ent.get_quat(), ent.get_scale()
                ent.place = place
                place()
                if ent.object_id:
                    self.attach_model(ent, obj, ent.object_id, 0.4)
                else:
                    obj.add(ed.icon("★", KINDS["pickup"][1], 14))
            elif tag == "MrTip":
                obj = ed.icon("?", KINDS["tip"][1], 16)
                ent = add(Ent("tip", obj, bind=xyz_binding(node), label="message %s" % node.get("message_id"),
                              why={"rot": WHY["sprite"], "scl": WHY["sprite"]}))
                ent.place = lambda obj=obj, ent=ent: setattr(obj, "pos", ent.bind.get())
                ent.place()
            elif tag in ("GloverSpawnPoint", "BallSpawnPoint", "CameraSpawnPoint"):
                glyph = {"GloverSpawnPoint": "✋", "BallSpawnPoint": "●", "CameraSpawnPoint": "◉"}[tag]
                obj = Node()
                obj.add(ed.icon(glyph, "#ffffff", 18))
                ent = add(Ent("spawn", obj, bind=xyz_binding(node), label=tag.replace("SpawnPoint", " start"),
                              why={"rot": WHY["sprite"], "scl": WHY["sprite"]}))
                ent.place = lambda obj=obj, ent=ent: setattr(obj, "pos", ent.bind.get())
                ent.place()
                if tag == "BallSpawnPoint":
                    self.attach_model(ent, obj, "0x8E4DDE49", 0.05, decorative=True)
            elif tag == "Vent":
                obj = ed.icon("≋", KINDS["wind"][1], 14)
                ent = add(Ent("wind", obj, bind=xyz_binding(node, ("origin_x", "origin_y", "origin_z")),
                              label="vent type %s" % node.get("type"), why={"rot": WHY["noOrientation"], "scl": WHY["noScale"]}))
                ent.place = lambda obj=obj, ent=ent: setattr(obj, "pos", ent.bind.get())
                ent.place()
            elif tag == "Wind":
                box = lines(box_edges(), "#9be7e0", "segments")
                ent = add(Ent("wind", box, label="wind volume",
                              why={"rot": "Wind volumes are axis-aligned boxes (left/top/front + width/height/depth)."}))
                centre, size = box_volume(ent, node, ("left", "top", "front"), ("width", "height", "depth"))

                def place(box=box, centre=centre, size=size):
                    box.pos, box.scale = centre(), size()
                ent.place = place
                place()
            elif tag == "Buzzer":
                a = xyz_binding(node, ("end_1_x", "end_1_y", "end_1_z"))
                b = xyz_binding(node, ("end_2_x", "end_2_y", "end_2_z"))
                line = lines([a.get(), b.get()], "#ffd23f")

                def mid_set(v, a=a, b=b):
                    d = np.asarray(v, float) - (a.get() + b.get()) * 0.5
                    a.set(a.get() + d)
                    b.set(b.get() + d)
                mid = Binding(node, lambda a=a, b=b: (a.get() + b.get()) * 0.5, mid_set)
                ent = add(Ent("hazard", line, bind=mid, label="buzzer", why={"rot": WHY["noOrientation"], "scl": WHY["noScale"]}))
                ent.place = lambda line=line, a=a, b=b: line.items[0].set_points([a.get(), b.get()])
                for bb, i in ((a, 1), (b, 2)):
                    h = solid(shape("ball4"), "#ffd23f")
                    h.pos = bb.get()

                    def place_end(h=h, bb=bb, ent=ent):
                        h.pos = bb.get()
                        ent.place()
                    ents.append(Ent("hazard", h, bind=bb, node=node, tag=tag, label="buzzer end %d" % i, is_handle=True,
                                    owner=ent, why={"rot": WHY["noOrientation"], "scl": WHY["noScale"]}, place=place_end))
                    world.add(h)
            elif tag == "Puzzle":
                puzzle_index += 1
                zone = camera_zone(node)
                for cond in kids(node, "PuzzleCond"):
                    for vol in kids(cond):
                        t = TRIGGERS.get(vol.tag)
                        if not t:
                            continue
                        label = ("camera zone · puzzle %d" % puzzle_index if zone
                                 else "%s in %s · puzzle %d" % (t[1], TRIGGER_NAME[t[0]], puzzle_index))
                        ent = add(self.trigger_entity(vol, t, label))
                        if zone:
                            # A line from the middle of the zone to about where its camera stands
                            ent.zone = True
                            line = lines([[0, 0, 0], [0, 0, 0]], "#b28dff", dash=(12, 8))
                            eye = ed.icon("◉", "#b28dff", 12)
                            marks = Node()
                            marks.add(line, eye)
                            world.add(marks)
                            place_box = ent.place

                            def place(node=node, zone=zone, marks=marks, line=line, eye=eye, place_box=place_box):
                                place_box()
                                v = zone_camera(camera_zone(node) or zone)
                                marks.on = v is not None
                                marks.visible = v is not None and "trigger" not in ed.hidden
                                if v is None:
                                    return
                                line.items[0].set_points([v[0], v[1]])
                                eye.pos = v[1].copy()
                            ent.place = place
                            place()
                            ent.marks = marks
            elif tag == "AmbientSoundAtPoint":
                obj = ed.icon("♪", KINDS["sound"][1], 10)
                ent = add(Ent("sound", obj, bind=xyz_binding(node), label="sound %s" % (node.get("sound_id") or ""),
                              why={"rot": WHY["noOrientation"], "scl": WHY["noScale"]}))
                ent.place = lambda obj=obj, ent=ent: setattr(obj, "pos", ent.bind.get())
                ent.place()
        # Where Glover starts: the first restart gate (PlatCheckpoint) unless the level has a GloverSpawnPoint.
        # parse.c TOKEN_RESTART: the first gate is the start, its theta Glover's facing (hub levels override it by world).
        if first_gate and not any(e.tag == "GloverSpawnPoint" for e in ents):
            gent, gate = first_gate
            obj = Node()
            s = ed.icon("✋", "#ffffff", 18)
            s.pos = vec.v3(0, 22, 0)
            obj.add(s)
            arrow = Node()
            arrow.add(lines([[0, 6, 0], [0, 6, 22]], "#ffffff"))
            from .gizmo import cone
            arrow.items.append(Mesh(cone(vec.v3(0, 6, 22), vec.v3(0, 6, 30), 3), Material(color=(1, 1, 1), lit=False)))
            obj.add(arrow)
            obj.pos = gent.world_pos()
            obj.quat = vec.quat_axis_angle((0, 1, 0), num(gate, "theta"))
            world.add(obj)
            ent = Ent("spawn", obj, node=gent.node, tag="Platform", label="Glover start (first checkpoint)", derived=True,
                      why={"rot": "Set by the first checkpoint platform’s theta.", "scl": WHY["noScale"]})
            obj.entity = gent   # clicking it selects the checkpoint platform
            ents.append(ent)
        for e in ents:
            if not e.derived:
                e.obj.entity = e
            if e.extra is not None:
                e.extra.entity = e
        return world, ents, fog, amb

    def attach_model(self, ent, obj, object_id, scale=1.0, is_water=False, decorative=False):
        ed = self.ed
        ent.imported = ed.is_import(object_id)
        model = ed.load_model(object_id) if object_id else None
        if model is None:
            if not decorative:
                ph = lines(box_edges(20, 20, 20), "#ff00ff", "segments")
                obj.add(ph)
                ent.missing_model = bool(object_id)
            return
        m = model.instance()
        m.scale = np.full(3, float(scale))
        ent.nodes = model.draw_nodes
        if is_water:
            for n, _ in m.walk(visible_only=False):
                n.items = [Mesh(it.geometry, it.material.copy(transparent=True, opacity=0.6, depth_write=False), it.matrix)
                           if isinstance(it, Mesh) else it for it in n.items]
        obj.add(m)


# ---------------------------------------------------------------------------------------------------------------
# Camera zones: a puzzle whose one condition is Glover inside a box (cond_type 35) and whose actions only set the
# camera. The shipped levels hold about 170 of them. camera.c puts the level's own camera back every frame Glover is
# in no zone. LookAtGlover's angle is the direction from the camera to Glover (atan2(dx, dz)); its distance is how
# fast the camera turns there. TweenYAdjust raises the camera over Glover, TweenDistance sets how far back it stands,
# and FlyTowardsPoint sends it to a fixed point.

CAM_ACTIONS = {70: "PuzzleActionCameraTweenYAdjust", 71: "PuzzleActionCameraTweenDistance",
               73: "PuzzleActionCameraFlyTowardsPoint", 74: "PuzzleActionCameraLookAtGlover"}
CAM_TYPE = {n: t for t, n in CAM_ACTIONS.items()}


def camera_zone(puzzle):
    """The zone's settings ({vol, look, speed, distance, height, fly}; missing ones are None), or None when the puzzle
    is anything else"""
    if puzzle is None or puzzle.tag != "Puzzle":
        return None
    conds, acts = kids(puzzle, "PuzzleCond"), kids(puzzle, "PuzzleAction")
    if len(conds) != 1 or not acts or len(kids(puzzle)) != len(conds) + len(acts):
        return None
    vol = first(conds[0], "PuzzleCondGloverWithinVolume")
    if vol is None:
        return None
    z = {"vol": vol, "look": None, "speed": None, "distance": None, "height": None, "fly": None}
    for a in acts:
        ch = kids(a)
        b = ch[0] if ch else None
        if b is None or b.tag not in CAM_TYPE:
            return None
        if b.tag == CAM_ACTIONS[74]:
            z["look"], z["speed"] = num(b, "angle"), num(b, "distance")
        elif b.tag == CAM_ACTIONS[71]:
            z["distance"] = num(b, "distance")
        elif b.tag == CAM_ACTIONS[70]:
            z["height"] = num(b, "y_adjust")
        else:
            z["fly"] = vec.v3(num(b, "x"), num(b, "y"), num(b, "z"))
    return z


def set_camera_zone(puzzle, z):
    """Replaces the zone's actions with these settings (None leaves an action out), in the game's order: look,
    distance, height, fly"""
    from .xmlutil import make_el, r1
    for a in kids(puzzle, "PuzzleAction"):
        puzzle.remove(a)

    def act(t, attrs):
        a = make_el("PuzzleAction", {"action_type": str(t)})
        a.append(make_el(CAM_ACTIONS[t], {**attrs, "activation_delay": "0"}))
        puzzle.append(a)
    if z.get("look") is not None:
        speed = z.get("speed")
        act(74, {"angle": fmt(z["look"] % (2 * math.pi)), "distance": fmt(2 if speed is None else speed)})
    if z.get("distance") is not None:
        act(71, {"distance": fmt(z["distance"])})
    if z.get("height") is not None:
        act(70, {"y_adjust": fmt(z["height"])})
    if z.get("fly") is not None:
        f = z["fly"]
        act(73, {"x": r1(f[0]), "y": r1(f[1]), "z": r1(f[2])})


# The presets, for a player who walks the way `facing` points (radians about y, 0 along +z). Angles are from the
# camera to Glover.
CAM_PRESETS = {
    "behind": ("Behind", "turns to look the way the player goes", lambda f, c: {"look": f, "speed": 2, "distance": 220, "height": 60}),
    "high": ("High", "rises and looks down", lambda f, c: {"height": 320, "distance": 140}),
    "side": ("Side", "swings round to the right of the way on", lambda f, c: {"look": f + math.pi / 2, "speed": 1.5, "distance": 260, "height": 60}),
    "left": ("Other side", "swings round to the left", lambda f, c: {"look": f - math.pi / 2, "speed": 1.5, "distance": 260, "height": 60}),
    "front": ("Front", "faces Glover: you can’t see where you go", lambda f, c: {"look": f + math.pi, "speed": 2, "distance": 160, "height": 30}),
    "low": ("Low", "drops to Glover’s height and comes close", lambda f, c: {"height": -60, "distance": 40}),
    "far": ("Far", "stands far back and high", lambda f, c: {"distance": 700, "height": 260}),
    "fixed": ("Fixed", "flies to a point ahead and watches from there",
              lambda f, c: {"fly": np.asarray(c, float) + vec.v3(math.sin(f) * 250, 260, math.cos(f) * 250)}),
}


def preset_zone(key, facing, centre):
    z = {"look": None, "speed": None, "distance": None, "height": None, "fly": None}
    z.update(CAM_PRESETS[key][2](facing, centre))
    return z


def zone_centre(vol):
    return vec.v3(num(vol, "x") + num(vol, "l") / 2, num(vol, "y") + num(vol, "w") / 2, num(vol, "z") + num(vol, "h") / 2)


def zone_camera(z):
    """Where the camera about stands for a Glover at the middle of the zone, as (glove, eye), or None when the zone
    keeps the level's direction"""
    c = zone_centre(z["vol"])
    if z.get("fly") is not None:
        return c, np.array(z["fly"], float)
    if z.get("look") is None:
        return None
    # camera.c: the camera stands distance / 3 + 30 + the height over Glover (in Atlantis 1: 130 back without a zone,
    # 149 back and 408 up in a High zone, tools/camera_zone_check.py)
    d = 130 if z.get("distance") is None else z["distance"]
    h = z.get("height") or 0
    return c, c + vec.v3(-math.sin(z["look"]) * d, d / 3 + 30 + h, -math.cos(z["look"]) * d)
