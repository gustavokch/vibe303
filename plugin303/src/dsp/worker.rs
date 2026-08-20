use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender, TrySendError};
use std::sync::{Arc, OnceLock};
use std::thread;

use super::vco::{Vco303, LUT_K};

pub struct LutJob {
    pub sr: f32,
    pub instance_id: u32,
    pub is_fun: bool,
    pub age: i32,
    pub tolerance: f64,
    pub recal_scale: f32,
    pub seed: u32,
    pub sequence: u64,
    pub sequence_checker: Arc<AtomicU64>,
    pub on_complete: Arc<dyn Fn([[f64; LUT_K]; 22]) + Send + Sync>,
}

static WORKER_CHANNEL: OnceLock<SyncSender<LutJob>> = OnceLock::new();

pub fn get_worker_sender() -> &'static SyncSender<LutJob> {
    WORKER_CHANNEL.get_or_init(|| {
        let (tx, rx) = sync_channel::<LutJob>(16);
        thread::Builder::new()
            .name("vibe303-lut-worker".to_string())
            .spawn(move || {
                while let Ok(job) = rx.recv() {
                    // Check if superseded before starting
                    if job.sequence_checker.load(Ordering::Acquire) != job.sequence {
                        continue;
                    }

                    let mut local_luts = [[0.0f64; LUT_K]; 22];

                    if job.age == 0 && job.tolerance == 0.0 && job.recal_scale == 1.0 {
                        let lut = Vco303::cv_lut_for(job.sr, 0, job.is_fun);
                        local_luts = [lut; 22];
                    } else {
                        let mut temp_vco = Vco303::raw(job.sr);
                        temp_vco.dense_solver = false;
                        temp_vco.set_instance_id(job.instance_id);
                        temp_vco.set_fun(job.is_fun);
                        temp_vco.set_analog_wear(job.tolerance, job.age, job.seed);

                        temp_vco.set_thermal_internal(0);
                        let base_lut = temp_vco.solve_cv_lut_with_recal(job.recal_scale);
                        local_luts[0] = base_lut;
                        for noise_step in 1..22 {
                            let temp_ratio = (273.15 + 25.0 + noise_step as f64) / 298.15;
                            for i in 0..LUT_K {
                                local_luts[noise_step as usize][i] = base_lut[i] * temp_ratio;
                            }
                        }
                    }

                    if job.sequence_checker.load(Ordering::Acquire) == job.sequence {
                        (job.on_complete)(local_luts);
                    }
                }
            })
            .expect("Failed to spawn background LUT worker");
        tx
    })
}

pub fn submit_lut_job(job: LutJob) -> bool {
    match get_worker_sender().try_send(job) {
        Ok(()) => true,
        Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => false,
    }
}
