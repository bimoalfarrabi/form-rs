//! The native editor: state, GPU preview and the dark panel UI.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use eframe::egui;
use eframe::wgpu;
use form_render::color::Rgba;
use form_render::params::{self, Field, FrameRequest, Material, Params, Quality, Ratio};
use form_render::video::{self, LoopJob};
use form_render::{Frame, MapTexture, Renderer};

use crate::logo::{self, Raster};

const BG: egui::Color32 = egui::Color32::from_rgb(0x14, 0x14, 0x14);
const SURFACE: egui::Color32 = egui::Color32::from_rgb(0x1c, 0x1c, 0x1c);
const INK: egui::Color32 = egui::Color32::from_rgb(0xef, 0xef, 0xed);
const MUTED: egui::Color32 = egui::Color32::from_rgb(0x88, 0x88, 0x86);
const LINE: egui::Color32 = egui::Color32::from_rgb(0x34, 0x34, 0x32);
const PANEL: egui::Color32 = egui::Color32::from_rgb(0x28, 0x28, 0x27);

/// Longest side of the live preview, in device pixels.
const PREVIEW_MAX: u32 = 900;

pub struct FormApp {
    device: wgpu::Device,
    queue: wgpu::Queue,
    egui_renderer: Option<Arc<egui::mutex::RwLock<eframe::egui_wgpu::Renderer>>>,
    render: Renderer,

    logo: Option<Raster>,
    filename: String,
    maps: [Option<MapTexture>; 3],

    material: Material,
    params: [Params; 3],
    preset_index: [usize; 3],
    custom_colors: [Option<Vec<Rgba>>; 3],
    background: Rgba,
    transparent: bool,
    quality: Quality,
    ratio: Ratio,
    paused: bool,
    status: String,

    time: f32,
    last_tick: Instant,
    preview: Option<PreviewTexture>,
    dirty: bool,
    /// When set, the app saves one screenshot and exits (used for docs/QA).
    pub screenshot_path: Option<PathBuf>,
    frames_drawn: u32,
    export_rx: Option<Receiver<ExportMessage>>,
    exporting: bool,
    ffmpeg: Option<String>,
}

enum ExportMessage {
    Progress(u32),
    Done(Result<PathBuf, String>),
}

struct PreviewTexture {
    id: egui::TextureId,
    size: (u32, u32),
}

