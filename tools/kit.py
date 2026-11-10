"""The editor's kit: models made by code (kit/), some with settings such as a gravestone's
words. core.py lists them, draws a preview and builds the glTF, which goes through the same converter as an imported model
(tools/custom_model.py). The same settings give the same glTF, so the converted set is reused.

    import kit
    kit.items()                                   # [{id, name, group, note, params: [...]}]
    soup = kit.build('grave', {'text': 'RIP/YOU', 'lean': 5})
    glb = soup.glb()
"""
import math, os, re, sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(os.path.dirname(HERE), 'kit'))

import meshkit  # noqa: E402,F401  (checks the models' folder is there)

TEXT = re.compile(r"[A-Z0-9 !?.,'\-]*")


def _text(v, lines, width, name):
    v = str(v).upper()
    out = [x.strip() for x in v.split('/')]
    out = [x for x in out if x]
    if not out:
        raise ValueError('%s: write at least one word' % name)
    if len(out) > lines:
        raise ValueError('%s: at most %d lines' % (name, lines))
    for x in out:
        if len(x) > width:
            raise ValueError('%s: "%s" is longer than %d letters' % (name, x, width))
        if not TEXT.fullmatch(x):
            raise ValueError('%s: only letters, digits, spaces and ! ? . , \' -' % name)
    return out


def _num(v, lo, hi, name):
    try:
        x = float(v)
    except (TypeError, ValueError):
        raise ValueError('%s must be a number' % name)
    if not (lo <= x <= hi) or math.isnan(x):
        raise ValueError('%s must be from %g to %g' % (name, lo, hi))
    return x


def _grave(p):
    import gravestone
    lines = _text(p.get('text', 'RIP'), 5, 6, 'Words')
    lean = _num(p.get('lean', 6), -15, 15, 'Lean')
    seed = sum(map(ord, '/'.join(lines))) % 97
    return gravestone.stone_soup(lines, lean, 3 + seed)


def _sign(p):
    import ruins
    top = _text(p.get('top', 'KEEP OUT'), 1, 11, 'Top')[0]
    bottom = _text(p.get('bottom', 'GONE'), 1, 10, 'Bottom')[0]
    size = 10 if len(top) <= 4 else 9 if len(top) <= 6 else 8 if len(top) <= 9 else 7
    return ruins.build_sign(top, bottom, size, 20 + sum(map(ord, top)) % 50)


def _pumpkin(p):
    import pumpkin
    return pumpkin.build(int(_num(p.get('face', 1), 1, 8, 'Face')) - 1)


def _fixed(module, fn, *args):
    def make(_p):
        import importlib
        return getattr(importlib.import_module(module), fn)(*args)
    return make


PUMPKIN_FACES = ['grins', 'howls', 'one round eye', 'weeps', 'screams', 'eyes sewn shut', 'a glove print', 'six eyes']

