//! Component drift low-frequency random walk generator.

/// RMS of the walk. Three times the RMS is 1, so the output stays inside
/// [-1, 1] for about 99.7 % of samples: the range the voice's drift depths
/// (cents, cutoff and resonance factors) are written for.
const WALK_RMS: f64 = 1.0 / 3.0;

/// A slow, seeded random walk for component drift: white noise through a
/// one-pole low-pass (an Ornstein-Uhlenbeck process). The input is scaled so
/// the output RMS is [`WALK_RMS`] at every sample rate and cutoff; without the
/// scaling the low-pass leaves an RMS proportional to `sqrt(cutoff / sr)`,
/// about 50x too small at 48 kHz and smaller still at higher rates.
pub struct DriftGenerator {
    /// Internal LCG state seed.
    state: u32,
    /// Low-pass state (the walk).
    lp: f64,
    /// `(cutoff_hz, sr)` that `alpha` and `gain` were computed for.
    coeff_key: (f32, f32),
    /// One-pole coefficient `1 - exp(-2π·cutoff/sr)`.
    alpha: f64,
    /// Input gain that sets the output RMS to `WALK_RMS`.
    gain: f64,
}

impl DriftGenerator {
    /// Creates a new drift generator initialized with a seed.
    pub fn new(seed: u32) -> Self {
        Self {
            state: seed,
            lp: 0.0,
            coeff_key: (0.0, 0.0),
            alpha: 0.0,
            gain: 0.0,
        }
    }

    /// Advance the drift generator. `cutoff_hz` sets the speed of the drift.
    /// Returns a value with RMS 1/3, so roughly in [-1.0, 1.0].
    #[inline]
    pub fn tick(&mut self, cutoff_hz: f32, sr: f32) -> f32 {
        if self.coeff_key != (cutoff_hz, sr) {
            self.coeff_key = (cutoff_hz, sr);
            self.alpha = (1.0 - (-std::f64::consts::TAU * cutoff_hz as f64 / sr as f64).exp())
                .clamp(1e-12, 1.0);
            // Var(lp) = alpha · gain² · Var(white) / (2 - alpha), Var(white) = 1/3.
            self.gain = WALK_RMS * (3.0 * (2.0 - self.alpha) / self.alpha).sqrt();
        }

        // Linear Congruential Generator step for white noise in [-1.0, 1.0]
        self.state = self.state.wrapping_mul(1103515245).wrapping_add(12345);
        let white = (self.state as f64 / u32::MAX as f64) * 2.0 - 1.0;

        self.lp += self.alpha * (self.gain * white - self.lp);
        self.lp as f32
    }
}

impl Default for DriftGenerator {
    fn default() -> Self {
        Self::new(1337)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The drift depths in `Voice::tick` assume a walk in about [-1, 1]. The
    /// unscaled low-pass gave an RMS of 0.02 at 48 kHz and 0.013 at 96 kHz,
    /// so DRIFT at 100 % moved the pitch by a fraction of a cent.
    #[test]
    fn walk_rms_is_one_third_at_every_rate() {
        for sr in [44_100.0f32, 48_000.0, 96_000.0, 192_000.0] {
            let mut g = DriftGenerator::new(1337);
            let n = (120.0 * sr) as usize;
            let mut sum_sq = 0.0f64;
            for _ in 0..n {
                sum_sq += (g.tick(0.15, sr) as f64).powi(2);
            }
            let rms = (sum_sq / n as f64).sqrt();
            assert!(
                (0.25..0.42).contains(&rms),
                "drift walk RMS {rms:.4} at {sr} Hz, want about 1/3"
            );
        }
    }
}
