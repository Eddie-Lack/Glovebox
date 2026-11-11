//! Python's `random.Random` seeded with a string: the Mersenne Twister with CPython's seeding, `getrandbits` and
//! `choice`.
//!
//! `reskin` of the former Python back end picked textures with `random.Random("...")`. The same string must give the same picks here.
//! `kit::rng` holds the generator for whole number seeds.

use sha2::{Digest, Sha512};

const N: usize = 624;
const M: usize = 397;

/// A generator that gives the same numbers as CPython's `random.Random(seed)` for a string seed.
#[derive(Clone)]
pub struct PyRandom {
    mt: [u32; N],
    index: usize,
}

impl PyRandom {
    /// `random.Random(text)`. CPython seeds with the big-endian number made of the text's UTF-8 bytes followed by
    /// their SHA-512 digest.
    pub fn from_str(text: &str) -> PyRandom {
        let mut bytes = text.as_bytes().to_vec();
        bytes.extend_from_slice(&Sha512::digest(text.as_bytes()));
        // The number as 32 bit words, lowest first, without leading zero words
        let mut key: Vec<u32> = bytes
            .rchunks(4)
            .map(|c| c.iter().fold(0u32, |acc, b| (acc << 8) | *b as u32))
            .collect();
        while key.len() > 1 && key.last() == Some(&0) {
            key.pop();
        }
        PyRandom::by_array(&key)
    }

    fn by_array(key: &[u32]) -> PyRandom {
        let mut mt = [0u32; N];
        mt[0] = 19650218;
        for i in 1..N {
            mt[i] = 1812433253u32.wrapping_mul(mt[i - 1] ^ (mt[i - 1] >> 30)).wrapping_add(i as u32);
        }
        let (mut i, mut j) = (1usize, 0usize);
        for _ in 0..N.max(key.len()) {
            mt[i] = (mt[i] ^ (mt[i - 1] ^ (mt[i - 1] >> 30)).wrapping_mul(1664525))
                .wrapping_add(key[j])
                .wrapping_add(j as u32);
            i += 1;
            j += 1;
            if i >= N {
                mt[0] = mt[N - 1];
                i = 1;
            }
            if j >= key.len() {
                j = 0;
            }
        }
        for _ in 0..N - 1 {
            mt[i] = (mt[i] ^ (mt[i - 1] ^ (mt[i - 1] >> 30)).wrapping_mul(1566083941)).wrapping_sub(i as u32);
            i += 1;
            if i >= N {
                mt[0] = mt[N - 1];
                i = 1;
            }
        }
        mt[0] = 0x8000_0000;
        PyRandom { mt, index: N }
    }

    /// `getrandbits(32)`.
    pub fn next_u32(&mut self) -> u32 {
        if self.index >= N {
            let mt = &mut self.mt;
            for k in 0..N {
                let y = (mt[k] & 0x8000_0000) | (mt[(k + 1) % N] & 0x7fff_ffff);
                let mag = if y & 1 != 0 { 0x9908_b0df } else { 0 };
                mt[k] = mt[(k + M) % N] ^ (y >> 1) ^ mag;
            }
            self.index = 0;
        }
        let mut y = self.mt[self.index];
        self.index += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^= y >> 18;
        y
    }

    /// CPython's `_randbelow`: a whole number from 0 up to, not including, `n`. `n` must be from 1 to 2^32 - 1.
    pub fn below(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        let k = 32 - n.leading_zeros();
        loop {
            let r = self.next_u32() >> (32 - k);
            if r < n {
                return r;
            }
        }
    }

    /// `choice(seq)`, or None for an empty list.
    pub fn choice<'a, T>(&mut self, seq: &'a [T]) -> Option<&'a T> {
        if seq.is_empty() || seq.len() > u32::MAX as usize {
            return None;
        }
        seq.get(self.below(seq.len() as u32) as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_seed_matches_cpython() {
        // random.Random("0x1 ['HUB'] 0"): choice(range(1000)), getrandbits(32), choice([1, 2, 3])
        let mut r = PyRandom::from_str("0x1 ['HUB'] 0");
        let seq: Vec<u32> = (0..1000).collect();
        assert_eq!(r.choice(&seq), Some(&467));
        assert_eq!(r.next_u32(), 872511034);
        assert_eq!(r.choice(&[1, 2, 3]), Some(&1));
    }
}
