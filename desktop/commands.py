"""What the level format allows inside each element, read from decomp.json (libgarib's ksy with the decomp's names),
and the order the game needs.

A command that "modifies" a kind (PLATFORM, ENEMY...) goes inside an element that "declares" that kind. libgarib
writes every element as one command in document order, and the game parses the level as a stream of tokens: each
child acts on the last declared object. So a child's place among its siblings is its place in the stream, and the
order matters (gloveworld src/parse.c):
  - A platform builds its collision when it is placed (PlatSetInitialPos or the first PlatPathPoint), from its scale
    and rotation at that moment. A platform that never moves never rebuilds it.
  - PlatScale after the first point is refused and its data is then read as tokens.
  - Spin, orbit, crumble and breakable modifiers write through the structure their main command makes, with no
    check that it exists.
  - EnemyFinalize ends an enemy: everything after it belongs to the next object.
"""
from .xmlutil import kids, make_el

# Commands that have their own tools (the enemy script editor and the puzzle editor) or that the editor writes for
# itself (an enemy's end)
OWN_TOOLS = {"EnemyNormalInstruction", "EnemyConditionalInstruction", "EnemyAttackInstruction", "PuzzleCond",
             "PuzzleAction", "EnemyFinalize", "Garib"}
FIRST = ("SetActorRotation", "SetActorScale")
LAST = ("EnemyFinalize",)
PLATFORMS = ("Platform", "NullPlatform")
# Commands the menus leave out: unused in the shipped levels, or they crash on a wrong value (Actor0xbf)
HIDDEN = {"PlatStrobe", "Rope", "PlatSine", "PlatOrbit", "Actor0xbf"}

# A platform's children in the order the game needs them (section 4 of the spec, parse.c): set-up, placement, path
# settings, motion, features, and the head start last
MOTION = {"PlatConstantSpin", "PlatSpin0x80", "PlatTopple0x81", "Plat0x9f", "PlatRocking", "PlatTurnTowardsPathPoint",
          "PlatGoForwards0x5f", "LookAtHand0x60", "LookAtBall0x61", "PlatCat0x69"}
# PlatCopySpinFromParent makes a motion too, but doesn't reset one that is there
MOTION_MAKERS = MOTION | {"PlatCopySpinFromParent"}
MOTION_MODS = {"PlatSpinPause0x7c", "PlatSpinFlip", "Plat0x7e", "Plat0xc3", "PlatSpinSound0xc5", "Plat0x78"}
ORBIT_MODS = {"PlatOrbitPause", "PlatOrbitFlip0x77", "PlatOrbitSound0xc4", "Plat0xc6"}
FREES_FACES = {"PlatNoClip", "PlatSpike"}       # and SetExit type loading_zone
PHASE = {}
for _p, _tags in enumerate((
        ("SetActorRotation", "SetActorScale", "PlatScale", "PlatSetParent", "PlatSetCollisionYOffset"),
        ("PlatSetInitialPos", "PlatPathPoint", "PlatPathSoundAtPointHit"),
        ("PlatMaxVelocity", "PlatPathAcceleration", "PlatReverseAtEndsOfPath", "PlatPathSetStartingPoint"),
        tuple(MOTION | MOTION_MODS | ORBIT_MODS) + ("PlatOrbitAroundPoint", "PlatCopySpinFromParent"),
        (),                                       # features: everything not named
        ("PlatNoClip", "PlatSpike"),
        ("PlatVentAdvanceFrames",))):
    for _t in _tags:
        PHASE[_t] = _p
FEATURES = 4
# The set-up commands that change the collision the platform builds when it is placed
SETUP = ("SetActorRotation", "SetActorScale", "PlatScale")
# Children that stay right after their anchor, and move with it
GLUE = {"PlatPathSoundAtPointHit": {"PlatPathPoint"}, "PlatDestructibleSound": {"PlatDestructible"},
        "PlatSpecial0xc7": {"PlatCrumb0x67"}}
