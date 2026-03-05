use egui::{Align, Color32, CornerRadius, Layout, Rect, RichText, Stroke, Vec2};

use crate::audio::{self, ReverbParams, StemMixParam};
use crate::playback::PlaybackEngine;
use crate::spectrogram::SpectrogramManager;
use crate::state::SeparationResult;
use crate::theme;

const SPECTROGRAM_HEIGHT: f32 = 70.0;
const INPUT_SPECTROGRAM_HEIGHT: f32 = 90.0;

pub fn render(
    ui: &mut egui::Ui,
    result: &SeparationResult,
    elapsed: std::time::Duration,
    spectrograms: &mut SpectrogramManager,
    playback: Option<&PlaybackEngine>,
    new_separation: &mut bool,
    export_status: &mut String,
) {
    spectrograms.poll(ui.ctx());

    // Keyboard shortcuts (only when no text field is focused)
    if !ui.ctx().wants_keyboard_input() {
        if let Some(pb) = playback {
            ui.ctx().input(|i| {
                if i.key_pressed(egui::Key::Space) {
                    pb.toggle_play_pause();
                }
                if i.key_pressed(egui::Key::ArrowRight) {
                    pb.seek_relative_secs(5.0);
                }
                if i.key_pressed(egui::Key::ArrowLeft) {
                    pb.seek_relative_secs(-5.0);
                }
            });
        }
    }

    let position_frac = playback.map(|p| p.position_fraction()).unwrap_or(0.0);
    let position_secs = playback.map(|p| p.position_secs()).unwrap_or(0.0);
    let duration_secs = playback.map(|p| p.duration_secs()).unwrap_or(0.0);
    let is_playing = playback.map(|p| p.is_playing()).unwrap_or(false);

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.add_space(12.0);

        // Header
        ui.vertical_centered(|ui| {
            let secs = elapsed.as_secs();
            ui.heading(
                RichText::new(format!(
                    "S\u{e9}paration termin\u{e9}e ! \u{2014} {:02}:{:02}",
                    secs / 60,
                    secs % 60
                ))
                .size(18.0)
                .strong()
                .color(theme::SUCCESS),
            );

            // Spectrogram loading progress
            if !spectrograms.is_complete() {
                let (done, total) = spectrograms.progress();
                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!("Spectrogrammes : {}/{}", done, total))
                        .size(11.0)
                        .color(theme::TEXT_DIM),
                );
            }
        });

        ui.add_space(8.0);

        // Input spectrogram
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Input")
                    .size(13.0)
                    .strong()
                    .color(theme::TEXT_DIM),
            );
        });
        render_spectrogram(
            ui,
            spectrograms.get("Input"),
            INPUT_SPECTROGRAM_HEIGHT,
            position_frac,
            playback,
        );

        ui.add_space(6.0);

        // Transport controls
        ui.horizontal(|ui| {
            if let Some(pb) = playback {
                // Play/pause button
                let icon = if is_playing { "\u{23f8}" } else { "\u{25b6}" };
                let btn = egui::Button::new(
                    RichText::new(icon).size(16.0).color(theme::ACCENT_CORAL),
                )
                .min_size(Vec2::new(32.0, 26.0));
                if ui.add(btn).clicked() {
                    pb.toggle_play_pause();
                }

                // Time display
                ui.label(
                    RichText::new(format_time(position_secs))
                        .size(11.0)
                        .color(theme::TEXT),
                );

                // Seek slider
                let mut frac = position_frac;
                let slider = egui::Slider::new(&mut frac, 0.0..=1.0)
                    .show_value(false)
                    .trailing_fill(true);
                let resp = ui.add(slider);
                if resp.drag_stopped() || (resp.changed() && !resp.dragged()) {
                    pb.seek_fraction(frac);
                }

                ui.label(
                    RichText::new(format_time(duration_secs))
                        .size(11.0)
                        .color(theme::TEXT_DIM),
                );

                ui.add_space(8.0);

                // Master volume
                ui.label(RichText::new("\u{1f50a}").size(13.0));
                let mut master = pb.master_gain();
                if ui
                    .add(
                        egui::Slider::new(&mut master, 0.0..=1.5)
                            .show_value(false)
                            .trailing_fill(true),
                    )
                    .changed()
                {
                    pb.set_master_gain(master);
                }
            } else {
                ui.label(
                    RichText::new("Sortie audio non disponible")
                        .size(12.0)
                        .color(theme::ERROR),
                );
            }
        });

        ui.add_space(6.0);

        // Keyboard hint
        if playback.is_some() {
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new("Espace : lecture \u{b7} \u{2190}\u{2192} : \u{b1}5s \u{b7} Clic spectrogramme : seek")
                        .size(10.0)
                        .color(theme::BORDER),
                );
            });
        }

        ui.add_space(4.0);
        ui.separator();
        ui.add_space(4.0);

        // Per-stem rows
        for stem in &result.stem_audio {
            let color = theme::stem_color(stem.id);
            let label = stem_label(stem.id);

            // Line 1: controls
            ui.horizontal(|ui| {
                // Colored dot
                let (dot_rect, _) =
                    ui.allocate_exact_size(Vec2::new(12.0, 12.0), egui::Sense::hover());
                ui.painter()
                    .circle_filled(dot_rect.center(), 6.0, color);

                // Stem name (fixed width)
                ui.allocate_ui(Vec2::new(70.0, 20.0), |ui| {
                    ui.label(
                        RichText::new(label)
                            .size(13.0)
                            .strong()
                            .color(color),
                    );
                });

                if let Some(pb) = playback {
                    let is_soloed = pb.is_soloed(stem.id);
                    let is_muted = pb.is_muted(stem.id);

                    // Solo button
                    let (solo_bg, solo_fg) = if is_soloed {
                        (theme::ACCENT_CORAL, theme::BG)
                    } else {
                        (theme::SURFACE2, theme::TEXT_DIM)
                    };
                    let solo_btn = egui::Button::new(
                        RichText::new("S").size(11.0).strong().color(solo_fg),
                    )
                    .fill(solo_bg)
                    .min_size(Vec2::new(22.0, 20.0))
                    .corner_radius(CornerRadius::same(3));
                    if ui.add(solo_btn).clicked() {
                        pb.toggle_solo(stem.id);
                    }

                    // Mute button
                    let (mute_bg, mute_fg) = if is_muted {
                        (theme::ERROR, theme::BG)
                    } else {
                        (theme::SURFACE2, theme::TEXT_DIM)
                    };
                    let mute_btn = egui::Button::new(
                        RichText::new("M").size(11.0).strong().color(mute_fg),
                    )
                    .fill(mute_bg)
                    .min_size(Vec2::new(22.0, 20.0))
                    .corner_radius(CornerRadius::same(3));
                    if ui.add(mute_btn).clicked() {
                        pb.toggle_mute(stem.id);
                    }

                    ui.add_space(8.0);

                    // Gain slider
                    let mut gain = pb.gain(stem.id);
                    let gain_slider = egui::Slider::new(&mut gain, 0.0..=2.0)
                        .show_value(false)
                        .trailing_fill(true);
                    if ui.add(gain_slider).changed() {
                        pb.set_gain(stem.id, gain);
                    }

                    // Gain value display
                    ui.label(
                        RichText::new(format!("{:.0}%", gain * 100.0))
                            .size(10.0)
                            .color(theme::TEXT_DIM),
                    );
                }
            });

            // Collapsible effects panel
            if let Some(pb) = playback {
                render_fx_panel(ui, stem.id, pb);
            }

            // Spectrogram
            let label_key = stem.id.as_str();
            render_spectrogram(
                ui,
                spectrograms.get(label_key),
                SPECTROGRAM_HEIGHT,
                position_frac,
                playback,
            );

            ui.add_space(4.0);
        }

        ui.add_space(4.0);

        // Global reverb controls
        if let Some(pb) = playback {
            ui.separator();
            ui.add_space(4.0);
            render_global_reverb(ui, pb);
            ui.add_space(4.0);
        }

        ui.separator();
        ui.add_space(4.0);

        // Export mix button
        if let Some(pb) = playback {
            ui.vertical_centered(|ui| {
                let export_btn = egui::Button::new(
                    RichText::new("Exporter le mix").size(14.0).color(theme::ACCENT_CORAL),
                )
                .min_size(Vec2::new(200.0, 32.0));

                if ui.add(export_btn).clicked() {
                    *export_status = do_export_mix(result, pb);
                }

                if !export_status.is_empty() {
                    ui.add_space(4.0);
                    let color = if export_status.starts_with("OK") {
                        theme::SUCCESS
                    } else {
                        theme::ERROR
                    };
                    ui.label(RichText::new(export_status.as_str()).size(11.0).color(color));
                }
            });
        }

        ui.add_space(6.0);
        ui.separator();
        ui.add_space(6.0);

        // Bottom controls
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
                    let _ = open_folder(&result.output_dir);
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

        ui.add_space(6.0);

        // File list
        ui.vertical_centered(|ui| {
            let files = result.stems_written.join(", ");
            ui.label(
                RichText::new(files)
                    .size(11.0)
                    .color(theme::TEXT_DIM),
            );
        });

        ui.add_space(12.0);
    });
}

