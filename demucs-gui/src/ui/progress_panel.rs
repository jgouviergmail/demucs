use egui::{Color32, RichText, Vec2};

use crate::state::AppPhase;

pub fn render(
    ui: &mut egui::Ui,
    phase: &AppPhase,
    input_name: &str,
    model_label: &str,
    elapsed: std::time::Duration,
    cancel_requested: &mut bool,
) {
    ui.vertical_centered(|ui| {
        ui.add_space(16.0);

        let title = match phase {
            AppPhase::Downloading { .. } => "T\u{e9}l\u{e9}chargement du mod\u{e8}le...",
            AppPhase::LoadingModel => "Chargement du mod\u{e8}le...",
            AppPhase::WritingOutput => "\u{c9}criture des fichiers...",
            _ => "S\u{e9}paration en cours...",
        };

        ui.heading(RichText::new(title).size(22.0).strong());
        ui.add_space(12.0);

        ui.horizontal(|ui| {
            ui.label(RichText::new("Fichier :").strong());
            ui.label(input_name);
            ui.label(RichText::new("\u{b7}").color(Color32::GRAY));
            ui.label(RichText::new("Mod\u{e8}le :").strong());
            ui.label(model_label);
        });

        ui.add_space(16.0);

        match phase {
            AppPhase::Downloading { progress, status } => {
                ui.label(
                    RichText::new(status)
                        .size(13.0)
                        .color(Color32::from_gray(160)),
                );
                ui.add_space(8.0);
                ui.add(
                    egui::ProgressBar::new(*progress)
                        .show_percentage()
                        .animate(true),
                );
            }
            AppPhase::Separating {
                progress,
                chunk_info,
                step_description,
                ..
            } => {
                let status = if chunk_info.is_empty() {
                    step_description.clone()
                } else {
                    format!("{} \u{2014} {}", chunk_info, step_description)
                };
                ui.label(
                    RichText::new(status)
                        .size(13.0)
                        .color(Color32::from_gray(160)),
                );
                ui.add_space(8.0);
                ui.add(
                    egui::ProgressBar::new(*progress)
                        .show_percentage()
                        .animate(true),
                );
            }
            AppPhase::LoadingModel | AppPhase::WritingOutput => {
                ui.spinner();
            }
            _ => {}
        }

        ui.add_space(12.0);

        let secs = elapsed.as_secs();
        ui.label(
            RichText::new(format!("Temps \u{e9}coul\u{e9} : {:02}:{:02}", secs / 60, secs % 60))
                .size(13.0)
                .color(Color32::from_gray(140)),
        );

        ui.add_space(20.0);

        if ui
            .add(
                egui::Button::new(RichText::new("Annuler").size(14.0))
                    .min_size(Vec2::new(150.0, 35.0)),
            )
            .clicked()
        {
            *cancel_requested = true;
        }
    });
}
