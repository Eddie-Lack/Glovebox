//! A small RGBA picture and the drawing the kit's textures need: points, lines, rectangles, ellipses, arcs, polygons,
//! one resize and PNG output.
//!
//! The Python kit draws with Pillow. The shapes here follow Pillow 13's rules (libImaging/Draw.c and Resample.c,
//! licence MIT-CMU), so the textures come out the same: coordinates are cut to whole numbers toward zero, a line of
//! width 1 is Bresenham's, a wide line and a polygon are filled by scan lines with Pillow's rounding, and an ellipse
//! is traced on a grid of half pixels.

/// One pixel: red, green, blue, alpha.
pub type Rgba = [u8; 4];

/// An RGBA picture, rows from the top.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub w: usize,
    pub h: usize,
    pub px: Vec<Rgba>,
}

#[derive(Clone, Copy)]
struct Edge {
    x0: i32,
    y0: i32,
    xmin: i32,
    ymin: i32,
    xmax: i32,
    ymax: i32,
    dx: f32,
}

fn edge(x0: i32, y0: i32, x1: i32, y1: i32) -> Edge {
    Edge {
        x0,
        y0,
        xmin: x0.min(x1),
        xmax: x0.max(x1),
        ymin: y0.min(y1),
        ymax: y0.max(y1),
        dx: if y0 == y1 { 0.0 } else { (x1 - x0) as f32 / (y1 - y0) as f32 },
    }
}

// Pillow's ROUND_UP and ROUND_DOWN: halves go away from zero or toward zero.
fn round_up32(f: f32) -> i32 {
    if f >= 0.0 { (f + 0.5).floor() as i32 } else { -((f.abs() + 0.5).floor()) as i32 }
}
fn round_down32(f: f32) -> i32 {
    if f >= 0.0 { (f - 0.5).ceil() as i32 } else { -((f.abs() - 0.5).ceil()) as i32 }
}
fn round_up64(f: f64) -> i32 {
    if f >= 0.0 { (f + 0.5).floor() as i32 } else { -((f.abs() + 0.5).floor()) as i32 }
}
fn round_down64(f: f64) -> i32 {
    if f >= 0.0 { (f - 0.5).ceil() as i32 } else { -((f.abs() - 0.5).ceil()) as i32 }
}

impl Image {
    /// A picture of one colour.
    pub fn new(w: usize, h: usize, colour: Rgba) -> Image {
        Image { w, h, px: vec![colour; w * h] }
    }

    /// The pixel at (x, y), or clear black outside the picture.
    pub fn get(&self, x: usize, y: usize) -> Rgba {
        if x < self.w && y < self.h { self.px[y * self.w + x] } else { [0; 4] }
    }

