use anyhow::{bail, Context, Result};
use hound::{SampleFormat, WavSpec, WavWriter};
use std::path::Path;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// Read a stereo audio file, returning (left, right, sample_rate).
pub fn read_audio(path: &Path) -> Result<(Vec<f32>, Vec<f32>, u32)> {
    let file = std::fs::File::open(path)
        .with_context(|| format!("Failed to open audio file: {}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .with_context(|| format!("Unsupported audio format: {}", path.display()))?;

    let mut format = probed.format;

    let track = format
        .default_track()
        .context("No audio track found")?
        .clone();

    let channels_hint = track.codec_params.channels.map(|c| c.count());
    if let Some(ch) = channels_hint {
        if ch > 2 {
            bail!("Expected mono or stereo audio, got {} channel(s).", ch);
        }
    }

    let sample_rate = track
        .codec_params
        .sample_rate
        .context("Could not determine sample rate")?;

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .context("Failed to create audio decoder")?;

    let mut left = Vec::new();
    let mut right = Vec::new();
    let mut channels: Option<usize> = channels_hint;

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(ref e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(e) => return Err(e).context("Error reading audio packet"),
        };

        if packet.track_id() != track.id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(e) => return Err(e).context("Error decoding audio"),
        };

        let spec = *decoded.spec();
        let ch = spec.channels.count();

        if channels.is_none() {
            if ch > 2 {
                bail!("Expected mono or stereo audio, got {} channel(s).", ch);
            }
            channels = Some(ch);
        }

        let n_frames = decoded.capacity();
        let mut sample_buf = SampleBuffer::<f32>::new(n_frames as u64, spec);
        sample_buf.copy_interleaved_ref(decoded);
        let samples = sample_buf.samples();

        if ch == 1 {
            for &s in samples {
                left.push(s);
                right.push(s);
            }
        } else {
            for frame in samples.chunks_exact(2) {
                left.push(frame[0]);
                right.push(frame[1]);
            }
        }
    }

    if left.is_empty() {
        bail!("No audio samples decoded from: {}", path.display());
    }

    Ok((left, right, sample_rate))
}

/// Write a stereo f32 WAV file.
pub fn write_wav(path: &Path, left: &[f32], right: &[f32], sample_rate: u32) -> Result<()> {
    let spec = WavSpec {
        channels: 2,
        sample_rate,
        bits_per_sample: 32,
        sample_format: SampleFormat::Float,
    };

    let mut writer = WavWriter::create(path, spec)
        .with_context(|| format!("Failed to create WAV file: {}", path.display()))?;

    for (l, r) in left.iter().zip(right.iter()) {
        writer.write_sample(*l)?;
        writer.write_sample(*r)?;
    }

    writer
        .finalize()
        .with_context(|| format!("Failed to finalize WAV file: {}", path.display()))?;
    Ok(())
}

/// Per-stem mix parameters for export.
pub struct StemMixParam {
    pub muted: bool,
    pub soloed: bool,
    pub gain: f32,
    pub gate_enabled: bool,
    pub gate_threshold_db: f32,
    pub pan: f32,
    pub phase_invert: bool,
    pub eq_enabled: bool,
    pub eq_low_db: f32,
    pub eq_mid_db: f32,
    pub eq_high_db: f32,
    pub reverb_send: f32,
    pub delay_enabled: bool,
    pub delay_send: f32,
    pub delay_time_ms: f32,
    pub delay_feedback: f32,
    pub limiter_enabled: bool,
}

/// Global reverb parameters for export.
pub struct ReverbParams {
    pub decay: f32,
    pub damping: f32,
}

