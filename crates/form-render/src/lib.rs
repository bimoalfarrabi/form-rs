//! Headless renderer for the Form logo materials.
//!
//! One offscreen `Rgba8Unorm` target per frame, three hand-ported WGSL
//! fragment shaders, and a mipmapped map texture. The device and queue are
//! supplied by the caller so the same code runs inside eframe and headless.

pub mod color;
pub mod fixtures;
pub mod params;
pub mod preprocess;
pub mod video;

#[cfg(test)]
mod shader_tests;

use anyhow::{Context, Result};
use params::{FrameRequest, GpuUniforms, Material};

pub use color::Rgba;
pub use params::{Field, LoopSettings, Params, Quality, Ratio};

/// The uploaded, mipmapped preprocessing map (R = field, G = mask, B = 255).
pub struct MapTexture {
    pub width: u32,
    pub height: u32,
    /// Kept alive: the view and the mip chain reference this GPU memory.
    #[allow(dead_code)]
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    pub rgba: Vec<u8>,
}

impl MapTexture {
    pub fn aspect(&self) -> f32 {
        self.width as f32 / self.height.max(1) as f32
    }
}

/// A rendered frame, ready for display or readback.
pub struct Frame {
    pub texture: wgpu::Texture,
    /// Plain `Rgba8Unorm` view (exact bytes the shader wrote).
    pub view: wgpu::TextureView,
    /// `Rgba8UnormSrgb` view of the same bytes, for egui's sRGB-aware shader.
    pub view_srgb: wgpu::TextureView,
    pub width: u32,
    pub height: u32,
}

pub struct Renderer {
    sampler: wgpu::Sampler,
    uniform_buffer: wgpu::Buffer,
    bind_group_layout: wgpu::BindGroupLayout,
    pipelines: Vec<Option<wgpu::RenderPipeline>>,
    blit: Blit,
}

struct Blit {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniforms: Vec<wgpu::Buffer>,
}

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

