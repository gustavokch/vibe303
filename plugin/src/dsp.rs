//! Per-channel DSP core shared by the VST3 `process` and the offline standalone.
//!
//! Both the realtime plugin and the offline renderer MUST run the identical
//! signal chain so the Phase-4 null test (standalone vs SPICE) reflects exactly
//! what the plugin does. That chain lives here, in `ChannelDsp`, and is used by
//! both — never duplicated.
//!
//! Chain: `driven = x * drive` -> model (directly at >= 96 kHz, else via the 2x
//! oversampler) -> `* out_gain` = wet. The dry path is delayed by the wet-path
//! latency so the Mix blend stays phase-aligned: the oversampler latency is 32
//! base-rate samples sub-96k (see `Ba283Plugin::latency_samples`, the single
//! source of truth) and 0 at >= 96 kHz (no oversampling, no delay).
//!
//! `process_sample` is zero-alloc: it only calls the existing zero-alloc
//! model/oversampler/delay methods plus arithmetic.

use crate::delay::ShortDelay;
use crate::model::Ba283Model;
use crate::oversample::Oversample2x;
use crate::wdf::{QualityConfig, WdfModel, WdfSnapshot};
use crate::InferenceBackend;

/// Dry-path delay depth on the sub-96k oversampling path. MUST equal
/// `Ba283Plugin::latency_samples()` below 96 kHz (32) so the Mix blend stays
/// phase-aligned. At >= 96 kHz the dry delay is bypassed entirely.
pub const OS_LATENCY_SAMPLES: usize = 32;

pub struct ChannelDsp {
    model: Ba283Model,
    os: Oversample2x,
    dry_delay: ShortDelay<OS_LATENCY_SAMPLES>,
    /// Circuit-faithful backend. Self-oversamples internally (OS=2) and decimates with IIR
    /// biquads, so its wet path has ~0 integer latency.
    wdf: WdfModel,
    /// Delays the WDF blended output by `OS_LATENCY_SAMPLES` sub-96k so the host-reported latency
    /// stays valid for BOTH backends (no renegotiation when switching).
    wdf_out_delay: ShortDelay<OS_LATENCY_SAMPLES>,
    /// Last settled snapshot installed into `wdf` (from the shared warmup). Carried across a tier
    /// rebuild so switching tiers does not require a fresh (expensive) warmup.
    wdf_snap: Option<WdfSnapshot>,
}

impl ChannelDsp {
    pub fn new() -> Self {
        Self {
            model: Ba283Model::new_from_embedded_weights(),
            os: Oversample2x::new(),
            dry_delay: ShortDelay::new(),
            // Placeholder SR/bias; rebuilt + restored with the real settings in `initialize`.
            wdf: WdfModel::new(48_000.0, 220.0, 0.5),
            wdf_out_delay: ShortDelay::new(),
            wdf_snap: None,
        }
    }

    pub fn reset(&mut self) {
        self.model.reset();
        self.os.reset();
        self.dry_delay.reset();
        self.wdf.reset(); // restore the settled snapshot installed in initialize
        self.wdf_out_delay.reset();
    }

    // --- WDF block-rate config (used by Phase D plugin wiring) ---

    /// Rebuild the WDF model at the real host SR / gain / bias (e.g. in `initialize`) at the
    /// realtime tier (all levers off).
    pub fn wdf_rebuild(&mut self, host_sr: f32, rgain: f32, bias: f32) {
        self.wdf_rebuild_with(QualityConfig::realtime(), host_sr, rgain, bias);
    }

    /// Rebuild the WDF model with an explicit [`QualityConfig`] (tier/lever selection).
    pub fn wdf_rebuild_with(&mut self, cfg: QualityConfig, host_sr: f32, rgain: f32, bias: f32) {
        self.wdf = WdfModel::new_with(cfg, host_sr as f64, rgain as f64, bias as f64);
    }

