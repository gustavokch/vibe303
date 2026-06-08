//! Stereo-linked levelNorm: feed-forward RMS matching of output level to input level.
//!
//! When enabled, the plugin scales the wet signal so its RMS tracks the input RMS, making the
//! output as loud as the input regardless of the preamp's natural gain (and regardless of the
//! absolute de-norm scale of either backend). It is **stereo-linked**: a single gain, derived from
//! the summed energy of all channels, is applied to both — so the stereo image is preserved.
//!
//! The estimator is **feed-forward** (gain = sqrt(in_ms / wet_ms) from the *pre-norm* wet), not a
//! feedback AGC, so it is unconditionally stable with no risk of oscillation. Zero-alloc: only
//! fixed scalars, safe on the audio thread.

/// Envelope time constant (seconds) for the one-pole RMS trackers and the gain slew.
const TAU_S: f32 = 0.075;
/// Input energy (summed mean-square over channels) below which the gain is held toward unity, so
/// near-silence does not drive the gain up. ~ -60 dBFS.
const ENERGY_FLOOR: f32 = 1.0e-6;
/// Smallest wet energy used in the gain ratio (avoids div-by-zero on a silent wet path).
const WET_EPS: f32 = 1.0e-12;
/// Norm-gain clamp (linear): -80 dB .. +48 dB. The lower bound must be low enough to tame the
/// faithful de-normed output, which carries the preamp's full (~+50 dB) voltage gain.
const MIN_GAIN: f32 = 1.0e-4;
const MAX_GAIN: f32 = 256.0;

/// Stereo-linked RMS-matching gain tracker.
pub struct LevelNorm {
    /// Input mean-square envelope (summed over channels).
    in_ms: f32,
    /// Wet (pre-norm) mean-square envelope (summed over channels).
    wet_ms: f32,
    /// One-pole smoothing coefficient per sample, set in [`prepare`].
    coeff: f32,
    /// Current smoothed norm gain (the value applied to the wet signal).
    gain: f32,
}

impl LevelNorm {
    pub fn new() -> Self {
        // Conservative default coeff for an unprepared instance (overwritten by `prepare`).
        Self {
            in_ms: 0.0,
            wet_ms: 0.0,
            coeff: 1.0,
            gain: 1.0,
        }
    }

    /// Set the per-sample one-pole coefficient from the host sample rate and reset the state.
    pub fn prepare(&mut self, sample_rate: f32) {
        let sr = sample_rate.max(1.0);
        self.coeff = 1.0 - (-1.0 / (TAU_S * sr)).exp();
        self.reset();
    }

    pub fn reset(&mut self) {
        self.in_ms = 0.0;
        self.wet_ms = 0.0;
        self.gain = 1.0;
    }

    /// Advance the envelopes one frame from the summed-square energy of the input and the (pre-norm)
    /// wet across all channels, then update the smoothed norm gain.
    #[inline]
    pub fn update(&mut self, in_sq: f32, wet_sq: f32) {
        self.in_ms += self.coeff * (in_sq - self.in_ms);
        self.wet_ms += self.coeff * (wet_sq - self.wet_ms);

        // Below the input floor, glide the gain back toward unity rather than chasing noise.
        let target = if self.in_ms <= ENERGY_FLOOR {
            1.0
        } else {
            (self.in_ms / self.wet_ms.max(WET_EPS)).sqrt()
        }
        .clamp(MIN_GAIN, MAX_GAIN);

        // Slew the gain itself for extra anti-pumping on top of the envelope smoothing.
        self.gain += self.coeff * (target - self.gain);
    }

    /// The current norm gain to apply to the wet signal this frame.
    #[inline]
    pub fn gain(&self) -> f32 {
        self.gain
    }
}

impl Default for LevelNorm {
    fn default() -> Self {
        Self::new()
    }
}
