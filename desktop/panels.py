"""The side panels: the outliner (left), the inspector and the checks (right), the Add palette and the path tool."""
import math
import numpy as np
from PySide6.QtCore import QEvent, QSize, Qt, QTimer, Signal
from PySide6.QtGui import QPixmap
from PySide6.QtWidgets import (QCheckBox, QComboBox, QFrame, QGridLayout, QHBoxLayout, QLabel, QLineEdit, QPushButton,
                               QPlainTextEdit, QProgressBar, QScrollArea, QSizePolicy, QTabBar, QToolButton, QTreeWidget,
                               QTreeWidgetItem, QVBoxLayout, QWidget, QAbstractItemView, QMenu, QSpinBox)
from . import vec
from .entities import CAM_PRESETS, KINDS, WHY, camera_zone, preset_zone, zone_camera, zone_centre
from .editor import DEG, GITHUB
from .xmlutil import fmt, hex_id, kids, num, inum, path_of

FLOAT = __import__("re").compile(r"^-?(0x[0-9a-f]+|\d+(\.\d*)?(e-?\d+)?)$", __import__("re").I)


def label(text, obj=None, wrap=True, rich=False):
    w = QLabel(text)
    if obj:
        w.setObjectName(obj)
    w.setWordWrap(wrap)
    w.setTextFormat(Qt.RichText if rich else Qt.PlainText)
    if rich:
        w.setOpenExternalLinks(True)
    return w


class ClickLabel(QLabel):
    """A wrapping line of text that acts as a button (a long one would make a QPushButton too wide)"""
    clicked = Signal()

    def __init__(self, text):
        super().__init__(text)
        self.setWordWrap(True)
        self.setTextFormat(Qt.PlainText)
        self.setCursor(Qt.PointingHandCursor)

    def mousePressEvent(self, e):
        if e.button() == Qt.LeftButton:
            self.clicked.emit()


def narrow(combo):
    """A combo box whose width doesn't follow its longest entry"""
    combo.setSizeAdjustPolicy(QComboBox.AdjustToMinimumContentsLengthWithIcon)
    combo.setMinimumContentsLength(12)
    return combo


# The game's songs (gloveworld include/enums.h, enum Track): PrepareSong plays levelInfo's song when a level loads
_WORLDS = ("Atlantis", "Carnival", "Pirates", "Prehistoric", "Fear", "Space")
SONGS = [(0, "Silence")] + [(1 + 4 * w + i, "%s %s" % (_WORLDS[w], ("1", "2", "3", "boss")[i])) for w in range(6) for i in range(4)] + [
    (25, "Title"), (26, "Intro"), (27, "Outro")] + [(28 + i, "Hub %d" % (i + 1)) for i in range(6)] + [
    (58, "Bonus"), (59, "Hasbro"), (60, "Title 2")] + [(k, "Jingle %d (short)" % k) for k in range(34, 58)]


def clear(layout):
    while layout.count():
        it = layout.takeAt(0)
        w = it.widget()
        if w is not None:
            w.setParent(None)
            w.deleteLater()
        elif it.layout() is not None:
            clear(it.layout())


class Clickable(QFrame):
    clicked = Signal()

    def mousePressEvent(self, e):
        if e.button() == Qt.LeftButton:
            self.clicked.emit()
        super().mousePressEvent(e)


def check_line(level, text, on_click=None, link=None, sub=None, title=None):
    """A line of the Checks panel (or of a dialog's report)"""
    f = Clickable()
    f.setObjectName("chk")
    f.setProperty("level", level)
    lay = QVBoxLayout(f)
    lay.setContentsMargins(8, 5, 8, 5)
    lay.setSpacing(2)
    lay.addWidget(label(text))
    if sub:
        lay.addWidget(label(sub, "src"))
    if link:
        lay.addWidget(label('game: <a style="color:#6fb7ff" href="%s">%s</a>' % link, "src", rich=True))
    if on_click:
        f.setCursor(Qt.PointingHandCursor)
        f.clicked.connect(on_click)
    if title:
        f.setToolTip(title)
    return f


def num_input(get, set_, step=1.0, refresh=None):
    """A number box that writes on Enter or when it loses focus"""
    w = QLineEdit()
    w.setObjectName("num")

    def show():
        v = get()
        t = ("%.3f" % v).rstrip("0").rstrip(".") if v == v else "nan"
        w.setText("0" if t == "-0" else t)
    show()

    def done():
        try:
            v = float(w.text())
        except ValueError:
            return show()
        if abs(v - get()) > 1e-9:
            set_(v)
    w.editingFinished.connect(done)
    if refresh is not None:
        refresh.append((w, show))
    return w


def trio(*widgets):
    box = QWidget()
    lay = QHBoxLayout(box)
    lay.setContentsMargins(0, 0, 0, 0)
    lay.setSpacing(4)
    for w in widgets:
        lay.addWidget(w)
    return box


# -------------------------------------------------------------------------------------------------------------------
# Outliner