impl FormApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Result<Self> {
        install_fonts(&cc.egui_ctx);
        let state = cc
            .wgpu_render_state
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("eframe did not provide a wgpu render state"))?;

        let device = state.device.clone();
        let queue = state.queue.clone();
        let egui_renderer = state.renderer.clone();
        let render = Renderer::new(&device);

        let mut app = Self {
            device,
            queue,
            egui_renderer: Some(egui_renderer),
            render,
            logo: None,
            filename: "Form / Original".to_string(),
            maps: [None, None, None],
            material: Material::Metal,
            params: [
                Params::for_material(Material::Metal),
                Params::for_material(Material::Heat),
                Params::for_material(Material::Glass),
            ],
            preset_index: [0, 0, 0],
            custom_colors: [None, None, None],
            background: Rgba::from_hex("#141414").unwrap_or(Rgba::BLACK),
            transparent: false,
            quality: Quality::K2,
            ratio: Ratio::Square,
            paused: false,
            status: String::new(),
            time: 0.0,
            last_tick: Instant::now(),
            preview: None,
            dirty: true,
            screenshot_path: None,
            frames_drawn: 0,
            export_rx: None,
            exporting: false,
            ffmpeg: video::ffmpeg_status(),
        };

        app.load_default_logo();
        Ok(app)
    }

    /// Headless constructor for screenshot tools.
    pub fn new_headless(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: eframe::egui_wgpu::Renderer,
    ) -> Self {
        Self {
            device: device.clone(),
            queue: queue.clone(),
            egui_renderer: Some(std::sync::Arc::new(egui::mutex::RwLock::new(renderer))),
            render: Renderer::new(device),
            logo: None,
            filename: "Form / Original".to_string(),
            maps: [None, None, None],
            material: Material::Metal,
            params: [
                Params::for_material(Material::Metal),
                Params::for_material(Material::Heat),
                Params::for_material(Material::Glass),
            ],
            preset_index: [0, 0, 0],
            custom_colors: [None, None, None],
            background: Rgba::from_hex("#141414").unwrap_or(Rgba::BLACK),
            transparent: false,
            quality: Quality::K2,
            ratio: Ratio::Square,
            paused: false,
            status: String::new(),
            time: 0.0,
            last_tick: Instant::now(),
            preview: None,
            dirty: true,
            screenshot_path: None,
            frames_drawn: 0,
            export_rx: None,
            exporting: false,
            ffmpeg: video::ffmpeg_status(),
        }
        .tap_default_logo()
    }

    fn load_default_logo(&mut self) {
        match logo::default_logo() {
            Ok(raster) => {
                self.logo = Some(raster);
                self.filename = "Form / Original".to_string();
                self.invalidate_maps();
            }
            Err(e) => self.status = format!("Could not prepare the default logo: {e}"),
        }
    }

    fn invalidate_maps(&mut self) {
        self.maps = [None, None, None];
        self.dirty = true;
    }

    fn load_file(&mut self, bytes: &[u8], name: &str) {
        self.status = "Reading logo…".to_string();
        let prepared = logo::decode(bytes, name).and_then(logo::prepare);
        match prepared {
            Ok(raster) => {
                self.logo = Some(raster);
                self.filename = name.to_string();
                self.invalidate_maps();
                self.status.clear();
            }
            Err(e) => self.status = e.to_string(),
        }
    }

    /// Ensure the active material has a map texture.
    fn ensure_map(&mut self) {
        let index = self.material as usize;
        if self.maps[index].is_some() {
            return;
        }
        let Some(raster) = self.logo.clone() else {
            return;
        };
        let result = match self.material {
            Material::Heat => {
                form_render::preprocess::heat::process(&raster.rgba, raster.width, raster.height)
                    .map(|p| (p.width, p.height, p.rgba))
            }
            _ => form_render::preprocess::metal::process(&raster.rgba, raster.width, raster.height)
                .map(|p| (p.width, p.height, p.rgba)),
        };

        match result {
            Ok((w, h, rgba)) => match self
                .render
                .upload_map(&self.device, &self.queue, w, h, &rgba)
            {
                Ok(map) => {
                    // Glass reuses the metal field, exactly like the web app.
                    if self.material == Material::Glass {
                        self.maps[Material::Metal as usize] = Some(map);
                        self.maps[Material::Glass as usize] = self
                            .render
                            .upload_map(&self.device, &self.queue, w, h, &rgba)
                            .ok();
                    } else {
                        self.maps[index] = Some(map);
                    }
                    self.dirty = true;
                }
                Err(e) => self.status = format!("Could not upload the material map: {e}"),
            },
            Err(e) => {
                self.status = format!("Could not prepare this logo: {e}");
            }
        }
    }

    fn active_colors(&self) -> Vec<Rgba> {
        let index = self.material as usize;
        self.custom_colors[index]
            .clone()
            .unwrap_or_else(|| params::preset_colors(self.material, self.preset_index[index]))
    }

    fn current_params(&self) -> &Params {
        &self.params[self.material as usize]
    }

    fn set_material(&mut self, material: Material) {
        if material == self.material {
            return;
        }
        self.material = material;
        self.dirty = true;
    }

    fn apply_preset(&mut self, index: usize) {
        let m = self.material;
        let slot = m as usize;
        self.preset_index[slot] = index;
        self.custom_colors[slot] = None;
        let mut params = self.params[slot];
        params::apply_preset_params(&mut params, m, index);
        self.params[slot] = params;
        self.dirty = true;
    }

    fn render_preview(&mut self, size: (u32, u32)) {
        self.ensure_map();
        let Some(map) = self.maps[self.material as usize].as_ref() else {
            return;
        };

        let request = FrameRequest {
            material: self.material,
            params: *self.current_params(),
            colors: self.active_colors(),
            background: self.background,
            transparent: self.transparent,
            time: self.time,
            palette_index: self.preset_index[self.material as usize],
            loop_settings: None,
            image_aspect: map.aspect(),
            width: size.0,
            height: size.1,
        };

        let frame = self.render.render(&self.device, &self.queue, map, &request);
        self.present(frame);
    }

    fn present(&mut self, frame: Frame) {
        let mut renderer = self.egui_renderer.as_ref().expect("renderer").write();
        match &self.preview {
            Some(tex) if tex.size == (frame.width, frame.height) => {
                renderer.update_egui_texture_from_wgpu_texture(
                    &self.device,
                    &frame.view_srgb,
                    wgpu::FilterMode::Linear,
                    tex.id,
                );
            }
            Some(tex) => {
                renderer.free_texture(&tex.id);
                let id = renderer.register_native_texture(
                    &self.device,
                    &frame.view_srgb,
                    wgpu::FilterMode::Linear,
                );
                self.preview = Some(PreviewTexture {
                    id,
                    size: (frame.width, frame.height),
                });
            }
            None => {
                let id = renderer.register_native_texture(
                    &self.device,
                    &frame.view_srgb,
                    wgpu::FilterMode::Linear,
                );
                self.preview = Some(PreviewTexture {
                    id,
                    size: (frame.width, frame.height),
                });
            }
        }
    }

    fn export_png(&mut self) {
        self.ensure_map();
        let Some(map) = self.maps[self.material as usize].as_ref() else {
            self.status = "Nothing to export yet.".to_string();
            return;
        };
        let (w, h) = self.ratio.buffer(self.quality);
        let request = FrameRequest {
            material: self.material,
            params: *self.current_params(),
            colors: self.active_colors(),
            background: self.background,
            transparent: self.transparent,
            time: self.time,
            palette_index: self.preset_index[self.material as usize],
            loop_settings: None,
            image_aspect: map.aspect(),
            width: w,
            height: h,
        };
        let frame = self.render.render(&self.device, &self.queue, map, &request);
        let rgba = self.render.read_frame(&self.device, &self.queue, &frame);

        let default_name = format!("logo-{}.png", self.material.slug());
        let Some(path) = rfd::FileDialog::new()
            .set_file_name(&default_name)
            .add_filter("PNG", &["png"])
            .save_file()
        else {
            return;
        };
        match form_render::write_png(&path, &rgba, w, h) {
            Ok(()) => self.status = format!("Saved {w}×{h} PNG"),
            Err(e) => self.status = format!("Could not save PNG: {e}"),
        }
    }

    /// Render a six-second seamless loop on a worker thread.
    fn export_loop(&mut self, ctx: &egui::Context) {
        if self.exporting {
            return;
        }
        let Some(codec) = self.ffmpeg.clone() else {
            self.status = "Video export needs ffmpeg with libvpx-vp9 on your PATH.".to_string();
            return;
        };
        self.ensure_map();
        let Some(map) = self.maps[self.material as usize].as_ref() else {
            self.status = "Nothing to export yet.".to_string();
            return;
        };

        let default_name = video::default_file_name(self.material);
        let Some(path) = rfd::FileDialog::new()
            .set_file_name(&default_name)
            .add_filter("WebM video", &["webm"])
            .save_file()
        else {
            return;
        };
        let path = video::ensure_webm_extension(&path);

        let (width, height) = self.ratio.buffer(self.quality);
        let job = LoopJob {
            map_rgba: map.rgba.clone(),
            map_width: map.width,
            map_height: map.height,
            material: self.material,
            params: *self.current_params(),
            colors: self.active_colors(),
            background: self.background,
            transparent: self.transparent,
            time: self.time,
            palette_index: self.preset_index[self.material as usize],
            width,
            height,
            fps: 30,
            frames: 180,
            path: path.clone(),
        };

        let (tx, rx): (Sender<ExportMessage>, Receiver<ExportMessage>) = std::sync::mpsc::channel();
        let progress_tx = tx.clone();
        let ui = ctx.clone();
        std::thread::spawn(move || {
            let mut report = |p: u32| {
                let _ = progress_tx.send(ExportMessage::Progress(p));
                ui.request_repaint();
            };
            let result = video::encode_loop(&job, &mut report);
            let _ = tx.send(ExportMessage::Done(
                result.map(|()| path).map_err(|e| e.to_string()),
            ));
            ui.request_repaint();
        });

        self.export_rx = Some(rx);
        self.exporting = true;
        self.status = format!("Rendering loop · 0% ({codec})");
    }

    /// Drain progress from the export thread.
    fn poll_export(&mut self) {
        let Some(rx) = &self.export_rx else {
            return;
        };
        let mut finished = false;
        while let Ok(message) = rx.try_recv() {
            match message {
                ExportMessage::Progress(p) => {
                    self.status = format!("Rendering loop · {p}%");
                }
                ExportMessage::Done(Ok(path)) => {
                    self.status = format!(
                        "Saved {}",
                        path.file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default()
                    );
                    finished = true;
                }
                ExportMessage::Done(Err(e)) => {
                    self.status = format!("Video export failed: {e}");
                    finished = true;
                }
            }
        }
        if finished {
            self.export_rx = None;
            self.exporting = false;
        }
    }

    fn tick(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_tick).as_secs_f32().min(0.1);
        self.last_tick = now;
        if self.paused {
            return;
        }
        let speed = params::motion_speed(self.current_params(), self.material);
        if speed > 0.0 {
            self.time += dt * speed;
            self.dirty = true;
        }
    }

    /// Preview pixel size for the workspace rectangle.
    fn preview_size(&self, rect: egui::Rect, ppp: f32) -> (u32, u32) {
        let ratio = self.ratio.value();
        let avail_w = (rect.width() - 32.0).max(64.0);
        let avail_h = (rect.height() - 96.0).max(64.0);
        let mut w = avail_w.min(avail_h * ratio);
        let mut h = w / ratio;
        let long = w.max(h) * ppp;
        if long > PREVIEW_MAX as f32 {
            let s = PREVIEW_MAX as f32 / long;
            w *= s;
            h *= s;
        }
        (
            (w * ppp).round().max(1.0) as u32,
            (h * ppp).round().max(1.0) as u32,
        )
    }
}

