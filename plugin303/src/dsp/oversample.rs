//! Zero-alloc polyphase FIR oversampling. `HalfBand` is the primitive: one 2×
//! stage with a Kaiser-windowed half-band sinc. `Os2x` is one long stage;
//! `Os4x` cascades it with a shorter inner stage; `Os8x` cascades three long
//! stages. Coeffs are built once per process; `up`/`down` touch only
//! fixed-size arrays.
//!
//! A stage with `M` odd-tap pairs has FIR length `L = 4M + 1`. `L ≡ 1 (mod 4)`
//! keeps every cascade stage's base-rate group delay an integer.

use std::sync::LazyLock;

/// Odd-tap pairs of the long stage (`Os2x`, `Os4x`'s outer stage): L = 65,
/// passband to about 0.42·fs, stopband from about 0.58·fs.
const OUTER_M: usize = 16;
/// Odd-tap pairs of `Os4x`'s inner stage (2× ↔ 4×): L = 29. Its input is
/// already band-limited to the base band (≤ 0.5·fs), so it only has to reject
/// images and folds at ≥ 1.5·fs. Relative to its own rate that transition is
/// about 3× wider than the long stage's, so 7 pairs at Kaiser β = 9 reach
/// −89 dB above 1.42·fs (the long stage: −91 dB), with passband ripple under
/// 0.001 dB to 0.42·fs.
const INNER_M: usize = 7;

/// Ring buffer capacities (powers of two) for the longest stage: the up path
/// reads `2M` input samples back, the down path `4M + 1` high-rate samples.
const BUF_UP: usize = 64;
const MASK_UP: usize = BUF_UP - 1;
const BUF_DN: usize = 128;
const MASK_DN: usize = BUF_DN - 1;

/// Modified Bessel I0 via series (enough terms for double precision at beta≈8).
fn i0(x: f64) -> f64 {
    let mut term = 1.0;
    let mut sum = 1.0;
    let xx = x * x / 4.0;
    for k in 1..40 {
        term *= xx / (k as f64 * k as f64);
        sum += term;
        if term < 1e-14 * sum {
            break;
        }
    }
    sum
}

fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-12 {
        1.0
    } else {
        let p = std::f64::consts::PI * x;
        p.sin() / p
    }
}

/// Build the length-`N` lowpass: windowed sinc at cutoff `fc` (normalised to the
/// high rate; 0.25 = base Nyquist), Kaiser window `beta`, normalised to unit DC gain.
fn build_lp<const N: usize>(beta: f64, fc: f64) -> [f32; N] {
    let c = (N - 1) as f64 / 2.0;
    let denom = i0(beta);
    let mut h = [0.0f64; N];
    let mut sum = 0.0;
    for i in 0..N {
        let n = i as f64 - c;
        let w = i0(beta * (1.0 - (n / c).powi(2)).max(0.0).sqrt()) / denom;
        h[i] = 2.0 * fc * sinc(2.0 * fc * n) * w;
        sum += h[i];
    }
    let mut out = [0.0f32; N];
    for i in 0..N {
        out[i] = (h[i] / sum) as f32; // unit DC gain
    }
    out
}

/// The half-band lowpass's nonzero taps: (centre tap, odd-offset taps).
///
/// The cutoff is exactly half the high rate's Nyquist, so every even-offset
/// tap but the centre is a zero of the sinc (`|h| < 1e-16`, rounding only).
/// Those taps are dropped: the interpolator's even branch is a pure delay, and
/// both the interpolator's odd branch and the decimator use the `M` odd taps.
fn half_band<const N: usize, const M: usize>(beta: f64) -> (f32, [f32; M]) {
    assert_eq!(N, 4 * M + 1);
    let h = build_lp::<N>(beta, 0.25);
    (h[2 * M], std::array::from_fn(|k| h[2 * k + 1]))
}

static OUTER_TAPS: LazyLock<(f32, [f32; OUTER_M])> =
    LazyLock::new(|| half_band::<{ 4 * OUTER_M + 1 }, OUTER_M>(8.0));
static INNER_TAPS: LazyLock<(f32, [f32; INNER_M])> =
    LazyLock::new(|| half_band::<{ 4 * INNER_M + 1 }, INNER_M>(9.0));

/// One 2× polyphase half-band stage with `M` odd-tap pairs. `up`
/// zero-stuff-interpolates one input sample into two output samples; `down`
/// lowpass-decimates two input samples into one.
pub struct HalfBand<const M: usize> {
    hc: f32,      // centre tap
    ho: [f32; M], // odd-offset taps (M symmetric pairs)
    hist_up: [f32; BUF_UP],
    head_up: usize,
    hist_dn: [f32; BUF_DN],
    head_dn: usize,
}

/// 2× oversampler (base rate ↔ 2×).
pub type Os2x = HalfBand<OUTER_M>;
/// `Os4x`'s inner stage (2× ↔ 4×).
type InnerStage = HalfBand<INNER_M>;

