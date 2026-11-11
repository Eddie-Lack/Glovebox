//! Python's `random.Random` for the kit: the Mersenne Twister with CPython's seeding and the methods the models call.
//!
//! The kit's pictures and shapes come from seeded random numbers. The same seed must give the same numbers as CPython.
//! Only whole number seeds are supported, because the kit uses no other kind.

const N: usize = 624;
const M: usize = 397;

/// A generator that gives the same numbers as `random.Random(seed)`.
#[derive(Clone)]
pub struct Random {
    mt: [u32; N],
    index: usize,
}

impl Random {
    /// `random.Random(seed)` for a whole number that is not negative.
    pub fn new(seed: u64) -> Random {
        // CPython passes the number as 32 bit words, lowest first, to init_by_array.
        let mut key = vec![seed as u32];
        if seed >> 32 != 0 {
            key.push((seed >> 32) as u32);
        }
        let mut r = Random { mt: [0; N], index: N };
        r.init_genrand(19650218);
        let mt = &mut r.mt;
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
        r
    }

    fn init_genrand(&mut self, s: u32) {
        self.mt[0] = s;
        for i in 1..N {
            self.mt[i] = 1812433253u32.wrapping_mul(self.mt[i - 1] ^ (self.mt[i - 1] >> 30)).wrapping_add(i as u32);
        }
        self.index = N;
    }

    /// The next 32 bits.
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

    /// `random()`: a number from 0 up to, not including, 1, with 53 bits.
    pub fn random(&mut self) -> f64 {
        let a = (self.next_u32() >> 5) as f64;
        let b = (self.next_u32() >> 6) as f64;
        (a * 67108864.0 + b) * (1.0 / 9007199254740992.0)
    }

    /// `uniform(a, b)`.
    pub fn uniform(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.random()
    }

    /// CPython's `_randbelow`: a whole number from 0 up to, not including, `n`. `n` must be from 1 to 2^32 - 1.
    fn below(&mut self, n: u32) -> u32 {
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

    /// `randrange(start, stop)`. `stop` must be greater than `start`.
    pub fn randrange(&mut self, start: i64, stop: i64) -> i64 {
        start + self.below((stop - start).clamp(0, u32::MAX as i64) as u32) as i64
    }

    /// `randint(a, b)`, both ends included.
    pub fn randint(&mut self, a: i64, b: i64) -> i64 {
        self.randrange(a, b + 1)
    }

    /// `choice(seq)`. `seq` must not be empty.
    pub fn choice<T: Copy>(&mut self, seq: &[T]) -> T {
        seq[self.below(seq.len() as u32) as usize]
    }
}

/// Python's `math.hypot(x, y)`. CPython has its own algorithm, which can differ from the C library's by one unit in the last place.
pub fn hypot(x: f64, y: f64) -> f64 {
    let (x, y) = (x.abs(), y.abs());
    let max = x.max(y);
    if x.is_infinite() || y.is_infinite() {
        return f64::INFINITY;
    }
    if x.is_nan() || y.is_nan() {
        return f64::NAN;
    }
    if max == 0.0 {
        return 0.0;
    }
    // frexp's exponent: max = m * 2^e with 0.5 <= m < 1.
    let bits = max.to_bits();
    let raw = ((bits >> 52) & 0x7ff) as i32;
    if raw == 0 {
        // A subnormal largest value does not happen in the kit. The C library's answer is close enough.
        return x.hypot(y);
    }
    let e = raw - 1022;
    let scale = f64::from_bits(((1023 - e) as u64) << 52);
    let (mut csum, mut frac1, mut frac2) = (1.0f64, 0.0f64, 0.0f64);
    for v in [x, y] {
        let v = v * scale;
        let hi = v * v;
        let lo = v.mul_add(v, -hi);
        let s = csum + hi;
        let slo = (csum - s) + hi;
        csum = s;
        frac1 += lo;
        frac2 += slo;
    }
    let mut h = (csum - 1.0 + (frac1 + frac2)).sqrt();
    let hi = -h * h;
    let lo = (-h).mul_add(h, -hi);
    let s = csum + hi;
    let slo = (csum - s) + hi;
    csum = s;
    frac1 += lo;
    frac2 += slo;
    let v = csum - 1.0 + (frac1 + frac2);
    h += v / (2.0 * h);
    h / scale
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_numbers_match_the_reference_generator() {
        // MT19937 seeded with init_by_array([1]) is what random.Random(1) uses. Its first float is well known.
        let mut r = Random::new(1);
        assert_eq!(r.random(), 0.13436424411240122);
        let mut r = Random::new(0);
        assert_eq!(r.random(), 0.8444218515250481);
    }

    #[test]
    fn whole_numbers_stay_in_range() {
        let mut r = Random::new(7);
        for _ in 0..1000 {
            let v = r.randint(-3, 5);
            assert!((-3..=5).contains(&v));
            assert!([2, 4, 6].contains(&r.choice(&[2, 4, 6])));
        }
    }

    #[test]
    fn hypot_of_a_right_triangle() {
        assert_eq!(hypot(3.0, 4.0), 5.0);
        assert_eq!(hypot(0.0, 0.0), 0.0);
        assert_eq!(hypot(-2.5, 0.0), 2.5);
    }
}
