# Guide for agents

Glovebox is a level editor for Glover (N64, USA, NGVE). Read [docs/technical.md](docs/technical.md)
before you change anything: it says how the editor is put together, what the game does with a level, and the rules a
built ROM has to keep on a console.

## Rules

- **Never commit the ROM** or anything made from it: `.z64`, `.bps`, `.lev`, extracted models or textures, the
  user folder, test dumps, savestates, emulator screenshots. The editor reads the player's own ROM.
- The one exception, allowed by the owner: the editor's own screenshots for the guide, in `docs/guide/img/`. Nothing
  else from the ROM goes in: no ROM, levels, models, textures, patches or emulator screenshots.
- **Never use leaked original source code.** Facts about the game come from the
  [gloveworld](https://github.com/Eddie-Lack/gloveworld) decompilation, the ROM's own code, or tests in an emulator.
- **A rule that fires on a shipped level is wrong.** Every check the editor makes must pass on all 48 shipped levels.
- **A ROM that plays in an emulator can still freeze a console.** Keep to "Real hardware" in docs/technical.md, and
  keep the hardware check (`crates/core/src/hwcheck.rs`) in the build: the mod patcher (`modpatch.rs`) runs it on
  every ROM it builds.
- Put what you learn in docs/technical.md, in the section it belongs to. Correct or remove what is no longer true.

## Pull requests

- Merge your own pull requests once they are ready. No one else reviews them.
- A pull request is ready when the tests that apply to the change pass and nothing is left to fix.
- After a merge, restart your branch from the latest `main` before the next change.

## Writing

- Write all prose (docs, comments, commits, pull requests) in plain language: short sentences, one idea each, terms
  defined on first use. No em dashes. No AI model names.
- Do not end sentences with a purpose clause that explains why the preceding detail matters, such as "so that
  X instead of Y". State the fact and stop.
- Write American English in all prose, in the app's texts and in identifiers: color, center, license, gray, "period"
  for the dot that ends a sentence, "check" and "checkbox" for a check mark.
- Exceptions: names that belong to something else (the game, gloveworld, libgarib, a crate, a file format), names that
  stored data depends on (settings, drafts, model set ids), quoted license texts and proper names keep their spelling.

## Docs for players

Write pages for players (the README, `docs/` except technical.md, `app/README.txt`) in the owner's style. The style
is written out in the practice ROM's CLAUDE.md, "Writing style for docs" (github.com/Eddie-Lack/Glover-Practice-Rom).
In short: talk to the player as "you", casually; one fact per line; no period at the end of table cells, bullets or
one-line paragraphs; links in italics; choices in backticks; important warnings in headings with "!".

The owner edits `README.md` and `app/README.txt` himself. They are the model for this style: match them, and keep his
wording when you change something near it.

## Running it from source

You need the Rust toolchain (stable) and the ROM. On Linux the window also needs `libxkbcommon-dev` and
`libwayland-dev`, and a Vulkan driver (`mesa-vulkan-drivers` is enough).

```sh
cargo run -p glovebox --release                 # the editor's window
cargo run -p glovebox --release -- --fixture    # the editor on two made-up levels, without a ROM
cargo run -p glovebox --release -- --home DIR   # with another user folder
```

- The first-run screen asks for the ROM. The app checks it and keeps a copy in its user folder.
- Keep the ROM outside the repository. Everything made from it goes to the user folder (the ROM copy, drafts,
  imported models, settings, test ROMs, the log): `%LOCALAPPDATA%\Glovebox` on Windows, `~/.local/share/glovebox`
  on Linux, or the folder `--home` names. Nothing is written next to the code.
- The crates: `crates/core` (formats, ROM building, game data), `crates/editor` (the editor's state and every edit,
  without a window), `crates/app` (the `glovebox` program: the egui window and the wgpu view).
- No `unsafe`. Files from outside (the ROM, XML, glTF, drafts) are untrusted: check every read and return an error.

## Tests

```sh
cargo test --workspace                                         # everything that needs no ROM
GLOVEBOX_ROM=path/to/glover.z64 cargo test --workspace         # also the tests on the real game
cargo run -p glovebox-core --example gbtool -- hwcheck ROM     # a built ROM against the console's rules
```

- The click tests are in `crates/app/tests/ui.rs`. They drive the real window on the fixture (made-up levels, no
  ROM) and click real buttons. Add a click test for every new button: a handler that only works when called from code
  has shipped before.
- They need a graphics adapter. A software one is enough: Direct3D 12's WARP on Windows, Mesa's lavapipe on Linux
  (`apt-get install mesa-vulkan-drivers libxkbcommon-dev libwayland-dev`). `GLOVEBOX_SHOTS=DIR` saves pictures.
- With `GLOVEBOX_ROM`, the test `the_rom` goes through the first-run screen, opens all 48 levels, runs the checks on
  them, builds a ROM, runs the hardware check on it and exports a patch.
- A test that lacks its data prints that it was skipped, and passes.
- The comparison tests (`*_reference.rs` and a few others) compare the Rust code with recorded reference outputs, on
  the whole game. They need the reference dumps (`GLOVEBOX_REFERENCE`, and for some `GLOVEBOX_REFHOME`). The dumps hold
  game data and are not in the repository. Without them these tests skip themselves. docs/technical.md, "Reference
  tests", says what they check.
- `gbtool` (`crates/core/examples/gbtool.rs`) is the back end on the command line: `gbtool xml ROM LEVEL` prints a
  level's XML, `gbtool build ROM LEVEL XML OUT [--quickboot] [--expansion] [--bps]` builds a ROM or a patch.
  `gbtool pack LEVEL XML OUTDIR NAME` writes a level pack for g64coopdx (docs/technical.md, "Level packs for
  g64coopdx"). `gbtool modelbanks GLTF OUT` writes a glTF model as a pack's `models.bin`.
- The game itself is tested in gloveworld's emulator harness (`tools/emu/emu.py SCRIPT.py --rom ROM`, set up with its
  `tools/emu/setup.sh`). It needs a built gloveworld: `make toolchain`, the ROM as `baserom.us.z64`, `make setup`,
  `make`. Make the ROM to test with `gbtool build`. Give every new ROM a new file name: the harness keeps data for a
  ROM by its name. Use angrylion (the default) for screenshots; rice sometimes draws black, and is fine for reading
  memory.
- The emulator always has the Expansion Pak. It also runs ROMs that freeze a console (docs/technical.md, "Real hardware").

## Building

```sh
cargo build --release -p glovebox                                # the app: target/release/glovebox(.exe)
.venv/bin/python tools/build_mod.py --gloveworld ../gloveworld   # mod/mod.bin and mod/mod.json, after a change in mod/
.venv/bin/python tools/build_patcher.py                          # after a change to web/patcher.html's script
```

- The app is one program with everything compiled in: the tables in `crates/core/data/`, `mod/mod.bin` and
  `mod/mod.json`. `APP_VERSION` sets the version it reports (`--version`); without it, the app reports the version in Cargo.toml.
- `build_mod.py` needs a built gloveworld and `mips-linux-gnu` binutils. Commit its two outputs with the change.
- The tables in `crates/core/data/` are committed. Developer tools in `tools/` write them again after a change to
  their sources: `gen_formats.py` and `gen_rom_map.py` (from `formats/`), `build_xref.py --gloveworld DIR` and
  `game_banks.py --gloveworld DIR --numbers` (from gloveworld), `measure_heap.py` (in the emulator). `scenes.json` and
  `noclip_tables.json` have no tool: they describe the retail game and do not change.
- The Python tools need Python 3 and PyYAML only (`python3 -m venv .venv && .venv/bin/pip install pyyaml`).
- The `Desktop app` workflow (`.github/workflows/app.yml`) runs the tests on Windows and Linux, builds the Windows
  app, runs its `--self-test` and packs the archive. A tag that starts with `v` publishes a release.
