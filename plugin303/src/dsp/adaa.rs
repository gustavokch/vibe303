//! 1st-order antiderivative anti-aliasing for static nonlinearities (Parker et al.).
//! ADAA1: `y = (F(x) - F(x₁)) / (x - x₁)` with `F = ∫f`; falls back to `f` at the
//! midpoint when the step is tiny (removable singularity).

/// ADAA1 wrapper around `tanh`. `F(x) = ln(cosh x)`, evaluated in a numerically
/// stable form: `|x| - ln2 + ln(1 + e^(-2|x|))`.
pub struct Adaa1Tanh {
    x1: f32,
    f1: f32,
}

fn ln_cosh(x: f32) -> f32 {
    let a = x.abs();
    a + (-2.0 * a).exp().ln_1p() - std::f32::consts::LN_2
}

impl Adaa1Tanh {
    pub fn new() -> Self {
        Self { x1: 0.0, f1: ln_cosh(0.0) }
    }

    pub fn reset(&mut self) {
        self.x1 = 0.0;
        self.f1 = ln_cosh(0.0);
    }

    pub fn tick(&mut self, x: f32) -> f32 {
        let f = ln_cosh(x);
        let dx = x - self.x1;
        let y = if dx.abs() < 1e-5 {
            (0.5 * (x + self.x1)).tanh()
        } else {
            (f - self.f1) / dx
        };
        self.x1 = x;
        self.f1 = f;
        y
    }
}

impl Default for Adaa1Tanh {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    /// In-place iterative radix-2 Cooley-Tukey FFT (test-only; allocation OK).
    fn fft(re: &mut [f64], im: &mut [f64]) {
        let n = re.len();
        let mut j = 0;
        for i in 1..n {
            let mut bit = n >> 1;
            while j & bit != 0 {
                j ^= bit;
                bit >>= 1;
            }
            j ^= bit;
            if i < j {
                re.swap(i, j);
                im.swap(i, j);
            }
        }
        let mut len = 2;
        while len <= n {
            let ang = -2.0 * PI / len as f64;
            let (wr, wi) = (ang.cos(), ang.sin());
            let mut i = 0;
            while i < n {
                let (mut cr, mut ci) = (1.0f64, 0.0f64);
                for k in 0..len / 2 {
                    let a = i + k;
                    let b = i + k + len / 2;
                    let tr = cr * re[b] - ci * im[b];
                    let ti = cr * im[b] + ci * re[b];
                    re[b] = re[a] - tr;
                    im[b] = im[a] - ti;
                    re[a] += tr;
                    im[a] += ti;
                    let ncr = cr * wr - ci * wi;
                    ci = cr * wi + ci * wr;
                    cr = ncr;
                }
                i += len;
            }
            len <<= 1;
        }
    }

    /// Spectral energy in bins that are NOT a true (below-Nyquist) harmonic of `f0` —
    /// i.e. the aliased fold-down energy.
    fn alias_energy(buf: &[f32], sr: f32, f0: f32) -> f64 {
        let n = buf.len();
        let mut re: Vec<f64> = buf.iter().map(|&v| v as f64).collect();
        let mut im = vec![0.0f64; n];
        fft(&mut re, &mut im);
        let df = sr / n as f32;
        let mut alias = 0.0;
        for i in 1..n / 2 {
            let f = i as f32 * df;
            // distance to the nearest integer harmonic of f0 below Nyquist
            let m = (f / f0).round().max(1.0);
            let nearest = m * f0;
            let legit = nearest < sr * 0.5 && (f - nearest).abs() < 2.0 * df;
            if !legit {
                alias += re[i] * re[i] + im[i] * im[i];
            }
        }
        alias
    }

    #[test]
    fn adaa_reduces_aliasing_vs_naive_tanh() {
        // A hot tone near Nyquist/2 is the worst case for a memoryless `tanh` — its
        // high harmonics fold deep into band. ADAA1 (no oversampling) suppresses them:
        // ~-4 dB alias energy here. (f0 chosen NOT to divide sr, else every harmonic
        // lands on-grid and the off-grid metric degenerates.)
        let sr = 48_000.0;
        let f0 = 11_000.0;
        let n = 16_384;
        let drive = 5.0;
        let inp = |i: usize| drive * (2.0 * std::f32::consts::PI * f0 * i as f32 / sr).sin();
        let naive: Vec<f32> = (0..n).map(|i| inp(i).tanh()).collect();
        let mut a = Adaa1Tanh::new();
        let adaa: Vec<f32> = (0..n).map(|i| a.tick(inp(i))).collect();
        let an = alias_energy(&naive, sr, f0);
        let aa = alias_energy(&adaa, sr, f0);
        assert!(
            aa < 0.5 * an,
            "ADAA did not cut aliasing: naive {an:.3e}, adaa {aa:.3e}"
        );
    }

    #[test]
    fn adaa_tracks_tanh_at_low_frequency() {
        // At a slow input ADAA1 must match plain tanh closely (no AA needed).
        let sr = 48_000.0;
        let f0 = 50.0;
        let mut a = Adaa1Tanh::new();
        let mut max_err = 0.0f32;
        for i in 0..4_000 {
            let x = 3.0 * (2.0 * std::f32::consts::PI * f0 * i as f32 / sr).sin();
            let y = a.tick(x);
            max_err = max_err.max((y - x.tanh()).abs());
        }
        assert!(max_err < 0.02, "ADAA strays from tanh at LF: {max_err}");
    }
}
