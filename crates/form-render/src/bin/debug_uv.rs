//! Debug helper: dumps the map sampled through the same UV formula the metal
//! shader uses, so a mismatch against the browser baseline is obvious.

use std::path::PathBuf;

use form_render::color::Rgba;
use form_render::params::{FrameRequest, Material, Params, Quality, Ratio};
use form_render::{headless_device, Renderer};

const DEBUG_WGSL: &str = r#"
struct VsOut { @builtin(position) pos: vec4f };

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VsOut {
  var corners = array<vec2f, 3>(vec2f(-1.0, -1.0), vec2f(3.0, -1.0), vec2f(-1.0, 3.0));
  return VsOut(vec4f(corners[i], 0.0, 1.0));
}

struct Uniforms {
  color_back: vec4f,
  color_tint: vec4f,
  glass: vec4f,
  light: vec4f,
  rim: vec4f,
  palette: array<vec4f, 5>,
  colors: array<vec4f, 6>,
  resolution: vec2f,
  time: f32, scale: f32, image_aspect: f32, softness: f32, repetition: f32,
  shift_red: f32, shift_blue: f32, distortion: f32, contour: f32, angle: f32,
  shape: f32, loop_phase: f32, loop_cycles: f32, loop_start: f32, loop_on: f32,
  is_image: f32, custom_palette: f32, frost: f32, backlight: f32, thickness: f32,
  haze: f32, light_size: f32, inner_glow: f32, outer_glow: f32, noise: f32,
  colors_count: f32,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var img_tex: texture_2d<f32>;
@group(0) @binding(2) var img_smp: sampler;

@fragment
fn fs_main(@builtin(position) pos: vec4f) -> @location(0) vec4f {
  let res = u.resolution;
  let aspect = u.image_aspect;
  let box_w = min(res.x / aspect, res.y) * aspect;
  let box = vec2f(box_w, box_w / aspect);
  let uv = 0.5 + (pos.xy - 0.5 * res) / box / u.scale;
  let c = textureSampleLevel(img_tex, img_smp, uv, 0.0);
  // Top-left quadrant: raw position gradient. Rest: map mask/field at mip 0.
  let quad = pos.x < 0.5 * res.x && pos.y < 0.5 * res.y;
  if (quad) {
    return vec4f(pos.x / res.x, pos.y / res.y, u.scale / 2.0, 1.0);
  }
  let out_of_bounds = uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0;
  let edge = select(0.0, 1.0, out_of_bounds);
  return vec4f(c.g, c.r, edge, 1.0);
}
"#;

fn main() -> anyhow::Result<()> {
    let map_path = form_render::fixtures::map_path("proc-metal.png");
    let out_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/opencode/rust-debug-uv.png"));

    let map_img = image::open(&map_path)?.to_rgba8();
    let (map_w, map_h) = map_img.dimensions();
    let (device, queue) = pollster::block_on(headless_device())?;
    let mut renderer = Renderer::new(&device);
    let map = renderer.upload_map(&device, &queue, map_w, map_h, map_img.as_raw())?;

    let (width, height) = Ratio::Square.buffer(Quality::K2);
    let request = FrameRequest {
        material: Material::Metal,
        params: Params::for_material(Material::Metal),
        colors: vec![Rgba::WHITE],
        background: Rgba::BLACK,
        transparent: false,
        time: 0.0,
        palette_index: 0,
        loop_settings: None,
        image_aspect: map.aspect(),
        width,
        height,
    };

    let frame = renderer.render_with_shader(&device, &queue, &map, &request, DEBUG_WGSL)?;
    let rgba = renderer.read_frame(&device, &queue, &frame);
    form_render::write_png(&out_path, &rgba, width, height)?;
    println!("wrote {}", out_path.display());
    Ok(())
}
