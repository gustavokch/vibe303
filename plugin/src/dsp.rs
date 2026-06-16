//! Per-channel DSP core shared by the VST3 `process` and the offline standalone.
//!
//! Both the realtime plugin and the offline renderer MUST run the identical
//! signal chain so the Phase-4 null test (standalone vs SPICE) reflects exactly
//! what the plugin does. That chain lives here, in `ChannelDsp`, and is used by
//! both — never duplicated.
//!
//! Chain: `driven = x * drive` -> model (directly when the host runs near the
//! model's native 48 kHz, else via the [`HostBridge`] that decimates the host to
//! ~48 kHz and interpolates back) -> `* out_gain` = wet. The dry path is delayed
//! by the wet-path latency so the Mix blend stays phase-aligned: the bridge
//! latency is `HOST_BRIDGE_LATENCY` host-rate samples when bridging (see
//! `Ba283Plugin::latency_samples`, the single source of truth) and 0 when the
//! model runs directly at the host rate.
//!
//! `process_sample` is zero-alloc: it only calls the existing zero-alloc
//! model/bridge/delay methods plus arithmetic.

use crate::adaa::{Adaa2Sat, SatTables};
use crate::dcblock::DcBlock;
use crate::delay::ShortDelay;
use crate::model::Ba283Model;
use crate::oversample::{HostBridge, HOST_BRIDGE_LATENCY};
use crate::wdf::{QualityConfig, WdfModel, WdfSnapshot};
use crate::InferenceBackend;

/// Crossfade / resume-fade window (seconds). Masks the discontinuity of a WDF tier/precision switch
/// (model rebuild + snapshot restore + zeroed biquads + operating-point DC step) and the wet snap on
/// plugin re-enable. 60 ms keeps the per-sample slope — and the DC blocker's response to the
/// operating-point step — well below audibility, while still feeling instant for a setting change.
const WDF_XFADE_S: f32 = 0.060;

/// Dry-path delay depth on the bridged path. MUST equal `Ba283Plugin::latency_samples()` when the
/// host bridge is active (`HOST_BRIDGE_LATENCY`) so the Mix blend stays phase-aligned. When the model
/// runs directly at the host rate the dry delay is bypassed entirely.
pub const OS_LATENCY_SAMPLES: usize = HOST_BRIDGE_LATENCY;

/// Host sample rate at or above which the model runs through the single 2x [`HostBridge`] (decimate
/// to host/2, interpolate back) instead of directly at the host rate. This is the single threshold
/// the latency report, dry delay, and process path all read.
///
/// The model is trained at 48 kHz and its LSTM time constants are rate-specific, so the effective
/// model rate matters for fidelity. The bridge is a *single* 2x stage, so the model always runs at
/// `host_rate / 2` when bridging:
///
/// | host rate | path     | model runs at | note                                              |
/// |-----------|----------|---------------|---------------------------------------------------|
/// | 44.1 kHz  | direct   | 44.1 kHz      | ~8% below 48k training rate (negligible)          |
/// | 48 kHz    | direct   | 48 kHz        | exact                                             |
/// | 64 kHz    | direct   | 64 kHz        | ~33% above 48k — notable SR mismatch (rare host)  |
/// | 88.2 kHz  | bridged  | 44.1 kHz      | ~8% below 48k (good)                              |
/// | 96 kHz    | bridged  | 48 kHz        | exact (the design target)                         |
/// | 176.4 kHz | bridged  | 88.2 kHz      | ~2x the training rate — degraded; one 2x stage    |
/// | 192 kHz   | bridged  | 96 kHz        | ~2x the training rate — degraded; one 2x stage    |
///
/// KNOWN LIMITATION: 176.4/192 kHz hosts land the model at 88.2/96 kHz (double the 48 kHz training
/// rate), and 64 kHz hosts run it ~33% fast. A second (4x) bridge stage would fix the top two rates
/// (176.4/192 -> 44.1/48) — deferred; documented rather than implemented.
pub const HOST_BRIDGE_THRESHOLD_HZ: f32 = 72_000.0;

