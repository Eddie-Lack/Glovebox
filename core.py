"""The editor's back end: everything that touches the game's data, done with libgarib.

  - levels as libgarib XML (level-tool disassemble), checked against libgarib's RelaxNG schema and assembled again
  - models as glTF and textures as PNG (objbank-tool, texbank-tool), converted once and cached
  - the catalog of every model in the game's banks and of the author's imported models
  - imported models (tools/custom_model.py), the kit (tools/kit.py) and reskinned models
  - which banks an edited level needs, and ROM and BPS builds (rom-asset-tool, tools/mod_patch.py)

desktop/backend.py calls these functions in the editor's own process. Nothing here opens a port.

Writable files (config.json, the split ROM data in data/, the cache in cache/ and the JSON that prepare.py writes
into web/) live in this folder, or in the folder that the GLOVEBOX_HOME environment variable names. The desktop app
(app/launcher.py) sets GLOVEBOX_HOME to a folder in the user's profile.
"""
import hashlib, io, json, os, random, re, shutil, subprocess, sys, tempfile, threading, warnings
from urllib.parse import quote

HERE = os.path.dirname(os.path.abspath(__file__))
LIBGARIB = os.path.join(HERE, "vendor", "libgarib")
sys.path.insert(0, os.path.join(LIBGARIB, "python"))
warnings.filterwarnings("ignore")

from lxml import etree as ET  # noqa: E402
from libgarib.parsers.glover_level import GloverLevel  # noqa: E402
from libgarib.levels import landscapeToXML, xmlToLandscape, level_schema_path  # noqa: E402
sys.path.insert(0, os.path.join(HERE, "tools"))
import bps  # noqa: E402
from mod_patch import apply_mod, apply_easy_controls, apply_level_info, ModError  # noqa: E402
from pycmd import python  # noqa: E402
import custom_model  # noqa: E402
import kit  # noqa: E402

HOME = os.path.abspath(os.environ.get("GLOVEBOX_HOME") or HERE)
DATA = os.path.join(HOME, "data", "Glover")
CACHE = os.path.join(HOME, "cache")
WEB = os.path.join(HERE, "web")
# The files prepare.py writes into web/. With GLOVEBOX_HOME set they are in GLOVEBOX_HOME/web, not next to the code.
GENERATED = os.path.join(HOME, "web")
GENERATED_FILES = {"scenes.json", "noclip_tables.json", "enemy_templates.json", "decomp.json"}
ROM_MAP = os.path.join(LIBGARIB, "formats", "map.0x8e6e01ff.0xccb4f948.yaml")
CONFIG = json.load(open(os.path.join(HOME, "config.json")))
SCENES = {s["id"]: s for s in json.load(open(os.path.join(GENERATED, "scenes.json")))}
SCHEMA = ET.RelaxNG(file=level_schema_path)
LOCK = threading.Lock()


def tool(name, *args):
    r = subprocess.run(python(os.path.join(LIBGARIB, "tools", name), *args, flags=("-W", "ignore")),
                       capture_output=True, text=True)
    if r.returncode != 0:
        code = r.returncode if 0 <= r.returncode <= 255 else "0x%08X" % (r.returncode & 0xFFFFFFFF)
        raise RuntimeError("%s failed (exit code %s):\n%s%s" % (name, code, r.stdout, r.stderr))
    return r.stdout


def level_xml(sid):
    with open(os.path.join(DATA, SCENES[sid]["landscape"]), "rb") as f:
        tree = landscapeToXML(GloverLevel.from_io(f), trim=True)
    ET.indent(tree, space="  ")
    return ET.tostring(tree, xml_declaration=True, encoding="utf-8")


def model_index(sid):
    """Unpack each object bank of the scene to glTF (cached) and return {objectId: url}."""
    scene = SCENES[sid]
    texs = scene["textureBanks"]
    tag = "+".join(t.split("_TEX")[0] for t in texs)
    index = {}
    for bank in scene["objectBanks"]:
        name = bank.split(".")[0] + "@" + tag
        out = os.path.join(CACHE, "models", name)
        with LOCK:
            if not os.path.isdir(out):
                tmp = out + ".tmp"
                shutil.rmtree(tmp, ignore_errors=True)
                os.makedirs(tmp)
                args = ["unpack", "--scale", "1", "--output-dir", tmp]
                for t in texs:
                    args += ["-t", os.path.join(DATA, t)]
                tool("objbank-tool.py", *args, os.path.join(DATA, bank))
                os.rename(tmp, out)
        for fn in sorted(os.listdir(out)):
            m = re.match(r"(0x[0-9a-fA-F]{8})", fn)
            if m and fn.endswith(".glb"):
                index[m.group(1).upper().replace("0X", "0x")] = "/models/%s/%s" % (quote(name), fn)
    return index


