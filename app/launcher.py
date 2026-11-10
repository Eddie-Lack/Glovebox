#!/usr/bin/env python3
"""The desktop app: the map editor in its own window (desktop/, Qt and OpenGL).

    Glovebox.exe [--home DIR] [--reset]      the app, built for Windows by tools/build_app.py
    python3 app/launcher.py [the same options]     the same, from a source checkout (any system)

On the first start the app asks for the ROM in a setup window. It checks it, keeps a copy in the user folder and
runs prepare.py. Then it opens the editor. When a new version of the app changes what prepare.py makes, the app runs
it again with the copy it kept.

The user folder (--home) holds everything the app writes: the ROM copy, the files split from it, the cache, the JSON
prepare.py makes, the autosaved drafts, the settings and the log (editor.log). It is %LOCALAPPDATA%\\Glovebox
on Windows. From source on macOS it is ~/Library/Application Support/Glovebox, and on Linux
$XDG_DATA_HOME/glovebox (by default ~/.local/share/glovebox). The app only uses a new or empty folder, or one it marked before
(glovebox-folder.txt). One copy of the app uses a folder at a time (editor.lock).

The built app is also the Python interpreter for the scripts the editor runs in child processes (libgarib's tools,
prepare.py): tools/pycmd.py starts them as `Glovebox --run-script SCRIPT ARGS`.

The app is a windowed program: it has no console. Its own sys.stdout and sys.stderr are None, so they go to editor.log.
The children it starts get pipes from their parent, and print into them as usual. Every error in a child is caught
and printed: an uncaught one would make the windowed program show a dialog and wait for a click.
"""
import argparse, hashlib, json, os, runpy, shutil, subprocess, sys, threading, traceback

FROZEN = getattr(sys, "frozen", False)
CODE = sys._MEIPASS if FROZEN else os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
APP = os.path.join(CODE, "app")
DECOMP = os.path.join(APP, "decomp")  # the gloveworld files prepare.py and core.py read (tools/build_app.py)
LIBGARIB_PY = os.path.join(CODE, "vendor", "libgarib", "python")
ROM_SHA1 = "270be17b3c8da9b88a7b99c2a545b0bce16837f4"  # Glover (USA), NGVE, big-endian (.z64)
ROM_MAX = 64 << 20
GENERATED = [os.path.join("web", n) for n in ("scenes.json", "noclip_tables.json", "enemy_templates.json", "decomp.json")]
sys.path.insert(0, os.path.join(CODE, "tools"))
from pycmd import python  # noqa: E402


def run_script(script, args):
    """--run-script: run a Python script as `python SCRIPT ARGS` would, in this process."""
    import warnings
    warnings.filterwarnings("ignore")
    sys.argv = [script, *args]
    # The frozen interpreter ignores PYTHONUNBUFFERED. The setup window shows prepare.py's lines as they come.
    for stream in (sys.stdout, sys.stderr):
        if stream:
            stream.reconfigure(line_buffering=True)
    sys.path.insert(0, os.path.dirname(os.path.abspath(script)))
    # libgarib's tools add libgarib to the path themselves only when they are not frozen
    # (_prefer_local_implementation.py).
    sys.path.insert(1, LIBGARIB_PY)
    try:
        runpy.run_path(script, run_name="__main__")
    except SystemExit:
        raise
    except BaseException:
        traceback.print_exc()   # into the parent's pipe
        return 1
    return 0


def default_home():
    if sys.platform == "win32":
        return os.path.join(os.environ.get("LOCALAPPDATA") or os.path.expanduser("~"), "Glovebox")
    if sys.platform == "darwin":
        return os.path.expanduser("~/Library/Application Support/Glovebox")
    return os.path.join(os.environ.get("XDG_DATA_HOME") or os.path.expanduser("~/.local/share"), "glovebox")


MARKER = "glovebox-folder.txt"