/// Full-scale input voltage convention from the training data: 0 dBFS (a host sample of 1.0) maps to
/// 0.5 V at the circuit input (`data_gen/stimulus.py`: "0 dBFS == 0.5 V"; dry block peak-normalized
/// to 0.5 V). This closes the #30 de-norm contract.
///
/// NeuralNet input is fed `x` directly: the trained `input_scale = 2.0` cancels this 0.5
/// (`x * 0.5 V/dBFS * 2.0 = x`), so we must NOT pre-multiply by `input_scale`. NeuralNet output is
/// de-normed by `output_ref_v / INPUT_FS_VOLTS` (= x128 at the canonical 64 V ref): recover volts,
/// then express them in the same dBFS convention. The WDF circuit is fed/observed in real volts, so
/// its input is `driven * INPUT_FS_VOLTS` and its output is divided by `INPUT_FS_VOLTS`. Both
/// backends therefore reference the same 0 dBFS ≡ 0.5 V scale and are level-matched.
pub const INPUT_FS_VOLTS: f32 = 0.5;

pub struct ChannelDsp {
    model: Ba283Model,
    bridge: HostBridge,
    dry_delay: ShortDelay<OS_LATENCY_SAMPLES>,
    /// Circuit-faithful backend. Self-oversamples internally (OS=2) and decimates with IIR
    /// biquads, so its wet path has ~0 integer latency.
    wdf: WdfModel,
    /// Delays the WDF blended output by `OS_LATENCY_SAMPLES` when the host bridge is active so the
    /// host-reported latency stays valid for BOTH backends (no renegotiation when switching).
    wdf_out_delay: ShortDelay<OS_LATENCY_SAMPLES>,
    /// Last settled snapshot installed into `wdf` (from the shared warmup). Carried across a tier
    /// rebuild so switching tiers does not require a fresh (expensive) warmup.
    wdf_snap: Option<WdfSnapshot>,
    /// The pre-switch WDF model, kept alive during a crossfade so the old output can be blended out
    /// (equal-gain smoothstep) against the new model's startup transient. Pre-allocated once at
    /// construction and reused — a tier switch moves the old model into this existing box (no
    /// audio-thread `Box` alloc). Only read while `xfade_remain > 0`; otherwise it holds a stale model.
    wdf_prev: Box<WdfModel>,
    /// Samples remaining in the active crossfade (counts down to 0). 0 = not crossfading.
    xfade_remain: u32,
    /// Total crossfade length in samples (set from the host SR at each tier switch).
    xfade_len: u32,
    /// Samples remaining in the resume fade-in (counts down to 0). Armed on `reset()` so that
    /// re-enabling the plugin (host deactivate→activate) ramps the wet up from zero instead of
    /// snapping the full-gain circuit output on — which is a VERY loud click against the dry signal.
    resume_remain: u32,
    /// DC blocker on the final blended output — kills operating-point DC offsets from G0 re-stamps.
    dcblock: DcBlock,
    /// Optional ADAA2 output-saturation stage (memoryless, antialiased). Bypassed when `sat_on` is
    /// false so the dry-of-saturation path stays bit-exact.
    sat: Adaa2Sat,
    /// De-normed wet (volts→dBFS) from the most recent `process_sample`, BEFORE the levelNorm gain,
    /// `out_gain`, and `mix` are applied. The plugin reads this to feed the stereo-linked levelNorm
    /// RMS estimator (the norm gain is computed across both channels, so it lives one level up).
    last_wet: f32,
}

impl ChannelDsp {
    pub fn new() -> Self {
        Self {
            model: Ba283Model::new_from_embedded_weights(),
            bridge: HostBridge::new(),
            dry_delay: ShortDelay::new(),
            // Placeholder SR/bias; rebuilt + restored with the real settings in `initialize`.
            wdf: WdfModel::new(48_000.0, 220.0, 0.5),
            wdf_out_delay: ShortDelay::new(),
            wdf_snap: None,
            // Pre-allocate the crossfade scratch model once (off the audio thread, at construction)
            // so a live tier switch reuses it instead of `Box::new`-ing on the audio thread.
            wdf_prev: Box::new(WdfModel::new(48_000.0, 220.0, 0.5)),
            xfade_remain: 0,
            xfade_len: 0,
            resume_remain: 0,
            dcblock: DcBlock::new(),
            sat: Adaa2Sat::new(SatTables::build()),
            last_wet: 0.0,
        }
    }

