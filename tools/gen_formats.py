#!/usr/bin/env python3
"""Writes crates/core/data/level_format.json from formats/glover.lev.ksy.

    python3 tools/gen_formats.py            write the file
    python3 tools/gen_formats.py --check    fail when the file is out of date

The editor does not read .ksy files. It compiles in the JSON this tool writes. Run the tool after a change to
formats/glover.lev.ksy, and commit both files. It needs PyYAML.

formats/glover.lev.ksy is libgarib's description of the level file (github.com/naclomi/libgarib, commit cf87929, MIT
license), with Glovebox's change: SetActorRotation and SetActorScale may also sit in a Platform, Enemy, Powerup or
Water. The JSON holds two sets of nesting rules because libgarib's tools use two:

  - "declares", "modifies", "groups_into": from the changed file. libgarib's XML schema was made from these. They say
    which element may sit in which.
  - "disasm_declares", "disasm_modifies": from libgarib's own file. libgarib's level reader was made from that file,
    and Glovebox never made it again. They say where a command goes when a .lev file becomes XML.

The tool makes libgarib's own text by undoing Glovebox's change (UNDO below) and checks its SHA-1. The first 7 hex
digits of that SHA-1 are the "data-format-version" that libgarib writes on <Level>.
"""
import argparse
import hashlib
import json
import os
import re
import sys

import yaml

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
KSY = os.path.join(ROOT, "formats", "glover.lev.ksy")
OUT = os.path.join(ROOT, "crates", "core", "data", "level_format.json")

# What libgarib's _version.py reports for a checkout of commit cf87929 that has no version tag.
LIBGARIB_VERSION = "unknown+gcf87929"
UPSTREAM_SHA1 = "87197ff3fcc9dd9051f8f8345d61a946e81cee4e"

_NOTE = "      # PLATFORM, ENEMY, POWERUP and WATER need glovebox's game mod (mod/transforms.c)\n"
# (text in formats/glover.lev.ksy, libgarib's text, how many times it occurs)
UNDO = [
    (_NOTE + "      modifies: [LAND_ACTOR, BG_ACTOR, ANIMATED_BG_ACTOR, PLATFORM, ENEMY, POWERUP, WATER]\n",
     "      modifies: [LAND_ACTOR, BG_ACTOR, ANIMATED_BG_ACTOR]\n", 2),
    ("    # lol dear lord\n    -semantic:\n      declares: WATER\n", "    # lol dear lord\n", 1),
    ("  powerup:\n    -semantic:\n      declares: POWERUP\n", "  powerup:\n", 1),
]

PRIMITIVES = {"u1": 1, "u2": 2, "u4": 4, "s1": 1, "s2": 2, "s4": 4, "f4": 4}
FIELD_KEYS = {"id", "type", "enum", "size", "encoding", "size-eos", "-semantic", "doc"}
TYPE_KEYS = {"seq", "enums", "-semantic", "doc"}
SEMANTIC_KEYS = {"declares", "modifies", "groups-into"}


def die(msg):
    sys.exit("gen_formats.py: " + msg)


def to_upper_camel(string):
    """libgarib's ksy.to_upper_camel: the element name of a type"""
    parts = []
    for component in str(string).split("."):
        parts.append("".join(word.capitalize() for word in re.split(r"[\s_\-]+", component)))
    return ".".join(parts)


def as_list(v):
    if v is None:
        return []
    return list(v) if isinstance(v, list) else [v]


def semantic_of(type_name, type_def):
    sem = type_def.get("-semantic", {})
    if set(sem) - SEMANTIC_KEYS:
        die("type %s: unknown -semantic keys %s" % (type_name, sorted(set(sem) - SEMANTIC_KEYS)))
    declares = sem.get("declares")
    if declares is not None and not isinstance(declares, str):
        die("type %s: 'declares' must be one name" % type_name)
    return {"declares": declares, "modifies": [str(m) for m in as_list(sem.get("modifies"))],
            "groups_into": sem.get("groups-into")}


def resolve_enum(ksy, type_name, ref):
    if "::" in ref:
        owner, name = ref.split("::")
    else:
        owner, name = type_name, ref
    if name not in ksy["types"].get(owner, {}).get("enums", {}):
        die("type %s: enum %s not found" % (type_name, ref))
    return "%s::%s" % (owner, name)


