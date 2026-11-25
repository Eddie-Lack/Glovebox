#!/usr/bin/env python3
"""Collects the licenses of the Rust crates and the fonts compiled into the app.

    python3 tools/licenses.py OUT [--target x86_64-pc-windows-msvc]

It asks `cargo metadata` for the crates the `glovebox` program links for the target: its dependencies and theirs.
Build and test dependencies are left out, and so are procedural macro crates: they run in the compiler and aren't
in the program.

OUT gets `CRATES.txt`, one entry per crate (name, version, the license as Cargo.toml states it, source, its files),
and each distinct license text once. A text that several crates share word for word, such as the Apache License,
is one file. A text of one crate keeps the crate's name. The workflow puts OUT into the release archive as
`licenses/`. Some crates (egui's among them) pack no license file; their entry names the license, and
`Apache-2.0.txt` holds that text. The tool fails when a crate states no license and has no license file.

It needs cargo and Python 3 only. cargo downloads the crates it lacks.
"""
import argparse, hashlib, json, os, re, shutil, subprocess, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
NAMES = re.compile(r"^(licen[cs]e|copying|copyright|notice|unlicense)|licen[cs]e", re.I)

ap = argparse.ArgumentParser()
ap.add_argument("out")
ap.add_argument("--target", default="x86_64-pc-windows-msvc")
a = ap.parse_args()

meta = json.loads(subprocess.run(["cargo", "metadata", "--format-version", "1", "--locked", "--filter-platform", a.target],
                                 cwd=ROOT, check=True, capture_output=True, text=True).stdout)
packages = {p["id"]: p for p in meta["packages"]}
nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
root = next(p["id"] for p in meta["packages"] if p["name"] == "glovebox")

# Normal dependencies only: a build or test dependency isn't part of the program, and neither is a procedural macro
def is_macro(pid):
    return any("proc-macro" in t["kind"] for t in packages[pid]["targets"])


seen, todo = set(), [root]
while todo:
    pid = todo.pop()
    if pid in seen or is_macro(pid):
        continue
    seen.add(pid)
    for dep in nodes[pid]["deps"]:
        if any(k["kind"] is None for k in dep["dep_kinds"]):
            todo.append(dep["pkg"])

# Every license file of every crate, grouped by its text (line ends and outer blank space aside)
crates, texts, missing = [], {}, []
for pid in sorted(seen, key=lambda i: (packages[i]["name"], packages[i]["version"])):
    p = packages[pid]
    if p["source"] is None:  # Glovebox's own crates
        continue
    src = os.path.dirname(p["manifest_path"])
    paths = [os.path.join(src, f) for f in sorted(os.listdir(src)) if NAMES.search(f) and os.path.isfile(os.path.join(src, f))]
    if p.get("license_file") and os.path.isfile(os.path.join(src, p["license_file"])):
        paths.append(os.path.join(src, p["license_file"]))
    fonts = os.path.join(src, "fonts")
    if os.path.isdir(fonts):
        paths += [os.path.join(fonts, f) for f in sorted(os.listdir(fonts)) if f.lower().endswith(".txt")]
    keys = []
    for path in dict.fromkeys(paths):
        text = open(path, encoding="utf-8", errors="replace").read().replace("\r\n", "\n").strip() + "\n"
        key = hashlib.sha256(text.encode()).hexdigest()
        texts.setdefault(key, {"text": text, "base": os.path.basename(path), "crates": []})["crates"].append(p)
        keys.append(key)
    if not p.get("license") and not keys:
        missing.append(p["name"])
    crates.append((p, keys))
if missing:
    sys.exit("no license found for: " + ", ".join(missing))

apache = next((k for k, t in texts.items() if "Apache License" in t["text"] and "Version 2.0, January 2004" in t["text"]
               and "END OF TERMS AND CONDITIONS" in t["text"] and len(t["text"]) < 12000), None)
if apache is None:
    sys.exit("no crate packs the Apache 2.0 text")

# File names: the Apache text is Apache-2.0.txt, a shared text is named after its kind, a crate's own after the crate
names, used = {}, set()
for key, t in sorted(texts.items(), key=lambda kv: (-len(kv[1]["crates"]), kv[1]["crates"][0]["name"], kv[1]["base"])):
    stem = re.sub(r"\.(txt|md)$", "", t["base"], flags=re.I)
    if key == apache:
        name = "Apache-2.0.txt"
    elif len(t["crates"]) > 1:
        name = "%s-%s.txt" % (stem, key[:8])
    else:
        name = "%s-%s-%s.txt" % (t["crates"][0]["name"], t["crates"][0]["version"], stem)
    assert name not in used, name
    used.add(name)
    names[key] = name

shutil.rmtree(a.out, ignore_errors=True)
os.makedirs(a.out)
for key, t in texts.items():
    with open(os.path.join(a.out, names[key]), "w", encoding="utf-8", newline="\n") as f:
        f.write(t["text"])
with open(os.path.join(a.out, "CRATES.txt"), "w", encoding="utf-8", newline="\n") as f:
    f.write("The Rust crates compiled into Glovebox.exe, with the license each one states and its license files in this folder.\n")
    f.write("A crate without files packs none: the license it names applies, and Apache-2.0.txt holds that text.\n\n")
    for p, keys in crates:
        f.write("%s %s\n  License: %s\n  Source: %s\n" % (p["name"], p["version"], p.get("license") or "see its files",
                                                       p.get("repository") or p.get("homepage") or "https://crates.io/crates/" + p["name"]))
        if keys:
            f.write("  Files: %s\n" % ", ".join(dict.fromkeys(names[k] for k in keys)))
        f.write("\n")
print("%d crates, %d files in %s" % (len(crates), len(texts) + 1, a.out))