    /// Sets one pixel. Does nothing outside the picture.
    pub fn put(&mut self, x: i32, y: i32, c: Rgba) {
        if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
            self.px[y as usize * self.w + x as usize] = c;
        }
    }

    /// `ImageDraw.point`.
    pub fn point(&mut self, x: f64, y: f64, c: Rgba) {
        self.put(x as i32, y as i32, c);
    }

    /// A row of pixels from x0 to x1, both included.
    pub fn hline(&mut self, x0: i32, y: i32, x1: i32, c: Rgba) {
        if y < 0 || y as usize >= self.h || x0 >= self.w as i32 || x1 < 0 {
            return;
        }
        let x0 = x0.max(0);
        let x1 = x1.min(self.w as i32 - 1);
        for x in x0..=x1 {
            self.px[y as usize * self.w + x as usize] = c;
        }
    }

    // Pillow's line of width 1. The last point is not drawn.
    fn line1(&mut self, mut x0: i32, mut y0: i32, x1: i32, y1: i32, c: Rgba) {
        let mut dy = y1 - y0;
        let ys = if dy < 0 { -1 } else { 1 };
        if dy == 0 {
            if x1 > x0 {
                self.hline(x0, y0, x1 - 1, c);
            } else if x0 > x1 {
                self.hline(x1 + 1, y0, x0, c);
            }
            return;
        }
        dy = dy.abs();
        let mut dx = x1 - x0;
        let xs = if dx < 0 { -1 } else { 1 };
        dx = dx.abs();
        if dx == 0 {
            for _ in 0..dy {
                self.put(x0, y0, c);
                y0 += ys;
            }
        } else if dx > dy {
            let n = dx;
            dy += dy;
            let mut e = dy - dx;
            dx += dx;
            for _ in 0..n {
                self.put(x0, y0, c);
                if e >= 0 {
                    y0 += ys;
                    e -= dx;
                }
                e += dy;
                x0 += xs;
            }
        } else {
            let n = dy;
            dx += dx;
            let mut e = dx - dy;
            dy += dy;
            for _ in 0..n {
                self.put(x0, y0, c);
                if e >= 0 {
                    x0 += xs;
                    e -= dy;
                }
                e += dx;
                y0 += ys;
            }
        }
    }

    // Pillow's scan line fill of a list of edges.
    fn fill_edges(&mut self, e: &[Edge], c: Rgba) {
        if e.is_empty() {
            return;
        }
        let mut ymin = self.h as i32 - 1;
        let mut ymax = 0;
        let mut table: Vec<Edge> = Vec::new();
        for ed in e {
            ymin = ymin.min(ed.ymin);
            ymax = ymax.max(ed.ymax);
            if ed.ymin == ed.ymax {
                self.hline(ed.xmin, ed.ymin, ed.xmax, c);
                continue;
            }
            table.push(*ed);
        }
        ymin = ymin.max(0);
        ymax = ymax.min(self.h as i32);
        let at = |ed: &Edge, y: i32| (y - ed.y0) as f32 * ed.dx + ed.x0 as f32;
        let mut xx: Vec<f32> = Vec::new();
        for y in ymin..=ymax {
            xx.clear();
            for i in 0..table.len() {
                let cur = table[i];
                if y < cur.ymin || y > cur.ymax {
                    continue;
                }
                let v = at(&cur, y);
                xx.push(v);
                if y == cur.ymax && y < ymax {
                    xx.push(v);
                } else if (y == cur.ymin || y == cur.ymax) && cur.dx != 0.0 {
                    // Pillow joins two edges that meet in a corner on this row.
                    for other in &table[..i] {
                        if (y != other.ymin && y != other.ymax) || other.dx == 0.0 {
                            continue;
                        }
                        if v.round() != at(other, y).round() {
                            continue;
                        }
                        let offset = if y == cur.ymax { -1 } else { 1 };
                        let adj = at(&cur, y + offset);
                        if y + offset >= other.ymin && y + offset <= other.ymax {
                            let adj_other = at(other, y + offset);
                            let last = xx.len() - 1;
                            if v > adj + 1.0 && v > adj_other + 1.0 {
                                xx[last] = adj.max(adj_other).round() + 1.0;
                            } else if v < adj - 1.0 && v < adj_other - 1.0 {
                                xx[last] = adj.min(adj_other).round() - 1.0;
                            }
                            break;
                        }
                    }
                }
            }
            xx.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let mut i = 1;
            while i < xx.len() {
                self.hline(round_up32(xx[i - 1]), y, round_down32(xx[i]), c);
                i += 2;
            }
        }
    }

    // Pillow's wide line: a four cornered polygon round the line.
    fn wide_line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: Rgba, width: i32) {
        let (dx, dy) = (x1 - x0, y1 - y0);
        if dx == 0 && dy == 0 {
            self.put(x0, y0, c);
            return;
        }
        let big = (dx as f64).hypot(dy as f64);
        let small = (width - 1) as f64 / 2.0;
        let ratio_max = round_up64(small) as f64 / big;
        let ratio_min = round_down64(small) as f64 / big;
        let dxmin = round_down64(ratio_min * dy as f64);
        let dxmax = round_down64(ratio_max * dy as f64);
        let dymin = round_down64(ratio_min * dx as f64);
        let dymax = round_down64(ratio_max * dx as f64);
        let v = [
            (x0 - dxmin, y0 + dymax),
            (x1 - dxmin, y1 + dymax),
            (x1 + dxmax, y1 - dymin),
            (x0 + dxmax, y0 - dymin),
        ];
        let e: Vec<Edge> = (0..4).map(|i| edge(v[i].0, v[i].1, v[(i + 1) % 4].0, v[(i + 1) % 4].1)).collect();
        self.fill_edges(&e, c);
    }

    /// `ImageDraw.line`: a line through the points.
    pub fn line(&mut self, pts: &[(f64, f64)], c: Rgba, width: i32) {
        if width == 0 {
            return;
        }
        let p: Vec<(i32, i32)> = pts.iter().map(|&(x, y)| (x as i32, y as i32)).collect();
        for k in 1..p.len() {
            if width == 1 {
                self.line1(p[k - 1].0, p[k - 1].1, p[k].0, p[k].1, c);
            } else {
                self.wide_line(p[k - 1].0, p[k - 1].1, p[k].0, p[k].1, c, width);
            }
        }
        if width == 1 && p.len() > 1 {
            let last = p[p.len() - 1];
            self.put(last.0, last.1, c);
        }
    }

    /// `ImageDraw.rectangle`: both corners included. The outline is one pixel wide.
    pub fn rectangle(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, fill: Option<Rgba>, outline: Option<Rgba>) {
        let (y0, y1) = if y0 > y1 { (y1, y0) } else { (y0, y1) };
        if let Some(c) = fill {
            for y in y0.max(0)..=y1.min(self.h as i32) {
                self.hline(x0, y, x1, c);
            }
        }
        if let Some(c) = outline {
            if Some(c) != fill {
                self.hline(x0, y0, x1, c);
                self.hline(x0, y1, x1, c);
                self.line1(x1, y0 + 1, x1, y1, c);
                self.line1(x0, y0 + 1, x0, y1, c);
            }
        }
    }

    /// `ImageDraw.polygon` with a fill.
    pub fn polygon(&mut self, pts: &[(f64, f64)], c: Rgba) {
        let p: Vec<(i32, i32)> = pts.iter().map(|&(x, y)| (x as i32, y as i32)).collect();
        if p.len() < 2 {
            return;
        }
        let mut e: Vec<Edge> = Vec::new();
        for i in 0..p.len() - 1 {
            let ((x0, y0), (x1, y1)) = (p[i], p[i + 1]);
            if y0 == y1 && i != 0 && y0 == p[i - 1].1 {
                // A level edge that follows another level edge in the same direction makes it longer.
                if let Some(last) = e.last_mut() {
                    if x1 > x0 && x0 > p[i - 1].0 {
                        last.xmax = x1;
                        continue;
                    } else if x1 < x0 && x0 < p[i - 1].0 {
                        last.xmin = x1;
                        continue;
                    }
                }
            }
            e.push(edge(x0, y0, x1, y1));
        }
        let last = p[p.len() - 1];
        if last != p[0] {
            e.push(edge(last.0, last.1, p[0].0, p[0].1));
        }
        self.fill_edges(&e, c);
    }

    /// `ImageDraw.ellipse` in the box from (x0, y0) to (x1, y1), both included.
    pub fn ellipse(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, fill: Option<Rgba>, outline: Option<Rgba>, width: i32) {
        let (a, b) = (x1 - x0, y1 - y0);
        if a < 0 || b < 0 {
            return;
        }
        if let Some(c) = fill {
            let mut st = Ellipse::new(a, b, a + b);
            while let Some((l, y, r)) = st.next() {
                self.hline(x0 + (l + a) / 2, y0 + (y + b) / 2, x0 + (r + a) / 2, c);
            }
        }
        if let Some(c) = outline {
            if Some(c) != fill && width != 0 {
                let mut st = Ellipse::new(a, b, width);
                while let Some((l, y, r)) = st.next() {
                    self.hline(x0 + (l + a) / 2, y0 + (y + b) / 2, x0 + (r + a) / 2, c);
                }
            }
        }
    }

    /// `ImageDraw.arc`: part of an ellipse's outline, from `start` to `end` degrees, clockwise from three o'clock.
    pub fn arc(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, start: f32, end: f32, c: Rgba, width: i32) {
        let (mut al, mut ar) = (start, end);
        normalize_angles(&mut al, &mut ar);
        if al + 360.0 == ar {
            self.ellipse(x0, y0, x1, y1, None, Some(c), width);
            return;
        }
        if al == ar {
            return;
        }
        let (a, b) = (x1 - x0, y1 - y0);
        if a < 0 || b < 0 {
            return;
        }
        let root = arc_tree(a, b, al, ar);
        let mut st = Ellipse::new(a, b, width);
        while let Some((l, y, r)) = st.next() {
            let ev = clip(root.as_ref(), l, y, r);
            for pair in ev.chunks(2) {
                if pair.len() == 2 {
                    self.hline(x0 + (pair[0].0 + a) / 2, y0 + (y + b) / 2, x0 + (pair[1].0 + a) / 2, c);
                }
            }
        }
    }

    /// Sets the pixels of a 1-bit glyph: `rows` are strings of `#` and other characters.
    pub fn stamp(&mut self, x: i32, y: i32, bits: &[Vec<bool>], c: Rgba) {
        for (j, row) in bits.iter().enumerate() {
            for (i, &on) in row.iter().enumerate() {
                if on {
                    self.put(x + i as i32, y + j as i32, c);
                }
            }
        }
    }

    /// Pillow's `resize((w, new_h), LANCZOS)` for a picture whose width stays the same.
    /// The picture must be opaque: Pillow weighs colours by alpha, which changes nothing when alpha is 255.
    pub fn resize_height_lanczos(&self, new_h: usize) -> Image {
        const BITS: u32 = 22;
        let mut out = Image::new(self.w, new_h, [0; 4]);
        if self.h == 0 || new_h == 0 {
            return out;
        }
        let scale = self.h as f64 / new_h as f64;
        let filterscale = scale.max(1.0);
        let support = 3.0 * filterscale;
        let inv = 1.0 / filterscale;
        let sinc = |x: f64| if x == 0.0 { 1.0 } else { (x * std::f64::consts::PI).sin() / (x * std::f64::consts::PI) };
        let lanczos = |x: f64| if (-3.0..3.0).contains(&x) { sinc(x) * sinc(x / 3.0) } else { 0.0 };
        for yy in 0..new_h {
            let center = (yy as f64 + 0.5) * scale;
            let ymin = ((center - support + 0.5) as i64).max(0);
            let ymax = ((center + support + 0.5) as i64).min(self.h as i64);
            let mut k: Vec<f64> = (ymin..ymax).map(|y| lanczos((y as f64 - center + 0.5) * inv)).collect();
            let ww: f64 = k.iter().fold(0.0, |a, b| a + b);
            if ww != 0.0 {
                for v in k.iter_mut() {
                    *v /= ww;
                }
            }
            let kk: Vec<i64> = k
                .iter()
                .map(|&v| if v < 0.0 { (-0.5 + v * (1u32 << BITS) as f64) as i64 } else { (0.5 + v * (1u32 << BITS) as f64) as i64 })
                .collect();
            for x in 0..self.w {
                let mut ss = [1i64 << (BITS - 1); 4];
                for (n, &coef) in kk.iter().enumerate() {
                    let p = self.px[(ymin as usize + n) * self.w + x];
                    for ch in 0..4 {
                        ss[ch] += p[ch] as i64 * coef;
                    }
                }
                let mut p = [0u8; 4];
                for ch in 0..4 {
                    p[ch] = (ss[ch] >> BITS).clamp(0, 255) as u8;
                }
                out.px[yy * self.w + x] = p;
            }
        }
        out
    }

    fn encode(&self, rgb: bool) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut buf, self.w.max(1) as u32, self.h.max(1) as u32);
            enc.set_color(if rgb { png::ColorType::Rgb } else { png::ColorType::Rgba });
            enc.set_depth(png::BitDepth::Eight);
            let n = if rgb { 3 } else { 4 };
            let mut data: Vec<u8> = Vec::with_capacity(self.px.len() * n);
            for p in &self.px {
                data.extend_from_slice(&p[..n]);
            }
            data.resize(self.w.max(1) * self.h.max(1) * n, 0);
            // Writing to a Vec cannot fail, and the header and data sizes agree.
            if let Ok(mut wr) = enc.write_header() {
                let _ = wr.write_image_data(&data);
            }
        }
        buf
    }

    /// The picture as an RGBA PNG file.
    pub fn png(&self) -> Vec<u8> {
        self.encode(false)
    }

    /// The picture as an RGB PNG file, without the alpha.
    pub fn png_rgb(&self) -> Vec<u8> {
        self.encode(true)
    }
}