def texture_dir(bank):
    """Unpack a texture bank to PNGs once (texbank-tool unpack)."""
    out = os.path.join(CACHE, "textures", bank.split(".")[0])
    with LOCK:
        if not os.path.isdir(out):
            tmp = out + ".tmp"
            shutil.rmtree(tmp, ignore_errors=True)
            tool("texbank-tool.py", "unpack", "--output-dir", tmp, os.path.join(DATA, bank))
            os.rename(tmp, out)
    return out


def find_texture(model_dir_name, png):
    """A model directory is named BANK@WORLD+WORLD...; search those texture banks (later banks win, like noclip)."""
    worlds = model_dir_name.split("@", 1)[1].split("+")
    for w in reversed(worlds):
        path = os.path.join(texture_dir(w + "_TEX_BANK.tex.fla"), png)
        if os.path.isfile(path):
            return path
    return None


# ---------------------------------------------------------------------------------------------
# Catalog of every model in the game, and importing models into a level that doesn't load them

_catalog = None


def catalog():
    """{objectId: {name, bank, url, bytes}} for every object in every bank, from each bank's first scene."""
    global _catalog
    if _catalog is None:
        path = os.path.join(CACHE, "catalog.json")
        if os.path.isfile(path):
            _catalog = json.load(open(path))
        else:
            cat = {}
            for sid, scene in SCENES.items():
                index = model_index(sid)
                for bank in scene["objectBanks"]:
                    stem = bank.split(".")[0]
                    singles = split_bank(stem)
                    for oid, info in singles.items():
                        if oid in cat or oid not in index:
                            continue
                        cat[oid] = {"name": info["name"], "bank": stem, "url": index[oid], "bytes": info["bytes"],
                                    "world": scene["textureBanks"][-1].split("_TEX")[0], "firstScene": sid}
            json.dump(cat, open(path, "w"))
            _catalog = cat
    return _catalog


def split_bank(stem):
    """objbank-tool split: one self-contained single-object bank per model (cached)."""
    out = os.path.join(CACHE, "split", stem)
    with LOCK:
        if not os.path.isdir(out):
            tmp = out + ".tmp"
            shutil.rmtree(tmp, ignore_errors=True)
            tool("objbank-tool.py", "split", "--output-dir", tmp, os.path.join(DATA, stem + ".obj.fla"))
            os.rename(tmp, out)
    res = {}
    for fn in os.listdir(out):
        m = re.match(r"0x([0-9a-f]{8})-(.*)\.obj\.bin$", fn)
        if m:
            res["0x" + m.group(1).upper()] = {"name": m.group(2), "path": os.path.join(out, fn),
                                              "bytes": os.path.getsize(os.path.join(out, fn))}
    return res


ALL_TEX_BANKS = sorted(f for f in os.listdir(DATA) if f.endswith(".tex.fla"))


def texture_ids(bank_file):
    return {"0x" + fn[2:10].upper() for fn in os.listdir(texture_dir(bank_file)) if fn.endswith(".png")}


def object_texture_ids(oid):
    """Textures an object uses: unpack its single-object bank against every texture bank and read the glTF."""
    if oid in custom_catalog():
        return set(custom_catalog()[oid]["textures"])
    out = os.path.join(CACHE, "objtex", oid + ".json")
    if os.path.isfile(out):
        return set(json.load(open(out)))
    single = split_bank(catalog()[oid]["bank"])[oid]["path"]
    with tempfile.TemporaryDirectory(dir=CACHE) as tmp:
        args = ["unpack", "--scale", "1", "--output-dir", tmp]
        for t in ALL_TEX_BANKS:
            args += ["-t", os.path.join(DATA, t)]
        tool("objbank-tool.py", *args, single)
        ids = set()
        for fn in os.listdir(tmp):
            if fn.endswith(".glb"):
                import pygltflib
                g = pygltflib.GLTF2().load(os.path.join(tmp, fn))
                for im in g.images:
                    m = re.search(r"(0x[0-9A-Fa-f]{8})", im.uri or "")
                    if m:
                        ids.add("0x" + m.group(1)[2:].upper())
    os.makedirs(os.path.dirname(out), exist_ok=True)
    json.dump(sorted(ids), open(out, "w"))
    return ids


