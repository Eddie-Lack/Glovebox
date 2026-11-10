#!/usr/bin/env python3
"""Checks the desktop editor (desktop/) without a ROM, on the made-up levels of tools/desktop_fixture.py.

    xvfb-run -a python3 tools/desktop_test.py [--shots DIR]

It needs a display with OpenGL 3.3 (xvfb-run and Mesa are enough on Linux). It drives the editor through its
window: real mouse clicks and drags in the 3D view (picking, the gizmo, box select, the path tool), the keyboard
shortcuts, the panels and the dialogs' checks. --shots saves a picture of the window after some of the checks.
"""
import argparse, math, os, sys, tempfile, traceback

HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "tools"))

import numpy as np
from PySide6.QtCore import QPoint, Qt, QTimer
from PySide6.QtTest import QTest
from PySide6.QtWidgets import QApplication

from desktop import xmlutil
from desktop.sanitize import sanitize_level_xml
from desktop.viewport import gl_problem, prepare_qt

RESULTS = []
# Check names hold arrows and the like; a Windows console's code page can't print them
for _stream in (sys.stdout, sys.stderr):
    try:
        _stream.reconfigure(encoding="utf-8", errors="replace")
    except (AttributeError, ValueError):
        pass


def check(name, ok, detail=None):
    RESULTS.append((name, bool(ok), detail))
    print("%s  %s%s" % ("ok  " if ok else "FAIL", name, ("  (%s)" % (detail,)) if not ok and detail is not None else ""), flush=True)


def wait(app, ms=50):
    QTest.qWait(ms)


def px(w, p):
    """The view's pixel for a world point"""
    v = w.ed.camera.to_pixels(p)
    return QPoint(int(round(v[0])), int(round(v[1])))


def ent(ed, pred):
    return next((e for e in ed.entities if pred(e)), None)


