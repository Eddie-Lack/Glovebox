#!/usr/bin/env python3
"""Add the Expansion Pak game mod (mod/mod.json and mod/mod.bin, from tools/build_mod.py) to a built ROM.

    python3 tools/mod_patch.py IN.z64 OUT.z64

It checks every word it replaces, writes the loader and the hooks into the game's code, stores the mod at
MOD_ROM (12 MiB, past anything libgarib writes), and recomputes the header checksum (the code is inside the range
the boot code checks). The mod needs the Expansion Pak; without one the loader undoes the hooks.

`custom` (apply_mod) stores generated models behind the mod and fills the mod's table of them (mod/banks.c).

`extra_banks` lists, per level, the game's texture and object bank numbers the mod loads from the ROM before
the level is parsed (mod/banks.c): {level: (texture_banks, object_banks)}.
"""
import hashlib, io, json, os, struct, sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
MOD_JSON = os.path.join(ROOT, "mod", "mod.json")
MOD_BIN = os.path.join(ROOT, "mod", "mod.bin")
sys.path.insert(0, os.path.join(ROOT, "vendor", "libgarib", "python"))
sys.path.insert(0, HERE)


class ModError(Exception):
    pass


def extra_banks_table(extra_banks, size):
    """modExtraBanks (mod/banks.c): per level, the level number, the counts, then the bank numbers; 0xFF ends it.
    A value may be (tex, obj, True): bit 7 of the level number is then set and the extra banks win on a texture id that
    is also in the level's own banks."""
    table = b""
    for level, value in sorted(extra_banks.items()):
        tex, obj = value[0], value[1]
        first = len(value) > 2 and value[2]
        if not (0 <= level < 0x7F) or len(tex) > 8 or len(obj) > 16 or not all(0 <= n < 0xFF for n in list(tex) + list(obj)):
            raise ModError("level %s: too many extra banks (8 texture and 16 object banks at most)" % level)
        table += bytes([level | (0x80 if first else 0), len(tex), len(obj)] + list(tex) + list(obj))
    if len(table) + 1 > size:
        raise ModError("the extra bank list is %d bytes; the mod has room for %d" % (len(table) + 1, size))
    return table + b"\xff"


STORE = 0x20000             # what the mod stores behind itself starts this far past MOD_ROM (the mod's code is under 128 KiB)


# Ball walking (ProcessInput, gloveworld src/controller.c): on Medium and Hard the stick pushes the ball the other way
# from Easy. This `bnez` skips Easy's branch when the save slot's difficulty is not Easy; a nop in its place gives
# every difficulty Easy's direction. Nothing else about the difficulty changes (lives, garibs per life, bosses).
EASY_CONTROLS = {"rom": 0x3A6D0, "ram": "0x801396D0", "old": "14600015", "new": "00000000"}


# levelInfo (gloveworld include/game.h): 48 rows of 0x38 bytes in the game's uncompressed data, at ROM 0xE7910.
# A row holds the name (0x28 bytes, shown in the level select), objBank, bit, sfxDist, a byte, the song (+0x35)
# and the bonus clock in seconds (+0x36, -1 for none).
LEVEL_INFO = 0xE7910
LEVEL_ROW = 0x38


def apply_level_info(rom, rows):
    """The ROM with these levelInfo rows changed. rows is {slot: {"name": str or None, "song": int or None,
    "bonus_time": int or None}}; None keeps the shipped value. Refuses a ROM whose table isn't where it should be."""
    out = bytearray(rom)
    row = lambda i: out[LEVEL_INFO + LEVEL_ROW * i:LEVEL_INFO + LEVEL_ROW * (i + 1)]
    if row(0)[:6] != b"HUB 1\0" or row(41)[:12] != b"SPACE BONUS\0":
        raise ModError("the ROM's level table isn't where the retail Glover (USA) ROM has it")
    for slot, info in rows.items():
        if not 0 <= slot < 48:
            raise ModError("no level slot %d" % slot)
        at = LEVEL_INFO + LEVEL_ROW * slot
        name = info.get("name")
        if name is not None:
            try:
                b = name.encode("ascii")
            except UnicodeEncodeError:
                raise ModError("level name %r isn't plain ASCII" % name)
            if len(b) > 0x27:
                raise ModError("a level name is %d characters; 39 fit" % len(b))
            out[at:at + 0x28] = b + bytes(0x28 - len(b))
        if info.get("song") is not None:
            out[at + 0x35] = int(info["song"]) & 0xFF
        if info.get("bonus_time") is not None:
            out[at + 0x36:at + 0x38] = struct.pack(">h", int(info["bonus_time"]))
    return bytes(out)


