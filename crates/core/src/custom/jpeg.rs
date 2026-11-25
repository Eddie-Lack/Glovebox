//! A JPEG decoder that gives the pixels libjpeg-turbo gives, which is what Pillow reads.
//!
//! Import model names a texture by a hash of its pixels, so a picture must decode to the same bytes on
//! every computer. This follows libjpeg's defaults step by step: Huffman decoding (baseline and progressive), the
//! accurate integer inverse DCT (`jidctint.c`), "fancy" chroma upsampling (`jdsample.c`) and the fixed point
//! color conversion (`jdcolor.c`). Arithmetic coding, lossless, 12-bit and CMYK files are refused.

const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20, 13, 6, 7, 14, 21,
    28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59, 52, 45, 38, 31, 39, 46, 53, 60, 61,
    54, 47, 55, 62, 63,
];

type Res<T> = Result<T, String>;

fn bad<T>(why: &str) -> Res<T> {
    Err(format!("broken JPEG data: {why}"))
}

struct Huffman {
    /// The largest code of each length, or -1
    maxcode: [i32; 17],
    mincode: [i32; 17],
    valptr: [usize; 17],
    vals: Vec<u8>,
}

struct Component {
    id: u8,
    h: usize,
    v: usize,
    tq: usize,
    /// The size in blocks that holds picture data
    blocks_w: usize,
    blocks_h: usize,
    /// The width in blocks of the stored rows: whole MCUs
    stride: usize,
    coefs: Vec<i16>,
}

/// Reads the entropy coded bits of a scan. A marker ends the data: after it, the bits read as zero.
struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
    acc: u32,
    n: u32,
    marker: bool,
}

impl Bits<'_> {
    fn bit(&mut self) -> u32 {
        if self.n == 0 {
            let mut byte = 0;
            if !self.marker {
                match (self.data.get(self.pos), self.data.get(self.pos + 1)) {
                    (Some(0xFF), Some(0)) => {
                        byte = 0xFF;
                        self.pos += 2;
                    }
                    (Some(0xFF), _) | (None, _) => self.marker = true,
                    (Some(b), _) => {
                        byte = *b;
                        self.pos += 1;
                    }
                }
            }
            self.acc = byte as u32;
            self.n = 8;
        }
        self.n -= 1;
        (self.acc >> self.n) & 1
    }

    fn receive(&mut self, count: u32) -> i32 {
        (0..count).fold(0, |v, _| (v << 1) | self.bit() as i32)
    }

    /// A value of `s` bits, as a signed difference.
    fn extend(&mut self, s: u32) -> i32 {
        if s == 0 {
            return 0;
        }
        let v = self.receive(s);
        if v < 1 << (s - 1) {
            v - (1 << s) + 1
        } else {
            v
        }
    }

    fn decode(&mut self, h: &Huffman) -> Res<u8> {
        let mut code = 0i32;
        for len in 1..=16 {
            code = (code << 1) | self.bit() as i32;
            if code <= h.maxcode[len] {
                let at = usize::try_from(code - h.mincode[len]).ok().and_then(|n| h.vals.get(h.valptr[len] + n));
                return match at {
                    Some(v) => Ok(*v),
                    None => bad("a Huffman code has no value"),
                };
            }
        }
        bad("a Huffman code isn't in its table")
    }

    /// Steps over a restart marker.
    fn restart(&mut self) -> Res<()> {
        self.n = 0;
        while self.data.get(self.pos) == Some(&0xFF) && self.data.get(self.pos + 1) == Some(&0xFF) {
            self.pos += 1;
        }
        match (self.data.get(self.pos), self.data.get(self.pos + 1)) {
            (Some(0xFF), Some(0xD0..=0xD7)) => {
                self.pos += 2;
                self.marker = false;
                Ok(())
            }
            _ => bad("a restart marker is missing"),
        }
    }
}

