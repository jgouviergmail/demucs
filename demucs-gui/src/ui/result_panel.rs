use egui::{Color32, RichText, Vec2};

pub fn render(
    ui: &mut egui::Ui,
    stems: &[String],
    output_dir: &str,
    elapsed: std::time::Duration,
    new_separation: &mut bool,
) {
    ui.vertical_centered(|ui| {
        ui.add_space(24.0);

        ui.heading(
            RichText::new("S\u{e9}paration termin\u{e9}e !")
                .size(24.0)
                .strong()
                .color(Color32::from_rgb(100, 200, 100)),
        );

        ui.add_space(16.0);

        ui.label(
            RichText::new(format!("Fichiers cr\u{e9}\u{e9}s dans {}", output_dir))
                .size(13.0)
                .color(Color32::from_gray(160)),
        );

        ui.add_space(12.0);

        for stem in stems {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("\u{2713}")
                        .color(Color32::from_rgb(100, 200, 100))
                        .strong(),
                );
                ui.label(stem);
            });
        }

        ui.add_space(12.0);

        let secs = elapsed.as_secs();
        ui.label(
            RichText::new(format!("Dur\u{e9}e : {:02}:{:02}", secs / 60, secs % 60))
                .size(13.0)
                .color(Color32::from_gray(140)),
        );

        ui.add_space(24.0);

        ui.horizontal(|ui| {
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                let space = (ui.available_width() - 340.0) / 2.0;
                if space > 0.0 {
                    ui.add_space(space);
                }

                if ui
                    .add(
                        egui::Button::new(RichText::new("Ouvrir le dossier").size(14.0))
                            .min_size(Vec2::new(160.0, 35.0)),
                    )
                    .clicked()
                {
                    let _ = open_folder(output_dir);
                }

                ui.add_space(12.0);

                if ui
                    .add(
                        egui::Button::new(RichText::new("Nouvelle s\u{e9}paration").size(14.0))
                            .min_size(Vec2::new(160.0, 35.0)),
                    )
                    .clicked()
                {
                    *new_separation = true;
                }
            });
        });
    });
}

use egui::{Align, Layout};

fn open_folder(path: &str) -> std::io::Result<()> {
    let resolved = std::path::Path::new(path)
        .canonicalize()
        .unwrap_or_else(|_| std::path::PathBuf::from(path));
    let path = resolved.to_str().unwrap_or(path);

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer").arg(path).spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(path).spawn()?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open").arg(path).spawn()?;
    }
    Ok(())
}
