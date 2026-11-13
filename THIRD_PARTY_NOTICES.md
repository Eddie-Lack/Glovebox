# Third party notices

This repository uses, bundles or builds on the projects below. Each section names the source, where it is used, its
licence and the licence text or a link to it. This file does not set a licence for Glovebox itself.

The editor never includes game data. Nothing here is copied from a Glover ROM.

## Contents

1. [libgarib](#1-libgarib)
2. [noclip.website](#2-noclipwebsite)
3. [three.js](#3-threejs)
4. [gloveworld](#4-gloveworld)
5. [Pillow](#5-pillow)
6. [libjpeg](#6-libjpeg)
7. [Rust crates and fonts in the app](#7-rust-crates-and-fonts-in-the-app)
8. [mupen64plus](#8-mupen64plus)
9. [Tools used but not included](#9-tools-used-but-not-included)

## 1. libgarib

- Source: https://github.com/naclomi/libgarib (by naclomi), at commit `cf879296afc5108867de03facbca428923618f29`
- Licence: MIT, copyright (c) 2021 Naomi Alterman
- Use:
  - `formats/glover.lev.ksy` (its description of the level file, with one change of ours) and
    `formats/map.0x8e6e01ff.0xccb4f948.yaml` (its map of the retail ROM) are copies of its files. The tables
    `crates/core/data/level_format.json`, `rom_map.json` and `decomp.json` are made from them.
  - Its algorithms are ported from Python to Rust in `crates/core`: the level file and its XML (`level.rs`), FLA2
    (`fla2.rs`), object banks and their models (`objbank.rs`, `model.rs`), texture banks (`texbank.rs`), the ROM
    tools with the quickboot and copyskip patches (`rom.rs`), and packing a model into an object bank
    (`custom/pack.rs`).
  - Our changes to it are under the same licence.
- The text below is its `LICENSE.md` at that commit.

```text
# Note

This repository contains descriptions of software interfaces and
implementations of algorithms that were reverse-engineered in a "clean room"
manner from copies of game software legally possessed by the repository author.
The re-interpretations of these original game softwares are available under the
MIT License, detailed below.

# MIT License

Copyright (c) 2021 Naomi Alterman 

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## 2. noclip.website

- Source: https://github.com/magcius/noclip.website, at commit `fd973ff240233c63883750a557c62baf6ff2a6c7`
- Licence: MIT, copyright (c) 2018 Jasper St. Pierre. The licence file also says that some of its game
  implementations were reverse engineered and are not clean-room work.
- Use:
  - `crates/core/data/scenes.json` is made from its Glover scene table (`src/Glover/scenes.ts`: level names, level
    files and banks), joined with the banks the game's own `LoadLevel` loads. `crates/core/data/noclip_tables.json`
    is made from its enemy and powerup model tables (`src/Glover/enemy.ts`). Both are committed data. They describe
    the retail game and have no generator in this repository.
  - The editor names a bank by the file name noclip's ROM splitter gives it.
  - The editor's object placement (rotation order, scale, enemy and powerup models) started from noclip's Glover
    code and is written again in `crates/editor` (docs/technical.md).
- The text below is its `LICENSE` at that commit.

```text
NOTE

Some implementations of game softwares were reverse-engineered, and not in a "clean-room"
manner. The re-interpretations of these original game softwares are available under the
MIT License.

Some files written specifically for the noclip project have not been reverse engineered,
and are available solely under the MIT License as displayed below. Please contact
Jasper St. Pierre at jstpierre@mecheye.net for further details on which source files are
completely clean from a reverse-engineering standpoint, and which are not.



MIT License

Copyright (c) 2018 Jasper St. Pierre

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## 3. three.js

- Source: https://github.com/mrdoob/three.js
- Licence: MIT, copyright (c) 2010-2023 three.js authors
- Use: the editor's maths (`crates/editor/src/vec.rs`: Euler angles, quaternions, look-at) follows three.js's
  formulas and conventions, and the lighting of the 3D view (`crates/app/src/render/shaders.wgsl`) follows its
  Lambert material. three.js itself is not included.

```text
The MIT License

Copyright © 2010-2023 three.js authors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
```

## 4. gloveworld

- Source: https://github.com/Eddie-Lack/gloveworld, a matching decompilation of Glover (USA)
- Licence: the checkout has no licence file. It is a project of the same owner as this repository. The owner
  decides how its files may be reused.
- Use, by file:
  - `crates/core/data/decomp.json` is built by `tools/build_xref.py` from gloveworld's sources: the names of
    `include/enums.h`, and where the level parser handles each command, with the lines of C at that place.
    `crates/core/data/bank_numbers.json` is built by `tools/game_banks.py` from the bank names and numbers in
    `src/banks.c`.
  - `mod/` is compiled against gloveworld's headers and its `build/NGVE.elf` with its GCC 2.7.2
    (`tools/build_mod.py`). `mod/mod.bin` is built output. The game mod is an addition to the game's code.
  - `tools/emu/boot_shots.py` runs on gloveworld's `tools/emu/emu.py` and uses the game's addresses. It does not
    contain `emu.py`.

## 5. Pillow

- Source: https://github.com/python-pillow/Pillow
- Licence: MIT-CMU (HPND)
- Use: two files follow Pillow's C code, written again in Rust, because the pictures must come out pixel for pixel
  as Pillow makes them. `crates/core/src/kit/image.rs` follows its drawing rules and resize (`libImaging/Draw.c`,
  `Resample.c`). `crates/core/src/custom/image.rs` follows its Lanczos resize (`Resample.c`). Pillow itself is not
  included.

```text
The Python Imaging Library (PIL) is

    Copyright © 1997-2011 by Secret Labs AB
    Copyright © 1995-2011 by Fredrik Lundh and contributors

Pillow is the friendly PIL fork. It is

    Copyright © 2010 by Jeffrey 'Alex' Clark and contributors

Like PIL, Pillow is licensed under the open source MIT-CMU License:

By obtaining, using, and/or copying this software and/or its associated
documentation, you agree that you have read, understood, and will comply
with the following terms and conditions:

Permission to use, copy, modify and distribute this software and its
documentation for any purpose and without fee is hereby granted,
provided that the above copyright notice appears in all copies, and that
both that copyright notice and this permission notice appear in supporting
documentation, and that the name of Secret Labs AB or the author not be
used in advertising or publicity pertaining to distribution of the software
without specific, written prior permission.

SECRET LABS AB AND THE AUTHOR DISCLAIMS ALL WARRANTIES WITH REGARD TO THIS
SOFTWARE, INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS.
IN NO EVENT SHALL SECRET LABS AB OR THE AUTHOR BE LIABLE FOR ANY SPECIAL,
INDIRECT OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES WHATSOEVER RESULTING FROM
LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE
OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR
```

## 6. libjpeg

- Sources: the Independent JPEG Group's libjpeg (https://ijg.org) and libjpeg-turbo
  (https://github.com/libjpeg-turbo/libjpeg-turbo)
- Licence: the IJG licence. Full text: https://github.com/libjpeg-turbo/libjpeg-turbo/blob/main/README.ijg
  (copyright (C) 1991-2020, Thomas G. Lane, Guido Vollbeding)
- Use: `crates/core/src/custom/jpeg.rs` is a JPEG decoder written in Rust that follows libjpeg's method step by step
  (Huffman decoding, the integer inverse DCT of `jidctint.c`, the upsampling of `jdsample.c`, the colour conversion of
  `jdcolor.c`), because a picture must decode to the pixels libjpeg-turbo gives. No libjpeg code is included.
- This software is based in part on the work of the Independent JPEG Group.

## 7. Rust crates and fonts in the app

The app is built from Rust crates of crates.io. `Cargo.lock` pins their versions. The release archive holds each
crate's licence files in its `licenses/` folder (`tools/licenses.py` collects them; `CRATES.txt` lists them per crate). Most are
under MIT or Apache 2.0, at the user's choice. The main ones:

| Crate | Used for | Licence |
| --- | --- | --- |
| [egui, eframe, egui-wgpu](https://github.com/emilk/egui) | The window and its widgets | MIT or Apache 2.0 |
| [wgpu](https://github.com/gfx-rs/wgpu) | Drawing, through Direct3D 12 or Vulkan | MIT or Apache 2.0 |
| [winit](https://github.com/rust-windowing/winit) | The window and its input | Apache 2.0 |
| [rfd](https://github.com/PolyMeilex/rfd) | The system's file dialogs and message boxes | MIT |
| [chrono](https://github.com/chronotope/chrono) | Local time | MIT or Apache 2.0 |
| [png](https://github.com/image-rs/image-png) | Reading and writing PNG | MIT or Apache 2.0 |
| [serde, serde_json](https://github.com/serde-rs/serde) | JSON | MIT or Apache 2.0 |
| [sha1, sha2](https://github.com/RustCrypto/hashes) | Hashes | MIT or Apache 2.0 |
| [crc32fast](https://github.com/srijs/rust-crc32fast) | CRC32 for patches | MIT or Apache 2.0 |

egui's font crate (`epaint_default_fonts`) compiles these fonts into the app. Their licence texts are in the
archive's `licenses/` folder, in the files that start with `epaint_default_fonts-`.

| Font | Licence |
| --- | --- |
| Ubuntu-Light | Ubuntu Font Licence 1.0 |
| Hack-Regular | MIT, with the Bitstream Vera licence for the glyphs it takes from that font |
| NotoEmoji-Regular | SIL Open Font License 1.1 |
| emoji-icon-font, and egui-icons (a subset of it) | MIT |

## 8. mupen64plus

- Source: https://github.com/mupen64plus/mupen64plus-core
- Licence: GNU GPL version 2 or later. Full text: https://www.gnu.org/licenses/old-licenses/gpl-2.0.txt
- Use: `tools/measure_heap.py` runs the game in mupen64plus through gloveworld's `tools/emu`. Nothing from mupen64plus is
  included.
- It can also use the angrylion RDP plugin, which `gloveworld/tools/emu/setup.sh` fetches and builds. It is
  under the MAME licence (https://github.com/ata4/angrylion-rdp-plus, file `MAME License.txt`). It is not included.

## 9. Tools used but not included

These are used to build or test. Nothing from them is in the repository or in a release.

- The [decompals build of GCC 2.7.2 for MIPS](https://github.com/decompals/mips-gcc-2.7.2) (GPL 2), through gloveworld:
  builds `mod/mod.bin`. The output is machine code from the editor's own C files.
- GNU binutils for MIPS (GPL 3): `mips-linux-gnu-ld`, `nm` and `objcopy`
- [PyYAML](https://pyyaml.org) (MIT): the developer tools in `tools/` read `formats/` with it
- [egui_kittest](https://github.com/emilk/egui) (MIT or Apache 2.0): the click tests
