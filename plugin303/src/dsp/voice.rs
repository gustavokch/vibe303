//! Monophonic voice with dual oscillators, sub-oscillator, drift/age modulation.

use super::env::AdsrEnv;
use super::filter::{Filter303, Tier};
use super::ladder::TptLadder;
use super::ladder_wdf::LadderVcf;
use super::osc::BlepOsc;
use super::oversample::{Os2x, Os4x};
use super::vco::Vco303;
use super::drift::DriftGenerator;
use crate::params::{OscWave, VcfTaps, VcaEnvMode};

/// Equal-tempered MIDI note -> frequency (A4 = note 69 = 440 Hz).
pub fn midi_to_freq(note: u8) -> f32 {
    440.0 * 2.0f32.powf((note as f32 - 69.0) / 12.0)
}

pub const ULTRA_OS_FACTOR: f32 = 4.0;
pub const HIGH_OS_FACTOR: f32 = 2.0;
const VCO_GAIN: f32 = 2.0;
const ULTRA_BRIGHT: f64 = 0.70;
const HIGH_BRIGHT: f64 = 0.50;
const HIGH_LEVEL_TRIM: f32 = 1.45; // +3.2 dB
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

pub struct Voice {
    // --- Low Tier ---
    osc: BlepOsc,
    osc2: BlepOsc,
    sub_osc: BlepOsc,
    filter: TptLadder,

    // --- High Tier ---
    osc_hi: BlepOsc,
    osc2_hi: BlepOsc,
    sub_osc_hi: BlepOsc,
    ladder_hi: LadderVcf,
    os2: Os2x,

    // --- Ultra Tier ---
    vco: Vco303,
    vco2: Vco303,
    // Reuse sub_osc_hi for Ultra oversampling input
    ladder: LadderVcf,
    os: Os4x,

    // --- Envelopes & Modulators ---
    meg: AdsrEnv,
    veg: AdsrEnv,
    drift_pitch: DriftGenerator,
    drift_cutoff: DriftGenerator,
    accent_acc: f32,

    // --- Tier Switch/Crossfade ---
    tier: Tier,
    xfade_from: Option<Tier>,
    xfade_pos: f32,
    xfade_step: f32,
    pending_tier: Option<Tier>,
    comp_low: CompDelay,
    comp_high: CompDelay,
    comp_ultra: CompDelay,
    max_latency: usize,
    sr: f32,

    // --- Parameter States ---
    current_note: u8,
    base_hz: f32,
    tuning_offset: f32,
    osc1_pitch_st: f32,
    osc2_pitch_st: f32,
    osc2_vol: f32,
    sub_vol: f32,
    cutoff_knob: f32,
    reso_knob: f32,
    env_mod: f32,
    decay_knob: f32,
    accent_knob_val: f32,
    master_volume: f32,
    drive: f32,
    age: f32,
    drift: f32,
    vca_env_mode: VcaEnvMode,
    pub gate: bool,
    accent_active: bool,
}

