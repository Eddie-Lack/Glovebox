#!/usr/bin/env python3
"""One-time setup for Glovebox.

1. Splits the ROM into noclip.website's Glover data set (noclip's own rom_split.py + file_offsets.yaml),
   giving data/Glover/*.lev, *.obj.fla, *.tex.fla exactly as noclip serves them.
2. Reads noclip's scene table (src/Glover/scenes.ts) for each level's name and object/texture banks.
3. Builds the decomp cross-reference from gloveworld (tools/build_xref.py).

    python3 prepare.py --rom ../gloveworld/baserom.us.z64 --gloveworld ../gloveworld

It writes data/, web/*.json and config.json into this folder, or into the folder that the GLOVEBOX_HOME environment
variable names (as core.py reads them). The desktop app runs it with GLOVEBOX_HOME set, with the few gloveworld
files it needs (app/decomp/) as --gloveworld, and with --decomp-json, which copies a ready cross-reference
instead of building it from the decomp's sources.
"""
import argparse, glob, json, os, re, shutil, subprocess, sys

HERE = os.path.dirname(os.path.abspath(__file__))
HOME = os.path.abspath(os.environ.get("GLOVEBOX_HOME") or HERE)
sys.path.insert(0, os.path.join(HERE, "tools"))
from pycmd import python  # noqa: E402
NOCLIP = os.path.join(HERE, "vendor", "noclip", "src", "Glover")
LIBGARIB = os.path.join(HERE, "vendor", "libgarib")


def scenes_from_noclip():
    src = open(os.path.join(NOCLIP, "scenes.ts")).read()
    banks = {}
    for m in re.finditer(r'\["([0-9a-f]{2})",\s*\{(.*?)\}\]', src, re.S):
        body = m.group(2)
        get = lambda key: re.findall(r'"([^"]+)"', (re.search(key + r":\s*\[(.*?)\]", body, re.S) or [None, ""])[1])
        banks[m.group(1)] = {
            "landscape": re.search(r'landscape:\s*"([^"]+)"', body).group(1),
            "objectBanks": get("object_banks"),
            "textureBanks": get("texture_banks"),
        }
    scenes, group = [], None
    descs = src[src.index("const sceneDescs"):]
    for m in re.finditer(r'^\s*(?:"([^"]+)",|new SceneDesc\(`([0-9a-f]{2})`,\s*"([^"]+)"\))', descs, re.M):
        if m.group(1):
            group = m.group(1)
        elif m.group(2) in banks:
            sid = m.group(2)
            scenes.append(dict(id=sid, index=int(sid, 16), name=m.group(3), group=group, **banks[sid]))
    return scenes


def noclip_tables():
    """Enemy and powerup model tables that noclip hard-codes (enemy.ts, scenes.ts)."""
    def arr(path, name):
        src = open(os.path.join(NOCLIP, path)).read()
        body = re.search(r"const %s\s*=\s*\[(.*?)\n\]" % name, src, re.S).group(1)
        out = []
        for line in body.split("\n"):
            m = re.match(r"\s*(0x[0-9A-Fa-f]+|[0-9.]+)\s*,?\s*(?://\s*(.*))?", line)
            if m:
                out.append({"value": float(m.group(1)) if "." in m.group(1) else int(m.group(1), 0),
                            "comment": (m.group(2) or "").strip(" /")})
        return out
    return {"enemyObjects": arr("enemy.ts", "enemy_objects"), "enemyScales": arr("enemy.ts", "enemy_scales"),
            "powerupObjects": arr("scenes.ts", "powerup_objects"), "ballObject": 0x8E4DDE49}


CUTSCENES = {"2b", "2c", "2d", "2e", "2f"}
BOSSES = {"0d", "12", "17", "1c", "21", "26", "27", "28"}