impl eframe::App for FormApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }
}

impl FormApp {
    /// One UI pass. Split out so a headless renderer can rasterise the same
    /// interface for screenshots and regression checks.
    pub fn draw(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let ctx = &ctx;
        self.tick();
        self.poll_export();

        for file in ctx.input(|i| i.raw.dropped_files.clone()) {
            let path = file.path().to_path_buf();
            let name = path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "dropped.png".to_string());
            match file.bytes() {
                Ok(bytes) => self.load_file(&bytes, &name),
                Err(_) => {
                    if let Ok(bytes) = std::fs::read(&path) {
                        self.load_file(&bytes, &name);
                    }
                }
            }
        }

        let mut action: Option<Action> = None;
        self.header(ui, &mut action);
        self.aside(ui, &mut action);
        let preview_rect = self.workspace(ui);

        match action {
            Some(Action::Load(path)) => {
                if let Ok(bytes) = std::fs::read(&path) {
                    let name = path
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "logo.png".to_string());
                    self.load_file(&bytes, &name);
                }
            }
            Some(Action::ExportPng) => self.export_png(),
            Some(Action::ExportLoop) => self.export_loop(ctx),
            Some(Action::Material(m)) => self.set_material(m),
            Some(Action::Preset(i)) => self.apply_preset(i),
            None => {}
        }

        if self.dirty {
            let ppp = ctx.pixels_per_point();
            let size = self.preview_size(preview_rect, ppp);
            self.render_preview(size);
            self.dirty = false;
        }

        if !self.paused {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }

        self.frames_drawn += 1;
        if let Some(path) = self.screenshot_path.clone() {
            // Let the first frames settle (map upload, first paint), then grab.
            if self.frames_drawn == 12 {
                ctx.request_screenshot(move |image| {
                    let size = image.size;
                    let mut rgba = Vec::with_capacity(size[0] * size[1] * 4);
                    for px in image.pixels.iter() {
                        rgba.extend_from_slice(&[px.r(), px.g(), px.b(), px.a()]);
                    }
                    match form_render::write_png(&path, &rgba, size[0] as u32, size[1] as u32) {
                        Ok(()) => eprintln!("saved screenshot to {}", path.display()),
                        Err(e) => eprintln!("screenshot failed: {e}"),
                    }
                });
            }
            if self.frames_drawn >= 16 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            ctx.request_repaint();
        }
    }

    pub fn set_dirty(&mut self) {
        self.dirty = true;
    }

    fn tap_default_logo(mut self) -> Self {
        self.load_default_logo();
        self
    }

    /// Run `f` with the egui-wgpu renderer (headless tools only).
    pub fn with_renderer<R>(&self, f: impl FnOnce(&mut eframe::egui_wgpu::Renderer) -> R) -> R {
        let arc = self.egui_renderer.as_ref().expect("renderer present");
        let mut guard = arc.write();
        f(&mut guard)
    }

    pub fn preview_texture(&self) -> Option<egui::TextureId> {
        self.preview.as_ref().map(|p| p.id)
    }
}

