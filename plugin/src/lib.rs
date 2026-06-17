use nih_plug::prelude::*;
use nih_plug_egui::EguiState;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub mod dcblock;
pub mod delay;
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

/// Time constant (seconds) for the per-block glide of the WDF block-rate params (circuit_gain,
/// bias). Smooths coarse host automation steps into small per-block increments so the G0 rebuild
/// re-settles cleanly instead of stepping. Below these epsilons the glide snaps to the target so it
/// fully settles (no perpetual rebuilds from a never-quite-equal one-pole).
const WDF_PARAM_TAU_S: f32 = 0.02;
const WDF_RGAIN_EPS: f32 = 1.0e-3;
const WDF_BIAS_EPS: f32 = 1.0e-5;
const WDF_AGE_EPS: f32 = 1.0e-5;
/// Settle-snap epsilon (dB) for the per-block EQ gain glide: once a band gain is within this of its
/// target, snap so the one-pole fully lands (no endless near-equal coeff recomputes).
const EQ_GAIN_EPS: f32 = 1.0e-3;
#[cfg(feature = "trafo-core")]
const WDF_TRAFO_EPS: f32 = 1.0e-5;

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

/// Operating point for the off-thread levelNorm gain probe. All `Copy` scalars, so posting one to
/// the background worker via `execute_background` never allocates on the audio thread.
#[derive(Clone, Copy)]
pub struct ProbeReq {
    cfg: QualityConfig,
    sr: f32,
    rgain: f32,
    bias: f32,
    age: f32,
    seed: u64,
    #[cfg(feature = "trafo-core")]
    trafo_on: bool,
    #[cfg(feature = "trafo-core")]
    trafo_hyst: f32,
    #[cfg(feature = "trafo-core")]
    trafo_sat: f32,
}

/// The worker's persistent warmed model plus the (cfg, sr) it was built for. Re-stamped in place on
/// each probe so a request costs an `idle_settle` re-converge, not a ~1 s cold `warmup`.
struct ProbeModel {
    model: WdfModel,
    cfg: QualityConfig,
    sr: f32,
}

/// State shared between the audio thread (posts requests, reads the result) and the background probe
/// worker (owns the model, writes the result). The model lives behind a `Mutex` only ever locked by
/// the single worker thread — the audio thread touches only the two atomics, so it never blocks.
struct ProbeShared {
    /// Persistent warmed probe model, lazily built on the first request. Worker-thread only.
    model: Mutex<Option<ProbeModel>>,
    /// Latest probed trim (`1 / G_v`, NOT drive-divided), as `f32` bits. Read each block by `process`.
    trim: AtomicU32,
    /// A probe is in flight. Set by the audio thread (coalesce: at most one queued), cleared by the
    /// worker when it finishes — so a fast sweep never floods the background queue.
    busy: AtomicBool,
}

impl ProbeShared {
    fn new() -> Self {
        Self {
            model: Mutex::new(None),
            trim: AtomicU32::new(1.0f32.to_bits()),
            busy: AtomicBool::new(false),
        }
    }
}

