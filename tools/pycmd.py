"""The command line that runs a Python script in a child process, from source or from the desktop app.

From source, the child is the interpreter that is running now. In the desktop app (app/launcher.py, built with
PyInstaller), sys.executable is the app itself. The app runs the script given after --run-script and does not
understand interpreter flags, so they are left out there.
"""
import sys


def python(script, *args, flags=()):
    if getattr(sys, "frozen", False):
        return [sys.executable, "--run-script", script, *args]
    return [sys.executable, *flags, script, *args]
