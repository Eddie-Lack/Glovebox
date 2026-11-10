"""Platform presets: common platforms (rings, exits, lifts, spikes...) as one click each.

Each preset makes whole elements with the children in the order the game reads them (desktop/commands.py). The
values were tested in the emulator and on a console. Platforms
use the game's own models: a platform with an imported custom model makes a console trap (entities.WHY).
"""
import math, struct
from .xmlutil import fmt, make_el

HOOP = ("0x70210547", "hoop.nd")     # GENERIC, the checkpoint ring
HEART = ("0x7FDADB91", "heart")      # GENERIC, the exit marker of the Space levels
BALLBIT = ("0x2859DA5E", "ballbit")  # GENERIC, radius 43.8 at scale 1
LIFT = ("0x090BCA4B", "lift")        # GENERIC, 48 x 48, collision top 6 above its origin, 21 thick
GLOVEBU = ("0x11C4C2A2", "glovebu")  # GENERIC, the slam button, 40 x 40
STEPY = ("0xF382F9F3", "stepy.n")    # OOTW_L2A, 210 x 210, collision top 46 above its origin
TELEPAD = ("0xA8B34B43", "ootelep")  # OOTW_L3, the teleport pad
RING_START, RING_MID = 40.0, 26.0    # height over the floor: at 12 the glove restarted inside the floor
DECK = (4.0, 1.0, 4.0)               # the lift scaled into a 192 x 192 deck


def el(_tag, **kw):
    return make_el(_tag, {k: fmt(v) if isinstance(v, float) else str(v) for k, v in kw.items()})


def platform(model, p, scale=None, place=True):
    n = el("Platform", object_id=model[0], name=model[1][:7])
    if scale and tuple(scale) != (1.0, 1.0, 1.0):
        n.append(el("PlatScale", x=float(scale[0]), y=float(scale[1]), z=float(scale[2])))
    if place:
        n.append(el("PlatSetInitialPos", x=float(p[0]), y=float(p[1]), z=float(p[2])))
    return n


def float_bits(v):
    """A float stored in a u32 field (the teleport's place), as libgarib writes it"""
    return "0x%08X" % struct.unpack(">I", struct.pack(">f", float(v)))[0]


def ring(p, facing, height):
    n = platform(HOOP, (p[0], p[1] + height, p[2]))
    # theta is Glover's facing at a restart and the start camera's turn. The spin's speed 0.05 matters: the game
    # reads the spin when the ring is touched, and a ring with no spin crashes.
    n.append(el("PlatCheckpoint", u16_0x17=0, theta=float(facing)))
    n.append(el("PlatConstantSpin", axis="y", initial_theta=0.0, speed=0.05))
    n.append(el("PlatNoClip"))
    return n


def heart(p, facing, behavior, scale=None):
    n = platform(HEART, p, scale)
    # A spin of speed 0 only turns the heart; the heart is updated every frame, so it is safe
    n.append(el("PlatConstantSpin", axis="y", initial_theta=round(facing + math.pi / 2, 4), speed=0.0))
    n.append(el("SetExit", behavior=behavior, type="solid_platform"))
    return n


def held_path(model, scale, points, speed, accel, tag, sound=None):
    """A platform held at its first path point until a puzzle nudges its tag (duration -100: held)"""
    n = platform(model, points[0], scale, place=False)
    for q in points:
        n.append(el("PlatPathPoint", duration=-100, x=float(q[0]), y=float(q[1]), z=float(q[2])))
    n.append(el("PlatMaxVelocity", velocity=float(speed)))
    n.append(el("PlatPathAcceleration", acceleration=float(accel)))
    if sound:
        n.append(el("PlatPathSoundAtPointHit", sound_id=sound, volume=100, pitch=128))
    n.append(el("PlatSetTag", tag=tag))
    return n


def release_puzzle(p, size, tag):
    """A puzzle that nudges a held platform once, when Glover walks into a box around p"""
    z = el("Puzzle")
    z.append(el("PuzzleNumtimes", n=1))
    c = el("PuzzleCond", cond_type=35)
    c.append(el("PuzzleCondGloverWithinVolume", x=float(p[0] - size / 2), y=float(p[1] - 20), z=float(p[2] - size / 2),
                l=float(size), w=float(size), h=float(size)))
    z.append(c)
    a = el("PuzzleAction", action_type=45)
    a.append(el("PuzzleActionPlatformNudge", velocity=10.0, platform_tag=tag, activation_delay=0, flags=4))
    z.append(a)
    return z