def raw_bank(bank_file):
    """A bank's data, by its file name in data/, or as custom:SET for a custom model's texture bank"""
    from libgarib.fla2 import data_from_stream
    path = (os.path.join(CUSTOM, bank_file[7:], "textures.tex.bin") if bank_file.startswith("custom:")
            else os.path.join(DATA, bank_file))
    with open(path, "rb") as f:
        return data_from_stream(f)


def texbank_records(data):
    """Split a texture bank into (records, filenames). Each record is self-contained: the game points
    tex->data just past the 36-byte header and makes tex->palette relative to the record (LoadTextureBank).
    A record's length is a multiple of 8 (the game's own banks all are): the pixels of the next record start 4 bytes into the
    bank's first 8 byte word, so every record must keep them on a 64 bit boundary, which the RDP needs when it loads a texture.
    libgarib's packer does not pad. A record that is not a multiple of 8 gets zeros at its end and its size field is corrected."""
    import struct
    n = struct.unpack(">I", data[:4])[0]
    pos, recs = 4, []
    for _ in range(n):
        tid, length = struct.unpack(">I", data[pos:pos + 4])[0], struct.unpack(">I", data[pos + 20:pos + 24])[0]
        rec = data[pos:pos + length]
        if length % 8:
            rec += bytes(-length % 8)
            rec = rec[:20] + struct.pack(">I", len(rec)) + rec[24:]
        recs.append(("0x%08X" % tid, rec))
        pos += length
    names = data[pos:].split(b"\0")[:n]
    return recs, names


def rom_keys():
    """Bank file stem -> rom-asset-tool region key, from a one-off `rom-asset-tool dump` manifest."""
    out = os.path.join(CACHE, "romdump")
    with LOCK:
        if not os.path.isdir(out):
            tool("rom-asset-tool.py", "--map", ROM_MAP, "--output-dir", out + ".tmp", "dump", CONFIG["rom"])
            os.rename(out + ".tmp", out)
    import yaml
    sub = os.listdir(out)[0]
    manifest = yaml.safe_load(open(os.path.join(out, sub, "manifest.yaml")))
    keys = {}
    for k, v in manifest.items():
        if isinstance(v, str) and (v.startswith("objbank/") or v.startswith("texbank/")):
            keys.setdefault(os.path.basename(v).split(".")[0], k)
    return keys


# ---------------------------------------------------------------------------------------------
# Custom models: glTF models the author imports (tools/custom_model.py). They live in the user folder, and ROM
# builds copy them into the level's own banks.

CUSTOM = os.path.join(HOME, "custom")
CUSTOM_LOCK = threading.Lock()
CUSTOM_SET = re.compile(r"[0-9a-f]{10}")
MAX_MODEL = 64 << 20


def custom_sets():
    """{set: meta.json} for every imported model"""
    out = {}
    if os.path.isdir(CUSTOM):
        for d in sorted(os.listdir(CUSTOM)):
            path = os.path.join(CUSTOM, d, "meta.json")
            if CUSTOM_SET.fullmatch(d) and os.path.isfile(path):
                out[d] = json.load(open(path))
    return out


def custom_catalog():
    """Entries like catalog()'s for every piece of every imported model, with the paths of its bank and of its
    texture bank"""
    cat = {}
    for s, meta in custom_sets().items():
        n = len(meta["pieces"])
        for i, p in enumerate(meta["pieces"]):
            cat[p["id"]] = {"name": meta["name"] if n == 1 else "%s %d/%d" % (meta["name"], i + 1, n),
                            "bank": "Custom: " + meta["name"],
                            "url": "/custom/%s/preview/%s" % (s, quote(p["preview"])) if p["preview"] else None,
                            "bytes": p["bytes"], "custom": s, "piece": p["name"], "offset": p["offset"],
                            "textures": p["textures"], "path": os.path.join(CUSTOM, s, p["id"] + ".obj.bin"),
                            "texBank": "custom:" + s, "reskinFrom": (meta.get("reskin") or {}).get("from")}
    return cat


