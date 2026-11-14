//! The kit's own letters: a small stroke font for the words on gravestones and signs.
//!
//! Each letter is a few strokes on a grid 5 wide and 7 high, written below, and drawn into a cell. The table holds
//! the layout as numbers: advances and cell boxes. They are the measures of Pillow's built-in font at sizes 7 to 10
//! in 1-bit mode. That font is not copied here.

use super::image::{Image, Rgba};

/// The letters the kit allows, in the table's order.
pub const LETTERS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!?.,'- ";

// Per size 7 to 10 and per letter: the advance that centers a line, the advance used when drawing, the cell's left
// and the top of its outline box from the pen, the cell's width and height, how far the whole line moves when the
// letter comes first, and the top of the letter's bitmap. A line is placed by the highest outline box and drawn from
// the highest bitmap. The two tops differ for a few letters.
const METRICS: [[[i8; 8]; 43]; 4] = [
    // size 7
    [[4, 4, 0, 2, 4, 5, 0, 2], [5, 6, 1, 2, 4, 5, 0, 2], [5, 6, 1, 2, 5, 5, 0, 2], [5, 6, 1, 2, 4, 5, 0, 2], [5, 5, 1, 2, 4, 5, 0, 2], [5, 5, 1, 2, 4, 5, 0, 2], [5, 6, 1, 2, 4, 5, 0, 2], [5, 6, 1, 2, 4, 5, 0, 2], [2, 3, 1, 2, 1, 5, 0, 2], [3, 5, 1, 2, 3, 5, 0, 2], [5, 5, 1, 2, 4, 5, 0, 2], [5, 5, 1, 2, 4, 5, 0, 2], [6, 7, 1, 2, 5, 5, 0, 2], [5, 6, 1, 2, 4, 5, 0, 2], [5, 7, 1, 2, 5, 5, 0, 2], [5, 6, 1, 2, 4, 5, 0, 2], [5, 7, 1, 2, 5, 5, 0, 2], [5, 6, 1, 2, 4, 5, 0, 2], [4, 5, 1, 2, 3, 5, 0, 2], [5, 5, 0, 2, 5, 5, 0, 2], [5, 6, 1, 2, 4, 5, 0, 2], [4, 4, 0, 2, 4, 5, 0, 2], [7, 7, 0, 2, 6, 5, 0, 2], [4, 4, 0, 2, 4, 5, 0, 2], [5, 5, 0, 2, 4, 5, 0, 2], [5, 5, 1, 2, 4, 5, 0, 2], [4, 4, 1, 2, 4, 5, 0, 2], [4, 4, 1, 2, 2, 5, 0, 2], [4, 4, 1, 2, 3, 5, 0, 2], [4, 4, 0, 2, 4, 5, 0, 2], [4, 4, 0, 2, 5, 5, 0, 2], [4, 4, 0, 2, 4, 5, 0, 2], [4, 4, 1, 2, 4, 5, 0, 2], [4, 4, 1, 2, 3, 5, 0, 2], [4, 4, 1, 2, 3, 5, 0, 2], [4, 4, 1, 2, 4, 5, 0, 2], [1, 3, 1, 2, 1, 5, 0, 2], [4, 5, 1, 2, 3, 5, 0, 2], [2, 2, 0, 6, 1, 1, 0, 6], [2, 2, 0, 6, 1, 2, -1, 6], [1, 3, 1, 2, 1, 2, 0, 2], [2, 2, 0, 4, 2, 1, 0, 4], [2, 2, 0, 0, 0, 0, 0, 0]],
    // size 8
    [[5, 5, 0, 3, 5, 5, 0, 3], [5, 6, 1, 3, 4, 5, 0, 3], [5, 6, 1, 3, 5, 5, 0, 3], [6, 7, 1, 3, 5, 5, 0, 3], [5, 5, 1, 3, 4, 5, 0, 3], [5, 5, 1, 3, 4, 5, 0, 3], [6, 7, 1, 3, 5, 5, 0, 3], [7, 7, 1, 3, 5, 5, 0, 3], [3, 3, 1, 3, 1, 5, 0, 3], [4, 6, 1, 3, 4, 5, 0, 3], [5, 5, 1, 3, 4, 5, 0, 3], [5, 5, 1, 3, 4, 5, 0, 3], [8, 8, 1, 3, 6, 5, 0, 3], [7, 6, 1, 3, 4, 5, 0, 3], [5, 7, 1, 3, 5, 5, 0, 3], [5, 6, 1, 3, 4, 5, 0, 3], [5, 7, 1, 3, 5, 5, 0, 3], [5, 6, 1, 3, 4, 5, 0, 3], [5, 6, 1, 3, 4, 5, 0, 3], [5, 5, 0, 3, 5, 5, 0, 3], [5, 6, 1, 3, 4, 5, 0, 3], [5, 5, 0, 3, 5, 5, 0, 3], [8, 8, 0, 3, 7, 5, 0, 3], [5, 5, 0, 3, 5, 5, 0, 3], [5, 5, 0, 3, 5, 5, 0, 3], [6, 6, 1, 3, 4, 5, 0, 3], [5, 5, 1, 3, 4, 5, 0, 3], [5, 5, 2, 3, 2, 5, 0, 3], [5, 5, 1, 3, 4, 5, 0, 3], [5, 5, 0, 3, 4, 5, -1, 3], [5, 5, 0, 3, 5, 5, 0, 3], [5, 5, 1, 3, 4, 5, 0, 3], [5, 5, 1, 3, 4, 5, 0, 3], [5, 5, 0, 3, 4, 5, 0, 3], [5, 5, 1, 3, 4, 5, 0, 3], [5, 5, 1, 3, 4, 5, 0, 3], [1, 3, 1, 3, 1, 5, 0, 3], [4, 5, 1, 3, 3, 5, 0, 3], [2, 2, 0, 7, 1, 1, 0, 7], [2, 2, 0, 7, 1, 2, -1, 7], [1, 3, 1, 3, 1, 2, 0, 3], [2, 2, 0, 5, 2, 1, 0, 5], [2, 2, 0, 0, 0, 0, 0, 0]],
    // size 9
    [[6, 6, 0, 3, 5, 6, 0, 3], [6, 6, 1, 3, 4, 6, 0, 3], [6, 7, 1, 3, 6, 6, 0, 3], [6, 7, 1, 3, 5, 6, 0, 3], [6, 6, 1, 3, 4, 6, 0, 3], [5, 5, 1, 3, 4, 6, 0, 3], [7, 8, 1, 3, 6, 6, 0, 3], [7, 7, 1, 3, 5, 6, 0, 3], [3, 3, 1, 3, 1, 6, 0, 3], [4, 6, 1, 3, 4, 6, 0, 3], [6, 6, 1, 3, 5, 6, 0, 3], [5, 6, 1, 3, 4, 6, 0, 3], [8, 8, 1, 3, 6, 6, 0, 3], [7, 7, 1, 3, 5, 6, 0, 3], [6, 8, 1, 3, 6, 6, 0, 3], [6, 6, 1, 3, 4, 6, 0, 3], [6, 8, 1, 3, 6, 6, 0, 3], [6, 6, 1, 3, 4, 6, 0, 3], [5, 6, 1, 3, 4, 6, 0, 3], [5, 6, 1, 3, 5, 6, 0, 3], [7, 7, 1, 3, 5, 6, 0, 3], [6, 6, 0, 3, 6, 6, 0, 3], [9, 9, 0, 3, 8, 6, 0, 3], [6, 6, 0, 3, 5, 6, 0, 3], [5, 5, 0, 3, 5, 6, -1, 3], [6, 6, 1, 3, 5, 6, 0, 3], [5, 5, 1, 3, 5, 6, 0, 3], [5, 5, 1, 3, 3, 6, 0, 3], [5, 5, 1, 3, 4, 6, 0, 3], [5, 5, 0, 3, 5, 6, 0, 3], [5, 5, 1, 3, 5, 6, 0, 3], [5, 5, 1, 3, 4, 6, 0, 3], [5, 5, 1, 3, 4, 6, 0, 3], [5, 5, 1, 3, 4, 6, 0, 3], [5, 5, 1, 3, 4, 6, 0, 3], [5, 5, 1, 3, 4, 6, 0, 3], [3, 3, 1, 3, 1, 6, 0, 3], [4, 6, 1, 3, 4, 6, 0, 3], [2, 2, 0, 7, 1, 1, 0, 8], [2, 2, -1, 7, 2, 2, 0, 8], [1, 3, 1, 2, 1, 2, 0, 2], [2, 2, 0, 6, 2, 1, 0, 6], [2, 2, 0, 0, 0, 0, 0, 0]],
    // size 10
    [[6, 6, 0, 2, 6, 8, 0, 2], [6, 7, 1, 2, 5, 8, 0, 2], [7, 8, 1, 2, 6, 8, 0, 2], [7, 8, 1, 2, 6, 8, 0, 2], [6, 6, 1, 2, 5, 8, 0, 2], [6, 6, 1, 2, 5, 8, 0, 2], [7, 8, 1, 2, 6, 8, 0, 2], [8, 8, 1, 2, 6, 8, 0, 2], [3, 3, 1, 2, 1, 8, 0, 2], [6, 6, 1, 2, 4, 8, 0, 2], [6, 6, 1, 2, 5, 8, 0, 2], [6, 6, 1, 2, 5, 8, 0, 2], [9, 9, 1, 2, 7, 8, 0, 2], [8, 7, 1, 2, 5, 8, 0, 2], [7, 9, 1, 2, 7, 8, 0, 2], [6, 7, 1, 2, 5, 8, 0, 2], [7, 9, 1, 2, 7, 8, 0, 2], [6, 7, 1, 2, 5, 8, 0, 2], [6, 7, 1, 2, 5, 8, 0, 2], [6, 6, 1, 2, 5, 8, 0, 2], [7, 7, 1, 2, 5, 8, 0, 2], [6, 6, 0, 2, 6, 8, 0, 2], [10, 10, 0, 2, 9, 8, 0, 2], [6, 6, 1, 2, 5, 8, 0, 2], [6, 6, 1, 2, 5, 8, 0, 2], [7, 7, 1, 2, 5, 8, 0, 2], [6, 6, 1, 2, 5, 8, 0, 2], [6, 6, 1, 2, 3, 8, 0, 2], [6, 6, 0, 2, 5, 8, 0, 2], [6, 6, 0, 2, 5, 8, 0, 2], [6, 6, 0, 2, 6, 8, 0, 2], [6, 6, 0, 2, 5, 8, 0, 2], [6, 6, 1, 2, 5, 8, 0, 2], [6, 6, 0, 2, 5, 8, 0, 2], [6, 6, 1, 2, 5, 8, 0, 2], [6, 6, 1, 2, 5, 8, 0, 2], [3, 3, 1, 2, 1, 8, 0, 2], [4, 6, 1, 2, 4, 8, 0, 2], [2, 3, 1, 9, 1, 1, 0, 9], [2, 2, -1, 9, 2, 2, 0, 9], [1, 3, 1, 2, 1, 3, 0, 2], [3, 3, 0, 6, 2, 1, 0, 6], [2, 2, 0, 0, 0, 0, 0, 0]],
];

