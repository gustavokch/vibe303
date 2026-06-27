//! Oscillators. M1: a minimal polyBLEP saw (good enough to not scream aliasing).
//! Circuit-faithful saw/square fitted to the vco303 model comes in M3.

pub struct Saw {
    phase: f32,
    inc: f32,
}

impl Saw {
    pub fn new() -> Self {
        Self { phase: 0.0, inc: 0.0 }
    }

    pub fn set_freq(&mut self, hz: f32, sr: f32) {
        self.inc = hz / sr;
    }

    pub fn tick(&mut self) -> f32 {
        let mut v = 2.0 * self.phase - 1.0; // naive saw
        v -= poly_blep(self.phase, self.inc); // correct the discontinuity
        self.phase += self.inc;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        v
    }
}

/// PolyBLEP residual for a single per-sample step `dt`, removing the alias energy
/// at the saw's wrap discontinuity.
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

impl Default for Saw {
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
        let mut s = Saw::new();
        s.set_freq(480.0, sr); // 100 samples/period
        let n = 4_800;
        let sum: f32 = (0..n).map(|_| s.tick()).sum();
        assert!((sum / n as f32).abs() < 0.05, "DC = {}", sum / n as f32);
    }

    #[test]
    fn saw_stays_in_range() {
        let sr = 48_000.0;
        let mut s = Saw::new();
        s.set_freq(440.0, sr);
        for _ in 0..10_000 {
            let v = s.tick();
            assert!(v.abs() <= 1.5, "out of range: {v}");
        }
    }
}