def import_custom(data, name, scale):
    """Converts a glTF model into a new set of custom models and returns the set's ID"""
    s = hashlib.sha1(data + repr(scale).encode()).hexdigest()[:10]
    final = os.path.join(CUSTOM, s)
    with CUSTOM_LOCK:
        if not os.path.isfile(os.path.join(final, "meta.json")):
            custom = custom_catalog()
            taken = {int(i, 16) for i in list(catalog()) + list(custom)}
            taken_tex = {int(t, 16) for b in ALL_TEX_BANKS for t in texture_ids(b)}
            taken_tex |= {int(t, 16) for c in custom.values() for t in c["textures"]}
            tmp = final + ".tmp"
            shutil.rmtree(tmp, ignore_errors=True)
            shutil.rmtree(final, ignore_errors=True)
            try:
                custom_model.convert(data, tmp, tool, name, scale, taken, taken_tex)
            except BaseException:
                shutil.rmtree(tmp, ignore_errors=True)
                raise
            os.rename(tmp, final)
    return s


WORLDS = ("ATLANTIS", "CARNIVAL", "PIRATES", "PREHISTORIC", "FORTRESS", "OOTW", "HUB")


def reskin_pool(worlds):
    """Texture IDs from these worlds' texture banks that no other bank holds. The same ID can be a different
    picture in another world, and a level that has the ID already shows its own picture."""
    count = {}
    for b in ALL_TEX_BANKS:
        for t in texture_ids(b):
            count[t] = count.get(t, 0) + 1
    out = []
    for w in worlds:
        bank = w + "_TEX_BANK.tex.fla"
        if bank in ALL_TEX_BANKS:
            out += sorted(t for t in texture_ids(bank) if count.get(t) == 1)
    return out


def reskin(oid, worlds, seed=0):
    """A copy of retail model oid with each texture swapped for one from the chosen worlds, kept as a custom model
    (CUSTOM/<set>: meta.json, the bank, textures.tex.bin, preview/). Returns the set. The IDs are
    replaced in one pass."""
    import struct
    worlds = sorted(worlds)
    rnd = random.Random("%s %s %s" % (oid, worlds, seed))
    pool = reskin_pool(worlds)
    olds = sorted(object_texture_ids(oid))
    if not pool or not olds:
        raise ValueError("no textures to swap")
    swap = {t: rnd.choice(pool) for t in olds}
    key = json.dumps([oid, sorted(swap.items())]).encode()
    sid = hashlib.sha1(b"reskin" + key).hexdigest()[:10]
    final = os.path.join(CUSTOM, sid)
    with CUSTOM_LOCK:
        if os.path.isfile(os.path.join(final, "meta.json")):
            return sid
        data = open(model_bank(oid)[0], "rb").read()
        taken = {int(i, 16) for i in list(catalog()) + list(custom_catalog())}
        new = rnd.getrandbits(32)
        while new in taken or new >> 24 == 0x7B:   # objbank-tool reads a file that starts with '{' as glTF
            new = rnd.getrandbits(32)
        new_id = "0x%08X" % new
        old_b = int(oid, 16).to_bytes(4, "big")
        if data.count(old_b) != 2:
            raise ValueError("the object ID of %s isn't where it should be" % oid)
        words = {old_b: new.to_bytes(4, "big")}
        for a, b in swap.items():
            ab = int(a, 16).to_bytes(4, "big")
            if data.count(ab) < 1:
                raise ValueError("texture %s isn't in %s" % (a, oid))
            words[ab] = int(b, 16).to_bytes(4, "big")
        # One pass: a new ID that equals another old one must not be replaced again
        data = re.sub(b"|".join(re.escape(k) for k in words), lambda m: words[m.group(0)], data)
        texs = sorted(set(swap.values()))
        world = catalog()[oid]["world"]
        recs = {}
        for t in texs:
            bank = next(b for b in ALL_TEX_BANKS if t in texture_ids(b))
            recs[t] = dict(texbank_records(raw_bank(bank))[0])[t]
        tmp = final + ".tmp"
        shutil.rmtree(tmp, ignore_errors=True)
        os.makedirs(tmp)
        bank_path = os.path.join(tmp, new_id + ".obj.bin")
        open(bank_path, "wb").write(data)
        texbank = os.path.join(tmp, "textures.tex.bin")
        open(texbank, "wb").write(struct.pack(">I", len(texs)) + b"".join(recs[t] for t in texs) +
                                  b"".join(b"%s.png\0" % t.encode() for t in texs))
        preview = os.path.join(tmp, "preview")
        tool("objbank-tool.py", "unpack", "--scale", "1", "--output-dir", preview, "-t", texbank, bank_path)
        found = [f for f in os.listdir(preview) if f.lower().startswith(new_id.lower()) and f.endswith(".glb")]
        name = catalog()[oid]["name"]
        meta = {"name": "%s (reskin)" % name, "scale": 1.0, "triangles": 0, "size": [0, 0, 0],
                "reskin": {"from": oid, "worlds": worlds, "seed": seed, "world": world},
                "pieces": [{"id": new_id, "name": name[:7], "offset": [0, 0, 0], "triangles": 0, "size": [0, 0, 0],
                            "textures": texs, "bytes": len(data), "preview": found[0] if found else ""}],
                "textures": {t: len(recs[t]) for t in texs}}
        json.dump(meta, open(os.path.join(tmp, "meta.json"), "w"), indent=1)
        os.rename(tmp, final)
    return sid


