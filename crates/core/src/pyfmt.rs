//! Numbers as text, the way the level XML has always written and read them.
//!
//! The level XML came from a Python tool. Its floats are Python's `repr` of a double ("1.0", "0.05000000074505806",
//! "1e-05"), and its integers above 10000 are hex. These functions write and read the same text.

/// Python's `repr(float)`: the shortest digits that read back to the same double, with ".0" on whole numbers and an
/// exponent below 1e-4 and from 1e16.
pub fn repr(v: f64) -> String {
    if v.is_nan() {
        return "nan".into();
    }
    if v.is_infinite() {
        return if v > 0.0 { "inf".into() } else { "-inf".into() };
    }
    if v == 0.0 {
        return if v.is_sign_negative() { "-0.0".into() } else { "0.0".into() };
    }
    // Rust's exponent form gives the shortest round-trip digits: d.ddddde-N
    let e = format!("{:e}", v.abs());
    // When two digit strings of that length read back to the same double, Python takes the one nearer to the exact
    // value, and the even one on a tie. Rust's shortest form does not always. Rounding to that length does.
    let e = {
        let n = e.split_once('e').map_or(1, |(m, _)| m.bytes().filter(|b| b.is_ascii_digit()).count());
        let rounded = format!("{:.*e}", n - 1, v.abs());
        if rounded.parse::<f64>() == Ok(v.abs()) { rounded } else { e }
    };
    let (mant, exp) = e.split_once('e').unwrap();
    let exp: i32 = exp.parse().unwrap();
    let digits: String = mant.chars().filter(|c| *c != '.').collect();
    let sign = if v < 0.0 { "-" } else { "" };
    let n = digits.len() as i32;
    if (-4..16).contains(&exp) {
        if exp >= n - 1 {
            format!("{}{}{}.0", sign, digits, "0".repeat((exp - (n - 1)) as usize))
        } else if exp >= 0 {
            format!("{}{}.{}", sign, &digits[..(exp + 1) as usize], &digits[(exp + 1) as usize..])
        } else {
            format!("{}0.{}{}", sign, "0".repeat((-exp - 1) as usize), digits)
        }
    } else {
        let m = if n == 1 { digits.clone() } else { format!("{}.{}", &digits[..1], &digits[1..]) };
        format!("{}{}e{}{:02}", sign, m, if exp < 0 { "-" } else { "+" }, exp.abs())
    }
}

/// Python's `str(float)` of a 32-bit float read from a file.
pub fn repr_f32(v: f32) -> String {
    repr(v as f64)
}

/// Python's `float(text)`: surrounding whitespace is allowed, as are "inf", "nan" and single underscores between
/// digits.
pub fn parse_float(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    let body = t.strip_prefix(['+', '-']).unwrap_or(t);
    let lower = body.to_ascii_lowercase();
    if matches!(lower.as_str(), "inf" | "infinity" | "nan") {
        return t.to_ascii_lowercase().replace("infinity", "inf").parse().ok();
    }
    // Rust accepts the same decimal grammar apart from underscores; keep letters other than e out.
    if !body.bytes().all(|b| b.is_ascii_digit() || matches!(b, b'.' | b'e' | b'E' | b'+' | b'-' | b'_')) {
        return None;
    }
    let cleaned = strip_underscores(t)?;
    if !cleaned.bytes().any(|b| b.is_ascii_digit()) {
        return None;
    }
    cleaned.parse().ok()
}

fn strip_underscores(t: &str) -> Option<String> {
    if !t.contains('_') {
        return Some(t.to_string());
    }
    let b = t.as_bytes();
    for (i, c) in b.iter().enumerate() {
        if *c == b'_' {
            let ok = i > 0 && i + 1 < b.len() && b[i - 1].is_ascii_digit() && b[i + 1].is_ascii_digit();
            if !ok {
                return None;
            }
        }
    }
    Some(t.replace('_', ""))
}

