// Heatmap, hand-ported from @paper-design/shaders 0.0.82 heatmapFragmentShader
// plus the app's patches and loop patch.
//
// Reference: reference/shaders/heat.frag.glsl

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

const TWO_PI: f32 = 6.28318530718;
const PI: f32 = 3.14159265358979323846;
const MAX_COLORS: u32 = 6u;

fn glsl_mod(x: f32, y: f32) -> f32 {
  return x - y * floor(x / y);
}

// Explicit formula so reversed edges (`sst(.6, .3, t)`) stay well defined.
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

fn get_img_frame(uv: vec2f, th: f32) -> f32 {
  var frame = 1.0;
  frame = frame * sst(0.0, th, uv.y);
  frame = frame * (1.0 - sst(1.0 - th, 1.0, uv.y));
  frame = frame * sst(0.0, th, uv.x);
  frame = frame * (1.0 - sst(1.0 - th, 1.0, uv.x));
  return frame;
}

fn circle(uv: vec2f, c: vec2f, r: vec2f) -> f32 {
  return 1.0 - sst(r.x, r.y, length(uv - c));
}

fn shadow_shape(uv_in: vec2f, t_in: f32, contour: f32) -> f32 {
  var uv = uv_in;
  var t = t_in;
  var scaled_uv = uv;

  // base shape trajectory
  let pos_y = mix(-1.0, 2.0, t);

  // scaleX when it's moving down
  scaled_uv.y = scaled_uv.y - 0.5;
  let main_circle_scale = sst(0.0, 0.8, pos_y) * lst(1.4, 0.9, pos_y);
  scaled_uv = scaled_uv * vec2f(1.0, 1.0 + 1.5 * main_circle_scale);
  scaled_uv.y = scaled_uv.y + 0.5;

  // base shape
  let inner_r = 0.4;
  let outer_r = 1.0 - 0.3 * (sst(0.1, 0.2, t) * (1.0 - sst(0.2, 0.5, t)));
  var s = circle(scaled_uv, vec2f(0.5, pos_y - 0.2), vec2f(inner_r, outer_r));
  s = pow(s, 1.4);
  s = s * 1.2;

  // flat gradient to take over the shadow shape
  {
    let pos = pos_y - uv.y;
    let edge = 1.2;
    var top_flattener = lst(-0.4, 0.0, pos) * (1.0 - sst(0.0, edge, pos));
    top_flattener = pow(top_flattener, 3.0);
    let mixer = 1.0 - sst(0.0, 0.3, pos);
    s = mix(top_flattener, s, mixer);
  }

  // apple right circle
  {
    let visibility = sst(0.6, 0.7, t) * (1.0 - sst(0.8, 0.9, t));
    let angle = -2.0 - t * TWO_PI;
    var right_circle = circle(
      uv,
      vec2f(0.95 - 0.2 * cos(angle), 0.4 - 0.1 * sin(angle)),
      vec2f(0.15, 0.3)
    );
    right_circle = right_circle * visibility;
    s = mix(s, 0.0, right_circle);
  }

  // apple top circle
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

  // apple bottom circle
  {
    let visibility = sst(0.0, 0.4, t) * (1.0 - sst(0.6, 0.8, t));
    s = mix(s, 0.0, visibility * circle(uv, vec2f(0.52, 0.92), vec2f(0.09, 0.25)));
  }

  // random balls that are invisible if apple logo is selected
  {
    let pos = sst(0.0, 0.6, t) * (1.0 - sst(0.6, 1.0, t));
    s = mix(s, 0.5, circle(uv, vec2f(0.0, 1.2 - 0.5 * pos), vec2f(0.1, 0.3)));
    s = mix(s, 0.0, circle(uv, vec2f(1.0, 0.5 + 0.5 * pos), vec2f(0.1, 0.3)));
    s = mix(s, 1.0, circle(uv, vec2f(0.95, 0.2 + 0.2 * sst(0.3, 0.4, t) * sst(0.7, 0.5, t)), vec2f(0.07, 0.22)));
    s = mix(s, 1.0, circle(uv, vec2f(0.95, 0.2 + 0.2 * sst(0.3, 0.4, t) * (1.0 - sst(0.5, 0.7, t))), vec2f(0.07, 0.22)));
    s = s / max(1e-4, sst(1.0, 0.85, uv.y));
  }

  // The original is `clamp(0., 1., s)`, which evaluates to min(1., s).
  s = min(1.0, s);
  return s;
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

  sum += w2 * textureSampleGrad(img_tex, img_smp, uv + vec2f(0.0, -r.y), dudx, dudy).g;
  sum += w2 * textureSampleGrad(img_tex, img_smp, uv + vec2f(0.0, r.y), dudx, dudy).g;
  sum += w2 * textureSampleGrad(img_tex, img_smp, uv + vec2f(-r.x, 0.0), dudx, dudy).g;
  sum += w2 * textureSampleGrad(img_tex, img_smp, uv + vec2f(r.x, 0.0), dudx, dudy).g;

  sum += w1 * textureSampleGrad(img_tex, img_smp, uv + vec2f(-r.x, -r.y), dudx, dudy).g;
  sum += w1 * textureSampleGrad(img_tex, img_smp, uv + vec2f(r.x, -r.y), dudx, dudy).g;
  sum += w1 * textureSampleGrad(img_tex, img_smp, uv + vec2f(-r.x, r.y), dudx, dudy).g;
  sum += w1 * textureSampleGrad(img_tex, img_smp, uv + vec2f(r.x, r.y), dudx, dudy).g;

  return sum / norm;
}

