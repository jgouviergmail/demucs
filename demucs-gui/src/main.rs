#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod audio;
mod listener;
mod state;
mod ui;
mod worker;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([600.0, 580.0])
            .with_min_inner_size([500.0, 450.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };

    eframe::run_native(
        "Demucs",
        options,
        Box::new(|_cc| Ok(Box::new(app::DemucsApp::new()))),
    )
}
