# Test, build and share

_Back to the [guide](README.md)_

The bottom row of the toolbar has it all.

![The bottom row of the toolbar: the tools, then Import XML, Export XML, .lev, boot here, Export patch, Test level, Build ROM, Build all and Patcher](img/toolbar-build-row.png)

## Test level

`Test level` (or `T`) builds a ROM that starts in your level and opens it in your emulator of choice.

<img src="img/test-level.png" alt="The Test level message that asks for your emulator" width="560">

- The first time, click `OK` then navigate to where your emulator is saved on your computer and select it.
- After that, it's just one click.
- The ROM goes in the user folder (`test/`).
- Another emulator? `Options` > `Choose your emulator…`

## Build ROM

*Don't share game ROMs. This is just for testing or playing yourself.*

`Build ROM` patches your edits into a ROM and asks where to save it.

Check `boot here` and the ROM starts in your level. Without it the game starts as normal.

## Export patch

`Export patch` makes a `.bps` patch. That's the file to share: people can apply it to their own ROM.

<img src="img/export-patch.png" alt="The Export patch dialog: what the patch holds, and the Export .bps button" width="560">

- The patch holds only what your edit changes.
- It starts the game normally. Check `Patched ROM boots straight into this level` for a patch that doesn't.
- The dialog tells you when the patch needs the Expansion Pak.

`Patcher ↗` opens the patcher page. It's one file and it works offline. Share it with your patch.

The page is `patcher.html` in the user folder. The status bar says where that is.

_[Sharing levels](../Sharing-levels.md)_ shows how players use it.

`Export level pack` in the same dialog saves the level for g64coopdx, the online co-op game on PC. See _[Sharing levels](../Sharing-levels.md#export-level-pack)_.

## Build all

`Build all…` puts every level you've edited into one ROM or one patch.

<img src="img/build-all.png" alt="The Build all dialog: the edited levels, the level to boot into, Export .bps and Build ROM" width="560">

Pick the level it boots into, or a normal start.

The file gets named after the levels in it, like `glover-atlantis-level-1-and-atlantis-level-2.bps`.

## Level files

- `Export XML` saves the level as a file.
- `.lev` saves it the way the game stores it.
- `Import XML` reads an XML back. It checks the file first.

Dropping an `.xml` on the window imports it too.

### Some levels need the Expansion Pak!

The editor adds its game mod to the ROM or patch when your level:

- tilts enemies, or rotates platforms, powerups or water
- scales enemies, powerups or water
- has a `SetActorScale` command in a platform
- imports models
- has more scenery than the game's own memory holds
- is an extra level

The Checks and the export dialog say when that happens.

On an emulator, the Expansion Pak has to be on. Most have it on by default.

_[The Expansion Pak game mod](../Game-mod.md)_ has the details.

Note: every ROM and patch the editor builds uses Easy ball controls, obviously

---

_[← Import your own model](Import-your-own-model-(untested).md)_ · _[Contents](README.md)_ · _[Options →](Options.md)_
