//! Logo decoding and the browser's `load()` preparation pipeline.
//!
//! Mirrors `main.js:70` so the pixels handed to preprocessing match what the
//! web app produces.

use anyhow::{bail, Context, Result};

/// A straight-alpha RGBA8 raster.
#[derive(Clone)]
pub struct Raster {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl Raster {
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Self {
        Self {
            rgba,
            width,
            height,
        }
    }

    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.width + x) * 4) as usize;
        [
            self.rgba[i],
            self.rgba[i + 1],
            self.rgba[i + 2],
            self.rgba[i + 3],
        ]
    }
}

/// The default logo, byte-identical to the inline SVG in `main.js:77`.
pub const DEFAULT_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="600" height="600" viewBox="0 0 600 600"><path fill="black" d="M300 40 540 180v240L300 560 60 420V180zm0 78L128 218v164l172 100 172-100V218zm0 75 106 62v90l-106 62-106-62v-90z" fill-rule="evenodd"/></svg>"#;

/// Rasterise an SVG at its intrinsic size, scaling so the long side is `long_side`.
fn render_svg(bytes: &[u8], long_side: Option<u32>) -> Result<Raster> {
    let text = std::str::from_utf8(bytes).context("SVG is not valid UTF-8")?;
    let opt = resvg::usvg::Options {
        font_family: "sans-serif".to_string(),
        ..Default::default()
    };
    let tree = resvg::usvg::Tree::from_str(text, &opt).context("parsing SVG")?;

    let size = tree.size();
    let (mut w, mut h) = (size.width(), size.height());
    if let Some(target) = long_side {
        let scale = target as f32 / w.max(h).max(1.0);
        w *= scale;
        h *= scale;
    }
    let (pw, ph) = (w.round().max(1.0) as u32, h.round().max(1.0) as u32);

    let mut pixmap = tiny_skia::Pixmap::new(pw, ph).context("allocating SVG pixmap")?;
    let sx = pw as f32 / size.width().max(1.0);
    let sy = ph as f32 / size.height().max(1.0);
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(sx, sy),
        &mut pixmap.as_mut(),
    );

    // tiny-skia gives premultiplied RGBA; the pipeline wants straight alpha.
    let mut rgba = Vec::with_capacity((pw * ph * 4) as usize);
    for px in pixmap.data().as_chunks::<4>().0 {
        let a = px[3];
        if a == 0 {
            rgba.extend_from_slice(&[0, 0, 0, 0]);
        } else {
            let un = |v: u8| ((v as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8;
            rgba.extend_from_slice(&[un(px[0]), un(px[1]), un(px[2]), a]);
        }
    }
    Ok(Raster::new(pw, ph, rgba))
}

/// Decode a dropped/opened file into a straight-alpha raster.
pub fn decode(bytes: &[u8], name: &str) -> Result<Raster> {
    let is_svg = name.to_ascii_lowercase().ends_with(".svg")
        || bytes.starts_with(b"<svg")
        || (bytes.starts_with(b"<?xml") && bytes.windows(4).any(|w| w == b"<svg"));
    if is_svg {
        // `load()` upscales small SVGs to 1600 on the long side.
        return render_svg(bytes, Some(1600));
    }
    let img = image::load_from_memory(bytes)
        .with_context(|| format!("decoding {name}"))?
        .to_rgba8();
    let (w, h) = img.dimensions();
    Ok(Raster::new(w, h, img.into_raw()))
}

/// `load()`'s geometry: scale cap, alpha requirement, alpha bbox, 8% pad and
/// the black `source-in` fill (main.js:70).
pub fn prepare(raster: Raster) -> Result<Raster> {
    let long = raster.width.max(raster.height) as f32;
    let factor = if long > 1600.0 { 1600.0 / long } else { 1.0 };
    let scaled = if factor < 1.0 {
        let nw = ((raster.width as f32 * factor).round() as u32).max(1);
        let nh = ((raster.height as f32 * factor).round() as u32).max(1);
        resize_premultiplied(&raster, nw, nh)
    } else {
        raster
    };

    let has_alpha = scaled.rgba.as_chunks::<4>().0.iter().any(|p| p[3] < 250);
    if !has_alpha {
        bail!("Use SVG or PNG with a transparent background.");
    }

    let (mut left, mut right, mut top, mut bottom) = (scaled.width, 0u32, scaled.height, 0u32);
    for y in 0..scaled.height {
        for x in 0..scaled.width {
            if scaled.pixel(x, y)[3] > 20 {
                left = left.min(x);
                right = right.max(x);
                top = top.min(y);
                bottom = bottom.max(y);
            }
        }
    }
    if right < left {
        bail!("This image is empty.");
    }

    let w = right - left + 1;
    let h = bottom - top + 1;
    let pad = ((w.max(h) as f32) * 0.08).ceil() as u32;
    let (cw, ch) = (w + pad * 2, h + pad * 2);

    // Crop 1:1 (no rescale), then force RGB to black while keeping alpha.
    let mut out = vec![0u8; (cw * ch * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let src = scaled.pixel(left + x, top + y);
            let dst = (((y + pad) * cw + (x + pad)) * 4) as usize;
            out[dst] = 0;
            out[dst + 1] = 0;
            out[dst + 2] = 0;
            out[dst + 3] = src[3];
        }
    }
    Ok(Raster::new(cw, ch, out))
}

/// High-quality resize on premultiplied alpha, matching the canvas path.
pub fn resize_premultiplied(src: &Raster, nw: u32, nh: u32) -> Raster {
    let mut pre = Vec::with_capacity(src.rgba.len());
    for p in src.rgba.as_chunks::<4>().0 {
        let a = p[3] as u32;
        pre.push(((p[0] as u32 * a + 127) / 255) as u8);
        pre.push(((p[1] as u32 * a + 127) / 255) as u8);
        pre.push(((p[2] as u32 * a + 127) / 255) as u8);
        pre.push(p[3]);
    }
    let buf = image::RgbaImage::from_raw(src.width, src.height, pre)
        .expect("premultiplied buffer matches dimensions");
    let resized = image::imageops::resize(&buf, nw, nh, image::imageops::FilterType::Lanczos3);
    let mut out = Vec::with_capacity((nw * nh * 4) as usize);
    for p in resized.pixels() {
        let a = p[3];
        if a == 0 {
            out.extend_from_slice(&[0, 0, 0, 0]);
        } else {
            let un = |v: u8| ((v as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8;
            out.extend_from_slice(&[un(p[0]), un(p[1]), un(p[2]), a]);
        }
    }
    Raster::new(nw, nh, out)
}

/// The bundled default logo, already prepared the way `load()` would leave it.
pub fn default_logo() -> Result<Raster> {
    prepare(render_svg(DEFAULT_SVG.as_bytes(), None)?)
}
