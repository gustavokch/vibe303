//! Steep linear-phase FIR resampler for the WDF host path (`!legacy_resample`, `os > 1`).
//!
//! Replaces the legacy `prev_in` linear-interpolation upsample + 4th-order Butterworth decimator,
//! which together cost ≈6 dB @20k at 48k host and ≈9 dB at 44.1k (the linear-interp upsample alone
//! was ≈4 dB — see `tests/fr_probe.rs`). Both legacy stages are gentle; this module instead uses
//! windowed-sinc (Kaiser) polyphase FIRs with a passband flat to ~20 kHz and a steep skirt that
//! still rejects the decimation images above host-Nyquist at 44.1/48 kHz.
//!
//! Cost: a fixed group-delay latency (`latency_host`), reported to the host and used to delay the
//! dry path (the legacy path was ~0 latency / 0-PDC). Two prototypes:
//!   - upsampler (1→os): relaxed transition (the first image of a 20 kHz tone sits far above it),
//!     `UP_HALF_HOST` host-samples of half-width.
//!   - decimator (os→1): steep transition (at 44.1k the fold point 22.05 kHz is only ~2 kHz above
//!     20 kHz), `DN_HALF_HOST` host-samples of half-width.
//!
//! RT-safe: all taps + ring buffers allocated once in [`FirResampler::new`]; `upsample`/`decimate`
//! are alloc-free. Tap tables are `Arc`-shared so cloning a `WdfModel` (e.g. `measure_gain`) is a
//! pointer bump, not a deep copy.

use std::sync::Arc;

/// Max internal oversampling factor supported (tiers use os ∈ {2, 4}). Bounds the stack buffer in
/// [`FirResampler::upsample`].
pub const MAX_OS: usize = 4;

/// Upsampler prototype half-width in HOST samples (taps = `2*UP_HALF_HOST*os + 1`). Relaxed: the
/// nearest image of a 20 kHz tone is ~24 kHz, a wide transition, so few taps suffice.
const UP_HALF_HOST: usize = 16;
/// Decimator prototype half-width in HOST samples (taps = `2*DN_HALF_HOST*os + 1`). Steep: at 44.1k
/// the passband edge (20 kHz) and the fold point (22.05 kHz) are only ~2 kHz apart.
const DN_HALF_HOST: usize = 64;
/// Kaiser β for ~96 dB stopband (β = 0.1102·(A−8.7), A=96).
const KAISER_BETA: f64 = 9.62;

/// Zeroth-order modified Bessel function I₀, series form (sufficient for Kaiser window design).
fn bessel_i0(x: f64) -> f64 {
    let mut sum = 1.0;
    let mut term = 1.0;
    let half_x = x * 0.5;
    for k in 1..40 {
        term *= (half_x / k as f64).powi(2);
        sum += term;
        if term < 1e-12 * sum {
            break;
        }
    }
    sum
}

/// Build a Kaiser-windowed sinc lowpass of `2*half+1` taps, normalized-frequency cutoff `fc`
/// (cycles/sample), scaled by `gain`. Symmetric → linear phase, group delay `half` samples.
fn windowed_sinc(half: usize, fc: f64, beta: f64, gain: f64) -> Vec<f64> {
    let ntap = 2 * half + 1;
    let two_fc = 2.0 * fc;
    let i0_beta = bessel_i0(beta);
    let mut h = vec![0.0f64; ntap];
    let mut sum = 0.0;
    for (i, hi) in h.iter_mut().enumerate() {
        let m = i as f64 - half as f64;
        let sinc = if m == 0.0 {
            two_fc
        } else {
            (std::f64::consts::PI * two_fc * m).sin() / (std::f64::consts::PI * m)
        };
        // Kaiser window.
        let r = m / half as f64;
        let w = bessel_i0(beta * (1.0 - r * r).max(0.0).sqrt()) / i0_beta;
        *hi = sinc * w;
        sum += *hi;
    }
    // Normalize to unity DC gain, then apply the requested gain.
    let norm = gain / sum;
    for hi in h.iter_mut() {
        *hi *= norm;
    }
    h
}

/// Steep linear-phase polyphase upsampler (1→os) + FIR decimator (os→1).
#[derive(Clone)]
pub struct FirResampler {
    os: usize,
    // --- upsampler (polyphase) ---
    up_taps: Arc<[f64]>, // length 2*up_half+1; gain os
    up_ring: Vec<f64>, // past host inputs, most-recent at `up_pos`; length = taps-per-phase
    up_pos: usize,     // ring write cursor
    // --- decimator ---
    dn_taps: Arc<[f64]>, // length 2*dn_half+1; unity DC gain
    dn_half: usize,
    dn_ring: Vec<f64>, // last `ntap` internal samples (circular)
    dn_pos: usize,
    lat_host: usize,
}

