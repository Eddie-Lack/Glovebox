#!/usr/bin/env python3
"""Collects the licences of the Rust crates and the fonts compiled into the app.

    python3 tools/licenses.py OUT [--target x86_64-pc-windows-msvc]

It asks `cargo metadata` for the crates the `glovebox` program links for the target: its dependencies and theirs,
without build and test dependencies. For each crate it makes OUT/<name>-<version>/ with the crate's licence,
copying and notice files, the licence texts of the fonts in egui's font crate, and `crate.txt` (the name, the
version, the licence as Cargo.toml states it, and the source). The workflow puts OUT into the release archive as
`licenses/`. Some crates (egui's among them) pack no licence file. For those, `crate.txt` names the licence, and
OUT/Apache-2.0.txt holds the Apache 2.0 text, taken from a crate that packs it. The tool fails when a crate states
no licence and has no licence file.

It needs cargo and Python 3 only. cargo downloads the crates it lacks.
"""
import argparse, json, os, re, shutil, subprocess, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
NAMES = re.compile(r"^(licen[cs]e|copying|copyright|notice|unlicense|authors|patents)|licen[cs]e", re.I)

ap = argparse.ArgumentParser()
ap.add_argument("out")
ap.add_argument("--target", default="x86_64-pc-windows-msvc")
a = ap.parse_args()

meta = json.loads(subprocess.run(["cargo", "metadata", "--format-version", "1", "--locked", "--filter-platform", a.target],
                                 cwd=ROOT, check=True, capture_output=True, text=True).stdout)
packages = {p["id"]: p for p in meta["packages"]}
nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
root = next(p["id"] for p in meta["packages"] if p["name"] == "glovebox")

# Normal dependencies only: a build or test dependency is not part of the program
seen, todo = set(), [root]
while todo:
    pid = todo.pop()
    if pid in seen:
        continue
    seen.add(pid)
    for dep in nodes[pid]["deps"]:
        if any(k["kind"] is None for k in dep["dep_kinds"]):
            todo.append(dep["pkg"])

shutil.rmtree(a.out, ignore_errors=True)
os.makedirs(a.out)
count, missing, apache = 0, [], None
for pid in sorted(seen, key=lambda i: (packages[i]["name"], packages[i]["version"])):
    p = packages[pid]
    if p["source"] is None:  # Glovebox's own crates
        continue
    src = os.path.dirname(p["manifest_path"])
    dst = os.path.join(a.out, "%s-%s" % (p["name"], p["version"]))
    os.makedirs(dst)
    files = [f for f in sorted(os.listdir(src)) if NAMES.search(f) and os.path.isfile(os.path.join(src, f))]
    if p.get("license_file") and os.path.isfile(os.path.join(src, p["license_file"])):
        files.append(p["license_file"])
    for f in dict.fromkeys(files):
        shutil.copy(os.path.join(src, f), os.path.join(dst, os.path.basename(f)))
        if apache is None and "apache" in f.lower():
            text = open(os.path.join(src, f), encoding="utf-8", errors="replace").read()
            if "Apache License" in text and "Version 2.0, January 2004" in text and "END OF TERMS AND CONDITIONS" in text:
                apache = text
    fonts = os.path.join(src, "fonts")
    if os.path.isdir(fonts):
        for f in sorted(os.listdir(fonts)):
            if f.lower().endswith(".txt"):
                shutil.copy(os.path.join(fonts, f), os.path.join(dst, "fonts-" + f))
    if not p.get("license") and not files:
        missing.append(p["name"])
    with open(os.path.join(dst, "crate.txt"), "w", encoding="utf-8", newline="\n") as f:
        f.write("%s %s\nLicence: %s\nSource: %s\n" % (p["name"], p["version"], p.get("license") or "see the files in this folder",
                                                   p.get("repository") or p.get("homepage") or "https://crates.io/crates/" + p["name"]))
        if not files:
            f.write("The crate's package holds no licence file. The licence named above applies.\n")
            if "Apache-2.0" in (p.get("license") or ""):
                f.write("The text of the Apache License 2.0 is in Apache-2.0.txt, next to this folder.\n")
    count += 1
if apache is None:
    sys.exit("no crate packs the Apache 2.0 text")
with open(os.path.join(a.out, "Apache-2.0.txt"), "w", encoding="utf-8", newline="\n") as f:
    f.write(apache)
if missing:
    sys.exit("no licence found for: " + ", ".join(missing))
print("%d crates in %s" % (count, a.out))
