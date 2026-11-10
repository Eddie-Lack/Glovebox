#!/usr/bin/env python3
"""Checks a built ROM against the rules of real N64 hardware, which an emulator does not enforce.

    python3 tools/hw_check.py ROM.z64
    python3 tools/hw_check.py --self-test ROM.z64     (a ROM with the mod's custom banks: breaks a copy in each way and checks the breaks are found)

`check_rom` returns a list of problems (empty: all fine). tools/mod_patch.py runs it on every ROM it builds and refuses a ROM with a
problem. The rules (docs/technical.md, "Real hardware"):

- The header holds the CRCs of the first megabyte of code (CIC-NUS-6102 algorithm, written here again so it does not depend on
  libgarib). The boot code of a real console does not start a ROM whose CRCs are wrong.
- The ROM is a whole number of mebibytes, and everything the mod reads from it lies inside it.
- Every DMA the mod makes from the ROM starts at an even address (these start at a multiple of 16) and has a length that is a
  multiple of 16.
- A texture bank's records are each a multiple of 8 bytes long, so the pixels of every texture start on a 64 bit boundary (the RDP
  loads textures from there).
- The pointers inside an object bank are aligned for what they point at.
- Every call site the mod hooks is a `jal` to a word inside the mod.
- Every extra level stored behind the mod starts on a 16 byte boundary, has a sane size word and zero padding to 16.
"""
import hashlib, json, os, struct, sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
MOD_JSON = os.path.join(ROOT, "mod", "mod.json")
MAX_LEVEL_FILE = 0x40000        # mod/slots.c MAX_LEVEL_FILE
SLOT_LEVEL_SIZE = 8 + 0x28      # mod/slots.c SlotLevel


def header_crc(rom):
    """The two header CRCs of a ROM with the CIC-NUS-6102 boot code (Glover): over 0x1000 to 0x101000."""
    mask = 0xFFFFFFFF
    t1 = t2 = t3 = t4 = t5 = t6 = 0xF8CA4DDC
    for i in range(0x1000, 0x101000, 4):
        d = struct.unpack_from(">I", rom, i)[0]
        if ((t6 + d) & mask) < t6:
            t4 = (t4 + 1) & mask
        t6 = (t6 + d) & mask
        t3 ^= d
        n = d & 0x1F
        r = ((d << n) | (d >> (32 - n))) & mask if n else d
        t5 = (t5 + r) & mask
        if t2 > d:
            t2 ^= r
        else:
            t2 ^= t6 ^ d
        t1 = (t1 + (t5 ^ d)) & mask
    return t6 ^ t4 ^ t3, t5 ^ t2 ^ t1


def check_texture_bank(rom, start, end, label, problems):
    data = rom[start:end]
    if len(data) < 4:
        problems.append("%s: texture bank is empty" % label)
        return
    n = struct.unpack_from(">I", data, 0)[0]
    pos = 4
    for i in range(n):
        if pos + 0x24 > len(data):
            problems.append("%s: texture %d starts past the end of the bank" % (label, i))
            return
        size = struct.unpack_from(">I", data, pos + 20)[0]
        if size < 0x24 or size % 8:
            problems.append("%s: texture %d has size %d: not a multiple of 8, so the pixels of the next one are not on a 64 bit boundary"
                            % (label, i, size))
            return
        pos += size
    if pos > len(data):
        problems.append("%s: the textures run past the end of the bank" % label)


