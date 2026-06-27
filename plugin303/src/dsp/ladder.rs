//! Diode-ladder VCF. M2: zero-delay-feedback 4-pole TPT ladder with resonance.
//! 4 cascaded 1-pole TPT stages closed by a `k`-weighted global feedback loop,
//! solved instantaneously (Zavalishin/Cytomic). Per-stage tanh saturation,
//! 2/3/4-pole tap select and the vcf303 ngspice fit come in M2.2–M2.4.

pub struct TptLadder {
    g: f32,       // prewarped per-stage coefficient, tan(pi*fc/sr)
    k: f32,       // resonance feedback gain, 0..4 (4 = self-oscillation threshold)
    z: [f32; 4],  // per-stage integrator states
    sr: f32,
}

/// Integrator-state headroom. Soft-clip is linear for `|x| << HEADROOM` so
/// small-signal AC response (the vcf303 fit) is untouched, and saturates near
/// the rails for self-osc / overdrive.
const HEADROOM: f32 = 8.0;

/// Soft-clip: `H*tanh(x/H)` — `~x` near 0, asymptotic to `±H`.
fn sat(x: f32) -> f32 {
    HEADROOM * (x / HEADROOM).tanh()
}

impl TptLadder {
    pub fn new(sr: f32) -> Self {
        let mut s = Self { g: 0.0, k: 0.0, z: [0.0; 4], sr };
        s.set_cutoff(1_000.0);
        s
    }

    pub fn set_cutoff(&mut self, hz: f32) {
        let wd = std::f32::consts::PI * hz.clamp(20.0, self.sr * 0.45) / self.sr;
        self.g = wd.tan(); // per-stage 1-pole TPT coefficient
    }

    /// Resonance in [0, 1]; 1.0 maps to the `k = 4` self-oscillation threshold.
    pub fn set_reso(&mut self, r: f32) {
        self.k = 4.0 * r.clamp(0.0, 1.0);
    }

    pub fn tick(&mut self, x: f32) -> f32 {
        let g = self.g;
        // Per-stage gain a = g/(1+g), complement b = 1/(1+g) (= 1 - a).
        let a = g / (1.0 + g);
        let b = 1.0 / (1.0 + g);
        // Stage output is y_i = a*x_i + b*s_i, so expanding the 4-stage cascade
        // gives y4 = a^4 * u + Sigma, where u is the resonance-fed input and Sigma
        // collects the state contributions.
        let sigma = a * a * a * b * self.z[0]
            + a * a * b * self.z[1]
            + a * b * self.z[2]
            + b * self.z[3];
        let a4 = a * a * a * a;
        // Close the loop instantaneously (ZDF): u = x - k*y4, y4 = a^4*u + Sigma.
        let y4 = (a4 * x + sigma) / (1.0 + self.k * a4);
        let u = x - self.k * y4;

        // Forward pass: advance every integrator state with the resolved input.
        // Each stage's integrator state is soft-clipped (per-stage tanh): linear
        // for |state| << HEADROOM, asymptotic to ±HEADROOM — the diode-ladder
        // squelch and the bound that keeps self-oscillation finite.
        let mut v = u;
        for z in self.z.iter_mut() {
            let hp = (v - *z) * a; // = g*(in - state)/(1+g)
            let lp = hp + *z;
            *z = sat(lp + hp);
            v = lp;
        }
        v // v ~= y4 (the 4-pole tap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms(f: &mut TptLadder, freq: f32, sr: f32) -> f32 {
        rms_amp(f, freq, sr, 1.0)
    }

    fn rms_amp(f: &mut TptLadder, freq: f32, sr: f32, amp: f32) -> f32 {
        let mut acc = 0.0f32;
        let n = 20_000;
        let mut ph = 0.0f32;
        for _ in 0..n {
            let x = amp * (2.0 * std::f32::consts::PI * ph).sin();
            let y = f.tick(x);
            acc += y * y;
            ph += freq / sr;
            if ph >= 1.0 {
                ph -= 1.0;
            }
        }
        (acc / n as f32).sqrt()
    }

    #[test]
    fn passes_dc_attenuates_hf() {
        let sr = 48_000.0;
        let mut f = TptLadder::new(sr);
        f.set_cutoff(1_000.0);
        let lo = rms(&mut f, 100.0, sr); // well below cutoff
        let mut f2 = TptLadder::new(sr);
        f2.set_cutoff(1_000.0);
        let hi = rms(&mut f2, 8_000.0, sr); // 3 octaves above
        assert!(lo > 0.6, "passband too low: {lo}");
        assert!(hi < 0.05, "stopband too high: {hi}");
    }

    #[test]
    fn resonance_lifts_gain_at_cutoff() {
        let sr = 48_000.0;
        let fc = 1_000.0;
        // Flat (no resonance): gain at cutoff is the gentle -3dB-ish ladder corner.
        let mut flat = TptLadder::new(sr);
        flat.set_cutoff(fc);
        flat.set_reso(0.0);
        let g_flat = rms(&mut flat, fc, sr);
        // High resonance: the feedback loop builds a peak at the cutoff.
        let mut reso = TptLadder::new(sr);
        reso.set_cutoff(fc);
        reso.set_reso(0.9);
        let g_reso = rms(&mut reso, fc, sr);
        assert!(
            g_reso > g_flat * 2.0,
            "resonance did not peak at cutoff: flat={g_flat}, reso={g_reso}"
        );
    }

    #[test]
    fn saturation_compresses_large_signals() {
        let sr = 48_000.0;
        let f0 = 100.0; // passband, gain ~1 so the stage drive tracks input amplitude
        // Small-signal gain (effectively linear).
        let mut small = TptLadder::new(sr);
        small.set_cutoff(1_000.0);
        let g_small = rms_amp(&mut small, f0, sr, 0.05) / 0.05;
        // Large-signal gain: per-stage tanh saturates, so gain drops.
        let mut large = TptLadder::new(sr);
        large.set_cutoff(1_000.0);
        let g_large = rms_amp(&mut large, f0, sr, 40.0) / 40.0;
        assert!(
            g_large < 0.8 * g_small,
            "no saturation: g_small={g_small}, g_large={g_large}"
        );
    }
}
