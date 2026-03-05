use std::path::PathBuf;

use demucs_core::model::metadata::{StemId, HTDEMUCS, HTDEMUCS_6S, HTDEMUCS_FT, ModelInfo};
use demucs_core::Stem;

// ── Application phase (state machine) ───────────────────────────────────────

pub enum AppPhase {
    Setup,
    Downloading {
        progress: f32,
        status: String,
    },
    LoadingModel,
    Separating {
        progress: f32,
        chunk_info: String,
        step_description: String,
        elapsed: std::time::Duration,
    },
    WritingOutput,
    Done {
        result: SeparationResult,
        elapsed: std::time::Duration,
    },
    Error {
        message: String,
    },
}

// ── Setup state ─────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ModelChoice {
    HtDemucs,
    HtDemucs6s,
    HtDemucsFt,
}

impl ModelChoice {
    pub fn info(&self) -> &'static ModelInfo {
        match self {
            ModelChoice::HtDemucs => &HTDEMUCS,
            ModelChoice::HtDemucs6s => &HTDEMUCS_6S,
            ModelChoice::HtDemucsFt => &HTDEMUCS_FT,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            ModelChoice::HtDemucs => "Standard",
            ModelChoice::HtDemucs6s => "6 Stems",
            ModelChoice::HtDemucsFt => "Fine-Tuned",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            ModelChoice::HtDemucs => "4 stems, bon compromis vitesse/qualit\u{e9} (84 Mo)",
            ModelChoice::HtDemucs6s => "6 stems, + guitare & piano (84 Mo)",
            ModelChoice::HtDemucsFt => "4 stems, meilleure qualit\u{e9} (333 Mo)",
        }
    }
}

// ── Trim settings ────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct TrimSettings {
    pub enabled: bool,
    /// Minimum silence duration (seconds) to trigger trimming
    pub min_silence_secs: f32,
    /// Replacement silence duration (seconds)
    pub replacement_secs: f32,
    /// Silence threshold in dB (below this = silence)
    pub threshold_db: f32,
}

impl Default for TrimSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            min_silence_secs: 2.0,
            replacement_secs: 0.5,
            threshold_db: -40.0,
        }
    }
}

pub struct SetupState {
    pub input_path: Option<PathBuf>,
    pub model_choice: ModelChoice,
    pub stem_selection: Vec<(StemId, bool)>,
    pub output_dir: PathBuf,
    pub trim: TrimSettings,
}

impl Default for SetupState {
    fn default() -> Self {
        let model = ModelChoice::HtDemucs;
        Self {
            input_path: None,
            model_choice: model,
            stem_selection: model
                .info()
                .stems
                .iter()
                .map(|&s| (s, true))
                .collect(),
            output_dir: PathBuf::from("./stems"),
            trim: TrimSettings::default(),
        }
    }
}

impl SetupState {
    pub fn update_stems_for_model(&mut self) {
        self.stem_selection = self
            .model_choice
            .info()
            .stems
            .iter()
            .map(|&s| (s, true))
            .collect();
    }

    pub fn selected_stems(&self) -> Vec<StemId> {
        self.stem_selection
            .iter()
            .filter(|(_, on)| *on)
            .map(|(id, _)| *id)
            .collect()
    }

    pub fn input_filename(&self) -> Option<String> {
        self.input_path
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
    }

    pub fn can_start(&self) -> bool {
        self.input_path.is_some() && !self.selected_stems().is_empty()
    }
}

// ── Separation result (audio kept in memory for review) ─────────────────────

pub struct StereoAudio {
    pub left: Vec<f32>,
    pub right: Vec<f32>,
}

pub struct SeparationResult {
    pub stems_written: Vec<String>,
    pub output_dir: String,
    pub stem_audio: Vec<Stem>,
    pub input_audio: StereoAudio,
    pub sample_rate: u32,
}

// ── Worker protocol ─────────────────────────────────────────────────────────

pub enum WorkerCommand {
    Start {
        input_path: PathBuf,
        model_choice: ModelChoice,
        selected_stems: Vec<StemId>,
        output_dir: PathBuf,
        trim: Option<TrimSettings>,
    },
    Cancel,
}

pub enum WorkerUpdate {
    DownloadStarted { model_id: String, size_mb: u32 },
    DownloadProgress { bytes_downloaded: u64, total_bytes: u64 },
    DownloadDone,
    ModelLoading,
    ModelLoaded,
    SeparationStarted,
    ForwardProgress { step: usize, total_steps: usize, description: String },
    ChunkProgress { chunk: usize, total_chunks: usize },
    SeparationDone,
    WritingStems,
    StemWritten { path: String },
    AllDone {
        output_dir: String,
        stems: Vec<String>,
        stem_audio: Vec<Stem>,
        input_audio: StereoAudio,
        sample_rate: u32,
    },
    Error { message: String },
}
