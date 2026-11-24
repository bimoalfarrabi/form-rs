//! Material state, palette data and the GPU uniform layout.
//!
//! Everything here is a direct transcription of `main.js` (see
//! `reference/INVENTORY.md` §1–2) so the native app can be compared against the
//! web original value for value.

use crate::color::Rgba;

/// The three materials the app exposes, in tab order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Material {
    Metal,
    Heat,
    Glass,
}

impl Material {
    pub const ALL: [Material; 3] = [Material::Metal, Material::Heat, Material::Glass];

    pub fn label(self) -> &'static str {
        match self {
            Material::Metal => "Liquid metal",
            Material::Heat => "Heatmap",
            Material::Glass => "Frosted glass",
        }
    }

    /// Slug used in export file names (`logo-<mode>.png`).
    pub fn slug(self) -> &'static str {
        match self {
            Material::Metal => "metal",
            Material::Heat => "heat",
            Material::Glass => "glass",
        }
    }

    /// Uniform values that differ per material (main.js:53).
    pub fn uniforms(self) -> &'static str {
        match self {
            Material::Metal => "metal",
            Material::Heat => "heat",
            Material::Glass => "glass",
        }
    }
}

/// The 16 shared sliders (main.js:25).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Params {
    pub light_size: f32,
    pub haze: f32,
    pub frost: f32,
    pub backlight: f32,
    pub thickness: f32,
    pub speed: f32,
    pub scale: f32,
    pub distortion: f32,
    pub contour: f32,
    pub repetition: f32,
    pub softness: f32,
    pub dispersion: f32,
    pub angle: f32,
    pub inner_glow: f32,
    pub outer_glow: f32,
    pub noise: f32,
}

impl Default for Params {
    /// `const state = {...}` (main.js:25) — the values the app starts with.
    fn default() -> Self {
        Self {
            light_size: 1.0,
            haze: 0.4,
            frost: 0.42,
            backlight: 1.1,
            thickness: 0.5,
            speed: 1.17,
            scale: 0.78,
            distortion: 0.0,
            contour: 1.0,
            repetition: 4.35,
            softness: 1.0,
            dispersion: 0.10,
            angle: 315.0,
            inner_glow: 0.55,
            outer_glow: 0.3,
            noise: 0.015,
        }
    }
}

impl Params {
    /// `materialStates[mode]` overrides (main.js:26-30). Metal and glass start
    /// from the shared defaults; heat overrides six of them.
    pub fn for_material(material: Material) -> Self {
        let mut p = Self::default();
        if material == Material::Heat {
            p.speed = 1.5;
            p.scale = 0.65;
            p.inner_glow = 0.41;
            p.outer_glow = 0.49;
            p.contour = 0.46;
            p.angle = 306.0;
        }
        p
    }

    pub fn get(&self, field: Field) -> f32 {
        match field {
            Field::LightSize => self.light_size,
            Field::Haze => self.haze,
            Field::Frost => self.frost,
            Field::Backlight => self.backlight,
            Field::Thickness => self.thickness,
            Field::Speed => self.speed,
            Field::Scale => self.scale,
            Field::Distortion => self.distortion,
            Field::Contour => self.contour,
            Field::Repetition => self.repetition,
            Field::Softness => self.softness,
            Field::Dispersion => self.dispersion,
            Field::Angle => self.angle,
            Field::InnerGlow => self.inner_glow,
            Field::OuterGlow => self.outer_glow,
            Field::Noise => self.noise,
        }
    }

    pub fn set(&mut self, field: Field, value: f32) {
        let slot = match field {
            Field::LightSize => &mut self.light_size,
            Field::Haze => &mut self.haze,
            Field::Frost => &mut self.frost,
            Field::Backlight => &mut self.backlight,
            Field::Thickness => &mut self.thickness,
            Field::Speed => &mut self.speed,
            Field::Scale => &mut self.scale,
            Field::Distortion => &mut self.distortion,
            Field::Contour => &mut self.contour,
            Field::Repetition => &mut self.repetition,
            Field::Softness => &mut self.softness,
            Field::Dispersion => &mut self.dispersion,
            Field::Angle => &mut self.angle,
            Field::InnerGlow => &mut self.inner_glow,
            Field::OuterGlow => &mut self.outer_glow,
            Field::Noise => &mut self.noise,
        };
        *slot = value;
    }
}

