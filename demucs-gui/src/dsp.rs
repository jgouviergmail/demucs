//! Audio DSP primitives: biquad EQ, Freeverb reverb, stereo delay, pan.
//!
//! All processing is sample-by-sample with no heap allocation in the hot path.
//! Formulas: Audio EQ Cookbook (Robert Bristow-Johnson), Freeverb (Jezar).

use std::f32::consts::PI;

// ---------------------------------------------------------------------------
// Biquad filter — Direct Form II Transposed
// ---------------------------------------------------------------------------

pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    /// Pass-through (unity gain, no filtering).
    pub fn passthrough() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    pub fn low_shelf(freq: f32, gain_db: f32, q: f32, sr: f32) -> Self {
        let mut b = Self::passthrough();
        b.set_low_shelf(freq, gain_db, q, sr);
        b
    }

    pub fn high_shelf(freq: f32, gain_db: f32, q: f32, sr: f32) -> Self {
        let mut b = Self::passthrough();
        b.set_high_shelf(freq, gain_db, q, sr);
        b
    }

    pub fn peaking(freq: f32, gain_db: f32, q: f32, sr: f32) -> Self {
        let mut b = Self::passthrough();
        b.set_peaking(freq, gain_db, q, sr);
        b
    }

    pub fn set_low_shelf(&mut self, freq: f32, gain_db: f32, q: f32, sr: f32) {
        let a = 10.0_f32.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * freq / sr;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q);
        let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;

        let a0 = (a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha;
        let a0_inv = 1.0 / a0;

        self.b0 = (a * ((a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha)) * a0_inv;
        self.b1 = (2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0)) * a0_inv;
        self.b2 = (a * ((a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha)) * a0_inv;
        self.a1 = (-2.0 * ((a - 1.0) + (a + 1.0) * cos_w0)) * a0_inv;
        self.a2 = ((a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha) * a0_inv;
    }

    pub fn set_high_shelf(&mut self, freq: f32, gain_db: f32, q: f32, sr: f32) {
        let a = 10.0_f32.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * freq / sr;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q);
        let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;

        let a0 = (a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha;
        let a0_inv = 1.0 / a0;

        self.b0 = (a * ((a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha)) * a0_inv;
        self.b1 = (-2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0)) * a0_inv;
        self.b2 = (a * ((a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha)) * a0_inv;
        self.a1 = (2.0 * ((a - 1.0) - (a + 1.0) * cos_w0)) * a0_inv;
        self.a2 = ((a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha) * a0_inv;
    }

    pub fn set_peaking(&mut self, freq: f32, gain_db: f32, q: f32, sr: f32) {
        let a = 10.0_f32.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * freq / sr;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q);

        let a0 = 1.0 + alpha / a;
        let a0_inv = 1.0 / a0;

        self.b0 = (1.0 + alpha * a) * a0_inv;
        self.b1 = (-2.0 * cos_w0) * a0_inv;
        self.b2 = (1.0 - alpha * a) * a0_inv;
        self.a1 = (-2.0 * cos_w0) * a0_inv;
        self.a2 = (1.0 - alpha / a) * a0_inv;
    }

    #[inline]
    pub fn process(&mut self, input: f32) -> f32 {
        let out = self.b0 * input + self.z1;
        self.z1 = self.b1 * input - self.a1 * out + self.z2;
        self.z2 = self.b2 * input - self.a2 * out;
        out
    }

    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
}

// ---------------------------------------------------------------------------
// Three-band EQ: low shelf 200Hz, mid peak 1kHz, high shelf 5kHz
// ---------------------------------------------------------------------------

const EQ_LOW_FREQ: f32 = 200.0;
const EQ_MID_FREQ: f32 = 1000.0;
const EQ_HIGH_FREQ: f32 = 5000.0;
const EQ_Q: f32 = 0.707;

pub struct ThreeBandEq {
    low_l: Biquad,
    low_r: Biquad,
    mid_l: Biquad,
    mid_r: Biquad,
    high_l: Biquad,
    high_r: Biquad,
}

impl ThreeBandEq {
    pub fn new(sr: u32) -> Self {
        let sr = sr as f32;
        Self {
            low_l: Biquad::low_shelf(EQ_LOW_FREQ, 0.0, EQ_Q, sr),
            low_r: Biquad::low_shelf(EQ_LOW_FREQ, 0.0, EQ_Q, sr),
            mid_l: Biquad::peaking(EQ_MID_FREQ, 0.0, EQ_Q, sr),
            mid_r: Biquad::peaking(EQ_MID_FREQ, 0.0, EQ_Q, sr),
            high_l: Biquad::high_shelf(EQ_HIGH_FREQ, 0.0, EQ_Q, sr),
            high_r: Biquad::high_shelf(EQ_HIGH_FREQ, 0.0, EQ_Q, sr),
        }
    }

    /// Recalculate coefficients when gains change. Preserves filter state.
    pub fn update_gains(&mut self, low_db: f32, mid_db: f32, high_db: f32, sr: u32) {
        let sr = sr as f32;
        self.low_l.set_low_shelf(EQ_LOW_FREQ, low_db, EQ_Q, sr);
        self.low_r.set_low_shelf(EQ_LOW_FREQ, low_db, EQ_Q, sr);
        self.mid_l.set_peaking(EQ_MID_FREQ, mid_db, EQ_Q, sr);
        self.mid_r.set_peaking(EQ_MID_FREQ, mid_db, EQ_Q, sr);
        self.high_l.set_high_shelf(EQ_HIGH_FREQ, high_db, EQ_Q, sr);
        self.high_r.set_high_shelf(EQ_HIGH_FREQ, high_db, EQ_Q, sr);
    }

    #[inline]
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let l = self.low_l.process(l);
        let r = self.low_r.process(r);
        let l = self.mid_l.process(l);
        let r = self.mid_r.process(r);
        let l = self.high_l.process(l);
        let r = self.high_r.process(r);
        (l, r)
    }

    pub fn reset(&mut self) {
        self.low_l.reset();
        self.low_r.reset();
        self.mid_l.reset();
        self.mid_r.reset();
        self.high_l.reset();
        self.high_r.reset();
    }
}

// ---------------------------------------------------------------------------
// Freeverb — 8 parallel comb filters + 4 series allpass filters, stereo
// ---------------------------------------------------------------------------

struct CombFilter {
    buffer: Vec<f32>,
    index: usize,
    feedback: f32,
    damp1: f32,
    damp2: f32,
    filterstore: f32,
}

impl CombFilter {
    fn new(size: usize) -> Self {
        Self {
            buffer: vec![0.0; size],
            index: 0,
            feedback: 0.5,
            damp1: 0.5,
            damp2: 0.5,
            filterstore: 0.0,
        }
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let output = self.buffer[self.index];
        self.filterstore = output * self.damp2 + self.filterstore * self.damp1;
        self.buffer[self.index] = input + self.filterstore * self.feedback;
        self.index += 1;
        if self.index >= self.buffer.len() {
            self.index = 0;
        }
        output
    }

    fn set_damp(&mut self, damp: f32) {
        self.damp1 = damp;
        self.damp2 = 1.0 - damp;
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.filterstore = 0.0;
        self.index = 0;
    }
}

struct AllpassFilter {
    buffer: Vec<f32>,
    index: usize,
}

impl AllpassFilter {
    fn new(size: usize) -> Self {
        Self {
            buffer: vec![0.0; size],
            index: 0,
        }
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let buffered = self.buffer[self.index];
        let output = -input + buffered;
        self.buffer[self.index] = input + buffered * 0.5;
        self.index += 1;
        if self.index >= self.buffer.len() {
            self.index = 0;
        }
        output
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.index = 0;
    }
}

/// Freeverb delay lengths at 44100 Hz (Jezar's original values).
const COMB_LENGTHS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASS_LENGTHS: [usize; 4] = [556, 441, 341, 225];
/// Stereo spread: right channel buffers are offset by this many samples.
const STEREO_SPREAD: usize = 23;

pub struct Freeverb {
    combs_l: Vec<CombFilter>,
    combs_r: Vec<CombFilter>,
    allpasses_l: Vec<AllpassFilter>,
    allpasses_r: Vec<AllpassFilter>,
    gain: f32,
}

impl Freeverb {
    pub fn new(sr: u32) -> Self {
        let scale = sr as f32 / 44100.0;
        let combs_l: Vec<_> = COMB_LENGTHS
            .iter()
            .map(|&len| CombFilter::new((len as f32 * scale) as usize))
            .collect();
        let combs_r: Vec<_> = COMB_LENGTHS
            .iter()
            .map(|&len| CombFilter::new(((len + STEREO_SPREAD) as f32 * scale) as usize))
            .collect();
        let allpasses_l: Vec<_> = ALLPASS_LENGTHS
            .iter()
            .map(|&len| AllpassFilter::new((len as f32 * scale) as usize))
            .collect();
        let allpasses_r: Vec<_> = ALLPASS_LENGTHS
            .iter()
            .map(|&len| AllpassFilter::new(((len + STEREO_SPREAD) as f32 * scale) as usize))
            .collect();

        let mut rv = Self {
            combs_l,
            combs_r,
            allpasses_l,
            allpasses_r,
            gain: 0.015,
        };
        rv.set_params(1.5, 0.5);
        rv
    }

    /// Set reverb parameters.
    /// - `decay`: 0.5..5.0 — reverb time in seconds (mapped to Freeverb feedback range 0.7..0.98)
    /// - `damping`: 0.0..1.0 — high-frequency damping
    pub fn set_params(&mut self, decay: f32, damping: f32) {
        // Map decay 0.5..5.0 → feedback 0.7..0.98 (Freeverb's sweet spot)
        let t = ((decay - 0.5) / 4.5).clamp(0.0, 1.0);
        let feedback = 0.7 + t * 0.28;
        let damp = damping.clamp(0.0, 1.0);

        for comb in self.combs_l.iter_mut().chain(self.combs_r.iter_mut()) {
            comb.feedback = feedback;
            comb.set_damp(damp);
        }
    }

    #[inline]
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let input = (l + r) * self.gain;

        let mut out_l = 0.0f32;
        let mut out_r = 0.0f32;

        for comb in self.combs_l.iter_mut() {
            out_l += comb.process(input);
        }
        for comb in self.combs_r.iter_mut() {
            out_r += comb.process(input);
        }

        for ap in self.allpasses_l.iter_mut() {
            out_l = ap.process(out_l);
        }
        for ap in self.allpasses_r.iter_mut() {
            out_r = ap.process(out_r);
        }

        (out_l, out_r)
    }

    pub fn reset(&mut self) {
        for comb in self.combs_l.iter_mut().chain(self.combs_r.iter_mut()) {
            comb.reset();
        }
        for ap in self.allpasses_l.iter_mut().chain(self.allpasses_r.iter_mut()) {
            ap.reset();
        }
    }
}

