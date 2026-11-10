"""The level XML as the editor holds it: an lxml tree with helpers (num, fmt, kids...)."""
import re
from lxml import etree as ET

PARSER = ET.XMLParser(resolve_entities=False, no_network=True, load_dtd=False, remove_pis=True, remove_comments=True,
                      remove_blank_text=True, huge_tree=False)


def num(e, a, d=0.0):
    v = None if e is None else e.get(a)
    if v is None:
        return d
    try:
        return float(v)
    except ValueError:
        pass
    try:
        return float(int(v, 0))   # libgarib writes ints over 10000 in hex
    except ValueError:
        m = re.match(r"\s*[-+]?(\d+\.?\d*|\.\d+)([eE][-+]?\d+)?", v)
        return float(m.group(0)) if m else float("nan")


def fmt(v):
    """A number as the level XML writes floats: 4 decimals at most, and at least one ("1.0")"""
    r = round(float(v) * 10000) / 10000 + 0.0  # no "-0.0"
    if r == int(r):
        return "%.1f" % r
    s = repr(r)
    return s


def inum(e, a, d=0):
    """An attribute as an int; d when it is missing or not a finite number"""
    v = num(e, a, d)
    return int(v) if v == v and abs(v) < 1e18 else d


def r1(v):
    return fmt(round(v))


def kids(e, tag=None):
    return [c for c in e if isinstance(c.tag, str) and (tag is None or c.tag == tag)]


def first(e, tag):
    if e is None:
        return None
    for c in e:
        if c.tag == tag:
            return c
    return None


def hex_id(s):
    if not s:
        return None
    try:
        return "0x%08X" % (int(s, 16) & 0xFFFFFFFF)
    except ValueError:
        return None


def id_hex(n):
    return "0x%08X" % (int(n) & 0xFFFFFFFF)


def path_of(node):
    p = []
    while node is not None and node.getparent() is not None:
        parent = node.getparent()
        p.insert(0, kids(parent).index(node))
        node = parent
    return p


def node_at(root, path):
    n = root
    for i in path:
        ch = kids(n) if n is not None else []
        n = ch[i] if 0 <= i < len(ch) else None
    return n


def closest(node, tag):
    while node is not None:
        if node.tag == tag:
            return node
        node = node.getparent()
    return None


def make_el(tag, attrs=None):
    e = ET.Element(tag)
    for k, v in (attrs or {}).items():
        e.set(k, v)
    return e


def ensure_child(parent, tag, attrs=None, before=None):
    """Gets or creates a child element. before(parent) picks the node to insert in front of (None appends); by
    default the new child goes first."""
    c = first(parent, tag)
    if c is None:
        c = make_el(tag, attrs)
        ref = (before or (lambda p: kids(p)[0] if kids(p) else None))(parent)
        if ref is None:
            parent.append(c)
        else:
            ref.addprevious(c)
    return c


def insert_after(src, node):
    src.addnext(node)
    node.tail = None


def normalize(root):
    """No comments, processing instructions or whitespace-only text: "is this still the shipped level?" is a plain
    string comparison whatever the XML came from"""
    for e in root.iter():
        if not isinstance(e.tag, str):
            continue
        if e.text is not None and not e.text.strip():
            e.text = None
        if e.tail is not None and not e.tail.strip():
            e.tail = None
    for e in list(root.iter(ET.Comment, ET.ProcessingInstruction)):
        parent = e.getparent()
        if parent is not None:
            parent.remove(e)
    return root


def parse(xml):
    if isinstance(xml, str):
        xml = xml.encode("utf-8")
    return normalize(ET.fromstring(xml, PARSER))


def serialize(root):
    return ET.tostring(root, encoding="unicode")


def pretty(root):
    import copy
    r = copy.deepcopy(root)
    ET.indent(r, space="  ")
    return "<?xml version='1.0' encoding='utf-8'?>\n" + ET.tostring(r, encoding="unicode") + "\n"


def clone(node):
    import copy
    c = copy.deepcopy(node)
    c.tail = None
    return c