def convert(ksy):
    types = ksy["types"]
    meta = ksy["meta"]
    if meta.get("endian") != "be":
        die("the format must be big-endian")
    top = ksy["seq"]
    expected_top = [{"id": "length", "type": "u4"},
                    {"id": "name", "type": "str", "encoding": "ASCII", "terminator": 0},
                    {"id": "body", "repeat": "eos", "type": "cmd"}]
    if top != expected_top:
        die("the file's own seq changed; level.rs reads length, name and commands by hand")

    enums = {}
    for type_name, type_def in types.items():
        for enum_name, members in type_def.get("enums", {}).items():
            pairs = []
            for value, name in members.items():
                if not isinstance(value, int) or not isinstance(name, str):
                    die("enum %s::%s: %r: %r is not an integer and a name" % (type_name, enum_name, value, name))
                pairs.append([name, value])
            if len({p[0] for p in pairs}) != len(pairs):
                die("enum %s::%s has a name twice" % (type_name, enum_name))
            enums["%s::%s" % (type_name, enum_name)] = pairs

    out_types = []
    tags = {}
    for type_name, type_def in types.items():
        if set(type_def) - TYPE_KEYS:
            die("type %s: unknown keys %s" % (type_name, sorted(set(type_def) - TYPE_KEYS)))
        tag = to_upper_camel(type_name)
        if tag in tags:
            die("types %s and %s have the same element name" % (tags[tag], type_name))
        tags[tag] = type_name
        fields = []
        seen = set()
        for f in type_def["seq"]:
            if set(f) - FIELD_KEYS:
                die("type %s field %s: unknown keys %s" % (type_name, f.get("id"), sorted(set(f) - FIELD_KEYS)))
            fid = f["id"]
            if fid in seen:
                die("type %s has field %s twice" % (type_name, fid))
            seen.add(fid)
            ftype = f.get("type")
            field = {"id": fid}
            if ftype is None:
                if f.get("size-eos") is not True:
                    die("type %s field %s: no type" % (type_name, fid))
                field["type"] = "bytes_eos"
            elif isinstance(ftype, dict):
                if set(ftype) != {"switch-on", "cases"}:
                    die("type %s field %s: unknown switch keys" % (type_name, fid))
                on = ftype["switch-on"]
                on_field = next((x for x in fields if x["id"] == on), None)
                if on_field is None or on_field["type"] not in ("u1", "u2", "u4") or "enum" in on_field:
                    die("type %s field %s: the switch must follow its unsigned integer field %s" % (type_name, fid, on))
                cases, default = [], None
                for code, case_type in ftype["cases"].items():
                    if case_type not in types:
                        die("type %s field %s: unknown case type %s" % (type_name, fid, case_type))
                    if code == "_":
                        default = case_type
                    elif isinstance(code, int):
                        cases.append([code, case_type])
                    else:
                        die("type %s field %s: case %r is not an integer" % (type_name, fid, code))
                field.update({"type": "switch", "on": on, "cases": cases, "default": default})
            elif ftype in PRIMITIVES:
                field["type"] = ftype
                if "enum" in f:
                    field["enum"] = resolve_enum(ksy, type_name, f["enum"])
            elif ftype == "str":
                if f.get("encoding") != "ASCII" or not isinstance(f.get("size"), int):
                    die("type %s field %s: a string needs a size and ASCII" % (type_name, fid))
                field.update({"type": "str", "size": f["size"]})
            elif ftype in types:
                field.update({"type": "struct", "struct": ftype})
            else:
                die("type %s field %s: unknown type %s" % (type_name, fid, ftype))
            if field["type"] not in PRIMITIVES and "enum" in f:
                die("type %s field %s: an enum needs an integer type" % (type_name, fid))
            fsem = f.get("-semantic", {})
            if "hash-namespace" in fsem:
                field["hash_namespace"] = str(fsem["hash-namespace"])
            if "tag-namespace" in fsem:
                field["tag_namespace"] = str(fsem["tag-namespace"])
            fields.append(field)
        if sum(1 for x in fields if x["type"] == "switch") > 1:
            die("type %s has two switches; level.rs handles one" % type_name)
        entry = {"id": type_name, "tag": tag, "fields": fields}
        entry.update(semantic_of(type_name, type_def))
        group = entry["groups_into"]
        entry["group_tag"] = to_upper_camel(group) if group is not None else None
        out_types.append(entry)

    by_id = {t["id"]: t for t in out_types}
    declared = {t["declares"] for t in out_types if t["declares"]}
    for t in out_types:
        for m in t["modifies"]:
            if m not in declared:
                die("type %s modifies %s, which no type declares" % (t["id"], m))
        if t["groups_into"] is not None:
            # libgarib's schema generator refuses other shapes
            if len(t["fields"]) != 1 or t["fields"][0]["type"] != "struct":
                die("type %s groups into an element but is not one struct field" % t["id"])
        elif t["id"] != "cmd" and any(f["type"] == "struct" for f in t["fields"]):
            die("type %s has a struct field outside a group; level.rs does not check that shape" % t["id"])

        for f in t["fields"]:
            # libgarib pads or cuts a string given for a struct of a known size. No such struct exists today.
            if f["type"] == "struct" and not any(x["type"] == "switch" for x in by_id[f["struct"]]["fields"]):
                die("type %s field %s: level.rs expects a struct field's type to hold a switch" % (t["id"], f["id"]))
    group_tags = {t["group_tag"] for t in out_types if t["group_tag"]}
    if group_tags & set(tags):
        die("a group element has the name of a type: %s" % sorted(group_tags & set(tags)))

    cmd = by_id["cmd"]
    if [f["id"] for f in cmd["fields"]] != ["type_code", "params"] or cmd["fields"][1]["type"] != "switch":
        die("type cmd changed")
    opcodes = cmd["fields"][1]["cases"]
    if len({c[1] for c in opcodes}) != len(opcodes) or len({c[0] for c in opcodes}) != len(opcodes):
        die("two commands share a code or a type")
    return {"types": out_types, "enums": enums, "opcodes": opcodes}


