//! Helpers on the level XML that `Doc` and `pyfmt` do not have: attributes as numbers, object ids as hex text,
//! `%g` formatting, and small ordered attribute lists.

use glovebox_core::pyfmt;
use glovebox_core::xml::{Doc, NodeId};

pub use glovebox_core::pyfmt::fmt;

/// An attribute as a number. `d` when the node or the attribute is missing. NaN when the text is not a number.
pub fn num(doc: &Doc, e: Option<NodeId>, a: &str, d: f64) -> f64 {
    match e.and_then(|e| doc.get(e, a)) {
        Some(v) => pyfmt::num(v),
        None => d,
    }
}

/// `num` of a node that exists, with 0 for a missing attribute.
pub fn num0(doc: &Doc, e: NodeId, a: &str) -> f64 {
    num(doc, Some(e), a, 0.0)
}

/// The whole part of a finite value inside the i64 range.
fn trunc_i64(v: f64) -> i64 {
    v.trunc() as i64
}

/// An attribute as an integer. `d` when it is missing or not a finite number.
pub fn inum(doc: &Doc, e: Option<NodeId>, a: &str, d: i64) -> i64 {
    let v = num(doc, e, a, d as f64);
    if v == v && v.abs() < 1e18 {
        trunc_i64(v)
    } else {
        d
    }
}

/// A number rounded to a whole one, as the level XML writes floats ("12.0").
pub fn r1(v: f64) -> String {
    fmt(pyfmt::py_round(v))
}

/// A hex number as Python's `int(text, 16)` reads it, kept to its low 32 bits: surrounding whitespace, a sign, an
/// optional 0x and single underscores between digits are allowed.
fn parse_hex_u32(s: &str) -> Option<u32> {
    let t = s.trim_matches(|c: char| c.is_whitespace());
    let (neg, body) = match t.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    let b = body.as_bytes();
    let mut i = 0;
    if b.len() >= 2 && b[0] == b'0' && (b[1] == b'x' || b[1] == b'X') {
        i = 2;
        // One underscore may follow the prefix
        if i < b.len() && b[i] == b'_' {
            i += 1;
        }
    }
    if i >= b.len() {
        return None;
    }
    let mut v: u32 = 0;
    let mut prev_digit = false;
    while i < b.len() {
        let c = b[i];
        if c == b'_' {
            if !prev_digit || i + 1 >= b.len() {
                return None;
            }
            prev_digit = false;
        } else {
            let d = (c as char).to_digit(16)?;
            v = v.wrapping_mul(16).wrapping_add(d);
            prev_digit = true;
        }
        i += 1;
    }
    if !prev_digit {
        return None;
    }
    Some(if neg { v.wrapping_neg() } else { v })
}

/// An object id as the editor names models: "0x" and eight upper-case hex digits. `None` for an empty or
/// malformed id.
pub fn hex_id(s: Option<&str>) -> Option<String> {
    let s = s?;
    if s.is_empty() {
        return None;
    }
    parse_hex_u32(s).map(|v| format!("0x{:08X}", v))
}

/// A number as an object id.
pub fn id_hex(n: i64) -> String {
    format!("0x{:08X}", n as u32)
}

/// Gets or creates a child element. `before` picks the node that the new child goes in front of. When it gives
/// `None`, the new child is appended. Without `before` the new child goes first.
pub fn ensure_child(
    doc: &mut Doc,
    parent: NodeId,
    tag: &str,
    attrs: &[(&str, &str)],
    before: Option<&dyn Fn(&Doc, NodeId) -> Option<NodeId>>,
) -> NodeId {
    if let Some(c) = doc.first(parent, tag) {
        return c;
    }
    let c = doc.make(tag, attrs);
    let reference = match before {
        Some(f) => f(doc, parent),
        None => doc.children(parent).first().copied(),
    };
    match reference {
        Some(r) => doc.add_prev(r, c),
        None => doc.append(parent, c),
    }
    c
}

/// Attributes in the order they were set.
pub type Attrs = Vec<(String, String)>;

/// Sets an attribute. A known key keeps its place.
pub fn put(attrs: &mut Attrs, k: &str, v: &str) {
    match attrs.iter_mut().find(|(n, _)| n == k) {
        Some(a) => a.1 = v.to_string(),
        None => attrs.push((k.to_string(), v.to_string())),
    }
}

pub fn attrs_of(pairs: &[(&str, &str)]) -> Attrs {
    let mut out = Attrs::new();
    for (k, v) in pairs {
        put(&mut out, k, v);
    }
    out
}

/// A new element with these attributes, without a parent.
pub fn make_el(doc: &mut Doc, tag: &str, attrs: &Attrs) -> NodeId {
    let e = doc.new_element(tag);
    for (k, v) in attrs {
        doc.set(e, k, v);
    }
    e
}

/// The first child of a node, whatever its tag.
pub fn body(doc: &Doc, node: NodeId) -> Option<NodeId> {
    doc.children(node).first().copied()
}

/// A whole number in any of Python's bases (`pyfmt::parse_int`). Missing or empty text is 0. `None` for text
/// that is not a number.
pub fn int0(s: Option<&str>) -> Option<i128> {
    match s {
        Some(v) if !v.is_empty() => pyfmt::parse_int(v),
        _ => Some(0),
    }
}

