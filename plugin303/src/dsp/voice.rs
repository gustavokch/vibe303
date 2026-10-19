//! Monophonic voice with dual oscillators, sub-oscillator, drift/age modulation.

use super::drift::DriftGenerator;
use super::env::AdsrEnv;
use super::filter::{Filter303, Tier};
use super::ladder::TptLadder;
use super::ladder_wdf::LadderVcf;
use super::osc::BlepOsc;
use super::oversample::{Os2x, Os4x};
use super::rc_env::{AccentSweep, DecayEnv, LeakyIntegrator, RcEnv};
use super::sequencer::SeqEvent;
use super::vco::Vco303;
use super::warm_clipper::{ClipperOs, ClipperParams, WarmClipper};
use crate::params::{
    CircuitTopology, ClipperOversampling, CompTolerance, OscWave, VcaEnvMode, VcfTaps,
};

/// Equal-tempered MIDI note -> frequency (A4 = note 69 = 440 Hz).
pub fn midi_to_freq(note: u8) -> f32 {
    440.0 * 2.0f32.powf((note as f32 - 69.0) / 12.0)
}

pub const BEST_OS_FACTOR: f32 = 4.0;
pub const HIGH_OS_FACTOR: f32 = 2.0;
pub const LOW_OS_FACTOR: f32 = 4.0;
pub const DRAFT_OS_FACTOR: f32 = 4.0;

/// VCO output gain. The circuit-VCO MNA solver outputs a raw saw wave with ~1.12 V p-p swing.
/// SPICE gold has a ~1.46 V p-p swing which after its own gain stages gives ~2.92 V p-p.
/// `VCO_GAIN = 2.92 / 1.120 = 2.607` matches the SPICE drive level into the VCF.
pub const VCO_GAIN: f32 = 2.607;

/// Per-tier VCF brightness (cutoff→bias scale on the shared `LadderVcf`). With
/// the active Q9 resonance (F6.3) the spectral-LSD optimum vs the whole-voice
/// gold sits *below* 1.0 — the resonance peak otherwise over-brightens. Tuned
/// per tier (different OS rate ⇒ different optimum): Best (4×) at 0.70 gives
/// LSD 3.15 dB. Low (4×) at 0.45 gives LSD 12.48 dB, corr 0.978 (0.50: 28.82).
/// Low's optimum is a narrow dip (0.43-0.46 stay under 15 dB): its resonance
/// peak lines up with the gold's there. See `brightness_sweep` /
/// `low_brightness_sweep` diagnostics.
pub const CIRCUIT_BRIGHT: f64 = 0.70;
pub const LOW_BRIGHT: f64 = 0.45;
/// Thermal-noise instance ids of the nodal ladders. A switch-in cold start
/// reseeds with the same id, so the tier's noise restarts as on construction.
const CIRCUIT_LADDER_ID: u32 = 3;
const LOW_LADDER_ID: u32 = 4;
/// Output level trims (linear gain) that match Draft and Low to Best, at the
/// resonance knots `0, 0.25, 0.5, 0.75, 1.0`; linear in between. The tiers'
/// filters lose different amounts of passband level with resonance, so one
/// gain cannot match every resonance setting. Fitted at 4x OS, before the
/// output clipper (so every tier drives it equally), as the inverse of the
/// median tier-vs-Best level (Gate VCA, notes 33/45/57/69 × cutoffs
/// 300/800/1500/3000 Hz per knot). High needs no trim (within 0.02 dB).
/// `tier_levels::draft_level_trim_report` re-derives the Draft table.
const DRAFT_LEVEL_TRIM: [f32; 5] = [1.9828, 0.3730, 0.2677, 0.2791, 0.6080];
const LOW_LEVEL_TRIM: [f32; 5] = [3.9445, 1.4438, 1.2956, 1.2051, 1.5399];

/// Piecewise-linear lookup in a 5-knot trim table over resonance `0..=1`.
#[inline]
fn level_trim(table: &[f32; 5], reso: f32) -> f32 {
    let p = reso.clamp(0.0, 1.0) * 4.0;
    let i = (p as usize).min(3);
    let t = p - i as f32;
    table[i] + (table[i + 1] - table[i]) * t
}
const XFADE_SECS: f32 = 0.010;
const COMP_BUF: usize = 64;

const DRAFT_FC_POLY: [f64; 10] = [
    6.616290964858e+00,
    6.706953156486e-02,
    4.409283505021e+00,
    -1.146864614641e-02,
    2.193149148157e+00,
    -3.530755364554e+01,
    2.957021263752e-03,
    -1.965955442023e-01,
    2.318414355915e+00,
    6.709887478993e+00,
];
const DRAFT_RESO_POLY: [f64; 10] = [
    3.457113966411e+00,
    -1.184911364169e+00,
    5.868184912841e+00,
    1.362861179177e-01,
    -1.933331744519e+00,
    8.040905442518e+00,
    -5.203007668669e-03,
    1.322268656051e-01,
    -7.271724916438e-01,
    1.463974519015e-01,
];
const LOW_FC_POLY: [f64; 10] = [
    2.669789218504e+00,
    1.892645368552e-01,
    -9.246591442637e-01,
    8.001958091115e-02,
    1.226576184485e-01,
    1.003796210111e+00,
    -2.530811864803e-03,
    -2.665874541207e-03,
    -9.870758207804e-02,
    -1.088898286084e-01,
];
const LOW_RESO_POLY: [f64; 10] = [
    -1.532576040675e+00,
    4.707246703637e-01,
    1.409196637283e+00,
    -4.761961286584e-02,
    -4.025363076610e-02,
    -5.908711687437e-01,
    1.587632040395e-03,
    -4.804656170699e-05,
    4.263068702275e-02,
    1.833875053802e-01,
];

#[inline]
fn eval_poly3(x: f64, y: f64, c: &[f64; 10]) -> f64 {
    c[0] + c[1] * x
        + c[2] * y
        + c[3] * x * x
        + c[4] * x * y
        + c[5] * y * y
        + c[6] * x * x * x
        + c[7] * x * x * y
        + c[8] * x * y * y
        + c[9] * y * y * y
}

#[derive(Clone)]
struct CompDelay {
    buf: [f32; COMP_BUF],
    pos: usize,
    delay: usize,
}

impl CompDelay {
    fn new(delay: usize) -> Self {
        Self {
            buf: [0.0; COMP_BUF],
            pos: 0,
            delay,
        }
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        self.buf[self.pos] = x;
        let rd = (self.pos + COMP_BUF - self.delay) % COMP_BUF;
        let y = self.buf[rd];
        self.pos = (self.pos + 1) % COMP_BUF;
        y
    }

    fn reset(&mut self) {
        self.buf = [0.0; COMP_BUF];
        self.pos = 0;
    }
}

/// Clipper control values passed from the plugin layer to the voice's
/// [`WarmClipper`]. Named fields make argument transposition a compile error;
/// the voice layer owns the `ClipperOversampling` → `ClipperOs` mapping.
#[derive(Clone, Copy, Debug)]
pub struct VoiceClipperParams {
    pub bypass: bool,
    pub os: ClipperOversampling,
    pub drive: f32,
    pub threshold: f32,
    pub bass: f32,
    pub tone: f32,
    pub asym: bool,
    pub mix: f32,
    pub in_db: f32,
    pub out_db: f32,
    pub tilt_db: f32,
    pub dynamics: f32,
}

fn clipper_params_map(p: VoiceClipperParams) -> ClipperParams {
    let drive_norm = p.drive.clamp(0.0, 1.0);
    let threshold_norm = p.threshold.clamp(0.0, 1.0);
    let bass_norm = p.bass.clamp(0.0, 1.0);
    let tone_norm = p.tone.clamp(0.0, 1.0);

    ClipperParams {
        bypass: p.bypass,
        os: match p.os {
            ClipperOversampling::X2 => ClipperOs::X2,
            ClipperOversampling::X4 => ClipperOs::X4,
        },
        drive_ohms: 22_000.0 * (100_000.0 / 22_000.0_f32).powf(drive_norm),
        threshold_ohms: 2_200.0 * (22_000.0 / 2_200.0_f32).powf(threshold_norm),
        bass_ohms: 4_700.0 * (22_000.0 / 4_700.0_f32).powf(bass_norm),
        tone_f: 470e-12 * (4.7e-9 / 470e-12_f32).powf(tone_norm),
        asym: p.asym,
        mix: p.mix,
        in_db: p.in_db,
        out_db: p.out_db,
        tilt_db: p.tilt_db,
        dynamics: p.dynamics,
    }
}

/// Monophonic voice structure managing the different fidelity paths, modulation sources, and envelopes.
pub struct Voice {
    // --- Draft Tier ---
    /// Primary polyBLEP oscillator for Draft tier.
    osc: BlepOsc,
    /// Detunable secondary oscillator for Draft tier.
    osc2: BlepOsc,
    /// Sub-oscillator for Draft tier (always a square wave, one octave below).
    sub_osc: BlepOsc,
    /// Textbook coincident-pole zero-delay-feedback VCF for Draft tier, run
    /// inside `os4_draft` at 4x.
    filter: TptLadder,
    /// Draft-tier oversampling filter (4x Kaiser FIR).
    os4_draft: Os4x,

