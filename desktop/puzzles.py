"""Puzzles: what each condition and action does, in plain words, the order a puzzle's children go in, and the checks
the schema can't make (gloveworld src/puzzle.c and src/parse.c).

How the game runs a puzzle: every frame, CheckPuzzles tests each puzzle's conditions. When they hold and the puzzle
has firings left (PuzzleNumtimes; none means every frame), each action starts its wait (activation_delay, in game
frames) and then runs once. A puzzle with no conditions fires on the first frame.
"""
import struct
from .xmlutil import kids, make_el, num, inum

# Plain names for the conditions, by cond_type. The game's own type is cond_type - 9.
CONDS = {
    9: "Platform rests at a path point", 12: "Platform just arrived at a path point (-1: is moving)",
    10: "Spin paused at a stop", 13: "Spin just reached a stop", 11: "Orbit paused at a stop", 14: "Orbit just reached a stop",
    15: "Glover is on a platform", 16: "Glover steps onto or off a platform", 17: "The ball is on a platform",
    18: "The ball rolls onto or off a platform", 19: "An enemy is on a platform", 20: "An enemy steps onto or off a platform",
    21: "Glover touches a platform", 22: "Glover starts or stops touching a platform", 23: "The ball touches a platform",
    24: "The ball hits a platform", 25: "An enemy touches a platform", 26: "An enemy starts or stops touching a platform",
    27: "Counter = value", 28: "Counter ≠ value", 29: "Counter > value", 30: "Counter < value",
    31: "Pushed block is at an edge", 32: "Pushed block just reached an edge",
    33: "An enemy is gone (or there)", 34: "Glover crosses a line",
    35: "Glover is inside a box", 36: "Glover is near a point", 37: "The ball is inside a box", 38: "The ball is near a point",
    39: "The camera is inside a box", 40: "The camera is near a point", 41: "Glover is inside a cylinder",
    42: "A platform is broken or gone", 43: "A pickup or garib group is collected", 44: "A pickup is on",
}
# The conditions true for only one frame at a time (an edge). The rest stay true while the state lasts.
EDGES = {13, 14, 16, 18, 20, 22, 24, 26, 32}

ACTIONS = {
    45: "Start a path platform (release it)", 46: "Set a platform's spin speed", 47: "Set a platform's orbit speed",
    48: "Send a path platform to a point", 51: "Spin a platform about an axis", 52: "Turn vents, buzzers, fans or teleports on or off",
    53: "Set a conveyor's push", 54: "Hide or show a platform", 55: "Make a platform fall (physics)",
    56: "Set a counter", 57: "Add to a counter", 58: "Subtract from a counter",
    59: "Drop a pickup", 60: "Bring in an enemy", 61: "Set a platform's velocity", 62: "Camera: look at a platform",
    63: "Camera: look at a point", 64: "Camera: move to a point", 65: "Camera: set its distance", 66: "Camera: raise or lower",
    67: "Jump an enemy's script", 68: "Turn wind on or off", 69: "Show a garib group",
    70: "Zone camera: raise or lower", 71: "Zone camera: distance", 72: "Zone camera: turn to the focus",
    73: "Zone camera: fly towards a point", 74: "Zone camera: look at Glover", 75: "Zone camera (retail 0x4b)",
    76: "Zone camera (retail 0x4c)", 77: "Zone camera: fly to a point near Glover", 78: "Change the water height",
    79: "Earthquake", 80: "Set the gravity", 81: "Start a cameo", 82: "Set a path platform's direction",
    83: "Make the ball usable", 84: "Play or stop a sound", 85: "Change the backdrop", 86: "Change the fog", 87: "Score",
}
CAMERA = set(range(62, 78)) | {84, 85, 86}
# Fields the game reads as a float but libgarib names u4: their XML holds the float's bits (1.0 = 0x3F800000)
FLOAT_BITS = {"hide_enabled", "physics_enabled", "point_idx", "axis_idx", "instruction_idx", "enabled", "direction"}
# Fields that name a platform by its PlatSetTag, in conditions and actions
PLAT_TAG = {"plat_tag", "platform_tag"}
PLAT_TAG_ACTIONS = {53, 61, 82}             # these call the field puzzle_tag but mean a platform
ENEMY_TAG = {33, 60, 67}                    # puzzle_tag is an Enemy's puzzle_tag
DELAY_MAX = 32767                           # activation_delay is an s16 in the game


