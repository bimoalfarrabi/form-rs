//! Metal preprocessing reference check:
//! `cargo run -p form-render --bin preprocess_ref`.
//!
//! Runs the Rust `toProcessedLiquidMetal` port on the browser's source raster
//! and prints per-channel diffs against the browser's `proc-metal.png`.

use std::path::PathBuf;

use form_render::preprocess::metal;

fn main() -> anyhow::Result<()> {
    let src_path = form_render::fixtures::map_path("proc-source.png");
    let ref_path = form_render::fixtures::map_path("proc-metal.png");
    let out_path = PathBuf::from("/tmp/opencode/rust-proc-metal.png");

    let src = image::open(&src_path)?.to_rgba8();
    let (sw, sh) = src.dimensions();
    let expected = image::open(&ref_path)?.to_rgba8();
    let (ew, eh) = expected.dimensions();

    // The captured proc-metal.png is 4096x4096: the default logo is an SVG
    // File, so the JS took the 4096-upscale branch (INVENTORY 3.2 step 2
    // claims it is dead, but the capture proves otherwise). The caller owns
    // that upscale; here we approximate its input by resampling proc-source
    // to the reference size with the same premultiplied resampler.
    let (w, h) = if (ew, eh) == (sw, sh) {
        (sw, sh)
    } else {
        (ew, eh)
    };
    let input = if (w, h) == (sw, sh) {
        src.into_raw()
    } else {
        form_render::preprocess::resample::resize_premultiplied_rgba(src.as_raw(), sw, sh, w, h)
    };

    let processed = metal::process(&input, w, h)?;

    form_render::write_png(
        &out_path,
        &processed.rgba,
        processed.width,
        processed.height,
    )?;
    println!(
        "wrote {} ({}x{})",
        out_path.display(),
        processed.width,
        processed.height
    );

    let count = processed.width as usize * processed.height as usize;
    println!(
        "input: {src_path:?} {sw}x{sh} ({} upscaled to {w}x{h}); reference: {ref_path:?} {ew}x{eh}",
        if (w, h) == (sw, sh) { "not" } else { "Lanczos" },
    );
    for (c, name) in ["R (field)", "G (alpha)", "B", "A"].iter().enumerate() {
        let mut sum = 0.0f64;
        let mut max = 0u8;
        let mut over2 = 0usize;
        let mut over8 = 0usize;
        for i in 0..count {
            let d = processed.rgba[i * 4 + c].abs_diff(expected.as_raw()[i * 4 + c]);
            sum += f64::from(d);
            max = max.max(d);
            over2 += usize::from(d > 2);
            over8 += usize::from(d > 8);
        }
        let pct = |n: usize| 100.0 * n as f64 / count as f64;
        println!(
            "{name}: mean|diff|={:.3} max={} >2: {:.2}% >8: {:.2}%",
            sum / count as f64,
            max,
            pct(over2),
            pct(over8),
        );
    }
    Ok(())
}