struct Scan {
    comps: Vec<(usize, usize, usize)>,
    ss: usize,
    se: usize,
    ah: u32,
    al: u32,
}

struct Decoder {
    width: usize,
    height: usize,
    progressive: bool,
    comps: Vec<Component>,
    hmax: usize,
    vmax: usize,
    quant: [Option<[u16; 64]>; 4],
    dc: [Option<Huffman>; 4],
    ac: [Option<Huffman>; 4],
    restart: usize,
    jfif: bool,
    adobe: Option<u8>,
}

fn be16(d: &[u8], at: usize) -> Res<usize> {
    match d.get(at..at + 2) {
        Some(b) => Ok((b[0] as usize) << 8 | b[1] as usize),
        None => bad("the file is cut short"),
    }
}

/// Decodes a JPEG file to RGBA. `check` is given the width and height before any pixel is stored.
pub fn decode(data: &[u8], check: &dyn Fn(u64, u64) -> Res<()>) -> Res<(usize, usize, Vec<u8>)> {
    let mut d = Decoder {
        width: 0,
        height: 0,
        progressive: false,
        comps: Vec::new(),
        hmax: 1,
        vmax: 1,
        quant: [None; 4],
        dc: [None, None, None, None],
        ac: [None, None, None, None],
        restart: 0,
        jfif: false,
        adobe: None,
    };
    let mut pos = 2;
    let mut scans = 0usize;
    loop {
        // The next marker
        while data.get(pos) != Some(&0xFF) {
            if pos >= data.len() {
                return bad("the file is cut short");
            }
            pos += 1;
        }
        while data.get(pos) == Some(&0xFF) {
            pos += 1;
        }
        let Some(&marker) = data.get(pos) else { return bad("the file is cut short") };
        pos += 1;
        if marker == 0xD9 {
            break;
        }
        if marker == 0 || marker == 0x01 || (0xD0..=0xD8).contains(&marker) {
            continue;
        }
        let len = be16(data, pos)?;
        let Some(seg) = data.get(pos + 2..pos + len).filter(|_| len >= 2) else { return bad("a segment is cut short") };
        pos += len;
        match marker {
            0xC0..=0xC2 => d.frame(seg, marker == 0xC2, check)?,
            0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF => return Err("this kind of JPEG file isn't supported".into()),
            0xC4 => d.huffman(seg)?,
            0xDB => d.quant_tables(seg)?,
            0xDD => d.restart = be16(seg, 0)?,
            0xE0 if seg.starts_with(b"JFIF\0") => d.jfif = true,
            0xEE if seg.starts_with(b"Adobe") && seg.len() >= 12 => d.adobe = Some(seg[11]),
            0xDA => {
                scans += 1;
                if scans > 1000 {
                    return bad("too many scans");
                }
                pos = d.scan(seg, data, pos)?;
            }
            _ => {}
        }
    }
    if d.comps.is_empty() || scans == 0 {
        return bad("the file has no picture");
    }
    d.picture()
}