enum Action {
    Load(PathBuf),
    ExportPng,
    ExportLoop,
    Material(Material),
    Preset(usize),
}

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "manrope".to_owned(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/manrope.ttf"
        ))),
    );
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "manrope".to_owned());
    ctx.set_fonts(fonts);
    ctx.style_mut_of(egui::Theme::Dark, |style| {
        style.visuals.panel_fill = BG;
        style.visuals.window_fill = BG;
        style.visuals.extreme_bg_color = SURFACE;
        style.visuals.faint_bg_color = PANEL;
        style.visuals.widgets.noninteractive.bg_fill = SURFACE;
        style.visuals.widgets.inactive.bg_fill = PANEL;
        style.visuals.widgets.hovered.bg_fill = PANEL;
        style.visuals.widgets.active.bg_fill = PANEL;
        style.visuals.override_text_color = Some(INK);
        style.spacing.slider_width = 124.0;
    });
}

fn to_color32(c: Rgba) -> egui::Color32 {
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    egui::Color32::from_rgba_unmultiplied(q(c.r), q(c.g), q(c.b), q(c.a))
}

fn from_color32(c: egui::Color32) -> Rgba {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    Rgba::new(
        r as f32 / 255.0,
        g as f32 / 255.0,
        b as f32 / 255.0,
        a as f32 / 255.0,
    )
}