def run(w, app, shots):
    ed, view = w.ed, w.view

    def shot(name):
        if shots:
            wait(app, 400)   # new widgets are laid out by the event loop
            w.grab().save(os.path.join(shots, name + ".png"))

    while w.busy or ed.root is None:
        wait(app, 100)
    wait(app, 300)
    check("level loads", ed.scene["id"] == "10" and len(ed.entities) > 15, len(ed.entities))
    kinds = {e.kind for e in ed.entities}
    check("every kind of entity is built", {"land", "platform", "garib", "enemy", "pickup", "tip", "spawn", "wind", "hazard",
                                            "sound", "trigger"} <= kinds, sorted(kinds))
    check("models load", ent(ed, lambda e: e.kind == "land" and e.obj.children) is not None and not any(e.missing_model for e in ed.entities))
    check("checks run", any("No new problems" in c.text for c in ed.checks()))
    shot("01-loaded")

    # Picking: a click on the enemy selects it
    enemy = ent(ed, lambda e: e.kind == "enemy")
    ed.focus_on(enemy)
    view.repaint()
    wait(app)
    QTest.mouseClick(view, Qt.LeftButton, Qt.NoModifier, px(w, enemy.obj.world_pos()))
    wait(app)
    check("click picks the enemy", ed.selection == [enemy], [e.label for e in ed.selection])
    check("the inspector shows it", any(getattr(c, "text", lambda: "")() == enemy.label for c in w.inspector.body.findChildren(type(w.status_text))))

    # The gizmo: drag the X arrow
    g = ed.gizmo
    check("the gizmo is attached", g.attached and g.mode == "translate")
    before = enemy.bind.get()
    s = g.size(ed.camera)
    a = px(w, g.pos + np.array([1.0, 0, 0]) * s * 0.7)
    b = QPoint(a.x() + 60, a.y())
    QTest.mouseMove(view, a)
    QTest.mousePress(view, Qt.LeftButton, Qt.NoModifier, a)
    for i in range(1, 7):
        QTest.mouseMove(view, QPoint(a.x() + 10 * i, a.y()))
        wait(app, 10)
    QTest.mouseRelease(view, Qt.LeftButton, Qt.NoModifier, b)
    wait(app)
    enemy = ent(ed, lambda e: e.kind == "enemy")
    after = enemy.bind.get()
    check("dragging the X arrow moves the enemy along x", abs(after[0] - before[0]) > 1 and abs(after[1] - before[1]) < 1e-6 and abs(after[2] - before[2]) < 1e-6,
          (before, after))
    check("snap 1 keeps whole numbers", abs(after[0] - round(after[0])) < 1e-6, after)
    check("the drag is one undo step", len(ed.undo_stack) == 1)
    ed.undo()
    enemy = ent(ed, lambda e: e.kind == "enemy")
    check("undo puts it back", np.allclose(enemy.bind.get(), before) and ed.selection == [enemy])
    ed.redo()
    enemy = ent(ed, lambda e: e.kind == "enemy")
    check("redo moves it again", np.allclose(enemy.bind.get(), after))

    # Rotation: E, then drag the Y ring
    QTest.keyClick(view, Qt.Key_E)
    wait(app)
    check("E switches to rotate", ed.mode == "rotate" and g.mode == "rotate")
    hs = g.handles(ed.camera)["Y"][1]
    pts = [px(w, p) for p in hs]
    i0 = max(range(len(pts)), key=lambda i: pts[i].y())     # the ring's nearest point
    a, b = pts[i0], pts[(i0 + 6) % len(pts)]
    QTest.mouseMove(view, a)
    QTest.mousePress(view, Qt.LeftButton, Qt.NoModifier, a)
    for t in np.linspace(0, 1, 8):
        QTest.mouseMove(view, QPoint(int(a.x() + (b.x() - a.x()) * t), int(a.y() + (b.y() - a.y()) * t)))
        wait(app, 10)
    QTest.mouseRelease(view, Qt.LeftButton, Qt.NoModifier, b)
    wait(app)
    enemy = ent(ed, lambda e: e.kind == "enemy")
    yr = xmlutil.num(enemy.node, "y_rotation")
    check("dragging the Y ring turns the enemy (y_rotation)", abs(yr) > 0.1 and xmlutil.first(enemy.node, "SetActorRotation") is None, yr)
    check("rotation snaps to 15°", abs(math.degrees(yr) / 15 - round(math.degrees(yr) / 15)) < 1e-3, math.degrees(yr))
    QTest.keyClick(view, Qt.Key_W)

    # Numeric transform in the inspector
    ed.apply_transform(rot_deg=90, axis="x")
    enemy = ent(ed, lambda e: e.kind == "enemy")
    check("a tilt about x needs the game mod (SetActorRotation)", xmlutil.first(enemy.node, "SetActorRotation") is not None and ed.mod_users())
    check("the checks say so", any("Expansion Pak game mod" in c.text for c in ed.checks()))
    ed.undo()

    # Scale a landscape piece
    grave = ent(ed, lambda e: e.label == "grave")
    ed.select(grave)
    ed.apply_transform(scale=2)
    grave = ent(ed, lambda e: e.label == "grave")
    check("scale writes SetActorScale", np.allclose(grave.get_scale(), [2, 2, 2]))
    check("rotation survives a scale", abs(vec_y(grave) - 0.6) < 1e-3)

    # Group rotation about the group centre
    garibs = [e for e in ed.entities if e.kind == "garib"]
    ed.set_selection(garibs)
    c0 = sum(e.bind.get() for e in garibs) / len(garibs)
    ed.apply_transform(rot_deg=90, axis="y")
    garibs = [e for e in ed.entities if e.kind == "garib"]
    c1 = sum(e.bind.get() for e in garibs) / len(garibs)
    check("a group rotation keeps the centre and moves the garibs", np.allclose(c0, c1, atol=0.01) and len(ed.selection) == 3)

    # Box select
    ed.set_selection([])
    ed.frame_level()
    view.repaint()
    QTest.keyClick(view, Qt.Key_B)
    check("B turns box select on", ed.box_mode and not g.attached)
    r = view.rect()
    QTest.mousePress(view, Qt.LeftButton, Qt.NoModifier, QPoint(5, 5))
    QTest.mouseMove(view, QPoint(r.width() // 2, r.height() // 2))
    QTest.mouseMove(view, QPoint(r.width() - 5, r.height() - 5))
    QTest.mouseRelease(view, Qt.LeftButton, Qt.NoModifier, QPoint(r.width() - 5, r.height() - 5))
    wait(app)
    sel_kinds = {e.kind for e in ed.selection}
    check("box select takes what is in the box, without the landscape", len(ed.selection) > 5 and "land" not in sel_kinds and not ed.box_mode, sel_kinds)
    shot("02-box-select")

    # Ctrl+click toggles
    first_sel = ed.selection[0]
    ed.focus_on(first_sel)
    view.repaint()
    n = len(ed.selection)
    QTest.mouseClick(view, Qt.LeftButton, Qt.ControlModifier, px(w, first_sel.obj.world_pos()))
    check("Ctrl+click takes one out of the selection", len(ed.selection) == n - 1 and first_sel not in ed.selection)

    # Duplicate and delete
    tip = ent(ed, lambda e: e.kind == "tip")
    ed.select(tip)
    QTest.keyClick(view, Qt.Key_D, Qt.ControlModifier)
    tips = [e for e in ed.entities if e.kind == "tip"]
    check("Ctrl+D duplicates, 40 to the side", len(tips) == 2 and abs(tips[1].bind.get()[0] - tips[0].bind.get()[0] - 40) < 1e-6)
    QTest.keyClick(view, Qt.Key_Delete)
    check("Delete removes the copy", len([e for e in ed.entities if e.kind == "tip"]) == 1)

    # Camera zones: the lone trigger volume duplicates with its puzzle; a volume of a puzzle with two doesn't
    zone = ent(ed, lambda e: e.zone)
    ed.select(zone)
    ed.duplicate()
    check("a camera zone duplicates whole", len([e for e in ed.entities if e.zone]) == 2 and len(list(ed.root.iter("Puzzle"))) == 3)
    ed.undo()
    two = ent(ed, lambda e: e.kind == "trigger" and not e.zone)
    ed.select(two)
    ed.remove()
    check("a volume of a two-condition puzzle stays", len(list(ed.root.iter("Puzzle"))) == 2 and "can’t be duplicated" in w.status_text.text())
    ed.select(zone := ent(ed, lambda e: e.zone))
    from desktop.entities import camera_zone, set_camera_zone, preset_zone, zone_centre
    ed.mutate(lambda: set_camera_zone(zone.node, preset_zone("high", 0, zone_centre(camera_zone(zone.node)["vol"]))))
    z = camera_zone(ent(ed, lambda e: e.zone).node)
    check("a preset rewrites the zone's actions", z["height"] == 320 and z["distance"] == 140 and z["look"] is None)
    ed.add_camera_zone("fixed")
    check("Add camera zone adds a puzzle with a fly-to point", camera_zone(ed.selected.node)["fly"] is not None)

    # Shift+click drops a garib on the ground
    ed.set_selection([])
    ed.frame_level()
    view.repaint()
    n = len([e for e in ed.entities if e.kind == "garib"])
    QTest.mouseClick(view, Qt.LeftButton, Qt.ShiftModifier, QPoint(view.width() // 2, int(view.height() * 0.75)))
    check("Shift+click drops a garib", len([e for e in ed.entities if e.kind == "garib"]) == n + 1)

    # The palette
    QTest.keyClick(view, Qt.Key_A)
    wait(app)
    check("A opens the palette", w.dock_palette.isVisible())
    for i in range(5):
        w.palette.tabs.setCurrentIndex(i)
        wait(app, 100)
    # A real click on a preset's Add button (buttons get Qt's `checked` argument)
    from PySide6.QtWidgets import QPushButton as _PB, QToolButton as _TB
    w.palette.tabs.setCurrentIndex(4)
    wait(app, 200)
    n = len(xmlutil.kids(ed.root, "Platform"))
    adds = [b for b in w.palette.scroll.widget().findChildren(_PB) + w.palette.scroll.widget().findChildren(_TB) if b.text() == "Add"]
    if adds:
        QTest.mouseClick(adds[2], Qt.LeftButton)
        wait(app, 100)
    check("clicking a preset's Add adds it", len(xmlutil.kids(ed.root, "Platform")) > n, w.status_text.text())
    ed.undo()
    w.palette.tabs.setCurrentIndex(2)
    wait(app, 300)
    for _ in range(40):
        wait(app, 50)
    check("model thumbnails are drawn", sum(1 for v in w.palette.thumbs.done.values() if v is not None) >= 3, len(w.palette.thumbs.done))
    shot("03-palette")
    sign = "0x1E5700A5"
    ed.add_model(sign, "LandActor")
    check("a model from another bank is an import", sign in ed.current_imports())
    for _ in range(100):   # the plan runs in the background, a moment after the edit
        if ed.plan is not None and sign in ed.plan["objects"]:
            break
        wait(app, 50)
    check("the import plan runs", ed.plan is not None and sign in ed.plan["objects"])
    check("imports show in the checks", "IMPORTS AT ROM BUILD" in " ".join(l.text() for l in w.checks.body.findChildren(type(w.status_text))))
    ed.add_enemy("dennis")
    check("Add enemy places one", len([e for e in ed.entities if e.kind == "enemy"]) == 2)
    ed.add_powerup(7, "BOWLINGBALL")
    ed.add_tip()
    check("Add powerup and Mr. Tip", len([e for e in ed.entities if e.kind == "pickup"]) == 2 and len([e for e in ed.entities if e.kind == "tip"]) == 2)
    QTest.keyClick(view, Qt.Key_Escape)

    # The path tool
    ed.set_selection([])
    w.set_path_mode(True)
    view.repaint()
    for x in (0.3, 0.5, 0.7):
        QTest.mouseClick(view, Qt.LeftButton, Qt.NoModifier, QPoint(int(view.width() * x), int(view.height() * 0.7)))
    check("clicks lay a path", len(ed.path["pts"]) == 3, len(ed.path["pts"]))
    n = len([e for e in ed.entities if e.kind == "garib"])
    w.path_panel.place()
    added = len([e for e in ed.entities if e.kind == "garib"]) - n
    check("Place puts garibs along it", added >= 3 and len(ed.selection) == added, added)
    # Camera zones over the path: boxes 140 to the sides, 300 below and 400 above
    nz = len(xmlutil.kids(ed.root, "Puzzle"))
    ed.path_zones("high", 0.0, 1.0, split=0)
    pz = xmlutil.kids(ed.root, "Puzzle")
    vol = pz[-1].find(".//PuzzleCondGloverWithinVolume") if len(pz) > nz else None
    pts = ed.path["pts"]
    check("a camera zone fits the path", vol is not None and abs(float(vol.get("x")) - (min(p[0] for p in pts) - 140)) <= 21
          and abs(float(vol.get("w")) - (max(p[1] for p in pts) - min(p[1] for p in pts) + 700)) <= 21, vol.attrib if vol is not None else None)
    acts = {a.get("action_type") for a in xmlutil.kids(pz[-1], "PuzzleAction")}
    check("the zone has the preset's actions", acts == {"70", "71"}, acts)
    ed.undo()
    ed.path_zones("side", 0.0, 1.0, split=100)
    check("a long stretch is split into boxes", len(xmlutil.kids(ed.root, "Puzzle")) - nz >= 2)
    ed.undo()
    # Scatter: the same seed gives the same result, and the children come first
    land = ent(ed, lambda e: e.tag == "BackgroundActor") or ent(ed, lambda e: e.tag == "LandActor")
    ed.set_selection([land])
    before = ed.serialize()
    ed.scatter_along_path(spacing=(150, 200), seed=3)
    one = ed.serialize()
    ed.undo()
    ed.set_selection([ent(ed, lambda e: e.node.get("name") == land.node.get("name") and e.tag == land.tag)])
    ed.scatter_along_path(spacing=(150, 200), seed=3)
    check("scatter with the same seed gives the same level", ed.serialize() == one and one != before)
    copies = [n for n in xmlutil.kids(ed.root) if n.tag == "BackgroundActor" and xmlutil.first(n, "SetActorRotation") is not None][-1:]
    check("a scattered copy is turned with SetActorRotation first", copies and xmlutil.kids(copies[0])[0].tag == "SetActorRotation")
    check("the status line gives the heap cost", "KB of heap" in w.status_text.text(), w.status_text.text())
    ed.undo()
    shot("04-path")
    QTest.keyClick(view, Qt.Key_Escape)
    check("Esc leaves the path tool", not ed.path["on"])

    # Fly camera
    QTest.keyClick(view, Qt.Key_C)
    check("C starts flying", view.fly["on"])
    p0 = ed.camera.pos.copy()
    QTest.keyPress(view, Qt.Key_W)
    # The camera moves on the view's frame timer; with software OpenGL a frame can take a while
    for _ in range(60):
        wait(app, 50)
        if float(np.dot(ed.camera.pos - p0, ed.camera.forward())) > 10:
            break
    QTest.keyRelease(view, Qt.Key_W)
    check("W flies forward", float(np.dot(ed.camera.pos - p0, ed.camera.forward())) > 10)
    QTest.keyClick(view, Qt.Key_Escape)
    check("Esc stops flying", not view.fly["on"])

    # Layers
    ed.set_layer("trigger", False)
    check("hiding a layer hides it", all(not e.obj.visible for e in ed.entities if e.kind == "trigger"))
    ed.set_layer("trigger", True)

    # Autosave and restore
    ed.flush_draft()
    check("edits are autosaved", w.drafts.load("10") is not None)
    xml = ed.serialize()
    w.load_scene("11")
    while w.busy or ed.scene["id"] != "11":
        wait(app, 50)
    w.load_scene("10")
    while w.busy or ed.scene["id"] != "10":
        wait(app, 50)
    check("the draft comes back", ed.serialize() == xml and "Restored your edits" in w.status_text.text(), w.status_text.text())
    check("the level list marks edited levels", "edited" in w.scene_box.currentText())
    ed.reset_level()
    check("Reset level goes back to the shipped level and drops the draft", not ed.is_edited() and w.drafts.load("10") is None)
    ed.undo()
    check("undo brings the edits back after a reset", ed.serialize() == xml)

    # Building: the fixture assembles .lev files with libgarib
    data, name, msg = ed.build("lev")
    check(".lev builds and the schema accepts the edits", len(data) > 100 and name.endswith(".lev"), msg)
    data, name, msg = ed.build("xml")
    res = sanitize_level_xml(data.decode(), ed.decomp)
    check("Export XML passes the import checks", res.ok, res.errors[:2])
    try:
        ed.build("rom")
        check("ROM builds say the fixture has no ROM", False)
    except ValueError as e:
        check("ROM builds say the fixture has no ROM", "no ROM" in str(e))

    # Import XML: a good file and hostile ones
    ok = sanitize_level_xml(xml, ed.decomp)
    check("the sanitizer accepts the editor's own XML", ok.ok, ok.errors[:2])
    bad = [
        '<!DOCTYPE x [<!ENTITY a "b">]><Level/>',
        '<Level name="x"><Script/><EndLevelData/></Level>',
        '<Level name="x"><LandActor object_id="zz" name="a" x="0" y="0" z="0"/><EndLevelData/></Level>',
        '<Level name="x"><PlatScale x="1" y="1" z="1"/><EndLevelData/></Level>',
        '<Level name="x"><EndLevelData/><MrTip x="0" y="0" z="0" message_id="1"/></Level>',
        '<Level name="x"><LandActor object_id="0x1" name="averylongname" x="0" y="0" z="0"/><EndLevelData/></Level>',
    ]
    check("the sanitizer refuses hostile and broken XML", all(not sanitize_level_xml(b, ed.decomp).ok for b in bad),
          [sanitize_level_xml(b, ed.decomp).errors[:1] for b in bad])
    ed.confirm_import(ok)
    check("an import is one undo step", ed.serialize() == xml and len(ed.undo_stack) > 0)

    # Thumbnails, the far plane and the checks' click targets
    dl = [c for c in ed.checks() if c.text.startswith("Draw load")]
    check("the draw load check is there", len(dl) == 1)
    dl[0].on_click()
    check("clicking it shows the far plane", ed.far_sphere is not None)
    shot("05-far-plane")

    # Rules from the decomp: a bad trigger volume
    vol = next(ed.root.iter("PuzzleCondBallWithinRangeOfPoint"))
    ed.mutate(lambda: vol.set("range", "0.0"))
    check("a zero-range trigger is flagged", any("range = 0" in c.text for c in ed.checks()))
    ed.undo()

    # Inspector edits
    tip = ent(ed, lambda e: e.kind == "tip")
    ed.select(tip)
    ed.set_attr(tip.node, "message_id", "5")
    check("an inspector field edits the XML", ent(ed, lambda e: e.kind == "tip").node.get("message_id") == "5")
    shot("06-inspector")

    # Commands inside elements: add, refuse a second one, move, remove; the schema check
    spin = ent(ed, lambda e: e.label == "spinner")
    ed.select(spin)
    allowed = ed.format.children_of("Platform")
    check("the Add command menu lists platform commands", {"PlatSpecial0xb9", "SetExit", "PlatCheckpoint"} <= set(allowed))
    ed.add_command(spin.node, "PlatSpecial0xb9")
    ed.add_command(spin.node, "SetActorRotation")
    tags = [c.tag for c in xmlutil.kids(ent(ed, lambda e: e.label == "spinner").node)]
    check("SetActorRotation goes first, others last", tags[0] == "SetActorRotation" and tags[-1] == "PlatSpecial0xb9", tags)
    n = len(ed.undo_stack)
    spin = ent(ed, lambda e: e.label == "spinner")
    ed.add_command(spin.node, "PlatSpecial0xb9")
    check("a second one is refused", len(ed.undo_stack) == n and "already has" in w.status_text.text())
    check("the schema accepts the commands", ed.backend.validate(ed.serialize()) == [])
    sp = xmlutil.first(spin.node, "PlatSpecial0xb9")
    ed.move_node(sp, -1)
    spin = ent(ed, lambda e: e.label == "spinner")
    tags2 = [c.tag for c in xmlutil.kids(spin.node)]
    check("↑ moves a command earlier", tags2.index("PlatSpecial0xb9") == len(tags2) - 2, tags2)
    ed.remove_node(xmlutil.first(spin.node, "PlatSpecial0xb9"))
    check("× removes it", xmlutil.first(ent(ed, lambda e: e.label == "spinner").node, "PlatSpecial0xb9") is None)
    enemy = ent(ed, lambda e: e.kind == "enemy")
    ed.add_command(enemy.node, "SetActorScale")
    check("nothing goes after EnemyFinalize", [c.tag for c in xmlutil.kids(ent(ed, lambda e: e.kind == "enemy").node)][-1] == "EnemyFinalize")
    end = xmlutil.first(ed.root, "EndLevelData")
    ed.remove_node(end)
    check("EndLevelData can't be removed", xmlutil.first(ed.root, "EndLevelData") is not None)
    last = xmlutil.kids(ed.root)[-2]
    ed.move_node(last, 1)
    check("nothing moves past EndLevelData", xmlutil.kids(ed.root)[-1].tag == "EndLevelData")
    bad = xmlutil.first(ent(ed, lambda e: e.label == "spinner").node, "PlatConstantSpin")
    ed.set_attr(bad, "axis", "w")
    check("a value the format refuses shows in the checks", any("libgarib's format" in c.text for c in ed.checks()))
    ed.undo()

    # The order the game reads a platform in, and what a command needs first
    mark = len(ed.undo_stack)
    plat = xmlutil.make_el("Platform", {"object_id": spin.node.get("object_id"), "name": "order"})
    plat.append(xmlutil.make_el("PlatSetInitialPos", {"x": "0.0", "y": "0.0", "z": "0.0"}))
    ed.add_element(lambda: plat)
    plat = xmlutil.kids(ed.root)[-2]
    ed.add_command(plat, "PlatScale")
    check("PlatScale goes before the platform's position", [c.tag for c in xmlutil.kids(plat)][:2] == ["PlatScale", "PlatSetInitialPos"],
          [c.tag for c in xmlutil.kids(plat)])
    check("a new scale is 1, not 0", xmlutil.first(plat, "PlatScale").get("x") == "1.0")
    sc = xmlutil.first(plat, "PlatScale")
    ed.move_node(xmlutil.kids(plat)[1], -1)
    check("a move the game would read wrong is refused", xmlutil.kids(plat)[0] is sc and "Can't move" in w.status_text.text(),
          w.status_text.text())
    n = len(ed.undo_stack)
    ed.add_command(plat, "PlatSpinPause0x7c")
    check("a spin pause without a spin is refused", len(ed.undo_stack) == n and "spin" in w.status_text.text())
    ed.add_command(plat, "PlatConstantSpin")
    ed.add_command(plat, "PlatSetTag")
    ed.add_command(plat, "PlatSpinPause0x7c")
    tags = [c.tag for c in xmlutil.kids(plat)]
    check("a spin pause goes right after its spin", tags.index("PlatSpinPause0x7c") == tags.index("PlatConstantSpin") + 1, tags)
    n = len(ed.undo_stack)
    ed.add_command(plat, "PlatRocking")
    check("a second motion is refused", len(ed.undo_stack) == n)
    ed.remove_node(xmlutil.first(plat, "PlatConstantSpin"))
    check("removing a spin removes its pause", xmlutil.first(plat, "PlatSpinPause0x7c") is None and xmlutil.first(plat, "PlatSetTag") is not None)
    for i in range(20):
        ed.add_command(plat, "PlatPathPoint")
    n = len(ed.undo_stack)
    ed.add_command(plat, "PlatPathPoint")
    pts = xmlutil.kids(plat, "PlatPathPoint")
    check("a 21st path point is refused", len(pts) == 20 and len(ed.undo_stack) == n, len(pts))
    check("path points follow each other 100 apart", float(pts[1].get("x")) - float(pts[0].get("x")) == 100.0)
    # Tidy order puts back an order made by hand
    for p_ in xmlutil.kids(plat, "PlatPathPoint"):
        plat.remove(p_)
    plat.remove(sc)
    plat.append(sc)
    ed.tidy_node(plat)
    check("Tidy order moves the scale back before the position", xmlutil.kids(plat)[0].tag == "PlatScale")
    for c in ed.checks():
        if c.level == "err" and "order" in c.text:
            check("a tidy platform has no order errors", False, c.text)

    # Enemies keep a normal instruction and their EnemyFinalize; empty groups go
    enemy = ent(ed, lambda e: e.kind == "enemy").node
    check("EnemyFinalize can't be removed", ed.removal_refused(xmlutil.first(enemy, "EnemyFinalize")) is not None)
    check("fog and ambient light can't be removed", all(ed.removal_refused(xmlutil.first(ed.root, t)) for t in ("FogConfiguration", "AmbientLight")
                                                         if xmlutil.first(ed.root, t) is not None))
    ed.add_enemy("unknown-type")
    new = xmlutil.kids(ed.root, "Enemy")[-1]
    check("an enemy with no shipped script gets a rest instruction", [c.tag for c in xmlutil.kids(new)] == ["NormalInstructions", "EnemyFinalize"])
    ins = xmlutil.first(xmlutil.first(new, "NormalInstructions"), "EnemyInstruction")
    check("its only normal instruction can't be removed", ed.removal_refused(ins) is not None)
    grp = xmlutil.make_el("AttackInstructions")
    grp.append(xmlutil.clone(ins))
    xmlutil.first(new, "EnemyFinalize").addprevious(grp)
    ed.remove_node(xmlutil.first(grp, "EnemyInstruction"))
    new = xmlutil.kids(ed.root, "Enemy")[-1]
    check("removing a group's last instruction removes the group", xmlutil.first(new, "AttackInstructions") is None)
    while len(ed.undo_stack) > mark:
        ed.undo()

    # The puzzle editor: a new puzzle, a condition and an action, their types, the checks
    mark = len(ed.undo_stack)
    from desktop import puzzles as PZ
    ed.add_puzzle()
    p = ed.puzzle_shown
    check("a new puzzle opens in the inspector", p is not None and "Puzzle %d" % len(xmlutil.kids(ed.root, "Puzzle")) in [
        x.text() for x in w.inspector.findChildren(type(w.status_text))])
    ed.add_puzzle_part(p, "PuzzleCond", 16)
    ed.add_puzzle_part(p, "PuzzleAction", 54)
    p = ed.puzzle_shown
    check("conditions go before actions, after the limit", [c.tag for c in xmlutil.kids(p)] == ["PuzzleNumtimes", "PuzzleCond", "PuzzleAction"])
    b = PZ.body(xmlutil.first(p, "PuzzleAction"))
    check("float fields are written as bits", b.get("hide_enabled") == "0x3F800000", b.attrib)
    ed.set_part_type(xmlutil.first(p, "PuzzleCond"), 33)
    p = ed.puzzle_shown
    check("changing a condition's type replaces its body", PZ.body(xmlutil.first(p, "PuzzleCond")).tag == "PuzzleCondSpecificEnemyExists"
          and xmlutil.first(p, "PuzzleCond").get("cond_type") == "33")
    ed.set_logic(p, "least", 3)
    check("a puzzle that needs more conditions than it has is an error", any("never fires" in c.text for c in ed.checks() if c.level == "err"))
    ed.set_logic(ed.puzzle_shown, "all")
    ed.set_fires(ed.puzzle_shown, None)
    check("no limit removes PuzzleNumtimes", xmlutil.first(ed.puzzle_shown, "PuzzleNumtimes") is None)
    check("the schema accepts the puzzle", ed.backend.validate(ed.serialize()) == [])
    xmlutil.first(ed.puzzle_shown, "PuzzleCond").set("cond_type", "35")
    check("a body that doesn't match its type is an error", any("expects a <PuzzleCondGloverWithinVolume>" in c.text for c in ed.checks()))
    # Real clicks on a condition card's buttons
    from PySide6.QtWidgets import QToolButton as _TB
    w.inspector.render()
    wait(app, 100)
    n = len(xmlutil.kids(ed.puzzle_shown, "PuzzleCond"))
    xs = [b for b in w.inspector.widget().findChildren(_TB) if b.text() == "×" and "Remove" == b.toolTip()]
    if xs:
        QTest.mouseClick(xs[0], Qt.LeftButton)
        wait(app, 100)
    check("clicking × on a condition removes it", len(xmlutil.kids(ed.puzzle_shown, "PuzzleCond")) == n - 1, w.status_text.text())
    ed.undo()
    # The checks of what a puzzle reads through a platform's pointers
    pz = ed.puzzle_shown
    ed.add_puzzle_part(pz, "PuzzleAction", 45)
    a = xmlutil.kids(ed.puzzle_shown, "PuzzleAction")[-1]
    ed.set_attr(PZ.body(a), "platform_tag", "-201")
    check("a negative nudge tag with no platform is an error", any("crashes when there is none" in c.text for c in ed.checks() if c.level == "err"))
    shot("07-puzzle")
    while len(ed.undo_stack) > mark:
        ed.undo()
    ed.open_puzzle(None)

    # Enemy scripts: a ready-made script, moving and removing with Goto targets that follow
    mark = len(ed.undo_stack)
    from desktop import scripts as SC
    enemy = ent(ed, lambda e: e.kind == "enemy")
    ed.select(enemy)
    ed.add_script_template(enemy.node, "Chase the glove")
    en = ent(ed, lambda e: e.kind == "enemy").node
    rows = SC.lists(en)["NormalInstructions"]
    check("a ready-made script replaces the normal list", [SC.code(r) for r in rows] == [4, 9, 18])
    check("the ready-made script passes the script checks", SC.rules(en, ed.flag_enum()) == [], SC.rules(en, ed.flag_enum()))
    ed.add_instruction(en, "NormalInstructions", 2)
    en = ent(ed, lambda e: e.kind == "enemy").node
    ed.move_instruction(SC.lists(en)["NormalInstructions"][3], -1)
    en = ent(ed, lambda e: e.kind == "enemy").node
    rows = SC.lists(en)["NormalInstructions"]
    check("moving an instruction keeps Goto pointing at the same one", [SC.code(r) for r in rows] == [4, 9, 2, 18]
          and SC.body(rows[3]).get("instr_idx") == "1")
    ed.remove_instruction(rows[0])
    en = ent(ed, lambda e: e.kind == "enemy").node
    rows = SC.lists(en)["NormalInstructions"]
    check("removing an instruction shifts later Goto targets", SC.body(rows[-1]).get("instr_idx") == "0", SC.body(rows[-1]).attrib)
    ed.add_instruction(en, "ConditionalInstructions", 8)
    en = ent(ed, lambda e: e.kind == "enemy").node
    check("a new kind gets its group before EnemyFinalize", [c.tag for c in xmlutil.kids(en)][-2:] == ["ConditionalInstructions", "EnemyFinalize"])
    check("Face the player gets a target", SC.flags_value(SC.lists(en)["ConditionalInstructions"][0].get("flags"), ed.flag_enum()) & 0x1C0)
    ed.set_instruction_type(SC.lists(en)["NormalInstructions"][-1], 9)
    check("a list that can run off its end is an error", any("reads past the list" in c.text for c in ed.checks() if c.level == "err"))
    check("the schema accepts the script", ed.backend.validate(ed.serialize()) == [])
    w.inspector.script_open = None
    w.inspector.render()
    wait(app, 100)
    en = ent(ed, lambda e: e.kind == "enemy").node
    before = [SC.code(r) for r in SC.lists(en)["NormalInstructions"]]
    from PySide6.QtWidgets import QLabel as _L
    card0 = next((l.parentWidget() for l in w.inspector.widget().findChildren(_L) if l.text().startswith("#0  ")), None)
    ups = [b for b in card0.findChildren(_TB) if b.text() == "↓"] if card0 else []
    if ups:
        QTest.mouseClick(ups[0], Qt.LeftButton)
        wait(app, 100)
    en = ent(ed, lambda e: e.kind == "enemy").node
    after = [SC.code(r) for r in SC.lists(en)["NormalInstructions"]]
    check("clicking ↓ on an instruction moves it", after[:2] == before[:2][::-1], (before, after))
    shot("08-enemy-script")
    while len(ed.undo_stack) > mark:
        ed.undo()

    # Level settings: a look preset, the file name check
    mark = len(ed.undo_stack)
    ed.apply_look("Graveyard violet")
    fog = xmlutil.first(ed.root, "FogConfiguration")
    check("a look preset sets the fog and lights", fog.get("near_clip") == "1150" and len(xmlutil.kids(ed.root, "DiffuseLight")) == 2)
    ed.set_attr(ed.root, "name", "LEVEL1")
    check("an even-length file name is an error", any("odd length" in c.text for c in ed.checks() if c.level == "err"))
    while len(ed.undo_stack) > mark:
        ed.undo()
    ed.set_level_info("name", "MY LEVEL")
    ed.set_level_info("song", 17)
    check("the level's title and music are kept", ed.level_info() == {"name": "MY LEVEL", "song": 17, "bonus_time": None, "extra": False}, ed.level_info())
    check("Build all lists the level", any(sid == ed.scene["id"] for sid, _, _ in ed.project_levels()))
    ed.set_level_info("name", None)
    ed.set_level_info("song", None)

    # Platform presets: each one is valid, and adds no problem of its own
    mark = len(ed.undo_stack)
    from desktop.presets import PRESETS
    base = {c.text for c in ed.checks() if c.level == "err"}
    for k in PRESETS:
        ed.add_preset(k)
    errs = [c.text for c in ed.checks() if c.level == "err" and c.text not in base and "isn't in any object bank" not in c.text]
    check("every platform preset passes the checks", not errs, errs[:3])
    check("the presets add their platforms", len([n for n in ed.root.iter("Platform") if n.get("name") in ("hoop.nd", "heart", "ballbit", "lift", "glovebu", "stepy.n", "ootelep")]) >= 16)
    while len(ed.undo_stack) > mark:
        ed.undo()


def vec_y(e):
    from desktop.vec import euler_yzx_from_quat
    return euler_yzx_from_quat(e.get_quat())[1]


def print_gl():
    """Which OpenGL the test runs on (on CI: Mesa's llvmpipe)"""
    from PySide6.QtGui import QOffscreenSurface, QOpenGLContext
    from desktop.viewport import surface_format
    from OpenGL import GL
    ctx, surface = QOpenGLContext(), QOffscreenSurface()
    ctx.setFormat(surface_format())
    ctx.create()
    surface.setFormat(ctx.format())
    surface.create()
    ctx.makeCurrent(surface)
    print("OpenGL: %s, %s" % (GL.glGetString(GL.GL_RENDERER).decode(), GL.glGetString(GL.GL_VERSION).decode()))
    ctx.doneCurrent()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--shots", help="save pictures of the window here")
    args = ap.parse_args()
    if args.shots:
        os.makedirs(args.shots, exist_ok=True)
    prepare_qt()   # as the app does
    app = QApplication(sys.argv)
    problem = gl_problem()
    if problem:
        print("FAIL  OpenGL 3.3: %s" % problem)
        return 1
    print_gl()
    from desktop.window import MainWindow, STYLE
    from desktop_fixture import FixtureBackend
    app.setStyleSheet(STYLE)
    w = MainWindow(FixtureBackend(), tempfile.mkdtemp(prefix="glovebox-test-"))
    w.resize(1500, 950)
    w.start()
    code = [1]

    def go():
        try:
            run(w, app, args.shots)
        except Exception:
            traceback.print_exc()
            RESULTS.append(("no exceptions", False, ""))
        failed = [r for r in RESULTS if not r[1]]
        print("%d checks, %d failed" % (len(RESULTS), len(failed)))
        code[0] = 1 if failed else 0
        app.quit()
    QTimer.singleShot(200, go)
    app.exec()
    return code[0]


if __name__ == "__main__":
    sys.exit(main())