impl Decoder {
    fn frame(&mut self, seg: &[u8], progressive: bool, check: &dyn Fn(u64, u64) -> Res<()>) -> Res<()> {
        if !self.comps.is_empty() {
            return bad("the file has two frames");
        }
        if seg.len() < 6 || seg[0] != 8 {
            return Err("only 8-bit JPEG files are supported".into());
        }
        (self.height, self.width) = (be16(seg, 1)?, be16(seg, 3)?);
        let count = seg[5] as usize;
        if count == 4 {
            return Err("CMYK JPEG files aren't supported".into());
        }
        if (count != 1 && count != 3) || seg.len() < 6 + count * 3 {
            return bad("the frame has a bad number of components");
        }
        check(self.width as u64, self.height as u64)?;
        self.progressive = progressive;
        for c in seg[6..6 + count * 3].chunks_exact(3) {
            let (h, v, tq) = ((c[1] >> 4) as usize, (c[1] & 15) as usize, c[2] as usize);
            if !(1..=4).contains(&h) || !(1..=4).contains(&v) || tq > 3 {
                return bad("a component has bad sampling factors");
            }
            self.hmax = self.hmax.max(h);
            self.vmax = self.vmax.max(v);
            self.comps.push(Component { id: c[0], h, v, tq, blocks_w: 0, blocks_h: 0, stride: 0, coefs: Vec::new() });
        }
        let (mcus_x, mcus_y) = (self.width.div_ceil(8 * self.hmax), self.height.div_ceil(8 * self.vmax));
        for c in &mut self.comps {
            c.blocks_w = (self.width * c.h).div_ceil(self.hmax).div_ceil(8);
            c.blocks_h = (self.height * c.v).div_ceil(self.vmax).div_ceil(8);
            c.stride = mcus_x * c.h;
            c.coefs = vec![0; c.stride * mcus_y * c.v * 64];
        }
        Ok(())
    }

    fn quant_tables(&mut self, mut seg: &[u8]) -> Res<()> {
        while !seg.is_empty() {
            let (wide, t) = (seg[0] >> 4 != 0, (seg[0] & 15) as usize);
            let size = if wide { 128 } else { 64 };
            if t > 3 || seg.len() < 1 + size {
                return bad("a quantization table is cut short");
            }
            let mut table = [0u16; 64];
            for (k, slot) in ZIGZAG.iter().enumerate() {
                table[*slot] = if wide { be16(seg, 1 + k * 2)? as u16 } else { seg[1 + k] as u16 };
            }
            self.quant[t] = Some(table);
            seg = &seg[1 + size..];
        }
        Ok(())
    }

    fn huffman(&mut self, mut seg: &[u8]) -> Res<()> {
        while !seg.is_empty() {
            if seg.len() < 17 {
                return bad("a Huffman table is cut short");
            }
            let (class, t) = (seg[0] >> 4, (seg[0] & 15) as usize);
            let total: usize = seg[1..17].iter().map(|n| *n as usize).sum();
            if class > 1 || t > 3 || total > 256 || seg.len() < 17 + total {
                return bad("a Huffman table is malformed");
            }
            let mut h = Huffman { maxcode: [-1; 17], mincode: [0; 17], valptr: [0; 17], vals: seg[17..17 + total].to_vec() };
            let (mut code, mut at) = (0i32, 0usize);
            for len in 1..=16 {
                let n = seg[len] as usize;
                if n > 0 {
                    h.valptr[len] = at;
                    h.mincode[len] = code;
                    code += n as i32;
                    h.maxcode[len] = code - 1;
                    at += n;
                }
                if code > 1 << len {
                    return bad("a Huffman table has too many codes");
                }
                code <<= 1;
            }
            if class == 0 {
                self.dc[t] = Some(h)
            } else {
                self.ac[t] = Some(h)
            }
            seg = &seg[17 + total..];
        }
        Ok(())
    }

