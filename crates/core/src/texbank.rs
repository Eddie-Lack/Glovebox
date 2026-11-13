//! Glover's texture banks: reading a bank, decoding a texture to RGBA pixels, and writing a bank from records.
//! Ported from libgarib's `textures.py` and `glover.texbank.ksy` (see THIRD_PARTY_NOTICES.md).
//!
//! A bank is a count, then one record for each texture, then one name for each texture. A record is a 36-byte
//! header and the pixel data. A palette, when the texture has one, follows the pixels inside the record.

use std::borrow::Cow;

/// The size of a record's header. The pixel data starts here.
pub const HEADER: usize = 36;

/// Values of `Texture::color_format`.
pub mod color {
    pub const RGBA: u16 = 0;
    pub const YUV: u16 = 1;
    pub const CI: u16 = 2;
    pub const IA: u16 = 3;
    pub const I: u16 = 4;
}

/// Values of `Texture::compression_format`: the size of one pixel.
pub mod compression {
    /// 4 bits for each pixel, an index into a palette of 16-bit colours
    pub const CI4: u16 = 0;
    /// 8 bits for each pixel, an index into a palette of 16-bit colours
    pub const CI8: u16 = 1;
    pub const UNCOMPRESSED_16B: u16 = 2;
    pub const UNCOMPRESSED_32B: u16 = 3;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TexError(pub String);

impl std::fmt::Display for TexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for TexError {}

fn err<T>(text: impl Into<String>) -> Result<T, TexError> {
    Err(TexError(text.into()))
}

fn u16_at(d: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([d[at], d[at + 1]])
}

fn u32_at(d: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

/// One texture of a bank: the fields of its header, and its data (pixels, then the palette).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Texture<'a> {
    pub id: u32,
    pub palette_anim_idx_min: u8,
    pub palette_anim_idx_max: u8,
    /// Bit 4 means the palette is animated. The others are not known.
    pub flags: u16,
    pub frame_increment: i16,
    pub frame_counter: i16,
    pub width: u16,
    pub height: u16,
    /// The base 2 logarithm of the width the game wraps at
    pub masks: u16,
    /// The base 2 logarithm of the height the game wraps at
    pub maskt: u16,
    /// The size of the record, header included
    pub length: u32,
    pub color_format: u16,
    pub compression_format: u16,
    pub data_ptr: u32,
    /// Where the palette starts in the record, or 0
    pub palette_offset: u32,
    /// The record without its header
    pub data: &'a [u8],
}

impl<'a> Texture<'a> {
    /// Reads the texture at the start of `rec`. `rec` may be longer than the record.
    pub fn parse(rec: &'a [u8]) -> Result<Texture<'a>, TexError> {
        if rec.len() < HEADER {
            return err("the texture's header is cut short");
        }
        let length = u32_at(rec, 20);
        if (length as usize) < HEADER || length as usize > rec.len() {
            return err(format!("texture 0x{:08X} has a bad size", u32_at(rec, 0)));
        }
        Ok(Texture {
            id: u32_at(rec, 0),
            palette_anim_idx_min: rec[4],
            palette_anim_idx_max: rec[5],
            flags: u16_at(rec, 6),
            frame_increment: u16_at(rec, 8) as i16,
            frame_counter: u16_at(rec, 10) as i16,
            width: u16_at(rec, 12),
            height: u16_at(rec, 14),
            masks: u16_at(rec, 16),
            maskt: u16_at(rec, 18),
            length,
            color_format: u16_at(rec, 24),
            compression_format: u16_at(rec, 26),
            data_ptr: u32_at(rec, 28),
            palette_offset: u32_at(rec, 32),
            data: &rec[HEADER..length as usize],
        })
    }

    /// The header's 36 bytes.
    pub fn header(&self) -> [u8; HEADER] {
        let mut h = [0u8; HEADER];
        h[0..4].copy_from_slice(&self.id.to_be_bytes());
        h[4] = self.palette_anim_idx_min;
        h[5] = self.palette_anim_idx_max;
        h[6..8].copy_from_slice(&self.flags.to_be_bytes());
        h[8..10].copy_from_slice(&self.frame_increment.to_be_bytes());
        h[10..12].copy_from_slice(&self.frame_counter.to_be_bytes());
        h[12..14].copy_from_slice(&self.width.to_be_bytes());
        h[14..16].copy_from_slice(&self.height.to_be_bytes());
        h[16..18].copy_from_slice(&self.masks.to_be_bytes());
        h[18..20].copy_from_slice(&self.maskt.to_be_bytes());
        h[20..24].copy_from_slice(&self.length.to_be_bytes());
        h[24..26].copy_from_slice(&self.color_format.to_be_bytes());
        h[26..28].copy_from_slice(&self.compression_format.to_be_bytes());
        h[28..32].copy_from_slice(&self.data_ptr.to_be_bytes());
        h[32..36].copy_from_slice(&self.palette_offset.to_be_bytes());
        h
    }

    /// True when the pixels are indices into a palette.
    pub fn has_palette(&self) -> bool {
        self.color_format != color::IA
            && matches!(self.compression_format, compression::CI4 | compression::CI8)
    }

    /// The palette's colours as RGBA, as libgarib decodes them.
    pub fn palette(&self) -> Result<Vec<[u8; 4]>, TexError> {
        let Some(start) = (self.palette_offset as usize).checked_sub(HEADER) else {
            return err(format!("texture 0x{:08X} has a bad palette offset", self.id));
        };
        let raw = self.data.get(start..).unwrap_or(&[]);
        if raw.len() % 2 != 0 {
            return err(format!("texture 0x{:08X} has half a palette entry", self.id));
        }
        Ok(raw.chunks_exact(2).map(|c| decode_rgba16(u16::from_be_bytes([c[0], c[1]]))).collect())
    }

    /// The pixels as RGBA, 4 bytes for each pixel, rows from the top. They equal the PNG libgarib's
    /// `texbank-tool unpack` writes, read as RGBA.
    pub fn decode(&self) -> Result<Vec<u8>, TexError> {
        let (w, h) = (self.width as usize, self.height as usize);
        let n = w * h;
        if n == 0 {
            return err(format!("texture 0x{:08X} has no pixels", self.id));
        }
        let short = || TexError(format!("texture 0x{:08X} has too little pixel data", self.id));
        let mut out = Vec::with_capacity(n * 4);
        match (self.color_format, self.compression_format) {
            (color::IA, compression::UNCOMPRESSED_16B) => {
                for c in self.data.get(..n * 2).ok_or_else(short)?.chunks_exact(2) {
                    out.extend_from_slice(&[c[0], c[0], c[0], c[1]]);
                }
            }
            (color::IA, _) => {
                return err(format!(
                    "Unsupported image format ({}/{}) for texture {}",
                    self.compression_format, self.color_format, self.id
                ))
            }
            (_, compression::CI4) | (_, compression::CI8) => {
                let palette = self.palette()?;
                let four = self.compression_format == compression::CI4;
                // libgarib leaves the pixels it has no data for at index 0.
                let bytes = if four { n / 2 } else { n };
                let indices = &self.data[..bytes.min(self.data.len())];
                let mut put = |i: usize| {
                    if palette.is_empty() {
                        // An image without a palette keeps the image library's grey ramp.
                        out.extend_from_slice(&[i as u8, i as u8, i as u8, 255]);
                    } else {
                        // An index past the palette is opaque black in the PNG.
                        out.extend_from_slice(palette.get(i).unwrap_or(&[0, 0, 0, 255]));
                    }
                };
                let mut count = 0;
                for &b in indices {
                    if four {
                        put((b >> 4) as usize);
                        put((b & 15) as usize);
                        count += 2;
                    } else {
                        put(b as usize);
                        count += 1;
                    }
                }
                for _ in count..n {
                    put(0);
                }
                out.truncate(n * 4);
            }
            (_, compression::UNCOMPRESSED_16B) => {
                for c in self.data.get(..n * 2).ok_or_else(short)?.chunks_exact(2) {
                    out.extend_from_slice(&decode_rgba16(u16::from_be_bytes([c[0], c[1]])));
                }
            }
            (_, compression::UNCOMPRESSED_32B) => {
                out.extend_from_slice(self.data.get(..n * 4).ok_or_else(short)?);
            }
            _ => {
                return err(format!(
                    "Unsupported image format ({}/{}) for texture {}",
                    self.compression_format, self.color_format, self.id
                ))
            }
        }
        Ok(out)
    }
}

/// A 16-bit colour (5 bits each of red, green and blue, 1 bit of alpha) as RGBA. libgarib divides by 32, not 31,
/// so full red is 240.
pub fn decode_rgba16(raw: u16) -> [u8; 4] {
    let c = |v: u16| ((v & 31) as u32 * 255 / 32) as u8;
    [c(raw >> 11), c(raw >> 6), c(raw >> 1), (raw & 1) as u8 * 255]
}

/// A parsed bank.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TexBank<'a> {
    pub textures: Vec<Texture<'a>>,
    /// One name for each texture, empty when the bank has none. The game's own banks have none.
    pub filenames: Vec<&'a [u8]>,
}

impl<'a> TexBank<'a> {
    /// Reads a bank. `data` is not compressed (see `fla2::data`).
    pub fn parse(data: &'a [u8]) -> Result<TexBank<'a>, TexError> {
        if data.len() < 4 {
            return err("the texture bank is cut short");
        }
        let n = u32_at(data, 0) as usize;
        if n > data.len() / HEADER {
            return err("the texture bank's count is too large");
        }
        let mut pos = 4;
        let mut textures = Vec::with_capacity(n);
        for _ in 0..n {
            let t = Texture::parse(&data[pos..])?;
            pos += t.length as usize;
            textures.push(t);
        }
        let mut filenames = Vec::with_capacity(n);
        for _ in 0..n {
            let rest = &data[pos..];
            let end = rest.iter().position(|&b| b == 0);
            filenames.push(&rest[..end.unwrap_or(rest.len())]);
            pos += end.map_or(rest.len(), |e| e + 1);
        }
        Ok(TexBank { textures, filenames })
    }

    pub fn get(&self, id: u32) -> Option<&Texture<'a>> {
        self.textures.iter().find(|t| t.id == id)
    }
}

/// The ids of a bank's textures, in the bank's order.
pub fn ids(data: &[u8]) -> Result<Vec<u32>, TexError> {
    Ok(TexBank::parse(data)?.textures.iter().map(|t| t.id).collect())
}

/// A bank's records and names.
pub struct Records<'a> {
    /// Each texture's id and its whole record. A record whose size was not a multiple of 8 has zeros at its end
    /// and a corrected size field.
    pub records: Vec<(u32, Cow<'a, [u8]>)>,
    /// What follows the records, split at the zero bytes, at most one piece for each record. A bank without names
    /// gives one empty name.
    pub names: Vec<&'a [u8]>,
}

/// Splits a bank into records. The pixels of the next record start 4 bytes into an 8-byte word of the bank. Every
/// record's size must be a multiple of 8, or the next record's pixels are not on a 64-bit boundary. The console
/// needs that boundary when it loads a texture.
pub fn records(data: &[u8]) -> Result<Records<'_>, TexError> {
    if data.len() < 4 {
        return err("the texture bank is cut short");
    }
    let n = u32_at(data, 0) as usize;
    if n > data.len() / 24 {
        return err("the texture bank's count is too large");
    }
    let mut pos = 4;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        if data.len() - pos < 24 {
            return err("a texture record is cut short");
        }
        let id = u32_at(data, pos);
        let length = u32_at(data, pos + 20) as usize;
        if length < 24 || length > data.len() - pos {
            return err(format!("texture 0x{id:08X} has a bad size"));
        }
        let rec = &data[pos..pos + length];
        if length % 8 == 0 {
            out.push((id, Cow::Borrowed(rec)));
        } else {
            let mut padded = rec.to_vec();
            padded.resize(length + (8 - length % 8), 0);
            let size = padded.len() as u32;
            padded[20..24].copy_from_slice(&size.to_be_bytes());
            out.push((id, Cow::Owned(padded)));
        }
        pos += length;
    }
    let names = data[pos..].split(|&b| b == 0).take(n).collect();
    Ok(Records { records: out, names })
}

