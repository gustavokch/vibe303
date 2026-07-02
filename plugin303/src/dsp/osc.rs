//! Oscillators. M1: a minimal polyBLEP saw (good enough to not scream aliasing).
//! Expanded to BlepOsc for Saw, Square (PWM), and Sweep morphing.

use crate::params::OscWave;

pub struct BlepOsc {
    phase: f32,
    inc: f32,
    pw: f32,
    wave: OscWave,
    sweep_pos: f32,
}

impl BlepOsc {
    pub fn new() -> Self {
        Self {
            phase: 0.0,
            inc: 0.0,
            pw: 0.5,
            wave: OscWave::Saw,
            sweep_pos: 0.0,
        }
    }

    pub fn set_freq(&mut self, hz: f32, sr: f32) {
        self.inc = hz / sr;
    }

    pub fn set_wave(&mut self, wave: OscWave) {
        self.wave = wave;
    }

    pub fn set_pw(&mut self, pw: f32) {
        self.pw = pw.clamp(0.01, 0.99);
    }

    pub fn set_sweep(&mut self, pos: f32) {
        self.sweep_pos = pos.clamp(0.0, 1.0);
    }

    fn saw_value(&self, phase: f32) -> f32 {
        let mut v = 2.0 * phase - 1.0; // naive saw
        v -= poly_blep(phase, self.inc); // correct transition at 0/1
        v
    }

    fn pulse_value(&self, phase: f32) -> f32 {
        let mut v = if phase < self.pw { 1.0 } else { -1.0 };
        v -= 2.0 * self.pw - 1.0; // remove DC
        v += 2.0 * poly_blep(phase, self.inc); // step up at 0
        let t = if phase >= self.pw { phase - self.pw } else { phase - self.pw + 1.0 };
        v -= 2.0 * poly_blep(t, self.inc); // step down at pw
        v
    }

    pub fn tick(&mut self) -> f32 {
        let saw = self.saw_value(self.phase);
        let pulse = self.pulse_value(self.phase);

        let out = match self.wave {
            OscWave::Saw => saw,
            OscWave::Square => pulse,
            OscWave::Sweep => saw * (1.0 - self.sweep_pos) + pulse * self.sweep_pos,
        };

        self.phase += self.inc;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        out
    }
}

/// PolyBLEP residual for a single per-sample step `dt`, removing the alias energy
/// at the wave's wrap discontinuity.
fn poly_blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let t = t / dt;
        t + t - t * t - 1.0
    } else if t > 1.0 - dt {
        let t = (t - 1.0) / dt;
        t * t + t + t + 1.0
    } else {
        0.0
    }
}

impl Default for BlepOsc {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saw_mean_near_zero_over_one_period() {
        let sr = 48_000.0;
        let mut s = BlepOsc::new();
        s.set_freq(480.0, sr); // 100 samples/period
        let n = 4_800;
        let sum: f32 = (0..n).map(|_| s.tick()).sum();
        assert!((sum / n as f32).abs() < 0.05, "DC = {}", sum / n as f32);
    }

    #[test]
    fn saw_stays_in_range() {
        let sr = 48_000.0;
        let mut s = BlepOsc::new();
        s.set_freq(440.0, sr);
        for _ in 0..10_000 {
            let v = s.tick();
            assert!(v.abs() <= 1.5, "out of range: {v}");
        }
    }

    #[test]
    fn pulse_mean_near_zero_with_pwm() {
        let sr = 48_000.0;
        let mut s = BlepOsc::new();
        s.set_wave(OscWave::Square);
        s.set_pw(0.3); // 30% duty cycle
        s.set_freq(480.0, sr);
        let n = 4_800;
        let sum: f32 = (0..n).map(|_| s.tick()).sum();
        assert!((sum / n as f32).abs() < 0.05, "PWM DC = {}", sum / n as f32);
    }
}
