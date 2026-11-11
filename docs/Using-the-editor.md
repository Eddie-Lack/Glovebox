# Using the editor

_Back to the [README](../README.md)_

## Moving around

- Pick a level in the list at the top, all 48 load
- Drag to turn the camera, right-drag to pan, wheel (or middle-drag) to zoom
- `F` frames the selection

`Fly` (or `C`) switches to a free camera:

- `W` `A` `S` `D` move along the view
- `Space` and `Shift` go straight up and down
- Drag to turn, wheel sets the speed
- `C` or `Esc` goes back to orbiting

Note: while you fly, those keys move the camera. Use the toolbar for Move and + Add, and Shift+click doesn't drop garibs

## Selecting and moving

- Click an object to select it
- `Ctrl`/`Cmd`+click adds to the selection
- `B` (or `Box select`) drags a box
- `Ctrl+A` selects everything visible except landscape
- `W` `E` `R` switch the gizmo between move, rotate and scale
- The inspector has number fields. With several objects it has "move / rotate / scale by"
- `Pivot` is the group's centre or each object's own origin
- Shift+click the ground to drop a garib
- Undo, redo, duplicate and delete work on the whole selection

An object's enemy script points and platform paths move with it

## What can rotate and scale

| Object | Rotation | Scale |
| --- | --- | --- |
| Land and background actors | X, Y, Z | X, Y, Z |
| Platforms | X, Y, Z (needs the mod) | One scale, before the first path point |
| Enemies | Y turns it, X and Z tilt it (needs the mod) | X, Y, Z (needs the mod) |
| Powerups | X, Y, Z, under their own spin (needs the mod) | X, Y, Z (needs the mod) |
| Water | X, Y, Z (needs the mod) | X, Y, Z (needs the mod) |
| Wind volumes | None | Resizes the box |
| Puzzle trigger volumes | None | Resizes the box, sphere or cylinder |
| Garibs, Mr. Tip, spawn points | None | None |

_See [The Expansion Pak game mod](Game-mod.md) for what "needs the mod" means_

## + Add

Opens a palette with a picture of each entry:

- `Collectables`: garib types, every powerup, Mr. Tip
- `Enemies`: all 52 types. A new enemy copies the shortest script that type has in a shipped level
- `Models`: all 1,522 models in the game. Place them as Scenery (solid), Background (no collision) or Platform
- `Kit`: props the editor makes itself, free to share _(see [Kit](#kit))_
- `Presets`: ready-made platforms _(see [Presets](#presets))_
- `Camera`: camera zones _(see [Camera zones](#camera-zones))_
- `Import model…` adds a model of your own _(see [Import model](Import-model.md))_

Note: boss enemies are tied to their arenas. The palette marks them

A model from another level or world is copied in when you build. The import panel shows its size against the level's free memory. The Checks panel lists every import

## Presets

Ready-made platforms, with their commands in the order the game reads them

- `Start ring`: Glover starts here. If the level has a start ring already, it moves that one
- `Checkpoint ring`: Glover restarts here once he's touched it
- `Level exit (heart)`: bring the ball to it to end the level
- `Bonus finish line`: five big hearts across the way, for a bonus level
- `World exit lift`: stand on it with the ball and it carries you out of the world
- `Lift`: rises once when Glover walks onto it. It comes with its puzzle
- `Slam button`: goes down when you slam it. Use its tag in a puzzle
- `Moving platform`, `Spinner`, `Conveyor`, `Ice floor`
- `Spike`: an invisible point that hurts Glover and pops the ball. Put any model on it
- `Exit zone`: an invisible level exit
- `Teleport pad`: sends Glover and the ball ahead. Change where in the inspector
- `Extra levels entry`: for hubs only _(see [The Expansion Pak game mod](Game-mod.md#extra-levels))_

Moving platform, spinner, conveyor and ice use the selected object's model if you've selected one

Note: platforms always use the game's own models. A platform with an imported model crashes a console

## Commands

Every object's card in the inspector has buttons:

- `+` adds a command inside it. The ones it can't take are greyed out, hover to see why
- `↑` `↓` move a command. A move the game would read wrong is refused
- `×` removes it, and anything that only worked with it
- `Tidy` (on platforms) puts commands the game would read wrong back in order

New commands start with the values the shipped levels use, and go where the game reads them right

## Puzzles

With nothing selected, the inspector lists every puzzle in the level. Click one to open it

- `Fires`: every frame while true, once, or a number of times
- `Needs`: all conditions, any condition, or at least a number of them
- `+ Add` adds a condition or an action. Pick its type in its card
- A puzzle with no conditions fires when the level starts

A trigger volume's card has `Edit its puzzle` too

Note: a puzzle with no limit runs its actions on every frame its conditions hold. Most puzzles want `Once`

## Enemy scripts

An enemy's card shows its script: the normal, conditional and attack lists

- `+ Add` adds an instruction, `…` shows its fields
- `↑` `↓` and `×` move and remove it. "Go to" targets follow
- Follow, flee and face pick their target: the glove, the ball or the closer one
- `Ready-made` swaps in a whole normal script: chase the glove, wander, patrol, or stand still

The checks show an error when a list could run off its end. End it with `Go to instruction`

## Level settings

With nothing selected you get the level's settings:

- `Look` sets the lights and fog to a preset: dark, coloured moods or daylight
- `File name` is the name inside the level file. Keep it 7 letters: an even length crashes a console
- `Title, music and clock`: the title in the level select, the music, and a countdown like a bonus level's
- `+ Add` adds fog, lights, a backdrop, gravity or an ambient sound

Note: at 0 the clock says TIME UP and sends you to the way room

Note: a ROM that boots straight into the level (`boot here`, `Test level`) plays its own music, not yours

## Checks

`Checks` runs the game's own level rules on your edits:

- 20 path points per platform
- Parent tags point at an earlier platform
- `PlatScale` and `PlatSetParent` come before the first path point
- Enemy scripts: more than 30 normal, 10 conditional or 30 attack instructions crash the game
- Trigger volumes that are flat or empty
- Exits that can never fire
- A checkpoint or start with no solid ground under it
- `fog_distance` of 1000 or more
- Platforms made from your own models _(a console stops on them)_

Each check links to the game's code. Problems a shipped level already has are labelled

### Draw load

The game draws everything within its far plane, even what's behind the camera. Too much at once and it lags, and past 256 object matrices it stops

`Checks` estimates the busiest spot in your level:

| Estimate | What it means |
| --- | --- |
| Under 190 | Fine |
| 190 to 239 | It slows down |
| 240 or more | At or near the limit |

Click the row to see the spot and the far plane

To fix it, put fewer pieces close together, or shorten the far plane (`near_clip` in `FogConfiguration`)

Note: it's an estimate. It came within 35 of the game in four levels, and ran up to 63 over in hub 1

## Puzzle trigger volumes

Drawn as see-through shapes, one per puzzle condition that tests where something is:

- Pink is Glover
- Orange is the ball
- Purple is the camera

Move and resize them with the gizmo or the inspector

Duplicate and delete take the whole puzzle when it has only that one volume. A volume of a puzzle with several can't be duplicated or deleted on its own

The `Triggers` button shows or hides them. The `Fog` button shows or hides the level's fog in the editor only (to change the game's fog, edit `FogConfiguration` in the inspector)

`Motion` spins the spinning platforms

## Exit triggers

Every exit is a platform with an `Exit` command. The game doesn't care what the model looks like: it tests a sphere round the platform's position

The green sphere shows that sphere at its real size. World entrances in the hubs and some level ends (Atlantis 3, Carnival 1) are a small hidden model, so the sphere is the only way to see how big they are

The `Exits` button in the toolbar shows or hides the spheres. The `Exit triggers` layer in the list on the left is the same switch

Who has to get inside the sphere:

| Exit | Who |
| --- | --- |
| A level's goal (type 1) | The ball, while Glover holds it |
| World entrances in the hub (types 6 to 11) | Glover |
| The hub's own doors (types 12, 13, 21) | Glover |
| Anything in the Wayroom, the Atlantis bonus, Atlantis 3 and Carnival 1 | Glover |
| Everything else | The ball |

It's the centre of Glover or the ball that counts, not the edge

Select the platform and the inspector's `Exit trigger` block tells you the radius, who has to touch it and what else the game wants for that exit type

- The size comes from the platform's model and its scale, so scale the platform to make the trigger bigger or smaller
- Clicks go through the sphere, so you can still select what's inside it
- A moving platform's sphere is drawn where the platform starts. In the game it moves with the platform

### An exit with no sphere can never fire!

The game measures the platform when it reads its position. An `Exit` of type `loading_zone` that comes before the position stops that, and the radius stays 0

The editor always adds the `Exit` after the position. `Checks` warns you when an imported level has it the other way round

## Camera zones

A box where the camera moves while Glover is inside it. When he leaves, the camera goes back. The original levels use about 170 of them

`+ Add` → `Camera`, then pick one:

| Zone | The camera |
| --- | --- |
| `Behind` | turns to look the way you go |
| `High` | rises and looks down on Glover |
| `Side` and `Other side` | swings round to the right or the left |
| `Front` | faces Glover, so you can't see where you're going |
| `Low` | drops to Glover's height and comes close |
| `Far` | stands far back and high |
| `Fixed` | flies to a point ahead and watches from there |

- A new zone expects you to walk the way the view looks
- Move and size the box like any trigger volume
- The inspector's `Camera zone` panel changes the preset, the heading, the look angle, the distance, the height and the point to fly to
- The dashed line shows about where the camera stands
- `Look from the zone's camera` shows about what you'll see

No game mod needed, they work on any ROM

## Kit

Props the editor makes itself, so they hold no game data and you can share them

- Graveyard: a gravestone and a signpost with your own words, a pumpkin with one of 8 faces, candles, a coffin, a broken fence, dead trees, a cobweb
- Ruins: rubble, a scorch mark, thorns
- Creatures: a shade, a giant hand, an eye, a pale face, a rag-doll glove, a hanging glove, gallows, a cage, a lantern

The picture follows what you type. `Background` places it with no collision, `Scenery` makes it solid

After that it's listed with your imported models, and it's copied into the level when you build

## New look

Select a piece of scenery with a model from the game, then use `New look` in the inspector:

1. Tick the worlds to take textures from
2. Press `Apply`. `Try another` picks again, `Original` goes back

The new look is kept with your imported models and copied into the level when you build

## Path

`Path` (or `P`) lays a path to place things along:

1. Click the ground to add points. `Backspace` takes the last one off
2. Pick `garibs`, or `copies of the selection` (scenery, kit props, enemies, powerups, Mr. Tip)
3. Set how far apart they go, and whether they go on the path or to one side or both
4. Press `Place`

Each one lands on the ground under it. `random turn` spins each copy of a single model

`Scatter` (under Scatter the selection) puts copies of the selected scenery beside the path:

- Random spacing, distance, height, scale and facing, in the ranges you set
- Select a few models and each copy picks one of them
- `Try another` takes it back and tries the next seed

`Add zones` (under Camera zone over the path) covers a stretch of the path with camera zones. Long stretches get split into shorter boxes

Note: the path is only a guide, the level doesn't keep it

## Autosave

- Every edit is saved in the app's user folder (`drafts/`)
- Reopen the level or the editor and your edits are back
- The level list marks edited levels with ●
- `Reset level` throws away a level's edits (you can still undo it)
- `Import XML` (or drop an `.xml` on the window) checks the file before it touches your level

## Test level

`Test level` (or `T`) builds a ROM that boots straight into the edited level and opens it in your emulator

- The first time, pick your emulator's program (ares, simple64, Project64…)
- The ROM goes in the user folder (`test/`)
- To change the emulator, delete the `emulator` line in `settings.ini` in the user folder

Note: a level that uses the game mod needs the emulator's Expansion Pak on _(most have it on)_

## Build ROM and export

- `Export XML` and `.lev` save the level where you choose
- `Build ROM` patches your edits into a ROM. `boot here` starts it in the edited level
- `Export patch` makes a patch to share _(see [Sharing levels](Sharing-levels.md))_
- `Build all…` puts every level you've edited into one ROM or patch. Pick the level it boots into, or a normal start

Every ROM and patch the editor builds uses Easy ball controls on every difficulty