def ahead(p, facing, d):
    return (p[0] + math.sin(facing) * d, p[1], p[2] + math.cos(facing) * d)


# key: (name, what it does, the model it uses)
PRESETS = {
    "start": ("Start ring", "Glover starts here. Moves the level's first checkpoint ring if it has one", HOOP),
    "ring": ("Checkpoint ring", "Glover restarts here once he has touched it. A rolling ball may pass under it: it counts when the ball comes down through it", HOOP),
    "exit": ("Level exit (heart)", "Ends the level when Glover brings the ball to it", HEART),
    "bonus": ("Bonus finish line", "A row of five big hearts across the way: the end of a bonus level", HEART),
    "world": ("World exit lift", "Carries Glover and the ball up and out of the world when he stands on it holding the ball", STEPY),
    "lift": ("Lift", "Rises once when Glover walks onto it. A puzzle with a box around it nudges it", LIFT),
    "button": ("Slam button", "Goes down when Glover slams it with the ball. Use its tag in a puzzle condition", GLOVEBU),
    "mover": ("Moving platform", "Goes back and forth along two points. Uses the selected object's model, or a deck", LIFT),
    "spinner": ("Spinner", "Turns about its centre all the time. Uses the selected object's model, or a deck", LIFT),
    "conveyor": ("Conveyor", "Pushes what stands on it along the view's direction. 7.5 a frame stops a walking ball", LIFT),
    "ice": ("Ice floor", "The ball keeps its speed on it", LIFT),
    "spike": ("Spike", "An invisible point that hurts Glover and bursts the ball. Put any model on it as the visible part", BALLBIT),
    "zone": ("Exit zone", "An invisible exit: the level ends when the glove holding the ball reaches it", BALLBIT),
    "entry": ("Extra levels entry", "In a hub: the glove going in here leads to the way room of the extra levels (Expansion Pak). Without one, the Space world's own levels load", BALLBIT),
    "teleport": ("Teleport pad", "Sends the ball 300 ahead along the view's direction. Drag the place in the inspector", TELEPAD),
}