/// Slider metadata (`fields`, main.js:60).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Field {
    LightSize,
    Haze,
    Frost,
    Backlight,
    Thickness,
    Speed,
    Scale,
    Distortion,
    Contour,
    Repetition,
    Softness,
    Dispersion,
    Angle,
    InnerGlow,
    OuterGlow,
    Noise,
}

impl Field {
    /// label, min, max, step — in that order.
    pub fn meta(self) -> (&'static str, f32, f32, f32) {
        match self {
            Field::LightSize => ("Light size", 0.25, 2.5, 0.01),
            Field::Haze => ("Atmosphere", 0.0, 1.0, 0.01),
            Field::Frost => ("Frost", 0.0, 1.0, 0.01),
            Field::Backlight => ("Backlight", 0.0, 2.0, 0.01),
            Field::Thickness => ("Thickness", 0.0, 1.0, 0.01),
            Field::Speed => ("Motion", 0.0, 1.5, 0.01),
            Field::Scale => ("Size", 0.2, 1.15, 0.01),
            Field::Distortion => ("Distortion", 0.0, 1.0, 0.01),
            Field::Contour => ("Contour", 0.0, 1.0, 0.01),
            Field::Repetition => ("Stripes", 1.0, 8.0, 0.1),
            Field::Softness => ("Softness", 0.0, 1.0, 0.01),
            Field::Dispersion => ("Iridescence", 0.0, 1.0, 0.01),
            Field::Angle => ("Direction", 0.0, 360.0, 1.0),
            Field::InnerGlow => ("Inner glow", 0.0, 1.0, 0.01),
            Field::OuterGlow => ("Outer glow", 0.0, 1.0, 0.01),
            Field::Noise => ("Grain", 0.0, 0.3, 0.005),
        }
    }

    /// Displayed value precision: Direction is an integer, the rest use 2 decimals.
    pub fn decimals(self) -> usize {
        match self {
            Field::Angle => 0,
            _ => 2,
        }
    }

    /// Which sliders a material shows, in order (main.js:61).
    pub fn for_material(material: Material) -> &'static [Field] {
        use Field::*;
        match material {
            Material::Glass => &[
                Speed, Scale, Frost, Backlight, LightSize, Haze, Thickness, Angle,
            ],
            Material::Metal => &[Speed, Scale, Distortion, Softness, Contour, Dispersion],
            Material::Heat => &[Speed, Scale, InnerGlow, OuterGlow, Contour, Angle],
        }
    }
}

/// Named presets per material (main.js:63).
pub fn preset_names(material: Material) -> [&'static str; 4] {
    match material {
        Material::Glass => ["Clear frost", "Warm frost", "Violet frost", "Mint frost"],
        _ => ["Spectrum", "Amber", "Violet", "Green"],
    }
}

const GLASS_PALETTES: [[&str; 3]; 4] = [
    ["#526068", "#e4f5ff", "#a7c6d1"],
    ["#62594c", "#fff1d4", "#dfccb2"],
    ["#575268", "#eee5ff", "#c9b9e7"],
    ["#4c625c", "#e5fff3", "#b0d9cb"],
];

/// `presetColors(index)` (main.js:37-42). Metal has its own five-stop ramp;
/// heat reuses `palettes` (six stops at index 0, four after).
pub fn preset_colors(material: Material, index: usize) -> Vec<Rgba> {
    match material {
        Material::Glass => GLASS_PALETTES[index.min(3)]
            .iter()
            .map(|s| Rgba::from_hex(s).unwrap())
            .collect(),
        Material::Metal => METAL_PALETTES[index.min(3)]
            .iter()
            .map(|s| Rgba::from_hex(s).unwrap())
            .collect(),
        Material::Heat => {
            const HEAT_PRESET_0: [&str; 6] = [
                "#171450", "#3730cf", "#9824e8", "#ff3250", "#ffac32", "#fff3b0",
            ];
            let stops: &[&str] = if index == 0 {
                &HEAT_PRESET_0
            } else {
                &HEAT_PALETTES[index.min(3)][1..]
            };
            stops.iter().map(|s| Rgba::from_hex(s).unwrap()).collect()
        }
    }
}

const METAL_PALETTES: [[&str; 5]; 4] = [
    ["#0a0a0a", "#3448ff", "#ee33a3", "#ffaf4a", "#ffffff"],
    ["#391a0c", "#995028", "#e6a14b", "#ffce7d", "#fff2d7"],
    ["#241069", "#7735ff", "#9145ff", "#d44dff", "#f1cbff"],
    ["#031b14", "#0d6041", "#269f69", "#8fdba0", "#efffe8"],
];