/// Mix stems with full effects chain and write the result as a WAV file.
///
/// Processing order per stem (same as real-time mixer):
/// gate → phase invert → EQ → gain (fader) → pan → delay → accumulate mix + reverb bus
/// After all stems: reverb bus → add to mix → master gain → clamp
pub fn export_mix(
    path: &Path,
    stems: &[demucs_core::Stem],
    params: &[StemMixParam],
    master_gain: f32,
    sample_rate: u32,
    reverb_params: &ReverbParams,
) -> Result<()> {
    use crate::dsp::{self, Freeverb, StemFxState};

    if stems.is_empty() {
        bail!("No stems to mix");
    }

    let n = stems[0].left.len();
    let any_solo = params.iter().any(|p| p.soloed);

    // Initialize per-stem DSP state
    let mut fx_states: Vec<StemFxState> = params
        .iter()
        .map(|p| {
            let mut fx = StemFxState::new(sample_rate);
            if p.eq_enabled {
                fx.maybe_update_eq(p.eq_low_db, p.eq_mid_db, p.eq_high_db, sample_rate);
            }
            fx
        })
        .collect();

    let mut reverb = Freeverb::new(sample_rate);
    reverb.set_params(reverb_params.decay, reverb_params.damping);

    let mut mix_l = vec![0.0f32; n];
    let mut mix_r = vec![0.0f32; n];

    // Process frame by frame
    for i in 0..n {
        let mut frame_l = 0.0f32;
        let mut frame_r = 0.0f32;
        let mut reverb_bus_l = 0.0f32;
        let mut reverb_bus_r = 0.0f32;

        for (j, (stem, param)) in stems.iter().zip(params.iter()).enumerate() {
            if param.muted {
                continue;
            }
            if any_solo && !param.soloed {
                continue;
            }

            if i >= stem.left.len() {
                continue;
            }

            let mut l = stem.left[i];
            let mut r = stem.right[i];

            // 0. Noise gate
            if param.gate_enabled {
                let threshold_lin = fx_states[j].gate_threshold_linear(param.gate_threshold_db);
                let (gl, gr) = fx_states[j].gate.process(l, r, threshold_lin);
                l = gl;
                r = gr;
            }

            // 1. Phase invert
            if param.phase_invert {
                l = -l;
                r = -r;
            }

            // 2. EQ
            if param.eq_enabled {
                let (el, er) = fx_states[j].eq.process(l, r);
                l = el;
                r = er;
            }

            // 3. Gain (fader)
            l *= param.gain;
            r *= param.gain;

            // 3b. Soft limiter
            if param.limiter_enabled {
                let (sl, sr_val) = dsp::soft_clip_stereo(l, r, 0.85);
                l = sl;
                r = sr_val;
            }

            // 4. Pan
            let (pl, pr) = dsp::apply_pan(l, r, param.pan);
            l = pl;
            r = pr;

            // 5. Delay
            if param.delay_enabled {
                let (dl, dr) = fx_states[j].delay.process(
                    l,
                    r,
                    param.delay_send,
                    param.delay_time_ms,
                    param.delay_feedback,
                );
                l = dl;
                r = dr;
            }

            // Accumulate mix
            frame_l += l;
            frame_r += r;

            // 6. Reverb send
            if param.reverb_send > 0.001 {
                reverb_bus_l += l * param.reverb_send;
                reverb_bus_r += r * param.reverb_send;
            }
        }

        // Process reverb bus (always process to let the tail decay naturally)
        let (rev_l, rev_r) = reverb.process(reverb_bus_l, reverb_bus_r);
        frame_l += rev_l;
        frame_r += rev_r;

        // Master gain + soft clip (knee-based, transparent below 0.95)
        frame_l *= master_gain;
        frame_r *= master_gain;
        let (cl, cr) = dsp::soft_clip_stereo(frame_l, frame_r, 0.95);
        mix_l[i] = cl;
        mix_r[i] = cr;
    }

    write_wav(path, &mix_l, &mix_r, sample_rate)
}

/// Trim long silences from a stereo audio signal.
///
/// Detects regions where both channels stay below `threshold_db` for at least
/// `min_silence_secs`. Those regions are replaced by `replacement_secs` of silence.
/// Returns new (left, right) vectors.
pub fn trim_silence(
    left: &[f32],
    right: &[f32],
    sample_rate: u32,
    threshold_db: f32,
    min_silence_secs: f32,
    replacement_secs: f32,
) -> (Vec<f32>, Vec<f32>) {
    let threshold_linear = 10.0_f32.powf(threshold_db / 20.0);
    let min_silence_samples = (min_silence_secs * sample_rate as f32) as usize;
    let replacement_samples = (replacement_secs * sample_rate as f32) as usize;

    // Find silent regions: contiguous runs where max(|L|, |R|) < threshold
    let n = left.len();
    let mut regions: Vec<(usize, usize)> = Vec::new(); // (start, end) exclusive
    let mut silence_start: Option<usize> = None;

    for i in 0..n {
        let amp = left[i].abs().max(right[i].abs());
        if amp < threshold_linear {
            if silence_start.is_none() {
                silence_start = Some(i);
            }
        } else {
            if let Some(start) = silence_start {
                let len = i - start;
                if len >= min_silence_samples {
                    regions.push((start, i));
                }
                silence_start = None;
            }
        }
    }
    // Handle trailing silence
    if let Some(start) = silence_start {
        let len = n - start;
        if len >= min_silence_samples {
            regions.push((start, n));
        }
    }

    if regions.is_empty() {
        return (left.to_vec(), right.to_vec());
    }

    // Build output: copy non-silent parts, insert replacement silence for each region
    let mut out_l = Vec::with_capacity(n);
    let mut out_r = Vec::with_capacity(n);
    let mut pos = 0;

    for (start, end) in &regions {
        // Copy audio before this silent region
        if pos < *start {
            out_l.extend_from_slice(&left[pos..*start]);
            out_r.extend_from_slice(&right[pos..*start]);
        }
        // Insert replacement silence
        out_l.extend(std::iter::repeat(0.0_f32).take(replacement_samples));
        out_r.extend(std::iter::repeat(0.0_f32).take(replacement_samples));
        pos = *end;
    }

    // Copy remaining audio after last silent region
    if pos < n {
        out_l.extend_from_slice(&left[pos..n]);
        out_r.extend_from_slice(&right[pos..n]);
    }

    (out_l, out_r)
}
