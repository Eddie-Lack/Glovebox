//! FLA2, the compression of the game's banks: LZ with a 4096-byte window. Ported from libgarib (see
//! THIRD_PARTY_NOTICES.md).

const MAGIC: &[u8; 4] = b"FLA2";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fla2Error(pub &'static str);

impl std::fmt::Display for Fla2Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for Fla2Error {}

pub fn is_fla2(data: &[u8]) -> bool {
    data.len() >= 8 && &data[..4] == MAGIC
}

/// The data of an FLA2 stream. Every read is bounds-checked.
pub fn decompress(src: &[u8]) -> Result<Vec<u8>, Fla2Error> {
    if !is_fla2(src) {
        return Err(Fla2Error("not an FLA2 stream"));
    }
    let length = u32::from_le_bytes([src[4], src[5], src[6], src[7]]) as usize;
    if length > 64 << 20 {
        return Err(Fla2Error("the FLA2 stream gives a size over 64 MiB"));
    }
    let mut out = Vec::with_capacity(length);
    let mut window = [0u8; 0x1000];
    let mut cursor = 0usize;
    let mut i = 8;
    loop {
        let cmd = *src.get(i).ok_or(Fla2Error("the FLA2 stream ends early"))?;
        i += 1;
        for bit in (0..8).rev() {
            if cmd >> bit & 1 == 0 {
                let b = *src.get(i).ok_or(Fla2Error("the FLA2 stream ends early"))?;
                i += 1;
                out.push(b);
                window[cursor] = b;
                cursor = (cursor + 1) & 0xFFF;
            } else {
                if i + 2 > src.len() {
                    return Err(Fla2Error("the FLA2 stream ends early"));
                }
                let (b0, b1) = (src[i] as usize, src[i + 1] as usize);
                i += 2;
                if b0 == 0 && b1 == 0 {
                    if out.len() != length {
                        return Err(Fla2Error("the FLA2 stream ends early"));
                    }
                    return Ok(out);
                }
                let len = (b0 & 0x0F) + 2;
                let dist = ((b0 & 0xF0) << 4) + b1;
                let mut rd = cursor.wrapping_sub(dist) & 0xFFF;
                for _ in 0..len {
                    let b = window[rd];
                    rd = (rd + 1) & 0xFFF;
                    window[cursor] = b;
                    cursor = (cursor + 1) & 0xFFF;
                    out.push(b);
                }
            }
            if out.len() > length {
                return Err(Fla2Error("the FLA2 stream is longer than it says"));
            }
        }
    }
}

/// A bank's data: decompressed when it is an FLA2 stream, as it is otherwise.
pub fn data(src: &[u8]) -> Result<Vec<u8>, Fla2Error> {
    if src.len() >= 4 && &src[..4] == MAGIC {
        decompress(src)
    } else {
        Ok(src.to_vec())
    }
}

/// Compresses the way libgarib's reference compressor does, byte for byte.
pub fn compress(src: &[u8]) -> Vec<u8> {
    let n = src.len();
    let mut dst = Vec::with_capacity(n / 2 + 16);
    dst.extend_from_slice(MAGIC);
    dst.extend_from_slice(&(n as u32).to_le_bytes());
    // The reference starts with an empty window: an unset cell matches nothing.
    let mut window = [-1i32; 0x1000];
    let mut wr = 0usize;
    let mut rd = 0usize;
    let mut done = n == 0;
    if done {
        // The reference reads one byte before it checks the length; an empty input has no stream.
        dst.extend_from_slice(&[0x80, 0, 0]);
        return dst;
    }
    while !done {
        let cmd_at = dst.len();
        dst.push(0);
        let mut cmd = 0u8;
        let mut last_bit = 0;
        for bit in 0..8 {
            last_bit = bit;
            let next = src[rd] as i32;
            rd += 1;
            window[wr] = next;
            wr = (wr + 1) & 0xFFF;
            let mut best: Option<(usize, usize)> = None;
            let mut search = wr.wrapping_sub(2) & 0xFFF;
            while (search.wrapping_sub(wr) & 0xFFF) >= 1 {
                if window[search] == next {
                    let mut local_wr = wr;
                    let mut sc = rd;
                    let mut wrd = (search + 1) & 0xFFF;
                    // The reference writes the bytes it tries into the window and leaves them there.
                    for _ in 0..16 {
                        if sc >= n {
                            break;
                        }
                        let nb = src[sc] as i32;
                        if window[wrd] != nb {
                            break;
                        }
                        window[local_wr] = nb;
                        local_wr = (local_wr + 1) & 0xFFF;
                        wrd = (wrd + 1) & 0xFFF;
                        sc += 1;
                    }
                    let len = wrd.wrapping_sub(search) & 0xFFF;
                    if len > best.map_or(0, |b| b.1) {
                        best = Some((search, len));
                    }
                    if (wr.wrapping_sub(search) & 0xFFF) <= len {
                        break;
                    }
                }
                search = search.wrapping_sub(1) & 0xFFF;
            }
            match best {
                Some((at, len)) if len > 2 => {
                    cmd |= 1 << (7 - bit);
                    let start = wr.wrapping_sub(1).wrapping_sub(at) & 0xFFF;
                    let blen = (len - 2) & 0xF;
                    let backref = ((start & 0xFF) << 8) | ((start & 0xF00) >> 4) | blen;
                    dst.extend_from_slice(&(backref as u16).to_le_bytes());
                    rd += blen + 1;
                    wr = (wr + blen + 1) & 0xFFF;
                }
                _ => dst.push(next as u8),
            }
            if rd >= n {
                done = true;
                if bit < 7 {
                    cmd |= 1 << (7 - (bit + 1));
                    dst.extend_from_slice(&[0, 0]);
                }
                break;
            }
        }
        dst[cmd_at] = cmd;
        if done && last_bit == 7 {
            dst.extend_from_slice(&[0x80, 0, 0]);
        }
    }
    dst
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let mut data = Vec::new();
        for i in 0..20000u32 {
            data.push((i % 7) as u8 ^ (i / 300) as u8);
        }
        data.extend_from_slice(b"abcabcabcabcabcabcabcabcabcabcabcabcabcabc");
        let packed = compress(&data);
        assert!(packed.len() < data.len());
        assert_eq!(decompress(&packed).unwrap(), data);
        assert!(decompress(&packed[..packed.len() - 4]).is_err());
        assert!(decompress(b"nope").is_err());
    }
}
