use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use demucs_core::model::metadata::StemId;
use rodio::source::UniformSourceIterator;
use rodio::{OutputStream, OutputStreamHandle, Source};

use crate::dsp::{self, Freeverb, StemFxState};

/// Sentinel value meaning "no seek pending".
const NO_SEEK: u64 = u64::MAX;

/// Default output device sample rate (WASAPI typically 48000).
const DEVICE_SAMPLE_RATE: u32 = 48000;

// ---------------------------------------------------------------------------
// Atomic helpers
// ---------------------------------------------------------------------------

#[inline]
fn load_f32(a: &AtomicU32) -> f32 {
    f32::from_bits(a.load(Ordering::Relaxed))
}

#[inline]
fn store_f32(a: &AtomicU32, v: f32) {
    a.store(v.to_bits(), Ordering::Relaxed);
}

// ---------------------------------------------------------------------------
// Per-stem atomic controls (shared between UI thread and mixer thread)
// ---------------------------------------------------------------------------

struct StemControl {
    #[allow(dead_code)]
    id: StemId,
    muted: AtomicBool,
    solo: AtomicBool,
    gain_bits: AtomicU32,
    // Effects
    gate_enabled: AtomicBool,
    gate_threshold_bits: AtomicU32,
    pan_bits: AtomicU32,
    phase_invert: AtomicBool,
    eq_enabled: AtomicBool,
    eq_low_bits: AtomicU32,
    eq_mid_bits: AtomicU32,
    eq_high_bits: AtomicU32,
    reverb_send_bits: AtomicU32,
    delay_enabled: AtomicBool,
    delay_send_bits: AtomicU32,
    delay_time_bits: AtomicU32,
    delay_feedback_bits: AtomicU32,
    limiter_enabled: AtomicBool,
}

impl StemControl {
    fn new(id: StemId) -> Self {
        Self {
            id,
            muted: AtomicBool::new(false),
            solo: AtomicBool::new(false),
            gain_bits: AtomicU32::new(1.0f32.to_bits()),
            gate_enabled: AtomicBool::new(false),
            gate_threshold_bits: AtomicU32::new((-40.0f32).to_bits()),
            pan_bits: AtomicU32::new(0.0f32.to_bits()),
            phase_invert: AtomicBool::new(false),
            eq_enabled: AtomicBool::new(false),
            eq_low_bits: AtomicU32::new(0.0f32.to_bits()),
            eq_mid_bits: AtomicU32::new(0.0f32.to_bits()),
            eq_high_bits: AtomicU32::new(0.0f32.to_bits()),
            reverb_send_bits: AtomicU32::new(0.0f32.to_bits()),
            delay_enabled: AtomicBool::new(false),
            delay_send_bits: AtomicU32::new(0.0f32.to_bits()),
            delay_time_bits: AtomicU32::new(250.0f32.to_bits()),
            delay_feedback_bits: AtomicU32::new(0.3f32.to_bits()),
            limiter_enabled: AtomicBool::new(false),
        }
    }

    fn gain(&self) -> f32 {
        load_f32(&self.gain_bits)
    }
}

// ---------------------------------------------------------------------------
// Shared state (UI ↔ mixer)
// ---------------------------------------------------------------------------

struct SharedState {
    controls: Vec<StemControl>,
    position: AtomicU64,
    seek_target: AtomicU64,
    master_gain_bits: AtomicU32,
    paused: AtomicBool,
    total_samples: u64,
    sample_rate: u32,
    // Global reverb params
    reverb_decay_bits: AtomicU32,
    reverb_damping_bits: AtomicU32,
}

/// Audio data for all stems, interleaved stereo per stem.
struct StemAudioData {
    stems: Vec<Vec<f32>>,
    num_samples: usize,
}

// ---------------------------------------------------------------------------
// StemMixer — real-time audio source with effects
// ---------------------------------------------------------------------------