/// A bank from records and names: the count, the records, then each name with a zero byte after it.
pub fn write_bank<R: AsRef<[u8]>, N: AsRef<[u8]>>(records: &[R], names: &[N]) -> Vec<u8> {
    let size = 4
        + records.iter().map(|r| r.as_ref().len()).sum::<usize>()
        + names.iter().map(|n| n.as_ref().len() + 1).sum::<usize>();
    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(&(records.len() as u32).to_be_bytes());
    for r in records {
        out.extend_from_slice(r.as_ref());
    }
    for n in names {
        out.extend_from_slice(n.as_ref());
        out.push(0);
    }
    out
}

/// The name the editor gives a texture it adds to a bank: `0x%08X.png`.
pub fn png_name(id: u32) -> Vec<u8> {
    format!("0x{id:08X}.png").into_bytes()
}

/// `target` with the records of `extra` added after its own, as a build writes a level's texture
/// bank. Each added record gets the name `png_name` gives its id.
pub fn import_bank(target: &[u8], extra: &[(u32, &[u8])]) -> Result<Vec<u8>, TexError> {
    let base = records(target)?;
    let mut recs: Vec<&[u8]> = base.records.iter().map(|(_, r)| r.as_ref()).collect();
    recs.extend(extra.iter().map(|(_, r)| *r));
    let mut names: Vec<Cow<[u8]>> = base.names.iter().map(|n| Cow::Borrowed(*n)).collect();
    names.extend(extra.iter().map(|(id, _)| Cow::Owned(png_name(*id))));
    Ok(write_bank(&recs, &names))
}

