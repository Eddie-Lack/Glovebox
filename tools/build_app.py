#!/usr/bin/env python3
"""Build the desktop app for Windows: one .exe that runs the editor in its own window (app/launcher.py, desktop/), made
with PyInstaller.

    py tools/build_app.py                                    build the app, into build/app/ (Windows only)
    python3 tools/build_app.py snapshot --gloveworld DIR     refresh app/decomp/ from a gloveworld checkout
    python3 tools/build_app.py sources --out DIR             download the GPL engines' sources (for a release)

Install the build's libraries first: `pip install -r app/requirements.txt`. PyInstaller builds for the system it
runs on, and the app is made for Windows only, so the build stops elsewhere. `snapshot` and `sources` run anywhere.
.github/workflows/app.yml builds it on GitHub's Windows runner.

The build needs only this repository. It fetches vendor/ at the commits setup.sh pins and applies patches/ to
libgarib, as prepare.py does. app/decomp/ holds the few gloveworld files the editor reads: glover.yaml, the
levelInfo and worldInfo lines of symbol_addrs.txt, LoadObjectBank and LoadTextureBank from src/banks.c,
include/enums.h, and the cross-reference (decomp.json) built from the decomp's sources. None of them come from
the ROM.

The executable holds the editor's code and its libraries, and no game data: the player chooses their ROM the first
time the app starts. Staging copies an explicit list of files and stops on any file that looks like game data.
The code keeps the repository's layout inside the executable (core.py, desktop/, tools/, vendor/...), and runs
from those files. PyInstaller adds the Python interpreter and the libraries that code imports, Qt among them. Of web/
the app holds only the patcher page for players and the heap table.

Output: build/app/dist/Glovebox.exe, checked with --self-test, and build/app/Glovebox-windows-x64.zip with
the executable, app/README.txt and the licences of the libraries inside it. The .exe is a windowed program (no console
window), with the app's icon (desktop/appicon.py) and version details: the version comes from APP_VERSION (a tag like
v1.2.3, which the workflow passes) or is 0.0.0 for a build without one. The build leaves out readline and OpenSSL,
which the editor does not use.

Unicorn and Keystone are under the GPL, version 2. `sources` downloads their source packages from PyPI, at the
versions in app/requirements.txt. The release job attaches them to each release.
"""
import argparse, ast, compileall, glob, hashlib, importlib.metadata, importlib.util, os, platform, py_compile, re
import shutil, subprocess, sys, sysconfig, zipfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BUILD = os.path.join(ROOT, "build", "app")
STAGE = os.path.join(BUILD, "stage")
DECOMP = os.path.join(ROOT, "app", "decomp")
LIBGARIB = os.path.join(ROOT, "vendor", "libgarib")
NAME = "Glovebox"

# What the app runs, relative to the repository. Folders end in /.
REPO_FILES = ["core.py", "prepare.py", "desktop/", "tools/bps.py", "tools/mod_patch.py", "tools/game_banks.py",
              "tools/build_xref.py", "tools/pycmd.py", "tools/custom_model.py", "tools/make_test_model.py", "tools/kit.py",
              "tools/hw_check.py",
              "kit/", "mod/mod.bin", "mod/mod.json", "web/patcher.html", "web/heap.json", "app/engine_check.py",
              "app/decomp/"]
VENDOR_FILES = [
    "vendor/libgarib/LICENSE.md", "vendor/libgarib/python/libgarib/", "vendor/libgarib/tools/objbank-tool.py",
    "vendor/libgarib/tools/texbank-tool.py", "vendor/libgarib/tools/rom-asset-tool.py",
    "vendor/libgarib/tools/_prefer_local_implementation.py", "vendor/libgarib/formats/map.0x8e6e01ff.0xccb4f948.yaml",
    "vendor/noclip/LICENSE", "vendor/noclip/src/Glover/scenes.ts", "vendor/noclip/src/Glover/enemy.ts",
    "vendor/noclip/src/Glover/tools/rom_split.py", "vendor/noclip/src/Glover/tools/file_offsets.yaml",
]
# The folders on the app's import path at run time (core.py, prepare.py, app/launcher.py and tools/kit.py set them)
IMPORT_ROOTS = ["", "tools", "vendor/libgarib/python", "vendor/libgarib/tools", "vendor/noclip/src/Glover/tools",
                "kit"]