struct StemMixer {
    data: Arc<StemAudioData>,
    state: Arc<SharedState>,
    cursor: u64,
    // DSP state (owned by mixer thread only)
    fx_states: Vec<StemFxState>,
    reverb: Freeverb,
    cached_right: f32,
    // Cache for reverb param change detection
    last_reverb_decay: f32,
    last_reverb_damping: f32,
}

impl Iterator for StemMixer {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let channel = (self.cursor % 2) as usize;

        // Right channel: return cached value, advance position
        if channel == 1 {
            self.cursor += 1;
            let sample_idx = (self.cursor / 2) as usize;
            self.state
                .position
                .store(sample_idx as u64, Ordering::Relaxed);
            return Some(self.cached_right);
        }

        // Left channel (channel == 0): process full stereo frame

        // Check for pending seek
        let seek = self.state.seek_target.load(Ordering::Relaxed);
        if seek != NO_SEEK {
            self.cursor = seek * 2;
            self.state.seek_target.store(NO_SEEK, Ordering::Relaxed);
            self.state.position.store(seek, Ordering::Relaxed);
            // Reset all DSP state to avoid clicks
            for fx in &mut self.fx_states {
                fx.reset();
            }
            self.reverb.reset();
        }

        let sample_idx = (self.cursor / 2) as usize;

        if sample_idx >= self.data.num_samples {
            self.cursor = 0;
            self.state.position.store(0, Ordering::Relaxed);
            self.state.paused.store(true, Ordering::Relaxed);
            self.cached_right = 0.0;
            return Some(0.0);
        }

        if self.state.paused.load(Ordering::Relaxed) {
            self.cached_right = 0.0;
            return Some(0.0);
        }

        // Update global reverb params if changed
        let decay = load_f32(&self.state.reverb_decay_bits);
        let damping = load_f32(&self.state.reverb_damping_bits);
        if decay != self.last_reverb_decay || damping != self.last_reverb_damping {
            self.reverb.set_params(decay, damping);
            self.last_reverb_decay = decay;
            self.last_reverb_damping = damping;
        }

        let any_solo = self
            .state
            .controls
            .iter()
            .any(|c| c.solo.load(Ordering::Relaxed));

        let mut mix_l = 0.0f32;
        let mut mix_r = 0.0f32;
        let mut reverb_bus_l = 0.0f32;
        let mut reverb_bus_r = 0.0f32;

        let sr = self.state.sample_rate;

