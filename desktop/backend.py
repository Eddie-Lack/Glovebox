"""The editor's back end: core.py's functions (libgarib, the model catalog, imports, ROM and patch builds), called
in this process.

A model is named by a path (/models/BANK@WORLDS/FILE.glb, /custom/SET/preview/FILE.glb); fetch() reads the
file a path names, and a model's textures are found next to it the same way.
"""
import hashlib, json, os, re, shutil, sys, time
from urllib.parse import unquote

HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


class Backend:
    def __init__(self):
        sys.path.insert(0, HERE)
        import core   # reads config.json and the prepared tables from GLOVEBOX_HOME
        self.s = core

    # -- Tables

    def scenes(self):
        return list(self.s.SCENES.values())

    def _generated(self, name, default=None):
        path = os.path.join(self.s.GENERATED, name)
        if not os.path.isfile(path):
            path = os.path.join(self.s.WEB, name)
        try:
            with open(path, encoding="utf-8") as f:
                return json.load(f)
        except (OSError, ValueError):
            if default is None:
                raise
            return default

    def decomp(self):
        return self._generated("decomp.json")

    def tables(self):
        return self._generated("noclip_tables.json")

    def templates(self):
        return self._generated("enemy_templates.json", {})

    def heap(self):
        return self._generated("heap.json", {})

    def mod_info(self):
        info = json.load(open(os.path.join(HERE, "mod", "mod.json")))
        return {"heapExtraBytes": info["heap_extra_bytes"], "modBytes": info["mod_bytes"]}

    # -- Levels and models

    def level_xml(self, sid):
        return self.s.level_xml(sid).decode("utf-8")

    def backdrops(self, sid):
        """Backdrop textures this level can show: the texture ids that the shipped levels use as backdrops and that
        are in this level's texture banks, most used first"""
        import collections
        from lxml import etree
        if not hasattr(self, "_backdrops"):
            used = collections.Counter()
            for other in self.s.SCENES:
                try:
                    root = etree.fromstring(self.s.level_xml(other))
                except Exception:
                    continue
                for b in root.iter("Backdrop"):
                    used[(b.get("texture_id") or "").upper().replace("0X", "0x")] += 1
            self._backdrops = used
        have = set()
        for bank in self.s.SCENES[sid]["textureBanks"]:
            have |= self.s.texture_ids(bank)
        return [t for t, _ in self._backdrops.most_common() if t in have]

    def faces(self, oid):
        """The number of collision faces of a model: the faces of its root mesh, which the game turns into planes for
        a landscape actor (parse.c, Obj_Planes). Cached."""
        if not hasattr(self, "_faces"):
            self._faces = {}
        if oid not in self._faces:
            # Read from the bank's bytes (glover.objbank.ksy): the first directory entry points to the object, the
            # object (+12) to its root mesh, the mesh (+20) to its geometry, which starts with num_faces. Importing
            # libgarib's objbank parser here would break its level parser: both patch the same base class.
            import struct
            try:
                path, _ = self.s.model_bank(oid)
                with open(path, "rb") as f:
                    d = f.read()
                u4 = lambda at: struct.unpack_from(">I", d, at)[0]
                obj = u4(4)
                mesh = u4(obj + 12) if obj else 0
                geo = u4(mesh + 20) if mesh else 0
                self._faces[oid] = struct.unpack_from(">H", d, geo)[0] if geo else 0
            except Exception:
                self._faces[oid] = 0
        return self._faces[oid]

    def models(self, sid):
        return {"models": self.s.model_index(sid), "loaded": sorted(self.s.loaded_ids(sid))}

    def orig_lev_size(self, sid):
        path = os.path.join(self.s.DATA, self.s.SCENES[sid]["landscape"])
        return os.path.getsize(path) if os.path.isfile(path) else None

    def catalog(self):
        """Every model in the game's banks and the author's imported models. The first call converts every object
        bank and takes minutes; later calls read the cache."""
        custom = {k: {f: v for f, v in c.items() if f != "path"} for k, c in self.s.custom_catalog().items()}
        return {**self.s.catalog(), **custom}

    def fetch(self, url):
        """The bytes of a model or texture, by the path the catalog gives"""
        s = self.s
        p = url.split("?")[0]
        m = re.fullmatch(r"/custom/([0-9a-f]{10})/preview/(?:([\w\-\.]+\.glb)|textures/(0x[0-9A-F]{8}\.png))", p)
        if m:
            path = (os.path.join(s.CUSTOM, m.group(1), "preview", m.group(2)) if m.group(2)
                    else os.path.join(s.CUSTOM, m.group(1), "textures", m.group(3)))
            return open(path, "rb").read()
        m = re.fullmatch(r"/models/([\w@+\- %]+)/textures/(0x[0-9A-Fa-f]{8}\.png)", p)
        if m:
            path = s.find_texture(unquote(m.group(1)), m.group(2).upper().replace("0X", "0x").replace(".PNG", ".png"))
            if not path:
                raise FileNotFoundError("texture not in this level's banks: %s" % p)
            return open(path, "rb").read()
        m = re.fullmatch(r"/models/([\w@+\- %]+)/([\w\-\.]+\.glb)", p)
        if m:
            return open(os.path.join(s.CACHE, "models", unquote(m.group(1)), m.group(2)), "rb").read()
        raise FileNotFoundError(p)

    def validate(self, xml):
        """libgarib's schema errors for a level's XML (at most 5), or []"""
        s = self.s
        try:
            tree = s.parse_xml_safely(xml.encode("utf-8") if isinstance(xml, str) else xml)
        except ValueError as e:
            return [str(e)]
        if s.SCHEMA.validate(tree.getroot()):
            return []
        return [str(e) for e in s.SCHEMA.error_log.filter_from_errors()][:5]

    def plan(self, sid, ids):
        s = self.s
        plan = s.plan_runtime(sid, ids) if ids else s.plan_imports(sid, ids)
        plan["texBytes"] = sum(len(dict(s.texbank_records(s.raw_bank(b))[0])[t]) for t, b in plan["textures"].items())
        return plan

    # -- Building

    def build_all(self, entries, fmt, quickboot=None, expansion=False, copy=False):
        """Several levels in one ROM or BPS patch. entries: [{sid, xml, imports, info}]. quickboot: a sid or None.
        Returns (bytes, headers)."""
        s = self.s
        if fmt not in ("rom", "bps"):
            raise ValueError("format must be rom or bps")
        levels, mod = [], expansion
        for e in entries:
            lev, needs = s.assemble(e["xml"])
            mod = mod or needs
            imports = list(e.get("imports") or ())
            if len(imports) > 256 or not all(s.IMPORT_ID.fullmatch(i) for i in imports):
                raise ValueError("imports must be a list of object IDs like 0x73DB3DE3")
            levels.append({"sid": e["sid"], "lev": lev, "imports": imports, "info": e.get("info"), "extra": bool(e.get("extra"))})
            mod = mod or bool(e.get("extra"))
        if quickboot and any(e["sid"] == quickboot and e.get("extra") for e in levels):
            quickboot = ("x", quickboot)
        rom, plans = s.build_levels(levels, quickboot, mod, copy, {"hub": 255, "tag": 99})
        h = {"X-Needs-Mod": "1" if mod else "0", "levels": [e["sid"] for e in levels],
             "extra": [e["sid"] for e in levels if e["extra"]],
             "objects": sum(len(p["objects"]) for p in plans.values())}
        if fmt == "rom":
            h["filename"] = "glover-%d-levels.z64" % len(levels)
            return rom, h
        clean = open(s.CONFIG["rom"], "rb").read()
        meta = json.dumps({"tool": "Glovebox", "game": "Glover (NGVE)", "sourceSha1": hashlib.sha1(clean).hexdigest(),
                           "targetSha1": hashlib.sha1(rom).hexdigest(), "levels": [s.SCENES[e["sid"]]["name"] for e in levels],
                           "levelIds": [e["sid"] for e in levels], "quickboot": quickboot, "needsExpansionPak": mod,
                           "created": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}, separators=(",", ":"))
        patch = s.bps.create(clean, rom, meta.encode())
        h["filename"] = "glover-%d-levels.bps" % len(levels)
        h["X-Patch-Size"] = str(len(patch))
        return patch, h

    def loaded_ids(self, sid):
        return set(self.s.loaded_ids(sid))

    def build(self, sid, xml, fmt, quickboot=False, imports=(), expansion=False, copy=False, info=None):
        """The level as .lev, ROM or BPS patch bytes, and facts about it as a dict of headers (plus "filename").
        Raises ValueError with a message for the author."""
        s = self.s
        lev, mod = s.assemble(xml)
        if fmt == "lev":
            return lev, {"filename": s.SCENES[sid]["landscape"], "X-Lev-Size": str(len(lev)), "X-Needs-Mod": "1" if mod else "0"}
        if fmt not in ("rom", "bps"):
            raise ValueError("format must be lev, rom or bps")
        imports = list(imports)
        if len(imports) > 256 or not all(s.IMPORT_ID.fullmatch(i) for i in imports):
            raise ValueError("imports must be a list of object IDs like 0x73DB3DE3")
        mod = mod or expansion
        rom, plan = s.build_rom(sid, lev, quickboot, imports, mod, copy, info)
        h = {"X-Lev-Size": str(len(lev)), "X-Needs-Mod": "1" if mod else "0",
             "X-Imports": {"objects": len(plan["objects"]), "textures": len(plan["textures"]), "objectBytes": plan["objectBytes"],
                           "texBytes": plan["texBytes"], "bank": plan["targetBank"], "texBank": plan["targetTexBank"],
                           "runtime": len(plan["runtime"]["objectBanks"]) + len(plan["runtime"]["textureBanks"]) if "runtime" in plan else 0}}
        if fmt == "rom":
            h["filename"] = "glover-%s-edited.z64" % sid
            return rom, h
        # BPS patch against the clean ROM. Plain edits come out as a true diff (relocated data is encoded as copies
        # from the player's ROM). With the mod, imports load from the player's ROM at run time. The patch has no
        # game data. Without it, imports rebuild the level's object and texture banks uncompressed, which the clean
        # ROM only has FLA2-compressed. Those banks travel in full.
        clean = open(s.CONFIG["rom"], "rb").read()
        meta = json.dumps({"tool": "Glovebox", "game": "Glover (NGVE)", "sourceSha1": hashlib.sha1(clean).hexdigest(),
                           "targetSha1": hashlib.sha1(rom).hexdigest(), "level": s.SCENES[sid]["name"], "levelId": sid,
                           "quickboot": quickboot, "importedModels": plan["objects"], "needsExpansionPak": mod,
                           "created": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}, separators=(",", ":"))
        patch = s.bps.create(clean, rom, meta.encode())
        slug = re.sub(r"[^A-Za-z0-9]+", "-", s.SCENES[sid]["name"]).strip("-").lower()
        h["filename"] = "glover-%s-%s.bps" % (sid, slug)
        h["X-Patch-Size"] = str(len(patch))
        return patch, h

    # -- Imported models and the kit

    def import_custom(self, data, name, scale):
        s = self.s
        if len(data) > s.MAX_MODEL:
            raise ValueError("The model is larger than %d MB" % (s.MAX_MODEL >> 20))
        name = re.sub(r"\s+", " ", name).strip()[:40] or "Model"
        set_ = s.import_custom(data, name, float(scale))
        return {"set": set_, **s.custom_sets()[set_]}

    def delete_custom(self, set_):
        s = self.s
        if not s.CUSTOM_SET.fullmatch(set_):
            raise ValueError("no such model")
        with s.CUSTOM_LOCK:
            shutil.rmtree(os.path.join(s.CUSTOM, set_), ignore_errors=True)

    def reskin(self, oid, worlds, seed=0):
        """A copy of a retail model with textures from other worlds, kept with the imported models. Returns its
        object ID."""
        sid = self.s.reskin(oid, worlds, seed)
        return next(i for i, c in self.s.custom_catalog().items() if c["custom"] == sid)

    def kit_items(self):
        return self.s.kit.items()

    def kit_preview(self, item, params):
        return self.s.kit_preview(item, self.s.kit.clean(item, {k: str(v) for k, v in params.items()}))

    def make_kit(self, item, params):
        set_ = self.s.import_kit(item, params)
        return {"set": set_, **self.s.custom_sets()[set_]}

    def rom_path(self):
        return self.s.CONFIG["rom"]
