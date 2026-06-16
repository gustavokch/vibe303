//! Neve 1073/1076-style EQ — analog-matched biquad stage placed BEFORE the model.
//!
//! On the real channel strip (schematic EH 10023) the signal flow is
//! `MIC → T1 → BA284 → EQ network → BA283/AV → T3 LO1166 → out`. The BA283/AV amplifier and the
//! LO1166 output transformer are exactly what the WDF/NN core models, and the EQ section sits
//! UPSTREAM of that card. So the faithful place for this stage is on the input signal, before the
//! model — the coupled amp+transformer solve stays untouched, and EQ boosts drive the amp harder on
//! boosted bands exactly as the hardware does.
//!
//! The EQ network is passive inductor-based; we model it with RBJ shelving/peaking biquads tuned to
//! the published 1073 curves, plus a gain-dependent Q on the mid bell to emulate the passive
//! *proportional-Q* signature (Q narrows as boost rises). The one thing static biquads cannot
//! reproduce — inductor harmonic saturation — is second-order here, and the `trafo-core`
//! (Jiles-Atherton) stage already supplies the dominant Neve transformer nonlinearity.
//!
//! Switched frequencies (fixed by EH 10023): HPF 50/80/160/300 Hz (~18 dB/oct), low shelf
//! 35/60/110/220 Hz, mid/presence bell 360/700/1.6k/3.2k/4.8k/7.2k Hz, high shelf fixed 12 kHz.
//!
//! Bypass is bit-exact: when `on == false`, `process` returns its input unchanged with no biquad
//! calls. The stage runs at host sample rate (the post-drive signal is host-rate); the gentle
//! ≤12 kHz curves are far from Nyquist so no oversampling is needed.

use crate::wdf::Biquad;

/// High shelf is a fixed-frequency band on the 1073/1076 (RV1 sets gain only).
const HIGH_SHELF_HZ: f64 = 12_000.0;
/// Default shelf Q (gentle, Neve-like). Used for both low and high shelves.
const SHELF_Q: f64 = 0.707;
/// 2nd-order section Q for the 3rd-order Butterworth high-pass (real-pole + this complex pair).
const HPF_BUTTER_Q: f64 = 1.0;

/// Zero a biquad's delay-line state without touching its coefficients.
#[inline]
fn reset_state(bq: &mut Biquad) {
    bq.x1 = 0.0;
    bq.x2 = 0.0;
    bq.y1 = 0.0;
    bq.y2 = 0.0;
}

/// Make a biquad an identity pass-through (`y = x`), state preserved.
#[inline]
fn set_identity(bq: &mut Biquad) {
    bq.b0 = 1.0;
    bq.b1 = 0.0;
    bq.b2 = 0.0;
    bq.a1 = 0.0;
    bq.a2 = 0.0;
}

/// RBJ peaking EQ. `Biquad::process` uses `y = b0 x + b1 x1 + b2 x2 − a1 y1 − a2 y2`, so the stored
/// `a1`/`a2` are the RBJ denominator coefficients already normalized by `a0`.
fn set_peaking(bq: &mut Biquad, fc: f64, fs: f64, gain_db: f64, q: f64) {
    let a = 10f64.powf(gain_db / 40.0);
    let w0 = 2.0 * std::f64::consts::PI * fc / fs;
    let (c, s) = (w0.cos(), w0.sin());
    let alpha = s / (2.0 * q);
    let a0 = 1.0 + alpha / a;
    bq.b0 = (1.0 + alpha * a) / a0;
    bq.b1 = (-2.0 * c) / a0;
    bq.b2 = (1.0 - alpha * a) / a0;
    bq.a1 = (-2.0 * c) / a0;
    bq.a2 = (1.0 - alpha / a) / a0;
}

/// RBJ low-shelf (Q-parameterized).
fn set_low_shelf(bq: &mut Biquad, fc: f64, fs: f64, gain_db: f64, q: f64) {
    let a = 10f64.powf(gain_db / 40.0);
    let w0 = 2.0 * std::f64::consts::PI * fc / fs;
    let (c, s) = (w0.cos(), w0.sin());
    let alpha = s / (2.0 * q);
    let tsa = 2.0 * a.sqrt() * alpha;
    let ap1 = a + 1.0;
    let am1 = a - 1.0;
    let a0 = ap1 + am1 * c + tsa;
    bq.b0 = a * (ap1 - am1 * c + tsa) / a0;
    bq.b1 = 2.0 * a * (am1 - ap1 * c) / a0;
    bq.b2 = a * (ap1 - am1 * c - tsa) / a0;
    bq.a1 = -2.0 * (am1 + ap1 * c) / a0;
    bq.a2 = (ap1 + am1 * c - tsa) / a0;
}

