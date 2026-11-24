// Frosted glass, hand-ported from frosted-glass.js plus the app's loop patch.
//
// Reference: reference/shaders/glass.frag.glsl

struct VsOut {
  @builtin(position) pos: vec4f,
};

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
  time: f32,
  scale: f32,
  image_aspect: f32,
  softness: f32,
  repetition: f32,
  shift_red: f32,
  shift_blue: f32,
  distortion: f32,
  contour: f32,
  angle: f32,
  shape: f32,
  loop_phase: f32,
  loop_cycles: f32,
  loop_start: f32,
  loop_on: f32,
  is_image: f32,
  custom_palette: f32,
  frost: f32,
  backlight: f32,
  thickness: f32,
  haze: f32,
  light_size: f32,
  inner_glow: f32,
  outer_glow: f32,
  noise: f32,
  colors_count: f32,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var img_tex: texture_2d<f32>;
@group(0) @binding(2) var img_smp: sampler;

fn hash21(p: vec2f) -> f32 {
  return fract(sin(dot(p, vec2f(127.1, 311.7))) * 43758.5453);
}

fn soft_ring(p: vec2f, radius: f32, width: f32) -> f32 {
  let d = (length(p) - radius) / width;
  return exp(-(d * d));
}

/// `u_loop ? phase*harmonic*cycles + offset : plain` — the loop patch's shape.
fn loop_or(harmonic: f32, offset: f32, plain: f32) -> f32 {
  if (u.loop_on == 1.0) {
    return u.loop_phase * harmonic * u.loop_cycles + offset;
  }
  return plain;
}

fn behind(p: vec2f, rough: f32) -> vec3f {
  let t = u.time * 0.23;
  let a = vec2f(
    0.14 * sin(loop_or(1.0, 0.00, t * 0.72)),
    0.24 * cos(loop_or(1.0, 0.00, t * 0.65))
  );
  let b = vec2f(
    -0.2 * cos(loop_or(1.0, 1.10, t * 0.61)),
    -0.25 * sin(loop_or(1.0, 1.10, t * 0.76))
  );
  let size = u.light_size;
  let width = (0.012 + rough * 0.075) * size;
  let ring = soft_ring(
    p - a,
    (0.105 + 0.015 * sin(loop_or(2.0, 0.40, t * 0.5))) * size,
    width
  );
  let rear = soft_ring((p - b) * vec2f(0.9, 1.1), 0.17 * size, width * 1.5);
  let disk = exp(-dot(p - b, p - b) / ((0.005 + rough * 0.022) * size * size));
  let scatter = exp(-dot(p - a, p - a) / ((0.025 + rough * 0.05) * size * size));
  return u.light.rgb * (ring * 0.56 + rear * 0.23 + disk * 0.34 + scatter * 0.1) * u.backlight;
}

