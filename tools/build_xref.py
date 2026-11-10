#!/usr/bin/env python3
"""Build web/decomp.json: a cross-reference between libgarib's level format and the gloveworld decompilation.

For every level command libgarib knows (an XML element such as <PlatSetInitialPos>), this records:
  - the opcode and the ksy type it comes from (libgarib/formats/glover.lev.ksy)
  - its field list and types
  - the decomp's name for the opcode (enum Token in gloveworld/include/enums.h)
  - where the game's parser handles it (file, line, function, and the C snippet)
It also exports the decomp enums the editor uses to label numeric fields (enemy types, pickups,
puzzle conditions/actions, enemy instructions, level IDs).

    python3 tools/build_xref.py --gloveworld ../gloveworld --libgarib vendor/libgarib --out web/decomp.json
"""
import argparse, json, os, re, sys

import yaml


def to_upper_camel(s):  # same as libgarib.ksy.to_upper_camel
    return "".join(w.capitalize() for w in re.split(r"[\s_\-]+", str(s)))


def parse_enums(path):
    src = open(path).read()
    src = re.sub(r"//[^\n]*", "", src)
    enums = {}
    for m in re.finditer(r"enum\s+(\w+)\s*\{(.*?)\};", src, re.S):
        name, body = m.group(1), m.group(2)
        val, members = -1, {}
        for item in body.split(","):
            item = item.strip()
            if not item:
                continue
            if "=" in item:
                k, v = (x.strip() for x in item.split("=", 1))
                try:
                    val = int(v, 0)
                except ValueError:
                    val = members.get(v, val + 1)
            else:
                k, val = item, val + 1
            members[k] = val
        enums[name] = members
    return enums


FUNC_RE = re.compile(r"^(?:static\s+|inline\s+)*[A-Za-z_][\w\s\*]*?\b(\w+)\s*\([^;]*\)\s*\{\s*$")