def claim_home(home):
    """The app deletes and rewrites what it made in the user folder, so it only uses a new or empty folder, or one it
    marked before. Returns an error message, or None."""
    if os.path.isfile(os.path.join(home, MARKER)):
        return None
    if os.path.isdir(home) and os.listdir(home):
        return ("The user folder %s already holds other files. The app only uses a new or empty folder (or one it "
                "made). Choose another --home." % home)
    os.makedirs(home, exist_ok=True)
    with open(os.path.join(home, MARKER), "w") as f:
        f.write("This folder belongs to Glovebox. The app deletes and rewrites the files it made here.\n")
    return None


def stamp():
    """A hash of the code and data prepare.py's output depends on. A new app version with a different hash prepares
    again."""
    paths = [os.path.join(CODE, p) for p in ("prepare.py", "tools/game_banks.py", "tools/build_xref.py",
                                             "vendor/noclip/src/Glover/scenes.ts", "vendor/noclip/src/Glover/enemy.ts",
                                             "vendor/noclip/src/Glover/tools/rom_split.py",
                                             "vendor/noclip/src/Glover/tools/file_offsets.yaml")]
    for top in (DECOMP, os.path.join(LIBGARIB_PY, "libgarib"), os.path.join(CODE, "vendor", "libgarib", "tools")):
        for root, dirs, files in os.walk(top):
            dirs[:] = sorted(d for d in dirs if d != "__pycache__")
            paths += [os.path.join(root, f) for f in sorted(files) if f.endswith((".py", ".rng", ".yaml", ".c", ".h",
                                                                                  ".txt", ".json"))]
    h = hashlib.sha1()
    for p in paths:
        h.update(os.path.relpath(p, CODE).replace("\\", "/").encode() + b"\0")
        with open(p, "rb") as f:
            h.update(hashlib.sha1(f.read()).digest())
    return h.hexdigest()


def read_json(path):
    try:
        with open(path) as f:
            return json.load(f)
    except (OSError, ValueError):
        return None


def to_z64(rom):
    """The ROM in big-endian byte order (.z64), from any of the three dump formats, or None."""
    head = rom[:4]
    if head == b"\x80\x37\x12\x40":
        return bytes(rom)
    out = bytearray(len(rom))
    if head == b"\x37\x80\x40\x12" and len(rom) % 2 == 0:  # .v64: 16-bit words swapped
        out[0::2], out[1::2] = rom[1::2], rom[0::2]
    elif head == b"\x40\x12\x37\x80" and len(rom) % 4 == 0:  # .n64: little-endian 32-bit words
        out[0::4], out[1::4], out[2::4], out[3::4] = rom[3::4], rom[2::4], rom[1::4], rom[0::4]
    else:
        return None
    return bytes(out)


