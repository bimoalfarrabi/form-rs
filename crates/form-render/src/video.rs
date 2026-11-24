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

const TAU: f32 = 6.283185307179586;

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

/// Is an `ffmpeg` with a VP9 encoder on this machine?
pub fn ffmpeg_status() -> Option<String> {
    let out = Command::new("ffmpeg")
        .args(["-hide_banner", "-encoders"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    if text.contains("libvpx-vp9") {
        Some("libvpx-vp9".to_string())
    } else if text.contains("libvpx ") {
        Some("libvpx".to_string())
    } else {
        None
    }
}

/// Render the loop and write `job.path`. `progress` receives 0..=100.
pub fn encode_loop(job: &LoopJob, progress: &mut dyn FnMut(u32)) -> Result<()> {
    let Some(codec) = ffmpeg_status() else {
        bail!("ffmpeg with libvpx-vp9 was not found on PATH");
    };

    // yuv420 needs even dimensions.
    let width = job.width & !1;
    let height = job.height & !1;
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

    let mut cmd = Command::new("ffmpeg");
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
        for px in out.chunks_exact_mut(4) {
            px[3] = 255;
        }
        return out;
    }
    let mut out = Vec::with_capacity(rgba.len());
    for px in rgba.chunks_exact(4) {
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
