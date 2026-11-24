//! Premultiplied-alpha resampling, standing in for canvas `drawImage`.

use image::imageops::FilterType;

/// Resize a straight-alpha RGBA8 buffer the way a browser canvas does: the
/// pixels are composited premultiplied, so colour channels are premultiplied
/// by alpha, resampled, then unpremultiplied again.
pub fn resize_premultiplied_rgba(rgba: &[u8], w: u32, h: u32, nw: u32, nh: u32) -> Vec<u8> {
    let mut pre = rgba.to_vec();
    for px in pre.as_chunks_mut::<4>().0 {
        let a = u32::from(px[3]);
        for c in &mut px[..3] {
            *c = ((u32::from(*c) * a + 127) / 255) as u8;
        }
    }
    let img = image::RgbaImage::from_raw(w, h, pre).expect("buffer matches dimensions");
    let resized = image::imageops::resize(&img, nw.max(1), nh.max(1), FilterType::Lanczos3);
    let mut out = resized.into_raw();
    for px in out.as_chunks_mut::<4>().0 {
        let a = u32::from(px[3]);
        for c in &mut px[..3] {
            // a == 0 leaves the channel at 0, like the zeroed premultiplied data.
            *c = ((u32::from(*c) * 255 + a / 2).checked_div(a).unwrap_or(0)).min(255) as u8;
        }
    }
    out
}