def apply_easy_controls(rom):
    """The ROM with Easy's ball-walking controls on every difficulty, and its header checksum recomputed."""
    from libgarib.rom import calculateChecksum

    out = bytearray(rom)
    p = EASY_CONTROLS
    if out[p["rom"]:p["rom"] + 4].hex() != p["old"]:
        raise ModError("the ROM's code at %s isn't the difficulty check the controls patch expects" % p["ram"])
    out[p["rom"]:p["rom"] + 4] = bytes.fromhex(p["new"])
    crc = calculateChecksum(io.BytesIO(bytes(out)), True)
    out[0x10:0x18] = crc[0] + crc[1]
    return bytes(out)


def custom_table(entries, base, size):
    """modCustom (mod/banks.c): the magic, the count, then per entry the key and the cart addresses of a texture bank and an object bank.
    `entries` is [(key, tex_bytes, obj_bytes)]. Returns the table and the bytes to store at `base`.
    Each bank starts on a 16 byte boundary and its end address is rounded up to 16, so every DMA the game makes for it has an even
    length that is a multiple of 8 (the PI's rules, docs/technical.md, "Real hardware"). The padding is zeros."""
    if len(entries) > 8:
        raise ModError("more than 8 custom bank sets")
    out, store = struct.pack(">II", 0x43555331, len(entries)), b""
    for key, tex, obj in entries:
        ts, os_ = base + len(store), 0
        tex_len = (len(tex) + 15) & ~15
        store += tex + bytes(tex_len - len(tex))
        os_ = base + len(store)
        obj_len = (len(obj) + 15) & ~15
        store += obj + bytes(obj_len - len(obj))
        out += struct.pack(">B3xIIII", key, ts if tex else 0, ts + tex_len if tex else 0, os_ if obj else 0, os_ + obj_len if obj else 0)
    out += bytes(size - len(out))
    return out, store


# Extra level slots (mod/slots.c): levels stored behind the mod in the Space slots 35 to 41
SLOTS_FIRST, SLOTS_COUNT = 35, 7
SLOT_KEY = 0x40              # modExtraBanks and modCustom entries of an extra level are keyed with its slot + SLOT_KEY
NAME_LEN = 0x28
MAX_LEVEL_FILE = 0x40000     # Mod_LoadLandscapeFile reads at most 256 KiB
ANY_HUB = 0xFF


def slots_table(levels, offsets, boot=False, entry_hub=ANY_HUB, entry_tag=99):
    """modSlots (mod/slots.c): the magic 'SLT1', the boot flag, the entry hub (0 to 7, or 0xFF for any hub), the entry
    platform tag, then per slot 35 to 41 the cart address of its file, the bonus time (0x7FFF keeps the retail one), the
    song (0xFF keeps it) and the name."""
    out = struct.pack(">IBBH", 0x534C5431, 1 if boot else 0, entry_hub, entry_tag)
    for i in range(SLOTS_COUNT):
        slot = SLOTS_FIRST + i
        d = levels.get(slot)
        if d is None:
            out += bytes(8 + NAME_LEN)
            continue
        try:
            name = (d.get("name") or "").encode("ascii")
        except UnicodeEncodeError:
            raise ModError("level name %r isn't plain ASCII" % d["name"])
        if len(name) >= NAME_LEN:
            raise ModError("level name %r is too long" % d["name"])
        bonus, song = d.get("bonus_time"), d.get("song")
        out += struct.pack(">IhBx", offsets[slot], 0x7FFF if bonus is None else bonus, 0xFF if song is None else song)
        out += name + bytes(NAME_LEN - len(name))
    return out