/// RBJ high-shelf (Q-parameterized).
fn set_high_shelf(bq: &mut Biquad, fc: f64, fs: f64, gain_db: f64, q: f64) {
    let a = 10f64.powf(gain_db / 40.0);
    let w0 = 2.0 * std::f64::consts::PI * fc / fs;
    let (c, s) = (w0.cos(), w0.sin());
    let alpha = s / (2.0 * q);
    let tsa = 2.0 * a.sqrt() * alpha;
    let ap1 = a + 1.0;
    let am1 = a - 1.0;
    let a0 = ap1 - am1 * c + tsa;
    bq.b0 = a * (ap1 + am1 * c + tsa) / a0;
    bq.b1 = -2.0 * a * (am1 + ap1 * c) / a0;
    bq.b2 = a * (ap1 + am1 * c - tsa) / a0;
    bq.a1 = 2.0 * (am1 - ap1 * c) / a0;
    bq.a2 = (ap1 - am1 * c - tsa) / a0;
}

/// RBJ 2nd-order high-pass.
fn set_highpass2(bq: &mut Biquad, fc: f64, fs: f64, q: f64) {
    let w0 = 2.0 * std::f64::consts::PI * fc / fs;
    let (c, s) = (w0.cos(), w0.sin());
    let alpha = s / (2.0 * q);
    let a0 = 1.0 + alpha;
    bq.b0 = (1.0 + c) / 2.0 / a0;
    bq.b1 = -(1.0 + c) / a0;
    bq.b2 = (1.0 + c) / 2.0 / a0;
    bq.a1 = (-2.0 * c) / a0;
    bq.a2 = (1.0 - alpha) / a0;
}

/// 1st-order high-pass (bilinear), realized in a biquad with the 2nd-order taps zeroed. Cascaded
/// with [`set_highpass2`] (Q = 1.0) this gives the 3rd-order ≈ 18 dB/oct slope of the 1073 HPF.
fn set_highpass1(bq: &mut Biquad, fc: f64, fs: f64) {
    let k = (std::f64::consts::PI * fc / fs).tan();
    let norm = 1.0 / (1.0 + k);
    bq.b0 = norm;
    bq.b1 = -norm;
    bq.b2 = 0.0;
    bq.a1 = (k - 1.0) * norm;
    bq.a2 = 0.0;
}

/// Proportional-Q of the mid/presence bell as a function of boost/cut magnitude. The passive
/// inductor band widens at low gain and narrows as the boost/cut grows — the defining 1073/1076
/// "musical" character that a fixed-Q parametric misses.
///
// CALIBRATION: provisional. Q rises ~0.7 (±0 dB) → ~2.0 (±18 dB); fit against digitized 1073
// presence curves in a later pass (see the offline sweep in the design doc).
fn mid_q(gain_db: f64) -> f64 {
    let g = gain_db.abs().min(18.0);
    0.7 + (2.0 - 0.7) * (g / 18.0)
}

/// Resolved per-band settings for one block. `hpf_hz <= 0` means the high-pass is bypassed.
#[derive(Clone, Copy, PartialEq)]
struct EqSettings {
    hpf_hz: f32,
    low_hz: f32,
    low_g: f32,
    mid_hz: f32,
    mid_g: f32,
    high_g: f32,
}

/// Per-channel analog-matched EQ stage. Fixed-size, zero-alloc on the audio path.
pub struct Eq {
    /// 3rd-order high-pass: `[0]` = 2nd-order section (Q = 1.0), `[1]` = 1st-order section.
    hpf: [Biquad; 2],
    low_shelf: Biquad,
    mid: Biquad,
    high_shelf: Biquad,
    /// Runtime bypass. When false, `process` is bit-exact identity.
    on: bool,
    /// Whether the high-pass is engaged (false when the HPF selector is Off).
    hpf_on: bool,
    fs: f64,
    /// Last-applied settings; coeffs are recomputed only when these change.
    cache: Option<EqSettings>,
}

impl Default for Eq {
    fn default() -> Self {
        Self::new()
    }
}

impl Eq {
    pub fn new() -> Self {
        Self {
            hpf: [Biquad::default(); 2],
            low_shelf: Biquad::default(),
            mid: Biquad::default(),
            high_shelf: Biquad::default(),
            on: false,
            hpf_on: false,
            fs: 48_000.0,
            cache: None,
        }
    }

