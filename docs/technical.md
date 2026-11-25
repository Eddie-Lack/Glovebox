# Technical notes

_Back to the [README](../README.md)_

How the editor is put together, what the game does with a level ("Facts from the decomp"), and the rules a built ROM
has to keep ("Real hardware").

The editor is a desktop level editor for Glover (N64, USA, game ID NGVE). It is one native program written in Rust: an egui window
with a wgpu view (`crates/app`) over the editor's state (`crates/editor`) and the back end (`crates/core`), all in one
process. It installs and unpacks nothing, and the only other program it starts is the player's emulator, for Test level
(see "Test level"). It builds on three projects:

| Source | Used for |
|---|---|
| **libgarib** (naclomi) | The formats and the ROM tools. Its descriptions of the level file and of the retail ROM's layout are in `formats/`. Its algorithms are ported to Rust in `crates/core`: `.lev` ↔ XML (the round trip gives the same bytes on all 48 levels), the XML's rules, object banks and their models, texture banks, FLA2, repacking and relocating the ROM's data, and the `copyskip` and `quickboot` patches. `copyskip` skips the copyright screen. `quickboot` boots straight into one level. |
| **noclip.website** | The level list. Its scene table supplies level names and the object and texture banks of each level (`crates/core/data/scenes.json`), and its enemy and powerup model tables are `noclip_tables.json`. The editor does not use its placement logic: noclip rotates actors in the order `ZYX` and swaps the X and Z scales (see "Selecting, rotating, scaling, adding"). The editor names a bank by the file name that noclip's ROM splitter gives it (`HUB_PART1.obj.fla`). The splitter is noclip's tool that writes the ROM's banks to files. The editor does not run it. |
| **gloveworld** decompilation (`../gloveworld`) | What each opcode means. The decomp names each opcode (`enum Token`) and points to the C that parses it. It gives real names for 31 opcodes that libgarib only has placeholders for (e.g. `PlatMvspn0x58` = `TOKEN_SPINTOMOVE`). It also supplies enum names for enemy types, puzzle conditions/actions and enemy instructions. The editor's checks also come from it: they run the game's own level-parser warnings on your edits, and the limits past which the game calls `Crash()`, its error report. `tools/build_xref.py`, `tools/game_banks.py`, `tools/measure_heap.py` and `tools/build_mod.py` read a gloveworld checkout. `build_mod.py` looks in `../gloveworld` by default. The program does not need one. |

## Where things are

