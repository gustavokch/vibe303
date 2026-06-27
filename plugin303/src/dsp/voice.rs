//! Monophonic voice: osc -> ladder VCF -> VCA(env). M1 wiring only.
//! Accent, glide, sub/detune and the souped levers come in M4/M6.

use super::env::AdEnv;
use super::filter::{Filter303, Tier};
use super::ladder::TptLadder;
use super::ladder_wdf::LadderVcf;
use super::osc::Saw;
use super::oversample::Os4x;
use super::vco::Vco303;

/// Equal-tempered MIDI note -> frequency (A4 = note 69 = 440 Hz).
pub fn midi_to_freq(note: u8) -> f32 {
    440.0 * 2.0f32.powf((note as f32 - 69.0) / 12.0)
}

/// Ultra-tier oversampling factor (nodal ladder runs at `OS_FACTOR × sr`).
pub const ULTRA_OS_FACTOR: f32 = 4.0;

/// Level trim to bring the DC-blocked VCO saw to ~unit (polyBLEP-saw) amplitude.
const VCO_GAIN: f32 = 2.0;

pub struct Voice {
    osc: Saw,
    /// Ultra-tier nodal circuit VCO (replaces the polyBLEP saw for Ultra only).
    vco: Vco303,
    filter: TptLadder,
    /// Ultra-tier nodal diode-ladder, preallocated, run inside `os`.
    ladder: LadderVcf,
    os: Os4x,
    env: AdEnv,
    tier: Tier,
    sr: f32,
}

impl Voice {
    pub fn new(sr: f32) -> Self {
        Self {
            osc: Saw::new(),
            vco: Vco303::new(sr),
            filter: TptLadder::new(sr),
            ladder: LadderVcf::new(sr * ULTRA_OS_FACTOR),
            os: Os4x::new(),
            env: AdEnv::new(sr),
            tier: Tier::Low,
            sr,
        }
    }

    /// Select the fidelity tier. Low/High route to the TPT path; Ultra routes to
    /// the nodal diode-ladder inside the 4× oversampler (F3.5). A plain field
    /// write — no allocation, RT-safe.
    pub fn set_tier(&mut self, tier: Tier) {
        self.tier = tier;
    }

    /// Added latency (base samples) of the current tier. Ultra adds the 4× OS
    /// round-trip group delay; Low/High are zero-latency. Host PDC wiring is F5.
    pub fn latency_samples(&self) -> usize {
        match self.tier {
            Tier::Low | Tier::High => 0,
            Tier::Ultra => self.os.latency_samples(),
        }
    }

    pub fn note_on(&mut self, note: u8) {
        let f = midi_to_freq(note);
        self.osc.set_freq(f, self.sr);
        self.vco.set_freq(f);
        self.env.trigger();
    }

    /// Filter cutoff in Hz. Applied to every tier's backend so tier switches are
    /// click-free on the parameter axis.
    pub fn set_cutoff(&mut self, hz: f32) {
        self.filter.set_cutoff(hz);
        self.ladder.set_cutoff(hz);
    }

    /// Filter resonance in [0, 1].
    pub fn set_reso(&mut self, r: f32) {
        self.filter.set_reso(r);
        self.ladder.set_reso(r);
    }

    /// Ultra-tier VCF brightness calibration (spectral V5). Default is the
    /// ladder's built-in 1.0; the spectral gate raises it toward the SPICE
    /// envelope. No effect on Low/High.
    pub fn set_ultra_brightness(&mut self, cal: f64) {
        self.ladder.set_cal(cal);
    }

    pub fn tick(&mut self) -> f32 {
        let f = match self.tier {
            // F4: High gets the cheaper WDF-lite solver; on TPT for now.
            Tier::Low | Tier::High => self.filter.tick(self.osc.tick()),
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
