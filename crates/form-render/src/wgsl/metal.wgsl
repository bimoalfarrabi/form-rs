struct VsOut {
  @builtin(position) pos: vec4f,
};

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VsOut {
  var corners = array<vec2f, 3>(vec2f(-1.0, -1.0), vec2f(3.0, -1.0), vec2f(-1.0, 3.0));
  return VsOut(vec4f(corners[i], 0.0, 1.0));
}

// Liquid metal, hand-ported from @paper-design/shaders 0.0.82 liquidMetalFragmentShader
// plus the palette patch and loop patch applied by the app.
//
// Reference: reference/shaders/metal.frag.glsl (the exact GLSL the browser runs).

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

const TWO_PI: f32 = 6.28318530718;
const PI: f32 = 3.14159265358979323846;

fn mod2(x: vec2f, y: f32) -> vec2f {
  return x - y * floor(x / y);
}

fn mod3(x: vec3f, y: f32) -> vec3f {
  return x - y * floor(x / y);
}

fn permute3(x: vec3f) -> vec3f {
  return mod3(((x * 34.0) + 1.0) * x, 289.0);
}

fn snoise(v: vec2f) -> f32 {
  let C = vec4f(0.211324865405187, 0.366025403784439, -0.577350269189626, 0.024390243902439);
  var i = floor(v + dot(v, C.yy));
  let x0 = v - i + dot(i, C.xx);
  let i1 = select(vec2f(0.0, 1.0), vec2f(1.0, 0.0), x0.x > x0.y);
  var x12 = vec4f(x0.x, x0.y, x0.x, x0.y) + vec4f(C.x, C.x, C.z, C.z);
  x12 = vec4f(x12.x - i1.x, x12.y - i1.y, x12.z, x12.w);
  i = mod2(i, 289.0);
  let p = permute3(
    permute3(i.y + vec3f(0.0, i1.y, 1.0)) + i.x + vec3f(0.0, i1.x, 1.0)
  );
  var m = max(
    vec3f(0.5 - dot(x0, x0), 0.5 - dot(x12.xy, x12.xy), 0.5 - dot(x12.zw, x12.zw)),
    vec3f(0.0)
  );
  m = m * m;
  m = m * m;
  let x = 2.0 * fract(p * C.www) - vec3f(1.0);
  let h = abs(x) - vec3f(0.5);
  let ox = floor(x + vec3f(0.5));
  let a0 = x - ox;
  m = m * (vec3f(1.79284291400159) - vec3f(0.85373472095314) * (a0 * a0 + h * h));
  var g: vec3f;
  g.x = a0.x * x0.x + h.x * x0.y;
  g.y = a0.y * x12.x + h.y * x12.z;
  g.z = a0.z * x12.y + h.z * x12.w;
  return 130.0 * dot(m, g);
}

fn get_color_changes(
  c1: f32,
  c2: f32,
  stripe_p: f32,
  w: vec3f,
  blur: f32,
  bump_in: f32,
  tint: f32,
) -> f32 {
  var ch = mix(c2, c1, smoothstep(0.0, 2.0 * blur, stripe_p));
  var bump = bump_in;

  var border = w.x;
  ch = mix(ch, c2, smoothstep(border, border + 2.0 * blur, stripe_p));

  if (u.is_image == 1.0) {
    bump = smoothstep(0.2, 0.8, bump);
  }
  border = w.x + 0.4 * (1.0 - bump) * w.y;
  ch = mix(ch, c1, smoothstep(border, border + 2.0 * blur, stripe_p));

  border = w.x + 0.5 * (1.0 - bump) * w.y;
  ch = mix(ch, c2, smoothstep(border, border + 2.0 * blur, stripe_p));

  border = w.x + w.y;
  ch = mix(ch, c1, smoothstep(border, border + 2.0 * blur, stripe_p));

  let gradient_t = (stripe_p - w.x - w.y) / w.z;
  let gradient = mix(c1, c2, smoothstep(0.0, 1.0, gradient_t));
  ch = mix(ch, gradient, smoothstep(border, border + 0.5 * blur, stripe_p));

  // Tint color is applied with color burn blending.
  ch = mix(ch, 1.0 - min(1.0, (1.0 - ch) / max(tint, 0.0001)), u.color_tint.a);
  return ch;
}