GAME_DATA = re.compile(r"\.(z64|v64|n64|rom|bps|ips|lev|fla|glb)$|\.(obj|tex)\.bin$", re.I)
GENERATED = {"web/scenes.json", "web/noclip_tables.json", "web/enemy_templates.json", "web/decomp.json",
             "config.json"}
# Modules the editor never uses: readline is GPL 3, and ssl and _hashlib bring OpenSSL (hashlib has its own SHA-1)
EXCLUDE = ["readline", "ssl", "_ssl", "_hashlib"]
GPL_SOURCES = ["unicorn", "keystone-engine"]


def run(cmd, **kw):
    print("+", " ".join(cmd), flush=True)
    subprocess.check_call(cmd, **kw)


def vendor():
    """vendor/ at the commits setup.sh pins, fetched the way setup.sh does."""
    pins = re.findall(r"^fetch (\S+) (\S+) ([0-9a-f]{40})$", open(os.path.join(ROOT, "setup.sh")).read(), re.M)
    assert pins, "no fetch lines in setup.sh"
    for url, rel, rev in pins:
        d = os.path.join(ROOT, rel)
        if not os.path.isdir(os.path.join(d, ".git")):
            run(["git", "init", "-q", d])
            run(["git", "-C", d, "remote", "add", "origin", url])
        head = subprocess.run(["git", "-C", d, "rev-parse", "-q", "--verify", "HEAD"], capture_output=True, text=True)
        if head.stdout.strip() != rev:
            run(["git", "-C", d, "fetch", "-q", "--depth", "1", "origin", rev])
            run(["git", "-C", d, "checkout", "-q", rev])
        print("%s @ %s" % (rel, rev[:7]))


def patch_libgarib():
    """patches/ applied to vendor/libgarib, as prepare.py applies them. Stops if a patch neither applies nor is
    applied already."""
    for patch in sorted(glob.glob(os.path.join(ROOT, "patches", "libgarib-*.patch"))):
        check = lambda *a: subprocess.run(["git", "-C", LIBGARIB, "apply", "--check", *a, patch],
                                          capture_output=True).returncode == 0
        if check("--reverse"):
            continue
        if not check():
            sys.exit("%s does not apply to vendor/libgarib" % os.path.basename(patch))
        run(["git", "-C", LIBGARIB, "apply", patch])


def walk(rel):
    for root, dirs, files in os.walk(os.path.join(ROOT, rel)):
        dirs[:] = [d for d in dirs if d != "__pycache__"]
        for f in files:
            yield os.path.relpath(os.path.join(root, f), ROOT).replace(os.sep, "/")


def stage():
    shutil.rmtree(STAGE, ignore_errors=True)
    files = []
    for rel in REPO_FILES + VENDOR_FILES:
        if not rel.endswith("/"):
            files.append(rel)
        else:
            files += list(walk(rel))
    for rel in files:
        if GAME_DATA.search(rel) or rel in GENERATED:
            sys.exit("refusing to put %s in the app: it looks like game data or a file made from the ROM" % rel)
        dst = os.path.join(STAGE, rel)
        os.makedirs(os.path.dirname(dst), exist_ok=True)
        shutil.copy2(os.path.join(ROOT, rel), dst)
    # libgarib finds its version with git at import time. The app's copy is not a checkout: write the version down.
    version = subprocess.run([sys.executable, "-W", "ignore", "-c",
                              "import sys; sys.path.insert(0, sys.argv[1]); import libgarib; print(libgarib.__version__)",
                              os.path.join(LIBGARIB, "python")], capture_output=True, text=True, check=True)
    with open(os.path.join(STAGE, "vendor", "libgarib", "python", "libgarib", "_static_version.py"), "w") as f:
        f.write("# Written by the editor's tools/build_app.py: the libgarib checkout the app was built from.\n"
                "version = %r\n" % version.stdout.strip())
    # Bytecode that Python uses without comparing it with the source's time stamp, which unpacking changes
    compileall.compile_dir(STAGE, quiet=1, invalidation_mode=py_compile.PycInvalidationMode.UNCHECKED_HASH)
    print("staged %d files" % len(files))


