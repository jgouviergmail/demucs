#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod audio;
mod dsp;
mod listener;
mod playback;
mod spectrogram;
mod state;
mod theme;
mod ui;
mod worker;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 750.0])
            .with_min_inner_size([700.0, 550.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };

    eframe::run_native(
        "Demucs",
        options,
        Box::new(|cc| {
            theme::apply_theme(&cc.egui_ctx);
            Ok(Box::new(app::DemucsApp::new()))
        }),
    )
}