    // --- Low Tier ---
    /// Primary polyBLEP oscillator for Low tier.
    osc_low: BlepOsc,
    /// Detunable secondary oscillator for Low tier.
    osc2_low: BlepOsc,
    /// Sub-oscillator for Low tier (always a square wave, one octave below).
    sub_osc_low: BlepOsc,
    /// Low-tier nodal diode-ladder (polyBLEP saw source), run inside `os4_low` at 4x.
    ladder_low: LadderVcf,
    /// Low-tier oversampling filter (4x Kaiser FIR).
    os4_low: Os4x,

    // --- High and Best Tiers (circuit VCO) ---
    // The oscillator source is shared and ticked once per output sample, also
    // during a High <-> Best crossfade. Each tier owns its filter path.
    /// Nodal circuit VCO for the High and Best tiers (replaces the polyBLEP saw there).
    vco: Vco303,
    /// Secondary nodal circuit VCO.
    vco2: Vco303,
    /// Sub-oscillator for the circuit-VCO tiers (always a square wave, one octave below).
    sub_osc_circuit: BlepOsc,
    /// High-tier nodal diode-ladder, run inside `os2_high` at 2x.
    ladder_high: LadderVcf,
    /// High-tier oversampling filter (2x Kaiser FIR).
    os2_high: Os2x,
    /// Best-tier nodal diode-ladder, run inside `os4_best` at 4x.
    ladder_best: LadderVcf,
    /// Best-tier oversampling filter (4x Kaiser FIR).
    os4_best: Os4x,

    // --- Envelopes & Modulators ---
    /// Filter envelope (MEG), ADSR structure collapsing to classic AD.
    meg: AdsrEnv,
    /// Amplitude envelope (VEG), ADSR structure.
    veg: AdsrEnv,
    /// Pitch random walk drift generator.
    drift_pitch: DriftGenerator,
    /// VCF cutoff random walk drift generator.
    drift_cutoff: DriftGenerator,
    /// Resonance Q random walk drift generator.
    drift_reso: DriftGenerator,
    /// Pitch random walk drift generator for Osc2.
    drift_pitch_osc2: DriftGenerator,
    /// Accent decay integrator accumulator.
    accent_acc: f32,

    // --- Classic 303 Envelope Chain ---
    /// Main Envelope Generator (pure exponential decay) — drives VCF cutoff
    meg_decay: DecayEnv,
    /// RC smoother for envMod path (tau = normalAttack, default 3ms)
    rc1: LeakyIntegrator,
    /// Accent Sweep memory circuit (C13)
    sweep_c13: AccentSweep,
    /// Amplitude envelope (RC-filtered AHDSR)
    vca_env: RcEnv,
    /// RC smoother for VCA accent injection (R119/C36, tau = 1.55ms)
    vca_accent_rc: LeakyIntegrator,
    /// Amplitude de-click lowpass (optional, tau ~ 2ms)
    amp_declicker: LeakyIntegrator,

    // Accent state
    accent_gain: f32,     // 0.0 or accent_knob value (set per-note)
    normal_decay_ms: f64, // decay knob → 30..3000 ms
    accent_decay_ms: f64, // accent decay knob -> 30..3000 ms

    // EnvMod scaling (measured curve from hardware)
    env_scaler: f64,
    env_offset: f64,

    // --- Tier Switch/Crossfade ---
    /// Active (target) tier.
    tier: Tier,
    /// Tier being faded *out*, if a crossfade is in progress. `None` = steady.
    xfade_from: Option<Tier>,
    /// Crossfade position in [0, 1] (0 = all `xfade_from`, 1 = all `tier`).
    xfade_pos: f32,
    /// Per-sample crossfade increment (= 1 / fade-length-samples).
    xfade_step: f32,
    /// Tier requested while a fade was still in flight — applied as the next fade
    /// once the current one completes, so a fast multi-tier sweep never hard-cuts
    /// a still-weighted path.
    pending_tier: Option<Tier>,
    /// Compensation delay line for Draft tier to align latency with Best.
    comp_draft: CompDelay,
    /// Compensation delay line for Low tier to align latency with Best.
    comp_low: CompDelay,
    /// Compensation delay line for High tier (2x OS) to align latency with Best.
    comp_high: CompDelay,
    /// Compensation delay line for Best tier (typically 0 delay: Best sets the max).
    comp_best: CompDelay,
    /// Common reported latency (base samples) = max OS group delay over all tiers.
    max_latency: usize,
    /// Global staged Germanium/JFET output clipper, after VCA/resonance compensation.
    clipper: WarmClipper,
    /// Sample rate.
    sr: f32,

    // --- Parameter States ---
    /// Currently triggered MIDI note.
    current_note: u8,
    /// Base note frequency in Hz.
    base_hz: f32,
    /// Pitch offset for Oscillator 1 (semitones).
    osc1_pitch_st: f32,
    /// Pitch offset for Oscillator 2 (semitones).
    osc2_pitch_st: f32,
    /// Relative volume of Oscillator 2 in [0, 1].
    osc2_vol: f32,
    /// Relative volume of sub-oscillator in [0, 1].
    sub_vol: f32,
    /// Normalised VCF Cutoff knob setting.
    cutoff_knob: f32,
    /// Normalised VCF Resonance knob setting.
    reso_knob: f32,
    /// Resonance after drift, as the filters see it this sample (level trims).
    final_reso: f32,
    /// Normalised VCF Envelope Modulation intensity knob setting.
    env_mod: f32,
    /// Normalised Envelope Decay knob setting.
    decay_knob: f32,
    /// Normalised Accent intensity knob setting.
    accent_knob_val: f32,
    /// Master output volume (dB).
    master_volume: f32,
    /// Main VCF drive factor.
    drive: f32,
    /// Component age wear level in [0, 10].
    age: i32,
    /// Component tolerance.
    tolerance: CompTolerance,
    /// Seed for component variation.
    comp_seed: i32,
    /// Voice drift amount in [0.0, 1.0].
    drift: f32,
    /// Trimmer recalibration scale in [0.0, 1.0].
    recalibration_scale: f64,
    draft_cutoff_mult: f64,
    draft_reso_mult: f64,
    /// Thermal noise level in [0, 50].
    thermal_noise: i32,
    /// Dynamic operating temperature in degrees Celsius (20.0 to 45.0)
    temperature: f32,
    /// Last VCO thermal step applied by `tick` (0..=21 step scale; -1 = unset).
    thermal_noise_state: i32,
    /// Last raw `thermal_noise` parameter applied to the ladder filters
    /// (parameter scale; -1 = unset). Kept separate from `thermal_noise_state`
    /// because the ladders track the raw parameter while the VCOs track the
    /// temperature-derived step.
    ladder_thermal_state: i32,
    /// Fixed tuning offset.
    tuning_offset: f32,
    /// VCA envelope trigger mode.
    vca_env_mode: VcaEnvMode,
    /// Toggle to use advanced envelope settings instead of classic 303 RC envelopes.
    advanced_env: bool,
    /// Current gate-on state of the voice.
    pub gate: bool,
    /// Current active step accent modifier.
    accent_active: bool,
    /// Active circuit topology.
    pub topology: CircuitTopology,
    /// Slide active state.
    slide_active: bool,
    /// Pitch slew for exponential slide (tau = 12ms -> 60ms constant-time slide).
    pitch_slew: LeakyIntegrator,
    /// Current pitch in semitones.
    current_pitch_st: f32,
    /// Target pitch in semitones.
    target_pitch_st: f32,
    /// Glide knob.
    glide_knob: f32,
    /// VCF resonance gain compensation amount in [0.0, 1.0].
    reso_comp: f32,
    /// Cached classic slide tau in ms.
    classic_slide_tau_ms: f64,
    /// Whether the VCO LUTs need recalculation before running Tier::High or Tier::Best.
    vco_dirty: bool,
}

