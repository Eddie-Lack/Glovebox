//! BPS patches (byuu's format, as used by Floating IPS, beat and RomPatcher.js). This is `tools/bps.py`, with the
//! same patch bytes.
//!
//! The encoder walks the target. SourceRead covers bytes unchanged at the same offset. TargetCopy covers runs of
//! one fill byte. SourceCopy finds data that moved (relocated banks) through an index of the source. Everything
//! else is TargetRead. A patch carries only what the edit adds, not the game's data.

use std::collections::HashMap;

/// Bytes hashed per index entry. Also the shortest copy worth encoding.
const K: usize = 32;
/// The index holds every eighth offset of the source.
const STRIDE: usize = 8;

/// The largest target `apply` makes unless told otherwise.
pub const MAX_TARGET: usize = 64 << 20;

/// Why a patch can't be applied. The texts are those of `tools/bps.py`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchError(pub String);

impl std::fmt::Display for PatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PatchError {}

fn err<T>(msg: impl Into<String>) -> Result<T, PatchError> {
    Err(PatchError(msg.into()))
}

fn varint(mut n: u64, out: &mut Vec<u8>) {
    loop {
        let x = (n & 0x7F) as u8;
        n >>= 7;
        if n == 0 {
            out.push(0x80 | x);
            return;
        }
        out.push(x);
        n -= 1;
    }
}

fn signed(n: i64, out: &mut Vec<u8>) {
    varint((n.unsigned_abs() << 1) | (n < 0) as u64, out);
}

/// Length of the run where a[ai..] equals b[bi..], up to `limit`.
fn run_equal(a: &[u8], ai: usize, b: &[u8], bi: usize, limit: usize) -> usize {
    let (x, y) = (&a[ai..ai + limit], &b[bi..bi + limit]);
    // Whole words first: most runs are long
    let mut n = 0;
    for (cx, cy) in x.chunks_exact(8).zip(y.chunks_exact(8)) {
        if cx != cy {
            break;
        }
        n += 8;
    }
    while n < limit && x[n] == y[n] {
        n += 1;
    }
    n
}

fn flush(pending: &mut Vec<u8>, out: &mut Vec<u8>) {
    if !pending.is_empty() {
        varint((((pending.len() - 1) as u64) << 2) | 1, out);
        out.append(pending);
    }
}

/// The patch that turns `source` into `target`.
pub fn create(source: &[u8], target: &[u8], metadata: &[u8]) -> Vec<u8> {
    let (ns, nt) = (source.len(), target.len());
    // The first place of every 32 byte block that starts at a multiple of STRIDE
    let mut index: HashMap<&[u8], usize> = HashMap::with_capacity(ns / STRIDE + 1);
    if ns >= K {
        for p in (0..=ns - K).step_by(STRIDE) {
            index.entry(&source[p..p + K]).or_insert(p);
        }
    }

    let mut out = b"BPS1".to_vec();
    varint(ns as u64, &mut out);
    varint(nt as u64, &mut out);
    varint(metadata.len() as u64, &mut out);
    out.extend_from_slice(metadata);

    let mut src_rel: i64 = 0; // sourceRelativeOffset
    let mut tgt_rel: i64 = 0; // targetRelativeOffset
    let mut pending: Vec<u8> = Vec::new(); // TargetRead bytes not yet written
    let mut i = 0usize;
    let mut last_delta: Option<i64> = None;
    while i < nt {
        // 1. Unchanged at the same offset
        if i < ns {
            let run = run_equal(target, i, source, i, nt.min(ns) - i);
            if run >= 4 || (run > 0 && pending.is_empty()) {
                flush(&mut pending, &mut out);
                varint(((run - 1) as u64) << 2, &mut out);
                i += run;
                continue;
            }
        }
        // 2. A run of one byte: copy from the byte before (an overlapping TargetCopy)
        if i > 0 && target[i] == target[i - 1] {
            let run = run_equal(target, i, target, i - 1, nt - i);
            if run >= 16 {
                flush(&mut pending, &mut out);
                varint((((run - 1) as u64) << 2) | 3, &mut out);
                signed((i - 1) as i64 - tgt_rel, &mut out);
                tgt_rel = (i - 1 + run) as i64;
                i += run;
                continue;
            }
        }
        // 3. Moved data: continue the previous shift, else look it up
        let mut best: Option<(usize, usize)> = None;
        if let Some(delta) = last_delta {
            let p = i as i64 + delta;
            if 0 <= p && (p as usize) < ns {
                let p = p as usize;
                let l = run_equal(target, i, source, p, (nt - i).min(ns - p));
                if l >= K {
                    best = Some((p, l));
                }
            }
        }
        if best.is_none() && i + K <= nt {
            if let Some(&p) = index.get(&target[i..i + K]) {
                let l = run_equal(target, i, source, p, (nt - i).min(ns - p));
                if l >= K {
                    best = Some((p, l));
                }
            }
        }
        if let Some((p, l)) = best {
            flush(&mut pending, &mut out);
            varint((((l - 1) as u64) << 2) | 2, &mut out);
            signed(p as i64 - src_rel, &mut out);
            src_rel = (p + l) as i64;
            last_delta = Some(p as i64 - i as i64);
            i += l;
            continue;
        }
        pending.push(target[i]);
        i += 1;
    }
    flush(&mut pending, &mut out);
    out.extend_from_slice(&crc32fast::hash(source).to_le_bytes());
    out.extend_from_slice(&crc32fast::hash(target).to_le_bytes());
    let crc = crc32fast::hash(&out);
    out.extend_from_slice(&crc.to_le_bytes());
    out
}