// Strokes per letter, as lists of points "xy" on the grid (x 0 to 4, y 0 to 6, y down). A space starts a new stroke.
const STROKES: [&str; 43] = [
    "06 02 20 42 46 04 44",                     // A
    "00 06 36 45 44 33 03 00 30 41 42 33",      // B
    "41 30 10 01 05 16 36 45",                  // C
    "00 06 36 45 41 30 00",                     // D
    "40 00 06 46 03 33",                        // E
    "40 00 06 03 33",                           // F
    "41 30 10 01 05 16 36 45 43 23",            // G
    "00 06 40 46 03 43",                        // H
    "20 26",                                    // I
    "40 45 36 16 05",                           // J
    "00 06 40 13 46 03 13",                     // K
    "00 06 46",                                 // L
    "06 00 23 40 46",                           // M
    "06 00 46 40",                              // N
    "10 30 41 45 36 16 05 01 10",               // O
    "06 00 30 41 42 33 03",                     // P
    "10 30 41 45 36 16 05 01 10 24 46",         // Q
    "06 00 30 41 42 33 03 23 46",               // R
    "41 30 10 01 02 13 33 44 45 36 16 05",      // S
    "00 40 20 26",                              // T
    "00 05 16 36 45 40",                        // U
    "00 26 40",                                 // V
    "00 16 23 36 40",                           // W
    "00 46 40 06",                              // X
    "00 23 40 23 26",                           // Y
    "00 40 06 46",                              // Z
    "10 30 41 45 36 16 05 01 10",               // 0
    "01 20 26 06 46",                           // 1
    "01 10 30 41 42 06 46",                     // 2
    "01 10 30 41 42 33 44 45 36 16 05 23 33",   // 3
    "36 30 04 44",                              // 4
    "40 00 03 33 44 45 36 06",                  // 5
    "41 30 10 01 05 16 36 45 44 33 03",         // 6
    "00 40 16",                                 // 7
    "10 30 41 42 33 13 02 01 10 13 04 05 16 36 45 44 33", // 8
    "05 16 36 45 41 30 10 01 02 13 43",         // 9
    "20 24 26 26",                              // !
    "01 10 30 41 42 24 26 26",                  // ?
    "00 46",                                    // .
    "40 06",                                    // ,
    "20 26",                                    // '
    "03 43",                                    // -
    "",                                         // space
];

