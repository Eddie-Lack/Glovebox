"""The desktop app's first-run window: the player chooses their ROM, the app checks it, keeps a copy and prepares the
levels (app/launcher.py's Setup runs prepare.py). Then the editor's window opens."""
from PySide6.QtCore import Qt, QTimer
from PySide6.QtWidgets import QFileDialog, QLabel, QPlainTextEdit, QPushButton, QVBoxLayout, QWidget


class SetupWindow(QWidget):
    def __init__(self, setup, rom_max, on_ready):
        super().__init__()
        self.setup, self.rom_max, self.on_ready = setup, rom_max, on_ready
        self.setWindowTitle("Glovebox: set up")
        self.resize(640, 520)
        lay = QVBoxLayout(self)
        lay.setContentsMargins(24, 20, 24, 20)
        title = QLabel("Glovebox")
        title.setObjectName("h2")
        lay.addWidget(title)
        text = QLabel("Choose your own Glover (USA) ROM (.z64, .v64 or .n64).\n\n"
                      "The app checks it and keeps a copy in its user folder:\n%s\n\n"
                      "Then it prepares the levels. The first start takes a few minutes, while it converts the game's "
                      "models. Afterwards it takes just a few seconds." % setup.home)
        text.setWordWrap(True)
        text.setTextInteractionFlags(Qt.TextSelectableByMouse)
        lay.addWidget(text)
        self.button = QPushButton("Choose the ROM…")
        self.button.setObjectName("primary")
        self.button.clicked.connect(self.choose)
        lay.addWidget(self.button, 0, Qt.AlignLeft)
        self.error = QLabel("")
        self.error.setWordWrap(True)
        self.error.setStyleSheet("color: #ff7b72")
        lay.addWidget(self.error)
        self.work = QLabel("Preparing the levels…")
        self.work.hide()
        lay.addWidget(self.work)
        self.log = QPlainTextEdit()
        self.log.setReadOnly(True)
        self.log.setObjectName("code")
        self.log.hide()
        lay.addWidget(self.log, 1)
        self.timer = QTimer(self)
        self.timer.setInterval(300)
        self.timer.timeout.connect(self.poll)

    def choose(self):
        path, _ = QFileDialog.getOpenFileName(self, "Choose your Glover (USA) ROM", "",
                                              "Nintendo 64 ROM (*.z64 *.v64 *.n64);;All files (*)")
        if not path:
            return
        try:
            with open(path, "rb") as f:
                error = self.setup.take_rom(f.read(self.rom_max + 1))
        except OSError as e:
            error = "The app could not read %s: %s" % (path, e)
        if error:
            self.error.setText(error)
            return
        self.start()

    def start(self):
        self.error.setText("")
        self.button.setEnabled(False)
        self.work.show()
        self.log.show()
        self.setup.start()
        self.timer.start()

    def poll(self):
        s = self.setup.status()
        self.log.setPlainText("\n".join(s["log"]))
        self.log.verticalScrollBar().setValue(self.log.verticalScrollBar().maximum())
        if s["state"] == "error":
            self.timer.stop()
            self.work.hide()
            self.button.setEnabled(True)
            self.error.setText("%s. Try the ROM again." % s["error"])
        elif s["state"] == "ready":
            self.timer.stop()
            self.close()
            self.on_ready()