impl Renderer {
    pub fn new(device: &wgpu::Device) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("form-unforms"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(
                            std::mem::size_of::<GpuUniforms>() as u64
                        ),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("form-uniforms"),
            size: std::mem::size_of::<GpuUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("form-map-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });

        let blit = Blit::new(device);

        Self {
            sampler,
            uniform_buffer,
            bind_group_layout,
            pipelines: vec![None; 3],
            blit,
        }
    }

    fn pipeline(&mut self, device: &wgpu::Device, material: Material) -> &wgpu::RenderPipeline {
        let index = material as usize;
        if self.pipelines[index].is_none() {
            let source = match material {
                Material::Metal => include_str!("wgsl/metal.wgsl"),
                Material::Heat => include_str!("wgsl/heat.wgsl"),
                Material::Glass => include_str!("wgsl/glass.wgsl"),
            };
            self.pipelines[index] = Some(build_pipeline(
                device,
                &self.bind_group_layout,
                source,
                material.label(),
            ));
        }
        self.pipelines[index].as_ref().unwrap()
    }

    /// Upload the preprocessing map and build its mip chain.
    ///
    /// `rgba` must be `width * height * 4` bytes, straight (not premultiplied).
    pub fn upload_map(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> Result<MapTexture> {
        anyhow::ensure!(
            rgba.len() == (width as usize) * (height as usize) * 4,
            "map buffer is {} bytes, expected {}x{}x4",
            rgba.len(),
            width,
            height
        );

        let mips = mip_count(width, height);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("form-map"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: mips,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        self.blit.generate_mips(device, queue, &texture, mips);

        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("form-map-view"),
            ..Default::default()
        });

        Ok(MapTexture {
            width,
            height,
            texture,
            view,
            rgba: rgba.to_vec(),
        })
    }

    /// Render one frame into a fresh texture.
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        map: &MapTexture,
        request: &FrameRequest,
    ) -> Frame {
        let uniforms = request.uniforms();
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("form-bind-group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&map.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("form-frame"),
            size: wgpu::Extent3d {
                width: request.width,
                height: request.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[wgpu::TextureFormat::Rgba8UnormSrgb],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let view_srgb = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("form-frame-srgb"),
            format: Some(wgpu::TextureFormat::Rgba8UnormSrgb),
            ..Default::default()
        });

        let pipeline = self.pipeline(device, request.material);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("form-encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("form-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // The shader composites its own background, so the
                        // attachment only has to be cleared.
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                multiview_mask: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit(Some(encoder.finish()));

        Frame {
            texture,
            view,
            view_srgb,
            width: request.width,
            height: request.height,
        }
    }

    /// Render with an ad-hoc shader source (debug taps, experiments).
    pub fn render_with_shader(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        map: &MapTexture,
        request: &FrameRequest,
        source: &str,
    ) -> Result<Frame> {
        let pipeline = build_pipeline(device, &self.bind_group_layout, source, "debug");
        let uniforms = request.uniforms();
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("debug-bind-group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&map.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("debug-frame"),
            size: wgpu::Extent3d {
                width: request.width,
                height: request.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[wgpu::TextureFormat::Rgba8UnormSrgb],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("debug-encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("debug-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                multiview_mask: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit(Some(encoder.finish()));
        let view_srgb = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("debug-frame-srgb"),
            format: Some(wgpu::TextureFormat::Rgba8UnormSrgb),
            ..Default::default()
        });
        Ok(Frame {
            texture,
            view,
            view_srgb,
            width: request.width,
            height: request.height,
        })
    }

    /// Blocking readback of a frame as straight RGBA8 rows.
    pub fn read_frame(&self, device: &wgpu::Device, queue: &wgpu::Queue, frame: &Frame) -> Vec<u8> {
        let bytes_per_row = frame.width * 4;
        let padded = padded_row_bytes(bytes_per_row);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("form-readback"),
            size: (padded as u64) * (frame.height as u64),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("form-readback-encoder"),
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &frame.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(frame.height),
                },
            },
            wgpu::Extent3d {
                width: frame.width,
                height: frame.height,
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
        rx.recv().expect("map_async callback").expect("map failed");

        let data = slice.get_mapped_range().expect("get_mapped_range");
        let mut out = Vec::with_capacity((bytes_per_row * frame.height) as usize);
        for row in 0..frame.height {
            let start = (row * padded) as usize;
            out.extend_from_slice(&data[start..start + bytes_per_row as usize]);
        }
        drop(data);
        buffer.unmap();
        out
    }
}

impl Blit {
    fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("form-blit-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(16),
                    },
                    count: None,
                },
            ],
        });

        // One small uniform buffer per mip level (max 13 for 8192px).
        let uniforms: Vec<wgpu::Buffer> = (0..13)
            .map(|i| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(&format!("form-blit-uniform-{i}")),
                    size: 16,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            })
            .collect();

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("form-blit-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let pipeline = build_pipeline(device, &layout, include_str!("wgsl/blit.wgsl"), "form-blit");

        Self {
            pipeline,
            layout,
            sampler,
            uniforms,
        }
    }

    fn generate_mips(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        levels: u32,
    ) {
        if levels <= 1 {
            return;
        }
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("form-mips"),
        });
        for level in 1..levels {
            let src_view = texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("form-mip-src"),
                base_mip_level: level - 1,
                mip_level_count: Some(1),
                ..Default::default()
            });
            let dst_view = texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("form-mip-dst"),
                base_mip_level: level,
                mip_level_count: Some(1),
                ..Default::default()
            });
            let dst_w = (texture.width() >> level).max(1) as f32;
            let dst_h = (texture.height() >> level).max(1) as f32;
            let slot = &self.uniforms[(level as usize - 1).min(self.uniforms.len() - 1)];
            queue.write_buffer(slot, 0, bytemuck::cast_slice(&[dst_w, dst_h, 0.0, 0.0]));
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("form-mip-bind"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&src_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: slot.as_entire_binding(),
                    },
                ],
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("form-mip-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &dst_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                multiview_mask: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit(Some(encoder.finish()));
    }
}

fn build_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    source: &str,
    label: &str,
) -> wgpu::RenderPipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: &[Some(layout)],
        immediate_size: 0,
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: FORMAT,
                // The shaders already premultiply; keep the bytes as written.
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

fn mip_count(width: u32, height: u32) -> u32 {
    32 - width.max(height).max(1).leading_zeros()
}

fn padded_row_bytes(bytes_per_row: u32) -> u32 {
    const ALIGN: u32 = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    bytes_per_row.div_ceil(ALIGN) * ALIGN
}

/// Write a straight-alpha RGBA8 buffer to a PNG, un-premultiplying nothing:
/// the shader output is already premultiplied, so divide by alpha first.
pub fn write_png(path: &std::path::Path, rgba: &[u8], width: u32, height: u32) -> Result<()> {
    let mut straight = Vec::with_capacity(rgba.len());
    for px in rgba.as_chunks::<4>().0 {
        let a = px[3];
        if a == 0 {
            straight.extend_from_slice(&[0, 0, 0, 0]);
        } else {
            let un = |v: u8| ((v as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8;
            straight.extend_from_slice(&[un(px[0]), un(px[1]), un(px[2]), a]);
        }
    }
    image::save_buffer(path, &straight, width, height, image::ColorType::Rgba8)
        .with_context(|| format!("writing {}", path.display()))
}

/// Device creation for headless runs (tests, CLI export).
pub async fn headless_device() -> Result<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
            apply_limit_buckets: false,
        })
        .await
        .context("no wgpu adapter")?;
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("form-headless"),
            required_features: wgpu::Features::empty(),
            required_limits: adapter.limits(),
            ..Default::default()
        })
        .await
        .context("requesting wgpu device")?;
    Ok((device, queue))
}

/// `wgpu::util::DeviceExt` is only needed by callers building their own buffers.
pub use wgpu::util::DeviceExt as _DeviceExt;
