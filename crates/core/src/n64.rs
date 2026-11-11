//! What every N64 ROM has in common: the byte order of a dump, the boot code, the two header CRCs, and
//! bounds-checked big-endian reads and writes.

use sha1::{Digest, Sha1};

/// SHA-1 of Glover (USA), NGVE, in big-endian (.z64) byte order.
pub const ROM_SHA1: &str = "270be17b3c8da9b88a7b99c2a545b0bce16837f4";

pub const HEADER_SIZE: usize = 0x40;
/// Where the two header CRCs are.
pub const HEADER_CRC_OFFSET: usize = 0x10;
/// The boot code checks the megabyte that starts here.
pub const CRC_START: usize = 0x1000;
pub const CRC_END: usize = 0x101000;

/// A read or a write outside the data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutOfBounds {
    pub offset: usize,
    pub size: usize,
    pub len: usize,
}

impl std::fmt::Display for OutOfBounds {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{} bytes at offset 0x{:X} are outside the data ({} bytes)", self.size, self.offset, self.len)
    }
}

impl std::error::Error for OutOfBounds {}

/// `size` bytes at `offset`.
pub fn bytes_at(data: &[u8], offset: usize, size: usize) -> Result<&[u8], OutOfBounds> {
    offset
        .checked_add(size)
        .and_then(|end| data.get(offset..end))
        .ok_or(OutOfBounds { offset, size, len: data.len() })
}

fn bytes_at_mut(data: &mut [u8], offset: usize, size: usize) -> Result<&mut [u8], OutOfBounds> {
    let len = data.len();
    offset
        .checked_add(size)
        .and_then(|end| data.get_mut(offset..end))
        .ok_or(OutOfBounds { offset, size, len })
}

pub fn read_u8(data: &[u8], offset: usize) -> Result<u8, OutOfBounds> {
    Ok(bytes_at(data, offset, 1)?[0])
}

pub fn read_u16(data: &[u8], offset: usize) -> Result<u16, OutOfBounds> {
    let b = bytes_at(data, offset, 2)?;
    Ok(u16::from_be_bytes([b[0], b[1]]))
}

pub fn read_i16(data: &[u8], offset: usize) -> Result<i16, OutOfBounds> {
    Ok(read_u16(data, offset)? as i16)
}

pub fn read_u32(data: &[u8], offset: usize) -> Result<u32, OutOfBounds> {
    let b = bytes_at(data, offset, 4)?;
    Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

pub fn write_u8(data: &mut [u8], offset: usize, value: u8) -> Result<(), OutOfBounds> {
    bytes_at_mut(data, offset, 1)?[0] = value;
    Ok(())
}

pub fn write_u16(data: &mut [u8], offset: usize, value: u16) -> Result<(), OutOfBounds> {
    bytes_at_mut(data, offset, 2)?.copy_from_slice(&value.to_be_bytes());
    Ok(())
}

pub fn write_u32(data: &mut [u8], offset: usize, value: u32) -> Result<(), OutOfBounds> {
    bytes_at_mut(data, offset, 4)?.copy_from_slice(&value.to_be_bytes());
    Ok(())
}

/// Copies `bytes` over the data at `offset`.
pub fn write_bytes(data: &mut [u8], offset: usize, bytes: &[u8]) -> Result<(), OutOfBounds> {
    bytes_at_mut(data, offset, bytes.len())?.copy_from_slice(bytes);
    Ok(())
}

/// The ROM in big-endian byte order (.z64), from any of the three dump formats. None when the first four bytes are
/// not an N64 header in one of those orders, or the length does not fit the order.
pub fn to_z64(rom: &[u8]) -> Option<Vec<u8>> {
    let head = rom.get(..4)?;
    if head == [0x80, 0x37, 0x12, 0x40] {
        return Some(rom.to_vec());
    }
    let mut out = rom.to_vec();
    if head == [0x37, 0x80, 0x40, 0x12] && rom.len() % 2 == 0 {
        // .v64: the two bytes of every 16 bit word are swapped
        for pair in out.chunks_exact_mut(2) {
            pair.swap(0, 1);
        }
    } else if head == [0x40, 0x12, 0x37, 0x80] && rom.len() % 4 == 0 {
        // .n64: little-endian 32 bit words
        for word in out.chunks_exact_mut(4) {
            word.reverse();
        }
    } else {
        return None;
    }
    Some(out)
}

/// SHA-1 of the data, as lower-case hex.
pub fn sha1_hex(data: &[u8]) -> String {
    let digest = Sha1::digest(data);
    let mut s = String::with_capacity(40);
    for b in digest {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

/// Whether this is the supported ROM, in .z64 byte order.
pub fn is_supported_rom(z64: &[u8]) -> bool {
    sha1_hex(z64) == ROM_SHA1
}

/// Why a file is not the ROM the editor supports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RomFileError {
    /// Not .z64, .v64 or .n64.
    NotN64,
    /// An N64 ROM, but another one. Holds its SHA-1 in .z64 byte order.
    WrongRom(String),
}

impl std::fmt::Display for RomFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            RomFileError::NotN64 => write!(f, "That file is not a Nintendo 64 ROM (.z64, .v64 or .n64)."),
            RomFileError::WrongRom(sha) => write!(
                f,
                "That ROM is not the one the editor supports. It needs Glover (USA), with SHA-1 {} once in .z64 byte \
                 order. This one's is {}.",
                ROM_SHA1, sha
            ),
        }
    }
}

impl std::error::Error for RomFileError {}

/// The supported ROM in .z64 byte order, from a dump in any of the three byte orders.
pub fn supported_rom(file: &[u8]) -> Result<Vec<u8>, RomFileError> {
    let rom = to_z64(file).ok_or(RomFileError::NotN64)?;
    let sha = sha1_hex(&rom);
    if sha != ROM_SHA1 {
        return Err(RomFileError::WrongRom(sha));
    }
    Ok(rom)
}

