//! ADAA2 — second-order antiderivative antialiasing for a memoryless soft-clip.
//!
//! The plugin's nonlinear backends (recurrent LSTM, implicit WDF circuit + Jiles-Atherton core) are
//! all stateful, so textbook memoryless ADAA does not retrofit onto them. This module instead
//! provides a NEW, optional memoryless saturation stage that is antialiased by construction — a
//! DC-centred asymmetric `tanh` soft-clip whose first and second antiderivatives are precomputed
//! into lookup tables (ADAA-LUT). The runtime hot path does ADAA2 (triangular kernel) with only
//! table lookups + a handful of FLOPs, with nested L'Hôpital fallbacks for the ill-conditioned
//! near-static cases. No heap, no locks in `process_sample`. See
//! `docs/plans/2026-06-15-adaa-antialiasing-design.md`.

use std::sync::Arc;

/// Table resolution. 8192 f32 = 32 KB per table; report shows 8192-pt linear interp ≈ analytical SNR.
const N: usize = 8192;
/// Bounded input domain the LUT covers. Input is hard-clamped to this before indexing. `tanh`
/// saturates well before ±8, so the antiderivatives are effectively affine past the edges.
const U_MIN: f32 = -8.0;
const U_MAX: f32 = 8.0;
/// Asymmetry operating point: shaping the signal around a non-zero `tanh` bias gives unequal
/// positive/negative curvature (even-harmonic colour) while staying C-infinity smooth.
const BIAS: f64 = 0.3;
/// Ill-conditioning threshold (input units). The 2nd-order divided difference amplifies f32
/// table-lerp noise by ~2/|x0−x2|, so a too-small threshold lets quiet/slow passages explode; `1e-2`
/// bounds the amplification while staying far below the per-sample step of any hard-driven (i.e.
/// actually-aliasing) signal, which always takes the antialiased main path. Cheap, well-predicted.
const TOL: f32 = 1.0e-2;

/// The fixed memoryless soft-clip shape `f(x)`, DC-centred so `f(0) = 0` (silence → silence).
#[inline]
fn shape(x: f64) -> f64 {
    (x + BIAS).tanh() - BIAS.tanh()
}

/// Precomputed antiderivative tables for the fixed [`shape`]. Built once off the audio thread and
/// shared read-only (`Arc`) — the shape never changes at runtime, so no lock-free hot-swap is needed.
pub struct SatTables {
    f0: Box<[f32]>, // the shape itself (outer L'Hôpital fallback)
    f1: Box<[f32]>, // first antiderivative (inner L'Hôpital fallback / ADAA1)
    f2: Box<[f32]>, // second antiderivative (ADAA2 main path)
    u_min: f32,
    inv_step: f32, // (N-1) / (U_MAX - U_MIN)
}

impl SatTables {
    /// Build the tables by sampling [`shape`] and trapezoidally integrating twice. The constants of
    /// integration are immaterial — the ADAA difference equations subtract two antiderivative
    /// evaluations, cancelling them.
    pub fn build() -> Arc<Self> {
        let step = (U_MAX - U_MIN) as f64 / (N as f64 - 1.0);
        let mut f0 = vec![0.0f32; N].into_boxed_slice();
        let mut f1 = vec![0.0f32; N].into_boxed_slice();
        let mut f2 = vec![0.0f32; N].into_boxed_slice();
        // Sample the shape, then cumulative-trapezoid twice (f64 accumulator for precision).
        let mut s0 = vec![0.0f64; N];
        for i in 0..N {
            s0[i] = shape(U_MIN as f64 + i as f64 * step);
            f0[i] = s0[i] as f32;
        }
        let (mut acc1, mut acc2) = (0.0f64, 0.0f64);
        let mut s1_prev = 0.0f64;
        for i in 1..N {
            acc1 += 0.5 * (s0[i - 1] + s0[i]) * step;
            let s1 = acc1;
            acc2 += 0.5 * (s1_prev + s1) * step;
            s1_prev = s1;
            f1[i] = s1 as f32;
            f2[i] = acc2 as f32;
        }
        Arc::new(Self {
            f0,
            f1,
            f2,
            u_min: U_MIN,
            inv_step: (N as f32 - 1.0) / (U_MAX - U_MIN),
        })
    }

    /// Clamp-then-linear-interp lookup. The clamp keeps the index in `0..N-1`, so release builds can
    /// elide bounds checks (`get_unchecked`); debug builds use the checked path to catch regressions.
    #[inline]
    fn lerp(&self, table: &[f32], u: f32) -> f32 {
        let pos = (u.clamp(U_MIN, U_MAX) - self.u_min) * self.inv_step; // in [0, N-1]
        // Clamp the base index FIRST, then take frac relative to it, so the top cell (pos = N-1)
        // reads table[N-1] with frac = 1 instead of dropping to table[N-2]. A frac computed before
        // the clamp leaves the edge divided differences inconsistent between f1 and f2.
        let i = (pos as usize).min(N - 2);
        let frac = pos - i as f32;
        #[cfg(debug_assertions)]
        {
            table[i] + (table[i + 1] - table[i]) * frac
        }
        #[cfg(not(debug_assertions))]
        unsafe {
            let a = *table.get_unchecked(i);
            let b = *table.get_unchecked(i + 1);
            a + (b - a) * frac
        }
    }