// ---------------------------------------------------------------------------------------------
// Ellipses, as Pillow traces them: on a grid with a step of 2, where a pixel is 2 wide.

struct Quarter {
    cx: i32,
    cy: i32,
    ex: i32,
    ey: i32,
    a2: i64,
    b2: i64,
    a2b2: i64,
    finished: bool,
}

impl Quarter {
    fn new(a: i32, b: i32) -> Quarter {
        if a < 0 || b < 0 {
            return Quarter { cx: 0, cy: 0, ex: 0, ey: 0, a2: 0, b2: 0, a2b2: 0, finished: true };
        }
        let (a2, b2) = (a as i64 * a as i64, b as i64 * b as i64);
        Quarter { cx: a, cy: b % 2, ex: a % 2, ey: b, a2, b2, a2b2: a2 * b2, finished: false }
    }

    fn delta(&self, x: i64, y: i64) -> i64 {
        (self.a2 * y * y + self.b2 * x * x - self.a2b2).abs()
    }

    fn next(&mut self) -> Option<(i32, i32)> {
        if self.finished {
            return None;
        }
        let ret = (self.cx, self.cy);
        if self.cx == self.ex && self.cy == self.ey {
            self.finished = true;
        } else {
            let (mut nx, mut ny) = (self.cx, self.cy + 2);
            let mut nd = self.delta(nx as i64, ny as i64);
            if nx > 1 {
                let d = self.delta(self.cx as i64 - 2, self.cy as i64 + 2);
                if nd > d {
                    nx = self.cx - 2;
                    ny = self.cy + 2;
                    nd = d;
                }
                let d = self.delta(self.cx as i64 - 2, self.cy as i64);
                if nd > d {
                    nx = self.cx - 2;
                    ny = self.cy;
                }
            }
            self.cx = nx;
            self.cy = ny;
        }
        Some(ret)
    }
}

