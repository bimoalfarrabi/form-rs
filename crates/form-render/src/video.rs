//! Seamless loop export: render `frames` frames with a closed-loop clock and
//! pipe the raw pixels to the system ffmpeg.
//!
//! Mirrors `encodeLoop` in the original web app: the animation is frozen at the
//! current time, and a loop phase sweeps a whole number of cycles over the
//! exported frames, so the last frame joins the first.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};

use crate::color::Rgba;
use crate::params::{self, FrameRequest, LoopSettings, Material, Params};
use crate::{headless_device, MapTexture, Renderer};

use std::f32::consts::TAU;

/// Round an export size down to even sides: VP9 yuv420 needs even sides.
pub fn even_size(width: u32, height: u32) -> (u32, u32) {
    (width & !1, height & !1)
}

/// Everything needed to render a loop, independent of the live editor state.
#[derive(Clone, Debug)]
pub struct LoopJob {
    pub map_rgba: Vec<u8>,
    pub map_width: u32,
    pub map_height: u32,
    pub material: Material,
    pub params: Params,
    pub colors: Vec<Rgba>,
    pub background: Rgba,
    pub transparent: bool,
    /// Animation clock frozen at this value, in seconds (`u_time`).
    pub time: f32,
    pub palette_index: usize,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub frames: u32,
    pub path: PathBuf,
}

impl LoopJob {
    pub fn image_aspect(&self) -> f32 {
        self.map_width as f32 / self.map_height.max(1) as f32
    }

    fn request(&self, loop_settings: LoopSettings) -> FrameRequest {
        FrameRequest {
            material: self.material,
            params: self.params,
            colors: self.colors.clone(),
            background: self.background,
            transparent: self.transparent,
            time: self.time,
            palette_index: self.palette_index,
            loop_settings: Some(loop_settings),
            image_aspect: self.image_aspect(),
            width: self.width,
            height: self.height,
        }
    }
}

/// A usable ffmpeg: where it is and which VP9 encoder it offers.
#[derive(Clone, Debug)]
pub struct Ffmpeg {
    pub path: PathBuf,
    pub codec: String,
}

impl Ffmpeg {
    /// Probe one binary. `None` when it cannot run or has no VP9 encoder.
    pub fn probe(path: &Path) -> Option<Self> {
        let out = Command::new(path)
            .args(["-hide_banner", "-encoders"])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&out.stdout);
        let codec = if text.contains("libvpx-vp9") {
            "libvpx-vp9"
        } else if text.contains("libvpx ") {
            "libvpx"
        } else {
            return None;
        };
        Some(Self {
            path: path.to_path_buf(),
            codec: codec.to_string(),
        })
    }
}

/// Where an ffmpeg bundled next to the app would live.
fn sidecar_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let name = if cfg!(windows) {
                "ffmpeg.exe"
            } else {
                "ffmpeg"
            };
            out.push(dir.join(name));
            out.push(dir.join("ffmpeg").join(name));
            // macOS .app: Contents/MacOS/form -> Contents/Resources/ffmpeg
            out.push(dir.join("../Resources").join(name));
        }
    }
    out
}

/// Find ffmpeg: `FORM_FFMPEG`, then a sidecar next to the app, then `PATH`.
pub fn discover() -> Option<Ffmpeg> {
    if let Some(path) = std::env::var_os("FORM_FFMPEG") {
        if let Some(found) = Ffmpeg::probe(Path::new(&path)) {
            return Some(found);
        }
    }
    for candidate in sidecar_candidates() {
        if let Some(found) = Ffmpeg::probe(&candidate) {
            return Some(found);
        }
    }
    let name = if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };
    Ffmpeg::probe(Path::new(name))
}