// ---------------------------------------------------------------------------
// Per-stem collapsible effects panel
// ---------------------------------------------------------------------------

fn render_fx_panel(
    ui: &mut egui::Ui,
    stem_id: demucs_core::model::metadata::StemId,
    pb: &PlaybackEngine,
) {
    egui::CollapsingHeader::new(
        RichText::new("Effets").size(11.0).color(theme::TEXT_DIM),
    )
    .id_salt(format!("fx_{}", stem_id.as_str()))
    .default_open(false)
    .show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 3.0;

        // Noise Gate
        let mut gate_on = pb.is_gate_enabled(stem_id);
        if ui
            .checkbox(&mut gate_on, RichText::new("Gate").size(11.0))
            .changed()
        {
            pb.set_gate_enabled(stem_id, gate_on);
        }
        if gate_on {
            ui.indent(format!("gate_indent_{}", stem_id.as_str()), |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Seuil").size(10.0).color(theme::TEXT_DIM));
                    let mut threshold = pb.gate_threshold(stem_id);
                    if ui
                        .add(
                            egui::Slider::new(&mut threshold, -60.0..=-20.0)
                                .show_value(false),
                        )
                        .changed()
                    {
                        pb.set_gate_threshold(stem_id, threshold);
                    }
                    ui.label(
                        RichText::new(format!("{:.0} dB", threshold))
                            .size(10.0)
                            .color(theme::TEXT_DIM),
                    );
                });
            });
        }

        // Soft limiter
        let mut limiter_on = pb.is_limiter_enabled(stem_id);
        if ui
            .checkbox(
                &mut limiter_on,
                RichText::new("Limiter").size(11.0),
            )
            .on_hover_text("Saturation douce (tanh) — arrondit les crêtes au lieu de les couper")
            .changed()
        {
            pb.set_limiter_enabled(stem_id, limiter_on);
        }

        // Pan
        ui.horizontal(|ui| {
            ui.label(RichText::new("Pan").size(11.0).color(theme::TEXT_DIM));
            ui.label(RichText::new("L").size(10.0).color(theme::TEXT_DIM));
            let mut pan = pb.pan(stem_id);
            if ui
                .add(
                    egui::Slider::new(&mut pan, -1.0..=1.0)
                        .show_value(false)
                        .trailing_fill(false),
                )
                .changed()
            {
                pb.set_pan(stem_id, pan);
            }
            ui.label(RichText::new("R").size(10.0).color(theme::TEXT_DIM));
            ui.label(
                RichText::new(format!("{:.1}", pan))
                    .size(10.0)
                    .color(theme::TEXT_DIM),
            );
        });

        // Phase invert
        let mut phase = pb.is_phase_inverted(stem_id);
        if ui
            .checkbox(&mut phase, RichText::new("Phase invers\u{e9}e").size(11.0))
            .changed()
        {
            pb.set_phase_invert(stem_id, phase);
        }

        // EQ
        let mut eq_on = pb.is_eq_enabled(stem_id);
        if ui
            .checkbox(&mut eq_on, RichText::new("EQ").size(11.0))
            .changed()
        {
            pb.set_eq_enabled(stem_id, eq_on);
        }
        if eq_on {
            ui.indent(format!("eq_indent_{}", stem_id.as_str()), |ui| {
                let mut low = pb.eq_low(stem_id);
                let mut mid = pb.eq_mid(stem_id);
                let mut high = pb.eq_high(stem_id);

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Grave").size(10.0).color(theme::TEXT_DIM));
                    if ui
                        .add(egui::Slider::new(&mut low, -12.0..=12.0).show_value(false))
                        .changed()
                    {
                        pb.set_eq_low(stem_id, low);
                    }
                    ui.label(
                        RichText::new(format!("{:+.0} dB", low))
                            .size(10.0)
                            .color(theme::TEXT_DIM),
                    );
                });

                ui.horizontal(|ui| {
                    ui.label(RichText::new("M\u{e9}dium").size(10.0).color(theme::TEXT_DIM));
                    if ui
                        .add(egui::Slider::new(&mut mid, -12.0..=12.0).show_value(false))
                        .changed()
                    {
                        pb.set_eq_mid(stem_id, mid);
                    }
                    ui.label(
                        RichText::new(format!("{:+.0} dB", mid))
                            .size(10.0)
                            .color(theme::TEXT_DIM),
                    );
                });

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Aigu").size(10.0).color(theme::TEXT_DIM));
                    if ui
                        .add(egui::Slider::new(&mut high, -12.0..=12.0).show_value(false))
                        .changed()
                    {
                        pb.set_eq_high(stem_id, high);
                    }
                    ui.label(
                        RichText::new(format!("{:+.0} dB", high))
                            .size(10.0)
                            .color(theme::TEXT_DIM),
                    );
                });
            });
        }

        // Reverb send
        ui.horizontal(|ui| {
            ui.label(RichText::new("Reverb").size(11.0).color(theme::TEXT_DIM));
            let mut send = pb.reverb_send(stem_id);
            if ui
                .add(
                    egui::Slider::new(&mut send, 0.0..=1.0)
                        .show_value(false)
                        .trailing_fill(true),
                )
                .changed()
            {
                pb.set_reverb_send(stem_id, send);
            }
            ui.label(
                RichText::new(format!("{:.0}%", send * 100.0))
                    .size(10.0)
                    .color(theme::TEXT_DIM),
            );
        });

        // Delay
        let mut delay_on = pb.is_delay_enabled(stem_id);
        if ui
            .checkbox(&mut delay_on, RichText::new("Delay").size(11.0))
            .changed()
        {
            pb.set_delay_enabled(stem_id, delay_on);
        }
        if delay_on {
            ui.indent(format!("delay_indent_{}", stem_id.as_str()), |ui| {
                let mut send = pb.delay_send(stem_id);
                let mut time = pb.delay_time(stem_id);
                let mut feedback = pb.delay_feedback(stem_id);

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Send").size(10.0).color(theme::TEXT_DIM));
                    if ui
                        .add(
                            egui::Slider::new(&mut send, 0.0..=1.0)
                                .show_value(false)
                                .trailing_fill(true),
                        )
                        .changed()
                    {
                        pb.set_delay_send(stem_id, send);
                    }
                    ui.label(
                        RichText::new(format!("{:.0}%", send * 100.0))
                            .size(10.0)
                            .color(theme::TEXT_DIM),
                    );
                });

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Temps").size(10.0).color(theme::TEXT_DIM));
                    if ui
                        .add(egui::Slider::new(&mut time, 10.0..=1000.0).show_value(false))
                        .changed()
                    {
                        pb.set_delay_time(stem_id, time);
                    }
                    ui.label(
                        RichText::new(format!("{:.0} ms", time))
                            .size(10.0)
                            .color(theme::TEXT_DIM),
                    );
                });

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Feedback").size(10.0).color(theme::TEXT_DIM));
                    if ui
                        .add(
                            egui::Slider::new(&mut feedback, 0.0..=0.95)
                                .show_value(false)
                                .trailing_fill(true),
                        )
                        .changed()
                    {
                        pb.set_delay_feedback(stem_id, feedback);
                    }
                    ui.label(
                        RichText::new(format!("{:.0}%", feedback * 100.0))
                            .size(10.0)
                            .color(theme::TEXT_DIM),
                    );
                });
            });
        }
    });
}