/// The raw `palettes` array (main.js:34) — used for heat and for the swatch
/// gradients of every material.
pub const HEAT_PALETTES: [[&str; 5]; 4] = [
    ["#fff", "#0a0a0a", "#3448ff", "#ee33a3", "#ffaf4a"],
    ["#ffe9c4", "#692326", "#ed8139", "#ffd275", "#fff4d4"],
    ["#9145ff", "#241069", "#7735ff", "#d44dff", "#f1cbff"],
    ["#62f028", "#063b28", "#0dcf74", "#99ff32", "#e8ffc1"],
];

/// Labels shown next to each colour row (main.js:44).
pub fn color_labels(material: Material, count: usize) -> Vec<String> {
    match material {
        Material::Glass => ["Glass", "Backlight", "Edge"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        Material::Metal => [
            "Deep shadow",
            "Shadow",
            "Midtone",
            "Reflection",
            "Highlight",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
        Material::Heat => (0..count).map(|i| format!("Color {}", i + 1)).collect(),
    }
}

/// Clicking a preset also snaps some metal parameters (main.js:65).
pub fn apply_preset_params(params: &mut Params, material: Material, index: usize) {
    if material != Material::Metal {
        return;
    }
    if index == 2 {
        params.angle = 315.0;
        params.repetition = 6.17;
        params.distortion = 0.69;
        params.contour = 0.48;
        params.softness = 0.8;
        params.dispersion = 0.08;
    } else {
        params.angle = 315.0;
        params.repetition = 4.35;
        params.distortion = 0.0;
        params.contour = 1.0;
        params.softness = 1.0;
        params.dispersion = 0.10;
    }
}

/// Animated speed fed to the shader clock (`motionSpeed`, main.js:54).
pub fn motion_speed(params: &Params, material: Material) -> f32 {
    match material {
        Material::Glass => params.speed * 3.0,
        _ => params.speed,
    }
}

/// `loopSettings(mode, speed, start)` from loop-export.js:16.
#[derive(Clone, Copy, Debug)]
pub struct LoopSettings {
    pub loop_on: bool,
    pub loop_phase: f32,
    pub loop_start: f32,
    pub loop_cycles: f32,
}

pub fn loop_settings(material: Material, speed: f32, start_ms: f64) -> LoopSettings {
    let cycles = if speed == 0.0 {
        0.0
    } else {
        match material {
            Material::Metal => (speed * 1.8).round().max(1.0),
            Material::Glass => (speed / 1.5).round().max(1.0),
            Material::Heat => 1.0,
        }
    };
    LoopSettings {
        loop_on: speed > 0.0,
        loop_phase: 0.0,
        loop_start: (start_ms / 1000.0) as f32,
        loop_cycles: cycles,
    }
}

/// Quality options offered in the UI.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Quality {
    K2,
    K4,
}

impl Quality {
    pub fn edge(self) -> u32 {
        match self {
            Quality::K2 => 2048,
            Quality::K4 => 4096,
        }
    }
}

/// Aspect ratios offered at the bottom of the preview.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ratio {
    Square,
    Landscape,
    Portrait,
}

impl Ratio {
    pub fn value(self) -> f32 {
        match self {
            Ratio::Square => 1.0,
            Ratio::Landscape => 1.77778,
            Ratio::Portrait => 0.5625,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Ratio::Square => "1:1",
            Ratio::Landscape => "16:9",
            Ratio::Portrait => "9:16",
        }
    }

    /// Buffer size for a quality tier: the long side is exactly `edge`.
    pub fn buffer(self, quality: Quality) -> (u32, u32) {
        let e = quality.edge();
        match self {
            Ratio::Square => (e, e),
            Ratio::Landscape => (e, (e as f32 / 1.77778).round() as u32),
            Ratio::Portrait => ((e as f32 * 0.5625).round() as u32, e),
        }
    }
}