def enemy_templates(scenes):
    """For each enemy type, the shortest script a shipped (non-cutscene) level gives it. The editor copies it
    for new enemies and shifts its absolute coordinates (home_*, destination_*, lookat_*, attention box)."""
    sys.path.insert(0, os.path.join(LIBGARIB, "python"))
    import warnings
    warnings.filterwarnings("ignore")
    from lxml import etree as ET
    from libgarib.parsers.glover_level import GloverLevel
    from libgarib.levels import landscapeToXML
    found = {}
    for s in scenes:
        with open(os.path.join(HOME, "data", "Glover", s["landscape"]), "rb") as f:
            root = landscapeToXML(GloverLevel.from_io(f), trim=True).getroot()
        for e in root.iter("Enemy"):
            t = e.get("type")
            rank = (s["id"] in CUTSCENES, len(list(e.iter())))
            found.setdefault(t, []).append((rank, s["id"], e))
    out = {}
    for t, lst in found.items():
        lst.sort(key=lambda x: x[0])
        rank, sid, e = lst[0]
        e.tail = None
        out[t] = {"level": sid, "boss": all(x[1] in BOSSES for x in lst), "cutsceneOnly": rank[0],
                  "xml": ET.tostring(e).decode(), "levels": sorted({x[1] for x in lst})}
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--rom", required=True)
    ap.add_argument("--gloveworld", required=True)
    ap.add_argument("--decomp-json", help="copy this cross-reference instead of building it from the decomp")
    args = ap.parse_args()
    web = os.path.join(HOME, "web")
    os.makedirs(web, exist_ok=True)

    # patches/: fixes libgarib's glTF export needs to get through every object bank, and the format rules that let
    # SetActorRotation/SetActorScale sit inside enemies, platforms, powerups and water (for the game mod in mod/).
    # The desktop app's copy of libgarib is patched when it is built, and is not a git checkout.
    if os.path.isdir(os.path.join(LIBGARIB, ".git")):
        for patch in sorted(glob.glob(os.path.join(HERE, "patches", "libgarib-*.patch"))):
            if subprocess.run(["git", "-C", LIBGARIB, "apply", "--check", patch], capture_output=True).returncode == 0:
                subprocess.check_call(["git", "-C", LIBGARIB, "apply", patch])
                print("applied", os.path.basename(patch))

    data = os.path.join(HOME, "data", "Glover")
    if not os.path.exists(os.path.join(data, "00.HUB1ln.n64.lev")):
        subprocess.check_call(python(os.path.join(NOCLIP, "tools", "rom_split.py"), args.rom,
                                     os.path.join(NOCLIP, "tools", "file_offsets.yaml"), data, flags=("-I",)))

    scenes = scenes_from_noclip()
    # The banks the game itself loads (LoadLevel in the decomp), next to noclip's list
    from game_banks import game_banks
    gb = game_banks(args.gloveworld, args.rom)
    canon = lambda n: n.split(".")[0]
    for sc in scenes:
        g = gb[sc["id"]]
        sc["gameObjectBanks"] = g["objectBanks"]
        sc["gameTextureBanks"] = [t if t.endswith("_TEX_BANK") else t + "_TEX_BANK" for t in g["textureBanks"]]
        # Banks both agree on: what the editor treats as "already loaded" when deciding what to import
        sc["sureObjectBanks"] = [b for b in sc["objectBanks"] if canon(b) in g["objectBanks"]]
        sc["sureTextureBanks"] = [t for t in sc["textureBanks"] if canon(t) in sc["gameTextureBanks"]]
        sc["importObjectBank"] = g["objectBanks"][-1]
        sc["importTextureBank"] = sc["gameTextureBanks"][-1]
    with open(os.path.join(web, "scenes.json"), "w") as f:
        json.dump(scenes, f, indent=1)
    print("%d scenes from noclip's scene table" % len(scenes))
    with open(os.path.join(web, "noclip_tables.json"), "w") as f:
        json.dump(noclip_tables(), f, indent=1)

    with open(os.path.join(web, "enemy_templates.json"), "w") as f:
        json.dump(enemy_templates(scenes), f, indent=1)
    if args.decomp_json:
        shutil.copy(args.decomp_json, os.path.join(web, "decomp.json"))
    else:
        subprocess.check_call(python(os.path.join(HERE, "tools", "build_xref.py"), "--gloveworld", args.gloveworld,
                                     "--libgarib", LIBGARIB, "--out", os.path.join(web, "decomp.json")))

    with open(os.path.join(HOME, "config.json"), "w") as f:
        json.dump({"rom": os.path.abspath(args.rom), "gloveworld": os.path.abspath(args.gloveworld)}, f, indent=1)
    print("ready" if os.environ.get("GLOVEBOX_HOME") else "ready: python3 -m desktop")


if __name__ == "__main__":
    main()