/// Python's `int(text, 0)`: decimal, or 0x, 0o and 0b prefixes, with a sign and surrounding whitespace.
pub fn parse_int(s: &str) -> Option<i128> {
    let t = s.trim();
    let (neg, body) = match t.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    let lower = body.to_ascii_lowercase();
    let (radix, digits) = if let Some(r) = lower.strip_prefix("0x") {
        (16, r)
    } else if let Some(r) = lower.strip_prefix("0o") {
        (8, r)
    } else if let Some(r) = lower.strip_prefix("0b") {
        (2, r)
    } else {
        // Python refuses a decimal with a leading zero, except zero itself
        if lower.len() > 1 && lower.starts_with('0') && lower.bytes().any(|b| b != b'0' && b != b'_') {
            return None;
        }
        (10, lower.as_str())
    };
    let digits = digits.strip_prefix('_').filter(|_| radix != 10).unwrap_or(digits);
    let digits = strip_underscores(digits)?;
    if digits.is_empty() || digits.starts_with(['+', '-']) {
        return None;
    }
    let v = i128::from_str_radix(&digits, radix).ok()?;
    Some(if neg { -v } else { v })
}

/// Python's `round(x)` for a float: halves go to the even neighbour.
pub fn py_round(v: f64) -> f64 {
    let r = v.round();
    if (v - v.trunc()).abs() == 0.5 {
        2.0 * (v / 2.0).round()
    } else {
        r
    }
}

/// A number as the editor writes floats into the level XML: 4 decimals at most, and at least one ("1.0").
pub fn fmt(v: f64) -> String {
    let r = py_round(v * 10000.0) / 10000.0 + 0.0;
    if r == r.trunc() && r.abs() < 1e16 {
        format!("{:.1}", r)
    } else {
        repr(r)
    }
}

/// An attribute's text as a number: a float, or an integer in any of Python's bases, or the number the text starts
/// with. NaN when it is none of these.
pub fn num(s: &str) -> f64 {
    if let Some(v) = parse_float(s) {
        return v;
    }
    if let Some(v) = parse_int(s) {
        return v as f64;
    }
    let t = s.trim_start();
    let b = t.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'-' || b[i] == b'+') {
        i += 1;
    }
    let ds = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let int_digits = i - ds;
    if i < b.len() && b[i] == b'.' {
        let dot = i;
        i += 1;
        let fs = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if int_digits == 0 && i == fs {
            return f64::NAN;
        }
        let _ = dot;
    } else if int_digits == 0 {
        return f64::NAN;
    }
    let mant_end = i;
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        let mut j = i + 1;
        if j < b.len() && (b[j] == b'-' || b[j] == b'+') {
            j += 1;
        }
        let es = j;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j > es {
            return t[..j].parse().unwrap_or(f64::NAN);
        }
    }
    t[..mant_end].parse().unwrap_or(f64::NAN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repr_matches_python() {
        assert_eq!(repr(1.0), "1.0");
        assert_eq!(repr(0.05f32 as f64), "0.05000000074505806");
        assert_eq!(repr(-0.0), "-0.0");
        assert_eq!(repr(1e16), "1e+16");
        assert_eq!(repr(1e15), "1000000000000000.0");
        assert_eq!(repr(0.0001), "0.0001");
        assert_eq!(repr(0.00001), "1e-05");
        assert_eq!(repr(1.5e-7), "1.5e-07");
        assert_eq!(repr(123456.789), "123456.789");
        assert_eq!(repr(3.4028234663852886e38), "3.4028234663852886e+38");
        assert_eq!(repr(-552.0), "-552.0");
    }

    #[test]
    fn parsing() {
        assert_eq!(parse_int("0x4000"), Some(0x4000));
        assert_eq!(parse_int(" -12 "), Some(-12));
        assert_eq!(parse_int("010"), None);
        assert_eq!(parse_int("0"), Some(0));
        assert_eq!(parse_int("1.0"), None);
        assert_eq!(parse_float("1e5"), Some(1e5));
        assert_eq!(parse_float("0x10"), None);
        assert!(parse_float("nan").unwrap().is_nan());
        assert_eq!(num("0x4000"), 16384.0);
        assert_eq!(num("12abc"), 12.0);
        assert!(num("abc").is_nan());
    }

    #[test]
    fn fmt_matches_python() {
        assert_eq!(fmt(1.0), "1.0");
        assert_eq!(fmt(-0.00001), "0.0");
        assert_eq!(fmt(1.23456), "1.2346");
        assert_eq!(fmt(0.5), "0.5");
        assert_eq!(fmt(-12.25), "-12.25");
    }
}
