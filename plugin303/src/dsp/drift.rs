//! Component drift low-frequency random walk generator.

/// A slow random walk generator simulating thermal fluctuations in resistors and capacitors.
///
/// Uses a Linear Congruential Generator (LCG) to generate pseudo-random noise, which is then
/// smoothed with a 1-pole lowpass filter to produce a continuous random walk.
pub struct DriftGenerator {
    /// Internal LCG state seed.
    state: u32,
    /// Smoothed random walk value.
    value: f32,
}

impl DriftGenerator {
    /// Creates a new drift generator initialized with a seed.
    pub fn new(seed: u32) -> Self {
        Self {
            state: seed,
            value: 0.0,
        }
    }

    /// Advance the drift generator. `cutoff_hz` sets the speed of the drift
    /// (e.g. 0.2 Hz for slow thermal walk). Returns a value in [-1.0, 1.0].
    pub fn tick(&mut self, cutoff_hz: f32, sr: f32) -> f32 {
        // Linear Congruential Generator step
        self.state = self.state.wrapping_mul(1103515245).wrapping_add(12345);
        let noise = ((self.state as f32) / (std::u32::MAX as f32)) * 2.0 - 1.0;

        // Apply 1-pole lowpass filter to create a smooth random walk
        let alpha = (2.0 * std::f32::consts::PI * cutoff_hz / sr).min(1.0);
        self.value += alpha * (noise - self.value);
        self.value
    }
}

impl Default for DriftGenerator {
    fn default() -> Self {
        Self::new(1337)
    }
}
