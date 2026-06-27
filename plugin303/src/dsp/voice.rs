//! Monophonic voice: osc -> ladder VCF -> VCA(env). M1 wiring only.
//! Accent, glide, sub/detune and the souped levers come in M4/M6.

use super::env::AdEnv;
use super::filter::{Filter303, Tier};
use super::ladder::TptLadder;
use super::ladder_wdf::LadderVcf;
use super::osc::Saw;
use super::oversample::{Os2x, Os4x};
use super::vco::Vco303;

/// Equal-tempered MIDI note -> frequency (A4 = note 69 = 440 Hz).
pub fn midi_to_freq(note: u8) -> f32 {
    440.0 * 2.0f32.powf((note as f32 - 69.0) / 12.0)
}

/// Ultra-tier oversampling factor (nodal ladder runs at `OS_FACTOR × sr`).
pub const ULTRA_OS_FACTOR: f32 = 4.0;

/// High-tier oversampling factor (polyBLEP saw → nodal ladder at `2 × sr`).
pub const HIGH_OS_FACTOR: f32 = 2.0;

/// Level trim to bring the DC-blocked VCO saw to ~unit (polyBLEP-saw) amplitude.
const VCO_GAIN: f32 = 2.0;

/// Tier-switch crossfade length in seconds (equal-power settle-then-crossfade).
/// Long enough to mask the incoming path's cold-start transient (VCO 1 ms self-
/// start, ladder warm-up) and any oscillator phase jump, short enough to feel
/// instant.
const XFADE_SECS: f32 = 0.010;

pub struct Voice {
    /// Low-tier polyBLEP saw.
    osc: Saw,
    /// High-tier polyBLEP saw — a *separate* oscillator so Low and High can run
    /// simultaneously during a crossfade without double-advancing one shared
    /// phase (which would jump the pitch mid-fade).
    osc_hi: Saw,
    /// Ultra-tier nodal circuit VCO (replaces the polyBLEP saw for Ultra only).
    vco: Vco303,
    filter: TptLadder,
    /// Ultra-tier nodal diode-ladder, preallocated, run inside `os`.
    ladder: LadderVcf,
    os: Os4x,
    /// High-tier nodal diode-ladder (polyBLEP saw source), run inside `os2`.
    ladder_hi: LadderVcf,
    os2: Os2x,
    env: AdEnv,
    /// Active (target) tier.
    tier: Tier,
    /// Tier being faded *out*, if a crossfade is in progress. `None` = steady.
    xfade_from: Option<Tier>,
    /// Crossfade position in [0, 1] (0 = all `xfade_from`, 1 = all `tier`).
    xfade_pos: f32,
    /// Per-sample crossfade increment (= 1 / fade-length-samples).
    xfade_step: f32,
    sr: f32,
}

impl Voice {
    pub fn new(sr: f32) -> Self {
        Self {
            osc: Saw::new(),
            osc_hi: Saw::new(),
            vco: Vco303::new(sr),
            filter: TptLadder::new(sr),
            ladder: LadderVcf::new(sr * ULTRA_OS_FACTOR),
            os: Os4x::new(),
            ladder_hi: LadderVcf::new(sr * HIGH_OS_FACTOR),
            os2: Os2x::new(),
            env: AdEnv::new(sr),
            tier: Tier::Low,
            xfade_from: None,
            xfade_pos: 1.0,
            xfade_step: 1.0 / (sr * XFADE_SECS).max(1.0),
            sr,
        }
    }

    /// Select the fidelity tier. Low routes to the TPT path; High routes to the
    /// nodal diode-ladder inside the 2× oversampler (polyBLEP saw, F4.1); Ultra
    /// routes to the circuit VCO → nodal ladder at 4× (F3.5).
    ///
    /// A change starts a short equal-power crossfade (F5.1) from the previous
    /// tier so the path switch is click-free — the tiers have different timbre
    /// and the incoming path starts cold, so a hard switch would pop. No
    /// allocation, RT-safe (plain field writes).
    /// Set the tier with no crossfade — for the initial/persisted selection at
    /// construction or `initialize`, where there is no prior audio to fade from.
    /// Runtime changes go through [`set_tier`] (faded).
    pub fn set_tier_immediate(&mut self, tier: Tier) {
        self.tier = tier;
        self.xfade_from = None;
        self.xfade_pos = 1.0;
    }

    pub fn set_tier(&mut self, tier: Tier) {
        if tier == self.tier {
            return;
        }
        // Snapshot the outgoing tier and (re)start the fade. If a fade was
        // already mid-flight, the current target becomes the new outgoing path —
        // a tiny level wobble at worst, and only when the user spins the param.
        self.xfade_from = Some(self.tier);
        self.xfade_pos = 0.0;
        self.tier = tier;
    }

    /// Added latency (base samples) of the current tier. High/Ultra add their OS
    /// round-trip group delay; Low is zero-latency. Host PDC wiring is F5.
    pub fn latency_samples(&self) -> usize {
        match self.tier {
            Tier::Low => 0,
            Tier::High => self.os2.latency_samples(),
            Tier::Ultra => self.os.latency_samples(),
        }
    }

