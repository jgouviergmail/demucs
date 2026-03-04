use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;

use anyhow::{bail, Context, Result};
use demucs_core::model::metadata::{download_url, ModelInfo, StemId};
use demucs_core::provider::fs::FsProvider;
use demucs_core::provider::ModelProvider;
use demucs_core::{num_chunks, Demucs, ModelOptions};

use crate::listener::GuiListener;
use crate::state::{ModelChoice, TrimSettings, WorkerCommand, WorkerUpdate};

#[cfg(feature = "cuda")]
type B = burn::backend::cuda::Cuda;
#[cfg(feature = "cuda")]
use cubecl::config::{autotune::AutotuneConfig, cache::CacheConfig, GlobalConfig};

#[cfg(all(not(feature = "cpu"), not(feature = "cuda")))]
use burn::backend::wgpu::{graphics::AutoGraphicsApi, init_setup, RuntimeOptions};
#[cfg(all(not(feature = "cpu"), not(feature = "cuda")))]
use cubecl::config::{autotune::AutotuneConfig, cache::CacheConfig, GlobalConfig};

#[cfg(all(not(feature = "cpu"), not(feature = "cuda")))]
type B = burn::backend::wgpu::Wgpu;

#[cfg(feature = "cpu")]
type B = burn::backend::NdArray<f32>;

const STACK_SIZE: usize = 64 * 1024 * 1024;

pub fn spawn_worker(
    cmd_rx: Receiver<WorkerCommand>,
    update_tx: Sender<WorkerUpdate>,
    cancel_flag: Arc<AtomicBool>,
) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name("demucs-worker".into())
        .stack_size(STACK_SIZE)
        .spawn(move || {
            worker_loop(cmd_rx, update_tx, cancel_flag);
        })
        .expect("Failed to spawn worker thread")
}