/// A bank of these records alone, each named by `png_name`. A reskinned model's texture bank is written this way.
pub fn bank_of(records: &[(u32, &[u8])]) -> Vec<u8> {
    let recs: Vec<&[u8]> = records.iter().map(|(_, r)| *r).collect();
    let names: Vec<Vec<u8>> = records.iter().map(|(id, _)| png_name(*id)).collect();
    write_bank(&recs, &names)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: u32, w: u16, h: u16, cf: u16, comp: u16, palette_offset: u32, data: &[u8]) -> Vec<u8> {
        let t = Texture {
            id,
            palette_anim_idx_min: 0,
            palette_anim_idx_max: 0,
            flags: 0,
            frame_increment: 0,
            frame_counter: 0,
            width: w,
            height: h,
            masks: 1,
            maskt: 1,
            length: (HEADER + data.len()) as u32,
            color_format: cf,
            compression_format: comp,
            data_ptr: 36,
            palette_offset,
            data,
        };
        let mut r = t.header().to_vec();
        r.extend_from_slice(data);
        r
    }

    #[test]
    fn rgba16() {
        assert_eq!(decode_rgba16(0xFFFF), [247, 247, 247, 255]);
        assert_eq!(decode_rgba16(0xF800), [247, 0, 0, 0]);
        assert_eq!(decode_rgba16(0x0001), [0, 0, 0, 255]);
        assert_eq!(decode_rgba16(1 << 6), [0, 7, 0, 0]);
    }

    #[test]
    fn header_round_trip() {
        let r = record(0x12345678, 2, 2, color::RGBA, compression::UNCOMPRESSED_32B, 0, &[9; 16]);
        let t = Texture::parse(&r).unwrap();
        assert_eq!(t.id, 0x12345678);
        assert_eq!(t.header()[..], r[..HEADER]);
        assert_eq!(t.decode().unwrap(), vec![9; 16]);
    }

    #[test]
    fn decode_formats() {
        // 2 by 2, 4 bits for each pixel: indices 1, 0, 2, 1 and a palette of three colours
        let data = [0x10, 0x21, 0x00, 0x01, 0xF8, 0x01, 0x07, 0xC0];
        let r = record(1, 2, 2, color::RGBA, compression::CI4, 36 + 2, &data);
        let t = Texture::parse(&r).unwrap();
        assert!(t.has_palette());
        assert_eq!(
            t.decode().unwrap(),
            [247, 0, 0, 255, 0, 0, 0, 255, 0, 247, 0, 0, 247, 0, 0, 255]
        );
        // 8 bits for each pixel, an index past the palette, and a missing pixel
        let data = [0x01, 0x05, 0x00, 0x00, 0x01, 0xF8, 0x01];
        let r = record(1, 2, 2, color::RGBA, compression::CI8, 36 + 3, &data);
        let px = Texture::parse(&r).unwrap().decode().unwrap();
        assert_eq!(px, [247, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255]);
        // intensity and alpha
        let r = record(1, 1, 2, color::IA, compression::UNCOMPRESSED_16B, 0, &[5, 6, 7, 8]);
        assert_eq!(Texture::parse(&r).unwrap().decode().unwrap(), [5, 5, 5, 6, 7, 7, 7, 8]);
        let r = record(1, 1, 2, color::IA, compression::CI8, 0, &[5, 6, 7, 8]);
        assert!(Texture::parse(&r).unwrap().decode().is_err());
        let r = record(1, 1, 1, color::RGBA, compression::UNCOMPRESSED_16B, 0, &[0xFF, 0xFF]);
        assert_eq!(Texture::parse(&r).unwrap().decode().unwrap(), [247, 247, 247, 255]);
    }

    #[test]
    fn malformed_never_panics() {
        let r = record(1, 200, 200, color::RGBA, compression::UNCOMPRESSED_16B, 0, &[0; 8]);
        assert!(Texture::parse(&r).unwrap().decode().is_err());
        let r = record(1, 2, 2, color::RGBA, compression::CI8, 7, &[0; 8]);
        assert!(Texture::parse(&r).unwrap().decode().is_err());
        let r = record(1, 2, 2, color::RGBA, compression::CI8, 36 + 5, &[0; 8]);
        assert!(Texture::parse(&r).unwrap().decode().is_err());
        assert!(TexBank::parse(&[0, 0]).is_err());
        assert!(TexBank::parse(&[0xFF, 0xFF, 0xFF, 0xFF]).is_err());
        assert!(records(&[0, 0, 0, 1, 1, 2, 3]).is_err());
        let mut bank = vec![0, 0, 0, 1];
        bank.extend_from_slice(&r);
        for cut in 0..bank.len() {
            let _ = TexBank::parse(&bank[..cut]);
            let _ = records(&bank[..cut]);
            let _ = ids(&bank[..cut]);
        }
        let mut bad = bank.clone();
        bad[4 + 20..4 + 24].copy_from_slice(&7u32.to_be_bytes());
        assert!(TexBank::parse(&bad).is_err());
        assert!(records(&bad).is_err());
    }

    #[test]
    fn records_pad_to_eight() {
        let a = record(0xA, 1, 1, color::RGBA, compression::UNCOMPRESSED_16B, 0, &[1, 2]);
        let b = record(0xB, 1, 1, color::RGBA, compression::UNCOMPRESSED_32B, 0, &[1, 2, 3, 4]);
        assert_eq!((a.len(), b.len()), (38, 40));
        let bank = write_bank(&[&a, &b], &[b"a.png".as_slice()]);
        let parsed = TexBank::parse(&bank).unwrap();
        assert_eq!(parsed.filenames, [b"a.png".as_slice(), b""]);
        assert_eq!(ids(&bank).unwrap(), [0xA, 0xB]);
        let r = records(&bank).unwrap();
        assert_eq!(r.names, [b"a.png".as_slice(), b""]);
        assert_eq!(r.records[0].1.len(), 40);
        assert_eq!(u32_at(&r.records[0].1, 20), 40);
        assert_eq!(&r.records[0].1[..20], &a[..20]);
        assert_eq!(&r.records[0].1[36..], [1, 2, 0, 0]);
        assert_eq!(r.records[1].1.as_ref(), &b[..]);
        // A bank without names gives one empty name, and writing it again adds one zero byte.
        let bare = write_bank(&[&b], &[] as &[&[u8]]);
        let r = records(&bare).unwrap();
        assert_eq!(r.names, [b"".as_slice()]);
        let again = import_bank(&bare, &[(0xA, &a)]).unwrap();
        let mut want = vec![0, 0, 0, 2];
        want.extend_from_slice(&b);
        want.extend_from_slice(&a);
        want.extend_from_slice(b"\x000x0000000A.png\x00");
        assert_eq!(again, want);
        assert_eq!(bank_of(&[(0xA, &a)])[4 + 38..], *b"0x0000000A.png\x00");
    }
}