// ---------------------------------------------------------------------------
// Global reverb controls
// ---------------------------------------------------------------------------

fn render_global_reverb(ui: &mut egui::Ui, pb: &PlaybackEngine) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Reverb globale")
                .size(12.0)
                .strong()
                .color(theme::TEXT_DIM),
        );

        ui.add_space(8.0);

        ui.label(RichText::new("Decay").size(10.0).color(theme::TEXT_DIM));
        let mut decay = pb.reverb_decay();
        if ui
            .add(
                egui::Slider::new(&mut decay, 0.5..=5.0)
                    .show_value(false)
                    .trailing_fill(true),
            )
            .changed()
        {
            pb.set_reverb_decay(decay);
        }
        ui.label(
            RichText::new(format!("{:.1}s", decay))
                .size(10.0)
                .color(theme::TEXT_DIM),
        );

        ui.add_space(8.0);

        ui.label(RichText::new("Damping").size(10.0).color(theme::TEXT_DIM));
        let mut damping = pb.reverb_damping();
        if ui
            .add(
                egui::Slider::new(&mut damping, 0.0..=1.0)
                    .show_value(false)
                    .trailing_fill(true),
            )
            .changed()
        {
            pb.set_reverb_damping(damping);
        }
        ui.label(
            RichText::new(format!("{:.0}%", damping * 100.0))
                .size(10.0)
                .color(theme::TEXT_DIM),
        );
    });
}