impl Voice {
    fn decay_map(k: f32) -> f32 {
        const P: [(f32, f32); 13] = [
            (0.0, 100.0),
            (0.1, 150.0),
            (0.2, 200.0),
            (0.3, 250.0),
            (0.4, 300.0),
            (0.5, 400.0),
            (0.6, 500.0),
            (0.7, 600.0),
            (0.8, 700.0),
            (0.85, 1000.0),
            (0.9, 1400.0),
            (0.95, 1750.0),
            (1.0, 2100.0),
        ];
        let mut lo = 0usize;
        let mut hi = P.len() - 1;
        while lo + 1 < hi {
            let mid = (lo + hi) / 2;
            if P[mid].0 < k {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let (x0, y0) = P[lo];
        let (x1, y1) = P[hi];
        if x1 - x0 < 1e-6 {
            y0
        } else {
            y0 + (y1 - y0) * (k - x0) / (x1 - x0)
        }
    }

    pub fn new(sr: f32) -> Self {
        let lat_draft = Os4x::new().latency_samples();
        let lat_low = Os4x::new().latency_samples();
        let lat_high = Os2x::new().latency_samples();
        let lat_best = Os4x::new().latency_samples();
        let max_latency = lat_best.max(lat_high).max(lat_low).max(lat_draft);
        // Each nodal ladder tunes its capacitors from the constructor rate, so
        // it is built at the oversampled clock its tier drives it at (see the
        // unit test pinning this below).
        let mut ladder_best = LadderVcf::new(sr * BEST_OS_FACTOR);
        ladder_best.set_instance_id(CIRCUIT_LADDER_ID);
        ladder_best.set_cal(CIRCUIT_BRIGHT);
        let mut ladder_high = LadderVcf::new(sr * HIGH_OS_FACTOR);
        ladder_high.set_instance_id(CIRCUIT_LADDER_ID);
        ladder_high.set_cal(CIRCUIT_BRIGHT);
        let mut ladder_low = LadderVcf::new(sr * LOW_OS_FACTOR);
        ladder_low.set_instance_id(LOW_LADDER_ID);
        ladder_low.set_cal(LOW_BRIGHT);

        // Sub oscillators are always square waves
        let square = || {
            let mut sub = BlepOsc::new();
            sub.set_wave(OscWave::Square);
            sub
        };

        let mut vco = Vco303::new(sr);
        vco.set_instance_id(1);
        let mut vco2 = Vco303::new(sr);
        vco2.set_instance_id(2);

        let mut voice = Self {
            osc: BlepOsc::new(),
            osc2: BlepOsc::new(),
            sub_osc: square(),
            filter: TptLadder::new(sr * DRAFT_OS_FACTOR),
            os4_draft: Os4x::new(),

            osc_low: BlepOsc::new(),
            osc2_low: BlepOsc::new(),
            sub_osc_low: square(),
            ladder_low,
            os4_low: Os4x::new(),

            vco,
            vco2,
            sub_osc_circuit: square(),
            ladder_high,
            os2_high: Os2x::new(),
            ladder_best,
            os4_best: Os4x::new(),

            meg: AdsrEnv::new(sr),
            veg: AdsrEnv::new(sr),
            drift_pitch: DriftGenerator::new(1337),
            drift_cutoff: DriftGenerator::new(7331),
            drift_reso: DriftGenerator::new(2468),
            drift_pitch_osc2: DriftGenerator::new(3579),
            accent_acc: 0.0,

            meg_decay: DecayEnv::new(sr as f64),
            rc1: {
                let mut li = LeakyIntegrator::new(sr as f64);
                li.set_time_constant(3.0);
                li
            },
            sweep_c13: AccentSweep::new(sr as f64),
            vca_env: RcEnv::new(sr as f64),
            vca_accent_rc: {
                let mut li = LeakyIntegrator::new(sr as f64);
                li.set_time_constant(1.55); // 47 kΩ * 0.033 µF = 1.55 ms
                li
            },
            amp_declicker: {
                let mut li = LeakyIntegrator::new(sr as f64);
                li.set_time_constant(2.0);
                li
            },
            accent_gain: 0.0,
            normal_decay_ms: 200.0,
            accent_decay_ms: 200.0,
            env_scaler: 1.0,
            env_offset: 0.0,

            pitch_slew: {
                let mut li = LeakyIntegrator::new(sr as f64);
                li.set_time_constant(22.0);
                li.set_state(60.0);
                li
            },

            tier: Tier::Draft,
            xfade_from: None,
            xfade_pos: 1.0,
            xfade_step: 1.0 / (sr * XFADE_SECS).max(1.0),
            pending_tier: None,
            comp_draft: CompDelay::new(max_latency - lat_draft),
            comp_low: CompDelay::new(max_latency - lat_low),
            comp_high: CompDelay::new(max_latency - lat_high),
            comp_best: CompDelay::new(max_latency - lat_best),
            max_latency,
            clipper: WarmClipper::new(sr),
            sr,

            current_note: 60,
            base_hz: 261.63,
            tuning_offset: 0.0,
            osc1_pitch_st: 0.0,
            osc2_pitch_st: 0.0,
            osc2_vol: 0.0,
            sub_vol: 0.0,
            cutoff_knob: 0.5,
            reso_knob: 0.0,
            final_reso: 0.0,
            env_mod: 0.0,
            decay_knob: 0.5,
            accent_knob_val: 0.0,
            master_volume: 0.0,
            drive: 1.0,
            age: 0,
            tolerance: CompTolerance::Zero,
            comp_seed: 0,
            drift: 0.0,
            recalibration_scale: 1.0,
            draft_cutoff_mult: 1.0,
            draft_reso_mult: 1.0,
            thermal_noise: 0,
            temperature: 20.0,
            thermal_noise_state: -1,
            ladder_thermal_state: -1,
            vca_env_mode: VcaEnvMode::Envelope,
            advanced_env: false,
            gate: false,
            accent_active: false,
            topology: CircuitTopology::Accurate,
            slide_active: false,
            current_pitch_st: 60.0,
            target_pitch_st: 60.0,
            glide_knob: 0.5,
            reso_comp: 0.0,
            classic_slide_tau_ms: 22.0,
            vco_dirty: true,
        };
        // Bring the circuit ladders to the current thermal and wear state, as
        // `update_params` does on every later change.
        let tol = voice.tolerance_f64();
        for ladder in [&mut voice.ladder_high, &mut voice.ladder_best] {
            ladder.set_thermal(voice.thermal_noise);
            ladder.set_analog_wear(
                tol,
                voice.age,
                voice.comp_seed as u32,
                voice.recalibration_scale,
            );
        }
        voice
    }

    pub fn get_reso_comp(&self) -> f32 {
        self.reso_comp
    }

    pub fn set_reso_comp(&mut self, comp: f32) {
        self.reso_comp = comp.clamp(0.0, 1.0);
    }

    pub fn get_accent_acc(&self) -> f32 {
        self.accent_acc
    }

    pub fn temperature(&self) -> f32 {
        self.temperature
    }

    pub fn get_vco_cv_lut(&self) -> [f64; 37] {
        self.vco.get_cv_lut()
    }

    pub fn recalculate_luts_sync(&mut self) {
        let tol_f64 = self.tolerance_f64();
        self.vco.recalculate_luts_sync_with_recal(
            self.age,
            tol_f64,
            self.comp_seed as u32,
            self.recalibration_scale,
        );
        self.vco2.recalculate_luts_sync_with_recal(
            self.age,
            tol_f64,
            self.comp_seed as u32,
            self.recalibration_scale,
        );
        self.vco_dirty = false;
    }

    pub fn get_current_pitch(&self) -> f32 {
        self.current_pitch_st
    }

    pub fn set_vca_env_mode(&mut self, mode: VcaEnvMode) {
        self.vca_env_mode = mode;
    }

    pub fn set_advanced_env(&mut self, enabled: bool) {
        self.advanced_env = enabled;
    }

    fn tolerance_f64(&self) -> f64 {
        self.tolerance.as_f64()
    }

    fn calculate_env_mod_scaler(&mut self, cutoff_hz: f64, env_mod_normalized: f64) {
        const C0: f64 = 3.138152786059267e+002; // lowest nominal cutoff
        const C1: f64 = 2.394411986817546e+003; // highest nominal cutoff
        const OF: f64 = 0.048292930943553; // offset factor
        const OC: f64 = 0.294391201442418; // offset constant
        const SLO_F: f64 = 3.773996325111173; // scaler factor at low cutoff
        const SLO_C: f64 = 0.736965594166206; // scaler constant at low cutoff
        const SHI_F: f64 = 4.194548788411135; // scaler factor at high cutoff
        const SHI_C: f64 = 0.864344900642434; // scaler constant at high cutoff

        let e = env_mod_normalized;
        let c = exp_to_lin(cutoff_hz, C0, C1).clamp(0.0, 1.0);
        let s_lo = SLO_F * e + SLO_C;
        let s_hi = SHI_F * e + SHI_C;
        self.env_scaler = (1.0 - c) * s_lo + c * s_hi;
        self.env_offset = OF * c + OC;
    }

    /// Update the internal voice states with block-rate parameter readings from the host.
    pub fn update_params(
        &mut self,
        tuning_cents: f32,
        osc1_pitch_st: f32,
        osc2_pitch_st: f32,
        osc2_vol: f32,
        osc1_wave: OscWave,
        osc1_pw: f32,
        sub_vol: f32,
        cutoff: f32,
        resonance: f32,
        env_mod: f32,
        decay_knob: f32,
        accent_knob: f32,
        volume_db: f32,
        vcf_taps: VcfTaps,
        vcf_drive_db: f32,
        advanced_env: bool,
        meg_attack: f32,
        meg_decay_adj: f32,
        meg_sustain: f32,
        meg_release: f32,
        veg_attack: f32,
        veg_decay_adj: f32,
        veg_sustain: f32,
        veg_release: f32,
        vca_env_mode: VcaEnvMode,
        comp_age: i32,
        comp_drift: f32,
        tolerance: CompTolerance,
        thermal_noise: i32,
        topology: CircuitTopology,
        sweep_pos: f32,
        comp_seed: i32,
        glide_knob: f32,
        reso_comp: f32,
        recalibration_scale: f64,
    ) {
        let age_changed = self.age != comp_age;
        let tolerance_changed = self.tolerance != tolerance || self.comp_seed != comp_seed;
        let recal_changed = self.recalibration_scale != recalibration_scale;
        let topology_changed = self.topology != topology;

        if self.comp_seed != comp_seed {
            let seed = comp_seed as u32;
            self.drift_pitch = DriftGenerator::new(seed.wrapping_add(1337));
            self.drift_cutoff = DriftGenerator::new(seed.wrapping_add(7331));
            self.drift_reso = DriftGenerator::new(seed.wrapping_add(2468));
            self.drift_pitch_osc2 = DriftGenerator::new(seed.wrapping_add(3579));
        }

        self.tuning_offset = tuning_cents;
        self.age = comp_age;
        self.drift = comp_drift;
        self.tolerance = tolerance;
        self.comp_seed = comp_seed;
        self.thermal_noise = thermal_noise;
        self.recalibration_scale = recalibration_scale;

        let mean_c_mult = 1.0 - 0.05 * (comp_age as f64 / 100.0);
        self.draft_cutoff_mult = 1.0 / mean_c_mult;
        self.draft_reso_mult = 1.0 - 0.10 * (comp_age as f64 / 100.0);

        let tol_f64 = self.tolerance_f64();

        self.set_topology(topology);

        // Topology-change recalcs are owned by `set_topology` (called above,
        // after `self.age`/`self.tolerance` are updated, so it sees fresh
        // values). This branch owns age/tolerance/recal changes only —
        // including `topology_changed` here would fire a second recalc that
        // always supersedes the one `set_topology` just started.
        if age_changed || tolerance_changed || recal_changed {
            let is_fun_mode = topology == CircuitTopology::Fun;
            self.vco.set_fun(is_fun_mode);
            self.vco2.set_fun(is_fun_mode);
            for ladder in [&mut self.ladder_high, &mut self.ladder_best, &mut self.ladder_low] {
                ladder.set_analog_wear(
                    tol_f64,
                    comp_age,
                    comp_seed as u32,
                    self.recalibration_scale,
                );
            }

            if self.tier == Tier::Best
                || self.tier == Tier::High
                || self.pending_tier == Some(Tier::Best)
                || self.pending_tier == Some(Tier::High)
                || self.xfade_from.is_some()
            {
                let ok1 = self.vco.recalculate_luts_async(
                    comp_age,
                    tol_f64,
                    comp_seed as u32,
                    recalibration_scale,
                );
                let ok2 = self.vco2.recalculate_luts_async(
                    comp_age,
                    tol_f64,
                    comp_seed as u32,
                    recalibration_scale,
                );
                if ok1 && ok2 {
                    self.vco_dirty = false;
                }
            } else {
                self.vco
                    .set_analog_wear(tol_f64, comp_age, comp_seed as u32);
                self.vco2
                    .set_analog_wear(tol_f64, comp_age, comp_seed as u32);
                self.vco_dirty = true;
            }
        }

        // Ladders track the raw thermal_noise parameter. The VCOs are deliberately
        // NOT set here: their thermal state uses the 0..=21 step scale derived from
        // `self.temperature` and is owned by `tick` (which runs every sample and
        // re-asserts it via `thermal_noise_state`). Passing the raw 0..100 parameter
        // to `Vco303::set_thermal` would be read as a step — up to 125 °C device
        // temperature and a clamped-hot cv_lut index — and would mix scales in
        // `thermal_noise_state`.
        if self.thermal_noise != self.ladder_thermal_state
            || age_changed
            || tolerance_changed
            || topology_changed
        {
            self.ladder_high.set_thermal(thermal_noise);
            self.ladder_best.set_thermal(thermal_noise);
            self.ladder_low.set_thermal(thermal_noise);
            self.ladder_thermal_state = thermal_noise;
        }

        let (
            final_osc1_pitch,
            final_osc2_pitch,
            final_osc2_vol,
            final_sub_vol,
            final_wave,
            final_pw,
            final_sweep,
            final_poles,
            final_drive_db,
            final_advanced_env,
            final_vca_env_mode,
            final_reso_comp,
        ) = match topology {
            CircuitTopology::Accurate => {
                // Coerce Sweep waveform to Saw (unsupported in accurate)
                let wave = match osc1_wave {
                    OscWave::Sweep => OscWave::Saw,
                    other => other,
                };
                (
                    0.0,                  // osc1_pitch_st
                    0.0,                  // osc2_pitch_st
                    0.0,                  // osc2_vol
                    0.0,                  // sub_vol
                    wave,                 // waveform
                    0.5,                  // osc1_pw (fixed to 50% for standard square)
                    0.0,                  // sweep_pos
                    VcfTaps::FourPole,    // vcf_taps
                    0.0,                  // vcf_drive
                    false,                // advanced_env
                    VcaEnvMode::Envelope, // vca_env_mode
                    reso_comp,            // reso_comp
                )
            }
            CircuitTopology::Fun => {
                // In fun mode, the oscillator is forced to Sweep (so discrete waveform switch is ignored)
                // and sweep_pos morphs between Saw and Square.
                (
                    osc1_pitch_st,
                    osc2_pitch_st,
                    osc2_vol,
                    sub_vol,
                    OscWave::Sweep,
                    osc1_pw,
                    sweep_pos,
                    vcf_taps,
                    vcf_drive_db,
                    advanced_env,
                    vca_env_mode,
                    reso_comp,
                )
            }
        };

        self.osc1_pitch_st = final_osc1_pitch;
        self.osc2_pitch_st = final_osc2_pitch;
        self.osc2_vol = final_osc2_vol;
        self.sub_vol = final_sub_vol;
        self.cutoff_knob = cutoff;
        self.reso_knob = resonance;
        self.env_mod = env_mod;
        self.decay_knob = decay_knob;
        self.accent_knob_val = accent_knob;
        self.master_volume = volume_db;
        self.drive = 10.0f32.powf(final_drive_db / 20.0);
        self.vca_env_mode = final_vca_env_mode;
        self.advanced_env = final_advanced_env;
        self.glide_knob = glide_knob;
        self.reso_comp = final_reso_comp;
        self.classic_slide_tau_ms = calculate_classic_slide_tau_ms(topology, glide_knob);

        // Propagate tap/pole count
        let poles_n = match final_poles {
            VcfTaps::TwoPole => 2,
            VcfTaps::ThreePole => 3,
            VcfTaps::FourPole => 4,
        };
        self.filter.set_poles(poles_n);
        self.ladder_high.set_poles(poles_n);
        self.ladder_best.set_poles(poles_n);
        self.ladder_low.set_poles(poles_n);

        // Update Osc Waveforms & PW parameters
        self.osc.set_wave(final_wave);
        self.osc.set_pw(final_pw);
        self.osc.set_sweep(final_sweep);

        self.osc2.set_wave(final_wave);
        self.osc2.set_pw(final_pw);
        self.osc2.set_sweep(final_sweep);

        self.osc_low.set_wave(final_wave);
        self.osc_low.set_pw(final_pw);
        self.osc_low.set_sweep(final_sweep);

        self.osc2_low.set_wave(final_wave);
        self.osc2_low.set_pw(final_pw);
        self.osc2_low.set_sweep(final_sweep);

        self.vco.set_wave(final_wave);
        self.vco.set_sweep(final_sweep as f64);

        self.vco2.set_wave(final_wave);
        self.vco2.set_sweep(final_sweep as f64);

        // Update Envelopes sample rates to self.sr
        let sr_f64 = self.sr as f64;
        self.meg_decay.set_sample_rate(sr_f64);
        self.vca_env.set_sample_rate(sr_f64);
        self.rc1.set_sample_rate(sr_f64);
        self.sweep_c13.set_sample_rate(sr_f64);
        self.vca_accent_rc.set_sample_rate(sr_f64);
        self.amp_declicker.set_sample_rate(sr_f64);
        self.pitch_slew.set_sample_rate(sr_f64);

        // Update Envelopes
        if final_advanced_env {
            self.meg.set_attack(meg_attack);
            self.meg.set_decay(meg_decay_adj);
            self.meg.set_sustain(meg_sustain);
            self.meg.set_release(meg_release);

            self.veg.set_attack(veg_attack);
            self.veg.set_decay(veg_decay_adj);
            self.veg.set_sustain(veg_sustain);
            self.veg.set_release(veg_release);
        } else {
            // Classic 303: attack is fixed to fast ~3 ms
            self.meg.set_attack(3.0);
            self.veg.set_attack(3.0);

            // Decay knob: 100 ms (min) to 2100 ms (max) with finer granularity 200-700 ms
            let decay_ms = Self::decay_map(decay_knob);
            self.meg.set_decay(decay_ms);
            self.meg.set_sustain(0.0);
            self.meg.set_release(200.0);

            // VCA Decay uses same scaled knob mapping
            let vca_decay_ms = Self::decay_map(decay_knob);
            self.veg.set_decay(vca_decay_ms);
            self.veg.set_sustain(0.0);
            self.veg.set_release(200.0);

            // --- Classic RC-based settings (both topologies) ---
            // MEG decay from knob: same scaled mapping (replaces 30..3000 linear)
            self.normal_decay_ms = Self::decay_map(decay_knob) as f64;
            // On hardware TB-303, the 4066 switch shorts the DECAY pot on accented notes,
            // locking MEG decay to a fixed ~200 ms regardless of the panel Decay or Accent knob settings.
            self.accent_decay_ms = 200.0;

            // VCA envelope: fixed 303 timings
            self.vca_env.set_attack(0.0);
            self.vca_env.set_decay(1230.0);
            self.vca_env.set_sustain(0.0);

            // RC lowpass smoothers constants
            self.rc1.set_time_constant(3.0); // normal attack

            // envMod scaling using measured hardware curve
            let cutoff_hz = lin_to_exp(cutoff as f64, 314.0, 2394.0);
            self.calculate_env_mod_scaler(cutoff_hz, env_mod as f64);
        }

        if self.topology == CircuitTopology::Accurate {
            self.target_pitch_st = self.target_pitch_st.clamp(24.0, 72.0);
            self.current_pitch_st = self.current_pitch_st.clamp(24.0, 72.0);
        } else {
            self.target_pitch_st = self.target_pitch_st.clamp(0.0, 108.0);
            self.current_pitch_st = self.current_pitch_st.clamp(0.0, 108.0);
        }

        self.vco.poll_lut_update();
        self.vco2.poll_lut_update();
    }

    /// Update global clipper controls at block rate without widening the long
    /// established `update_params` API used by diagnostics and integration tests.
    pub fn update_clipper_params(&mut self, p: VoiceClipperParams) {
        self.clipper.set_params(clipper_params_map(p));
    }

    pub fn update_clipper_params_immediate(&mut self, p: VoiceClipperParams) {
        self.clipper.set_params_immediate(clipper_params_map(p));
    }

    /// Set the tier with no crossfade — for the initial/persisted selection at
    /// construction or `initialize`, where there is no prior audio to fade from.
    /// Runtime changes go through [`set_tier`] (faded).
    pub fn set_tier_immediate(&mut self, tier: Tier) {
        self.tier = tier;
        self.xfade_from = None;
        self.xfade_pos = 1.0;
        self.pending_tier = None;

        if (tier == Tier::Best || tier == Tier::High) && self.vco_dirty {
            let tol_f64 = self.tolerance_f64();
            let ok1 = self.vco.recalculate_luts_async(
                self.age,
                tol_f64,
                self.comp_seed as u32,
                self.recalibration_scale,
            );
            let ok2 = self.vco2.recalculate_luts_async(
                self.age,
                tol_f64,
                self.comp_seed as u32,
                self.recalibration_scale,
            );
            if ok1 && ok2 {
                self.vco_dirty = false;
            }
        }
    }

    /// Clear all runtime state to silence, **in place** — the host reset path.
    ///
    /// nih-plug's `reset()` may fire from the audio thread and must not allocate
    /// (`plugin.rs` `reset` contract). Reconstructing the Voice (`Voice::new`)
    /// would re-enter the VCO pitch-LUT cache: on a warm cache that is only a
    /// `Mutex` lock, but on a cold cache it runs the secant LUT build (heavy) +
    /// a `Vec::push` allocation — both illegal here. So the LUT-bearing VCO and
    /// the nodal ladders are reset *in place* (dynamic state cleared; calibration
    /// and the cached LUT preserved), and the remaining fixed-array components
    /// are cheap, alloc-free `new()`s. The cache is therefore touched only from
    /// `initialize()` (non-RT). Leaves a clean steady Draft; the caller re-applies
    /// the persisted tier via [`set_tier_immediate`].
    pub fn reset(&mut self) {
        self.osc = BlepOsc::new();
        self.osc2 = BlepOsc::new();
        self.sub_osc = BlepOsc::new();
        self.sub_osc.set_wave(OscWave::Square);

        self.osc_low = BlepOsc::new();
        self.osc2_low = BlepOsc::new();
        self.sub_osc_low = BlepOsc::new();
        self.sub_osc_low.set_wave(OscWave::Square);

        self.vco.reset(); // preserves cv_lut — no cache lock, no rebuild, no alloc
        self.vco2.reset();
        self.sub_osc_circuit = BlepOsc::new();
        self.sub_osc_circuit.set_wave(OscWave::Square);
        self.filter.reset();
        // Nodal ladders: in-place reset preserves cutoff / reso / brightness.
        self.ladder_high.reset();
        self.ladder_best.reset();
        self.ladder_low.reset();
        self.os4_best = Os4x::new();
        self.os2_high = Os2x::new();
        self.os4_low = Os4x::new();
        self.os4_draft = Os4x::new();
        self.meg.reset();
        self.veg.reset();
        self.meg_decay.reset();
        self.rc1.reset();
        self.sweep_c13.reset();
        self.vca_env.reset();
        self.vca_accent_rc.reset();
        self.amp_declicker.reset();
        self.pitch_slew.reset();
        self.pitch_slew.set_sample_rate(self.sr as f64);
        self.pitch_slew.set_time_constant(22.0);
        self.pitch_slew.set_state(60.0);
        self.accent_gain = 0.0;
        self.normal_decay_ms = 200.0;
        self.accent_decay_ms = 200.0;
        self.env_scaler = 1.0;
        self.env_offset = 0.0;
        self.drift_pitch = DriftGenerator::new(1337);
        self.drift_cutoff = DriftGenerator::new(7331);
        self.drift_reso = DriftGenerator::new(2468);
        self.drift_pitch_osc2 = DriftGenerator::new(3579);
        self.accent_acc = 0.0;
        self.tier = Tier::Draft;
        self.topology = CircuitTopology::Accurate;
        self.advanced_env = false;
        self.slide_active = false;
        self.current_pitch_st = 60.0;
        self.target_pitch_st = 60.0;
        self.glide_knob = 0.5;
        self.reso_comp = 0.0;
        self.classic_slide_tau_ms = calculate_classic_slide_tau_ms(self.topology, self.glide_knob);
        self.xfade_from = None;
        self.xfade_pos = 1.0;
        self.pending_tier = None;
        self.comp_draft.reset();
        self.comp_low.reset();
        self.comp_high.reset();
        self.comp_best.reset();
        self.clipper.reset();
        self.comp_seed = -1;
    }

    /// Select the fidelity tier. Draft routes to the TPT path; Low routes to the
    /// nodal diode-ladder inside the 2× oversampler (polyBLEP saw, F4.1); High routes
    /// to the circuit VCO → nodal ladder at 2×; Best routes to the same at 4× (F3.5).
    ///
    /// A change starts a short equal-power crossfade (F5.1) from the previous
    /// tier so the path switch is click-free — the tiers have different timbre
    /// and the incoming path starts cold, so a hard switch would pop. No
    /// allocation, RT-safe (field writes and fixed-size in-place state clears).
    pub fn set_tier(&mut self, tier: Tier) {
        if (tier == Tier::Best || tier == Tier::High) && self.vco_dirty {
            let tol_f64 = self.tolerance_f64();
            let ok1 = self.vco.recalculate_luts_async(
                self.age,
                tol_f64,
                self.comp_seed as u32,
                self.recalibration_scale,
            );
            let ok2 = self.vco2.recalculate_luts_async(
                self.age,
                tol_f64,
                self.comp_seed as u32,
                self.recalibration_scale,
            );
            if ok1 && ok2 {
                self.vco_dirty = false;
            }
        }

        if self.xfade_from.is_some() {
            // If a fade is already in flight, queue the new target instead of
            // clobbering the outgoing path (which would hard-cut a still-weighted
            // tier → click on a fast multi-tier sweep). The queued tier starts its
            // fade when the current one completes (see `tick`).
            self.pending_tier = if tier == self.tier { None } else { Some(tier) };
            return;
        }
        if tier == self.tier {
            return;
        }
        self.start_fade(tier);
    }

    /// Starts an equal-power crossfade from the current tier to `to`, with the
    /// incoming path cold-started.
    fn start_fade(&mut self, to: Tier) {
        self.xfade_from = Some(self.tier);
        self.xfade_pos = 0.0;
        self.tier = to;
        self.reset_tier_path(to);
    }

    /// Clears one tier's filter, oversampler and latency-compensation state,
    /// and reseeds its ladder's thermal noise. Without this a tier resumes the
    /// state it held when it last ran, possibly on another note, and a
    /// resonant nodal ladder takes about a second to forget it. Oscillators are
    /// not touched. Alloc-free (in-place resets and fixed-array `new()`s).
    fn reset_tier_path(&mut self, tier: Tier) {
        match tier {
            Tier::Draft => {
                self.filter.reset();
                self.os4_draft = Os4x::new();
                self.comp_draft.reset();
            }
            Tier::Low => {
                self.ladder_low.reset();
                self.ladder_low.set_instance_id(LOW_LADDER_ID);
                self.os4_low = Os4x::new();
                self.comp_low.reset();
            }
            Tier::High => {
                self.ladder_high.reset();
                self.ladder_high.set_instance_id(CIRCUIT_LADDER_ID);
                self.os2_high = Os2x::new();
                self.comp_high.reset();
            }
            Tier::Best => {
                self.ladder_best.reset();
                self.ladder_best.set_instance_id(CIRCUIT_LADDER_ID);
                self.os4_best = Os4x::new();
                self.comp_best.reset();
            }
        }
    }

    /// Reported host latency is the tier alignment plus the constant 48-sample
    /// clipper alignment. It is therefore 96 samples for every tier and clipper
    /// state, including bypass and OS crossfades.
    pub fn latency_samples(&self) -> usize {
        self.max_latency + self.clipper.latency_samples()
    }

    pub fn note_on(&mut self, note: u8) {
        self.current_note = note;
        self.base_hz = midi_to_freq(note);
        let target = note as f32;
        self.target_pitch_st = target;
        if !self.slide_active {
            self.current_pitch_st = target;
            self.pitch_slew.set_state(target as f64);
        }
        self.gate = true;
        self.meg.trigger();
        self.veg.trigger();
        self.meg_decay.trigger();
        self.vca_env.note_on();
    }

    pub fn note_off(&mut self) {
        self.gate = false;
        self.meg.release();
        self.veg.release();
        self.vca_env.note_off();
    }

    pub fn set_gate(&mut self, gate: bool, trigger: bool, accent: bool, slide: bool) {
        self.gate = gate;
        self.accent_active = accent;
        self.slide_active = slide;

        // --- Classic RC Envelopes: update accent state even if not triggering (G1, G14) ---
        if accent {
            self.accent_gain = self.accent_knob_val;
            self.meg_decay.set_decay(self.accent_decay_ms);
            self.vca_env.set_release(50.0);
        } else {
            self.accent_gain = 0.0;
            self.meg_decay.set_decay(self.normal_decay_ms);
            self.vca_env.set_release(0.5);
        }

        if trigger {
            self.meg.trigger();
            self.veg.trigger();
            if accent {
                self.accent_acc = (self.accent_acc + 1.0).min(1.8);
            }
            self.meg_decay.trigger();
            self.vca_env.note_on();
        }

        if !gate {
            self.meg.release();
            self.veg.release();
            self.vca_env.note_off();
        }
    }

    pub fn set_freq(&mut self, hz: f32) {
        self.base_hz = hz;
        let mut target = 12.0 * (hz / 440.0).log2() + 69.0;
        if self.topology == CircuitTopology::Accurate {
            target = target.clamp(24.0, 72.0);
        } else {
            target = target.clamp(0.0, 108.0);
        }
        self.target_pitch_st = target;
        if !self.slide_active {
            self.current_pitch_st = target;
            self.pitch_slew.set_state(target as f64);
        }
    }

    pub fn on_seq_event(&mut self, ev: SeqEvent, accent_amount: f32, decay_knob_ms: f32) {
        match ev {
            SeqEvent::Trigger {
                key,
                octave,
                accent,
            } => {
                let target = 60.0 + key as f32 + 12.0 * octave as f32;
                let target = if self.topology == CircuitTopology::Accurate {
                    target.clamp(24.0, 72.0)
                } else {
                    target.clamp(0.0, 108.0)
                };
                self.target_pitch_st = target;
                self.current_pitch_st = target;
                self.pitch_slew.set_state(target as f64);
                self.slide_active = false;

                self.gate = true;
                self.accent_active = accent;
                if accent {
                    self.accent_gain = if accent_amount > 0.0 {
                        accent_amount
                    } else {
                        self.accent_knob_val
                    };
                    self.meg_decay.set_decay(self.accent_decay_ms);
                    self.vca_env.set_release(50.0);
                    self.accent_acc += (1.0 - self.accent_acc) * 0.8;
                } else {
                    self.accent_gain = 0.0;
                    self.meg_decay.set_decay(if decay_knob_ms > 0.0 {
                        decay_knob_ms as f64
                    } else {
                        self.normal_decay_ms
                    });
                    self.vca_env.set_release(0.5);
                }

                self.meg.trigger();
                self.veg.trigger();
                self.meg_decay.trigger();
                self.vca_env.note_on();
            }
            SeqEvent::SlideTo {
                key,
                octave,
                accent,
            } => {
                let target = 60.0 + key as f32 + 12.0 * octave as f32;
                let target = if self.topology == CircuitTopology::Accurate {
                    target.clamp(24.0, 72.0)
                } else {
                    target.clamp(0.0, 108.0)
                };
                self.target_pitch_st = target;
                self.slide_active = true;
                self.accent_active = accent;

                if accent {
                    self.accent_gain = if accent_amount > 0.0 {
                        accent_amount
                    } else {
                        self.accent_knob_val
                    };
                    self.meg_decay.set_decay(self.accent_decay_ms);
                    self.vca_env.set_release(50.0);
                    self.accent_acc += (1.0 - self.accent_acc) * 0.8;
                } else {
                    self.accent_gain = 0.0;
                    self.meg_decay.set_decay(if decay_knob_ms > 0.0 {
                        decay_knob_ms as f64
                    } else {
                        self.normal_decay_ms
                    });
                    self.vca_env.set_release(0.5);
                }
            }
            SeqEvent::GateOff => {
                self.set_gate(false, false, false, false);
            }
        }
    }

    /// Renders a single sample for a specific tier, advancing its state.
    ///
    /// `circuit_mix` caches the circuit oscillator mix for this output sample.
    /// High and Best share the circuit VCOs, so during a High <-> Best fade the
    /// second circuit path reuses the mix instead of ticking the VCOs again.
    fn render_tier(&mut self, tier: Tier, circuit_mix: &mut Option<f32>) -> f32 {
        match tier {
            // Draft: polyBLEP saw → TPT ladder at 4× oversample (cheap, ships).
            // DRAFT_LEVEL_TRIM matches its loudness to Best.
            Tier::Draft => {
                let osc1 = self.osc.tick();
                let sub = self.sub_osc.tick() * self.sub_vol;
                let osc2 = if self.osc2_vol > 0.0 {
                    self.osc2.tick()
                } else {
                    0.0
                };
                let mix = (osc1 + sub) * (1.0 - self.osc2_vol) + osc2 * self.osc2_vol;

                let hi = self.os4_draft.up(mix * self.drive);
                let o = [
                    self.filter.tick(hi[0]),
                    self.filter.tick(hi[1]),
                    self.filter.tick(hi[2]),
                    self.filter.tick(hi[3]),
                ];
                self.comp_draft.process(
                    self.os4_draft.down(o)
                        * (level_trim(&DRAFT_LEVEL_TRIM, self.final_reso) / self.drive),
                )
            }
            // Low (F4.1): polyBLEP saw → nodal diode-ladder at 4× oversample.
            // Skips the costly nodal VCO Newton but keeps the circuit-filter
            // character — sits between Draft and Best at a fraction of Best cost.
            Tier::Low => {
                let osc1 = self.osc_low.tick();
                let sub = self.sub_osc_low.tick() * self.sub_vol;
                let osc2 = if self.osc2_vol > 0.0 {
                    self.osc2_low.tick()
                } else {
                    0.0
                };
                let mix = (osc1 + sub) * (1.0 - self.osc2_vol) + osc2 * self.osc2_vol;

                let hi = self.os4_low.up(mix * self.drive);
                let o = [
                    self.ladder_low.tick(hi[0]),
                    self.ladder_low.tick(hi[1]),
                    self.ladder_low.tick(hi[2]),
                    self.ladder_low.tick(hi[3]),
                ];
                // LOW_LEVEL_TRIM equalises Low's loudness to Best (tier-switch
                // is then timbre-only). Pre-comp-delay so the crossfade blends
                // level-matched paths.
                self.comp_low.process(
                    self.os4_low.down(o)
                        * (level_trim(&LOW_LEVEL_TRIM, self.final_reso) / self.drive),
                )
            }
            // High: circuit VCO saw → nodal diode-ladder at 2× oversample.
            Tier::High => {
                let mix = self.circuit_mix(circuit_mix);
                let hi = self.os2_high.up(mix * self.drive);
                let o = [self.ladder_high.tick(hi[0]), self.ladder_high.tick(hi[1])];
                self.comp_high.process(self.os2_high.down(o) / self.drive)
            }
            // Best: circuit VCO saw → nodal diode-ladder at 4× oversample. The
            // 4× OS + decimation FIR band-limits the ladder nonlinearity.
            Tier::Best => {
                let mix = self.circuit_mix(circuit_mix);
                let hi = self.os4_best.up(mix * self.drive);
                let o = [
                    self.ladder_best.tick(hi[0]),
                    self.ladder_best.tick(hi[1]),
                    self.ladder_best.tick(hi[2]),
                    self.ladder_best.tick(hi[3]),
                ];
                self.comp_best.process(self.os4_best.down(o) / self.drive)
            }
        }
    }

    /// Circuit oscillator mix (VCO, sub, VCO 2) for the current output sample.
    /// Ticks the oscillators only if `cached` is empty, then caches the result.
    ///
    /// The nodal VCO runs at base rate; its sharp JFET reset edge is corrected by
    /// a 2-tap first-order BLEP computed inside `Vco303::tick()`. `blep_now` is
    /// already calibrated to SPICE OR_SWING × VCO_GAIN (= 2.92 V); tap 1 is
    /// auto-injected on the following tick.
    ///
    /// RESIDUAL LIMITATION: higher-order alias from the smooth ramp (not the
    /// reset edge) is negligible relative to the reset. A full 4× VCO (a future
    /// full-rate-VCO tier) gives complete alias freedom at ~4× the CPU cost.
    #[inline]
    fn circuit_mix(&mut self, cached: &mut Option<f32>) -> f32 {
        if let Some(mix) = *cached {
            return mix;
        }
        let raw = self.vco.tick() * VCO_GAIN;
        let blep = match self.vco.wave {
            OscWave::Saw => self.vco.blep_now,
            OscWave::Sweep => self.vco.blep_now * (1.0 - self.vco.sweep_pos as f32),
            OscWave::Square => 0.0,
        };
        let osc1 = raw + blep;
        let sub = self.sub_osc_circuit.tick() * self.sub_vol;
        let osc2 = if self.osc2_vol > 0.0 {
            let raw2 = self.vco2.tick() * VCO_GAIN;
            let blep2 = match self.vco2.wave {
                OscWave::Saw => self.vco2.blep_now,
                OscWave::Sweep => self.vco2.blep_now * (1.0 - self.vco2.sweep_pos as f32),
                OscWave::Square => 0.0,
            };
            raw2 + blep2
        } else {
            0.0
        };
        let mix = (osc1 + sub) * (1.0 - self.osc2_vol) + osc2 * self.osc2_vol;
        *cached = Some(mix);
        mix
    }

    /// Advances the voice by one sample, computing modulations, rendering the active
    /// tier (or crossfading), and applying the VCA envelope and master volume.
    pub fn tick(&mut self) -> f32 {
        self.vco.poll_lut_update();
        self.vco2.poll_lut_update();

        // Update operating temperature
        let ambient_temp = 20.0 + 20.0 * (self.thermal_noise as f32 / 100.0);
        let target_temp = if self.gate {
            ambient_temp + 3.0
        } else {
            ambient_temp
        };
        let tau = if self.gate { 15.0 } else { 30.0 };
        let alpha = 1.0 - (-1.0 / (tau * self.sr)).exp();
        self.temperature += alpha * (target_temp - self.temperature);

        let noise_step = (((self.temperature - 20.0) / 25.0 * 21.0).round() as i32).clamp(0, 21);
        if noise_step != self.thermal_noise_state {
            self.thermal_noise_state = noise_step;
            self.vco.set_thermal(noise_step);
            self.vco2.set_thermal(noise_step);
        }

        // Tick meg_decay if classic mode
        let meg_out = if self.advanced_env {
            0.0
        } else {
            self.meg_decay.tick()
        };

        // Pitch glide slew
        if self.advanced_env {
            let glide_time_ms = 10.0 + self.glide_knob * 400.0;
            self.pitch_slew.set_time_constant(glide_time_ms as f64);
            self.current_pitch_st = self.pitch_slew.tick(self.target_pitch_st as f64) as f32;
        } else {
            // Classic mode: CV-domain exponential lag (hardware C35 0.22uF + R93 100k gives tau = 22 ms -> ~60 ms slide settling time)
            if self.slide_active {
                self.pitch_slew.set_time_constant(self.classic_slide_tau_ms);
                self.current_pitch_st = self.pitch_slew.tick(self.target_pitch_st as f64) as f32;
            } else {
                self.current_pitch_st = self.target_pitch_st;
                self.pitch_slew.set_state(self.target_pitch_st as f64);
            }
        }

        // Pitch drift (LFO pink noise walk + systematic temperature drift)
        let drift_pitch_val = self.drift_pitch.tick(0.15, self.sr);
        let pitch_drift_st = drift_pitch_val * 0.12 * self.drift;
        let pitch_thermal_st = -0.10 * (self.temperature - 20.0) / 25.0;

        let osc2_drift_val = self.drift_pitch_osc2.tick(0.15, self.sr);
        let osc2_drift_st = osc2_drift_val * 0.12 * self.drift;

        // Tuning offset
        let tuning_st = self.tuning_offset / 100.0; // cents to semitones
        let dp = 0.0f32;

        let vco1_f = 440.0
            * 2.0f32.powf(
                (self.current_pitch_st - 69.0 + self.osc1_pitch_st + tuning_st + pitch_drift_st)
                    / 12.0,
            );
        let osc1_f = 440.0
            * 2.0f32.powf(
                (self.current_pitch_st + dp - 69.0
                    + self.osc1_pitch_st
                    + tuning_st
                    + pitch_drift_st
                    + pitch_thermal_st)
                    / 12.0,
            );
        self.osc.set_freq(osc1_f, self.sr);
        self.osc_low.set_freq(osc1_f, self.sr);
        self.vco.set_freq(vco1_f);

        let sub_f = osc1_f * 0.5;
        self.sub_osc.set_freq(sub_f, self.sr);
        self.sub_osc_low.set_freq(sub_f, self.sr);
        self.sub_osc_circuit.set_freq(sub_f, self.sr);

        if self.osc2_vol > 0.0 {
            let vco2_f = 440.0
                * 2.0f32.powf(
                    (self.current_pitch_st - 69.0 + self.osc2_pitch_st + tuning_st + osc2_drift_st)
                        / 12.0,
                );
            let osc2_f = 440.0
                * 2.0f32.powf(
                    (self.current_pitch_st + dp - 69.0
                        + self.osc2_pitch_st
                        + tuning_st
                        + osc2_drift_st
                        + pitch_thermal_st)
                        / 12.0,
                );
            self.osc2.set_freq(osc2_f, self.sr);
            self.osc2_low.set_freq(osc2_f, self.sr);
            self.vco2.set_freq(vco2_f);
        }

        // --- 2. Filter Cutoff Modulation ---
        let drift_cut_val = self.drift_cutoff.tick(0.08, self.sr);
        let cutoff_drift_factor = 1.0 + drift_cut_val * 0.08 * self.drift;
        let cutoff_thermal_factor = 1.0 - 0.05 * (self.temperature - 20.0) / 25.0;

        let reso_drift_val = self.drift_reso.tick(0.05, self.sr);
        let reso_drift_factor = 1.0 + reso_drift_val * 0.03 * self.drift;

        let acc_decay_coeff = (-1.0 / (0.2 * self.sr)).exp();
        self.accent_acc *= acc_decay_coeff;

        let final_cutoff_hz = if self.advanced_env {
            // Modulate filter decay using same scaled knob mapping
            let decay_normal_ms = Self::decay_map(self.decay_knob);
            let effective_decay_ms = decay_normal_ms
                + (self.accent_knob_val * self.accent_acc).min(1.0) * (200.0 - decay_normal_ms);
            self.meg.set_decay(effective_decay_ms);

            // MEG envelope
            let meg_val = self.meg.tick(self.gate);

            // VCF Cutoff Base in Hz
            let base_cutoff_hz = 200.0 * 20.0f32.powf(self.cutoff_knob); // 200 Hz to 4000 Hz
                                                                         // Modulated Cutoff
            let mod_cutoff_hz = base_cutoff_hz
                + meg_val * self.env_mod * 4000.0
                + self.accent_knob_val * self.accent_acc * 3000.0;

            (mod_cutoff_hz * cutoff_drift_factor * cutoff_thermal_factor)
                .clamp(20.0, self.sr * 0.45)
        } else {
            // Classic mode: dual RC smoothers
            let rc1_out = self.rc1.tick(meg_out);

            // Sweep injection is the output of the C13 memory cap
            let sweep_drive = if self.accent_gain > 0.0 {
                meg_out * (self.accent_gain as f64)
            } else {
                0.0
            };
            let sweep_out = self.sweep_c13.tick(sweep_drive, self.reso_knob as f64);

            // Base cutoff in Hz (314..2394 range)
            let base_cutoff_hz = lin_to_exp(self.cutoff_knob as f64, 314.0, 2394.0);

            // Modulated Cutoff in Hz (exponential/octave domain)
            let env_mod_term = self.env_scaler * (rc1_out - self.env_offset);

            // Adding sweep_out (memory trace) to the octave sum (G4, G5, G6)
            let mod_cutoff_hz = base_cutoff_hz * 2.0_f64.powf(env_mod_term + sweep_out);

            let cutoff_hz = mod_cutoff_hz as f32 * cutoff_drift_factor * cutoff_thermal_factor;
            cutoff_hz.clamp(20.0, self.sr * 0.45)
        };

        // VCF Resonance
        let final_reso = (self.reso_knob * reso_drift_factor).clamp(0.0, 1.0);
        self.final_reso = final_reso;

        // Apply to filters
        let (draft_fc, draft_reso) = Self::map_draft_vcf(final_cutoff_hz, final_reso);
        let draft_fc =
            (draft_fc as f64 * self.draft_cutoff_mult).clamp(20.0, (self.sr * 0.45) as f64);
        let draft_reso = (draft_reso * self.draft_reso_mult as f32).clamp(0.0, 1.0);

        let (low_fc, low_reso) = Self::map_low_vcf(final_cutoff_hz, final_reso);
        let low_fc = (low_fc as f64).clamp(20.0, (self.sr * 0.45) as f64);
        self.filter.set_cutoff(draft_fc as f32);
        self.filter.set_reso(draft_reso as f32);
        self.ladder_low.set_cutoff(low_fc as f32);
        self.ladder_low.set_reso(low_reso as f32);

        for ladder in [&mut self.ladder_high, &mut self.ladder_best] {
            ladder.set_cutoff(final_cutoff_hz as f32);
            ladder.set_reso(final_reso);
        }

        // --- 3. Render Tier audio ---
        let mut circuit_mix = None;
        let f = match self.xfade_from {
            // Steady state: render the active tier only.
            None => self.render_tier(self.tier, &mut circuit_mix),
            // Crossfade: render both paths and blend equal-power. The outgoing
            // path keeps running until the fade completes, then is dropped.
            Some(from) => {
                let a = self.render_tier(from, &mut circuit_mix);
                let b = self.render_tier(self.tier, &mut circuit_mix);
                let g = self.xfade_pos.min(1.0);
                // Equal-power (constant-energy) weights: the tiers are largely
                // uncorrelated in timbre, so equal-power avoids the mid-fade dip
                // a linear blend would give.
                let theta = g * std::f32::consts::FRAC_PI_2;
                let out = a * theta.cos() + b * theta.sin();
                self.xfade_pos += self.xfade_step;
                if self.xfade_pos >= 1.0 {
                    // Fade complete: launch any tier queued mid-fade.
                    self.xfade_from = None;
                    if let Some(next) = self.pending_tier.take() {
                        if next != self.tier {
                            self.start_fade(next);
                        }
                    }
                }
                out
            }
        };

        // --- 4. VCA Envelope Modulation ---
        let vca_gain = match self.vca_env_mode {
            VcaEnvMode::Gate => {
                let gate_level = if self.gate { 1.0f32 } else { 0.0f32 };
                if self.advanced_env {
                    gate_level * (1.0 + self.accent_knob_val * self.accent_acc * 0.5)
                } else {
                    gate_level
                }
            }
            VcaEnvMode::Envelope => {
                if self.advanced_env {
                    let veg_val = self.veg.tick(self.gate);
                    veg_val * (1.0 + self.accent_knob_val * self.accent_acc * 0.5)
                } else {
                    let amp_out = self.vca_env.tick();

                    // Accent injection is filtered by the R119/C36 RC network (tau=1.55ms)
                    let accent_drive = if self.vca_env.is_note_on() {
                        0.45 * meg_out + self.accent_gain as f64 * 4.0 * meg_out
                    } else {
                        0.0
                    };
                    let accent_injection = self.vca_accent_rc.tick(accent_drive);

                    let composite_amp = amp_out + accent_injection;
                    self.amp_declicker.tick(composite_amp) as f32
                }
            }
        };

        // VCF resonance gain compensation: tap into the VCF resonance CV rail and feed into VCA level rail.
        // Inverse ladder compensation factor: k_eff = 4.0 * final_reso, G_comp = 1.0 + self.reso_comp * k_eff.
        let reso_gain_comp = 1.0 + self.reso_comp * (4.0 * final_reso);
        let vca_gain = vca_gain * reso_gain_comp;

        // Global output clipper is after the VCA/resonance chain and before the
        // master fader. Thus volume changes never alter drive or sidechain state.
        let clipped = self.clipper.tick(f * vca_gain);

        // Master Output volume db
        let master_gain = 10.0f32.powf(self.master_volume / 20.0);

        clipped * master_gain
    }

    pub fn set_cutoff(&mut self, hz: f32) {
        let hz = hz.clamp(20.0, self.sr * 0.45);
        self.cutoff_knob = (hz / 200.0).ln() / 20.0f32.ln();
        self.cutoff_knob = self.cutoff_knob.clamp(0.0, 1.0);
    }

    pub fn set_reso(&mut self, r: f32) {
        self.reso_knob = r.clamp(0.0, 1.0);
    }

    pub fn set_wave(&mut self, wave: OscWave) {
        self.osc.set_wave(wave);
        self.osc2.set_wave(wave);
        self.osc_low.set_wave(wave);
        self.osc2_low.set_wave(wave);
        self.vco.set_wave(wave);
        self.vco2.set_wave(wave);
    }

    pub fn set_circuit_brightness(&mut self, cal: f64) {
        self.ladder_high.set_cal(cal);
        self.ladder_best.set_cal(cal);
    }

    pub fn set_low_brightness(&mut self, cal: f64) {
        self.ladder_low.set_cal(cal);
    }

    pub fn set_env_mod(&mut self, env_mod: f32) {
        self.env_mod = env_mod.clamp(0.0, 1.0);
    }

    pub fn set_circuit_reso_gain(&mut self, g: f64) {
        self.ladder_high.set_reso_gain(g);
        self.ladder_best.set_reso_gain(g);
    }

    /// Diagnostic: effective brightness of the Low, High and Best nodal ladders.
    #[doc(hidden)]
    pub fn dbg_ladder_cals(&self) -> [f64; 3] {
        [self.ladder_low.cal(), self.ladder_high.cal(), self.ladder_best.cal()]
    }

    // The Draft cubic polynomials were fitted by calibrate_vcf.py on the grid
    // [200 Hz, 4000 Hz] x [0.0, 0.9]. Above ~6 kHz the unconstrained cubic
    // flattens and drives reso to 1.0 (self-oscillation). Clamping the cutoff input
    // to [log2(200), log2(4000)] for both polys and adding the clamped octaves
    // back onto the mapped cutoff keeps the cutoff monotone and prevents resonance
    // runaway above 4 kHz.
    const DRAFT_FIT_LOG2_FC_MIN: f64 = 7.643856189774724; // (200.0_f64).log2()
    const DRAFT_FIT_LOG2_FC_MAX: f64 = 11.965784284662087; // (4000.0_f64).log2()

    /// Evaluates the Draft VCF control map (cutoff and resonance) given the
    /// target (Best) cutoff in Hz and resonance in [0, 1].
    pub fn map_draft_vcf(cutoff_hz: f32, reso: f32) -> (f32, f32) {
        let x = (cutoff_hz as f64).log2();
        let y = reso as f64;
        let x_clamped = x.clamp(Self::DRAFT_FIT_LOG2_FC_MIN, Self::DRAFT_FIT_LOG2_FC_MAX);
        let x_extra = x - x_clamped;
        let draft_fc = 2.0f64.powf(eval_poly3(x_clamped, y, &DRAFT_FC_POLY) + x_extra);
        let draft_reso = eval_poly3(x_clamped, y, &DRAFT_RESO_POLY).clamp(0.0, 1.0);
        (draft_fc as f32, draft_reso as f32)
    }

    /// Evaluates the Low VCF control map (cutoff and resonance) given the
    /// target (Best) cutoff in Hz and resonance in [0, 1].
    pub fn map_low_vcf(cutoff_hz: f32, reso: f32) -> (f32, f32) {
        let x = (cutoff_hz as f64).log2();
        let y = reso as f64;
        let low_fc = 2.0f64.powf(eval_poly3(x, y, &LOW_FC_POLY));
        let low_reso = eval_poly3(x, y, &LOW_RESO_POLY).clamp(0.0, 1.0);
        (low_fc as f32, low_reso as f32)
    }

    pub fn set_topology(&mut self, topology: CircuitTopology) {
        if self.topology != topology {
            self.topology = topology;
            self.classic_slide_tau_ms = calculate_classic_slide_tau_ms(topology, self.glide_knob);
            let is_fun = topology == CircuitTopology::Fun;
            self.vco.set_fun(is_fun);
            self.vco2.set_fun(is_fun);
            let tol_f64 = self.tolerance_f64();
            self.vco.recalculate_luts_async(
                self.age,
                tol_f64,
                self.comp_seed as u32,
                self.recalibration_scale,
            );
            self.vco2.recalculate_luts_async(
                self.age,
                tol_f64,
                self.comp_seed as u32,
                self.recalibration_scale,
            );
            self.vco.set_thermal(self.thermal_noise_state);
            self.vco2.set_thermal(self.thermal_noise_state);
        }
    }
}

#[inline]
fn calculate_classic_slide_tau_ms(topology: CircuitTopology, glide_knob: f32) -> f64 {
    if topology == CircuitTopology::Fun {
        let k = (glide_knob as f64).clamp(0.0, 1.0);
        2.0 * 55.0f64.powf(k.powf(1.5))
    } else {
        22.0
    }
}

#[inline]
fn lin_to_exp(x: f64, min: f64, max: f64) -> f64 {
    min * (max / min).powf(x)
}

#[inline]
fn exp_to_lin(y: f64, min: f64, max: f64) -> f64 {
    (y / min).ln() / (max / min).ln()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression pin for 21b7307: each nodal ladder must be built at the
    /// oversampled clock its tier drives it at, because the nodal solver tunes
    /// its capacitors from the constructor rate.
    #[test]
    fn voice_new_builds_ladders_at_oversampled_rate() {
        for sr in [44_100.0f32, 48_000.0, 96_000.0] {
            let v = Voice::new(sr);
            for (name, ladder, factor) in [
                ("low", &v.ladder_low, LOW_OS_FACTOR),
                ("high", &v.ladder_high, HIGH_OS_FACTOR),
                ("best", &v.ladder_best, BEST_OS_FACTOR),
            ] {
                let expected = sr as f64 * factor as f64;
                assert!(
                    (ladder.sample_rate() - expected).abs() < 1e-6,
                    "sr {sr}: {name} ladder built at {} Hz, expected {expected} Hz",
                    ladder.sample_rate(),
                );
            }
        }
    }
}