impl FormApp {
    fn header(&mut self, ui: &mut egui::Ui, action: &mut Option<Action>) {
        egui::Panel::top("header")
            .exact_size(64.0)
            .frame(
                egui::Frame::NONE
                    .fill(BG)
                    .inner_margin(egui::Margin::symmetric(28, 0)),
            )
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 3.0;
                    ui.label(
                        egui::RichText::new("form")
                            .size(27.0)
                            .strong()
                            .color(INK)
                            .extra_letter_spacing(-1.5),
                    );
                    ui.label(egui::RichText::new("®").size(10.0).color(INK));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let button = egui::Button::new(
                            egui::RichText::new("+  Your logo").size(13.0).color(INK),
                        )
                        .fill(SURFACE)
                        .stroke(egui::Stroke::new(1.0, LINE))
                        .corner_radius(egui::CornerRadius::same(9))
                        .min_size(egui::vec2(0.0, 38.0));
                        if ui.add(button).clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("Logo", &["svg", "png", "webp", "jpg", "jpeg"])
                                .pick_file()
                            {
                                *action = Some(Action::Load(path));
                            }
                        }
                    });
                });
            });
    }

    fn aside(&mut self, ui: &mut egui::Ui, action: &mut Option<Action>) {
        egui::Panel::right("aside")
            .exact_size(328.0)
            .resizable(false)
            .frame(egui::Frame::NONE.fill(BG).inner_margin(egui::Margin {
                left: 12,
                right: 12,
                top: 4,
                bottom: 16,
            }))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(8.0, 10.0);
                self.tabs(ui, action);
                ui.add_space(2.0);
                self.sliders(ui);
                ui.add_space(2.0);
                self.presets(ui, action);
                self.colors(ui);
                ui.add_space(2.0);
                self.output_settings(ui);
                ui.add_space(4.0);
                self.export_buttons(ui, action);
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    ui.add_space(2.0);
                    ui.hyperlink_to(
                        egui::RichText::new("Made with Paper Shaders ↗")
                            .size(10.0)
                            .color(MUTED),
                        "https://github.com/paper-design/shaders",
                    );
                });
            });
    }

    fn tabs(&mut self, ui: &mut egui::Ui, action: &mut Option<Action>) {
        egui::Frame::NONE
            .fill(SURFACE)
            .corner_radius(egui::CornerRadius::same(11))
            .inner_margin(egui::Margin::same(3))
            .show(ui, |ui| {
                ui.columns(3, |cols| {
                    for (i, material) in Material::ALL.iter().enumerate() {
                        let selected = *material == self.material;
                        let text = egui::RichText::new(material.label())
                            .size(11.0)
                            .color(if selected { INK } else { MUTED });
                        let button = egui::Button::new(text)
                            .fill(if selected {
                                PANEL
                            } else {
                                egui::Color32::TRANSPARENT
                            })
                            .stroke(egui::Stroke::NONE)
                            .corner_radius(egui::CornerRadius::same(8))
                            .min_size(egui::vec2(cols[i].available_width(), 32.0));
                        if cols[i].add(button).clicked() {
                            *action = Some(Action::Material(*material));
                        }
                    }
                });
            });
    }

    fn sliders(&mut self, ui: &mut egui::Ui) {
        let material = self.material;
        let fields = Field::for_material(material);
        let mut params = self.params[material as usize];

        egui::Grid::new("controls")
            .num_columns(2)
            .spacing(egui::vec2(18.0, 12.0))
            .show(ui, |ui| {
                for (i, field) in fields.iter().enumerate() {
                    let (label, min, max, step) = field.meta();
                    let decimals = field.decimals();
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 6.0;
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(label).size(11.0).color(INK));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let value = params.get(*field);
                                    ui.label(
                                        egui::RichText::new(format!("{:.*}", decimals, value))
                                            .size(10.0)
                                            .color(MUTED),
                                    );
                                },
                            );
                        });
                        let mut value = params.get(*field);
                        // Inside a Grid cell `available_width` is the whole grid, so
                        // derive the column width from the panel instead.
                        let width = (ui.available_width() - 18.0) / 2.0;
                        let response = ui.add_sized(
                            [width.max(96.0), 18.0],
                            egui::Slider::new(&mut value, min..=max)
                                .step_by(step as f64)
                                .show_value(false),
                        );
                        if response.changed() {
                            params.set(*field, value);
                        }
                    });
                    if i % 2 == 1 {
                        ui.end_row();
                    }
                }
            });

        if params != self.params[material as usize] {
            self.params[material as usize] = params;
            self.dirty = true;
        }
    }

    fn presets(&mut self, ui: &mut egui::Ui, action: &mut Option<Action>) {
        ui.label(egui::RichText::new("Presets").size(11.0).color(MUTED));
        let material = self.material;
        let names = params::preset_names(material);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 9.0;
            for (i, name) in names.iter().enumerate() {
                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(27.0, 27.0), egui::Sense::click());
                let colors = params::swatch_colors(material, i);
                paint_swatch(ui, rect, &colors);
                if self.preset_index[material as usize] == i
                    && self.custom_colors[material as usize].is_none()
                {
                    ui.painter().rect_stroke(
                        rect.expand(2.0),
                        egui::CornerRadius::same(8),
                        egui::Stroke::new(1.5, INK),
                        egui::StrokeKind::Outside,
                    );
                }
                if response.clicked() {
                    *action = Some(Action::Preset(i));
                }
                response.on_hover_text(*name);
            }
        });
    }

    fn colors(&mut self, ui: &mut egui::Ui) {
        let material = self.material;
        let mut colors = self.active_colors();
        let labels = params::color_labels(material, colors.len());
        let mut changed = false;

        ui.add_space(4.0);
        ui.label(egui::RichText::new("Colors").size(12.0).color(INK));

        egui::Grid::new("colors")
            .num_columns(3)
            .spacing(egui::vec2(9.0, 6.0))
            .show(ui, |ui| {
                for (i, color) in colors.iter_mut().enumerate() {
                    let mut c32 = to_color32(*color);
                    if ui.color_edit_button_srgba(&mut c32).changed() {
                        *color = from_color32(c32);
                        changed = true;
                    }
                    ui.label(
                        egui::RichText::new(labels.get(i).cloned().unwrap_or_default())
                            .size(11.0)
                            .color(MUTED),
                    );
                    let mut hex = color.to_hex();
                    let edit = egui::TextEdit::singleline(&mut hex)
                        .font(egui::TextStyle::Monospace)
                        .desired_width(88.0);
                    if ui.add(edit).changed() {
                        if let Some(parsed) = Rgba::from_hex(&hex) {
                            *color = parsed;
                            changed = true;
                        }
                    }
                    ui.end_row();
                }
            });

        if changed {
            self.custom_colors[material as usize] = Some(colors);
            self.dirty = true;
        }

        ui.horizontal(|ui| {
            if ui
                .add(
                    egui::Button::new(egui::RichText::new("Reset colors").size(10.0).color(MUTED))
                        .fill(egui::Color32::TRANSPARENT)
                        .stroke(egui::Stroke::NONE),
                )
                .clicked()
            {
                self.custom_colors[material as usize] = None;
                self.dirty = true;
            }
        });
    }

    fn output_settings(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Quality").size(11.0).color(INK));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                egui::ComboBox::from_id_salt("quality")
                    .selected_text(match self.quality {
                        Quality::K2 => "2K",
                        Quality::K4 => "4K",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.quality, Quality::K2, "2K");
                        ui.selectable_value(&mut self.quality, Quality::K4, "4K");
                    });
            });
        });

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Background").size(11.0).color(INK));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mut c = to_color32(self.background);
                if ui.color_edit_button_srgba(&mut c).changed() {
                    self.background = from_color32(c);
                    self.dirty = true;
                }
                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(34.0, 26.0), egui::Sense::click());
                let fill = if self.transparent {
                    PANEL
                } else {
                    egui::Color32::TRANSPARENT
                };
                ui.painter().rect(
                    rect,
                    egui::CornerRadius::same(7),
                    fill,
                    egui::Stroke::new(1.0, LINE),
                    egui::StrokeKind::Inside,
                );
                paint_checker_icon(
                    ui,
                    rect.shrink(7.0),
                    if self.transparent { INK } else { MUTED },
                );
                if response.clicked() {
                    self.transparent = !self.transparent;
                    self.dirty = true;
                }
                response.on_hover_text("Transparent background");
            });
        });
    }

    fn export_buttons(&mut self, ui: &mut egui::Ui, action: &mut Option<Action>) {
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 8.0) / 2.0;
            let png = egui::Button::new(egui::RichText::new("Save PNG").size(12.0).color(INK))
                .fill(SURFACE)
                .stroke(egui::Stroke::new(1.0, LINE))
                .corner_radius(egui::CornerRadius::same(9))
                .min_size(egui::vec2(width, 38.0));
            if ui.add(png).clicked() {
                *action = Some(Action::ExportPng);
            }
            let ready = self.ffmpeg.is_some() && !self.exporting;
            let label = if self.exporting {
                "Rendering..."
            } else {
                "Loop - 6s"
            };
            let video = egui::Button::new(egui::RichText::new(label).size(12.0).color(if ready {
                INK
            } else {
                MUTED
            }))
            .fill(SURFACE)
            .stroke(egui::Stroke::new(1.0, LINE))
            .corner_radius(egui::CornerRadius::same(9))
            .min_size(egui::vec2(width, 38.0));
            let response = ui.add_enabled(ready, video);
            if response.clicked() {
                *action = Some(Action::ExportLoop);
            }
            if self.ffmpeg.is_none() {
                response.on_disabled_hover_text("Needs ffmpeg with libvpx-vp9 on your PATH");
            }
        });
    }

    fn workspace(&mut self, ui: &mut egui::Ui) -> egui::Rect {
        let mut preview_rect = egui::Rect::NOTHING;
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(BG).inner_margin(egui::Margin {
                left: 28,
                right: 24,
                top: 4,
                bottom: 20,
            }))
            .show(ui, |ui| {
                let card = ui.available_rect_before_wrap();
                let radius = egui::CornerRadius::same(16);
                ui.painter().rect_filled(card, radius, SURFACE);
                ui.painter().rect_stroke(
                    card,
                    radius,
                    egui::Stroke::new(1.0, LINE),
                    egui::StrokeKind::Inside,
                );

                let ratio = self.ratio.value();
                let w = (card.width() - 32.0).min((card.height() - 96.0) * ratio);
                let h = w / ratio;
                let rect = egui::Rect::from_center_size(card.center(), egui::vec2(w, h));

                if self.transparent {
                    checker(ui, rect);
                }

                if let Some(preview) = &self.preview {
                    let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                    egui::Image::new((preview.id, rect.size()))
                        .uv(uv)
                        .paint_at(ui, rect);
                }

                ui.painter().text(
                    egui::pos2(card.left() + 24.0, card.top() + 23.0),
                    egui::Align2::LEFT_CENTER,
                    &self.filename,
                    egui::FontId::proportional(11.0),
                    MUTED,
                );
                ui.painter().text(
                    egui::pos2(card.right() - 24.0, card.top() + 23.0),
                    egui::Align2::RIGHT_CENTER,
                    self.material.label(),
                    egui::FontId::proportional(11.0),
                    MUTED,
                );

                let bottom = egui::Rect::from_min_max(
                    egui::pos2(card.left() + 24.0, card.bottom() - 24.0 - 38.0),
                    egui::pos2(card.right() - 24.0, card.bottom() - 24.0),
                );
                ui.scope_builder(egui::UiBuilder::new().max_rect(bottom), |ui| {
                    ui.horizontal_centered(|ui| {
                        let (rect, response) =
                            ui.allocate_exact_size(egui::vec2(38.0, 38.0), egui::Sense::click());
                        ui.painter().rect(
                            rect,
                            egui::CornerRadius::same(9),
                            egui::Color32::TRANSPARENT,
                            egui::Stroke::new(1.0, LINE),
                            egui::StrokeKind::Inside,
                        );
                        paint_transport(ui, rect, self.paused);
                        if response.clicked() {
                            self.paused = !self.paused;
                            self.dirty = true;
                        }
                        response.on_hover_text(if self.paused { "Play" } else { "Pause" });
                        ui.label(egui::RichText::new(&self.status).size(11.0).color(MUTED));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            egui::ComboBox::from_id_salt("ratio")
                                .selected_text(self.ratio.label())
                                .width(66.0)
                                .show_ui(ui, |ui| {
                                    for ratio in [Ratio::Square, Ratio::Landscape, Ratio::Portrait]
                                    {
                                        if ui
                                            .selectable_label(self.ratio == ratio, ratio.label())
                                            .clicked()
                                        {
                                            self.ratio = ratio;
                                            self.dirty = true;
                                        }
                                    }
                                });
                        });
                    });
                });

                preview_rect = rect;
                ui.allocate_space(card.size());
            });
        preview_rect
    }
}

