//! Heatmap bisection taps: renders the material with small source patches so a
//! divergent stage is visible at a glance.

use form_render::color::Rgba;
use form_render::params::{self, FrameRequest, Material, Params, Quality, Ratio};
use form_render::{headless_device, Renderer};

const BASE: &str = include_str!("../wgsl/heat.wgsl");

fn main() -> anyhow::Result<()> {
    let map_img = image::open(form_render::fixtures::map_path("proc-heat.png"))?.to_rgba8();
    let (w, h) = map_img.dimensions();
    let (device, queue) = pollster::block_on(headless_device())?;
    let mut renderer = Renderer::new(&device);
    let map = renderer.upload_map(&device, &queue, w, h, map_img.as_raw())?;

    let (width, height) = Ratio::Square.buffer(Quality::K2);
    let request = FrameRequest {
        material: Material::Heat,
        params: Params::for_material(Material::Heat),
        colors: params::preset_colors(Material::Heat, 0),
        background: Rgba::from_hex("#141414").unwrap(),
        transparent: false,
        time: 0.0,
        palette_index: 0,
        loop_settings: None,
        image_aspect: map.aspect(),
        width,
        height,
    };

    let taps: Vec<(&str, String)> = vec![
        (
            "nosmt",
            BASE.replace("inner = mix(inner, 0.0, shadow);", "inner = inner;")
                .replace("inner = mix(inner, 0.0, shadow_copy);", "inner = inner;")
                .replace("inner = mix(inner, 0.0, shadow_copy2);", "inner = inner;"),
        ),
        (
            "shape",
            BASE.replace(
                "var color = gradient.rgb * outer_shape;",
                "var color = vec3f(shape, shape, shape);",
            ),
        ),
        (
            "inner",
            BASE.replace(
                "var color = gradient.rgb * outer_shape;",
                "var color = vec3f(heat, heat, heat);",
            ),
        ),
        (
            "gchan",
            BASE.replace(
                "var color = gradient.rgb * outer_shape;",
                "var color = vec3f(img.g, img.g, img.g);",
            ),
        ),
        (
            "shadows",
            BASE.replace(
                "var color = gradient.rgb * outer_shape;",
                "var color = vec3f(shadow, shadow_copy, shadow_copy2);",
            ),
        ),
    ];

    for (name, src) in taps {
        let frame = renderer.render_with_shader(&device, &queue, &map, &request, &src)?;
        let rgba = renderer.read_frame(&device, &queue, &frame);
        let out = format!("/tmp/opencode/tap-heat-{name}.png");
        form_render::write_png(std::path::Path::new(&out), &rgba, width, height)?;
        println!("{name} -> {out}");
    }
    Ok(())
}