GLUE.update({t: MOTION_MAKERS | MOTION_MODS for t in MOTION_MODS})
GLUE.update({t: {"PlatOrbitAroundPoint"} | ORBIT_MODS for t in ORBIT_MODS})
# Children that need another one earlier in the same platform: (anchors, how many, what to tell the author)
NEEDS = {t: (MOTION_MAKERS, 1, "Add a spin or other motion first") for t in MOTION_MODS}
NEEDS.update({t: ({"PlatOrbitAroundPoint"}, 1, "Add an orbit first") for t in ORBIT_MODS})
NEEDS.update({
    "PlatPathSoundAtPointHit": ({"PlatPathPoint"}, 1, "Add a path point first"),
    "PlatTurnTowardsPathPoint": ({"PlatPathPoint"}, 2, "Add two path points first"),
    "PlatGoForwards0x5f": ({"PlatPathPoint"}, 2, "Add two path points first"),
    "PlatPathSetStartingPoint": ({"PlatPathPoint"}, 1, "Add a path point first"),
    "PlatDestructibleSound": ({"PlatDestructible"}, 1, "Make it breakable first"),
    "PlatSpecial0xc7": ({"PlatCrumb0x67"}, 1, "Make it crumble first"),
})
# Pairs that write the same field: the game refuses the second one and reads its data as tokens
EXCLUSIVE = (("PlatSpinPause0x7c", "PlatSpinFlip"), ("PlatOrbitPause", "PlatOrbitFlip0x77"),
             ("PuzzleAnd", "PuzzleOr", "PuzzleAny"))
# Commands of which an element may hold one; a second one replaces the first in the game or confuses it
ONE_OF = {"SetActorRotation", "SetActorScale", "PlatScale", "PlatSetParent", "PlatSetInitialPos", "PlatSetTag",
          "PlatCheckpoint", "SetExit", "SetTeleport", "PlatOrbitAroundPoint", "PlatDestructible", "PlatCrumb0x67",
          "PlatMaxVelocity", "PlatPathAcceleration", "PlatformConveyor", "PlatActorSurfaceType", "PlatSpecial0xb9",
          "PlatNoClip", "PuzzleNumtimes", "EnemySetAttentionBbox"}
# What a NullPlatform (a platform with no model: a parent or orbit centre) may hold. The rest use the model or the
# collision, which it doesn't have (parse.c TOKEN_DUMMY).
NULL_OK = {"SetActorRotation", "SetActorScale", "PlatSetTag", "PlatSetInitialPos", "PlatOrbitAroundPoint", "PlatPathPoint",
           "PlatScale", "PlatConstantSpin", "PlatMaxVelocity", "PlatPathAcceleration", "PlatSetParent",
           "PlatReverseAtEndsOfPath", "PlatPathSetStartingPoint", "PlatCopySpinFromParent"} | ORBIT_MODS
# How many of a child the game has room for
LIMITS = {"PlatPathPoint": 20, "BuzzerDutyCycle": 6, "VentDutyCycle": 6, "CameoInst": 20}
# Commands with a platform that never moves: the game never rebuilds its collision (parse.c, the flag 0x20000)
MOVERS = {"PlatPathPoint", "PlatOrbitAroundPoint", "PlatPush0x5b", "PlatCrumb0x67", "PlatSetParent", "PlatMvspn0x58",
          "PlatMvspn0x59", "PlatMvspn0x5a", "PlatMvspn0x73", "PlatMvspn0x74", "LookAtHand0x60", "LookAtBall0x61"} | MOTION

# Values for a new command, where the type-neutral 0 would be wrong (the shipped levels' usual values, or
# tested ones)
DEFAULTS = {
    "SetActorScale": {"x": "1.0", "y": "1.0", "z": "1.0"},
    "PlatScale": {"x": "1.0", "y": "1.0", "z": "1.0"},
    "PlatPathPoint": {"duration": "-100"},
    "PlatMaxVelocity": {"velocity": "10.0"},
    "PlatPathAcceleration": {"acceleration": "0.8"},
    "PlatConstantSpin": {"axis": "y", "initial_theta": "0.0", "speed": "0.05"},
    "SetExit": {"behavior": "1", "type": "solid_platform"},
    "PlatOrbitAroundPoint": {"axis": "1", "speed": "0.02"},
    "PlatActorSurfaceType": {"value": "0"},
    "SetObjectSparkle": {"period": "10"},
    "BuzzerDutyCycle": {"frames_off": "40", "frames_on": "40"},
    "VentDutyCycle": {"frames_off": "1", "frames_on": "1"},
    "PlatMovementSound": {"sound_id": "167", "volume": "100", "pitch": "128"},
    "PlatPathSoundAtPointHit": {"sound_id": "191", "volume": "100", "pitch": "128"},
    "PlatDestructibleSound": {"sound_id": "138", "volume": "100", "pitch": "128"},
    "PlatCrumb0x67": {"u16_0x02": "70", "u16_0x04": "10", "u16_0x08": "0x3FCCCCCD"},
    "PlatDestructible": {"flags": "3", "num_fragments": "3"},
    "Backdrop": {"scale_x": "4096", "scale_y": "4096", "scroll_speed_x": "1"},
    "SetGravity": {"strength": "1.2"},
    "PuzzleNumtimes": {"n": "1"},
    "FogConfiguration": {"fog_enabled": "1", "r": "40", "g": "40", "b": "60", "fog_distance": "990", "near_clip": "1500"},
    "AmbientLight": {"r": "110", "g": "110", "b": "110"},
    "DiffuseLight": {"r": "160", "g": "160", "b": "160", "theta_x": "0.0", "theta_y": "3.7"},
}