def apply_mod(rom, extra_banks=None, custom=None, slots=None):
    """`custom` is {key: (texture bank bytes, object bank bytes)}: generated models the mod loads for that key (the key of extra_banks).
    `slots` is {"levels": {slot: {"lev", "name", "song", "bonus_time"}}, "boot", "entry_hub", "entry_tag"}: extra levels stored
    behind the mod (mod/slots.c)."""
    from libgarib.rom import calculateChecksum

    info = json.load(open(MOD_JSON))
    blob = open(MOD_BIN, "rb").read()
    if hashlib.sha1(blob).hexdigest() != info["mod_sha1"]:
        raise ModError("mod/mod.bin doesn't match mod/mod.json; run tools/build_mod.py")
    if extra_banks:
        t = info["extra_banks"]
        table = extra_banks_table(extra_banks, t["size"])
        blob = blob[:t["offset"]] + table + blob[t["offset"] + len(table):]
    store = b""
    if slots and slots.get("levels"):
        t = info.get("slots")
        if not t or blob[t["offset"]:t["offset"] + 4] != b"SLT1":
            raise ModError("mod/mod.bin has no extra slot table; run tools/build_mod.py")
        offsets, base = {}, int(info["mod_rom"], 16) + STORE
        for slot, d in sorted(slots["levels"].items()):
            if not SLOTS_FIRST <= slot < SLOTS_FIRST + SLOTS_COUNT:
                raise ModError("extra level slot %s is outside 35 to 41" % slot)
            lev = d["lev"]
            if struct.unpack(">I", lev[:4])[0] != len(lev) or len(lev) % 4:
                raise ModError("the level file of slot %s is not in the game's size-prefixed format" % slot)
            if len(lev) > MAX_LEVEL_FILE:
                raise ModError("the level file of slot %s is %d KB; the mod reads 256 KB at most" % (slot, len(lev) // 1024))
            offsets[slot] = base + len(store)
            store += lev + bytes(-len(lev) % 16)
        table = slots_table(slots["levels"], offsets, slots.get("boot"), slots.get("entry_hub", ANY_HUB), slots.get("entry_tag", 99))
        if len(table) != t["size"]:
            raise ModError("the extra slot table is %d bytes; the mod has %d" % (len(table), t["size"]))
        blob = blob[:t["offset"]] + table + blob[t["offset"] + len(table):]
    if custom:
        t = info["custom"]
        if blob[t["offset"]:t["offset"] + 4] != b"CUS1":
            raise ModError("mod/mod.bin has no custom model table; run tools/build_mod.py")
        base = int(info["mod_rom"], 16) + STORE + len(store)
        table, cstore = custom_table([(k, v[0], v[1]) for k, v in sorted(custom.items())], base, t["size"])
        blob = blob[:t["offset"]] + table + blob[t["offset"] + len(table):]
        store += cstore
    if len(blob) > STORE:
        raise ModError("the mod is %d bytes; what it stores starts at +0x%X" % (len(blob), STORE))
    out = bytearray(rom)
    for r in info["regions"]:
        off, new = int(r["rom"], 16), bytes.fromhex(r["new"])
        if hashlib.sha1(out[off:off + len(new)]).hexdigest() != r["old_sha1"]:
            raise ModError("the ROM's code at %s isn't what the mod expects (%s)" % (r["ram"], r["why"]))
        out[off:off + len(new)] = new
    for h in info["hooks"]:
        off = int(h["rom"], 16)
        if out[off:off + 4].hex() != h["old"]:
            raise ModError("the ROM's code at %s isn't what the mod expects (%s)" % (h["ram"], h["why"]))
        out[off:off + 4] = bytes.fromhex(h["new"])
    base = int(info["mod_rom"], 16)
    if len(out) > base:
        raise ModError("the built ROM is %d bytes, past where the mod goes (0x%X)" % (len(out), base))
    out += bytes(base - len(out)) + blob
    if store:
        out += bytes(STORE - len(blob)) + store
    out += bytes(-len(out) % 0x100000)  # whole MiB
    crc = calculateChecksum(io.BytesIO(bytes(out)), True)
    out[0x10:0x18] = crc[0] + crc[1]
    import hw_check
    problems = hw_check.check_rom(bytes(out), info)
    if problems:
        raise ModError("the ROM breaks a rule of real N64 hardware (tools/hw_check.py): " + "; ".join(problems))
    return bytes(out)


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    data = apply_mod(open(sys.argv[1], "rb").read())
    open(sys.argv[2], "wb").write(data)
    print("%s: %d bytes" % (sys.argv[2], len(data)))
