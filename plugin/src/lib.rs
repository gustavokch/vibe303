use nih_plug::prelude::*;
use nih_plug_egui::EguiState;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub mod dcblock;
pub mod delay;
pub mod dpr;
pub mod dsp;
pub mod eq;
mod editor;
pub mod model;
pub mod oversample;
pub mod wavenet;
pub mod wdf;

use dsp::ChannelDsp;

/// Static levelNorm trim clamp (linear): −80 dB .. +48 dB. The lower bound must be low enough to
/// tame the faithful de-normed output, which carries the preamp's full (~+50 dB) voltage gain.
const NORM_TRIM_MIN: f32 = 1.0e-4;
const NORM_TRIM_MAX: f32 = 256.0;
use wdf::{QualityConfig, WdfModel};

/// Time constant (seconds) for the per-block glide of the WDF block-rate params (bias, age).
/// Smooths coarse host automation steps into small per-block increments so the G0 rebuild re-settles
/// cleanly instead of stepping. Below these epsilons the glide snaps to the target so it fully
/// settles (no perpetual rebuilds from a never-quite-equal one-pole). (Circuit gain is now a discrete
/// switch — it snaps straight to the position with no glide.)
const WDF_PARAM_TAU_S: f32 = 0.02;
const WDF_BIAS_EPS: f32 = 1.0e-5;
const WDF_AGE_EPS: f32 = 1.0e-5;
/// Settle-snap epsilon (dB) for the per-block EQ gain glide: once a band gain is within this of its
/// target, snap so the one-pole fully lands (no endless near-equal coeff recomputes).
const EQ_GAIN_EPS: f32 = 1.0e-3;
#[cfg(feature = "trafo-core")]
const WDF_TRAFO_EPS: f32 = 1.0e-5;

// Oversampling factor of the resident adaptive-os `wdf_hi` sibling (the armed state on the os=1
// lossy tiers). os=4 is the high-payoff target: the A0 decimator gate (2026-06-18) confirmed the
// existing 4th-order Butterworth decimator is already within +0.57 dB of the steep-sinc ideal at
// 4:1 (zero added latency — the 0-PDC contract holds), and os1→os4 recovers +15.6..+23 dB of alias
// floor vs +7.5..+10.7 dB for os1→os2.
#[cfg(feature = "adaptive-os")]
const ADAPTIVE_HI_OS: usize = 4;

/// Flush-to-zero + denormals-are-zero on the calling (audio) thread. The FP control register is
/// thread-local, so this must run on the audio thread (top of `process`), not in `initialize` —
/// the host may run those on different threads. Subnormal arithmetic carries a ~100x penalty; the
/// DC blocker (`r ≈ 0.9995` feedback), EQ biquads, and WDF f64 internals can drift into subnormal
/// range during silence/decay tails. There is no safe std API for the FP control register, hence
/// the (small, sound) unsafe blocks; other arches compile to a no-op.
#[inline]
fn enable_ftz_daz() {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        // MXCSR: bit 15 = FTZ, bit 6 = DAZ. The `_mm_{get,set}csr` intrinsics are deprecated in
        // favour of stmxcsr/ldmxcsr inline asm.
        use std::arch::asm;
        let mut mxcsr: u32 = 0;
        asm!("stmxcsr [{}]", in(reg) &mut mxcsr, options(nostack, preserves_flags));
        mxcsr |= 0x8000 | 0x0040;
        asm!("ldmxcsr [{}]", in(reg) &mxcsr, options(nostack, preserves_flags, readonly));
    }
    #[cfg(target_arch = "aarch64")]
    unsafe {
        // FPCR bit 24 = FZ (flushes subnormal f32/f64 in and out; the ARM equivalent of FTZ+DAZ).
        // No stable intrinsic exists, so read/modify/write via unprivileged (EL0-legal) mrs/msr.
        use std::arch::asm;
        let mut fpcr: u64;
        asm!("mrs {}, fpcr", out(reg) fpcr, options(nomem, nostack));
        asm!("msr fpcr, {}", in(reg) fpcr | (1 << 24), options(nomem, nostack));
    }
}

/// Default aging "unit" seed (a fixed nonzero constant so a fresh instance is deterministic).
const DEFAULT_AGE_SEED: u64 = 0xBA283_0000_0001;

/// Convert decibels to a linear gain factor.
#[inline]
fn db_to_gain(db: f32) -> f32 {
    10.0f32.powf(db * 0.05)
}

/// levelNorm applied gain from a probed trim and the current Drive. `trim` is `1/G_v` (the inverse
/// circuit voltage gain from the probe); dividing out the known Drive scalar levels the post-Drive
/// output, clamped to the trim range. Shared by `process` and the levelNorm regression tests so the
/// formula and clamp can never drift between them.
pub fn norm_apply_gain(trim: f32, drive: f32) -> f32 {
    (trim / drive.max(1.0e-6)).clamp(NORM_TRIM_MIN, NORM_TRIM_MAX)
}

// The historical OFF-THREAD levelNorm gain probe was removed when `circuit_gain` became a 5-position
// switch: each position's small-signal voltage gain `G_i` is now a fixed constant, so levelNorm reads
// the static `GAIN_STEPS[pos].wdf_makeup` (= `1/G_i`, measured offline by `bin/measure_gi`) instead of
// re-measuring the circuit at runtime. This makes levelNorm deterministic and alloc-free with no
// background worker. (The makeup is referenced to default bias/age/trafo; aging/bias drift the true
// gain slightly, an accepted tradeoff for the fixed-position design.)

/// Number of per-channel DSP cores (mono path still allocates two; only one is driven).
const N_CH: usize = 2;
/// Retire-queue capacity: max model boxes awaiting an off-thread drop. A switch retires ≤2 lo + ≤2
/// hi boxes and switch coalescing bounds concurrency, so 8 slots is ample headroom.
const RETIRE_CAP: usize = 8;
/// Failsafe bound (in `process` blocks) for a pending rebuild whose `busy` latch is held but whose
/// build never lands. `execute_background` is fire-and-forget — on a full task queue nih-plug drops
/// the post silently, leaving `busy` stuck `true` forever and wedging the switch machine. After this
/// many blocks with the latch held and nothing published, the latch is force-released so the next
/// block re-posts. The bound is far larger than any real off-thread build (a WDF rebuild settles in
/// well under ~20 blocks at any sane block size), so a trip means the post was dropped — i.e. NO
/// worker is live — and the force-release cannot race a running build. Re-posts the SAME pending req
/// (same `gen`), so even a pathological concurrent build would be gen-identical and safe at publish.
const REBUILD_STALL_LIMIT: u32 = 512;

/// Operating point for an OFF-THREAD tier/OS/adaptive rebuild. All `Copy`, so posting one via
/// `execute_background` never allocates on the audio thread. `gen` (monotonic) lets the audio thread
/// consume only the build it is waiting for and lets the worker skip superseded params.
#[derive(Clone, Copy)]
pub struct RebuildReq {
    gen: u64,
    cfg: QualityConfig,
    sr: f32,
    rgain: f32,
    bias: f32,
    /// `Some` ⇒ build the adaptive-os hi sibling at this config; `None` ⇒ clear it.
    #[cfg(feature = "adaptive-os")]
    cfg_hi: Option<QualityConfig>,
}

/// The background task posted to nih-plug's executor: an off-thread model rebuild, so a tier/OS switch
/// builds the new model here (worker thread), not inline on the audio thread. (The levelNorm probe
/// task was removed — levelNorm is now a static per-position table; see the note above.)
#[derive(Clone, Copy)]
pub enum BgTask {
    Rebuild(RebuildReq),
}

/// Per-channel models built off the audio thread by [`run_rebuild`], awaiting consume by `process`.
struct RebuiltSet {
    gen: u64,
    lo: [Box<WdfModel>; N_CH],
    #[cfg(feature = "adaptive-os")]
    hi: [Option<Box<WdfModel>>; N_CH],
}

/// Fixed-capacity ring the audio thread parks retired model boxes in (pointer move, no alloc); the
/// worker drains and drops them so the `free` (and the `Arc<DepLutSet>` refcount-zero) runs off the
/// audio thread.
struct RetireRing {
    slots: [Option<Box<WdfModel>>; RETIRE_CAP],
}
impl RetireRing {
    fn new() -> Self {
        Self { slots: Default::default() }
    }
    /// Park a box for off-thread drop; hands it back as `Err` if full (caller drops inline as a
    /// documented fallback — should not happen given switch coalescing).
    fn push(&mut self, b: Box<WdfModel>) -> Result<(), Box<WdfModel>> {
        for slot in self.slots.iter_mut() {
            if slot.is_none() {
                *slot = Some(b);
                return Ok(());
            }
        }
        Err(b)
    }
}

/// Shared state for the off-thread tier/OS rebuild. Mirrors [`ProbeShared`]: the audio thread only
/// ever `try_lock`s (never blocks) and otherwise touches atomics; the worker owns the heavy build.
struct RebuildShared {
    /// Worker → audio: the latest built per-channel models. `try_lock`'d by the audio thread.
    ready: Mutex<Option<RebuiltSet>>,
    /// `gen` of the models in `ready` (0 = none). Audio consumes only on a match with its pending
    /// gen; a superseded build is ignored and then overwritten (dropped) by the worker.
    ready_gen: AtomicU64,
    /// Audio → worker: retired model boxes awaiting an off-thread drop.
    retire: Mutex<RetireRing>,
    /// Newest rebuild gen the audio thread has requested.
    latest_gen: AtomicU64,
    /// A rebuild is in flight (coalesce: at most one queued).
    busy: AtomicBool,
}
impl RebuildShared {
    fn new() -> Self {
        Self {
            ready: Mutex::new(None),
            ready_gen: AtomicU64::new(0),
            retire: Mutex::new(RetireRing::new()),
            latest_gen: AtomicU64::new(0),
            busy: AtomicBool::new(false),
        }
    }
}