def check_object_bank(rom, start, end, label, problems):
    """The CPU reads an object bank after the mod has turned its offsets into pointers (mod/banks.c LoadCustomBanks, the game's
    RestoreObjectPointers): every word, float and halfword it reads must sit on its own alignment, or the console raises an address
    error. Walks the entries the way the loader does."""
    data = rom[start:end]
    seen = set()

    def u32(o):
        return struct.unpack_from(">I", data, o)[0] if 0 <= o <= len(data) - 4 else None

    def ptr(o, align, what):
        v = u32(o)
        if v is None:
            problems.append("%s: %s is read past the end of the bank" % (label, what))
            return None
        if not v:
            return 0
        off = v & 0xFFFFFF
        if off >= len(data) or off % align:
            problems.append("%s: %s points to offset 0x%X: %s" % (label, what, off, "outside the bank" if off >= len(data) else "not %d byte aligned" % align))
            return None
        return off

    def obj(o):
        if o in seen:
            return
        seen.add(o)
        for what, at, align in (("mesh", 0x14, 4), ("scaleKeys", 0x1C, 4), ("posKeys", 0x20, 4), ("rotKeys", 0x24, 4), ("sprites", 0x2C, 4),
                                ("sphere", 0x3C, 4)):
            ptr(o + at, align, "object field " + what)
        m = ptr(o + 0x14, 4, "mesh")
        if m:
            for what, at, align in (("verts", 4, 4), ("faces", 8, 2), ("faceColours", 0xC, 1), ("uv", 0x10, 2), ("colours", 0x18, 1),
                                    ("faceFlags", 0x1C, 1), ("textures", 0x20, 4)):
                ptr(m + at, align, "mesh field " + what)
        for what in (0x34, 0x38):
            c = ptr(o + what, 4, "child or sibling")
            if c:
                obj(c)

    n = 0
    while n < 4096:
        crc, inst = u32(8 * n), u32(8 * n + 4)
        if crc is None or inst is None:
            problems.append("%s: the entry list has no end" % label)
            return
        if not inst:
            break
        i = inst & 0xFFFFFF
        if i + 0x1C > len(data) or i % 4:
            problems.append("%s: entry %d points to 0x%X: outside the bank or not aligned" % (label, n, i))
            return
        o = ptr(i + 0x0C, 4, "instance object")
        if o:
            obj(o)
        n += 1


def check_rom(rom, info=None):
    problems = []
    info = info or json.load(open(MOD_JSON))
    if len(rom) % 0x100000 or len(rom) < 0x800000 or len(rom) > 0x4000000:
        problems.append("the ROM is %d bytes: it must be a whole number of MiB, at least 8 MiB and at most 64 MiB" % len(rom))
    crc = header_crc(rom)
    have = struct.unpack_from(">II", rom, 0x10)
    if tuple(crc) != have:
        problems.append("header CRC is %08X %08X, the code gives %08X %08X" % (have + tuple(crc)))
    mod_rom, mod_size = int(info["mod_rom"], 16), info["mod_size"]
    if mod_rom % 16 or mod_size % 16 or mod_rom + mod_size > len(rom):
        problems.append("the mod (ROM 0x%X, %d bytes) is not 16 byte aligned or is not inside the ROM" % (mod_rom, mod_size))
    for name, addr in info.get("hw_symbols", {}).items():
        if int(addr, 16) % 16:
            problems.append("mod symbol %s is at %s: not 16 byte aligned" % (name, addr))
    mod_ram = int(info["mod_ram"], 16)
    if mod_ram % 16 or int(info["mod_end"], 16) > 0x80800000:
        problems.append("the mod does not fit in RAM below 0x80800000")
    for h in info["hooks"]:
        word = int(h["new"], 16)
        target = ((word & 0x03FFFFFF) << 2) | 0x80000000
        into_mod = mod_ram <= target < mod_ram + mod_size
        loader = h["why"].endswith("(loader)")  # the loader is in the game's own code (over ScreenShot)
        if word >> 26 != 3 or (into_mod == loader):
            problems.append("hook at %s is not a jal to %s (%s)" % (h["ram"], "the loader" if loader else "a word inside the mod", h["why"]))
    base = mod_rom
    t = base + info["slots"]["offset"] if "slots" in info else None
    if t is not None and rom[t:t + 4] == b"SLT1":
        for i in range(7):
            o = t + 8 + i * SLOT_LEVEL_SIZE
            r = struct.unpack_from(">I", rom, o)[0]
            if not r:
                continue
            label = "extra level %d" % (35 + i)
            if r % 16 or r + 8 > len(rom):
                problems.append("%s starts at 0x%X: not 16 byte aligned or outside the ROM" % (label, r))
                continue
            size = struct.unpack_from(">I", rom, r)[0]
            if size % 4 or not 16 <= size <= MAX_LEVEL_FILE:
                problems.append("%s: its size word is %d" % (label, size))
                continue
            end = r + ((size + 15) & ~15)
            if end > len(rom):
                problems.append("%s: the DMA of %d bytes runs past the end of the ROM" % (label, end - r))
            elif any(rom[r + size:end]):
                problems.append("%s: the padding after the file is not zero" % label)
    c = base + info["custom"]["offset"]
    if rom[c:c + 4] == b"CUS1":
        count = struct.unpack_from(">I", rom, c + 4)[0]
        for i in range(min(count, 8)):
            key, ts, te, os_, oe = struct.unpack_from(">B3xIIII", rom, c + 8 + i * 20)
            for what, a, b in (("textures", ts, te), ("objects", os_, oe)):
                if not a and not b:
                    continue
                label = "custom banks of key 0x%02X, %s" % (key, what)
                if a % 16 or b % 16 or b <= a or b > len(rom):
                    problems.append("%s: 0x%X to 0x%X: the start and the end must be multiples of 16 inside the ROM" % (label, a, b))
                    continue
                if what == "textures":
                    check_texture_bank(rom, a, b, label, problems)
                else:
                    check_object_bank(rom, a, b, label, problems)
    return problems


