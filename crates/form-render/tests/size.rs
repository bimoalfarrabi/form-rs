//! Readback of rows that are not multiples of wgpu's 256-byte copy alignment.
//!
//! Gated on `FORM_GPU_TESTS` so `cargo test --workspace` stays GPU-free by
//! default. At 1200 px wide a row is 4800 bytes, which rounds up to 4864 in
//! the readback buffer — the padding path every export width that is not a
//! multiple of 64 px takes.

use form_render::color::Rgba;
use form_render::params::{preset_colors, FrameRequest, Material, Params};
use form_render::{headless_device, Renderer};

#[test]
fn non_256_aligned_row_readback() -> anyhow::Result<()> {
    if std::env::var_os("FORM_GPU_TESTS").is_none() {
        return Ok(());
    }

    let (device, queue) = pollster::block_on(headless_device())?;
    let mut renderer = Renderer::new(&device);

    // Uniform opaque mid-grey, 64x64.
    let map: Vec<u8> = [128u8, 128, 128, 255].repeat(64 * 64);
    let map = renderer.upload_map(&device, &queue, 64, 64, &map)?;

    let request = FrameRequest {
        material: Material::Metal,
        params: Params::for_material(Material::Metal),
        colors: preset_colors(Material::Metal, 0),
        background: Rgba::from_hex("#141414").expect("valid hex"),
        transparent: false,
        time: 0.0,
        loop_settings: None,
        palette_index: 0,
        image_aspect: 1.0,
        width: 1200,
        height: 630,
    };

    let frame = renderer.render(&device, &queue, &map, &request);
    let bytes = renderer.read_frame(&device, &queue, &frame);

    let (w, h) = (request.width as usize, request.height as usize);
    let pixels = w * h;
    assert_eq!(
        bytes.len(),
        pixels * 4,
        "readback is {} bytes, expected {w}x{h}x4",
        bytes.len()
    );

    for (name, pixel) in [("first", 0), ("middle", pixels / 2), ("last", pixels - 1)] {
        assert_eq!(
            bytes[pixel * 4 + 3],
            255,
            "{name} pixel alpha is {}, expected 255",
            bytes[pixel * 4 + 3]
        );
    }

    let last_row = &bytes[(h - 1) * w * 4..];
    assert!(
        last_row.iter().any(|&b| b != 0),
        "last row is all zero — a mis-padded readback is dropping row data"
    );

    Ok(())
}
