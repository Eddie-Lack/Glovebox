#!/usr/bin/env python3
"""BPS patches (byuu's format, as used by Floating IPS, beat and RomPatcher.js).

Encoder: SourceRead covers bytes unchanged at the same offset; SourceCopy finds data that libgarib
moved elsewhere (relocated banks) through a hash index of the source; TargetCopy covers runs of fill bytes;
everything else is TargetRead. So a patch carries only what the edit actually adds, not the game's data.

    python3 tools/bps.py create SOURCE TARGET OUT.bps [--metadata TEXT]
    python3 tools/bps.py apply  SOURCE PATCH.bps OUT
"""
import argparse, sys, zlib

import numpy as np

K = 32          # bytes hashed per index entry; also the shortest copy worth encoding
STRIDE = 8      # source index granularity (a moved block is found within STRIDE bytes of its start)


def _varint(n, out):
    while True:
        x = n & 0x7F
        n >>= 7
        if n == 0:
            out.append(0x80 | x)
            return
        out.append(x)
        n -= 1


def _signed(n, out):
    _varint((abs(n) << 1) | (1 if n < 0 else 0), out)


def _run_equal(a, ai, b, bi, limit):
    """Length of the run where a[ai:] == b[bi:], up to `limit`, compared in growing chunks."""
    n, chunk = 0, 64
    while n < limit:
        m = min(chunk, limit - n)
        eq = a[ai + n:ai + n + m] == b[bi + n:bi + n + m]
        if not eq.all():
            return n + int(np.argmin(eq))
        n += m
        chunk = min(chunk * 4, 1 << 20)
    return limit


def create(source: bytes, target: bytes, metadata: bytes = b"") -> bytes:
    S = np.frombuffer(source, dtype=np.uint8)
    T = np.frombuffer(target, dtype=np.uint8)
    ns, nt = len(S), len(T)
    index = {}
    for p in range(0, ns - K + 1, STRIDE):
        index.setdefault(source[p:p + K], p)

    out = bytearray(b"BPS1")
    _varint(ns, out)
    _varint(nt, out)
    _varint(len(metadata), out)
    out += metadata

    src_rel = 0   # sourceRelativeOffset
    tgt_rel = 0   # targetRelativeOffset
    pending = bytearray()  # TargetRead bytes not yet emitted

    def flush():
        if pending:
            _varint(((len(pending) - 1) << 2) | 1, out)
            out.extend(pending)
            pending.clear()

    i = 0
    last_delta = None
    while i < nt:
        # 1. Unchanged at the same offset
        if i < ns:
            run = _run_equal(T, i, S, i, min(nt, ns) - i)
            if run >= 4 or (run and not pending):
                flush()
                _varint(((run - 1) << 2) | 0, out)
                i += run
                continue
        # 2. Fill-byte run: copy from the byte before (overlapping TargetCopy)
        if i > 0 and T[i] == T[i - 1]:
            run = _run_equal(T, i, T, i - 1, nt - i)
            if run >= 16:
                flush()
                _varint(((run - 1) << 2) | 3, out)
                _signed((i - 1) - tgt_rel, out)
                tgt_rel = i - 1 + run
                i += run
                continue
        # 3. Moved data: continue the previous shift, else look it up
        best_p, best_len = None, 0
        if last_delta is not None and 0 <= i + last_delta < ns:
            p = i + last_delta
            l = _run_equal(T, i, S, p, min(nt - i, ns - p))
            if l >= K:
                best_p, best_len = p, l
        if best_p is None and i + K <= nt:
            p = index.get(target[i:i + K])
            if p is not None:
                l = _run_equal(T, i, S, p, min(nt - i, ns - p))
                if l >= K:
                    best_p, best_len = p, l
        if best_p is not None:
            flush()
            _varint(((best_len - 1) << 2) | 2, out)
            _signed(best_p - src_rel, out)
            src_rel = best_p + best_len
            last_delta = best_p - i
            i += best_len
            continue
        pending.append(target[i])
        i += 1
    flush()
    out += zlib.crc32(source).to_bytes(4, "little")
    out += zlib.crc32(target).to_bytes(4, "little")
    out += zlib.crc32(bytes(out)).to_bytes(4, "little")
    return bytes(out)