def hidden_imports():
    """Every module the staged code imports that is not part of the staged code: the standard library and the
    libraries from app/requirements.txt. PyInstaller follows their own imports."""
    local = set()
    for r in IMPORT_ROOTS:
        d = os.path.join(STAGE, r)
        for f in os.listdir(d):
            if f.endswith(".py") or os.path.isfile(os.path.join(d, f, "__init__.py")):
                local.add(f[:-3] if f.endswith(".py") else f)
    names = set()
    for root, dirs, files in os.walk(STAGE):
        for f in files:
            if not f.endswith(".py"):
                continue
            tree = ast.parse(open(os.path.join(root, f), "rb").read())
            for node in ast.walk(tree):
                if isinstance(node, ast.Import):
                    names.update(a.name for a in node.names)
                elif isinstance(node, ast.ImportFrom) and node.level == 0 and node.module:
                    names.add(node.module)
                    names.update("%s.%s" % (node.module, a.name) for a in node.names if a.name != "*")
    out = []
    for n in sorted(names):
        if n.split(".")[0] in local or n.split(".")[0] == "__future__":
            continue
        try:
            if importlib.util.find_spec(n) is not None:
                out.append(n)
        except (ImportError, ValueError, AttributeError):
            pass  # a name imported from a module, not a module
    return out


def pyinstaller():
    sep = os.pathsep
    icon, version = os.path.join(BUILD, "Glovebox.ico"), os.path.join(BUILD, "version.txt")
    sys.path.insert(0, ROOT)
    from desktop.appicon import write_ico
    write_ico(icon)
    with open(version, "w", encoding="utf-8") as f:
        f.write(version_info())
    # --windowed: no console window. The children the app starts for libgarib's tools get pipes from it
    # (app/launcher.py, tools/pycmd.py).
    cmd = [sys.executable, "-m", "PyInstaller", "--noconfirm", "--clean", "--onefile", "--windowed", "--name", NAME,
           "--icon", icon, "--version-file", version,
           "--distpath", os.path.join(BUILD, "dist"), "--workpath", os.path.join(BUILD, "work"),
           "--specpath", BUILD, "--add-data", STAGE + sep + ".", "--collect-binaries", "keystone",
           # PyOpenGL picks its platform module (WGL here) and array handlers at run time
           "--collect-submodules", "OpenGL"]
    for name in EXCLUDE:
        cmd += ["--exclude-module", name]
    # unicorn loads its library with ctypes from unicorn/lib, which PyInstaller doesn't find by itself
    unicorn = importlib.util.find_spec("unicorn").submodule_search_locations[0]
    for lib in glob.glob(os.path.join(unicorn, "lib", "*")):
        if not lib.endswith((".a", ".lib")):
            cmd += ["--add-binary", lib + sep + "unicorn/lib"]
    for name in hidden_imports():
        cmd += ["--hidden-import", name]
    run(cmd + [os.path.join(ROOT, "app", "launcher.py")])
    exe = os.path.join(BUILD, "dist", NAME + ".exe")
    clear_guard_cf(exe)
    # A windowed program started without pipes has no output: give it one, and show its report
    print("+", exe, "--self-test", flush=True)
    r = subprocess.run([exe, "--self-test"], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, errors="replace")
    print(r.stdout, flush=True)
    if r.returncode != 0:
        sys.exit("--self-test failed (exit code %d)" % r.returncode)
    return exe


