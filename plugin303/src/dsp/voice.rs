//! Monophonic voice with dual oscillators, sub-oscillator, drift/age modulation.

use super::env::AdsrEnv;
use super::filter::{Filter303, Tier};
use super::ladder::TptLadder;
use super::ladder_wdf::LadderVcf;
use super::osc::BlepOsc;
use super::oversample::{Os2x, Os4x};
use super::vco::Vco303;
use super::drift::DriftGenerator;
use crate::params::{OscWave, VcfTaps, VcaEnvMode, CircuitTopology, CompTolerance};

/// Equal-tempered MIDI note -> frequency (A4 = note 69 = 440 Hz).
pub fn midi_to_freq(note: u8) -> f32 {
    440.0 * 2.0f32.powf((note as f32 - 69.0) / 12.0)
}

pub const ULTRA_OS_FACTOR: f32 = 4.0;
pub const HIGH_OS_FACTOR: f32 = 2.0;

/// VCO output gain. The Ultra MNA solver outputs a raw saw wave with ~1.12 V p-p swing.
/// SPICE gold has a ~1.46 V p-p swing which after its own gain stages gives ~2.92 V p-p.
/// `VCO_GAIN = 2.92 / 1.120 = 2.607` matches the SPICE drive level into the VCF.
pub const VCO_GAIN: f32 = 2.607;


/// Per-tier VCF brightness (cutoff→bias scale on the shared `LadderVcf`). With
/// the active Q9 resonance (F6.3) the spectral-LSD optimum vs the whole-voice
/// gold sits *below* 1.0 — the resonance peak otherwise over-brightens. Tuned
/// per tier (different OS rate ⇒ different optimum): Ultra (4×) at 0.70 minimises
/// LSD to 3.87 dB; High (2×) at 0.50 keeps corr ≥ 0.95 while beating its prior
/// LSD. See `brightness_sweep`/`high_brightness_sweep` diagnostics (F6.5).
const ULTRA_BRIGHT: f64 = 0.70;
const HIGH_BRIGHT: f64 = 0.50;
const HIGH_LEVEL_TRIM: f32 = 1.890; // +5.5 dB
const XFADE_SECS: f32 = 0.010;
const COMP_BUF: usize = 64;

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

/// Monophonic voice structure managing the different fidelity paths, modulation sources, and envelopes.
pub struct Voice {
    // --- Low Tier ---
    /// Primary polyBLEP oscillator for Low tier.
    osc: BlepOsc,
    /// Detunable secondary oscillator for Low tier.
    osc2: BlepOsc,
    /// Sub-oscillator for Low tier (always a square wave, one octave below).
    sub_osc: BlepOsc,
    /// Textbook coincident-pole zero-delay-feedback VCF for Low tier.
    filter: TptLadder,

    // --- High Tier ---
    /// Primary polyBLEP oscillator for High tier.
    osc_hi: BlepOsc,
    /// Detunable secondary oscillator for High tier.
    osc2_hi: BlepOsc,
    /// Sub-oscillator for High tier (always a square wave, one octave below).
    sub_osc_hi: BlepOsc,
    /// High-tier nodal diode-ladder (polyBLEP saw source), run inside `os2` at 2x.
    ladder_hi: LadderVcf,
    /// High-tier oversampling filter (2x Kaiser FIR).
    os2: Os2x,

    // --- Ultra Tier ---
    /// Ultra-tier nodal circuit VCO (replaces the polyBLEP saw for Ultra only).
    vco: Vco303,
    /// Secondary Ultra-tier nodal circuit VCO.
    vco2: Vco303,
    /// Ultra-tier nodal diode-ladder, preallocated, run inside `os` at 4x.
    ladder: LadderVcf,
    /// Ultra-tier oversampling filter (4x Kaiser FIR).
    os: Os4x,

