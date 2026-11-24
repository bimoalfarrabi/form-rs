//! Headless golden render.
//!
//! ```text
//! cargo run -p form-render --bin render_ref -- <metal|heat|glass> [preset] [out.png]
//! ```
//!
//! Renders at u_time=0 with the browser's defaults so the output can be diffed
//! against `reference/baseline/ref-<material>-f0.png`.

use std::path::PathBuf;

use form_render::color::Rgba;
use form_render::params::{self, FrameRequest, Material, Params, Quality, Ratio};
use form_render::{headless_device, Renderer};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let material = match args.first().map(String::as_str).unwrap_or("metal") {
        "heat" => Material::Heat,
        "glass" => Material::Glass,
        _ => Material::Metal,
    };
    let preset: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);

    let map_name = if material == Material::Heat {
        "proc-heat.png"
    } else {
        "proc-metal.png"
    };
    let out_path = args
        .get(2)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("/tmp/opencode/rust-{}.png", material.slug())));

    let map_img = image::open(form_render::fixtures::fixture_dir().join(map_name))?.to_rgba8();
    let (map_w, map_h) = map_img.dimensions();

    let (device, queue) = pollster::block_on(headless_device())?;
    let mut renderer = Renderer::new(&device);
    let map = renderer.upload_map(&device, &queue, map_w, map_h, map_img.as_raw())?;

    let mut params = Params::for_material(material);
    if material == Material::Metal {
        params::apply_preset_params(&mut params, material, preset);
    }

    let (width, height) = Ratio::Square.buffer(Quality::K2);
    let request = FrameRequest {
        material,
        params,
        colors: params::preset_colors(material, preset),
        background: Rgba::from_hex("#141414").unwrap(),
        transparent: false,
        time: 0.0,
        palette_index: preset,
        loop_settings: None,
        image_aspect: map.aspect(),
        width,
        height,
    };

    let frame = renderer.render(&device, &queue, &map, &request);
    let rgba = renderer.read_frame(&device, &queue, &frame);
    form_render::write_png(&out_path, &rgba, width, height)?;
    println!(
        "{} preset {preset}: wrote {} ({}x{})",
        material.label(),
        out_path.display(),
        width,
        height
    );
    Ok(())
}
