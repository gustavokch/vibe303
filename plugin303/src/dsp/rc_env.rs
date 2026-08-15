//! RC-filter-based envelope generators and lowpass smoothers.
//! Replicates the exact decay, attack/release stages, and slew behavior
//! of the jc303/Open303 reference.

/// Main Envelope Generator (MEG) that realizes a pure exponential decay.
pub struct DecayEnv {
    /// Coefficient for multiplicative accumulation.
    c: f64,
    /// Current output state.
    y: f64,
    /// Initial value after trigger.
    y_init: f64,
    /// Time constant in milliseconds.
    tau: f64,
    /// Sample rate.
    fs: f64,
}

impl DecayEnv {
    pub fn new(fs: f64) -> Self {
        let mut s = Self {
            c: 1.0,
            y: 0.0,
            y_init: 1.0,
            tau: 200.0,
            fs,
        };
        s.calculate_coefficient();
        s
    }

    pub fn set_sample_rate(&mut self, new_fs: f64) {
        if new_fs > 0.0 {
            self.fs = new_fs;
            self.calculate_coefficient();
        }
    }

    pub fn set_decay(&mut self, new_tau_ms: f64) {
        if new_tau_ms > 0.001 {
            self.tau = new_tau_ms;
            self.calculate_coefficient();
        }
    }

    pub fn get_decay_time_constant(&self) -> f64 {
        self.tau
    }

    pub fn trigger(&mut self) {
        self.y = self.y_init;
    }

    pub fn end_is_reached(&self, threshold: f64) -> bool {
        self.y < threshold
    }

    pub fn reset(&mut self) {
        self.y = 0.0;
    }

    #[inline]
    pub fn tick(&mut self) -> f64 {
        self.y *= self.c;
        self.y
    }
}

/// An exponential envelope generator (VCA) with adjustable start, attack, peak,
/// hold, decay, sustain, release, and end values.
pub struct RcEnv {
    // Level parameters
    #[allow(dead_code)]
    start_level: f64,
    peak_level: f64,
    sustain_level: f64,
    end_level: f64,

    // Time parameters in milliseconds
    attack_time: f64,
    hold_time: f64,
    decay_time: f64,
    release_time: f64,

    // Scale parameters
    time_scale: f64,
    tau_scale: f64,
    peak_scale: f64,

    // Stage accumulation markers
    att_plus_hld: f64,
    att_plus_hld_plus_dec: f64,

    // Internal state
    time: f64,
    increment: f64,
    attack_coeff: f64,
    decay_coeff: f64,
    release_coeff: f64,
    previous_output: f64,
    sample_rate: f64,
    note_is_on: bool,
}

impl RcEnv {
    pub fn new(sr: f64) -> Self {
        let mut s = Self {
            start_level: 0.0,
            peak_level: 1.0,
            sustain_level: 0.5,
            end_level: 0.0,
            attack_time: 0.0,
            hold_time: 0.0,
            decay_time: 100.0,
            release_time: 10.0,
            time_scale: 1.0,
            tau_scale: 1.0,
            peak_scale: 1.0,
            att_plus_hld: 0.0,
            att_plus_hld_plus_dec: 0.0,
            time: 0.0,
            increment: 0.0,
            attack_coeff: 1.0,
            decay_coeff: 1.0,
            release_coeff: 1.0,
            previous_output: 0.0,
            sample_rate: sr,
            note_is_on: false,
        };
        s.set_sample_rate(sr);
        s
    }

    pub fn set_sample_rate(&mut self, sr: f64) {
        if sr > 0.0 {
            self.sample_rate = sr;
        }
        self.update_increment();
        self.update_coefficients();
    }

    pub fn set_attack(&mut self, ms: f64) {
        self.attack_time = ms.max(0.0);
        self.update_attack_coeff();
        self.calculate_accumulated_times();
    }

    pub fn set_hold(&mut self, ms: f64) {
        self.hold_time = ms.max(0.0);
        self.calculate_accumulated_times();
    }

    pub fn set_decay(&mut self, ms: f64) {
        self.decay_time = ms.max(0.0);
        self.update_decay_coeff();
        self.calculate_accumulated_times();
    }

    pub fn set_release(&mut self, ms: f64) {
        self.release_time = ms.max(0.0);
        self.update_release_coeff();
    }

    pub fn set_sustain(&mut self, level: f64) {
        self.sustain_level = level;
    }

    pub fn is_note_on(&self) -> bool {
        self.note_is_on
    }

    pub fn note_on(&mut self) {
        self.time = 0.0;
        self.note_is_on = true;
    }

    pub fn note_off(&mut self) {
        self.note_is_on = false;
        // Advance time to start of release phase
        self.time = self.attack_time + self.hold_time + self.decay_time + self.increment;
    }

    pub fn reset(&mut self) {
        self.time = 0.0;
        self.previous_output = 0.0;
        self.note_is_on = false;
    }

    pub fn end_is_reached(&self) -> bool {
        !self.note_is_on && self.previous_output < 0.000001
    }

    #[inline]
    pub fn tick(&mut self) -> f64 {
        let out: f64;

        if self.time <= self.att_plus_hld {
            out = self.previous_output
                + self.attack_coeff * (self.peak_scale * self.peak_level - self.previous_output);
            self.time += self.increment;
        } else if self.time <= self.att_plus_hld_plus_dec {
            out = self.previous_output
                + self.decay_coeff * (self.sustain_level - self.previous_output);
            self.time += self.increment;
        } else if self.note_is_on {
            out = self.previous_output
                + self.decay_coeff * (self.sustain_level - self.previous_output);
        } else {
            out =
                self.previous_output + self.release_coeff * (self.end_level - self.previous_output);
            self.time += self.increment;
        }

        self.previous_output = out;
        out
    }

