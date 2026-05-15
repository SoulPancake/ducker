mod app;
mod audio;
mod dsp;
mod meter;
mod params;

use app::DuckerApp;

fn main() -> eframe::Result<()> {
    let viewport = egui::ViewportBuilder::default()
        .with_inner_size([820.0, 560.0])
        .with_resizable(false)
        .with_maximize_button(false)
        .with_title("DUCKER");

    let options = eframe::NativeOptions {
        viewport,
        centered: true,
        ..Default::default()
    };

    eframe::run_native(
        "DUCKER",
        options,
        Box::new(|cc| Ok(Box::new(DuckerApp::new(cc)))),
    )
}
