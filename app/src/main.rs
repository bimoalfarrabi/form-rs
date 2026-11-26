#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use form_app::FormApp;

fn main() -> eframe::Result<()> {
    if std::env::args().any(|a| a == "--check-ffmpeg") {
        match form_render::video::discover() {
            Some(f) => {
                println!("{} ({})", f.path.display(), f.codec);
                return Ok(());
            }
            None => {
                eprintln!("no ffmpeg with libvpx-vp9 found");
                std::process::exit(1);
            }
        }
    }

    let shot_path = std::env::args()
        .skip(1)
        .collect::<Vec<_>>()
        .windows(2)
        .find(|w| w[0] == "--screenshot")
        .map(|w| std::path::PathBuf::from(&w[1]));

    // A positional `.form` argument opens that project on the first frame.
    let project_path = std::env::args()
        .skip(1)
        .filter(|arg| !arg.starts_with("--"))
        .find(|arg| {
            std::path::Path::new(arg)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("form"))
        })
        .map(std::path::PathBuf::from);

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([1000.0, 700.0])
            .with_title("Form — Logo materials"),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

    eframe::run_native(
        "Form",
        options,
        Box::new(move |cc| {
            let mut app = FormApp::new(cc)?;
            app.screenshot_path = shot_path;
            app.initial_project = project_path;
            Ok(Box::new(app))
        }),
    )
}
