"""Autosaved level drafts, kept as files in the user folder (drafts/<level id>.json).

A draft is the edited level XML for one level ID plus when it was saved. Drafts are validated with the same checks as
imported files before they're used. A damaged or tampered file is refused like a bad import.
"""
import json, os, re, time

ID = re.compile(r"[0-9a-f]{2}")


class Drafts:
    def __init__(self, folder):
        self.folder = folder

    def _path(self, sid):
        if not ID.fullmatch(sid):
            raise ValueError("bad level id")
        return os.path.join(self.folder, sid + ".json")

    def save(self, sid, xml):
        """Returns False when the folder can't be written"""
        try:
            os.makedirs(self.folder, exist_ok=True)
            path = self._path(sid)
            with open(path + ".tmp", "w", encoding="utf-8") as f:
                json.dump({"id": sid, "xml": xml, "savedAt": time.time()}, f)
            os.replace(path + ".tmp", path)
            return True
        except OSError:
            return False

    def load(self, sid):
        try:
            with open(self._path(sid), encoding="utf-8") as f:
                r = json.load(f)
            return r if isinstance(r, dict) and isinstance(r.get("xml"), str) and isinstance(r.get("savedAt"), (int, float)) else None
        except (OSError, ValueError):
            return None

    def delete(self, sid):
        try:
            os.remove(self._path(sid))
        except OSError:
            pass

    # Each level's name, music and bonus clock for the game's level table. They aren't in the level file, so they
    # are kept in info.json: {level id: {"name": str or None, "song": int or None, "bonus_time": int or None,
    # "extra": bool}}. extra: the level is built as an extra level behind the game mod (Space slots only).

    def _info_path(self):
        return os.path.join(self.folder, "info.json")

    def all_info(self):
        try:
            with open(self._info_path(), encoding="utf-8") as f:
                r = json.load(f)
        except (OSError, ValueError):
            return {}
        out = {}
        for sid, v in (r.items() if isinstance(r, dict) else ()):
            if ID.fullmatch(sid) and isinstance(v, dict):
                name = v.get("name")
                song, clock = v.get("song"), v.get("bonus_time")
                out[sid] = {"name": name if isinstance(name, str) and len(name) <= 39 and name.isascii() else None,
                            "song": song if isinstance(song, int) and 0 <= song <= 60 else None,
                            "bonus_time": clock if isinstance(clock, int) and -1 <= clock <= 32766 else None,
                            "extra": v.get("extra") is True}
        return out

    def info(self, sid):
        return self.all_info().get(sid) or {"name": None, "song": None, "bonus_time": None, "extra": False}

    def save_info(self, sid, info):
        allv = self.all_info()
        if any(v is not None and v is not False for v in info.values()):
            allv[sid] = dict(info)
        else:
            allv.pop(sid, None)
        try:
            os.makedirs(self.folder, exist_ok=True)
            with open(self._info_path() + ".tmp", "w", encoding="utf-8") as f:
                json.dump(allv, f)
            os.replace(self._info_path() + ".tmp", self._info_path())
            return True
        except OSError:
            return False

    def list(self):
        """{level id: saved at}"""
        out = {}
        if os.path.isdir(self.folder):
            for fn in os.listdir(self.folder):
                if fn.endswith(".json") and ID.fullmatch(fn[:-5]):
                    r = self.load(fn[:-5])
                    if r:
                        out[fn[:-5]] = r["savedAt"]
        return out
