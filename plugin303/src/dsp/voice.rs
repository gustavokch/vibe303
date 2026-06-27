//! Monophonic voice: osc -> ladder VCF -> VCA(env). M1 wiring only.
//! Accent, glide, sub/detune and the souped levers come in M4/M6.

use super::env::AdEnv;
use super::filter::Tier;
use super::ladder::TptLadder;
use super::osc::Saw;

/// Equal-tempered MIDI note -> frequency (A4 = note 69 = 440 Hz).
pub fn midi_to_freq(note: u8) -> f32 {
    440.0 * 2.0f32.powf((note as f32 - 69.0) / 12.0)
}

pub struct Voice {
    osc: Saw,
    filter: TptLadder,
    env: AdEnv,
    tier: Tier,
    sr: f32,
}

impl Voice {
    pub fn new(sr: f32) -> Self {
        Self {
            osc: Saw::new(),
            filter: TptLadder::new(sr),
            env: AdEnv::new(sr),
            tier: Tier::Low,
            sr,
        }
    }

    /// Select the fidelity tier. F2: High/Ultra route to the TPT path until the WDF
    /// backends land (F3/F4). A plain field write — no allocation, RT-safe.
    pub fn set_tier(&mut self, tier: Tier) {
        self.tier = tier;
    }

    pub fn note_on(&mut self, note: u8) {
        self.osc.set_freq(midi_to_freq(note), self.sr);
        self.env.trigger();
    }

    /// Filter cutoff in Hz.
    pub fn set_cutoff(&mut self, hz: f32) {
        self.filter.set_cutoff(hz);
    }

    /// Filter resonance in [0, 1].
    pub fn set_reso(&mut self, r: f32) {
        self.filter.set_reso(r);
    }

    pub fn tick(&mut self) -> f32 {
        let s = self.osc.tick();
        // F3/F4: route High/Ultra to the WDF backends. All tiers use TPT for now.
        let f = match self.tier {
            Tier::Low | Tier::High | Tier::Ultra => self.filter.tick(s),
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