fn le32(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

/// Reads a number at `*pos`, which must stay before `end`.
fn read_varint(patch: &[u8], pos: &mut usize, end: usize) -> Result<u128, PatchError> {
    let (mut data, mut shift): (u128, u128) = (0, 1);
    loop {
        if *pos >= end {
            return err("truncated patch");
        }
        let x = patch[*pos];
        *pos += 1;
        data += (x & 0x7F) as u128 * shift;
        if x & 0x80 != 0 {
            return Ok(data);
        }
        shift <<= 7;
        data += shift;
        if shift > 1 << 56 {
            return err("bad number in patch");
        }
    }
}

/// The target a patch makes from `source`, with the default size limit.
pub fn apply(source: &[u8], patch: &[u8]) -> Result<Vec<u8>, PatchError> {
    apply_with_limit(source, patch, MAX_TARGET)
}

/// The target a patch makes from `source`. Refuses a patch that is damaged, is for another source, or makes a
/// target larger than `max_target`.
pub fn apply_with_limit(source: &[u8], patch: &[u8], max_target: usize) -> Result<Vec<u8>, PatchError> {
    if patch.len() < 4 + 3 + 12 || &patch[..4] != b"BPS1" {
        return err("not a BPS patch");
    }
    let end = patch.len() - 12;
    if crc32fast::hash(&patch[..patch.len() - 4]) != le32(&patch[patch.len() - 4..]) {
        return err("patch is corrupt (CRC mismatch)");
    }
    let mut pos = 4usize;
    let ns = read_varint(patch, &mut pos, end)?;
    let nt = read_varint(patch, &mut pos, end)?;
    let nm = read_varint(patch, &mut pos, end)?;
    if ns != source.len() as u128 {
        return err(format!("source size {}, patch expects {}", source.len(), ns));
    }
    let ns = source.len();
    if crc32fast::hash(source) != le32(&patch[end..end + 4]) {
        return err("this patch is for a different ROM (source CRC mismatch)");
    }
    if nt > max_target as u128 {
        return err("target too large");
    }
    let nt = nt as usize;
    // Metadata that runs past the end leaves no commands
    pos = (pos as u128).saturating_add(nm).min(end as u128) as usize;
    let mut out = vec![0u8; nt];
    let mut o = 0usize;
    let (mut src_rel, mut tgt_rel): (i128, i128) = (0, 0);
    while pos < end {
        let d = read_varint(patch, &mut pos, end)?;
        let cmd = (d & 3) as u8;
        let length = (d >> 2) + 1;
        if o as u128 + length > nt as u128 {
            return err("patch writes past the end of the output");
        }
        let length = length as usize;
        match cmd {
            0 => {
                if o + length > ns {
                    return err("SourceRead past the end of the source");
                }
                out[o..o + length].copy_from_slice(&source[o..o + length]);
            }
            1 => {
                if pos + length > end {
                    return err("truncated TargetRead");
                }
                out[o..o + length].copy_from_slice(&patch[pos..pos + length]);
                pos += length;
            }
            _ => {
                let v = read_varint(patch, &mut pos, end)?;
                let off = if v & 1 != 0 { -((v >> 1) as i128) } else { (v >> 1) as i128 };
                if cmd == 2 {
                    src_rel += off;
                    if src_rel < 0 || src_rel + length as i128 > ns as i128 {
                        return err("SourceCopy out of range");
                    }
                    let s = src_rel as usize;
                    out[o..o + length].copy_from_slice(&source[s..s + length]);
                    src_rel += length as i128;
                } else {
                    tgt_rel += off;
                    if tgt_rel < 0 || tgt_rel >= o as i128 {
                        return err("TargetCopy out of range");
                    }
                    let t = tgt_rel as usize;
                    // The two ranges may overlap: copy byte by byte
                    for k in 0..length {
                        out[o + k] = out[t + k];
                    }
                    tgt_rel += length as i128;
                }
            }
        }
        o += length;
    }
    if o != nt {
        return err("patch ended early");
    }
    if crc32fast::hash(&out) != le32(&patch[end + 4..end + 8]) {
        return err("result doesn't match the patch's target CRC");
    }
    Ok(out)
}

/// The metadata a patch carries. None when the patch is too short to hold what its header says.
pub fn metadata_of(patch: &[u8]) -> Option<&[u8]> {
    let mut pos = 4usize;
    let mut vals = [0u128; 3];
    for v in &mut vals {
        *v = read_varint(patch, &mut pos, patch.len()).ok()?;
    }
    let len = usize::try_from(vals[2]).ok()?;
    patch.get(pos..pos.checked_add(len)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Made-up bytes that do not repeat soon.
    fn noise(n: usize, seed: u32) -> Vec<u8> {
        let mut x = seed | 1;
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                (x >> 11) as u8
            })
            .collect()
    }

    #[test]
    fn numbers() {
        let enc = |n| {
            let mut o = Vec::new();
            varint(n, &mut o);
            o
        };
        assert_eq!(enc(0), [0x80]);
        assert_eq!(enc(127), [0xFF]);
        assert_eq!(enc(128), [0x00, 0x80]);
        assert_eq!(enc(16511), [0x7F, 0xFF]);
        assert_eq!(enc(16512), [0x00, 0x00, 0x80]);
        for n in [0u64, 1, 127, 128, 16511, 16512, 1 << 24, (1 << 40) + 12345] {
            let e = enc(n);
            let mut pos = 0;
            assert_eq!(read_varint(&e, &mut pos, e.len()), Ok(n as u128));
            assert_eq!(pos, e.len());
        }
        let mut o = Vec::new();
        signed(-3, &mut o);
        signed(3, &mut o);
        assert_eq!(o, [0x87, 0x86]);
    }

    #[test]
    fn identical_files_give_one_command() {
        let s = noise(1000, 7);
        let p = create(&s, &s, b"");
        // Header (4 + 2 + 2 + 1), one SourceRead of 1000 bytes (2 bytes), three CRCs
        assert_eq!(p.len(), 9 + 2 + 12);
        assert_eq!(apply(&s, &p).unwrap(), s);
        assert_eq!(metadata_of(&p), Some(&b""[..]));
    }

    #[test]
    fn round_trips() {
        let s = noise(40_000, 1);
        let mut cases: Vec<Vec<u8>> = Vec::new();
        // A changed byte, a moved block, a fill run, new data, a longer and a shorter target
        let mut t = s.clone();
        t[1234] ^= 0x55;
        cases.push(t);
        let mut t = s[..10_000].to_vec();
        t.extend_from_slice(&noise(333, 9));
        t.extend_from_slice(&s[10_000..]);
        cases.push(t);
        let mut t = s.clone();
        for b in &mut t[5000..5600] {
            *b = 0;
        }
        t.extend_from_slice(&[0xFF; 100]);
        t.extend_from_slice(&s[20_003..21_000]);
        cases.push(t);
        cases.push(s[..77].to_vec());
        cases.push(Vec::new());
        cases.push(noise(5000, 3));
        for t in &cases {
            let p = create(&s, t, b"meta \xff data");
            assert_eq!(&apply(&s, &p).unwrap(), t);
            assert_eq!(metadata_of(&p), Some(&b"meta \xff data"[..]));
        }
        // An empty or short source
        let p = create(b"", &s[..100], b"");
        assert_eq!(apply(b"", &p).unwrap(), &s[..100]);
        let p = create(&s[..10], &s[..100], b"");
        assert_eq!(apply(&s[..10], &p).unwrap(), &s[..100]);
    }

    #[test]
    fn a_moved_block_is_copied_not_stored() {
        let s = noise(20_000, 5);
        let mut t = noise(64, 11);
        t.extend_from_slice(&s[4096..12_000]);
        let p = create(&s, &t, b"");
        assert!(p.len() < 200, "the patch is {} bytes", p.len());
        assert_eq!(apply(&s, &p).unwrap(), t);
    }

    #[test]
    fn damaged_patches_are_refused() {
        let s = noise(3000, 1);
        let mut t = s.clone();
        t[100] ^= 1;
        t.extend_from_slice(&noise(50, 2));
        let p = create(&s, &t, b"m");
        assert_eq!(apply(&s, &p[..10]).unwrap_err().0, "not a BPS patch");
        let mut q = p.clone();
        q[0] = b'X';
        assert_eq!(apply(&s, &q).unwrap_err().0, "not a BPS patch");
        for i in 4..p.len() {
            let mut q = p.clone();
            q[i] ^= 0x10;
            assert_eq!(apply(&s, &q).unwrap_err().0, "patch is corrupt (CRC mismatch)");
        }
        assert_eq!(apply(&s[1..], &p).unwrap_err().0, "source size 2999, patch expects 3000");
        let mut other = s.clone();
        other[0] ^= 1;
        assert_eq!(apply(&other, &p).unwrap_err().0, "this patch is for a different ROM (source CRC mismatch)");
        assert_eq!(apply_with_limit(&s, &p, 100).unwrap_err().0, "target too large");
    }

    /// A patch with these commands and valid CRCs.
    fn raw(ns: usize, nt: u64, body: &[u8], source: &[u8], target_crc: u32) -> Vec<u8> {
        let mut p = b"BPS1".to_vec();
        varint(ns as u64, &mut p);
        varint(nt, &mut p);
        varint(0, &mut p);
        p.extend_from_slice(body);
        p.extend_from_slice(&crc32fast::hash(source).to_le_bytes());
        p.extend_from_slice(&target_crc.to_le_bytes());
        let c = crc32fast::hash(&p);
        p.extend_from_slice(&c.to_le_bytes());
        p
    }

    #[test]
    fn bad_commands_are_refused() {
        let s = [1u8, 2, 3, 4, 5, 6, 7, 8];
        let cmd = |len: u64, c: u64| {
            let mut o = Vec::new();
            varint(((len - 1) << 2) | c, &mut o);
            o
        };
        let e = |nt, body: &[u8]| apply(&s, &raw(8, nt, body, &s, 0)).unwrap_err().0;
        assert_eq!(e(4, &cmd(5, 0)), "patch writes past the end of the output");
        assert_eq!(e(12, &cmd(12, 0)), "SourceRead past the end of the source");
        assert_eq!(e(8, &cmd(8, 1)), "truncated TargetRead");
        let mut b = cmd(4, 2);
        signed(6, &mut b);
        assert_eq!(e(4, &b), "SourceCopy out of range");
        let mut b = cmd(4, 2);
        signed(-1, &mut b);
        assert_eq!(e(4, &b), "SourceCopy out of range");
        let mut b = cmd(4, 3);
        signed(0, &mut b);
        assert_eq!(e(4, &b), "TargetCopy out of range");
        assert_eq!(e(4, &cmd(4, 2)), "truncated patch");
        assert_eq!(e(8, &cmd(4, 0)), "patch ended early");
        assert_eq!(e(4, &cmd(4, 0)), "result doesn't match the patch's target CRC");
        assert_eq!(e(4, &[0u8; 9]), "bad number in patch");
        // A huge length must not overflow
        assert_eq!(e(4, &[0x7F, 0x7F, 0x7F, 0x7F, 0x7F, 0x7F, 0x7F, 0x7F, 0xFF]), "patch writes past the end of the output");
        // An overlapping TargetCopy repeats the byte before
        let mut b = cmd(1, 1);
        b.push(9);
        b.extend(cmd(5, 3));
        signed(0, &mut b);
        let want = [9u8; 6];
        assert_eq!(apply(&s, &raw(8, 6, &b, &s, crc32fast::hash(&want))).unwrap(), want);
    }
}
