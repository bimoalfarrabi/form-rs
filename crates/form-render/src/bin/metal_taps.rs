//! Metal bisection taps: renders intermediate stages of the metal shader.

use form_render::color::Rgba;
use form_render::params::{self, FrameRequest, Material, Params, Quality, Ratio};
use form_render::{headless_device, Renderer};

const BASE: &str = include_str!("../wgsl/metal.wgsl");

fn main() -> anyhow::Result<()> {
    let map_img = image::open(form_render::fixtures::map_path("proc-metal.png"))?.to_rgba8();
    let (w, h) = map_img.dimensions();
    let (device, queue) = pollster::block_on(headless_device())?;
    let mut renderer = Renderer::new(&device);
    let map = renderer.upload_map(&device, &queue, w, h, map_img.as_raw())?;

    let (width, height) = Ratio::Square.buffer(Quality::K2);
    let request = FrameRequest {
        material: Material::Metal,
        params: Params::for_material(Material::Metal),
        colors: params::preset_colors(Material::Metal, 0),
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
        ("edge", BASE.replace("  var opacity = img.g;", "  return vec4f(vec3f(edge), 1.0);\n  var opacity = img.g;")),
        ("bump", BASE.replace("  var noise_uv = uv - vec2f(t);", "  return vec4f(vec3f(bump), 1.0);\n  var noise_uv = uv - vec2f(t);")),
        ("direction", BASE.replace("  var color_dispersion = 1.0 - bump;", "  return vec4f(vec3f(fract(direction)), 1.0);\n  var color_dispersion = 1.0 - bump;")),
        ("fwidth", BASE.replace("  var color_dispersion = 1.0 - bump;", "  return vec4f(vec3f(fwidth(fract(direction)) * 10.0), 1.0);\n  var color_dispersion = 1.0 - bump;")),
        ("blur", BASE.replace("  var w = vec3f(", "  return vec4f(vec3f(blur * 20.0), 1.0);\n  var w = vec3f(")),
        ("noise", BASE.replace("  edge = edge + (1.0 - edge) * u.distortion * noise;", "  return vec4f(vec3f(noise * 0.5 + 0.5), 1.0);")),
        ("opacity", BASE.replace("  var color = vec3f(0.0);", "  return vec4f(vec3f(opacity), 1.0);\n  var color = vec3f(0.0);")),
        ("uv", BASE.replace("  let cycle_width = u.repetition;", "  return vec4f(uv, 0.0, 1.0);\n  let cycle_width = u.repetition;")),
    ];

    for (name, src) in taps {
        let frame = renderer.render_with_shader(&device, &queue, &map, &request, &src)?;
        let rgba = renderer.read_frame(&device, &queue, &frame);
        let out = format!("/tmp/opencode/caps/rust-metal-{name}.png");
        form_render::write_png(std::path::Path::new(&out), &rgba, width, height)?;
        println!("{name} -> {out}");
    }
    Ok(())
}
