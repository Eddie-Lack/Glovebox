#!/usr/bin/env python3
"""Writes crates/core/data/rom_map.json from formats/map.0x8e6e01ff.0xccb4f948.yaml.

    python3 tools/gen_rom_map.py

The yaml file is libgarib's map of the retail ROM: where the code is, and which instructions hold the start and
the end of every bank, of the level files and of the audio data. The editor compiles the JSON in
(crates/core/src/rom.rs). Run this after a change to the yaml file. It needs PyYAML.

The regions keep the order of the yaml file. rom.rs sorts them by ROM offset with a stable sort, like libgarib.
"""
import json, os

import yaml

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SRC = os.path.join(ROOT, "formats", "map.0x8e6e01ff.0xccb4f948.yaml")
OUT = os.path.join(ROOT, "crates", "core", "data", "rom_map.json")

REGION_TYPES = {"static": ("start", "end"), "bounded_single": ("start_pointers", "end_pointers"),
                "bounded_array": ("start_pointers", "end_pointers"), "unbounded_single": ("start_pointers", "size")}
REGS = ("at v0 v1 a0 a1 a2 a3 t0 t1 t2 t3 t4 t5 t6 t7 s0 s1 s2 s3 s4 s5 s6 s7 t8 t9 gp sp fp ra").split()


def pointers(region, key):
    out = []
    for p in region[key]:
        if p["reg"].lower() not in REGS or not isinstance(p["address"], int):
            raise SystemExit("bad pointer in region %r: %r" % (region.get("name"), p))
        out.append({"address": p["address"], "reg": p["reg"].lower()})
    return out


def main():
    with open(SRC) as f:
        src = yaml.safe_load(f)
    regions = []
    for r in src["regions"]:
        kind = r["region_type"]
        if kind not in REGION_TYPES:
            raise SystemExit("unknown region type %r" % kind)
        if "namespaced_id" not in r and "name" not in r:
            raise SystemExit("a region has no key: %r" % r)
        out = {"data_type": r["data_type"], "region_type": kind}
        for key in ("namespaced_id", "name"):
            if key in r:
                out[key] = r[key]
        for key in REGION_TYPES[kind]:
            out[key] = pointers(r, key) if key.endswith("pointers") else r[key]
        regions.append(out)
    data = {"rom_checksum": src["rom_checksum"], "regions": regions}
    with open(OUT, "w", newline="\n") as f:
        f.write("{\n \"rom_checksum\": %s,\n \"regions\": [\n" % json.dumps(data["rom_checksum"]))
        f.write(",\n".join("  " + json.dumps(r) for r in regions))
        f.write("\n ]\n}\n")
    print("wrote %s (%d regions)" % (OUT, len(regions)))


if __name__ == "__main__":
    main()