// Which strokes start a new line: the index of each stroke's first point. Kept next to STROKES by hand.
fn breaks(i: usize) -> &'static [usize] {
    match LETTERS.as_bytes()[i] {
        b'A' => &[5],
        b'B' => &[7],
        b'E' => &[4],
        b'F' => &[3],
        b'H' => &[2, 4],
        b'K' => &[2, 5],
        b'Q' => &[9],
        b'R' => &[7],
        b'T' => &[2],
        b'X' => &[2],
        b'Y' => &[3],
        b'1' => &[3],
        b'3' => &[11],
        b'8' => &[9],
        b'!' => &[2],
        b'?' => &[6],
        _ => &[],
    }
}

fn index(c: char) -> Option<usize> {
    LETTERS.chars().position(|x| x == c)
}

fn metrics(size: u32, c: char) -> [i32; 8] {
    let s = size.clamp(7, 10) as usize - 7;
    let m = METRICS[s][index(c).unwrap_or(42)];
    [m[0] as i32, m[1] as i32, m[2] as i32, m[3] as i32, m[4] as i32, m[5] as i32, m[6] as i32, m[7] as i32]
}

/// The width `textlength` gave for the text at this size (7 to 10). Letters outside the kit's set count as spaces.
pub fn text_length(text: &str, size: u32) -> f64 {
    text.chars().map(|c| metrics(size, c)[0]).sum::<i32>() as f64
}