    /// Set the host sample rate. Forces a coeff recompute on the next [`Eq::set`] and clears state.
    pub fn prepare(&mut self, fs: f32) {
        self.fs = fs as f64;
        self.cache = None;
        self.reset();
    }

    /// Zero all delay-line state (coeffs preserved).
    pub fn reset(&mut self) {
        for bq in self.hpf.iter_mut() {
            reset_state(bq);
        }
        reset_state(&mut self.low_shelf);
        reset_state(&mut self.mid);
        reset_state(&mut self.high_shelf);
    }

    /// Block-rate setter. `hpf_hz <= 0` bypasses the high-pass. Recomputes coefficients only when the
    /// resolved settings differ from the last applied set (biquad state is preserved across a change,
    /// so a coefficient update glides rather than clicks).
    #[allow(clippy::too_many_arguments)]
    pub fn set(
        &mut self,
        on: bool,
        hpf_hz: f32,
        low_hz: f32,
        low_g: f32,
        mid_hz: f32,
        mid_g: f32,
        high_g: f32,
    ) {
        self.on = on;
        let s = EqSettings { hpf_hz, low_hz, low_g, mid_hz, mid_g, high_g };
        if self.cache == Some(s) {
            return;
        }
        self.cache = Some(s);

        self.hpf_on = hpf_hz > 0.0;
        if self.hpf_on {
            set_highpass2(&mut self.hpf[0], hpf_hz as f64, self.fs, HPF_BUTTER_Q);
            set_highpass1(&mut self.hpf[1], hpf_hz as f64, self.fs);
        }
        // Gain ≈ 0 dB → identity, so a flat band adds no rounding noise and the all-flat state is
        // provably transparent.
        if low_g.abs() < 1.0e-4 {
            set_identity(&mut self.low_shelf);
        } else {
            set_low_shelf(&mut self.low_shelf, low_hz as f64, self.fs, low_g as f64, SHELF_Q);
        }
        if mid_g.abs() < 1.0e-4 {
            set_identity(&mut self.mid);
        } else {
            set_peaking(&mut self.mid, mid_hz as f64, self.fs, mid_g as f64, mid_q(mid_g as f64));
        }
        if high_g.abs() < 1.0e-4 {
            set_identity(&mut self.high_shelf);
        } else {
            set_high_shelf(&mut self.high_shelf, HIGH_SHELF_HZ, self.fs, high_g as f64, SHELF_Q);
        }
    }

    /// Filter one sample. Bit-exact identity when the stage is off.
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        if !self.on {
            return x;
        }
        let mut y = x as f64;
        if self.hpf_on {
            y = self.hpf[0].process(y);
            y = self.hpf[1].process(y);
        }
        y = self.low_shelf.process(y);
        y = self.mid.process(y);
        y = self.high_shelf.process(y);
        y as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run a steady sine through a band and return the settled output RMS / input RMS in dB.
    fn band_gain_db(eq: &mut Eq, fc: f64, fs: f64) -> f64 {
        let n = (fs * 4.0) as usize; // ~4 s: long settle so transient is negligible
        let warm = n / 2;
        let (mut si, mut so) = (0.0f64, 0.0f64);
        for i in 0..n {
            let t = i as f64 / fs;
            let x = (2.0 * std::f64::consts::PI * fc * t).sin();
            let y = eq.process(x as f32) as f64;
            if i >= warm {
                si += x * x;
                so += y * y;
            }
        }
        10.0 * (so / si).log10()
    }

    #[test]
    fn bypass_is_bit_exact() {
        let mut eq = Eq::new();
        eq.prepare(48_000.0);
        eq.set(false, 80.0, 60.0, 6.0, 1600.0, 6.0, 6.0); // settings set, but on=false
        let xs = [0.0f32, 0.5, -0.731, 1.0, -1.0, 0.123, 0.999, -0.456];
        for &x in &xs {
            assert_eq!(eq.process(x).to_bits(), x.to_bits(), "bypass must be bit-exact for {x}");
        }
    }

    #[test]
    fn flat_defaults_are_transparent() {
        // EQ on, all gains 0, HPF off → all bands identity → ~0 dB at any probe frequency.
        let mut eq = Eq::new();
        eq.prepare(48_000.0);
        eq.set(true, 0.0, 60.0, 0.0, 1600.0, 0.0, 0.0);
        for &fc in &[100.0, 1000.0, 8000.0] {
            let g = band_gain_db(&mut eq, fc, 48_000.0);
            assert!(g.abs() < 0.05, "flat EQ not transparent at {fc} Hz: {g} dB");
        }
    }

