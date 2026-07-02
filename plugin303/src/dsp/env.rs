//! Envelope generators (ADSR). Supports classic AD and advanced ADSR.
//!
//! Faithful 303 AD shape (exp decay, accent) is emulated dynamically in `voice.rs`
//! by combining `AdsrEnv` (with sustain=0) with the accent integration capacitor value.

/// ADSR stage tracker.
#[derive(Clone, Copy, PartialEq)]
enum Stage {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

/// A standard ADSR envelope generator that collapses back to a pure AD envelope
/// when `sustain` is set to `0.0`.
pub struct AdsrEnv {
    /// Current stage of the envelope contour.
    stage: Stage,
    /// Current output level of the envelope in [0, 1].
    level: f32,
    /// Sample rate.
    sr: f32,
    /// Per-sample envelope level increment during the attack stage.
    atk_inc: f32,
    /// Per-sample envelope level decrement during the decay stage.
    dec_inc: f32,
    /// Sustain level in [0, 1].
    sus_level: f32,
    /// Per-sample envelope level decrement during the release stage.
    rel_inc: f32,
}

impl AdsrEnv {
    pub fn new(sr: f32) -> Self {
        let mut s = Self {
            stage: Stage::Idle,
            level: 0.0,
            sr,
            atk_inc: 0.0,
            dec_inc: 0.0,
            sus_level: 0.0,
            rel_inc: 0.0,
        };
        s.set_attack(3.0);
        s.set_decay(200.0);
        s.set_sustain(0.0);
        s.set_release(200.0);
        s
    }

    pub fn set_attack(&mut self, ms: f32) {
        let n = (ms * 0.001 * self.sr).max(1.0);
        self.atk_inc = 1.0 / n;
    }

    pub fn set_decay(&mut self, ms: f32) {
        let n = (ms * 0.001 * self.sr).max(1.0);
        self.dec_inc = 1.0 / n;
    }

    pub fn set_sustain(&mut self, level: f32) {
        self.sus_level = level.clamp(0.0, 1.0);
    }

    pub fn set_release(&mut self, ms: f32) {
        let n = (ms * 0.001 * self.sr).max(1.0);
        self.rel_inc = 1.0 / n;
    }

    pub fn trigger(&mut self) {
        self.stage = Stage::Attack;
    }

    pub fn release(&mut self) {
        if self.stage != Stage::Idle {
            self.stage = Stage::Release;
        }
    }

    pub fn reset(&mut self) {
        self.stage = Stage::Idle;
        self.level = 0.0;
    }

    pub fn tick(&mut self, gate: bool) -> f32 {
        // If gate goes low, transition to release stage (unless already idle)
        if !gate && self.sus_level > 0.0 && self.stage != Stage::Release && self.stage != Stage::Idle {
            self.stage = Stage::Release;
        }

        match self.stage {
            Stage::Idle => {
                self.level = 0.0;
            }
            Stage::Attack => {
                self.level += self.atk_inc;
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.stage = Stage::Decay;
                }
            }
            Stage::Decay => {
                if self.level > self.sus_level {
                    self.level -= self.dec_inc;
                    if self.level <= self.sus_level {
                        self.level = self.sus_level;
                        self.stage = if self.sus_level > 0.0 {
                            Stage::Sustain
                        } else {
                            Stage::Idle
                        };
                    }
                } else {
                    self.level = self.sus_level;
                    self.stage = if self.sus_level > 0.0 {
                        Stage::Sustain
                    } else {
                        Stage::Idle
                    };
                }
            }
            Stage::Sustain => {
                self.level = self.sus_level;
                // Stays in sustain until gate goes low
            }
            Stage::Release => {
                self.level -= self.rel_inc;
                if self.level <= 0.0 {
                    self.level = 0.0;
                    self.stage = Stage::Idle;
                }
            }
        }
        self.level
    }

    /// For backward compatibility with basic AD triggering
    pub fn trigger_ad(&mut self) {
        self.trigger();
    }

    pub fn tick_ad(&mut self) -> f32 {
        self.tick(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_outputs_zero() {
        let mut e = AdsrEnv::new(48_000.0);
        for _ in 0..1_000 {
            assert_eq!(e.tick(false), 0.0);
        }
    }

    #[test]
    fn attack_rises_then_decay_falls() {
        let sr = 48_000.0;
        let mut e = AdsrEnv::new(sr);
        e.set_attack(2.0); // ~96 samples
        e.set_decay(10.0); // ~480 samples
        e.set_sustain(0.0);
        e.trigger();
        
        let mut peak = 0.0f32;
        for _ in 0..96 {
            peak = e.tick(true);
        }
        assert!(peak > 0.95, "attack did not reach peak: {peak}");
        
        let mut tail = 1.0f32;
        for _ in 0..600 {
            tail = e.tick(true);
        }
        assert!(tail < 0.05, "decay did not fall: {tail}");
    }

    #[test]
    fn sustains_until_released() {
        let sr = 48_000.0;
        let mut e = AdsrEnv::new(sr);
        e.set_attack(2.0);
        e.set_decay(10.0);
        e.set_sustain(0.5);
        e.set_release(10.0);
        e.trigger();
        
        for _ in 0..600 {
            e.tick(true);
        }
        let sus_level = e.tick(true);
        assert!((sus_level - 0.5).abs() < 0.05, "did not sustain: {sus_level}");
        
        e.release();
        let mut tail = 0.5f32;
        for _ in 0..600 {
            tail = e.tick(false);
        }
        assert!(tail < 0.05, "release did not fall: {tail}");
    }
}
