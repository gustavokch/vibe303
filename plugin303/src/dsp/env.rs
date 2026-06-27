//! Amplitude envelope. M1: a minimal AD (attack-decay) contour, linear ramps.
//! Faithful 303 AD shape (exp decay, accent) comes in M4.

#[derive(Clone, Copy, PartialEq)]
enum Stage {
    Idle,
    Attack,
    Decay,
}

pub struct AdEnv {
    stage: Stage,
    level: f32,
    sr: f32,
    atk_inc: f32, // per-sample rise during attack
    dec_inc: f32, // per-sample fall during decay
}

impl AdEnv {
    pub fn new(sr: f32) -> Self {
        let mut s = Self {
            stage: Stage::Idle,
            level: 0.0,
            sr,
            atk_inc: 0.0,
            dec_inc: 0.0,
        };
        s.set_attack(3.0);
        s.set_decay(200.0);
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

    /// Gate-on: restart the contour from the attack stage.
    pub fn trigger(&mut self) {
        self.stage = Stage::Attack;
    }

    pub fn tick(&mut self) -> f32 {
        match self.stage {
            Stage::Idle => {}
            Stage::Attack => {
                self.level += self.atk_inc;
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.stage = Stage::Decay;
                }
            }
            Stage::Decay => {
                self.level -= self.dec_inc;
                if self.level <= 0.0 {
                    self.level = 0.0;
                    self.stage = Stage::Idle;
                }
            }
        }
        self.level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_outputs_zero() {
        let mut e = AdEnv::new(48_000.0);
        for _ in 0..1_000 {
            assert_eq!(e.tick(), 0.0);
        }
    }

    #[test]
    fn attack_rises_then_decay_falls() {
        let sr = 48_000.0;
        let mut e = AdEnv::new(sr);
        e.set_attack(2.0); // ~96 samples
        e.set_decay(10.0); // ~480 samples
        e.trigger();
        // After the attack window the level should be at/near the peak.
        let mut peak = 0.0f32;
        for _ in 0..96 {
            peak = e.tick();
        }
        assert!(peak > 0.95, "attack did not reach peak: {peak}");
        // After the decay window it should be back near zero.
        let mut tail = 1.0f32;
        for _ in 0..600 {
            tail = e.tick();
        }
        assert!(tail < 0.05, "decay did not fall: {tail}");
    }

    #[test]
    fn stays_in_unit_range() {
        let mut e = AdEnv::new(48_000.0);
        e.trigger();
        for _ in 0..50_000 {
            let v = e.tick();
            assert!((0.0..=1.0).contains(&v), "out of range: {v}");
        }
    }
}