    #[test]
    fn bands_hit_their_target_gain() {
        let fs = 48_000.0;
        // Low shelf at DC-ish probe.
        let mut eq = Eq::new();
        eq.prepare(fs as f32);
        eq.set(true, 0.0, 60.0, 12.0, 1600.0, 0.0, 0.0);
        let g = band_gain_db(&mut eq, 30.0, fs);
        assert!((g - 12.0).abs() < 1.5, "low shelf +12 dB → {g} dB at 30 Hz");

        // Mid bell at its center.
        let mut eq = Eq::new();
        eq.prepare(fs as f32);
        eq.set(true, 0.0, 60.0, 0.0, 1600.0, 12.0, 0.0);
        let g = band_gain_db(&mut eq, 1600.0, fs);
        assert!((g - 12.0).abs() < 1.0, "mid +12 dB → {g} dB at 1.6 kHz");

        // High shelf above its corner.
        let mut eq = Eq::new();
        eq.prepare(fs as f32);
        eq.set(true, 0.0, 60.0, 0.0, 1600.0, 0.0, 12.0);
        let g = band_gain_db(&mut eq, 16_000.0, fs);
        assert!((g - 12.0).abs() < 1.5, "high shelf +12 dB → {g} dB at 16 kHz");
    }

    #[test]
    fn hpf_attenuates_below_corner() {
        let fs = 48_000.0;
        let mut eq = Eq::new();
        eq.prepare(fs as f32);
        eq.set(true, 80.0, 60.0, 0.0, 1600.0, 0.0, 0.0);
        let pass = band_gain_db(&mut eq, 1000.0, fs); // well in the passband
        eq.reset();
        let oct_below = band_gain_db(&mut eq, 40.0, fs); // one octave below the 80 Hz corner
        assert!(pass.abs() < 0.3, "HPF passband not flat: {pass} dB");
        // 3rd-order ≈ 18 dB/oct → an octave below the corner sits well down.
        assert!(oct_below < -12.0, "HPF too shallow: {oct_below} dB one octave below corner");
    }

    #[test]
    fn proportional_q_narrows_with_boost() {
        assert!(mid_q(0.0) < mid_q(6.0), "Q should widen at low boost");
        assert!(mid_q(6.0) < mid_q(18.0), "Q should narrow (rise) as boost grows");
    }

    #[test]
    fn coeffs_stable_across_settings() {
        for &fs in &[44_100.0f32, 48_000.0, 96_000.0, 192_000.0] {
            let mut eq = Eq::new();
            eq.prepare(fs);
            for &hpf in &[0.0f32, 50.0, 80.0, 160.0, 300.0] {
                for &lf in &[35.0f32, 60.0, 110.0, 220.0] {
                    for &mf in &[360.0f32, 700.0, 1600.0, 3200.0, 4800.0, 7200.0] {
                        for &g in &[-16.0f32, -8.0, 0.0, 8.0, 18.0] {
                            eq.set(true, hpf, lf, g, mf, g, g);
                            for bq in eq.hpf.iter().chain([&eq.low_shelf, &eq.mid, &eq.high_shelf]) {
                                // Stability: denominator poles strictly inside the unit circle.
                                assert!(bq.a2.abs() < 1.0, "|a2| >= 1 (fs={fs}, g={g})");
                                assert!(bq.a1.abs() < 1.0 + bq.a2, "pole outside unit circle (fs={fs})");
                                assert!(bq.b0.is_finite() && bq.b1.is_finite() && bq.b2.is_finite());
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn reset_clears_state() {
        let mut eq = Eq::new();
        eq.prepare(48_000.0);
        eq.set(true, 0.0, 60.0, 12.0, 1600.0, 12.0, 12.0);
        for _ in 0..1000 {
            eq.process(0.7);
        }
        eq.reset();
        let mut fresh = Eq::new();
        fresh.prepare(48_000.0);
        fresh.set(true, 0.0, 60.0, 12.0, 1600.0, 12.0, 12.0);
        // After reset, the impulse response must match a fresh instance sample-for-sample.
        for i in 0..64 {
            let x = if i == 0 { 1.0 } else { 0.0 };
            assert_eq!(eq.process(x).to_bits(), fresh.process(x).to_bits(), "state not cleared at {i}");
        }
    }
}