// Rows of an ellipse outline of some width: (left, y, right) on the grid of half pixels, the middle at 0.
struct Ellipse {
    outer: Quarter,
    inner: Quarter,
    py: i32,
    pl: i32,
    pr: i32,
    buf: Vec<(i32, i32, i32)>,
    finished: bool,
    leftmost: i32,
}

impl Ellipse {
    fn new(a: i32, b: i32, w: i32) -> Ellipse {
        let mut s = Ellipse {
            outer: Quarter::new(a, b),
            inner: Quarter::new(-1, -1),
            py: 0,
            pl: 0,
            pr: 0,
            buf: Vec::new(),
            finished: true,
            leftmost: a % 2,
        };
        if w >= 1 {
            if let Some((x, y)) = s.outer.next() {
                s.pr = x;
                s.py = y;
                s.finished = false;
                s.inner = Quarter::new(a - 2 * (w - 1), b - 2 * (w - 1));
                s.pl = s.leftmost;
            }
        }
        s
    }

    fn next(&mut self) -> Option<(i32, i32, i32)> {
        if self.buf.is_empty() {
            if self.finished {
                return None;
            }
            let (y, mut l, r) = (self.py, self.pl, self.pr);
            loop {
                match self.outer.next() {
                    None => {
                        self.finished = true;
                        break;
                    }
                    Some((cx, cy)) => {
                        if cy > y {
                            self.pr = cx;
                            self.py = cy;
                            break;
                        }
                    }
                }
            }
            loop {
                match self.inner.next() {
                    None => {
                        self.pl = self.leftmost;
                        break;
                    }
                    Some((cx, cy)) => {
                        if cy > y {
                            self.pl = cx;
                            break;
                        }
                        l = cx;
                    }
                }
            }
            if (l > 0 || l < r) && y > 0 {
                self.buf.push((if l == 0 { 2 } else { l }, y, r));
            }
            if y > 0 {
                self.buf.push((-r, y, -l));
            }
            if l > 0 || l < r {
                self.buf.push((if l == 0 { 2 } else { l }, -y, r));
            }
            self.buf.push((-r, -y, -l));
        }
        self.buf.pop()
    }
}

