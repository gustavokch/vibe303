use std::sync::mpsc::{sync_channel, SyncSender, TrySendError};
use std::sync::{Arc, LazyLock};
use std::thread;

use super::vco::{Vco303, VcoLutBuffer};

/// One LUT request (see `Vco303::request_luts`): plain data plus an `Arc`
/// clone of the target buffer, so building and sending it on the audio thread
/// does not allocate.
pub struct LutJob {
    pub sr: f32,
    pub instance_id: u32,
    pub is_fun: bool,
    pub age: i32,
    pub tolerance: f64,
    pub recal_scale: f64,
    pub seed: u32,
    /// Request number; the job is dropped once `buffer` has a newer request.
    pub sequence: u64,
    pub buffer: Arc<VcoLutBuffer>,
}

/// Job queue of the LUT worker thread, spawned on first use.
static WORKER_CHANNEL: LazyLock<SyncSender<LutJob>> = LazyLock::new(|| {
    let (tx, rx) = sync_channel::<LutJob>(16);
    thread::Builder::new()
        .name("vibe303-lut-worker".to_string())
        .spawn(move || {
            while let Ok(job) = rx.recv() {
                let current = || job.buffer.is_current(job.sequence);
                if !current() {
                    continue;
                }
                let Some(luts) = Vco303::solve_luts(
                    job.sr,
                    job.instance_id,
                    job.is_fun,
                    job.age,
                    job.tolerance,
                    job.seed,
                    job.recal_scale,
                    current,
                ) else {
                    continue;
                };
                if current() {
                    job.buffer.store(luts, job.sequence);
                }
            }
        })
        .expect("Failed to spawn background LUT worker");
    tx
});

/// Start the LUT worker thread if it is not running. Spawning allocates, so
/// the plugin calls this from `initialize`, before the audio thread submits.
pub fn start() {
    LazyLock::force(&WORKER_CHANNEL);
}

/// Queue a LUT job without blocking. RT-safe once [`start`] has run. `false`
/// if the queue is full.
pub fn submit_lut_job(job: LutJob) -> bool {
    match WORKER_CHANNEL.try_send(job) {
        Ok(()) => true,
        Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => false,
    }
}