// th is always 0 in this app, where the expression degenerates to a hard [0,1] box.
fn get_img_frame(uv: vec2f, th: f32) -> f32 {
  if (th <= 0.0) {
    let inside = uv.x >= 0.0 && uv.x <= 1.0 && uv.y >= 0.0 && uv.y <= 1.0;
    return select(0.0, 1.0, inside);
  }
  var frame = 1.0;
  frame = frame * smoothstep(0.0, th, uv.y);
  frame = frame * (1.0 - smoothstep(1.0 - th, 1.0, uv.y));
  frame = frame * smoothstep(0.0, th, uv.x);
  frame = frame * (1.0 - smoothstep(1.0 - th, 1.0, uv.x));
  return frame;
}

fn blur_edge_3x3(uv: vec2f, dudx: vec2f, dudy: vec2f, radius: f32, center_sample: f32) -> f32 {
  let dims = vec2f(textureDimensions(img_tex, 0));
  let texel = 1.0 / dims;
  let r = radius * texel;

  let w1 = 1.0;
  let w2 = 2.0;
  let w4 = 4.0;
  let norm = 16.0;
  var sum = w4 * center_sample;

  sum += w2 * textureSampleGrad(img_tex, img_smp, uv + vec2f(0.0, -r.y), dudx, dudy).r;
  sum += w2 * textureSampleGrad(img_tex, img_smp, uv + vec2f(0.0, r.y), dudx, dudy).r;
  sum += w2 * textureSampleGrad(img_tex, img_smp, uv + vec2f(-r.x, 0.0), dudx, dudy).r;
  sum += w2 * textureSampleGrad(img_tex, img_smp, uv + vec2f(r.x, 0.0), dudx, dudy).r;

  sum += w1 * textureSampleGrad(img_tex, img_smp, uv + vec2f(-r.x, -r.y), dudx, dudy).r;
  sum += w1 * textureSampleGrad(img_tex, img_smp, uv + vec2f(r.x, -r.y), dudx, dudy).r;
  sum += w1 * textureSampleGrad(img_tex, img_smp, uv + vec2f(-r.x, r.y), dudx, dudy).r;
  sum += w1 * textureSampleGrad(img_tex, img_smp, uv + vec2f(r.x, r.y), dudx, dudy).r;

  return sum / norm;
}

fn rotate(uv: vec2f, th: f32) -> vec2f {
  let m = mat2x2<f32>(cos(th), sin(th), -sin(th), cos(th));
  return m * uv;
}

fn centered_uv(pos: vec2f, aspect: f32) -> vec2f {
  let res = u.resolution;
  let box_w = min(res.x / aspect, res.y) * aspect;
  let box = vec2f(box_w, box_w / aspect);
  return 0.5 + (pos - 0.5 * res) / box / u.scale;
}

