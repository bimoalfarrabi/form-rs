//! Compares the CPU `toProcessedHeatmap` port against the browser baseline.

use std::path::PathBuf;

use form_render::preprocess::heat;

fn main() -> anyhow::Result<()> {
    let src_path = form_render::fixtures::map_path("proc-source.png");
    let ref_path = form_render::fixtures::map_path("proc-heat.png");

    let src = image::open(&src_path)?.to_rgba8();
    let (sw, sh) = src.dimensions();
    println!("source: {src_path:?} ({sw}x{sh})");

    let out = heat::process(src.as_raw(), sw, sh)?;
    println!("output: {}x{} (expected 1750x1750)", out.width, out.height);

    let out_img = image::RgbaImage::from_raw(out.width, out.height, out.rgba.clone())
        .ok_or_else(|| anyhow::anyhow!("bad output buffer"))?;
    let out_dir = std::env::var("CTX_TMP").unwrap_or_else(|_| "/tmp/opencode".into());
    std::fs::create_dir_all(&out_dir)?;
    let out_path = PathBuf::from(out_dir).join("rust-proc-heat.png");
    out_img.save(&out_path)?;
    println!("wrote {out_path:?}");

    let expected = image::open(&ref_path)?.to_rgba8();
    let (ew, eh) = expected.dimensions();
    anyhow::ensure!(
        (ew, eh) == (out.width, out.height),
        "size mismatch: reference {ew}x{eh} vs output {}x{}",
        out.width,
        out.height
    );

    for (chan, name) in [(0usize, "R"), (1, "G"), (2, "B"), (3, "A")] {
        let mut sum_abs = 0f64;
        let mut max = 0u32;
        let mut gt2 = 0u64;
        let mut gt8 = 0u64;
        let total = (out.width as usize * out.height as usize) as f64;
        let out_px = out.rgba.as_chunks::<4>().0;
        let ref_px = expected.as_raw().as_chunks::<4>().0;
        for (p, e) in out_px.iter().zip(ref_px.iter()) {
            let d = (p[chan] as i32 - e[chan] as i32).unsigned_abs();
            sum_abs += d as f64;
            max = max.max(d);
            if d > 2 {
                gt2 += 1;
            }
            if d > 8 {
                gt8 += 1;
            }
        }
        println!(
            "{name}: mean_abs={:.4} max={max} pct_gt2={:.4}% pct_gt8={:.4}%",
            sum_abs / total,
            100.0 * gt2 as f64 / total,
            100.0 * gt8 as f64 / total,
        );
    }
    Ok(())
}
