# Technical notes

_Back to the [README](../README.md)_

How the editor is put together, what the game does with a level, and the rules a built ROM has to keep.

The editor is a desktop level editor for Glover (N64, NGVE): a Qt window with an OpenGL view (`desktop/`), which
calls `core.py`'s functions in its own process. It ties three projects together:

| Source | Used for |
|---|---|
| **noclip.website** (`vendor/noclip`, `src/Glover`) | The data set: `rom_split.py` + `file_offsets.yaml` split the ROM into the same `data/Glover/*.lev / *.obj.fla / *.tex.fla` files noclip serves. Its scene table supplies level names and object/texture banks, and its placement logic (actor rotation order, X/Z-swapped scale, enemy and powerup model tables) is ported to the editor. |
| **libgarib** (`vendor/libgarib`) | Every format operation: `.lev` ↔ XML (`level-tool`, byte-identical round trip), object banks → glTF (`objbank-tool`), texture banks → PNG (`texbank-tool`), RelaxNG schema validation, and ROM patching/relocation plus the `copyskip`/`quickboot` QoL patches (`rom-asset-tool`). |
| **gloveworld** decompilation (`../gloveworld`) | Meaning: the decomp names each opcode (`enum Token`) and points to the C that parses it. It gives real names for 31 opcodes that libgarib only has placeholders for (e.g. `PlatMvspn0x58` = `TOKEN_SPINTOMOVE`). It also supplies enum names for enemy types, puzzle conditions/actions and enemy instructions, plus the checks: the game's own level-parser warnings and `Crash()` limits, re-run on your edits. |


## Where things are

