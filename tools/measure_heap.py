#!/usr/bin/env python3
"""Boot every level (quickboot ROM, original level) in gloveworld's emulator harness and record the game's
free Jalloc heap after loading. Writes web/heap.json, used by the editor's memory budget.

    python3 tools/measure_heap.py --gloveworld ../gloveworld [--levels 0a,0b]

Needs prepare.py to have run, and gloveworld's tools/emu set up.
"""
import argparse, json, os, re, subprocess, sys, tempfile

HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, HERE)
ap = argparse.ArgumentParser()
ap.add_argument("--gloveworld", required=True)
ap.add_argument("--levels")
ap.add_argument("--frame", type=int, default=700)
a = ap.parse_args()
import core  # noqa: E402 (reads config.json and the prepared tables)

out_path = os.path.join(HERE, "web", "heap.json")
result = json.load(open(out_path)) if os.path.isfile(out_path) else {}
ids = a.levels.split(",") if a.levels else list(core.SCENES)
for sid in ids:
    lev, _ = core.assemble(core.level_xml(sid))
    data, _ = core.build_rom(sid, lev, True)
    with tempfile.TemporaryDirectory() as tmp:
        # A new file name for each level: the emulator harness keeps data for a ROM by its name
        rom = os.path.join(tmp, "heap-%s.z64" % sid)
        open(rom, "wb").write(data)
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