@fragment
fn fs_main(@builtin(position) pos: vec4f) -> @location(0) vec4f {
  let res = u.resolution;
  // gl_FragCoord is bottom-left; WGSL position is top-left. `fc` keeps the
  // original's coordinate space for the grain hash and screen fades.
  let fc = vec2f(pos.x, res.y - pos.y);

  // Fragment coordinates avoid half-precision UV stepping at 2K/4K.
  let box_width = min(res.x / u.image_aspect, res.y) * u.image_aspect;
  let box_size = vec2f(box_width, box_width / u.image_aspect);
  var uv = (fc - 0.5 * res) / box_size / u.scale + vec2f(0.5);
  uv.y = 1.0 - uv.y;

  let footprint = fwidth(uv) * 0.38;
  let field = textureSample(img_tex, img_smp, uv);
  let mask = (
    field.g * 4.0
    + textureSample(img_tex, img_smp, uv + footprint).g
    + textureSample(img_tex, img_smp, uv - footprint).g
    + textureSample(img_tex, img_smp, uv + vec2f(footprint.x, -footprint.y)).g
    + textureSample(img_tex, img_smp, uv + vec2f(-footprint.x, footprint.y)).g
  ) / 8.0;

  let texel = 1.0 / vec2f(textureDimensions(img_tex, 0));
  // The Poisson field supplies a continuous curved face and rounded glass lip.
  let delta = max(texel * 16.0, vec2f(0.006));
  let slope = vec2f(
    textureSampleLevel(img_tex, img_smp, uv + vec2f(delta.x, 0.0), 4.0).r
      - textureSampleLevel(img_tex, img_smp, uv - vec2f(delta.x, 0.0), 4.0).r,
    textureSampleLevel(img_tex, img_smp, uv + vec2f(0.0, delta.y), 4.0).r
      - textureSampleLevel(img_tex, img_smp, uv - vec2f(0.0, delta.y), 4.0).r
  ) / max(delta.x + delta.y, 0.0001);

  var bend = slope * (0.085 + 0.1 * u.thickness);
  bend = bend / (1.0 + length(bend) * 0.7);
  let normal = normalize(vec3f(-bend, 1.0));
  let ray = refract(vec3f(0.0, 0.0, -1.0), normal, 1.0 / 1.46);
  let offset = ray.xy * (0.16 + 0.23 * u.thickness) / max(0.25, abs(ray.z));
  let angle = radians(u.angle);
  let rot = mat2x2<f32>(cos(angle), -sin(angle), sin(angle), cos(angle));
  let p = rot * (uv - vec2f(0.5));

  var transmission = behind(p + rot * offset, u.frost);
  // Subtle second internal reflection gives the backlight real depth.
  transmission = transmission
    + behind(p - rot * offset * 0.58 + vec2f(0.012, -0.02), min(1.0, u.frost + 0.22)) * 0.18;

  let cos_view = clamp(normal.z, 0.0, 1.0);
  let fresnel = 0.035 + 0.965 * pow(1.0 - cos_view, 5.0);
  let rim = pow(clamp(field.r, 0.0, 1.0), 7.0);

  let key_dir = normalize(vec3f(
    -0.7 + 0.3 * sin(loop_or(1.0, 0.00, u.time * 0.13)),
    0.8,
    1.1
  ));
  let key = pow(max(0.0, dot(normal, key_dir)), 24.0);
  let strip_d = (normal.x * 0.8 + normal.y * 0.5 - 0.32) / (0.05 + 0.12 * u.frost);
  let strip = exp(-(strip_d * strip_d));
  var reflection = key * 0.18 + strip * 0.045;

  // A rounded bevel bends the backlight into localized bright/dark ribbons.
  let edge_height = clamp(textureSampleLevel(img_tex, img_smp, uv, 2.0).r, 0.0, 1.0);
  let bevel_band = smoothstep(0.24, 0.62, edge_height) * (1.0 - smoothstep(0.9, 0.995, edge_height));
  let moving_key = normalize(vec2f(
    cos(loop_or(1.0, 0.80, u.time * 0.19 + 0.8)),
    sin(loop_or(1.0, 0.80, u.time * 0.19 + 0.8))
  ));
  let bevel_facing = pow(max(0.0, dot(normalize(slope + vec2f(0.00001)), moving_key)), 5.0);
  let bevel_width = 0.105 + 0.055 * u.frost;
  let crest_d = (edge_height - (0.73 + 0.035 * sin(loop_or(1.0, 0.00, u.time * 0.17)))) / bevel_width;
  let crest = exp(-(crest_d * crest_d));
  let groove_d = (edge_height - 0.51) / (0.05 + 0.04 * u.frost);
  let groove = exp(-(groove_d * groove_d));

  let bevel_transmission = behind(p + rot * offset * 1.8, u.frost * 0.65);
  let bevel_strength = 0.25 + 0.75 * u.thickness;
  transmission = transmission * (1.0 - groove * bevel_facing * 0.32 * bevel_strength);
  transmission = transmission + bevel_transmission * bevel_band * bevel_facing * 0.45 * bevel_strength;
  reflection = reflection
    + crest * bevel_facing * (0.035 + 0.095 * u.thickness) * (1.0 - smoothstep(0.9, 0.995, edge_height));

  // Thickness-dependent selective absorption plus frosted surface scattering.
  let glass_value = max(u.glass.r, max(u.glass.g, u.glass.b));
  let glass_hue = u.glass.rgb / max(glass_value, 0.08);
  let optical_path = 0.35 + u.thickness * 1.45 + rim * 0.65;
  var absorption = exp(-(vec3f(1.0) - glass_hue) * optical_path * 1.5);
  absorption = absorption * mix(0.35, 1.0, smoothstep(0.0, 0.18, glass_value));
  let incident = dot(transmission, vec3f(0.2126, 0.7152, 0.0722));
  var transmitted = transmission
    * mix(vec3f(1.0), absorption, 0.28 + 0.16 * u.thickness)
    * (1.0 - fresnel);
  // Frost scatters a little incident light into the glass color: opposing hues remain readable.
  let scattering = 0.5 + 0.3 * u.frost;
  let scatter_zone = 1.0 - smoothstep(0.07, 0.38, incident);
  transmitted = mix(transmitted, glass_hue * incident * 0.95, scatter_zone * scattering);

  let body_color = u.glass.rgb * (0.016 + 0.04 * u.frost + 0.028 * u.thickness) * (1.0 + incident * 0.6);
  let edge_color = mix(u.rim.rgb, glass_hue, 0.18 + 0.2 * fresnel);
  // One broad, faint bevel reflection; no pixel-wide contour stroke.
  let reflected = edge_color * reflection * (0.65 + 0.35 * fresnel);

  let grain = (hash21(fc) - 0.5) * 0.009 * u.frost * (1.0 - smoothstep(0.65, 0.95, edge_height));
  let screen_uv = fc / res;
  let frame_fade = smoothstep(0.0, 0.12, screen_uv.x)
    * smoothstep(0.0, 0.12, 1.0 - screen_uv.x)
    * smoothstep(0.0, 0.12, screen_uv.y)
    * smoothstep(0.0, 0.12, 1.0 - screen_uv.y);
  let atmosphere = behind(p, 1.0) * u.haze * 0.38 * frame_fade;
  let atmosphere_tint = mix(vec3f(1.0), mix(absorption, glass_hue, 0.18), mask * 0.6);

  var surface = (transmitted + body_color + reflected + vec3f(grain)) * mask
    + atmosphere * atmosphere_tint * (1.0 - mask * 0.55);
  surface = clamp(surface, vec3f(0.0), vec3f(1.0));

  let density = mask * (0.045 + 0.065 * u.frost);
  let energy = max(surface.r, max(surface.g, surface.b));
  let alpha = clamp(density + energy * 0.8, 0.0, 0.92);

  let result = surface + u.color_back.rgb * u.color_back.a * (1.0 - alpha);
  let out_alpha = alpha + u.color_back.a * (1.0 - alpha);
  return vec4f(result, out_alpha);
}
