"""Strict validation of level XML from outside the editor (Import XML, drag and drop, autosaved drafts).

The file is checked as text, parsed as inert XML (no DTD, no entities, no network), validated against libgarib's
level format (decomp.json: every element is a known command or type, every attribute a field of it, every value
within its field's type), and copied element by element into a fresh tree. Only that copy reaches the editor, so
comments, processing instructions, foreign namespaces and anything the checks don't know are dropped.
"""
import re
from lxml import etree as ET

LIMITS = {"bytes": 4 << 20, "elements": 50000, "depth": 8, "attrsPerElement": 64, "attrLength": 64,
          "rootAttrLength": 64}
INT_RANGES = {"u1": (0, 0xFF), "u2": (0, 0xFFFF), "u4": (0, 0xFFFFFFFF),
              "s1": (-0x80, 0x7F), "s2": (-0x8000, 0x7FFF), "s4": (-0x80000000, 0x7FFFFFFF)}
ROOT_ATTRS = {"name", "libgarib-version", "data-format-version"}
NAME_RE = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")
PRINTABLE = re.compile(r"^[\x20-\x7E]*$")
FORBIDDEN = [(re.compile(r"<!DOCTYPE", re.I), "a DOCTYPE"), (re.compile(r"<!ENTITY", re.I), "an entity declaration"),
             (re.compile(r"<\?xml-stylesheet", re.I), "a stylesheet instruction"),
             (re.compile(r"xmlns:xi\s*=|<xi:include", re.I), "XInclude"), (re.compile(r"<!\[CDATA\[", re.I), "a CDATA section")]
FLOAT_RE = re.compile(r"^[-+]?(\d+\.?\d*|\.\d+)([eE][-+]?\d+)?$")
PARSER = ET.XMLParser(resolve_entities=False, no_network=True, load_dtd=False, huge_tree=False)


def as_list(v):
    return [] if v is None else v if isinstance(v, list) else [v]


class FormatModel:
    """The level format, from decomp.json (which carries libgarib's ksy: commands, nested types, enums)"""

    def __init__(self, decomp):
        self.decomp = decomp
        self.commands = decomp["commands"]
        self.types = decomp["types"]
        camel = lambda s: "".join(w[:1].upper() + w[1:] for w in re.split(r"[_\s-]+", s))
        self.groups, self.group_child = {}, {}
        for name, c in self.commands.items():
            if c.get("groupsInto"):
                self.groups[c["groupsInto"]] = name
                self.group_child[c["groupsInto"]] = camel(c["fields"][0]["type"])
        self.declarers = {}
        for name, c in self.commands.items():
            for d in as_list(c.get("declares")):
                self.declarers.setdefault(d, set()).add(name)

    def fields_of(self, tag):
        c = self.commands.get(tag) or self.types.get(tag)
        return c["fields"] if c else None

    def ksy_of(self, tag):
        c = self.commands.get(tag) or self.types.get(tag)
        return c["ksy"] if c else None

    def enum_values(self, tag, f):
        if not f.get("enum"):
            return None
        key = f["enum"] if "::" in f["enum"] else "%s::%s" % (self.ksy_of(tag), f["enum"])
        e = self.decomp["ksyEnums"].get(key)
        return list(e.values()) if e else None


_models = {}


def format_model(decomp):
    if id(decomp) not in _models:
        _models[id(decomp)] = FormatModel(decomp)
    return _models[id(decomp)]


def check_value(model, tag, field, value):
    if len(value) > LIMITS["attrLength"]:
        return "is longer than %d characters" % LIMITS["attrLength"]
    if not PRINTABLE.match(value):
        return "contains non-printable characters"
    enums = model.enum_values(tag, field)
    if enums and value in enums:
        return None
    t = field["type"]
    if t == "str":
        return None if len(value) <= 8 else "is longer than the 8 bytes the format stores"
    if t in ("f4", "f8"):
        if not FLOAT_RE.match(value):
            return "is not a number"
        n = float(value)
        if n != n or abs(n) > 3.4e38:
            return "is out of range for a 32-bit float"
        return None
    if t in INT_RANGES:
        if re.fullmatch(r"0x[0-9a-fA-F]{1,8}", value):
            n = int(value, 16)
        elif re.fullmatch(r"[-+]?\d{1,11}", value):
            n = int(value)
        else:
            if enums:
                return "is not one of the allowed values (%s%s)" % (", ".join(enums[:8]), ", …" if len(enums) > 8 else "")
            return "is not an integer"
        lo, hi = INT_RANGES[t]
        if n < lo or n > hi:
            return "is out of range for %s (%d…%d)" % (t, lo, hi)
        return None
    return "has a type (%s) the editor can't validate" % t


class Result:
    def __init__(self, ok, root, errors, warnings, info):
        self.ok, self.root, self.errors, self.warnings, self.info = ok, root, errors, warnings, info