def version_info():
    """The .exe's version details (Properties, Details), as PyInstaller's --version-file wants them"""
    tag = os.environ.get("APP_VERSION", "")
    m = re.fullmatch(r"v?(\d+)\.(\d+)\.(\d+)", tag)
    nums = tuple(int(x) for x in m.groups()) + (0,) if m else (0, 0, 0, 0)
    text = "%d.%d.%d" % nums[:3]
    return """VSVersionInfo(
  ffi=FixedFileInfo(filevers=%(n)r, prodvers=%(n)r, mask=0x3f, flags=0x0, OS=0x40004, fileType=0x1, subtype=0x0,
                    date=(0, 0)),
  kids=[StringFileInfo([StringTable('040904B0', [
    StringStruct('CompanyName', 'Glovebox'),
    StringStruct('FileDescription', 'Glovebox: a level editor for Glover (N64)'),
    StringStruct('FileVersion', %(t)r),
    StringStruct('InternalName', 'Glovebox'),
    StringStruct('OriginalFilename', 'Glovebox.exe'),
    StringStruct('ProductName', 'Glovebox'),
    StringStruct('ProductVersion', %(t)r)])]),
    VarFileInfo([VarStruct('Translation', [1033, 1200])])]
)
""" % {"n": nums, "t": text}


def clear_guard_cf(exe):
    """Clears IMAGE_DLLCHARACTERISTICS_GUARD_CF (0x4000) in the header of PyInstaller's Windows bootloader. With it,
    Windows enforces Control Flow Guard on the whole app, and Unicorn (libgarib's ROM tool), which jumps into code it
    generates, stops the process with 0xC0000409. python.exe does not set the flag."""
    import struct
    with open(exe, "r+b") as f:
        head = f.read(4096)
        at = struct.unpack_from("<I", head, 0x3C)[0] + 24 + 70  # PE header, optional header, DllCharacteristics
        flags = struct.unpack_from("<H", head, at)[0]
        f.seek(at)
        f.write(struct.pack("<H", flags & ~0x4000))
    print("Control Flow Guard: %s" % ("turned off" if flags & 0x4000 else "was off"))


def licences(out):
    """The licence files of Python, PyInstaller and every installed library (each in its own folder, at its path in
    the library), and the bundled editor parts."""
    os.makedirs(out, exist_ok=True)
    for dist in importlib.metadata.distributions():
        name = dist.metadata["Name"]
        if name.lower() in ("pip", "wheel"):
            continue
        for f in dist.files or []:
            rel = str(f).replace("\\", "/")
            base = os.path.basename(rel).upper()
            if rel.endswith((".py", ".pyc")) or not (base.startswith(("LICENSE", "LICENCE", "COPYING", "NOTICE", "AUTHORS"))
                                                     or ".dist-info/licenses/" in rel):
                continue
            src = dist.locate_file(f)
            if os.path.isfile(src):
                dst = os.path.join(out, name, *rel.split("/"))
                os.makedirs(os.path.dirname(dst), exist_ok=True)
                shutil.copy(src, dst)
    for p in (os.path.join(sysconfig.get_paths()["stdlib"], "LICENSE.txt"), os.path.join(sys.base_prefix, "LICENSE.txt")):
        if os.path.isfile(p):
            shutil.copy(p, os.path.join(out, "Python-LICENSE.txt"))
            break
    for src, name in (("vendor/libgarib/LICENSE.md", "libgarib-LICENSE.md"), ("vendor/noclip/LICENSE", "noclip-LICENSE")):
        if os.path.isfile(os.path.join(ROOT, src)):
            shutil.copy(os.path.join(ROOT, src), os.path.join(out, name))
    # The Unicorn and Keystone engines' own licence (GPL 2). Their wheels hold only the bindings' licence.
    for f in glob.glob(os.path.join(ROOT, "app", "licenses", "*")):
        shutil.copy(f, out)


def sources(out):
    """The source packages of the GPL engines from PyPI, at the versions in app/requirements.txt."""
    import json, urllib.request
    os.makedirs(out, exist_ok=True)
    req = open(os.path.join(ROOT, "app", "requirements.txt")).read()
    for name in GPL_SOURCES:
        version = re.search(r"^%s==([\w.]+)" % re.escape(name), req, re.M).group(1)
        with urllib.request.urlopen("https://pypi.org/pypi/%s/%s/json" % (name, version)) as r:
            sdist = [u for u in json.load(r)["urls"] if u["packagetype"] == "sdist"][0]
        path = os.path.join(out, sdist["filename"])
        urllib.request.urlretrieve(sdist["url"], path)
        if hashlib.sha256(open(path, "rb").read()).hexdigest() != sdist["digests"]["sha256"]:
            sys.exit("%s: checksum does not match PyPI's" % sdist["filename"])
        print(path)