/// Render the loop and write `job.path`. `progress` receives 0..=100.
pub fn encode_loop(job: &LoopJob, ffmpeg: &Ffmpeg, progress: &mut dyn FnMut(u32)) -> Result<()> {
    let codec = ffmpeg.codec.clone();

    // yuv420 needs even dimensions.
    let (width, height) = even_size(job.width, job.height);
    if width == 0 || height == 0 {
        bail!("output size is too small to encode");
    }

    let tmp = job.path.with_extension("webm.part");
    let bitrate = (width as u64 * height as u64 * 3).max(8_000_000);
    let pixel_format = if job.transparent {
        "yuva420p"
    } else {
        "yuv420p"
    };

    let mut cmd = Command::new(&ffmpeg.path);
    cmd.args([
        "-y",
        "-hide_banner",
        "-loglevel",
        "error",
        "-f",
        "rawvideo",
        "-pix_fmt",
        "rgba",
        "-s",
        &format!("{width}x{height}"),
        "-r",
        &job.fps.to_string(),
        "-i",
        "-",
        "-frames:v",
        &job.frames.to_string(),
        "-c:v",
        &codec,
        "-b:v",
        &bitrate.to_string(),
        "-pix_fmt",
        pixel_format,
        // VP9 needs alt-ref disabled for an alpha channel.
        "-auto-alt-ref",
        "0",
        "-row-mt",
        "1",
        "-deadline",
        "good",
        "-cpu-used",
        "4",
    ]);
    if job.transparent {
        cmd.args(["-metadata:s:v:0", "alpha_mode=1"]);
    }
    cmd.args(["-f", "webm"]).arg(&tmp);

    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("starting ffmpeg")?;
    let mut stdin = child.stdin.take().context("ffmpeg stdin")?;

    // Drain stderr on its own thread so a chatty encoder cannot deadlock us.
    let stderr = child.stderr.take();
    let stderr_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut s) = stderr {
            let _ = std::io::Read::read_to_end(&mut s, &mut buf);
        }
        String::from_utf8_lossy(&buf).to_string()
    });

    let (device, queue) = pollster::block_on(headless_device())?;
    let mut renderer = Renderer::new(&device);
    let map: MapTexture = renderer.upload_map(
        &device,
        &queue,
        job.map_width,
        job.map_height,
        &job.map_rgba,
    )?;

    let speed = params::motion_speed(&job.params, job.material);
    let base = params::loop_settings(job.material, speed, (job.time as f64) * 1000.0);

    let mut stderr_thread = Some(stderr_thread);
    let mut take_log = || {
        stderr_thread
            .take()
            .map(|h| h.join().unwrap_or_default())
            .unwrap_or_default()
    };

    let mut failed: Option<anyhow::Error> = None;
    for i in 0..job.frames {
        let phase = TAU * i as f32 / job.frames as f32;
        let settings = LoopSettings {
            loop_phase: phase,
            ..base
        };
        let request = job.request(settings);
        let frame = renderer.render(&device, &queue, &map, &request);
        let rgba = renderer.read_frame(&device, &queue, &frame);
        let bytes = straight_alpha(&rgba, job.transparent);
        if let Err(e) = stdin.write_all(&bytes) {
            let _ = child.wait();
            let log = take_log();
            failed = Some(anyhow::anyhow!(
                "ffmpeg stopped early ({}): {}",
                e,
                log.trim()
            ));
            break;
        }
        if i % 6 == 0 {
            progress(i * 100 / job.frames);
        }
    }

    if failed.is_none() {
        stdin.flush().ok();
        drop(stdin);
        match child.wait() {
            Ok(status) if status.success() => {}
            Ok(status) => {
                let log = take_log();
                failed = Some(anyhow::anyhow!(
                    "ffmpeg failed ({}): {}",
                    status,
                    log.trim()
                ));
            }
            Err(e) => failed = Some(anyhow::anyhow!("waiting for ffmpeg: {e}")),
        }
    }

    if let Some(e) = failed {
        let _ = child.kill();
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }

    std::fs::rename(&tmp, &job.path).context("moving the encoded loop into place")?;
    progress(100);
    Ok(())
}

/// The shaders write premultiplied colour; PNG and video want straight alpha.
fn straight_alpha(rgba: &[u8], transparent: bool) -> Vec<u8> {
    if !transparent {
        // Opaque frames composite their own background, so only alpha matters.
        let mut out = rgba.to_vec();
        for px in out.as_chunks_mut::<4>().0 {
            px[3] = 255;
        }
        return out;
    }
    let mut out = Vec::with_capacity(rgba.len());
    for px in rgba.as_chunks::<4>().0 {
        let a = px[3];
        if a == 0 {
            out.extend_from_slice(&[0, 0, 0, 0]);
        } else {
            let un = |v: u8| ((v as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8;
            out.extend_from_slice(&[un(px[0]), un(px[1]), un(px[2]), a]);
        }
    }
    out
}

/// Default file name for an export of `material`.
pub fn default_file_name(material: Material) -> String {
    format!("form-{}-loop.webm", material.slug())
}

/// True when `path` looks like something we can hand to ffmpeg.
pub fn ensure_webm_extension(path: &Path) -> PathBuf {
    if path.extension().and_then(|e| e.to_str()) == Some("webm") {
        path.to_path_buf()
    } else {
        path.with_extension("webm")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fake ffmpeg that answers `-encoders` with (or without) libvpx-vp9.
    #[cfg(unix)]
    fn fake_ffmpeg(dir: &Path, name: &str, vp9: bool) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join(name);
        let body = if vp9 {
            "#!/bin/sh\nif [ \"$1\" = \"-hide_banner\" ]; then\n  echo ' V....D libvpx-vp9  libvpx VP9 (codec vp9)'\n  exit 0\nfi\nexit 1\n"
        } else {
            "#!/bin/sh\necho ' V....D mpeg4  MPEG-4'\nexit 0\n"
        };
        std::fs::write(&path, body).unwrap();
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).unwrap();
        path
    }

    #[cfg(unix)]
    #[test]
    fn probe_requires_a_vp9_encoder() {
        let dir = std::env::temp_dir().join("form-probe-test");
        let good = fake_ffmpeg(&dir, "ffmpeg-vp9", true);
        let bad = fake_ffmpeg(&dir, "ffmpeg-plain", false);
        assert!(Ffmpeg::probe(&good).is_some());
        assert!(Ffmpeg::probe(&bad).is_none());
        assert!(Ffmpeg::probe(&dir.join("missing")).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn probe_reports_the_codec_name() {
        let dir = std::env::temp_dir().join("form-probe-test");
        let good = fake_ffmpeg(&dir, "ffmpeg-vp9-name", true);
        assert_eq!(Ffmpeg::probe(&good).unwrap().codec, "libvpx-vp9");
    }

    #[test]
    fn even_size_keeps_even_sides_and_rounds_odd_down() {
        assert_eq!(even_size(2048, 1152), (2048, 1152));
        assert_eq!(even_size(1200, 630), (1200, 630));
        assert_eq!(even_size(1201, 631), (1200, 630));
        assert_eq!(even_size(3, 5), (2, 4));
        assert_eq!(even_size(1, 1), (0, 0));
    }
}
