use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

use demucs_core::listener::{ForwardEvent, ForwardListener};

use crate::state::WorkerUpdate;

pub struct GuiListener {
    tx: Sender<WorkerUpdate>,
    cancelled: Arc<AtomicBool>,
    step: usize,
    total_steps: usize,
}

impl GuiListener {
    pub fn new(
        tx: Sender<WorkerUpdate>,
        cancelled: Arc<AtomicBool>,
        n_models: usize,
        num_chunks: usize,
    ) -> Self {
        Self {
            tx,
            cancelled,
            step: 0,
            total_steps: n_models * 18 * num_chunks,
        }
    }
}

impl ForwardListener for GuiListener {
    fn on_event(&mut self, event: ForwardEvent) {
        match &event {
            ForwardEvent::EncoderDone {
                domain,
                layer,
                num_layers,
                ..
            } => {
                self.step += 1;
                let _ = self.tx.send(WorkerUpdate::ForwardProgress {
                    step: self.step,
                    total_steps: self.total_steps,
                    description: format!("Encoder {} {}/{}", domain, layer + 1, num_layers),
                });
            }
            ForwardEvent::DecoderDone {
                domain,
                layer,
                num_layers,
                ..
            } => {
                self.step += 1;
                let _ = self.tx.send(WorkerUpdate::ForwardProgress {
                    step: self.step,
                    total_steps: self.total_steps,
                    description: format!("Decoder {} {}/{}", domain, layer + 1, num_layers),
                });
            }
            ForwardEvent::TransformerDone { .. } => {
                self.step += 1;
                let _ = self.tx.send(WorkerUpdate::ForwardProgress {
                    step: self.step,
                    total_steps: self.total_steps,
                    description: "Transformer".into(),
                });
            }
            ForwardEvent::Denormalized => {
                self.step += 1;
                let _ = self.tx.send(WorkerUpdate::ForwardProgress {
                    step: self.step,
                    total_steps: self.total_steps,
                    description: "Denormalization".into(),
                });
            }
            ForwardEvent::ChunkStarted { index, total } => {
                let _ = self.tx.send(WorkerUpdate::ChunkProgress {
                    chunk: *index,
                    total_chunks: *total,
                });
            }
            _ => {}
        }
    }

    fn wants_stats(&self) -> bool {
        false
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}