        for (i, ctrl) in self.state.controls.iter().enumerate() {
            if ctrl.muted.load(Ordering::Relaxed) {
                continue;
            }
            if any_solo && !ctrl.solo.load(Ordering::Relaxed) {
                continue;
            }

            let idx_l = sample_idx * 2;
            let idx_r = sample_idx * 2 + 1;
            if idx_r >= self.data.stems[i].len() {
                continue;
            }

            let mut l = self.data.stems[i][idx_l];
            let mut r = self.data.stems[i][idx_r];

            // 0. Noise gate (first in chain — cleans artifacts)
            if ctrl.gate_enabled.load(Ordering::Relaxed) {
                let threshold_db = load_f32(&ctrl.gate_threshold_bits);
                let threshold_lin = self.fx_states[i].gate_threshold_linear(threshold_db);
                let (gl, gr) = self.fx_states[i].gate.process(l, r, threshold_lin);
                l = gl;
                r = gr;
            }

            // 1. Phase invert (pre-fader insert)
            if ctrl.phase_invert.load(Ordering::Relaxed) {
                l = -l;
                r = -r;
            }

            // 2. EQ (pre-fader insert)
            if ctrl.eq_enabled.load(Ordering::Relaxed) {
                let eq_low = load_f32(&ctrl.eq_low_bits);
                let eq_mid = load_f32(&ctrl.eq_mid_bits);
                let eq_high = load_f32(&ctrl.eq_high_bits);
                self.fx_states[i].maybe_update_eq(eq_low, eq_mid, eq_high, sr);
                let (el, er) = self.fx_states[i].eq.process(l, r);
                l = el;
                r = er;
            }

            // 3. Gain (fader)
            let gain = ctrl.gain();
            l *= gain;
            r *= gain;

            // 3b. Soft limiter (tames peaks after gain, prevents per-stem clipping)
            if ctrl.limiter_enabled.load(Ordering::Relaxed) {
                let (sl, sr_val) = dsp::soft_clip_stereo(l, r, 0.85);
                l = sl;
                r = sr_val;
            }

            // 4. Pan
            let pan = load_f32(&ctrl.pan_bits);
            let (pl, pr) = dsp::apply_pan(l, r, pan);
            l = pl;
            r = pr;

            // 5. Delay (post-fader, inline)
            if ctrl.delay_enabled.load(Ordering::Relaxed) {
                let send = load_f32(&ctrl.delay_send_bits);
                let time_ms = load_f32(&ctrl.delay_time_bits);
                let feedback = load_f32(&ctrl.delay_feedback_bits);
                let (dl, dr) = self.fx_states[i].delay.process(l, r, send, time_ms, feedback);
                l = dl;
                r = dr;
            }

            // Accumulate mix
            mix_l += l;
            mix_r += r;

            // 6. Reverb send (post-fader)
            let reverb_send = load_f32(&ctrl.reverb_send_bits);
            if reverb_send > 0.001 {
                reverb_bus_l += l * reverb_send;
                reverb_bus_r += r * reverb_send;
            }
        }

        // Process reverb bus (always process to let the tail decay naturally)
        let (rev_l, rev_r) = self.reverb.process(reverb_bus_l, reverb_bus_r);
        mix_l += rev_l;
        mix_r += rev_r;

        // Master gain + soft clip (knee-based, transparent below 0.95)
        let master = load_f32(&self.state.master_gain_bits);
        mix_l *= master;
        mix_r *= master;
        let (mix_l, mix_r) = dsp::soft_clip_stereo(mix_l, mix_r, 0.95);

        self.cached_right = mix_r;
        self.cursor += 1;

        Some(mix_l)
    }
}

impl Source for StemMixer {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> u16 {
        2
    }

    fn sample_rate(&self) -> u32 {
        self.state.sample_rate
    }

    fn total_duration(&self) -> Option<Duration> {
        Some(Duration::from_secs_f64(
            self.data.num_samples as f64 / self.state.sample_rate as f64,
        ))
    }
}

// ---------------------------------------------------------------------------
// PlaybackEngine — public API
// ---------------------------------------------------------------------------

pub struct PlaybackEngine {
    _stream: OutputStream,
    _handle: OutputStreamHandle,
    state: Arc<SharedState>,
    stem_ids: Vec<StemId>,
}

