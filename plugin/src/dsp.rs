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

use crate::dcblock::DcBlock;
use crate::eq::Eq;
use crate::delay::ShortDelay;
use crate::model::Ba283Model;
use crate::oversample::{HostBridge, HOST_BRIDGE_LATENCY};
use crate::wavenet::WaveNet;
use crate::wdf::{QualityConfig, WdfModel, WdfSnapshot};
use crate::InferenceBackend;

/// Crossfade / resume-fade window (seconds). Masks the discontinuity of a WDF tier/precision switch
/// (model rebuild + snapshot restore + zeroed biquads + operating-point DC step) and the wet snap on
/// plugin re-enable. 60 ms keeps the per-sample slope — and the DC blocker's response to the
/// operating-point step — well below audibility, while still feeling instant for a setting change.
const WDF_XFADE_S: f32 = 0.060;

/// Adaptive-oversampling detector + crossfade constants (feature `adaptive-os`). The envelope is a
/// peak follower (instant attack, `OS_REL_S` exponential release) on the driven, pre-model input
/// volts. It arms a resident os=2 model above `OS_ARM_V`, disarms below `OS_DISARM_V` (−6 dB
/// hysteresis) but only after `OS_DWELL_S` minimum dwell (bounds the switch rate). The lo↔hi handoff
/// is a short smoothstep crossfade — much faster than the 60 ms tier fade because the two models
/// render the same circuit at the same operating point (highly correlated) under a loud transient.
/// Thresholds are provisional, calibrated in the Stage-1 gates (G1/G2).
#[cfg(feature = "adaptive-os")]
const OS_XFADE_S: f32 = 0.008; // 8 ms lo↔hi crossfade
#[cfg(feature = "adaptive-os")]
const OS_DWELL_S: f32 = 0.100; // 100 ms minimum dwell at hi-os (anti-thrash)
#[cfg(feature = "adaptive-os")]
const OS_REL_S: f32 = 0.050; // 50 ms envelope release (holds hi-os through the transient decay)
// Calibrated from the os=1 alias-vs-level sweep (`adaptive_os_stage0::calibrate_arm_threshold`):
// the os=1 alias floor crosses audibility (~−40 dB) at in_v ≈ 0.4–0.5 V and degrades steeply above
// (−15 dB at 0.5 V, +3 dB at 1.0 V), so arm just below the knee. Below ~0.4 V os=1 is already clean
// (≤−40 dB) → no need to oversample, full CPU savings on quiet/moderate material.
#[cfg(feature = "adaptive-os")]
const OS_ARM_V: f32 = 0.4; // arm hi-os when the input envelope (volts) exceeds this
#[cfg(feature = "adaptive-os")]
const OS_DISARM_V: f32 = 0.2; // disarm below this (−6 dB hysteresis vs OS_ARM_V)
#[cfg(feature = "adaptive-os")]
const OS_PRIME_S: f32 = 0.006; // 6 ms silent pre-warm of the incoming model before it is faded in