class Setup:
    """The first-run state, shared by the setup window and the thread that runs prepare.py."""

    def __init__(self, home):
        self.home = home
        self.rom = os.path.join(home, "rom", "glover.z64")
        self.lock = threading.Lock()
        self.done = threading.Event()
        self.state, self.log, self.error = "need-rom", [], None

    def status(self):
        with self.lock:
            return {"state": self.state, "log": self.log[-200:], "error": self.error, "home": self.home}

    def take_rom(self, data):
        """Checks an uploaded ROM and keeps it. Returns an error message, or None."""
        if len(data) > ROM_MAX:
            return "That file is too big to be a Nintendo 64 ROM."
        rom = to_z64(data)
        if rom is None:
            return "That file is not a Nintendo 64 ROM (.z64, .v64 or .n64)."
        if hashlib.sha1(rom).hexdigest() != ROM_SHA1:
            return ("That ROM is not the one the editor supports. It needs Glover (USA), with SHA-1 %s once in .z64 "
                    "byte order. This one's is %s." % (ROM_SHA1, hashlib.sha1(rom).hexdigest()))
        os.makedirs(os.path.dirname(self.rom), exist_ok=True)
        with open(self.rom + ".tmp", "wb") as f:
            f.write(rom)
        os.replace(self.rom + ".tmp", self.rom)
        return None

    def start(self):
        with self.lock:
            if self.state == "working":
                return
            self.state, self.log, self.error = "working", [], None
        threading.Thread(target=self.prepare, daemon=True).start()

    def say(self, line):
        print(line, flush=True)
        with self.lock:
            self.log.append(line)

    def prepare(self):
        try:
            # What prepare.py made for another version goes, and the cache made from it. Nothing else in the folder.
            for d in (os.path.join("data", "Glover"), "cache"):
                shutil.rmtree(os.path.join(self.home, d), ignore_errors=True)
            for f in GENERATED + ["config.json", "prepared.json"]:
                if os.path.isfile(os.path.join(self.home, f)):
                    os.remove(os.path.join(self.home, f))
            self.say("Preparing the levels from your ROM.")
            env = {**os.environ, "GLOVEBOX_HOME": self.home, "PYTHONUNBUFFERED": "1"}
            cmd = python(os.path.join(CODE, "prepare.py"), "--rom", self.rom, "--gloveworld", DECOMP,
                         "--decomp-json", os.path.join(DECOMP, "decomp.json"))
            p = subprocess.Popen(cmd, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
                                 errors="replace")
            for line in p.stdout:
                self.say(line.rstrip())
            if p.wait() != 0:
                raise RuntimeError("prepare.py stopped with exit code %d" % p.returncode)
            with open(os.path.join(self.home, "prepared.json"), "w") as f:
                json.dump({"stamp": stamp()}, f)
            with self.lock:
                self.state = "ready"
            self.say("Ready.")
            self.done.set()
        except Exception as e:
            traceback.print_exc()
            with self.lock:
                self.state, self.error = "error", str(e)


def ready(home):
    config = read_json(os.path.join(home, "config.json")) or {}
    prepared = read_json(os.path.join(home, "prepared.json")) or {}
    return bool(config.get("rom") and os.path.isfile(config["rom"])
                and all(os.path.isfile(os.path.join(home, f)) for f in GENERATED)
                and prepared.get("stamp") == stamp())


def point_config_at_decomp(home):
    """config.json names the gloveworld folder by its path. In the app that is app/decomp/ in the unpack folder,
    which is a new folder at every start."""
    path = os.path.join(home, "config.json")
    config = read_json(path)
    if config is not None and config.get("gloveworld") != DECOMP:
        config["gloveworld"] = DECOMP
        with open(path + ".tmp", "w") as f:
            json.dump(config, f, indent=1)
        os.replace(path + ".tmp", path)


def run(args):
    home = os.path.abspath(args.home or default_home())
    error = claim_home(home)
    if error:
        return message(error)
    from PySide6.QtCore import QLockFile
    lock = QLockFile(os.path.join(home, "editor.lock"))
    lock.setStaleLockTime(0)
    if not lock.tryLock(100):
        # Before the log: a second copy must not empty the running copy's editor.log
        return message("The editor is already running with this user folder." + (
            " Close it first, then start the app with --reset again." if args.reset else ""), 0 if not args.reset else 1)
    log_to(os.path.join(home, "editor.log"))
    print("Glovebox")
    print("User folder: %s" % home)
    if args.reset:
        for f in ("prepared.json", os.path.join("rom", "glover.z64")):
            if os.path.isfile(os.path.join(home, f)):
                os.remove(os.path.join(home, f))
        print("Reset: the app asks for the ROM again.")
    app = application()
    from desktop.viewport import GL_HELP, gl_problem
    problem = gl_problem()
    if problem:
        return message("%s\n\n%s" % (problem, GL_HELP))
    windows = []

    def open_editor():
        point_config_at_decomp(home)
        os.environ["GLOVEBOX_HOME"] = home
        sys.path.insert(0, CODE)
        from desktop.backend import Backend
        from desktop.window import MainWindow
        w = MainWindow(Backend(), home)
        windows.append(w)
        w.start()

    if not ready(home):
        from desktop.setup import SetupWindow
        setup = Setup(home)
        w = SetupWindow(setup, ROM_MAX, open_editor)
        windows.append(w)
        w.show()
        if os.path.isfile(setup.rom):
            w.start()  # a new version of the app: prepare again from the ROM copy
    else:
        open_editor()
    code = app.exec()
    lock.unlock()
    return code