    #[inline]
    pub fn f0(&self, u: f32) -> f32 {
        self.lerp(&self.f0, u)
    }

    #[inline]
    pub fn f1(&self, u: f32) -> f32 {
        self.lerp(&self.f1, u)
    }

    #[inline]
    pub fn f2(&self, u: f32) -> f32 {
        self.lerp(&self.f2, u)
    }
}

/// Per-channel ADAA2 saturation state: the two previous (already drive-scaled) input samples.
pub struct Adaa2Sat {
    tables: Arc<SatTables>,
    x1: f32,
    x2: f32,
}

impl Adaa2Sat {
    pub fn new(tables: Arc<SatTables>) -> Self {
        Self { tables, x1: 0.0, x2: 0.0 }
    }

    pub fn reset(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
    }

    /// Antialiased shape of one (drive-scaled) input sample.
    #[inline]
    pub fn process_sample(&mut self, x0: f32) -> f32 {
        // Hard-clamp the input to the table domain on entry so the divided-difference denominators and
        // the (clamped) antiderivative lookups stay consistent — past the edge the stage behaves like
        // a hard clip rather than letting the reconstruction overshoot. State stores clamped values.
        let x0 = x0.clamp(U_MIN, U_MAX);
        let t = &self.tables;
        let (x1, x2) = (self.x1, self.x2);

        // First divided difference of F2 ("ADAA1 of F1"); L'Hôpital → f1 at the midpoint.
        let d2 = |a: f32, b: f32| -> f32 {
            let d = a - b;
            if d.abs() > TOL {
                (t.f2(a) - t.f2(b)) / d
            } else {
                t.f1((a + b) * 0.5)
            }
        };
        // First-order ADAA of f (the outer L'Hôpital fallback); inner L'Hôpital → f0 at the midpoint.
        let adaa1 = |a: f32, b: f32| -> f32 {
            let d = a - b;
            if d.abs() > TOL {
                (t.f1(a) - t.f1(b)) / d
            } else {
                t.f0((a + b) * 0.5)
            }
        };

        let outer = x0 - x2;
        let y = if outer.abs() > TOL {
            2.0 * (d2(x0, x1) - d2(x1, x2)) / outer
        } else {
            // x0 ≈ x2 (turning point / static): the 2nd-order kernel degenerates → fall to ADAA1.
            adaa1(x0, x1)
        };

        self.x2 = x1;
        self.x1 = x0;
        y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f0_table_matches_shape() {
        let t = SatTables::build();
        for &x in &[-3.0f32, -0.5, 0.0, 0.5, 2.0] {
            let got = t.f0(x) as f64;
            let want = shape(x as f64);
            assert!((got - want).abs() < 1e-3, "f0({x}) = {got}, want {want}");
        }
    }

    #[test]
    fn constant_input_returns_shape() {
        // x0 = x1 = x2 = c → both outer and inner differences vanish → nested L'Hôpital fallback
        // bottoms out at f0(c) = shape(c).
        let mut sat = Adaa2Sat::new(SatTables::build());
        for &c in &[-2.0f32, -0.4, 0.0, 0.7, 3.0] {
            let mut y = 0.0;
            for _ in 0..4 {
                y = sat.process_sample(c);
            }
            let want = shape(c as f64);
            assert!((y as f64 - want).abs() < 2e-3, "constant {c} → {y}, want {want}");
        }
    }

    #[test]
    fn branch_boundary_is_continuous_and_finite() {
        // A decaying sine sweeps the per-sample difference continuously through TOL (main path ↔
        // L'Hôpital fallback) and the input through the clamp edges. The output must stay finite and
        // free of spurious branch-switch jumps.
        let mut sat = Adaa2Sat::new(SatTables::build());
        let mut prev = sat.process_sample(0.0);
        let mut max_jump = 0.0f32;
        for i in 0..20_000 {
            // amplitude decays from 12 (past the clamp) toward ~0 so diffs shrink below TOL
            let amp = 12.0 * (1.0 - i as f32 / 20_000.0);
            let x = amp * (0.1 * i as f32).sin();
            let y = sat.process_sample(x);
            assert!(y.is_finite(), "non-finite output {y} at i={i}, x={x}");
            assert!(y.abs() < 4.0, "output {y} out of range at i={i}, x={x}");
            max_jump = max_jump.max((y - prev).abs());
            prev = y;
        }
        // The shape itself is Lipschitz (|f'| ≤ 1) and the input step is bounded, so a correct ADAA2
        // never jumps wildly; a branch discontinuity would spike this far past the input slew.
        assert!(max_jump < 1.5, "spurious output jump {max_jump} (branch discontinuity?)");
    }
}