// ---------------------------------------------------------------------------
// Stereo Delay with ring buffer
// ---------------------------------------------------------------------------

pub struct StereoDelay {
    buffer_l: Vec<f32>,
    buffer_r: Vec<f32>,
    write_idx: usize,
    sr: u32,
}

impl StereoDelay {
    /// Create a delay with max delay of `max_delay_ms` milliseconds.
    pub fn new(sr: u32, max_delay_ms: f32) -> Self {
        let max_samples = ((max_delay_ms / 1000.0) * sr as f32) as usize + 1;
        Self {
            buffer_l: vec![0.0; max_samples],
            buffer_r: vec![0.0; max_samples],
            write_idx: 0,
            sr,
        }
    }

    /// Process one stereo frame. Returns the mixed (dry + wet) output.
    ///
    /// - `send`: 0.0..1.0 — wet amount
    /// - `time_ms`: delay time in milliseconds
    /// - `feedback`: 0.0..0.95
    #[inline]
    pub fn process(
        &mut self,
        l: f32,
        r: f32,
        send: f32,
        time_ms: f32,
        feedback: f32,
    ) -> (f32, f32) {
        let delay_samples = ((time_ms / 1000.0) * self.sr as f32) as usize;
        let buf_len = self.buffer_l.len();
        let delay_samples = delay_samples.min(buf_len - 1);

        let read_idx = (self.write_idx + buf_len - delay_samples) % buf_len;
        let delayed_l = self.buffer_l[read_idx];
        let delayed_r = self.buffer_r[read_idx];

        // Write full signal + feedback into buffer (send only controls output wet level)
        let feedback = feedback.min(0.95);
        self.buffer_l[self.write_idx] = l + delayed_l * feedback;
        self.buffer_r[self.write_idx] = r + delayed_r * feedback;

        self.write_idx = (self.write_idx + 1) % buf_len;

        // Return dry + delayed × send
        (l + delayed_l * send, r + delayed_r * send)
    }