    pub fn note_on(&mut self, note: u8) {
        let f = midi_to_freq(note);
        self.osc.set_freq(f, self.sr);
        self.osc_hi.set_freq(f, self.sr);
        self.vco.set_freq(f);
        self.env.trigger();
    }

    /// Filter cutoff in Hz. Applied to every tier's backend so tier switches are
    /// click-free on the parameter axis.
    pub fn set_cutoff(&mut self, hz: f32) {
        self.filter.set_cutoff(hz);
        self.ladder.set_cutoff(hz);
        self.ladder_hi.set_cutoff(hz);
    }

    /// Filter resonance in [0, 1].
    pub fn set_reso(&mut self, r: f32) {
        self.filter.set_reso(r);
        self.ladder.set_reso(r);
        self.ladder_hi.set_reso(r);
    }

    /// Ultra-tier VCF brightness calibration (spectral V5). Default is the
    /// ladder's built-in 1.0; the spectral gate raises it toward the SPICE
    /// envelope. No effect on Low/High.
    pub fn set_ultra_brightness(&mut self, cal: f64) {
        self.ladder.set_cal(cal);
    }

    /// One pre-envelope sample from a single tier's signal path, advancing only
    /// that tier's backend state. Each tier owns independent oscillator/filter
    /// state, so two tiers can be rendered in the same sample (for a crossfade)
    /// without interfering.
    fn render_tier(&mut self, tier: Tier) -> f32 {
        match tier {
            // Low: polyBLEP saw → TPT ladder (cheap, ships).
            Tier::Low => self.filter.tick(self.osc.tick()),
            // High (F4.1): polyBLEP saw → nodal diode-ladder at 2× oversample.
            // Skips the costly nodal VCO Newton but keeps the circuit-filter
            // character — sits between Low and Ultra at a fraction of Ultra cost.
            Tier::High => {
                let hi = self.os2.up(self.osc_hi.tick());
                let o = [self.ladder_hi.tick(hi[0]), self.ladder_hi.tick(hi[1])];
                self.os2.down(o)
            }
            // Ultra: circuit VCO saw → nodal diode-ladder at 4× oversample.
            Tier::Ultra => {
                let s = self.vco.tick() * VCO_GAIN;
                let hi = self.os.up(s);
                let o = [
                    self.ladder.tick(hi[0]),
                    self.ladder.tick(hi[1]),
                    self.ladder.tick(hi[2]),
                    self.ladder.tick(hi[3]),
                ];
                self.os.down(o)
            }
        }
    }

    pub fn tick(&mut self) -> f32 {
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
                    self.xfade_from = None;
                }
                out
            }
        };
        f * self.env.tick()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn midi_to_freq_anchors() {
        assert!((midi_to_freq(69) - 440.0).abs() < 0.01);
        assert!((midi_to_freq(81) - 880.0).abs() < 0.01); // one octave up
        assert!((midi_to_freq(57) - 220.0).abs() < 0.01); // one octave down
    }

    fn rms(v: &mut Voice, n: usize) -> f32 {
        let mut acc = 0.0f32;
        for _ in 0..n {
            let y = v.tick();
            acc += y * y;
        }
        (acc / n as f32).sqrt()
    }

    #[test]
    fn silent_before_note_on() {
        let mut v = Voice::new(48_000.0);
        assert_eq!(rms(&mut v, 1_000), 0.0);
    }

    #[test]
    fn cutoff_setter_changes_brightness() {
        fn hf_rms(cut: f32) -> f32 {
            let mut v = Voice::new(48_000.0);
            v.set_cutoff(cut);
            v.note_on(57);
            for _ in 0..400 {
                v.tick();
            }
            let mut acc = 0.0f32;
            for _ in 0..4_000 {
                let s = v.tick();
                acc += s * s;
            }
            (acc / 4_000.0).sqrt()
        }
        assert!(
            hf_rms(4_000.0) > hf_rms(300.0) * 1.5,
            "cutoff did not change output energy"
        );
    }

    #[test]
    fn latency_matches_tier_oversampler() {
        // PDC contract (F5.2): Low is zero-latency; High/Ultra report their OS
        // round-trip group delay, and Ultra (4×) ≥ High (2×).
        let mut v = Voice::new(48_000.0);
        v.set_tier(Tier::Low);
        assert_eq!(v.latency_samples(), 0, "Low must be zero-latency");
        v.set_tier(Tier::High);
        let high = v.latency_samples();
        v.set_tier(Tier::Ultra);
        let ultra = v.latency_samples();
        assert!(high > 0, "High OS must report latency");
        assert!(ultra >= high, "Ultra (4×) latency must be ≥ High (2×)");
    }

    #[test]
    fn produces_signal_after_note_on() {
        let mut v = Voice::new(48_000.0);
        v.note_on(69);
        // Skip the short attack, then measure the sustained decay body.
        for _ in 0..200 {
            v.tick();
        }
        assert!(rms(&mut v, 2_000) > 0.01, "voice produced no signal");
    }
}