// ---------------------------------------------------------------------------
// Spectrogram rendering
// ---------------------------------------------------------------------------

fn render_spectrogram(
    ui: &mut egui::Ui,
    entry: Option<&crate::spectrogram::SpectrogramEntry>,
    height: f32,
    position_frac: f32,
    playback: Option<&PlaybackEngine>,
) {
    let available_width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(available_width, height),
        egui::Sense::click(),
    );

    let rounding = CornerRadius::same(4);

    if let Some(entry) = entry {
        let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        ui.painter().rect_filled(rect, rounding, Color32::BLACK);
        ui.painter().image(entry.texture.id(), rect, uv, Color32::WHITE);
    } else {
        ui.painter().rect_filled(rect, rounding, theme::SURFACE2);
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "Calcul...",
            egui::FontId::proportional(11.0),
            theme::TEXT_DIM,
        );
    }

    // Playback cursor
    let x = rect.left() + position_frac * rect.width();
    if position_frac > 0.001 {
        // Glow
        ui.painter().line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            Stroke::new(3.0, Color32::from_rgba_premultiplied(255, 255, 255, 40)),
        );
        // Cursor
        ui.painter().line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            Stroke::new(1.0, Color32::WHITE),
        );
    }

    // Click to seek
    if response.clicked() {
        if let Some(pb) = playback {
            if let Some(pos) = response.interact_pointer_pos() {
                let frac = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                pb.seek_fraction(frac);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn format_time(secs: f32) -> String {
    let total = secs as u64;
    format!("{:02}:{:02}", total / 60, total % 60)
}

fn stem_label(id: demucs_core::model::metadata::StemId) -> &'static str {
    match id.as_str() {
        "drums" => "Batterie",
        "bass" => "Basse",
        "vocals" => "Voix",
        "other" => "Autre",
        "guitar" => "Guitare",
        "piano" => "Piano",
        _ => "?",
    }
}

fn do_export_mix(result: &SeparationResult, pb: &PlaybackEngine) -> String {
    let params: Vec<StemMixParam> = result
        .stem_audio
        .iter()
        .map(|stem| StemMixParam {
            muted: pb.is_muted(stem.id),
            soloed: pb.is_soloed(stem.id),
            gain: pb.gain(stem.id),
            gate_enabled: pb.is_gate_enabled(stem.id),
            gate_threshold_db: pb.gate_threshold(stem.id),
            pan: pb.pan(stem.id),
            phase_invert: pb.is_phase_inverted(stem.id),
            eq_enabled: pb.is_eq_enabled(stem.id),
            eq_low_db: pb.eq_low(stem.id),
            eq_mid_db: pb.eq_mid(stem.id),
            eq_high_db: pb.eq_high(stem.id),
            reverb_send: pb.reverb_send(stem.id),
            delay_enabled: pb.is_delay_enabled(stem.id),
            delay_send: pb.delay_send(stem.id),
            delay_time_ms: pb.delay_time(stem.id),
            delay_feedback: pb.delay_feedback(stem.id),
            limiter_enabled: pb.is_limiter_enabled(stem.id),
        })
        .collect();

    let reverb_params = ReverbParams {
        decay: pb.reverb_decay(),
        damping: pb.reverb_damping(),
    };

    let out_dir = std::path::Path::new(&result.output_dir);
    let mix_path = out_dir.join("mix.wav");

    match audio::export_mix(
        &mix_path,
        &result.stem_audio,
        &params,
        pb.master_gain(),
        result.sample_rate,
        &reverb_params,
    ) {
        Ok(()) => format!("OK \u{2014} {}", mix_path.display()),
        Err(e) => format!("Erreur : {}", e),
    }
}

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