def build():
    with open(KSY, "r", encoding="utf-8", newline="") as f:
        text = f.read()
    # libgarib hashes the text as Python reads it in text mode
    text = text.replace("\r\n", "\n")
    upstream = text
    for ours, theirs, count in UNDO:
        if upstream.count(ours) != count:
            die("formats/glover.lev.ksy no longer holds Glovebox's change as UNDO lists it; update UNDO")
        upstream = upstream.replace(ours, theirs)
    upstream_sha1 = hashlib.sha1(upstream.encode("utf-8")).hexdigest()
    if upstream_sha1 != UPSTREAM_SHA1:
        die("undoing Glovebox's change does not give libgarib's file (SHA-1 %s). A change to the format other than\n"
            "the nesting rules in UNDO also changes what a .lev file reads as. Update UNDO, or decide on a new\n"
            "data-format-version and change this check." % upstream_sha1)

    ours = convert(yaml.safe_load(text))
    theirs = convert(yaml.safe_load(upstream))
    theirs_by_id = {t["id"]: t for t in theirs["types"]}
    for t in ours["types"]:
        u = theirs_by_id[t["id"]]
        if (t["fields"], t["groups_into"]) != (u["fields"], u["groups_into"]):
            die("type %s differs from libgarib's in more than its nesting" % t["id"])
        t["disasm_declares"] = u["declares"]
        t["disasm_modifies"] = u["modifies"]
    if (ours["enums"], ours["opcodes"]) != (theirs["enums"], theirs["opcodes"]):
        die("enums or command codes differ from libgarib's")

    doc = {
        "comment": "Made by tools/gen_formats.py from formats/glover.lev.ksy. Do not edit.",
        "ksy_sha1": hashlib.sha1(text.encode("utf-8")).hexdigest(),
        "libgarib_version": LIBGARIB_VERSION,
        "data_format_version": upstream_sha1[:7],
        "hex_cutoff": 10000,
        "opcodes": ours["opcodes"],
        "enums": ours["enums"],
        "types": ours["types"],
    }
    return json.dumps(doc, indent=1) + "\n"


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--check", action="store_true", help="fail when level_format.json is out of date")
    args = ap.parse_args()
    text = build()
    if args.check:
        if not os.path.isfile(OUT) or open(OUT, encoding="utf-8").read() != text:
            die("crates/core/data/level_format.json is out of date; run tools/gen_formats.py")
        return
    with open(OUT, "w", encoding="utf-8", newline="\n") as f:
        f.write(text)


if __name__ == "__main__":
    main()