/// GPU uniform block. Field order matches `metal.wgsl`'s `Uniforms` exactly and
/// the total size is a multiple of 16 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuUniforms {
    pub color_back: [f32; 4],
    pub color_tint: [f32; 4],
    pub glass: [f32; 4],
    pub light: [f32; 4],
    pub rim: [f32; 4],
    pub palette: [[f32; 4]; 5],
    pub colors: [[f32; 4]; 6],

    pub resolution: [f32; 2],
    pub time: f32,
    pub scale: f32,
    pub image_aspect: f32,
    pub softness: f32,
    pub repetition: f32,
    pub shift_red: f32,
    pub shift_blue: f32,
    pub distortion: f32,
    pub contour: f32,
    pub angle: f32,
    pub shape: f32,
    pub loop_phase: f32,
    pub loop_cycles: f32,
    pub loop_start: f32,
    pub loop_on: f32,
    pub is_image: f32,
    pub custom_palette: f32,
    pub frost: f32,
    pub backlight: f32,
    pub thickness: f32,
    pub haze: f32,
    pub light_size: f32,
    pub inner_glow: f32,
    pub outer_glow: f32,
    pub noise: f32,
    pub colors_count: f32,
}

const _: () = assert!(std::mem::size_of::<GpuUniforms>() == 368);

/// Everything the renderer needs for one frame.
#[derive(Clone, Debug)]
pub struct FrameRequest {
    pub material: Material,
    pub params: Params,
    pub colors: Vec<Rgba>,
    pub background: Rgba,
    pub transparent: bool,
    /// Animation clock in seconds (`u_time`).
    pub time: f32,
    /// Preset index; only the metal dispersion split depends on it.
    pub palette_index: usize,
    pub loop_settings: Option<LoopSettings>,
    /// Aspect of the uploaded processed map, for `u_imageAspectRatio`.
    pub image_aspect: f32,
    /// Output size in pixels.
    pub width: u32,
    pub height: u32,
}

impl FrameRequest {
    pub fn uniforms(&self) -> GpuUniforms {
        let mut u = <GpuUniforms as bytemuck::Zeroable>::zeroed();

        u.resolution = [self.width as f32, self.height as f32];
        u.time = self.time;
        u.scale = self.params.scale;
        u.image_aspect = self.image_aspect;
        u.softness = self.params.softness;
        u.repetition = self.params.repetition;
        u.distortion = self.params.distortion;
        u.contour = self.params.contour;
        u.angle = self.params.angle;
        u.shape = 0.0;
        u.is_image = 1.0;
        u.frost = self.params.frost;
        u.backlight = self.params.backlight;
        u.thickness = self.params.thickness;
        u.haze = self.params.haze;
        u.light_size = self.params.light_size;
        u.inner_glow = self.params.inner_glow;
        u.outer_glow = self.params.outer_glow;
        u.noise = self.params.noise;

        let loop_s = self.loop_settings.unwrap_or(LoopSettings {
            loop_on: false,
            loop_phase: 0.0,
            loop_start: 0.0,
            loop_cycles: 1.0,
        });
        u.loop_on = if loop_s.loop_on { 1.0 } else { 0.0 };
        u.loop_phase = loop_s.loop_phase;
        u.loop_cycles = loop_s.loop_cycles;
        u.loop_start = loop_s.loop_start;

        u.color_back = if self.transparent {
            [0.0, 0.0, 0.0, 0.0]
        } else {
            self.background.to_array()
        };

        u.color_tint = [1.0, 1.0, 1.0, 1.0];
        u.custom_palette = 0.0;
        u.colors_count = self.colors.len() as f32;

        match self.material {
            Material::Glass => {
                u.glass = self
                    .colors
                    .first()
                    .copied()
                    .unwrap_or(Rgba::BLACK)
                    .to_array();
                u.light = self
                    .colors
                    .get(1)
                    .copied()
                    .unwrap_or(Rgba::BLACK)
                    .to_array();
                u.rim = self
                    .colors
                    .get(2)
                    .copied()
                    .unwrap_or(Rgba::BLACK)
                    .to_array();
            }
            Material::Metal => {
                u.custom_palette = 1.0;
                for (i, c) in self.colors.iter().take(5).enumerate() {
                    u.palette[i] = c.to_array();
                }
                // main.js:53 — the split depends on the preset index.
                let (red_mul, blue_mul) = if self.palette_index == 2 {
                    (0.5, 1.5)
                } else {
                    (0.7, 1.3)
                };
                u.shift_red = self.params.dispersion * red_mul;
                u.shift_blue = self.params.dispersion * blue_mul;
            }
            Material::Heat => {
                for (i, c) in self.colors.iter().take(6).enumerate() {
                    u.colors[i] = c.to_array();
                }
            }
        }

        u
    }
}