/// Adaptive-os crossfade phase. `Lo`/`Hi` are the steady states (only that model is solved). On a
/// transition the incoming model is re-seeded from the settled DC snapshot and then SILENTLY PRIMED
/// (`PrimeHi`/`PrimeLo`): it is solved on the live input for `OS_PRIME_S` while the outgoing model
/// still drives the output, so its biquads + operating point catch up to the current signal before
/// it becomes audible. Only then does the smoothstep crossfade (`ToHi`/`ToLo`) run. Without priming,
/// re-seeding from DC while the signal is mid-swing injects a large settling transient at the seam.
#[cfg(feature = "adaptive-os")]
#[derive(Clone, Copy, PartialEq, Eq)]
enum OsPhase {
    Lo,
    PrimeHi,
    ToHi,
    Hi,
    PrimeLo,
    ToLo,
}

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
    /// Active NeuralNet engine: the gain-conditioned streaming FiLM WaveNet (`model.cond.nam`).
    wavenet: WaveNet,
    /// Former NeuralNet engine (3×18 LSTM). Kept compiled but dormant — no longer on any signal
    /// path; retained for an easy revert until the WaveNet is fully proven in hosts. Lazily
    /// `None` in the plugin (never parses its 153 KB blob); only the offline `standalone` A/B tool
    /// populates it via `set_model`. `#[allow(dead_code)]` covers the never-read field in the
    /// cdylib build.
    #[allow(dead_code)]
    model: Option<Ba283Model>,
    bridge: HostBridge,
    dry_delay: ShortDelay<OS_LATENCY_SAMPLES>,
    /// Circuit-faithful backend. Self-oversamples internally (OS=2) and decimates with IIR
    /// biquads, so its wet path has ~0 integer latency. Boxed so a live tier/OS switch installs a
    /// worker-prebuilt model by a single pointer move (`install_prebuilt`) — no struct memcpy and
    /// no audio-thread alloc.
    wdf: Box<WdfModel>,
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
    /// Neve 1073/1076-style EQ, applied to the input signal BEFORE the model (the EQ section sits
    /// upstream of the BA283/AV card on the real strip). Bit-exact identity while disabled.
    eq: Eq,
    /// De-normed wet (volts→dBFS) from the most recent `process_sample`, BEFORE the levelNorm gain,
    /// `out_gain`, and `mix` are applied. The plugin reads this to feed the stereo-linked levelNorm
    /// RMS estimator (the norm gain is computed across both channels, so it lives one level up).
    last_wet: f32,

    // --- Adaptive oversampling (feature `adaptive-os`) ---
    /// Resident high-os (os=2) sibling of `wdf`, built block-rate alongside it on the UltraFast/Warp
    /// tiers under `Oversampling::Auto`. `None` (and the whole machine inert) on every other tier or
    /// when the user forces an oversampling factor. Solved ONLY while armed/fading — parked (cold)
    /// otherwise, which is the CPU win. Re-seeded from `wdf_snap` on each arm (dt-continuity fix).
    #[cfg(feature = "adaptive-os")]
    wdf_hi: Option<Box<WdfModel>>,
    /// Peak-follower envelope on the driven pre-model input volts.
    #[cfg(feature = "adaptive-os")]
    os_env: f32,
    /// Per-sample envelope release multiplier (from `OS_REL_S` + host SR).
    #[cfg(feature = "adaptive-os")]
    os_rel_coef: f32,
    /// Current crossfade phase. `Lo` when `wdf_hi` is `None` (inert).
    #[cfg(feature = "adaptive-os")]
    os_phase: OsPhase,
    /// lo↔hi crossfade samples remaining (0 = steady).
    #[cfg(feature = "adaptive-os")]
    os_xfade_remain: u32,
    /// lo↔hi crossfade length in samples (from `OS_XFADE_S` + host SR).
    #[cfg(feature = "adaptive-os")]
    os_xfade_len: u32,
    /// Silent-prime samples remaining for the incoming model (0 = not priming).
    #[cfg(feature = "adaptive-os")]
    os_prime_remain: u32,
    /// Silent-prime length in samples (from `OS_PRIME_S` + host SR).
    #[cfg(feature = "adaptive-os")]
    os_prime_len: u32,
    /// Minimum-dwell samples remaining at hi-os before a disarm is allowed.
    #[cfg(feature = "adaptive-os")]
    os_dwell_remain: u32,
    /// Minimum-dwell length in samples (from `OS_DWELL_S` + host SR).
    #[cfg(feature = "adaptive-os")]
    os_dwell_len: u32,
    /// Count of arm events since the last `os_reset_stats` — Stage-1 G2/G3 instrumentation.
    #[cfg(feature = "adaptive-os")]
    os_arm_count: u64,
    /// Count of host samples spent solving the hi-os model (steady-hi + fades) — G2 duty proxy.
    #[cfg(feature = "adaptive-os")]
    os_hi_samples: u64,
}

