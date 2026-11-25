#!/usr/bin/env python3
"""Build the Expansion Pak game mod (mod/) into mod/mod.bin and mod/mod.json.

    python3 tools/build_mod.py [--gloveworld ../gloveworld]

Needs a built gloveworld checkout: its GCC 2.7.2 (`make toolchain`), its headers and build/NGVE.elf, plus
mips-linux-gnu binutils. The outputs are committed. Building ROMs does not need any of this.

mod/aligned.c, mod/heap.c, mod/transforms.c, mod/banks.c, mod/guard.c and mod/slots.c (SOURCES) are linked at
MOD_RAM, near the top of the Expansion Pak's memory, against the game's symbols, and are stored in the ROM at MOD_ROM.
The game's heap gets the memory below MOD_RAM (heap.c).
mod/loader.c is linked over ScreenShot, a debug function the game never calls. The JSON lists every word to patch
in the game's code: the loader, and the `jal`s that now call the mod.
"""
import argparse, hashlib, json, os, re, struct, subprocess, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
MOD = os.path.join(ROOT, "mod")
MOD_RAM = 0x807E0000  # the mod's code and data; the heap ends here, and 128 KiB is left for the mod
MOD_TOP = 0x80800000
SOURCES = ["aligned.c", "heap.c", "transforms.c", "banks.c", "guard.c", "slots.c"]
MOD_ROM = 0xC00000  # 4 MiB past the original 8 MiB ROM; libgarib's repacked data (which grows with imports) never reaches it
CODE_RAM, CODE_ROM = 0x80100000, 0x1000  # the game's code segment: RAM 0x80100000 is ROM 0x1000


def run(cmd, **kw):
    r = subprocess.run(cmd, capture_output=True, text=True, **kw)
    if r.returncode:
        sys.exit("failed: %s\n%s%s" % (" ".join(cmd), r.stdout, r.stderr))
    return r.stdout


def symbols(elf):
    out = {}
    for line in run(["mips-linux-gnu-nm", "-S", elf]).splitlines():
        p = line.split()
        if len(p) == 4:
            out[p[3]] = (int(p[0], 16), int(p[1], 16))
        elif len(p) == 3:
            out.setdefault(p[2], (int(p[0], 16), 0))
    return out


def jal(addr):
    return 0x0C000000 | ((addr & 0x0FFFFFFF) >> 2)


def call_sites(code, syms, callee, within=None):
    """RAM addresses of every `jal callee` in the game's code, optionally only inside one function."""
    word = struct.pack(">I", jal(syms[callee][0]))
    lo, hi = (syms[within][0], syms[within][0] + syms[within][1]) if within else (0x8010E0D0, 0x801BC710)
    sites = []
    for ram in range(lo, hi, 4):
        off = ram - CODE_RAM + CODE_ROM
        if code[off:off + 4] == word:
            sites.append(ram)
    return sites