def build(key, p, facing, tag=None, model=None, ground=None):
    """The elements of a preset at ground point p, facing the view's direction. tag: a free platform tag. model:
    the selected object's (id, name), for the presets that take any model. ground(q) gives the ground height under
    a point. Returns the elements in level order."""
    deck = (model, None) if model else (LIFT, DECK)
    if key in ("start", "ring"):
        return [ring(p, facing, RING_START if key == "start" else RING_MID)]
    if key == "exit":
        return [heart(p, facing, 1)]
    if key == "bonus":
        out = []
        for k in range(-2, 3):
            q = (p[0] - k * 120 * math.cos(facing), p[1], p[2] + k * 120 * math.sin(facing))
            if ground:
                q = (q[0], ground(q), q[2])
            out.append(heart(q, facing, 5, (2.5, 2.5, 2.5)))
        return out
    if key == "world":
        # The lift waits at the floor; exit 3 starts its path when Glover stands on it with the ball. At speed 5
        # and acceleration 0.5 it stops exactly (at 10 and 1 it overshot by 190). Its top is 46 over its origin.
        low = (p[0], p[1] - 46.0 + 1, p[2])
        n = held_path(STEPY, (170 / 210, 1.0, 170 / 210), [low, (low[0], low[1] + 700, low[2])], 5.0, 0.5, tag)
        n.append(el("SetExit", behavior=3, type="solid_platform"))
        return [n]
    if key == "lift":
        # The lift's top is 6 x 1.2 over its origin
        low = (p[0], p[1] - 7.0, p[2])
        n = held_path(LIFT, (1.2, 1.2, 1.2), [low, (low[0], low[1] + 200, low[2])], 10.0, 0.5, tag, sound=210)
        return [n, release_puzzle(p, 60.0, tag)]
    if key == "button":
        # Duration -2 waits for a slam; it then sinks 6 and stays down until a puzzle sends it back
        top = (p[0], p[1] - 2.5, p[2])
        n = platform(GLOVEBU, top, (1.25, 1.25, 1.25), place=False)
        n.append(el("PlatPathPoint", duration=-2, x=float(top[0]), y=float(top[1]), z=float(top[2])))
        n.append(el("PlatPathPoint", duration=-100, x=float(top[0]), y=float(top[1] - 6), z=float(top[2])))
        n.append(el("PlatMaxVelocity", velocity=1.0))
        n.append(el("PlatPathAcceleration", acceleration=0.8))
        n.append(el("PlatPathSoundAtPointHit", sound_id=210, volume=100, pitch=128))
        n.append(el("PlatSetTag", tag=tag))
        return [n]
    if key == "mover":
        a, b = (p[0], p[1] + 30, p[2]), ahead((p[0], p[1] + 30, p[2]), facing, 300)
        n = platform(deck[0], a, deck[1])
        n.append(el("PlatMaxVelocity", velocity=3.0))
        n.append(el("PlatPathAcceleration", acceleration=0.2))
        for q in (a, b):
            n.append(el("PlatPathPoint", duration=60, x=float(q[0]), y=float(q[1]), z=float(q[2])))
        n.append(el("PlatReverseAtEndsOfPath"))
        return [n]
    if key == "spinner":
        n = platform(deck[0], (p[0], p[1] + 30, p[2]), (6.0, 0.5, 1.0) if not model else None)
        n.append(el("PlatConstantSpin", axis="y", initial_theta=0.0, speed=0.03))
        return [n]
    if key == "conveyor":
        n = platform(deck[0], (p[0], p[1] - 6, p[2]), deck[1])
        n.append(el("PlatformConveyor", vel_x=round(math.sin(facing) * 3, 4), vel_y=0.0, vel_z=round(math.cos(facing) * 3, 4)))
        return [n]
    if key == "ice":
        n = platform(deck[0], (p[0], p[1] - 6, p[2]), deck[1])
        n.append(el("PlatActorSurfaceType", value=2))
        return [n]
    if key == "spike":
        # A small invisible ballbit: the spike is a point with a radius of 20. SetExit 0 hides it, and it then takes
        # none of the frame's object matrices. No spin: a speed-0 spin on a platform that is never updated traps
        # a console.
        n = platform(BALLBIT, (p[0], p[1] + 18, p[2]), (0.2, 0.2, 0.2))
        n.append(el("PlatSpike"))
        n.append(el("SetExit", behavior=0, type="loading_zone"))
        return [n]
    if key == "zone":
        n = platform(BALLBIT, (p[0], p[1] + 30, p[2]))
        n.append(el("SetExit", behavior=1, type="loading_zone"))
        return [n]
    if key == "entry":
        # mod/slots.c: a SPACE_EXIT (11) loading zone with the entry tag sets the flag for the extra levels. The game
        # tests the glove's centre against the ballbit's radius, 43.8.
        n = platform(BALLBIT, (p[0], p[1] + 30, p[2]))
        n.append(el("SetExit", behavior=11, type="loading_zone"))
        n.append(el("PlatSetTag", tag=99))
        return [n]
    if key == "teleport":
        n = platform(TELEPAD, (p[0], p[1] - 13 * 0.4, p[2]), (0.4, 0.4, 0.4))
        to = ahead(p, facing, 300)
        if ground:
            to = (to[0], ground(to), to[2])
        # libgarib's names are shifted from the game's fields (parse.c TOKEN_TELEPORT): target_tag holds the flags
        # (1 the ball, 2 the glove without the ball, 0x10 stop on arrival), out_framecount the target platform's tag
        # (0: a fixed place), in_framecount and u16_0x12 the shrink and grow frames, u32_0x00..08 the place
        n.append(el("SetTeleport", target_tag=0x13, out_framecount=0, in_framecount=10, u16_0x12=10,
                    u32_0x00=float_bits(to[0]), u32_0x04=float_bits(to[1]), u32_0x08=float_bits(to[2])))
        return [n]
    raise KeyError(key)