impl<const M: usize> HalfBand<M> {
    /// Round-trip group delay in samples of the stage's lower rate: up and
    /// down are each `(L - 1) / 2` high-rate samples, `(L - 1) / 2 = 2M` in total.
    pub const LATENCY: usize = 2 * M;

    fn with_taps((hc, ho): (f32, [f32; M])) -> Self {
        Self {
            hc,
            ho,
            hist_up: [0.0; BUF_UP],
            head_up: 0,
            hist_dn: [0.0; BUF_DN],
            head_dn: 0,
        }
    }

    /// One input sample -> two high-rate samples. ×2 compensates the zero-stuff loss.
    pub fn up(&mut self, x: f32) -> [f32; 2] {
        self.head_up = (self.head_up + 1) & MASK_UP;
        self.hist_up[self.head_up] = x;
        let head = self.head_up;

        let ye = self.hc * self.hist_up[(head + BUF_UP - M) & MASK_UP];
        let mut yo = 0.0f32;
        for k in 0..M {
            let s_k = self.hist_up[(head + BUF_UP - k) & MASK_UP];
            let s_mirror = self.hist_up[(head + BUF_UP + 1 + k - 2 * M) & MASK_UP];
            yo += self.ho[k] * (s_k + s_mirror);
        }
        [2.0 * ye, 2.0 * yo]
    }

    /// Two high-rate samples -> one output sample (lowpass + decimate by 2). The
    /// two taps are pushed newest-last; the output keeps the even decimation
    /// phase (`xhi[2n]`, = `hist_dn[k+1]`), which makes the up→down delay an
    /// integer `(L-1)/2` output samples instead of a half-sample.
    pub fn down(&mut self, hi: [f32; 2]) -> f32 {
        let p0 = (self.head_dn + 1) & MASK_DN;
        let p1 = (self.head_dn + 2) & MASK_DN;
        self.hist_dn[p0] = hi[0];
        self.hist_dn[p1] = hi[1];
        self.head_dn = p1;
        let head = p1;

        let mut y = self.hc * self.hist_dn[(head + BUF_DN - 1 - 2 * M) & MASK_DN];
        // Odd tap k = 2j + 1 and its mirror 4M - k.
        for j in 0..M {
            let s_a = self.hist_dn[(head + BUF_DN - 2 - 2 * j) & MASK_DN];
            let s_b = self.hist_dn[(head + BUF_DN + 2 * j - 4 * M) & MASK_DN];
            y += self.ho[j] * (s_a + s_b);
        }
        y
    }

    /// Round-trip group delay of this stage (`LATENCY`).
    pub fn latency_samples(&self) -> usize {
        Self::LATENCY
    }
}

impl HalfBand<OUTER_M> {
    pub fn new() -> Self {
        Self::with_taps(*OUTER_TAPS)
    }
}

impl Default for HalfBand<OUTER_M> {
    fn default() -> Self {
        Self::new()
    }
}

impl HalfBand<INNER_M> {
    fn new() -> Self {
        Self::with_taps(*INNER_TAPS)
    }
}

/// 4× = the long 2× stage, then the short inner stage. The inner stage
/// processes the 2×-rate stream sequentially (two `up`/`down` calls per outer
/// sample) — it is one filter on a stream, NOT two parallel branches.
pub struct Os4x {
    a: Os2x,
    b: InnerStage,
    db: InnerStage,
    da: Os2x,
}

impl Os4x {
    /// The outer stage's delay is in base samples; the inner stage's runs at
    /// 2×, so it counts half. 32 + 7 = 39 base samples.
    pub const LATENCY: usize = Os2x::LATENCY + InnerStage::LATENCY / 2;

    pub fn new() -> Self {
        Self {
            a: Os2x::new(),
            b: InnerStage::new(),
            db: InnerStage::new(),
            da: Os2x::new(),
        }
    }
    pub fn up(&mut self, x: f32) -> [f32; 4] {
        let s = self.a.up(x); // 2× stream: s[0], s[1]
        let o0 = self.b.up(s[0]); // each upsampled to 4×, in order
        let o1 = self.b.up(s[1]);
        [o0[0], o0[1], o1[0], o1[1]]
    }
    pub fn down(&mut self, hi: [f32; 4]) -> f32 {
        let s0 = self.db.down([hi[0], hi[1]]); // 4× -> 2×, sequential pairs
        let s1 = self.db.down([hi[2], hi[3]]);
        self.da.down([s0, s1]) // 2× -> base
    }
    pub fn latency_samples(&self) -> usize {
        Self::LATENCY
    }
}

impl Default for Os4x {
    fn default() -> Self {
        Self::new()
    }
}