/// Build the per-channel WDF models for a tier/OS switch OFF the audio thread, publish them to
/// `shared.ready`, and drain the retire queue (dropping retired models here, not on the audio
/// thread). The body of the background executor's `Rebuild` arm; also called synchronously in tests.
/// MUST run off the audio thread — `WdfModel::new_with` allocates and rebuilds the circuit.
fn run_rebuild(shared: &RebuildShared, req: RebuildReq) {
    // Build both channels' lo models. rgain/bias are applied at build; age/seed/trafo are re-stamped
    // by the audio thread's forced legs at consume — exactly as the old inline `wdf_set_tier` path.
    let lo: [Box<WdfModel>; N_CH] = [
        Box::new(WdfModel::new_with(req.cfg, req.sr as f64, req.rgain as f64, req.bias as f64)),
        Box::new(WdfModel::new_with(req.cfg, req.sr as f64, req.rgain as f64, req.bias as f64)),
    ];
    #[cfg(feature = "adaptive-os")]
    let hi: [Option<Box<WdfModel>>; N_CH] = {
        let build = |c: QualityConfig| {
            Box::new(WdfModel::new_with(c, req.sr as f64, req.rgain as f64, req.bias as f64))
        };
        [req.cfg_hi.map(build), req.cfg_hi.map(build)]
    };
    let set = RebuiltSet {
        gen: req.gen,
        lo,
        #[cfg(feature = "adaptive-os")]
        hi,
    };
    // Publish (any previous unconsumed set is dropped here, on the worker thread).
    *shared.ready.lock().unwrap() = Some(set);
    shared.ready_gen.store(req.gen, Ordering::Release);
    // Drain retired models and drop them here (off the audio thread). Take the slots out under the
    // lock (a pointer move) and drop them AFTER releasing it, so the lock never spans a `free`. This
    // keeps the audio thread's `retire_model` `try_lock` from ever contending with an in-progress
    // drop — i.e. the inline-free fallback in `retire_model` stays unreachable in normal operation.
    let drained = {
        let mut ring = shared.retire.lock().unwrap();
        std::mem::take(&mut ring.slots)
    };
    drop(drained);
    shared.busy.store(false, Ordering::Release);
}

pub struct Ba283Plugin {
    params: Arc<Ba283Params>,
    /// One per-channel DSP core (model + oversampler + dry-delay) per channel
    /// (mono/stereo). The SAME `ChannelDsp` chain is used by the offline
    /// standalone, so the Phase-4 null target matches the plugin exactly.
    channels: [ChannelDsp; 2],
    /// Host sample rate cached in `initialize`. Default to the model's native
    /// 48 kHz so the direct (no-bridge) path is taken until told otherwise.
    sample_rate: f32,
    /// Last WDF R_KJ gain / bias applied to the channels (block-rate). `NAN` forces a first-block
    /// apply. WDF-only; the NeuralNet path ignores these.
    last_rgain: f32,
    last_bias: f32,
    /// Last WDF tier applied to the channels (block-rate). A change triggers a rebuild + snapshot
    /// re-install on each channel.
    last_tier: WdfTier,
    /// Last WDF precision applied (block-rate). A change triggers the same rebuild path as a tier
    /// change (the precision is folded into the `QualityConfig` via `with_precision`).
    last_precision: Precision,
    /// Last WDF oversampling override applied (block-rate). A change triggers the same rebuild path
    /// as a tier change (folded into the `QualityConfig` via `with_oversampling`).
    last_oversampling: Oversampling,
    /// Last `Adaptive OS` toggle state applied (block-rate). A change re-runs the tier rebuild path
    /// so the resident `wdf_hi` sibling is added (toggle on) or dropped (toggle off) immediately.
    #[cfg(feature = "adaptive-os")]
    last_adaptive_os: bool,
    /// The levelNorm gain actually applied at the end of the last block (the per-sample ramp's start
    /// for the next block). Carries the ramp state across blocks so a Circuit-Gain sweep — or a
    /// level-norm on/off toggle — fades click-free instead of stepping. NOT a program-dependent RMS
    /// estimator (so no pumping). Applied only when the `level_norm` param is on (WDF backend only).
    norm_trim: f32,
    /// The levelNorm trim target (`1 / G_v` for the current gain position), read each block from the
    /// static `GAIN_STEPS[pos].wdf_makeup` table (no runtime probe). The applied `norm_trim` ramps
    /// toward it per-sample so a position change fades click-free. At a settled position this equals
    /// `norm_trim`.
    norm_trim_target: f32,
    /// Forces a one-shot snap of `norm_trim` straight to the (drive-cancelled) target (set on init /
    /// reset / first WDF block) — leveled from the first sample, no startup ramp.
    norm_reprobe: bool,
    /// Off-thread tier/OS rebuild handoff (request via `execute_background`, models swapped in by a
    /// later `process` block). Makes a tier/OS switch alloc-free on the audio thread: the worker
    /// builds the new models, `process` only pointer-swaps them in and parks the retired ones.
    rebuild: Arc<RebuildShared>,
    /// The latest tier/OS switch awaiting its off-thread build (`None` when idle). Re-posted each
    /// block until the worker delivers, so a fast sweep converges to the most recent switch.
    rebuild_pending: Option<RebuildReq>,
    /// Blocks the current `rebuild_pending` has waited with the `busy` latch held but nothing
    /// published. Resets on a post / consume; trips the [`REBUILD_STALL_LIMIT`] failsafe.
    rebuild_stall: u32,
    /// Per-block-glided WDF R_KJ gain / bias (smooth the coarse block-rate steps into the G0
    /// rebuild). Distinct from the NeuralNet per-sample bias smoother.
    wdf_rgain_smooth: f32,
    wdf_bias_smooth: f32,
    wdf_age_smooth: f32,
    /// Per-block-glided EQ band gains (dB). The EQ recomputes biquad coeffs at block rate, so a fast
    /// gain-knob drag would step coeffs coarsely (audible zipper over the ±16/18 dB span). Gliding the
    /// gain targets turns each drag into small per-block coeff steps, matching `wdf_rgain_smooth`.
    eq_low_g_smooth: f32,
    eq_mid_g_smooth: f32,
    eq_high_g_smooth: f32,
    /// Last WDF aging value / seed applied to the channels (block-rate). `NAN`/`MAX` force a
    /// first-block apply. `last_reroll` edge-detects the momentary re-roll trigger.
    last_age: f32,
    last_seed: u64,
    last_reroll: bool,
    #[cfg(feature = "trafo-core")]
    wdf_trafo_hyst_smooth: f32,
    #[cfg(feature = "trafo-core")]
    wdf_trafo_sat_smooth: f32,
    #[cfg(feature = "trafo-core")]
    last_trafo_on: bool,
    #[cfg(feature = "trafo-core")]
    last_trafo_hyst: f32,
    #[cfg(feature = "trafo-core")]
    last_trafo_sat: f32,
}

/// Which emulation runs per sample. `Wdf` (the circuit-faithful MNA solver) is the default;
/// `NeuralNet` is the learned model. Selectable live in the plugin. NOTE: variant order is
/// load-bearing — nih_plug serializes enum params by variant index, so reordering breaks presets.
#[derive(Enum, PartialEq, Eq, Clone, Copy, Debug)]
pub enum InferenceBackend {
    #[name = "Neural Net"]
    NeuralNet,
    #[name = "Circuit (WDF)"]
    Wdf,
}

/// WDF solver tier. `Realtime` is the bit-exact reference (all speed levers off); `Fast` bakes in
/// predict + schur + freeze-caps + ls-trim (~5x faster, near-transparent); `UltraFast` adds the
/// cheap low-order transport exp on top of Fast; `Warp` additionally drops the BJT junction charge
/// model (~19 % more per-sample cost gone) — the caps shape only the top octave. UltraFast/Warp also
/// drop internal oversampling 2→1 (their dominant ~1.8x speed lever), which is peak-safe under the
/// full-convergence solver they run. Precision is now device-model-only and the [`Oversampling`] dial
/// is a separate axis, so selecting UltraFast/Warp drops CPU at every precision — Med/Low no longer
/// fall back to the os=2 cost. WDF-only; the NeuralNet backend ignores it.
#[derive(Enum, PartialEq, Eq, Clone, Copy, Debug)]
pub enum WdfTier {
    #[name = "Realtime"]
    Realtime,
    #[name = "Fast"]
    Fast,
    #[name = "Ultra Fast"]
    UltraFast,
    #[name = "Warp"]
    Warp,
}

impl WdfTier {
    fn config(self) -> QualityConfig {
        match self {
            WdfTier::Realtime => QualityConfig::realtime(),
            WdfTier::Fast => QualityConfig::fast(),
            WdfTier::UltraFast => QualityConfig::ultra_fast(),
            WdfTier::Warp => QualityConfig::warp(),
        }
    }
}

/// One position of the BA283 hardware gain switch. `r_kj_ohm` is the value stamped into the WDF R_KJ
/// leg (the open position uses a large sentinel — resistor removed → leg conductance ≈ 0). `label`
/// is the (balanced) dB reading on the rotary. `wdf_makeup` is the DRIVE-swept static levelNorm
/// circuit-makeup `1/G_sat(drive)` — one entry per [`DRIVE_GRID_DB`] point — the reciprocal of the
/// position's measured LARGE-SIGNAL voltage gain at each Drive. levelNorm applies
/// `wdf_makeup_at(drive) / drive`: storing the saturated gain (not a single small-signal `1/G_v`)
/// stops the `/drive` from over-attenuating once Drive pushes the circuit into saturation, so the
/// DRIVE knob colours at constant loudness instead of sagging. Measured offline by `bin/measure_gi`
/// at the realtime tier with default bias/age/trafo; the drift-guard test in
/// `tests/test_stepped_gain.rs` re-measures these and asserts they match, so a silent solver/circuit
/// change cannot desync them.
struct GainStep {
    label: &'static str,
    r_kj_ohm: f32,
    wdf_makeup: &'static [f32],
}

/// DRIVE grid (dB) the [`GainStep::wdf_makeup`] curves are sampled at, spanning the `drive_db` range
/// (−12..+24) every 3 dB. [`wdf_makeup_at`] linearly interpolates over this grid in dB. MUST match
/// `DRIVE_GRID_DB` in `bin/measure_gi` (the drift-guard test pins this).
const DRIVE_GRID_DB: &[f32] = &[
    -12.0, -9.0, -6.0, -3.0, 0.0, 3.0, 6.0, 9.0, 12.0, 15.0, 18.0, 21.0, 24.0,
];