impl PlaybackEngine {
    pub fn new(stems: &[demucs_core::Stem], sample_rate: u32) -> Option<Self> {
        let (stream, handle) = OutputStream::try_default().ok()?;

        let mut stem_interleaved = Vec::new();
        let mut ids = Vec::new();
        let num_samples = stems.first().map(|s| s.left.len()).unwrap_or(0);

        for stem in stems {
            ids.push(stem.id);
            let mut interleaved = Vec::with_capacity(stem.left.len() * 2);
            for i in 0..stem.left.len() {
                interleaved.push(stem.left[i]);
                interleaved.push(stem.right[i]);
            }
            stem_interleaved.push(interleaved);
        }

        let data = Arc::new(StemAudioData {
            stems: stem_interleaved,
            num_samples,
        });

        let controls: Vec<StemControl> = ids.iter().map(|&id| StemControl::new(id)).collect();
        let num_stems = controls.len();

        let shared = Arc::new(SharedState {
            controls,
            position: AtomicU64::new(0),
            seek_target: AtomicU64::new(NO_SEEK),
            master_gain_bits: AtomicU32::new(1.0f32.to_bits()),
            paused: AtomicBool::new(true),
            total_samples: num_samples as u64,
            sample_rate,
            reverb_decay_bits: AtomicU32::new(1.5f32.to_bits()),
            reverb_damping_bits: AtomicU32::new(0.5f32.to_bits()),
        });

        let fx_states: Vec<StemFxState> = (0..num_stems).map(|_| StemFxState::new(sample_rate)).collect();

        let mixer = StemMixer {
            data,
            state: shared.clone(),
            cursor: 0,
            fx_states,
            reverb: Freeverb::new(sample_rate),
            cached_right: 0.0,
            last_reverb_decay: 1.5,
            last_reverb_damping: 0.5,
        };

        if sample_rate != DEVICE_SAMPLE_RATE {
            let resampled = UniformSourceIterator::new(mixer, 2, DEVICE_SAMPLE_RATE);
            handle.play_raw(resampled).ok()?;
        } else {
            handle.play_raw(mixer).ok()?;
        }

        Some(Self {
            _stream: stream,
            _handle: handle,
            state: shared,
            stem_ids: ids,
        })
    }

    // --- Transport ---

    pub fn is_playing(&self) -> bool {
        !self.state.paused.load(Ordering::Relaxed)
    }

    pub fn toggle_play_pause(&self) {
        let was_paused = self.state.paused.load(Ordering::Relaxed);
        self.state.paused.store(!was_paused, Ordering::Relaxed);
    }

    pub fn seek(&self, sample: u64) {
        let clamped = sample.min(self.state.total_samples);
        self.state.seek_target.store(clamped, Ordering::Relaxed);
    }

    pub fn seek_fraction(&self, fraction: f32) {
        let sample = (fraction.clamp(0.0, 1.0) * self.state.total_samples as f32) as u64;
        self.seek(sample);
    }

    pub fn seek_relative_secs(&self, delta: f32) {
        let current = self.position_secs();
        let target = (current + delta).max(0.0).min(self.duration_secs());
        let sample = (target * self.state.sample_rate as f32) as u64;
        self.seek(sample);
    }

    pub fn position_samples(&self) -> u64 {
        self.state.position.load(Ordering::Relaxed)
    }

    pub fn position_secs(&self) -> f32 {
        self.position_samples() as f32 / self.state.sample_rate as f32
    }

    pub fn duration_secs(&self) -> f32 {
        self.state.total_samples as f32 / self.state.sample_rate as f32
    }

    pub fn position_fraction(&self) -> f32 {
        if self.state.total_samples == 0 {
            return 0.0;
        }
        self.position_samples() as f32 / self.state.total_samples as f32
    }

    // --- Mute / Solo / Gain ---

    pub fn toggle_mute(&self, stem: StemId) {
        if let Some(idx) = self.stem_index(stem) {
            let ctrl = &self.state.controls[idx];
            let was = ctrl.muted.load(Ordering::Relaxed);
            ctrl.muted.store(!was, Ordering::Relaxed);
        }
    }

    pub fn toggle_solo(&self, stem: StemId) {
        if let Some(idx) = self.stem_index(stem) {
            let ctrl = &self.state.controls[idx];
            let was = ctrl.solo.load(Ordering::Relaxed);
            ctrl.solo.store(!was, Ordering::Relaxed);
        }
    }

    pub fn set_gain(&self, stem: StemId, gain: f32) {
        if let Some(idx) = self.stem_index(stem) {
            store_f32(&self.state.controls[idx].gain_bits, gain);
        }
    }

    pub fn set_master_gain(&self, gain: f32) {
        store_f32(&self.state.master_gain_bits, gain);
    }

    pub fn master_gain(&self) -> f32 {
        load_f32(&self.state.master_gain_bits)
    }

    pub fn is_muted(&self, stem: StemId) -> bool {
        self.stem_index(stem)
            .map(|i| self.state.controls[i].muted.load(Ordering::Relaxed))
            .unwrap_or(false)
    }

