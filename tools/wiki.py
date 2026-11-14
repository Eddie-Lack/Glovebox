#!/usr/bin/env python3
"""Copies the guide between docs/guide and a checkout of the GitHub wiki.

    python3 tools/wiki.py to-wiki WIKI_DIR      docs/guide -> the wiki
    python3 tools/wiki.py from-wiki WIKI_DIR    the wiki -> docs/guide

The wiki names its home page Home.md, links pages without ".md", and reads pictures and the repository's other docs by
full address. Pictures are copied to the wiki's img/ folder. The wiki's _Sidebar.md is left alone.
"""
import os, re, shutil, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
GUIDE = os.path.join(ROOT, "docs", "guide")
REPO = "https://github.com/Eddie-Lack/Glovebox"
RAW = "https://raw.githubusercontent.com/wiki/Eddie-Lack/Glovebox/img/"
PAGE = r"[A-Za-z0-9-]+(?:-\([a-z]+\))?"


def to_wiki(s):
    s = s.replace("](../../", "](%s/blob/main/" % REPO).replace("](../", "](%s/blob/main/docs/" % REPO)
    s = s.replace("](README.md)", "](Home)")
    s = re.sub(r"\]\((%s)\.md(#[^)]*)?\)" % PAGE, lambda m: "](%s%s)" % (m.group(1), m.group(2) or ""), s)
    return s.replace("](img/", "](" + RAW).replace('src="img/', 'src="' + RAW)


def from_wiki(s):
    s = s.replace(RAW, "img/")
    s = s.replace("](%s/blob/main/docs/" % REPO, "](../").replace("](%s/blob/main/" % REPO, "](../../")
    s = re.sub(r"\]\(%s/wiki/(%s)(#[^)]*)?\)" % (re.escape(REPO), PAGE), lambda m: "](%s%s)" % (m.group(1), m.group(2) or ""), s)
    s = s.replace("](Home)", "](README.md)")
    return re.sub(r"\]\((%s)(#[^)]*)?\)" % PAGE, lambda m: "](%s.md%s)" % (m.group(1), m.group(2) or ""), s)


def pages(folder):
    return sorted(f for f in os.listdir(folder) if f.endswith(".md") and not f.startswith("_"))


def main():
    if len(sys.argv) != 3 or sys.argv[1] not in ("to-wiki", "from-wiki"):
        sys.exit(__doc__)
    wiki = sys.argv[2]
    if sys.argv[1] == "to-wiki":
        for f in pages(GUIDE):
            text = open(os.path.join(GUIDE, f), encoding="utf-8").read()
            out = "Home.md" if f == "README.md" else f
            open(os.path.join(wiki, out), "w", encoding="utf-8", newline="\n").write(to_wiki(text))
        shutil.copytree(os.path.join(GUIDE, "img"), os.path.join(wiki, "img"), dirs_exist_ok=True)
    else:
        for f in pages(wiki):
            text = open(os.path.join(wiki, f), encoding="utf-8").read()
            out = "README.md" if f == "Home.md" else f
            open(os.path.join(GUIDE, out), "w", encoding="utf-8", newline="\n").write(from_wiki(text))
    print("done")


main()
