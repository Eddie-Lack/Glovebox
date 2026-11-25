"""emu.py script (gloveworld/tools/emu): boot a ROM and take screenshots, pressing Start/A to get past screens."""
import os
SHOTS = [int(x) for x in os.environ.get('SHOTS', '200,400,600,900,1200').split(',')]
PRESS = os.environ.get('PRESS', '')  # e.g. "START@150,A@250"


def run(emu):
    presses = {}
    for p in filter(None, PRESS.split(',')):
        b, f = p.split('@')
        presses[int(f)] = b
    for f in range(max(SHOTS) + 1):
        if f in presses:
            emu.hold(presses[f])
        if f - 8 in presses:
            emu.release()
        if f in SHOTS:
            emu.shot('f%05d' % f)
            size, st, dy, free = heap(emu)
            print('frame %d heap size=%d static=%d dynamic=%d free=%d' % (f, size, st, dy, free), flush=True)
            # Actor.pos is at +0x34 (include/actor.h); hand, ball, camera are Actors
            # The view SetupViewing (main.c) builds: guLookAt(actualCamPos[db], actualCamTargetPos[db], camVect),
            # guPerspective(fieldOfView, 4/3). Symbols from gloveworld's symbol_addrs.txt.
            db = emu.r8(0x801EC7F4)
            v = lambda base: tuple(emu.rf(base + 12 * db + 4 * i) for i in range(3))
            print('  view eye %.3f %.3f %.3f target %.3f %.3f %.3f up %.4f %.4f %.4f fov %d' % (
                v(0x80202248) + v(0x802586F8) + tuple(emu.rf(0x80202260 + 4 * i) for i in range(3)) + (emu.r16(0x801EC7DE),)), flush=True)
            # gameInfo.mode 13 = "LEVEL UNPLAYABLE" (FindObject, banks.c); message at +0x14 (include/game.h)
            msg = bytes(emu.read(0x801E7530 + 0x14, 0x40)).split(b'\0')[0].decode('ascii', 'replace')
            print('  mode %d message %r' % (emu.r8(0x801E7530), msg), flush=True)
            # enemies[] (Enemy is 0x300 bytes, Actor first; numberOfEnemies isn't needed, unused slots read 0)
            for i in range(int(os.environ.get('ENEMIES', '0'))):
                base = 0x802996E8 + 0x300 * i + 0x34
                print('  enemy %d pos %.1f %.1f %.1f' % ((i,) + tuple(emu.rf(base + 4 * k) for k in range(3))), flush=True)
            for name, addr in (('hand', 0x802902D8), ('ball', 0x8029F978), ('camera', 0x8028F8E0)):
                print('  %s pos %.1f %.1f %.1f' % ((name,) + tuple(emu.rf(addr + 0x34 + 4 * i) for i in range(3))), flush=True)
        yield 1


def heap(emu):
    """Free Jalloc heap (gloveworld include/jalloc.h: JallocControl at jallocControl)."""
    base = 0x8025D690 + 0x6400
    size, static, dynamic = emu.r32(base + 4), emu.r32(base + 0x10), emu.r32(base + 0x14)
    return size, static, dynamic, size - static - dynamic
