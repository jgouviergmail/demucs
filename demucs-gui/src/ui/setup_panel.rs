use std::path::PathBuf;

use egui::{Color32, CornerRadius, RichText, Stroke, StrokeKind, Vec2};
use egui_file_dialog::FileDialog;

use crate::state::{ModelChoice, SetupState};
use crate::theme;

const AUDIO_EXTENSIONS: &[&str] = &["wav", "mp3", "flac", "ogg", "m4a", "aac", "aiff", "aif"];

pub fn render(
    ui: &mut egui::Ui,
    setup: &mut SetupState,
    file_dialog: &mut FileDialog,
    folder_dialog: &mut FileDialog,
    start_requested: &mut bool,
) {
    ui.vertical_centered(|ui| {
        ui.add_space(8.0);
        ui.heading(RichText::new("DEMUCS").size(28.0).strong().color(theme::ACCENT_CORAL));
        ui.label(
            RichText::new("S\u{e9}paration de sources audio")
                .size(14.0)
                .color(theme::TEXT_DIM),
        );
        ui.add_space(16.0);
    });

    // Drop zone
    let drop_zone_height = 100.0;
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), drop_zone_height),
        egui::Sense::click(),
    );

    let is_hovering = !ui.ctx().input(|i| i.raw.hovered_files.is_empty());
    let stroke_color = if is_hovering {
        theme::ACCENT_PURPLE
    } else {
        theme::BORDER
    };
    let fill = if is_hovering {
        Color32::from_rgba_premultiplied(0x7c, 0x6f, 0xf0, 10)
    } else {
        Color32::TRANSPARENT
    };

    ui.painter().rect(
        rect,
        CornerRadius::same(8),
        fill,
        Stroke::new(2.0, stroke_color),
        StrokeKind::Outside,
    );

    ui.painter().text(
        rect.center() - Vec2::new(0.0, 12.0),
        egui::Align2::CENTER_CENTER,
        "Glissez un fichier audio ici\nou cliquez pour parcourir",
        egui::FontId::proportional(15.0),
        theme::TEXT_DIM,
    );
    ui.painter().text(
        rect.center() + Vec2::new(0.0, 20.0),
        egui::Align2::CENTER_CENTER,
        "WAV \u{b7} MP3 \u{b7} FLAC \u{b7} OGG \u{b7} M4A \u{b7} AIFF",
        egui::FontId::proportional(11.0),
        theme::BORDER,
    );

    if response.clicked() {
        file_dialog.pick_file();
    }

    // Handle dropped files
    let dropped = ui.ctx().input(|i| i.raw.dropped_files.clone());
    if let Some(file) = dropped.first() {
        if let Some(path) = &file.path {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if AUDIO_EXTENSIONS.contains(&ext.to_lowercase().as_str()) {
                    setup.input_path = Some(path.clone());
                }
            }
        }
    }

    ui.add_space(8.0);

    // Show selected file
    if let Some(name) = setup.input_filename() {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Fichier :").strong());
            ui.label(&name);
        });
    }

    ui.add_space(12.0);

    // Model selection
    ui.label(RichText::new("Mod\u{e8}le").size(16.0).strong());
    ui.add_space(4.0);

    let prev_model = setup.model_choice;
    for choice in [ModelChoice::HtDemucs, ModelChoice::HtDemucs6s, ModelChoice::HtDemucsFt] {
        ui.horizontal(|ui| {
            ui.radio_value(&mut setup.model_choice, choice, choice.label());
            ui.label(
                RichText::new(choice.description())
                    .size(12.0)
                    .color(theme::TEXT_DIM),
            );
        });
    }

    if setup.model_choice != prev_model {
        setup.update_stems_for_model();
    }

    ui.add_space(12.0);

    // Stem selection
    ui.label(RichText::new("Stems \u{e0} extraire").size(16.0).strong());
    ui.add_space(4.0);

    ui.horizontal_wrapped(|ui| {
        for (stem_id, enabled) in setup.stem_selection.iter_mut() {
            let label = match stem_id.as_str() {
                "drums" => "Batterie",
                "bass" => "Basse",
                "vocals" => "Voix",
                "other" => "Autre",
                "guitar" => "Guitare",
                "piano" => "Piano",
                _ => stem_id.as_str(),
            };
            ui.checkbox(enabled, label);
        }
    });

    ui.add_space(12.0);

    // Output directory
    ui.label(RichText::new("Dossier de sortie").size(16.0).strong());
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        let mut dir_str = setup.output_dir.display().to_string();
        let response = ui.add(
            egui::TextEdit::singleline(&mut dir_str)
                .desired_width(ui.available_width() - 80.0),
        );
        if response.changed() {
            setup.output_dir = PathBuf::from(&dir_str);
        }
        if ui.button("Parcourir").clicked() {
            folder_dialog.pick_directory();
        }
    });

    ui.add_space(12.0);

    // Trim silence option
    ui.label(RichText::new("Post-traitement").size(16.0).strong());
    ui.add_space(4.0);

    ui.checkbox(
        &mut setup.trim.enabled,
        "Trimmer les silences (vocals)",
    );

    if setup.trim.enabled {
        ui.add_space(4.0);
        ui.indent("trim_settings", |ui| {
            ui.horizontal(|ui| {
                ui.label("Silence min. pour trimmer :");
                ui.add(
                    egui::DragValue::new(&mut setup.trim.min_silence_secs)
                        .range(0.5..=10.0)
                        .speed(0.1)
                        .suffix(" s"),
                );
            });
            ui.horizontal(|ui| {
                ui.label("Silence de remplacement :");
                ui.add(
                    egui::DragValue::new(&mut setup.trim.replacement_secs)
                        .range(0.0..=5.0)
                        .speed(0.1)
                        .suffix(" s"),
                );
            });
            ui.horizontal(|ui| {
                ui.label("Seuil de silence :");
                ui.add(
                    egui::DragValue::new(&mut setup.trim.threshold_db)
                        .range(-80.0..=-10.0)
                        .speed(0.5)
                        .suffix(" dB"),
                );
            });
        });
    }

    ui.add_space(20.0);

    // Start button
    ui.vertical_centered(|ui| {
        let can_start = setup.can_start();
        let button = egui::Button::new(
            RichText::new("Lancer la s\u{e9}paration")
                .size(16.0)
                .strong(),
        )
        .min_size(Vec2::new(300.0, 40.0));

        if ui.add_enabled(can_start, button).clicked() {
            *start_requested = true;
        }
    });
}