/// Real BA283 gain-switch wafer positions (5). Lower R ⇒ more gain. Position 0 is the open contact
/// (no R_KJ): faithful min gain, modelled by a large sentinel resistance. The `circuit_gain` param is
/// an `IntParam(0..4)` indexing this table — a faithful rotary detent, never continuous. Each
/// `wdf_makeup` row is `1/G_sat` across `DRIVE_GRID_DB` (the Drive-0 column matches the historical
/// small-signal `1/G_v` to ~5 sig figs; later columns rise as the circuit saturates). From
/// `bin/measure_gi`.
static GAIN_STEPS: &[GainStep] = &[
    GainStep { label: "15 dB", r_kj_ohm: 1.0e9, wdf_makeup: &[0.202893, 0.202893, 0.202893, 0.202893, 0.202893, 0.202893, 0.202894, 0.202895, 0.202897, 0.204302, 0.213085, 0.257231, 0.343651] },
    GainStep { label: "20 dB", r_kj_ohm: 1_200.0, wdf_makeup: &[0.118732, 0.118732, 0.118732, 0.118732, 0.118733, 0.118733, 0.118734, 0.118737, 0.118757, 0.127161, 0.160859, 0.214971, 0.295261] },
    GainStep { label: "25 dB", r_kj_ohm: 470.0, wdf_makeup: &[0.073245, 0.073245, 0.073245, 0.073245, 0.073246, 0.073247, 0.073250, 0.073255, 0.081828, 0.105094, 0.140683, 0.192686, 0.269547] },
    GainStep { label: "30 dB", r_kj_ohm: 220.0, wdf_makeup: &[0.043796, 0.043797, 0.043797, 0.043798, 0.043799, 0.043802, 0.044009, 0.053177, 0.069519, 0.093665, 0.128705, 0.180198, 0.255104] },
    GainStep { label: "35 dB", r_kj_ohm: 110.0, wdf_makeup: &[0.026130, 0.026131, 0.026131, 0.026133, 0.026137, 0.027833, 0.035305, 0.046863, 0.063611, 0.087916, 0.123313, 0.174285, 0.247381] },
];

/// Linear-interpolate a [`GainStep`]'s `wdf_makeup` curve (`1/G_sat`) at `drive_db` over
/// [`DRIVE_GRID_DB`], flat-holding past either end. Mirrors [`nn_calib_lookup`]'s interp style.
fn wdf_makeup_at(step: &GainStep, drive_db: f32) -> f32 {
    let g = DRIVE_GRID_DB;
    let m = step.wdf_makeup;
    if drive_db <= g[0] {
        return m[0];
    }
    let n = g.len();
    if drive_db >= g[n - 1] {
        return m[n - 1];
    }
    for i in 1..n {
        if drive_db <= g[i] {
            let f = (drive_db - g[i - 1]) / (g[i] - g[i - 1]);
            return m[i - 1] + f * (m[i] - m[i - 1]);
        }
    }
    m[n - 1]
}

/// Number of hardware gain-switch positions (the `circuit_gain` IntParam range is `0..=GAIN_STEPS-1`).
const GAIN_STEPS_LEN: i32 = 5;

/// Clamp a `circuit_gain` param value to a valid `GAIN_STEPS` index.
#[inline]
fn gain_pos(idx: i32) -> usize {
    idx.clamp(0, GAIN_STEPS_LEN - 1) as usize
}

/// Map the `circuit_gain` R_KJ value (Ω) onto the WaveNet's trained 0..1 gain axis. The conditioned
/// model was reamped over a geometric R_KJ sweep — 10000 Ω → gain_norm 0.0, 82 Ω → 1.0 — so the
/// inverse is a log ratio (verified against the eval table, e.g. 906 Ω → 0.5). The param's 47 Ω /
/// open positions extrapolate past 1.0 and are clamped (the model saw no data hotter than 82 Ω).
///
/// `pub` so the calibration/regression tests drive the NN with the SAME cond curve the plugin uses —
/// replicating this mapping in the tests would let it silently drift out of sync with `NN_GAIN_CALIB`.
pub fn rgain_to_gain_norm(rgain_ohm: f32) -> f32 {
    const R_LO: f32 = 10_000.0; // gain_norm 0.0
    const R_HI: f32 = 82.0; // gain_norm 1.0
    let g = (rgain_ohm.max(1.0).ln() - R_LO.ln()) / (R_HI.ln() - R_LO.ln());
    g.clamp(0.0, 1.0)
}

/// Map the `input_gain` trim (dB) onto the WS4 dual-conditioned model's second 0..1 FiLM channel.
/// Mirrors `data_gen.netlist.input_gain_db_to_norm` and the `input_gain` param range (Linear
/// -24..+24 dB), so 0 dB (the levelNorm-ON neutral) → 0.5. Inert for a 2-channel model (the WaveNet
/// reads only the first conditioning channel until `wavenet::COND` is flipped to 3).
pub fn input_gain_db_to_norm(db: f32) -> f32 {
    const DB_MIN: f32 = -24.0;
    const DB_MAX: f32 = 24.0;
    ((db - DB_MIN) / (DB_MAX - DB_MIN)).clamp(0.0, 1.0)
}

/// NN loudness-makeup + level-norm calibration, indexed by `circuit_gain` R_KJ (Ω) in LOG-ohm space.
/// Each row is `(ln R, makeup, gnn_inv)`, measured once offline by `tests/calib_nn_makeup.rs`:
///   `makeup = Gv(R) / G_nn(R)` — multiply the NN wet by this so its absolute loudness matches the
///     WDF backend at the same gain. The embedded WaveNet already tracks gain via FiLM but runs
///     ~6 dB hot, and its cond axis saturates at 82 Ω (`rgain_to_gain_norm` clamps), so the makeup is
///     ~0.48 (−6.4 dB) across 10 k–82 Ω and rises toward 47 Ω where the WDF circuit keeps gaining.
///   `gnn_inv = 1 / G_nn(R)` — the level-norm trim seed: `norm_apply_gain(gnn_inv, drive)` levels the
///     NN output to ~unity (gain- AND Drive-independent), mirroring the WDF probe's `1/(G_v·drive)`.
/// Interpolated linearly in ln-ohm (matches the `circuit_gain` skew); flat-held past either end.
/// NOT a live probe — the NN is a fixed black box, so the curve is a static measurement.
static NN_GAIN_CALIB: &[(f32, f32, f32)] = &[
    (3.8501, 0.7334, 0.010927), //   47 Ω (cond clamps at 82 Ω → G_nn flat below)
    (4.4067, 0.5022, 0.010927), //   82 Ω
    (4.7005, 0.4967, 0.013361), //  110 Ω
    (5.3936, 0.4686, 0.020883), //  220 Ω (default)
    (6.1527, 0.4809, 0.035590), //  470 Ω
    (6.8090, 0.4822, 0.050945), //  906 Ω
    (7.0901, 0.4983, 0.059545), // 1200 Ω
    (9.2103, 0.4769, 0.089445), //  10 kΩ (cond clamps at 0 → G_nn flat above)
];

/// Linear-interpolate [`NN_GAIN_CALIB`] in ln-ohm, flat-holding past either end. Returns
/// `(makeup, gnn_inv)`.
fn nn_calib_lookup(rgain_ohm: f32) -> (f32, f32) {
    let x = rgain_ohm.max(1.0).ln();
    let t = NN_GAIN_CALIB;
    if x <= t[0].0 {
        return (t[0].1, t[0].2);
    }
    let last = t[t.len() - 1];
    if x >= last.0 {
        return (last.1, last.2);
    }
    for w in t.windows(2) {
        let (a, b) = (w[0], w[1]);
        if x <= b.0 {
            let f = (x - a.0) / (b.0 - a.0);
            return (a.1 + f * (b.1 - a.1), a.2 + f * (b.2 - a.2));
        }
    }
    (last.1, last.2)
}

/// NN loudness makeup for `circuit_gain` R_KJ (Ω): multiplies the NN wet so its absolute level tracks
/// the WDF backend (level-norm OFF). See [`NN_GAIN_CALIB`].
pub fn nn_makeup_gain(rgain_ohm: f32) -> f32 {
    nn_calib_lookup(rgain_ohm).0.clamp(NORM_TRIM_MIN, NORM_TRIM_MAX)
}

/// NN level-norm trim seed `1/G_nn(R)` for `circuit_gain` R_KJ (Ω): fed through `norm_apply_gain` with
/// the current Drive to level the NN output (level-norm ON). See [`NN_GAIN_CALIB`].
pub fn nn_norm_trim(rgain_ohm: f32) -> f32 {
    nn_calib_lookup(rgain_ohm).1
}

/// Lossy precision dial — the device-model fidelity axis, orthogonal to [`WdfTier`] and
/// [`Oversampling`]. `High` is the identity (Realtime+High stays bit-exact / golden); `Medium`/`Low`
/// step the BJT charge model + transport exp down from the tier baseline (`cap_tier` + `cheap_exp`),
/// trading fidelity for speed within the harness budget (ESR loss ≤ −50 dB vs Realtime+High). It no
/// longer touches the iteration strategy or oversampling rate, so it never slows a tier. WDF-only;
/// the NeuralNet backend ignores it. Maps 1:1 to [`wdf::Precision`].
#[derive(Enum, PartialEq, Eq, Clone, Copy, Debug)]
pub enum Precision {
    #[name = "High"]
    High,
    #[name = "Medium"]
    Medium,
    #[name = "Low"]
    Low,
}

impl Precision {
    fn to_wdf(self) -> wdf::Precision {
        match self {
            Precision::High => wdf::Precision::High,
            Precision::Medium => wdf::Precision::Medium,
            Precision::Low => wdf::Precision::Low,
        }
    }
}

/// Internal-oversampling dial — the antialiasing-rate axis, orthogonal to [`WdfTier`] and
/// [`Precision`]. `Auto` defers to the tier's own rate (UltraFast/Warp keep os=1, Realtime/Fast keep
/// os=2), so the default reproduces current behaviour bit-for-bit; `X1`/`X2` force the WDF internal
/// rate. The WDF os is IIR-decimated (phase-only, ~0 integer latency), so this never changes reported
/// latency / PDC. Only 1×/2× ship now (the decimator is 2×-tuned); 4× lands behind a 4:1 decimator.
/// WDF-only; the NeuralNet backend ignores it. Maps 1:1 to [`wdf::Oversampling`].
#[derive(Enum, PartialEq, Eq, Clone, Copy, Debug)]
pub enum Oversampling {
    #[name = "Auto"]
    Auto,
    #[name = "1x"]
    X1,
    #[name = "2x"]
    X2,
}

impl Oversampling {
    fn to_wdf(self) -> wdf::Oversampling {
        match self {
            Oversampling::Auto => wdf::Oversampling::Auto,
            Oversampling::X1 => wdf::Oversampling::X1,
            Oversampling::X2 => wdf::Oversampling::X2,
        }
    }
}

