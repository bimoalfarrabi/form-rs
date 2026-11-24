//! Port of `toProcessedHeatmap` (heatmap.js:259-330).

use anyhow::Result;

use crate::preprocess::resample::resize_premultiplied_rgba;

const CANVAS_SIZE: u32 = 1000;
const MAX_BLUR: u32 = CANVAS_SIZE * 15 / 100; // floor(1000*0.15) = 150
const PADDING: u32 = (MAX_BLUR * 5).div_ceil(2); // ceil(150*2.5) = 375
const BIG_BLUR_RADIUS: u32 = MAX_BLUR;
const INNER_BLUR_RADIUS: u32 = (MAX_BLUR * 12 + 50) / 100; // round(0.12*150) = 18, min applied below
const CONTOUR_RADIUS: u32 = 5;
const INNER_BLUR: u32 = if INNER_BLUR_RADIUS < 1 {
    1
} else {
    INNER_BLUR_RADIUS
};

/// Result of the heatmap preprocessing pass.
pub struct ProcessedHeat {
    pub width: u32,
    pub height: u32,
    /// Packed RGBA: R = contour blur, G = big blur, B = inner blur, A = 255.
    pub rgba: Vec<u8>,
}

pub fn process(rgba: &[u8], width: u32, height: u32) -> Result<ProcessedHeat> {
    anyhow::ensure!(
        rgba.len() == (width as usize) * (height as usize) * 4,
        "input buffer size mismatch"
    );

    let ratio = f64::from(width) / f64::from(height);
    let mut img_w = CANVAS_SIZE;
    let mut img_h = CANVAS_SIZE;
    if ratio > 1.0 {
        img_h = (f64::from(CANVAS_SIZE) / ratio).floor() as u32;
    } else {
        img_w = (f64::from(CANVAS_SIZE) * ratio).floor() as u32;
    }

    let out_w = img_w + 2 * PADDING;
    let out_h = img_h + 2 * PADDING;

    // White canvas, logo composited straight-alpha over white at (375, 375).
    let logo = resize_premultiplied_rgba(rgba, width, height, img_w, img_h);
    let mut canvas = vec![255u8; (out_w as usize) * (out_h as usize) * 4];
    for y in 0..img_h {
        let src_row = (y as usize) * img_w as usize * 4;
        let dst_row = ((y + PADDING) as usize) * out_w as usize * 4 + PADDING as usize * 4;
        for x in 0..img_w {
            let s = src_row + x as usize * 4;
            let d = dst_row + x as usize * 4;
            let a = logo[s + 3] as f64 / 255.0;
            for c in 0..3 {
                canvas[d + c] = (logo[s + c] as f64 * a + 255.0 * (1.0 - a)).round() as u8;
            }
            canvas[d + 3] = 255;
        }
    }

    let gray = to_gray(&canvas, out_w as usize, out_h as usize);
    let big = multi_pass_blur_gray(&gray, out_w, out_h, BIG_BLUR_RADIUS, 3);
    let inner = multi_pass_blur_gray(&gray, out_w, out_h, INNER_BLUR, 3);
    let contour = multi_pass_blur_gray(&gray, out_w, out_h, CONTOUR_RADIUS, 1);

    let total = (out_w as usize) * (out_h as usize);
    let mut out = vec![255u8; total * 4];
    for i in 0..total {
        out[i * 4] = contour[i];
        out[i * 4 + 1] = big[i];
        out[i * 4 + 2] = inner[i];
    }
    Ok(ProcessedHeat {
        width: out_w,
        height: out_h,
        rgba: out,
    })
}

fn to_gray(rgba: &[u8], w: usize, h: usize) -> Vec<u8> {
    rgba.as_chunks::<4>()
        .0
        .iter()
        .take(w * h)
        .map(|px| {
            // Truncation on purpose: JS `| 0`.
            (0.299 * f64::from(px[0]) + 0.587 * f64::from(px[1]) + 0.114 * f64::from(px[2])) as u8
        })
        .collect()
}

