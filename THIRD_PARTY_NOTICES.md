# Third party notices

This repository uses, bundles or builds on the projects below. Each section names the source, where it is used, its
licence and the licence text or a link to it. This file does not set a licence for Glovebox itself.

The editor never includes game data. Nothing here is copied from a Glover ROM.

## Contents

1. [libgarib](#1-libgarib)
2. [noclip.website](#2-noclipwebsite)
3. [three.js](#3-threejs)
4. [gloveworld](#4-gloveworld)
5. [mupen64plus](#5-mupen64plus)
6. [Unicorn and Keystone](#6-unicorn-and-keystone)
7. [Python libraries in the desktop app](#7-python-libraries-in-the-desktop-app)
8. [Tools used but not included](#8-tools-used-but-not-included)

## 1. libgarib

- Source: https://github.com/naclomi/libgarib (by naclomi), pinned in `setup.sh` at commit
  `cf879296afc5108867de03facbca428923618f29`
- Licence: MIT, copyright (c) 2021 Naomi Alterman
- Use: fetched by `setup.sh` into `vendor/` (not committed). The desktop app bundles the parts it runs
  (`vendor/libgarib/python/libgarib/`, three tools, the format map, `LICENSE.md`). `patches/libgarib-*.patch` are our changes to
  it. The patches are under the same licence.
- The text below is `vendor/libgarib/LICENSE.md` at the pinned commit.

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

- Source: https://github.com/magcius/noclip.website, pinned in `setup.sh` at commit
  `fd973ff240233c63883750a557c62baf6ff2a6c7`
- Licence: MIT, copyright (c) 2018 Jasper St. Pierre. The licence file also says that some of its game
  implementations were reverse engineered and are not clean-room work.
- Use: `prepare.py` and the app run its Glover ROM splitter (`src/Glover/tools/rom_split.py`, `file_offsets.yaml`) and read its scene
  table and enemy tables (`src/Glover/scenes.ts`, `enemy.ts`). The editor's object placement (rotation order,
  scale, enemy and powerup model tables) follows noclip's Glover code and is written again in `desktop/`
  (docs/technical.md). The app bundles the files named in `tools/build_app.py` with `vendor/noclip/LICENSE`.
- The text below is `vendor/noclip/LICENSE` at the pinned commit.

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
- Use: the editor's maths (`desktop/vec.py`: Euler angles, quaternions, look-at) and its lighting follow three.js's
  code and conventions. three.js itself is not included.

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
  - `app/decomp/` holds copies of gloveworld files for the desktop app: `glover.yaml`, two lines of `symbol_addrs.txt`,
    `include/enums.h`, `src/banks.c` (only `LoadObjectBank` and `LoadTextureBank`) and `decomp.json`, which
    `tools/build_xref.py` builds from gloveworld's sources.
  - `mod/` is compiled against gloveworld's headers and its `build/NGVE.elf` with its GCC 2.7.2
    (`tools/build_mod.py`). `mod/mod.bin` is built output. The game mod is an addition to the game's code.
  - `tools/emu/boot_shots.py` runs on gloveworld's `tools/emu/emu.py` and uses the game's addresses. It does not
    contain `emu.py`.

## 5. mupen64plus

- Source: https://github.com/mupen64plus/mupen64plus-core
- Licence: GNU GPL version 2 or later. Full text: https://www.gnu.org/licenses/old-licenses/gpl-2.0.txt
- Use: `tools/measure_heap.py` runs the game in mupen64plus through gloveworld's `tools/emu`. Nothing from mupen64plus is
  included.
- It can also use the angrylion RDP plugin, which `gloveworld/tools/emu/setup.sh` fetches and builds. It is
  under the MAME licence (https://github.com/ata4/angrylion-rdp-plus, file `MAME License.txt`). It is not included.

## 6. Unicorn and Keystone

- Sources: https://github.com/unicorn-engine/unicorn and https://github.com/keystone-engine/keystone
  (PyPI: `unicorn`, `keystone-engine`)
- Licence: GNU GPL version 2. The texts are `app/licenses/unicorn-COPYING` and `app/licenses/keystone-COPYING`.
- Use: libgarib's ROM tools import both. The desktop app bundles them. Each release carries their source packages
  (`tools/build_app.py sources`). `app/README.txt` tells players where the licences and sources are.

## 7. Python libraries in the desktop app

`app/requirements.txt` pins the versions. PyInstaller puts them in the app. The licence texts are in the `licenses/`
folder of the app archive, in a folder per library. The first column links the project.

| Library | Licence |
| --- | --- |
| [capstone](https://www.capstone-engine.org) | BSD 3-clause |
| [construct](https://github.com/construct/construct) | MIT |
| [dataclasses-json](https://github.com/lidatong/dataclasses-json) | MIT |
| [jmespath](https://github.com/jmespath/jmespath.py) | MIT |
| [kaitaistruct](https://github.com/kaitai-io/kaitai_struct_python_runtime) | MIT |
| [lxml](https://lxml.de) | BSD 3-clause |
| [numpy](https://numpy.org) | BSD 3-clause |
| [Pillow](https://python-pillow.org) | MIT-CMU (HPND) |
| [pygltflib](https://pypi.org/project/pygltflib/) | MIT |
| [PyYAML](https://pyyaml.org) | MIT |
| [PySide6](https://doc.qt.io/qtforpython-6/) (PySide6-Essentials, with Qt 6.9) | LGPL 3 (also GPL 2 or 3). Qt's libraries stay separate shared libraries in the app |
| [PyOpenGL](https://pyopengl.sourceforge.net) | BSD 3-clause style |
| [packaging](https://github.com/pypa/packaging) | Apache 2.0 or BSD 2-clause |
| [PyInstaller](https://pyinstaller.org) | GPL 2 or later, with an exception that allows bundled programs under any licence |
| Python and its standard library | Python Software Foundation licence |


## 8. Tools used but not included

These are used to build or test. Nothing from them is in the repository or in a release.

- The [decompals build of GCC 2.7.2 for MIPS](https://github.com/decompals/mips-gcc-2.7.2) (GPL 2), through gloveworld:
  builds `mod/mod.bin`. The output is machine code from the editor's own C files.
- GNU binutils for MIPS (GPL 3): `mips-linux-gnu-ld`, `nm` and `objcopy`