/// Editor DPI scaling. `Auto` follows the scale the host reports via
/// `IPlugViewContentScaleSupport` (the correct path in virtually every DAW). The explicit factors
/// are a manual override for hosts that never report a scale, or report the wrong one — common on
/// Linux/X11 where baseview cannot read the compositor's HiDPI scale itself. UI-only: it changes
/// nothing in the DSP and is not host-automatable.
///
/// It IS persisted in plugin state (it is a `Params` field), so an explicit factor follows the
/// project to another machine: a value picked on a HiDPI display will over-scale on a 1× one.
/// That is intentional — it is a manual escape hatch — and `Auto` (the default) sidesteps it by
/// always tracking the current host.
#[derive(Enum, PartialEq, Eq, Clone, Copy, Debug)]
pub enum UiScale {
    #[name = "Auto"]
    Auto,
    #[name = "100%"]
    X100,
    #[name = "125%"]
    X125,
    #[name = "150%"]
    X150,
    #[name = "175%"]
    X175,
    #[name = "200%"]
    X200,
}

/// Switched high-pass-filter corner of the 1073/1076 EQ (S4): OFF / 50 / 80 / 160 / 300 Hz, ~18
/// dB/oct. `Off` bypasses the HPF entirely (folding the on/off into the selector, like
/// [`Oversampling::Auto`]). Both backends; the EQ runs ahead of the model.
#[derive(Enum, PartialEq, Eq, Clone, Copy, Debug)]
pub enum HpfFreq {
    #[name = "Off"]
    Off,
    #[name = "50 Hz"]
    Hz50,
    #[name = "80 Hz"]
    Hz80,
    #[name = "160 Hz"]
    Hz160,
    #[name = "300 Hz"]
    Hz300,
}

impl HpfFreq {
    /// Corner in Hz, or `0.0` for `Off` (the EQ stage treats `<= 0` as bypassed).
    fn hz(self) -> f32 {
        match self {
            HpfFreq::Off => 0.0,
            HpfFreq::Hz50 => 50.0,
            HpfFreq::Hz80 => 80.0,
            HpfFreq::Hz160 => 160.0,
            HpfFreq::Hz300 => 300.0,
        }
    }
}

/// Switched low-shelf corner of the 1073/1076 EQ (S3): 35 / 60 / 110 / 220 Hz.
#[derive(Enum, PartialEq, Eq, Clone, Copy, Debug)]
pub enum LowShelfFreq {
    #[name = "35 Hz"]
    Hz35,
    #[name = "60 Hz"]
    Hz60,
    #[name = "110 Hz"]
    Hz110,
    #[name = "220 Hz"]
    Hz220,
}

impl LowShelfFreq {
    fn hz(self) -> f32 {
        match self {
            LowShelfFreq::Hz35 => 35.0,
            LowShelfFreq::Hz60 => 60.0,
            LowShelfFreq::Hz110 => 110.0,
            LowShelfFreq::Hz220 => 220.0,
        }
    }
}

/// Switched mid/presence-bell center of the 1073/1076 EQ (S5): 360 / 700 / 1.6k / 3.2k / 4.8k /
/// 7.2k Hz. Proportional-Q (narrows with boost) is applied in [`crate::eq`].
#[derive(Enum, PartialEq, Eq, Clone, Copy, Debug)]
pub enum MidFreq {
    #[name = "360 Hz"]
    Hz360,
    #[name = "700 Hz"]
    Hz700,
    #[name = "1.6 kHz"]
    Hz1600,
    #[name = "3.2 kHz"]
    Hz3200,
    #[name = "4.8 kHz"]
    Hz4800,
    #[name = "7.2 kHz"]
    Hz7200,
}

impl MidFreq {
    fn hz(self) -> f32 {
        match self {
            MidFreq::Hz360 => 360.0,
            MidFreq::Hz700 => 700.0,
            MidFreq::Hz1600 => 1600.0,
            MidFreq::Hz3200 => 3200.0,
            MidFreq::Hz4800 => 4800.0,
            MidFreq::Hz7200 => 7200.0,
        }
    }
}

#[derive(Params)]
pub struct Ba283Params {
    #[id = "backend"]
    pub backend: EnumParam<InferenceBackend>,
    /// Editor HiDPI scale (UI-only, not automatable). See [`UiScale`].
    #[id = "ui_scale"]
    pub ui_scale: EnumParam<UiScale>,
    /// WDF solver tier (Realtime vs Fast). WDF-only; inert for the NeuralNet backend.
    #[id = "wdf_tier"]
    pub wdf_tier: EnumParam<WdfTier>,
    /// Lossy precision dial (High/Medium/Low), orthogonal to the tier. High = no extra loss
    /// (bit-exact at Realtime). WDF-only; inert for the NeuralNet backend.
    #[id = "precision"]
    pub precision: EnumParam<Precision>,
    /// Internal-oversampling dial (Auto/1x/2x), orthogonal to tier + precision. Auto defers to the
    /// tier (bit-exact default). WDF-only; inert for the NeuralNet backend.
    #[id = "oversampling"]
    pub oversampling: EnumParam<Oversampling>,
    /// Adaptive level-gated oversampling (feature `adaptive-os`): on the os=1 lossy tiers
    /// (UltraFast/Warp) under Oversampling = Auto, engage a resident os=4 model only on hot
    /// transients (os=1 when quiet) — near-os=4 alias quality at near-os=1 average cost. Off by
    /// default (opt in via the toggle, so an update never silently changes the sound/CPU of an
    /// existing session). Inert (collapses to plain os=1) when off, or when the tier is not lossy,
    /// or when Oversampling ≠ Auto. WDF-only.
    #[cfg(feature = "adaptive-os")]
    #[id = "adaptive_os"]
    pub adaptive_os: BoolParam,
    #[id = "drive_db"]
    pub drive_db: FloatParam,
    #[id = "bias"]
    pub bias: FloatParam,
    #[id = "output_db"]
    pub output_db: FloatParam,
    #[id = "mix"]
    pub mix: FloatParam,
    /// Neve 1073/1076-style EQ ON/OFF (both backends). OFF (default) = bit-exact today's sound; ON
    /// inserts the analog-matched biquad EQ on the input signal, ahead of the model (matching the
    /// real signal flow EQ → BA283/AV → LO1166). See [`crate::eq`].
    #[id = "eq_on"]
    pub eq_on: BoolParam,
    /// EQ high-pass corner (S4): Off / 50 / 80 / 160 / 300 Hz. `Off` bypasses the HPF.
    #[id = "eq_hpf_freq"]
    pub eq_hpf_freq: EnumParam<HpfFreq>,
    /// EQ low-shelf corner (S3): 35 / 60 / 110 / 220 Hz.
    #[id = "eq_low_freq"]
    pub eq_low_freq: EnumParam<LowShelfFreq>,
    /// EQ low-shelf gain (RV2), ±16 dB. 0 dB = flat.
    #[id = "eq_low_gain"]
    pub eq_low_gain: FloatParam,
    /// EQ mid/presence center (S5): 360 / 700 / 1.6k / 3.2k / 4.8k / 7.2k Hz.
    #[id = "eq_mid_freq"]
    pub eq_mid_freq: EnumParam<MidFreq>,
    /// EQ mid/presence gain (RV3), ±18 dB. Proportional-Q narrows with boost. 0 dB = flat.
    #[id = "eq_mid_gain"]
    pub eq_mid_gain: FloatParam,
    /// EQ high-shelf gain (RV1, fixed 12 kHz), ±16 dB. 0 dB = flat.
    #[id = "eq_high_gain"]
    pub eq_high_gain: FloatParam,
    /// Level-neutral gain staging. ON (the DEFAULT) ⇒ output is auto-compensated to unity regardless
    /// of circuit-gain position and Drive (only tone/saturation changes); `input_gain`/`output_db`/
    /// `link` are forced neutral and grayed in the editor. OFF ⇒ faithful preamp: circuit gain + Drive
    /// are audible as level and the user trims manually via `input_gain`/`output_db` (+ `link`).
    #[id = "level_norm"]
    pub level_norm: BoolParam,
    /// Clean pre-circuit input trim (dB). Sits at the head of the chain (`x * input_gain → eq →
    /// Drive → circuit`). Active only when `level_norm` is OFF (forced neutral / grayed when ON).
    #[id = "input_gain"]
    pub input_gain: FloatParam,
    /// Couples `input_gain` and `output_db` in OPPOSITE directions (input +X dB ⇒ output −X dB) for
    /// manual net-neutral gain staging — push the circuit hot, pull the output back. Only meaningful
    /// when `level_norm` is OFF; the editor slaves the output knob to input while ON.
    #[id = "link"]
    pub link: BoolParam,
    /// Hardware gain switch: a 5-position rotary (`GAIN_STEPS`, 15/20/25/30/35 dB) indexing the WDF
    /// R_KJ leg — a faithful detent, never continuous. Also conditions the NeuralNet (FiLM gain-norm).
    /// Block-rate via a cheap `G0` rebuild on each detent crossing.
    #[id = "circuit_gain"]
    pub circuit_gain: IntParam,
    /// Component aging: random per-component drift (caps + resistors) emulating vintage units.
    /// 0 = pristine (bit-exact today); 1 = fully aged. WDF-only; inert for the NeuralNet backend.
    /// Block-rate via a `G0` rebuild, glided per block like bias/gain.
    #[id = "age"]
    pub age: FloatParam,
    /// Momentary "re-roll unit" trigger: a rising edge picks a new random drift pattern (a different
    /// vintage unit). WDF-only.
    #[id = "age_reroll"]
    pub age_reroll: BoolParam,
    /// Persisted aging seed — the "unit identity". Not an automatable param; serialized with the
    /// plugin state so a saved project recalls the exact same unit. Re-roll mutates it.
    #[persist = "age_seed"]
    pub age_seed: Arc<AtomicU64>,
    /// Persisted egui editor state (window size). Not a param; serialized so the window geometry
    /// round-trips with the project. Fixed-size today, but `create_egui_editor` requires it.
    #[persist = "editor_state"]
    pub editor_state: Arc<EguiState>,
    /// Transformer (Jiles-Atherton magnetic core) ON/OFF. ON (default) = today's sound; OFF =
    /// bit-exact linear core (the JA RK4 cost vanishes — A/B the transformer). WDF-only. The whole
    /// model ships in default builds (`trafo-core` is a default feature since the GO verdict); opt
    /// out at compile time with `--no-default-features`.
    #[cfg(feature = "trafo-core")]
    #[id = "trafo_on"]
    pub trafo_on: BoolParam,
    /// Transformer HYSTERESIS dial (0..1). 0 = no loop (near-anhysteretic), 0.5 = today's voicing
    /// (`k=300`, `c=0.7`), 1 = wide loop. Shapes the core's pinning/reversibility (loop width).
    #[cfg(feature = "trafo-core")]
    #[id = "trafo_hyst"]
    pub trafo_hyst: FloatParam,
    /// Transformer SATURATION dial (0..1). 0 = clean (knee above signal), 0.5 = today's voicing
    /// (`b_sat=0.3T`), 1 = hard/early knee. Shapes where the saturation knee sits.
    #[cfg(feature = "trafo-core")]
    #[id = "trafo_sat"]
    pub trafo_sat: FloatParam,
    /// DPR (Dual Phase Rotator) ON/OFF. OFF (default) = bit-exact today's sound; ON inserts an
    /// all-pass phase rotator on the output path (ahead of the levelNorm makeup gain and the output gain) that
    /// reshapes waveform asymmetry without changing the magnitude spectrum. Both backends. See
    /// [`crate::dpr`].
    #[id = "dpr_on"]
    pub dpr_on: BoolParam,
    /// DPR amount (0..1): sweeps the shared all-pass corner frequency on a log scale (~80 Hz at 0,
    /// ~200 Hz at the 0.5 default, ~500 Hz at 1). Higher = the phase rotation reaches further into
    /// the low mids, reshaping the attack more audibly. Inert while `dpr_on` is false.
    #[id = "dpr_amount"]
    pub dpr_amount: FloatParam,
}

