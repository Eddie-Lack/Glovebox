# Guide for agents

Glovebox is a level editor for Glover (N64, USA, NGVE). Read [docs/technical.md](docs/technical.md)
before you change anything: it says how the editor is put together, what the game does with a level, and the rules a
built ROM has to keep on a console.

## Rules

- **Never commit the ROM** or anything made from it: `data/`, `cache/`, `.z64`, `.bps`, `.lev`, extracted models
  or textures, savestates, emulator screenshots. The editor reads the player's own ROM.
- **Never use leaked original source code.** Facts about the game come from the
  [gloveworld](https://github.com/Eddie-Lack/gloveworld) decompilation, the ROM's own code, or tests in an emulator.
- **A rule that fires on a shipped level is wrong.** Every check the editor makes must pass on all 48 shipped levels.
- **A ROM that plays in an emulator can still freeze a console.** Keep to "Real hardware" in docs/technical.md, and
  keep `tools/hw_check.py` in the build.
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

## Docs for players

Write pages for players (the README, `docs/` except technical.md, `app/README.txt`) in the owner's style. The style
is written out in the practice ROM's CLAUDE.md, "Writing style for docs" (github.com/Eddie-Lack/Glover-Practice-Rom).
In short: talk to the player as "you", casually; one fact per line; no full stop at the end of table cells, bullets or
one-line paragraphs; links in italics; choices in backticks; important warnings in headings with "!".

## Running it from source

You need Python 3, the ROM, and a checkout of gloveworld. gloveworld does not need to be built to run the editor.

```sh
./setup.sh                          # fetches noclip and libgarib at pinned commits into vendor/, creates .venv
.venv/bin/python prepare.py --rom path/to/glover.z64 --gloveworld ../gloveworld
.venv/bin/python -m desktop         # the editor's window
.venv/bin/python -m desktop --fixture   # the editor on two made-up levels, without a ROM
```

Keep the ROM outside the repository. `GLOVEBOX_HOME` names the folder for everything made from it (`config.json`,
`data/`, `cache/`, the generated JSON, drafts, imported models); without it they go next to the code, where
`.gitignore` covers them.

## Tests

```sh
xvfb-run -a .venv/bin/python tools/desktop_test.py [--shots DIR]   # the editor on the fixture, no ROM
.venv/bin/python tools/hw_check.py ROM                             # a built ROM against the console's rules
```

- `desktop_test.py` needs a display with OpenGL 3.3. Mesa is enough
  (`apt-get install libegl1 libxkbcommon-x11-0 libxcb-cursor0 libgl1-mesa-dri`).
- It clicks real buttons. Add a click test for every new button: a handler that only works when called from code has
  shipped before.
- With the ROM, also load all 48 levels and run the checks on them, and build a ROM.
- The game itself is tested in gloveworld's emulator harness (`tools/emu/emu.py SCRIPT.py --rom ROM`, set up with its
  `tools/emu/setup.sh`). It needs a built gloveworld: `make toolchain`, the ROM as `baserom.us.z64`, `make setup`,
  `make`. Give every new ROM a new file name: the harness keeps data for a ROM by its name. Use angrylion (the
  default) for screenshots; rice sometimes draws black, and is fine for reading memory.
- The emulator always has the Expansion Pak, and it forgives what a console refuses.

## Building

```sh
.venv/bin/python tools/build_mod.py --gloveworld ../gloveworld   # mod/mod.bin and mod/mod.json, after a change in mod/
.venv/bin/python tools/build_patcher.py                          # after a change to web/patcher.html's script
```

`build_mod.py` needs a built gloveworld and `mips-linux-gnu` binutils. Commit its two outputs with the change.

The Windows app is built on Windows, or by the `Desktop app` workflow (`.github/workflows/app.yml`), which also runs
`desktop_test.py` on Windows and Linux. A tag that starts with `v` publishes a release.

```bat
py -m venv build\venv
build\venv\Scripts\pip install -r app\requirements.txt
build\venv\Scripts\python tools\build_app.py
```