    // --- Envelopes & Modulators ---
    /// Filter envelope (MEG), ADSR structure collapsing to classic AD.
    meg: AdsrEnv,
    /// Amplitude envelope (VEG), ADSR structure.
    veg: AdsrEnv,
    /// Pitch random walk drift generator.
    drift_pitch: DriftGenerator,
    /// VCF cutoff random walk drift generator.
    drift_cutoff: DriftGenerator,
    /// Accent decay integrator accumulator.
    accent_acc: f32,

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
    /// Compensation delay line for Low tier to align latency with Ultra.
    comp_low: CompDelay,
    /// Compensation delay line for High tier to align latency with Ultra.
    comp_high: CompDelay,
    /// Compensation delay line for Ultra tier (typically 0 delay).
    comp_ultra: CompDelay,
    /// Common reported latency (base samples) = max OS group delay over all tiers.
    max_latency: usize,
    /// Sample rate.
    sr: f32,

    // --- Parameter States ---
    /// Currently triggered MIDI note.
    current_note: u8,
    /// Base note frequency in Hz.
    base_hz: f32,
    /// Master fine tuning offset (cents).
    tuning_offset: f32,
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
    /// Thermal noise setting in [0, 21].
    thermal_noise: i32,
    /// Voice drift amount in [0.0, 1.0].
    drift: f32,
    /// Dynamic operating temperature in degrees Celsius (20.0 to 45.0)
    temperature: f32,
    /// Cached thermal state to avoid rebuilding LUTs.
    thermal_noise_state: i32,
    /// VCA envelope trigger mode.
    vca_env_mode: VcaEnvMode,
    /// Current gate-on state of the voice.
    pub gate: bool,
    /// Current active step accent modifier.
    accent_active: bool,
    /// Active circuit topology.
    pub topology: CircuitTopology,
}