impl Default for Ba283Params {
    fn default() -> Self {
        Self {
            backend: EnumParam::new("Backend", InferenceBackend::Wdf),
            ui_scale: EnumParam::new("UI Scale", UiScale::Auto).non_automatable(),
            wdf_tier: EnumParam::new("WDF Tier", WdfTier::Realtime),
            precision: EnumParam::new("Precision", Precision::High),
            oversampling: EnumParam::new("Oversampling", Oversampling::Auto),
            #[cfg(feature = "adaptive-os")]
            adaptive_os: BoolParam::new("Adaptive OS", false),
            drive_db: FloatParam::new(
                "Drive",
                0.0,
                FloatRange::Linear {
                    min: -12.0,
                    max: 24.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1))
            .with_smoother(SmoothingStyle::Linear(20.0)),
            bias: FloatParam::new("Bias", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_rounded(2))
                .with_smoother(SmoothingStyle::Linear(50.0)),
            output_db: FloatParam::new(
                "Output",
                0.0,
                FloatRange::Linear {
                    min: -24.0,
                    max: 12.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1))
            .with_smoother(SmoothingStyle::Linear(20.0)),
            mix: FloatParam::new(
                "Mix",
                100.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 100.0,
                },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_rounded(0))
            .with_smoother(SmoothingStyle::Linear(20.0)),
            // EQ defaults: OFF + all gains 0 dB so the stage is a provable no-op until engaged.
            eq_on: BoolParam::new("EQ", false),
            eq_hpf_freq: EnumParam::new("EQ HPF", HpfFreq::Off),
            eq_low_freq: EnumParam::new("EQ Low Freq", LowShelfFreq::Hz60),
            eq_low_gain: FloatParam::new("EQ Low", 0.0, FloatRange::Linear { min: -16.0, max: 16.0 })
                .with_unit(" dB")
                .with_value_to_string(formatters::v2s_f32_rounded(1)),
            eq_mid_freq: EnumParam::new("EQ Mid Freq", MidFreq::Hz1600),
            eq_mid_gain: FloatParam::new("EQ Mid", 0.0, FloatRange::Linear { min: -18.0, max: 18.0 })
                .with_unit(" dB")
                .with_value_to_string(formatters::v2s_f32_rounded(1)),
            eq_high_gain: FloatParam::new("EQ High", 0.0, FloatRange::Linear { min: -16.0, max: 16.0 })
                .with_unit(" dB")
                .with_value_to_string(formatters::v2s_f32_rounded(1)),
            // Default ON: inserting the plugin at defaults is level-neutral (unity) — fixes the
            // historical "+30 dB louder at default" bug. Turn OFF for faithful manual gain staging.
            level_norm: BoolParam::new("Level Norm", true),
            input_gain: FloatParam::new(
                "Input Gain",
                0.0,
                FloatRange::Linear {
                    min: -24.0,
                    max: 24.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1))
            .with_smoother(SmoothingStyle::Linear(20.0)),
            link: BoolParam::new("Link", false),
            // Default position idx 0 (15 dB) — the cleanest / least-coloured stock insert. Under
            // levelNorm ON the level is neutral at any position, so this is a pure-tone default.
            circuit_gain: IntParam::new(
                "Circuit Gain",
                0,
                IntRange::Linear { min: 0, max: GAIN_STEPS_LEN - 1 },
            )
            .with_value_to_string(Arc::new(|i| {
                GAIN_STEPS[gain_pos(i)].label.to_string()
            }))
            .with_string_to_value(Arc::new(|s| {
                GAIN_STEPS.iter().position(|g| g.label == s.trim()).map(|p| p as i32)
            })),
            age: FloatParam::new("Age", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_rounded(2))
                .with_smoother(SmoothingStyle::Linear(50.0)),
            age_reroll: BoolParam::new("Re-roll Unit", false),
            age_seed: Arc::new(AtomicU64::new(DEFAULT_AGE_SEED)),
            editor_state: EguiState::from_size(660, 1010),
            #[cfg(feature = "trafo-core")]
            trafo_on: BoolParam::new("Transformer", true),
            #[cfg(feature = "trafo-core")]
            trafo_hyst: FloatParam::new("Hysteresis", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_rounded(2))
                .with_smoother(SmoothingStyle::Linear(50.0)),
            #[cfg(feature = "trafo-core")]
            trafo_sat: FloatParam::new("Saturation", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_rounded(2))
                .with_smoother(SmoothingStyle::Linear(50.0)),
            // DPR defaults: OFF (bit-exact today) + amount 0.5 → ~200 Hz canonical rotator corner.
            dpr_on: BoolParam::new("DPR", false),
            // Amount is consumed block-rate (read via `.value()` in `process`, like the EQ params),
            // NOT per-sample-smoothed: the resolved corner `fc` only recomputes the all-pass coeffs
            // when it changes, so a per-sample smoother would defeat that cache for no benefit.
            dpr_amount: FloatParam::new("DPR Amount", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_rounded(2)),
        }
    }
}

impl Default for Ba283Plugin {
    fn default() -> Self {
        Self {
            params: Arc::new(Ba283Params::default()),
            channels: [ChannelDsp::new(), ChannelDsp::new()],
            sample_rate: 48_000.0,
            last_rgain: f32::NAN,
            last_bias: f32::NAN,
            last_tier: WdfTier::Realtime,
            last_precision: Precision::High,
            last_oversampling: Oversampling::Auto,
            #[cfg(feature = "adaptive-os")]
            last_adaptive_os: false,
            norm_trim: 1.0,
            norm_trim_target: 1.0,
            norm_reprobe: true,
            rebuild: Arc::new(RebuildShared::new()),
            rebuild_pending: None,
            rebuild_stall: 0,
            wdf_rgain_smooth: GAIN_STEPS[0].r_kj_ohm,
            wdf_bias_smooth: 0.5,
            wdf_age_smooth: 0.0,
            eq_low_g_smooth: 0.0,
            eq_mid_g_smooth: 0.0,
            eq_high_g_smooth: 0.0,
            last_age: f32::NAN,
            last_seed: u64::MAX,
            last_reroll: false,
            #[cfg(feature = "trafo-core")]
            wdf_trafo_hyst_smooth: 0.5,
            #[cfg(feature = "trafo-core")]
            wdf_trafo_sat_smooth: 0.5,
            #[cfg(feature = "trafo-core")]
            last_trafo_on: false,
            #[cfg(feature = "trafo-core")]
            last_trafo_hyst: f32::NAN,
            #[cfg(feature = "trafo-core")]
            last_trafo_sat: f32::NAN,
        }
    }
}

impl Ba283Plugin {
    /// Host-rate latency introduced by the [`HostBridge`]. When the host runs near the model's
    /// native 48 kHz (below `HOST_BRIDGE_THRESHOLD_HZ`) the model runs directly, so latency is zero.
    /// At/above the threshold (88.2/96/176.4/192 kHz) the decimate -> model -> interpolate cascade
    /// has a group delay of `HOST_BRIDGE_LATENCY` host-rate samples (measured by the
    /// `host_bridge_identity_after_latency` test). This is the single source of truth for the host
    /// report and the dry-path delay. (Effective *model* rate per host rate — and the known
    /// 176.4/192 kHz degradation — is documented on `dsp::HOST_BRIDGE_THRESHOLD_HZ`.)
    fn latency_samples(&self) -> u32 {
        if self.sample_rate >= dsp::HOST_BRIDGE_THRESHOLD_HZ {
            dsp::OS_LATENCY_SAMPLES as u32
        } else {
            0
        }
    }

    /// Drive-swept levelNorm trim (`1 / G_sat(drive)`) for a `circuit_gain` position at `drive_db`.
    /// Test/diagnostic accessor. `drive_db = 0` returns the unity-Drive column.
    #[doc(hidden)]
    pub fn pos_makeup(pos: i32, drive_db: f32) -> f32 {
        wdf_makeup_at(&GAIN_STEPS[gain_pos(pos)], drive_db)
    }

    /// Park a retired model box for an off-thread drop (the worker drains the ring). Alloc-free
    /// pointer move via the fixed-capacity ring. On the should-never-happen overflow OR a momentary
    /// lock contention with the worker's drain, drop inline as a documented fallback.
    fn retire_model(&self, b: Box<WdfModel>) {
        match self.rebuild.retire.try_lock() {
            Ok(mut ring) => {
                if let Err(b) = ring.push(b) {
                    nih_warn!("BA283: model retire ring full; dropping on the audio thread");
                    drop(b);
                }
            }
            Err(_) => drop(b),
        }
    }