def index_sources(src_dir):
    """Map TOKEN_* name -> list of {file, line, func, snippet}."""
    hits = {}
    for fn in sorted(os.listdir(src_dir)):
        if not fn.endswith(".c"):
            continue
        lines = open(os.path.join(src_dir, fn), errors="replace").read().split("\n")
        func = None
        for i, line in enumerate(lines):
            fm = FUNC_RE.match(line)
            if fm and not line.startswith(" "):
                func = fm.group(1)
            for tok in re.findall(r"\bTOKEN_\w+", line):
                # Only record places that dispatch on the token, not every mention.
                if not re.search(r"(token|type)\s*==\s*" + tok + r"\b|case\s+" + tok + r"\b", line):
                    continue
                end = i + 1
                depth = line.count("{") - line.count("}")
                while end < len(lines) and end - i < 60:
                    l = lines[end]
                    depth += l.count("{") - l.count("}")
                    end += 1
                    if depth <= 0 and ("return TRUE" in l or l.strip() in ("}", "break;") or depth < 0):
                        break
                hits.setdefault(tok, []).append({
                    "file": "src/" + fn, "line": i + 1, "func": func,
                    "snippet": "\n".join(lines[i:end]),
                })
    return hits


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--gloveworld", required=True)
    ap.add_argument("--libgarib", required=True)
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    ksy = yaml.safe_load(open(os.path.join(args.libgarib, "formats", "glover.lev.ksy")))
    enums = parse_enums(os.path.join(args.gloveworld, "include", "enums.h"))
    tokens_by_val = {v: k for k, v in enums["Token"].items()}
    handlers = index_sources(os.path.join(args.gloveworld, "src"))
    try:
        rev = os.popen("git -C %s rev-parse --short HEAD" % args.gloveworld).read().strip()
    except Exception:
        rev = ""

    types = ksy["types"]
    # Top-level opcode switch: cmd.params switch-on type_code
    top_cases = None
    for t in types.values():
        for a in t.get("seq", []):
            at = a.get("type")
            if isinstance(at, dict) and at.get("switch-on") == "type_code":
                top_cases = at["cases"]
    commands = {}
    for code, tname in top_cases.items():
        if tname in ("unknown",) or not isinstance(code, int):
            continue
        tdef = types.get(tname, {})
        sem = tdef.get("-semantic", {})
        fields = []
        for a in tdef.get("seq", []):
            at = a.get("type", "bytes[%s]" % a.get("size", "?"))
            fields.append({"id": a["id"], "type": at if isinstance(at, str) else "switch",
                           "enum": a.get("enum")})
        tok = tokens_by_val.get(code)
        commands[to_upper_camel(tname)] = {
            "opcode": code, "ksy": tname, "fields": fields,
            "declares": sem.get("declares"), "modifies": sem.get("modifies"),
            "groupsInto": to_upper_camel(sem["groups-into"]) if "groups-into" in sem else None,
            # parse.c is where level commands are read; other matches are later uses of the same token value
            "token": tok, "handlers": sorted(handlers.get(tok, []) if tok else [], key=lambda h: h["file"] != "src/parse.c"),
        }

    # Sub-switches whose numbering lines up with a decomp enum (offsets found by comparing parse.c with the ksy)
    def sub_switch(type_name, field):
        for a in types[type_name]["seq"]:
            at = a.get("type")
            if isinstance(at, dict) and at.get("switch-on") == field:
                return {int(k): to_upper_camel(v) for k, v in at["cases"].items() if isinstance(k, int)}
        return {}

    sub = {
        "PuzzleCond": {"field": "cond_type", "enum": "PuzzleCondType", "offset": enums["Token"]["TOKEN_CONDITION_AT_POINT"],
                       "cases": sub_switch("puzzle_cond", "cond_type")},
        "PuzzleAction": {"field": "action_type", "enum": "PuzzleActionType", "offset": enums["Token"]["TOKEN_ACTION_START_MOVE"],
                         "cases": sub_switch("puzzle_action", "action_type")},
        "EnemyInstruction": {"field": "instr_type", "enum": "NmeInstruction", "offset": 0,
                             "cases": sub_switch("enemy_instruction", "instr_type")},
    }

    # Every ksy type (including nested ones such as enemy_instruction or puzzle_cond_* bodies) for field types
    all_types = {}
    for tname, tdef in types.items():
        fields = []
        for a in tdef.get("seq", []) or []:
            at = a.get("type", "bytes[%s]" % a.get("size", "?"))
            fields.append({"id": a["id"], "type": at if isinstance(at, str) else "switch", "enum": a.get("enum")})
        all_types[to_upper_camel(tname)] = {"ksy": tname, "fields": fields}

    # The game's own level-script diagnostics (PRINTF warnings and Crash() calls), for the editor's checks
    diagnostics = []
    src_dir = os.path.join(args.gloveworld, "src")
    for fn in sorted(os.listdir(src_dir)):
        if not fn.endswith(".c"):
            continue
        for i, line in enumerate(open(os.path.join(src_dir, fn), errors="replace")):
            m = re.search(r'(PRINTF|Crash)\("((?:WARNING|Error|ERROR)?[^"]*)"', line)
            if m and (m.group(1) == "Crash" or re.match(r"(WARNING|Error|ERROR)", m.group(2))):
                diagnostics.append({"file": "src/" + fn, "line": i + 1, "kind": m.group(1), "text": m.group(2).strip()})

    ksy_enums = {}
    for tname, tdef in types.items():
        for ename, members in (tdef.get("enums") or {}).items():
            ksy_enums[tname + "::" + ename] = {str(k): v for k, v in members.items()}

    out = {
        "source": {"gloveworld": rev, "enumsFile": "include/enums.h"},
        "commands": commands,
        "types": all_types,
        "diagnostics": diagnostics,
        "subSwitches": sub,
        "decompEnums": {k: enums[k] for k in ("ActorType", "Pickup", "Level", "NmeInstruction", "NmeCondition",
                                              "PuzzleCondType", "PuzzleActionType", "CameoInstruction", "ExitType",
                                              "World", "Token") if k in enums},
        "ksyEnums": ksy_enums,
    }
    os.makedirs(os.path.dirname(os.path.abspath(args.out)), exist_ok=True)
    json.dump(out, open(args.out, "w"), indent=1)
    named = sum(1 for c in commands.values() if c["token"])
    handled = sum(1 for c in commands.values() if c["handlers"])
    unk = [n for n, c in commands.items() if re.search(r"0x[0-9a-f]+|Todo", n, re.I) and c["token"]]
    print("%d commands, %d named by the decomp, %d with a parser location; %d libgarib placeholder names resolved"
          % (len(commands), named, handled, len(unk)), file=sys.stderr)


if __name__ == "__main__":
    main()
