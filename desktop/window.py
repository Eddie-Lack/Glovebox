"""The editor's main window: the toolbar, the 3D view, the side panels, the dialogs and the keyboard shortcuts."""
import os, subprocess, sys, time
from PySide6.QtCore import QSettings, Qt, QTimer, QUrl
from PySide6.QtGui import QAction, QActionGroup, QDesktopServices, QStandardItem, QStandardItemModel
from PySide6.QtWidgets import (QApplication, QCheckBox, QComboBox, QDialog, QDockWidget, QFileDialog, QHBoxLayout, QLabel,
                               QLineEdit, QMainWindow, QMessageBox, QPushButton, QSplitter, QToolBar, QVBoxLayout, QWidget)
from .editor import Editor
from .jobs import Jobs
from .panels import Checks, FitScroll, Inspector, Outliner, Palette, PathPanel, check_line, clear, label
from .sanitize import LIMITS, match_scenes, sanitize_level_xml
from .viewport import Viewport

HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

STYLE = """
* { font-size: 13px; }
QMainWindow, QWidget { background: #1d1f26; color: #e6e7ec; }
QToolBar { background: #1d1f26; border-bottom: 1px solid #323642; spacing: 4px; padding: 3px 6px; }
QToolBar QLabel { color: #9aa0ae; }
QPushButton, QToolButton { background: #252832; border: 1px solid #323642; border-radius: 6px; padding: 4px 10px; }
QPushButton:hover, QToolButton:hover { border-color: #4a5060; background: #2c303b; }
QPushButton:disabled, QToolButton:disabled { color: #6b7080; }
QToolButton:checked { background: #3a3220; color: #f2c14e; border-color: #f2c14e; }
QPushButton#primary, QToolButton#primary { background: #f2c14e; color: #231a00; border-color: #f2c14e; font-weight: 600; }
QPushButton#accent, QToolButton#accent { color: #f2c14e; border-color: #8a6f2c; }
QToolButton#mini { padding: 0px 5px; border-radius: 4px; font-size: 11px; }
QLineEdit, QComboBox, QPlainTextEdit { background: #15161b; border: 1px solid #323642; border-radius: 6px; padding: 3px 6px; }
QLineEdit#num, QLineEdit#field { min-width: 40px; }
QTreeWidget, QScrollArea { background: #1d1f26; border: none; }
QTreeWidget::item { padding: 2px; }
QDockWidget::title { background: #252832; padding: 4px 8px; }
QDockWidget { titlebar-close-icon: none; }
QStatusBar { background: #1d1f26; border-top: 1px solid #323642; }
QStatusBar QLabel[cls="ok"] { color: #6bd88a; }
QStatusBar QLabel[cls="err"] { color: #ff6b6b; }
QLabel#h2 { font-size: 16px; font-weight: 600; }
QLabel#h3 { font-weight: 600; }
QLabel#h4 { color: #9aa0ae; font-size: 11px; font-weight: 600; letter-spacing: 1px; margin-top: 6px; }
QLabel#muted, QCheckBox#muted { color: #9aa0ae; }
QLabel#note { color: #9aa0ae; font-size: 12px; }
QLabel#src { color: #9aa0ae; font-size: 11px; }
QLabel#op { color: #9aa0ae; font-family: monospace; }
QLabel#tok { color: #c58bff; font-family: monospace; }
QLabel#fieldname { color: #9aa0ae; font-family: monospace; font-size: 12px; }
QLabel#thumb { background: #15161b; border: 1px solid #323642; border-radius: 6px; }
QLabel#budget { color: #9aa0ae; }
QFrame#card { background: #252832; border: 1px solid #323642; border-radius: 8px; }
QFrame#decomp { background: #1d1f26; border-radius: 6px; }
QFrame#prow { border-bottom: 1px solid #323642; }
QFrame#chk { border-radius: 6px; border-left: 3px solid #9aa0ae; background: #252832; }
QFrame#chk[level="ok"] { border-left-color: #6bd88a; }
QFrame#chk[level="err"] { border-left-color: #ff6b6b; }
QFrame#chk[level="warn"] { border-left-color: #ffb36b; }
QFrame#chk[level="imp"] { border-left-color: #6fb7ff; }
QFrame#budget { border-radius: 6px; background: #252832; }
QFrame#budget[over="true"] QProgressBar::chunk { background: #ff6b6b; }
QProgressBar { background: #15161b; border: none; border-radius: 3px; }
QProgressBar::chunk { background: #f2c14e; border-radius: 3px; }
QPlainTextEdit#code { font-family: monospace; font-size: 11px; }
QLabel#hudLevel { background: transparent; color: #e6e7ec; font-weight: 600; }
QLabel#hudHint { background: transparent; color: #9aa0ae; font-size: 12px; }
QLabel#chip { background: rgba(37,40,50,200); border: 1px solid #323642; border-radius: 9px; padding: 1px 8px; font-size: 12px; }
QLabel#chip[edited="true"] { color: #f2c14e; border-color: #8a6f2c; }
QLabel#loading { background: rgba(21,22,27,220); border: 1px solid #323642; border-radius: 8px; padding: 10px 16px; }
QTabBar::tab { background: #252832; padding: 4px 10px; border: 1px solid #323642; }
QTabBar::tab:selected { color: #f2c14e; }
QCheckBox::indicator { width: 12px; height: 12px; border: 1px solid #4a5060; border-radius: 3px; background: #15161b; }
QCheckBox::indicator:checked { background: #f2c14e; border-color: #f2c14e; }
"""