    /// Consume a finished off-thread tier/OS rebuild if its models are ready: pointer-swap them into
    /// each channel ([`ChannelDsp::install_prebuilt`]), park the retired boxes for an off-thread drop,
    /// and force the param legs to re-stamp the freshly built (pristine) models. Returns `true` if a
    /// swap happened. ALLOC-FREE and FREE-FREE on the audio thread — the gate test asserts it.
    fn consume_pending_rebuild(&mut self) -> bool {
        let req = match self.rebuild_pending {
            Some(r) => r,
            None => return false,
        };
        if self.rebuild.ready_gen.load(Ordering::Acquire) != req.gen {
            return false;
        }
        // Take the published set under a non-blocking try_lock (the worker holds it only briefly to
        // move pointers); bail to next block on contention. The guard temporary is released before
        // the channel borrows below.
        let taken = match self.rebuild.ready.try_lock() {
            Ok(mut guard) => guard.take(),
            Err(_) => return false,
        };
        let set = match taken {
            Some(s) => s,
            None => return false,
        };
        debug_assert_eq!(set.gen, req.gen);
        let [lo0, lo1] = set.lo;
        let r0 = self.channels[0].install_prebuilt(lo0, self.sample_rate);
        let r1 = self.channels[1].install_prebuilt(lo1, self.sample_rate);
        self.retire_model(r0);
        self.retire_model(r1);
        #[cfg(feature = "adaptive-os")]
        {
            let [hi0, hi1] = set.hi;
            if let Some(rh) = self.channels[0].install_prebuilt_hi(hi0, self.sample_rate) {
                self.retire_model(rh);
            }
            if let Some(rh) = self.channels[1].install_prebuilt_hi(hi1, self.sample_rate) {
                self.retire_model(rh);
            }
        }
        self.rebuild_pending = None;
        // The freshly installed models are pristine (no aging / bias-rgain / trafo applied yet) —
        // force every leg in `process` to re-stamp them this block.
        self.last_age = f32::NAN;
        self.last_seed = u64::MAX;
        self.last_rgain = f32::NAN;
        self.last_bias = f32::NAN;
        #[cfg(feature = "trafo-core")]
        {
            self.last_trafo_on = !self.params.trafo_on.value();
            self.last_trafo_hyst = f32::NAN;
            self.last_trafo_sat = f32::NAN;
        }
        true
    }

    /// TEST: arm a pending tier/OS rebuild exactly as `process`'s detection gate does (bump the gen,
    /// stash the request) without a host. When `adaptive-os` is built in, the hi sibling is requested
    /// too (so the gate test exercises the hi swap + retire). `cfg` should be a lossy tier to build
    /// the depletion LUTs (the heavy alloc moved off-thread).
    #[doc(hidden)]
    pub fn force_arm_rebuild(&mut self, cfg: QualityConfig, rgain: f32, bias: f32) {
        #[cfg(feature = "adaptive-os")]
        let cfg_hi = {
            let mut hi = cfg;
            hi.os = ADAPTIVE_HI_OS;
            hi.decimate = true;
            Some(hi)
        };
        let gen = self.rebuild.latest_gen.load(Ordering::Relaxed) + 1;
        self.rebuild.latest_gen.store(gen, Ordering::Relaxed);
        self.rebuild_pending = Some(RebuildReq {
            gen,
            cfg,
            sr: self.sample_rate,
            rgain,
            bias,
            #[cfg(feature = "adaptive-os")]
            cfg_hi,
        });
    }

    /// TEST: run the off-thread worker body (build + publish + drain retire) for the currently-pending
    /// rebuild, exactly as nih_plug's background thread would. Off the audio thread (allocates).
    #[doc(hidden)]
    pub fn force_run_pending_rebuild(&self) {
        if let Some(req) = self.rebuild_pending {
            run_rebuild(&self.rebuild, req);
        }
    }

    /// TEST: the audio-thread consume swap. Wrap this in `assert_no_alloc` to gate alloc-freedom of a
    /// tier/OS switch. Returns `true` if a swap happened.
    #[doc(hidden)]
    pub fn force_consume_rebuild(&mut self) -> bool {
        self.consume_pending_rebuild()
    }

}