/// 8× = three long 2× stages in series (each inner stage processes its input
/// stream sequentially, like `Os4x`).
///
/// NOT on any current ship path (Draft, Low and High run 2×, Best 4×).
/// Retained + exercised by `tests/ladder_alias.rs` as the 8× alias-rejection
/// reference and as headroom for a future 8× tier.
pub struct Os8x {
    a: Os2x,
    b: Os2x,
    c: Os2x,
    dc: Os2x,
    db: Os2x,
    da: Os2x,
}

impl Os8x {
    pub fn new() -> Self {
        Self {
            a: Os2x::new(),
            b: Os2x::new(),
            c: Os2x::new(),
            dc: Os2x::new(),
            db: Os2x::new(),
            da: Os2x::new(),
        }
    }
    pub fn up(&mut self, x: f32) -> [f32; 8] {
        let s = self.a.up(x); // 2×
        let mut out = [0.0f32; 8];
        let mut idx = 0;
        for &si in &s {
            let t = self.b.up(si); // 4×
            for &ti in &t {
                let u = self.c.up(ti); // 8×
                out[idx] = u[0];
                out[idx + 1] = u[1];
                idx += 2;
            }
        }
        out
    }
    pub fn down(&mut self, hi: [f32; 8]) -> f32 {
        let mut q4 = [0.0f32; 4];
        for j in 0..4 {
            q4[j] = self.dc.down([hi[2 * j], hi[2 * j + 1]]); // 8× -> 4×
        }
        let s0 = self.db.down([q4[0], q4[1]]); // 4× -> 2×
        let s1 = self.db.down([q4[2], q4[3]]);
        self.da.down([s0, s1]) // 2× -> base
    }
    pub fn latency_samples(&self) -> usize {
        Os2x::LATENCY + Os2x::LATENCY / 2 + Os2x::LATENCY / 4
    }
}

impl Default for Os8x {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(freq: f32, sr: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / sr).sin())
            .collect()
    }

    #[test]
    fn up_then_down_roundtrips_band_limited_tone() {
        let sr = 48_000.0;
        let mut os = Os2x::new();
        let x = tone(1_000.0, sr, 8_192);
        let mut y = vec![0.0; x.len()];
        for i in 0..x.len() {
            let hi = os.up(x[i]);
            y[i] = os.down(hi);
        }
        let lat = os.latency_samples();
        let a = &x[..x.len() - lat];
        let b = &y[lat..];
        let err: f32 = a.iter().zip(b).map(|(p, q)| (p - q) * (p - q)).sum();
        let sig: f32 = a.iter().map(|p| p * p).sum();
        assert!(
            10.0 * (err / sig).log10() < -40.0,
            "roundtrip ESR {} dB too high",
            10.0 * (err / sig).log10()
        );
    }

    #[test]
    fn upsampler_attenuates_above_base_nyquist() {
        let sr = 96_000.0;
        let mut os = Os2x::new();
        let hot = tone(36_000.0, sr, 8_192); // above base Nyquist (24 kHz)
        let mut out = vec![0.0; hot.len() / 2];
        for i in 0..out.len() {
            out[i] = os.down([hot[2 * i], hot[2 * i + 1]]);
        }
        let e: f32 = out.iter().map(|v| v * v).sum::<f32>() / out.len() as f32;
        assert!(e.sqrt() < 0.05, "alias not attenuated: rms {}", e.sqrt());
    }

    #[test]
    fn os4x_roundtrips() {
        let sr = 48_000.0;
        let mut os = Os4x::new();
        let x = tone(1_000.0, sr, 8_192);
        let mut y = vec![0.0; x.len()];
        for i in 0..x.len() {
            let hi = os.up(x[i]);
            y[i] = os.down(hi);
        }
        // The reported latency must be the true alignment (best over a small window).
        let report = os.latency_samples();
        let esr = |lat: usize| {
            let a = &x[..x.len() - lat];
            let b = &y[lat..];
            let err: f32 = a.iter().zip(b).map(|(p, q)| (p - q) * (p - q)).sum();
            let sig: f32 = a.iter().map(|p| p * p).sum();
            10.0 * (err / sig).log10()
        };
        let best = (report.saturating_sub(4)..=report + 4)
            .min_by(|&i, &j| esr(i).total_cmp(&esr(j)))
            .unwrap();
        assert_eq!(best, report, "latency_samples() mis-reports alignment");
        assert!(
            esr(report) < -35.0,
            "os4x roundtrip ESR {:.2} dB too high",
            esr(report)
        );
    }

    /// `HalfBand` skips the even-offset taps, so both filters must stay half-band.
    #[test]
    fn lowpass_is_half_band() {
        fn check<const N: usize>(beta: f64) {
            let h = build_lp::<N>(beta, 0.25);
            let c = (N - 1) / 2;
            for i in (0..N).step_by(2).filter(|&i| i != c) {
                assert!(h[i].abs() < 1e-12, "N={N}: even tap {i} = {:e}", h[i]);
            }
        }
        check::<{ 4 * OUTER_M + 1 }>(8.0);
        check::<{ 4 * INNER_M + 1 }>(9.0);
    }
}