class PatchError(ValueError):
    pass


def apply(source: bytes, patch: bytes, max_target=64 << 20) -> bytes:
    """Reference decoder (web/patcher.html implements the same steps)."""
    if len(patch) < 4 + 3 + 12 or patch[:4] != b"BPS1":
        raise PatchError("not a BPS patch")
    end = len(patch) - 12
    if zlib.crc32(patch[:-4]) != int.from_bytes(patch[-4:], "little"):
        raise PatchError("patch is corrupt (CRC mismatch)")
    pos = 4

    def varint():
        nonlocal pos
        data, shift = 0, 1
        while True:
            if pos >= end:
                raise PatchError("truncated patch")
            x = patch[pos]
            pos += 1
            data += (x & 0x7F) * shift
            if x & 0x80:
                return data
            shift <<= 7
            data += shift
            if shift > 1 << 56:
                raise PatchError("bad number in patch")

    ns, nt, nm = varint(), varint(), varint()
    if ns != len(source):
        raise PatchError("source size %d, patch expects %d" % (len(source), ns))
    if zlib.crc32(source) != int.from_bytes(patch[-12:-8], "little"):
        raise PatchError("this patch is for a different ROM (source CRC mismatch)")
    if nt > max_target:
        raise PatchError("target too large")
    pos += nm
    out = bytearray(nt)
    o = src_rel = tgt_rel = 0
    while pos < end:
        d = varint()
        cmd, length = d & 3, (d >> 2) + 1
        if o + length > nt:
            raise PatchError("patch writes past the end of the output")
        if cmd == 0:
            if o + length > ns:
                raise PatchError("SourceRead past the end of the source")
            out[o:o + length] = source[o:o + length]
        elif cmd == 1:
            if pos + length > end:
                raise PatchError("truncated TargetRead")
            out[o:o + length] = patch[pos:pos + length]
            pos += length
        else:
            v = varint()
            off = -(v >> 1) if v & 1 else v >> 1
            if cmd == 2:
                src_rel += off
                if src_rel < 0 or src_rel + length > ns:
                    raise PatchError("SourceCopy out of range")
                out[o:o + length] = source[src_rel:src_rel + length]
                src_rel += length
            else:
                tgt_rel += off
                if tgt_rel < 0 or tgt_rel >= o:
                    raise PatchError("TargetCopy out of range")
                for k in range(length):  # may overlap
                    out[o + k] = out[tgt_rel + k]
                tgt_rel += length
        o += length
    if o != nt:
        raise PatchError("patch ended early")
    if zlib.crc32(bytes(out)) != int.from_bytes(patch[-8:-4], "little"):
        raise PatchError("result doesn't match the patch's target CRC")
    return bytes(out)


def metadata_of(patch: bytes) -> bytes:
    pos = 4
    vals = []
    for _ in range(3):
        data, shift = 0, 1
        while True:
            x = patch[pos]; pos += 1
            data += (x & 0x7F) * shift
            if x & 0x80:
                break
            shift <<= 7; data += shift
        vals.append(data)
    return patch[pos:pos + vals[2]]


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    c = sub.add_parser("create"); c.add_argument("source"); c.add_argument("target"); c.add_argument("out"); c.add_argument("--metadata", default="")
    a = sub.add_parser("apply"); a.add_argument("source"); a.add_argument("patch"); a.add_argument("out")
    args = ap.parse_args()
    if args.cmd == "create":
        p = create(open(args.source, "rb").read(), open(args.target, "rb").read(), args.metadata.encode())
        open(args.out, "wb").write(p)
        print("wrote %s (%d bytes)" % (args.out, len(p)))
    else:
        open(args.out, "wb").write(apply(open(args.source, "rb").read(), open(args.patch, "rb").read()))
        print("wrote", args.out)