impl Plugin for Ba283Plugin {
    const NAME: &'static str = "BA283";
    const VENDOR: &'static str = "desmen";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
            ..AudioIOLayout::const_default()
        },
    ];

    type SysExMessage = ();
    type BackgroundTask = BgTask;

    /// The off-thread worker. nih_plug runs this closure on a dedicated background thread for every
    /// `execute_background(task)` posted from `process`: the tier/OS rebuild (builds the new models,
    /// publishes the boxes, drains the retire queue). Reaches its shared state via a cloned `Arc`.
    fn task_executor(&mut self) -> TaskExecutor<Self> {
        let rebuild = self.rebuild.clone();
        Box::new(move |task| match task {
            BgTask::Rebuild(req) => run_rebuild(&rebuild, req),
        })
    }

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        editor::create(self.params.clone(), self.params.editor_state.clone())
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        context: &mut impl InitContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate;

        // Warm ONE WDF model at the real SR / current gain / bias (both channels are identical), then
        // install its settled snapshot into each channel as the reset target. Off the audio thread —
        // bounded (~1 s of internal-rate solves with early-exit), acceptable at plugin load.
        let rgain = GAIN_STEPS[gain_pos(self.params.circuit_gain.value())].r_kj_ohm;
        let bias = self.params.bias.value();
        let tier = self.params.wdf_tier.value();
        let precision = self.params.precision.value();
        let oversampling = self.params.oversampling.value();
        let cfg = tier
            .config()
            .with_precision(precision.to_wdf())
            .with_oversampling(oversampling.to_wdf());
        // Warm at the realtime tier (the DC operating point is tier-independent — freeze-caps only
        // changes the per-iter companion), then install the settled snapshot into each channel's
        // tier-configured model.
        let mut warm = WdfModel::new(self.sample_rate as f64, rgain as f64, bias as f64);
        warm.warmup();
        let snap = warm.snapshot();
        let age = self.params.age.value();
        let seed = self.params.age_seed.load(Ordering::Relaxed);
        #[cfg(feature = "adaptive-os")]
        let adaptive_on = self.params.adaptive_os.value();
        for ch in self.channels.iter_mut() {
            ch.wdf_rebuild_with(cfg, self.sample_rate, rgain, bias);
            ch.wdf_restore(&snap);
            // Adaptive oversampling (feature `adaptive-os`): on the os=1 lossy tiers under Auto, with
            // the toggle on, build a resident hi-os sibling the envelope detector engages on hot
            // transients.
            #[cfg(feature = "adaptive-os")]
            if adaptive_on
                && matches!(tier, WdfTier::UltraFast | WdfTier::Warp)
                && oversampling == Oversampling::Auto
            {
                let mut hi = cfg;
                hi.os = ADAPTIVE_HI_OS;
                hi.decimate = true;
                ch.wdf_set_hi(hi, self.sample_rate, rgain, bias);
            } else {
                ch.wdf_clear_hi();
            }
            // Re-apply aging after the fresh build (build resets to pristine), then refresh the
            // bias/rgain legs so their aged factor is current.
            ch.wdf_set_age_seed(seed);
            ch.wdf_set_age(age);
            ch.wdf_set_rgain(rgain);
            ch.wdf_set_bias(bias);
            // EQ runs at host rate ahead of the model; size its biquad coeffs to the real SR.
            ch.eq_prepare(self.sample_rate);
        }
        self.last_rgain = rgain;
        self.last_bias = bias;
        self.last_tier = tier;
        self.last_precision = precision;
        self.last_oversampling = oversampling;
        #[cfg(feature = "adaptive-os")]
        {
            self.last_adaptive_os = adaptive_on;
        }
        self.last_age = age;
        self.last_seed = seed;
        self.last_reroll = self.params.age_reroll.value();
        self.wdf_rgain_smooth = rgain;
        self.wdf_bias_smooth = bias;
        self.wdf_age_smooth = age;
        #[cfg(feature = "trafo-core")]
        {
            let on = self.params.trafo_on.value();
            let hyst = self.params.trafo_hyst.value();
            let sat = self.params.trafo_sat.value();
            for ch in self.channels.iter_mut() {
                ch.wdf_set_trafo_enabled(on);
                ch.wdf_set_trafo_voicing(hyst, sat);
            }
            self.last_trafo_on = on;
            self.last_trafo_hyst = hyst;
            self.last_trafo_sat = sat;
            self.wdf_trafo_hyst_smooth = hyst;
            self.wdf_trafo_sat_smooth = sat;
        }

        // levelNorm trim is now a static per-position constant (`GAIN_STEPS[pos].wdf_makeup`), read in
        // `process`. Force a one-shot snap on the first block so the output is leveled from the very
        // first sample with no startup ramp.
        self.norm_reprobe = true;

        // Report the oversampler latency to the host (this nih_plug rev exposes
        // latency via InitContext::set_latency_samples; there is no
        // Plugin::latency_samples trait method).
        context.set_latency_samples(self.latency_samples());
        true
    }

    fn reset(&mut self) {
        for ch in self.channels.iter_mut() {
            ch.reset();
        }
        // Re-probe the trim after a reset (the model is re-settled to its snapshot).
        self.norm_reprobe = true;
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Full chain via the shared per-channel `ChannelDsp` core: Drive ->
        // model (direct near 48 kHz, else via the 2x HostBridge) -> Output, blended
        // with a latency-aligned dry path by Mix. Params are sample-smoothed.
        enable_ftz_daz();
        let use_bridge = self.sample_rate >= dsp::HOST_BRIDGE_THRESHOLD_HZ;
        let backend = self.params.backend.value();
        // FiLM gain-conditioning value for the WaveNet (NeuralNet) backend: the discrete gain position
        // mapped onto the model's trained 0..1 gain axis via its hardware R_KJ. Inert for the WDF
        // backend. (STAGED for the WS4 retrain: the current placeholder weights still use the
        // continuous `rgain_to_gain_norm` map keyed by the position's R_KJ; the retrain will replace
        // this with a discrete 5-entry FiLM embedding indexed directly by `gain_pos`.)
        let gain_pos_idx = gain_pos(self.params.circuit_gain.value());
        let gain_norm = rgain_to_gain_norm(GAIN_STEPS[gain_pos_idx].r_kj_ohm);

        // Neve EQ (both backends): resolve the switched frequencies and push the current settings to
        // each channel at block rate. `Eq::set` recomputes biquad coeffs only when they change, and
        // the stage is a bit-exact identity while `eq_on` is false.
        let eq_on = self.params.eq_on.value();
        let eq_hpf_hz = self.params.eq_hpf_freq.value().hz();
        let eq_low_hz = self.params.eq_low_freq.value().hz();
        let eq_mid_hz = self.params.eq_mid_freq.value().hz();
        let eq_low_t = self.params.eq_low_gain.value();
        let eq_mid_t = self.params.eq_mid_gain.value();
        let eq_high_t = self.params.eq_high_gain.value();
        // Glide the band gains while the EQ is engaged so a knob drag becomes small per-block coeff
        // steps instead of coarse jumps (same one-pole + settle-snap as the WDF block-rate params).
        // While off, snap to target: the stage is bypassed (no audio touched), and a re-enable then
        // starts from the real gains. The bit-exact-off contract is unaffected — `Eq::set` ignores
        // these values until `eq_on`.
        let (eq_low_g, eq_mid_g, eq_high_g) = if eq_on {
            let n = buffer.samples().max(1) as f32;
            let a = 1.0 - (-(n / self.sample_rate) / WDF_PARAM_TAU_S).exp();
            self.eq_low_g_smooth += a * (eq_low_t - self.eq_low_g_smooth);
            self.eq_mid_g_smooth += a * (eq_mid_t - self.eq_mid_g_smooth);
            self.eq_high_g_smooth += a * (eq_high_t - self.eq_high_g_smooth);
            if (eq_low_t - self.eq_low_g_smooth).abs() < EQ_GAIN_EPS {
                self.eq_low_g_smooth = eq_low_t;
            }
            if (eq_mid_t - self.eq_mid_g_smooth).abs() < EQ_GAIN_EPS {
                self.eq_mid_g_smooth = eq_mid_t;
            }
            if (eq_high_t - self.eq_high_g_smooth).abs() < EQ_GAIN_EPS {
                self.eq_high_g_smooth = eq_high_t;
            }
            (self.eq_low_g_smooth, self.eq_mid_g_smooth, self.eq_high_g_smooth)
        } else {
            self.eq_low_g_smooth = eq_low_t;
            self.eq_mid_g_smooth = eq_mid_t;
            self.eq_high_g_smooth = eq_high_t;
            (eq_low_t, eq_mid_t, eq_high_t)
        };
        for ch in self.channels.iter_mut() {
            ch.eq_set(eq_on, eq_hpf_hz, eq_low_hz, eq_low_g, eq_mid_hz, eq_mid_g, eq_high_g);
        }

        // DPR (phase rotator) block-rate config. The all-pass coeffs recompute only when the
        // amount-derived corner changes; the stage is a bit-exact identity while `dpr_on` is false.
        let dpr_on = self.params.dpr_on.value();
        let dpr_amount = self.params.dpr_amount.value();
        for ch in self.channels.iter_mut() {
            ch.dpr_set(dpr_on, dpr_amount, self.sample_rate);
        }

        // WDF gain/bias are consumed at block rate (a cheap G0 rebuild, not per-sample-smoothed). Apply
        // to both channels only when changed; integrator state is preserved so the model re-settles
        // dynamically. NeuralNet blocks skip this entirely.
        if backend == InferenceBackend::Wdf {
            // Tracks whether the WDF operating point changed this block — forces the param legs to
            // re-stamp and arms the levelNorm one-shot snap on a tier/age/gain change.
            let mut changed = false;
            // Glide the block-rate bias/age params toward their targets so a fast knob drag turns into
            // small per-block G0 rebuilds (smooth re-settle) instead of coarse steps. Snap within an
            // epsilon so the one-pole fully settles (no endless near-equal rebuilds).
            let n = buffer.samples().max(1) as f32;
            let a = 1.0 - (-(n / self.sample_rate) / WDF_PARAM_TAU_S).exp();
            // Circuit gain is a hardware DETENT: snap straight to the position's R_KJ (no glide). The
            // apply gate below re-stamps R_KJ once per detent crossing — a cheap G0 rebuild — and the
            // circuit re-settles dynamically (the 60 ms tier crossfade is not needed for a G0 stamp,
            // which preserves integrator state; the levelNorm target jumps via the per-sample ramp).
            let rgain_t = GAIN_STEPS[gain_pos_idx].r_kj_ohm;
            self.wdf_rgain_smooth = rgain_t;
            let bias_t = self.params.bias.value();
            let age_t = self.params.age.value();
            self.wdf_bias_smooth += a * (bias_t - self.wdf_bias_smooth);
            self.wdf_age_smooth += a * (age_t - self.wdf_age_smooth);
            if (bias_t - self.wdf_bias_smooth).abs() < WDF_BIAS_EPS {
                self.wdf_bias_smooth = bias_t;
            }
            if (age_t - self.wdf_age_smooth).abs() < WDF_AGE_EPS {
                self.wdf_age_smooth = age_t;
            }
            let rgain = self.wdf_rgain_smooth;
            let bias = self.wdf_bias_smooth;
            let age = self.wdf_age_smooth;
            // Transformer: the ON/OFF gate is read raw (edge-detected below; the JA reseed makes the
            // toggle click-free). The two voicing dials are glided into the per-block re-voice so a
            // knob drag doesn't step `m_s/k/c`. No partition flip to land on (the J-A is a post-solve
            // filter), so a plain one-pole with an epsilon settle-snap suffices.
            #[cfg(feature = "trafo-core")]
            let (trafo_on, trafo_hyst, trafo_sat) = {
                let on = self.params.trafo_on.value();
                let hyst_t = self.params.trafo_hyst.value();
                let sat_t = self.params.trafo_sat.value();
                self.wdf_trafo_hyst_smooth += a * (hyst_t - self.wdf_trafo_hyst_smooth);
                self.wdf_trafo_sat_smooth += a * (sat_t - self.wdf_trafo_sat_smooth);
                if (hyst_t - self.wdf_trafo_hyst_smooth).abs() < WDF_TRAFO_EPS {
                    self.wdf_trafo_hyst_smooth = hyst_t;
                }
                if (sat_t - self.wdf_trafo_sat_smooth).abs() < WDF_TRAFO_EPS {
                    self.wdf_trafo_sat_smooth = sat_t;
                }
                (on, self.wdf_trafo_hyst_smooth, self.wdf_trafo_sat_smooth)
            };
            // Tier change: rebuild each channel at the new tier and re-install the settled snapshot
            // (no fresh warmup). Block-rate, on actual change only. Reset rgain/bias trackers so the
            // post-rebuild model picks up the current values via the cheap G0 path below.
            let tier = self.params.wdf_tier.value();
            let precision = self.params.precision.value();
            let oversampling = self.params.oversampling.value();
            // Flipping the `Adaptive OS` toggle re-runs this gate so the `wdf_hi` sibling is added or
            // dropped at once (no tier change needed).
            #[cfg(feature = "adaptive-os")]
            let adaptive_on = self.params.adaptive_os.value();
            #[cfg(feature = "adaptive-os")]
            let adaptive_changed = adaptive_on != self.last_adaptive_os;
            #[cfg(not(feature = "adaptive-os"))]
            let adaptive_changed = false;
            if tier != self.last_tier
                || precision != self.last_precision
                || oversampling != self.last_oversampling
                || adaptive_changed
            {
                let cfg = tier
                    .config()
                    .with_precision(precision.to_wdf())
                    .with_oversampling(oversampling.to_wdf());
                // Hi-os sibling config: built only on the adaptive-eligible lossy tiers under Auto
                // with the toggle on; `None` clears it.
                #[cfg(feature = "adaptive-os")]
                let cfg_hi = if adaptive_on
                    && matches!(tier, WdfTier::UltraFast | WdfTier::Warp)
                    && oversampling == Oversampling::Auto
                {
                    let mut hi = cfg;
                    hi.os = ADAPTIVE_HI_OS;
                    hi.decimate = true;
                    Some(hi)
                } else {
                    None
                };
                // Queue an OFF-THREAD rebuild instead of building inline — the swap is alloc-free
                // (done at consume below). The current models keep playing until the worker delivers;
                // the existing 60 ms crossfade then masks the swap. A new gen each request so a fast
                // sweep converges to the latest switch (older builds are ignored / dropped).
                let gen = self.rebuild.latest_gen.load(Ordering::Relaxed) + 1;
                self.rebuild.latest_gen.store(gen, Ordering::Relaxed);
                self.rebuild_pending = Some(RebuildReq {
                    gen,
                    cfg,
                    sr: self.sample_rate,
                    rgain,
                    bias,
                    #[cfg(feature = "adaptive-os")]
                    cfg_hi,
                });
                // Accept the requested state now (the swap is deferred). A further flip before the
                // swap lands re-fires this gate with a newer gen.
                self.last_tier = tier;
                self.last_precision = precision;
                self.last_oversampling = oversampling;
                #[cfg(feature = "adaptive-os")]
                {
                    self.last_adaptive_os = adaptive_on;
                }
            }
            // Consume a finished off-thread build (alloc-free pointer-swap; see the method). The
            // freshly installed models are pristine, so a consume forces the param legs below to
            // re-stamp them and reprobes the levelNorm trim for the new operating point.
            if self.consume_pending_rebuild() {
                changed = true;
            }
            // (Re)post: while a switch is pending and its models are not yet built, keep one request
            // in flight (coalesced by `busy`) so a fast tier/OS sweep converges without flooding.
            if let Some(req) = self.rebuild_pending {
                if self.rebuild.ready_gen.load(Ordering::Acquire) != req.gen {
                    if self
                        .rebuild
                        .busy
                        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
                        .is_ok()
                    {
                        // We own the latch: post and reset the stall watchdog.
                        self.rebuild_stall = 0;
                        context.execute_background(BgTask::Rebuild(req));
                    } else {
                        // Latch held but nothing published yet. Normally the worker is mid-build and
                        // will clear it; but a dropped `execute_background` (full task queue) leaves
                        // it stuck forever. Watchdog it: past the bound the build cannot still be in
                        // flight (see `REBUILD_STALL_LIMIT`), so force-release so the next block
                        // re-posts the same (gen-identical) request.
                        self.rebuild_stall = self.rebuild_stall.saturating_add(1);
                        if self.rebuild_stall >= REBUILD_STALL_LIMIT {
                            self.rebuild.busy.store(false, Ordering::Release);
                            self.rebuild_stall = 0;
                        }
                    }
                } else {
                    self.rebuild_stall = 0;
                }
            } else {
                self.rebuild_stall = 0;
            }
            // Aging (ordered after tier, before bias/rgain): re-roll edge -> seed -> age. Any change
            // forces the live bias/rgain legs to re-apply with the current aged factor.
            let reroll = self.params.age_reroll.value();
            let mut seed = self.params.age_seed.load(Ordering::Relaxed);
            if reroll && !self.last_reroll {
                seed = wdf::reroll_seed(seed);
                self.params.age_seed.store(seed, Ordering::Relaxed);
            }
            self.last_reroll = reroll;
            if seed != self.last_seed {
                for ch in self.channels.iter_mut() {
                    ch.wdf_set_age_seed(seed);
                }
                self.last_seed = seed;
                self.last_age = f32::NAN;
                self.last_rgain = f32::NAN;
                self.last_bias = f32::NAN;
                changed = true;
            }
            if age != self.last_age {
                for ch in self.channels.iter_mut() {
                    ch.wdf_set_age(age);
                }
                self.last_age = age;
                self.last_rgain = f32::NAN;
                self.last_bias = f32::NAN;
                changed = true;
            }
            #[cfg(feature = "trafo-core")]
            if trafo_on != self.last_trafo_on {
                for ch in self.channels.iter_mut() {
                    ch.wdf_set_trafo_enabled(trafo_on);
                }
                self.last_trafo_on = trafo_on;
                changed = true;
            }
            #[cfg(feature = "trafo-core")]
            if trafo_hyst != self.last_trafo_hyst || trafo_sat != self.last_trafo_sat {
                for ch in self.channels.iter_mut() {
                    ch.wdf_set_trafo_voicing(trafo_hyst, trafo_sat);
                }
                self.last_trafo_hyst = trafo_hyst;
                self.last_trafo_sat = trafo_sat;
                changed = true;
            }
            if rgain != self.last_rgain {
                for ch in self.channels.iter_mut() {
                    ch.wdf_set_rgain(rgain);
                }
                self.last_rgain = rgain;
                changed = true;
            }
            if bias != self.last_bias {
                for ch in self.channels.iter_mut() {
                    ch.wdf_set_bias(bias);
                }
                self.last_bias = bias;
                changed = true;
            }

            // `changed` (used above to force the param legs to re-stamp) is also consumed implicitly:
            // a gain/tier/age change moves the static levelNorm target below. No off-thread probe is
            // posted — the trim is the fixed per-position constant read just below.
            let _ = changed;
        }

        // levelNorm trim TARGET: the DRIVE-swept per-position circuit makeup `1/G_sat(drive)` for the
        // current gain switch position and the current block-rate Drive (measured offline; see
        // `GAIN_STEPS` / `wdf_makeup_at`). No runtime probe — positions and the curve are fixed, so
        // this is deterministic. The per-sample ramp below smooths a position/Drive change in. Inert
        // for the NeuralNet backend (which uses its own static `nn_norm_trim`).
        let drive_db_block = self.params.drive_db.value();
        self.norm_trim_target = wdf_makeup_at(&GAIN_STEPS[gain_pos_idx], drive_db_block);

        // levelNorm + NN loudness makeup. Two backends, two mechanisms, both folded into `target_gain`
        // (the per-sample ramp below smooths every transition click-free):
        //   • WDF: the Drive-swept circuit-makeup trim (`1/G_sat(drive)`), applied only when the
        //     param is on; off → unity (faithful circuit gain).
        //   • NeuralNet: Level Norm controls loudness as a single switch. OFF leaves the NN at its
        //     natural FiLM-conditioned level (unity makeup), symmetric with the WDF "faithful circuit
        //     gain" OFF arm; ON levels to unity via `1/G_nn` (Drive-cancelled, same shape as the WDF
        //     trim). No off-thread probe — the NN is a fixed black box, so the trim is a static
        //     measurement (`nn_norm_trim` / tests/calib_nn_makeup.rs).
        //
        // Ramp the APPLIED gain per-sample from this block's start value (`norm_trim`) to the latest
        // target across the block, then carry the end value to the next block — so a moving operating
        // point fades click-free instead of stepping. On a level-norm on/off toggle the target jumps
        // unity<->trim and the same one-block ramp fades it.
        // Drive levelling: the signal is fed `x * drive` into the circuit, so without compensation
        // the output runs hot by the full Drive. levelNorm applies `target = (1/G_sat(drive)) / drive`
        // — `norm_trim_target` is now the reciprocal of the position's LARGE-SIGNAL gain at this Drive
        // (the Drive-swept `GAIN_STEPS` curve), NOT a single small-signal `1/G_v`. So once Drive pushes
        // the circuit into saturation (`G_sat < G_v`) the larger trim cancels exactly what the `/drive`
        // would otherwise over-attenuate, holding the output at ~constant loudness (calibrated at the
        // A0 = 0 dBFS reference; see `bin/measure_gi`) while the drive-induced colour stays. This fixes
        // the regression where the analytic linear `1/(G_v·drive)` sagged the output ~20 dB at +24 dB
        // Drive (the circuit was driven hard but the makeup buried it). Output gain is intentionally
        // NOT cancelled (it is the user's final post-normalization trim).
        // NOTE: the target uses the block-rate Drive (`.value()`), while the audio loop below applies
        // the per-sample SMOOTHED Drive — so during a fast Drive automation the cancellation lags
        // within the block by the smoother's residual. Bounded (≤ the 20 ms Drive smoother span) and
        // transient; matching them sample-accurately would require folding the trim into the smoothed
        // Drive per sample, not worth the cost for a level trim.
        let level_norm_on = self.params.level_norm.value();
        let drive_block = db_to_gain(drive_db_block);
        let target_gain = match backend {
            InferenceBackend::Wdf => {
                if level_norm_on {
                    norm_apply_gain(self.norm_trim_target, drive_block)
                } else {
                    1.0
                }
            }
            InferenceBackend::NeuralNet => {
                // OFF → raw NN (FiLM-only, unity makeup), the NN's natural level. ON → level to ~unity
                // via the `1/G_nn` trim, keyed by the discrete position's hardware R_KJ. (STAGED: the
                // retrain will level-normalize the NN targets, collapsing this toward a single
                // constant; the 5-entry table absorbs the residual per-position correction.)
                //
                // TODO(drive-swept NN parity): this still uses the small-signal `1/G_nn` with the
                // analytic `/drive`, so under heavy Drive the NN output sags the same way the WDF path
                // did before the Drive-swept `wdf_makeup` fix. The WDF backend is the shipped default
                // and the embedded NN weights are still placeholder, so a Drive-swept `nn_norm_trim_at`
                // table is deferred until real weights land (it would otherwise bake throwaway values).
                // Mirror the WDF recipe then: sweep Drive in `tests/calib_nn_makeup.rs`, store
                // `1/G_nn_sat(drive)`, and interpolate here.
                if level_norm_on {
                    norm_apply_gain(nn_norm_trim(GAIN_STEPS[gain_pos_idx].r_kj_ohm), drive_block)
                } else {
                    1.0
                }
            }
        };
        // Forced reprobe (init / reset / first WDF block): snap the applied gain straight to the
        // final, Drive-cancelled target so the output is leveled from the very first sample — Drive
        // included — with no startup ramp. (Snapping to the raw `norm_trim_target` here would still
        // leave a one-block ramp by the Drive factor.)
        if self.norm_reprobe {
            self.norm_trim = target_gain;
            self.norm_reprobe = false;
        }
        let nframes = buffer.samples().max(1) as f32;
        let g_start = self.norm_trim;
        let g_step = (target_gain - g_start) / nframes;
        let mut frame_i = 0.0f32;
        for mut frame in buffer.iter_samples() {
            let drive = db_to_gain(self.params.drive_db.smoothed.next());
            let bias = self.params.bias.smoothed.next();
            // Advance the input/output smoothers every sample (keeps them coherent across a levelNorm
            // toggle). When levelNorm is ON, force INPUT neutral — the auto-comp (`norm_gain`) owns
            // the staging into the circuit, and the editor grays INPUT — but keep OUTPUT live so the
            // user can trim the final level even under levelNorm (the DRIVE knob now colours at
            // constant loudness; OUTPUT is the post-normalization level control). levelNorm OFF → both
            // are the user's manual trims (with `link` mirroring output↔input in the editor).
            let in_gain_db = self.params.input_gain.smoothed.next();
            let in_smooth = db_to_gain(in_gain_db);
            let out_smooth = db_to_gain(self.params.output_db.smoothed.next());
            let (in_g, out_g) = if level_norm_on { (1.0, out_smooth) } else { (in_smooth, out_smooth) };
            // WS4 input-gain FiLM conditioning (NeuralNet 3-channel model). When levelNorm is ON the
            // input trim is forced neutral (0 dB), so condition the model at unity drive too; OFF →
            // the user's actual input_gain. Inert for the WDF backend and for a 2-channel model.
            let input_cond = input_gain_db_to_norm(if level_norm_on { 0.0 } else { in_gain_db });
            let mix = self.params.mix.smoothed.next() * 0.01;
            frame_i += 1.0;
            let norm_gain = g_start + g_step * frame_i;
            for (ch, sample) in frame.iter_mut().enumerate() {
                // AUDIO_IO_LAYOUTS only declare mono/stereo, but guard anyway.
                if ch >= self.channels.len() {
                    break;
                }
                let x = *sample;
                // The conditioning scalar feeds the NeuralNet path only: the WaveNet wants the
                // block-rate `gain_norm`; the WDF path ignores this argument (it uses its own
                // block-rate bias leg) so passing the smoothed `bias` there is harmless.
                let cond = if backend == InferenceBackend::NeuralNet {
                    gain_norm
                } else {
                    bias
                };
                *sample = self.channels[ch].process_sample(
                    x, in_g, drive, cond, input_cond, out_g, norm_gain, mix, use_bridge, backend,
                );
            }
        }
        // The applied gain reached the target by the last frame; carry it as the next block's start.
        self.norm_trim = target_gain;
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for Ba283Plugin {
    const VST3_CLASS_ID: [u8; 16] = *b"ba283-neve-vst3.";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Dynamics];
}

