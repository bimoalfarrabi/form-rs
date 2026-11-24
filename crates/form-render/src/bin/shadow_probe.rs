//! Probe: renders the WGSL `shadow_shape` over the unit square for a range of
//! `t` values, so it can be compared with the browser's GLSL function.

use form_render::color::Rgba;
use form_render::params::{FrameRequest, Material, Params, Quality, Ratio};
use form_render::{headless_device, Renderer};

const PROBE: &str = r#"
struct VsOut { @builtin(position) pos: vec4f };

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VsOut {
  var corners = array<vec2f, 3>(vec2f(-1.0, -1.0), vec2f(3.0, -1.0), vec2f(-1.0, 3.0));
  return VsOut(vec4f(corners[i], 0.0, 1.0));
}

struct Uniforms {
  color_back: vec4f, color_tint: vec4f, glass: vec4f, light: vec4f, rim: vec4f,
  palette: array<vec4f, 5>, colors: array<vec4f, 6>,
  resolution: vec2f, time: f32, scale: f32, image_aspect: f32, softness: f32,
  repetition: f32, shift_red: f32, shift_blue: f32, distortion: f32, contour: f32,
  angle: f32, shape: f32, loop_phase: f32, loop_cycles: f32, loop_start: f32,
  loop_on: f32, is_image: f32, custom_palette: f32, frost: f32, backlight: f32,
  thickness: f32, haze: f32, light_size: f32, inner_glow: f32, outer_glow: f32,
  noise: f32, colors_count: f32,
};
@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var img_tex: texture_2d<f32>;
@group(0) @binding(2) var img_smp: sampler;

const TWO_PI: f32 = 6.28318530718;
const PI: f32 = 3.14159265358979323846;

fn sst(e0: f32, e1: f32, x: f32) -> f32 {
  // GLSL leaves reversed edges undefined; ANGLE returns 0 for edge0 > edge1,
  // and the original heat shader relies on that in several places.
  if (e1 <= e0) {
    return 0.0;
  }
  let t = clamp((x - e0) / (e1 - e0), 0.0, 1.0);
  return t * t * (3.0 - 2.0 * t);
}
fn lst(e0: f32, e1: f32, x: f32) -> f32 {
  return clamp((x - e0) / (e1 - e0), 0.0, 1.0);
}
fn circle(uv: vec2f, c: vec2f, r: vec2f) -> f32 {
  return 1.0 - sst(r.x, r.y, length(uv - c));
}