def archive(exe):
    machine = platform.machine().lower()
    arch = {"amd64": "x64", "x86_64": "x64", "arm64": "arm64"}.get(machine, machine)
    folder = os.path.join(BUILD, "package", NAME)
    shutil.rmtree(os.path.dirname(folder), ignore_errors=True)
    os.makedirs(folder)
    shutil.copy2(exe, folder)
    shutil.copy(os.path.join(ROOT, "app", "README.txt"), folder)
    licences(os.path.join(folder, "licenses"))
    path = os.path.join(BUILD, "%s-windows-%s.zip" % (NAME, arch))
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
        for root, dirs, files in os.walk(folder):
            for f in files:
                full = os.path.join(root, f)
                z.write(full, os.path.relpath(full, os.path.dirname(folder)))
    print("app: %s (%d MB)" % (path, os.path.getsize(path) >> 20))
    return path


def snapshot(gloveworld):
    """app/decomp/ from a gloveworld checkout."""
    sys.path.insert(0, os.path.join(ROOT, "tools"))
    from game_banks import bank_names
    from build_xref import parse_enums
    vendor()
    patch_libgarib()
    shutil.rmtree(DECOMP, ignore_errors=True)
    os.makedirs(os.path.join(DECOMP, "src"))
    os.makedirs(os.path.join(DECOMP, "include"))
    rev = subprocess.run(["git", "-C", gloveworld, "rev-parse", "--short", "HEAD"], capture_output=True,
                         text=True).stdout.strip() or "unknown"
    shutil.copy(os.path.join(gloveworld, "glover.yaml"), DECOMP)
    shutil.copy(os.path.join(gloveworld, "include", "enums.h"), os.path.join(DECOMP, "include"))
    lines = [l for l in open(os.path.join(gloveworld, "symbol_addrs.txt"))
             if re.match(r"(levelInfo|worldInfo) = ", l)]
    assert len(lines) == 2, "levelInfo and worldInfo not found in symbol_addrs.txt"
    with open(os.path.join(DECOMP, "symbol_addrs.txt"), "w") as f:
        f.writelines(lines)
    src = open(os.path.join(gloveworld, "src", "banks.c")).read()
    parts = []
    for func in ("LoadObjectBank", "LoadTextureBank"):
        body = src[src.index("void %s(u32 num) {" % func):]
        parts.append(body[:body.index("\n}\n") + 3])
    with open(os.path.join(DECOMP, "src", "banks.c"), "w") as f:
        f.write("/* LoadObjectBank and LoadTextureBank from gloveworld's src/banks.c (%s), for tools/game_banks.py. */\n\n"
                % rev + "\n".join(parts))
    consts = lambda d: {k: v for e in parse_enums(os.path.join(d, "include", "enums.h")).values() for k, v in e.items()}
    for func in ("LoadObjectBank", "LoadTextureBank"):
        assert bank_names(src, func, consts(gloveworld)) == bank_names(
            open(os.path.join(DECOMP, "src", "banks.c")).read(), func, consts(DECOMP)), func
    run([sys.executable, os.path.join(ROOT, "tools", "build_xref.py"), "--gloveworld", gloveworld,
         "--libgarib", LIBGARIB, "--out", os.path.join(DECOMP, "decomp.json")])
    print("app/decomp from gloveworld %s" % rev)


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd")
    s = sub.add_parser("snapshot", help="refresh app/decomp/ from a gloveworld checkout")
    s.add_argument("--gloveworld", required=True)
    s = sub.add_parser("sources", help="download the GPL engines' source packages")
    s.add_argument("--out", required=True)
    a = ap.parse_args()
    if a.cmd == "snapshot":
        return snapshot(os.path.abspath(a.gloveworld))
    if a.cmd == "sources":
        return sources(a.out)
    if sys.platform != "win32":
        sys.exit("The app is made for Windows: build it on Windows (or let .github/workflows/app.yml build it). "
                 "To run the editor here, use `python3 -m desktop` from source.")
    vendor()
    patch_libgarib()
    stage()
    archive(pyinstaller())


if __name__ == "__main__":
    main()
