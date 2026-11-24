_Back to [README](../README.md)_

# The Expansion Pak game mod

The Expansion Pak is extra memory for the N64. On an emulator it's a setting, and most have it on

The editor adds the mod by itself when your level needs it _(see [When the editor adds it](#when-the-editor-adds-it))_

## What the mod adds

- Rotation and scale for enemies, platforms, powerups and water (the retail game only does it for landscape and background actors, which you place as `Scenery` and `Background`)
- More memory: 3,968 KB more for the level
- Models and textures from other worlds, loaded from the player's own ROM when the level starts
- Extra levels: new levels in the Space slots, with the shipped Space levels kept _(see [Extra levels](#extra-levels))_

Note: animated textures on models from other worlds don't animate

## When the editor adds it

`Build ROM` and `Export patch` add the mod when your level:

- rotates or scales one of those objects
- imports models
- has more scenery than the game's own memory holds
- is an extra level

`Checks` and the export dialog will say when it gets added

Other levels are built like normal

## Extra levels

An edited level normally takes the place of the shipped one

A Space level (1, 2, 3, the boss or the bonus) can be an extra level instead, and the shipped one stays

1. Open the Space level and edit it
2. With nothing selected, check `Extra level (Expansion Pak)` in the Inspector, under `Title, music and clock`
3. Open a hub, `+ Add`, `Presets`, `Extra levels entry`, and put it where you want the way in
4. `Build all…` puts the Space level and the hub into one ROM or patch

Walk Glover into the entry zone and you go to the Space way room, with your extra levels behind its doors

- Progress in the extra levels is saved apart from the shipped Space levels
- Every other portal takes you back to the shipped game
- A checkpoint ring that you add after you check `Extra level` counts when the ball rolls through it
- A ring that you added before you checked `Extra level` doesn't count when the ball rolls through it. Add those rings again

Note: the entry zone only works once the hub's portals work

## Limits

- Garibs and Mr. Tip are flat sprites. Spawn points (where Glover, the ball and the camera start) are points. None of them rotate
- Boss enemies are tied to their arenas
- Bullets and other projectiles of an enemy from another world are sometimes invisible

_See [Technical notes](technical.md) for how it works_