def application():
    """The Qt application, with the editor's look and an OpenGL 3.3 view"""
    sys.path.insert(0, CODE)
    from PySide6.QtWidgets import QApplication
    if QApplication.instance():
        return QApplication.instance()
    from desktop.appicon import qicon
    from desktop.viewport import prepare_qt
    from desktop.window import STYLE
    prepare_qt()
    app = QApplication(sys.argv)
    app.setApplicationName("Glovebox")
    app.setStyleSheet(STYLE)
    app.setWindowIcon(qicon())
    return app


def log_to(path):
    """The windowed app has no console: what it would print goes to a log in the user folder"""
    if sys.__stdout__ is not None and sys.__stderr__ is not None:
        return
    try:
        f = open(path, "w", encoding="utf-8", errors="replace", buffering=1)
    except OSError:
        return
    if sys.__stdout__ is None:
        sys.stdout = f
    if sys.__stderr__ is None:
        sys.stderr = f


def message(text, code=1):
    """Says something the player must see, in a window (and in the log)"""
    print(text)
    try:
        from PySide6.QtWidgets import QMessageBox
        application()
        QMessageBox.information(None, "Glovebox", text)
    except Exception:
        pass
    return code


def exit_code(code):
    """An exit code as Windows shows it (0xC0000005 for a crash) or as a number elsewhere."""
    return "0x%08X" % (code & 0xFFFFFFFF) if sys.platform == "win32" and (code < 0 or code > 255) else str(code)


def self_test():
    """Checks that the app holds everything the editor runs: the libraries, libgarib's tools and the schema."""
    import warnings
    warnings.filterwarnings("ignore")
    sys.path.insert(0, LIBGARIB_PY)
    import capstone, keystone, unicorn, lxml.etree, numpy, PIL.Image, pygltflib, jmespath, yaml  # noqa: F401
    import libgarib.rom  # noqa: F401  (creates capstone, keystone and unicorn engines at import)
    import libgarib.objects, libgarib.textures, libgarib.fla2  # noqa: F401
    from libgarib.levels import level_schema_path
    lxml.etree.RelaxNG(file=level_schema_path)
    print("libraries: ok")
    gui_check()
    tools = os.path.join(CODE, "vendor", "libgarib", "tools")
    for script in [os.path.join(tools, t) for t in ("objbank-tool.py", "texbank-tool.py", "rom-asset-tool.py")] + [
            os.path.join(CODE, "prepare.py")]:
        r = subprocess.run(python(script, "--help"), capture_output=True, text=True)
        if r.returncode != 0 or "usage" not in r.stdout.lower():
            print(r.stdout, r.stderr)
            raise SystemExit("%s --help failed" % os.path.basename(script))
        print("%s: ok" % os.path.basename(script))
    r = subprocess.run(python(os.path.join(APP, "engine_check.py")), capture_output=True, text=True)
    if r.returncode != 0 or "engines: ok" not in r.stdout:
        print(r.stdout, r.stderr)
        raise SystemExit("the ROM tool's engines failed (exit code %s)" % exit_code(r.returncode))
    print(r.stdout.strip())
    # Import model runs objbank-tool and texbank-tool on a converted glTF (tools/custom_model.py)
    sys.path.insert(0, os.path.join(CODE, "tools"))
    import custom_model, make_test_model, tempfile

    def run_tool(script, *args):
        r = subprocess.run(python(os.path.join(tools, script), *args), capture_output=True, text=True)
        if r.returncode != 0:
            print(r.stdout, r.stderr)
            raise SystemExit("%s failed while converting a model (exit code %s)" % (script, exit_code(r.returncode)))
    with tempfile.TemporaryDirectory() as tmp:
        meta = custom_model.convert(make_test_model.build(4), tmp, run_tool, "Test")
    print("model import: ok (%d triangles)" % meta["triangles"])
    # The kit's models are drawn with Pillow's own font and converted the same way (tools/kit.py)
    import kit
    with tempfile.TemporaryDirectory() as tmp:
        meta = custom_model.convert(kit.build("grave", kit.clean("grave", {"text": "RIP/TEST"})).glb(), tmp, run_tool, "Kit")
    kit.preview_png("pumpkin", kit.clean("pumpkin", {}))
    print("kit: ok (%d models, a gravestone of %d triangles)" % (len(kit.items()), meta["triangles"]))
    for name in ("core.py", "desktop/window.py", "tools/hw_check.py", "web/patcher.html", "web/heap.json", "mod/mod.bin",
                 "app/decomp/decomp.json", "vendor/libgarib/formats/map.0x8e6e01ff.0xccb4f948.yaml"):
        if not os.path.isfile(os.path.join(CODE, name)):
            raise SystemExit("missing from the app: %s" % name)
    print("files: ok")
    print("stamp: %s" % stamp())
    return 0


