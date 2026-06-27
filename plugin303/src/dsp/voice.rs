//! Monophonic voice: osc -> ladder VCF -> VCA(env). M1 wiring only.
//! Accent, glide, sub/detune and the souped levers come in M4/M6.

use super::env::AdEnv;
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
    sr: f32,
}

impl Voice {
    pub fn new(sr: f32) -> Self {
        Self {
            osc: Saw::new(),
            filter: TptLadder::new(sr),
            env: AdEnv::new(sr),
            sr,
        }
    }

    pub fn note_on(&mut self, note: u8) {
        self.osc.set_freq(midi_to_freq(note), self.sr);
        self.env.trigger();
    }

    pub fn tick(&mut self) -> f32 {
        let s = self.osc.tick();
        let f = self.filter.tick(s);
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