    pub fn reset(&mut self) {
        self.buffer_l.fill(0.0);
        self.buffer_r.fill(0.0);
        self.write_idx = 0;
    }
}

// ---------------------------------------------------------------------------
// Noise Gate — envelope-following gate with attack/release smoothing
// ---------------------------------------------------------------------------

pub struct NoiseGate {
    envelope: f32,
    gain: f32,
    attack_coeff: f32,
    release_coeff: f32,
    sr: f32,
}

impl NoiseGate {
    pub fn new(sr: u32) -> Self {
        let sr = sr as f32;
        let mut g = Self {
            envelope: 0.0,
            gain: 0.0,
            attack_coeff: 0.0,
            release_coeff: 0.0,
            sr,
        };
        g.set_times(0.001, 0.050); // 1ms attack, 50ms release
        g
    }

    /// Set attack and release times in seconds.
    pub fn set_times(&mut self, attack_secs: f32, release_secs: f32) {
        // Exponential smoothing coefficients
        self.attack_coeff = (-1.0 / (attack_secs * self.sr)).exp();
        self.release_coeff = (-1.0 / (release_secs * self.sr)).exp();
    }

    /// Process a stereo frame through the gate.
    /// - `threshold_lin`: gate threshold in linear scale (pre-converted from dB)
    /// Returns attenuated (l, r).
    #[inline]
    pub fn process(&mut self, l: f32, r: f32, threshold_lin: f32) -> (f32, f32) {
        // Peak envelope follower (max of L/R)
        let input_level = l.abs().max(r.abs());

        // Smooth envelope (fast attack, slower release)
        let coeff = if input_level > self.envelope {
            self.attack_coeff
        } else {
            self.release_coeff
        };
        self.envelope = coeff * self.envelope + (1.0 - coeff) * input_level;

        // Smooth gain transition (avoids clicks)
        let target_gain = if self.envelope > threshold_lin {
            1.0
        } else {
            // Soft knee: fade to 0 below threshold
            (self.envelope / threshold_lin).powi(2)
        };

        // Smooth the gain itself to avoid zipper noise
        self.gain = self.gain * 0.99 + target_gain * 0.01;

        (l * self.gain, r * self.gain)
    }

