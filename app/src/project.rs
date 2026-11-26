//! The `.form` project file: the editor's state plus the original logo bytes.
//!
//! JSON with an explicit version. Unknown fields are ignored so a newer file
//! still opens where it can, but a different `version` is refused rather than
//! guessed at.

use std::path::Path;

use anyhow::{bail, Context, Result};
use base64::Engine as _;
use form_render::color::Rgba;
use form_render::params::{Material, Params, Quality, Ratio};
use serde::{Deserialize, Serialize};

pub const EXTENSION: &str = "form";
const VERSION: u32 = 1;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Project {
    pub version: u32,
    pub material: String,
    pub params: Vec<ParamsDto>,
    pub presets: Vec<usize>,
    pub colors: Vec<Option<Vec<String>>>,
    pub background: String,
    pub transparent: bool,
    pub quality: String,
    pub ratio: String,
    pub logo: LogoDto,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ParamsDto {
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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum LogoDto {
    /// The bundled Form mark.
    Default,
    /// The bytes the user opened, so reopening restores exactly what they see.
    Embedded { name: String, data: String },
}

impl ParamsDto {
    pub fn from_params(p: &Params) -> Self {
        Self {
            light_size: p.light_size,
            haze: p.haze,
            frost: p.frost,
            backlight: p.backlight,
            thickness: p.thickness,
            speed: p.speed,
            scale: p.scale,
            distortion: p.distortion,
            contour: p.contour,
            repetition: p.repetition,
            softness: p.softness,
            dispersion: p.dispersion,
            angle: p.angle,
            inner_glow: p.inner_glow,
            outer_glow: p.outer_glow,
            noise: p.noise,
        }
    }

    pub fn to_params(&self) -> Params {
        Params {
            light_size: self.light_size,
            haze: self.haze,
            frost: self.frost,
            backlight: self.backlight,
            thickness: self.thickness,
            speed: self.speed,
            scale: self.scale,
            distortion: self.distortion,
            contour: self.contour,
            repetition: self.repetition,
            softness: self.softness,
            dispersion: self.dispersion,
            angle: self.angle,
            inner_glow: self.inner_glow,
            outer_glow: self.outer_glow,
            noise: self.noise,
        }
    }
}

pub fn material_slug(material: Material) -> &'static str {
    material.slug()
}

pub fn material_from_slug(slug: &str) -> Option<Material> {
    match slug {
        "metal" => Some(Material::Metal),
        "heat" => Some(Material::Heat),
        "glass" => Some(Material::Glass),
        _ => None,
    }
}

pub fn quality_slug(quality: Quality) -> &'static str {
    match quality {
        Quality::K2 => "2k",
        Quality::K4 => "4k",
    }
}

pub fn quality_from_slug(slug: &str) -> Option<Quality> {
    match slug {
        "2k" => Some(Quality::K2),
        "4k" => Some(Quality::K4),
        _ => None,
    }
}

pub fn ratio_slug(ratio: Ratio) -> &'static str {
    match ratio {
        Ratio::Square => "1:1",
        Ratio::Landscape => "16:9",
        Ratio::Portrait => "9:16",
    }
}

pub fn ratio_from_slug(slug: &str) -> Option<Ratio> {
    match slug {
        "1:1" => Some(Ratio::Square),
        "16:9" => Some(Ratio::Landscape),
        "9:16" => Some(Ratio::Portrait),
        _ => None,
    }
}

impl Project {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        material: Material,
        params: [Params; 3],
        presets: [usize; 3],
        colors: [Option<Vec<Rgba>>; 3],
        background: Rgba,
        transparent: bool,
        quality: Quality,
        ratio: Ratio,
        logo: LogoDto,
    ) -> Self {
        Self {
            version: VERSION,
            material: material_slug(material).to_string(),
            params: params.iter().map(ParamsDto::from_params).collect(),
            presets: presets.to_vec(),
            colors: colors
                .iter()
                .map(|set| {
                    set.as_ref()
                        .map(|list| list.iter().map(|c| c.to_hex()).collect())
                })
                .collect(),
            background: background.to_hex(),
            transparent,
            quality: quality_slug(quality).to_string(),
            ratio: ratio_slug(ratio).to_string(),
            logo,
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let text = serde_json::to_string_pretty(self).context("serialising the project")?;
        std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))
    }

    pub fn open(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let project: Project =
            serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        if project.version != VERSION {
            bail!(
                "this project was saved by a different version of Form (file {}, expected {})",
                project.version,
                VERSION
            );
        }
        if project.params.len() != 3 || project.presets.len() != 3 || project.colors.len() != 3 {
            bail!("the project is missing per-material settings");
        }
        Ok(project)
    }

    /// Decode the embedded logo, if any.
    pub fn logo_bytes(&self) -> Result<Option<(String, Vec<u8>)>> {
        match &self.logo {
            LogoDto::Default => Ok(None),
            LogoDto::Embedded { name, data } => {
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(data)
                    .context("the embedded logo is not valid base64")?;
                Ok(Some((name.clone(), bytes)))
            }
        }
    }

    pub fn embed_logo(name: &str, bytes: &[u8]) -> LogoDto {
        LogoDto::Embedded {
            name: name.to_string(),
            data: base64::engine::general_purpose::STANDARD.encode(bytes),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Project {
        Project::new(
            Material::Glass,
            [
                Params::for_material(Material::Metal),
                Params::for_material(Material::Heat),
                Params::for_material(Material::Glass),
            ],
            [2, 1, 0],
            [None, Some(vec![Rgba::from_hex("#0a0a0a").unwrap()]), None],
            Rgba::from_hex("#141414").unwrap(),
            true,
            Quality::K4,
            Ratio::Portrait,
            Project::embed_logo("logo.svg", b"<svg/>"),
        )
    }

    #[test]
    fn round_trips_through_json() {
        let project = sample();
        let text = serde_json::to_string(&project).unwrap();
        let back: Project = serde_json::from_str(&text).unwrap();
        assert_eq!(project, back);
        let (name, bytes) = back.logo_bytes().unwrap().unwrap();
        assert_eq!(name, "logo.svg");
        assert_eq!(bytes, b"<svg/>");
    }

    #[test]
    fn rejects_unknown_versions() {
        let mut value: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&sample()).unwrap()).unwrap();
        value["version"] = serde_json::json!(99);
        let dir = std::env::temp_dir().join("form-project-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("future.form");
        std::fs::write(&path, value.to_string()).unwrap();
        let err = Project::open(&path).unwrap_err().to_string();
        assert!(err.contains("different version"), "{err}");
    }

    #[test]
    fn rejects_incomplete_material_sets() {
        let mut value: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&sample()).unwrap()).unwrap();
        value["presets"] = serde_json::json!([0]);
        let dir = std::env::temp_dir().join("form-project-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("incomplete.form");
        std::fs::write(&path, value.to_string()).unwrap();
        let err = Project::open(&path).unwrap_err().to_string();
        assert!(err.contains("per-material"), "{err}");
    }

    #[test]
    fn sliders_survive_the_round_trip() {
        let project = sample();
        let dto: Vec<ParamsDto> =
            serde_json::from_str(&serde_json::to_string(&project.params).unwrap()).unwrap();
        let params = dto[0].to_params();
        assert!((params.repetition - 4.35).abs() < 1e-6);
        assert_eq!(params.angle, 315.0);
    }
}