impl ChannelDsp {
    pub fn new() -> Self {
        Self {
            wavenet: WaveNet::new_from_embedded(),
            // Dormant LSTM: not constructed in the plugin path (saves the 153 KB parse per channel).
            // The offline `standalone --weights` tool installs one via `set_model`.
            model: None,
            bridge: HostBridge::new(),
            dry_delay: ShortDelay::new(),
            // Placeholder SR/bias; rebuilt + restored with the real settings in `initialize`.
            wdf: Box::new(WdfModel::new(48_000.0, 220.0, 0.5)),
            wdf_out_delay: ShortDelay::new(),
            wdf_snap: None,
            // Pre-allocate the crossfade scratch model once (off the audio thread, at construction)
            // so a live tier switch reuses it instead of `Box::new`-ing on the audio thread.
            wdf_prev: Box::new(WdfModel::new(48_000.0, 220.0, 0.5)),
            xfade_remain: 0,
            xfade_len: 0,
            resume_remain: 0,
            dcblock: DcBlock::new(),
            eq: Eq::new(),
            last_wet: 0.0,
            #[cfg(feature = "adaptive-os")]
            wdf_hi: None,
            #[cfg(feature = "adaptive-os")]
            os_env: 0.0,
            #[cfg(feature = "adaptive-os")]
            os_rel_coef: 0.0,
            #[cfg(feature = "adaptive-os")]
            os_phase: OsPhase::Lo,
            #[cfg(feature = "adaptive-os")]
            os_xfade_remain: 0,
            #[cfg(feature = "adaptive-os")]
            os_xfade_len: 0,
            #[cfg(feature = "adaptive-os")]
            os_prime_remain: 0,
            #[cfg(feature = "adaptive-os")]
            os_prime_len: 0,
            #[cfg(feature = "adaptive-os")]
            os_dwell_remain: 0,
            #[cfg(feature = "adaptive-os")]
            os_dwell_len: 0,
            #[cfg(feature = "adaptive-os")]
            os_arm_count: 0,
            #[cfg(feature = "adaptive-os")]
            os_hi_samples: 0,
        }
    }

    /// De-normed wet (pre-levelNorm/out_gain/mix) from the last `process_sample` — for the
    /// stereo-linked levelNorm RMS estimator in the plugin.
    #[inline]
    pub fn last_wet(&self) -> f32 {
        self.last_wet
    }

    /// Install a dormant LSTM model (offline tools only — e.g. the `standalone` A/B path). The
    /// active NeuralNet engine is the WaveNet; this field is not on the plugin's signal path.
    pub fn set_model(&mut self, model: Ba283Model) {
        self.model = Some(model);
    }

    pub fn reset(&mut self) {
        self.wavenet.reset();
        if let Some(model) = &mut self.model {
            model.reset();
        }
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
        self.eq.reset();
        self.last_wet = 0.0;
        // Adaptive-os: drop to the lo-os steady state and clear the envelope (cancels any in-flight
        // lo↔hi crossfade; the hi model goes unread until the next arm re-seeds it from the snapshot).
        #[cfg(feature = "adaptive-os")]
        {
            self.os_phase = OsPhase::Lo;
            self.os_xfade_remain = 0;
            self.os_prime_remain = 0;
            self.os_dwell_remain = 0;
            self.os_env = 0.0;
        }
    }

    /// Size the EQ biquad coefficients to the host sample rate (called from `initialize`). The EQ
    /// runs at host rate ahead of the model.
    pub fn eq_prepare(&mut self, host_sr: f32) {
        self.eq.prepare(host_sr);
    }