fn paint_swatch(ui: &egui::Ui, rect: egui::Rect, colors: &[Rgba]) {
    let radius = 8.0;
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::same(radius as u8), SURFACE);
    if colors.is_empty() {
        return;
    }
    let stops: Vec<egui::Color32> = colors.iter().copied().map(to_color32).collect();
    ui.painter()
        .add(egui::Shape::mesh(gradient_mesh(rect, &stops)));
    // egui can only clip to a rectangle, so restore the rounded corners.
    for corner in corners(rect) {
        ui.painter().add(egui::Shape::convex_polygon(
            corner_wedge(corner, radius),
            SURFACE,
            egui::Stroke::NONE,
        ));
    }
}

/// CSS `linear-gradient(135deg, …)`: colour depends only on the projection of a
/// point onto the down-right diagonal, with the stops spread evenly over the
/// box's extent along that axis.
fn gradient_mesh(rect: egui::Rect, stops: &[egui::Color32]) -> egui::Mesh {
    const SUBDIVISIONS: usize = 10;
    let c = std::f32::consts::FRAC_1_SQRT_2;
    let dir = egui::vec2(c, c);
    let project = |p: egui::Pos2| p.x * dir.x + p.y * dir.y;

    let corner_points = [
        rect.left_top(),
        rect.right_top(),
        rect.left_bottom(),
        rect.right_bottom(),
    ];
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;
    for p in corner_points {
        let v = project(p);
        min = min.min(v);
        max = max.max(v);
    }
    let span = (max - min).max(1e-3);

    let sample = |p: egui::Pos2| gradient_color(stops, (project(p) - min) / span);

    let mut mesh = egui::Mesh::default();
    let n = SUBDIVISIONS;
    for iy in 0..=n {
        for ix in 0..=n {
            let p = egui::pos2(
                rect.left() + rect.width() * ix as f32 / n as f32,
                rect.top() + rect.height() * iy as f32 / n as f32,
            );
            mesh.colored_vertex(p, sample(p));
        }
    }
    let index = |ix: usize, iy: usize| (iy * (n + 1) + ix) as u32;
    for iy in 0..n {
        for ix in 0..n {
            let a = index(ix, iy);
            let b = index(ix + 1, iy);
            let c = index(ix + 1, iy + 1);
            let d = index(ix, iy + 1);
            mesh.add_triangle(a, b, c);
            mesh.add_triangle(a, c, d);
        }
    }
    mesh
}