def self_test(rom):
    """Breaks a good ROM in the ways the rules name and checks that each break is found."""
    info = json.load(open(MOD_JSON))
    base = int(info["mod_rom"], 16)
    c = base + info["custom"]["offset"]
    ts, te = struct.unpack_from(">II", rom, c + 8 + 4)
    cases = {}
    r = bytearray(rom); r[0x14] ^= 1
    cases["wrong CRC"] = bytes(r)
    cases["ROM not a whole MiB"] = rom + bytes(1)
    r = bytearray(rom); struct.pack_into(">I", r, c + 8 + 8, te - 1)
    cases["odd DMA length of a custom bank"] = bytes(r)
    r = bytearray(rom); struct.pack_into(">I", r, ts + 4 + 20, struct.unpack_from(">I", rom, ts + 4 + 20)[0] + 4)
    cases["texture record not a multiple of 8"] = bytes(r)
    r = bytearray(rom); struct.pack_into(">I", r, c + 8 + 4, ts + 2)
    cases["unaligned bank start"] = bytes(r)
    os0 = struct.unpack_from(">I", rom, c + 8 + 12)[0]
    r = bytearray(rom)
    inst = struct.unpack_from(">I", rom, os0 + 4)[0]
    obj_at = os0 + (inst & 0xFFFFFF) + 0x0C
    struct.pack_into(">I", r, obj_at, struct.unpack_from(">I", rom, obj_at)[0] + 2)
    cases["object pointer not aligned"] = bytes(r)
    bad = [name for name, data in cases.items() if not check_rom(data, info)]
    for name in cases:
        print("self-test: %-40s %s" % (name, "NOT FOUND" if name in bad else "found"))
    return not bad


def main():
    if len(sys.argv) == 3 and sys.argv[1] == "--self-test":
        sys.exit(0 if self_test(open(sys.argv[2], "rb").read()) else 1)
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    rom = open(sys.argv[1], "rb").read()
    problems = check_rom(rom)
    for p in problems:
        print("PROBLEM:", p)
    print("%s: %s" % (sys.argv[1], "%d problem(s)" % len(problems) if problems else "all hardware checks pass (CRC %08X %08X)" % header_crc(rom)))
    sys.exit(1 if problems else 0)


if __name__ == "__main__":
    main()