# Plain names for the commands an author is likely to want, shown first in the Add command menu
NAMES = {
    "PlatConstantSpin": "Spin (constant)", "PlatPathPoint": "Path point", "PlatMaxVelocity": "Path speed",
    "PlatPathAcceleration": "Path acceleration", "PlatReverseAtEndsOfPath": "Reverse at the path's ends",
    "PlatCheckpoint": "Checkpoint", "SetExit": "Exit", "SetTeleport": "Teleport", "PlatformConveyor": "Conveyor",
    "PlatActorSurfaceType": "Surface type (ice...)", "PlatSpecial0xb9": "Camera passes through",
    "PlatNoClip": "No collision", "PlatSetTag": "Tag (for puzzles)", "PlatSetParent": "Parent platform",
    "PlatScale": "Scale", "SetActorRotation": "Rotation", "SetActorScale": "Scale",
    "PlatPathSoundAtPointHit": "Sound at path point", "PlatSpike": "Spikes (hurts)", "PlatCauseDamage": "Hurts",
    "PlatOrbitAroundPoint": "Orbit a point", "PlatRocking": "Rocking", "PlatDestructible": "Breakable",
    "PlatMovementSound": "Movement sound", "PlatSetInitialPos": "Start position", "PuzzleNumtimes": "Times it can fire",
    "PuzzleAnd": "Conditions: all", "PuzzleOr": "Conditions: any", "PuzzleAny": "Conditions: any (2)",
    "EnemySetAttentionBbox": "Attention box", "PlatSetCollisionYOffset": "Collision height offset",
    "PlatInteractsWithWater": "Floats on water", "SetObjectSparkle": "Sparkle",
}


def as_list(v):
    return [] if v is None else v if isinstance(v, list) else [v]


class Format:
    def __init__(self, decomp):
        self.decomp = decomp
        self.commands = decomp["commands"]
        self.types = decomp["types"]
        self.declares = {}   # element -> kinds it declares
        self.by_kind = {}    # kind -> commands that modify it
        for name, c in self.commands.items():
            for k in as_list(c.get("declares")):
                self.declares.setdefault(name, []).append(k)
            for k in as_list(c.get("modifies")):
                self.by_kind.setdefault(k, []).append(name)

    def children_of(self, tag):
        """Commands that may go inside an element, the common ones first"""
        out = []
        for k in self.declares.get(tag, []):
            for name in self.by_kind.get(k, []):
                if name not in out and name not in OWN_TOOLS and name not in HIDDEN and not self.commands[name].get("groupsInto"):
                    out.append(name)
        if tag == "NullPlatform":
            out = [n for n in out if n in NULL_OK]
        return sorted(out, key=lambda n: (n not in NAMES, NAMES.get(n, n).lower()))

    def fields(self, tag):
        c = self.commands.get(tag) or self.types.get(tag)
        return [f for f in (c or {}).get("fields", []) if f["type"] != "switch"]

    def enum(self, tag, field):
        c = self.commands.get(tag) or self.types.get(tag)
        if not field.get("enum") or not c:
            return None
        key = field["enum"] if "::" in field["enum"] else c["ksy"] + "::" + field["enum"]
        return self.decomp["ksyEnums"].get(key)

    def defaults(self, tag):
        """Every field with a neutral value: 0 (0.0 for floats), the first enum value, an empty name"""
        out = {}
        for f in self.fields(tag):
            en = self.enum(tag, f)
            if en:
                out[f["id"]] = list(en.values())[0]
            elif f["type"] == "str":
                out[f["id"]] = ""
            elif f["type"].startswith("f"):
                out[f["id"]] = "0.0"
            else:
                out[f["id"]] = "0"
        out.update(DEFAULTS.get(tag, {}))
        return out

    def label(self, tag):
        return NAMES.get(tag, tag)

    def make(self, tag, attrs=None):
        a = self.defaults(tag)
        a.update(attrs or {})
        return make_el(tag, a)