def model_bank(oid):
    """A model's single-object bank and its size"""
    c = custom_catalog().get(oid)
    if c:
        return c["path"], c["bytes"]
    single = split_bank(catalog()[oid]["bank"])[oid]
    return single["path"], single["bytes"]


def loaded_ids(sid):
    """Objects the game is sure to have loaded for this level: banks both the game's LoadLevel and noclip list."""
    ids = set()
    for b in SCENES[sid]["sureObjectBanks"]:
        ids |= set(split_bank(b.split(".")[0]))
    return ids


def plan_imports(sid, imports):
    """What a ROM build has to add so this level can draw `imports` (object IDs from other banks).
    Targets are the banks the game loads for this level (LoadLevel): its own object bank, loaded last, and
    its last texture bank."""
    scene = SCENES[sid]
    loaded = loaded_ids(sid)
    custom = custom_catalog()
    cat = {**catalog(), **custom}
    objs = [o for o in dict.fromkeys(imports) if o not in loaded]
    for o in objs:
        if o not in cat:
            raise ValueError("Object %s isn't in any of the game's object banks or the imported models" % o)
    have_tex = set().union(*(texture_ids(t) for t in scene["sureTextureBanks"]))
    need_tex = sorted(set().union(*(object_texture_ids(o) for o in objs)) - have_tex) if objs else []
    # A custom model's textures come from its own texture bank
    tex_src = {t: custom[o]["texBank"] for o in objs if o in custom for t in custom[o]["textures"] if t in need_tex}
    for t in need_tex:
        if t in tex_src:
            continue
        for bank in ALL_TEX_BANKS:
            if t in texture_ids(bank):
                tex_src[t] = bank
                break
    return {"objects": objs, "targetBank": scene["importObjectBank"],
            "textures": tex_src, "targetTexBank": scene["importTextureBank"],
            "objectBytes": sum(model_bank(o)[1] for o in objs)}