/// The letter as a bitmap of the given cell size.
pub fn glyph(c: char, w: usize, h: usize) -> Vec<Vec<bool>> {
    let mut bits = vec![vec![false; w]; h];
    let Some(i) = index(c) else { return bits };
    if w == 0 || h == 0 {
        return bits;
    }
    if c == '.' {
        return vec![vec![true; w]; h];
    }
    let pts: Vec<(i32, i32)> = STROKES[i]
        .split(' ')
        .filter(|s| s.len() == 2)
        .map(|s| {
            let b = s.as_bytes();
            let (x, y) = ((b[0] - b'0') as i32, (b[1] - b'0') as i32);
            // The grid point's place in the cell, rounded to the nearest pixel.
            ((x * (w as i32 - 1) * 2 + 4) / 8, (y * (h as i32 - 1) * 2 + 6) / 12)
        })
        .collect();
    let br = breaks(i);
    for k in 0..pts.len() {
        if k == 0 || br.contains(&k) {
            bits[pts[k].1 as usize][pts[k].0 as usize] = true;
            continue;
        }
        let ((mut x, mut y), (x1, y1)) = (pts[k - 1], pts[k]);
        let (dx, dy) = ((x1 - x).abs(), -(y1 - y).abs());
        let (sx, sy) = (if x < x1 { 1 } else { -1 }, if y < y1 { 1 } else { -1 });
        let mut err = dx + dy;
        loop {
            bits[y as usize][x as usize] = true;
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }
    bits
}

/// Draws one line of text with its pen at (x, y), as `ImageDraw.text` placed it in 1-bit mode.
pub fn draw_text(im: &mut Image, x: f64, y: f64, text: &str, color: Rgba, size: u32) {
    let inked: Vec<[i32; 8]> = text.chars().filter(|c| *c != ' ' && index(*c).is_some()).map(|c| metrics(size, c)).collect();
    if inked.is_empty() {
        return;
    }
    let first = text.chars().next().map(|c| metrics(size, c)[6]).unwrap_or(0);
    let shift = inked.iter().map(|m| m[3]).min().unwrap_or(0) - inked.iter().map(|m| m[7]).min().unwrap_or(0);
    // The pen's half pixel is rounded up.
    let mut pen = x as i32 + if x - x.trunc() >= 0.5 { 1 } else { 0 } + first;
    let top = y as i32 + if y - y.trunc() >= 0.5 { 1 } else { 0 };
    for c in text.chars() {
        let m = metrics(size, c);
        if m[4] > 0 && m[5] > 0 {
            im.stamp(pen + m[2], top + m[7] + shift, &glyph(c, m[4] as usize, m[5] as usize), color);
        }
        pen += m[1];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_letter_fills_its_cell_at_every_size() {
        for size in 7..=10 {
            for c in LETTERS.chars().filter(|c| *c != ' ') {
                let m = metrics(size, c);
                let g = glyph(c, m[4] as usize, m[5] as usize);
                assert!(g.first().is_some_and(|r| r.iter().any(|b| *b)), "{c} at {size}: empty top row");
                assert!(g.last().is_some_and(|r| r.iter().any(|b| *b)), "{c} at {size}: empty bottom row");
                assert!(g.iter().any(|r| r[0]), "{c} at {size}: empty left column");
                assert!(g.iter().any(|r| r[r.len() - 1]), "{c} at {size}: empty right column");
            }
        }
    }

    #[test]
    fn length_adds_up() {
        assert_eq!(text_length("RIP", 7), 12.0);
        assert_eq!(text_length("", 8), 0.0);
    }

    #[test]
    fn text_is_drawn_inside_the_picture() {
        let mut im = Image::new(32, 16, [0; 4]);
        draw_text(&mut im, 4.5, 2.0, "A,B", [9, 9, 9, 255], 7);
        assert!(im.px.iter().any(|p| p[3] == 255));
    }
}