| Path | What |
|---|---|
| `desktop/` | The editor: a Qt window (PySide6) with an OpenGL 3.3 view (PyOpenGL). |
| `core.py` | The back end's functions: level XML via libgarib, glTF and texture export, the model catalog, import planning, ROM and BPS builds, hardened XML parsing. |
| `prepare.py` | One-off: splits the ROM with noclip's tool into `data/Glover`, and writes `scenes.json` (noclip's scene table and the banks the game's `LoadLevel` loads), `noclip_tables.json`, `enemy_templates.json` and `decomp.json` (via `tools/build_xref.py`). Applies every `patches/libgarib-*.patch`. |
| `app/` | The Windows app: `launcher.py` (the user folder, the first-run setup window, then the editor), `decomp/` (the gloveworld files the editor reads), the pinned libraries and the guide for players. `tools/build_app.py` builds it with PyInstaller. |
| `mod/` | The Expansion Pak game mod: `transforms.c` (rotation and scale for every object), `heap.c` (larger heap), `banks.c` (models and textures from other banks), `guard.c` (the matrix guard), `slots.c` (extra levels), which run from 0x807E0000; `loader.c` (over `ScreenShot`); and the committed build outputs `mod.bin` and `mod.json`. |
| `tools/build_mod.py` | Builds `mod/` with gloveworld's GCC 2.7.2 against `build/NGVE.elf`, finds the call sites to hook and writes `mod.json`. |
| `tools/mod_patch.py` | Adds the mod to a built ROM, with the extra banks each level loads and the extra levels. Also writes the level table and the ball controls. Checks each word before it replaces it. |
| `tools/hw_check.py` | Checks a built ROM against the rules of real hardware. `mod_patch.py` runs it on every ROM it builds. `--self-test ROM` breaks a copy in each way and checks the breaks are found. |
| `tools/bps.py`, `web/patcher.html` | The BPS encoder and decoder, and the offline patcher page for players. After editing the page's script, run `tools/build_patcher.py` to pin the new hash. |
| `tools/game_banks.py`, `tools/build_xref.py` | Which banks the game loads per level, and the cross-reference between libgarib's format and the decomp (token names, parser locations, enums, diagnostics). |
| `tools/custom_model.py`, `tools/kit.py`, `kit/` | Import model: a glTF into Glover objects, a texture bank and previews. The kit: models made by code. |
| `tools/measure_heap.py`, `web/heap.json` | Free heap per level, measured in the emulator. |
| `tools/desktop_fixture.py`, `tools/desktop_test.py` | A back end without a ROM (two made-up levels, the kit's models) and the editor's test on it. |
| `patches/libgarib-*.patch` | Fixes to libgarib, applied in name order by `prepare.py` and `build_app.py`. |

`desktop/` by file:

| File | What |
|---|---|
| `editor.py` | The state and every edit: loading, selection, the transform engine, undo, autosave, adding and removing, checks, draw load, imports and the budget, the path tool, building. Signals tell the widgets what changed. |
| `entities.py` | Entities from the level XML (`buildEntities`), bindings, attached points, trigger volumes, camera zones. |
| `window.py`, `panels.py`, `setup.py` | The main window, toolbar, shortcuts and dialogs; the outliner, inspector, checks, palette and path panel; the first-run window. |
| `viewport.py`, `render.py`, `scene.py`, `camera.py`, `gizmo.py`, `gltf.py` | The 3D view: a small scene graph, an OpenGL 3.3 renderer, orbit and fly cameras, the gizmo, the glTF loader. |
| `backend.py` | `core.py`'s functions, called in the same process. `tools/desktop_fixture.py` is a stand-in with the same methods. |
| `sanitize.py`, `drafts.py`, `xmlutil.py`, `vec.py` | Import checks, drafts as files, lxml helpers, three.js maths (Euler `'YZX'`). |

- A model is named by a path (`/models/…`, `/custom/…`); `Backend.fetch` reads the file it names.
- The level XML is an lxml tree. Undo snapshots and "is this the shipped level?" compare `xmlutil.serialize`.
- Every edit rebuilds the entities from the XML. Models are parsed once per level.
- Drawing: opaque meshes are merged into one world-space buffer per material after each rebuild (`make_batches`).
  What moves without a rebuild (a gizmo drag, the motion preview) is drawn on its own. PyOpenGL's error checking is
  off: it cost more than the drawing.
- Lighting follows three.js r160 (Lambert's albedo / π, ambient 0.8, sun 1.4, sRGB output). Sprites keep their
  world size, as three.js sprites do.
- Picking runs on the CPU: a ray against each mesh's triangles (numpy), sprites as camera-facing squares, lines
  within 5 pixels.

## Sharing levels: patches and the patcher

- **Export patch** (toolbar) builds the ROM and diffs it against the original into a **BPS patch**
  (`tools/bps.py`). It carries CRC32 checks of the original ROM, the result and the patch itself, plus JSON
  metadata: the level, whether it boots straight into it, and the SHA-1 of both ROMs.
  - **Plain edits are true diffs.** Data libgarib only moved (relocated banks) is encoded as copies from the
    player's own ROM. The Atlantis 1 garib edit is a 1.8 KB patch.
  - **Edits with imported models are small too, with the game mod.** By default the mod loads imported models
    from the player's own ROM at run time. The patch has no game data. Players need the Expansion Pak.
  - **Copied imports are not true diffs.** With "Copy the models into the ROM instead", the build rewrites the
    level's object bank and the world texture bank uncompressed, and the clean ROM only holds them
    FLA2-compressed. Those banks travel in full, game data included: one 4.9 KB imported model gave a 165 KB
    patch. The export dialog says so before you export.
  - Patches start the game normally by default. "Boots straight into this level" is a separate opt-in in
    the export dialog (the toolbar's "boot here" only applies to Build ROM).
- **`web/patcher.html`** is for players: one self-contained file that works offline, even opened from disk.
  Load your own Glover (USA) ROM (.z64, .v64 or .n64; the byte order is converted) and the .bps, then
  "Create patched ROM". Everything happens in the page:
  - Its Content-Security-Policy has `connect-src 'none'`: the browser itself blocks any network request.
    The script is pinned by hash (`tools/build_patcher.py`; run it after editing the script).
  - Every read and copy in the decoder is bounds-checked.
  - A wrong ROM, damaged patch or malformed patch is refused with a plain explanation; nothing is saved.
  - Patch metadata is shown as text only.
- Patches also work in other BPS tools: RomPatcher.js applies them to the same bytes.
- Verified in the emulator: a shared patch (normal start) applied to the clean ROM boots through the
  copyright screen, studio intro, title fly-through, main menu, file select and name entry, never reaching
  `gameInfo.mode` 13.

## Autosave, reset, import

- **Autosave**: after every edit the level's XML is saved in the user folder (`drafts/<level id>.json`,
  `desktop/drafts.py`). Reopening the level, restarting the editor or switching levels and back restores it. The HUD
  chip shows "Edited · autosaved HH:MM", and the level list marks edited levels with ●. Undo after a restore
  goes back to the shipped level.
- **Reset level** discards a level's edits and its autosave, after a confirmation. It is undoable in the
  current session.
- **Import XML** (or drop an .xml on the window) validates the file strictly before it can touch the level
  (`desktop/sanitize.py`):
  - It refuses DOCTYPEs, entity declarations, stylesheet instructions, XInclude and CDATA before parsing,
    which rules out entity expansion and external-entity reads.
  - Every element must be a command or type from libgarib's format, in a place that format allows (a `Garib`
    inside a `GaribGroup`, `PlatScale` inside a platform…).
  - Every attribute must be a field of that element. Every value must fit its type: finite 32-bit floats,
    integers within u1–s4 ranges, known enum names, names of at most 8 printable ASCII bytes.
  - It also enforces limits on file size (4 MB, checked before reading), element count, nesting depth and
    attribute length.
  - It requires a single `EndLevelData` at the end: the game parses until that token. A missing one is
    added and a misplaced one is refused.
  - The validated content is copied into a fresh document, which is what reaches the editor. Comments,
    namespaces and anything unknown never do.
  - The dialog reports problems by element path, matches the file to its level by name, and imports as an
    undoable edit.
  - Autosaved drafts go through the same validation when they're restored.
- `core.py` repeats the safety checks for builds: no DTD, entities or network access in its XML parser, a size
  limit, and strict validation of level IDs and import IDs.

## Selecting, rotating, scaling, adding

- **Multi-select**: Ctrl/Cmd+click toggles; **B** (or the Box select button) drags a box; **Ctrl+A** selects
  everything visible except landscape; the outliner supports Ctrl/Shift+click and an "all" button per group.
  Move, rotate, scale, duplicate and delete apply to the whole selection. **Pivot** chooses the group centre or
  each object's own origin.
- **W / E / R** switch the gizmo between move, rotate and scale. The inspector also has numeric fields
  (one object) or "move / rotate / scale by" (several objects).
- What each kind of object can store (the editor only writes what the game reads):

  | Object | Rotation | Scale |
  |---|---|---|
  | Land / background actors | X, Y, Z via `SetActorRotation` | X, Y, Z via `SetActorScale` |
  | Platforms | X, Y, Z via `SetActorRotation`, added to a spinning platform's angles (mod) | `PlatScale`, kept before the first path point |
  | Enemies | Y sets `y_rotation`; X and Z tilt via `SetActorRotation` (mod) | X, Y, Z via `SetActorScale` (mod) |
  | Powerups | X, Y, Z via `SetActorRotation`, under the powerup's own spin (mod) | X, Y, Z via `SetActorScale` (mod) |
  | Water | X, Y, Z via `SetActorRotation` on its model (mod) | X, Y, Z via `SetActorScale` on its model (mod) |
  | Wind volumes | none | resizes the box |
  | Puzzle trigger volumes | none (axis-aligned) | resizes the box, sphere or cylinder about its centre |
  | Garibs, Mr. Tip, spawns | none: sprites or points in the game; group rotations move them around the pivot | none |

  "(mod)" means the game applies it only with the Expansion Pak game mod, described below.

  The transform maths comes from the decomp: rotation is `GetQuaternionFromXYZRot` with `rotOrder` {1,2,0}
  (R = Ry·Rz·Rx, Euler order `YZX`), and scale is `guScaleF(x, y, z)` with no axis swap. noclip uses `ZYX`
  and swaps X/Z. Shipped levels only use Y rotation and uniform scale. The two agree on shipped data.
  An edited level rendered from the emulator's exact camera (`guLookAt(actualCamPos, actualCamTargetPos)`, read
  from memory) under both conventions showed it: the decomp's matches the game and noclip's does not.