def cases(fmt_, tag):
    return {int(k): v for k, v in fmt_.decomp["subSwitches"][tag]["cases"].items()}


def body_tag(fmt_, kind, code):
    """The body element a cond_type or action_type takes (the Default body for codes libgarib doesn't name)"""
    return cases(fmt_, kind).get(code, kind + "Default")


def float_from_bits(s):
    try:
        return struct.unpack(">f", struct.pack(">I", int(s, 0) & 0xFFFFFFFF))[0]
    except (TypeError, ValueError):
        return 0.0


def float_bits(v):
    return "0x%08X" % struct.unpack(">I", struct.pack(">f", float(v)))[0]


def make_cond(fmt_, code, attrs=None):
    c = make_el("PuzzleCond", {"cond_type": str(code)})
    b = fmt_.make(body_tag(fmt_, "PuzzleCond", code), attrs)
    if code in (35, 37, 39):
        for k in ("l", "w", "h"):
            b.set(k, "200.0")
    elif code in (36, 38, 40, 41):
        b.set("range", "100.0")
        if code == 41:
            b.set("max_y", "100.0")
    elif code in range(15, 27) or code == 33:
        for k in ("invert_result", "started_or_stopped"):
            if b.get(k) is not None:
                b.set(k, "1")
    c.append(b)
    return c


def make_action(fmt_, code, attrs=None):
    a = make_el("PuzzleAction", {"action_type": str(code)})
    b = fmt_.make(body_tag(fmt_, "PuzzleAction", code), attrs)
    for k in FLOAT_BITS:
        if b.get(k) is not None:
            b.set(k, float_bits(1.0))
    if code == 45:
        b.set("velocity", "-1.0")   # keep the platform's own speed (PlatMaxVelocity)
        b.set("flags", "4")
    a.append(b)
    return a


def rank(node):
    """A puzzle's children go: PuzzleNumtimes, the logic (And/Or/Any), the conditions, the actions"""
    return {"PuzzleNumtimes": 0, "PuzzleAnd": 1, "PuzzleOr": 1, "PuzzleAny": 1, "PuzzleCond": 2}.get(node.tag, 3)


def insert(puzzle, child):
    ch = kids(puzzle)
    i = next((i for i, c in enumerate(ch) if rank(c) > rank(child)), len(ch))
    if i >= len(ch):
        puzzle.append(child)
    else:
        ch[i].addprevious(child)
    child.tail = None
    return child


def code(node, attr):
    try:
        return int(node.get(attr) or "0", 0)
    except ValueError:
        return -1


def body(node):
    k = kids(node)
    return k[0] if k else None


def fires(puzzle):
    """None (every frame while true) or the number of firings"""
    n = next((c for c in kids(puzzle, "PuzzleNumtimes")), None)
    return None if n is None else inum(n, "n")


def logic(puzzle):
    """("all"|"any"|"least", n): the last of And/Or/Any wins (they all set requiredCount)"""
    out = ("all", 0)
    for c in kids(puzzle):
        if c.tag == "PuzzleOr":
            out = ("any", 1)
        elif c.tag == "PuzzleAny":
            n = inum(c, "op")
            out = ("all", 0) if n == 0 else ("least", n)
        elif c.tag == "PuzzleAnd":
            out = ("all", 0)
    return out


