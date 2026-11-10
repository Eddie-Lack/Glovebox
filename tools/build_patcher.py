#!/usr/bin/env python3
"""Pin web/patcher.html's inline script in its Content-Security-Policy (script-src 'sha256-…').

Run after editing the patcher's script; --check exits non-zero if the hash is stale (used by the tests).

    python3 tools/build_patcher.py [--check]
"""
import base64, hashlib, os, re, sys

PATH = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "web", "patcher.html")
html = open(PATH, encoding="utf-8").read()
scripts = re.findall(r"<script>(.*?)</script>", html, re.S)
assert len(scripts) == 1, "the patcher must have exactly one inline script"
digest = "sha256-" + base64.b64encode(hashlib.sha256(scripts[0].encode("utf-8")).digest()).decode()
current = re.search(r"script-src '([^']+)'", html).group(1)
if "--check" in sys.argv:
    print("ok" if current == digest else "stale: %s != %s" % (current, digest))
    sys.exit(0 if current == digest else 1)
html = html.replace("script-src '%s'" % current, "script-src '%s'" % digest)
open(PATH, "w", encoding="utf-8").write(html)
print("pinned", digest)
