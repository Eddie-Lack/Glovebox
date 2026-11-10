#!/usr/bin/env python3
"""Which object and texture banks the game itself loads for each level, from the gloveworld decomp.

Follows LoadLevel (src/game.c): texture bank 2 (GENERIC), then the world's texBank (WAYROOM uses world 0's,
PRESENT uses 12, levels >= 46 use 11), CARNIVALBONUS adds 15, FLYTHRU (44) adds 13; object banks are the
world's objBank, then 1 (GENERIC) or 69 for FLYTHRU, then levelInfo[level].objBank (loaded last).
levelInfo/worldInfo are read from the decomp's built ELF. Without a build, they are read from the ROM: both
are in the uncompressed main code segment, whose address comes from glover.yaml and the symbols from
symbol_addrs.txt. Bank numbers are named from the LoadObjectBank/LoadTextureBank switch statements in
src/banks.c.

    python3 tools/game_banks.py --gloveworld ../gloveworld [--rom baserom.z64] [--compare web/scenes.json]
"""
import argparse, json, os, re, struct, subprocess


def read(elf, addr, n):
    out = subprocess.run(["mips-linux-gnu-objdump", "-s", "--start-address=0x%x" % addr, "--stop-address=0x%x" % (addr + n), elf],
                         capture_output=True, text=True).stdout
    data = b""
    for line in out.splitlines():
        m = re.match(r"\s*([0-9a-f]{8}) ((?:[0-9a-f]{2,8} ){1,4})", line)
        if m:
            data += bytes.fromhex(m.group(2).replace(" ", ""))
    return data[:n]


def symbol(elf, name):
    out = subprocess.run(["mips-linux-gnu-nm", elf], capture_output=True, text=True).stdout
    return int(re.search(r"^([0-9a-f]+) \w %s$" % re.escape(name), out, re.M).group(1), 16)


def rom_reader(gloveworld, rom):
    """read(addr, n) and symbol(name) for the ROM, through the decomp's main segment mapping."""
    yaml = open(gloveworld + "/glover.yaml").read()
    m = re.search(r"- name: main\s+type: code\s+start: (0x[0-9A-Fa-f]+)\s+vram: (0x[0-9A-Fa-f]+)", yaml)
    start, vram = int(m.group(1), 16), int(m.group(2), 16)
    data = open(rom, "rb").read()
    syms = dict(re.findall(r"^(\w+) = (0x[0-9A-Fa-f]+);", open(gloveworld + "/symbol_addrs.txt").read(), re.M))
    return (lambda addr, n: data[addr - vram + start:addr - vram + start + n]), (lambda name: int(syms[name], 16))


def bank_names(src, func, consts={}):
    body = src[src.index("void %s(u32 num) {" % func):]
    body = body[:body.index("\n}\n")]
    names, pending = {}, []
    for line in body.splitlines():
        m = re.match(r"\s*case (\w+):", line)
        if m:
            pending.append(int(m.group(1)) if m.group(1).isdigit() else consts[m.group(1)])
        m = re.search(r'sprintf\(message, "([^"]+)"\)', line)
        if m and pending:
            for n in pending:
                names[n] = m.group(1)
            pending = []
    return names


def level_to_world(level):
    """LevelToWorld (src/game.c). WAYROOM (42) returns gameInfo.world, i.e. whichever world you came from."""
    if 0 <= level < 10 or level in (44, 45):
        return 0
    for w, lo in enumerate((10, 15, 20, 25, 30), 1):
        if lo <= level < lo + 5:
            return w
    if level == 42:
        return None
    return 6 if level < 46 else 7


def game_banks(gloveworld, rom=None):
    elf = gloveworld + "/build/NGVE.elf"
    if os.path.exists(elf) or not rom:
        rd, sym = (lambda addr, n: read(elf, addr, n)), (lambda name: symbol(elf, name))
    else:
        rd, sym = rom_reader(gloveworld, rom)
    src = open(gloveworld + "/src/banks.c").read()
    # Some cases are named by enum constants (e.g. GENERIC_TEX_BANK)
    from build_xref import parse_enums
    consts = {k: v for e in parse_enums(gloveworld + "/include/enums.h").values() for k, v in e.items()}
    obj_names, tex_names = bank_names(src, "LoadObjectBank", consts), bank_names(src, "LoadTextureBank", consts)
    li, wi = sym("levelInfo"), sym("worldInfo")
    worlds = [struct.unpack(">bb", rd(wi + 0x18 * w, 2)) for w in range(7)]
    out = {}
    for level in range(48):
        obj_bank = struct.unpack(">i", rd(li + 0x38 * level + 0x28, 4))[0]
        world = level_to_world(level)
        tex, obj = [2], []
        if level < 46:
            tex.append(worlds[0][1] if level == 42 else 12 if level == 43 else worlds[world][1])
            obj.append(worlds[world if world is not None else 0][0])  # WAYROOM: the current world's bank
        else:
            tex.append(11)
        if level == 19:
            tex.append(15)
        if level == 44:
            tex.append(13)
        obj += [69 if level == 44 else 1, obj_bank]
        out["%02x" % level] = {"objectBanks": [obj_names.get(b, str(b)) for b in obj],
                              "textureBanks": [tex_names.get(b, str(b)) for b in tex]}
    return out


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--gloveworld", required=True)
    ap.add_argument("--rom", help="read levelInfo/worldInfo from this ROM when the decomp isn't built")
    ap.add_argument("--compare")
    a = ap.parse_args()
    banks = game_banks(a.gloveworld, a.rom)
    if not a.compare:
        print(json.dumps(banks, indent=1))
    else:
        scenes = {s["id"]: s for s in json.load(open(a.compare))}
        norm = lambda names: sorted(n.split(".")[0].replace("_TEX_BANK", "").upper() for n in names)
        diffs = 0
        for sid, g in banks.items():
            s = scenes[sid]
            for key in ("objectBanks", "textureBanks"):
                gn, nn = norm(g[key]), norm(s[key])
                if gn != nn:
                    diffs += 1
                    print(sid, s["name"], key, "game:", gn, "noclip:", nn)
        print("%d differences across %d levels" % (diffs, len(banks)))