def summary(puzzle):
    conds = [CONDS.get(code(c, "cond_type"), "condition %s" % c.get("cond_type")) for c in kids(puzzle, "PuzzleCond")]
    acts = [ACTIONS.get(code(a, "action_type"), "action %s" % a.get("action_type")) for a in kids(puzzle, "PuzzleAction")]
    how, n = logic(puzzle)
    join = {"all": " and ", "any": " or ", "least": " / "}[how]
    low = lambda t: t if t.startswith("Glover") else t[:1].lower() + t[1:]
    when = "At level start" if not conds else ("At least %d of: " % n if how == "least" else "When ") + join.join(low(c) for c in conds)
    f = fires(puzzle)
    times = "every frame while true" if f is None else "once" if f == 1 else "%d times" % f
    return "%s → %s (%s)" % (when, "; ".join(low(a) for a in acts) or "nothing", times)


# What a condition or action reads through a pointer of the platforms with its tag (puzzle.c, no NULL check):
# their path, their motion (a spin) or their orbit. (children that make it, what to tell the author)
PATH = ({"PlatPathPoint"}, "a path")
SPIN = ({"PlatConstantSpin", "PlatSpin0x80", "PlatTopple0x81", "Plat0x9f", "PlatRocking", "PlatTurnTowardsPathPoint",
         "PlatGoForwards0x5f", "LookAtHand0x60", "LookAtBall0x61", "PlatCat0x69", "PlatCopySpinFromParent"}, "a spin")
ORBIT = ({"PlatOrbitAroundPoint"}, "an orbit")
COND_PARTS = {9: PATH, 12: PATH, 10: SPIN, 13: SPIN, 11: ORBIT, 14: ORBIT}
ACTION_PARTS = {48: PATH, 82: PATH, 46: SPIN, 51: SPIN, 47: ORBIT}