def gui_check():
    """The window's libraries: Qt with its platform plugins, PyOpenGL, and the editor's own modules. Qt runs without a
    screen here (the offscreen platform), so this also works where no one is logged in."""
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    sys.path.insert(0, CODE)
    from PySide6.QtCore import QLibraryInfo
    from PySide6.QtWidgets import QApplication
    import desktop.render  # noqa: F401  (before OpenGL.GL: it turns PyOpenGL's error checks off)
    import OpenGL.GL  # noqa: F401
    import desktop.window, desktop.backend, desktop.setup  # noqa: F401
    plugins = QLibraryInfo.path(QLibraryInfo.LibraryPath.PluginsPath)
    native = {"win32": "qwindows", "darwin": "qcocoa"}.get(sys.platform, "qxcb")
    found = [f for f in os.listdir(os.path.join(plugins, "platforms")) if f.startswith(("lib" + native, native))]
    if not found:
        raise SystemExit("Qt's %s platform plugin is missing from %s" % (native, plugins))
    QApplication.instance() or QApplication([sys.argv[0]])
    from desktop.viewport import Icons
    if Icons().texture("✦", "#f2c14e").rgba.shape != (64, 64, 4):
        raise SystemExit("drawing an icon failed")
    from desktop.appicon import qicon
    if qicon().isNull():
        raise SystemExit("the app's icon is empty")
    print("window: ok (Qt %s, %s)" % (QLibraryInfo.version().toString(), found[0]))


def main():
    if len(sys.argv) > 2 and sys.argv[1] == "--run-script":
        return run_script(sys.argv[2], sys.argv[3:])
    for name in ("stdout", "stderr"):
        if getattr(sys, name) is None:   # the windowed app, until run() opens the log
            setattr(sys, name, open(os.devnull, "w"))
    sys.stdout.reconfigure(line_buffering=True)
    ap = argparse.ArgumentParser(prog="Glovebox", description="The Glover map editor.")
    ap.add_argument("--home", help="the user folder (default: %s)" % default_home())
    ap.add_argument("--reset", action="store_true", help="forget the ROM and ask for it again")
    ap.add_argument("--self-test", action="store_true", help="check the app's contents and exit")
    args = ap.parse_args()
    if args.self_test:
        try:
            return self_test()
        except SystemExit:
            raise
        except BaseException:
            traceback.print_exc()
            return 1
    try:
        code = run(args)
    except KeyboardInterrupt:
        print("Stopped.")
        return 0
    except Exception:
        traceback.print_exc()
        code = message("The editor could not start:\n\n%s" % traceback.format_exc(limit=3))
    return code


if __name__ == "__main__":
    sys.exit(main())
