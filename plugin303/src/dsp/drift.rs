//! Component drift low-frequency random walk generator.

/// A slow random walk generator simulating thermal fluctuations in resistors and capacitors.
///
/// Uses Paul Kellet's cascaded IIR filter method to generate high-quality -3 dB/octave pink noise
/// from a white noise source. A slow lowpass filter is applied to constrain the high-frequency
/// jitter and extract the slow macroscopic wandering drift.
pub struct DriftGenerator {
    /// Internal LCG state seed.
    state: u32,
    b0: f32,
    b1: f32,
    b2: f32,
    b3: f32,
    b4: f32,
    b5: f32,
    b6: f32,
    /// Lowpass smoothing state for drift
    lp: f32,
}

impl DriftGenerator {
    /// Creates a new drift generator initialized with a seed.
    pub fn new(seed: u32) -> Self {
        Self {
            state: seed,
            b0: 0.0,
            b1: 0.0,
            b2: 0.0,
            b3: 0.0,
            b4: 0.0,
            b5: 0.0,
            b6: 0.0,
            lp: 0.0,
        }
    }

    /// Advance the drift generator. `cutoff_hz` sets the speed of the macroscopic drift.
    /// Returns a value in roughly [-1.0, 1.0].
    pub fn tick(&mut self, cutoff_hz: f32, sr: f32) -> f32 {
        // Linear Congruential Generator step for white noise in [-1.0, 1.0]
        self.state = self.state.wrapping_mul(1103515245).wrapping_add(12345);
        let white = ((self.state as f32) / (std::u32::MAX as f32)) * 2.0 - 1.0;

        // Paul Kellet's refined pinking filter
        self.b0 = 0.99886 * self.b0 + white * 0.0555179;
        self.b1 = 0.99332 * self.b1 + white * 0.0750759;
        self.b2 = 0.96900 * self.b2 + white * 0.1538520;
        self.b3 = 0.86650 * self.b3 + white * 0.3104856;
        self.b4 = 0.55000 * self.b4 + white * 0.5329522;
        self.b5 = -0.7616 * self.b5 - white * 0.0168980;

        let pink =
            self.b0 + self.b1 + self.b2 + self.b3 + self.b4 + self.b5 + self.b6 + white * 0.5362;
        self.b6 = white * 0.115926;

        // pink is roughly in [-3.0, 3.0]. Scale it down.
        let scaled_pink = pink * 0.15;

        // Apply a slow 1-pole lowpass filter to extract the slow macroscopic drift
        // while preserving the 1/f nature below the cutoff.
        let alpha = (2.0 * std::f32::consts::PI * cutoff_hz / sr).min(1.0);
        self.lp += alpha * (scaled_pink - self.lp);

        self.lp
    }
}

impl Default for DriftGenerator {
    fn default() -> Self {
        Self::new(1337)
    }
}
