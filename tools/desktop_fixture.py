#!/usr/bin/env python3
"""A stand-in for the desktop editor's back end that needs no ROM: two made-up levels, the kit's models and the test
map (tools/make_test_model.py), the real decomp cross-reference (app/decomp/decomp.json) and noclip's tables.

tools/desktop_test.py runs the editor against it. To look at the editor without a ROM:

    python3 -m desktop --fixture

Building a .lev works (libgarib assembles the XML); building a ROM or a patch needs the ROM and says so.
"""
import json, os, sys

HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(HERE, "tools"))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "vendor", "libgarib", "python"))

GROUND, BLOCK, GRAVE, PUMPKIN, SIGN, BALL, ENEMY = ("0x1E5700A1", "0x1E5700A2", "0x1E5700A3", "0x1E5700A4", "0x1E5700A5",
                                                    "0x8E4DDE49", "0x1E5700A6")


def scene(sid, name, group="Atlantis"):
    return {"id": sid, "index": int(sid, 16), "name": name, "group": group, "landscape": "%s.FIX%slnd.n64.lev" % (sid, sid),
            "objectBanks": ["FIXTURE_BANK.obj.fla"], "textureBanks": ["FIXTURE_TEX_BANK.tex.fla"],
            "gameObjectBanks": ["FIXTURE_BANK"], "gameTextureBanks": ["FIXTURE_TEX_BANK"],
            "sureObjectBanks": ["FIXTURE_BANK.obj.fla"], "sureTextureBanks": ["FIXTURE_TEX_BANK.tex.fla"],
            "importObjectBank": "FIXTURE_BANK", "importTextureBank": "FIXTURE_TEX_BANK"}


LEVEL = """<Level name="FIX10.lnd" libgarib-version="0.0" data-format-version="1">
  <FogConfiguration fog_enabled="1" r="70" g="110" b="150" fog_distance="900" near_clip="1500"/>
  <AmbientLight r="120" g="120" b="130"/>
  <LandActor object_id="{ground}" name="ground" x="0.0" y="0.0" z="0.0"/>
  <LandActor object_id="{grave}" name="grave" x="150.0" y="40.0" z="-120.0">
    <SetActorRotation x="0.0" y="0.6" z="0.0"/>
  </LandActor>
  <BackgroundActor object_id="{pumpkin}" name="pumpkin" x="-200.0" y="45.0" z="80.0"/>
  <Platform object_id="{block}" name="lift">
    <PlatSetTag tag="7"/>
    <PlatScale x="0.2" y="0.1" z="0.2"/>
    <PlatPathPoint duration="60" x="300.0" y="80.0" z="200.0"/>
    <PlatPathPoint duration="60" x="300.0" y="260.0" z="200.0"/>
    <PlatCheckpoint u16_0x17="0" theta="1.2"/>
  </Platform>
  <Platform object_id="{block}" name="spinner">
    <PlatSetInitialPos x="-320.0" y="90.0" z="-260.0"/>
    <PlatScale x="0.3" y="0.05" z="0.3"/>
    <PlatConstantSpin axis="y" initial_theta="0.0" speed="0.05"/>
  </Platform>
  <GaribGroup group_id="0" initial_state="1">
    <Garib x="0.0" y="80.0" z="0.0" type="garib" dynamic_shadow="0"/>
    <Garib x="60.0" y="80.0" z="0.0" type="garib" dynamic_shadow="0"/>
    <Garib x="120.0" y="80.0" z="0.0" type="extra_life" dynamic_shadow="0"/>
  </GaribGroup>
  <Enemy type="dennis" puzzle_tag="0" x="-120.0" y="60.0" z="-80.0" y_rotation="0.0">
    <EnemyFinalize/>
  </Enemy>
  <Powerup type="7" u16_0x02="300" u16_0x04="300" x="200.0" y="70.0" z="40.0"/>
  <MrTip x="-60.0" y="90.0" z="160.0" message_id="1"/>
  <GloverSpawnPoint x="0.0" y="70.0" z="250.0"/>
  <BallSpawnPoint type="0" x="30.0" y="70.0" z="250.0"/>
  <Wind left="-400.0" top="40.0" front="300.0" width="150.0" height="120.0" depth="150.0" vel_x="0.0" vel_y="2.0" vel_z="0.0" turbulence="0.0" unknown_0x2c="0" active="1" tag="0"/>
  <Buzzer tag="0" platform_1_tag="0" platform_2_tag="0" draw_flags="0" r="255" g="255" b="0" color_jitter="0" end_1_x="-450.0" end_1_y="60.0" end_1_z="-100.0" end_2_x="-450.0" end_2_y="60.0" end_2_z="100.0" draw_diameter="10.0" draw_thickness="2.0"/>
  <Puzzle>
    <PuzzleCond cond_type="35">
      <PuzzleCondGloverWithinVolume x="-100.0" y="0.0" z="350.0" l="300.0" w="400.0" h="300.0"/>
    </PuzzleCond>
    <PuzzleAction action_type="74">
      <PuzzleActionCameraLookAtGlover angle="3.1416" distance="2.0" activation_delay="0"/>
    </PuzzleAction>
  </Puzzle>
  <Puzzle>
    <PuzzleCond cond_type="38">
      <PuzzleCondBallWithinRangeOfPoint x="400.0" y="60.0" z="-300.0" range="80.0"/>
    </PuzzleCond>
    <PuzzleCond cond_type="41">
      <PuzzleCondGloverWithinRangeOfPoint2 x="420.0" z="-150.0" range="60.0" max_y="160.0" min_y="20.0"/>
    </PuzzleCond>
  </Puzzle>
  <AmbientSoundAtPoint sound_id="12" volume="100" flags="0" h_0x06="0" h_0x08="0" h_0x0a="0" platform_tag="0" puzzle_tag="0" x="250.0" y="60.0" z="-250.0" radius="300.0"/>
  <EndLevelData/>
</Level>
""".format(ground=GROUND, grave=GRAVE, pumpkin=PUMPKIN, block=BLOCK)

