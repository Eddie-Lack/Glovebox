# Import model

_Back to the [README](../README.md)_

`Import model…` (in `+ Add`, `Models`) turns a model from another game or a 3D program into Glover models

Glover can draw it and stand on it

## What you need

- One `.glb` file (binary glTF) with its textures inside
- Blender exports these, and so do most model rippers
- A `.gltf` file works if its data and images are embedded
- A map from another N64 game needs ripping first, for example from an emulator or its decompilation

## How it goes

1. Choose the file, name it and set the scale
2. The dialog shows the triangles, how many pieces it becomes and its size next to the level
3. Press `Import and place`
4. All the pieces appear as Scenery at the centre of the view, in one undo step

- A model over 1,500 triangles is cut into pieces along its longest side
- Each texture becomes a small N64 texture (at most 2,048 pixels, power-of-two sides)
- Transparent pixels become cut-outs

`Your imported models` at the top of the Models tab lists them. `Place` adds all the pieces again, `Remove` deletes the model

Each piece is also in the model list, for use as Background or Platform

Imported models are kept in the editor's user folder (`custom/`) and are in every level

## Limits

- 200,000 triangles and 64 MB per file
- No vertex colours, animation, skinning, or more than one texture per material
- Enemies, doors, water and scripts from another game don't carry over. Rebuild them with Glover's objects and puzzles
- A big map can need more memory than the game has, even with the Expansion Pak. The budget under `Checks` shows it
- On a real console, don't use an imported model as a Platform. Use Scenery or Background

Note: the patch you export holds the models, so only share levels with models you have the right to share