def build_import_banks(plan, tmp):
    """Write the enlarged object/texture banks; return manifest lines."""
    lines, keys = [], rom_keys()
    if plan["objects"]:
        singles = [model_bank(o)[0] for o in plan["objects"]]
        out = os.path.join(tmp, plan["targetBank"] + ".obj.bin")
        # Raw (uncompressed) bank: the game's DMAGetSize/DMAMemory accept both FLA2 and raw data.
        tool("objbank-tool.py", "pack", "--output-file", out, os.path.join(DATA, plan["targetBank"] + ".obj.fla"), *singles)
        lines.append('"%s": %s' % (keys[plan["targetBank"]], os.path.basename(out)))
    if plan["textures"]:
        import struct
        recs, names = texbank_records(raw_bank(plan["targetTexBank"] + ".tex.fla"))
        extra = []
        for tid, bank in plan["textures"].items():
            src = dict(texbank_records(raw_bank(bank))[0])
            extra.append(src[tid])
        data = struct.pack(">I", len(recs) + len(extra)) + b"".join(r for _, r in recs) + b"".join(extra)
        data += b"".join(n + b"\0" for n in names) + b"".join(b"%s.png\0" % t.encode() for t in plan["textures"])
        out = os.path.join(tmp, plan["targetTexBank"] + ".tex.bin")
        open(out, "wb").write(data)
        lines.append('"%s": %s' % (keys[plan["targetTexBank"]], os.path.basename(out)))
    plan["texBytes"] = sum(len(dict(texbank_records(raw_bank(b))[0])[t]) for t, b in plan["textures"].items())
    return lines


_bank_numbers = None


def bank_numbers():
    """{"obj": {stem: number}, "tex": {stem: number}}: the game's bank numbers, from the switch statements in
    LoadObjectBank and LoadTextureBank (gloveworld src/banks.c). Cached in cache/."""
    global _bank_numbers
    if _bank_numbers is None:
        path = os.path.join(CACHE, "bank_numbers.json")
        if not os.path.isfile(path):
            from game_banks import bank_names
            from build_xref import parse_enums
            gw = CONFIG["gloveworld"]
            src = open(os.path.join(gw, "src", "banks.c")).read()
            consts = {k: v for e in parse_enums(os.path.join(gw, "include", "enums.h")).values() for k, v in e.items()}
            nums = {"obj": {}, "tex": {}}
            for kind, func in (("obj", "LoadObjectBank"), ("tex", "LoadTextureBank")):
                for n, name in sorted(bank_names(src, func, consts).items(), reverse=True):
                    nums[kind][name] = n  # LEVELEND_TEX_BANK is 1 and 14; the lower number wins
            json.dump(nums, open(path, "w"), indent=1)
        _bank_numbers = json.load(open(path))
    return _bank_numbers


def plan_runtime(sid, imports):
    """With the game mod, imports are not copied into the ROM: the mod loads the banks they come from, from the
    player's own ROM, when the level starts (mod/banks.c). This picks those banks: each model's object bank, and
    texture banks for the textures the level's own banks lack, preferring the model's own world. Custom models are
    not in the ROM: plan["copy"] is the part of the plan that copies them into the level's banks."""
    plan = plan_imports(sid, imports)
    scene, cat, nums, custom = SCENES[sid], catalog(), bank_numbers(), custom_catalog()
    objs = [o for o in plan["objects"] if o not in custom]
    copied = [o for o in plan["objects"] if o in custom]
    custom_tex = {t for o in copied for t in custom[o]["textures"]}
    plan["copy"] = {"objects": copied, "textures": {t: b for t, b in plan["textures"].items() if t in custom_tex},
                    "targetBank": plan["targetBank"], "targetTexBank": plan["targetTexBank"],
                    "objectBytes": sum(model_bank(o)[1] for o in copied)}
    plan["copy"]["texBytes"] = sum(len(dict(texbank_records(raw_bank(b))[0])[t]) for t, b in plan["copy"]["textures"].items())
    obj_banks = [b for b in dict.fromkeys(cat[o]["bank"] for o in objs) if b not in scene["gameObjectBanks"]]
    loaded_tex = {t.split(".")[0] for t in scene["gameTextureBanks"]}
    need = set(plan["textures"]) - custom_tex
    candidates = [cat[o]["world"] + "_TEX_BANK" for o in objs] + [b.split(".")[0] for t, b in plan["textures"].items() if t in need]
    tex_banks = []
    for b in dict.fromkeys(candidates):
        # The font bank always goes to slot 0. It can't be loaded as an extra bank.
        if b in loaded_tex or b not in nums["tex"] or b == "FONT_TEX_BANK":
            continue
        have = texture_ids(b + ".tex.fla") & need
        if have:
            tex_banks.append(b)
            need -= have
    if need:
        raise ValueError("No texture bank has textures %s" % ", ".join(sorted(need)))
    plan["runtime"] = {
        "level": scene["index"],
        "objectBanks": obj_banks, "textureBanks": tex_banks,
        "objectNumbers": [nums["obj"][b] for b in obj_banks], "textureNumbers": [nums["tex"][b] for b in tex_banks],
        "bytes": sum(len(raw_bank(b + ".obj.fla")) for b in obj_banks) + sum(len(raw_bank(b + ".tex.fla")) for b in tex_banks)}
    plan["texBytes"] = 0
    return plan