fn gradient_color(stops: &[egui::Color32], t: f32) -> egui::Color32 {
    if stops.len() == 1 {
        return stops[0];
    }
    let scaled = t.clamp(0.0, 1.0) * (stops.len() - 1) as f32;
    let i = (scaled.floor() as usize).min(stops.len() - 2);
    let f = scaled - i as f32;
    let a = stops[i];
    let b = stops[i + 1];
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * f).round() as u8;
    egui::Color32::from_rgba_unmultiplied(
        mix(a.r(), b.r()),
        mix(a.g(), b.g()),
        mix(a.b(), b.b()),
        mix(a.a(), b.a()),
    )
}

/// (corner, inward x, inward y) for each corner of a rectangle.
fn corners(rect: egui::Rect) -> [(egui::Pos2, egui::Vec2, egui::Vec2); 4] {
    [
        (rect.left_top(), egui::vec2(1.0, 0.0), egui::vec2(0.0, 1.0)),
        (
            rect.right_top(),
            egui::vec2(-1.0, 0.0),
            egui::vec2(0.0, 1.0),
        ),
        (
            rect.right_bottom(),
            egui::vec2(-1.0, 0.0),
            egui::vec2(0.0, -1.0),
        ),
        (
            rect.left_bottom(),
            egui::vec2(1.0, 0.0),
            egui::vec2(0.0, -1.0),
        ),
    ]
}

