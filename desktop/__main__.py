"""Runs the desktop editor from a source checkout, after prepare.py (see README):

    python3 -m desktop                 the editor, on the folder prepare.py wrote (GLOVEBOX_HOME, or the checkout)
    python3 -m desktop --fixture       the editor on made-up levels, without a ROM (tools/desktop_fixture.py)

The desktop app (app/launcher.py) runs the same window after its first-run setup.
"""
import argparse, os, sys

HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def main(argv=None):
    ap = argparse.ArgumentParser(prog="python3 -m desktop")
    ap.add_argument("--fixture", action="store_true", help="made-up levels and models instead of the ROM's")
    ap.add_argument("--home", help="the folder for drafts and settings (default: GLOVEBOX_HOME or the checkout)")
    args = ap.parse_args(argv)
    home = os.path.abspath(args.home or os.environ.get("GLOVEBOX_HOME") or HERE)
    if args.fixture:
        sys.path.insert(0, os.path.join(HERE, "tools"))
        from desktop_fixture import FixtureBackend
        backend = FixtureBackend()
        if not args.home:
            import tempfile
            home = tempfile.mkdtemp(prefix="garib-fixture-")
    else:
        os.environ.setdefault("GLOVEBOX_HOME", home)
        if not os.path.isfile(os.path.join(os.environ["GLOVEBOX_HOME"], "config.json")):
            sys.exit("%s has no config.json: run prepare.py first (see the README), or try --fixture" % os.environ["GLOVEBOX_HOME"])
        from .backend import Backend
        backend = Backend()
    return run(backend, home)


def run(backend, home):
    from PySide6.QtWidgets import QApplication, QMessageBox
    from .appicon import qicon
    from .viewport import GL_HELP, gl_problem, prepare_qt
    from .window import MainWindow, STYLE
    if not QApplication.instance():
        prepare_qt()
    app = QApplication.instance() or QApplication(sys.argv)
    app.setApplicationName("Glovebox")
    app.setStyleSheet(STYLE)
    app.setWindowIcon(qicon())
    problem = gl_problem()
    if problem:
        QMessageBox.critical(None, "Glovebox", "%s\n\n%s" % (problem, GL_HELP))
        return 1
    os.makedirs(home, exist_ok=True)
    w = MainWindow(backend, home)
    w.start()
    return app.exec()


if __name__ == "__main__":
    sys.exit(main())
