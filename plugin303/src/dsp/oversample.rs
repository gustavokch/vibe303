//! Zero-alloc polyphase FIR oversampling. `Os2x` is the primitive (Kaiser-windowed
//! sinc, base-Nyquist cutoff); `Os4x`/`Os8x` cascade it. Coeffs are built in `new()`;
//! `up`/`down` touch only fixed-size arrays.
//!
//! FIR length is odd (`L = 2P-1`) so the linear-phase group delay `(L-1)/2` is an
//! integer; the up→down round trip then has an integer base-rate latency.

/// Half the FIR length (taps per polyphase branch on the up path).
const P: usize = 33;
/// Full FIR length — odd and `≡ 1 (mod 4)`, so every cascade stage's base-rate group
/// delay `(L-1)/2^stage` stays an integer (L=65 → 32 / 16 / 8 for 2×/4×/8×).
const L: usize = 2 * P - 1; // 65

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

/// Build the length-`L` lowpass: windowed sinc at cutoff `fc` (normalised to the
/// high rate; 0.25 = base Nyquist), Kaiser window `beta`, normalised to unit DC gain.
fn build_lp(beta: f64, fc: f64) -> [f32; L] {
    let c = (L - 1) as f64 / 2.0;
    let denom = i0(beta);
    let mut h = [0.0f64; L];
    let mut sum = 0.0;
    for i in 0..L {
        let n = i as f64 - c;
        let w = i0(beta * (1.0 - (n / c).powi(2)).max(0.0).sqrt()) / denom;
        h[i] = 2.0 * fc * sinc(2.0 * fc * n) * w;
        sum += h[i];
    }
    let mut out = [0.0f32; L];
    for i in 0..L {
        out[i] = (h[i] / sum) as f32; // unit DC gain
    }
    out
}

/// 2× polyphase oversampler. `up` zero-stuff-interpolates one base sample into two
/// high-rate samples; `down` lowpass-decimates two high-rate samples into one.
pub struct Os2x {
    he: [f32; P], // even polyphase branch (33 taps)
    ho: [f32; P], // odd polyphase branch (32 taps, last padded 0)
    hist_up: [f32; P],
    h: [f32; L],
    hist_dn: [f32; L + 1], // +1: decimator output reads the even phase (index 2n)
}

impl Os2x {
    pub fn new() -> Self {
        let h = build_lp(8.0, 0.25);
        let mut he = [0.0f32; P];
        let mut ho = [0.0f32; P];
        for k in 0..P {
            he[k] = h[2 * k]; // indices 0,2,..,62
            ho[k] = if 2 * k + 1 < L { h[2 * k + 1] } else { 0.0 }; // 1,3,..,61
        }
        Self {
            he,
            ho,
            hist_up: [0.0; P],
            h,
            hist_dn: [0.0; L + 1],
        }
    }

    /// One base sample -> two high-rate samples. ×2 compensates the zero-stuff loss.
    pub fn up(&mut self, x: f32) -> [f32; 2] {
        self.hist_up.copy_within(0..P - 1, 1);
        self.hist_up[0] = x;
        let mut ye = 0.0f32;
        let mut yo = 0.0f32;
        for k in 0..P {
            ye += self.he[k] * self.hist_up[k];
            yo += self.ho[k] * self.hist_up[k];
        }
        [2.0 * ye, 2.0 * yo]
    }

    /// Two high-rate samples -> one base sample (lowpass + decimate by 2). The two
    /// taps are pushed newest-last; the output keeps the even decimation phase
    /// (`xhi[2n]`, = `hist_dn[k+1]`), which makes the up→down delay an integer
    /// `(L-1)/2` base samples instead of a half-sample.
    pub fn down(&mut self, hi: [f32; 2]) -> f32 {
        self.hist_dn.copy_within(0..L, 1);
        self.hist_dn[0] = hi[0];
        self.hist_dn.copy_within(0..L, 1);
        self.hist_dn[0] = hi[1];
        let mut y = 0.0f32;
        for k in 0..L {
            y += self.h[k] * self.hist_dn[k + 1];
        }
        y
    }

    /// Base-rate round-trip group delay of one 2× stage.
    pub fn latency_samples(&self) -> usize {
        (L - 1) / 2 // 32 base samples (up + down each (L-1)/2 hi = (L-1)/2 base total)
    }
}

impl Default for Os2x {
    fn default() -> Self {
        Self::new()
    }
}

/// 4× = two 2× stages in series. The inner stage processes the 2×-rate stream
/// sequentially (two `up`/`down` calls per outer sample) — it is one filter on a
/// stream, NOT two parallel branches.
pub struct Os4x {
    a: Os2x,
    b: Os2x,
    db: Os2x,
    da: Os2x,
}

impl Os4x {
    pub fn new() -> Self {
        Self {
            a: Os2x::new(),
            b: Os2x::new(),
            db: Os2x::new(),
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
        // outer 2× stage = (L-1)/2 base; inner 2× stage runs at 2× so its (L-1)/2
        // hi-rate delay is (L-1)/4 base. Both integers because L ≡ 1 (mod 4).
        (L - 1) / 2 + (L - 1) / 4
    }
}

impl Default for Os4x {
    fn default() -> Self {
        Self::new()
    }
}

/// 8× = three 2× stages in series (each inner stage processes its input stream
/// sequentially, like `Os4x`).
///
/// NOT on any current ship path — F5 fixed OS per tier (High 2×, Ultra 4×) and
/// dropped the OS-switch param. Retained + exercised by `tests/ladder_alias.rs`
/// as the 8× alias-rejection reference and as headroom for a future 8× tier.
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
        (L - 1) / 2 + (L - 1) / 4 + (L - 1) / 8
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
}