    /// Push the current EQ settings (block-rate). `hpf_hz <= 0` bypasses the high-pass. Coeffs are
    /// recomputed only when they change; the stage stays bit-exact identity while `on` is false.
    #[allow(clippy::too_many_arguments)]
    pub fn eq_set(
        &mut self,
        on: bool,
        hpf_hz: f32,
        low_hz: f32,
        low_g: f32,
        mid_hz: f32,
        mid_g: f32,
        high_g: f32,
    ) {
        self.eq.set(on, hpf_hz, low_hz, low_g, mid_hz, mid_g, high_g);
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
        *self.wdf = WdfModel::new_with(cfg, host_sr as f64, rgain as f64, bias as f64);
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
    /// NOTE: `WdfModel::new_with` rebuilds the circuit and allocates its `Vec` build scratch + the
    /// depletion LUTs, so this is NOT alloc-free. It is retained for the OFF-audio-thread callers
    /// (`initialize`, the offline standalone, tests). The LIVE audio-thread tier/OS switch instead
    /// goes through the off-thread worker ([`crate::run_rebuild`]) + the alloc-free pointer-swap
    /// [`Self::install_prebuilt`], so a switch in `process` never allocates.
    pub fn wdf_set_tier(&mut self, cfg: QualityConfig, host_sr: f32, rgain: f32, bias: f32) {
        // Build the new model and swap it in, moving the old (settled) model into the pre-allocated
        // crossfade box so it can fade out against the new one's startup transient (zeroed biquads +
        // operating-point jump) — no audio-thread `Box` alloc for the carry-over.
        let new = WdfModel::new_with(cfg, host_sr as f64, rgain as f64, bias as f64);
        self.xfade_len = (host_sr * WDF_XFADE_S).max(1.0) as u32;
        *self.wdf_prev = std::mem::replace(&mut *self.wdf, new);
        if let Some(snap) = self.wdf_snap {
            self.wdf.restore(&snap);
            self.wdf.save_snapshot();
        }
        self.xfade_remain = self.xfade_len;
    }

    /// Install a worker-prebuilt lo (`wdf`) model for a tier/OS switch — the alloc-free counterpart
    /// to [`Self::wdf_set_tier`]. The new model arrives as a `Box` built off the audio thread, so the
    /// whole rotation (new → `wdf`, old `wdf` → `wdf_prev`, stale `wdf_prev` → returned) is pointer
    /// moves only: NO `WdfModel::new_with`, NO `Box` alloc. The returned stale box is handed back to
    /// the caller to drop OFF the audio thread (its `Arc<DepLutSet>` may otherwise `free` here). The
    /// carried settled snapshot is re-installed (fixed memcpy) and the 60 ms crossfade is armed,
    /// exactly as `wdf_set_tier` does.
    pub fn install_prebuilt(&mut self, lo: Box<WdfModel>, host_sr: f32) -> Box<WdfModel> {
        self.xfade_len = (host_sr * WDF_XFADE_S).max(1.0) as u32;
        // new lo box becomes live; the old live box moves out for the crossfade.
        let old_live = std::mem::replace(&mut self.wdf, lo);
        if let Some(snap) = self.wdf_snap {
            self.wdf.restore(&snap);
            self.wdf.save_snapshot();
        }
        // old live → crossfade scratch; the stale 2-switches-ago scratch is returned for off-thread drop.
        let retired = std::mem::replace(&mut self.wdf_prev, old_live);
        self.xfade_remain = self.xfade_len;
        retired
    }

    /// Install (or clear) a worker-prebuilt hi-os sibling — the alloc-free counterpart to
    /// [`Self::wdf_set_hi`] / [`Self::wdf_clear_hi`]. `hi` is `Some(box)` to arm the adaptive machine
    /// or `None` to make it inert; either way the previous `wdf_hi` box (if any) is returned for the
    /// caller to drop off the audio thread. Seeds the incoming model from the settled snapshot and
    /// resets the detector to the lo-os steady state, sizing the envelope / crossfade / dwell timers
    /// from the host SR (identical to `wdf_set_hi`).
    #[cfg(feature = "adaptive-os")]
    pub fn install_prebuilt_hi(
        &mut self,
        hi: Option<Box<WdfModel>>,
        host_sr: f32,
    ) -> Option<Box<WdfModel>> {
        let retired = std::mem::replace(&mut self.wdf_hi, hi);
        if let Some(m) = self.wdf_hi.as_mut() {
            if let Some(snap) = self.wdf_snap {
                m.restore(&snap);
                m.save_snapshot();
            }
        }
        self.os_phase = OsPhase::Lo;
        self.os_xfade_remain = 0;
        self.os_prime_remain = 0;
        self.os_dwell_remain = 0;
        self.os_env = 0.0;
        self.os_xfade_len = (host_sr * OS_XFADE_S).max(1.0) as u32;
        self.os_prime_len = (host_sr * OS_PRIME_S).max(1.0) as u32;
        self.os_dwell_len = (host_sr * OS_DWELL_S).max(1.0) as u32;
        self.os_rel_coef = (-1.0 / (OS_REL_S * host_sr)).exp();
        retired
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
        #[cfg(feature = "adaptive-os")]
        if let Some(hi) = self.wdf_hi.as_mut() {
            hi.set_bias(bias as f64);
        }
    }

    /// Live circuit-gain (R_KJ) change: rebuild G0 cheaply, preserving integrator state.
    pub fn wdf_set_rgain(&mut self, rgain: f32) {
        self.wdf.set_rgain(rgain as f64);
        #[cfg(feature = "adaptive-os")]
        if let Some(hi) = self.wdf_hi.as_mut() {
            hi.set_rgain(rgain as f64);
        }
    }

    /// Live aging change: drift all caps + resistors from base, rebuild G0 (preserves state).
    pub fn wdf_set_age(&mut self, age: f32) {
        self.wdf.set_age(age as f64);
        #[cfg(feature = "adaptive-os")]
        if let Some(hi) = self.wdf_hi.as_mut() {
            hi.set_age(age as f64);
        }
    }

    /// Re-roll the aged "unit": regenerate the drift pattern from `seed`, re-apply at current age.
    pub fn wdf_set_age_seed(&mut self, seed: u64) {
        self.wdf.set_age_seed(seed);
        #[cfg(feature = "adaptive-os")]
        if let Some(hi) = self.wdf_hi.as_mut() {
            hi.set_age_seed(seed);
        }
    }

    /// Live transformer ON/OFF (feature `trafo-core`). OFF => bit-exact linear core.
    #[cfg(feature = "trafo-core")]
    pub fn wdf_set_trafo_enabled(&mut self, on: bool) {
        self.wdf.set_trafo_enabled(on);
        #[cfg(feature = "adaptive-os")]
        if let Some(hi) = self.wdf_hi.as_mut() {
            hi.set_trafo_enabled(on);
        }
    }

    /// Live transformer voicing: HYSTERESIS + SATURATION dials (each 0..1; feature `trafo-core`).
    #[cfg(feature = "trafo-core")]
    pub fn wdf_set_trafo_voicing(&mut self, hyst: f32, sat: f32) {
        self.wdf.set_trafo_voicing(hyst as f64, sat as f64);
        #[cfg(feature = "adaptive-os")]
        if let Some(hi) = self.wdf_hi.as_mut() {
            hi.set_trafo_voicing(hyst as f64, sat as f64);
        }
    }

    /// Zero the WDF Newton-iteration counters (offline A/B: call after warmup, before the audio pass).
    pub fn wdf_reset_iter_stats(&mut self) {
        self.wdf.reset_iter_stats();
    }

    /// WDF Newton-iteration stats since the last reset: `(samples, avg_iters, max_iters)`.
    pub fn wdf_iter_stats(&self) -> (u64, f64, usize) {
        self.wdf.iter_stats()
    }

    /// Install/refresh the resident hi-os model for adaptive oversampling (feature `adaptive-os`).
    /// Built at `cfg_hi` (the tier's config with os=2 + decimation), re-seeded from the carried
    /// settled snapshot so it matches the lo model's operating point. Block-rate only (allocates,
    /// like `wdf_set_tier`). Resets the detector to the lo-os steady state and sizes the envelope /
    /// crossfade / dwell timers from the host SR.
    #[cfg(feature = "adaptive-os")]
    pub fn wdf_set_hi(&mut self, cfg_hi: QualityConfig, host_sr: f32, rgain: f32, bias: f32) {
        let mut m = WdfModel::new_with(cfg_hi, host_sr as f64, rgain as f64, bias as f64);
        if let Some(snap) = self.wdf_snap {
            m.restore(&snap);
            m.save_snapshot();
        }
        match self.wdf_hi.as_mut() {
            Some(b) => **b = m, // reuse the box (no extra audio-thread Box alloc)
            None => self.wdf_hi = Some(Box::new(m)),
        }
        self.os_phase = OsPhase::Lo;
        self.os_xfade_remain = 0;
        self.os_prime_remain = 0;
        self.os_dwell_remain = 0;
        self.os_env = 0.0;
        self.os_xfade_len = (host_sr * OS_XFADE_S).max(1.0) as u32;
        self.os_prime_len = (host_sr * OS_PRIME_S).max(1.0) as u32;
        self.os_dwell_len = (host_sr * OS_DWELL_S).max(1.0) as u32;
        self.os_rel_coef = (-1.0 / (OS_REL_S * host_sr)).exp();
    }

    /// Drop the resident hi-os model (tier is not adaptive-eligible, or the user forced an
    /// oversampling factor). The machine goes inert: `process_sample` runs `wdf` exactly as the
    /// default build does.
    #[cfg(feature = "adaptive-os")]
    pub fn wdf_clear_hi(&mut self) {
        self.wdf_hi = None;
        self.os_phase = OsPhase::Lo;
        self.os_xfade_remain = 0;
        self.os_prime_remain = 0;
        self.os_dwell_remain = 0;
    }

    /// Zero the adaptive-os arm/duty counters (Stage-1 G2/G3 instrumentation).
    #[cfg(feature = "adaptive-os")]
    pub fn os_reset_stats(&mut self) {
        self.os_arm_count = 0;
        self.os_hi_samples = 0;
    }

    /// `(arm_count, hi_samples)` since the last `os_reset_stats`: how often the gate engaged and how
    /// many host samples ran the hi-os model (the average-cost / duty proxy for G2).
    #[cfg(feature = "adaptive-os")]
    pub fn os_stats(&self) -> (u64, u64) {
        (self.os_arm_count, self.os_hi_samples)
    }

    /// Run one host sample through the adaptive-os state machine, returning the model output in VOLTS
    /// (pre `/INPUT_FS_VOLTS`). The envelope must already be updated for this sample. Solves only the
    /// model(s) the current phase needs — the parked sibling stays cold (the CPU win). When `wdf_hi`
    /// is `None` this is exactly `self.wdf.process_host_sample(in_v)`.
    #[cfg(feature = "adaptive-os")]
    #[inline]
    fn wdf_solve_adaptive(&mut self, in_v: f32) -> f32 {
        if self.wdf_hi.is_none() {
            return self.wdf.process_host_sample(in_v);
        }
        // Phase transitions. On arm/disarm the incoming model is re-seeded from the settled DC
        // snapshot and enters a silent PRIME phase (solved on the live input, output still from the
        // outgoing model) so it catches up to the current signal before being faded in — without
        // this, re-seeding from DC mid-swing injects a large settling transient at the seam.
        match self.os_phase {
            OsPhase::Lo => {
                if self.os_env >= OS_ARM_V {
                    if let Some(snap) = self.wdf_snap {
                        self.wdf_hi.as_mut().unwrap().restore(&snap);
                    }
                    self.os_phase = OsPhase::PrimeHi;
                    self.os_prime_remain = self.os_prime_len;
                    self.os_dwell_remain = self.os_dwell_len;
                    self.os_arm_count += 1;
                }
            }
            OsPhase::Hi => {
                if self.os_dwell_remain > 0 {
                    self.os_dwell_remain -= 1;
                }
                if self.os_dwell_remain == 0 && self.os_env < OS_DISARM_V {
                    // Re-seed the lo model (parked while at hi-os, so its state is stale).
                    if let Some(snap) = self.wdf_snap {
                        self.wdf.restore(&snap);
                    }
                    self.os_phase = OsPhase::PrimeLo;
                    self.os_prime_remain = self.os_prime_len;
                }
            }
            // Mid prime/crossfade (`PrimeHi`/`ToHi`/`PrimeLo`/`ToLo`): no new transition. In
            // particular a `ToLo`/`PrimeLo` disarm fade is NOT interruptible — a hot transient landing
            // inside the ~14 ms disarm seam is rendered on lo-os until the fade reaches `Lo`, then the
            // next sample re-arms from there. Deliberate: it keeps the seam a single monotonic fade
            // (no fade reversal / partial-crossfade clicks) and bounds the switch rate. The cost is a
            // few ms of lo-os on a transient that arrives during a disarm — acceptable vs the artifact
            // risk of cancelling an in-flight crossfade.
            _ => {}
        }
        // Output for this phase. `os_hi_samples` counts every sample the hi model is solved (prime +
        // fade + steady) — the G2 cost proxy.
        match self.os_phase {
            OsPhase::Lo => self.wdf.process_host_sample(in_v),
            OsPhase::Hi => {
                self.os_hi_samples += 1;
                self.wdf_hi.as_mut().unwrap().process_host_sample(in_v)
            }
            OsPhase::PrimeHi => {
                // Settle hi silently; output stays on lo. Both solve, but hi is not yet audible.
                self.os_hi_samples += 1;
                let y_lo = self.wdf.process_host_sample(in_v);
                self.wdf_hi.as_mut().unwrap().process_host_sample(in_v);
                self.os_prime_remain -= 1;
                if self.os_prime_remain == 0 {
                    self.os_phase = OsPhase::ToHi;
                    self.os_xfade_remain = self.os_xfade_len;
                }
                y_lo
            }
            OsPhase::ToHi => {
                self.os_hi_samples += 1;
                let y_lo = self.wdf.process_host_sample(in_v);
                let y_hi = self.wdf_hi.as_mut().unwrap().process_host_sample(in_v);
                self.os_xfade_remain -= 1;
                let t = 1.0 - self.os_xfade_remain as f32 / self.os_xfade_len.max(1) as f32; // 0→1
                let s = t * t * (3.0 - 2.0 * t);
                if self.os_xfade_remain == 0 {
                    self.os_phase = OsPhase::Hi;
                }
                y_lo * (1.0 - s) + y_hi * s
            }
            OsPhase::PrimeLo => {
                // Settle lo silently; output stays on hi.
                self.os_hi_samples += 1;
                let y_hi = self.wdf_hi.as_mut().unwrap().process_host_sample(in_v);
                self.wdf.process_host_sample(in_v);
                self.os_prime_remain -= 1;
                if self.os_prime_remain == 0 {
                    self.os_phase = OsPhase::ToLo;
                    self.os_xfade_remain = self.os_xfade_len;
                }
                y_hi
            }
            OsPhase::ToLo => {
                self.os_hi_samples += 1;
                let y_hi = self.wdf_hi.as_mut().unwrap().process_host_sample(in_v);
                let y_lo = self.wdf.process_host_sample(in_v);
                self.os_xfade_remain -= 1;
                let t = 1.0 - self.os_xfade_remain as f32 / self.os_xfade_len.max(1) as f32; // 0→1
                let s = t * t * (3.0 - 2.0 * t);
                if self.os_xfade_remain == 0 {
                    self.os_phase = OsPhase::Lo;
                }
                y_hi * (1.0 - s) + y_lo * s
            }
        }
    }

    /// Run one sample through the selected backend. `input_gain`/`drive`/`out_gain`/`norm_gain` are
    /// linear gains, `mix` is the wet fraction in 0..1. `cond` drives the NeuralNet path only — it is
    /// the WaveNet's FiLM gain-conditioning value (0..1). The WDF path ignores it (it uses the
    /// block-rate bias/gain set via `wdf_set_bias`/`wdf_set_rgain`). The host-reported latency
    /// (`Ba283Plugin::latency_samples`) is honoured for BOTH backends.
    ///
    /// Signal chain: `x → ×input_gain → eq → ×drive → [model] → ×norm_gain → ×out_gain → mix`.
    /// `input_gain` is the clean pre-circuit trim; `drive` pushes the POST-EQ signal into the model
    /// (the saturation amount). `norm_gain` is the stereo-linked levelNorm gain (1.0 when off),
    /// applied to the de-normed wet BEFORE `out_gain` and `mix`. The pre-norm de-normed wet is stashed
    /// in `last_wet` so the plugin can drive the levelNorm RMS estimator across both channels.
    #[allow(clippy::too_many_arguments)]
    pub fn process_sample(
        &mut self,
        x: f32,
        input_gain: f32,
        drive: f32,
        cond: f32,
        out_gain: f32,
        norm_gain: f32,
        mix: f32,
        use_bridge: bool,
        backend: InferenceBackend,
    ) -> f32 {
        // Neve EQ sits ahead of the BA283/AV card on the real strip, so it shapes the signal BEFORE
        // either backend — EQ boosts drive the amp harder on boosted bands, exactly as the hardware
        // does. `input_gain` is the clean pre-EQ trim; `drive` then pushes the post-EQ signal into the
        // model (the saturation amount). Bit-exact identity while EQ off + input_gain/drive at unity,
        // so the model's golden path is untouched.
        let driven = self.eq.process(x * input_gain) * drive;
        match backend {
            InferenceBackend::NeuralNet => {
                // De-norm: recover volts (`* output_ref_v`) then express in the 0 dBFS ≡ 0.5 V
                // convention (`/ INPUT_FS_VOLTS`). The WaveNet applies its own `input_scale` to the
                // audio internally, so it is fed `driven` directly. `cond` is the FiLM
                // gain-conditioning value (0..1) the host sets from the circuit-gain knob.
                let deref = self.wavenet.output_ref_v() / INPUT_FS_VOLTS;
                // Disjoint field borrows so the bridge closure can call the net.
                let (wavenet, bridge) = (&mut self.wavenet, &mut self.bridge);
                let model_out = if use_bridge {
                    // Host runs ~2x the model rate: decimate to ~48 kHz, run the net once per host
                    // pair, interpolate back (HostBridge is itself zero-alloc).
                    bridge.process(driven, |lo| wavenet.process(lo, cond))
                } else {
                    // Host runs at ~the model rate: evaluate the net directly.
                    wavenet.process(driven, cond)
                };
                let wet = model_out * deref;
                self.last_wet = wet;
                let wet = wet * norm_gain;
                let wet = wet * out_gain;
                // Dry path delayed to match the wet-path (bridge) latency so Mix aligns.
                let dry_aligned = if use_bridge { self.dry_delay.push_pop(x) } else { x };
                wet * mix + dry_aligned * (1.0 - mix)
            }
            InferenceBackend::Wdf => {
                // The circuit is fed/observed in volts: dBFS->volts in (`* INPUT_FS_VOLTS`),
                // volts->dBFS out (`/ INPUT_FS_VOLTS`), matching the SPICE 0.5 V stimulus.
                let in_v = driven * INPUT_FS_VOLTS;
                // Adaptive-os envelope: peak follower (instant attack, exp release) on the driven
                // input volts, updated every sample so it is current when a tier crossfade ends.
                #[cfg(feature = "adaptive-os")]
                {
                    let a = in_v.abs();
                    self.os_env = if a > self.os_env { a } else { self.os_env * self.os_rel_coef };
                }
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
                    // Adaptive-os state machine (inert / bit-identical to the line below when the
                    // feature is off or `wdf_hi` is None).
                    #[cfg(feature = "adaptive-os")]
                    {
                        self.wdf_solve_adaptive(in_v) / INPUT_FS_VOLTS
                    }
                    #[cfg(not(feature = "adaptive-os"))]
                    {
                        self.wdf.process_host_sample(in_v) / INPUT_FS_VOLTS
                    }
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
                let wet = wet * norm_gain;
                let wet = wet * out_gain;
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