| Path | What |
|---|---|
| `crates/core` | `glovebox-core`, the back end: the formats, the game data read from the player's ROM, ROM and patch builds, Import model (the conversion of the player's own models), the kit (props made by code). No window. |
| `crates/editor` | `glovebox-editor`: the editor's state and every edit, the entities, the scene graph, the checks. No window, no drawing. |
| `crates/app` | `glovebox`, the program: the window, the panels, the renderer, the user folder, and the click tests (`tests/ui.rs`). The user folder is where the app keeps its copy of the ROM, the drafts, the settings and the other files it writes ("The Windows app"). |
| `crates/core/examples/gbtool.rs` | The back end on the command line: a level's XML, a ROM or patch build, the hardware check, a level pack for g64coopdx, the import of a glTF model, and a glTF model as a pack's `models.bin`. |
| `crates/core/data/` | The tables compiled into the program (below). None holds bytes copied from the ROM. |
| `formats/` | libgarib's description of the level file (`glover.lev.ksy`, with one change of ours: `SetActorRotation` and `SetActorScale` may also sit in a platform, enemy, powerup or water) and its map of the retail ROM. The program does not read them: `tools/gen_formats.py` and `tools/gen_rom_map.py` turn them into JSON tables. |
| `mod/` | The Expansion Pak game mod ("Expansion Pak game mod", under "Selecting, rotating, scaling, adding"): `transforms.c` (rotation and scale for every object), `heap.c` (larger heap), `banks.c` (models and textures from other banks), `guard.c` (the matrix guard, which skips draws that would pass the frame's 256 object matrices), `slots.c` (extra levels), which run from 0x807E0000; `aligned.c` (the mod's one buffer that must sit on a 16 byte boundary, the header of an extra level's file); `loader.c` (the loader, written over `ScreenShot`, a debug function the game never calls); and the committed build outputs `mod.bin` and `mod.json`, which the program compiles in. |
| `tools/build_mod.py` | Builds `mod/` with gloveworld's GCC 2.7.2 against gloveworld's `build/NGVE.elf`, finds the call sites to hook and writes `mod.json`. |
| `web/patcher.html`, `tools/build_patcher.py` | The offline patcher page for players. It applies a BPS patch to the player's own ROM ("Sharing levels: patches and the patcher"). After editing the page's script, run `tools/build_patcher.py`. It writes the script's new hash into the page's Content-Security-Policy. |
| `tools/licenses.py` | Collects the licenses of the Rust crates and fonts in the program, for the release archive. |
| `tools/wiki.py` | Copies the guide between `docs/guide` and a checkout of the GitHub wiki, in either direction. |
| `app/README.txt` | The guide for players that goes into the release archive. |
| `docs/guide/` | The guide with pictures for players: one Markdown page for each job, and the pictures in `img/` (see "The guide's pictures"). |
| `.github/workflows/app.yml` | Tests, the Windows build, the archive and the release. |

The next table lists the tables in `crates/core/data/` and the developer tools that write them. The tools need Python 3 with PyYAML and
nothing else. The program never runs them.

| Table | What | Written by |
|---|---|---|
| `level_format.json` | The level file's commands, types, fields and enums, and what may sit inside what | `tools/gen_formats.py`, from `formats/glover.lev.ksy` (`--check` fails when the table is out of date) |
| `rom_map.json` | Where the retail ROM keeps each bank, level file and pointer | `tools/gen_rom_map.py`, from `formats/map.0x8e6e01ff.0xccb4f948.yaml` (the two numbers are the retail ROM's header CRCs) |
| `decomp.json` | The level format joined with the decomp: token names, parser locations, enums, diagnostics | `tools/build_xref.py --gloveworld DIR`, from gloveworld's sources and `formats/glover.lev.ksy` |
| `bank_numbers.json` | The game's number for each bank | `tools/game_banks.py --gloveworld DIR --numbers`, from `LoadObjectBank` and `LoadTextureBank` in gloveworld's `src/banks.c` |
| `heap.json` | Free heap per level | `tools/measure_heap.py --rom ROM --gloveworld DIR`, measured in gloveworld's emulator harness (`tools/emu`) |
| `scenes.json` | The 48 levels: name, level file and banks, in noclip's scene table and in the game's `LoadLevel` | No tool. See below |
| `noclip_tables.json` | Which model each enemy and powerup type uses | No tool. See below |

- `build_xref.py` and `game_banks.py --numbers` read gloveworld's sources only. Run on gloveworld at the commit that
  `decomp.json` records (`source.gloveworld`), they write the same bytes as the committed files.
- `scenes.json` and `noclip_tables.json` were made once from noclip.website's scene table and enemy tables
  (`src/Glover/scenes.ts` and `enemy.ts` in noclip.website's source) joined with the banks the game's `LoadLevel`
  loads. `tools/game_banks.py` prints those banks, and with `--compare crates/core/data/scenes.json` it compares them
  with the table. It does not write the table. The two tables describe the retail game, which does not change. They
  are committed data without a generator.

`crates/core` by module:

| Module | What |
|---|---|
| `xml` | The level XML as a tree (`Doc`, `NodeId`), a hardened parser (no DTD, no entities) and the writer. |
| `pyfmt` | Numbers as the level XML writes them: floats as Python's `repr` writes them, integers above 10000 as hex. libgarib, a Python library, defined the XML ("Reference tests"). |
| `level` | `.lev` ↔ XML (`disassemble`, `assemble`) and the XML's rules (`validate`). |
| `fla2` | FLA2, the compression of the game's banks. |
| `objbank`, `model` | Object banks: a hardened parser, the linker that writes a bank, `split` into single-object banks, `pack` (adds the objects of single-object banks to a bank), and the model the editor draws for an object. |
| `texbank` | Texture banks: records, decoding to RGBA, writing a bank. |
| `n64`, `rom` | The byte order of a dump, the boot code and the header CRCs; the retail ROM as regions, and a rebuilt ROM with some replaced, the `copyskip` and `quickboot` patches. |
| `gamedata` | The game's data read from the ROM and kept in memory: levels, banks, the catalog of every model, what each level has loaded. It writes no file. |
| `imports` | Which banks an edited level needs for the models it imports, and the enlarged banks a build writes. `plan_imports` plans a build that copies the models into the level's banks. `plan_runtime` plans a build whose game mod loads their banks from the player's ROM when the level starts. |
| `build` | ROM, patch and level file builds (`assemble`, `build_rom`, `build_levels`). |
| `modpatch` | Adds the mod to a built ROM, with the extra banks each level loads and the extra levels. Also writes the level table and the ball controls (Easy's controls on every difficulty, "Notes and limits"). Checks each word before it replaces it. |
| `hwcheck` | Checks a built ROM against the rules of real hardware ("Real hardware"). `modpatch::apply_mod` runs it on every ROM that gets the mod. `self_test` breaks a copy of a ROM in each way that the rules name, and checks that each break is found. |
| `bps` | The BPS encoder and decoder. |
| `custom` | Import model: `gltf` (the reader), `image` and `jpeg` (PNG and JPEG to pixels, the resize), `pack` (triangles to an object bank), and the stored sets. A set is one imported file's pieces and textures in the user folder ("Imported models"). |
| `kit` | The kit: models made by code, their textures and previews. |
| `backend` | `Backend`, what the window asks the back end for. `GameBackend` answers from the ROM and the user folder. |
| `pack` | Level packs for g64coopdx ("Level packs for g64coopdx"). |
| `pyrandom` | Python's `random.Random` seeded with a string: the Mersenne Twister with CPython's seeding. Reskin picks textures with it. |
| `tables` | The compiled-in tables and the mod. |

`crates/editor` by module:

| Module | What |
|---|---|
| `editor` | The state and every edit: loading (`load`), selection (`select`), the transform engine (`transform`), undo and adding and removing (`edit`), commands, scripts, puzzles and camera zones (`cmds`), the checks, the draw load (how hard the busiest spot is to draw), the imports and their memory budget (`checks`), the path tool (`path`), the entities' scene nodes (`nodes`). A queue of events tells the window what changed. |
| `entities` | Entities from the level XML (`build`), bindings, attached points, trigger volumes, camera zones. A binding says how a point of an entity is read from the XML and written back. Attached points are the points that move with their object, such as an enemy script's points. |
| `exits` | Exit triggers: the radius the game gives a platform (`radius`) and who must come within it (`rule`). |
| `scene`, `camera`, `gizmo` | The 3D view's data: a small scene graph with picking, orbit and fly cameras, the gizmo. |
| `commands`, `puzzles`, `scripts`, `presets` | The level format's order and rules, puzzles, enemy scripts, platform presets ("Commands, puzzles, enemy scripts, extra levels", below). |
| `sanitize`, `drafts`, `xmlutil`, `vec` | Import checks, drafts as files, XML helpers, vector math with three.js's conventions (Euler order `'YZX'`). |
| `fixture` | A back end without a ROM: two made-up levels and a few made-up models. The tests and `--fixture` use it. |

`crates/app` by module:

| Module | What |
|---|---|
| `shell`, `main.rs` | The options, the user folder with its marker file, lock and log ("The Windows app"), the ROM copy, the choice of graphics adapter, the self-test. |
| `app`, `setup` | The application: the first-run screen or the editor's window, opening levels, builds, Test level (a build with quickboot, opened in the emulator), the keyboard. |
| `toolbar`, `outliner`, `inspector`, `checks`, `palette`, `path_panel`, `dialogs` | The panels and the dialogs. File dialogs are the system's (`rfd`), behind a trait that the tests replace. |
| `layout` | Which panels show and where: the layout, its settings, the columns around the view, the Panels menu. |
| `viewport`, `render` | The 3D view: the mouse and keys, the HUD, and the wgpu renderer (`render/shaders.wgsl`). |
| `jobs` | Slow work (opening a level, builds, imports, model pictures) on worker threads. |
| `settings`, `style`, `theme`, `icon` | `settings.ini`, the drawing of the active theme (the widgets' shapes, title bars, the logo), the themes as data, the app's icon. |
| `tour` | The welcome on the first start, and the tour of the window. |
| `numfield` | Number fields that change by dragging. |

## Models and drawing

- A model is named by a `ModelRef` (a game object's id, or a piece of an imported set). `Backend::model` and
  `Backend::texture` give the model and the pixels.
- The level XML is a `Doc` tree. Undo snapshots and "is this the shipped level?" compare its serialized text.
- Every edit rebuilds the entities from the XML. Models are parsed once per level.
- Drawing: opaque meshes are merged into one world-space buffer per material after each rebuild (`make_batches`).
  What moves without a rebuild (a gizmo drag, the motion preview) is drawn on its own. The renderer draws into its
  own texture, which the window shows as an image.
- Lighting follows three.js r160 (Lambert's albedo / π, ambient 0.8, sun 1.4, sRGB output). Sprites keep their
  world size, as three.js sprites do.
- Picking runs on the CPU: a ray against each mesh's triangles, sprites as camera-facing squares, lines
  within 5 pixels.
- The round icons of things without a model (garibs, Mr. Tip) are drawn by the program's own shapes
  (`render/icons.rs`). They do not depend on a system font.

## Panels

- The player arranges the panels (`layout.rs`).
  - There are five panels (`PanelId`): Level, Inspector, Checks, Add to level and Path. The view is in the middle and
    takes the room they leave. With every panel hidden it fills the window.
  - A `Layout` holds, for each panel, its side (left or right; the checks can also be at the bottom, under the
    view) and whether it shows, and for each side how its panels share it: `Tabbed` or `Stacked`. The default is
    Level on the left, and on the right the inspector over the checks. Add to level and Path are tabs beside the
    inspector. Both sides are `Tabbed`.
  - Add to level and Path show while their tool is on. The layout keeps their side only. Hiding one turns its
    tool off, and `+ Add`, `A`, `Path` and `P` show it again.
  - Each side is one column (`Window::columns`), an egui side panel. `Tabbed` draws a row of tabs and the panel
    in front. A column with one panel has a title bar and no tabs. `Stacked` draws the panels one under the other,
    each with its title bar, as egui top panels with a divider to drag. The checks are never a tab: they are the
    lowest part of their column, with a divider over them, or a bottom panel under the view.
  - `Window::front` lists the tabbed panels, the one chosen last first. A column shows the first of its panels in
    that list. Opening Add to level or Path, a click on a tab, and moving a panel to another side bring it to the
    front.
  - On a tabbed side of a window at least 1560 points wide (`layout::WIDE`), Add to level and Path get a column of
    their own beside the side's other panels. They stay tabs when that would leave the view less than 480 points
    (`layout::MIN_VIEW`) at the widths the columns have. No other rule limits the view: on a small window the
    columns keep their minimum widths (300 points for the inspector, Add to level and Path, 200 for Level).
  - The setting `panels` holds the layout as one value, for example
    `level:left:on,inspector:right:on,checks:right:on,add:right,path:right,left:tabbed,right:tabbed`.
    `Layout::from_text` wants all seven parts, each once, with known words. Anything else gives the default layout.
  - The setting `panel_sizes` holds the sizes the player dragged, as `name:points` parts: a column's width under
    the name of the panel in it that needs the most room (`inspector`, `add`, `path` or `level`, and `checks-column`
    for a column with only the checks), the height of the checks (`checks`, `checks-bottom`) and of a stacked panel
    (`stack-` and the panel's name, as `stack-level`). A width stays with its panel when the panel changes sides. A
    size is written when it changed while a mouse button was down. At the start, a part with a kept size starts at
    that size instead of its default. A part that can't be read is left out.
  - `Reset layout` removes both settings' effect: the default layout, and new ids for the egui panels
    (`layout_gen` is part of each id), which drops the sizes egui kept.
  - The controls: `Options` > `Panels` (`layout::menu`, a submenu that stays open while its entries are clicked)
    has a checkbox and a side for each panel, `Stacked` or `Tabbed` for each side, and `Reset layout`. Each title bar,
    each row of tabs and the checks' heading has a small menu (`panel_menu`): `Move to left`, `Move to right`,
    `Move to bottom` for the checks, and `Hide`. Panels are not dragged to dock them.
  - `Window::panels` has the rectangles of the last frame: the columns of Level and of the inspector, the checks,
    the column the tool panels have for themselves, and each panel's own part of the window (`slots`).
  - With the inspector hidden, a selection writes what was selected to the status line at the bottom of the window
    (`Window::pump`).
  - `egui_dock` is not used. Its newest release (0.21.1) needs egui 0.36, and the app uses 0.37.
- A panel's size comes from its default and from drags on its edge. The default widths are in `layout.rs`:
  `LEVEL_WIDTH` 270, `INSPECTOR_WIDTH` 380, and `ADD_WIDTH` 360 for Add to level and Path. The checks start at 36%
  of their column's height (180 to 320 points), and at 200 points under the view. A column has the default and the
  limits of the panel in it that needs the most room. Its contents never change it:
  - `style::fixed` shows a panel's contents in a child `Ui` of the panel's size and clips it. Every panel uses it.
  - egui grows a panel to fit its contents and remembers the larger size. A card is a box with a title bar
    (`style::card`). A card measures the buttons of its title bar in its first frame and is too wide in that frame.
    Without a fixed size, each new selection widens the inspector, up to its largest width, and it goes back there
    after every drag.
  - Contents must fit the default width. Each scroll area reports how much wider its content was (`style::fits`),
    and the click test `panels_resize` checks that this is zero for every kind of object, all objects selected at
    once, a puzzle, the level's settings, every tab of Add to level and the Path panel.
  - A widget that is too wide also widens every row after it in the same `Ui`: egui moves the layout's right edge.
  - The 3D view does not take the pointer in the strips along its left and right sides (egui's
    `resize_grab_radius_side`), nor in the strip along its bottom when the checks are under it. The view is added after the panels, and it would otherwise take the half of each resize handle that
    lies over it.
- A color is picked in the inspector. There is no color dialog.

## Themes

- The look is a theme. The themes are data in `theme.rs`; `style.rs` draws everything from the active one.
  - A `Theme` holds the colors of every surface and text, the tones of the buttons (`Tone`: fill, text, line), and
    one variant for each part that has a shape: `Shape` (buttons), `Field` (text boxes), `Bar` (title bars), `Logo`,
    `Divider`, `Backdrop` (behind the setup screen), `Border` (the frame of the setup box, of a dialog and of the
    box in the middle of the 3D view that shows loading or an error) and `Mark` (the signs of the Checks panel). It
    also names the fonts, the chips over the 3D view, the colors of the selection and of the box select, and the 3D
    view's background for a level without fog. The chips are small labels over the view, such as the level's name
    and the autosave chip. The box select is the box that `B` drags to select.
  - `THEMES` lists them. The first is the default, 1998. The three plain themes come next, then the seven palette
    themes, then the four styled ones (Cartridge, Blueprint, Paper and CRT). `PALETTES` lists the palette themes
    again: the Theme list shows them in a column of their own.
  - The plain themes (Light, Dark, High contrast) use the system's sans face (Segoe UI on Windows, then Tahoma,
    Arial, Liberation Sans, DejaVu Sans). They have the plain variants `Logo::Plain`, `Divider::Line` and
    `Backdrop::Plain`, and title bars that are a flat strip with a line under it. They use `Bar::Solid`, which
    draws a light line over the bar and a dark line under it, and give the light line the color of the fill.
  - The palette themes (Sage, Sand, Ocean, Lavender, Coral, Slate, Plum) look like a phone app. Each has five
    colors: a ground, a card, a main color, a second color and the text. Every other color is a mix of those
    five: the toolbar is the ground with a little of the main color, a hairline is the ground with a little of
    the text. They share the fonts and the plain variants of the plain themes. Their own shapes: buttons and
    fields with a corner radius of 6 (`radius`), cards, check lines (the lines of the Checks panel), chips, pop-ups
    and dialogs with one of 8 (`round`, 0 in every other theme), and `Bar::Soft` title bars (a tinted strip for a
    panel or a card, a hairline under a small heading). A switch is a toolbar button that stays down while its tool
    is on, such as `Fly`. A switch that is on is a tint of the main color with a line and bold letters in it. The main action and the chosen row are filled with the main color. A checked box is too, with a check mark in
    the color of the main action's letters. Good, warning and bad are a green, an amber and a red that suit the
    palette. Coral's bad is a crimson, which differs from its coral main color. The 3D view of a level without fog is a dark tone
    of the palette.
  - `Theme::disabled` is the opacity of a control that cannot be used (egui's `disabled_alpha`): 1 is fully opaque.
    1998 and the styled themes have 0.55. The plain ones have 0.6 or 0.62 and the palette ones 0.6 to 0.71: their disabled text keeps a
    contrast of 4.5.

    | id | name | follows | main choices |
    | --- | --- | --- | --- |
    | `1998` | 1998 | the patcher page (`web/patcher.html`), a game web site of 1998 | navy panels, gray beveled buttons, sunken white fields, red title bars with yellow capitals, a slanted yellow logo, a rainbow divider, stars |
    | `light` | Light | a desktop tool in the system's light colors | neutral grays, white buttons and fields with a thin line and a corner radius of 3, one blue for the selection, the focus and the main action, a switch that is on filled dark gray, no decoration |
    | `dark` | Dark | the same tool in the system's dark colors | near-black grays, one light blue for the selection, the focus and the main action, a switch that is on filled light gray |
    | `contrast` | High contrast | the system's high contrast mode | white on black, a white line around every control, yellow for the pointer and the focus (a focused button is filled yellow), cyan for the selection and the main action, a switch that is on white with black letters |
    | `sage` | Sage | a calm phone app in muted greens | a very pale green ground (`e9f0e4`), off-white cards, a deep sage (`456f52`) for the main action, the selection and the focus, moss headers, an ochre box select |
    | `sand` | Sand | the same app in warm paper tones | a sand ground (`f4eee3`), cream cards, a caramel brown (`a65f2b`), a teal box select |
    | `ocean` | Ocean | the same app in sea tones | a pale blue-gray ground (`eaf2f6`), near-white cards, a deep teal blue (`1b6e8c`), an orange box select |
    | `lavender` | Lavender | the same app in violets | a pale lilac ground (`f2eff8`), near-white cards, a violet (`6650b0`), a rose box select |
    | `coral` | Coral | the same app in warm pinks | a blush ground (`fbf0ec`), near-white cards, a coral red (`c54e3e`), a teal box select, crimson for bad |
    | `slate` | Slate | a dark phone app in blue-grays | a slate ground (`222932`), lighter slate cards, a soft teal (`6cc4b6`) with dark letters, an amber box select |
    | `plum` | Plum | a dark phone app in warm purples | a plum ground (`2a2030`), lighter plum cards, a rose (`e79bb8`) with dark letters, a gold box select |
    | `cartridge` | Cartridge | a gray console of the late 1990s and its controller | warm gray panels, charcoal label plates with white capitals, keys with a lip, the four face-button colors for states only (green for on and good, red for the main action and bad, yellow for warnings, blue for the selection) |
    | `blueprint` | Blueprint | a drafting sheet | blue paper with a grid, hairline controls without fills, titles followed by dimension lines, a ruler for a divider, drawn signs |
    | `paper` | Paper | a printed manual in two inks | off-white paper, black ink and vermilion, a serif face, rules instead of boxes, fields as tinted blanks with a line under them and typed entries |
    | `crt` | CRT | a debug monitor | black with green phosphor in one fixed-width face, amber for the main action, inverse video for what is on, chosen or under the pointer, double-line frames, two-letter codes for the checks |

  - `style::apply(ctx, theme)` sets a theme for a window: the fonts, egui's visuals and the widget themes. It keeps
    the theme in the egui context's data, and `style::theme(ctx)` reads it there. Each window has its own:
    tests with different themes run side by side. `apply` can run at any time. The next frame is in the new
    look.
  - The Theme list of the toolbar's Options menu (`toolbar::options_menu`) has two columns: the default, the plain
    and the styled themes, then the palette themes. A click calls `Window::set_theme`. It writes the id to `settings.ini` as `theme` and
    applies the theme after the frame. `App::new` reads the setting before the first frame, for the setup screen
    too. An unknown or missing id is `1998`.
  - egui's buttons, text boxes and separators get their shapes from egui's widget themes, which are not the app's
    themes. The app's widget theme is `Look`: it draws those widgets in the shapes of the active theme. It is
    registered with `replace_widget_theme`, which needs egui's `experimental` feature. A toolbar switch that is on
    has the class `CLASS_ON`, the main action (Build ROM) `CLASS_PRIMARY`. An entry of a list or a menu is flat:
    the chosen one and the one under the pointer take the theme's tones `sel` and `hover`.
  - egui marks the entries of a menu with a transparent `weak_bg_fill`, and `Look` reads that mark. `apply`
    therefore never sets a transparent `weak_bg_fill`, even in a theme whose buttons have no fill.
  - An egui frame has one stroke. A bevel is a light stroke and a dark hard shadow of one pixel. A key's lip is a
    hard shadow under it. The line under a Paper field is a hard shadow that shows below its tint only.
  - Title bars (`title_bar`, `h4`, the head of a `card`) show their text in capitals. The accessible name keeps the
    text as written. A card whose title is a command's tag (`FogConfiguration`) is not put in capitals: in capitals
    its words would run together.
  - Checkboxes are drawn by `style::checkbox`. The Level panel has a layer for each kind of object the level has,
    and one for the exit triggers. Each layer has a checkbox that shows or hides it. A layer's box takes the
    layer's color.
  - Fonts: each theme names font files of the computer for four families (text, `bold`, `label` for title bars,
    `logo`). `style.rs` looks for them in the system's font folders. A family without a file takes the family
    before it, and every family ends with the fonts compiled in: egui's own, with its code font Hack. CRT uses the
    code font compiled in. No font file is in the repository.
  - The editor's state (`crates/editor`) gives a level without fog a fixed background color. `theme::VIEW_BG`
    holds the same color. The view replaces that color with the theme's `view_bg`. 1998 keeps `VIEW_BG`; the render
    comparison tests (`crates/app/tests/render.rs`, "Reference tests") use it.
  - To add a theme: write a `Theme` in `theme.rs` and add it to `THEMES`. Add a palette theme to `PALETTES` too:
    the Theme list puts those in its second column. A new variant of a shape needs its drawing in `style.rs`. A
    theme must keep these rules:
    - No file outside `theme.rs` holds a color of the window. `style.rs` only picks black or white for a check mark on a
      colored box, and shades a mark with a see-through white or black.
    - The panels, the cards, the toolbar and the pop-ups are all dark or all light: `text` and `muted` are read on
      each of them.
    - Layout does not depend on the theme: the same margins, row heights and frame widths (`RIDGE`). Only the
      width of the text differs.
    - A switch that is on, the main action, the keyboard focus, the chosen row, and good, warning and bad must
      each be clear at a glance.
    - Text keeps a contrast of 4.5 on its ground (3 for bold text and for the marks): body, muted, disabled and
      field text, the chosen row, the buttons, and the good, warning and bad colors.
    - `GLOVEBOX_THEME=id cargo test -p glovebox --test ui` runs every click test in that theme. The test `themes`
      chooses each theme through the Options menu at three window sizes and checks that the toolbar keeps to its
      two rows. `the_fonts_have_the_symbols` checks that the fonts of each theme have a list of arrows, shapes and
      punctuation marks.

## Info texts

- A note is a text that explains how the game or the editor works: the small muted text under a title, a field, a
  group of fields or a row, the second line of a check, the paragraph under a dialog's title, the key hints over
  the view. The Info texts list of the Options menu sets how notes show: `Shown`, `On hover` or `Off`. The setting is
  `info` in `settings.ini`; `Window::set_info` writes it and `App::new` reads it. Without a setting the notes are
  on hover (`Info::DEFAULT`).
  - Every note is drawn by the helpers under "Notes" in `style.rs`: `note`, `note_at`, `note_on`, `card_note`,
    `src_on`, `para`, `para_at`, `para_on`, and the `sub` line of `check_line`. The state is in the egui
    context's data (`style::info`), as the theme is.
  - A text about the player's own level is not a note and always shows: values, counts, status lines, the main
    line of a check, a wrong value's message, refusals, the notices about the Expansion Pak. Those use `muted`,
    `remark`, `src` or `check_line`.
  - `Shown` draws the note as a label. `Off` draws nothing. `On hover` draws nothing in the panel and lays an
    area that senses only the pointer over the thing the note explains. The area has the note as its tooltip.
    Clicks and drags go to the widgets under it. A widget's own tooltip shows with it.
  - The thing a note explains is its anchor. Title bars, headings, rows of fields (`anchor_row`), checkboxes and
    card titles name themselves as the anchor when they are drawn (`style::anchor`), and `note` takes the last
    one. `card_note` takes the whole card and its title. `note_on` and `para_on` take a place the caller gives.
    A note with no anchor within 48 points of it shows as a small "?" with the tooltip.
  - On hover, a dotted line under the anchor's letters is the cue that it has a note. Check lines, buttons and
    tabs get no cue.
  - A check's second line joins the check's own tooltip on hover (`tip_with_note`). The key hints over the view
    become a chip "Keys ?" with the hints as its tooltip.
  - A new explanatory text must use one of the note helpers. The test `info_texts` checks the three states.
  - The click tests read the notes in the panels. Their harness writes `info=shown` into the settings
    (`T::with_theme`). `info_texts` starts without the setting.

## The welcome and the tour

- The welcome and the tour are in `tour.rs`. The welcome is a dialog with `Show me around` and `No thanks`. It shows
  the first time a level is open and the settings have no `tour=seen`. An answer, Esc or a click beside it writes
  `tour=seen`. `Take the tour` of the Options menu starts the tour at any time (`Window::start_tour`).
  - The steps are data: `tour::STEPS`, each with a title, a text, a `Target` and a `Show`. The last step's text
    depends on the Info texts setting (`last_text`), and it has the link to the guide.
  - A `Target` names a part of the window. The toolbar, the panels and the view report their rectangles in every
    frame with `tour::State::mark`. Several widgets of one target give the rectangle around them all (`Move`,
    `Rotate` and `Scale`). No position is written in the code. A target that is not in the window has a fallback:
    the toolbar, or the column of the inspector.
  - A `Show` says what the window shows during the step: nothing selected, one thing near Glover's start selected
    and framed, or Add to level open. `Window::enter_step` sets it when the step opens.
  - A step on the Level panel, the inspector or the checks shows that panel when the player hid it, and brings it
    in front of its tabs. Each step starts from the player's layout: a panel shown for one step is hidden again
    in the next. These changes are not written to the settings.
  - The tour does not edit the level. `Window::end_tour` puts back the layout and the tabs in front, the selection,
    the open puzzle, Add to level
    and its tab, box select, the fly camera and the camera's place.
  - The picture: `style::spotlight` dims the window around the target with the theme's scrim and draws a line in the
    focus color. The scrim is the color that dims the window behind a dialog. `style::callout` draws the bubble in
    the colors of the menus, with a pointer. `tour::place` puts the bubble under, over or beside the target, inside the window and never over the target.
    Beside a target it can be narrower. The bubble's size comes from its texts, which are laid out first.
  - The tour is an `egui::Modal` over the whole window. Clicks do not reach the window under it.
    `Window::modal` is true during a dialog, the welcome and the tour: the editor's keys, the view's mouse and
    dropped files are off. Esc ends the tour, the arrow keys and Enter move between the steps.
  - The click tests start with `tour=seen` in the settings (`T::with_theme`). The test `welcome_and_tour` starts
    without it (`T::with_settings`), and walks every step at three window sizes, and once with the inspector on
    the left and the checks hidden.

## Number fields

- Number fields change by dragging (`numfield.rs`). A field without the keyboard focus is drawn as a text box that
  takes no input, under an area that takes the press. A press that moves to the side changes the number: one step
  for each point, ten times finer with Shift, ten times coarser with Ctrl. A press without movement gives the text
  box the focus. A text box with the focus is egui's own, and a drag in it selects text.
  - The step is 1 for positions, angles and whole numbers, 0.01 for scales, and 1, 0.1 or 0.01 for other floats by
    the decimals they show. A field of the level format keeps the limits of its type (`u1` is 0 to 255).
  - A dragged value is handed to the same code as a typed one, on every frame it changes.
  - The fields leave signals for the window. It calls `Editor::begin_scrub` before the frame's edits and
    `end_scrub` after the last: the drag is one undo step. Esc calls `cancel_scrub`, which restores the XML from
    before the drag. While a drag is open, the editor's keyboard shortcuts are off.

## Sharing levels: patches and the patcher

- **Export patch** (toolbar) builds the ROM and diffs it against the original into a **BPS patch**
  (`crates/core/src/bps.rs`). It carries CRC32 checks of the original ROM, the result and the patch itself, plus JSON
  metadata: the level, whether it boots straight into it, and the SHA-1 of both ROMs.
  - A patch of a plain edit holds only the changed bytes. Data the build only moved (relocated banks) is encoded as copies from the
    player's own ROM. The Atlantis 1 garib edit is a 1.8 KB patch.
  - A patch of a level with imported models is also small when the game mod loads them. By default the mod loads imported models
    from the player's own ROM at run time. The patch has no game data. Players need the Expansion Pak.
  - A patch with copied imports holds whole banks. With "Copy the models into the ROM instead", the build rewrites the
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
  - Its look follows the practice ROM's patch page (glover.love): a 1998 game web site, with a star background,
    a ridge-framed column, red title bars and gray beveled buttons. It is all inline CSS and `data:` SVG.
    Nothing in it comes from the ROM. A change to the style or the markup needs no new hash.
  - Every read and copy in the decoder is bounds-checked.
  - A wrong ROM, damaged patch or malformed patch is refused with a plain explanation; nothing is saved.
  - Patch metadata is shown as text only.
- Patches also work in other BPS tools: RomPatcher.js applies them to the same bytes.
- Verified in the emulator: a shared patch (normal start) applied to the clean ROM boots through the
  copyright screen, studio intro, title fly-through, main menu, file select and name entry. It never reaches
  `gameInfo.mode` 13.

## Level packs for g64coopdx

g64coopdx is the online co-op game on PC ([github.com/Eddie-Lack/g64coopdx](https://github.com/Eddie-Lack/g64coopdx)).
It reads a level from a **level pack**, not from a patched ROM. A pack is a folder with two to four files:

- `pack.txt`, the manifest: `format=1`, `name=` (1 to 32 plain ASCII characters) and `level=` (the number of the
  level whose place the pack's level takes). Each line ends with a line feed alone.
- `level.lev`, the level in the game's own format: what `level::assemble_xml` gives.
- `models.bin`, only when the level needs models that its own banks lack. g64coopdx's `docs/discovery/model-banks.md`
  has its format: a header with the numbers of the game's banks it needs, then a texture bank and an object bank of
  the pack's own, uncompressed. A pack that needs only the game's banks has no banks of its own.
- `script.lua`, optional: a Lua script for g64coopdx. Glovebox does not write one.

A pack holds no game data. Its level file names models and textures by their ids. The pack's level takes the place of
the level it was made for, and the game loads that level's banks with it. Each player's own ROM supplies them.

- `crates/core/src/pack.rs` makes a pack. `make_pack` makes one from a level's XML alone. `export` adds the level's
  imports. `LevelPack::write_to` writes the files, and removes an old `models.bin` when the pack has none.
- The pack's id is the first 16 bytes of a BLAKE2b hash of its files, in the order `pack.txt`, `level.lev`,
  `script.lua`, `models.bin` (each file's name, a zero byte, its length and its bytes). g64coopdx computes the same and
  refuses a player whose pack has another id. The test `id_matches_g64coopdx` holds the ids that g64coopdx's tools
  print: its own test's id for a pack of two files, and `g64pack make`'s ids for the same pack with the cube's
  `models.bin`, with and without a script. Glovebox writes no `script.lua`. `write_to` keeps one that the folder
  has, and returns the folder's id, which counts it. The app and `gbtool pack` print that id.
- `gbtool pack ROM SID XML OUTDIR NAME [--home DIR]` writes a pack from the command line. The `Export level pack`
  button in the app's Export patch dialog does the same for the open level (`Backend::level_pack`), and shows the
  refusals in the status line. The toolbar has no room for one more button at 1280 pixels. The button opens a dialog
  that asks for the pack's name, then a Save dialog names the pack's folder.
- A level that rotates or scales an enemy, a platform, a powerup or water is refused (`level::needs_mod`). On the
  console only the game mod applies `SetActorRotation` and `SetActorScale` to them. g64coopdx applies the two
  commands to landscape and background actors only, as the unmodded game does.
- Verified with g64coopdx: Atlantis 1's XML gives a `level.lev` with the same bytes as the ROM's level file, and a pack
  of that level with its first checkpoint moved plays online in two programs with equal state hashes.

### The level's imports

- `pack::object_refs` reads the object ids from the assembled level file, at the commands g64coopdx reads
  (`g64_lev_object_ids` in its src/client/pack.c): Platform, BackgroundActor, LandActor and AnimatedBackgroundActor
  (g64coopdx's CAMEO_OBJ) at field offset 0, PlatSine at 4, PlatDestructible (its debris) and Rope at 6, PlatOrbit at
  8, Water at 38. A debris id of 0 is no debris. The XML's `object_id` attributes miss the rope's and the bridges' ids.
- An import is an id that the level's own banks lack (`GameData::loaded_ids`, as the editor's rule). `export` gives
  them to `imports::plan_runtime`, as for a ROM with the game mod. Imported models go into the pack's own banks
  (`custom::pack::model_banks`). The game's other banks go into `models.bin`'s lists, by number. `copy_imports` does
  not apply: a pack never copies the game's models. An id in no bank and in no imported model is an error.
- Each player's g64coopdx game loads the game's banks that `models.bin` names from the player's own ROM (its
  model-banks.md, "Retail banks"). The app's status line and `gbtool pack` say so (`pack::GAME_BANKS_TEXT`) when a
  pack names them.
- The game reads a sprite's texture with no test, and crashes on a sprite whose texture no loaded bank has. So
  g64coopdx refuses a pack that names an object bank of the game's whose sprites need a texture from no bank the level
  has (`g64_pack_check_retail_sprites`). It checks every object of the bank, also the ones the level does not place.
  The banks it counts are the font's (10), GENERIC (2), the level's own and the texture banks that `models.bin` names.
  `pack::add_sprite_banks` runs after `plan_runtime` and adds texture banks until every sprite texture of each named
  object bank is in one. It tries the texture banks of the levels that load the object bank first, then every other
  one. The added banks come after the plan's own. More than 4 in all is refused, and so is a sprite texture in no
  bank. `plan_runtime` itself does not change: the game mod's builds do not use the rule.
- Example: Carnival's shared bank (object bank 19) has a body, 0x23313E06, whose faces need no texture bank of
  Carnival's. Other objects of the bank have sprites with a texture of Carnival's texture bank (4). Without the rule
  the pack for Atlantis 1 named object bank 19 alone, and `g64pack check` refused it. With it, the pack names texture
  bank 4 too and passes. One object of each object bank in the scenes' catalog, placed in Hub 1, Atlantis 1 and
  the Wayroom, gives a pack that passes `g64pack check`.
- The flag of `models.bin` (the game's texture banks win over the level's own) is always 0. The game mod's builds never
  set it either.
- All 48 shipped levels export with no `models.bin` (`every_shipped_level_makes_a_pack`).
- `pack::pack_models` refuses, with a message:
  - a reskin: a set whose `meta.json` names the model it was made from
  - a model whose id is one that the game finds an enemy, a bullet or a pick-up by (`pack::GAME_NAME_IDS`, copied from
    g64coopdx's `g64_models_game_ids`)
  - a cameo (AnimatedBackgroundActor) of an imported model. The game animates a cameo, and the model has no animation.
    The message uses the palette's names: Scenery, Background and Platform.
  - a model with a mesh named `gball`, `cball`, `GShinL`, `GShinR` or `gbl_bt`. The game points the ball's and the
    hand's textures at those. The export does not rename it. The mesh's name comes from the file's name when it was
    imported. A set's folder name comes from the file's bytes and the scale alone, so an import of the same file under
    another name gives back the old set. The author removes the model, renames the file, imports it again and places
    the new model.
  - models over 4 MiB, more than 4 of the game's banks of a kind, the font or generic texture banks (10 and 2), and
    object banks 0 and 1
- An imported model as a platform is allowed. `model_banks` writes the rotation key of no turn ("models.bin"
  below), and g64coopdx's `make models` test plays such platforms. A ROM for the console still must not have one
  ("Platforms from converted models").
- The refusals need no ROM. Their tests are in `pack.rs`. `a_pack_with_imports` (in `crates/core/tests/pack_rom.rs`)
  runs the export with the ROM. It also exports the cube as a platform of Atlantis 1. With `GLOVEBOX_PACK_OUT=DIR` it
  writes that pack to DIR, and g64coopdx's `g64pack check DIR --rom ROM` passes it.

### models.bin

- `custom::pack::model_banks` builds the two banks from converted pieces. It needs no ROM. The object bank holds the
  pieces alone, linked with no game bank under them. Two pieces with the same id are an error. The texture bank holds
  the records the pieces use, padded to a multiple of 8 bytes with their size fixed, and no names after them. It ends
  with 4 zero bytes, so its size is a multiple of 8 too.
- The converter writes a rotation key of all zeros. The game turns it into a matrix of values that are not numbers.
  `model_banks` writes the key of no turn (0, 0, 0, 1) in its place. The converter itself does not change.
- `pack::models_bin` writes the file: the header, the numbers of the game's banks, zeros up to a multiple of 8, then
  the two banks.
- `gbtool modelbanks GLTF OUT` converts a glTF file at scale 1 and writes `models.bin`. It needs no ROM, so it does
  not avoid the game's ids. `gbtool import ROM GLTF [--scale S] [--home DIR]` imports a model into a user folder, as
  the app's Import model does.
- `crates/core/tests/data/cube.gltf` is a cube drawn from scratch by `make_cube.py` next to it. g64coopdx keeps its
  `models.bin` as the fixture of its bank check. The test `the_cube_clashes_with_no_game_id` checks its ids against
  every object bank and texture bank in the ROM map.
- Verified with g64coopdx's `g64pack check DIR --rom ROM`: Atlantis 1 with a model drawn from scratch and imported
  with `gbtool import`, as a LandActor and a Platform, and Atlantis 1 with a Carnival 1 bell (models.bin names texture
  bank 4 and object bank 20), and Atlantis 1 with that model and the cube, two imported sets in one pack. All three
  pass the ROM's rules and the rule for the level's objects, and g64pack prints the id that gbtool prints. The pack
  with the bell names banks that each player's game loads from the player's own ROM.

## Autosave, reset, import

- **Autosave**: after every edit the level's XML is saved in the user folder (`drafts/<level id>.json`,
  `crates/editor/src/drafts.rs`). Reopening the level, restarting the editor or switching levels and back restores it. The HUD
  chip shows "Edited · autosaved HH:MM", and the level list marks edited levels with ●. Undo after a restore
  goes back to the shipped level.
- **Reset level** discards a level's edits and its autosave, after a confirmation. It is undoable in the
  current session.
- **Import XML** (or drop an .xml on the window) checks the file before any of it reaches the level
  (`crates/editor/src/sanitize.rs`):
  - It refuses DOCTYPEs, entity declarations, stylesheet instructions, XInclude and CDATA before parsing.
    Entity expansion and external-entity reads cannot happen.
  - Every element must be a command or type from libgarib's format, in a place that format allows (a `Garib`
    inside a `GaribGroup`, `PlatScale` inside a platform…).
  - Every attribute must be a field of that element. Every value must fit its type: finite 32-bit floats,
    integers within u1–s4 ranges, known enum names, names of at most 8 printable ASCII bytes.
  - It also enforces limits on file size (4 MB, checked before reading), element count, nesting depth and
    attribute length.
  - It requires a single `EndLevelData` at the end: the game parses until that token. A missing one is
    added and a misplaced one is refused.
  - The validated content is copied into a fresh document, which is what reaches the editor. Comments,
    namespaces and anything unknown are not copied.
  - The dialog reports problems by element path, matches the file to its level by name, and imports as an
    undoable edit.
  - Autosaved drafts go through the same validation when they're restored.
- The back end repeats the safety checks for builds (`level::parse_level_xml`): no DTD or entities in its XML
  parser, a size limit, and strict validation of level IDs and import IDs. No part of the program opens a network
  connection.

## Selecting, rotating, scaling, adding

- **Multi-select**: Ctrl/Cmd+click toggles; **B** (or the Box select button) drags a box; **Ctrl+A** selects
  everything visible except landscape; the outliner supports Ctrl/Shift+click and an "all" button per group.
  Move, rotate, scale, duplicate and delete apply to the whole selection. **Pivot** chooses the group center or
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
  | Puzzle trigger volumes | none (axis-aligned) | resizes the box, sphere or cylinder about its center |
  | Garibs, Mr. Tip, spawns | none: sprites or points in the game; group rotations move them around the pivot | none |

  "(mod)" means the game applies it only with the Expansion Pak game mod, described below.

  The transform math comes from the decomp: rotation is `GetQuaternionFromXYZRot` with `rotOrder` {1,2,0}
  (R = Ry·Rz·Rx, Euler order `YZX`), and scale is `guScaleF(x, y, z)` with no axis swap. noclip uses `ZYX`
  and swaps X/Z. Shipped levels only use Y rotation and uniform scale. The two agree on shipped data.
  An edited level rendered from the emulator's exact camera (`guLookAt(actualCamPos, actualCamTargetPos)`, read
  from memory) under both conventions showed it: the decomp's matches the game and noclip's does not.
- **Expansion Pak game mod.** A small game mod (`mod/`) uses the N64 Expansion Pak for five things:
  - **Rotation and scale for every 3D object.** The unmodded game applies `SetActorRotation` and
    `SetActorScale` only to landscape and background actors. The mod applies them to enemies, platforms,
    powerups and water too (`mod/transforms.c`, details below).
  - **A larger heap.** The game's heap (Jalloc) normally ends at 4 MB and holds about 1.3 MB. The mod moves its
    end up to 0x807E0000, just below the mod itself. That adds 3,968 KB (`mod/heap.c`).
  - **Models from other banks** (`mod/banks.c`). The game has 5 slots for model banks and 5 for texture banks,
    and a level uses about 3 of each. The patch lists the extra banks each level needs. Before the level is
    parsed, the mod loads them from the ROM with the game's own `LoadObjectBank` and `LoadTextureBank`: it
    empties the slots for the call, then moves the new bank into its own lists and puts the level's banks
    back. `FindObject` and `FindTexture` are redirected to versions that search the game's slots first, then
    the mod's lists. The extra banks are freed before the next level loads. Animated textures in them do not
    animate.
  - **The matrix guard** (`mod/guard.c`). It skips draws that would pass the frame's 256 object matrices ("Real
    hardware", RDP and RSP).
  - **Extra level slots** (`mod/slots.c`). New levels can use the slots of the Space levels. A loading zone in a hub
    switches to them ("Commands, puzzles, enemy scripts, extra levels").
  - **Which builds add the mod.** Build ROM and Export patch add it whenever the level rotates or scales an enemy, a
    platform, a powerup or water, or imports models (unless "Copy the models into the ROM instead" is on and the
    copies fit the game's own heap). The checks panel and the export dialog say so. A build with an extra level also
    gets the mod. Other levels are built without the mod.
  - **Where the mod is stored and runs.**
    - The mod is about 5 KB of code, built from C against the gloveworld decomp. It is stored in the ROM at 12 MiB,
      past anything the ROM builder writes, and runs from Expansion Pak RAM at 0x807E0000.
    - A 620-byte loader replaces `ScreenShot`, a debug function the game never calls. It runs once at boot.
    - The game's own code keeps its exact layout. The ROM changes are 16 same-size `jal` changes. At boot, with
      an Expansion Pak, the mod also writes a jump over the first two instructions of `FindObject` and
      `FindTexture` in memory. The ROM builder finds the game's pointers at fixed code offsets, and these offsets do not move.
  - **The heap.** `MainProc` starts the heap before the loader can run (the loader needs `dmaMessageQ`, which
    is created later). Nothing is allocated in between. The mod starts the heap again with the larger size.
    The three calls that restart the heap on level changes (`game.c`, `frontend.c`) go through the mod, which
    keeps the larger size. Jalloc itself has no 4 MB limits: static blocks grow down from the end, dynamic ones
    up from the start, and sizes are 32-bit. Nothing else in the game masks addresses to 4 MB.
  - **The commands the mod applies.** The level parser sends `SetActorRotation`/`SetActorScale` to the most recent object of any
    kind, not only the last landscape actor. In all 48 shipped levels, every one of these commands follows a
    landscape-type actor. Shipped levels behave as in the retail game.
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
    `crates/core/src/modpatch.rs` adds the mod to a ROM. It checks every word it replaces first.
  - Verified in the emulator:
    - A tilted platform, and a General Wu lying on its side at twice its size, appear and play. The level stays
      in play mode.
    - The editor's preview, rendered from the emulator's camera, matches the game. A preview with the opposite
      rotation order does not match it.
    - With the loader's Expansion Pak check forced to fail, the mod doesn't load. Glover, the camera and the
      enemy then end up exactly where they do in the control ROM, at the same game frame.
- **+ Add** (A) opens a palette. Each entry shows a picture of its model, drawn when the entry scrolls into
  view:
  - The Enemies, Models and Kit tabs have two views, `List` and `Grid` (one choice for the three tabs, the setting
    `add_view`: `list` or `grid`). `List` has a row for each entry with its name, its notes and a button for each
    way to place it. `Grid` has the pictures only, as square tiles.
  - A click on a tile places the entry. On Models and Kit a row of radio buttons over the grid says how: the
    choices of the list's buttons (`MODELS_AS`, `KIT_AS` in `palette.rs`; the settings `add_models_as`: `scenery`,
    `background` or `platform`, and `add_kit_as`: `background` or `scenery`). Enemies have one way and no radio
    buttons. An unknown value of a setting is the list and the first choice.
  - A tile's tooltip has what its row shows: the name, the id, the bank, the notes. A corner mark flags a boss and
    one of the player's own models. A tile that can't be placed (a converted model under `Platform`, an enemy
    without a model) is dimmed, and a click writes the reason to the status line. A tile is a button for the
    keyboard and for screen readers, named by its entry.
  - A kit tile is made with the item's settings as they are. The corner with three dots, or a right-click, shows
    the item's settings in a box over the grid (`kit_settings`).
  - The grid scrolls virtually (`tile_grid`, egui's `ScrollArea::show_rows`): the columns are as many 80 point
    tiles as fit the panel, the tiles stretch to fill the row, and only the rows in view are laid out. The Models
    grid has every model the search leaves. The list stops at 150 rows.
  - Model pictures are bounded. At most `THUMBS_MAX` (320) are kept: when there are more, the ones that are not in
    view go, the ones seen longest ago first (`palette::work`). At most 32 models are read at a time. A tile that
    is still in view asks again in the next frame.
  - The search box has an id of its own. Its clear button comes and goes before it in the row, and an id made by
    counting would move the keyboard focus to that button after the first letter.
  - **Collectables**: garib types, every powerup (`Pickup` enum), Mr. Tip.
  - **Enemies**: all 52 types. A new enemy copies the shortest script that type has in a shipped level, with
    its absolute points (`home_*`, `destination_*`, `lookat_*`, attention box) moved to the new spot.
  - **Models**: all 1,522 models in the game, from every object bank, placed as Scenery (`LandActor`, solid),
    Background (no collision) or Platform. **Import model…** adds a model of your own (see [Import model](Import-model.md)).
- Attached points follow their object. Enemy scripts use world coordinates (`home_*`, `destination_*`,
  `lookat_*`, attention box), and platforms have path points and orbit centers. Moving, rotating, scaling or
  duplicating an object applies the same transform to all of them. In the emulator, a General Wu moved 300
  units this way roams its new area; with only its `x` changed, it walks back to the old one.
- **Imports**: a model from another level's bank is imported when you press Build ROM. By default the game mod
  loads it from the player's ROM at run time (see "Models from other banks" above). The editor picks the
  banks: each model's own bank, and the texture banks that hold the textures the level's banks lack,
  preferring the model's own world. The checks panel shows their size against this level's free heap plus
  the Expansion Pak's. With "Copy the models into the ROM instead", the build splits the model
  out of its bank and packs it into the bank the game loads last for this level (`levelInfo[level].objBank`
  in `LoadLevel`, e.g. `ATLANTIS_L1`); textures it needs are
  appended to the world texture bank, record by record. The game fixes up each texture record relative to
  itself (`LoadTextureBank`). The records are copied unchanged. Imports are worked out from the XML at
  build time, and undo and XML export use the same XML. The checks panel lists them with a memory
  budget. Every level that loads the enlarged object or texture bank pays for the growth. The budget
  is checked against the tightest of those levels, using free heap measured in the emulator
  (`crates/core/data/heap.json`, all 48 levels, `tools/measure_heap.py`). Free heap ranges from 36 KB (Carnival 1) to
  435 KB. Imports that need more add the Expansion Pak game mod, whose heap is 3,968 KB larger.
  Verified in the emulator:
  - It adds a 242 KB model from the INTRO bank to Carnival 1.
  - Without the mod the game runs out of heap (1,456 bytes free) and stops.
  - With it the heap is 5.4 MB, the level plays, and 3.8 MB stays free.
  - With the Expansion Pak check forced to fail, the heap keeps its 1.3 MB and the game stops as without the mod.
- Which banks a level loads comes from the game's own `LoadLevel` (`tools/game_banks.py` reads `levelInfo`
  and `worldInfo` from the decomp build; the result is in `scenes.json`). It agrees with noclip's scene table for every level except
  the Wayroom, the studio logos and the two flythrus. These four have six differences. In the Wayroom, the shared
  bank depends on the world you came from. A model counts as "already loaded" only if it's in a bank both lists load.
- **Puzzle trigger volumes** are drawn as translucent shapes, one per puzzle condition that tests where
  Glover, the ball or the camera is. The shapes follow `PuzzleEvaluateCondition` in the decomp:
  - Boxes (`…WithinVolume`) are a min corner `x, y, z` plus a size `l, w, h` along X, Y, Z
    (`IsPointWithinBox`).
  - Ranges (`…WithinRangeOfPoint`) are spheres: a center and a radius.
  - `PuzzleCondGloverWithinRangeOfPoint2` is a vertical cylinder: center `x, z`, radius `range`, from
    `min_y` to `max_y`.

  Pink is Glover, orange the ball, purple the camera. Move them with the gizmo and resize them with scale or
  the inspector's size fields; the inspector shows the whole puzzle. Duplicate and Delete copy or remove the
  whole puzzle when the volume is its only one (a camera zone, for one). A volume of a puzzle with several
  can't be duplicated or deleted on its own. The shipped levels hold 209 boxes and no spheres or cylinders.
  Clicks pass through the translucent fill to objects inside.
- **triggers** (toolbar checkbox) shows or hides the puzzle trigger volumes. It is the same switch as the
  "Puzzle triggers" layer. The setting is remembered between sessions. Hidden volumes can't be picked, and the
  level keeps them.
- **Exit triggers** are drawn as a green translucent sphere with three rings around each platform whose
  `SetExit` the game tests (an exit type other than 0). The center is the platform's position, for a moving
  platform its start. The radius is the one the game gives the platform ("Facts from the decomp", exits):
  `exits::radius` walks the platform's commands in the level file's order, on the vertices that the faces of the
  model's root mesh use (`Backend::root_vertices`, read from the object bank with
  `objbank::root_face_vertices`). The sphere is a node of its own in the world (`EntNodes::exit`). A click
  goes through it, and it is in no selection box. The rings are drawn a second time, faint and without the
  depth test: the hub's entrances sit inside their doors. It follows the platform when the platform moves, when its
  scale changes and when its commands change. A radius of 0 draws nothing, and so do a model the editor
  cannot read and an exit of type 3 (it tests no distance). The inspector's "Exit trigger" block gives the exit type, the radius, who must come within it
  and the other conditions of `CheckForExit` for that type and level (`exits::rule`). The editor draws the
  exit post's model at a scale of 1.5 on every axis; the game uses 1.5, 2, 1.5, and the radius is computed
  with the game's.
- **exits** (toolbar checkbox) shows or hides the exit triggers. It is the same switch as the "Exit triggers"
  layer, which counts the platforms with an exit. The layer's key in `Editor::hidden` is `exit`; no entity has
  that kind. The setting is remembered between sessions as `exits`. Hiding the platforms hides their spheres.
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
| Side, Other side | swings around to the right or the left of the way on |
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
  footprint. Past 256 the frame never finishes and the game stops. It slows down well below that.
- The far plane is `FogConfiguration`'s `near_clip` (1000 without one).
- The editor counts the object nodes of every model within reach of a camera 130 behind and 50 over Glover,
  facing four ways, at every garib, powerup, checkpoint, enemy and camera zone.
- From 190 it's a warning, from 240 an error. Both need an estimate over the shipped level's own. Shipped levels
  reach these numbers too: hub 1 261, hubs 2 to 6 and hub 8 209 to 239, Pirates 2 198, and the two flythrus 225 and
  213. A level over 190 that is not busier than the shipped one gets a note. The shipped level's estimate is worked out when the
  level opens (`prime_orig_load`), also under a restored draft. Click the row to see the spot and the far plane.
- `the_draw_load_of_shipped_levels` (`crates/editor/tests/shipped_rom.rs`, with `GLOVEBOX_ROM`) checks that no
  shipped level gets the warning or the error, and that hub 1 with more landscape gets the error, also from its draft.
- Checked against the game's own count (the mod's `modPeakMatrices`) at 8 spots from quiet to busy in each of five
  levels. In Atlantis 1, Carnival 1, Carnival Bonus and Pirates 2 the estimate was within
  35 of the game at every spot. In hub 1 it said up to 63 more than the game. The shipped hub 1 asks for 228 at
  its fountain (the estimate says 261), and Pirates 2 for 199 (198).
  A hub built with the editor slowed down on a console at 206 to 228.
- The checks also report a `fog_distance` of 1000 or more.

### Ground under checkpoints

Glover starts at the first checkpoint and restarts at the last one he touched. A checkpoint, or a
`GloverSpawnPoint` in a level with no checkpoint, with no solid ground within 400 under it is a warning. The view
marks the first checkpoint as Glover's start, also in a level with a `GloverSpawnPoint`. The spawn point is then
labeled as not used. No shipped level has both. Solid means a landscape piece
(`LandActor`) or a platform. Background actors have no collision.

### The kit

**+ Add → Kit** has props made by the editor's own code (`crates/core/src/kit/`).
They hold no game data, so they're free to share.

- Graveyard: a gravestone with your words, a signpost with your words, a pumpkin with one of eight faces,
  candles, a coffin, a broken fence, two dead trees and a cobweb
- Ruins: rubble, a scorch mark and thorns
- Creatures: a shade, a giant hand, an eye, a pale face, a rag-doll glove, a hanging glove, gallows, a cage and
  a lantern

Each row shows a preview, and the preview follows the settings. **Background** places the model without
collision, **Scenery** as a solid `LandActor`. The editor builds the model and converts it like an imported
model ([Import model](Import-model.md)), and it then shows with the imported models. The same settings give the same model.

- The shapes and textures come from seeded random numbers (`kit/rng.rs`: the Mersenne Twister with CPython's seeding).
  On Linux the geometry is fixed bit for bit, and the reference tests hold it. Sines, cosines and `atan2` come
  from the system's math library: on Windows a coordinate can differ in its last bit. This was not measured.
- The textures are drawn by `kit/image.rs`, which follows Pillow's drawing rules.
- The words on gravestones and signs use the kit's own letter shapes (`kit/font.rs`, a stroke font on a 5 by 7 grid).
  The letters sit in cells with the measures of Pillow's built-in font. That font is not copied.
- The game repeats every texture: the object format has no setting that stops it. A kit texture that must show
  once (a face, a board with words) is laid inside its own picture, with coordinates from 0 to 1. A unit test in
  `kit/mod.rs` checks this for every item.
- The pumpkin's face is a 64 by 32 texture on the four front columns of its body (the side toward +z, as the
  gravestone's words). The other eight columns repeat a 16 by 32 texture of plain skin, one tile per column.
  A face texture laid around the whole body would show on three sides.
- A converted model is stored under a hash of its file. A pumpkin placed with an earlier version of the editor keeps
  the model it was placed with, with the face on three sides.

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
key of all zeros. A console stops when it draws one as a platform. An emulator runs on. The Models tab doesn't
offer **Platform** for them, and the checks flag a converted model that is used as a platform. This rule is for
ROMs. A level pack for g64coopdx may use one as a platform ("Level packs for g64coopdx").

## Commands, puzzles, enemy scripts, extra levels

| File | What |
|---|---|
| `crates/editor/src/commands.rs` | What goes inside what (from decomp.json), new-command defaults, and the order the game reads a platform in: phases for inserting, glued children (a spin pause after its spin), needs-before, one-of and exclusive rules, the NullPlatform whitelist. `tidy()` moves only what the game reads wrong. `platform_rules()` feeds the checks. |
| `crates/editor/src/puzzles.rs` | Plain names for every cond_type and action_type, canonical child order, float-in-u4 fields, puzzle checks. |
| `crates/editor/src/scripts.rs` | Enemy instruction lists (flat per kind across groups), Goto and Loop retargeting, ready-made scripts, script checks. |
| `crates/editor/src/presets.rs` | Platform presets (rings, exits, lifts, spikes, teleport, the extra-levels entry zone). |
| `mod/slots.c` | Extra level slots: the loader, the levelInfo swap, the save swap and its EEPROM record, the world exit, tag 77 rings. |

- The 48 shipped levels give no error from these checks, and `tidy()` changes none of them. A rule that fires on a
  shipped level is wrong. Shipped levels put PlatScale after PlatSetInitialPos on moving or face-freed platforms,
  PlatSetCollisionYOffset after placement, a path sound anywhere after its point, and PlatVentAdvanceFrames before
  points; all of these are fine.
- `xmlutil::num` reads the XML's hex ints (`0x4000`).
- `objbank::faces` reads a model's root-mesh face count from the bank bytes.
- Title, music and bonus clock go into levelInfo in the ROM (0xE7910 + 0x38 × slot; name +0, song +0x35, clock
  +0x36). They are kept in `drafts/info.json`, not in the level XML. In the emulator a 45 s clock counted down and
  PrepareSong got the chosen song. Quickboot ROMs then switch to song 59 whatever the row says.
- `build::build_levels` builds any number of levels in one ROM. Two levels can't copy models into the same bank.
  Extra levels get no manifest line; `modpatch::apply_mod` stores them at MOD_ROM + 0x20000, 16-byte aligned, and
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
- Glover starts at the first `PlatCheckpoint` (restart gate), also when a `GloverSpawnPoint` exists. The parser puts
  him at the spawn point (`TOKEN_HAND_START`). The first gate's restart then moves him to the gate (`TOKEN_RESTART`
  in `parse.c`, `RestartPlayer` in `platform.c`).
- Ball walking: `ProcessInput` (`src/controller.c`) subtracts the stick's push from the ball's velocity on Easy and
  adds it otherwise, so Medium and Hard feel inverted. The check is one `bnez` at 0x801396D0 (ROM 0x3A6D0);
  `apply_easy_controls` in `crates/core/src/modpatch.rs` replaces it with a nop in every built ROM. GameShark equivalent:
  `811396D0 0000` and `811396D2 0000`. The difficulty itself is `saveSlot[currentSaveSlot].difficultyLevel`
  (0x801EAA55 + 0x1C per slot; slot 6 is the level select's).
- A missing *vital* object makes the game set `gameInfo.mode = 13` ("LEVEL UNPLAYABLE"). Bullet models are
  non-vital.
- Heap: about 1.3 MB (`JallocInit`). Free heap per level is in `crates/core/data/heap.json` (36 KB in Carnival 1 up to
  435 KB).
- The ROM builder (`crates/core/src/rom.rs`, after libgarib) finds every asset pointer at a fixed code offset
  (`formats/map.0x8e6e01ff.0xccb4f948.yaml`, 280 `lui`/`addiu` sites) and repacks all assets from offset 0, ending near
  0x77DE20. So game code changes must keep the code segment's layout. The mod only makes same-size `jal` changes,
  writes its loader over `ScreenShot` (unused), and puts its code at ROM 0xC00000 (12 MiB): the builder's output passes
  8 MiB with many imports, because rebuilt banks are stored uncompressed.
- Drawing load: the game slows to a crawl when it draws too much at once. At a far plane of 6000 with a whole
  stage in view it stops. The far plane is `FogConfiguration`'s `near_clip` (`farPlaneDist`); `fog_distance` is
  `gSPFogPosition`'s min (0-1000). The game culls by distance only: `IsActorVisible` (`object.c`) compares an actor's
  distance from the camera with the far plane plus its radius, with no test against the view, so everything within
  the far plane is drawn, behind the camera too. A smaller far plane cuts the drawing load, because fewer actors are within it.
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
- Texture ids can clash between worlds (Atlantis and Space share four ids with different pictures). `FindTexture` takes the level's own banks first. Bit 7 of a level's number in `modExtraBanks` makes the extra banks win (`modpatch::apply_mod` takes the flag with each level's banks).
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
- Exits (parse.c `TOKEN_EXITDOOR`, `TOKEN_PLATFORM`, `TOKEN_POS`, `TOKEN_SCALE`; platform.c `InitPlatformInfo`;
  game.c `CheckForExit`):
  - An exit is a platform with a `SetExit`. `behavior` is the game's `exitType`. `UpdatePlatform` calls
    `CheckForExit` for every platform whose `exitType` is not 0. `type` 0 (`loading_zone`) sets `alpha = 0` and
    `flags |= 4`, and frees the collision faces.
  - The test is `DistanceBetweenPoints(pos, &platform->actor.pos) < platform->actor.radius`: a sphere around the
    platform's position. `pos` is the hand for exit types 6 to 12 and 21, for type 13 outside the Assault
    Course, and in the Wayroom, the Atlantis bonus, Atlantis 3 and Carnival 1. Otherwise it is the ball.
  - Exit type 3 tests no distance: the ball must be on the platform (`ball.obj->platform`) while Glover holds it.
  - In the Wayroom, the Castle Cave, Atlantis 3 and Carnival 1, and for types 6 to 12, the distance is the
    whole test. Atlantis 3 and Carnival 1 then end 50 frames later (`newLevExitTimer`).
  - Everywhere else the game also asks: nothing for type 21 and in the Atlantis bonus; for type 13 that
    `hubLevel` equals `hubTarget` or `ballStatic` is set; for every other type that Glover holds the ball.
    Type 1, and every type in the Assault Course, then plays the goal animation (not in the first Space
    boss). In the Assault Course that needs the ball held.
  - In the hub stages (levels 0 to 7) an exit other than 12 and 21 does nothing while `ballStatic` is 0,
    `hubTarget` differs from `hubLevel` and `worldComplete[exitType - 5]` is 0.
  - For types 6 to 12 a ball that Glover does not hold is popped when it comes within the radius
    (`ballLeftBehind`).
  - `actor.radius` starts at 0 and only grows. `exitpost.ndo` (`0x7FDADB91`) starts at 20, with a scale of
    1.5, 2, 1.5. `InitPlatformInfo` grows the radius to the longest vertex of the faces of `obj->mesh` (the
    object's root mesh, its own vertices without any key frame), each axis multiplied by `actor.scale`. It
    returns at once when `flags & 0x84`: a `NullPlatform` has 0x80, and a loading-zone `SetExit`, a
    `PlatNoClip` or a `PlatSpike` already read has set 4.
  - `InitPlatformInfo` runs from `SetPlatformPos`: when the parser reads `PlatSetInitialPos` and the first
    `PlatPathPoint` (an orbiting platform takes no path point). It also runs for the pieces of a rope bridge.
    `actor.scale` is what `PlatScale` set before that moment (`PlatScale` is not read after the first path
    point), or the game mod's `SetActorScale` (mod/transforms.c). A scale that comes after the position is
    drawn and does not change the radius.
  - So a loading-zone `SetExit` before the position leaves the radius at 0, and the exit can never fire. The
    check "The exit can never fire" reports it. The 48 shipped
    levels have 126 platforms with an exit, and none has a radius of 0. The exit posts of the levels have 20,
    their start value. The hub's entrances are `ballbit` models of 18.0 to 57.4,
    the Wayroom's doors 87.6, the Castle Cave's 131.4. The end of Carnival 1 is a `ballbit` at scale 10
    (437.8), and the end of Atlantis 3 a `hoop` at scale 10 (800.0).
  - Measured in the emulator: each of the 48 levels was booted from a quickboot ROM, and every platform with an
    `exitType` was read from the game's list. All 126 radii equal the editor's to the one decimal the comparison
    kept (the largest difference was 0.05). The list is `platformList` (0x802904D0): a count, then a sentinel
    `Platform` at +4 whose `next` (+0x00) starts the chain. In a `Platform`, `flags` is at +0x14, `exitType` at
    +0x18 and the actor at +0x80; in the actor, `radius` is at +0x30, `pos` at +0x34 and `scale` at +0xBC.
    Levels 0x28, 0x2e and 0x2f did not reach play mode by quickboot; they have no exits.
- Camera zones ("Level tools", above): a puzzle with one `PuzzleCondGloverWithinVolume` and only actions 70
  (`TweenYAdjust`), 71 (`TweenDistance`), 73 (`FlyTowardsPoint`) and 74 (`LookAtGlover`: `angle` is `atan2(dx, dz)` of
  Glover minus the camera, `distance` is the turn speed). `camera.c` puts the level's camera back every frame Glover is
  in no zone (unless he is falling). The shipped levels hold about 170 of this shape. `camera_zone()` in
  `crates/editor/src/entities.rs` reads one, `set_camera_zone()` writes its actions.
- Drawing: the game counts one object matrix per object node it draws (all nodes of the object tree, `CountNodes` in
  `mod/guard.c`), plus shadows and footprints. In the editor's models (`crates/core/src/model.rs`) these are the nodes with a mesh; a node
  with several materials is still one. In the game the
  camera stands about 130 behind and 50 over the glove (measured in Atlantis 1).
- Camera height: `camera.c` puts the camera `distance / 3` plus `heightOffset` over the glove, and `heightOffset` is
  about 30 plus `targetHeight` (a zone's `TweenYAdjust`). Measured: a High zone (320, 140) puts it 408 up. Puzzle
  actions run every frame their condition holds (`CheckPuzzles`: `timesLeft` is -1 without a `PuzzleNumtimes`), and
  `camera.c` puts `levelCamHeight` and `levelCamDistance` back after it uses them. The action ids are the
  `PuzzleActionType` values plus 45: 70 `ACTION_CAM_HEIGHT`, 71 `ACTION_CAM_DISTANCE`, 73 `ACTION_CAM_FIXEDPOS`,
  74 `ACTION_CAM_FIXEDROT`.
- In Atlantis 1 the garibs float over water and the seabed: a glove put at one by a test drowns or falls and restarts.
  The camera zone check puts its zones around the start instead.

## Real hardware

mupen64plus runs ROMs that a console does not run, and most of the editor's checks are tested on mupen64plus only. A ROM that works in the
emulator can freeze on a console. The rules below come from the PI, the caches, the RDP and the boot code. `crates/core/src/hwcheck.rs` checks the
ones a tool can check, for every ROM `crates/core/src/modpatch.rs` builds; `tools/build_mod.py` checks the alignment of the mod's own buffers.

**Boot and ROM**
- The header holds two CRCs over the first megabyte of code (0x1000 to 0x101000, the CIC-NUS-6102 algorithm). The boot code of a console
  does not start a ROM with wrong CRCs. `hwcheck.rs` has its own copy of the algorithm and compares it with the header (the ROM builder's).
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
  (`modpatch.rs`: `custom_table`).
- A ROM address that does not match a file (a zero or absurd size word) shows the game's own `LEVEL UNPLAYABLE - NO LEV FILE` screen.
- GCC 2.7.2 keeps at most 8 bytes of an `aligned(16)` attribute, so the address of such a variable depends on the code linked before it. Data
  that needs 16 bytes has to go first in its section after an `ALIGN(16)` in the linker script (`tools/build_mod.py`), and the build should check
  the linked address. `mod/aligned.c` holds the mod's one such buffer, the header of an extra level's file.

**Caches** (an emulator that ignores them hides every fault of this kind)
- Code written to RAM at run time: `osWritebackDCache` over the words, then `osInvalICache` over the same words (`Patch`, `Redirect`
  and the loader do). DMAed code gets `osInvalICache` over its whole length.
- Data the RSP or RDP reads that the CPU wrote needs `osWritebackDCache` before the task starts. The game writes back its whole
  `dynamic` area before it sends a frame (`SendLastFrame`) and the whole data cache before every audio task. The display lists that the mod
  appends to `glistp` reach RAM that way, and so does everything else it writes. The mod's textures are never written by the CPU.
- `osInvalDCache` over a DMA target before the DMA. Never invalidate a range where the CPU has dirty data that must stay (the game does it
  for `size` on the stack and it works because a partial line at either end is written back first).

**RDP and RSP**
- A texture's pixels start on a 64 bit boundary. A bank's records are each a multiple of 8 bytes long (the bank's own count word makes the
  first record start 4 bytes in, and the 0x24 byte header does the rest). libgarib's `texbank-tool pack` does not pad: `texbank::records`
  pads every record it reads and corrects its size field. Custom banks and every imported model use padded records.
  Without the padding, every second texture of a custom bank is only 4 byte aligned, and the console reads such a texture as shifted pixels.
- Set every RDP mode a picture needs before drawing it. The game leaves its modes in many states.
- `gDPFillRectangle` and `gSPTextureRectangle` have unsigned 10 and 12 bit corners: a negative or too large value wraps. Clip first.
- A task's display list must stay under 20000 bytes. At 20000 bytes or more the game does not create the task and stops swapping frames. This looks like
  a freeze (`CleanupAndSendDisplayList`).
- A texture load of a 16 bit texture holds at most 2048 texels (the converter keeps to it) and the whole of TMEM is 4096 bytes.
- A frame has 256 object matrices (`Dynamic.objectMtx`). Every drawn object node, shadow and footprint takes one, unchecked: past 256 they overwrite the camera's
  matrices and the start of the display list, and the frame never finishes (the game stops, the music goes on). A hub built with the editor did this (259 in one view).
  The mod's guard (`mod/guard.c`, with the Expansion Pak) skips the farthest actors and then shadows and footprints instead. Keep every view under 232 (the guard keeps 24 for the glove, the ball, shadows and footprints): each prop or retail piece is at least one.

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
- mupen64plus does not show the hardware faults in this section. The matrix overflow is an exception: it stopped the game in
  mupen64plus (`mod/guard.c`). Keep `crates/core/src/hwcheck.rs` in the build, and test on the console.

## Imported models

- `crates/core/src/custom/` converts a model in the program itself. `gltf.rs` reads the file (a `.glb`, or a `.gltf`
  text file whose buffers and images are data URIs): accessors, node transforms, embedded images. `mod.rs` cuts the
  triangles into pieces and makes the textures. `pack.rs` builds one single-object bank per piece, straight from the
  triangles. The file is untrusted: every read is checked, and a damaged file gives an error.
- The editor stores each import in `custom/<set>/` in the user folder: `meta.json`, one single-object bank per piece
  (`<id>.obj.bin`) and `textures.tex.bin`. `<set>` is a hash of the file and the scale. `meta.json` lists the pieces
  and, for each texture, its ID and the length of its record. The pictures in the lists are drawn from the banks when
  a row scrolls into view. They are not stored.
- An object's ID is `hash_str` of the root node's name (libgarib's rule), and the mesh name is its first 8
  characters. The converter pads the short name with NULs and appends a hash.
- A texture is named by a hash of its pixels and its size. The same picture in two models is one texture. Every
  triangle needs a texture: the packer reads its size to scale the texture coordinates.
- The pixels must come out the same on every computer, or the IDs change. `image.rs` decodes PNG with the
  `png` crate and resizes with Pillow's Lanczos filter (two passes with fixed point weights, in Pillow's order).
  `jpeg.rs` is the editor's own JPEG decoder. It gives libjpeg-turbo's pixels for baseline and progressive files, and
  refuses arithmetic-coded, lossless, CMYK and 12-bit files and fractional sampling.
- A texture record is padded to a multiple of 8 bytes (Real hardware, above).
- 2,048 texels is the most one `G_LOADBLOCK` loads. Texture coordinates are s10.5 in texels, so the converter
  moves repeating coordinates by whole repeats, per piece, then per triangle, then clamps.
- IDs avoid every object and texture ID in the game's banks and in other imports.
- Builds: `plan_imports` (`crates/core/src/imports.rs`) treats custom pieces like copied imports (texture source
  `custom:<set>`). `plan_runtime` leaves them out of the run-time banks and puts them in the plan's `copy` list, which
  `build_rom` copies even when the mod loads the other imports. The editor's `runtime_imports` is false when every
  import is custom.
- `modCustom` (`mod/banks.c`: magic `CUS1`, a count, then per key the cart addresses of a texture bank and an object bank) holds
  generated models stored behind the mod (`modpatch::apply_mod`, its `custom` list). `Mod_LoadExtraBanks` loads them with
  `LoadCustomBanks`, a copy of the game's own bank loaders that puts the banks into the mod's extra lists. The editor doesn't use it
  yet: its imports go into the level's banks or the run-time extra banks.

## The Windows app

- The app is one program, `Glovebox.exe`. The tables, the game mod, the fonts and the icon are compiled in. It
  installs nothing, unpacks nothing, starts no other program except the player's emulator (Test level), and opens no
  port.
- It reads the game's data from the ROM copy into memory at each start (`gamedata.rs`), in well under a second. No
  split ROM, converted model or texture is kept on disk.
- The user folder is `%LOCALAPPDATA%\Glovebox` (`shell::default_home`; on Linux `$XDG_DATA_HOME/glovebox` or
  `~/.local/share/glovebox`). `--home DIR` names another. It holds:

  | File | What |
  |---|---|
  | `rom/glover.z64` | The checked copy of the player's ROM |
  | `drafts/` | Autosaved levels (`<level id>.json`) and `info.json` (titles, music, clocks) |
  | `custom/` | Imported models and kit models |
  | `test/` | The ROMs Test level builds |
  | `settings.ini` | `scene`, `fog`, `triggers`, `exits`, `theme`, `info` (`shown`, `hover` or `off`: how the notes show; anything else is `hover`), `tour` (`seen` once the welcome was answered), `panels` and `panel_sizes` (the layout of the panels and their dragged sizes), `add_view`, `add_models_as` and `add_kit_as` (the view of Add to level and what a click on a tile places), `saveDir` and `emulator`, as `key=value` lines |
  | `editor.log` | The app's messages, a new file at every start |
  | `editor.lock`, `glovebox-folder.txt` | The lock and the marker (below) |

- The user folder must be new or empty the first time. The app then writes `glovebox-folder.txt` into it, and later
  accepts it because of that file.
- One copy of the app uses a user folder at a time: it holds a file lock on `editor.lock`. A second copy says so and
  exits. `--reset` refuses while the editor runs.
- The first start shows the setup screen (`setup.rs`) in the editor's window. The player picks the ROM. The app
  converts `.v64` and `.n64` to `.z64`, checks the SHA-1 (`270be17b…`, as in gloveworld's `NGVE.sha1`) and keeps the
  copy. The editor then opens in the same window.
- The release build is a windowed program: no console. `--help`, `--version` and `--self-test` typed in a command
  prompt print nothing. Redirect them: `Glovebox.exe --self-test > report.txt`.
- An error in the program (a panic) goes to `editor.log` and into a message box, and the app stops. Edits are
  autosaved before that. Slow work runs on worker threads (`jobs.rs`); the window stays live.
- `--self-test` checks the compiled-in tables and the icon, opens a graphics device without a window and draws a
  frame of a made-up level. With a ROM in the user folder it also opens the ROM, builds a ROM with a kit model, runs
  the hardware check on it and runs the hardware check's own test (`hwcheck::self_test`). It prints a report and
  exits with 1 when a check failed.
- `--fixture` opens the editor on the made-up levels of `crates/editor/src/fixture.rs`, without a ROM.
- Graphics: the view is drawn with wgpu. On Windows the app asks for Direct3D 12 and Vulkan adapters and takes the
  best one that can draw to the window (`shell::choose_adapter`): a real graphics card before a software adapter,
  Direct3D 12 before Vulkan. With no usable graphics card it takes Windows' software adapter (WARP), which is slower.
  The log names the adapter. When no window can be made, a message says what to do. On Linux it uses Vulkan or
  OpenGL.
- The .exe's icon is drawn by `crates/app/src/icon.rs` (the window uses it too), and `build.rs` writes it into the
  program with the version details. The version comes from `APP_VERSION` (the release tag without its `v`), or from
  Cargo.toml without it. `--version` prints it.
- The release profile uses thin LTO, strips symbols and aborts on a panic. The program has no `unsafe` code of its own.
- CI (`.github/workflows/app.yml`): GitHub's runners have no graphics card. The tests draw with Direct3D 12's WARP on
  Windows and with Mesa's lavapipe (software Vulkan) on Linux. The click tests (`crates/app/tests/ui.rs`) drive the
  real window through `egui_kittest`, without a screen. The Windows job then builds the release program, runs its
  `--self-test` and packs `Glovebox-<version>-windows-x64.zip`: `Glovebox.exe`, `README.txt`,
  `THIRD_PARTY_NOTICES.md` and `licenses/`.
- `licenses/` is made by `tools/licenses.py`: the license files of every crate linked into the Windows program
  (from `cargo metadata`, without build and test dependencies and without procedural macros, which run in the
  compiler) and of the fonts egui embeds. Each distinct text is there once, and `CRATES.txt` lists the crates with
  their licenses and files.
- The app is released for Windows. It also builds and runs on Linux from source (`cargo run -p glovebox --release`).

## The guide and the wiki

The GitHub wiki holds a copy of `docs/guide`. `python3 tools/wiki.py to-wiki DIR` writes the guide into a checkout of
the wiki, and `from-wiki DIR` brings wiki edits back. The wiki's home page is `Home.md`, its links have no `.md`, and
its pictures and its links to other docs are full addresses. The owner edits the wiki's wording himself: bring his
edits back with `from-wiki` before changing a guide page, and keep his wording.

## The guide's pictures

The pictures in `docs/guide/img/` are made by the test `guide_shots` (`crates/app/tests/guide/mod.rs`, a part of
`tests/ui.rs`). One command makes them all again, from the repository's folder:

```sh
GLOVEBOX_ROM=path/to/glover.z64 GLOVEBOX_GUIDE=docs/guide/img cargo test -p glovebox --test ui guide_shots
```

- Without both variables the test does nothing and passes. `GLOVEBOX_GUIDE_ONLY=add,exit` makes the pictures of
  some steps only (the names are in `run`).
- The test opens the real window at 1280 by 800 in the Sage theme with the fog off, takes the ROM on the setup
  screen, and drives Atlantis Level 1 into each state the guide shows. Each step starts from the shipped level.
- Its settings name the default layout of the panels and the list view of Add to level, and the test checks the
  layout before the first picture. The step `panels` moves the panels for `window-panels.png` and puts the default
  layout back. The step `add` switches to the grid for two pictures and back to the list.
- The routine sets the Info texts itself. The welcome, the tour's step and `first-level.png` have the notes on
  hover, as a first start shows them. Every other picture has them shown: the pages explain the panels with their
  notes. `info-texts-on-hover.png` is the one picture of a panel on hover.
- A picture of one panel is cut out of the window's picture. For a tall panel the window is 1060 high.
- The numbers on `window-tour.png` are drawn by the test (`Pic::marker`). The list under the picture in
  `docs/guide/The-editor.md` explains them. No other picture is drawn on.
- The small pictures of the contents list (`thumb-*.png`) are parts of the same pictures, 240 by 150. Most are at
  half size. Those of a side panel are at two thirds: the panel is 380 wide.
- The files are PNGs with a palette of 256 colors (`Pic::save`). The 64 most used colors keep their exact values.
  That keeps the window's flat colors right. A median cut shares the rest among the shades of the 3D view.
- The pointer shows in the pictures of an open menu and of the box select. Elsewhere it leaves the window first.
- The autosave chip shows the time of the run. Everything else is the same from run to run.
- These pictures show the game's levels. The owner allows them in the repository. Nothing else made from the ROM
  goes in (CLAUDE.md, "Rules").
- After a change to a page's pictures, look at each new picture, and check the page's links and picture paths.

## Reference tests

The comparison tests check the Rust code against recorded reference outputs ("reference dumps"), on the whole game.
The dumps hold game data and are not in the repository. Without them each test prints that it was skipped, and
passes. `GLOVEBOX_ROM` names the ROM, `GLOVEBOX_REFERENCE` the folder of dumps and `GLOVEBOX_REFHOME` a folder with
the split ROM and libgarib's exported files.

| Test | What it compares | On |
|---|---|---|
| `crates/core/tests/level_reference.rs` | The XML text of a level file, the level file of that XML, the schema check, and what libgarib does with edited levels (accepts, refuses, the bytes) | All 48 levels and a set of edited ones |
| `banks_reference.rs` | Texture IDs and pixels, written texture banks, single-object banks (`split`), packed banks, each object's texture IDs, and the model against libgarib's glTF export | Every object and texture bank of the game |
| `rom_reference.rs` | The ROM's regions against libgarib's dump and noclip's split files, the pointer pairs, rebuilt ROMs, ROMs with the mod, the hardware check's findings, BPS patches | The retail ROM |
| `build_reference.rs` | Level XML, the catalog, loaded and texture IDs, enemy templates, backdrops and face counts, import plans, built ROMs and patches, imported models | The whole game and the build scenarios |
| `custom_reference.rs` | Import model: `meta.json`, object banks and texture bank, the error texts, decoded and resized pictures against Pillow, the stored set, `reskin` | A set of reference models |
| `kit_reference.rs` | The kit's geometry bit for bit, and its textures without words pixel for pixel. The pumpkin was corrected after the recordings: its skin triangles' texture, its GLB and its preview are not compared | Every kit item with several settings |
| `crates/editor/tests/logic_reference.rs` | The vector math, the command tables and platform rules, puzzles and scripts, presets, the import checks (on the levels and on hostile files), the entities, the camera and drafts | All 48 levels |
| `editor_reference.rs` | The editor's state after loading, and the XML after each recorded edit step | All 48 levels |
| `crates/app/tests/render.rs` | The picture of a made-up scene and of a level | By mean color difference: two renderers do not give equal pixels |

The Rust code gives the reference outputs for these reasons:

- Floats in the XML are written as Python's `repr` writes a double, and read as Python's `float()` reads them
  (`pyfmt.rs`). libgarib, a Python library, defined the format. The XML writer gives the bytes of lxml, which
  libgarib writes with.
- The editor's math is in `f64`, in a fixed order. The dot products in `vec.rs` use fused multiply-adds (`mul_add`),
  as the reference results were computed.
- Random choices (the kit, `reskin`, scatter) use the Mersenne Twister with CPython's seeding (`pyrandom.rs`,
  `kit/rng.rs`).

## Notes and limits

- Every built ROM and patch walks the ball with Easy's controls on every difficulty: on Medium and Hard the game
  otherwise pushes the ball the opposite way to the stick. Only the controls change; lives, garibs per life and the
  bosses still follow the chosen difficulty (`crates/core/src/modpatch.rs`, `EASY_CONTROLS_ROM`).
- A ROM without the game mod is not padded to a whole number of MiB and does not go through the hardware check: it is
  the retail ROM with its data repacked. A ROM with the mod is padded and checked (`modpatch::apply_mod`).
- Every built ROM comes from your ROM. Keep it local.
- libgarib's behavior that the Rust code reproduces on purpose:
  - The level format follows two sets of nesting rules, because libgarib's tools use two.
    `SetActorRotation` and `SetActorScale` may sit inside enemies, platforms, powerups and water in the XML's rules
    (our change to `formats/glover.lev.ksy`). Turning a `.lev` into XML follows libgarib's own, unchanged rules:
    there the two commands only attach to landscape and background actors. `tools/gen_formats.py` writes both sets
    into `level_format.json`.
  - `<Level>` carries libgarib's `libgarib-version` and `data-format-version` attributes, with the values libgarib
    wrote.
  - The packer's vertex cache optimizer is kept as libgarib has it. Its cache stays empty in practice: the result is
    the best first triangle, then the others in their order.
  - The object packer takes no wrap mode. A glTF sampler's wrap mode only tells the converter along which axes the
    texture coordinates repeat.
  - The `quickboot` and `copyskip` patches are the instruction words libgarib's assembler produced, written as
    numbers (`rom.rs`). The program holds no assembler and no CPU emulator.
- The Rust code differs from libgarib in these places. Models: it handles animated models, missing textures,
  zero-length normals, and meshes with and without textures in one object. Packing: vertex indices do not
  overflow, a normal of exactly 1 fits its byte, a mesh without vertex colors gets a display list, and more than
  four texture coordinates at one position are allowed.
- `quickboot` takes the level's number as a number. libgarib's assembler reads a bare `10` in text as hex, and that
  boots Carnival 2 (`StartNewLevel` in the decomp).
- Two texture banks are named `LEVELEND_TEX_BANK`: the game's banks 1 and 14. `bank_numbers.json` has 1 for the name.
  noclip's split file of that name is bank 14, and the catalog reads that one. A build that adds textures to the
  bank replaces bank 1.
- Rendering is approximate: static poses, Lambert lighting, garibs/tips as icons.
- Enemies from another world bring their own model, but projectiles come from the game's bullet table and
  are looked up as optional (non-vital `FindObject`). They may be invisible. Boss enemies are tied to their
  arenas and are marked in the palette.
- `.lev`/XML exports that use imported models, or rotate or scale enemies, platforms, powerups or water, only
  work in a ROM built by the editor (it warns).
- Platform rotation is a `SetActorRotation` and needs the game mod. A level that stores it as a speed-0
  `PlatConstantSpin` on one axis also works: the editor reads and shows the spin's initial angle.
