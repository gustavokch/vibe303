//! One-pole DC blocker (first-order high-pass) for the final output stage.
//!
//! Operating-point re-stamps (G0 rebuilds on tier/precision/age/gain/bias changes) can leave a small
//! DC offset on the wet path. A standard one-pole DC blocker removes it without touching the
//! passband: `y[n] = x[n] - x[n-1] + R * y[n-1]`, where `R = 1 - 2π·fc/sr` places the high-pass
//! corner at `fc`. Zero-alloc; safe on the audio thread.

/// High-pass corner (Hz). Low enough to leave the audband untouched (≈ −3 dB at 5 Hz).
const FC_HZ: f32 = 5.0;

pub struct DcBlock {
    r: f32,
    x_prev: f32,
    y_prev: f32,
}

impl DcBlock {
    pub fn new() -> Self {
        Self { r: 0.9995, x_prev: 0.0, y_prev: 0.0 }
    }

    /// Set the pole from the sample rate and reset state.
    pub fn prepare(&mut self, sample_rate: f32) {
        let sr = sample_rate.max(1.0);
        // R = 1 - 2π fc / sr, clamped to a stable, effective range.
        self.r = (1.0 - 2.0 * std::f32::consts::PI * FC_HZ / sr).clamp(0.9, 0.99999);
        self.reset();
    }

    pub fn reset(&mut self) {
        self.x_prev = 0.0;
        self.y_prev = 0.0;
    }

    /// Process one sample.
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = x - self.x_prev + self.r * self.y_prev;
        self.x_prev = x;
        self.y_prev = y;
        y
    }
}

impl Default for DcBlock {
    fn default() -> Self {
        Self::new()
    }
}
