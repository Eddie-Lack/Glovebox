"""Enemy scripts: an enemy's normal, conditional and attack instructions (gloveworld src/nmeprocess.c, src/enemy.c).

The game keeps one flat list per kind: 30 normal, 10 conditional and 30 attack instructions. libgarib groups
consecutive instructions of a kind into <NormalInstructions> and the like, and a level may hold several groups of a
kind. Indices (a Goto's instr_idx, a Loop's target) count the instructions of a kind across all its groups, from 0.

An enemy runs its normal list from the start. An instruction ends when its lifetime runs out (lifetime 0 or -1: no
limit) or when its action is done, and the next one runs. The game reads past the list's end when the last one
ends, so a list must end with a Goto, an End attack, or an instruction that never ends.
"""
from .xmlutil import kids, make_el, num, inum

GROUPS = ("NormalInstructions", "ConditionalInstructions", "AttackInstructions")
LIMITS = {"NormalInstructions": 30, "ConditionalInstructions": 10, "AttackInstructions": 30}
KIND_NAMES = {"NormalInstructions": "Normal", "ConditionalInstructions": "Conditional", "AttackInstructions": "Attack"}

# instr_type: plain name. The game's name (NmeInstruction) in brackets where it says more.
TYPES = {
    0: "Move to a point", 1: "Walk to a point", 2: "Turn to a point", 3: "Wander in a box", 4: "Rest",
    5: "Shoot (1)", 6: "Shoot (2)", 7: "Play an animation", 8: "Face the player", 9: "Follow the player",
    10: "Grab the player", 11: "Throw the player", 12: "Flee the player", 13: "Circle (steer)", 14: "Attack cooldown",
    15: "Start the attack list", 16: "End the attack", 17: "Charge", 18: "Go to instruction", 19: "Do nothing",
    20: "Loop back", 21: "Set the loop counter", 22: "Move along a vector", 23: "Jump", 24: "Die",
}
TARGETS = {0x40: "the glove", 0x80: "the ball", 0x100: "the closer of the two"}
NEEDS_TARGET = {8, 9, 12, 17}      # these read the target the flags pick (enemy.c: no target, a garbage pointer)
NEVER_ENDS = {3, 12, 13, 17, 22}   # these end only by their lifetime (Wander, Flee, Circle, Charge, Move along a vector)
ENDS_BY_ANIM = {4, 5, 7, 8}        # with lifetime 0 these never end; with -1 they end with their animation
JUMPS = {18, 20}                   # a Goto or a Loop names an index in its list


def code(node):
    try:
        return int(node.get("instr_type") or "0", 0)
    except ValueError:
        return -1


def body(node):
    k = kids(node)
    return k[0] if k else None


def flags_value(s, enum):
    """flags as libgarib writes them: a number (decimal or hex), a name, or names joined by |"""
    s = (s or "0").strip()
    try:
        return int(s, 0)
    except ValueError:
        pass
    names = {v: int(k) for k, v in (enum or {}).items()}
    out = 0
    for part in s.split("|"):
        out |= names.get(part.strip(), 0)
    return out


def lists(enemy):
    """{group tag: [instructions in game order]}"""
    return {g: [i for grp in kids(enemy, g) for i in kids(grp)] for g in GROUPS}


def make_instruction(fmt_, t, attrs=None, body_attrs=None):
    a = {"instr_type": str(t), "lifetime": "0", "execution_condition_param_a": "0.0", "execution_condition_param_b": "0.0",
         "flags": "0", "execution_condition": "always"}
    a.update(attrs or {})
    n = make_el("EnemyInstruction", a)
    tag = fmt_.decomp["subSwitches"]["EnemyInstruction"]["cases"].get(str(t))
    b = fmt_.make(tag, body_attrs)
    if t in (0, 1, 9, 22) and b.get("vel_magnitude") is not None and "vel_magnitude" not in (body_attrs or {}):
        b.set("vel_magnitude", "3.0")
    if t == 4 and "anim_start_playing" not in (body_attrs or {}):
        b.set("anim_start_playing", "1")
    if t in NEEDS_TARGET and "flags" not in (attrs or {}):
        n.set("flags", str(0x41))     # the glove, turning as it moves
    n.append(b)
    return n


def group_for(enemy, kind, create=True):
    """The last group of a kind, or a new one in the order Normal, Conditional, Attack, before EnemyFinalize"""
    have = kids(enemy, kind)
    if have:
        return have[-1]
    if not create:
        return None
    g = make_el(kind)
    later = [c for c in kids(enemy) if c.tag in GROUPS[GROUPS.index(kind) + 1:] or c.tag == "EnemyFinalize"]
    if later:
        later[0].addprevious(g)
    else:
        enemy.append(g)
    g.tail = None
    return g