    fn calculate_accumulated_times(&mut self) {
        self.att_plus_hld = self.attack_time + self.hold_time;
        self.att_plus_hld_plus_dec = self.att_plus_hld + self.decay_time;
    }

    fn update_increment(&mut self) {
        self.increment = 1000.0 * self.time_scale / self.sample_rate;
    }

    fn update_attack_coeff(&mut self) {
        if self.attack_time > 0.0 {
            let tau =
                (self.sample_rate * 0.001 * self.attack_time) * self.tau_scale / self.time_scale;
            self.attack_coeff = 1.0 - (-1.0 / tau).exp();
        } else {
            self.attack_coeff = 1.0;
        }
    }

    fn update_decay_coeff(&mut self) {
        if self.decay_time > 0.0 {
            let tau =
                (self.sample_rate * 0.001 * self.decay_time) * self.tau_scale / self.time_scale;
            self.decay_coeff = 1.0 - (-1.0 / tau).exp();
        } else {
            self.decay_coeff = 1.0;
        }
    }

    fn update_release_coeff(&mut self) {
        if self.release_time > 0.0 {
            let tau =
                (self.sample_rate * 0.001 * self.release_time) * self.tau_scale / self.time_scale;
            self.release_coeff = 1.0 - (-1.0 / tau).exp();
        } else {
            self.release_coeff = 1.0;
        }
    }

    fn update_coefficients(&mut self) {
        self.update_attack_coeff();
        self.update_decay_coeff();
        self.update_release_coeff();
    }
}

/// A leaky integrator lowpass filter with user-adjustable time constant (in ms).
pub struct LeakyIntegrator {
    coeff: f64,
    y1: f64,
    tau: f64,
    fs: f64,
}

impl LeakyIntegrator {
    pub fn new(fs: f64) -> Self {
        let mut s = Self {
            coeff: 0.0,
            y1: 0.0,
            tau: 1.0,
            fs,
        };
        s.calculate_coefficient();
        s
    }

    pub fn set_sample_rate(&mut self, fs: f64) {
        if fs > 0.0 {
            self.fs = fs;
            self.calculate_coefficient();
        }
    }

    pub fn set_time_constant(&mut self, new_tau_ms: f64) {
        self.tau = new_tau_ms.max(0.0);
        self.calculate_coefficient();
    }

    pub fn set_state(&mut self, state: f64) {
        self.y1 = state;
    }

    pub fn get_state(&self) -> f64 {
        self.y1
    }

    pub fn reset(&mut self) {
        self.y1 = 0.0;
    }

    #[inline]
    pub fn tick(&mut self, input: f64) -> f64 {
        self.y1 = input + self.coeff * (self.y1 - input);
        self.y1
    }

    fn calculate_coefficient(&mut self) {
        if self.tau > 0.0 {
            self.coeff = (-1.0 / (0.001 * self.tau * self.fs)).exp();
        } else {
            self.coeff = 0.0;
        }
    }
}

impl DecayEnv {
    fn calculate_coefficient(&mut self) {
        self.c = (-1.0 / (0.001 * self.tau * self.fs)).exp();
        self.y_init = 1.0 / self.c;
    }
}

/// Models the TB-303 Accent Sweep circuit (diode D24 + C13).
/// Features asymmetric charge/discharge times and free-running memory.
pub struct AccentSweep {
    v_c13: f64,
    fs: f64,
    coeff_chg: f64,
    coeff_dis: f64,
}

impl AccentSweep {
    pub fn new(fs: f64) -> Self {
        let mut s = Self {
            v_c13: 0.0,
            fs: 44100.0,
            coeff_chg: 0.0,
            coeff_dis: 0.0,
        };
        s.set_sample_rate(fs);
        s
    }

    pub fn set_sample_rate(&mut self, fs: f64) {
        if fs > 0.0 {
            self.fs = fs;
            // Charge: fast (diode D24 conducting, tau ~ 50ms)
            let tau_chg = 50.0;
            self.coeff_chg = 1.0 - (-1.0 / (0.001 * tau_chg * self.fs)).exp();
            // Discharge: slow (diode D24 reverse-biased, tau ~ 130ms)
            let tau_dis = 130.0;
            self.coeff_dis = 1.0 - (-1.0 / (0.001 * tau_dis * self.fs)).exp();
        }
    }

    pub fn reset(&mut self) {
        self.v_c13 = 0.0;
    }

    /// `src` is the raw envelope value * accent_knob.
    /// `resonance_knob` (0.0 to 1.0) controls the charge resistance and depth (VR4-B).
    #[inline]
    pub fn tick(&mut self, src: f64, resonance_knob: f64) -> f64 {
        // VR4-B couples sweep depth with resonance. Provides baseline sweep depth at min resonance
        // and expands to full depth as resonance knob is raised.
        let sweep_scale = 0.35 + 0.65 * resonance_knob.clamp(0.0, 1.0);
        let drive = src * sweep_scale;

        if drive > self.v_c13 {
            self.v_c13 += self.coeff_chg * (drive - self.v_c13);
        } else {
            self.v_c13 += self.coeff_dis * (0.0 - self.v_c13);
        }
        self.v_c13
    }
}
