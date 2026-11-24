//! Port of `toProcessedLiquidMetal` (liquid-metal.js:341-540).
//!
//! Input is a straight-alpha RGBA8 raster of the logo (already black-filled
//! and padded by the caller, as in `load()`); the SVG 4096-upscale branch of
//! the JS is not applicable here.

use anyhow::Result;

use super::clamp_u8_round;
use super::resample::resize_premultiplied_rgba;

/// Result of the metal preprocessing pass.
pub struct ProcessedMetal {
    pub width: u32,
    pub height: u32,
    /// Packed RGBA: R = Poisson height, G = alpha, B = 255, A = 255.
    pub rgba: Vec<u8>,
}

const WORKING_SIZE: f64 = 512.0;
const ITERATIONS: usize = 40;
const C: f64 = 0.01;
const OMEGA: f64 = 1.9;

pub fn process(rgba: &[u8], width: u32, height: u32) -> Result<ProcessedMetal> {
    let pixels = width as usize * height as usize;
    if rgba.len() != pixels * 4 {
        anyhow::bail!(
            "input buffer is {} bytes, expected {} for {}x{}",
            rgba.len(),
            pixels * 4,
            width,
            height
        );
    }

    // JS: scale = 512 / min(origW, origH), working grid = round(orig * scale).
    let min_dim = width.min(height) as f64;
    let scale = WORKING_SIZE / min_dim;
    let gw = (width as f64 * scale).round() as u32;
    let gh = (height as f64 * scale).round() as u32;

    // JS shapeCanvas ← drawImage(img, 0, 0, width, height): the browser
    // resamples premultiplied alpha, so we do the same.
    let work = resize_premultiplied_rgba(rgba, width, height, gw, gh);

    // shapeMask[i] = alpha === 0 ? 0 : 1.
    let n = gw as usize * gh as usize;
    let mut shape = vec![0u8; n];
    for (i, px) in work.as_chunks::<4>().0.iter().enumerate() {
        shape[i] = u8::from(px[3] != 0);
    }

    let u = solve_poisson(&shape, gw, gh);

    // JS: maxVal = max(u) over interior indices only; 0 means the solve never
    // moved, where the JS divides 0/0 and stores the NaN as 0.
    let mut max_val = 0.0f32;
    for y in 1..gh as usize - 1 {
        for x in 1..gw as usize - 1 {
            let idx = y * gw as usize + x;
            if is_interior(&shape, gw as usize, gh as usize, x, y) && u[idx] > max_val {
                max_val = u[idx];
            }
        }
    }

    // Height field on the working grid: outside (255,255,255,0), inside
    // gray = 255*(1 - u/maxVal).
    let mut field = vec![0u8; n * 4];
    for (i, px) in field.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        if shape[i] != 0 {
            let gray = if max_val > 0.0 {
                clamp_u8_round(255.0 * (1.0 - f64::from(u[i]) / f64::from(max_val)))
            } else {
                0
            };
            px[0] = gray;
            px[1] = gray;
            px[2] = gray;
            px[3] = 255;
        } else {
            px[0] = 255;
            px[1] = 255;
            px[2] = 255;
            px[3] = 0;
        }
    }

    // JS: tempCanvas drawn into the original-size canvas with
    // imageSmoothingQuality = "high".
    let upscaled = resize_premultiplied_rgba(&field, gw, gh, width, height);

    // Channel packing: R = Poisson height, G = original alpha, B = A = 255.
    let mut out = vec![0u8; pixels * 4];
    for i in 0..pixels {
        let a = rgba[i * 4 + 3];
        let ua = upscaled[i * 4 + 3];
        out[i * 4] = if a == 0 {
            255
        } else if ua == 0 {
            0
        } else {
            upscaled[i * 4]
        };
        out[i * 4 + 1] = a;
        out[i * 4 + 2] = 255;
        out[i * 4 + 3] = 255;
    }

    Ok(ProcessedMetal {
        width,
        height,
        rgba: out,
    })
}

/// A shape pixel is interior iff none of its 8 neighbours is missing from the
/// shape (this also excludes the image border). Non-shape pixels are neither.
fn is_interior(shape: &[u8], w: usize, h: usize, x: usize, y: usize) -> bool {
    if x == 0 || y == 0 || x == w - 1 || y == h - 1 || shape[y * w + x] == 0 {
        return false;
    }
    shape[y * w + x - 1] != 0
        && shape[y * w + x + 1] != 0
        && shape[(y - 1) * w + x] != 0
        && shape[(y + 1) * w + x] != 0
        && shape[(y - 1) * w + x - 1] != 0
        && shape[(y - 1) * w + x + 1] != 0
        && shape[(y + 1) * w + x - 1] != 0
        && shape[(y + 1) * w + x + 1] != 0
}

/// Red-black SOR (`solvePoissonSparse`): 4-connected neighbours inside the
/// shape only; boundary and outside pixels stay 0 and are never updated.
/// Math runs in f64 but is stored as f32 on every write (Float32Array).
fn solve_poisson(shape: &[u8], width: u32, height: u32) -> Vec<f32> {
    let w = width as usize;
    let h = height as usize;
    let mut u = vec![0.0f32; w * h];

    let mut interior = Vec::new();
    for y in 1..h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            if is_interior(shape, w, h, x, y) {
                interior.push(y * w + x);
            }
        }
    }
    // Red-black ordering by (x + y) % 2: two sweeps per iteration.
    let mut red = Vec::new();
    let mut black = Vec::new();
    for &idx in &interior {
        if (idx % w + idx / w).is_multiple_of(2) {
            red.push(idx);
        } else {
            black.push(idx);
        }
    }

    for _ in 0..ITERATIONS {
        for half in [&red, &black] {
            for &idx in half {
                let mut sum = 0.0f64;
                // buildSparseData: a neighbour counts only when it is shape;
                // interior pixels are never on the image border.
                if shape[idx + 1] != 0 {
                    sum += f64::from(u[idx + 1]);
                }
                if shape[idx - 1] != 0 {
                    sum += f64::from(u[idx - 1]);
                }
                if shape[idx - w] != 0 {
                    sum += f64::from(u[idx - w]);
                }
                if shape[idx + w] != 0 {
                    sum += f64::from(u[idx + w]);
                }
                let new = (C + sum) / 4.0;
                u[idx] = (OMEGA * new + (1.0 - OMEGA) * f64::from(u[idx])) as f32;
            }
        }
    }
    u
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 8x8 grid, filled square at rows/cols 2..=5. Interior (all 8 neighbours
    /// in the shape) is the 3..=4 block; the square is symmetric under
    /// (x, y) -> (7 - x, 7 - y).
    #[test]
    fn square_field_is_symmetric_and_bounded() {
        let (w, h) = (8usize, 8usize);
        let mut shape = vec![0u8; w * h];
        for y in 2..6 {
            for x in 2..6 {
                shape[y * w + x] = 1;
            }
        }
        let u = solve_poisson(&shape, w as u32, h as u32);

        for y in 0..h {
            for x in 0..w {
                let idx = y * w + x;
                let interior = (3..=4).contains(&x) && (3..=4).contains(&y);
                if interior {
                    assert!(
                        u[idx].is_finite() && u[idx] > 0.0,
                        "interior ({x},{y}) = {}",
                        u[idx]
                    );
                } else {
                    assert_eq!(u[idx], 0.0, "non-interior ({x},{y}) must stay 0");
                }
                let mirrored = u[(h - 1 - y) * w + (w - 1 - x)];
                assert_eq!(u[idx], mirrored, "asymmetric at ({x},{y})");
            }
        }
    }
}