- **Expansion Pak game mod.** A small game mod (`mod/`) uses the N64 Expansion Pak for three things:
  - **Rotation and scale for every 3D object.** The unmodded game applies `SetActorRotation` and
    `SetActorScale` only to landscape and background actors. The mod applies them to enemies, platforms,
    powerups and water too (`mod/transforms.c`, details below).
  - **A larger heap.** The game's heap (Jalloc) normally ends at 4 MB and holds about 1.3 MB. The mod moves its
    end up to 0x807E0000, just below the mod itself, adding 3,968 KB (`mod/heap.c`).
  - **Models from other banks** (`mod/banks.c`). The game has 5 slots for model banks and 5 for texture banks,
    and a level uses about 3 of each. The patch lists the extra banks each level needs. Before the level is
    parsed, the mod loads them from the ROM with the game's own `LoadObjectBank` and `LoadTextureBank`: it
    empties the slots for the call, then moves the new bank into its own lists and puts the level's banks
    back. `FindObject` and `FindTexture` are redirected to versions that search the game's slots first, then
    the mod's lists. The extra banks are freed before the next level loads. Animated textures in them do not
    animate.
  - **When it's added.** Build ROM and Export patch add it whenever the level rotates or scales one of those
    objects or imports models (unless "Copy the models into the ROM instead" is on and the copies fit the
    game's own heap). The checks panel, the import panel and the export dialog say so. Other levels are built
    exactly as before, without the mod.
  - **Where it lives.**
    - The mod is about 5 KB of code, built from C against the gloveworld decomp. It is stored in the ROM at 12 MiB,
      past anything libgarib writes, and runs from Expansion Pak RAM at 0x807E0000.
    - A 620-byte loader replaces `ScreenShot`, a debug function the game never calls. It runs once at boot.
    - The game's own code keeps its exact layout. The ROM changes are 16 same-size `jal` changes. At boot, with
      an Expansion Pak, the mod also writes a jump over the first two instructions of `FindObject` and
      `FindTexture` in memory. This keeps libgarib's ROM patching, which uses fixed code offsets, working.
  - **The heap.** `MainProc` starts the heap before the loader can run (the loader needs `dmaMessageQ`, which
    is created later). Nothing is allocated in between. The mod starts the heap again with the larger size.
    The three calls that restart the heap on level changes (`game.c`, `frontend.c`) go through the mod, which
    keeps the larger size. Jalloc itself has no 4 MB limits: static blocks grow down from the end, dynamic ones
    up from the start, and sizes are 32-bit. Nothing else in the game masks addresses to 4 MB.
  - **What it does.** The level parser sends `SetActorRotation`/`SetActorScale` to the most recent object of any
    kind, not only the last landscape actor. In all 48 shipped levels, every one of these commands follows a
    landscape-type actor. Shipped levels behave exactly as before.
    - Water takes the rotation and scale on its model. Its swimming volume stays an axis-aligned box.
    - A platform takes the rotation before its collision is built. It collides as drawn. A spinning platform
      gets the rotation added to its spin angles.
    - Enemies and powerups are re-rotated and re-scaled every frame by the game's own code (AI facing, the
      powerup spin). So the mod keeps a base rotation and scale and multiplies them in when the object is
      drawn (`DrawActor`). It uses the game's own `QuaternionMultiply`. The base applies in the object's
      own axes, as for landscape actors. An enemy's collision sphere and shadow scale with it, by the mean of
      the three axes, and again when it respawns.
  - **Without an Expansion Pak**, the loader puts the original instructions back. It keeps one small parser
    filter that skips these commands. The game then ignores the extra rotations and scales; it does not apply
    them to the previous landscape piece. The heap keeps its original size. A level whose copied imports
    need the larger heap runs out of memory and stops. A level whose imports load at run time can't find those
    models, and the game stops it as "LEVEL UNPLAYABLE" (`gameInfo.mode` 13) before play starts.
  - Garibs and Mr. Tip are flat sprites, and the start markers are points. They have no 3D rotation or size.
  - `tools/build_mod.py` rebuilds `mod/mod.bin` and `mod/mod.json`. It needs a built gloveworld
    and its GCC 2.7.2. The outputs are committed. Building ROMs doesn't need the toolchain.
    `tools/mod_patch.py` adds the mod to a ROM. It checks every word it replaces first.
  - Verified in the emulator:
    - A tilted platform, and a General Wu lying on its side at twice its size, appear and play. The level stays
      in play mode.
    - The editor's preview, rendered from the emulator's camera, matches the game. The opposite rotation order
      does not.
    - With the loader's Expansion Pak check forced to fail, the mod doesn't load. Glover, the camera and the
      enemy then end up exactly where they do in the control ROM, at the same game frame.
- **+ Add** (A) opens a palette. Each entry shows a picture of its model, drawn when the entry scrolls into
  view:
  - **Collectables**: garib types, every powerup (`Pickup` enum), Mr. Tip.
  - **Enemies**: all 52 types. A new enemy copies the shortest script that type has in a shipped level, with
    its absolute points (`home_*`, `destination_*`, `lookat_*`, attention box) moved to the new spot.
  - **Models**: all 1,522 models in the game, from every object bank, placed as Scenery (`LandActor`, solid),
    Background (no collision) or Platform. **Import model…** adds a model of your own (see [Import model](Import-model.md)).
- **Attached points follow their object.** Enemy scripts use world coordinates (`home_*`, `destination_*`,
  `lookat_*`, attention box), and platforms have path points and orbit centres. Moving, rotating, scaling or
  duplicating an object applies the same transform to all of them. In the emulator, a General Wu moved 300
  units this way roams its new area; with only its `x` changed, it walks back to the old one.
- **Imports**: a model from another level's bank is imported when you press Build ROM. By default the game mod
  loads it from the player's ROM at run time (see "Models from other banks" above). The editor picks the
  banks: each model's own bank, and the texture banks that hold the textures the level's banks lack,
  preferring the model's own world. The import panel shows their size against this level's free heap plus
  the Expansion Pak's. With "Copy the models into the ROM instead", libgarib splits it
  out of its bank and packs it into the bank the game loads last for this level (`levelInfo[level].objBank`
  in `LoadLevel`, e.g. `ATLANTIS_L1`); textures it needs are
  appended to the world texture bank, record by record. The game fixes up each texture record relative to
  itself (`LoadTextureBank`). The records are copied unchanged. Imports are worked out from the XML at
  build time. Undo and XML export always match the build. The checks panel lists them with a memory
  budget. Every level that loads the enlarged object or texture bank pays for the growth. The budget
  is checked against the tightest of those levels, using free heap measured in the emulator
  (`web/heap.json`, all 48 levels, `tools/measure_heap.py`). Free heap ranges from 36 KB (Carnival 1) to
  435 KB. Imports that need more add the Expansion Pak game mod, whose heap is 3,968 KB larger.
  Verified in the emulator:
  - It adds a 242 KB model from the INTRO bank to Carnival 1.
  - Without the mod the game runs out of heap (1,456 bytes free) and stops.
  - With it the heap is 5.4 MB, the level plays, and 3.8 MB stays free.
  - With the Expansion Pak check forced to fail, the heap keeps its 1.3 MB and the game stops as without the mod.
- Which banks a level loads comes from the game's own `LoadLevel` (`tools/game_banks.py` reads `levelInfo`
  and `worldInfo` from the decomp build). It agrees with noclip's scene table for every playable level.
  Six differences remain, in the Wayroom (its shared bank depends on the world you came from), the studio
  logos and the flythrus. A model counts as "already loaded" only if it's in a bank both lists load.