LEVEL2 = """<Level name="FIX11.lnd" libgarib-version="0.0" data-format-version="1">
  <LandActor object_id="{ground}" name="ground" x="0.0" y="0.0" z="0.0"/>
  <GaribGroup group_id="0" initial_state="1">
    <Garib x="0.0" y="80.0" z="0.0" type="garib" dynamic_shadow="0"/>
  </GaribGroup>
  <EndLevelData/>
</Level>
""".format(ground=GROUND)


def cube_glb(color):
    import pygltflib as gl
    import make_test_model
    g = gl.GLTF2.load_from_bytes(make_test_model.build(2))
    # The test map's red block alone, recoloured
    g.nodes = [gl.Node(name="block", mesh=1, scale=[100, 100, 100])]
    g.scenes = [gl.Scene(nodes=[0])]
    g.materials[1].pbrMetallicRoughness.baseColorFactor = list(color) + [1]
    return b"".join(g.save_to_bytes())


class FixtureBackend:
    def __init__(self):
        import kit, make_test_model
        self.files = {}
        self.files["/models/FIX/ground.glb"] = make_test_model.build(24)
        self.files["/models/FIX/block.glb"] = cube_glb((0.3, 0.5, 0.9))
        self.files["/models/FIX/enemy.glb"] = cube_glb((0.9, 0.3, 0.3))
        self.files["/models/FIX/ball.glb"] = cube_glb((0.8, 0.8, 0.8))
        for name, item, params in (("grave", "grave", {"text": "RIP/TEST"}), ("pumpkin", "pumpkin", {}),
                                   ("sign", "sign", {})):
            try:
                self.files["/models/FIX/%s.glb" % name] = kit.build(item, kit.clean(item, params)).glb()
            except Exception:
                self.files["/models/FIX/%s.glb" % name] = cube_glb((0.6, 0.6, 0.6))
        self.level_models = {GROUND: "/models/FIX/ground.glb", BLOCK: "/models/FIX/block.glb",
                             GRAVE: "/models/FIX/grave.glb", PUMPKIN: "/models/FIX/pumpkin.glb", BALL: "/models/FIX/ball.glb",
                             ENEMY: "/models/FIX/enemy.glb"}
        self.cat = {
            GROUND: {"name": "ground", "bank": "FIXTURE_BANK", "url": "/models/FIX/ground.glb", "bytes": 40000, "world": "FIXTURE"},
            BLOCK: {"name": "block", "bank": "FIXTURE_BANK", "url": "/models/FIX/block.glb", "bytes": 900, "world": "FIXTURE"},
            GRAVE: {"name": "grave", "bank": "FIXTURE_BANK", "url": "/models/FIX/grave.glb", "bytes": 3000, "world": "FIXTURE"},
            PUMPKIN: {"name": "pumpkin", "bank": "FIXTURE_BANK", "url": "/models/FIX/pumpkin.glb", "bytes": 3000, "world": "FIXTURE"},
            ENEMY: {"name": "dennis", "bank": "FIXTURE_BANK", "url": "/models/FIX/enemy.glb", "bytes": 5000, "world": "FIXTURE"},
            BALL: {"name": "ball", "bank": "FIXTURE_BANK", "url": "/models/FIX/ball.glb", "bytes": 800, "world": "FIXTURE"},
            SIGN: {"name": "sign", "bank": "OTHER_BANK", "url": "/models/FIX/sign.glb", "bytes": 2500, "world": "OTHER"},
        }
        self.custom = {}
        self._decomp = json.load(open(os.path.join(HERE, "app", "decomp", "decomp.json")))
        # The enemy "dennis" (the first enemy type) uses the fixture's enemy model
        import prepare
        tables = prepare.noclip_tables()
        en = self._decomp["ksyEnums"]["enemy::enemy_type"]
        tv = int(next(k for k, v in en.items() if v == "dennis"))
        tables["enemyObjects"][tv]["value"] = int(ENEMY, 16)
        self._tables = tables

    def scenes(self):
        return [scene("10", "Fixture 1"), scene("11", "Fixture 2", "Hub")]

    def decomp(self):
        return self._decomp

    def tables(self):
        return self._tables

    def templates(self):
        return {}

    def heap(self):
        return {"10": {"free": 200 * 1024}, "11": {"free": 120 * 1024}}

    def mod_info(self):
        return {"heapExtraBytes": 4 << 20, "modBytes": 5000}

    def level_xml(self, sid):
        return LEVEL if sid == "10" else LEVEL2

    def models(self, sid):
        return {"models": dict(self.level_models), "loaded": sorted(i for i in self.level_models)}

    def orig_lev_size(self, sid):
        return 1234

    def catalog(self):
        return {**self.cat, **self.custom}

    def fetch(self, url):
        if url in self.files:
            return self.files[url]
        raise FileNotFoundError(url)

    def plan(self, sid, ids):
        objs = [i for i in ids if i in self.cat or i in self.custom]
        return {"objects": objs, "targetBank": "FIXTURE_BANK", "textures": {}, "targetTexBank": "FIXTURE_TEX_BANK",
                "objectBytes": sum((self.cat.get(i) or self.custom.get(i))["bytes"] for i in objs), "texBytes": 0,
                "runtime": {"level": int(sid, 16), "objectBanks": ["OTHER_BANK"], "textureBanks": [], "objectNumbers": [9],
                            "textureNumbers": [], "bytes": 30000},
                "copy": {"objects": [], "textures": {}, "targetBank": "FIXTURE_BANK", "targetTexBank": "FIXTURE_TEX_BANK",
                         "objectBytes": 0, "texBytes": 0}}

    def validate(self, xml):
        from lxml import etree as ET
        from libgarib.levels import level_schema_path
        if not hasattr(self, "_schema"):
            self._schema = ET.RelaxNG(file=level_schema_path)
        root = ET.fromstring(xml.encode("utf-8") if isinstance(xml, str) else xml)
        return [] if self._schema.validate(root) else [str(e) for e in self._schema.error_log.filter_from_errors()][:5]

    def build(self, sid, xml, fmt, quickboot=False, imports=(), expansion=False, copy=False, info=None):
        from lxml import etree as ET
        from libgarib.levels import xmlToLandscape, level_schema_path
        if fmt != "lev":
            raise ValueError("The fixture has no ROM: only .lev builds work here.")
        tree = ET.ElementTree(ET.fromstring(xml))
        schema = ET.RelaxNG(file=level_schema_path)
        if not schema.validate(tree.getroot()):
            raise ValueError("Level XML does not match libgarib's schema:\n" + "\n".join(str(e) for e in list(schema.error_log)[:20]))
        lev = xmlToLandscape(tree)
        return lev, {"filename": scene(sid, "")["landscape"], "X-Lev-Size": str(len(lev)), "X-Needs-Mod": "0"}

    def import_custom(self, data, name, scale):
        import hashlib
        s = hashlib.sha1(data).hexdigest()[:10]
        oid = "0x7%07X" % (int(s[:7], 16))
        self.files["/custom/%s/preview/model.glb" % s] = data
        self.custom[oid] = {"name": name, "bank": "Custom: " + name, "url": "/custom/%s/preview/model.glb" % s, "bytes": len(data),
                            "custom": s, "piece": name, "offset": [0, 0, 0], "textures": [], "texBank": "custom:" + s}
        return {"set": s, "name": name}

    def delete_custom(self, set_):
        for k in [k for k, c in self.custom.items() if c["custom"] == set_]:
            del self.custom[k]

    def kit_items(self):
        import kit
        return kit.items()

    def kit_preview(self, item, params):
        import kit
        return kit.preview_png(item, kit.clean(item, {k: str(v) for k, v in params.items()}))

    def make_kit(self, item, params):
        import kit
        p = kit.clean(item, params)
        return self.import_custom(kit.build(item, p).glb(), kit.label(item, p), 1.0)

    def rom_path(self):
        return None


if __name__ == "__main__":
    b = FixtureBackend()
    print(len(b.files), "models;", b.build("10", LEVEL.encode(), "lev")[1])
