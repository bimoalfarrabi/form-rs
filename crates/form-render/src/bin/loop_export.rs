//! Headless loop export: `loop_export <material> <preset> <out.webm> [frames]`.

use form_render::color::Rgba;
use form_render::params::{self, Material, Params};
use form_render::video::{self, LoopJob};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let material = match args.first().map(String::as_str).unwrap_or("heat") {
        "metal" => Material::Metal,
        "glass" => Material::Glass,
        _ => Material::Heat,
    };
    let preset: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let out = args
        .get(2)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp/opencode/loop.webm"));
    let frames: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(180);
    let transparent = args.get(4).map(|s| s == "alpha").unwrap_or(false);

    let map_name = if material == Material::Heat {
        "proc-heat.png"
    } else {
        "proc-metal.png"
    };
    let map = image::open(form_render::fixtures::map_path(map_name))?.to_rgba8();
    let (w, h) = map.dimensions();

    let job = LoopJob {
        map_rgba: map.into_raw(),
        map_width: w,
        map_height: h,
        material,
        params: Params::for_material(material),
        colors: params::preset_colors(material, preset),
        background: Rgba::from_hex("#141414").unwrap(),
        transparent,
        time: 0.0,
        palette_index: preset,
        width: 512,
        height: 512,
        fps: 30,
        frames,
        path: out.clone(),
    };

    let mut last = 101;
    let started = std::time::Instant::now();
    video::encode_loop(&job, &mut |p| {
        if p / 10 != last / 10 {
            last = p;
            eprintln!("  {p}%");
        }
    })?;
    let meta = std::fs::metadata(&out)?;
    println!(
        "wrote {} ({:.1} MB, {frames} frames in {:.1}s)",
        out.display(),
        meta.len() as f64 / 1_048_576.0,
        started.elapsed().as_secs_f32()
    );
    Ok(())
}