class Outliner(QWidget):
    def __init__(self, ed):
        super().__init__()
        self.ed = ed
        lay = QVBoxLayout(self)
        lay.setContentsMargins(8, 8, 8, 8)
        self.search = QLineEdit()
        self.search.setPlaceholderText("Filter entities…")
        self.search.setClearButtonEnabled(True)
        self.search.textChanged.connect(self.on_filter)
        lay.addWidget(self.search)
        self.box_land = QCheckBox("box select includes landscape")
        self.box_land.setObjectName("muted")
        lay.addWidget(self.box_land)
        self.layers = QWidget()
        self.layers_lay = QGridLayout(self.layers)
        self.layers_lay.setContentsMargins(0, 0, 0, 0)
        self.layers_lay.setSpacing(2)
        lay.addWidget(self.layers)
        self.tree = QTreeWidget()
        self.tree.setHeaderHidden(True)
        self.tree.setColumnCount(2)
        self.tree.setSelectionMode(QAbstractItemView.NoSelection)
        self.tree.setRootIsDecorated(True)
        self.tree.itemClicked.connect(self.on_click)
        self.tree.setIndentation(12)
        lay.addWidget(self.tree, 1)
        self.items = {}
        self.open = {}
        self._kinds = None
        ed.selection_changed.connect(self.render)
        ed.rebuilt.connect(self.rebuild)

    def on_filter(self, t):
        self.ed.filter = t
        self.rebuild()

    def render_layers(self):
        clear(self.layers_lay)
        i = 0
        for k, (name, color) in KINDS.items():
            n = len([e for e in self.ed.entities if e.kind == k and not e.is_handle and not e.derived])
            if not n:
                continue
            cb = QCheckBox("%s %d" % (name, n))
            cb.setChecked(k not in self.ed.hidden)
            cb.setStyleSheet("QCheckBox::indicator:checked { background: %s; border-color: %s; }" % (color, color))
            cb.toggled.connect(lambda on, k=k: self.ed.set_layer(k, on))
            self.layers_lay.addWidget(cb, i // 2, i % 2)
            i += 1

    def rebuild(self):
        """The tree, after the entities were built again"""
        ed = self.ed
        self.render_layers()
        for i in range(self.tree.topLevelItemCount()):
            it = self.tree.topLevelItem(i)
            self.open[it.data(0, Qt.UserRole)] = it.isExpanded()
        self.tree.clear()
        self.items = {}
        f = ed.filter.lower()
        for k, (name, color) in KINDS.items():
            lst = [e for e in ed.entities if e.kind == k and not e.is_handle and not e.derived and
                   (not f or f in (e.label + " " + (e.tag or "")).lower())]
            if not lst:
                continue
            head = QTreeWidgetItem([name.upper() + "  %d" % len(lst), ""])
            head.setData(0, Qt.UserRole, k)
            head.setFirstColumnSpanned(False)
            self.tree.addTopLevelItem(head)
            btn = QToolButton()
            btn.setText("all")
            btn.setObjectName("mini")
            btn.setToolTip("Select all %s (Ctrl adds to the selection)" % name.lower())
            btn.clicked.connect(lambda _=False, lst=lst: self.select_all(lst))
            self.tree.setItemWidget(head, 1, btn)
            for e in lst:
                it = QTreeWidgetItem(["%s   %s" % (e.label, "" if e.tag == "GaribGroup" else e.tag), ""])
                it.setData(0, Qt.UserRole + 1, id(e))
                head.addChild(it)
                self.items[id(e)] = (it, e, lst)
            default_open = (k not in ("land", "sound")) or bool(f)
            head.setExpanded(self.open.get(k, default_open))
        self.tree.resizeColumnToContents(1)
        self.tree.setColumnWidth(0, max(120, self.tree.viewport().width() - 44))
        self.render()

    def select_all(self, lst):
        from PySide6.QtWidgets import QApplication
        if QApplication.keyboardModifiers() & (Qt.ControlModifier | Qt.MetaModifier):
            self.ed.set_selection(self.ed.selection + lst)
        else:
            self.ed.set_selection(lst)

    def render(self):
        """Marks the selected rows"""
        ed = self.ed
        sel = set(map(id, ed.selection))
        owners = {id(s.owner) for s in ed.selection if s.owner is not None}
        bad = {}
        for key, (it, e, _) in self.items.items():
            on = key in sel or key in owners
            if key not in bad:
                bad[key] = any(l == "err" for l, _ in ed.issues_for(e))
            it.setForeground(0, Qt.GlobalColor.yellow if on else (Qt.GlobalColor.red if bad[key] else Qt.GlobalColor.white))
            font = it.font(0)
            font.setBold(on)
            it.setFont(0, font)
            it.setForeground(1, Qt.GlobalColor.gray)
            if e.imported:
                it.setToolTip(0, "Imported at ROM build")
        for i in range(self.tree.topLevelItemCount()):
            head = self.tree.topLevelItem(i)
            n = sum(1 for j in range(head.childCount()) if head.child(j).data(0, Qt.UserRole + 1) in sel)
            k = head.data(0, Qt.UserRole)
            base = KINDS[k][0].upper() + "  %d" % head.childCount()
            head.setText(0, base + ("  · %d selected" % n if n else ""))
            if n and not head.isExpanded():
                head.setExpanded(True)

    def on_click(self, it, col):
        from PySide6.QtWidgets import QApplication
        key = it.data(0, Qt.UserRole + 1)
        if key is None or key not in self.items:
            return
        _, e, lst = self.items[key]
        mods = QApplication.keyboardModifiers()
        if mods & (Qt.ControlModifier | Qt.MetaModifier):
            self.ed.toggle_select(e)
        elif mods & Qt.ShiftModifier:
            self.ed.range_select(lst, e)
        else:
            self.ed.select(e, focus=True)


# -------------------------------------------------------------------------------------------------------------------
# Inspector

class FitScroll(QScrollArea):
    """A scroll area that sizes its body to its width and to the height the body's wrapped text needs.
    QScrollArea's own sizing (widgetResizable) sometimes keeps an old height and squeezes the cards."""

    def setWidget(self, w):
        super().setWidget(w)
        w.installEventFilter(self)
        self.refit()

    def refit(self):
        """Fits once the widgets just added are shown (Qt shows them from the event queue)"""
        QTimer.singleShot(0, self.fit)

    def eventFilter(self, obj, e):
        if obj is self.widget() and e.type() == QEvent.LayoutRequest:
            self.refit()
        return super().eventFilter(obj, e)

    def fit(self):
        b = self.widget()
        if b is None or b.layout() is None:
            return
        w = self.viewport().width()
        lay = b.layout()
        h = lay.totalHeightForWidth(w) if lay.hasHeightForWidth() else lay.totalMinimumSize().height()
        h = max(h, lay.totalMinimumSize().height(), self.viewport().height())
        if b.size() != QSize(w, h):
            b.resize(w, h)

    def resizeEvent(self, e):
        super().resizeEvent(e)
        self.fit()


class Inspector(FitScroll):
    reskin = Signal(object, object, int)   # node, worlds, seed: give scenery a new look (the window runs it)

    def __init__(self, ed):
        super().__init__()
        self.ed = ed
        self.setWidgetResizable(False)   # fit() sizes the body
        self.setHorizontalScrollBarPolicy(Qt.ScrollBarAlwaysOff)   # long lines wrap
        self.body = QWidget()
        self.lay = QVBoxLayout(self.body)
        self.lay.setContentsMargins(10, 10, 10, 10)
        self.lay.setSpacing(8)
        self.setWidget(self.body)
        self.refresh = []          # (widget, show) pairs: values that follow a gizmo drag
        self.shown = None
        ed.selection_changed.connect(self.on_selection)
        ed.rebuilt.connect(self.render)

    def on_selection(self):
        key = [id(e) for e in self.ed.selection]
        if key == self.shown and self.ed._drag is not None:
            # During a drag only the values change
            for w, show in self.refresh:
                if not w.hasFocus():
                    show()
            return
        self.render()

    def render(self):
        ed = self.ed
        self.refresh = []
        same = [id(e) for e in ed.selection] == self.shown
        self.shown = [id(e) for e in ed.selection]
        # A fresh body each time: the scroll area doesn't grow the old one when the new content is taller
        keep = self.verticalScrollBar().value() if same else 0
        self.body = QWidget()
        self.lay = QVBoxLayout(self.body)
        self.lay.setContentsMargins(10, 10, 10, 10)
        self.lay.setSpacing(8)
        try:
            self.fill()
        finally:
            # The old body may hold the button whose click led here: delete it later, not now
            old = self.takeWidget()
            if old is not None:
                old.hide()
                old.deleteLater()
            self.setWidget(self.body)
            QTimer.singleShot(0, lambda: self.verticalScrollBar().setValue(keep))

    def fill(self):
        ed = self.ed
        if ed.root is None:
            return
        sel = ed.selection
        if not sel and ed.puzzle_shown is not None:
            self.puzzle_view(ed.puzzle_shown)
        elif not sel:
            self.lay.addWidget(label("No selection", "h2"))
            self.lay.addWidget(label("Click something in the level, drag a box (B), or pick from the list on the left. Ctrl+click adds to the selection."))
            self.lay.addWidget(self.stats())
            self.level_settings()
        elif len(sel) > 1:
            self.multi_panel()
        else:
            ent = sel[0]
            self.lay.addWidget(label(ent.label, "h2"))
            self.lay.addWidget(label("%s · <%s>" % (KINDS[ent.kind][0], ent.tag) + (" · model %s" % ent.object_id if ent.object_id else "") +
                                     (" · imported from %s" % ed.catalog[ent.object_id]["bank"] if ent.imported else ""), "muted"))
            for level, msg in ed.issues_for(ent):
                self.lay.addWidget(check_line(level, msg))
            if ent.bind:
                self.lay.addWidget(self.transform_panel(ent))
            if ent.zone:
                self.lay.addWidget(self.camera_zone_panel(ent))
            if ent.kind == "trigger":
                b = QPushButton("Edit its puzzle (conditions and actions)")
                b.clicked.connect(lambda: ed.open_puzzle(ent.node))
                self.lay.addWidget(b)
            node = ent.sub if ent.tag == "GaribGroup" else ent.sub if ent.is_handle and ent.sub is not None else ent.node
            if node.tag == "Enemy":
                self.enemy_panel(node)
            else:
                self.lay.addWidget(self.cmd_card(node))
            if node.tag in ("LandActor", "BackgroundActor") and ent.object_id:
                self.new_look(node, ent.object_id)
            if ent.tag == "GaribGroup":
                n = len(kids(ent.node, "Garib"))
                self.lay.addWidget(self.cmd_card(ent.node, False, " · parent of %d garib%s" % (n, "" if n == 1 else "s")))
            elif ent.is_handle and ent.owner is not None and ent.owner.node is not None:
                b = QPushButton("Select %s" % ent.owner.label)
                b.clicked.connect(lambda: ed.select(ent.owner))
                self.lay.addWidget(b)
        self.lay.addStretch(1)

    def level_settings(self):
        """The level's look and feel: fog, lights, backdrop, gravity and ambient sound, which have no place in the view"""
        ed = self.ed
        from .editor import LOOK, LOOKS
        head = QHBoxLayout()
        head.addWidget(label("LEVEL SETTINGS", "h4"), 1)
        looks = QMenu(self)
        for name in LOOKS:
            a = looks.addAction(name)
            a.triggered.connect(lambda _=False, name=name: ed.apply_look(name))
        lb = QToolButton()
        lb.setText("Look")
        lb.setToolTip("Set the lights and fog to a preset (dark, coloured moods, or the shipped levels' daylight)")
        lb.setObjectName("mini")
        lb.setMenu(looks)
        lb.setPopupMode(QToolButton.InstantPopup)
        head.addWidget(lb)
        menu = QMenu(self)
        for tag, text in LOOK.items():
            a = menu.addAction(text)
            a.triggered.connect(lambda _=False, tag=tag: ed.add_look(tag))
        b = QToolButton()
        b.setText("+ Add")
        b.setObjectName("mini")
        b.setMenu(menu)
        b.setPopupMode(QToolButton.InstantPopup)
        head.addWidget(b)
        self.lay.addLayout(head)
        self.lay.addWidget(label("The game reads these once when the level starts. Fog's near_clip is the far plane: the game draws "
                                 "everything nearer than it, so a shorter one draws less.", "note"))
        w, g = self.fields()
        name = QLineEdit(ed.root.get("name") or "")
        name.setObjectName("field")
        name.setMaxLength(7)

        def set_name():
            v = name.text()
            if v != (ed.root.get("name") or ""):
                ed.set_attr(ed.root, "name", v)
        name.editingFinished.connect(set_name)
        g.addWidget(label("File name", "fieldname", wrap=False), 0, 0)
        g.addWidget(name, 0, 1)
        g.addWidget(label("The name inside the level file, 7 letters like the shipped ones (an even length crashes a console). "
                          "It isn't the title the game shows.", "note"), 1, 0, 1, 2)
        self.lay.addWidget(w)
        self.slot_card()
        for node in kids(ed.root):
            if node.tag in LOOK:
                self.lay.addWidget(self.cmd_card(node))
        self.puzzle_list()

    def slot_card(self):
        """The level's row in the game's level table: title, music and bonus clock (not in the level file)"""
        ed = self.ed
        info = ed.level_info()
        f, lay = self.card("Title, music and clock")
        w, g = self.fields()
        title = QLineEdit(info["name"] or "")
        title.setObjectName("field")
        title.setMaxLength(39)
        title.setPlaceholderText("the shipped title")

        def set_title():
            v = title.text().upper().strip()
            if v and (not v.isascii() or not all(c.isalnum() or c in " .-!?':" for c in v)):
                return ed.set_status("Use letters, numbers, spaces and . - ! ? ' : in a title.", "err")
            if (v or None) != ed.level_info()["name"]:
                ed.set_level_info("name", v or None)
        title.editingFinished.connect(set_title)
        song = narrow(QComboBox())
        song.addItem("Keep the shipped music", None)
        for k, text in SONGS:
            song.addItem("%d  %s" % (k, text), k)
        song.setCurrentIndex(max(0, song.findData(info["song"])) if info["song"] is not None else 0)
        song.activated.connect(lambda _: ed.set_level_info("song", song.currentData()))
        clock = narrow(QComboBox())
        clock.addItems(["Keep the shipped clock", "No clock", "Seconds:"])
        bt = info["bonus_time"]
        clock.setCurrentIndex(0 if bt is None else 1 if bt == -1 else 2)
        secs = QSpinBox()
        secs.setRange(1, 32766)
        secs.setValue(bt if bt and bt > 0 else 60)
        secs.setVisible(clock.currentIndex() == 2)
        def set_clock(k):
            secs.setVisible(k == 2)
            ed.set_level_info("bonus_time", None if k == 0 else -1 if k == 1 else secs.value())
        clock.activated.connect(set_clock)
        secs.editingFinished.connect(lambda: clock.currentIndex() == 2 and ed.set_level_info("bonus_time", secs.value()))
        for r, (l, wd) in enumerate([("title", title), ("music", song), ("clock", clock), ("", secs)]):
            g.addWidget(label(l, "fieldname", wrap=False), r, 0)
            g.addWidget(wd, r, 1)
        lay.addWidget(w)
        lay.addWidget(label("The game's level table, written into the ROM at build. The title shows in the level select. "
                            "At 0 the clock says TIME UP and sends the player to the way room, as in a bonus level.", "note"))
        if ed.can_be_extra():
            ex = QCheckBox("Extra level (Expansion Pak)")
            ex.setChecked(bool(info.get("extra")))
            ex.toggled.connect(lambda on: ed.set_level_info("extra", bool(on)))
            lay.addWidget(ex)
            lay.addWidget(label("An extra level is stored behind the game mod. The shipped level stays in its slot. Players reach the extra "
                                "levels through an Extra levels entry zone in a hub (Add, Presets), then the Space way room. "
                                "Progress in them is kept apart from the shipped Space levels.", "note"))
        self.lay.addWidget(f)

    def new_look(self, node, oid):
        """Reskin: the same model with textures from other worlds"""
        ed = self.ed
        info = ed.catalog.get(oid) or {}
        src = info.get("reskinFrom") or (None if info.get("custom") else oid)
        if not src:
            return
        own = (ed.catalog.get(src) or {}).get("world")
        f, lay = self.card("New look")
        lay.addWidget(label("A copy of the model with textures from other worlds. The copy is kept with your imported models and "
                            "copied into the level's banks at ROM build.", "note"))
        boxes = []
        grid = QGridLayout()
        for i, w_ in enumerate(("ATLANTIS", "CARNIVAL", "PIRATES", "PREHISTORIC", "FORTRESS", "OOTW", "HUB")):
            b = QCheckBox({"OOTW": "Space"}.get(w_, w_.title()))
            b.setEnabled(w_ != own)
            b.setChecked(w_ != own and w_ in ("PIRATES",) or False)
            boxes.append((w_, b))
            grid.addWidget(b, i // 2, i % 2)
        lay.addLayout(grid)
        seed = QSpinBox()
        seed.setRange(0, 99999)
        seed.setValue(int(getattr(self, "reskin_seed", 0)))
        row = QHBoxLayout()
        row.addWidget(label("seed", "fieldname", wrap=False))
        row.addWidget(seed, 1)
        lay.addLayout(row)

        def go(next_seed=False):
            worlds = [w_ for w_, b in boxes if b.isChecked()]
            if not worlds:
                return ed.set_status("Pick at least one world.", "err")
            if next_seed:
                seed.setValue(seed.value() + 1)
            self.reskin_seed = seed.value()
            self.reskin.emit(node, worlds, seed.value())
        btns = QHBoxLayout()
        a = QPushButton("Apply")
        a.clicked.connect(lambda: go())
        t = QPushButton("Try another")
        t.clicked.connect(lambda: go(True))
        btns.addWidget(a)
        btns.addWidget(t)
        if info.get("reskinFrom"):
            o = QPushButton("Original")
            o.setToolTip("Back to the shipped model")
            o.clicked.connect(lambda: ed.mutate(lambda: (node.set("object_id", src), node.set("name", (ed.catalog.get(src) or {}).get("name", "")[:7]))))
            btns.addWidget(o)
        lay.addLayout(btns)
        self.lay.addWidget(f)

    def enemy_panel(self, enemy):
        """An enemy's own fields and commands, then its script as three lists"""
        from . import scripts
        ed = self.ed
        self.lay.addWidget(self.cmd_card(enemy, with_children=False))
        for c in kids(enemy):
            if c.tag not in scripts.GROUPS and c.tag != "EnemyFinalize":
                self.lay.addWidget(self.cmd_card(c))
        head = QHBoxLayout()
        head.addWidget(label("ENEMY SCRIPT", "h4"), 1)
        menu = QMenu(self)
        for name in scripts.templates(ed.format, enemy):
            a = menu.addAction(name)
            a.triggered.connect(lambda _=False, name=name: ed.add_script_template(enemy, name))
        b = QToolButton()
        b.setText("Ready-made")
        b.setToolTip("Replace the normal list with a ready-made script")
        b.setObjectName("mini")
        b.setMenu(menu)
        b.setPopupMode(QToolButton.InstantPopup)
        head.addWidget(b)
        self.lay.addLayout(head)
        self.lay.addWidget(label("The enemy runs its normal list from #0. Each instruction ends when its lifetime runs out (0 or -1: no limit) "
                                 "or its action is done. Conditional ones interrupt it when their condition holds; Start the attack list runs the attack list.", "note"))
        ls = scripts.lists(enemy)
        open_ = getattr(self, "script_open", None)
        for kind in scripts.GROUPS:
            items = ls[kind]
            h = QHBoxLayout()
            h.addWidget(label("%s  %d/%d" % (scripts.KIND_NAMES[kind], len(items), scripts.LIMITS[kind]), "h3"), 1)
            m = QMenu(self)
            for t, text in scripts.TYPES.items():
                a = m.addAction("%s   (%d)" % (text, t))
                a.triggered.connect(lambda _=False, t=t, kind=kind: ed.add_instruction(enemy, kind, t))
            ab = QToolButton()
            ab.setText("+ Add")
            ab.setObjectName("mini")
            ab.setMenu(m)
            ab.setPopupMode(QToolButton.InstantPopup)
            h.addWidget(ab)
            self.lay.addLayout(h)
            for i, ins in enumerate(items):
                t = scripts.code(ins)
                bd = scripts.body(ins)
                extra = ""
                if t == 18 and bd is not None:
                    extra = " → #%d" % inum(bd, "instr_idx")
                life = inum(ins, "lifetime")
                key = (tuple(path_of(enemy)), kind, i)
                btns = [("…", "Show or hide its fields", lambda key=key: (setattr(self, "script_open", None if open_ == key else key), self.render())),
                        ("↑", "Move earlier", lambda ins=ins: ed.move_instruction(ins, -1)),
                        ("↓", "Move later", lambda ins=ins: ed.move_instruction(ins, 1))]
                if not ed.removal_refused(ins):
                    btns.append(("×", "Remove", lambda ins=ins: ed.remove_instruction(ins)))
                cf, cl = self.card("#%d  %s%s" % (i, scripts.TYPES.get(t, "type %d" % t), extra),
                                   "%d frames" % life if life > 0 else None, btns)
                cond = ins.get("execution_condition") or "always"
                if cond != "always":
                    cl.addWidget(label("when " + cond.replace("_", " "), "note"))
                if t in scripts.NEEDS_TARGET:
                    tg = narrow(QComboBox())
                    for bit, text in scripts.TARGETS.items():
                        tg.addItem("Target: " + text, bit)
                    fl = scripts.flags_value(ins.get("flags"), ed.flag_enum())
                    cur = next((b_ for b_ in scripts.TARGETS if fl & b_), None)
                    if cur is None:
                        tg.addItem("Target: none (pick one)", 0)
                        cur = 0
                    tg.setCurrentIndex(tg.findData(cur))
                    tg.activated.connect(lambda _, tg=tg, ins=ins: ed.set_instruction_target(ins, tg.currentData()))
                    cl.addWidget(tg)
                if open_ == key:
                    pick = narrow(QComboBox())
                    for k, text in scripts.TYPES.items():
                        pick.addItem("%s   (%d)" % (text, k), k)
                    pick.setCurrentIndex(max(0, pick.findData(t)))
                    pick.activated.connect(lambda _, pick=pick, ins=ins: ed.set_instruction_type(ins, pick.currentData()))
                    cl.addWidget(pick)
                    cl.addWidget(self.cmd_card(ins, with_children=False, buttons=False))
                    if bd is not None:
                        cl.addWidget(self.cmd_card(bd, with_children=False, buttons=False))
                self.lay.addWidget(cf)

    def puzzle_list(self):
        """Every puzzle in the level, also those with no trigger volume in the view"""
        from .puzzles import summary
        ed = self.ed
        ps = kids(ed.root, "Puzzle")
        head = QHBoxLayout()
        head.addWidget(label("PUZZLES (%d)" % len(ps), "h4"), 1)
        b = QToolButton()
        b.setText("+ New puzzle")
        b.setObjectName("mini")
        b.clicked.connect(ed.add_puzzle)
        head.addWidget(b)
        self.lay.addLayout(head)
        self.lay.addWidget(label("A puzzle runs its actions when its conditions hold. The game tests every puzzle each frame.", "note"))
        for i, p in enumerate(ps):
            row = ClickLabel("%d. %s" % (i + 1, summary(p)))
            row.setObjectName("prow")
            row.setToolTip("Open puzzle %d" % (i + 1))
            row.clicked.connect(lambda _=False, p=p: ed.open_puzzle(p))
            self.lay.addWidget(row)

    def puzzle_view(self, p):
        """The puzzle card: when it fires, its condition logic, its conditions and its actions"""
        from .puzzles import ACTIONS, CONDS, body, code, fires, logic, summary
        ed = self.ed
        back = QPushButton("← Level settings and all puzzles")
        back.clicked.connect(lambda: ed.open_puzzle(None) or self.render())
        self.lay.addWidget(back)
        i = kids(ed.root, "Puzzle").index(p)
        self.lay.addWidget(label("Puzzle %d" % (i + 1), "h2"))
        self.lay.addWidget(label(summary(p), "muted"))
        for v in ed.violations:
            if v["top"] is p:
                self.lay.addWidget(check_line("note" if v["shipped"] else v.get("level", "err"), v["msg"]))
        f, lay = self.card("Puzzle", None, [("×", "Delete this puzzle", lambda: ed.remove_puzzle(p))])
        w, g = self.fields()
        n = fires(p)
        fire = narrow(QComboBox())
        fire.addItems(["Every frame while true", "Once", "A number of times"])
        fire.setCurrentIndex(0 if n is None else 1 if n == 1 else 2)
        times = QSpinBox()
        times.setRange(1, 32767)
        times.setValue(n or 1)
        times.setVisible(fire.currentIndex() == 2)
        fire.activated.connect(lambda k: ed.set_fires(p, None if k == 0 else 1 if k == 1 else max(2, times.value())))
        times.editingFinished.connect(lambda: ed.set_fires(p, times.value()))
        g.addWidget(label("Fires", "fieldname", wrap=False), 0, 0)
        g.addWidget(fire, 0, 1)
        g.addWidget(times, 1, 1)
        how, m = logic(p)
        lg = narrow(QComboBox())
        lg.addItems(["All conditions", "Any condition", "At least N conditions"])
        lg.setCurrentIndex({"all": 0, "any": 1, "least": 2}[how])
        least = QSpinBox()
        least.setRange(1, max(1, m, len(kids(p, "PuzzleCond"))))
        least.setValue(max(1, m))
        least.setVisible(how == "least")
        lg.activated.connect(lambda k: ed.set_logic(p, ("all", "any", "least")[k], least.value()))
        least.editingFinished.connect(lambda: ed.set_logic(p, "least", least.value()))
        g.addWidget(label("Needs", "fieldname", wrap=False), 2, 0)
        g.addWidget(lg, 2, 1)
        g.addWidget(least, 3, 1)
        lay.addWidget(w)
        lay.addWidget(label("Without a limit, actions run again on every frame the conditions hold. Camera zones need that; most other puzzles want Once.", "note"))
        self.lay.addWidget(f)
        for kind, names, attr, title in (("PuzzleCond", CONDS, "cond_type", "CONDITIONS"), ("PuzzleAction", ACTIONS, "action_type", "ACTIONS")):
            head = QHBoxLayout()
            head.addWidget(label(title, "h4"), 1)
            menu = QMenu(self)
            for k, text in names.items():
                a = menu.addAction("%s   (%d)" % (text, k))
                a.triggered.connect(lambda _=False, k=k, kind=kind: ed.add_puzzle_part(p, kind, k))
            b = QToolButton()
            b.setText("+ Add")
            b.setObjectName("mini")
            b.setMenu(menu)
            b.setPopupMode(QToolButton.InstantPopup)
            head.addWidget(b)
            self.lay.addLayout(head)
            parts = kids(p, kind)
            if not parts:
                self.lay.addWidget(label("None: it fires when the level starts." if kind == "PuzzleCond" else "None yet.", "note"))
            for part in parts:
                t = code(part, attr)
                btns = [("↑", "Move earlier", lambda part=part: ed.move_node(part, -1)),
                        ("↓", "Move later", lambda part=part: ed.move_node(part, 1)),
                        ("×", "Remove", lambda part=part: ed.remove_node(part))]
                cf, cl = self.card(names.get(t, "%s %d" % (kind, t)), str(t), btns)
                pick = narrow(QComboBox())
                for k, text in names.items():
                    pick.addItem("%s   (%d)" % (text, k), k)
                if t not in names:
                    pick.addItem("%d" % t, t)
                pick.setCurrentIndex(pick.findData(t))
                pick.activated.connect(lambda _, pick=pick, part=part: ed.set_part_type(part, pick.currentData()))
                cl.addWidget(pick)
                bd = body(part)
                if bd is not None:
                    cl.addWidget(self.cmd_card(bd, with_children=False, buttons=False))
                self.lay.addWidget(cf)

    def stats(self):
        ed = self.ed
        count = lambda t: len(list(ed.root.iter(t)))
        rows = [("Garibs", count("Garib")), ("Enemies", count("Enemy")), ("Platforms", count("Platform") + count("NullPlatform")),
                ("Puzzles", count("Puzzle")), ("Mr. Tip messages", count("MrTip")), ("Landscape pieces", count("LandActor")),
                ("Commands", sum(1 for e in ed.root.iter() if isinstance(e.tag, str)) - 1)]
        w = QWidget()
        g = QGridLayout(w)
        g.setContentsMargins(0, 8, 0, 0)
        for i, (a, b) in enumerate(rows):
            g.addWidget(label(a, "muted"), i, 0)
            g.addWidget(label(str(b)), i, 1)
        return w

    def card(self, title, op=None, buttons=()):
        """A titled box. buttons: (text, tip, fn) or (text, tip, QMenu) for the header's right"""
        f = QFrame()
        f.setObjectName("card")
        f.setSizePolicy(QSizePolicy.Preferred, QSizePolicy.Minimum)
        lay = QVBoxLayout(f)
        lay.setContentsMargins(8, 6, 8, 8)
        lay.setSpacing(4)
        head = QHBoxLayout()
        head.addWidget(label(title, "h3"), 1)
        if op:
            head.addWidget(label(op, "op", wrap=False))
        for text, tip, fn in buttons:
            b = QToolButton()
            b.setText(text)
            b.setToolTip(tip)
            b.setObjectName("mini")
            if isinstance(fn, QMenu):
                b.setMenu(fn)
                b.setPopupMode(QToolButton.InstantPopup)
            else:
                # clicked passes `checked`: a lambda with default arguments would take it as its first one
                b.clicked.connect(lambda _=False, fn=fn: fn())
            head.addWidget(b)
        lay.addLayout(head)
        return f, lay

    def colour_button(self, node):
        """A swatch of r, g, b that opens a colour picker. Fog and lights use 0 to 255: the game keeps the
        low byte of a bigger value"""
        from PySide6.QtGui import QColor
        from PySide6.QtWidgets import QColorDialog
        ed = self.ed
        clamp = lambda v: max(0, min(255, inum(node, v)))
        b = QPushButton("Colour…")
        b.setStyleSheet("background: rgb(%d,%d,%d); color: %s" % (clamp("r"), clamp("g"), clamp("b"),
                                                                  "#000" if sum(clamp(k) for k in "rgb") > 380 else "#fff"))

        def pick():
            c = QColorDialog.getColor(QColor(clamp("r"), clamp("g"), clamp("b")), self, "Colour of <%s>" % node.tag)
            if c.isValid():
                def fn():
                    node.set("r", str(c.red()))
                    node.set("g", str(c.green()))
                    node.set("b", str(c.blue()))
                ed.mutate(fn)
        b.clicked.connect(pick)
        return b

    def command_buttons(self, node):
        """+ (add a command inside), ↑ ↓ (move) and × (remove) for an element of the level, and Tidy order on a
        platform. Menu entries the element can't take are greyed out, with the reason as the tooltip."""
        from .commands import PLATFORMS, refusal
        ed = self.ed
        out = []
        allowed = ed.format.children_of(node.tag)
        if allowed:
            menu = QMenu(self)
            menu.setToolTipsVisible(True)
            for t in allowed:
                a = menu.addAction(ed.format.label(t) + ("" if ed.format.label(t) == t else "   <%s>" % t))
                why = refusal(node, t)
                if why:
                    a.setEnabled(False)
                    a.setToolTip(why)
                a.triggered.connect(lambda _=False, t=t: ed.add_command(node, t))
            out.append(("+", "Add a command inside <%s>" % node.tag, menu))
        if node.tag in PLATFORMS and len(kids(node)) > 1:
            out.append(("Tidy", "Tidy order: move the commands the game would read in the wrong order", lambda: ed.tidy_node(node)))
        parent = node.getparent()
        if parent is not None and node.tag != "EnemyFinalize":
            out.append(("↑", "Move earlier (the game reads the level in order)", lambda: ed.move_node(node, -1)))
            out.append(("↓", "Move later", lambda: ed.move_node(node, 1)))
            if not ed.removal_refused(node):
                out.append(("×", "Remove <%s>" % node.tag, lambda: ed.remove_node(node)))
        return out

    def fields(self):
        w = QWidget()
        g = QGridLayout(w)
        g.setContentsMargins(0, 0, 0, 0)
        g.setHorizontalSpacing(8)
        g.setVerticalSpacing(3)
        g.setColumnStretch(1, 1)
        return w, g

    def transform_panel(self, ent):
        """Position, rotation and scale as the game stores them for this kind of object"""
        ed = self.ed
        f, lay = self.card("Transform")
        w, g = self.fields()
        row = [0]

        def add(name, widget, note=None):
            g.addWidget(label(name, "fieldname", wrap=False), row[0], 0)
            g.addWidget(widget, row[0], 1)
            row[0] += 1
            if note:
                g.addWidget(label(note, "note"), row[0], 0, 1, 2)
                row[0] += 1

        def pos_set(i):
            def set_(v):
                def fn():
                    p = ent.bind.get()
                    d = np.zeros(3)
                    d[i] = v - p[i]
                    ent.bind.set(p + d)
                    for b in ent.points:
                        b.set(b.get() + d)
                ed.mutate(fn)
            return set_
        add("position", trio(*[num_input(lambda i=i: ent.bind.get()[i], pos_set(i), 1, self.refresh) for i in range(3)]))
        if ent.rot == "xyz":
            e = lambda: vec.euler_yzx_from_quat(ent.get_quat())

            def set_a(i):
                def set_(v):
                    def fn():
                        cur = e()
                        cur[i] = math.radians(v)
                        ent.set_quat(vec.quat_from_euler_yzx(*cur))
                    ed.mutate(fn)
                return set_
            add("rotation °", trio(*[num_input(lambda i=i: e()[i] * DEG, set_a(i), 5, self.refresh) for i in range(3)]),
                ent.why.get("rot") or "SetActorRotation, applied Y·Z·X like GetQuaternionFromXYZRot in the decomp.")
        else:
            add("rotation", label(ent.why.get("rot") or WHY["noOrientation"], "note"))
        if ent.scl == "xyz":
            def set_s(i):
                def set_(v):
                    def fn():
                        s = ent.get_scale()
                        s[i] = v
                        ent.set_scale(s)
                    ed.mutate(fn)
                return set_
            add("scale", trio(*[num_input(lambda i=i: ent.get_scale()[i], set_s(i), 0.05, self.refresh) for i in range(3)]),
                "PlatScale. The game ignores it after the first path point. The editor keeps it before them." if ent.tag == "Platform"
                else ent.why.get("scl") or "SetActorScale: x, y, z in the model’s own axes (guScaleF in object.c).")
        elif ent.scl == "box":
            add("size", trio(*[num_input(get, lambda v, set_=set_: ed.mutate(lambda: set_(v)), 5, self.refresh) for _, get, set_ in ent.size_fields]),
                ", ".join(n for n, _, _ in ent.size_fields) + (". Scaling keeps the centre." if ent.kind == "trigger" else ""))
        else:
            add("scale", label(ent.why.get("scl") or WHY["noScale"], "note"))
        lay.addWidget(w)
        return f

    def camera_zone_panel(self, ent):
        ed = self.ed
        puzzle = ent.node
        z = camera_zone(puzzle)
        f, lay = self.card("Camera zone")
        w, g = self.fields()
        row = [0]

        def put(a, b=None, span=False):
            if span:
                g.addWidget(a, row[0], 0, 1, 2)
            else:
                g.addWidget(a, row[0], 0)
                g.addWidget(b, row[0], 1)
            row[0] += 1
        preset = QComboBox()
        preset.addItem("Choose…", "")
        for k, (lab, note, _) in CAM_PRESETS.items():
            preset.addItem("%s: %s" % (lab, note), k)
        facing = QLineEdit(str(round(ed.view_facing() * DEG)))
        facing.setObjectName("num")

        def on_preset():
            k = preset.currentData()
            if not k:
                return
            try:
                fv = float(facing.text())
            except ValueError:
                fv = 0.0
            c = zone_centre(z["vol"])
            ed.mutate(lambda: __import__("desktop.entities", fromlist=["set_camera_zone"]).set_camera_zone(puzzle, preset_zone(k, fv / DEG, c)))
        preset.activated.connect(lambda _: on_preset())
        put(label("preset", "fieldname"), preset)
        put(label("player walks °", "fieldname"), facing)
        put(label("A preset replaces the actions below. “Player walks” is the heading the player goes along in the zone (0 is +z); it starts as the way the view looks.", "note"), span=True)

        def row_(name, on, inputs, enable, disable, note):
            cb = QCheckBox(name)
            cb.setChecked(on)
            cb.toggled.connect(lambda c: ed.write_zone(puzzle, enable if c else disable))
            for i in inputs:
                i.setEnabled(on)
            put(cb, trio(*inputs))
            if note:
                put(label(note, "note"), span=True)

        def n(get, set_, step):
            return num_input(get, lambda v: ed.write_zone(puzzle, lambda cur: set_(cur, v)), step)

        def enable_look(cur):
            cur["look"], cur["speed"] = ed.view_facing(), 2

        row_("look °", z["look"] is not None,
             [n(lambda: (z["look"] or 0) * DEG, lambda cur, v: cur.update(look=v / DEG), 15),
              n(lambda: 2 if z["speed"] is None else z["speed"], lambda cur, v: cur.update(speed=v), 0.5)],
             enable_look, lambda cur: cur.update(look=None),
             "LookAtGlover: the heading from the camera to Glover, and how fast it turns there (the shipped levels use 1 to 4).")
        row_("distance", z["distance"] is not None, [n(lambda: z["distance"] or 0, lambda cur, v: cur.update(distance=v), 10)],
             lambda cur: cur.update(distance=250), lambda cur: cur.update(distance=None), "TweenDistance: how far behind Glover the camera stands.")
        row_("height", z["height"] is not None, [n(lambda: z["height"] or 0, lambda cur, v: cur.update(height=v), 10)],
             lambda cur: cur.update(height=100), lambda cur: cur.update(height=None), "TweenYAdjust: how far over Glover the camera rises (negative goes under).")
        fly = z["fly"] if z["fly"] is not None else vec.v3()

        def set_fly(cur, i, v):
            cur["fly"][i] = v

        def enable_fly(cur):
            v = zone_camera(dict(cur, look=cur["look"] if cur["look"] is not None else ed.view_facing()))
            cur["fly"] = v[1] if v else ed.orbit.target.copy()
        row_("fly to", z["fly"] is not None, [n(lambda i=i: fly[i], lambda cur, v, i=i: set_fly(cur, i, v), 10) for i in range(3)],
             enable_fly, lambda cur: cur.update(fly=None), "FlyTowardsPoint: the camera flies to this point and watches Glover from there.")
        lay.addWidget(w)
        view = zone_camera(z)
        if view:
            b = QPushButton("Look from the zone’s camera")

            def look():
                ed.camera.pos = view[1].copy()
                ed.orbit.target = view[0].copy()
                ed.orbit.update()
                ed.view_changed.emit()
            b.clicked.connect(look)
            lay.addWidget(b)
            lay.addWidget(label("Shows about what the player sees with Glover in the middle of the box.", "note"))
        return f

    def decomp_block(self, tag, node):
        ed = self.ed
        cname, c = ed.command_for(node) if node is not None else (tag, ed.decomp["commands"].get(tag))
        sw = ed.decomp["subSwitches"].get(tag)
        if not c and not sw:
            return None
        box = QFrame()
        box.setObjectName("decomp")
        lay = QVBoxLayout(box)
        lay.setContentsMargins(6, 4, 6, 4)
        lay.setSpacing(2)
        if c:
            import re
            resolved = re.search(r"0x[0-9a-f]+|todo", c["ksy"], re.I) and c.get("token")
            text = "decomp: <span style='color:#c58bff; font-family:monospace'>%s</span>" % (c.get("token") or "(unnamed)")
            if cname != tag:
                text += "  · via &lt;%s&gt;" % cname
            if resolved:
                text += "  · names libgarib’s placeholder <code>%s</code>" % c["ksy"]
            lay.addWidget(label(text, rich=True))
        for h in (c or {}).get("handlers", [])[:2]:
            href = GITHUB + (ed.decomp["source"].get("gloveworld") or "main") + "/" + h["file"] + "#L" + str(h["line"])
            row = QHBoxLayout()
            t = QToolButton()
            t.setText("▸")
            t.setCheckable(True)
            t.setObjectName("mini")
            row.addWidget(t)
            row.addWidget(label('<a style="color:#6fb7ff" href="%s">%s:%s</a> in %s()' % (href, h["file"], h["line"], h["func"]), rich=True))
            row.addStretch(1)
            lay.addLayout(row)
            code = QPlainTextEdit(h["snippet"])
            code.setReadOnly(True)
            code.setObjectName("code")
            code.setFixedHeight(min(220, 16 * (h["snippet"].count("\n") + 2)))
            code.hide()
            t.toggled.connect(lambda on, code=code, t=t: (code.setVisible(on), t.setText("▾" if on else "▸")))
            lay.addWidget(code)
        if sw and node is not None:
            try:
                raw = int(node.get(sw["field"]), 10)
                name = ed.decomp_enum_name(sw["enum"], raw - sw["offset"])
                if name:
                    lay.addWidget(label("%s %d → %s::%s" % (sw["field"], raw, sw["enum"], name), "tok"))
            except (TypeError, ValueError):
                pass
        if tag == "Enemy" and node is not None:
            v = ed.enum_value("Enemy", "type", node.get("type"))
            n = ed.decomp_enum_name("ActorType", v)
            if n:
                lay.addWidget(label("type %s → ActorType::%s" % (v, n), "tok"))
        if tag == "Powerup" and node is not None:
            n = ed.decomp_enum_name("Pickup", inum(node, "type"))
            if n:
                lay.addWidget(label("type %d → Pickup::%s" % (num(node, "type"), n), "tok"))
        if tag == "EnemyInstruction" and node is not None:
            v = ed.enum_value("EnemyInstruction", "execution_condition", node.get("execution_condition"))
            n = ed.decomp_enum_name("NmeCondition", v)
            if n:
                lay.addWidget(label("execution_condition → NmeCondition::%s" % n, "tok"))
        return box

    def cmd_card(self, node, with_children=True, extra="", buttons=True):
        from .puzzles import FLOAT_BITS, float_bits, float_from_bits
        ed = self.ed
        tag = node.tag
        c = ed.type_for(tag)
        op = "0x%02x" % c["opcode"] if c and c.get("opcode") is not None else None
        f, lay = self.card(tag + extra, op, self.command_buttons(node) if buttons else ())
        bits = node.getparent() is not None and node.getparent().tag == "PuzzleAction"
        if node.attrib:
            w, g = self.fields()
            r = 0
            for name in node.attrib:
                fd = next((x for x in (c or {}).get("fields", []) if x["id"] == name), None)
                en = ed.ksy_enum_for(tag, name)
                if en:
                    inp = narrow(QComboBox())
                    inp.addItems(list(en.values()))
                    if node.get(name) not in en.values():
                        inp.addItem(node.get(name))
                    inp.setCurrentText(node.get(name))
                    inp.activated.connect(lambda _, inp=inp, name=name: ed.set_attr(node, name, inp.currentText()))
                    self.refresh.append((inp, lambda inp=inp, name=name: inp.setCurrentText(node.get(name) or "")))
                elif bits and name in FLOAT_BITS:
                    # A float the game reads from a field libgarib names u4: show the number, write its bits
                    inp = QLineEdit(fmt(float_from_bits(node.get(name))))
                    inp.setObjectName("field")
                    shown = inp.text()

                    def done_bits(inp=inp, name=name, shown=shown):
                        if inp.text().strip() == shown:
                            return   # only focus left: the stored bits stay as they are
                        try:
                            v = float(inp.text())
                        except ValueError:
                            inp.setText(fmt(float_from_bits(node.get(name))))
                            return ed.set_status("%s expects a number" % name, "err")
                        if float_bits(v) != node.get(name):
                            ed.set_attr(node, name, float_bits(v))
                    inp.editingFinished.connect(done_bits)
                    fd = dict(fd or {}, type="f4·bits")
                else:
                    inp = QLineEdit(node.get(name))
                    inp.setObjectName("field")

                    def done(inp=inp, name=name, fd=fd):
                        v = inp.text().strip()
                        if v == node.get(name):
                            return
                        if fd and fd["type"][:1] in "fsu" and fd["type"][1:].isdigit() and not FLOAT.match(v):
                            inp.setText(node.get(name))
                            return ed.set_status("%s expects a number (%s)" % (name, fd["type"]), "err")
                        try:
                            val = fmt(float(v)) if fd and fd["type"].startswith("f") else v
                        except ValueError:
                            val = v
                        ed.set_attr(node, name, val)
                    inp.editingFinished.connect(done)
                    self.refresh.append((inp, lambda inp=inp, name=name: inp.setText(node.get(name) or "")))
                g.addWidget(label(name + ("  " + fd["type"] if fd else ""), "fieldname", wrap=False), r, 0)
                g.addWidget(inp, r, 1)
                r += 1
                if name == "object_id":
                    oid = hex_id(node.get(name))
                    note = ("model loaded from this level’s object banks" if oid in ed.loaded
                            else "imported from %s at ROM build" % ed.catalog[oid]["bank"] if oid in ed.catalog else "⚠ not in any object bank")
                    g.addWidget(label(note, "note"), r, 0, 1, 2)
                    r += 1
            lay.addWidget(w)
            if all(node.get(k) is not None for k in "rgb"):
                lay.addWidget(self.colour_button(node))
        d = self.decomp_block(tag, node)
        if d:
            lay.addWidget(d)
        if with_children:
            ch = kids(node)
            if ch:
                wrap = QFrame()
                wrap.setObjectName("children")
                wl = QVBoxLayout(wrap)
                wl.setContentsMargins(10, 2, 0, 0)
                wl.setSpacing(6)
                for k in ch:
                    wl.addWidget(self.cmd_card(k))
                lay.addWidget(wrap)
        return f

    def multi_panel(self):
        ed = self.ed
        sel, tset = ed.selection, ed.transform_set()
        by = {}
        for e in sel:
            by[e.kind] = by.get(e.kind, 0) + 1
        rot = len([e for e in tset if e.rot])
        scl = len([e for e in tset if e.scl])
        self.lay.addWidget(label("%d objects selected" % len(sel), "h2"))
        self.lay.addWidget(label(" · ".join("%d %s" % (n, KINDS[k][0].lower()) for k, n in by.items()), "muted"))
        f, lay = self.card("Transform selection")
        w, g = self.fields()
        mv = [QLineEdit("0") for _ in range(3)]
        for m in mv:
            m.setObjectName("num")
        axis = QComboBox()
        axis.addItems(["y", "x", "z"])
        deg = QLineEdit("90")
        deg.setObjectName("num")
        sc = QLineEdit("1.5")
        sc.setObjectName("num")

        def fl(w_, d=0.0):
            try:
                return float(w_.text())
            except ValueError:
                return d
        b1, b2, b3 = QPushButton("Apply"), QPushButton("Apply"), QPushButton("Apply")
        b1.clicked.connect(lambda: ed.apply_transform(move=vec.v3(*[fl(m) for m in mv])))
        b2.clicked.connect(lambda: ed.apply_transform(rot_deg=fl(deg), axis=axis.currentText()))
        b3.clicked.connect(lambda: ed.apply_transform(scale=fl(sc, 1)))
        g.addWidget(label("move by", "fieldname"), 0, 0)
        g.addWidget(trio(*mv, b1), 0, 1)
        g.addWidget(label("rotate by °", "fieldname"), 1, 0)
        g.addWidget(trio(axis, deg, b2), 1, 1)
        g.addWidget(label("scale by", "fieldname"), 2, 0)
        g.addWidget(trio(sc, b3), 2, 1)
        lay.addWidget(w)
        lay.addWidget(label("Pivot: %s. %d of %d can store a rotation and %d a size; the rest only move." % (
            "group centre (positions rotate and scale around it)" if ed.pivot_mode == "group" else "each object’s own origin",
            rot, len(tset), scl), "note"))
        self.lay.addWidget(f)
        for e in sel[:200]:
            row = QWidget()
            rl = QHBoxLayout(row)
            rl.setContentsMargins(0, 0, 0, 0)
            rl.addWidget(label(e.label, wrap=False), 1)
            rl.addWidget(label(e.tag or "", "muted", wrap=False))
            x = QToolButton()
            x.setText("×")
            x.setObjectName("mini")
            x.setToolTip("Remove from selection")
            x.clicked.connect(lambda _=False, e=e: ed.toggle_select(e))
            rl.addWidget(x)
            self.lay.addWidget(row)


# -------------------------------------------------------------------------------------------------------------------
# Checks, imports and the memory budget

class Checks(FitScroll):
    def __init__(self, ed):
        super().__init__()
        self.ed = ed
        self.setWidgetResizable(False)   # fit() sizes the body
        self.setHorizontalScrollBarPolicy(Qt.ScrollBarAlwaysOff)   # long lines wrap
        self.body = QWidget()
        self.lay = QVBoxLayout(self.body)
        self.lay.setContentsMargins(10, 6, 10, 10)
        self.lay.setSpacing(4)
        self.setWidget(self.body)
        self.imports = QWidget()
        self.imp_lay = QVBoxLayout(self.imports)
        self.imp_lay.setContentsMargins(0, 0, 0, 0)
        self.imp_lay.setSpacing(4)
        ed.rebuilt.connect(self.render)
        ed.imports_changed.connect(self.render_imports)

    def render(self):
        ed = self.ed
        self.imports.setParent(None)
        clear(self.lay)
        if ed.root is None:
            return
        self.lay.addWidget(label("CHECKS", "h4"))
        for c in ed.checks():
            self.lay.addWidget(check_line(c.level, c.text, c.on_click, c.link, c.sub, c.title))
        self.lay.addWidget(self.imports)
        self.lay.addStretch(1)
        self.render_imports()
        self.refit()

    def render_imports(self):
        self.fill_imports()
        self.refit()

    def fill_imports(self):
        ed = self.ed
        clear(self.imp_lay)
        if ed.root is None or ed.scene is None:
            return
        ids = ed.current_imports()
        heap = ed.heap.get(ed.scene["id"])
        add = self.imp_lay.addWidget
        kb = lambda n: "%.0f" % (n / 1024)
        if not ids:
            if heap:
                add(label("Free heap after this level loads: %s KB (measured in the emulator)." % kb(heap["free"]), "budget"))
            return
        add(label("IMPORTS AT ROM BUILD", "h4"))
        for i in ids:
            c = ed.catalog[i]
            add(check_line("imp", "%s (%s) from %s, %.1f KB" % (c["name"], i, c["bank"], c["bytes"] / 1024),
                           lambda i=i: ed.set_selection([e for e in ed.entities if e.object_id == i and not e.is_handle])))
        if any(not (ed.catalog.get(i) or {}).get("custom") for i in ids):
            cb = QCheckBox("Copy the models into the ROM instead")
            cb.setToolTip("Works without the Expansion Pak, but patches then contain game data")
            cb.setChecked(ed.copy_imports)

            def on(c):
                ed.copy_imports = c
                QTimer.singleShot(0, self.render_imports)
            cb.toggled.connect(on)
            add(cb)
            add(label("Works without the Expansion Pak, but patches then contain game data", "note"))
        p = ed.plan
        if p and ed.runtime_imports() and p.get("runtime"):
            rt = p["runtime"]
            copy = p.get("copy") or {}
            copied = copy["objectBytes"] + copy["texBytes"] if copy.get("objects") else 0
            cost = rt["bytes"] + copied
            if rt["objectBanks"] or rt["textureBanks"]:
                no, nt = len(rt["objectBanks"]), len(rt["textureBanks"])
                add(check_line("imp", "Build ROM and Export patch add the Expansion Pak game mod. When this level starts, it loads %d model bank%s and %d texture bank%s (%s KB) from the player’s own ROM. The ROM and patch contain no game data from those banks. The ROM then needs the Expansion Pak." % (
                    no, "" if no == 1 else "s", nt, "" if nt == 1 else "s", kb(rt["bytes"]))))
            if copied:
                n, t = len(copy["objects"]), len(copy["textures"])
                add(check_line("imp", "%d piece%s of your imported models (%s KB with %d texture%s) are copied into %s and %s. The ROM and patch contain them. The game mod gives them the Expansion Pak’s memory." % (
                    n, "" if n == 1 else "s", kb(copied), t, "" if t == 1 else "s", copy["targetBank"], copy["targetTexBank"])))
            extra = (ed.mod_info or {}).get("heapExtraBytes", 0)
            if heap:
                room = max(0, heap["free"] - 16 * 1024) + extra
                self.budget("%s KB of %s KB free with the Expansion Pak" % (kb(cost), kb(room)), cost / max(1, room),
                            "Only this level loads these banks. Free heap is measured in the emulator, minus 16 KB for allocations during play.")
            return
        if p:
            n = len(p["textures"])
            add(check_line("imp", "%d texture%s (%.1f KB) appended to %s." % (n, "" if n == 1 else "s", p.get("texBytes", 0) / 1024, p["targetTexBank"])))
            b = ed.import_budget()
            if b:
                t = b["rows"][0]
                if b["over"]:
                    if b["overWithPak"]:
                        pak = "Over budget in %s even with the Expansion Pak's memory. " % ", ".join(r["s"]["name"] for r in b["overWithPak"])
                    else:
                        pak = ("Over the game's own heap in %s. Build ROM and Export patch add the Expansion Pak game mod: its heap is %s KB larger, leaving at least %s KB free (%s). The ROM then needs the Expansion Pak. " % (
                            ", ".join(r["s"]["name"] for r in b["over"]), kb(b["extra"]), kb(b["leastWithPak"]["left"]), b["leastWithPak"]["s"]["name"]))
                else:
                    pak = ""
                n = len(b["rows"])
                self.budget("Tightest: %s, %.1f KB of %s KB free" % (t["s"]["name"], t["cost"] / 1024, kb(t["room"])) + (
                    " (%s KB with the Expansion Pak)" % kb(t["room"] + b["extra"]) if b["over"] and not b["overWithPak"] else ""),
                    t["ratio"], "%d level%s load %s or %s and grow by this much. " % (n, "" if n == 1 else "s", p["targetBank"], p["targetTexBank"]) + pak +
                    "Free heap is measured per level in the emulator, minus 16 KB for allocations during play (measured runs used 2–3 KB).",
                    over=bool(b["overWithPak"]))

    def budget(self, text, ratio, small, over=None):
        f = QFrame()
        f.setObjectName("budget")
        f.setProperty("over", "true" if (ratio > 1 if over is None else over) else "false")
        lay = QVBoxLayout(f)
        lay.setContentsMargins(8, 6, 8, 6)
        lay.addWidget(label(text))
        bar = QProgressBar()
        bar.setRange(0, 1000)
        bar.setValue(int(min(1, ratio) * 1000))
        bar.setTextVisible(False)
        bar.setFixedHeight(6)
        lay.addWidget(bar)
        lay.addWidget(label(small, "src"))
        self.imp_lay.addWidget(f)


# -------------------------------------------------------------------------------------------------------------------
# The Add palette

class Thumbs:
    """Model pictures for the palette, drawn one at a time while the window is idle, and kept"""

    def __init__(self, viewport):
        self.viewport = viewport
        self.done = {}
        self.queue = []
        self.timer = QTimer()
        self.timer.setInterval(0)
        self.timer.timeout.connect(self.step)

    def request(self, ed, oid, target):
        if oid in self.done:
            if self.done[oid] is not None:
                target.setPixmap(self.done[oid])
            return
        self.queue.append((ed, oid, target))
        if not self.timer.isActive():
            self.timer.start()

    def step(self):
        while self.queue:
            ed, oid, target = self.queue.pop(0)
            try:
                if not target.isVisible() and oid not in self.done:
                    pass
            except RuntimeError:   # the row is gone
                continue
            if oid not in self.done:
                model = ed.load_model(oid)
                img = self.viewport.thumbnail(model) if model is not None else None
                self.done[oid] = QPixmap.fromImage(img) if img is not None else None
            try:
                if self.done[oid] is not None:
                    target.setPixmap(self.done[oid])
            except RuntimeError:
                pass
            return
        self.timer.stop()


def thumb_label(thumbs, ed, oid):
    w = QLabel()
    w.setObjectName("thumb")
    w.setFixedSize(64, 64)
    w.setAlignment(Qt.AlignCenter)
    if oid and ed.model_url(oid):
        thumbs.request(ed, oid, w)
    return w


class Palette(QWidget):
    """Collectables, enemies, models from any level, the kit and camera zones"""
    import_model = Signal()

    def __init__(self, ed, viewport, jobs, window):
        super().__init__()
        self.ed, self.viewport, self.jobs, self.window = ed, viewport, jobs, window
        self.thumbs = Thumbs(viewport)
        lay = QVBoxLayout(self)
        lay.setContentsMargins(6, 6, 6, 6)
        self.tabs = QTabBar()
        for t in ("Collectables", "Enemies", "Models", "Kit", "Presets", "Camera"):
            self.tabs.addTab(t)
        self.tabs.currentChanged.connect(lambda _: self.render())
        lay.addWidget(self.tabs)
        self.scroll = QScrollArea()
        self.scroll.setWidgetResizable(True)
        self.scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarAlwaysOff)
        lay.addWidget(self.scroll, 1)
        lay.addWidget(label("New objects appear where the view’s centre meets the ground.", "muted"))
        self.query = ""
        self.bank = ""
        self.kit = None

    def tab(self):
        return ["collect", "enemies", "models", "kit", "presets", "camera"][self.tabs.currentIndex()]

    def render(self):
        ed = self.ed
        if ed.root is None:
            return
        body = QWidget()
        lay = QVBoxLayout(body)
        lay.setContentsMargins(4, 4, 4, 4)
        lay.setSpacing(6)
        t = self.tab()
        if t == "collect":
            self.collect(lay)
        elif t == "enemies":
            self.enemies(lay)
        elif t == "models":
            self.models(lay)
        elif t == "kit":
            self.kit_tab(lay)
        elif t == "presets":
            from .presets import PRESETS
            lay.addWidget(label("Ready-made platforms: rings, exits, lifts, spikes and more, with the "
                                "commands in the order the game reads them. Each one uses the game's own models.", "muted"))
            for k, (lab, note, model) in PRESETS.items():
                lay.addWidget(self.prow(model[0], lab, note, [("Add", None, lambda k=k: ed.add_preset(k), True)]))
        else:
            lay.addWidget(label("A camera zone is a box: while Glover is inside it, the camera moves as chosen, and it goes back when he leaves. "
                                "It is a puzzle like the shipped levels’ camera zones, so it needs no game mod. The zone expects the player to walk the way the view looks; change it in the inspector.", "muted"))
            for k, (lab, note, _) in CAM_PRESETS.items():
                lay.addWidget(self.prow(None, lab, note, [("Add", None, lambda k=k: ed.add_camera_zone(k), True)]))
        lay.addStretch(1)
        self.scroll.setWidget(body)

    def prow(self, oid, title, sub, buttons, extra=None, thumb=True):
        row = QFrame()
        row.setObjectName("prow")
        rl = QHBoxLayout(row)
        rl.setContentsMargins(4, 4, 4, 4)
        if thumb and oid is not False:
            rl.addWidget(thumb_label(self.thumbs, self.ed, oid) if not isinstance(oid, QLabel) else oid)
        mid = QVBoxLayout()
        mid.setSpacing(1)
        mid.addWidget(label(title, "b", rich="<" in title))
        if sub:
            mid.addWidget(label(sub, "muted"))
        if extra is not None:
            mid.addWidget(extra)
        rl.addLayout(mid, 1)
        bl = QVBoxLayout()
        bl.setSpacing(2)
        for text, tip, fn, enabled in buttons:
            b = QPushButton(text)
            if tip:
                b.setToolTip(tip)
            b.setEnabled(bool(enabled))
            b.clicked.connect(lambda _=False, fn=fn: fn())
            bl.addWidget(b)
        rl.addLayout(bl)
        return row

    def grid(self, lay, items, cols=2):
        w = QWidget()
        g = QGridLayout(w)
        g.setContentsMargins(0, 0, 0, 0)
        g.setSpacing(4)
        for i, b in enumerate(items):
            g.addWidget(b, i // cols, i % cols)
        lay.addWidget(w)

    def collect(self, lay):
        ed = self.ed
        lay.addWidget(label("GARIBS", "h4"))
        btns = []
        for t, l in (("garib", "✦ Garib"), ("bang_500pt", "! 500-point bang"), ("extra_life", "♥ Extra life"), ("mad_garib", "✸ Mad garib")):
            b = QPushButton(l)
            b.clicked.connect(lambda _=False, t=t: ed.add_garib(ed.placement_point() + vec.v3(0, 12, 0), t))
            btns.append(b)
        self.grid(lay, btns)
        lay.addWidget(label("Added to the selected garib’s group, or a new visible group.", "muted"))
        lay.addWidget(label("POWERUPS", "h4"))
        btns = []
        for name, v in ed.decomp["decompEnums"]["Pickup"].items():
            if v >= 15:
                continue
            oid = ed.powerup_model_id(v)
            b = QPushButton(name.lower() + ("  (import)" if ed.is_import(oid) else ""))
            b.setToolTip("model %s" % oid if oid else "no model")
            if oid and ed.model_url(oid):
                th = QLabel()
                self.thumbs.request(ed, oid, th)

                def set_icon(b=b, th=th):
                    if th.pixmap() is not None and not th.pixmap().isNull():
                        b.setIcon(th.pixmap())
                        b.setIconSize(QSize(32, 32))
                QTimer.singleShot(1500, set_icon)
            b.clicked.connect(lambda _=False, v=v, name=name: ed.add_powerup(v, name))
            btns.append(b)
        self.grid(lay, btns)
        lay.addWidget(label("OTHER", "h4"))
        b = QPushButton("? Mr. Tip")
        b.clicked.connect(ed.add_tip)
        self.grid(lay, [b])

    def enemies(self, lay):
        ed = self.ed
        lay.addWidget(label("A new enemy copies the shortest script that type has in a shipped level, with its patrol/target points moved to the new spot. Bosses are tied to their arenas and may not behave elsewhere.", "muted"))
        for v, name in ed.decomp["ksyEnums"]["enemy::enemy_type"].items():
            oid = ed.enemy_model_id(name)
            t = ed.templates.get(name)
            where = ("no model" if not oid else "in this level" if oid in ed.loaded else "imports from %s" % ed.catalog[oid]["bank"]
                     if oid in ed.catalog else "model missing")
            sub = where + (" · script from %s" % t["level"] if t else " · no shipped script") + (" · boss" if t and t.get("boss") else "")
            token = ed.decomp_enum_name("ActorType", int(v)) or ""
            lay.addWidget(self.prow(oid if oid and ed.model_url(oid) else None, "<b>%s</b> <span style='color:#c58bff; font-family:monospace'>%s</span>" % (name, token), sub,
                                    [("Add", None, lambda _=False, name=name: ed.add_enemy(name), oid and ed.model_url(oid))]))

    def models(self, lay):
        ed = self.ed
        head = QHBoxLayout()
        b = QPushButton("Import model…")
        b.setObjectName("accent")
        b.clicked.connect(self.import_model.emit)
        head.addWidget(b)
        head.addWidget(label("from another game or a 3D program (.glb)", "muted"))
        lay.addLayout(head)
        sets = []
        for c in ed.catalog.values():
            if c.get("custom") and c["custom"] not in sets:
                sets.append(c["custom"])
        if sets:
            lay.addWidget(label("YOUR IMPORTED MODELS", "h4"))
            for s in sets:
                pieces = ed.custom_pieces(s)
                c = pieces[0][1]
                kb = sum(x["bytes"] for _, x in pieces) / 1024
                lay.addWidget(self.prow(pieces[0][0], c["bank"].replace("Custom: ", ""), "%d piece%s · %.0f KB" % (len(pieces), "" if len(pieces) == 1 else "s", kb),
                                        [("Place", "Every piece as LandActor: solid scenery with collision", lambda _=False, s=s: ed.place_custom(s), True),
                                         ("Remove", "Remove this model from the editor", lambda _=False, s=s: self.window.remove_custom(s), True)]))
            lay.addWidget(label("ALL MODELS", "h4"))
        q = QLineEdit(self.query)
        q.setPlaceholderText("Search %s models by name, ID or bank…" % format(len(ed.catalog), ","))
        q.setClearButtonEnabled(True)
        bank = QComboBox()
        bank.addItem("All banks", "")
        bank.addItem("This level’s banks", "@level")
        for b_ in sorted({c["bank"] for c in ed.catalog.values()}):
            bank.addItem(b_, b_)
        i = bank.findData(self.bank)
        bank.setCurrentIndex(max(0, i))
        row = QHBoxLayout()
        row.addWidget(q, 1)
        row.addWidget(bank)
        lay.addLayout(row)
        holder = QWidget()
        hl = QVBoxLayout(holder)
        hl.setContentsMargins(0, 0, 0, 0)
        hl.setSpacing(4)
        lay.addWidget(holder)

        def draw():
            self.query, self.bank = q.text(), bank.currentData()
            clear(hl)
            term = q.text().lower()
            level_banks = {b_.split(".")[0] for b_ in ed.scene["sureObjectBanks"]}
            rows = [(i_, c) for i_, c in ed.catalog.items()
                    if (not self.bank or (c["bank"] in level_banks if self.bank == "@level" else c["bank"] == self.bank))
                    and (not term or term in ("%s %s %s" % (c["name"], i_, c["bank"])).lower())]
            rows.sort(key=lambda r: (r[1]["bank"] not in level_banks, r[1]["name"].lower()))
            for i_, c in rows[:150]:
                hl.addWidget(self.prow(i_, "<b>%s</b> <small>%s</small>" % (c["name"], i_),
                                       "%s · %.1f KB" % (c["bank"], c["bytes"] / 1024) + (" · in this level" if c["bank"] in level_banks else " · imported at build"),
                                       [("Scenery", "LandActor: solid scenery with collision", lambda _=False, i_=i_: ed.add_model(i_, "LandActor"), True),
                                        ("Background", "BackgroundActor: decoration without collision", lambda _=False, i_=i_: ed.add_model(i_, "BackgroundActor"), True),
                                        ("Platform", WHY["customPlatform"] if c.get("custom") else "Platform: solid, can move, spin and be scaled",
                                         lambda _=False, i_=i_: ed.add_model(i_, "Platform"), not c.get("custom"))]))
            if len(rows) > 150:
                hl.addWidget(label("%d more; narrow the search." % (len(rows) - 150), "muted"))
        timer = QTimer(q)
        timer.setSingleShot(True)
        timer.setInterval(250)
        timer.timeout.connect(draw)
        q.textChanged.connect(lambda _: timer.start())
        bank.currentIndexChanged.connect(lambda _: draw())
        draw()
        QTimer.singleShot(0, q.setFocus)

    def kit_tab(self, lay):
        """The kit: models made by code (tools/kit.py), some with settings. Placing one builds it and converts it like
        an imported model. It then shows under Models with the other imported models."""
        ed = self.ed
        if self.kit is None:
            try:
                self.kit = ed.backend.kit_items()
            except Exception:
                self.kit = []
        lay.addWidget(label("Props made by the editor, free to share. Background has no collision; Scenery is solid. "
                            "They are copied into the level’s banks at ROM build, like imported models.", "muted"))
        group = None
        for it in self.kit:
            if it["group"] != group:
                group = it["group"]
                lay.addWidget(label(group.upper(), "h4"))
            vals = {p["key"]: p["default"] for p in it["params"]}
            img = QLabel()
            img.setObjectName("thumb")
            img.setFixedSize(64, 64)
            img.setScaledContents(True)

            def draw(it=it, vals=vals, img=img):
                def done(png):
                    try:
                        pm = QPixmap()
                        pm.loadFromData(png)
                        img.setPixmap(pm)
                    except RuntimeError:
                        pass
                self.jobs.run(lambda: ed.backend.kit_preview(it["id"], dict(vals)), done, lambda e: None)
            params = QWidget()
            pl = QGridLayout(params)
            pl.setContentsMargins(0, 0, 0, 0)
            timer = QTimer(params)
            timer.setSingleShot(True)
            timer.setInterval(300)
            timer.timeout.connect(draw)
            for r, p in enumerate(it["params"]):
                if p["kind"] == "choice":
                    inp = QComboBox()
                    for i, l in enumerate(p["extra"]):
                        inp.addItem(l, str(i + 1))
                    inp.setCurrentIndex(max(0, int(p["default"]) - 1) if str(p["default"]).isdigit() else 0)
                    inp.currentIndexChanged.connect(lambda _, inp=inp, p=p, vals=vals, timer=timer: (vals.__setitem__(p["key"], inp.currentData()), timer.start()))
                else:
                    inp = QLineEdit(str(p["default"]))
                    if p.get("extra"):
                        inp.setToolTip(p["extra"])
                    inp.textChanged.connect(lambda v, p=p, vals=vals, timer=timer: (vals.__setitem__(p["key"], v), timer.start()))
                pl.addWidget(label(p["label"], "fieldname"), r, 0)
                pl.addWidget(inp, r, 1)
            lay.addWidget(self.prow(img, "<b>%s</b>" % it["name"], it["note"],
                                    [("Background", "BackgroundActor: decoration without collision", lambda _=False, it=it, vals=vals: self.window.place_kit(it, dict(vals), "BackgroundActor"), True),
                                     ("Scenery", "LandActor: solid, built from the model’s triangles", lambda _=False, it=it, vals=vals: self.window.place_kit(it, dict(vals), "LandActor"), True)],
                                    params if it["params"] else None))
            draw()


# -------------------------------------------------------------------------------------------------------------------
# The path tool's panel

class PathPanel(QWidget):
    def __init__(self, ed):
        super().__init__()
        self.ed = ed
        lay = QVBoxLayout(self)
        lay.setContentsMargins(8, 8, 8, 8)
        self.info = label("", "muted")
        lay.addWidget(self.info)
        w = QWidget()
        g = QGridLayout(w)
        g.setContentsMargins(0, 0, 0, 0)
        self.what = QComboBox()
        self.what.addItem("garibs", "garib")
        self.what.addItem("copies of the selection", "copy")
        self.garib = QComboBox()
        for v, l in (("garib", "garib"), ("bang_500pt", "500-point bang"), ("extra_life", "extra life"), ("mad_garib", "mad garib")):
            self.garib.addItem(l, v)
        self.step = QLineEdit("60")
        self.side = QComboBox()
        for v, l in ((0, "on the path"), (1, "right side"), (-1, "left side"), (2, "both sides")):
            self.side.addItem(l, v)
        self.off = QLineEdit("120")
        self.up = QLineEdit("12")
        self.turn = QCheckBox("one selected model, about y")
        rows = [("place", self.what), ("garib type", self.garib), ("every", self.step), ("where", self.side),
                ("to the side", self.off), ("over the ground", self.up), ("random turn", self.turn)]
        for r, (l, wd) in enumerate(rows):
            g.addWidget(label(l, "fieldname"), r, 0)
            g.addWidget(wd, r, 1)
        lay.addWidget(w)
        self.what.currentIndexChanged.connect(lambda _: self.up.setText("12" if self.what.currentData() == "garib" else "0"))
        lay.addWidget(label("Click the ground to add points. Backspace takes the last one off. Each copy is dropped onto the ground under it.", "note"))
        row = QHBoxLayout()
        c = QPushButton("Clear path")
        c.clicked.connect(self.clear_path)
        p = QPushButton("Place")
        p.setObjectName("primary")
        p.clicked.connect(self.place)
        row.addWidget(c)
        row.addStretch(1)
        row.addWidget(p)
        lay.addLayout(row)
        lay.addWidget(self.scatter_box())
        lay.addWidget(self.zone_box())
        lay.addStretch(1)
        ed.view_changed.connect(self.update_info)

    def pair(self, a, b):
        w = QWidget()
        h = QHBoxLayout(w)
        h.setContentsMargins(0, 0, 0, 0)
        edits = [QLineEdit(a), QLineEdit(b)]
        for e in edits:
            e.setObjectName("num")
            h.addWidget(e)
        def vals():
            v = (float(edits[0].text()), float(edits[1].text()))
            if not all(math.isfinite(x) for x in v):
                raise ValueError("not a finite number")
            return v
        w.vals = vals
        return w

    def scatter_box(self):
        """Scatter: copies of the selected scenery beside the path at random spacing, distance, scale and turn"""
        f = QFrame()
        f.setObjectName("card")
        lay = QVBoxLayout(f)
        lay.setContentsMargins(8, 6, 8, 8)
        lay.addWidget(label("Scatter the selection", "h3"))
        w = QWidget()
        g = QGridLayout(w)
        g.setContentsMargins(0, 0, 0, 0)
        self.sc_space = self.pair("80", "170")
        self.sc_dist = self.pair("180", "420")
        self.sc_scale = self.pair("0.8", "1.2")
        self.sc_dy = self.pair("-25", "15")
        self.sc_side = narrow(QComboBox())
        for v, l in (((1, -1), "both sides"), ((1,), "right side"), ((-1,), "left side")):
            self.sc_side.addItem(l, v)
        self.sc_turn = narrow(QComboBox())
        for v, l in (("free", "any way"), ("player", "towards the path"), ("path", "along the path")):
            self.sc_turn.addItem(l, v)
        self.sc_tilt = QCheckBox("tilt a little")
        self.sc_solid = QCheckBox("solid (LandActor)")
        self.sc_seed = QSpinBox()
        self.sc_seed.setRange(0, 99999)
        rows = [("spacing", self.sc_space), ("from the path", self.sc_dist), ("where", self.sc_side), ("scale", self.sc_scale),
                ("height", self.sc_dy), ("facing", self.sc_turn), ("", self.sc_tilt), ("", self.sc_solid), ("seed", self.sc_seed)]
        for r, (l, wd) in enumerate(rows):
            g.addWidget(label(l, "fieldname"), r, 0)
            g.addWidget(wd, r, 1)
        lay.addWidget(w)
        lay.addWidget(label("Select one or more landscape pieces or kit models; each copy picks one of them at random. "
                            "Every copy takes heap memory: the status line says how much.", "note"))
        row = QHBoxLayout()
        again = QPushButton("Try another")
        again.setToolTip("Undo the last scatter and scatter again with the next seed")
        again.clicked.connect(self.rescatter)
        go = QPushButton("Scatter")
        go.setObjectName("primary")
        go.clicked.connect(self.scatter)
        row.addWidget(again)
        row.addStretch(1)
        row.addWidget(go)
        lay.addLayout(row)
        self.scattered = None
        return f

    def scatter(self):
        try:
            args = dict(spacing=self.sc_space.vals(), dist=self.sc_dist.vals(), scale=self.sc_scale.vals(), dy=self.sc_dy.vals(),
                        sides=self.sc_side.currentData(), turn=self.sc_turn.currentData(), tilt=0.15 if self.sc_tilt.isChecked() else 0.0,
                        solid=self.sc_solid.isChecked(), seed=self.sc_seed.value())
        except ValueError:
            return self.ed.set_status("The scatter settings must be numbers.", "err")
        ed = self.ed
        top = ed.undo_stack[-1] if ed.undo_stack else None
        from .xmlutil import path_of
        sel = [path_of(e.node) for e in ed.selection if not e.is_handle and not e.derived]
        ed.scatter_along_path(**args)
        # The scatter's own undo entry and the level right after it: Try another undoes only that
        made = ed.undo_stack[-1] if ed.undo_stack and ed.undo_stack[-1] is not top else None
        self.scattered = (made, ed.serialize(), sel) if made is not None else None

    def rescatter(self):
        ed = self.ed
        if self.scattered and ed.undo_stack and ed.undo_stack[-1] is self.scattered[0] and ed.serialize() == self.scattered[1]:
            from .xmlutil import node_at
            paths = self.scattered[2]
            ed.undo()
            nodes = [node_at(ed.root, p_) for p_ in paths]
            ed.set_selection([e for e in ed.entities if not e.is_handle and not e.derived and any(e.node is n for n in nodes)])
        self.sc_seed.setValue(self.sc_seed.value() + 1)
        self.scatter()

    def zone_box(self):
        """Camera zones fitted to a stretch of the path"""
        from .entities import CAM_PRESETS
        f = QFrame()
        f.setObjectName("card")
        lay = QVBoxLayout(f)
        lay.setContentsMargins(8, 6, 8, 8)
        lay.addWidget(label("Camera zone over the path", "h3"))
        w = QWidget()
        g = QGridLayout(w)
        g.setContentsMargins(0, 0, 0, 0)
        self.zn_kind = narrow(QComboBox())
        for k, (lab, note, _) in CAM_PRESETS.items():
            self.zn_kind.addItem(lab, k)
            self.zn_kind.setItemData(self.zn_kind.count() - 1, note, Qt.ToolTipRole)
        self.zn_span = self.pair("0", "100")
        self.zn_split = QLineEdit("600")
        self.zn_split.setObjectName("num")
        for r, (l, wd) in enumerate([("camera", self.zn_kind), ("from, to %", self.zn_span), ("boxes up to", self.zn_split)]):
            g.addWidget(label(l, "fieldname"), r, 0)
            g.addWidget(wd, r, 1)
        lay.addWidget(w)
        lay.addWidget(label("Boxes 140 to each side of the path, 300 below and 400 above. The boxes line up with the axes, "
                            "so a diagonal stretch is split into shorter boxes.", "note"))
        b = QPushButton("Add zones")
        b.clicked.connect(self.add_zones)
        lay.addWidget(b)
        return f

    def add_zones(self):
        try:
            a, b = self.zn_span.vals()
            split = float(self.zn_split.text() or 0)
        except ValueError:
            return self.ed.set_status("The zone settings must be numbers.", "err")
        self.ed.path_zones(self.zn_kind.currentData(), a / 100, b / 100, split=split)

    def update_info(self):
        self.info.setText(self.ed.path_info())

    def clear_path(self):
        self.ed.path["pts"] = []
        self.ed.draw_path()

    def place(self):
        f = lambda w, d: float(w.text()) if w.text().strip() else d
        try:
            self.ed.place_along_path(self.what.currentData(), self.garib.currentData(), f(self.step, 60), self.side.currentData(),
                                     f(self.off, 0), f(self.up, 0), self.turn.isChecked())
        except ValueError:
            self.ed.set_status("The path settings must be numbers.", "err")