MAX_XML = 8 << 20
# Level XML never needs a DTD, entities or processing instructions; refusing them rules out entity
# expansion and external-entity (XXE) reads before lxml sees the document.
SAFE_PARSER = ET.XMLParser(resolve_entities=False, no_network=True, load_dtd=False, dtd_validation=False,
                           remove_pis=True, remove_comments=True, huge_tree=False)
_FORBIDDEN = re.compile(rb"<!DOCTYPE|<!ENTITY|<\?xml-stylesheet|SYSTEM\s*[\"']|<xi:|xmlns:xi", re.I)
IMPORT_ID = re.compile(r"0x[0-9A-F]{8}")


def parse_xml_safely(xml_bytes):
    if len(xml_bytes) > MAX_XML:
        raise ValueError("Level XML is larger than %d MB" % (MAX_XML >> 20))
    if _FORBIDDEN.search(xml_bytes):
        raise ValueError("Level XML may not contain a DOCTYPE, entities, stylesheets or XInclude")
    try:
        tree = ET.parse(io.BytesIO(xml_bytes), SAFE_PARSER)
    except ET.XMLSyntaxError as e:
        raise ValueError("Level XML is not well-formed: %s" % e)
    if tree.getroot().tag != "Level":
        raise ValueError("Root element must be <Level>")
    return tree


# Objects whose SetActorRotation/SetActorScale only the game mod (mod/transforms.c) applies
MOD_PARENTS = {"Enemy", "Platform", "NullPlatform", "Powerup", "Water"}


def needs_mod(tree):
    return any(e.getparent() is not None and e.getparent().tag in MOD_PARENTS
               for e in tree.getroot().iter("SetActorRotation", "SetActorScale"))


def assemble(xml_bytes):
    """The level as .lev bytes, and whether its rotations and scales need the Expansion Pak game mod."""
    tree = parse_xml_safely(xml_bytes)
    if not SCHEMA.validate(tree.getroot()):
        errs = [str(e) for e in SCHEMA.error_log.filter_from_errors()][:20]
        raise ValueError("Level XML does not match libgarib's schema:\n" + "\n".join(errs))
    return xmlToLandscape(tree), needs_mod(tree)


def build_rom(sid, lev, quickboot, imports=(), mod=False, copy=False, info=None):
    """One level: see build_levels"""
    rom, plans = build_levels([{"sid": sid, "lev": lev, "imports": imports, "info": info}], sid if quickboot else None, mod, copy)
    return rom, plans[sid]