- **Puzzle trigger volumes** are drawn as translucent shapes, one per puzzle condition that tests where
  Glover, the ball or the camera is. The shapes follow `PuzzleEvaluateCondition` in the decomp:
  - Boxes (`…WithinVolume`) are a min corner `x, y, z` plus a size `l, w, h` along X, Y, Z
    (`IsPointWithinBox`).
  - Ranges (`…WithinRangeOfPoint`) are spheres: a centre and a radius.
  - `PuzzleCondGloverWithinRangeOfPoint2` is a vertical cylinder: centre `x, z`, radius `range`, from
    `min_y` to `max_y`.

  Pink is Glover, orange the ball, purple the camera. Move them with the gizmo and resize them with scale or
  the inspector's size fields; the inspector shows the whole puzzle. Duplicate and Delete copy or remove the
  whole puzzle when the volume is its only one (a camera zone, for one). A volume of a puzzle with several
  can't be duplicated or deleted on its own. The shipped levels hold 209 boxes and no spheres or cylinders.
  Clicks pass through the translucent fill to objects inside.
- **triggers** (toolbar checkbox) shows or hides the puzzle trigger volumes. It is the same switch as the
  "Puzzle triggers" layer. The setting is remembered between sessions. Hidden volumes can't be picked, and the
  level keeps them.
- **fog** (toolbar checkbox) shows or hides the level's fog in the editor view. The setting is remembered
  between sessions. It doesn't change the level: in-game fog is the `FogConfiguration` command's
  `fog_enabled` field, which you can edit in the inspector.

## Level tools

### Camera zones

**+ Add → Camera** adds a camera zone: a box where the camera moves a set way while Glover is inside it. When he
leaves, the camera goes back to the level's own. The shipped levels have about 170 of these. Each is a puzzle
with one condition (Glover inside a box, `cond_type` 35) and camera actions only, so it needs no game mod.

| Preset | While Glover is in the box, the camera |
|---|---|
| Behind | turns to look the way the player goes |
| High | rises and looks down on Glover |
| Side, Other side | swings round to the right or the left of the way on |
| Front | faces Glover: the player can't see where they go |
| Low | drops to Glover's height and comes close |
| Far | stands far back and high |
| Fixed | flies to a point ahead and watches from there |

- A new zone expects the player to walk the way the view looks. Move and size the box like any trigger volume.
- The inspector's **Camera zone** panel picks a preset for any heading, and turns each action on or off:
  - **look**: `LookAtGlover`, the heading from the camera to Glover and how fast it turns there
  - **distance**: `TweenDistance`
  - **height**: `TweenYAdjust`
  - **fly to**: `FlyTowardsPoint`
- A dashed line runs from the middle of the zone to about where its camera stands. **Look from the zone's
  camera** shows about what the player sees.
- The shipped zones of this shape show the panel too.
- How the actions work (`camera.c`, `puzzle.c`): the look angle is `atan2(dx, dz)` of Glover minus the camera,
  and the camera puts the level's own settings back on every frame Glover is in no zone.

### Draw load

The checks estimate how hard the busiest spot of the level is to draw.

- The game draws every actor within the far plane of the camera: behind the camera too, since
  `IsActorVisible` (`object.c`) only compares the distance with the far plane plus the actor's radius.
- Every object node it draws takes one of the frame's 256 object matrices, and so does each shadow and
  footprint. Past 256 the frame never finishes and the game stops. Long before that it slows down.
- The far plane is `FogConfiguration`'s `near_clip` (1000 without one).
- The editor counts the object nodes of every model within reach of a camera 130 behind and 50 over Glover,
  facing four ways, at every garib, powerup, checkpoint, enemy and camera zone.
- From 190 it's a warning, from 240 an error. Click the row to see the spot and the far plane.
- Checked against the game's own count (the mod's `modPeakMatrices`) at 8 spots from quiet to busy in each of five
  levels. In Atlantis 1, Carnival 1, Carnival Bonus and Pirates 2 the estimate was within
  35 of the game at every spot. In hub 1 it said up to 63 more than the game. The shipped hub 1 asks for 228 at
  its fountain (the estimate says 261), and Pirates 2 for 199 (198).
  A hub built with the editor slowed down on a console at 206 to 228.
- Also checked: `fog_distance` must stay under 1000.

### Ground under checkpoints

Glover starts at the first checkpoint and restarts at the last one he touched. A checkpoint, or a
`GloverSpawnPoint`, with no solid ground within 400 under it is a warning. Solid means a landscape piece
(`LandActor`) or a platform. Background actors have no collision.

### The kit

**+ Add → Kit** has props made by the editor's own code (`tools/kit.py`, the models in `kit/`).
They hold no game data, so they're free to share.

- Graveyard: a gravestone with your words, a signpost with your words, a pumpkin with one of eight faces,
  candles, a coffin, a broken fence, two dead trees and a cobweb
- Ruins: rubble, a scorch mark and thorns
- Creatures: a shade, a giant hand, an eye, a pale face, a rag-doll glove, a hanging glove, gallows, a cage and
  a lantern

Each row shows a preview, and the preview follows the settings. **Background** places the model without
collision, **Scenery** as a solid `LandActor`. The editor builds the model and converts it like an imported
model ([Import model](Import-model.md)), and it then shows with the imported models. The same settings give the same model.

### Path

**Path** (<kbd>P</kbd>) lays a path: click the ground to add points, and <kbd>Backspace</kbd> takes the last one off.
**Place** then puts things along it:

- **garibs** of any type, in a new garib group
- **copies of the selection**: scenery, kit models, enemies (with their scripts), powerups or Mr. Tip

Set how far apart they go, on the path or to one or both sides of it, and how high over the ground. Each one is
dropped onto the ground under it. **random turn** turns each copy of a single model. The path itself is only a
guide in the editor and isn't saved.

### Platforms from converted models

A platform must use a model from the game. A converted model (imported or from the kit) has a first rotation
key of all zeros. A console stops when it draws one as a platform; an emulator doesn't. The Models tab doesn't
offer **Platform** for them, and the checks flag one that is.

## Commands, puzzles, enemy scripts, extra levels

| File | What |
|---|---|
| `desktop/commands.py` | What goes inside what (from decomp.json), new-command defaults, and the order the game reads a platform in: phases for inserting, glued children (a spin pause after its spin), needs-before, one-of and exclusive rules, the NullPlatform whitelist. `tidy()` moves only what the game reads wrong. `platform_rules()` feeds the checks. |
| `desktop/puzzles.py` | Plain names for every cond_type and action_type, canonical child order, float-in-u4 fields, puzzle checks. |
| `desktop/scripts.py` | Enemy instruction lists (flat per kind across groups), Goto and Loop retargeting, ready-made scripts, script checks. |
| `desktop/presets.py` | Platform presets (rings, exits, lifts, spikes, teleport, the extra-levels entry zone). |
| `mod/slots.c` | Extra level slots: the loader, the levelInfo swap, the save swap and its EEPROM record, the world exit, tag 77 rings. |

