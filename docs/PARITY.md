# Parity with the browser editor

The native renderer is compared against the original web app at fixed settings.
This document records the method, the fixtures and the numbers, so a future
change can be judged rather than guessed at.

## Fixtures

The browser app is the source of truth. Its preprocessing maps and final renders
are captured from a running copy of
[youraipulse/Form](https://github.com/youraipulse/Form) and are **not**
distributed with this repository.

Set `FORM_FIXTURES` to a directory containing:

| file | produced by |
|---|---|
| `proc-source.png` | the default logo rasterised at 600×600 (black, alpha = shape) |
| `proc-metal.png` | `toProcessedLiquidMetal(source)` → `pngBlob` |
| `proc-heat.png` | `toProcessedHeatmap(source)` → `blob` |

Final renders (`ref-<material>-f0.png`) are captured with:

```js
const m = window.__form.mount;
m.setSpeed(0); m.setFrame(0);          // u_time = 0
document.querySelector('#shader canvas').toDataURL('image/png');
```

from the default page state: default logo, default parameters, `#141414`
background, 2K, 1:1. Capture inside a single evaluation — freezing the clock and
clicking the export button in two separate steps lets the animation advance.

## Golden renders

```bash
FORM_FIXTURES=/path/to/fixtures \
  cargo run -p form-render --bin render_ref -- metal 0 /tmp/metal.png
```

`render_ref` takes `<metal|heat|glass> [preset] [out.png]`. It uploads the
fixture map, renders at 2048×2048 with `u_time = 0`, and writes a PNG.

## Results

Diff is the maximum absolute per-channel difference over RGB, against the
browser capture.

| material | mean | > 2/255 | > 8/255 |
|---|---|---|---|
| heat | **0.13** | 0.04 % | 0.00 % |
| glass | **0.20** | 1.07 % | 0.18 % |
| metal | 1.80 | 8.96 % | 4.16 % |

- **heat** and **glass** are effectively exact; what remains is mip-filter
  rounding.
- **metal** keeps ~1 % of pixels with a large difference, all on stripe edges
  where `fract(direction)` wraps and `fwidth` is large. The shape, framing,
  palette and interior values match.

Preprocessing maps, compared channel by channel against the browser output:

| map | channel | mean | max | > 8 |
|---|---|---|---|---|
| metal | R (field) | 1.35 | 255 | 4.03 % |
| metal | G (alpha) | 0.62 | 154 | 1.06 % |
| heat | G, B | 0.01 | 1 | 0 % |
| heat | R | 0.03 | 4 | 0 % |

Metal's R tail comes from resampling the 600 px source up to 4096 px before the
Poisson solve (the browser rasterises the SVG at 4096 directly), not from the
solver: alpha classification agrees on 100 % of pixels.

## Behaviours that had to be reproduced

1. **Reversed `smoothstep`.** The heat shader calls `smoothstep(1., .85, y)` and
   `smoothstep(.6, .3, t)`. GLSL leaves that undefined; ANGLE returns `0`, and
   `shadowShape` depends on it. `sst()` in `heat.wgsl` returns `0` when
   `edge1 <= edge0`.
2. **`clamp(0., 1., s)`** in `shadowShape` is `min(1., s)`.
3. **`gl_FragCoord` origin.** WGSL's `@builtin(position)` is top-left. Metal and
   heat use the centred-fit formula with no flip; glass needs
   `uv.y = 1 - uv.y` and a bottom-left `fc` for its grain hash and fades.
4. **`pow(negative, …)`.** Metal evaluates `pow(uv.y, .3)` outside the image.
   GLSL lets NaN leak; WGSL does not, so the port clamps the base.
5. **Premultiplied, sRGB-encoded output.** The render target is `Rgba8Unorm`,
   not `*Srgb`; egui reads it through an `Rgba8UnormSrgb` view; PNG export
   un-premultiplies with clamping.
6. **Mip chain.** Glass samples `textureLod(…, 4.)`/`(…, 2.)`; wgpu has no
   `generateMipmap`, so mips come from blit passes with a per-level
   destination-size uniform.
7. **`u_imageAspectRatio`** for heat is the padded canvas aspect (1750×1750 for
   a square logo), not the logo's.

## Why the shaders are hand-ported

`naga`'s GLSL frontend accepts only `#version 440|450|460` and rejects loose
uniforms and `sampler2D` (verified against a five-line shader). `glslang` →
SPIR-V → `naga` works but adds a C++ toolchain to the build. The three shaders
are ~700 lines of GLSL, so hand-porting with golden-image tests is cheaper and
keeps the WGSL readable.