fn image_box(res: vec2f) -> vec2f {
  let box_w = min(res.x / u.image_aspect, res.y) * u.image_aspect;
  return vec2f(box_w, box_w / u.image_aspect);
}

fn centered_uv(pos: vec2f, box: vec2f) -> vec2f {
  return 0.5 + (pos - 0.5 * u.resolution) / box / u.scale;
}

@fragment
fn fs_main(@builtin(position) pos: vec4f) -> @location(0) vec4f {
  let res = u.resolution;
  let image_uv = centered_uv(pos.xy, image_box(res));
  var img_uv = image_uv - vec2f(0.5);
  img_uv = img_uv * 0.5714285714285714;
  img_uv = img_uv + vec2f(0.5);
  let img_soft_frame = get_img_frame(img_uv, 0.03);

  let img = textureSample(img_tex, img_smp, img_uv);
  let dudx = dpdx(img_uv);
  let dudy = dpdy(img_uv);

  if (img.a == 0.0) {
    return u.color_back;
  }

  var t = 0.1 * u.time;
  if (u.loop_on == 1.0) {
    t = 0.1 * u.loop_start + u.loop_phase / 6.28318530718 * u.loop_cycles;
  }
  t = t - 0.3;

  var t_copy = t + 1.0 / 3.0;
  var t_copy2 = t + 2.0 / 3.0;
  t = glsl_mod(t, 1.0);
  t_copy = glsl_mod(t_copy, 1.0);
  t_copy2 = glsl_mod(t_copy2, 1.0);

  var animation_uv = img_uv - vec2f(0.5);
  let angle = -u.angle * PI / 180.0;
  let cos_a = cos(angle);
  let sin_a = sin(angle);
  animation_uv = vec2f(
    animation_uv.x * cos_a - animation_uv.y * sin_a,
    animation_uv.x * sin_a + animation_uv.y * cos_a
  ) + vec2f(0.5);

  let shape = img.r;

  let blurred_g = blur_edge_3x3(img_uv, dudx, dudy, 8.0, img.g);

  var outer_blur = 1.0 - mix(1.0, blurred_g, shape);
  let inner_blur = mix(blurred_g, 0.0, shape);
  let contour = mix(img.b, 0.0, shape);

  outer_blur = outer_blur * img_soft_frame;

  let shadow = shadow_shape(animation_uv, t, inner_blur);
  let shadow_copy = shadow_shape(animation_uv, t_copy, inner_blur);
  let shadow_copy2 = shadow_shape(animation_uv, t_copy2, inner_blur);

  var inner = 0.8 + 0.8 * inner_blur;
  inner = mix(inner, 0.0, shadow);
  inner = mix(inner, 0.0, shadow_copy);
  inner = mix(inner, 0.0, shadow_copy2);
  inner = inner * mix(0.0, 2.0, u.inner_glow);
  inner = inner + (u.contour * 2.0) * contour;
  inner = min(1.0, inner);
  inner = inner * (1.0 - shape);

  var outer = 0.0;
  {
    t = t * 3.0;
    t = glsl_mod(t - 0.1, 1.0);
    outer = 0.9 * pow(outer_blur, 0.8);
    let y = glsl_mod(animation_uv.y - t, 1.0);
    var animated_mask = sst(0.3, 0.65, y) * (1.0 - sst(0.65, 1.0, y));
    animated_mask = 0.5 + animated_mask;
    outer = outer * animated_mask;
    outer = outer * mix(0.0, 5.0, pow(u.outer_glow, 2.0));
    outer = outer * img_soft_frame;
  }

  inner = pow(inner, 1.2);
  // The browser patch adds a screen-edge fade; gl_FragCoord's origin is
  // symmetric here, so pos.xy works unchanged.
  let screen_uv = pos.xy / res;
  let halo_fade = sst(0.0, 0.18, screen_uv.x)
    * sst(0.0, 0.18, 1.0 - screen_uv.x)
    * sst(0.0, 0.18, screen_uv.y)
    * sst(0.0, 0.18, 1.0 - screen_uv.y);
  let heat = max(0.0, (clamp(inner + outer * halo_fade, 0.0, 1.0) - 0.025) / 0.975);

  let mixer = heat * u.colors_count;
  var gradient = u.colors[0];
  gradient = vec4f(gradient.rgb * gradient.a, gradient.a);
  var outer_shape = 0.0;
  for (var i: u32 = 1u; i < MAX_COLORS + 1u; i = i + 1u) {
    if (f32(i) > u.colors_count) {
      break;
    }
    let m = clamp(mixer - f32(i - 1u), 0.0, 1.0);
    if (i == 1u) {
      outer_shape = m;
    }
    let c = u.colors[i - 1u];
    gradient = mix(gradient, vec4f(c.rgb * c.a, c.a), m);
  }

  var color = gradient.rgb * outer_shape;
  var opacity = gradient.a * outer_shape;

  let bg_color = u.color_back.rgb * u.color_back.a;
  color = color + bg_color * (1.0 - opacity);
  opacity = opacity + u.color_back.a * (1.0 - opacity);

  return vec4f(color, opacity);
}