impl Voice {
    pub fn new(sr: f32) -> Self {
        let lat_low = 0;
        let lat_high = Os2x::new().latency_samples();
        let lat_ultra = Os4x::new().latency_samples();
        let max_latency = lat_ultra.max(lat_high).max(lat_low);
        let mut ladder = LadderVcf::new(sr * ULTRA_OS_FACTOR);
        ladder.set_instance_id(3);
        ladder.set_cal(ULTRA_BRIGHT);
        let mut ladder_hi = LadderVcf::new(sr * HIGH_OS_FACTOR);
        ladder_hi.set_instance_id(4);
        ladder_hi.set_cal(HIGH_BRIGHT);

        // Sub oscillators are always square waves
        let mut sub1 = BlepOsc::new();
        sub1.set_wave(OscWave::Square);
        let mut sub2 = BlepOsc::new();
        sub2.set_wave(OscWave::Square);

        let mut vco = Vco303::new(sr);
        vco.set_instance_id(1);
        vco.recalculate_luts_sync(0, 0.0);
        let mut vco2 = Vco303::new(sr);
        vco2.set_instance_id(2);
        vco2.recalculate_luts_sync(0, 0.0);

        Self {
            osc: BlepOsc::new(),
            osc2: BlepOsc::new(),
            sub_osc: sub1,
            filter: TptLadder::new(sr),

            osc_hi: BlepOsc::new(),
            osc2_hi: BlepOsc::new(),
            sub_osc_hi: sub2,
            ladder_hi,
            os2: Os2x::new(),

            vco,
            vco2,
            ladder,
            os: Os4x::new(),

            meg: AdsrEnv::new(sr),
            veg: AdsrEnv::new(sr),
            drift_pitch: DriftGenerator::new(1337),
            drift_cutoff: DriftGenerator::new(7331),
            accent_acc: 0.0,

            tier: Tier::Low,
            xfade_from: None,
            xfade_pos: 1.0,
            xfade_step: 1.0 / (sr * XFADE_SECS).max(1.0),
            pending_tier: None,
            comp_low: CompDelay::new(max_latency - lat_low),
            comp_high: CompDelay::new(max_latency - lat_high),
            comp_ultra: CompDelay::new(max_latency - lat_ultra),
            max_latency,
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
            env_mod: 0.0,
            decay_knob: 0.5,
            accent_knob_val: 0.0,
            master_volume: 0.0,
            drive: 1.0,
            age: 0,
            tolerance: CompTolerance::Zero,
            comp_seed: 0,
            thermal_noise: 0,
            drift: 0.0,
            temperature: 20.0,
            thermal_noise_state: -1,
            vca_env_mode: VcaEnvMode::Envelope,
            gate: false,
            accent_active: false,
            topology: CircuitTopology::Accurate,
        }
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
    ) {
        let age_changed = self.age != comp_age;
        let tolerance_changed = self.tolerance != tolerance || self.comp_seed != comp_seed;

        self.topology = topology;
        self.tuning_offset = tuning_cents;
        self.age = comp_age;
        self.drift = comp_drift;
        self.tolerance = tolerance;
        self.comp_seed = comp_seed;
        self.thermal_noise = thermal_noise;

        let tol_f64 = match tolerance {
            CompTolerance::Zero => 0.0,
            CompTolerance::PointFive => 0.005,
            CompTolerance::Two => 0.02,
            CompTolerance::Five => 0.05,
            CompTolerance::Ten => 0.10,
        };

        if age_changed || tolerance_changed {
            self.ladder.set_analog_wear(tol_f64, comp_age);
            self.ladder_hi.set_analog_wear(tol_f64, comp_age);
            self.vco.set_analog_wear(tol_f64, comp_age);
            self.vco2.set_analog_wear(tol_f64, comp_age);
            self.vco.recalculate_luts_async(comp_age, tol_f64);
            self.vco2.recalculate_luts_async(comp_age, tol_f64);
        }

        if self.thermal_noise != self.thermal_noise_state {
            self.ladder.set_thermal(thermal_noise);
            self.ladder_hi.set_thermal(thermal_noise);
            self.vco.set_thermal(thermal_noise);
            self.vco2.set_thermal(thermal_noise);
            self.thermal_noise_state = thermal_noise;
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
        ) = match topology {
            CircuitTopology::Accurate => {
                // Coerce Sweep waveform to Saw (unsupported in accurate)
                let wave = match osc1_wave {
                    OscWave::Sweep => OscWave::Saw,
                    other => other,
                };
                (
                    0.0,                    // osc1_pitch_st
                    0.0,                    // osc2_pitch_st
                    0.0,                    // osc2_vol
                    0.0,                    // sub_vol
                    wave,                   // waveform
                    0.5,                    // osc1_pw (fixed to 50% for standard square)
                    0.0,                    // sweep_pos
                    VcfTaps::FourPole,      // vcf_taps
                    0.0,                    // vcf_drive
                    false,                  // advanced_env
                    VcaEnvMode::Envelope,   // vca_env_mode
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

        // Propagate tap/pole count
        let poles_n = match final_poles {
            VcfTaps::TwoPole => 2,
            VcfTaps::ThreePole => 3,
            VcfTaps::FourPole => 4,
        };
        self.filter.set_poles(poles_n);
        self.ladder.set_poles(poles_n);
        self.ladder_hi.set_poles(poles_n);

        // Update Osc Waveforms & PW parameters
        self.osc.set_wave(final_wave);
        self.osc.set_pw(final_pw);
        self.osc.set_sweep(final_sweep);
        
        self.osc2.set_wave(final_wave);
        self.osc2.set_pw(final_pw);
        self.osc2.set_sweep(final_sweep);

        self.osc_hi.set_wave(final_wave);
        self.osc_hi.set_pw(final_pw);
        self.osc_hi.set_sweep(final_sweep);
        
        self.osc2_hi.set_wave(final_wave);
        self.osc2_hi.set_pw(final_pw);
        self.osc2_hi.set_sweep(final_sweep);

        self.vco.set_wave(final_wave);
        self.vco.set_sweep(final_sweep as f64);
        
        self.vco2.set_wave(final_wave);
        self.vco2.set_sweep(final_sweep as f64);

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

            // VCF Decay Knob range: 200 ms to 2000 ms
            let decay_ms = 200.0 + decay_knob * 1800.0;
            self.meg.set_decay(decay_ms);
            self.meg.set_sustain(0.0);
            self.meg.set_release(200.0);

            // VCA Decay maps to the same decay knob: 200 ms to 3000 ms
            let vca_decay_ms = 200.0 + decay_knob * 2800.0;
            self.veg.set_decay(vca_decay_ms);
            self.veg.set_sustain(0.0);
            self.veg.set_release(200.0);
        }
    }

    /// Set the tier with no crossfade — for the initial/persisted selection at
    /// construction or `initialize`, where there is no prior audio to fade from.
    /// Runtime changes go through [`set_tier`] (faded).
    pub fn set_tier_immediate(&mut self, tier: Tier) {
        self.tier = tier;
        self.xfade_from = None;
        self.xfade_pos = 1.0;
        self.pending_tier = None;
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
    /// `initialize()` (non-RT). Leaves a clean steady Low; the caller re-applies
    /// the persisted tier via [`set_tier_immediate`].
    pub fn reset(&mut self) {
        self.osc = BlepOsc::new();
        self.osc2 = BlepOsc::new();
        self.sub_osc = BlepOsc::new();
        self.sub_osc.set_wave(OscWave::Square);

        self.osc_hi = BlepOsc::new();
        self.osc2_hi = BlepOsc::new();
        self.sub_osc_hi = BlepOsc::new();
        self.sub_osc_hi.set_wave(OscWave::Square);

        self.vco.reset(); // preserves cv_lut — no cache lock, no rebuild, no alloc
        self.vco2.reset();
        self.filter.reset();
        self.ladder.reset(); // preserves cutoff / reso / brightness
        self.ladder_hi.reset();
        self.os = Os4x::new();
        self.os2 = Os2x::new();
        self.meg.reset();
        self.veg.reset();
        self.drift_pitch = DriftGenerator::new(1337);
        self.drift_cutoff = DriftGenerator::new(7331);
        self.accent_acc = 0.0;
        self.temperature = 20.0;
        self.tier = Tier::Low;
        self.topology = CircuitTopology::Accurate;
        self.xfade_from = None;
        self.xfade_pos = 1.0;
        self.pending_tier = None;
        self.comp_low.reset();
        self.comp_high.reset();
        self.comp_ultra.reset();
    }

    /// Select the fidelity tier. Low routes to the TPT path; High routes to the
    /// nodal diode-ladder inside the 2× oversampler (polyBLEP saw, F4.1); Ultra
    /// routes to the circuit VCO → nodal ladder at 4× (F3.5).
    ///
    /// A change starts a short equal-power crossfade (F5.1) from the previous
    /// tier so the path switch is click-free — the tiers have different timbre
    /// and the incoming path starts cold, so a hard switch would pop. No
    /// allocation, RT-safe (plain field writes).
    pub fn set_tier(&mut self, tier: Tier) {
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
        self.xfade_from = Some(self.tier);
        self.xfade_pos = 0.0;
        self.tier = tier;
    }

    /// Reported host latency (base samples). Constant across tiers: every tier is
    /// padded to the max OS group delay (Ultra's 4×), so the PDC value never
    /// changes at runtime — no host graph re-init on a tier switch, and the
    /// crossfade blends paths that share the same total latency.
    pub fn latency_samples(&self) -> usize {
        self.max_latency
    }

    pub fn note_on(&mut self, note: u8) {
        self.current_note = note;
        self.base_hz = midi_to_freq(note);
        self.meg.trigger();
        self.veg.trigger();
    }

    pub fn set_gate(&mut self, gate: bool, trigger: bool, accent: bool) {
        self.gate = gate;
        self.accent_active = accent;
        if trigger {
            self.meg.trigger();
            self.veg.trigger();
        }
        if !gate {
            self.meg.release();
            self.veg.release();
        }
    }

    pub fn set_freq(&mut self, hz: f32) {
        self.base_hz = hz;
    }

    /// Renders a single sample for a specific tier, advancing its state.
    fn render_tier(&mut self, tier: Tier) -> f32 {
        match tier {
            // Low: polyBLEP saw → TPT ladder (cheap, ships). Padded to the common
            // latency so the host PDC is constant across tiers.
            Tier::Low => {
                let osc1 = self.osc.tick();
                let sub = self.sub_osc.tick() * self.sub_vol;
                let osc2 = if self.osc2_vol > 0.0 { self.osc2.tick() } else { 0.0 };
                let mix = (osc1 + sub) * (1.0 - self.osc2_vol) + osc2 * self.osc2_vol;
                
                let y = self.filter.tick(mix * self.drive) / self.drive;
                self.comp_low.process(y)
            }
            // High (F4.1): polyBLEP saw → nodal diode-ladder at 2× oversample.
            // Skips the costly nodal VCO Newton but keeps the circuit-filter
            // character — sits between Low and Ultra at a fraction of Ultra cost.
            Tier::High => {
                let osc1 = self.osc_hi.tick();
                let sub = self.sub_osc_hi.tick() * self.sub_vol;
                let osc2 = if self.osc2_vol > 0.0 { self.osc2_hi.tick() } else { 0.0 };
                let mix = (osc1 + sub) * (1.0 - self.osc2_vol) + osc2 * self.osc2_vol;

                let hi = self.os2.up(mix * self.drive);
                let o = [
                    self.ladder_hi.tick(hi[0]),
                    self.ladder_hi.tick(hi[1]),
                ];
                // HIGH_LEVEL_TRIM equalises High's loudness to Ultra (tier-switch
                // is then timbre-only). Pre-comp-delay so the crossfade blends
                // level-matched paths.
                self.comp_high.process(self.os2.down(o) * (HIGH_LEVEL_TRIM / self.drive))
            }
            // Ultra: circuit VCO saw → nodal diode-ladder at 4× oversample.
            // The nodal VCO runs at base rate; its sharp JFET reset edge is corrected
            // by a 2-tap first-order BLEP computed inside `Vco303::tick()`. `blep_now`
            // is already calibrated to SPICE OR_SWING × VCO_GAIN (= 2.92 V); tap 1 is
            // auto-injected on the following tick. The 4× OS + decimation FIR then
            // band-limits the downstream ladder nonlinearity as before.
            //
            // RESIDUAL LIMITATION: the VCO still runs at base rate; higher-order alias
            // from the smooth ramp (not the reset edge) is negligible relative to the
            // reset. Option A (full 4× VCO, future `Tier::UltraHd`) gives complete alias
            // freedom at ~4× the CPU cost.
            Tier::Ultra => {
                let raw = self.vco.tick() * VCO_GAIN;
                let blep = match self.vco.wave {
                    OscWave::Saw => self.vco.blep_now,
                    OscWave::Sweep => self.vco.blep_now * (1.0 - self.vco.sweep_pos as f32),
                    OscWave::Square => 0.0,
                };
                let osc1 = raw + blep;
                let sub = self.sub_osc_hi.tick() * self.sub_vol;
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

                let hi = self.os.up(mix * self.drive);
                let o = [
                    self.ladder.tick(hi[0]),
                    self.ladder.tick(hi[1]),
                    self.ladder.tick(hi[2]),
                    self.ladder.tick(hi[3]),
                ];
                self.comp_ultra.process(self.os.down(o) / self.drive)
            }
        }
    }

    /// Advances the voice by one sample, computing modulations, rendering the active
    /// tier (or crossfading), and applying the VCA envelope and master volume.
    pub fn tick(&mut self) -> f32 {
        // Update operating temperature
        let target_temp = if self.gate {
            20.0 + 25.0 * self.drift
        } else {
            20.0
        };
        let tau = if self.gate { 10.0 } else { 30.0 };
        let alpha = 1.0 - (-1.0 / (tau * self.sr)).exp();
        self.temperature += alpha * (target_temp - self.temperature);

        // --- 1. Compute Pitch & Cutoff Modulations ---
        // Pitch drift (LFO pink noise walk + systematic temperature drift)
        let drift_pitch_val = self.drift_pitch.tick(0.2, self.sr); // slow 0.2 Hz walk
        let pitch_drift_st = drift_pitch_val * 0.2 * self.drift;
        let pitch_thermal_st = -0.1 * (self.temperature - 20.0) / 25.0; // -10 cents thermal warm-up

        // Tuning offset
        let tuning_st = self.tuning_offset / 100.0; // cents to semitones

        // Apply pitch update (Note: component age/tolerance do NOT affect pitch tracking)
        let osc1_f = self.base_hz * 2.0f32.powf((self.osc1_pitch_st + tuning_st + pitch_drift_st + pitch_thermal_st) / 12.0);
        self.osc.set_freq(osc1_f, self.sr);
        self.osc_hi.set_freq(osc1_f, self.sr);
        self.vco.set_freq(osc1_f);

        let sub_f = osc1_f * 0.5;
        self.sub_osc.set_freq(sub_f, self.sr);
        self.sub_osc_hi.set_freq(sub_f, self.sr);

        if self.osc2_vol > 0.0 {
            let osc2_f = self.base_hz * 2.0f32.powf((self.osc2_pitch_st + tuning_st + pitch_drift_st + pitch_thermal_st) / 12.0);
            self.osc2.set_freq(osc2_f, self.sr);
            self.osc2_hi.set_freq(osc2_f, self.sr);
            self.vco2.set_freq(osc2_f);
        }

        // --- 2. Filter Cutoff Modulation ---
        // Cutoff drift (LFO pink noise walk + systematic temperature drift)
        let drift_cut_val = self.drift_cutoff.tick(0.1, self.sr);
        let cutoff_drift_factor = 1.0 + drift_cut_val * 0.05 * self.drift;
        let cutoff_thermal_factor = 1.0 - 0.05 * (self.temperature - 20.0) / 25.0; // -5% cutoff warm-up

        // Age-related cutoff and resonance scaling (0 to 10 scale)
        let age_f32 = self.age as f32 / 10.0;
        let age_cutoff_factor = 1.0 - age_f32 * 0.15;
        let age_reso_factor = 1.0 - age_f32 * 0.20;

        // Accent accumulator charging
        let target_accent = if self.accent_active { 1.0 } else { 0.0 };
        // ~100 ms time constant leak integrator
        let acc_decay_coeff = 1.0 - (-1.0 / (0.1 * self.sr)).exp();
        self.accent_acc += acc_decay_coeff * (target_accent - self.accent_acc);

        // MEG envelope
        let meg_val = self.meg.tick(self.gate);

        // VCF Cutoff Base in Hz
        let base_cutoff_hz = 200.0 * 20.0f32.powf(self.cutoff_knob); // 200 Hz to 4000 Hz
        // Modulated Cutoff
        let mod_cutoff_hz = base_cutoff_hz 
            + meg_val * self.env_mod * 4000.0 * (1.0 + self.accent_knob_val * self.accent_acc);
        let final_cutoff_hz = (mod_cutoff_hz * cutoff_drift_factor * cutoff_thermal_factor * age_cutoff_factor)
            .clamp(20.0, self.sr * 0.45);

        // VCF Resonance
        let final_reso = self.reso_knob * age_reso_factor;

        // Apply to filters
        self.filter.set_cutoff(final_cutoff_hz);
        self.filter.set_reso(final_reso);
        self.ladder.set_cutoff(final_cutoff_hz as f32);
        self.ladder.set_reso(final_reso);
        self.ladder_hi.set_cutoff(final_cutoff_hz as f32);
        self.ladder_hi.set_reso(final_reso);

        // --- 3. Render Tier audio ---
        let f = match self.xfade_from {
            // Steady state: render the active tier only.
            None => self.render_tier(self.tier),
            // Crossfade: render both paths and blend equal-power. The outgoing
            // path keeps running until the fade completes, then is dropped.
            Some(from) => {
                let a = self.render_tier(from);
                let b = self.render_tier(self.tier);
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
                            self.xfade_from = Some(self.tier);
                            self.xfade_pos = 0.0;
                            self.tier = next;
                        }
                    }
                }
                out
            }
        };

        // --- 4. VCA Envelope Modulation ---
        let veg_val = self.veg.tick(self.gate);
        let vca_level = match self.vca_env_mode {
            VcaEnvMode::Gate => if self.gate { 1.0 } else { 0.0 },
            VcaEnvMode::Envelope => veg_val,
        };

        // Accent boosts VCA level
        let vca_gain = vca_level * (1.0 + self.accent_knob_val * self.accent_acc * 0.5);

        // Master Output volume db
        let master_gain = 10.0f32.powf(self.master_volume / 20.0);

        f * vca_gain * master_gain
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
        self.osc_hi.set_wave(wave);
        self.osc2_hi.set_wave(wave);
        self.vco.set_wave(wave);
        self.vco2.set_wave(wave);
    }

    pub fn set_ultra_brightness(&mut self, cal: f64) {
        self.ladder.set_cal(cal);
    }

    pub fn set_high_brightness(&mut self, cal: f64) {
        self.ladder_hi.set_cal(cal);
    }

    pub fn set_ultra_reso_gain(&mut self, g: f64) {
        self.ladder.set_reso_gain(g);
    }
}
