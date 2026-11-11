//! Pictures for Import model: PNG and JPEG decoding to RGBA, and Pillow's Lanczos resize.
//!
//! The converter names a texture by a hash of its pixels, so the resize must give Pillow's pixels exactly.
//! `resize` follows Pillow's `Image.resize` for an RGBA picture: premultiply the alpha, resample across then down
//! with fixed point weights (`Resample.c`), and divide the alpha out again.

/// The most pixels Pillow opens with `MAX_IMAGE_PIXELS = 4096 * 4096`: twice that number.
const MAX_PIXELS: u64 = 2 * 4096 * 4096;

/// An RGBA picture, 8 bits a channel, rows from the top.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rgba {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u8>,
}

impl Rgba {
    pub fn filled(w: usize, h: usize, colour: [u8; 4]) -> Rgba {
        Rgba { w, h, px: colour.repeat(w * h) }
    }
}

fn check_size(w: u64, h: u64) -> Result<(), String> {
    if w * h > MAX_PIXELS {
        return Err(format!(
            "Image size ({} pixels) exceeds limit of {MAX_PIXELS} pixels, could be decompression bomb DOS attack.",
            w * h
        ));
    }
    if w == 0 || h == 0 {
        return Err("the image is empty".into());
    }
    Ok(())
}

/// Decodes a PNG or JPEG file to RGBA, as Pillow's `Image.open(...).convert("RGBA")` does.
pub fn decode(data: &[u8]) -> Result<Rgba, String> {
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        decode_png(data)
    } else if data.starts_with(b"\xFF\xD8") {
        decode_jpeg(data)
    } else {
        Err("cannot identify image file".into())
    }
}

fn decode_png(data: &[u8]) -> Result<Rgba, String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(data));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let (w, h) = (reader.info().width as u64, reader.info().height as u64);
    check_size(w, h)?;
    let size = reader.output_buffer_size().ok_or("the image is too large")?;
    let mut buf = vec![0u8; size];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    buf.truncate(info.buffer_size());
    let px: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf.chunks_exact(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&p| [p, p, p, 255]).collect(),
        png::ColorType::Indexed => return Err("the image's palette could not be read".into()),
    };
    let (w, h) = (w as usize, h as usize);
    if px.len() != w * h * 4 {
        return Err("the image is cut short".into());
    }
    Ok(Rgba { w, h, px })
}

fn decode_jpeg(data: &[u8]) -> Result<Rgba, String> {
    use zune_jpeg::zune_core::{bytestream::ZCursor, colorspace::ColorSpace, options::DecoderOptions};
    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGB);
    let mut decoder = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(data), options);
    decoder.decode_headers().map_err(|e| e.to_string())?;
    let info = decoder.info().ok_or("the image has no header")?;
    let (w, h) = (info.width as u64, info.height as u64);
    check_size(w, h)?;
    let rgb = decoder.decode().map_err(|e| e.to_string())?;
    let (w, h) = (w as usize, h as usize);
    if rgb.len() != w * h * 3 {
        return Err("the image is cut short".into());
    }
    Ok(Rgba { w, h, px: rgb.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect() })
}

// ---------------------------------------------------------------------------------------------
// Pillow's resize

const PRECISION_BITS: u32 = 32 - 8 - 2;

fn sinc(x: f64) -> f64 {
    if x == 0.0 {
        return 1.0;
    }
    let x = x * std::f64::consts::PI;
    x.sin() / x
}

fn lanczos(x: f64) -> f64 {
    if (-3.0..3.0).contains(&x) {
        sinc(x) * sinc(x / 3.0)
    } else {
        0.0
    }
}

/// For each output position: the first input position, the number of weights, and the weights in fixed point.
struct Coeffs {
    ksize: usize,
    bounds: Vec<(usize, usize)>,
    kk: Vec<i32>,
}