    /// Decodes one scan. Returns where the data after it starts.
    fn scan(&mut self, seg: &[u8], data: &[u8], pos: usize) -> Res<usize> {
        let count = *seg.first().unwrap_or(&0) as usize;
        if count == 0 || count > self.comps.len() || seg.len() < 4 + count * 2 {
            return bad("a scan header is malformed");
        }
        let mut scan = Scan { comps: Vec::new(), ss: 0, se: 63, ah: 0, al: 0 };
        for c in seg[1..1 + count * 2].chunks_exact(2) {
            let Some(index) = self.comps.iter().position(|k| k.id == c[0]) else {
                return bad("a scan names a component the frame lacks");
            };
            if scan.comps.iter().any(|k: &(usize, usize, usize)| k.0 == index) {
                return bad("a scan names a component twice");
            }
            scan.comps.push((index, (c[1] >> 4) as usize & 3, (c[1] & 15) as usize & 3));
        }
        let tail = &seg[1 + count * 2..];
        if self.progressive {
            (scan.ss, scan.se, scan.ah, scan.al) = (tail[0] as usize, tail[1] as usize, (tail[2] >> 4) as u32, (tail[2] & 15) as u32);
            if scan.ss > scan.se || scan.se > 63 || scan.al > 13 || (scan.ss == 0) != (scan.se == 0) || (scan.ss > 0 && count != 1) {
                return bad("a progressive scan is malformed");
            }
        }
        for (_, td, ta) in &scan.comps {
            let need_dc = scan.ss == 0 && scan.ah == 0;
            let need_ac = if self.progressive { scan.ss > 0 } else { true };
            if (need_dc && self.dc[*td].is_none()) || (need_ac && self.ac[*ta].is_none()) {
                return bad("a scan uses a Huffman table the file lacks");
            }
        }
        let mut bits = Bits { data, pos, acc: 0, n: 0, marker: false };
        let mut pred = [0i32; 4];
        let mut eobrun = 0u32;
        // The blocks of one MCU: the component and the block's place in it
        let interleaved = scan.comps.len() > 1;
        let (mcus_x, mcus_y) = if interleaved {
            (self.width.div_ceil(8 * self.hmax), self.height.div_ceil(8 * self.vmax))
        } else {
            let c = &self.comps[scan.comps[0].0];
            (c.blocks_w, c.blocks_h)
        };
        let mut left = self.restart;
        for my in 0..mcus_y {
            for mx in 0..mcus_x {
                if self.restart > 0 {
                    if left == 0 {
                        bits.restart()?;
                        pred = [0; 4];
                        eobrun = 0;
                        left = self.restart;
                    }
                    left -= 1;
                }
                for (k, (index, td, ta)) in scan.comps.iter().enumerate() {
                    let (h, v) = if interleaved { (self.comps[*index].h, self.comps[*index].v) } else { (1, 1) };
                    for by in 0..v {
                        for bx in 0..h {
                            let c = &mut self.comps[*index];
                            let at = ((my * v + by) * c.stride + mx * h + bx) * 64;
                            let Some(block) = c.coefs.get_mut(at..at + 64) else { return bad("a block is out of place") };
                            let (dc, ac) = (self.dc[*td].as_ref(), self.ac[*ta].as_ref());
                            if !self.progressive {
                                baseline(&mut bits, block, dc, ac, &mut pred[k])?;
                            } else if scan.ss == 0 {
                                if scan.ah == 0 {
                                    let Some(dc) = dc else { return bad("no table") };
                                    let s = bits.decode(dc)? as u32;
                                    pred[k] = pred[k].wrapping_add(bits.extend(s.min(16)));
                                    block[0] = (pred[k] << scan.al) as i16;
                                } else if bits.bit() != 0 {
                                    block[0] |= 1 << scan.al;
                                }
                            } else {
                                let Some(ac) = ac else { return bad("no table") };
                                if scan.ah == 0 {
                                    ac_first(&mut bits, block, ac, &scan, &mut eobrun)?;
                                } else {
                                    ac_refine(&mut bits, block, ac, &scan, &mut eobrun)?;
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(bits.pos)
    }

    /// The pixels of the decoded picture as RGBA.
    fn picture(&self) -> Res<(usize, usize, Vec<u8>)> {
        let (w, h) = (self.width, self.height);
        // Each component at the picture's size
        let mut planes: Vec<Vec<u8>> = Vec::new();
        for c in &self.comps {
            let Some(q) = &self.quant[c.tq] else { return bad("a quantization table is missing") };
            let (pw, ph) = (c.blocks_w * 8, c.blocks_h * 8);
            let mut plane = vec![0u8; pw * ph];
            for by in 0..c.blocks_h {
                for bx in 0..c.blocks_w {
                    let at = (by * c.stride + bx) * 64;
                    let out = idct(&c.coefs[at..at + 64], q);
                    for y in 0..8 {
                        plane[(by * 8 + y) * pw + bx * 8..][..8].copy_from_slice(&out[y * 8..y * 8 + 8]);
                    }
                }
            }
            // The component's own size
            let (cw, ch) = ((w * c.h).div_ceil(self.hmax), (h * c.v).div_ceil(self.vmax));
            planes.push(upsample(&plane, pw, cw, ch, self.hmax / c.h, self.vmax / c.v, self.hmax % c.h + self.vmax % c.v, w, h)?);
        }
        let mut out = Vec::with_capacity(w * h * 4);
        if planes.len() == 1 {
            for y in &planes[0] {
                out.extend_from_slice(&[*y, *y, *y, 255]);
            }
            return Ok((w, h, out));
        }
        let ids: Vec<u8> = self.comps.iter().map(|c| c.id).collect();
        let rgb = match (self.jfif, self.adobe) {
            (true, _) => false,
            (false, Some(transform)) => transform == 0,
            (false, None) => ids == [b'R', b'G', b'B'],
        };
        for i in 0..w * h {
            let (y, cb, cr) = (planes[0][i] as i32, planes[1][i] as i32 - 128, planes[2][i] as i32 - 128);
            if rgb {
                out.extend_from_slice(&[planes[0][i], planes[1][i], planes[2][i], 255]);
                continue;
            }
            let r = y + ((91881 * cr + 32768) >> 16);
            let g = y + ((-22554 * cb + 32768 - 46802 * cr) >> 16);
            let b = y + ((116130 * cb + 32768) >> 16);
            out.extend_from_slice(&[r.clamp(0, 255) as u8, g.clamp(0, 255) as u8, b.clamp(0, 255) as u8, 255]);
        }
        Ok((w, h, out))
    }
}

fn baseline(bits: &mut Bits, block: &mut [i16], dc: Option<&Huffman>, ac: Option<&Huffman>, pred: &mut i32) -> Res<()> {
    let (Some(dc), Some(ac)) = (dc, ac) else { return bad("no table") };
    let s = bits.decode(dc)? as u32;
    *pred = pred.wrapping_add(bits.extend(s.min(16)));
    block[0] = *pred as i16;
    let mut k = 1;
    while k < 64 {
        let rs = bits.decode(ac)?;
        let (r, s) = ((rs >> 4) as usize, (rs & 15) as u32);
        if s == 0 {
            if r != 15 {
                break;
            }
            k += 16;
            continue;
        }
        k += r;
        let v = bits.extend(s);
        if k < 64 {
            block[ZIGZAG[k]] = v as i16;
        }
        k += 1;
    }
    Ok(())
}

fn ac_first(bits: &mut Bits, block: &mut [i16], ac: &Huffman, scan: &Scan, eobrun: &mut u32) -> Res<()> {
    if *eobrun > 0 {
        *eobrun -= 1;
        return Ok(());
    }
    let mut k = scan.ss;
    while k <= scan.se {
        let rs = bits.decode(ac)?;
        let (r, s) = ((rs >> 4) as u32, (rs & 15) as u32);
        if s != 0 {
            k += r as usize;
            let v = bits.extend(s);
            if k < 64 {
                block[ZIGZAG[k]] = ((v as u32) << scan.al) as i16;
            }
        } else if r == 15 {
            k += 15;
        } else {
            *eobrun = (1 << r) + bits.receive(r) as u32 - 1;
            break;
        }
        k += 1;
    }
    Ok(())
}

fn ac_refine(bits: &mut Bits, block: &mut [i16], ac: &Huffman, scan: &Scan, eobrun: &mut u32) -> Res<()> {
    let (p1, m1) = (1i16 << scan.al, -1i16 << scan.al);
    let refine = |bits: &mut Bits, c: &mut i16| {
        if bits.bit() != 0 && *c & p1 == 0 {
            *c = c.wrapping_add(if *c >= 0 { p1 } else { m1 });
        }
    };
    let mut k = scan.ss;
    if *eobrun == 0 {
        while k <= scan.se {
            let rs = bits.decode(ac)?;
            let (mut r, s) = ((rs >> 4) as i32, (rs & 15) as u32);
            let mut value = 0;
            if s != 0 {
                value = if bits.bit() != 0 { p1 } else { m1 };
            } else if r != 15 {
                *eobrun = (1 << r) + bits.receive(r as u32) as u32;
                break;
            }
            // Step over the coefficients that are set already, and `r` that are still zero
            while k <= scan.se {
                let c = &mut block[ZIGZAG[k]];
                if *c != 0 {
                    refine(bits, c);
                } else {
                    r -= 1;
                    if r < 0 {
                        break;
                    }
                }
                k += 1;
            }
            if value != 0 && k < 64 {
                block[ZIGZAG[k]] = value;
            }
            k += 1;
        }
    }
    if *eobrun > 0 {
        while k <= scan.se {
            let c = &mut block[ZIGZAG[k]];
            if *c != 0 {
                refine(bits, c);
            }
            k += 1;
        }
        *eobrun -= 1;
    }
    Ok(())
}

/// libjpeg's table that turns the inverse DCT's result into a sample.
fn range_limit(v: i32) -> u8 {
    match v & 1023 {
        i @ 0..=127 => (i + 128) as u8,
        128..=511 => 255,
        512..=895 => 0,
        i => (i - 896) as u8,
    }
}

/// The accurate integer inverse DCT of `jidctint.c`, on a block of coefficients in natural order.
fn idct(coefs: &[i16], q: &[u16; 64]) -> [u8; 64] {
    const C: i32 = 13;
    const P: i32 = 2;
    fn pass(i: [i32; 8], shift: i32, dc_shift: i32) -> [i32; 8] {
        let m = |a: i32, b: i32| a.wrapping_mul(b);
        let z1 = m(i[2].wrapping_add(i[6]), 4433);
        let tmp2 = z1.wrapping_add(m(i[6], -15137));
        let tmp3 = z1.wrapping_add(m(i[2], 6270));
        let tmp0 = i[0].wrapping_add(i[4]) << dc_shift;
        let tmp1 = i[0].wrapping_sub(i[4]) << dc_shift;
        let (tmp10, tmp13) = (tmp0.wrapping_add(tmp3), tmp0.wrapping_sub(tmp3));
        let (tmp11, tmp12) = (tmp1.wrapping_add(tmp2), tmp1.wrapping_sub(tmp2));
        let (t0, t1, t2, t3) = (i[7], i[5], i[3], i[1]);
        let (z1, z2, z3, z4) = (t0.wrapping_add(t3), t1.wrapping_add(t2), t0.wrapping_add(t2), t1.wrapping_add(t3));
        let z5 = m(z3.wrapping_add(z4), 9633);
        let (t0, t1, t2, t3) = (m(t0, 2446), m(t1, 16819), m(t2, 25172), m(t3, 12299));
        let (z1, z2) = (m(z1, -7373), m(z2, -20995));
        let (z3, z4) = (m(z3, -16069).wrapping_add(z5), m(z4, -3196).wrapping_add(z5));
        let t0 = t0.wrapping_add(z1).wrapping_add(z3);
        let t1 = t1.wrapping_add(z2).wrapping_add(z4);
        let t2 = t2.wrapping_add(z2).wrapping_add(z3);
        let t3 = t3.wrapping_add(z1).wrapping_add(z4);
        let d = |x: i32| x.wrapping_add(1 << (shift - 1)) >> shift;
        [
            d(tmp10.wrapping_add(t3)),
            d(tmp11.wrapping_add(t2)),
            d(tmp12.wrapping_add(t1)),
            d(tmp13.wrapping_add(t0)),
            d(tmp13.wrapping_sub(t0)),
            d(tmp12.wrapping_sub(t1)),
            d(tmp11.wrapping_sub(t2)),
            d(tmp10.wrapping_sub(t3)),
        ]
    }
    let mut ws = [0i32; 64];
    for x in 0..8 {
        let col = [0, 1, 2, 3, 4, 5, 6, 7].map(|y| coefs[y * 8 + x] as i32 * q[y * 8 + x] as i32);
        let out = pass(col, C - P, C);
        for y in 0..8 {
            ws[y * 8 + x] = out[y];
        }
    }
    let mut out = [0u8; 64];
    for y in 0..8 {
        let row = pass(ws[y * 8..y * 8 + 8].try_into().unwrap_or([0; 8]), C + P + 3, C);
        for x in 0..8 {
            out[y * 8 + x] = range_limit(row[x]);
        }
    }
    out
}

/// A component's plane at the picture's size, as libjpeg's upsampler makes it. The plane has `cw` by `ch` samples
/// of picture data in rows of `stride`. `fx` and `fy` are how many times it's enlarged.
#[allow(clippy::too_many_arguments)]
fn upsample(plane: &[u8], stride: usize, cw: usize, ch: usize, fx: usize, fy: usize, odd: usize, w: usize, h: usize) -> Res<Vec<u8>> {
    if odd != 0 {
        return Err("JPEG files with fractional sampling aren't supported".into());
    }
    let row = |y: usize| &plane[y * stride..y * stride + cw];
    // The rows after enlarging down: each one from its row and, for the smooth kinds, the row next to it
    let fancy_h = fx == 2 && (fy == 1 || fy == 2) && cw > 2;
    let fancy_v = (fy == 2 && fx == 1) || (fy == 2 && fancy_h);
    let mut out = Vec::with_capacity(w * h);
    let mut line = vec![0i32; cw];
    for y in 0..h {
        let src = (y / fy).min(ch - 1);
        // The sums libjpeg forms for a row: 3 parts of the near row and 1 of the far one
        if fancy_v {
            let far = if y % 2 == 0 { src.saturating_sub(1) } else { (src + 1).min(ch - 1) };
            for (x, v) in line.iter_mut().enumerate() {
                *v = row(src)[x] as i32 * 3 + row(far)[x] as i32;
            }
        } else {
            for (x, v) in line.iter_mut().enumerate() {
                *v = row(src)[x] as i32;
            }
        }
        if fancy_h && fancy_v {
            // h2v2_fancy_upsample
            for x in 0..w {
                let c = (x / 2).min(cw - 1);
                let this = line[c];
                out.push(if x % 2 == 0 {
                    if c == 0 { (this * 4 + 8) >> 4 } else { (this * 3 + line[c - 1] + 8) >> 4 }
                } else if c == cw - 1 {
                    (this * 4 + 7) >> 4
                } else {
                    (this * 3 + line[c + 1] + 7) >> 4
                } as u8);
            }
        } else if fancy_h {
            // h2v1_fancy_upsample
            for x in 0..w {
                let c = (x / 2).min(cw - 1);
                let this = line[c];
                out.push(if x % 2 == 0 {
                    if c == 0 { this } else { (this * 3 + line[c - 1] + 1) >> 2 }
                } else if c == cw - 1 {
                    this
                } else {
                    (this * 3 + line[c + 1] + 2) >> 2
                } as u8);
            }
        } else if fancy_v {
            // h1v2_fancy_upsample
            let bias = if y % 2 == 0 { 1 } else { 2 };
            out.extend((0..w).map(|x| ((line[x.min(cw - 1)] + bias) >> 2) as u8));
        } else {
            out.extend((0..w).map(|x| line[(x / fx).min(cw - 1)] as u8));
        }
    }
    Ok(out)
}