- The 48 shipped levels give no error from the new checks, and `tidy()` changes none of them. A rule that fires on a
  shipped level is wrong. Shipped levels put PlatScale after PlatSetInitialPos on moving or face-freed platforms,
  PlatSetCollisionYOffset after placement, a path sound anywhere after its point, and PlatVentAdvanceFrames before
  points; all of these are fine.
- `num()` reads libgarib's hex ints (`0x4000`); it used to read them as 0.
- Importing libgarib's objbank parser in the same process breaks its level parser (both patch one base class).
  `Backend.faces` reads a model's root-mesh face count from the bank bytes instead.
- Title, music and bonus clock go into levelInfo in the ROM (0xE7910 + 0x38 × slot; name +0, song +0x35, clock
  +0x36). They are kept in `drafts/info.json`, not in the level XML. In the emulator a 45 s clock counted down and
  PrepareSong got the chosen song. Quickboot ROMs then switch to song 59 whatever the row says.
- `core.build_levels` builds any number of levels in one ROM. Two levels can't copy models into the same bank.
  Extra levels get no manifest line; `apply_mod(slots=…)` stores them at MOD_ROM + 0x20000, 16-byte aligned, and
  fills `modSlots` ('SLT1', 344 bytes). Their banks are keyed slot + 0x40.
- Extra slots are 35, 36, 37, 38 and 41 (39 and 40 have fixed game code). The entry zone is a SetExit 11
  loading zone with tag 99 in any hub; tag 77 marks rings a rolling ball may touch. `free_tag()` skips both.
- In the emulator: a zone in hub 1 led to the Space way room with `modSlotsActive` 1; a booted extra Space 1 loaded
  its own file and title; a mod ROM without extra levels played as before; Atlantis 1 with every preset, a
  chasing enemy and a new puzzle played (31 platforms against 14, matrix peak 117).
- `tools/emu/emu.py` (gloveworld) keeps data for a ROM by file name: give each new ROM a new name, or it runs the old one.

## Test level

**Test level** builds the ROM as Build ROM does, with quickboot, into the user folder's
`test/` and opens it in the emulator the player chose the first time (kept in `settings.ini`).

## Facts from the decomp

- Landscape rotation: `GetQuaternionFromXYZRot` with `rotOrder` {1,2,0} → R = Ry·Rz·Rx (three.js Euler
  `'YZX'`). Scale is `guScaleF(x, y, z)` with no swap. Confirmed against an emulator frame; noclip's `'ZYX'` +
  X/Z swap is wrong for edits.
- Collision follows rotation and scale (`InitPlanes` takes `rotQuat`/`scale`).
- Platforms: one `TOKEN_SPIN` each; `PlatScale` and `PlatSetParent` are ignored after the first path point;
  max 20 path points. Enemy scripts: more than 30 normal, 10 conditional or 30 attack instructions calls
  `Crash()`.