    /// De-normed wet (pre-levelNorm/out_gain/mix) from the last `process_sample` — for the
    /// stereo-linked levelNorm RMS estimator in the plugin.
    #[inline]
    pub fn last_wet(&self) -> f32 {
        self.last_wet
    }

    /// Replace the NeuralNet model (offline tools only — e.g. load a different trained export than
    /// the embedded one for an A/B). The de-norm chain reads `output_ref_v` from the new model.
    pub fn set_model(&mut self, model: Ba283Model) {
        self.model = model;
    }

    pub fn reset(&mut self) {
        self.model.reset();
        self.bridge.reset();
        self.dry_delay.reset();
        self.wdf.reset(); // restore the settled snapshot installed in initialize
        self.wdf_out_delay.reset();
        self.xfade_remain = 0; // cancels any in-flight crossfade (wdf_prev goes unread)
        // Arm the resume fade-in so re-enabling the plugin ramps the wet up instead of snapping the
        // full-gain circuit output on (a loud click). Falls back to NO fade if no SR is known yet
        // (xfade_len == 0 -> resume_remain == 0), which is harmless: there is nothing to ramp.
        self.resume_remain = self.xfade_len;
        self.dcblock.reset();
        self.sat.reset();
        self.last_wet = 0.0;
    }

    // --- WDF block-rate config (used by Phase D plugin wiring) ---

    /// Rebuild the WDF model at the real host SR / gain / bias (e.g. in `initialize`) at the
    /// realtime tier (all levers off). Also sizes the crossfade window and prepares the DC blocker
    /// from the host SR (this is the SR-setup path; a live tier switch keeps the DC-blocker state).
    pub fn wdf_rebuild(&mut self, host_sr: f32, rgain: f32, bias: f32) {
        self.wdf_rebuild_with(QualityConfig::realtime(), host_sr, rgain, bias);
        self.dcblock.prepare(host_sr);
        self.xfade_remain = 0; // no crossfade in progress after a full rebuild
    }

    /// Rebuild the WDF model with an explicit [`QualityConfig`] (tier/lever selection).
    pub fn wdf_rebuild_with(&mut self, cfg: QualityConfig, host_sr: f32, rgain: f32, bias: f32) {
        self.wdf = WdfModel::new_with(cfg, host_sr as f64, rgain as f64, bias as f64);
        self.xfade_len = (host_sr * WDF_XFADE_S).max(1.0) as u32;
    }

    /// Switch the WDF tier live: rebuild at `cfg` (rebuilds G0 + the Schur partition) and re-install
    /// the carried settled snapshot so no fresh warmup is needed. The DC operating point is
    /// tier-independent (freeze-caps only changes the per-iter companion), so the restored state
    /// re-settles cleanly. The rebuild also zeroes the decimation biquads and jumps the operating
    /// point, so the pre-switch model is kept alive and the output is equal-gain (smoothstep)
    /// crossfaded from old to new over `xfade_len` samples — masking the click the raw switch would
    /// otherwise produce.
    ///
    /// NOTE: `WdfModel::new_with` rebuilds the circuit and still allocates its `Vec` build scratch, so
    /// this is NOT alloc-free — call only on an ACTUAL tier/precision change (a user dropdown gesture,
    /// block-rate), never per sample. The old model is moved into the pre-allocated `wdf_prev` box, so
    /// no `Box` is allocated here; making the rebuild itself alloc-free would need an off-thread model
    /// handoff (a follow-up, mirroring the off-thread levelNorm probe).
    pub fn wdf_set_tier(&mut self, cfg: QualityConfig, host_sr: f32, rgain: f32, bias: f32) {
        // Build the new model and swap it in, moving the old (settled) model into the pre-allocated
        // crossfade box so it can fade out against the new one's startup transient (zeroed biquads +
        // operating-point jump) — no audio-thread `Box` alloc for the carry-over.
        let new = WdfModel::new_with(cfg, host_sr as f64, rgain as f64, bias as f64);
        self.xfade_len = (host_sr * WDF_XFADE_S).max(1.0) as u32;
        *self.wdf_prev = std::mem::replace(&mut self.wdf, new);
        if let Some(snap) = self.wdf_snap {
            self.wdf.restore(&snap);
            self.wdf.save_snapshot();
        }
        self.xfade_remain = self.xfade_len;
    }

