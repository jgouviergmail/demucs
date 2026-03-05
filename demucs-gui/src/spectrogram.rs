use std::sync::mpsc;
use std::sync::Arc;

use demucs_core::dsp::spectrogram::{compute_spectrogram, SpectrogramData};
use egui::{ColorImage, TextureHandle};

use crate::theme::MagmaLut;

const RENDER_HEIGHT: usize = 512;
const MIN_FREQ: f32 = 30.0;
const DYNAMIC_RANGE: f32 = 80.0;
const MAX_TEXTURE_WIDTH: usize = 8192;
/// Max concurrent spectrogram compute threads.
const MAX_WORKERS: usize = 2;

struct SpectrogramJob {
    label: String,
    image: ColorImage,
}

/// A pending spectrogram task (mono samples + metadata).
struct SpectrogramTask {
    label: String,
    mono: Vec<f32>,
    sample_rate: u32,
}

pub struct SpectrogramEntry {
    pub label: String,
    pub texture: TextureHandle,
}

pub struct SpectrogramManager {
    rx: mpsc::Receiver<SpectrogramJob>,
    pub entries: Vec<SpectrogramEntry>,
    pending: usize,
    total: usize,
}

impl SpectrogramManager {
    pub fn new(
        stems: &[demucs_core::Stem],
        input_left: &[f32],
        input_right: &[f32],
        sample_rate: u32,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        let lut = Arc::new(MagmaLut::new());

        // Build task list: input first, then each stem
        let mut tasks = Vec::new();

        let input_mono: Vec<f32> = input_left
            .iter()
            .zip(input_right.iter())
            .map(|(l, r)| (l + r) * 0.5)
            .collect();
        tasks.push(SpectrogramTask {
            label: "Input".to_string(),
            mono: input_mono,
            sample_rate,
        });

        for stem in stems {
            let mono: Vec<f32> = stem
                .left
                .iter()
                .zip(stem.right.iter())
                .map(|(l, r)| (l + r) * 0.5)
                .collect();
            tasks.push(SpectrogramTask {
                label: stem.id.as_str().to_string(),
                mono,
                sample_rate,
            });
        }

        let total = tasks.len();

        // Channel-based semaphore: MAX_WORKERS tokens limit concurrency
        let (sem_tx, sem_rx) = mpsc::sync_channel::<()>(MAX_WORKERS);
        for _ in 0..MAX_WORKERS {
            let _ = sem_tx.send(());
        }

        // Coordinator thread: for each task, acquire a token, spawn a worker
        let result_tx = tx;
        std::thread::spawn(move || {
            for task in tasks {
                // Wait for a free slot
                if sem_rx.recv().is_err() {
                    break;
                }
                let result_tx = result_tx.clone();
                let sem_tx = sem_tx.clone();
                let lut = lut.clone();

                std::thread::spawn(move || {
                    if let Ok(data) = compute_spectrogram(&task.mono) {
                        let image = spectrogram_to_image(&data, task.sample_rate, &lut);
                        let _ = result_tx.send(SpectrogramJob {
                            label: task.label,
                            image,
                        });
                    }
                    // Release token
                    let _ = sem_tx.send(());
                });
            }
        });

        Self {
            rx,
            entries: Vec::new(),
            pending: total,
            total,
        }
    }

    /// Poll for completed spectrograms and upload to GPU. Returns true if new ones arrived.
    pub fn poll(&mut self, ctx: &egui::Context) -> bool {
        let mut any = false;
        while let Ok(job) = self.rx.try_recv() {
            let texture = ctx.load_texture(
                &job.label,
                job.image,
                egui::TextureOptions::LINEAR,
            );
            self.entries.push(SpectrogramEntry {
                label: job.label,
                texture,
            });
            self.pending -= 1;
            any = true;
        }
        any
    }

    pub fn is_complete(&self) -> bool {
        self.pending == 0
    }

    pub fn progress(&self) -> (usize, usize) {
        (self.total - self.pending, self.total)
    }

    pub fn get(&self, label: &str) -> Option<&SpectrogramEntry> {
        self.entries.iter().find(|e| e.label == label)
    }
}

fn spectrogram_to_image(
    data: &SpectrogramData,
    sample_rate: u32,
    lut: &MagmaLut,
) -> ColorImage {
    let num_frames = data.num_frames as usize;
    let num_bins = data.num_bins as usize;
    let nyquist = sample_rate as f32 / 2.0;

    let out_width = num_frames.min(MAX_TEXTURE_WIDTH);
    let out_height = RENDER_HEIGHT;

    let max_db = data.mags.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let min_db = max_db - DYNAMIC_RANGE;

    let log_min = MIN_FREQ.ln();
    let log_max = nyquist.ln();

    let mut pixels = vec![0u8; out_width * out_height * 4];

    for y in 0..out_height {
        let t = 1.0 - (y as f32 / (out_height - 1) as f32);
        let freq = (log_min + t * (log_max - log_min)).exp();
        let bin_f = freq / nyquist * num_bins as f32;

        let bin_lo = (bin_f as usize).min(num_bins - 1);
        let bin_hi = (bin_lo + 1).min(num_bins - 1);
        let bin_frac = bin_f - bin_lo as f32;

        for x in 0..out_width {
            let frame_f = x as f32 / out_width as f32 * num_frames as f32;
            let frame = (frame_f as usize).min(num_frames - 1);

            let idx_lo = frame * num_bins + bin_lo;
            let idx_hi = frame * num_bins + bin_hi;

            let db = if idx_lo < data.mags.len() && idx_hi < data.mags.len() {
                data.mags[idx_lo] * (1.0 - bin_frac) + data.mags[idx_hi] * bin_frac
            } else if idx_lo < data.mags.len() {
                data.mags[idx_lo]
            } else {
                min_db
            };

            let norm = ((db - min_db) / DYNAMIC_RANGE).clamp(0.0, 1.0);
            let rgba = lut.lookup(norm);

            let px = (y * out_width + x) * 4;
            pixels[px..px + 4].copy_from_slice(&rgba);
        }
    }

    ColorImage::from_rgba_unmultiplied([out_width, out_height], &pixels)
}