impl FirResampler {
    /// Build for internal factor `os` (≥2). Cutoffs sit at host-Nyquist (`0.5/os` of the internal
    /// rate), so images above host-Nyquist are rejected and the audio passband stays flat.
    pub fn new(os: usize) -> Self {
        assert!((2..=MAX_OS).contains(&os), "FirResampler os out of range: {os}");
        let fc = 0.5 / os as f64; // host Nyquist, normalized to the internal rate
        let up_half = UP_HALF_HOST * os;
        let dn_half = DN_HALF_HOST * os;
        let up_taps = windowed_sinc(up_half, fc, KAISER_BETA, os as f64);
        let dn_taps = windowed_sinc(dn_half, fc, KAISER_BETA, 1.0);
        // Taps-per-phase for the upsampler ring (host inputs touched by one output): ceil(ntap/os).
        let up_tpp = (up_taps.len() + os - 1) / os;
        // Total latency in host samples = (up group delay + dn group delay) at the internal rate,
        // divided by os. Both prototypes are symmetric (linear phase) with group delay = half.
        let lat_host = (up_half + dn_half) / os;
        Self {
            os,
            up_taps: up_taps.into(),
            up_ring: vec![0.0; up_tpp],
            up_pos: 0,
            dn_taps: dn_taps.into(),
            dn_half,
            // Length 2·dn_half + os: holds the full symmetric tap span (2·dn_half+1) plus the (os−1)
            // newer subsamples of the current frame, so the output can be centered on the host-grid
            // (phase-0) subsample — keeping the up/down delays phase-aligned and the latency integer.
            dn_ring: vec![0.0; 2 * dn_half + os],
            dn_pos: 0,
            lat_host,
        }
    }

    /// Reported group-delay latency, in HOST samples.
    pub fn latency_host(&self) -> usize {
        self.lat_host
    }

    pub fn reset(&mut self) {
        self.up_ring.iter_mut().for_each(|v| *v = 0.0);
        self.dn_ring.iter_mut().for_each(|v| *v = 0.0);
        self.up_pos = 0;
        self.dn_pos = 0;
    }

    /// Push one host input sample; write its `os` interpolated internal samples into `out[..os]`.
    /// Polyphase: phase `p` output = `sum_j up_taps[p + j*os] * x[k-j]`.
    #[inline]
    pub fn upsample(&mut self, x: f64, out: &mut [f64; MAX_OS]) {
        // Advance the host-input ring (newest at up_pos).
        self.up_ring[self.up_pos] = x;
        let len = self.up_ring.len();
        for (p, slot) in out.iter_mut().enumerate().take(self.os) {
            let mut acc = 0.0;
            let mut tap = p;
            let mut j = 0;
            while tap < self.up_taps.len() {
                // x[k-j] lives `j` slots back from up_pos in the ring.
                let idx = (self.up_pos + len - j) % len;
                acc += self.up_taps[tap] * self.up_ring[idx];
                tap += self.os;
                j += 1;
            }
            *slot = acc;
        }
        self.up_pos = (self.up_pos + 1) % len;
    }

    /// Feed the `os` internal output samples for one host frame; return the single decimated host
    /// output. The FIR runs at the internal rate over a circular history; we evaluate it once per
    /// host frame (phase-aligned to the last subsample), which is the standard decimate-by-os.
    #[inline]
    pub fn decimate(&mut self, internal: &[f64]) -> f64 {
        let len = self.dn_ring.len();
        for &u in internal.iter().take(self.os) {
            self.dn_ring[self.dn_pos] = u;
            self.dn_pos = (self.dn_pos + 1) % len;
        }
        // Center the symmetric FIR on the host-GRID (phase-0) subsample — the first one pushed this
        // frame — delayed by `dn_half` so all ±dn_half taps are present. dn_pos points one past the
        // newest sample; the newest phase-0 subsample is `os` back from there.
        let p0 = (self.dn_pos + len - self.os) % len; // newest host-grid subsample
        let center = (p0 + len - self.dn_half) % len; // grid point dn_half in the past
        let mut acc = 0.0;
        // taps t=0..2*dn_half map to ring positions center-dn_half .. center+dn_half (= p0).
        let base = (center + len - self.dn_half) % len;
        let mut idx = base;
        for &ht in self.dn_taps.iter() {
            acc += ht * self.dn_ring[idx];
            idx += 1;
            if idx == len {
                idx = 0;
            }
        }
        acc
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Up→down with NO model in between must reconstruct the input (unity gain) after the reported
    /// latency, for each supported os. This pins the latency value and the polyphase/decimate
    /// alignment. Low-frequency tone (well inside the flat passband) so the FIRs pass it cleanly.
    #[test]
    fn upsample_then_downsample_is_unity_after_latency() {
        for os in 2..=MAX_OS {
            let mut r = FirResampler::new(os);
            let lat = r.latency_host();
            let n = 4096usize;
            let f = 0.01; // cycles/host-sample — deep in the passband
            let mut up = [0.0f64; MAX_OS];
            let mut out = vec![0.0f64; n];
            for (k, o) in out.iter_mut().enumerate() {
                let x = (std::f64::consts::TAU * f * k as f64).sin();
                r.upsample(x, &mut up);
                *o = r.decimate(&up[..os]);
            }
            // Compare a settled window of output against the input delayed by `lat`.
            let mut max_err = 0.0f64;
            for k in (lat + 512)..(n - 16) {
                let want = (std::f64::consts::TAU * f * (k - lat) as f64).sin();
                max_err = max_err.max((out[k] - want).abs());
            }
            assert!(
                max_err < 2e-3,
                "os={os}: reconstruction err {max_err:.2e} exceeds 2e-3 (lat={lat})"
            );
        }
    }

    /// DC must pass at unity (sum-normalized prototypes).
    #[test]
    fn dc_unity_gain() {
        for os in 2..=MAX_OS {
            let mut r = FirResampler::new(os);
            let mut up = [0.0f64; MAX_OS];
            let mut last = 0.0;
            for _ in 0..2048 {
                r.upsample(1.0, &mut up);
                last = r.decimate(&up[..os]);
            }
            assert!((last - 1.0).abs() < 1e-6, "os={os}: DC gain {last} != 1");
        }
    }
}
