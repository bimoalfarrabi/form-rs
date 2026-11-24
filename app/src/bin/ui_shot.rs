//! Headless UI screenshot: renders one frame of the editor to a PNG.
//!
//! ```text
//! cargo run -p form-app --bin ui_shot -- /tmp/opencode/ui.png [width] [height]
//! ```

use eframe::egui;
use eframe::wgpu;
use egui_wgpu::{Renderer, RendererOptions, ScreenDescriptor};

use form_app::{install_fonts, FormApp};

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = args
        .first()
        .cloned()
        .unwrap_or_else(|| "/tmp/opencode/ui.png".to_string());
    let width: u32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1440);
    let height: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(900);

    let (device, queue) = pollster::block_on(form_render::headless_device())?;
    let renderer = Renderer::new(&device, FORMAT, RendererOptions::default());
    let mut app = FormApp::new_headless(&device, &queue, renderer);

    let ctx = egui::Context::default();
    install_fonts(&ctx);
    ctx.set_pixels_per_point(1.0);

    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(width as f32, height as f32),
        )),
        ..Default::default()
    };

    // Two passes: the first creates the preview texture, the second paints it.
    let mut first = ctx.run_ui(raw.clone(), |ui| app.draw(ui));
    first.textures_delta.clear();
    drop(first);
    app.set_dirty();
    let mut output = ctx.run_ui(raw, |ui| app.draw(ui));

    let screen = ScreenDescriptor {
        size_in_pixels: [width, height],
        pixels_per_point: 1.0,
    };
    println!(
        "shapes: {} free: {}",
        output.shapes.len(),
        output.textures_delta.free.len()
    );
    for (i, cs) in output.shapes.iter().enumerate().take(8) {
        println!("  [{i}] clip={:?} shape={:?}", cs.clip_rect, cs.shape);
    }
    let jobs = ctx.tessellate(output.shapes, 1.0);
    println!("jobs: {}", jobs.len());

    let mut textures = std::mem::take(&mut output.textures_delta);

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("ui-shot"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("ui-shot-encoder"),
    });
    let extra = app.with_renderer(|renderer| {
        for (id, deltas) in &textures.set {
            for delta in deltas {
                renderer.update_texture(&device, &queue, *id, delta);
            }
        }
        for id in &textures.free {
            renderer.free_texture(id);
        }
        renderer.update_buffers(&device, &queue, &mut encoder, &jobs, &screen)
    });
    textures.clear();
    {
        let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ui-shot-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.078,
                        g: 0.078,
                        b: 0.078,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            multiview_mask: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        let mut pass = pass.forget_lifetime();
        app.with_renderer(|renderer| renderer.render(&mut pass, &jobs, &screen));
    }
    queue.submit(extra.into_iter().chain(std::iter::once(encoder.finish())));

    let padded = (width * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("ui-shot-readback"),
        size: (padded as u64) * (height as u64),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("ui-shot-readback-encoder"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));

    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    let _ = device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    });
    rx.recv()?.map_err(|e| anyhow::anyhow!("{e}"))?;

    let data = slice.get_mapped_range()?;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for row in 0..height {
        let start = (row * padded) as usize;
        rgba.extend_from_slice(&data[start..start + (width * 4) as usize]);
    }
    drop(data);
    buffer.unmap();

    form_render::write_png(std::path::Path::new(&out), &rgba, width, height)?;
    println!("wrote {out} ({width}x{height})");
    Ok(())
}