// Pillow cuts an arc out of an ellipse with a tree of half planes.
enum Node {
    Clip(f64, f64, f64),
    And(Box<Node>, Box<Node>),
    Or(Box<Node>, Box<Node>),
}

fn transpose(n: &mut Node) {
    match n {
        Node::Clip(a, b, _) => std::mem::swap(a, b),
        Node::And(l, r) | Node::Or(l, r) => {
            transpose(l);
            transpose(r);
        }
    }
}

// The ends of the parts of the row from x0 to x1 that the tree keeps: (x, +1 for a start or -1 for an end).
fn clip(root: Option<&Node>, mut x0: i32, y: i32, mut x1: i32) -> Vec<(i32, i8)> {
    let Some(node) = root else {
        return vec![(x0, 1), (x1, -1)];
    };
    match node {
        Node::Clip(a, b, c) => {
            let eps = 1e-9;
            let yf = y as f64;
            if a.abs() < eps {
                if b * yf + c < -eps {
                    x0 = 1;
                    x1 = 0;
                }
            } else {
                let ix = -(b * yf + c) / a;
                if a * x0 as f64 + b * yf + c < eps {
                    x0 = (x0 as f64).max(ix).round() as i32;
                }
                if a * x1 as f64 + b * yf + c < eps {
                    x1 = (x1 as f64).min(ix).round() as i32;
                }
            }
            if x0 <= x1 { vec![(x0, 1), (x1, -1)] } else { Vec::new() }
        }
        Node::And(l, r) | Node::Or(l, r) => {
            let is_or = matches!(node, Node::Or(..));
            let l1 = clip(Some(l), x0, y, x1);
            let l2 = clip(Some(r), x0, y, x1);
            let (mut i1, mut i2, mut k1, mut k2) = (0usize, 0usize, 0i32, 0i32);
            let mut out: Vec<(i32, i8)> = Vec::new();
            while i1 < l1.len() || i2 < l2.len() {
                let first = i2 >= l2.len() || (i1 < l1.len() && (l1[i1].0 < l2[i2].0 || (l1[i1].0 == l2[i2].0 && l1[i1].1 > l2[i2].1)));
                let t = if first {
                    k1 += l1[i1].1 as i32;
                    i1 += 1;
                    l1[i1 - 1]
                } else {
                    k2 += l2[i2].1 as i32;
                    i2 += 1;
                    l2[i2 - 1]
                };
                let tail = out.last().map(|e| e.1);
                let keep = if is_or {
                    (t.1 == 1 && (tail.is_none() || tail == Some(-1))) || (t.1 == -1 && k1 == 0 && k2 == 0)
                } else {
                    (t.1 == 1 && (tail.is_none() || tail == Some(-1)) && k1 > 0 && k2 > 0) || (t.1 == -1 && tail == Some(1) && (k1 == 0 || k2 == 0))
                };
                if keep {
                    out.push(t);
                }
            }
            out
        }
    }
}