def sanitize_level_xml(text, decomp, source="file"):
    """Validates level XML text. Returns a Result; .root is a fresh tree of only validated content (None on errors)."""
    errors, warnings = [], []
    info = {"elements": 0, "name": None}

    def fail(msg):
        return Result(False, None, (errors + [msg])[:50], warnings, info)

    if not isinstance(text, str):
        return fail("Not text.")
    if len(text) > LIMITS["bytes"]:
        return fail("The %s is %.1f MB; level XML is at most %d MB." % (source, len(text) / 1048576, LIMITS["bytes"] >> 20))
    for rx, what in FORBIDDEN:
        if rx.search(text):
            return fail("The %s contains %s, which level XML never needs; it was refused before parsing." % (source, what))
    try:
        root = ET.fromstring(text.encode("utf-8"), PARSER)
    except (ET.XMLSyntaxError, ValueError) as e:
        return fail("Not well-formed XML: " + str(e).split("\n")[0][:200])
    if not isinstance(root.tag, str) or root.tag != "Level" or root.prefix:
        return fail("The root element must be <Level> (libgarib level XML).")

    model = format_model(decomp)
    out = ET.Element("Level")
    for name, value in root.attrib.items():
        if name not in ROOT_ATTRS:
            warnings.append("Dropped unknown attribute %s on <Level>." % name)
            continue
        if len(value) > LIMITS["rootAttrLength"] or not PRINTABLE.match(value):
            errors.append("<Level %s> is not short printable text." % name)
            continue
        out.set(name, value)
    info["name"] = out.get("name")
    path_of = lambda stack: " › ".join(stack)
    dropped = [0]

    def walk(src, dst, depth, stack):
        if src.text and src.text.strip():
            errors.append("%s: unexpected text “%s”." % (path_of(stack), src.text.strip()[:30]))
        index = 0
        for node in src:
            if node.tail and node.tail.strip():
                errors.append("%s: unexpected text “%s”." % (path_of(stack), node.tail.strip()[:30]))
            if not isinstance(node.tag, str):
                dropped[0] += 1
                continue
            index += 1
            if len(errors) >= 50:
                return
            qn = ET.QName(node)
            tag = qn.localname
            here = stack + ["%s[%d]" % (tag, index)]
            if qn.namespace is not None:
                errors.append("%s: namespaced elements aren't part of level XML." % path_of(here))
                continue
            if not NAME_RE.match(tag):
                errors.append("%s: invalid element name." % path_of(here))
                continue
            info["elements"] += 1
            if info["elements"] > LIMITS["elements"]:
                errors.append("More than %d elements." % LIMITS["elements"])
                return
            if depth > LIMITS["depth"]:
                errors.append("%s: nested deeper than %d levels." % (path_of(here), LIMITS["depth"]))
                continue
            is_command, is_type, is_group = tag in model.commands, tag in model.types, tag in model.groups
            if not (is_command or is_type or is_group):
                errors.append("%s: <%s> is not a level command libgarib knows." % (path_of(here), tag))
                continue
            parent_tag = "Level" if src is root else ET.QName(src).localname
            if parent_tag in model.groups:
                if tag != model.group_child[parent_tag]:
                    errors.append("%s: <%s> may only contain <%s>." % (path_of(here), parent_tag, model.group_child[parent_tag]))
            elif is_group:
                owner = model.commands[model.groups[tag]]
                if not any(parent_tag in model.declarers.get(m, ()) for m in as_list(owner.get("modifies"))):
                    errors.append("%s: <%s> can't appear inside <%s>." % (path_of(here), tag, parent_tag))
            elif is_command:
                c = model.commands[tag]
                if parent_tag == "Level":
                    if c.get("groupsInto"):
                        errors.append("%s: <%s> belongs inside <%s>." % (path_of(here), tag, c["groupsInto"]))
                    elif as_list(c.get("modifies")):
                        owners = [o for m in as_list(c.get("modifies")) for o in sorted(model.declarers.get(m, ()))]
                        errors.append("%s: <%s> belongs inside %s." % (path_of(here), tag, " or ".join("<%s>" % o for o in owners)))
                elif not any(parent_tag in model.declarers.get(m, ()) for m in as_list(c.get("modifies"))):
                    errors.append("%s: <%s> can't appear inside <%s>." % (path_of(here), tag, parent_tag))
            elif parent_tag == "Level":
                errors.append("%s: <%s> is part of another command and can't stand alone." % (path_of(here), tag))

            by_id = {f["id"]: f for f in (model.fields_of(tag) or [])}
            el = ET.SubElement(dst, tag)
            if len(node.attrib) > LIMITS["attrsPerElement"]:
                errors.append("%s: too many attributes." % path_of(here))
                continue
            for name, value in node.attrib.items():
                if name.startswith("{"):
                    errors.append("%s: namespaced attribute %s." % (path_of(here), name))
                    continue
                f = by_id.get(name)
                if not f or f["type"] == "switch":
                    errors.append("%s: <%s> has no field “%s”." % (path_of(here), tag, name))
                    continue
                problem = check_value(model, tag, f, value)
                if problem:
                    errors.append('%s: %s="%s" %s.' % (path_of(here), name, value[:24], problem))
                    continue
                el.set(name, value)
            walk(node, el, depth + 1, here)

    walk(root, out, 1, ["Level"])
    if dropped[0]:
        warnings.append("Dropped %d comment%s or processing instruction%s." % (dropped[0], "" if dropped[0] == 1 else "s",
                                                                             "" if dropped[0] == 1 else "s"))
    ends = [e for e in out if e.tag == "EndLevelData"]
    if len(ends) > 1:
        errors.append("More than one <EndLevelData/>; the game stops parsing at the first.")
    elif len(ends) == 1 and out[-1] is not ends[0]:
        errors.append("<EndLevelData/> must be the last command; the game ignores everything after it.")
    elif not ends and not errors:
        ET.SubElement(out, "EndLevelData")
        warnings.append("Added the missing <EndLevelData/> terminator the game parses until.")
    if len(out) <= 1:
        errors.append("The level has no commands.")
    if errors:
        return Result(False, None, errors[:50], warnings, info)
    return Result(True, out, errors, warnings, info)


def match_scenes(name, scenes):
    """Which scenes an XML's <Level name> matches (noclip names its files after it: "AT1.lnd" -> 10.AT1lnd.n64.lev)"""
    key = re.sub(r"[^A-Za-z0-9]", "", name or "").lower()
    if not key:
        return []
    return [s for s in scenes if (s["landscape"].split(".")[1] if "." in s["landscape"] else "").lower() == key]