/// Integral-image box blur; window clipped to the image, `out = round(sum/area)`.
fn blur_gray(gray: &[u8], w: u32, h: u32, radius: u32) -> Vec<u8> {
    if radius == 0 {
        return gray.to_vec();
    }
    let (w, h) = (w as usize, h as usize);
    let mut integral = vec![0u64; w * h];
    for y in 0..h {
        let mut row_sum = 0u64;
        for x in 0..w {
            let idx = y * w + x;
            row_sum += u64::from(gray[idx]);
            integral[idx] = row_sum + if y > 0 { integral[idx - w] } else { 0 };
        }
    }
    let r = radius as i64;
    let mut out = vec![0u8; w * h];
    for y in 0..h {
        let y1 = (y as i64 - r).max(0) as usize;
        let y2 = ((y as i64 + r).min(h as i64 - 1)) as usize;
        for x in 0..w {
            let x1 = (x as i64 - r).max(0) as usize;
            let x2 = ((x as i64 + r).min(w as i64 - 1)) as usize;
            let a = integral[y2 * w + x2];
            let b = if x1 > 0 { integral[y2 * w + x1 - 1] } else { 0 };
            let c = if y1 > 0 {
                integral[(y1 - 1) * w + x2]
            } else {
                0
            };
            let d = if x1 > 0 && y1 > 0 {
                integral[(y1 - 1) * w + x1 - 1]
            } else {
                0
            };
            // i64: the intermediate `(a - b) - c` can go negative even when the
            // final window sum does not (JS Numbers absorb this silently).
            let sum = a as i64 - b as i64 - c as i64 + d as i64;
            let area = ((x2 - x1 + 1) * (y2 - y1 + 1)) as f64;
            out[y * w + x] = crate::preprocess::clamp_u8_round(sum as f64 / area);
        }
    }
    out
}

fn multi_pass_blur_gray(gray: &[u8], w: u32, h: u32, radius: u32, passes: u32) -> Vec<u8> {
    if radius == 0 || passes <= 1 {
        return blur_gray(gray, w, h, radius);
    }
    let mut cur = gray.to_vec();
    for _ in 0..passes {
        cur = blur_gray(&cur, w, h, radius);
    }
    cur
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_blur_of_constant_image_is_constant() {
        let w = 9u32;
        let h = 7u32;
        let gray = vec![137u8; (w * h) as usize];
        for radius in [0u32, 1, 3, 40] {
            let out = blur_gray(&gray, w, h, radius);
            assert!(out.iter().all(|&v| v == 137), "radius {radius}");
        }
    }

    #[test]
    fn single_bright_pixel_spreads_with_clipped_area_weights() {
        // One bright pixel at (0,0) of a 5x5 image, radius 2.
        let w = 5u32;
        let h = 5u32;
        let mut gray = vec![0u8; (w * h) as usize];
        gray[0] = 100;
        let out = blur_gray(&gray, w, h, 2);
        // (0,0): window rows 0..=2 x cols 0..=2, area 9, sum 100 -> round(11.1) = 11
        assert_eq!(out[0], 11);
        // (1,0): window cols 0..=3, area 12 -> round(8.33) = 8
        assert_eq!(out[1], 8);
        // (2,0): window cols 0..=4 (full width), rows 0..=2, area 15 -> round(6.67) = 7
        assert_eq!(out[2], 7);
        // (2,2): window covers the whole 5x5 image, area 25, sum 100 -> 4
        assert_eq!(out[2 * 5 + 2], 4);
        // (3,0): window rows 0..=2 x cols 1..=4 excludes (0,0)
        assert_eq!(out[3], 0);
        // (4,4): window rows 2..=4 x cols 2..=4 excludes (0,0)
        assert_eq!(out[4 * 5 + 4], 0);
    }

    #[test]
    fn box_blur_clips_window_to_image_edges() {
        // Bright pixel at (0,0) of a 5x3 image, radius 2: the window is
        // clipped in y for every pixel (rows -2..2 do not exist).
        let w = 5u32;
        let h = 3u32;
        let mut gray = vec![0u8; (w * h) as usize];
        gray[0] = 100;
        let out = blur_gray(&gray, w, h, 2);
        // (2,1): window rows 0..=2 x cols 0..=4, area 15, sum 100 -> round(6.67) = 7
        assert_eq!(out[5 + 2], 7);
        // (4,2): window rows 0..=2 x cols 2..=4 excludes (0,0)
        assert_eq!(out[2 * 5 + 4], 0);
    }
}