    /// Switch the WDF tier live: rebuild at `cfg` (rebuilds G0 + the Schur partition) and re-install
    /// the carried settled snapshot so no fresh warmup is needed. The DC operating point is
    /// tier-independent (freeze-caps only changes the per-iter companion), so the restored state
    /// re-settles cleanly. NOTE: the rebuild constructs a fresh model (allocates) — call at block
    /// rate on an actual tier change only, never per sample.
    pub fn wdf_set_tier(&mut self, cfg: QualityConfig, host_sr: f32, rgain: f32, bias: f32) {
        self.wdf_rebuild_with(cfg, host_sr, rgain, bias);
        if let Some(snap) = self.wdf_snap {
            self.wdf.restore(&snap);
            self.wdf.save_snapshot();
        }
    }

    /// Install a settled snapshot (from a single shared warmup) and make it the reset target.
    pub fn wdf_restore(&mut self, snap: &WdfSnapshot) {
        self.wdf.restore(snap);
        self.wdf.save_snapshot();
        self.wdf_snap = Some(*snap);
    }

    /// Warm the WDF model in place and make the settled state the reset target. Off the audio thread.
    pub fn wdf_warmup(&mut self) {
        self.wdf.warmup();
        self.wdf.save_snapshot();
        self.wdf_snap = Some(self.wdf.snapshot());
    }

    /// Capture the current WDF settled state (for sharing one warmup across channels).
    pub fn wdf_snapshot(&self) -> WdfSnapshot {
        self.wdf.snapshot()
    }

    /// Live bias change: rebuild G0 cheaply, preserving integrator state (dynamic re-settle).
    pub fn wdf_set_bias(&mut self, bias: f32) {
        self.wdf.set_bias(bias as f64);
    }

    /// Live circuit-gain (R_KJ) change: rebuild G0 cheaply, preserving integrator state.
    pub fn wdf_set_rgain(&mut self, rgain: f32) {
        self.wdf.set_rgain(rgain as f64);
    }

    /// Zero the WDF Newton-iteration counters (offline A/B: call after warmup, before the audio pass).
    pub fn wdf_reset_iter_stats(&mut self) {
        self.wdf.reset_iter_stats();
    }

    /// WDF Newton-iteration stats since the last reset: `(samples, avg_iters, max_iters)`.
    pub fn wdf_iter_stats(&self) -> (u64, f64, usize) {
        self.wdf.iter_stats()
    }

    /// Run one sample through the selected backend. `drive`/`out_gain` are linear gains, `mix` is the
    /// wet fraction in 0..1. `bias` drives the NeuralNet path only — the WDF path uses the block-rate
    /// bias/gain set via `wdf_set_bias`/`wdf_set_rgain`. The host-reported latency
    /// (`Ba283Plugin::latency_samples`) is honoured for BOTH backends.
    #[allow(clippy::too_many_arguments)]
    pub fn process_sample(
        &mut self,
        x: f32,
        drive: f32,
        bias: f32,
        out_gain: f32,
        mix: f32,
        host_ge_96k: bool,
        backend: InferenceBackend,
    ) -> f32 {
        let driven = x * drive;
        match backend {
            InferenceBackend::NeuralNet => {
                let wet = if host_ge_96k {
                    self.model.forward(&[driven, bias])[0]
                } else {
                    let (u0, u1) = self.os.upsample(driven);
                    let w0 = self.model.forward(&[u0, bias])[0];
                    let w1 = self.model.forward(&[u1, bias])[0];
                    self.os.downsample(w0, w1)
                } * out_gain;
                // Dry path delayed to match the wet-path latency so Mix aligns.
                let dry_aligned = if host_ge_96k { x } else { self.dry_delay.push_pop(x) };
                wet * mix + dry_aligned * (1.0 - mix)
            }
            InferenceBackend::Wdf => {
                let wet = self.wdf.process_host_sample(driven) * out_gain;
                // WDF decimation is IIR -> ~0 integer latency; dry is undelayed here.
                let blended = wet * mix + x * (1.0 - mix);
                // Keep the host-reported latency honest: delay the blended output by the same 32
                // sub-96k the NeuralNet path reports, so switching backends needs no renegotiation.
                if host_ge_96k {
                    blended
                } else {
                    self.wdf_out_delay.push_pop(blended)
                }
            }
        }
    }
}

impl Default for ChannelDsp {
    fn default() -> Self {
        Self::new()
    }
}