fn worker_loop(
    cmd_rx: Receiver<WorkerCommand>,
    tx: Sender<WorkerUpdate>,
    cancel_flag: Arc<AtomicBool>,
) {
    let mut cached_model: Option<(String, Demucs<B>)> = None;
    let mut gpu_initialized = false;

    while let Ok(cmd) = cmd_rx.recv() {
        match cmd {
            WorkerCommand::Start {
                input_path,
                model_choice,
                selected_stems,
                output_dir,
                trim,
            } => {
                cancel_flag.store(false, Ordering::Relaxed);
                if let Err(e) = run_pipeline(
                    &tx,
                    &cancel_flag,
                    &mut cached_model,
                    &mut gpu_initialized,
                    input_path,
                    model_choice,
                    selected_stems,
                    output_dir,
                    trim,
                ) {
                    let _ = tx.send(WorkerUpdate::Error {
                        message: format!("{:#}", e),
                    });
                }
            }
            WorkerCommand::Cancel => {
                cancel_flag.store(true, Ordering::Relaxed);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn run_pipeline(
    tx: &Sender<WorkerUpdate>,
    cancel_flag: &Arc<AtomicBool>,
    cached_model: &mut Option<(String, Demucs<B>)>,
    gpu_initialized: &mut bool,
    input_path: PathBuf,
    model_choice: ModelChoice,
    selected_stems: Vec<StemId>,
    output_dir: PathBuf,
    trim: Option<TrimSettings>,
) -> Result<()> {
    let info = model_choice.info();

    // 1. Build model options
    let opts = build_options(info, &selected_stems);

    // 2. Ensure model weights are available
    let provider = FsProvider::new().context("Failed to initialize model cache")?;
    let bytes = if provider.is_cached(info) {
        provider
            .load_cached(info)
            .context("Failed to load cached model")?
    } else {
        let data = download_with_progress(info, tx, cancel_flag)?;
        provider
            .cache_model(info, &data)
            .context("Failed to cache model")?;
        data
    };

    if cancel_flag.load(Ordering::Relaxed) {
        bail!("Cancelled");
    }

    // 3. Initialize GPU backend (once)
    if !*gpu_initialized {
        init_gpu_backend();
        *gpu_initialized = true;
    }

    // 4. Load model (or reuse cache)
    // For htdemucs_ft, stems are baked into ModelOptions, so different stems = reload
    let cache_key = if info.id == "htdemucs_ft" {
        let mut key = info.id.to_string();
        for s in &selected_stems {
            key.push('_');
            key.push_str(s.as_str());
        }
        key
    } else {
        info.id.to_string()
    };

    let need_reload = cached_model
        .as_ref()
        .map(|(id, _)| id != &cache_key)
        .unwrap_or(true);

    if need_reload {
        let _ = tx.send(WorkerUpdate::ModelLoading);
        let device = Default::default();
        let model =
            Demucs::<B>::from_bytes(opts, &bytes, device).context("Failed to load model")?;
        let _ = tx.send(WorkerUpdate::ModelLoaded);
        *cached_model = Some((cache_key, model));
    }

    let model = &cached_model.as_ref().unwrap().1;

    // 5. Read audio
    let (left, right, sample_rate) = crate::audio::read_audio(&input_path)?;

    if cancel_flag.load(Ordering::Relaxed) {
        bail!("Cancelled");
    }

    // 6. Run separation
    let _ = tx.send(WorkerUpdate::SeparationStarted);

    let n_models = if info.id == "htdemucs_ft" {
        selected_stems.len()
    } else {
        1
    };
    let n_samples_44k = if sample_rate != 44100 {
        (left.len() as f64 * 44100.0 / sample_rate as f64).ceil() as usize
    } else {
        left.len()
    };
    let chunks = num_chunks(n_samples_44k);

    let mut listener = GuiListener::new(tx.clone(), cancel_flag.clone(), n_models, chunks);

    let stems =
        pollster::block_on(model.separate_with_listener(&left, &right, sample_rate, &mut listener))
            .map_err(|e| anyhow::anyhow!("{}", e))?;

    let _ = tx.send(WorkerUpdate::SeparationDone);

    // 7. Write output
    let _ = tx.send(WorkerUpdate::WritingStems);
    std::fs::create_dir_all(&output_dir)
        .with_context(|| format!("Failed to create output directory: {}", output_dir.display()))?;

    let source_name = input_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");

    let mut written_stems = Vec::new();

    for stem in &stems {
        if !selected_stems.contains(&stem.id) {
            continue;
        }
        let filename = format!("{}_{}.wav", source_name, stem.id.as_str());
        let path = output_dir.join(&filename);
        crate::audio::write_wav(&path, &stem.left, &stem.right, sample_rate)?;
        let _ = tx.send(WorkerUpdate::StemWritten {
            path: path.display().to_string(),
        });
        written_stems.push(filename);

        // Trim silence on vocals if enabled
        if stem.id.as_str() == "vocals" {
            if let Some(ref trim_cfg) = trim {
                let (trimmed_l, trimmed_r) = crate::audio::trim_silence(
                    &stem.left,
                    &stem.right,
                    sample_rate,
                    trim_cfg.threshold_db,
                    trim_cfg.min_silence_secs,
                    trim_cfg.replacement_secs,
                );
                let trimmed_filename =
                    format!("{}_{}_trimmed.wav", source_name, stem.id.as_str());
                let trimmed_path = output_dir.join(&trimmed_filename);
                crate::audio::write_wav(&trimmed_path, &trimmed_l, &trimmed_r, sample_rate)?;
                let _ = tx.send(WorkerUpdate::StemWritten {
                    path: trimmed_path.display().to_string(),
                });
                written_stems.push(trimmed_filename);
            }
        }
    }

    let abs_output = output_dir.canonicalize().unwrap_or_else(|_| output_dir.clone());
    let _ = tx.send(WorkerUpdate::AllDone {
        output_dir: abs_output.display().to_string(),
        stems: written_stems,
    });

    Ok(())
}

fn build_options(info: &ModelInfo, selected: &[StemId]) -> ModelOptions {
    if info.id == "htdemucs_ft" {
        ModelOptions::FineTuned(selected.to_vec())
    } else if info.id == "htdemucs_6s" {
        ModelOptions::SixStem
    } else {
        ModelOptions::FourStem
    }
}

fn init_gpu_backend() {
    #[cfg(feature = "cuda")]
    {
        GlobalConfig::set(GlobalConfig {
            autotune: AutotuneConfig {
                cache: CacheConfig::Global,
                ..Default::default()
            },
            ..Default::default()
        });
    }

    #[cfg(all(not(feature = "cpu"), not(feature = "cuda")))]
    {
        let device: <B as Backend>::Device = Default::default();
        GlobalConfig::set(GlobalConfig {
            autotune: AutotuneConfig {
                cache: CacheConfig::Global,
                ..Default::default()
            },
            ..Default::default()
        });
        let options = RuntimeOptions {
            tasks_max: 128,
            ..Default::default()
        };
        init_setup::<AutoGraphicsApi>(&device, options);
    }
}

fn download_with_progress(
    info: &ModelInfo,
    tx: &Sender<WorkerUpdate>,
    cancel: &AtomicBool,
) -> Result<Vec<u8>> {
    let url = download_url(info);
    let _ = tx.send(WorkerUpdate::DownloadStarted {
        model_id: info.id.to_string(),
        size_mb: info.size_mb,
    });

    let tls =
        std::sync::Arc::new(ureq::native_tls::TlsConnector::new().context("Failed to init TLS")?);
    let agent = ureq::AgentBuilder::new().tls_connector(tls).build();
    let response = agent
        .get(&url)
        .call()
        .with_context(|| format!("Failed to download model from {}", url))?;

    if response.status() != 200 {
        bail!("HTTP {} when downloading {}", response.status(), url);
    }

    let total_bytes = response
        .header("Content-Length")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(info.size_mb as u64 * 1_000_000);

    let mut data = Vec::with_capacity(total_bytes as usize);
    let mut reader = response.into_reader();
    let mut buf = [0u8; 65536];
    let mut downloaded = 0u64;

    loop {
        if cancel.load(Ordering::Relaxed) {
            bail!("Download cancelled");
        }
        let n = reader.read(&mut buf).context("Failed to read download")?;
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n]);
        downloaded += n as u64;
        let _ = tx.send(WorkerUpdate::DownloadProgress {
            bytes_downloaded: downloaded,
            total_bytes,
        });
    }

    let _ = tx.send(WorkerUpdate::DownloadDone);
    Ok(data)
}
