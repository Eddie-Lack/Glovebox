#!/usr/bin/env python3
"""Boot every level (quickboot ROM, original level) in gloveworld's emulator harness and record the game's
free Jalloc heap after loading. Writes crates/core/data/heap.json, which the editor compiles in for its memory
budget.

    python3 tools/measure_heap.py --rom path/to/glover.z64 --gloveworld ../gloveworld [--levels 0a,0b]

Needs cargo (it builds the ROMs with gbtool, crates/core/examples/gbtool.rs) and gloveworld's tools/emu set up.
"""
import argparse, json, os, re, subprocess, sys, tempfile

HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ap = argparse.ArgumentParser()
ap.add_argument("--rom", required=True)
ap.add_argument("--gloveworld", required=True)
ap.add_argument("--levels")
ap.add_argument("--frame", type=int, default=700)
a = ap.parse_args()
a.rom = os.path.abspath(a.rom)


def gbtool(*args, **kw):
    cmd = ["cargo", "run", "-q", "--release", "--locked", "-p", "glovebox-core", "--example", "gbtool", "--"]
    return subprocess.run(cmd + list(args), cwd=HERE, check=True, **kw)


data = os.path.join(HERE, "crates", "core", "data")
out_path = os.path.join(data, "heap.json")
result = json.load(open(out_path)) if os.path.isfile(out_path) else {}
ids = a.levels.split(",") if a.levels else [s["id"] for s in json.load(open(os.path.join(data, "scenes.json")))]
for sid in ids:
    with tempfile.TemporaryDirectory() as tmp:
        # A new file name for each level: the emulator harness keeps data for a ROM by its name
        rom = os.path.join(tmp, "heap-%s.z64" % sid)
        xml = os.path.join(tmp, "level.xml")
        with open(xml, "wb") as f:
            f.write(gbtool("xml", a.rom, sid, stdout=subprocess.PIPE).stdout)
        gbtool("build", a.rom, sid, xml, rom, "--quickboot")
        env = dict(os.environ, SHOTS=str(a.frame))
        try:
            r = subprocess.run([sys.executable, os.path.join(a.gloveworld, "tools", "emu", "emu.py"),
                                os.path.join(HERE, "tools", "emu", "boot_shots.py"), "--rom", rom, "--out", tmp],
                               capture_output=True, text=True, timeout=300, env=env, cwd=a.gloveworld)
            m = re.search(r"heap size=(\d+) static=(\d+) dynamic=(\d+) free=(-?\d+)", r.stdout)
        except subprocess.TimeoutExpired:
            m = None
        if m:
            size, st, dy, free = map(int, m.groups())
            result[sid] = {"size": size, "static": st, "dynamic": dy, "free": free, "frame": a.frame}
            print(sid, "free", free, flush=True)
        else:
            print(sid, "no reading", flush=True)
    json.dump(result, open(out_path, "w"), indent=1)