    /// Install a settled snapshot (from a single shared warmup) and make it the reset target.
    pub fn wdf_restore(&mut self, snap: &WdfSnapshot) {
        self.wdf.restore(snap);
        self.wdf.save_snapshot();
        self.wdf_snap = Some(*snap);
    }

    /// Measure the settled circuit's static voltage gain (for the one-shot levelNorm trim). Probes a
    /// scratch clone of the WDF model — does not disturb the live state. Block-rate only.
    pub fn wdf_measure_gain(&self) -> f32 {
        self.wdf.measure_gain()
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

    /// Live aging change: drift all caps + resistors from base, rebuild G0 (preserves state).
    pub fn wdf_set_age(&mut self, age: f32) {
        self.wdf.set_age(age as f64);
    }

    /// Re-roll the aged "unit": regenerate the drift pattern from `seed`, re-apply at current age.
    pub fn wdf_set_age_seed(&mut self, seed: u64) {
        self.wdf.set_age_seed(seed);
    }

    /// Live transformer ON/OFF (feature `trafo-core`). OFF => bit-exact linear core.
    #[cfg(feature = "trafo-core")]
    pub fn wdf_set_trafo_enabled(&mut self, on: bool) {
        self.wdf.set_trafo_enabled(on);
    }

    /// Live transformer voicing: HYSTERESIS + SATURATION dials (each 0..1; feature `trafo-core`).
    #[cfg(feature = "trafo-core")]
    pub fn wdf_set_trafo_voicing(&mut self, hyst: f32, sat: f32) {
        self.wdf.set_trafo_voicing(hyst as f64, sat as f64);
    }

    /// Zero the WDF Newton-iteration counters (offline A/B: call after warmup, before the audio pass).
    pub fn wdf_reset_iter_stats(&mut self) {
        self.wdf.reset_iter_stats();
    }

    /// WDF Newton-iteration stats since the last reset: `(samples, avg_iters, max_iters)`.
    pub fn wdf_iter_stats(&self) -> (u64, f64, usize) {
        self.wdf.iter_stats()
    }

    /// Run one sample through the selected backend. `drive`/`out_gain`/`norm_gain` are linear gains,
    /// `mix` is the wet fraction in 0..1. `bias` drives the NeuralNet path only — the WDF path uses
    /// the block-rate bias/gain set via `wdf_set_bias`/`wdf_set_rgain`. The host-reported latency
    /// (`Ba283Plugin::latency_samples`) is honoured for BOTH backends.
    ///
    /// `norm_gain` is the stereo-linked levelNorm gain (1.0 when levelNorm is off); it is applied to
    /// the de-normed wet BEFORE `out_gain` and `mix`. The pre-norm de-normed wet is stashed in
    /// `last_wet` so the plugin can drive the levelNorm RMS estimator across both channels.
    #[allow(clippy::too_many_arguments)]
    pub fn process_sample(
        &mut self,
        x: f32,
        drive: f32,
        bias: f32,
        out_gain: f32,
        norm_gain: f32,
        mix: f32,
        use_bridge: bool,
        backend: InferenceBackend,
        sat_on: bool,
        sat_amount: f32,
    ) -> f32 {
        let driven = x * drive;
        match backend {
            InferenceBackend::NeuralNet => {
                // De-norm: recover volts (`* output_ref_v`) then express in the 0 dBFS ≡ 0.5 V
                // convention (`/ INPUT_FS_VOLTS`). Input stays `driven` (input_scale cancels 0.5).
                let deref = self.model.output_ref_v() / INPUT_FS_VOLTS;
                // Disjoint field borrows so the bridge closure can call the model.
                let (model, bridge) = (&mut self.model, &mut self.bridge);
                let model_out = if use_bridge {
                    // Host runs ~2x the model rate: decimate to ~48 kHz, run the model once per host
                    // pair, interpolate back (HostBridge is itself zero-alloc).
                    bridge.process(driven, |lo| model.forward(&[lo, bias])[0])
                } else {
                    // Host runs at ~the model rate: evaluate the model directly.
                    model.forward(&[driven, bias])[0]
                };
                let wet = model_out * deref;
                self.last_wet = wet;
                let wet = wet * norm_gain * out_gain;
                // Optional antialiased output saturation (post-level, pre-mix). Bypassed when off.
                let wet = if sat_on { self.sat.process(wet, sat_amount) } else { wet };
                // Dry path delayed to match the wet-path (bridge) latency so Mix aligns.
                let dry_aligned = if use_bridge { self.dry_delay.push_pop(x) } else { x };
                wet * mix + dry_aligned * (1.0 - mix)
            }
            InferenceBackend::Wdf => {
                // The circuit is fed/observed in volts: dBFS->volts in (`* INPUT_FS_VOLTS`),
                // volts->dBFS out (`/ INPUT_FS_VOLTS`), matching the SPICE 0.5 V stimulus.
                let in_v = driven * INPUT_FS_VOLTS;
                let wet = if self.xfade_remain > 0 {
                    // Tier/precision switch in progress: equal-gain smoothstep crossfade the
                    // pre-switch model out against the new model so the rebuild/biquad-reset step is
                    // inaudible. `wdf_prev` holds the old model for the duration of the fade.
                    let y_new = self.wdf.process_host_sample(in_v);
                    let y_old = self.wdf_prev.process_host_sample(in_v);
                    self.xfade_remain -= 1;
                    let t = 1.0 - self.xfade_remain as f32 / self.xfade_len.max(1) as f32; // 0 -> 1
                    // SMOOTHSTEP crossfade with gains summing to 1 (equal-gain, NOT equal-power). The
                    // two models render almost the same signal (same circuit, near-identical solver),
                    // so they are highly correlated — an equal-power fade would add a +3 dB bump.
                    // Smoothstep (s = t²(3−2t)) has ZERO slope at both ends, so there is no
                    // gain-derivative corner at the fade boundaries — a linear fade's corners click
                    // audibly in silence (no signal to mask them), which is what surfaced when
                    // switching to Medium with the transport stopped.
                    let s = t * t * (3.0 - 2.0 * t);
                    let (g_old, g_new) = (1.0 - s, s);
                    (y_old * g_old + y_new * g_new) / INPUT_FS_VOLTS
                } else {
                    self.wdf.process_host_sample(in_v) / INPUT_FS_VOLTS
                };
                // DC blocker on the WET path only (before mix): kills operating-point DC offsets
                // from G0 re-stamps without touching the clean dry path — a fully-dry (mix=0) bypass
                // stays bit-exact.
                let wet = self.dcblock.process(wet);
                // Resume fade-in: ramp the wet up from zero (smoothstep) after a reset so re-enabling
                // the plugin does not snap the full-gain circuit output on against the dry signal.
                let wet = if self.resume_remain > 0 {
                    self.resume_remain -= 1;
                    let t = 1.0 - self.resume_remain as f32 / self.xfade_len.max(1) as f32; // 0 -> 1
                    wet * (t * t * (3.0 - 2.0 * t))
                } else {
                    wet
                };
                self.last_wet = wet;
                let wet = wet * norm_gain * out_gain;
                // Optional antialiased output saturation (post-level, pre-mix). Bypassed when off.
                let wet = if sat_on { self.sat.process(wet, sat_amount) } else { wet };
                // WDF decimation is IIR -> ~0 integer latency; dry is undelayed here.
                let blended = wet * mix + x * (1.0 - mix);
                // Keep the host-reported latency honest: when the NeuralNet path would be bridged
                // (and report HOST_BRIDGE_LATENCY), delay the blended WDF output by the same amount
                // so switching backends needs no latency renegotiation. When the model runs direct
                // (NN latency 0) the WDF output is undelayed too.
                if use_bridge {
                    self.wdf_out_delay.push_pop(blended)
                } else {
                    blended
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