impl ClapPlugin for Ba283Plugin {
    const CLAP_ID: &'static str = "com.desmen.ba283";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Neve BA283 neural emulation");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[ClapFeature::AudioEffect];
}

nih_export_clap!(Ba283Plugin);
nih_export_vst3!(Ba283Plugin);

#[cfg(test)]
mod gain_step_tests {
    use super::{gain_pos, GAIN_STEPS, GAIN_STEPS_LEN};

    #[test]
    fn gain_pos_clamps_to_valid_index() {
        // The IntParam range is 0..=4; gain_pos guards against any out-of-range index.
        assert_eq!(gain_pos(-3), 0);
        assert_eq!(gain_pos(0), 0);
        assert_eq!(gain_pos(4), 4);
        assert_eq!(gain_pos(99), GAIN_STEPS.len() - 1);
        assert_eq!(GAIN_STEPS.len() as i32, GAIN_STEPS_LEN);
    }

    #[test]
    fn r_kj_is_monotonic_descending() {
        // More gain ⇒ smaller R. Guards the table ordering the editor/process logic relies on.
        for pair in GAIN_STEPS.windows(2) {
            assert!(
                pair[0].r_kj_ohm > pair[1].r_kj_ohm,
                "{} ({} Ω) must have larger R than {} ({} Ω)",
                pair[0].label,
                pair[0].r_kj_ohm,
                pair[1].label,
                pair[1].r_kj_ohm
            );
        }
    }
}