def phase(node):
    return PHASE.get(node.tag, FEATURES)


def frees_faces(node):
    return node.tag in FREES_FACES or (node.tag == "SetExit" and node.get("type") in ("loading_zone", "0"))


def glued_end(ch, i):
    """The index after child i and the children glued to it"""
    j = i + 1
    while j < len(ch) and ch[j].tag in GLUE and ch[i].tag in GLUE[ch[j].tag]:
        j += 1
    return j


def place_for(parent, tag):
    """The index a new child with this tag goes at"""
    ch = kids(parent)
    if parent.tag in PLATFORMS:
        if tag in GLUE:
            at = [i for i, c in enumerate(ch) if c.tag in GLUE[tag]]
            if at:
                return glued_end(ch, at[-1])
        if tag == "PlatPathPoint":
            at = [i for i, c in enumerate(ch) if c.tag == "PlatPathPoint"]
            if at:
                return glued_end(ch, at[-1])
        # At the end of its phase, and never after a child of a later phase
        p = PHASE.get(tag, FEATURES)
        same = [i for i, c in enumerate(ch) if phase(c) == p and c.tag not in GLUE]
        later = [i for i, c in enumerate(ch) if phase(c) > p]
        i = glued_end(ch, same[-1]) if same else len(ch)
        return min(i, later[0]) if later else i
    if tag in FIRST:
        i = 0
        while i < len(ch) and ch[i].tag in FIRST and FIRST.index(ch[i].tag) < FIRST.index(tag):
            i += 1
        return i
    i = len(ch)
    # Nothing goes after an enemy's EnemyFinalize
    if ch and ch[-1].tag in LAST and tag not in LAST:
        i -= 1
    return i


def insert(parent, child):
    """Puts a new child command where the game reads it right"""
    ch = kids(parent)
    i = place_for(parent, child.tag)
    if i >= len(ch):
        parent.append(child)
    else:
        ch[i].addprevious(child)
    child.tail = None
    return child


def placement(tags):
    """The index of the child that places the platform, and of its first path point (None when there is none)"""
    placed = next((i for i, t in enumerate(tags) if t in ("PlatSetInitialPos", "PlatPathPoint")), None)
    return placed, (tags.index("PlatPathPoint") if "PlatPathPoint" in tags else None)


def keeps_collision(node, tags):
    """A platform that never moves and has collision: the game builds the collision once, when it is placed"""
    return node.tag == "Platform" and not (MOVERS & set(tags)) and not any(frees_faces(c) for c in kids(node))


def misplaced(node):
    """The first child in the wrong place and the index it should go before, or None. Only what the game needs is
    checked: the shipped levels put most children in any order."""
    ch = kids(node)
    tags = [c.tag for c in ch]
    placed, first_point = placement(tags)
    still = keeps_collision(node, tags)
    for i, c in enumerate(ch):
        if placed is not None and i > placed and ((c.tag in SETUP and still) or (
                first_point is not None and i > first_point and c.tag in ("PlatScale", "PlatSetParent"))):
            return c, placed
        need = NEEDS.get(c.tag)
        if need and missing_need(node, c.tag, i) and not missing_need(node, c.tag):
            at = [j for j, t in enumerate(tags) if t in need[0]][need[1] - 1]
            return c, at + 1
        if c.tag == "PlatActorSurfaceType":
            free = next((j for j, x in enumerate(ch[:i]) if frees_faces(x)), None)
            if free is not None:
                return c, free
    return None


def tidy(parent):
    """Moves a platform's children that are in an order the game reads wrong. The rest keep their order. Returns
    whether anything moved."""
    if parent.tag not in PLATFORMS:
        return False
    moved = False
    for _ in range(4 * len(kids(parent))):
        m = misplaced(parent)
        if m is None:
            break
        c, j = m
        ch = kids(parent)
        target = ch[j] if j < len(ch) else None
        parent.remove(c)
        if target is None:
            parent.append(c)
        else:
            target.addprevious(c)
        c.tail = None
        moved = True
    return moved


def missing_need(parent, tag, before=None):
    """What a new child with this tag needs and the parent doesn't have yet (None when nothing). With before, only
    the children before that index count."""
    need = NEEDS.get(tag)
    if not need:
        return None
    ch = kids(parent)[:before] if before is not None else kids(parent)
    if sum(1 for c in ch if c.tag in need[0]) < need[1]:
        return need[2]
    return None


