# Glovebox

A level editor for Glover (N64, USA). It runs on your computer, in its own window

Move things, add enemies, garibs and models from any level, import your own models, then test the level in your emulator or share it as a patch

## You need your own ROM!!

Glover (USA) as a `.z64`, `.v64` or `.n64` file

The editor never includes game data. Everything it shows comes from your ROM and stays on your computer

## The desktop app

1. Get the app for Windows from the _[releases page](https://github.com/Eddie-Lack/Glovebox/releases)_
2. Unzip it and start `Glovebox.exe`. The editor opens in its own window, in under a second
3. The first time, it asks for your ROM
4. That's it. The first level opens right away

You need 64-bit Windows 10 or 11

Any graphics card with Direct3D 12 or Vulkan works. Without one, the editor draws in software

Note: the app isn't signed, so Windows warns you _(see [the app's guide](app/README.txt))_

## Running it from source

You need [Rust](https://rustup.rs)

`cargo run -p glovebox --release`

It runs on Linux too

## More

- [Using the editor](docs/Using-the-editor.md)
- [Sharing levels](docs/Sharing-levels.md)
- [Import model](docs/Import-model.md)
- [The Expansion Pak game mod](docs/Game-mod.md)
- [Technical notes](docs/technical.md)
- [Third party notices](THIRD_PARTY_NOTICES.md)

## Thanks

- [naclomi](https://github.com/naclomi) for [libgarib](https://github.com/naclomi/libgarib) and generally being the GOAT
- [noclip.website](https://github.com/magcius/noclip.website) for the level list and how objects are placed
- The [decompals build of GCC 2.7.2 for MIPS](https://github.com/decompals/mips-gcc-2.7.2) builds the game mod
- Thanks to Piko Interactive for providing some of the files used. Check out [bleem.net](https://bleem.net)