    pub fn reset(&mut self) {
        self.envelope = 0.0;
        self.gain = 0.0;
    }
}

// ---------------------------------------------------------------------------
// Pan — equal-power cosine law
// ---------------------------------------------------------------------------

/// Apply equal-power stereo panning.
///
/// - `pan`: -1.0 (full left) to +1.0 (full right), 0.0 = center
#[inline]
pub fn apply_pan(l: f32, r: f32, pan: f32) -> (f32, f32) {
    let angle = (pan.clamp(-1.0, 1.0) + 1.0) * 0.25 * PI; // 0 to PI/2
    let gain_l = angle.cos();
    let gain_r = angle.sin();
    (l * gain_l, r * gain_r)
}

// ---------------------------------------------------------------------------
// Soft clipper — knee-based limiter
// ---------------------------------------------------------------------------

/// Soft-clip a sample with a smooth knee.
///
/// **Fully transparent** (linear, bit-exact) below the knee point.
/// Above the knee, exponential saturation smoothly approaches ±1.0.
/// C1 continuous (no jump in value or derivative at the knee).
///
/// - `knee`: 0.0..1.0 — below this level the signal passes through unchanged.
///   Typical values: 0.85 (per-stem limiter), 0.95 (master, very gentle).
#[inline]
pub fn soft_clip(sample: f32, knee: f32) -> f32 {
    let abs_x = sample.abs();
    if abs_x <= knee {
        sample
    } else {
        let range = 1.0 - knee;
        let overshoot = abs_x - knee;
        let saturated = knee + range * (1.0 - (-overshoot / range).exp());
        sample.signum() * saturated
    }
}

/// Soft-clip a stereo pair.
#[inline]
pub fn soft_clip_stereo(l: f32, r: f32, knee: f32) -> (f32, f32) {
    (soft_clip(l, knee), soft_clip(r, knee))
}

// ---------------------------------------------------------------------------
// Per-stem FX state (owned by mixer thread, never shared)
// ---------------------------------------------------------------------------

pub struct StemFxState {
    pub gate: NoiseGate,
    pub eq: ThreeBandEq,
    pub delay: StereoDelay,
    pub last_eq_low: f32,
    pub last_eq_mid: f32,
    pub last_eq_high: f32,
    // Cached gate threshold (avoids powf per sample)
    pub last_gate_threshold_db: f32,
    pub gate_threshold_lin: f32,
}

impl StemFxState {
    pub fn new(sr: u32) -> Self {
        let default_threshold_db = -40.0f32;
        Self {
            gate: NoiseGate::new(sr),
            eq: ThreeBandEq::new(sr),
            delay: StereoDelay::new(sr, 1000.0),
            last_eq_low: 0.0,
            last_eq_mid: 0.0,
            last_eq_high: 0.0,
            last_gate_threshold_db: default_threshold_db,
            gate_threshold_lin: 10.0_f32.powf(default_threshold_db / 20.0),
        }
    }

    pub fn reset(&mut self) {
        self.gate.reset();
        self.eq.reset();
        self.delay.reset();
    }

    /// Get cached linear gate threshold, recalculating only when dB value changes.
    #[inline]
    pub fn gate_threshold_linear(&mut self, threshold_db: f32) -> f32 {
        if threshold_db != self.last_gate_threshold_db {
            self.gate_threshold_lin = 10.0_f32.powf(threshold_db / 20.0);
            self.last_gate_threshold_db = threshold_db;
        }
        self.gate_threshold_lin
    }

    /// Update EQ coefficients only when gains have changed.
    #[inline]
    pub fn maybe_update_eq(&mut self, low_db: f32, mid_db: f32, high_db: f32, sr: u32) {
        if low_db != self.last_eq_low
            || mid_db != self.last_eq_mid
            || high_db != self.last_eq_high
        {
            self.eq.update_gains(low_db, mid_db, high_db, sr);
            self.last_eq_low = low_db;
            self.last_eq_mid = mid_db;
            self.last_eq_high = high_db;
        }
    }
}
