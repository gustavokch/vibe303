//! DPR — Dual Phase Rotator.
//!
//! A broadcast-style phase rotator: a cascade of first-order all-pass sections that redistributes
//! the energy of an asymmetric waveform about the zero axis (reshaping the attack / peak symmetry)
//! WITHOUT changing the magnitude spectrum. The classic CBS/Orban "phase asymmetry eliminator"
//! topology is four cascaded 1st-order all-pass stages tuned to the low mids (~200 Hz); the effect
//! is linear (adds no harmonics) so it is a near "free lunch" reshaping of the time-domain envelope.
//!
//! It sits on the output path between the levelNorm gain and the output gain, and is a bit-exact
//! pass-through while `on` is false. The amount knob sweeps the shared corner frequency `fc` on a
//! log scale (≈80 Hz → ≈500 Hz, default 0.5 → ≈200 Hz): low amount keeps the rotation toward the
//! sub-bass (subtle), high amount lifts it into the low mids for a stronger, more audible reshaping.
//!
//! The effect is intrinsically signal-dependent — asymmetric inputs reshape the most; a perfectly
//! symmetric input (no even-harmonic content) is barely changed (and can even gain peak-to-peak).

/// Number of cascaded all-pass stages — the canonical broadcast phase-rotator count.
const STAGES: usize = 4;
/// Amount → corner-frequency map endpoints (Hz). Default amount 0.5 lands at ~200 Hz (Orban).
const FC_MIN_HZ: f32 = 80.0;
const FC_MAX_HZ: f32 = 500.0;

/// One first-order all-pass, single-state Gray–Markel lattice form.
///
/// `H(z) = (a + z^-1) / (1 + a*z^-1)` with one delay element:
/// `y = a*x + s ; s = x - a*y`. Magnitude is exactly 0 dB at every frequency; only phase changes,
/// passing −90° at the corner `fc`.
#[derive(Clone, Copy, Debug, Default)]
struct Allpass1 {
    a: f32,
    s: f32,
}

impl Allpass1 {
    #[inline]
    fn set_fc(&mut self, fc: f32, fs: f32) {
        // Bilinear pole placement: a = (tan(pi*fc/fs) - 1) / (tan(pi*fc/fs) + 1), in (-1, 0).
        let t = (std::f32::consts::PI * fc / fs).tan();
        self.a = (t - 1.0) / (t + 1.0);
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let y = self.a * x + self.s;
        self.s = x - self.a * y;
        y
    }

    #[inline]
    fn reset(&mut self) {
        self.s = 0.0;
    }
}

/// Cascade of [`STAGES`] all-pass sections sharing one corner frequency.
#[derive(Clone, Debug)]
pub struct Dpr {
    stages: [Allpass1; STAGES],
    on: bool,
    /// Cached corner frequency so coefficients are recomputed only when it actually changes.
    fc: f32,
}

impl Default for Dpr {
    fn default() -> Self {
        Self::new()
    }
}

impl Dpr {
    pub fn new() -> Self {
        Self {
            stages: [Allpass1::default(); STAGES],
            on: false,
            fc: f32::NAN, // force a coeff recompute on the first `set`
        }
    }

    /// Map a 0..1 amount to the shared all-pass corner frequency (log scale, `FC_MIN..FC_MAX`).
    #[inline]
    fn amount_to_fc(amount: f32) -> f32 {
        let a = amount.clamp(0.0, 1.0);
        FC_MIN_HZ * (FC_MAX_HZ / FC_MIN_HZ).powf(a)
    }

    /// Block-rate config. `on == false` makes [`Dpr::process`] a bit-exact pass-through. The all-pass
    /// coefficients are recomputed only when the resolved `fc` changes, so a static knob costs nothing.
    pub fn set(&mut self, on: bool, amount: f32, fs: f32) {
        self.on = on;
        let fc = Self::amount_to_fc(amount);
        if fc != self.fc {
            for st in &mut self.stages {
                st.set_fc(fc, fs);
            }
            self.fc = fc;
        }
    }

    /// One sample through the cascade. Bypass (identity) while `on` is false.
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        if !self.on {
            return x;
        }
        let mut y = x;
        for st in &mut self.stages {
            y = st.process(y);
        }
        y
    }

    /// Zero the delay state without touching coefficients (host deactivate→activate / reset).
    pub fn reset(&mut self) {
        for st in &mut self.stages {
            st.reset();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `on == false` is an exact identity, sample for sample.
    #[test]
    fn bypass_is_bit_exact() {
        let mut dpr = Dpr::new();
        dpr.set(false, 0.5, 48_000.0);
        for i in 0..1024 {
            let x = (i as f32 * 0.013).sin() * 0.7;
            assert_eq!(dpr.process(x), x);
        }
    }

    /// An all-pass cascade preserves signal energy (magnitude-flat): output RMS ≈ input RMS over a
    /// long broadband run, after a short settle. Only the phase/waveform shape changes.
    #[test]
    fn allpass_preserves_energy() {
        let mut dpr = Dpr::new();
        dpr.set(true, 0.5, 48_000.0);
        // Deterministic pseudo-noise (xorshift) so the test needs no rng dep.
        let mut state: u32 = 0x1234_5678;
        let mut rng = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state as f32 / u32::MAX as f32) * 2.0 - 1.0
        };
        let n = 200_000;
        let warm = 2_000;
        let (mut ein, mut eout) = (0.0f64, 0.0f64);
        for i in 0..n {
            let x = rng() * 0.5;
            let y = dpr.process(x);
            if i >= warm {
                ein += (x as f64) * (x as f64);
                eout += (y as f64) * (y as f64);
            }
        }
        let ratio = (eout / ein).sqrt();
        assert!((ratio - 1.0).abs() < 1e-3, "all-pass changed energy: rms ratio {ratio}");
    }

    /// On an asymmetric input the cascade pulls the positive/negative peak magnitudes toward each
    /// other (the whole point of a phase rotator). Half-wave-rectified sine: big positive lobes, ~0
    /// negative — DPR should grow a negative excursion, shrinking the |pos|/|neg| ratio.
    #[test]
    fn reduces_peak_asymmetry() {
        let fs = 48_000.0f32;
        let f0 = 110.0f32;
        let n = 48_000usize;
        let warm = 4_000usize;

        // Baseline asymmetry of the raw signal.
        let mut raw_pos = 0.0f32;
        let mut raw_neg = 0.0f32;
        let mut dpr = Dpr::new();
        dpr.set(true, 0.6, fs);
        let mut pos = 0.0f32;
        let mut neg = 0.0f32;
        for i in 0..n {
            let s = (2.0 * std::f32::consts::PI * f0 * i as f32 / fs).sin();
            let x = if s > 0.0 { s } else { 0.0 }; // half-wave rectified: asymmetric
            let y = dpr.process(x);
            if i >= warm {
                raw_pos = raw_pos.max(x);
                raw_neg = raw_neg.max(-x);
                pos = pos.max(y);
                neg = neg.max(-y);
            }
        }
        let raw_ratio = raw_pos / raw_neg.max(1e-6); // huge (neg ~ 0)
        let out_ratio = pos / neg.max(1e-6);
        assert!(
            out_ratio < raw_ratio,
            "DPR did not reduce asymmetry: raw {raw_ratio} -> out {out_ratio}"
        );
    }
}