def rules(root, fmt_):
    """The puzzle checks: [(node, key, message, level)] with level err or warn"""
    out = []
    plat_tags = {num(t, "tag") for t in root.iter("PlatSetTag")}
    enemy_tags = {num(e, "puzzle_tag") for e in kids(root, "Enemy")}
    by_tag = {}
    for p in root.iter("Platform", "NullPlatform"):
        for t in kids(p, "PlatSetTag"):
            by_tag.setdefault(num(t, "tag"), []).append({c.tag for c in kids(p)})
    normal = {}
    for e in kids(root, "Enemy"):
        normal.setdefault(num(e, "puzzle_tag"), []).append(sum(len(kids(g)) for g in kids(e, "NormalInstructions")))

    def lacks(tag, part):
        """The platforms with this tag that don't have the part"""
        return [s for s in by_tag.get(tag, []) if not part[0] & s]
    cc, ac = cases(fmt_, "PuzzleCond"), cases(fmt_, "PuzzleAction")
    for c in kids(root):
        if c.tag.startswith("Puzzle") and c.tag != "Puzzle":
            out.append((c, "stray:%s" % c.tag, "%s outside a puzzle: the game adds it to the previous level's last puzzle, which is gone." % c.tag, "err"))
    for i, p in enumerate(kids(root, "Puzzle")):
        key = "puzzle %d" % (i + 1)
        conds, acts = kids(p, "PuzzleCond"), kids(p, "PuzzleAction")
        how, n = logic(p)
        if fires(p) == 0:
            out.append((p, key + ":zero", "It can fire 0 times, so it never runs.", "warn"))
        if how == "any" and not conds or how == "least" and n > len(conds):
            out.append((p, key + ":never", "Needs %d of its %d conditions, so it never fires." % (max(n, 1), len(conds)), "err"))
        for c in conds:
            t, b = code(c, "cond_type"), body(c)
            want = cc.get(t, "PuzzleCondDefault")
            if b is None or b.tag != want:
                out.append((p, key + ":cbody:%d" % conds.index(c), "Condition %d expects a <%s> body, not <%s>. The game would read the wrong number of bytes." % (
                    t, want, b.tag if b is not None else "nothing"), "err"))
                continue
            for f in ("plat_tag", "puzzle_tag", "reg_a"):
                if b.get(f) is not None and num(b, f) > 255:
                    out.append((p, key + ":c255", "%s: the tag %g is over 255. The game keeps conditions' tags in one byte." % (CONDS.get(t, t), num(b, f)), "err"))
            part = COND_PARTS.get(t)
            if part and b.get("plat_tag") is not None and lacks(num(b, "plat_tag"), part):
                out.append((p, key + ":cpart:%g" % num(b, "plat_tag"), "%s: a platform with the tag %g has no %s; the game reads it through an empty pointer." % (
                    CONDS.get(t, t), num(b, "plat_tag"), part[1][part[1].index(" ") + 1:]), "err"))
            if b.get("plat_tag") is not None and num(b, "plat_tag") not in plat_tags:
                out.append((p, key + ":cplat:%g" % num(b, "plat_tag"), "%s: no platform has the tag %g, and the game then takes the condition as true." % (
                    CONDS.get(t, t), num(b, "plat_tag")), "warn"))
        for a in acts:
            t, b = code(a, "action_type"), body(a)
            want = ac.get(t, "PuzzleActionDefault")
            if b is None or b.tag != want:
                out.append((p, key + ":abody:%d" % acts.index(a), "Action %d expects a <%s> body, not <%s>. The game would read the wrong number of bytes." % (
                    t, want, b.tag if b is not None else "nothing"), "err"))
                continue
            if num(b, "activation_delay") > DELAY_MAX:
                out.append((p, key + ":delay", "A wait of %g frames: over %d it never runs." % (num(b, "activation_delay"), DELAY_MAX), "err"))
            tag = b.get("platform_tag") if b.get("platform_tag") is not None else b.get("puzzle_tag") if t in PLAT_TAG_ACTIONS else None
            if tag is not None and num(b, "platform_tag" if b.get("platform_tag") is not None else "puzzle_tag") not in plat_tags \
                    and num(b, "platform_tag" if b.get("platform_tag") is not None else "puzzle_tag") > 0:
                out.append((p, key + ":aplat:%s" % tag, "%s: no platform has the tag %s." % (ACTIONS.get(t, t), tag), "warn"))
            if t == 45 and b.get("platform_tag") is not None and num(b, "platform_tag") < 0 and -num(b, "platform_tag") not in plat_tags:
                out.append((p, key + ":aplatneg", "Start a path platform with the tag %g: the game moves the first platform with the tag %g, and crashes when there is none." % (
                    num(b, "platform_tag"), -num(b, "platform_tag")), "err"))
            part = ACTION_PARTS.get(t)
            ptag = num(b, "platform_tag") if b.get("platform_tag") is not None else num(b, "puzzle_tag") if t in PLAT_TAG_ACTIONS else None
            if part and ptag is not None and lacks(ptag, part):
                out.append((p, key + ":apart:%g" % ptag, "%s: a platform with the tag %g has no %s; the game writes through an empty pointer." % (
                    ACTIONS.get(t, t), ptag, part[1][part[1].index(" ") + 1:]), "err"))
            if t == 67 and b.get("instruction_idx") is not None:
                idx = float_from_bits(b.get("instruction_idx"))
                if any(not 0 <= int(idx) < n for n in normal.get(num(b, "puzzle_tag"), [])):
                    out.append((p, key + ":a67", "Jump an enemy's script to #%d: an enemy with the puzzle tag %g has no such normal instruction." % (
                        int(idx), num(b, "puzzle_tag")), "err"))
            if t in (60, 67) and num(b, "puzzle_tag") not in enemy_tags:
                out.append((p, key + ":aenemy", "%s: no enemy has the puzzle tag %g." % (ACTIONS.get(t, t), num(b, "puzzle_tag")), "warn"))
        held = conds and all(code(c, "cond_type") not in EDGES and not (code(c, "cond_type") == 12 and num(body(c), "arg2") >= 0)
                             for c in conds if body(c) is not None)
        if fires(p) is None and held and any(code(a, "action_type") not in CAMERA for a in acts):
            out.append((p, key + ":repeat", "It has no firing limit and its conditions can stay true, so its actions run again every frame they hold. Set Fires to Once if they should run once.", "warn"))
    return out