# id: (name, group, note, params, builder). A param is (key, label, kind, default, extra): kind 'text' or 'number' (extra: a hint) or
# 'choice' (extra: the labels of 1, 2, ...).
ITEMS = {
    'grave': ('Gravestone', 'Graveyard', 'A crooked stone with your words, about 60 triangles',
              [('text', 'Words (/ starts a line)', 'text', 'RIP/YOU', 'up to 5 lines of 6 letters'),
               ('lean', 'Lean °', 'number', 6, '-15 to 15')], _grave),
    'sign': ('Signpost', 'Graveyard', 'A leaning post with a painted board',
             [('top', 'Name', 'text', 'KEEP OUT', 'up to 11 letters'), ('bottom', 'Red word', 'text', 'GONE', 'up to 10 letters')], _sign),
    'pumpkin': ('Pumpkin', 'Graveyard', 'A jack-o\'-lantern with a glowing face, 46 across',
                [('face', 'Face', 'choice', 1, PUMPKIN_FACES)], _pumpkin),
    'candles': ('Candles', 'Graveyard', 'Three melted candles with flames', [], _fixed('candles', 'build')),
    'coffin': ('Coffin', 'Graveyard', 'A coffin with its lid pushed aside, red eyes inside', [], _fixed('coffin', 'build')),
    'fence': ('Broken fence', 'Graveyard', 'Leaning posts and hanging planks, 150 long', [], _fixed('fence', 'build', 2)),
    'tree1': ('Dead tree', 'Graveyard', 'A tall bent trunk with bare branches', [], _fixed('tree', 'build', 1, 150.0, 0.25)),
    'tree2': ('Crooked tree', 'Graveyard', 'A short twisted trunk', [], _fixed('tree', 'build', 5, 110.0, 0.55)),
    'web': ('Cobweb', 'Graveyard', 'A flat web, 40 across, both sides drawn', [], _fixed('web', 'build')),
    'rubble': ('Rubble', 'Ruins', 'A heap of broken stone', [], _fixed('ruins', 'build_rubble')),
    'scorch': ('Scorch mark', 'Ruins', 'A burnt patch for the ground', [], _fixed('ruins', 'build_scorch')),
    'thorns': ('Thorns', 'Ruins', 'A clump of thorny vines (solid when placed as Scenery)', [], _fixed('barrier', 'build_thorns')),
    'gate_atl': ('Bone gate', 'Ruins', 'Bone bars and iron crossbars over a wall of glowing runes; place it as Scenery to bar a way', [],
                 _fixed('barrier', 'build_gate_atl')),
    'gate_cave': ('Flesh gate', 'Ruins', 'A stitched curtain with eyes behind thorn vines, 150 wide; place it as Scenery to bar a way', [],
                  _fixed('barrier', 'build_gate_cave')),
    'shade': ('Shade', 'Creatures', 'A dark figure standing on the path', [], _fixed('stalker', 'build_shade')),
    'hand': ('Giant hand', 'Creatures', 'A hand reaching out of the ground', [], _fixed('stalker', 'build_hand')),
    'eye': ('Watching eye', 'Creatures', 'A bloodshot eye; reads right only from the front', [], _fixed('watcher', 'build_eye')),
    'face': ('Pale face', 'Creatures', 'A face with red points for eyes; reads right only from the front', [], _fixed('watcher', 'build_face')),
    'effigy': ('Rag-doll glove', 'Creatures', 'A giant stitched glove on a cross, 330 tall', [], _fixed('effigy', 'build')),
    'glove': ('Hanging glove', 'Creatures', 'A limp glove on a rope', [], _fixed('glove_rope', 'build_single', 1, 70.0)),
    'gallows': ('Gallows', 'Creatures', 'A beam with gloves hanging from it', [], _fixed('glove_rope', 'build_gallows', 3)),
    'cage': ('Hanging cage', 'Creatures', 'An iron cage on a chain', [], _fixed('stalker', 'build_cage')),
    'lantern': ('Lantern', 'Creatures', 'A lantern on a post', [], _fixed('stalker', 'build_lantern')),
}


def items():
    out = []
    for k, (name, group, note, params, _) in ITEMS.items():
        out.append({'id': k, 'name': name, 'group': group, 'note': note,
                    'params': [{'key': a, 'label': b, 'kind': c, 'default': d, 'extra': e} for a, b, c, d, e in params]})
    return out


def clean(item, params):
    """The item's settings with every value checked (ValueError otherwise), defaults filled in and unknown keys dropped"""
    if item not in ITEMS:
        raise ValueError('No kit model %r' % item)
    out = {}
    for key, _label, _kind, default, _extra in ITEMS[item][3]:
        out[key] = params.get(key, default)
    ITEMS[item][4](dict(out))      # raises on a bad value
    return out


def build(item, params):
    """The model as a meshkit Soup"""
    if item not in ITEMS:
        raise ValueError('No kit model %r' % item)
    return ITEMS[item][4](params)


def label(item, params):
    """A short name for the converted model: the item's name and its words"""
    name = ITEMS[item][0]
    words = ' '.join(str(params[k]).replace('/', ' ') for k, *_ in ITEMS[item][3] if k in ('text', 'top'))
    if item == 'pumpkin':
        words = PUMPKIN_FACES[int(float(params['face'])) - 1]
    return ('%s: %s' % (name, words) if words else name)[:40]


def preview_png(item, params, size=128):
    import io
    import preview
    im = preview.render(build(item, params), size=size)
    buf = io.BytesIO()
    im.save(buf, 'PNG')
    return buf.getvalue()
