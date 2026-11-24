use form_app::FormApp;

fn main() -> eframe::Result<()> {
    let shot_path = std::env::args()
        .skip(1)
        .collect::<Vec<_>>()
        .windows(2)
        .find(|w| w[0] == "--screenshot")
        .map(|w| std::path::PathBuf::from(&w[1]));

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
            Ok(Box::new(app))
        }),
    )
}