@fragment
fn fs_main(@builtin(position) pos: vec4f) -> @location(0) vec4f {
  let first_frame_offset = 2.8;
  var t = 0.3 * (u.time + first_frame_offset);
  if (u.loop_on == 1.0) {
    t = 0.3 * (u.loop_start + first_frame_offset) + u.loop_phase / 6.28318530718 * u.loop_cycles;
  }

  let uv = centered_uv(pos.xy, u.image_aspect);
  let dudx = dpdx(uv);
  let dudy = dpdy(uv);
  let img = textureSampleGrad(img_tex, img_smp, uv, dudx, dudy);

  let cycle_width = u.repetition;
  var edge = 0.0;

  var rotated_uv = uv - vec2f(0.5);
  let angle = (-u.angle + 70.0) * PI / 180.0;
  let cos_a = cos(angle);
  let sin_a = sin(angle);
  rotated_uv = vec2f(
    rotated_uv.x * cos_a - rotated_uv.y * sin_a,
    rotated_uv.x * sin_a + rotated_uv.y * cos_a
  ) + vec2f(0.5);

  // u_isImage is always true for this app; the procedural-shape branches are dropped.
  let edge_raw = img.r;
  edge = blur_edge_3x3(uv, dudx, dudy, 6.0, edge_raw);
  edge = pow(edge, 1.6);
  edge = edge * mix(0.0, 1.0, smoothstep(0.0, 0.4, u.contour));

  var opacity = img.g;
  let frame = get_img_frame(uv, 0.0);
  opacity = opacity * frame;

  let diag_bl_to_tr = rotated_uv.x - rotated_uv.y;
  let diag_tl_to_br = rotated_uv.x + rotated_uv.y;

  var color = vec3f(0.0);
  let color1 = vec3f(0.98, 0.98, 1.0);
  let color2 = vec3f(0.1, 0.1, 0.1 + 0.1 * smoothstep(0.7, 1.3, diag_tl_to_br));

  var grad_uv = uv - vec2f(0.5);

  let dist = length(grad_uv + vec2f(0.0, 0.2 * diag_bl_to_tr));
  grad_uv = rotate(grad_uv, (0.25 - 0.2 * diag_bl_to_tr) * PI);
  var direction = grad_uv.x;

  var bump = pow(1.8 * dist, 1.2);
  bump = 1.0 - bump;
  bump = bump * pow(max(uv.y, 0.0), 0.3);

  let thin_strip_1_ratio = 0.12 / cycle_width * (1.0 - 0.4 * bump);
  let thin_strip_2_ratio = 0.07 / cycle_width * (1.0 + 0.4 * bump);
  let wide_strip_ratio = 1.0 - thin_strip_1_ratio - thin_strip_2_ratio;

  let thin_strip_1_width = cycle_width * thin_strip_1_ratio;
  let thin_strip_2_width = cycle_width * thin_strip_2_ratio;

  var noise_uv = uv - vec2f(t);
  if (u.loop_on == 1.0) {
    noise_uv = uv - vec2f(0.3 * (u.loop_start + 2.8))
      + 0.65 * vec2f(cos(u.loop_phase * u.loop_cycles), sin(u.loop_phase * u.loop_cycles));
  }
  let noise = snoise(noise_uv);

  edge = edge + (1.0 - edge) * u.distortion * noise;

  direction = direction + diag_bl_to_tr;
  direction = direction - 2.0 * noise * diag_bl_to_tr
    * (smoothstep(0.0, 1.0, edge) * (1.0 - smoothstep(0.0, 1.0, edge)));
  // u_contour range 2
  direction = direction * mix(1.0, 1.0 - edge, smoothstep(0.5, 1.0, u.contour));
  direction = direction - 1.7 * edge * smoothstep(0.5, 1.0, u.contour);
  direction = direction + 0.2 * pow(u.contour, 4.0) * (1.0 - smoothstep(0.0, 1.0, edge));

  bump = bump * clamp(pow(max(uv.y, 0.0), 0.1), 0.3, 1.0);
  direction = direction * (0.1 + (1.1 - edge) * bump);

  direction = direction * (0.4 + 0.6 * (1.0 - smoothstep(0.5, 1.0, edge)));
  direction = direction + 0.18 * (smoothstep(0.1, 0.2, uv.y) * (1.0 - smoothstep(0.2, 0.4, uv.y)));
  direction = direction + 0.03 * (smoothstep(0.1, 0.2, 1.0 - uv.y) * (1.0 - smoothstep(0.2, 0.4, 1.0 - uv.y)));

  direction = direction * (0.5 + 0.5 * pow(max(uv.y, 0.0), 2.0));
  direction = direction * cycle_width;
  direction = direction - t;

  var color_dispersion = 1.0 - bump;
  color_dispersion = clamp(color_dispersion, 0.0, 1.0);
  var dispersion_red = color_dispersion;
  dispersion_red = dispersion_red + 0.03 * bump * noise;
  dispersion_red = dispersion_red + 5.0
    * (smoothstep(-0.1, 0.2, uv.y) * (1.0 - smoothstep(0.1, 0.5, uv.y)))
    * (smoothstep(0.4, 0.6, bump) * (1.0 - smoothstep(0.4, 1.0, bump)));
  dispersion_red = dispersion_red - diag_bl_to_tr;

  var dispersion_blue = color_dispersion;
  dispersion_blue = dispersion_blue * 1.3;
  dispersion_blue = dispersion_blue
    + (smoothstep(0.0, 0.4, uv.y) * (1.0 - smoothstep(0.1, 0.8, uv.y)))
    * (smoothstep(0.4, 0.6, bump) * (1.0 - smoothstep(0.4, 0.8, bump)));
  dispersion_blue = dispersion_blue - 0.2 * edge;

  dispersion_red = dispersion_red * (u.shift_red / 20.0);
  dispersion_blue = dispersion_blue * (u.shift_blue / 20.0);

  var blur = 0.0;
  var r_extra_blur = 0.0;
  var g_extra_blur = 0.0;
  let softness = 0.05 * u.softness;
  blur = softness + 0.5 * smoothstep(1.0, 10.0, u.repetition) * smoothstep(0.0, 1.0, edge);
  let small_canvas_t = 1.0 - smoothstep(100.0, 500.0, min(u.resolution.x, u.resolution.y));
  blur = blur + small_canvas_t * smoothstep(0.0, 1.0, edge);
  r_extra_blur = softness * (0.05 + 0.1 * (u.shift_red / 20.0) * bump);
  g_extra_blur = softness * 0.05 / max(0.001, abs(1.0 - diag_bl_to_tr));

  var w = vec3f(thin_strip_1_width, thin_strip_2_width, wide_strip_ratio);
  w.y = w.y - 0.02 * smoothstep(0.0, 1.0, edge + bump);

  let stripe_r = fract(direction + dispersion_red);
  let r = get_color_changes(
    color1.r, color2.r, stripe_r, w, blur + fwidth(stripe_r) + r_extra_blur, bump, u.color_tint.r
  );
  let stripe_g = fract(direction);
  let g = get_color_changes(
    color1.g, color2.g, stripe_g, w, blur + fwidth(stripe_g) + g_extra_blur, bump, u.color_tint.g
  );
  let stripe_b = fract(direction - dispersion_blue);
  let b = get_color_changes(
    color1.b, color2.b, stripe_b, w, blur + fwidth(stripe_b), bump, u.color_tint.b
  );

  color = vec3f(r, g, b);
  if (u.custom_palette == 1.0) {
    // Color the reflected light, preserving the original luminance/contrast.
    let luminance = clamp(dot(color, vec3f(0.2126, 0.7152, 0.0722)), 0.0, 1.0);
    let tone = luminance * 4.0;
    var pigment: vec3f;
    if (tone < 1.0) {
      pigment = mix(u.palette[0].rgb, u.palette[1].rgb, smoothstep(0.0, 1.0, tone));
    } else if (tone < 2.0) {
      pigment = mix(u.palette[1].rgb, u.palette[2].rgb, smoothstep(0.0, 1.0, tone - 1.0));
    } else if (tone < 3.0) {
      pigment = mix(u.palette[2].rgb, u.palette[3].rgb, smoothstep(0.0, 1.0, tone - 2.0));
    } else {
      pigment = mix(u.palette[3].rgb, u.palette[4].rgb, smoothstep(0.0, 1.0, tone - 3.0));
    }
    let tinted_light = clamp(pigment * luminance * 1.35, vec3f(0.0), vec3f(1.0));
    color = mix(tinted_light, vec3f(luminance), smoothstep(0.72, 0.99, luminance) * 0.5);
    // Keep a small amount of the original spectral separation at reflections.
    color = clamp(color + (vec3f(r, g, b) - vec3f(luminance)) * 0.18, vec3f(0.0), vec3f(1.0));
  }

  color = color * opacity;

  let bg_color = u.color_back.rgb * u.color_back.a;
  color = color + bg_color * (1.0 - opacity);
  opacity = opacity + u.color_back.a * (1.0 - opacity);

  return vec4f(color, opacity);
}