def retarget(enemy, kind, fix):
    """Rewrites every Goto and Loop index into this kind's list with fix(old index) (None: leave it). A conditional
    instruction's Goto jumps in the normal list."""
    if kind == "ConditionalInstructions":
        return
    ls = lists(enemy)
    for ins in ls[kind] + (ls["ConditionalInstructions"] if kind == "NormalInstructions" else []):
        b = body(ins)
        if b is None:
            continue
        if code(ins) == 18 and b.get("instr_idx") is not None:
            new = fix(inum(b, "instr_idx"))
            if new is not None:
                b.set("instr_idx", str(new))
        elif code(ins) == 20 and b.get("u32_0x02") is not None:
            new = fix(inum(b, "u32_0x02"))
            if new is not None:
                b.set("u32_0x02", str(new))


def retarget_puzzles(enemy, kind, fix):
    """Puzzle action 67 (Jump an enemy's script) names an index in the normal list of every enemy with its puzzle
    tag. Rewrites it with fix(old index) when this enemy is the only one with the tag."""
    from .puzzles import float_bits, float_from_bits
    root = enemy.getroottree().getroot()
    tag = num(enemy, "puzzle_tag")
    if kind != "NormalInstructions" or sum(1 for e in kids(root, "Enemy") if num(e, "puzzle_tag") == tag) != 1:
        return
    for a in root.iter("PuzzleAction"):
        b = body(a)
        if inum(a, "action_type") == 67 and b is not None and num(b, "puzzle_tag") == tag and b.get("instruction_idx") is not None:
            new = fix(int(float_from_bits(b.get("instruction_idx"))))
            if new is not None:
                b.set("instruction_idx", float_bits(new))


def templates(fmt_, enemy):
    """Ready-made normal scripts. Points are around the enemy."""
    x, y, z = num(enemy, "x"), num(enemy, "y"), num(enemy, "z")
    p = lambda dx, dz: {"destination_x": repr(x + dx), "destination_y": repr(y), "destination_z": repr(z + dz)}
    return {
        "Chase the glove": [
            make_instruction(fmt_, 4, {"lifetime": "10"}),
            make_instruction(fmt_, 9, {"flags": "65"}),
            make_instruction(fmt_, 18, {"lifetime": "-1"}, {"instr_idx": "1"})],
        "Wander in a box": [
            make_instruction(fmt_, 3, {"lifetime": "0"}, {"home_x": repr(x - 150), "home_y": repr(y), "home_z": repr(z - 150),
                                                          "extent_x": "300.0", "extent_y": "0.0", "extent_z": "300.0",
                                                          # an int the game reads through a float field: 60 as float bits
                                                          "min_travel_distance": "8.407790785948902e-44"})],
        "Patrol between two points": [
            make_instruction(fmt_, 1, {}, p(200, 0)), make_instruction(fmt_, 4, {"lifetime": "20"}),
            make_instruction(fmt_, 1, {}, p(-200, 0)), make_instruction(fmt_, 4, {"lifetime": "20"}),
            make_instruction(fmt_, 18, {"lifetime": "-1"}, {"instr_idx": "0"})],
        "Stand still": [make_instruction(fmt_, 4, {"lifetime": "0"})],
    }


def rules(enemy, enum_flags):
    """The script checks for one enemy: [(key, message, level)]"""
    out = []
    ls = lists(enemy)
    for kind, items in ls.items():
        name = KIND_NAMES[kind].lower()
        for i, ins in enumerate(items):
            t, b = code(ins), body(ins)
            label = "%s #%d (%s)" % (KIND_NAMES[kind], i, TYPES.get(t, t))
            if t not in TYPES:
                out.append(("type:%s:%d" % (kind, i), "%s: instruction type %s doesn't exist; the game reads the rest of the level out of step." % (label, t), "err"))
                continue
            if t in JUMPS and b is not None:
                field = "instr_idx" if t == 18 else "u32_0x02"
                target = inum(b, field)
                if b.get(field) is not None and not 0 <= target < len(ls["NormalInstructions" if kind == "ConditionalInstructions" else kind]):
                    out.append(("jump:%s:%d" % (kind, i), "%s goes to #%d, which the list doesn't have." % (label, target), "err"))
            if t in NEEDS_TARGET and not flags_value(ins.get("flags"), enum_flags) & 0x1C0:
                out.append(("target:%s:%d" % (kind, i), "%s has no target. Pick the glove, the ball or the closer one in its flags (0x40, 0x80, 0x100)." % label, "err"))
            if ins.get("execution_condition") in ("periodic", "9") and num(ins, "execution_condition_param_a") == 0:
                out.append(("periodic:%s:%d" % (kind, i), "%s: 'periodic' every 0 frames divides by zero." % label, "err"))
            if t == 15 and not ls["AttackInstructions"]:
                out.append(("attack:%d" % i, "%s starts the attack list, which is empty." % label, "warn"))
        if items and kind != "ConditionalInstructions":
            last = items[-1]
            t, life = code(last), num(last, "lifetime")
            ends = not (t in (18, 16) or t in NEVER_ENDS and life <= 0 or t in ENDS_BY_ANIM and life == 0 or t == 19 and life <= 0)
            if ends:
                out.append(("end:%s" % kind, "The %s list's last instruction (%s) can end, and the game then reads past the list. End it with Go to instruction%s." % (
                    name, TYPES.get(t, t), " or End the attack" if kind == "AttackInstructions" else ""), "err"))
    return out