/// The slice between a square corner and its rounded arc, painted in the card
/// colour so the gradient mesh reads as a rounded swatch.
fn corner_wedge(
    (corner, dx, dy): (egui::Pos2, egui::Vec2, egui::Vec2),
    radius: f32,
) -> Vec<egui::Pos2> {
    const ARC_STEPS: usize = 6;
    let mut points = vec![corner, corner + dx * radius];
    let center = corner + dx * radius + dy * radius;
    for step in 1..ARC_STEPS {
        let theta = std::f32::consts::FRAC_PI_2 * (1.0 - step as f32 / ARC_STEPS as f32);
        points.push(center - (dx * theta.cos() + dy * theta.sin()) * radius);
    }
    points.push(corner + dy * radius);
    points
}

fn checker(ui: &egui::Ui, rect: egui::Rect) {
    let cell = 18.0;
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        egui::CornerRadius::ZERO,
        egui::Color32::from_rgb(0x22, 0x22, 0x22),
    );
    let mut y = rect.top();
    let mut row = 0;
    while y < rect.bottom() {
        let mut x = rect.left() + if row % 2 == 0 { 0.0 } else { cell };
        while x < rect.right() {
            let cell_rect = egui::Rect::from_min_size(
                egui::pos2(x, y),
                egui::vec2(cell.min(rect.right() - x), cell.min(rect.bottom() - y)),
            );
            painter.rect_filled(
                cell_rect,
                egui::CornerRadius::ZERO,
                egui::Color32::from_rgb(0x2c, 0x2c, 0x2c),
            );
            x += cell * 2.0;
        }
        y += cell;
        row += 1;
    }
}

/// A small checkerboard used for the transparent-background toggle.
fn paint_checker_icon(ui: &egui::Ui, rect: egui::Rect, color: egui::Color32) {
    let cell = rect.width() / 3.0;
    let painter = ui.painter();
    for row in 0..3 {
        for col in 0..3 {
            if (row + col) % 2 == 0 {
                let min = egui::pos2(
                    rect.left() + col as f32 * cell,
                    rect.top() + row as f32 * cell,
                );
                painter.rect_filled(
                    egui::Rect::from_min_size(min, egui::vec2(cell, cell)),
                    egui::CornerRadius::ZERO,
                    color,
                );
            }
        }
    }
}

/// Play triangle or pause bars, drawn rather than typed (no icon font).
fn paint_transport(ui: &egui::Ui, rect: egui::Rect, paused: bool) {
    let painter = ui.painter();
    let c = INK;
    // Paused shows a play triangle; playing shows pause bars.
    if !paused {
        let h = rect.height() * 0.34;
        let w = rect.width() * 0.10;
        let cy = rect.center().y;
        for dx in [-0.09f32, 0.03] {
            let x = rect.center().x + rect.width() * dx;
            painter.rect_filled(
                egui::Rect::from_min_size(egui::pos2(x, cy - h / 2.0), egui::vec2(w, h)),
                egui::CornerRadius::same(1),
                c,
            );
        }
    } else {
        let h = rect.height() * 0.36;
        let w = rect.width() * 0.30;
        let center = rect.center();
        painter.add(egui::Shape::convex_polygon(
            vec![
                egui::pos2(center.x - w * 0.45, center.y - h / 2.0),
                egui::pos2(center.x - w * 0.45, center.y + h / 2.0),
                egui::pos2(center.x + w * 0.75, center.y),
            ],
            c,
            egui::Stroke::NONE,
        ));
    }
}