impl Voice {
    pub fn new(sr: f32) -> Self {
        let lat_low = 0;
        let lat_high = Os2x::new().latency_samples();
        let lat_ultra = Os4x::new().latency_samples();
        let max_latency = lat_ultra.max(lat_high).max(lat_low);
        let mut ladder = LadderVcf::new(sr * ULTRA_OS_FACTOR);
        ladder.set_cal(ULTRA_BRIGHT);
        let mut ladder_hi = LadderVcf::new(sr * HIGH_OS_FACTOR);
        ladder_hi.set_cal(HIGH_BRIGHT);

        // Sub oscillators are always square waves
        let mut sub1 = BlepOsc::new();
        sub1.set_wave(OscWave::Square);
        let mut sub2 = BlepOsc::new();
        sub2.set_wave(OscWave::Square);

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

            vco: Vco303::new(sr),
            vco2: Vco303::new(sr),
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
            age: 0.0,
            drift: 0.0,
            vca_env_mode: VcaEnvMode::Envelope,
            gate: false,
            accent_active: false,
        }
    }

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
        comp_age: f32,
        comp_drift: f32,
    ) {
        self.tuning_offset = tuning_cents;
        self.osc1_pitch_st = osc1_pitch_st;
        self.osc2_pitch_st = osc2_pitch_st;
        self.osc2_vol = osc2_vol;
        self.sub_vol = sub_vol;
        self.cutoff_knob = cutoff;
        self.reso_knob = resonance;
        self.env_mod = env_mod;
        self.decay_knob = decay_knob;
        self.accent_knob_val = accent_knob;
        self.master_volume = volume_db;
        self.drive = 10.0f32.powf(vcf_drive_db / 20.0);
        self.age = comp_age;
        self.drift = comp_drift;
        self.vca_env_mode = vca_env_mode;

        // Propagate tap/pole count
        let poles_n = match vcf_taps {
            VcfTaps::TwoPole => 2,
            VcfTaps::ThreePole => 3,
            VcfTaps::FourPole => 4,
        };
        self.filter.set_poles(poles_n);
        self.ladder.set_poles(poles_n);
        self.ladder_hi.set_poles(poles_n);

        // Update Osc Waveforms & PW parameters
        self.osc.set_wave(osc1_wave);
        self.osc.set_pw(osc1_pw);
        self.osc2.set_wave(osc1_wave); // osc2 mirrors primary wave type
        self.osc2.set_pw(osc1_pw);

        self.osc_hi.set_wave(osc1_wave);
        self.osc_hi.set_pw(osc1_pw);
        self.osc2_hi.set_wave(osc1_wave);
        self.osc2_hi.set_pw(osc1_pw);

        self.vco.set_wave(osc1_wave);
        self.vco.set_sweep(osc1_pw as f64); // Sweep morph knob maps to PW control
        self.vco2.set_wave(osc1_wave);
        self.vco2.set_sweep(osc1_pw as f64);

        // Update Envelopes
        if advanced_env {
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

    pub fn set_tier_immediate(&mut self, tier: Tier) {
        self.tier = tier;
        self.xfade_from = None;
        self.xfade_pos = 1.0;
        self.pending_tier = None;
    }

    pub fn reset(&mut self) {
        self.osc = BlepOsc::new();
        self.osc2 = BlepOsc::new();
        self.sub_osc = BlepOsc::new();
        self.sub_osc.set_wave(OscWave::Square);

        self.osc_hi = BlepOsc::new();
        self.osc2_hi = BlepOsc::new();
        self.sub_osc_hi = BlepOsc::new();
        self.sub_osc_hi.set_wave(OscWave::Square);

        self.vco.reset();
        self.vco2.reset();
        self.filter.reset();
        self.ladder.reset();
        self.ladder_hi.reset();
        self.os = Os4x::new();
        self.os2 = Os2x::new();
        self.meg.reset();
        self.veg.reset();
        self.drift_pitch = DriftGenerator::new(1337);
        self.drift_cutoff = DriftGenerator::new(7331);
        self.accent_acc = 0.0;
        self.tier = Tier::Low;
        self.xfade_from = None;
        self.xfade_pos = 1.0;
        self.pending_tier = None;
        self.comp_low.reset();
        self.comp_high.reset();
        self.comp_ultra.reset();
    }

    pub fn set_tier(&mut self, tier: Tier) {
        if self.xfade_from.is_some() {
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

    fn render_tier(&mut self, tier: Tier) -> f32 {
        match tier {
            Tier::Low => {
                let osc1 = self.osc.tick();
                let sub = self.sub_osc.tick() * self.sub_vol;
                let osc2 = if self.osc2_vol > 0.0 { self.osc2.tick() } else { 0.0 };
                let mix = (osc1 + sub) * (1.0 - self.osc2_vol) + osc2 * self.osc2_vol;
                
                let y = self.filter.tick(mix * self.drive) / self.drive;
                self.comp_low.process(y)
            }
            Tier::High => {
                let osc1 = self.osc_hi.tick();
                let sub = self.sub_osc_hi.tick() * self.sub_vol;
                let osc2 = if self.osc2_vol > 0.0 { self.osc2_hi.tick() } else { 0.0 };
                let mix = (osc1 + sub) * (1.0 - self.osc2_vol) + osc2 * self.osc2_vol;

                let hi = self.os2.up(mix * self.drive);
                let o = [
                    self.ladder_hi.tick(hi[0]),
                    self.ladder_hi.tick(hi[1])
                ];
                self.comp_high.process(self.os2.down(o) * (HIGH_LEVEL_TRIM / self.drive))
            }
            Tier::Ultra => {
                let osc1 = self.vco.tick() * VCO_GAIN;
                let sub = self.sub_osc_hi.tick() * self.sub_vol;
                let osc2 = if self.osc2_vol > 0.0 { self.vco2.tick() * VCO_GAIN } else { 0.0 };
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

    pub fn tick(&mut self) -> f32 {
        // --- 1. Compute Pitch & Cutoff Modulations ---
        // Pitch drift (LFO random walk)
        let drift_pitch_val = self.drift_pitch.tick(0.2, self.sr); // slow 0.2 Hz walk
        let pitch_drift_st = drift_pitch_val * self.drift * 0.5; // Up to +-50 cents

        // Tuning offset
        let tuning_st = self.tuning_offset / 100.0; // cents to semitones

        // Age-related pitch tracking stretch
        let stretch_st = self.age * 0.02 * (self.current_note as f32 - 60.0) / 12.0;

        // Apply pitch update
        let osc1_f = self.base_hz * 2.0f32.powf((self.osc1_pitch_st + tuning_st + pitch_drift_st + stretch_st) / 12.0);
        self.osc.set_freq(osc1_f, self.sr);
        self.osc_hi.set_freq(osc1_f, self.sr);
        self.vco.set_freq(osc1_f);

        let sub_f = osc1_f * 0.5;
        self.sub_osc.set_freq(sub_f, self.sr);
        self.sub_osc_hi.set_freq(sub_f, self.sr);

        if self.osc2_vol > 0.0 {
            let osc2_f = self.base_hz * 2.0f32.powf((self.osc2_pitch_st + tuning_st + pitch_drift_st + stretch_st) / 12.0);
            self.osc2.set_freq(osc2_f, self.sr);
            self.osc2_hi.set_freq(osc2_f, self.sr);
            self.vco2.set_freq(osc2_f);
        }

        // --- 2. Filter Cutoff Modulation ---
        // Cutoff drift (LFO random walk)
        let drift_cut_val = self.drift_cutoff.tick(0.1, self.sr);
        let cutoff_drift_factor = 1.0 + drift_cut_val * self.drift * 0.15; // Up to +-15%

        // Age-related cutoff and resonance scaling
        let age_cutoff_factor = 1.0 - self.age * 0.15;
        let age_reso_factor = 1.0 - self.age * 0.20;

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
        let final_cutoff_hz = (mod_cutoff_hz * cutoff_drift_factor * age_cutoff_factor)
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
            None => self.render_tier(self.tier),
            Some(from) => {
                let a = self.render_tier(from);
                let b = self.render_tier(self.tier);
                let g = self.xfade_pos.min(1.0);
                let theta = g * std::f32::consts::FRAC_PI_2;
                let out = a * theta.cos() + b * theta.sin();
                self.xfade_pos += self.xfade_step;
                if self.xfade_pos >= 1.0 {
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