/// Run one levelNorm gain probe: build the worker model (first call, or when cfg/SR change) or cheaply
/// re-stamp it (the common case), re-settle + measure its small-signal voltage gain, and publish
/// `1/G_v` to `shared.trim`. The body of the background executor; also called synchronously in tests.
/// MUST run off the audio thread (it warms / re-settles the circuit).
fn run_probe(shared: &ProbeShared, req: ProbeReq) {
    let mut guard = shared.model.lock().unwrap();
    let rebuild = match guard.as_ref() {
        Some(slot) => slot.cfg != req.cfg || slot.sr != req.sr,
        None => true,
    };
    if rebuild {
        let mut m = WdfModel::new_with(req.cfg, req.sr as f64, req.rgain as f64, req.bias as f64);
        m.set_age_seed(req.seed);
        m.set_age(req.age as f64);
        #[cfg(feature = "trafo-core")]
        {
            m.set_trafo_enabled(req.trafo_on);
            m.set_trafo_voicing(req.trafo_hyst as f64, req.trafo_sat as f64);
        }
        m.warmup();
        *guard = Some(ProbeModel { model: m, cfg: req.cfg, sr: req.sr });
    } else {
        let slot = guard.as_mut().unwrap();
        slot.model.set_age_seed(req.seed);
        slot.model.set_age(req.age as f64);
        slot.model.set_rgain(req.rgain as f64);
        slot.model.set_bias(req.bias as f64);
        #[cfg(feature = "trafo-core")]
        {
            slot.model.set_trafo_enabled(req.trafo_on);
            slot.model.set_trafo_voicing(req.trafo_hyst as f64, req.trafo_sat as f64);
        }
    }
    let slot = guard.as_mut().unwrap();
    let g = slot.model.resettle_and_measure_gain();
    let trim = (1.0 / g).clamp(NORM_TRIM_MIN, NORM_TRIM_MAX);
    shared.trim.store(trim.to_bits(), Ordering::Relaxed);
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
    /// The levelNorm gain actually applied at the end of the last block (the per-sample ramp's start
    /// for the next block). Carries the ramp state across blocks so a Circuit-Gain sweep — or a
    /// level-norm on/off toggle — fades click-free instead of stepping. NOT a program-dependent RMS
    /// estimator (so no pumping). Applied only when the `level_norm` param is on (WDF backend only).
    norm_trim: f32,
    /// The latest probed trim target (`1 / G_v`), refreshed each block from [`Self::probe`]'s atomic
    /// (written by the OFF-THREAD probe worker — the full-fidelity probe is far too slow to run
    /// inline). The applied `norm_trim` ramps toward it per-sample. At a settled operating point this
    /// equals `norm_trim`.
    norm_trim_target: f32,
    /// Forces a one-shot snap of `norm_trim` straight to the (drive-cancelled) target (set on init /
    /// reset / first WDF block) — leveled from the first sample, no startup ramp.
    norm_reprobe: bool,
    /// Off-thread levelNorm gain probe (request via `execute_background`, result via an atomic). The
    /// probe re-settles + measures the circuit at ~5–30 ms — orders of magnitude past a block budget
    /// — so it MUST NOT run on the audio thread; `process` only posts an operating point and reads
    /// back the latest `1/G_v`.
    probe: Arc<ProbeShared>,
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

/// One position of the BA283 stepped gain switch. `r_kj_ohm` is the value actually stamped into the
/// WDF R_KJ leg; the open position uses a large sentinel (resistor removed → leg conductance ≈ 0).
/// `knob_ohm` is the same value clamped into the `circuit_gain` param domain (47–10k Ω) — it drives
/// the knob detent position / DAW automation only, never the model. `label` is the (balanced) dB
/// reading shown in stepped mode.
struct GainStep {
    label: &'static str,
    r_kj_ohm: f32,
    knob_ohm: f32,
}

/// Real BA283 gain-switch wafer resistances (5 positions). Lower R ⇒ more gain. Position 0 is the
/// open contact (no R_KJ): faithful min gain, modelled by a large sentinel resistance. The default
/// `circuit_gain = 220 Ω` lands exactly on position 3 (30 dB) so toggling stepped on causes no jump.
static GAIN_STEPS: &[GainStep] = &[
    GainStep { label: "15 dB", r_kj_ohm: 1.0e9, knob_ohm: 10_000.0 },
    GainStep { label: "20 dB", r_kj_ohm: 1_200.0, knob_ohm: 1_200.0 },
    GainStep { label: "25 dB", r_kj_ohm: 470.0, knob_ohm: 470.0 },
    GainStep { label: "30 dB", r_kj_ohm: 220.0, knob_ohm: 220.0 },
    GainStep { label: "35 dB", r_kj_ohm: 110.0, knob_ohm: 110.0 },
];

/// Map the `circuit_gain` R_KJ value (Ω) onto the WaveNet's trained 0..1 gain axis. The conditioned
/// model was reamped over a geometric R_KJ sweep — 10000 Ω → gain_norm 0.0, 82 Ω → 1.0 — so the
/// inverse is a log ratio (verified against the eval table, e.g. 906 Ω → 0.5). The param's 47 Ω /
/// open positions extrapolate past 1.0 and are clamped (the model saw no data hotter than 82 Ω).
fn rgain_to_gain_norm(rgain_ohm: f32) -> f32 {
    const R_LO: f32 = 10_000.0; // gain_norm 0.0
    const R_HI: f32 = 82.0; // gain_norm 1.0
    let g = (rgain_ohm.max(1.0).ln() - R_LO.ln()) / (R_HI.ln() - R_LO.ln());
    g.clamp(0.0, 1.0)
}

/// Index of the gain step whose `knob_ohm` is nearest `rgain_ohm` in LOG-ohm space (matches the
/// `circuit_gain` skew_factor(-2.0), so the snap midpoints sit where the knob spacing is uniform).
fn nearest_gain_step(rgain_ohm: f32) -> usize {
    let target = rgain_ohm.max(1.0).ln();
    let mut best = 0usize;
    let mut best_d = f32::INFINITY;
    for (i, s) in GAIN_STEPS.iter().enumerate() {
        let d = (s.knob_ohm.ln() - target).abs();
        if d < best_d {
            best_d = d;
            best = i;
        }
    }
    best
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
    /// When on, scale the wet signal so output RMS tracks input RMS (stereo-linked, applied before
    /// `output_db`/`mix`). Off = faithful preamp gain (the default).
    #[id = "level_norm"]
    pub level_norm: BoolParam,
    /// Hardware-stepped gain: when on, the `circuit_gain` knob hard-switches between the 5 real
    /// BA283 wafer positions (`GAIN_STEPS`) instead of gliding continuously — a rotary detent that
    /// re-settles the circuit instantly. Authenticity feature; no CPU effect. WDF-only.
    #[id = "gain_stepped"]
    pub gain_stepped: BoolParam,
    /// R_KJ gain shunt (ohms), WDF-only. Inert for the NeuralNet backend. Block-rate via a `G0`
    /// rebuild (matches the C++ `setBias`/`setRgain`); the plugin glides it per block to smooth
    /// coarse automation steps.
    #[id = "circuit_gain"]
    pub circuit_gain: FloatParam,
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
}

impl Default for Ba283Params {
    fn default() -> Self {
        Self {
            backend: EnumParam::new("Backend", InferenceBackend::Wdf),
            ui_scale: EnumParam::new("UI Scale", UiScale::Auto).non_automatable(),
            wdf_tier: EnumParam::new("WDF Tier", WdfTier::Realtime),
            precision: EnumParam::new("Precision", Precision::High),
            oversampling: EnumParam::new("Oversampling", Oversampling::Auto),
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
            level_norm: BoolParam::new("Level Norm", false),
            gain_stepped: BoolParam::new("Stepped Gain", false),
            circuit_gain: FloatParam::new(
                "Circuit Gain",
                220.0,
                FloatRange::Skewed {
                    min: 47.0,
                    max: 10_000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" Ohm")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),
            age: FloatParam::new("Age", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_rounded(2))
                .with_smoother(SmoothingStyle::Linear(50.0)),
            age_reroll: BoolParam::new("Re-roll Unit", false),
            age_seed: Arc::new(AtomicU64::new(DEFAULT_AGE_SEED)),
            editor_state: EguiState::from_size(660, 900),
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
            norm_trim: 1.0,
            norm_trim_target: 1.0,
            norm_reprobe: true,
            probe: Arc::new(ProbeShared::new()),
            wdf_rgain_smooth: 220.0,
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

    /// Latest probed levelNorm trim (`1 / G_v`, NOT drive-divided). Test/diagnostic accessor.
    #[doc(hidden)]
    pub fn probe_trim(&self) -> f32 {
        f32::from_bits(self.probe.trim.load(Ordering::Relaxed))
    }

    /// Run one probe synchronously (the background worker's body), exercising the real build /
    /// re-stamp / measure path. Test/diagnostic helper — never call on the audio thread.
    #[doc(hidden)]
    #[allow(clippy::too_many_arguments)]
    pub fn force_probe(
        &self,
        cfg: QualityConfig,
        sr: f32,
        rgain: f32,
        bias: f32,
        age: f32,
        seed: u64,
        trafo: f32,
    ) {
        let _ = trafo; // unused when the `trafo-core` feature is off
        // The single `trafo` arg is interpreted as legacy depth: > 0 enables the core and drives the
        // SATURATION dial; HYSTERESIS stays at the default. The probe only measures small-signal
        // gain, so the exact voicing is immaterial here.
        let req = ProbeReq {
            cfg,
            sr,
            rgain,
            bias,
            age,
            seed,
            #[cfg(feature = "trafo-core")]
            trafo_on: trafo > 0.0,
            #[cfg(feature = "trafo-core")]
            trafo_hyst: 0.5,
            #[cfg(feature = "trafo-core")]
            trafo_sat: trafo,
        };
        run_probe(&self.probe, req);
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
    type BackgroundTask = ProbeReq;

    /// The off-thread levelNorm probe worker. nih_plug runs this closure on a dedicated background
    /// thread for every `execute_background(req)` posted from `process`. It owns the persistent probe
    /// model (via the shared `Arc`) and publishes `1/G_v` back through the atomic.
    fn task_executor(&mut self) -> TaskExecutor<Self> {
        let shared = self.probe.clone();
        Box::new(move |req| run_probe(&shared, req))
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
        let rgain = self.params.circuit_gain.value();
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
        for ch in self.channels.iter_mut() {
            ch.wdf_rebuild_with(cfg, self.sample_rate, rgain, bias);
            ch.wdf_restore(&snap);
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

        // Seed the levelNorm trim synchronously off the audio thread (this is `initialize`): probe the
        // just-warmed live model so the very first block is already leveled, before the background
        // worker has run. `norm_reprobe` then snaps the applied gain to it on the first block.
        let seed_g = self.channels[0].wdf_measure_gain();
        self.probe
            .trim
            .store((1.0 / seed_g).clamp(NORM_TRIM_MIN, NORM_TRIM_MAX).to_bits(), Ordering::Relaxed);
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
        let use_bridge = self.sample_rate >= dsp::HOST_BRIDGE_THRESHOLD_HZ;
        let backend = self.params.backend.value();
        // FiLM gain-conditioning value for the WaveNet (NeuralNet) backend: the circuit-gain knob
        // mapped onto the model's trained 0..1 gain axis (block-rate; the model is robust to the
        // small per-block steps a knob drag produces). Inert for the WDF backend.
        let gain_norm = rgain_to_gain_norm(self.params.circuit_gain.value());

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

        // WDF gain/bias are consumed at block rate (a cheap G0 rebuild, not per-sample-smoothed). Apply
        // to both channels only when changed; integrator state is preserved so the model re-settles
        // dynamically. NeuralNet blocks skip this entirely.
        if backend == InferenceBackend::Wdf {
            // Tracks whether the WDF operating point changed this block — drives the levelNorm trim
            // re-probe below (re-measured on every changed block so the trim tracks a live sweep).
            let mut changed = false;
            // Glide the block-rate WDF params toward their targets so a fast knob drag turns into
            // small per-block G0 rebuilds (smooth re-settle) instead of coarse steps. Snap within an
            // epsilon so the one-pole fully settles (no endless near-equal rebuilds).
            let n = buffer.samples().max(1) as f32;
            let a = 1.0 - (-(n / self.sample_rate) / WDF_PARAM_TAU_S).exp();
            let mut rgain_t = self.params.circuit_gain.value();
            let bias_t = self.params.bias.value();
            let age_t = self.params.age.value();
            // Stepped gain: hard-switch to the nearest hardware position and BYPASS the glide (a
            // rotary detent jumps instantly). Snapping `wdf_rgain_smooth` here makes the one-pole
            // below a no-op (delta 0) so `rgain` lands exactly on the step; the apply gate then
            // re-stamps R_KJ once per detent crossing and the circuit re-settles like `set_bias`.
            if self.params.gain_stepped.value() {
                rgain_t = GAIN_STEPS[nearest_gain_step(rgain_t)].r_kj_ohm;
                self.wdf_rgain_smooth = rgain_t;
            }
            self.wdf_rgain_smooth += a * (rgain_t - self.wdf_rgain_smooth);
            self.wdf_bias_smooth += a * (bias_t - self.wdf_bias_smooth);
            self.wdf_age_smooth += a * (age_t - self.wdf_age_smooth);
            // Relative snap: WDF_RGAIN_EPS is a fraction of the target (Circuit Gain spans 47..10000 Ω,
            // so a fixed absolute epsilon would never be hit near the top and the one-pole tail would
            // creep for ~50 blocks, holding `changed` true and starving the trim re-probe). Relative
            // lands the glide within a few blocks at any setting.
            if (rgain_t - self.wdf_rgain_smooth).abs() < WDF_RGAIN_EPS * rgain_t.abs().max(1.0) {
                self.wdf_rgain_smooth = rgain_t;
            }
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
            if tier != self.last_tier
                || precision != self.last_precision
                || oversampling != self.last_oversampling
            {
                let cfg = tier
                    .config()
                    .with_precision(precision.to_wdf())
                    .with_oversampling(oversampling.to_wdf());
                for ch in self.channels.iter_mut() {
                    ch.wdf_set_tier(cfg, self.sample_rate, rgain, bias);
                }
                self.last_tier = tier;
                self.last_precision = precision;
                self.last_oversampling = oversampling;
                changed = true;
                // The rebuild reset aging + the bias/rgain legs to pristine — force all to re-apply.
                self.last_age = f32::NAN;
                self.last_seed = u64::MAX;
                self.last_rgain = f32::NAN;
                self.last_bias = f32::NAN;
                // The rebuild also reset the JA core to linear/default — force the gate + both
                // voicing dials to re-apply.
                #[cfg(feature = "trafo-core")]
                {
                    self.last_trafo_on = !self.params.trafo_on.value();
                    self.last_trafo_hyst = f32::NAN;
                    self.last_trafo_sat = f32::NAN;
                }
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

            // levelNorm trim TARGET: post an OFF-THREAD probe of the circuit's small-signal voltage
            // gain whenever the operating point moves (each glide block of a knob drag / automation
            // lane) or a forced reprobe is pending. The probe re-settles + measures the circuit
            // (~5–30 ms — far past a block budget), so it MUST run on the background worker, never
            // inline. `busy` coalesces to a single in-flight request so a fast sweep never floods the
            // queue; the worker writes `1/G_v` back to the atomic, read below. The probe is a fixed
            // internal sine on the worker's model — a pure function of the operating point, NOT the
            // program — so tracking it introduces no pumping.
            if (changed || self.norm_reprobe)
                && self
                    .probe
                    .busy
                    .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
                    .is_ok()
            {
                let req = ProbeReq {
                    cfg: tier
                        .config()
                        .with_precision(precision.to_wdf())
                        .with_oversampling(oversampling.to_wdf()),
                    sr: self.sample_rate,
                    rgain,
                    bias,
                    age,
                    seed,
                    #[cfg(feature = "trafo-core")]
                    trafo_on,
                    #[cfg(feature = "trafo-core")]
                    trafo_hyst,
                    #[cfg(feature = "trafo-core")]
                    trafo_sat,
                };
                context.execute_background(req);
            }
        }

        // Refresh the trim target from the worker's latest result (lags the operating point by the
        // probe's completion latency; the per-sample ramp below smooths each step in). Seeded
        // synchronously in `initialize`, so it is already correct on the first block.
        self.norm_trim_target = f32::from_bits(self.probe.trim.load(Ordering::Relaxed));

        // levelNorm: the operating-point-tracking trim (the probed circuit-gain reciprocal), applied
        // only when the param is on AND the WDF backend is active. The NeuralNet backend is ignored
        // (its model is broken / not gain-probed), so its norm gain stays unity.
        //
        // Ramp the APPLIED gain per-sample from this block's start value (`norm_trim`) to the latest
        // target across the block, then carry the end value to the next block — so a moving operating
        // point fades click-free instead of stepping. On a level-norm on/off toggle the target jumps
        // unity<->trim and the same one-block ramp fades it. The trim itself comes from the off-thread
        // probe (read above), which lags the operating point by the probe latency; that lag is
        // inaudible for a level trim and the ramp smooths each stepped target in.
        // Drive must also be cancelled: the probed `norm_trim_target` (= 1/G_v) only inverts the
        // circuit's intrinsic small-signal gain, but the signal is fed `x * drive` into the circuit,
        // so without this the output runs hot by the full Drive amount (e.g. +12 dB at Drive +12 dB).
        // Drive is a known scalar, so divide it out analytically: target = 1/(G_v * drive). In the
        // linear region this levels exactly; once Drive pushes the circuit into saturation the
        // effective gain drops below G_v so the output sits slightly *under* unity — safe (no peak),
        // and it preserves the drive-induced colour. Output gain is intentionally NOT cancelled (it is
        // the user's final post-normalization trim, matching the old levelNorm's pre-output position).
        // NOTE: the target uses the block-rate Drive (`.value()`), while the audio loop below applies
        // the per-sample SMOOTHED Drive — so during a fast Drive automation the cancellation lags
        // within the block by the smoother's residual. Bounded (≤ the 20 ms Drive smoother span) and
        // transient; matching them sample-accurately would require folding the trim into the smoothed
        // Drive per sample, not worth the cost for a level trim.
        let level_norm_on = self.params.level_norm.value();
        let drive_block = db_to_gain(self.params.drive_db.value());
        let target_gain = if level_norm_on && backend == InferenceBackend::Wdf {
            norm_apply_gain(self.norm_trim_target, drive_block)
        } else {
            1.0
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
            let out_g = db_to_gain(self.params.output_db.smoothed.next());
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
                    x, drive, cond, out_g, norm_gain, mix, use_bridge, backend,
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
    use super::{nearest_gain_step, GAIN_STEPS};

    #[test]
    fn snaps_each_step_to_itself() {
        for (i, s) in GAIN_STEPS.iter().enumerate() {
            assert_eq!(
                nearest_gain_step(s.knob_ohm),
                i,
                "step {} ({}) must snap to itself",
                i,
                s.label
            );
        }
    }

    #[test]
    fn snaps_to_nearest_in_log_space() {
        // 220 Ω is the default (idx 3). Values between 220 and 470 snap to whichever is nearer in
        // log-Ω: the geometric mean sqrt(220*470) ≈ 321.5 is the boundary.
        let mid = (220.0_f32 * 470.0).sqrt();
        assert_eq!(nearest_gain_step(mid * 0.99), 3, "just below the 220↔470 midpoint → 220 (idx 3)");
        assert_eq!(nearest_gain_step(mid * 1.01), 2, "just above the 220↔470 midpoint → 470 (idx 2)");
    }

    #[test]
    fn clamps_outside_param_range() {
        // Far below the smallest knob_ohm (110) → highest-gain step (idx 4). Far above the largest
        // (10k, the open proxy) → idx 0. nearest_gain_step never panics or indexes out of range.
        assert_eq!(nearest_gain_step(1.0), GAIN_STEPS.len() - 1);
        assert_eq!(nearest_gain_step(1.0e6), 0);
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
