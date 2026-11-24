//! CPU ports of `toProcessedLiquidMetal` and `toProcessedHeatmap`.
//!
//! Both take a straight-alpha RGBA8 raster of the logo (black pixels, alpha =
//! shape) and return the packed map the shaders sample.

pub mod heat;
pub mod metal;
pub mod resample;

/// Rust equivalent of `Uint8ClampedArray` rounding (round half to even).
pub fn clamp_u8_round(v: f64) -> u8 {
    let r = v.round();
    let r = if (v - v.trunc()).abs() == 0.5 {
        // round half to even
        let t = v.trunc();
        if (t as i64) % 2 == 0 {
            t
        } else {
            t + v.signum()
        }
    } else {
        r
    };
    r.clamp(0.0, 255.0) as u8
}