    pub fn is_soloed(&self, stem: StemId) -> bool {
        self.stem_index(stem)
            .map(|i| self.state.controls[i].solo.load(Ordering::Relaxed))
            .unwrap_or(false)
    }

    pub fn gain(&self, stem: StemId) -> f32 {
        self.stem_index(stem)
            .map(|i| self.state.controls[i].gain())
            .unwrap_or(1.0)
    }

    // --- Pan ---

    pub fn set_pan(&self, stem: StemId, pan: f32) {
        if let Some(idx) = self.stem_index(stem) {
            store_f32(&self.state.controls[idx].pan_bits, pan);
        }
    }

    pub fn pan(&self, stem: StemId) -> f32 {
        self.stem_index(stem)
            .map(|i| load_f32(&self.state.controls[i].pan_bits))
            .unwrap_or(0.0)
    }

    // --- Phase invert ---

    pub fn set_phase_invert(&self, stem: StemId, invert: bool) {
        if let Some(idx) = self.stem_index(stem) {
            self.state.controls[idx]
                .phase_invert
                .store(invert, Ordering::Relaxed);
        }
    }

    pub fn is_phase_inverted(&self, stem: StemId) -> bool {
        self.stem_index(stem)
            .map(|i| self.state.controls[i].phase_invert.load(Ordering::Relaxed))
            .unwrap_or(false)
    }

    // --- EQ ---

    pub fn set_eq_enabled(&self, stem: StemId, enabled: bool) {
        if let Some(idx) = self.stem_index(stem) {
            self.state.controls[idx]
                .eq_enabled
                .store(enabled, Ordering::Relaxed);
        }
    }

    pub fn is_eq_enabled(&self, stem: StemId) -> bool {
        self.stem_index(stem)
            .map(|i| self.state.controls[i].eq_enabled.load(Ordering::Relaxed))
            .unwrap_or(false)
    }

    pub fn set_eq_low(&self, stem: StemId, db: f32) {
        if let Some(idx) = self.stem_index(stem) {
            store_f32(&self.state.controls[idx].eq_low_bits, db);
        }
    }

    pub fn set_eq_mid(&self, stem: StemId, db: f32) {
        if let Some(idx) = self.stem_index(stem) {
            store_f32(&self.state.controls[idx].eq_mid_bits, db);
        }
    }

    pub fn set_eq_high(&self, stem: StemId, db: f32) {
        if let Some(idx) = self.stem_index(stem) {
            store_f32(&self.state.controls[idx].eq_high_bits, db);
        }
    }

    pub fn eq_low(&self, stem: StemId) -> f32 {
        self.stem_index(stem)
            .map(|i| load_f32(&self.state.controls[i].eq_low_bits))
            .unwrap_or(0.0)
    }

    pub fn eq_mid(&self, stem: StemId) -> f32 {
        self.stem_index(stem)
            .map(|i| load_f32(&self.state.controls[i].eq_mid_bits))
            .unwrap_or(0.0)
    }

    pub fn eq_high(&self, stem: StemId) -> f32 {
        self.stem_index(stem)
            .map(|i| load_f32(&self.state.controls[i].eq_high_bits))
            .unwrap_or(0.0)
    }

    // --- Reverb ---

    pub fn set_reverb_send(&self, stem: StemId, amount: f32) {
        if let Some(idx) = self.stem_index(stem) {
            store_f32(&self.state.controls[idx].reverb_send_bits, amount);
        }
    }

    pub fn reverb_send(&self, stem: StemId) -> f32 {
        self.stem_index(stem)
            .map(|i| load_f32(&self.state.controls[i].reverb_send_bits))
            .unwrap_or(0.0)
    }

    pub fn set_reverb_decay(&self, decay: f32) {
        store_f32(&self.state.reverb_decay_bits, decay);
    }