class MainWindow(QMainWindow):
    def __init__(self, backend, home):
        super().__init__()
        self.home = home
        self.settings = QSettings(os.path.join(home, "settings.ini"), QSettings.IniFormat)
        from .drafts import Drafts
        self.drafts = Drafts(os.path.join(home, "drafts"))
        self.ed = ed = Editor(backend, self.drafts, self.settings)
        self.jobs = Jobs()
        self.setWindowTitle("Glovebox")
        self.resize(1500, 900)
        self.setAcceptDrops(True)
        self.view = Viewport(ed)
        self.view.include_land = lambda: self.outliner.box_land.isChecked()
        self.setCentralWidget(self.view)
        self.busy = False

        # Docks: the outliner on the left, the inspector and the checks on the right, the palette and the path tool
        self.outliner = Outliner(ed)
        self.inspector = Inspector(ed)
        self.inspector.reskin.connect(self.reskin)
        self.checks = Checks(ed)
        # The kit's previews get their own worker: a tab of them must not hold up builds and the import plan
        self.palette = Palette(ed, self.view, Jobs(1), self)
        self.palette.import_model.connect(self.open_model_dialog)
        self.path_panel = PathPanel(ed)
        self.dock_left = self.dock("Level", self.outliner, Qt.LeftDockWidgetArea)
        right = QSplitter(Qt.Vertical)
        right.addWidget(self.inspector)
        right.addWidget(self.checks)
        right.setSizes([560, 340])
        self.dock_right = self.dock("Inspector", right, Qt.RightDockWidgetArea)
        self.dock_palette = self.dock("Add to level", self.palette, Qt.RightDockWidgetArea)
        path_scroll = FitScroll()
        path_scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarAlwaysOff)
        path_scroll.setWidget(self.path_panel)
        self.dock_path = self.dock("Path", path_scroll, Qt.RightDockWidgetArea)
        self.dock_palette.hide()
        self.dock_path.hide()
        self.dock_palette.visibilityChanged.connect(self.on_palette_visible)
        self.dock_path.visibilityChanged.connect(lambda v: (not v and ed.path["on"]) and ed.set_path_mode(False))
        self.resizeDocks([self.dock_left, self.dock_right], [270, 380], Qt.Horizontal)

        self.status_text = QLabel("Starting…")
        self.status_text.setWordWrap(True)
        self.status_size = QLabel("")
        self.statusBar().addWidget(self.status_text, 1)
        self.statusBar().addPermanentWidget(self.status_size)
        self.make_toolbar()

        ed.status.connect(self.set_status)
        ed.edited.connect(self.update_draft_ui)
        ed.selection_changed.connect(self.update_buttons)
        ed.mode_changed.connect(self.update_buttons)
        ed.scene_loaded.connect(self.on_scene_loaded)
        self.view.fly_changed.connect(lambda on: self.act_fly.setChecked(on))
        # Autosave after a short pause
        self.draft_timer = QTimer(self)
        self.draft_timer.setSingleShot(True)
        self.draft_timer.setInterval(400)
        self.draft_timer.timeout.connect(ed.flush_draft)
        ed.draft_timer_hook = self.draft_timer.start
        # The import plan runs in the background, a moment after the last edit
        self.plan_timer = QTimer(self)
        self.plan_timer.setSingleShot(True)
        self.plan_timer.setInterval(250)
        self.plan_timer.timeout.connect(self.run_plan)
        ed.plan_hook = lambda ids: (setattr(self, "_plan_ids", ids), self.plan_timer.start())
        QApplication.instance().installEventFilter(self)

        ed.show_fog = self.settings.value("fog", "on") != "off"
        self.act_fog.setChecked(ed.show_fog)
        if self.settings.value("triggers", "on") == "off":
            ed.hidden.add("trigger")
        self.act_triggers.setChecked("trigger" not in ed.hidden)

    def dock(self, title, widget, area):
        d = QDockWidget(title, self)
        d.setObjectName(title)
        d.setWidget(widget)
        d.setFeatures(QDockWidget.DockWidgetMovable | QDockWidget.DockWidgetClosable | QDockWidget.DockWidgetFloatable)
        self.addDockWidget(area, d)
        return d

    # -----------------------------------------------------------------------------------------------------------
    # Toolbar

    def make_toolbar(self):
        ed = self.ed
        tb = QToolBar("Edit")
        tb.setObjectName("edit")
        tb.setMovable(False)
        self.addToolBar(tb)
        brand = QLabel("<b style='color:#f2c14e'>Glove</b>box  ")
        tb.addWidget(brand)
        self.scene_box = QComboBox()
        self.scene_box.setMinimumWidth(240)
        self.scene_box.setToolTip("Level (from noclip.website's scene table)")
        self.scene_box.activated.connect(lambda i: self.load_scene(self.scene_box.itemData(i)))
        tb.addWidget(self.scene_box)
        tb.addSeparator()

        def act(text, tip, fn, key=None, checkable=False, obj=None):
            a = QAction(text, self)
            a.setToolTip(tip)
            a.setCheckable(checkable)
            a.triggered.connect(fn)
            tb.addAction(a)
            if obj:
                tb.widgetForAction(a).setObjectName(obj)
            return a
        self.act_reset = act("Reset level", "Discard all edits to this level and its autosave (undoable)", self.reset_level)
        self.act_undo = act("Undo", "Undo (Ctrl+Z)", ed.undo)
        self.act_redo = act("Redo", "Redo (Ctrl+Shift+Z)", ed.redo)
        tb.addSeparator()
        self.act_add = act("+ Add", "Add objects, enemies, collectables and models from any level (A)", lambda: self.toggle_palette(), checkable=True, obj="accent")
        group = QActionGroup(self)
        self.mode_acts = {}
        for m, text, key in (("translate", "Move", "W"), ("rotate", "Rotate", "E"), ("scale", "Scale", "R")):
            a = act(text, "%s (%s)" % (text, key), lambda _=False, m=m: ed.set_mode(m), checkable=True)
            group.addAction(a)
            self.mode_acts[m] = a
        self.mode_acts["translate"].setChecked(True)
        tb.addWidget(QLabel(" Pivot"))
        self.pivot = QComboBox()
        self.pivot.addItem("group centre", "group")
        self.pivot.addItem("each origin", "individual")
        self.pivot.setToolTip("Where a multi-object rotation or scale is centred")
        self.pivot.currentIndexChanged.connect(lambda _: (setattr(ed, "pivot_mode", self.pivot.currentData()), self.inspector.render()))
        tb.addWidget(self.pivot)
        tb.addWidget(QLabel(" Snap"))
        self.snap = QComboBox()
        for v in ("off", "1", "10", "50"):
            self.snap.addItem(v, 0.0 if v == "off" else float(v))
        self.snap.setCurrentIndex(1)
        self.snap.setToolTip("Snap: grid for moves, 15° for rotation, 0.05 for scale")
        self.snap.currentIndexChanged.connect(lambda _: (setattr(ed, "snap", self.snap.currentData()), ed.update_pivot()))
        tb.addWidget(self.snap)
        self.act_dup = act("Duplicate", "Duplicate selection (Ctrl+D)", ed.duplicate)
        self.act_del = act("Delete", "Delete selection (Del)", ed.remove)
        self.act_focus = act("Focus", "Frame selection (F)", lambda: ed.selected and ed.focus_on(ed.selected))

        self.addToolBarBreak()
        tb2 = QToolBar("View and files")
        tb2.setObjectName("files")
        tb2.setMovable(False)
        self.addToolBar(tb2)
        tb = tb2
        self.act_box = act("Box select", "Box select (B). Ctrl+click toggles, Ctrl+A selects all", lambda: ed.set_box_mode(not ed.box_mode), checkable=True)
        self.act_fly = act("Fly", "Fly camera (C): WASD moves, Space and Shift go up and down, drag to look, the wheel sets the speed",
                           lambda: self.view.set_fly(not self.view.fly["on"]), checkable=True)
        self.act_path = act("Path", "Path (P): click the ground to lay a path, then place garibs or copies of the selection along it",
                            lambda: self.set_path_mode(not ed.path["on"]), checkable=True)
        self.act_motion = act("Motion", "Animate spinning platforms", lambda on: self.view.set_preview(on), checkable=True)
        self.act_fog = act("Fog", "Show the level's fog in the editor view. Turning it off only affects the editor; to change the fog in the game, edit the level's FogConfiguration (fog_enabled).",
                           self.toggle_fog, checkable=True)
        self.act_triggers = act("Triggers", "Show the puzzle trigger volumes (the areas a puzzle condition tests). Hiding them only affects the editor view; the level keeps them.",
                                lambda on: ed.set_layer("trigger", on), checkable=True)
        tb.addSeparator()
        act("Import XML", "Import a libgarib level XML (checked here first; dropping a file on the window also works)", self.open_import_dialog)
        act("Export XML", "Save the level as libgarib XML", lambda: self.export("xml"))
        act(".lev", "Assemble with libgarib level-tool and save the .lev", lambda: self.export("lev"))
        self.quickboot = QCheckBox("boot here")
        self.quickboot.setChecked(True)
        self.quickboot.setToolTip("Build ROM: the ROM boots straight into this level (libgarib's copyskip and quickboot patches)")
        tb.addWidget(self.quickboot)
        act("Export patch", "Export a BPS patch: share it, and players apply it to their own ROM with the patcher", self.open_patch_dialog)
        act("Test level", "Build the level into a ROM and open it in your emulator (T)", self.test_level)
        act("Build ROM", "Patch the edited level (and any imported models) into the ROM with libgarib", lambda: self.export("rom"), obj="primary")
        act("Build all…", "Build every edited level into one ROM or patch", self.open_all_dialog)
        act("Patcher ↗", "Open the patcher page for players (works offline; the ROM never leaves the page)", self.open_patcher)

    def update_buttons(self):
        ed = self.ed
        self.act_undo.setEnabled(bool(ed.undo_stack))
        self.act_redo.setEnabled(bool(ed.redo_stack))
        self.mode_acts[ed.mode].setChecked(True)
        self.act_box.setChecked(ed.box_mode)
        self.act_path.setChecked(ed.path["on"])
        self.view.setCursor(Qt.CrossCursor if ed.box_mode or ed.path["on"] else Qt.ArrowCursor)

    def toggle_fog(self, on):
        self.ed.show_fog = on
        self.settings.setValue("fog", "on" if on else "off")
        self.view.update()

    def set_status(self, msg, cls=""):
        self.status_text.setText(msg)
        self.status_text.setProperty("cls", cls)
        self.status_text.style().unpolish(self.status_text)
        self.status_text.style().polish(self.status_text)

    def loading(self, text):
        self.view.set_loading(text)
        QApplication.processEvents()

    # -----------------------------------------------------------------------------------------------------------
    # Start and levels

    def start(self):
        ed = self.ed
        self.show()
        self.loading("Loading level list…")
        ed.start()
        model = QStandardItemModel()
        group = None
        for s in ed.scenes:
            if s["group"] != group:
                group = s["group"]
                h = QStandardItem(group)
                h.setEnabled(False)
                h.setData(None, Qt.UserRole)
                model.appendRow(h)
            it = QStandardItem("   %s  (%s)" % (s["name"], s["id"]))
            it.setData(s["id"], Qt.UserRole)
            model.appendRow(it)
        self.scene_box.setModel(model)
        self.loading("Indexing every model in the game… The first start takes a few minutes.")

        def done(cat):
            ed.catalog = cat
            self.after_catalog()

        def failed(e):
            ed.catalog = {}
            self.set_status("Couldn’t index the game’s models: %s" % e, "err")
            self.after_catalog()
        self.jobs.run(ed.backend.catalog, done, failed)

    def after_catalog(self):
        ed = self.ed
        start = self.settings.value("scene", "") or ""
        if not any(s["id"] == start for s in ed.scenes):
            start = "0a" if any(s["id"] == "0a" for s in ed.scenes) else ed.scenes[0]["id"]
        self.load_scene(start)

    def load_scene(self, sid):
        if not sid or self.busy:
            return
        ed = self.ed
        self.busy = True
        name = next(s["name"] for s in ed.scenes if s["id"] == sid)
        self.loading("Loading %s…" % name)
        # The back end's first look at a level unpacks its banks: that part runs in the background
        backend = ed.backend

        def fetch():
            return backend.level_xml(sid), backend.models(sid), backend.orig_lev_size(sid)

        def done(res):
            xml, models, size = res
            orig = (backend.level_xml, backend.models, backend.orig_lev_size)
            backend.level_xml, backend.models, backend.orig_lev_size = (lambda _: xml), (lambda _: models), (lambda _: size)
            try:
                self.loading("Loading %d models…" % len(models["models"]))
                ed.load_scene(sid)
            except Exception as e:
                import traceback
                traceback.print_exc()
                self.set_status("Failed to load level: %s" % e, "err")
            finally:
                backend.level_xml, backend.models, backend.orig_lev_size = orig
                self.busy = False
                self.loading("")

        def failed(e):
            self.busy = False
            self.loading("")
            self.set_status("Failed to load level: %s" % e, "err")
        self.jobs.run(fetch, done, failed)

    def on_scene_loaded(self):
        ed = self.ed
        i = self.scene_box.findData(ed.scene["id"], Qt.UserRole)
        if i >= 0:
            self.scene_box.setCurrentIndex(i)
        self.view.hud_level.setText("%s — %s" % (ed.scene["name"], ed.root.get("name") or ""))
        self.status_size.setText("%d B .lev (original)" % ed.orig_lev_size if ed.orig_lev_size else "")
        self.update_buttons()
        if self.dock_palette.isVisible():
            self.palette.render()
        self.view.layout_hud()
        self.view.update()

    def update_draft_ui(self):
        ed = self.ed
        if ed.scene is None:
            return
        edited = ed.is_edited()
        chip = self.view.hud_chip
        chip.setText(("Edited · autosaved %s" % time.strftime("%H:%M:%S", time.localtime(ed.draft_saved_at)) if ed.draft_saved_at
                      else "Edited · saving…") if edited else "Original level")
        chip.setToolTip("Your edits are saved in the user folder (%s) and come back when you reopen this level." % self.drafts.folder
                        if edited else "No edits to this level.")
        chip.setProperty("edited", "true" if edited else "false")
        chip.style().unpolish(chip)
        chip.style().polish(chip)
        chip.adjustSize()
        self.act_reset.setEnabled(edited)
        drafts = self.drafts.list()
        model = self.scene_box.model()
        for row in range(model.rowCount()):
            it = model.item(row)
            sid = it.data(Qt.UserRole)
            if not sid:
                continue
            s = next(x for x in ed.scenes if x["id"] == sid)
            it.setText("   %s  (%s)%s" % (s["name"], sid, "  ● edited" if sid in drafts else ""))
        self.update_buttons()

    def reset_level(self):
        ed = self.ed
        if not ed.is_edited():
            return self.set_status("%s is already the shipped level." % ed.scene["name"])
        if QMessageBox.question(self, "Reset level", "Reset %s to the shipped level? This discards your edits and their autosave. You can still undo it in this session." % ed.scene["name"]) == QMessageBox.Yes:
            ed.reset_level()

    def run_plan(self):
        ed = self.ed
        ids = self._plan_ids
        sid = ed.scene["id"]

        def done(plan):
            if ed.scene["id"] == sid and ids == ed.current_imports():
                ed.plan = plan
                ed.imports_changed.emit()
        self.jobs.run(lambda: ed.backend.plan(sid, ids), done, lambda e: (setattr(ed, "plan", None), ed.imports_changed.emit()))

    # -----------------------------------------------------------------------------------------------------------
    # Palette and path

    def toggle_palette(self, on=None):
        on = not self.dock_palette.isVisible() if on is None else on
        self.dock_palette.setVisible(on)
        if on:
            self.palette.render()
            self.dock_palette.raise_()

    def on_palette_visible(self, v):
        self.act_add.setChecked(v)

    def set_path_mode(self, on):
        self.ed.set_path_mode(on)
        self.dock_path.setVisible(on)
        if on:
            self.dock_path.raise_()
        self.path_panel.update_info()

    def place_kit(self, it, vals, as_):
        ed = self.ed
        self.loading("Making %s…" % it["name"].lower())

        def done(res):
            ed.catalog = ed.backend.catalog()
            self.loading("")
            ed.place_custom(res["set"], as_)

        def failed(e):
            self.loading("")
            self.set_status("Could not make %s: %s" % (it["name"].lower(), e), "err")
        self.jobs.run(lambda: ed.backend.make_kit(it["id"], vals), done, failed)

    def reskin(self, node, worlds, seed):
        """Gives the selected scenery a copy of its model with other worlds' textures (a background job)"""
        ed = self.ed
        from .xmlutil import hex_id
        oid = hex_id(node.get("object_id"))
        info = ed.catalog.get(oid) or {}
        src = info.get("reskinFrom") or oid
        self.loading("Making a new look for %s…" % (ed.catalog.get(src) or {}).get("name", src))
        from .xmlutil import node_at, path_of
        where, sid, had = path_of(node), ed.scene["id"], node.get("object_id")

        def done(new):
            ed.catalog = ed.backend.catalog()
            self.loading("")
            # The level may have changed while the job ran (undo, another level): find the piece again
            node = node_at(ed.root, where) if ed.scene and ed.scene["id"] == sid else None
            if node is None or node.get("object_id") != had:
                return ed.set_status("The new look is ready, but the piece changed meanwhile. It is listed with your imported models.", "")
            name = (ed.catalog.get(new) or {}).get("piece") or node.get("name")

            def fn():
                node.set("object_id", new)
                node.set("name", name[:7])
            ed.mutate(fn)
            ed.set_status("The model now has textures from %s. It is copied into the level's banks at ROM build." % ", ".join(
                w.title() for w in worlds), "ok")

        def failed(e):
            self.loading("")
            self.set_status("Could not make a new look: %s" % e, "err")
        self.jobs.run(lambda: ed.backend.reskin(src, worlds, seed), done, failed)

    def remove_custom(self, set_):
        ed = self.ed
        pieces = ed.custom_pieces(set_)
        name = pieces[0][1]["bank"].replace("Custom: ", "") if pieces else set_
        if QMessageBox.question(self, "Remove model", "Remove the imported model %s? Levels that use it then show a placeholder, and can't be built until you remove those pieces." % name) != QMessageBox.Yes:
            return
        ed.backend.delete_custom(set_)
        ed.catalog = ed.backend.catalog()
        self.palette.render()
        self.set_status("Removed %s." % name, "ok")

    # -----------------------------------------------------------------------------------------------------------
    # Dialogs

    def dialog(self, title):
        d = QDialog(self)
        d.setWindowTitle(title)
        d.setMinimumWidth(520)
        lay = QVBoxLayout(d)
        return d, lay

    def open_import_dialog(self, path=None):
        """Import XML: the file is read and checked here (desktop/sanitize.py); nothing else sees it first"""
        ed = self.ed
        d, lay = self.dialog("Import level XML")
        lay.addWidget(label("The editor reads and checks the file. It must be libgarib level XML. Anything unknown is refused.", "muted"))
        row = QHBoxLayout()
        pick = QPushButton("Choose file…")
        name = label("No file chosen", "muted")
        row.addWidget(pick)
        row.addWidget(name, 1)
        lay.addLayout(row)
        lay.addWidget(label("or drop an .xml file on the window", "muted"))
        report = QVBoxLayout()
        lay.addLayout(report)
        trow = QHBoxLayout()
        trow.addWidget(QLabel("Import into"))
        target = QComboBox()
        trow.addWidget(target, 1)
        lay.addLayout(trow)
        btns = QHBoxLayout()
        btns.addStretch(1)
        cancel, go = QPushButton("Cancel"), QPushButton("Import")
        go.setObjectName("primary")
        go.setEnabled(False)
        btns.addWidget(cancel)
        btns.addWidget(go)
        lay.addLayout(btns)
        state = {"res": None}

        def show(res):
            clear(report)
            state["res"] = res
            if not res.ok:
                report.addWidget(check_line("err", "Not imported: %d problem%s found." % (len(res.errors), "" if len(res.errors) == 1 else "s")))
                for e in res.errors[:12]:
                    report.addWidget(label(e, "src"))
                if len(res.errors) > 12:
                    report.addWidget(label("…and %d more." % (len(res.errors) - 12), "muted"))
                go.setEnabled(False)
                return
            matches = match_scenes(res.info["name"], ed.scenes)
            report.addWidget(check_line("ok", "Valid level XML: %s%d commands. Every element, field and value checked against libgarib’s level format." % (
                "“%s”, " % res.info["name"] if res.info["name"] else "", res.info["elements"])))
            for w in res.warnings:
                report.addWidget(check_line("note", w))
            rules = ed.run_rules(res.root)
            if rules:
                report.addWidget(check_line("note", "%d level-parser warning%s (shown in Checks after import), e.g. %s" % (len(rules), "" if len(rules) == 1 else "s", rules[0]["msg"])))
            target.clear()
            ids = list(dict.fromkeys([m["id"] for m in matches] + [ed.scene["id"]]))
            for i in ids:
                sc = next(x for x in ed.scenes if x["id"] == i)
                target.addItem("%s (%s)" % (sc["name"], i) + (" — matches this file" if any(m["id"] == i for m in matches) else " — current level" if i == ed.scene["id"] else ""), i)
            target.setCurrentIndex(ids.index(matches[0]["id"]) if len(matches) == 1 else ids.index(ed.scene["id"]))
            go.setEnabled(True)

        def read(path):
            name.setText("%s · %.1f KB" % (os.path.basename(path), os.path.getsize(path) / 1024))
            if os.path.getsize(path) > LIMITS["bytes"]:
                clear(report)
                report.addWidget(check_line("err", "This file is %.1f MB; level XML is at most %d MB. It was not read." % (os.path.getsize(path) / 1048576, LIMITS["bytes"] >> 20)))
                return
            try:
                text = open(path, encoding="utf-8").read()
            except (OSError, UnicodeDecodeError):
                clear(report)
                report.addWidget(check_line("err", "The file couldn’t be read."))
                return
            show(sanitize_level_xml(text, ed.decomp, "file"))

        def choose():
            p, _ = QFileDialog.getOpenFileName(d, "Import level XML", "", "Level XML (*.xml);;All files (*)")
            if p:
                read(p)

        def confirm():
            res = state["res"]
            if not res or not res.ok:
                return
            tid = target.currentData()
            d.accept()
            if tid != ed.scene["id"]:
                self.load_scene_then(tid, lambda: ed.confirm_import(res))
            else:
                ed.confirm_import(res)
        pick.clicked.connect(choose)
        cancel.clicked.connect(d.reject)
        go.clicked.connect(confirm)
        if path:
            read(path)
        d.exec()

    def load_scene_then(self, sid, fn):
        def wait():
            if self.busy:
                return QTimer.singleShot(100, wait)
            if self.ed.scene["id"] == sid:
                fn()
        self.load_scene(sid)
        QTimer.singleShot(100, wait)

    def open_patch_dialog(self):
        """Patches start the game normally unless the author opts in to booting straight into the level. The
        "boot here" toolbar option only applies to Build ROM."""
        ed = self.ed
        d, lay = self.dialog("Export patch")
        lay.addWidget(label("A BPS patch for other players. They apply it to their own Glover (USA) ROM with patcher.html. "
                            "Share the .bps and patcher.html (one file, works offline).", "muted"))
        imports = ed.current_imports()
        edited = ed.is_edited()
        lay.addWidget(check_line("ok" if edited else "note", "%s, with your edits." % ed.scene["name"] if edited else
                                 "%s has no edits; the patch would only change how the ROM is laid out." % ed.scene["name"]))
        game = [i for i in imports if not (ed.catalog.get(i) or {}).get("custom")]
        custom = len(imports) - len(game)
        if custom:
            lay.addWidget(check_line("note", "Uses %d piece(s) of your imported models. The patch contains them, in this level’s rebuilt model bank (%s) and texture bank (%s)." % (
                custom, ed.scene["importObjectBank"], ed.scene["importTextureBank"])))
        if game and ed.runtime_imports():
            lay.addWidget(check_line("ok", "Uses %d model(s) from other levels. The game mod loads them from the player’s own ROM. The patch contains no game data. Players need the Expansion Pak." % len(game)))
        elif game:
            lay.addWidget(check_line("note", "Uses %d model(s) from other levels. The patch then contains this level’s rebuilt model bank (%s) and texture bank (%s) in full, game data included. It is much larger than a plain edit." % (
                len(game), ed.scene["importObjectBank"], ed.scene["importTextureBank"])))
        elif not custom:
            lay.addWidget(check_line("ok", "No imported models: the patch is a true diff and contains only what your edits add."))
        if ed.mod_users():
            lay.addWidget(check_line("note", "Rotates or scales enemies, platforms, powerups or water. The patch adds the Expansion Pak game mod (about 5 KB). Players need the Expansion Pak; without one the game skips those rotations and scales."))
        if ed.needs_pak_heap():
            lay.addWidget(check_line("note", "The imported models need more memory than the game’s own heap. The patch adds the Expansion Pak game mod, which makes the heap about 4 MB larger. Players need the Expansion Pak; without one the game runs out of memory in this level."))
        qb = QCheckBox("Patched ROM boots straight into this level (skips the title screens, for testing)")
        lay.addWidget(qb)
        btns = QHBoxLayout()
        btns.addStretch(1)
        cancel, go = QPushButton("Cancel"), QPushButton("Export .bps")
        go.setObjectName("primary")
        btns.addWidget(cancel)
        btns.addWidget(go)
        lay.addLayout(btns)
        cancel.clicked.connect(d.reject)
        go.clicked.connect(d.accept)
        if d.exec() == QDialog.Accepted:
            self.export("bps", qb.isChecked())

    def open_all_dialog(self):
        """Build all: every level with edits (its draft, or a title, music or clock) in one ROM or BPS patch"""
        ed = self.ed
        levels = ed.project_levels()
        d, lay = self.dialog("Build all edited levels")
        if not levels:
            lay.addWidget(label("No level has edits yet.", "muted"))
            b = QPushButton("Close")
            b.clicked.connect(d.reject)
            lay.addWidget(b)
            return d.exec()
        lay.addWidget(label("Every checked level goes into one ROM or patch, with its title, music and clock. Two levels of one world "
                            "can't both copy models into their banks; the game mod then loads them from the ROM.", "muted"))
        boxes = []
        for sid, name, xml in levels:
            info = ed.drafts.info(sid)
            extra = ", ".join(x for x in ("edited" if xml is not None else "",
                                          "title %s" % info["name"] if info["name"] else "",
                                          "music %d" % info["song"] if info["song"] is not None else "",
                                          "clock" if info["bonus_time"] is not None else "") if x)
            b = QCheckBox("%s (%s)  %s" % (name, sid, extra))
            b.setChecked(True)
            boxes.append((sid, b))
            lay.addWidget(b)
        row = QHBoxLayout()
        row.addWidget(QLabel("Boot into"))
        boot = QComboBox()
        boot.addItem("the title screen (normal start)", None)
        for sid, name, _ in levels:
            boot.addItem(name, sid)
        row.addWidget(boot, 1)
        lay.addLayout(row)
        btns = QHBoxLayout()
        btns.addStretch(1)
        cancel, rom, bps = QPushButton("Cancel"), QPushButton("Build ROM"), QPushButton("Export .bps")
        rom.setObjectName("primary")
        for b in (cancel, bps, rom):
            btns.addWidget(b)
        lay.addLayout(btns)
        choice = []
        cancel.clicked.connect(d.reject)
        rom.clicked.connect(lambda: (choice.append("rom"), d.accept()))
        bps.clicked.connect(lambda: (choice.append("bps"), d.accept()))
        if d.exec() != QDialog.Accepted or not choice:
            return
        sids = [sid for sid, b in boxes if b.isChecked()]
        qb = boot.currentData()
        self.loading("Building %d levels…" % len(sids))

        def done(res):
            self.loading("")
            self.save(*res)

        def failed(e):
            self.loading("")
            self.set_status(str(e), "err")
        try:
            job = ed.build_all_job(sids, choice[0], qb)   # reads the levels now; the worker only builds
        except Exception as e:
            return failed(e)
        self.jobs.run(job, done, failed)

    def open_model_dialog(self):
        """Import model: one .glb, converted into Glover models by the back end (tools/custom_model.py)"""
        ed = self.ed
        d, lay = self.dialog("Import model")
        lay.addWidget(label("One .glb file (binary glTF) with its textures inside. Blender and most model rippers export these. "
                            "The editor turns it into Glover models. Textures shrink to N64 textures of at most 2,048 pixels. "
                            "A big model, like a map, is cut into solid pieces of at most 1,500 triangles.", "muted"))
        row = QHBoxLayout()
        pick = QPushButton("Choose .glb…")
        fname = label("No file chosen", "muted")
        row.addWidget(pick)
        row.addWidget(fname, 1)
        lay.addLayout(row)
        name = QLineEdit()
        name.setMaxLength(40)
        name.setPlaceholderText("Model")
        scale = QLineEdit("1")
        for l, w in (("Name", name), ("Scale", scale)):
            r = QHBoxLayout()
            r.addWidget(QLabel(l))
            r.addWidget(w, 1)
            lay.addLayout(r)
        info = label("", "muted")
        lay.addWidget(info)
        btns = QHBoxLayout()
        btns.addStretch(1)
        cancel, go = QPushButton("Cancel"), QPushButton("Import and place")
        go.setObjectName("primary")
        btns.addWidget(cancel)
        btns.addWidget(go)
        lay.addLayout(btns)
        state = {"path": None, "stats": None}

        def level_size():
            from .vec import Box
            box = Box()
            for e in ed.entities:
                if e.kind == "land":
                    box.add_box(e.obj.box())
            return None if box.empty() else box.size()

        def show():
            try:
                sc = float(scale.text())
            except ValueError:
                sc = 0
            lv = level_size()
            level = " This level is about %d × %d units across." % (round(lv[0]), round(lv[2])) if lv is not None else ""
            st = state["stats"]
            go.setEnabled(bool(state["path"]) and sc > 0 and st is not None and "error" not in st)
            if not state["path"]:
                text = "Choose a .glb file." + level
            elif st is None:
                text = "Reading the model…"
            elif "error" in st:
                text = "This file can't be read as a glTF model: %s" % st["error"]
            else:
                import math
                size = st["box"].size() * sc
                pieces = 1 if st["triangles"] <= 1500 else 2 ** math.ceil(math.log2(st["triangles"] / 1500))
                text = ("%s triangles, in %d piece%s. At this scale it is %d × %d × %d units (width × height × depth)." % (
                    format(st["triangles"], ","), pieces, "" if pieces == 1 else "s", round(size[0]), round(size[1]), round(size[2])) + level)
            info.setText(text)

        def choose():
            p, _ = QFileDialog.getOpenFileName(d, "Import model", "", "glTF models (*.glb *.gltf);;All files (*)")
            if not p:
                return
            state["path"], state["stats"] = p, None
            fname.setText(os.path.basename(p))
            if not name.text():
                import re
                name.setText(re.sub(r"\.(glb|gltf)$", "", os.path.basename(p), flags=re.I)[:40])
            show()
            from .gltf import stats
            try:
                tris, box = stats(open(p, "rb").read())
                state["stats"] = {"triangles": tris, "box": box}
            except Exception as e:
                state["stats"] = {"error": str(e)}
            show()
        pick.clicked.connect(choose)
        scale.textChanged.connect(lambda _: show())
        cancel.clicked.connect(d.reject)
        go.clicked.connect(d.accept)
        show()
        if d.exec() != QDialog.Accepted:
            return
        nm, sc, path = name.text().strip() or "Model", float(scale.text()), state["path"]
        self.loading("Converting %s into Glover models…" % nm)

        def done(res):
            ed.catalog = ed.backend.catalog()
            self.loading("")
            ed.place_custom(res["set"])
            if self.dock_palette.isVisible():
                self.palette.render()

        def failed(e):
            self.loading("")
            self.set_status("Could not import %s: %s" % (nm, e), "err")
        self.jobs.run(lambda: ed.backend.import_custom(open(path, "rb").read(), nm, sc), done, failed)

    # -----------------------------------------------------------------------------------------------------------
    # Build, export and test

    def export(self, fmt, quickboot=None):
        ed = self.ed
        qb = self.quickboot.isChecked() if quickboot is None else quickboot
        imports = ed.current_imports()
        if fmt in ("rom", "bps"):
            self.set_status(("Importing %d model(s) and patching the ROM with libgarib" % len(imports) if imports else
                             "Assembling the level and patching the ROM with libgarib") + (", then diffing it against the original…" if fmt == "bps" else "…"))
        if fmt == "xml":
            data, name, msg = ed.build("xml")
            return self.save(data, name, msg or "Saved %s." % name)
        self.loading("Building…")

        def done(res):
            self.loading("")
            data, name, msg = res
            self.save(data, name, msg)
            if fmt in ("rom", "lev"):
                import re
                m = re.search(r"level is (\d+) bytes", msg)
                if m:
                    self.status_size.setText("%s B .lev" % m.group(1))

        def failed(e):
            self.loading("")
            self.set_status(str(e), "err")
        try:
            job = ed.build_job(fmt, qb)   # reads the level now; the worker only builds
        except ValueError as e:
            return failed(e)
        self.jobs.run(job, done, failed)

    def save(self, data, name, msg):
        folder = self.settings.value("saveDir", os.path.expanduser("~"))
        path, _ = QFileDialog.getSaveFileName(self, "Save %s" % name, os.path.join(folder, name))
        if not path:
            self.set_status("Not saved.")
            return False
        try:
            with open(path, "wb") as f:
                f.write(data)
        except OSError as e:
            self.set_status("Couldn’t save %s: %s" % (path, e), "err")
            return False
        self.settings.setValue("saveDir", os.path.dirname(path))
        self.set_status(msg.replace("Built %s" % name, "Built %s" % path).replace("Exported %s" % name, "Exported %s" % path) if msg else "Saved %s." % path, "ok")
        return True

    def test_level(self):
        """Builds the level into a ROM in the user folder and opens it in the emulator chosen in the settings"""
        ed = self.ed
        imports = ed.current_imports()
        emulator = self.settings.value("emulator", "")
        if not emulator or not os.path.exists(emulator):
            QMessageBox.information(self, "Test level", "Test level opens the built ROM in your emulator. Choose the emulator's program (for example ares, simple64 or Project64) in the next window.\n\n"
                                    "Expansion Pak game mod levels need the emulator's Expansion Pak turned on (most have it on).")
            if sys.platform == "darwin":
                emulator, _ = QFileDialog.getOpenFileName(self, "Choose your emulator", "/Applications", "Applications (*.app);;All files (*)")
            else:
                emulator, _ = QFileDialog.getOpenFileName(self, "Choose your emulator", "", "Programs (*.exe);;All files (*)" if sys.platform == "win32" else "All files (*)")
            if not emulator:
                return self.set_status("Test level needs an emulator.", "err")
            self.settings.setValue("emulator", emulator)
        self.set_status("Importing %d model(s) and building the level to test…" % len(imports) if imports else "Building the level to test…")
        self.loading("Building the level to test…")
        sid, name = ed.scene["id"], ed.scene["name"]
        try:
            # The same build as Build ROM with boot here: title, music, clock and the extra-level setting included
            job = ed.build_job("rom", True)
        except ValueError as e:
            self.loading("")
            return self.set_status(str(e), "err")

        def build():
            data, _, _ = job()
            folder = os.path.join(self.home, "test")
            os.makedirs(folder, exist_ok=True)
            path = os.path.join(folder, "glover-%s-test.z64" % sid)
            with open(path, "wb") as f:
                f.write(data)
            return path

        def done(path):
            self.loading("")
            if launch(emulator, path):
                self.set_status("Testing %s in %s. The ROM boots straight into the level: %s" % (name, os.path.basename(emulator), path), "ok")
            else:
                self.settings.remove("emulator")   # the next Test level asks again
                self.set_status("Built %s, but %s didn't start. Test level asks for the emulator again next time." % (path, emulator), "err")

        def failed(e):
            self.loading("")
            self.set_status(str(e), "err")
        self.jobs.run(build, done, failed)

    def open_patcher(self):
        QDesktopServices.openUrl(QUrl.fromLocalFile(os.path.join(HERE, "web", "patcher.html")))

    # -----------------------------------------------------------------------------------------------------------
    # Keys and files dropped on the window

    def eventFilter(self, obj, e):
        from PySide6.QtCore import QEvent
        t = e.type()
        if t not in (QEvent.KeyPress, QEvent.KeyRelease, QEvent.ShortcutOverride):
            return False
        if not isinstance(obj, QWidget) or obj.window() is not self or QApplication.activeModalWidget() is not None:
            return False
        w = QApplication.focusWidget()
        from PySide6.QtWidgets import QAbstractSpinBox, QPlainTextEdit
        if isinstance(w, (QLineEdit, QPlainTextEdit, QAbstractSpinBox)):
            return False
        if t == QEvent.ShortcutOverride:
            return False
        if t == QEvent.KeyRelease:
            if not e.isAutoRepeat():
                self.view.fly_key(e.key(), False)
                if e.key() in (Qt.Key_Control, Qt.Key_Meta):
                    self.ed.gizmo.enabled = True
            return False
        return self.key(e)

    def key(self, e):
        ed = self.ed
        k = e.key()
        mods = e.modifiers()
        mod = bool(mods & (Qt.ControlModifier | Qt.MetaModifier))
        if k in (Qt.Key_Control, Qt.Key_Meta):
            ed.gizmo.enabled = False   # holding Ctrl suspends the gizmo: Ctrl+click reaches the object under it
            return False
        if self.view.fly["on"] and not mod and k in (Qt.Key_W, Qt.Key_A, Qt.Key_S, Qt.Key_D, Qt.Key_Space, Qt.Key_Shift):
            if not e.isAutoRepeat():
                self.view.fly_key(k, True)
            return True
        if mod and k == Qt.Key_Z:
            ed.redo() if mods & Qt.ShiftModifier else ed.undo()
        elif mod and k == Qt.Key_Y:
            ed.redo()
        elif mod and k == Qt.Key_D:
            ed.duplicate()
        elif mod and k == Qt.Key_A:
            ed.set_selection([x for x in ed.pickable() if not x.is_handle and x.kind != "land"])
        elif mod:
            return False
        elif ed.path["on"] and k == Qt.Key_Backspace:
            if ed.path["pts"]:
                ed.path["pts"].pop()
            ed.draw_path()
        elif k in (Qt.Key_Delete, Qt.Key_Backspace):
            ed.remove()
        elif k == Qt.Key_F:
            if ed.selected:
                ed.focus_on(ed.selected)
        elif k == Qt.Key_W:
            ed.set_mode("translate")
        elif k == Qt.Key_E:
            ed.set_mode("rotate")
        elif k == Qt.Key_R:
            ed.set_mode("scale")
        elif k == Qt.Key_B:
            ed.set_box_mode(not ed.box_mode)
        elif k == Qt.Key_C:
            self.view.set_fly(not self.view.fly["on"])
        elif k == Qt.Key_T:
            self.test_level()
        elif k == Qt.Key_A:
            self.toggle_palette()
        elif k == Qt.Key_P:
            self.set_path_mode(not ed.path["on"])
        elif k == Qt.Key_Escape:
            if self.view.fly["on"]:
                self.view.set_fly(False)
            elif ed.path["on"]:
                self.set_path_mode(False)
            elif ed.box_mode:
                ed.set_box_mode(False)
            elif self.dock_palette.isVisible():
                self.toggle_palette(False)
            else:
                ed.set_selection([])
        else:
            return False
        return True

    def dragEnterEvent(self, e):
        if e.mimeData().hasUrls():
            e.acceptProposedAction()

    def dropEvent(self, e):
        urls = [u for u in e.mimeData().urls() if u.isLocalFile()]
        if urls:
            e.acceptProposedAction()
            QTimer.singleShot(0, lambda: self.open_import_dialog(urls[0].toLocalFile()))

    def closeEvent(self, e):
        self.ed.flush_draft()
        super().closeEvent(e)


def launch(program, path):
    """Opens a file with a program: an executable, or an .app bundle on macOS (from source)"""
    try:
        if sys.platform == "darwin" and program.endswith(".app"):
            subprocess.Popen(["open", "-a", program, path])
        elif sys.platform == "win32":
            # A console emulator would otherwise get a console window of its own
            subprocess.Popen([program, path], stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                             creationflags=subprocess.CREATE_NO_WINDOW)
        else:
            subprocess.Popen([program, path], stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                             start_new_session=True)
        return True
    except OSError:
        return False