/// A number as `%g` prints it: six significant digits, no trailing zeros, an exponent below 1e-4 and from 1e6.
pub fn fmt_g(v: f64) -> String {
    if v.is_nan() {
        return "nan".into();
    }
    if v.is_infinite() {
        return if v > 0.0 { "inf".into() } else { "-inf".into() };
    }
    if v == 0.0 {
        return if v.is_sign_negative() { "-0".into() } else { "0".into() };
    }
    let e = format!("{:.5e}", v.abs());
    let (mant, exp) = e.split_once('e').unwrap_or((&e, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let digits: String = mant.chars().filter(|c| *c != '.').collect();
    let digits = digits.trim_end_matches('0');
    let digits = if digits.is_empty() { "0" } else { digits };
    let sign = if v < 0.0 { "-" } else { "" };
    let n = digits.len() as i32;
    if (-4..6).contains(&exp) {
        if exp >= n - 1 {
            format!("{}{}{}", sign, digits, "0".repeat((exp - (n - 1)) as usize))
        } else if exp >= 0 {
            format!("{}{}.{}", sign, &digits[..(exp + 1) as usize], &digits[(exp + 1) as usize..])
        } else {
            format!("{}0.{}{}", sign, "0".repeat((-exp - 1) as usize), digits)
        }
    } else {
        let m = if n == 1 { digits.to_string() } else { format!("{}.{}", &digits[..1], &digits[1..]) };
        format!("{}{}e{}{:02}", sign, m, if exp < 0 { "-" } else { "+" }, exp.abs())
    }
}

/// A float as the 32-bit pattern the game reads: "0x3F800000" for 1.0.
pub fn float_bits(v: f64) -> String {
    format!("0x{:08X}", (v as f32).to_bits())
}

/// The float a 32-bit pattern holds. 0.0 when the text is not an integer.
pub fn float_from_bits(s: Option<&str>) -> f64 {
    match s.and_then(pyfmt::parse_int) {
        Some(v) => f32::from_bits(v as u32) as f64,
        None => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers() {
        let doc = Doc::parse(r#"<a x="1.5" h="0x10" bad="zz" big="1e30"/>"#).unwrap();
        let r = Some(doc.root());
        assert_eq!(num(&doc, r, "x", 0.0), 1.5);
        assert_eq!(num(&doc, r, "h", 0.0), 16.0);
        assert_eq!(num(&doc, r, "none", 7.0), 7.0);
        assert_eq!(num(&doc, None, "x", 3.0), 3.0);
        assert!(num(&doc, r, "bad", 0.0).is_nan());
        assert_eq!(inum(&doc, r, "x", 0), 1);
        assert_eq!(inum(&doc, r, "bad", 4), 4);
        assert_eq!(inum(&doc, r, "big", 4), 4);
        assert_eq!(r1(2.5), "2.0");
        assert_eq!(r1(-0.4), "0.0");
    }

    #[test]
    fn ids() {
        assert_eq!(hex_id(Some("0x70210547")).as_deref(), Some("0x70210547"));
        assert_eq!(hex_id(Some("abc")).as_deref(), Some("0x00000ABC"));
        assert_eq!(hex_id(Some("-1")).as_deref(), Some("0xFFFFFFFF"));
        assert_eq!(hex_id(Some(" 0X1_0 ")).as_deref(), Some("0x00000010"));
        assert_eq!(hex_id(Some("1ffffffff")).as_deref(), Some("0xFFFFFFFF"));
        assert_eq!(hex_id(Some("")), None);
        assert_eq!(hex_id(Some("0x")), None);
        assert_eq!(hex_id(Some("xyz")), None);
        assert_eq!(hex_id(Some("1__0")), None);
        assert_eq!(hex_id(None), None);
        assert_eq!(id_hex(-1), "0xFFFFFFFF");
        assert_eq!(id_hex(0x1234), "0x00001234");
    }

    #[test]
    fn percent_g() {
        assert_eq!(fmt_g(5.0), "5");
        assert_eq!(fmt_g(-2.5), "-2.5");
        assert_eq!(fmt_g(1234567.0), "1.23457e+06");
        assert_eq!(fmt_g(100000.0), "100000");
        assert_eq!(fmt_g(0.0001), "0.0001");
        assert_eq!(fmt_g(0.00001234), "1.234e-05");
        assert_eq!(fmt_g(0.1 + 0.2), "0.3");
        assert_eq!(fmt_g(999999.5), "1e+06");
        assert_eq!(fmt_g(f64::NAN), "nan");
    }

    #[test]
    fn bits_and_children() {
        assert_eq!(float_bits(1.0), "0x3F800000");
        assert_eq!(float_from_bits(Some("0x40000000")), 2.0);
        assert_eq!(float_from_bits(Some("x")), 0.0);
        assert_eq!(float_from_bits(None), 0.0);
        let mut doc = Doc::parse("<a><b/><c/></a>").unwrap();
        let root = doc.root();
        let c = ensure_child(&mut doc, root, "c", &[], None);
        assert_eq!(doc.index_of(c), Some(1));
        ensure_child(&mut doc, root, "d", &[("k", "v")], None);
        ensure_child(&mut doc, root, "e", &[], Some(&|_, _| None));
        assert_eq!(doc.serialize(), r#"<a><d k="v"/><b/><c/><e/></a>"#);
    }
}
