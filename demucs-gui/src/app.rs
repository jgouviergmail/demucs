use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Instant;

use egui_file_dialog::FileDialog;

use crate::state::{AppPhase, SetupState, WorkerCommand, WorkerUpdate};
use crate::worker;

pub struct DemucsApp {
    phase: AppPhase,
    setup: SetupState,
    cmd_tx: mpsc::Sender<WorkerCommand>,
    update_rx: mpsc::Receiver<WorkerUpdate>,
    cancel_flag: Arc<AtomicBool>,
    start_time: Option<Instant>,
    // Preserved for "Nouvelle separation"
    last_input_name: String,
    last_model_label: String,
    // File dialogs
    file_dialog: FileDialog,
    folder_dialog: FileDialog,
}

impl DemucsApp {
    pub fn new() -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (update_tx, update_rx) = mpsc::channel();
        let cancel_flag = Arc::new(AtomicBool::new(false));

        let _worker = worker::spawn_worker(cmd_rx, update_tx, cancel_flag.clone());

        Self {
            phase: AppPhase::Setup,
            setup: SetupState::default(),
            cmd_tx,
            update_rx,
            cancel_flag,
            start_time: None,
            last_input_name: String::new(),
            last_model_label: String::new(),
            file_dialog: FileDialog::new(),
            folder_dialog: FileDialog::new(),
        }
    }

    fn process_updates(&mut self) {
        while let Ok(update) = self.update_rx.try_recv() {
            match update {
                WorkerUpdate::DownloadStarted { model_id, size_mb } => {
                    self.phase = AppPhase::Downloading {
                        progress: 0.0,
                        status: format!(
                            "T\u{e9}l\u{e9}chargement de {} ({} Mo)...",
                            model_id, size_mb
                        ),
                    };
                }
                WorkerUpdate::DownloadProgress {
                    bytes_downloaded,
                    total_bytes,
                } => {
                    if let AppPhase::Downloading {
                        ref mut progress,
                        ref mut status,
                    } = self.phase
                    {
                        *progress = bytes_downloaded as f32 / total_bytes as f32;
                        let mb_done = bytes_downloaded as f64 / 1_000_000.0;
                        let mb_total = total_bytes as f64 / 1_000_000.0;
                        *status = format!("{:.1} / {:.1} Mo", mb_done, mb_total);
                    }
                }
                WorkerUpdate::DownloadDone => {}
                WorkerUpdate::ModelLoading => {
                    self.phase = AppPhase::LoadingModel;
                }
                WorkerUpdate::ModelLoaded => {}
                WorkerUpdate::SeparationStarted => {
                    self.phase = AppPhase::Separating {
                        progress: 0.0,
                        chunk_info: String::new(),
                        step_description: "D\u{e9}marrage...".into(),
                        elapsed: self.elapsed(),
                    };
                }
                WorkerUpdate::ForwardProgress {
                    step,
                    total_steps,
                    description,
                } => {
                    let el = self.elapsed();
                    if let AppPhase::Separating {
                        ref mut progress,
                        ref mut step_description,
                        ref mut elapsed,
                        ..
                    } = self.phase
                    {
                        *progress = step as f32 / total_steps as f32;
                        *step_description = description;
                        *elapsed = el;
                    }
                }
                WorkerUpdate::ChunkProgress {
                    chunk,
                    total_chunks,
                } => {
                    if let AppPhase::Separating {
                        ref mut chunk_info,
                        ..
                    } = self.phase
                    {
                        *chunk_info = format!("Chunk {}/{}", chunk + 1, total_chunks);
                    }
                }
                WorkerUpdate::SeparationDone => {}
                WorkerUpdate::WritingStems => {
                    self.phase = AppPhase::WritingOutput;
                }
                WorkerUpdate::StemWritten { .. } => {}
                WorkerUpdate::AllDone { output_dir, stems } => {
                    self.phase = AppPhase::Done {
                        stems_written: stems,
                        output_dir,
                        elapsed: self.elapsed(),
                    };
                }
                WorkerUpdate::Error { message } => {
                    self.phase = AppPhase::Error { message };
                }
            }
        }
    }

    fn elapsed(&self) -> std::time::Duration {
        self.start_time
            .map(|t| t.elapsed())
            .unwrap_or_default()
    }
}

impl eframe::App for DemucsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.process_updates();

        // Handle file dialog results
        self.file_dialog.update(ctx);
        self.folder_dialog.update(ctx);

        if let Some(path) = self.file_dialog.take_picked() {
            self.setup.input_path = Some(path);
        }
        if let Some(path) = self.folder_dialog.take_picked() {
            self.setup.output_dir = path;
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            match &self.phase {
                AppPhase::Setup => {
                    let mut start_requested = false;
                    crate::ui::setup_panel::render(
                        ui,
                        &mut self.setup,
                        &mut self.file_dialog,
                        &mut self.folder_dialog,
                        &mut start_requested,
                    );
                    if start_requested {
                        self.last_input_name = self
                            .setup
                            .input_filename()
                            .unwrap_or_default();
                        self.last_model_label = self.setup.model_choice.label().to_string();
                        self.start_time = Some(Instant::now());
                        self.cancel_flag.store(false, Ordering::Relaxed);
                        let trim = if self.setup.trim.enabled {
                            Some(self.setup.trim.clone())
                        } else {
                            None
                        };
                        let _ = self.cmd_tx.send(WorkerCommand::Start {
                            input_path: self.setup.input_path.clone().unwrap(),
                            model_choice: self.setup.model_choice,
                            selected_stems: self.setup.selected_stems(),
                            output_dir: self.setup.output_dir.clone(),
                            trim,
                        });
                        self.phase = AppPhase::LoadingModel;
                    }
                }
                AppPhase::Downloading { .. }
                | AppPhase::LoadingModel
                | AppPhase::Separating { .. }
                | AppPhase::WritingOutput => {
                    let mut cancel_requested = false;
                    crate::ui::progress_panel::render(
                        ui,
                        &self.phase,
                        &self.last_input_name,
                        &self.last_model_label,
                        self.elapsed(),
                        &mut cancel_requested,
                    );
                    if cancel_requested {
                        self.cancel_flag.store(true, Ordering::Relaxed);
                        let _ = self.cmd_tx.send(WorkerCommand::Cancel);
                    }
                }
                AppPhase::Done {
                    stems_written,
                    output_dir,
                    elapsed,
                } => {
                    let stems = stems_written.clone();
                    let dir = output_dir.clone();
                    let el = *elapsed;
                    let mut new_sep = false;
                    crate::ui::result_panel::render(ui, &stems, &dir, el, &mut new_sep);
                    if new_sep {
                        self.phase = AppPhase::Setup;
                    }
                }
                AppPhase::Error { message } => {
                    let msg = message.clone();
                    ui.vertical_centered(|ui| {
                        ui.add_space(40.0);
                        ui.heading(
                            egui::RichText::new("Erreur")
                                .size(22.0)
                                .color(egui::Color32::from_rgb(255, 100, 100)),
                        );
                        ui.add_space(16.0);
                        ui.label(&msg);
                        ui.add_space(24.0);
                        if ui.button("Retour").clicked() {
                            self.phase = AppPhase::Setup;
                        }
                    });
                }
            }
        });

        // Keep repainting while working
        if !matches!(self.phase, AppPhase::Setup | AppPhase::Done { .. }) {
            ctx.request_repaint();
        }
    }
}