fn shadow_shape(uv_in: vec2f, t_in: f32, contour: f32) -> f32 {
  var uv = uv_in;
  var t = t_in;
  var scaled_uv = uv;
  let pos_y = mix(-1.0, 2.0, t);
  scaled_uv.y = scaled_uv.y - 0.5;
  let main_circle_scale = sst(0.0, 0.8, pos_y) * lst(1.4, 0.9, pos_y);
  scaled_uv = scaled_uv * vec2f(1.0, 1.0 + 1.5 * main_circle_scale);
  scaled_uv.y = scaled_uv.y + 0.5;
  let inner_r = 0.4;
  let outer_r = 1.0 - 0.3 * (sst(0.1, 0.2, t) * (1.0 - sst(0.2, 0.5, t)));
  var s = circle(scaled_uv, vec2f(0.5, pos_y - 0.2), vec2f(inner_r, outer_r));
  s = pow(s, 1.4);
  s = s * 1.2;
  {
    let pos = pos_y - uv.y;
    let edge = 1.2;
    var top_flattener = lst(-0.4, 0.0, pos) * (1.0 - sst(0.0, edge, pos));
    top_flattener = pow(top_flattener, 3.0);
    let mixer = 1.0 - sst(0.0, 0.3, pos);
    s = mix(top_flattener, s, mixer);
  }
  {
    let visibility = sst(0.6, 0.7, t) * (1.0 - sst(0.8, 0.9, t));
    let angle = -2.0 - t * TWO_PI;
    var right_circle = circle(uv, vec2f(0.95 - 0.2 * cos(angle), 0.4 - 0.1 * sin(angle)), vec2f(0.15, 0.3));
    right_circle = right_circle * visibility;
    s = mix(s, 0.0, right_circle);
  }
  {
    var top_circle = circle(uv, vec2f(0.5, 0.19), vec2f(0.05, 0.25));
    top_circle = top_circle + 2.0 * contour * circle(uv, vec2f(0.5, 0.19), vec2f(0.2, 0.5));
    let visibility = 0.55 * sst(0.2, 0.3, t) * (1.0 - sst(0.3, 0.45, t));
    top_circle = top_circle * visibility;
    s = mix(s, 0.0, top_circle);
  }
  var leaf_mask = circle(uv, vec2f(0.53, 0.13), vec2f(0.08, 0.19));
  leaf_mask = mix(leaf_mask, 0.0, 1.0 - sst(0.4, 0.54, uv.x));
  leaf_mask = mix(0.0, leaf_mask, sst(0.0, 0.2, uv.y));
  leaf_mask = leaf_mask * (sst(0.5, 1.1, pos_y) * sst(1.5, 1.3, pos_y));
  s = s + leaf_mask;
  {
    let visibility = sst(0.0, 0.4, t) * (1.0 - sst(0.6, 0.8, t));
    s = mix(s, 0.0, visibility * circle(uv, vec2f(0.52, 0.92), vec2f(0.09, 0.25)));
  }
  {
    let pos = sst(0.0, 0.6, t) * (1.0 - sst(0.6, 1.0, t));
    s = mix(s, 0.5, circle(uv, vec2f(0.0, 1.2 - 0.5 * pos), vec2f(0.1, 0.3)));
    s = mix(s, 0.0, circle(uv, vec2f(1.0, 0.5 + 0.5 * pos), vec2f(0.1, 0.3)));
    s = mix(s, 1.0, circle(uv, vec2f(0.95, 0.2 + 0.2 * sst(0.3, 0.4, t) * sst(0.7, 0.5, t)), vec2f(0.07, 0.22)));
    s = mix(s, 1.0, circle(uv, vec2f(0.95, 0.2 + 0.2 * sst(0.3, 0.4, t) * (1.0 - sst(0.5, 0.7, t))), vec2f(0.07, 0.22)));
    s = s / max(1e-4, sst(1.0, 0.85, uv.y));
  }
  s = clamp(s, 0.0, 1.0);
  return s;
}

fn object_uv(pos: vec2f) -> vec2f {
  let res = u.resolution;
  let min_dim = min(res.x, res.y);
  let box = vec2f(min_dim, min_dim);
  return 0.5 + (pos - 0.5 * res) / box / u.scale;
}

@fragment
fn fs_main(@builtin(position) pos: vec4f) -> @location(0) vec4f {
  let uv = object_uv(pos.xy);
  let s = shadow_shape(uv, u.time, 0.0);
  return vec4f(vec3f(s), 1.0);
}
"#;

fn main() -> anyhow::Result<()> {
    let map_img = image::open(form_render::fixtures::map_path("proc-heat.png"))?.to_rgba8();
    let (w, h) = map_img.dimensions();
    let (device, queue) = pollster::block_on(headless_device())?;
    let mut renderer = Renderer::new(&device);
    let map = renderer.upload_map(&device, &queue, w, h, map_img.as_raw())?;

    let (width, height) = Ratio::Square.buffer(Quality::K2);
    for i in 0..20 {
        let t = i as f32 / 20.0;
        let mut params = Params::for_material(Material::Heat);
        params.scale = 1.0;
        let request = FrameRequest {
            material: Material::Heat,
            params,
            colors: vec![Rgba::WHITE],
            background: Rgba::BLACK,
            transparent: false,
            time: t,
            palette_index: 0,
            loop_settings: None,
            image_aspect: map.aspect(),
            width,
            height,
        };
        let frame = renderer.render_with_shader(&device, &queue, &map, &request, PROBE)?;
        let rgba = renderer.read_frame(&device, &queue, &frame);
        let out = format!("/tmp/opencode/caps/rust-probe-shadow-t{i}.png");
        form_render::write_png(std::path::Path::new(&out), &rgba, width, height)?;
    }
    println!("wrote 20 probe frames");
    Ok(())
}