def refusal(parent, tag):
    """Why the parent can't take another child with this tag (None when it can)"""
    ch = kids(parent)
    have = [c.tag for c in ch]
    if tag in ONE_OF and tag in have:
        return "It already has one."
    if parent.tag in PLATFORMS and tag in MOTION and MOTION & set(have):
        return "It already has a motion; a second one replaces it (\u201cPlat already has spin\u201d)."
    for pair in EXCLUSIVE:
        if tag in pair and set(pair) & set(have) - {tag}:
            return "It can't have this and %s together." % next(t for t in pair if t in have and t != tag)
    if tag in LIMITS and have.count(tag) >= LIMITS[tag]:
        return "The game has room for %d." % LIMITS[tag]
    if parent.tag == "NullPlatform" and tag not in NULL_OK:
        return "A platform with no model can't use it."
    if parent.tag in PLATFORMS and tag == "PlatPathPoint" and "PlatOrbitAroundPoint" in have:
        return "An orbiting platform can't follow a path."
    if parent.tag in PLATFORMS and tag == "PlatOrbitAroundPoint" and have.count("PlatPathPoint") >= 2:
        return "A platform with two or more path points can't orbit."
    return missing_need(parent, tag)


def platform_rules(node):
    """The order and combination rules a platform breaks: (key, message, the game's warning text or None)"""
    out = []
    ch = kids(node)
    tags = [c.tag for c in ch]
    placed, first_point = placement(tags)
    still = keeps_collision(node, tags)
    if tags.count("PlatPathPoint") > 20:
        out.append(("points", "%d path points; the game keeps the first 20 (PlatformAddPoint)." % tags.count("PlatPathPoint"), "too many POINTS"))
    motions = [t for t in tags if t in MOTION]
    if len(motions) > 1:
        out.append(("motion", "Has %d motions (%s); each one resets the one before (\u201cPlat already has spin\u201d)." % (
            len(motions), ", ".join(motions)), None))
    for i, c in enumerate(ch):
        t = c.tag
        miss = missing_need(node, t, i)
        if miss:
            out.append(("needs:%s" % t, "%s has nothing to act on: %s. The game writes through an empty pointer." % (t, miss.lower()), None))
        if node.tag == "NullPlatform" and t not in NULL_OK:
            out.append(("null:%s" % t, "%s on a platform with no model: it uses a model or collision the platform doesn't have." % t, None))
        if t == "PlatScale" and first_point is not None and i > first_point:
            out.append(("scale-order", "PlatScale comes after the first path point. The game refuses it and reads its numbers as commands.", "scale after first point"))
        elif t in SETUP and placed is not None and i > placed and still:
            out.append(("setup-order:%s" % t, "%s comes after the platform is placed. It doesn't move, so its collision keeps the old size and turn. Use Tidy order." % t, None))
        if t == "PlatSetParent" and first_point is not None and i > first_point:
            out.append(("parent-order", "PlatSetParent comes after the first path point and is ignored.", "parenttag after first point"))
        if t == "PlatOrbitAroundPoint" and tags[:i].count("PlatPathPoint") >= 2:
            out.append(("orbit", "Orbit after two or more path points: the game refuses it and reads its numbers as commands.", "cannot orbit - more than one point"))
        if t == "PlatPathPoint" and "PlatOrbitAroundPoint" in tags[:i]:
            out.append(("orbit-point", "A path point after an orbit is ignored.", "it is an orbiter"))
        if t == "PlatPathSetStartingPoint":
            try:
                idx = int(c.get("point_idx") or "0", 0)
            except ValueError:
                idx = 0
            if idx >= tags[:i].count("PlatPathPoint"):
                out.append(("startpoint", "The starting point %d isn't one of the path points before it." % idx, None))
        if t == "PlatActorSurfaceType" and any(frees_faces(x) for x in ch[:i]):
            out.append(("surface", "The surface type comes after the collision was freed (no collision, spikes or a loading zone).", None))
    for pair in EXCLUSIVE[:2]:
        if all(t in tags for t in pair):
            later = max(pair, key=tags.index)
            out.append(("excl:%s" % later, "Has both %s; the game refuses the later one and reads its numbers as commands." % " and ".join(pair), None))
    if tags.count("PlatSetTag") > 1:
        out.append(("2tag", "Has %d tags; puzzles find the platform by the last one." % tags.count("PlatSetTag"), None))
    return out


def ensure_cmd(parent, tag, attrs=None):
    """Gets a child command, or adds one where the game reads it right"""
    for c in kids(parent):
        if c.tag == tag:
            return c
    return insert(parent, make_el(tag, attrs or {}))