- Glover starts at the first `PlatCheckpoint` (restart gate) unless a `GloverSpawnPoint` exists.
- Ball walking: `ProcessInput` (`src/controller.c`) subtracts the stick's push from the ball's velocity on Easy and
  adds it otherwise, so Medium and Hard feel inverted. The check is one `bnez` at 0x801396D0 (ROM 0x3A6D0);
  `apply_easy_controls` in `tools/mod_patch.py` replaces it with a nop in every built ROM. GameShark equivalent:
  `811396D0 0000` and `811396D2 0000`. The difficulty itself is `saveSlot[currentSaveSlot].difficultyLevel`
  (0x801EAA55 + 0x1C per slot; slot 6 is the level select's).
- A missing *vital* object makes the game set `gameInfo.mode = 13` ("LEVEL UNPLAYABLE"). Bullet models are
  non-vital.
- Heap: about 1.3 MB (`JallocInit`). Free heap per level is in `web/heap.json` (36 KB in Carnival 1 up to
  435 KB).
- `quickboot(level_id=N)` in libgarib assembles N as hex; the editor passes `0x..`.
- libgarib finds every asset pointer at a fixed code offset (`map.0x8e6e01ff.0xccb4f948.yaml`, 280 `lui`/`addiu`
  sites) and repacks all assets from offset 0, ending near 0x77DE20. So game code changes must keep the code
  segment's layout. The mod only makes same-size `jal` changes, writes its loader over `ScreenShot` (unused),
  and puts its code at ROM 0xC00000 (12 MiB): libgarib's output passes 8 MiB with many imports, because rebuilt
  banks are stored uncompressed.
- Drawing load: the game slows to a crawl (and at a far plane of 6000 with a whole stage in view, stops) when it
  draws too much at once. The far plane is `FogConfiguration`'s `near_clip` (`farPlaneDist`); `fog_distance` is
  `gSPFogPosition`'s min (0-1000). The game culls by distance only: `IsActorVisible` (`object.c`) compares an actor's
  distance from the camera with the far plane plus its radius, with no test against the view, so everything within
  the far plane is drawn, behind the camera too. A smaller far plane is the strongest cut of the drawing load.
- Glover starts at the first `PlatCheckpoint` platform even when a `GloverSpawnPoint` exists (seen with
  quickboot). Inserting a new checkpoint platform before the level's own platforms froze the game at the start;
  moving the existing first checkpoint works.
- The game camera keeps an offset from Glover and doesn't turn behind him. It follows `cameraInfo.actor`
  (0x8028F9C8 + 4), which trails Glover; to teleport Glover in the emulator, move that and `camera.pos` too.
  Glover walks about 1 unit per video frame.
- RAM: the game uses 4 MB. The Jalloc heap ends at 0x80400000 (`JallocInit` in `MainProc`, `game.c`,
  `frontend.c`). Nothing checks `osMemSize` (0x80000318). 0x80400000-0x80800000 is free with an Expansion Pak.
  With the mod the heap ends at 0x807E0000 and the mod sits above it. Jalloc puts static blocks at the top of
  the heap and dynamic ones at the bottom, with 32-bit sizes and at most `JALLOC_NUM_BLOCKS` (0x500) blocks.
  The game runs out of heap by spinning in `JallocAlloc` (`Crash()` only prints).
- Banks: `LoadObjectBank(num)` and `LoadTextureBank(num)` (`banks.c`) fill the first empty one of 5 slots
  (`objectBanks`, `textureBanks`); the bank numbers are the cases of their switch statements. `FindObject` and
  `FindTexture` search only those slots. The font texture bank always goes to slot 0. `LoadTextureBank` adds
  animated textures to `cyclingTextures` (20 entries, no bound check). `RestoreObjectPointers` allocates each
  textured mesh's `origUV` on the heap (`OFTC%d`); `FreeOFTC` frees them. Level exit frees the 5 slots. The end-of-world and end-of-level screens restart the heap (`FreeScreen`, `FreeLevel2`); the mod forgets its extra banks then (`Mod_ForgetExtraBanks`, called from `Mod_JallocInit`). Without that, the game froze loading the hub after a boss with extra banks: it searched the old banks' memory.
- Texture ids can clash between worlds (Atlantis and Space share four ids with different pictures). `FindTexture` takes the level's own banks first. Bit 7 of a level's number in `modExtraBanks` makes the extra banks win (`tools/mod_patch.py` takes `(tex, obj, True)`).
- `dmaMessageQ` is created by `CreateMessageQueues`, called in `MainProc` after `JallocInit`. Code that DMAs
  at boot must run after it; the loader hooks `InitCRCTable`, the next call.
- All 3D objects are `Actor`s drawn by `DrawActor`, which applies `rotQuat` then the per-axis `scale` (T·R·S).
  Enemies (`UpdateActor` rebuilds `rotQuat` from `rot`, `RestartEnemy` resets `scale`) and powerups
  (`UpdatePickups` spins `rot.y` and eases `scale`) are rewritten every frame. Spinning platforms rebuild
  `rotQuat` from `motion->rot`. Landscape matrices are baked once (`CalculateStaticMatrices`).
- `SetActorRotation`/`SetActorScale` (`TOKEN_LANDROT`/`TOKEN_LANDSCALE`) go to `lastLandscape`, set only for
  landscape, background and animated background actors. In the 48 shipped levels all 725 of them follow such
  an actor.
- `QuaternionMultiply(dst, a, b)` is the Hamilton product a·b; `GetQuaternionFromXYZRot` chains qY·qZ·qX with it.
- Puzzle trigger volumes (parse.c `TOKEN_CONDITION`, puzzle.c `PuzzleEvaluateCondition`): condition ids 35-41
  are read into a `PuzzleCamCond`; the game's condition type is the id minus 9. Boxes (35, 37, 39) are a min
  corner plus a size, `l, w, h` → `size.x, y, z`, tested by `IsPointWithinBox`. Ranges (36, 38, 40) are
  `distance² < radius²`. Id 41 is a cylinder: `pos` holds (x, z, radius) and `size` holds (max_y, min_y).
  Id 34 (`PuzzleCond0x22`, `CONDITION_HAND_CROSSES_LINE`) always returns true.
- Camera zones (docs/technical.md, "Level tools"): a puzzle with one `PuzzleCondGloverWithinVolume` and only actions 70
  (`TweenYAdjust`), 71 (`TweenDistance`), 73 (`FlyTowardsPoint`) and 74 (`LookAtGlover`: `angle` is `atan2(dx, dz)` of
  Glover minus the camera, `distance` is the turn speed). `camera.c` puts the level's camera back every frame Glover is
  in no zone (unless he is falling). The shipped levels hold about 170 of this shape. `camera_zone()` in
  `desktop/entities.py` reads one, `set_camera_zone()` writes its actions.
- Drawing: the game counts one object matrix per object node it draws (all nodes of the object tree, `CountNodes` in
  `mod/guard.c`), plus shadows and footprints. In the glTF exports these are the nodes with a mesh; a node
  with several materials is still one. In the game the
  camera stands about 130 behind and 50 over the glove (measured in Atlantis 1).
- Camera height: `camera.c` puts the camera `distance / 3` plus `heightOffset` over the glove, and `heightOffset` is
  about 30 plus `targetHeight` (a zone's `TweenYAdjust`). Measured: a High zone (320, 140) puts it 408 up. Puzzle
  actions run every frame their condition holds (`CheckPuzzles`: `timesLeft` is -1 without a `PuzzleNumtimes`), and
  `camera.c` puts `levelCamHeight` and `levelCamDistance` back after it uses them. The action ids are the
  `PuzzleActionType` values plus 45: 70 `ACTION_CAM_HEIGHT`, 71 `ACTION_CAM_DISTANCE`, 73 `ACTION_CAM_FIXEDPOS`,
  74 `ACTION_CAM_FIXEDROT`.
- In Atlantis 1 the garibs float over water and the seabed: a glove put at one by a test drowns or falls and restarts.
  The camera zone check puts its zones round the start instead.

## Real hardware

mupen64plus runs what the console refuses, and most of the editor's checks ran only on mupen64plus. A ROM that works in the
emulator can freeze on a console. The rules below come from the PI, the caches, the RDP and the boot code. `tools/hw_check.py` checks the
ones a tool can check, for every ROM `tools/mod_patch.py` builds; `tools/build_mod.py` checks the alignment of the mod's own buffers.

**Boot and ROM**
- The header holds two CRCs over the first megabyte of code (0x1000 to 0x101000, the CIC-NUS-6102 algorithm). The boot code of a console
  does not start a ROM with wrong CRCs. `hw_check.py` has its own copy of the algorithm and compares it with the header (libgarib's).
- The ROM is a whole number of MiB (the build pads to it) and every address the mod reads lies inside it. The ROM header is Glover's
  (`NGVE`). A flash cart that looks up the save type by the game ID finds EEPROM.
- A warm start (an EverDrive can start a game without a power cycle) leaves RAM with the last program's data. Nothing in the mod may
  read RAM it has not written: `Mod_Init` clears the mod's bss, `JallocAlloc(size, 0, ...)` returns junk and every user of it fills it.

**DMA (PI)** from the ROM into RAM
- The ROM address is even (everything the mod stores is on a multiple of 16), the RAM address is 8 byte aligned (16 here), the length is
  even and the mod keeps it a multiple of 16. The PI writes whole 8 byte words: a length that is not a multiple of 8 can write past the
  end, and an unaligned RAM address loses bytes. The game's allocator hands out 16 byte aligned blocks of a multiple of 16 bytes.
- Use the game's own loaders (`DMAMemory`, `DMAGetSize`: they invalidate the data cache over the target first). The mod's loader uses
  `DMAMemory` too. The mod stores every custom bank padded with zeros to a multiple of 16 and reads the padded length
  (`mod_patch.py`: `custom_table`).
- A ROM address that does not match a file (a zero or absurd size word) shows the game's own `LEVEL UNPLAYABLE - NO LEV FILE` screen.
- GCC 2.7.2 keeps at most 8 bytes of an `aligned(16)` attribute, so the address of such a variable depends on the code linked before it. Data
  that needs 16 bytes has to go first in its section after an `ALIGN(16)` in the linker script (`tools/build_mod.py`), and the build should check
  the linked address. `mod/aligned.c` holds the mod's one such buffer, the header of an extra level's file.

**Caches** (an emulator that ignores them hides every fault of this kind)
- Code written to RAM at run time: `osWritebackDCache` over the words, then `osInvalICache` over the same words (`Patch`, `Redirect`
  and the loader do). DMAed code gets `osInvalICache` over its whole length.
- Data the RSP or RDP reads that the CPU wrote needs `osWritebackDCache` before the task starts. The game writes back its whole
  `dynamic` area before it sends a frame (`SendLastFrame`) and the whole data cache before every audio task. The display lists the mod
  appends to `glistp` and everything else it writes reach RAM that way. The mod's textures are never written by the CPU.
- `osInvalDCache` over a DMA target before the DMA. Never invalidate a range the CPU has dirty data in that must stay (the game does it
  for `size` on the stack and it works because a partial line at either end is written back first).

**RDP and RSP**
- A texture's pixels start on a 64 bit boundary. A bank's records are each a multiple of 8 bytes long (the bank's own count word makes the
  first record start 4 bytes in, and the 0x24 byte header does the rest). libgarib's `texbank-tool pack` does not pad: `core.texbank_records`
  pads every record it reads and corrects its size field. Custom banks and every imported model use padded records.
  Before this, every second texture of a custom bank was only 4 byte aligned, and the console reads such a texture as shifted pixels.
- Set every RDP mode a picture needs before drawing it. The game leaves its modes in many states.
- `gDPFillRectangle` and `gSPTextureRectangle` have unsigned 10 and 12 bit corners: a negative or too large value wraps. Clip first.
- A task's display list must stay under 20000 bytes. The game then refuses to create the task and stops swapping frames, which looks like
  a freeze (`CleanupAndSendDisplayList`).
- A texture load of a 16 bit texture holds at most 2048 texels (the converter keeps to it) and the whole of TMEM is 4096 bytes.
- A frame has 256 object matrices (`Dynamic.objectMtx`). Every drawn object node, shadow and footprint takes one, unchecked: past 256 they overwrite the camera's
  matrices and the start of the display list, and the frame never finishes (the game stops, the music goes on). A hub built with the editor did this (259 in one view).
  The mod's guard (`mod/guard.c`, with the Expansion Pak) skips the farthest actors and then shadows and footprints instead. Keep every view under 232 (the guard keeps 24 for shadows and footprints): each prop or retail piece is at least one.

**EEPROM**
- Glover's chip is a 4 Kbit EEPROM: 64 blocks of 8 bytes. The game uses blocks 0 to 55 (ID, best times, save slots, CRCs). Set the
  EverDrive's save type to **EEP 4K** (a 16K chip works too; SRAM, FlashRAM and none do not).

**CPU**
- Word and halfword accesses are aligned (the compiler uses `lwl`/`lwr` where it cannot know). A float load or store is 4 byte aligned.
  GCC 2.7.2 does not align a `.bss` variable after an odd-sized one (two `s32` landed on a 2 byte boundary after a 30 byte array): `tools/build_mod.py`
  checks every data symbol of the mod against its size.
- The console traps floating point operations on denormal numbers and on NaN or infinity converted to an integer. Keep both out of the
  mod's math and out of generated level data.

**Memory**
- The mod is at 0x807E0000 to 0x807EC000 and nothing in the mod reaches 0x80800000 (`build_mod.py` checks the end). With an Expansion Pak
  the game's heap is 0x802B0D10 to 0x807E0000.
- Without an Expansion Pak the mod puts the game back to retail (`ModLoader`) and every retail level plays. A level whose models come
  from the mod needs the Expansion Pak: the game shows `LEVEL UNPLAYABLE` there without it.

**Platforms**
- A platform (`Platform` in the level XML) must use a retail model. The converter's objects (custom models, imported models) have a first rotation key of all
  zeros, and a platform is drawn from it: `QuaternionToMatrix` divides by zero, and a console traps the invalid operation that follows (mupen64plus
  runs on). Use custom models as `BackgroundActor` or `LandActor` (solid). A platform that is never updated (a `PlatSpike`, `SetExit` with
  alpha 0) also needs no `PlatConstantSpin`: its rotation stays zero.

**Testing**
- mupen64plus forgives all of the above. The faults of this list were found by reading. `tools/hw_check.py` is the guard that
  stays in the build. Test on the console.

## Imported models

- `tools/custom_model.py` reads the glTF itself (accessors, node transforms, embedded images) and writes one
  normalised glTF per piece for `objbank-tool pack`, then `texbank-tool pack` and `objbank-tool unpack` (the
  previews). The editor stores each import in `GLOVEBOX_HOME/custom/<set>/` (`meta.json`, one single-object bank
  per piece, `textures.tex.bin`, `textures/`, `preview/`). `<set>` is a hash of the file and the scale.
- libgarib takes an object's ID from `hash_str` of the root node's name, and the mesh name from its first 8
  characters. The converter pads the short name with NULs and appends a hash. `lg_id` metadata would be simpler,
  but libgarib's `MetadataManager.__setitem__` fails on it (`field.validator` on the enum member).
- libgarib takes a texture's ID from the image file name (`textures/0xIIIIIIII.png`) and its format from the
  PNG's `Comment` JSON. Every triangle needs a texture: `deriveScaledUVs` reads its size.
- A texture record is padded to a multiple of 8 bytes when `core.texbank_records` reads it (Real hardware, above); libgarib's packer leaves it as it is.
- 2,048 texels is the most one `G_LOADBLOCK` loads. Texture coordinates are s10.5 in texels, so the converter
  moves repeating coordinates by whole repeats, per piece, then per triangle, then clamps.
- IDs avoid every object and texture ID in the game's banks and in other imports.
- Builds: `plan_imports` treats custom pieces like copied imports (`model_bank`, texture source `custom:<set>`).
  `plan_runtime` leaves them out of the run-time banks and puts them in `plan["copy"]`, which `build_rom`
  copies even when the mod loads the other imports. The editor's `runtime_imports` is false when every import is
  custom.
- `modCustom` (`mod/banks.c`: magic `CUS1`, a count, then per key the cart addresses of a texture bank and an object bank) holds
  generated models stored behind the mod (`tools/mod_patch.py`, `apply_mod(custom=...)`). `Mod_LoadExtraBanks` loads them with
  `LoadCustomBanks`, a copy of the game's own bank loaders that puts the banks into the mod's extra lists. The editor doesn't use it
  yet: its imports go into the level's banks or the run-time extra banks.

## The Windows app

- `core.py` and `prepare.py` keep their writable files (`config.json`, `data/`, `cache/` and the JSON in `web/`)
  in the folder that `GLOVEBOX_HOME` names, or next to the code without it. The launcher sets `GLOVEBOX_HOME` to the user folder.
- The executable keeps the repository's layout (`core.py`, `desktop/`, `tools/`, `vendor/`...) as plain files in its
  unpack folder, and runs them from there. `__file__`-relative paths work unchanged there. PyInstaller adds the
  interpreter and the libraries. `build_app.py` lists them by parsing every staged `.py` file's imports
  (`hidden_imports`), because PyInstaller does not analyse code it only copies.
- Frozen, `sys.executable` is the app. `tools/pycmd.py` turns `[python, script, args]` into
  `[app, --run-script, script, args]`, and the launcher runs the script with `runpy`. Each libgarib tool still runs
  in its own process: libgarib's parsers rebind `KaitaiStruct.getConstructType`, so two of them in one process
  break level building. In onefile mode a child of the app reuses the parent's unpack folder.
- `build_app.py` applies `patches/` to `vendor/libgarib` before staging, and writes libgarib's version into
  `_static_version.py`. Without that, every `import libgarib` runs `git describe`. `prepare.py` patches only when
  `vendor/libgarib` is a git checkout.
- The app is made for Windows only. `build_app.py` stops on other systems; the editor still runs
  from source anywhere (`python -m desktop`).
- PyInstaller has no hooks for unicorn or keystone. The build adds `unicorn/lib/*` by hand and collects keystone's
  binaries.
- One copy of the app uses a user folder at a time (`editor.lock`, `QLockFile`). A second copy says so and exits.
  The app opens no port.
- The launcher hashes the code and data `prepare.py`'s output depends on (`stamp()`). When a new version changes
  the hash, it prepares again from the ROM copy and clears the cache.
- Preparing takes seconds. The editor's first start then takes minutes, because the catalog converts every object
  bank (in a background thread, with a note over the view). This is the same from source.
- The user folder must be new or empty the first time. The launcher then writes `glovebox-folder.txt`
  into it, and later accepts it because of that file. `--reset` refuses while the editor runs.
- `config.json` stores the gloveworld folder's path. In the app that is `app/decomp/` in the unpack folder, which
  changes at every start. The launcher rewrites it before it imports `core.py`.
- The .exe is windowed (`--windowed`): no console. CPython leaves `sys.stdout` and `sys.stderr` None when a process
  has no valid standard handles (a double-click), so the launcher sends them to `editor.log` in the user folder. A
  child started with pipes (`core.py`'s `tool()`, prepare.py in the setup window, the self-test) gets real streams
  from them (PyInstaller's onefile parent passes its handles on), and prints into the pipes as before.
- In a windowed build an uncaught exception opens a modal dialog and the process waits for a click: `core.py`'s
  `tool()` would hang. `run_script` and `--self-test` catch every error and print the traceback instead.
- `--help` and `--self-test` typed in a command prompt print nothing (a windowed program doesn't attach to it).
  Redirect them: `Glovebox.exe --self-test > report.txt`.
- The .exe's icon is drawn by `desktop/appicon.py` (the window uses it too). Its version details come from
  `APP_VERSION` (the release tag; 0.0.0 without one).
- OpenGL: `prepare_qt()` makes Qt use the graphics driver's OpenGL (`AA_UseDesktopOpenGL`). Without it Qt can switch
  to its own software `opengl32sw.dll` on a driver it distrusts, while PyOpenGL keeps calling System32's
  `opengl32.dll`: an empty view. Qt gives a lower version instead of failing when the driver can't do 3.3, so
  `gl_problem()` checks the version (and compiles the shaders) before any window opens, and says what to do.
- Windows: PyInstaller's bootloader turns on Control Flow Guard for the whole process. Unicorn jumps into the code
  it generates, so libgarib's ROM tool died with `0xC0000409` and no output (Build ROM failed in v1.0.0).
  `build_app.py` clears the flag (`clear_guard_cf`). `--self-test` runs all three engines in a child process
  (`app/engine_check.py`), and a failing tool's error names its exit code.
- CI: GitHub's Windows runner has no graphics card. The workflow installs Mesa (llvmpipe) as the system's OpenGL
  driver (`systemwidedeploy.cmd`, the `MSOGL` key) to run `desktop_test.py`. Qt loads `opengl32.dll` from System32
  only, so a copy next to `python.exe` would split Qt and PyOpenGL. The app never includes Mesa.
- The build leaves out readline (GPL 3) and OpenSSL (`ssl`, `_hashlib`); hashlib then uses its own SHA-1. The
  archive's `licenses/` folder holds each library's licence files at their path in the library. Releases carry the Unicorn and Keystone sources.
- The first start opens the setup window (`desktop/setup.py`). The player picks the ROM; the launcher converts
  `.v64` and `.n64` to `.z64` and checks the SHA-1 (`270be17b…`, as in gloveworld's `NGVE.sha1`).
- `--self-test` also loads Qt with its native platform plugin listed, PyOpenGL and the editor's modules, without a
  screen (Qt's offscreen platform).

## Notes and limits

- Every built ROM and patch walks the ball with Easy's controls on every difficulty: on Medium and Hard the game
  otherwise pushes the ball the opposite way to the stick. Only the controls change; lives, garibs per life and the
  bosses still follow the chosen difficulty (`tools/mod_patch.py`, `EASY_CONTROLS`).
- `data/`, `cache/` and every built ROM come from your ROM. Keep them local.
- Fixes to libgarib needed here (all in `patches/`): animated models crashed glTF export (missing
  `scale_factor`); numpy scalars weren't JSON-serialisable; missing textures, zero-length normals, and mixed
  textured/untextured meshes aborted the whole bank. A second patch lets `SetActorRotation`/`SetActorScale`
  sit inside enemies, platforms, powerups and water (the ksy semantics and the RelaxNG schema made from them).
  A third makes glTF packing work with NumPy 2 and with real models: vertex indices overflowed, a normal of
  exactly 1 overflowed its byte, a mesh without vertex colours stopped the display list, and more than four
  texture coordinates at one position stopped the import.
- libgarib's `quickboot(level_id=N)` assembles `N` as hex. A bare `10` boots Carnival 2. The editor passes
  `0x..` explicitly (found by checking `StartNewLevel` in the decomp).
- Rendering is approximate: static poses, Lambert lighting, garibs/tips as icons.
- Enemies from another world bring their own model, but projectiles come from the game's bullet table and
  are looked up as optional (non-vital `FindObject`). They may be invisible. Boss enemies are tied to their
  arenas and are marked in the palette.
- `.lev`/XML exports that use imported models, or rotate or scale enemies, platforms, powerups or water, only
  work in a ROM built by the editor (it warns).
- Platform rotation used to be stored as a speed-0 `PlatConstantSpin` on one axis. It is now a
  `SetActorRotation` and needs the game mod. Levels edited before keep working: the spin's initial angle is
  still read and shown.
