use std::sync::mpsc::{sync_channel, SyncSender, TrySendError};
use std::sync::{Arc, LazyLock, Weak};
use std::thread;

use super::vco::{Vco303, VcoLutBuffer};

/// One LUT request (see `Vco303::request_luts`): plain data plus a weak
/// reference to the target buffer, so building and sending it on the audio
/// thread does not allocate, and a queued job does not keep its VCO's buffer
/// alive.
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
    /// The VCO's LUT store; dead once the VCO is dropped (`initialize`
    /// replaced the voice), and the job is then dropped too.
    pub buffer: Weak<VcoLutBuffer>,
}

impl LutJob {
    /// The target buffer while the job is still wanted: its VCO exists and
    /// this is the VCO's latest request.
    fn target(&self) -> Option<Arc<VcoLutBuffer>> {
        self.buffer
            .upgrade()
            .filter(|buffer| buffer.is_current(self.sequence))
    }
}

/// Job queue of the LUT worker thread, spawned on first use.
static WORKER_CHANNEL: LazyLock<SyncSender<LutJob>> = LazyLock::new(|| {
    let (tx, rx) = sync_channel::<LutJob>(16);
    thread::Builder::new()
        .name("vibe303-lut-worker".to_string())
        .spawn(move || {
            while let Ok(job) = rx.recv() {
                let wanted = || job.target().is_some();
                if !wanted() {
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
                    wanted,
                ) else {
                    continue;
                };
                if let Some(buffer) = job.target() {
                    buffer.store(luts, job.sequence, true);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn job_for(vco: &Vco303) -> LutJob {
        LutJob {
            sr: 48_000.0,
            instance_id: 0,
            is_fun: false,
            age: 0,
            tolerance: 0.0,
            recal_scale: 1.0,
            seed: 0,
            sequence: vco.lut_buffer.next_request(),
            buffer: Arc::downgrade(&vco.lut_buffer),
        }
    }

    /// Each job costs seconds of solving, so the worker must skip a job that
    /// a newer request superseded, and a job whose VCO is gone: nih-plug runs
    /// `initialize` (a new `Voice`) again after every state load, so the old
    /// voice's jobs can still be queued ahead of the new voice's request.
    #[test]
    fn stale_jobs_are_not_wanted() {
        let vco = Vco303::raw(48_000.0);
        let superseded = job_for(&vco);
        let latest = job_for(&vco);
        assert!(superseded.target().is_none(), "superseded job still wanted");
        assert!(latest.target().is_some(), "latest job of a live VCO not wanted");
        drop(vco);
        assert!(latest.target().is_none(), "job of a dropped VCO still wanted");
    }
}
