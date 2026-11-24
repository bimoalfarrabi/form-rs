//! Fullscreen blit used to build the mip chain.
//!
//! `dst_size` is the render target's size in pixels, so the source is sampled
//! at the destination's normalised coordinates (a 2x box downsample).

struct VsOut {
  @builtin(position) pos: vec4f,
};

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VsOut {
  var corners = array<vec2f, 3>(vec2f(-1.0, -1.0), vec2f(3.0, -1.0), vec2f(-1.0, 3.0));
  return VsOut(vec4f(corners[i], 0.0, 1.0));
}

struct BlitUniforms {
  dst_size: vec2f,
  pad: vec2f,
};

@group(0) @binding(0) var src_tex: texture_2d<f32>;
@group(0) @binding(1) var src_smp: sampler;
@group(0) @binding(2) var<uniform> b: BlitUniforms;

@fragment
fn fs_main(@builtin(position) pos: vec4f) -> @location(0) vec4f {
  let uv = pos.xy / b.dst_size;
  return textureSampleLevel(src_tex, src_smp, uv, 0.0);
}