/// The boot code (CIC chip number) of a .z64 ROM, from the CRC-32 of 0x40 to 0x1000. None when it is not a known one.
pub fn identify_cic(rom: &[u8]) -> Option<u16> {
    let boot = rom.get(HEADER_SIZE..0x1000)?;
    match crc32fast::hash(boot) {
        0x6170A4A1 => Some(6101),
        0x90BB6CB5 => Some(6102),
        0x0B050EE0 => Some(6103),
        0x98BC2C86 => Some(6105),
        0xACC8580A => Some(6106),
        _ => None,
    }
}

/// The two header CRCs of a ROM with the CIC-NUS-6102 boot code (Glover has it; 6101 uses the same sums): over 0x1000
/// to 0x101000. An error when the ROM is shorter than that.
pub fn header_crc(rom: &[u8]) -> Result<(u32, u32), OutOfBounds> {
    let data = bytes_at(rom, CRC_START, CRC_END - CRC_START)?;
    let seed: u32 = 0xF8CA4DDC;
    let (mut t1, mut t2, mut t3, mut t4, mut t5, mut t6) = (seed, seed, seed, seed, seed, seed);
    for w in data.chunks_exact(4) {
        let d = u32::from_be_bytes([w[0], w[1], w[2], w[3]]);
        let (sum, carry) = t6.overflowing_add(d);
        if carry {
            t4 = t4.wrapping_add(1);
        }
        t6 = sum;
        t3 ^= d;
        let r = d.rotate_left(d & 0x1F);
        t5 = t5.wrapping_add(r);
        if t2 > d {
            t2 ^= r;
        } else {
            t2 ^= t6 ^ d;
        }
        t1 = t1.wrapping_add(t5 ^ d);
    }
    Ok((t6 ^ t4 ^ t3, t5 ^ t2 ^ t1))
}

/// The two CRCs the header holds now.
pub fn stored_crc(rom: &[u8]) -> Result<(u32, u32), OutOfBounds> {
    Ok((read_u32(rom, HEADER_CRC_OFFSET)?, read_u32(rom, HEADER_CRC_OFFSET + 4)?))
}

/// Recomputes the header CRCs and writes them into the header.
pub fn fix_header_crc(rom: &mut [u8]) -> Result<(u32, u32), OutOfBounds> {
    let crc = header_crc(rom)?;
    write_u32(rom, HEADER_CRC_OFFSET, crc.0)?;
    write_u32(rom, HEADER_CRC_OFFSET + 4, crc.1)?;
    Ok(crc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_writes_check_bounds() {
        let mut d = vec![1u8, 2, 3, 4, 5];
        assert_eq!(read_u32(&d, 0), Ok(0x01020304));
        assert_eq!(read_u32(&d, 1), Ok(0x02030405));
        assert!(read_u32(&d, 2).is_err());
        assert!(read_u32(&d, usize::MAX).is_err());
        assert_eq!(read_u16(&d, 3), Ok(0x0405));
        assert_eq!(read_i16(&[0xFF, 0xFE], 0), Ok(-2));
        assert!(read_u16(&d, 4).is_err());
        assert_eq!(read_u8(&d, 4), Ok(5));
        assert!(read_u8(&d, 5).is_err());
        write_u32(&mut d, 1, 0xAABBCCDD).unwrap();
        assert_eq!(d, [1, 0xAA, 0xBB, 0xCC, 0xDD]);
        assert!(write_u32(&mut d, 2, 0).is_err());
        write_u16(&mut d, 0, 0x1122).unwrap();
        write_u8(&mut d, 4, 9).unwrap();
        assert_eq!(d, [0x11, 0x22, 0xBB, 0xCC, 9]);
        assert!(write_bytes(&mut d, 3, &[1, 2, 3]).is_err());
        write_bytes(&mut d, 3, &[7, 8]).unwrap();
        assert_eq!(d, [0x11, 0x22, 0xBB, 7, 8]);
    }

    #[test]
    fn byte_orders() {
        let z: Vec<u8> = vec![0x80, 0x37, 0x12, 0x40, 1, 2, 3, 4];
        let v: Vec<u8> = vec![0x37, 0x80, 0x40, 0x12, 2, 1, 4, 3];
        let n: Vec<u8> = vec![0x40, 0x12, 0x37, 0x80, 4, 3, 2, 1];
        assert_eq!(to_z64(&z), Some(z.clone()));
        assert_eq!(to_z64(&v), Some(z.clone()));
        assert_eq!(to_z64(&n), Some(z.clone()));
        assert_eq!(to_z64(&v[..7]), None);
        assert_eq!(to_z64(&n[..6]), None);
        assert_eq!(to_z64(&z[..3]), None);
        assert_eq!(to_z64(&[0u8; 8]), None);
        assert_eq!(supported_rom(&[0u8; 8]), Err(RomFileError::NotN64));
        assert!(matches!(supported_rom(&v), Err(RomFileError::WrongRom(_))));
    }

    #[test]
    fn sha1_of_known_text() {
        assert_eq!(sha1_hex(b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
    }

    #[test]
    fn crc_needs_the_whole_megabyte() {
        assert!(header_crc(&vec![0u8; CRC_END - 1]).is_err());
        let mut rom = vec![0u8; CRC_END];
        let crc = header_crc(&rom).unwrap();
        assert_eq!(fix_header_crc(&mut rom).unwrap(), crc);
        assert_eq!(stored_crc(&rom).unwrap(), crc);
        assert_eq!(identify_cic(&rom), None);
    }
}