// Angles so that 0 <= al < 360 and al <= ar <= al + 360.
fn normalize_angles(al: &mut f32, ar: &mut f32) {
    if *ar - *al >= 360.0 {
        *al = 0.0;
        *ar = 360.0;
    } else {
        let (l, r) = (*al as f64, *ar as f64);
        *al = (if l < 0.0 { 360.0 - ((-l) % 360.0) } else { l } % 360.0) as f32;
        let l2 = *al as f64;
        *ar = (l2 + (if r < l2 { 360.0 - ((l2 - r) % 360.0) } else { r - l2 } % 360.0)) as f32;
    }
}

fn arc_tree(a: i32, b: i32, al: f32, ar: f32) -> Option<Node> {
    if a < b {
        // Pillow builds the tree for the ellipse lying on its side and mirrors it.
        let mut t = arc_tree(b, a, 90.0 - ar, 90.0 - al);
        if let Some(n) = t.as_mut() {
            transpose(n);
        }
        return t;
    }
    let (mut al, mut ar) = (al, ar);
    normalize_angles(&mut al, &mut ar);
    if ar == al + 360.0 {
        return None;
    }
    let pi = std::f64::consts::PI;
    let (af, bf) = (a as f64, b as f64);
    let (alf, arf) = (al as f64, ar as f64);
    let aa = (a * a - b * b) as f64;
    let lc = Node::Clip(-af * (alf * pi / 180.0).sin(), bf * (alf * pi / 180.0).cos(), aa * (alf * pi / 90.0).sin() / 2.0);
    let rc = Node::Clip(af * (arf * pi / 180.0).sin(), -bf * (arf * pi / 180.0).cos(), -aa * (arf * pi / 90.0).sin() / 2.0);
    let both = |l: Node, r: Node, and: bool| if and { Node::And(Box::new(l), Box::new(r)) } else { Node::Or(Box::new(l), Box::new(r)) };
    let half = |x: f32| (x / 180.0) as i32;
    Some(if alf % 180.0 == 0.0 || arf % 180.0 == 0.0 {
        both(lc, rc, ar - al < 180.0)
    } else if (half(al) + half(ar)) % 2 == 1 {
        let lside = Node::Clip(0.0, if half(al) % 2 == 0 { 1.0 } else { -1.0 }, 0.0);
        let rside = Node::Clip(0.0, if half(ar) % 2 == 0 { 1.0 } else { -1.0 }, 0.0);
        both(both(lside, lc, true), both(rside, rc, true), false)
    } else {
        let and = ar - al < 180.0;
        let side = Node::Clip(0.0, if ar < 180.0 || ar > 540.0 { 1.0 } else { -1.0 }, 0.0);
        both(both(lc, rc, and), side, and)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: Rgba = [255, 255, 255, 255];

    fn count(im: &Image) -> usize {
        im.px.iter().filter(|p| **p == W).count()
    }

    #[test]
    fn a_line_includes_both_ends() {
        let mut im = Image::new(8, 8, [0; 4]);
        im.line(&[(1.0, 1.0), (6.0, 1.0)], W, 1);
        assert_eq!(count(&im), 6);
        let mut im = Image::new(8, 8, [0; 4]);
        im.line(&[(1.9, 1.2), (1.0, 6.7)], W, 1);
        assert_eq!(count(&im), 6);
        assert_eq!(im.get(1, 6), W);
    }

    #[test]
    fn a_rectangle_fills_or_outlines() {
        let mut im = Image::new(8, 8, [0; 4]);
        im.rectangle(1, 1, 4, 3, Some(W), None);
        assert_eq!(count(&im), 12);
        let mut im = Image::new(8, 8, [0; 4]);
        im.rectangle(1, 1, 4, 4, None, Some(W));
        assert_eq!(count(&im), 12);
    }

    #[test]
    fn a_filled_ellipse_is_symmetric_and_stays_in_its_box() {
        let mut im = Image::new(16, 16, [0; 4]);
        im.ellipse(2, 3, 12, 9, Some(W), None, 1);
        for y in 0..16 {
            for x in 0..16 {
                if im.get(x, y) == W {
                    assert!((2..=12).contains(&x) && (3..=9).contains(&y));
                    assert_eq!(im.get(14 - x, y), W);
                    assert_eq!(im.get(x, 12 - y), W);
                }
            }
        }
        assert_eq!(im.get(7, 6), W);
        assert_eq!(im.get(2, 6), W);
    }

    #[test]
    fn a_triangle_fills_between_its_corners() {
        let mut im = Image::new(16, 16, [0; 4]);
        im.polygon(&[(2.0, 2.0), (12.0, 2.0), (2.0, 12.0)], W);
        assert_eq!(im.get(3, 3), W);
        assert_eq!(im.get(12, 2), W);
        assert_eq!(im.get(2, 12), W);
        assert_eq!(im.get(12, 12), [0; 4]);
    }

    #[test]
    fn png_has_the_signature() {
        let im = Image::new(2, 3, W);
        assert_eq!(&im.png()[..8], b"\x89PNG\r\n\x1a\n");
    }
}
