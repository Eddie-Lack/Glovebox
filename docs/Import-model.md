# Import model

_Back to the [README](../README.md)_

`Import model…` (in `+ Add`, `Models`) turns a model from another game or a 3D program into Glover models

The game draws it and Glover can stand on it

## What you need

- One `.glb` file (binary glTF) with its textures inside
- Blender exports these, and so do most model rippers
- A `.gltf` text file works too, if its data and images are embedded in it (not in files next to it)
- Textures can be PNG or JPEG
- A map from another N64 game has to be ripped first, for example from an emulator or its decompilation

## Importing a model

1. Choose the file, name it and set the scale
2. The dialog shows the triangles, how many pieces it becomes and its size next to the level
3. Click `Import and place`
4. All the pieces appear as Scenery at the center of the view, in one undo step

- A model over 1,500 triangles is cut into pieces along its longest side
- Each texture becomes a small N64 texture (at most 2,048 pixels, power-of-two sides)
- Transparent pixels become cut-outs

`Your imported models` at the top of the Models tab lists them. `Place` adds all the pieces again, `Remove` deletes the model

After `Remove`, levels that use the model show a placeholder. They can't be built until you remove those pieces

Each piece is also in the model list, for use as Background or Platform

Models you imported with an older version of the editor work as platforms too. You don't have to import them again

Imported models are kept in the editor's user folder (`custom/`) and you can use them in every level

The user folder is `%LOCALAPPDATA%\Glovebox` on Windows and `~/.local/share/glovebox` on Linux

## Limits

- 200,000 triangles and 64 MB per file
- No vertex colors, animation, skinning, or more than one texture per material
- The editor can't read a few rare kinds of JPEG: arithmetic-coded, CMYK and 12-bit ones. Save the texture as PNG instead
- Enemies, doors, water and scripts from another game don't carry over. Rebuild them with Glover's objects and puzzles
- A big map can need more memory than the game has, even with the Expansion Pak. `Checks` shows how much memory the level has left

Note: the patch you export holds the models, so only share levels with models you have the right to share