fn coeffs(in_size: usize, out_size: usize) -> Coeffs {
    let scale = in_size as f64 / out_size as f64;
    let filterscale = scale.max(1.0);
    let support = 3.0 * filterscale;
    let ksize = support.ceil() as usize * 2 + 1;
    let mut kk = vec![0i32; out_size * ksize];
    let mut bounds = Vec::with_capacity(out_size);
    let ss = 1.0 / filterscale;
    let mut k = vec![0f64; ksize];
    for xx in 0..out_size {
        let center = (xx as f64 + 0.5) * scale;
        let xmin = ((center - support + 0.5) as i64).max(0);
        let xmax = ((center + support + 0.5) as i64).min(in_size as i64);
        let n = (xmax - xmin).max(0) as usize;
        let mut ww = 0.0;
        for (x, w) in k.iter_mut().enumerate().take(n) {
            *w = lanczos((x as f64 + xmin as f64 - center + 0.5) * ss);
            ww += *w;
        }
        for (x, w) in k.iter().enumerate().take(n) {
            let w = if ww != 0.0 { *w / ww } else { *w };
            let fixed = w * (1u32 << PRECISION_BITS) as f64;
            kk[xx * ksize + x] = if w < 0.0 { (-0.5 + fixed) as i32 } else { (0.5 + fixed) as i32 };
        }
        bounds.push((xmin as usize, n));
    }
    Coeffs { ksize, bounds, kk }
}

fn clip8(v: i32) -> u8 {
    (v >> PRECISION_BITS).clamp(0, 255) as u8
}

/// One pass of the resample along x. `rows` picks the input rows to keep.
fn horizontal(src: &[u8], w: usize, rows: std::ops::Range<usize>, out_w: usize, c: &Coeffs) -> Vec<u8> {
    let mut out = Vec::with_capacity(rows.len() * out_w * 4);
    for y in rows {
        let line = &src[y * w * 4..(y + 1) * w * 4];
        for xx in 0..out_w {
            let (xmin, n) = c.bounds[xx];
            let k = &c.kk[xx * c.ksize..xx * c.ksize + n];
            let mut ss = [1i32 << (PRECISION_BITS - 1); 4];
            for (x, k) in k.iter().enumerate() {
                let p = &line[(x + xmin) * 4..(x + xmin) * 4 + 4];
                for b in 0..4 {
                    ss[b] = ss[b].wrapping_add(p[b] as i32 * k);
                }
            }
            out.extend(ss.map(clip8));
        }
    }
    out
}

fn vertical(src: &[u8], w: usize, out_h: usize, c: &Coeffs) -> Vec<u8> {
    let mut out = Vec::with_capacity(out_h * w * 4);
    for yy in 0..out_h {
        let (ymin, n) = c.bounds[yy];
        let k = &c.kk[yy * c.ksize..yy * c.ksize + n];
        for x in 0..w {
            let mut ss = [1i32 << (PRECISION_BITS - 1); 4];
            for (y, k) in k.iter().enumerate() {
                let p = &src[((y + ymin) * w + x) * 4..((y + ymin) * w + x) * 4 + 4];
                for b in 0..4 {
                    ss[b] = ss[b].wrapping_add(p[b] as i32 * k);
                }
            }
            out.extend(ss.map(clip8));
        }
    }
    out
}

/// Pillow's `im.resize((w, h), Image.LANCZOS)` for an RGBA picture. Sizes must not be 0.
pub fn resize(im: &Rgba, w: usize, h: usize) -> Rgba {
    if (im.w, im.h) == (w, h) || w == 0 || h == 0 || im.w == 0 || im.h == 0 {
        return im.clone();
    }
    // RGBA to premultiplied alpha
    let mut px = im.px.clone();
    for p in px.chunks_exact_mut(4) {
        let a = p[3] as u32;
        for c in &mut p[..3] {
            let t = *c as u32 * a + 128;
            *c = (((t >> 8) + t) >> 8) as u8;
        }
    }
    let (mut cur_w, mut cur_h) = (im.w, im.h);
    let vert = coeffs(im.h, h);
    let mut vert_bounds = vert.bounds.clone();
    if im.w != w {
        // Only the rows the second pass reads are resampled
        let first = vert_bounds[0].0;
        let last = vert_bounds[h - 1].0 + vert_bounds[h - 1].1;
        let horiz = coeffs(im.w, w);
        px = horizontal(&px, im.w, first..last, w, &horiz);
        for b in &mut vert_bounds {
            b.0 -= first;
        }
        cur_w = w;
        cur_h = last - first;
    }
    if im.h != h {
        let c = Coeffs { ksize: vert.ksize, bounds: vert_bounds, kk: vert.kk };
        px = vertical(&px, cur_w, h, &c);
        cur_h = h;
    }
    debug_assert_eq!(px.len(), cur_w * cur_h * 4);
    // Back to plain alpha
    for p in px.chunks_exact_mut(4) {
        let a = p[3] as u32;
        if a != 255 && a != 0 {
            for c in &mut p[..3] {
                *c = (255 * *c as u32 / a).min(255) as u8;
            }
        }
    }
    Rgba { w, h, px }
}
