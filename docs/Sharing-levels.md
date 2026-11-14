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