    pub fn set_reverb_damping(&self, damping: f32) {
        store_f32(&self.state.reverb_damping_bits, damping);
    }

    pub fn reverb_decay(&self) -> f32 {
        load_f32(&self.state.reverb_decay_bits)
    }

    pub fn reverb_damping(&self) -> f32 {
        load_f32(&self.state.reverb_damping_bits)
    }

    // --- Delay ---

    pub fn set_delay_enabled(&self, stem: StemId, enabled: bool) {
        if let Some(idx) = self.stem_index(stem) {
            self.state.controls[idx]
                .delay_enabled
                .store(enabled, Ordering::Relaxed);
        }
    }

    pub fn is_delay_enabled(&self, stem: StemId) -> bool {
        self.stem_index(stem)
            .map(|i| self.state.controls[i].delay_enabled.load(Ordering::Relaxed))
            .unwrap_or(false)
    }

    pub fn set_delay_send(&self, stem: StemId, amount: f32) {
        if let Some(idx) = self.stem_index(stem) {
            store_f32(&self.state.controls[idx].delay_send_bits, amount);
        }
    }

    pub fn delay_send(&self, stem: StemId) -> f32 {
        self.stem_index(stem)
            .map(|i| load_f32(&self.state.controls[i].delay_send_bits))
            .unwrap_or(0.0)
    }

    pub fn set_delay_time(&self, stem: StemId, ms: f32) {
        if let Some(idx) = self.stem_index(stem) {
            store_f32(&self.state.controls[idx].delay_time_bits, ms);
        }
    }

    pub fn delay_time(&self, stem: StemId) -> f32 {
        self.stem_index(stem)
            .map(|i| load_f32(&self.state.controls[i].delay_time_bits))
            .unwrap_or(250.0)
    }

    pub fn set_delay_feedback(&self, stem: StemId, feedback: f32) {
        if let Some(idx) = self.stem_index(stem) {
            store_f32(&self.state.controls[idx].delay_feedback_bits, feedback);
        }
    }

    pub fn delay_feedback(&self, stem: StemId) -> f32 {
        self.stem_index(stem)
            .map(|i| load_f32(&self.state.controls[i].delay_feedback_bits))
            .unwrap_or(0.3)
    }

    // --- Noise Gate ---

    pub fn set_gate_enabled(&self, stem: StemId, enabled: bool) {
        if let Some(idx) = self.stem_index(stem) {
            self.state.controls[idx]
                .gate_enabled
                .store(enabled, Ordering::Relaxed);
        }
    }

    pub fn is_gate_enabled(&self, stem: StemId) -> bool {
        self.stem_index(stem)
            .map(|i| self.state.controls[i].gate_enabled.load(Ordering::Relaxed))
            .unwrap_or(false)
    }

    pub fn set_gate_threshold(&self, stem: StemId, db: f32) {
        if let Some(idx) = self.stem_index(stem) {
            store_f32(&self.state.controls[idx].gate_threshold_bits, db);
        }
    }

    pub fn gate_threshold(&self, stem: StemId) -> f32 {
        self.stem_index(stem)
            .map(|i| load_f32(&self.state.controls[i].gate_threshold_bits))
            .unwrap_or(-40.0)
    }

    // --- Limiter ---

    pub fn set_limiter_enabled(&self, stem: StemId, enabled: bool) {
        if let Some(idx) = self.stem_index(stem) {
            self.state.controls[idx]
                .limiter_enabled
                .store(enabled, Ordering::Relaxed);
        }
    }

    pub fn is_limiter_enabled(&self, stem: StemId) -> bool {
        self.stem_index(stem)
            .map(|i| self.state.controls[i].limiter_enabled.load(Ordering::Relaxed))
            .unwrap_or(false)
    }

    // --- Internal ---

    fn stem_index(&self, stem: StemId) -> Option<usize> {
        self.stem_ids.iter().position(|&id| id == stem)
    }
}
