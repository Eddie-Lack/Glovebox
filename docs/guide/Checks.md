# The checks

_Back to the [guide](README.md)_

The `Checks` run the game's own level rules on your level, after every edit.

<img src="img/checks-panel.png" alt="The Checks panel: no new problems, and the draw load" width="380">

## Reading it

- A check mark means all good.
- Problems will say what's wrong, and on which object or puzzle when there's one.
- Click a row that names an object to select it and look at it. A row that names a puzzle opens it.
- With `Advanced details` on (in `Options`), many rows link to the place in the game's code that makes the rule.
- The links go to _[gloveworld](https://github.com/Eddie-Lack/gloveworld)_, the game's code turned back into C.
- Problems the vanilla level already has are labeled. (those ones aren't yours)

In the `Level` panel, the name of an object with an error changes color.

_[The reference](../Using-the-editor.md#checks)_ lists what gets checked.

## Far outside the level

An object far away from the rest of the level gets a warning. Click it to go there.

## Draw load

The game draws everything within its far plane, even what's behind the camera.

Too much at once and it lags. Past 256 object matrices it stops.

Each part of a model the game draws takes one object matrix, and so does each shadow and footprint. The estimate counts them.

| Estimate | What it means |
| --- | --- |
| Under 190 | Fine |
| 190 to 239 | It slows down |
| 240 or more | At or near the limit |

Some vanilla levels are over 190 too, and hub 1 is over 240. Your level only gets a warning or an error when it's busier than the vanilla one

Click the `Draw load` row. The view goes to the busiest spot and draws the far plane around it.

![The whole of Atlantis 1 inside the rings of its far plane](img/checks-draw-load.png)

Everything inside the rings is drawn when Glover stands in the middle.

To fix a high number, put fewer pieces close together, or shorten the far plane.

The far plane is the `near_clip` field of `FogConfiguration`, despite the name.

Note: it's an estimate.

## Memory

Under the rows, the panel says how much memory the level leaves free.

When you add models from other levels it lists each import, and shows a bar of the memory they take.

---

_[← Level settings](Level-settings.md)_ · _[Contents](README.md)_ · _[Import your own model →](Import-your-own-model-(untested).md)_