def build_levels(levels, quickboot=None, mod=False, copy=False, entry=None):
    """A ROM with every level in levels replaced: [{sid, lev, imports, info, extra}], info being the level's name,
    song and bonus clock for the level table (None keeps them). quickboot: the sid to boot into, or None (with an
    extra level, ("x", sid)).

    An entry with extra set is an extra level: it is stored behind the game mod in its Space slot (35 to 41) and
    loads when the player comes in through the entry zone (mod/slots.c). entry is {"hub": 0-7 or 255, "tag": n}.

    With the game mod, imported models are loaded from the player's ROM at run time (plan_runtime), unless copy is
    set. Otherwise they are copied into the level's object and texture banks (build_import_banks); with the mod
    too, they then still get its larger heap. Two levels can't both copy models into the same bank."""
    if len({(e["sid"], bool(e.get("extra"))) for e in levels}) != len(levels):
        raise ValueError("a level is in the build twice")
    slots = {}
    if any(e.get("extra") for e in levels):
        mod = True
    with tempfile.TemporaryDirectory(dir=os.path.join(CACHE)) as tmp:
        lines, plans, extra, banks = [], {}, {}, {}
        for e in levels:
            sid = e["sid"]
            index = SCENES[sid]["index"]
            if e.get("extra"):
                if not 35 <= index <= 41 or index in (39, 40):
                    raise ValueError("%s can't be an extra slot: only Space 1, 2, 3, its boss and its bonus can." % SCENES[sid]["name"])
                info = e.get("info") or {}
                slots[index] = {"lev": e["lev"], "name": info.get("name") or SCENES[sid]["name"].upper()[:39],
                                "song": info.get("song"), "bonus_time": info.get("bonus_time")}
            else:
                name = "edited_%s.lev" % sid
                with open(os.path.join(tmp, name), "wb") as f:
                    f.write(e["lev"])
                lines.append("\"('landscape', 'Landscapes', %d)\": %s" % (index, name))
            imports = list(e.get("imports") or ())
            if mod and not copy:
                plan = plan_runtime(sid, imports)
                into = plan["copy"]
                rt = plan.get("runtime")
                if rt and (rt["objectNumbers"] or rt["textureNumbers"]):
                    # An extra level's banks are keyed with its slot + 0x40 (mod/slots.c Mod_SlotLevel)
                    extra[rt["level"] + (0x40 if e.get("extra") else 0)] = (rt["textureNumbers"], rt["objectNumbers"])
            else:
                plan = into = plan_imports(sid, imports)
            if into["objects"]:
                for b in (into["targetBank"], into["targetTexBank"]):
                    if b in banks:
                        raise ValueError("%s and %s both copy models into %s. Build them into separate ROMs, or let the "
                                         "Expansion Pak game mod load the models from the ROM." % (
                                             SCENES[banks[b]]["name"], SCENES[sid]["name"], b))
                    banks[b] = sid
                lines += build_import_banks(into, tmp)
            else:
                into.setdefault("texBytes", 0)   # build_import_banks sets it; the callers read it
            plans[("x" + sid) if e.get("extra") else sid] = plan
        with open(os.path.join(tmp, "manifest.yaml"), "w") as f:
            f.write("".join(line + "\n" for line in lines))
        args = ["--map", ROM_MAP, "--output-dir", tmp, "patch", "--manifest", os.path.join(tmp, "manifest.yaml")]
        boot_extra = isinstance(quickboot, (tuple, list))
        if quickboot:
            qsid = quickboot[1] if boot_extra else quickboot
            args += ["--qol", "copyskip", "--qol", "quickboot(level_id=0x%x)" % SCENES[qsid]["index"]]
        rom_copy = os.path.join(tmp, "glover.z64")
        shutil.copy(CONFIG["rom"], rom_copy)
        tool("rom-asset-tool.py", *args, rom_copy)
        with open(os.path.join(tmp, "glover.patched.z64"), "rb") as f:
            rom = f.read()
        if mod:
            try:
                entry = entry or {}
                rom = apply_mod(rom, extra or None, slots={"levels": slots, "boot": boot_extra, "entry_hub": entry.get("hub", 255),
                                                          "entry_tag": entry.get("tag", 99)} if slots else None)
            except ModError as e:
                raise ValueError("Could not add the Expansion Pak game mod: %s" % e)
        rows = {SCENES[e["sid"]]["index"]: e["info"] for e in levels if e.get("info") and not e.get("extra")}
        if rows:
            try:
                rom = apply_level_info(rom, rows)
            except ModError as e:
                raise ValueError("Could not set the level's name, music or clock: %s" % e)
        # Every built ROM, and so every patch, walks the ball with Easy's controls (not inverted). This also
        # recomputes the header checksum.
        try:
            rom = apply_easy_controls(rom)
        except ModError as e:
            raise ValueError("Could not set Easy's controls: %s" % e)
        return rom, plans


KIT_PREVIEWS = {}


def kit_preview(item, params):
    key = json.dumps([item, params], sort_keys=True)
    if key not in KIT_PREVIEWS:
        if len(KIT_PREVIEWS) > 200:
            KIT_PREVIEWS.clear()
        KIT_PREVIEWS[key] = kit.preview_png(item, params)
    return KIT_PREVIEWS[key]


def import_kit(item, params):
    """Builds a kit model and converts it like an imported model. The same settings give the same set."""
    params = kit.clean(item, params)
    return import_custom(kit.build(item, params).glb(), kit.label(item, params), 1.0)