def main():
    ap = argparse.ArgumentParser()
    cfg = os.path.join(ROOT, "config.json")
    default_gw = json.load(open(cfg))["gloveworld"] if os.path.exists(cfg) else os.path.join(ROOT, "..", "gloveworld")
    ap.add_argument("--gloveworld", default=default_gw)
    ap.add_argument("--elf", help="also keep the linked mod.elf at this path (for objdump and the hardware checks)")
    a = ap.parse_args()
    gw = os.path.abspath(a.gloveworld)
    elf = os.path.join(gw, "build", "NGVE.elf")
    rom = open(os.path.join(gw, "build", "NGVE.z64"), "rb").read()
    gcc_dir = os.path.join(gw, "tools", "gcc-2.7.2")
    env = dict(os.environ, COMPILER_PATH=gcc_dir)
    cflags = ["-DMOD_RAM=0x%08X" % MOD_RAM, "-G0", "-mcpu=r4000", "-mips2", "-mmips-as", "-funsigned-char", "-O2",
              "-Wa,-EB,-mips3,-mabi=32,-mgp32,-march=vr4300,-mfp32,-mno-shared,--no-pad-sections,-Iinclude",
              "-D_LANGUAGE_C", "-DF3DEX_GBI", "-D_FINALROM", "-DNDEBUG", "-DTARGET_N64",
              "-I" + os.path.join(gw, "include"), "-I" + os.path.join(gw, "include", "PR"), "-I" + os.path.join(gw, "src"), "-I" + gw]
    game = symbols(elf)
    tmp = tempfile.mkdtemp(prefix="mod-")
    # The game's symbols as absolute definitions (--just-symbols trips ld's jal range check for MIPS)
    game_ld = os.path.join(tmp, "game.ld")
    with open(game_ld, "w") as f:
        for name, (addr, _) in sorted(game.items()):
            if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", name) and 0x80000000 <= addr < 0x80400000:
                f.write("%s = 0x%08X;\n" % (name, addr))

    # The call sites that guard.c patches at run time (Mod_InitGuard), with an Expansion Pak only,
    # and the ones that slots.c patches (Mod_InitSlots), only when the ROM stores extra levels
    guard_sites = [("TransformAndDrawObject", "TransformAndDrawObject", "DrawActor"), ("CastShadow", "CastShadow", "ProcessShadows"),
                   ("DrawFootprint", "DrawFootprint", "DrawFootprints"),
                   ("LoadLandscapeFile", "LoadLandscapeFile", "ParseLandscapeFile"), ("CheckForExit", "CheckForExit", None),
                   ("BonusOpen", "BonusOpen", "CheckForExits"), ("StartNewGame", "StartNewGame", None),
                   ("EepromWrite", "osEepromLongWrite", None), ("EepromRead", "osEepromLongRead", None),
                   ("CheckRestartGate", "CheckRestartGate", "UpdatePlatform")]
    with open(os.path.join(tmp, "sites.h"), "w") as f:
        for name, callee, within in guard_sites:
            sites = call_sites(rom, game, callee, within)
            if not sites:
                sys.exit("no call site found for " + callee)
            f.write("static u32 *const sites%s[] = { %s };\n" % (name, ", ".join("(u32 *)0x%08X" % a for a in sites)))

    # 1. The mod itself, at MOD_RAM. Mod_Init (heap.c) clears the .bss between modBssStart and modBssEnd.
    objs = []
    for src in SOURCES:
        obj = os.path.join(tmp, src.replace(".c", ".o"))
        run([os.path.join(gcc_dir, "gcc"), "-c"] + cflags + ["-I" + tmp, "-o", obj, os.path.join(MOD, src)], env=env, cwd=gw)
        objs.append(obj)
    with open(os.path.join(tmp, "mod.ld"), "w") as f:
        f.write("SECTIONS {\n    . = 0x%08X;\n    .text : { *(.text) }\n"
                "    .rodata : { . = ALIGN(16); *(.rodata .rodata.* .rdata) }\n"
                "    .data : { *(.data .sdata) }\n    . = ALIGN(16);\n    modBssStart = .;\n"
                "    .bss : { *aligned.o(.bss .sbss COMMON) *(.bss .sbss COMMON) }\n"
                "    modBssEnd = .;\n    /DISCARD/ : { *(.reginfo .MIPS.abiflags .pdr .comment .gnu.attributes .mdebug*) }\n}\n" % MOD_RAM)
    run(["mips-linux-gnu-ld", "-T", "mod.ld", "-T", game_ld, "-o", "mod.elf"] + objs, cwd=tmp)
    if a.elf:
        import shutil
        shutil.copy(os.path.join(tmp, "mod.elf"), a.elf)
    blob_path = os.path.join(MOD, "mod.bin")
    run(["mips-linux-gnu-objcopy", "-O", "binary", "--only-section=.text", "--only-section=.rodata", "--only-section=.data",
         os.path.join(tmp, "mod.elf"), blob_path])
    blob = open(blob_path, "rb").read()
    mod = symbols(os.path.join(tmp, "mod.elf"))
    bss_end = mod["modBssEnd"][0]
    if bss_end > MOD_TOP:
        sys.exit("the mod ends at 0x%08X, past the end of RAM" % bss_end)
    # Real hardware: the PI and the data cache need a DMA buffer to be 8 byte aligned and alone in its 16 byte cache lines. GCC 2.7.2 may
    # drop an `aligned` attribute, so the linked address is checked.
    for name in ("levelHeader",):
        addr, size = mod[name]
        if addr % 16 or (size and size % 16):
            sys.exit("%s is at 0x%08X with %d bytes: it must be 16 byte aligned and a multiple of 16 bytes (see docs/technical.md, Real hardware)" % (name, addr, size))
    # Real hardware: a word or halfword access must be aligned (a console traps the others). GCC 2.7.2 put two s32 of .bss right after a 30
    # byte array, on a 2 byte boundary, and mupen64plus read them anyway. A .bss symbol has no size here: its gap to the next symbol stands in.
    kinds = {l.split()[-1]: l.split()[-2] for l in run(["mips-linux-gnu-nm", os.path.join(tmp, "mod.elf")]).splitlines()}
    data = sorted((a, n) for n, (a, s) in mod.items() if MOD_RAM <= a < bss_end and kinds.get(n) in ("b", "B", "d", "D"))
    for (a, n), (b, _) in zip(data, data[1:] + [(bss_end, None)]):
        gap = mod[n][1] or b - a
        need = 4 if gap % 4 == 0 else 2 if gap % 2 == 0 else 1
        if gap and a % need:
            sys.exit("%s is at 0x%08X (%d bytes): it must be %d byte aligned (see docs/technical.md, Real hardware, CPU)" % (n, a, gap, need))
    if MOD_RAM % 16 or (len(blob) + 15) & ~15 > mod["modBssStart"][0] - MOD_RAM:
        sys.exit("the mod's data does not end before its bss starts")

    # 2. The hooks: same-size `jal` changes in the game's code
    hooks = []
    def hook(sites, target, why):
        if not sites:
            sys.exit("no call site found for " + why)
        for ram in sites:
            off = ram - CODE_RAM + CODE_ROM
            hooks.append({"ram": "0x%08X" % ram, "rom": "0x%X" % off, "why": why,
                          "old": rom[off:off + 4].hex(), "new": "%08x" % jal(target)})
    hook(call_sites(rom, game, "ParseTokenGeneric", "ParseLandscapeFile"), mod["Mod_ParseTokenGeneric"][0], "ParseLandscapeFile -> ParseTokenGeneric")
    hook(call_sites(rom, game, "ParseLandscapeFile"), mod["Mod_ParseLandscapeFile"][0], "ParseLandscapeFile")
    hook(call_sites(rom, game, "DrawActor"), mod["Mod_DrawActor"][0], "DrawActor")
    restart = [s for s in call_sites(rom, game, "RestartEnemy")
               if not (game["LoadEnemy"][0] <= s < game["LoadEnemy"][0] + game["LoadEnemy"][1])]
    hook(restart, mod["Mod_RestartEnemy"][0], "RestartEnemy (respawn)")
    # MainProc's JallocInit runs before the loader; Mod_Init starts the heap again. The other calls run on level changes.
    heap = [s for s in call_sites(rom, game, "JallocInit")
            if not (game["MainProc"][0] <= s < game["MainProc"][0] + game["MainProc"][1])]
    hook(heap, mod["Mod_JallocInit"][0], "JallocInit (heap end at MOD_RAM)")

    # 3. The loader over ScreenShot; it restores the hooks above when there is no Expansion Pak
    shot_ram, shot_size = game["ScreenShot"]
    loader_ram = (shot_ram + 15) & ~15  # the .text section is 16-byte aligned
    loader_max = shot_ram + shot_size - loader_ram
    with open(os.path.join(tmp, "hooks.h"), "w") as f:
        f.write("#define MOD_RAM 0x%08X\n#define MOD_ROM 0x%X\n#define MOD_SIZE 0x%X\n#define PARSE_HOOK %s\n"
                % (MOD_RAM, MOD_ROM, (len(blob) + 15) & ~15, hooks[0]["ram"]))
        f.write("static Hook hooks[] = {\n" + "".join("    { (u32 *)%s, 0x%s },\n" % (h["ram"], h["old"]) for h in hooks) + "};\n")
    with open(os.path.join(tmp, "loader.ld"), "w") as f:
        f.write("SECTIONS {\n    . = 0x%08X;\n    .text : { *(.text) *(.rodata .rodata.* .rdata) *(.data .sdata) }\n"
                "    .bss : { *(.bss .sbss COMMON) }\n"
                "    /DISCARD/ : { *(.reginfo .MIPS.abiflags .pdr .comment .gnu.attributes .mdebug*) }\n}\n" % loader_ram)
    run([os.path.join(gcc_dir, "gcc"), "-c"] + cflags + ["-I" + tmp, "-o", os.path.join(tmp, "loader.o"), os.path.join(MOD, "loader.c")], env=env, cwd=gw)
    with open(os.path.join(tmp, "mod_syms.ld"), "w") as f:
        f.write("Mod_Init = 0x%08X;\n" % mod["Mod_Init"][0])
    run(["mips-linux-gnu-ld", "-T", "loader.ld", "-T", game_ld, "-T", "mod_syms.ld",
         "-o", "loader.elf", "loader.o"], cwd=tmp)
    run(["mips-linux-gnu-objcopy", "-O", "binary", "--only-section=.text", os.path.join(tmp, "loader.elf"), os.path.join(tmp, "loader.bin")])
    loader = open(os.path.join(tmp, "loader.bin"), "rb").read()
    lsyms = symbols(os.path.join(tmp, "loader.elf"))
    if len(loader) > loader_max:
        sys.exit("loader is %d bytes; ScreenShot has %d" % (len(loader), loader_max))
    off = loader_ram - CODE_RAM + CODE_ROM
    region = {"ram": "0x%08X" % loader_ram, "rom": "0x%X" % off, "why": "loader (over ScreenShot)",
              "old_sha1": hashlib.sha1(rom[off:off + len(loader)]).hexdigest(), "new": loader.hex()}
    hook(call_sites(rom, game, "InitCRCTable", "MainProc"), lsyms["ModLoader"][0], "MainProc -> InitCRCTable (loader)")

    out = {"about": "Expansion Pak game mod: rotation and scale for every 3D object, a larger heap, models from other banks, the matrix guard, extra level slots. "
                    "Built by tools/build_mod.py from mod/.",
           "mod_rom": "0x%X" % MOD_ROM, "mod_ram": "0x%08X" % MOD_RAM, "mod_bytes": len(blob),
           "heap_end": "0x%08X" % MOD_RAM, "heap_extra_bytes": MOD_RAM - 0x80400000,
           "mod_sha1": hashlib.sha1(blob).hexdigest(), "mod_size": (len(blob) + 15) & ~15, "mod_end": "0x%08X" % bss_end,
           "extra_banks": {"offset": mod["modExtraBanks"][0] - MOD_RAM, "size": mod["modExtraBanks"][1]},
           "custom": {"offset": mod["modCustom"][0] - MOD_RAM, "size": mod["modCustom"][1]},
           "slots": {"offset": mod["modSlots"][0] - MOD_RAM, "size": mod["modSlots"][1]},
           "hw_symbols": {"levelHeader": "0x%08X" % mod["levelHeader"][0]},
           "symbols": {n: "0x%08X" % mod[n][0] for n in ("modHeapGen", "modPeakMatrices", "modSkippedDraws", "modSlotsActive")},
           "regions": [region], "hooks": hooks}
    json.dump(out, open(os.path.join(MOD, "mod.json"), "w"), indent=1)
    print("mod %d bytes (+%d bss), loader %d of %d bytes, %d hooks" % (len(blob), bss_end - MOD_RAM - len(blob), len(loader), loader_max, len(hooks)))


if __name__ == "__main__":
    main()
