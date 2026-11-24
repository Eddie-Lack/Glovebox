# Sharing levels

_Back to the [README](../README.md)_

## Export patch

`Export patch` in the toolbar makes a `.bps` patch. Players apply it to their own ROM

- The patch holds only what your edit changes
- It checks the original ROM, the result and itself
- It starts the game normally. `Patched ROM boots straight into this level` is a separate option
- An edit with imported models needs the Expansion Pak. The game mod loads the models from the player's own ROM
- `Copy the models into the ROM instead` puts the models in the patch. It gets big (one 4.9 KB model made a 165 KB patch) and holds game data. The export dialog warns you

Note: only share levels that use models you have the right to share

## Export level pack

`Export level pack` in the `Export patch` dialog saves the level as a level pack for g64coopdx, the online co-op game on PC

- A pack is a folder. Give it a name (1 to 32 plain ASCII characters), then pick where the folder goes
- Your imported models go in the pack's `models.bin`
- Models from other levels don't: the pack names their banks, and each player's game loads them from their own ROM
- The pack also names the texture banks of those models' sprites. g64coopdx needs them
- The status line tells you when your pack uses models from other levels
- The pack holds no game data

It refuses:

- a level that needs the Expansion Pak game mod (rotations and scales on enemies, platforms, powerups or water)
- a model made with other textures from one of the game's models
- an imported model as a cameo. Use it as a landscape, a background or a platform
- an imported model with the name of the ball or the hand (`gball`, `cball`, `GShinL`, `GShinR`, `gbl_bt`). `Remove` the model in the Models tab, give its file another name, import it again and place the new model
- more than 4 MB of imported models, or models from more than 4 banks of each kind. The texture banks of the sprites count too

## The patcher

`web/patcher.html` is for players. It's one file and it works offline

1. Open it in a browser
2. Choose your Glover (USA) ROM (`.z64`, `.v64` or `.n64`)
3. Choose the `.bps` patch
4. Click `Create patched ROM`

- Your ROM never leaves the page
- It refuses a wrong ROM or a damaged patch
- Any other BPS patcher works too, on a `.z64` ROM

## Level files

- `Export XML` saves the level as libgarib XML
- `Import XML` reads one back. It checks the file first and tells you what's wrong, by element
- The file size limit is 4 MB

Note: an XML or `.lev` that uses imported models, or rotates or scales enemies, platforms, powerups or water, only works in a ROM the editor built. The editor warns you
