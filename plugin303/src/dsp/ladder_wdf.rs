//! Nodal diode-ladder VCF solver — Ultra tier core (F3.3).
//!
//! Per-sample **nodal MNA + Newton-Raphson** of the vcf303 4-rung diode ladder
//! (8× 1N4148 + 4 shunt caps), biased by the cutoff tail current. Trapezoidal
//! cap companions, gmin homotopy on stall, fixed-size dense LU on the stack
//! (zero-alloc), warm-started from the previous sample. `impl Filter303`.
//!
//! Module name kept `ladder_wdf` for continuity with the design doc, though the
//! method is nodal-Newton, not wave-digital.
//!
//! Topology (`eval/303/_vcf303_netlist_gen.py`): two rails L/R, nodes
//! `tl,al,bl,cl,dl` / `tr,ar,br,cr,dr`. Rungs = diodes pointing down each rail
//! (`tl→al→bl→cl→dl`, mirror R). Shunt caps bridge the rails per rung
//! (`al–ar 33n`, `bl–br 33n`, `cl–cr 33n`, `dl–dr 18n`). Differential signal
//! drives the bottom (`dl,dr`); cutoff is the ladder bias current `I_tail`
//! injected at the rail tops (each diode's `r_d = nVt/I_d` ⇒ corner ∝ I); the
//! differential output is read across `tl–tr`. The active T38/T20/T21/Q9 stages
//! are linear drive/read/feedback wrappers added in F3.5.

use super::diode::Diode1N4148;
use super::filter::Filter303;
use std::f64::consts::TAU;

// Node indices into the 10-node rail system.
const TL: usize = 0;
const AL: usize = 1;
const BL: usize = 2;
const CL: usize = 3;
const DL: usize = 4;
const TR: usize = 5;
const AR: usize = 6;
const BR: usize = 7;
const CR: usize = 8;
const DR: usize = 9;
const N: usize = 10;

/// Diode rungs as (anode, cathode), current flowing down each rail.
const DIODES: [(usize, usize); 8] = [
    (TL, AL),
    (AL, BL),
    (BL, CL),
    (CL, DL),
    (TR, AR),
    (AR, BR),
    (BR, CR),
    (CR, DR),
];

/// Shunt caps as (left node, right node, farads).
const CAPS: [(usize, usize, f64); 4] = [
    (AL, AR, 3.3e-8),
    (BL, BR, 3.3e-8),
    (CL, CR, 3.3e-8),
    (DL, DR, 1.8e-8),
];

const MAX_ITERS: usize = 50;
const TOL: f64 = 1e-9;
const DENORM: f64 = 1e-30;
/// Always-on leakage so reverse-biased nodes never float the matrix.
const GMIN_BASE: f64 = 1e-12;
/// Cap value the cutoff→bias map is referenced to. The detected −3 dB corner
/// lands well below `fc` here (the diode ladder spreads its 4 poles, dropping
/// the dominant pole below the single-section estimate, and the bias can't be
/// lifted to compensate without driving the diodes past `vcrit` into their
/// frozen-conductance regime). Absolute corner→`fc` calibration to the SPICE
/// gold is an F3.5 task; F3.3 only needs the corner to *track* the bias and the
/// stopband to be four-pole.
const C_REF: f64 = 3.3e-8;

/// Solve `A·x = b` by dense LU with partial pivoting on the stack. Returns
/// `None` if the matrix is singular. Zero-alloc.
fn solve(mut a: [[f64; N]; N], mut b: [f64; N]) -> Option<[f64; N]> {
    for col in 0..N {
        // Partial pivot.
        let mut piv = col;
        let mut max = a[col][col].abs();
        for r in (col + 1)..N {
            if a[r][col].abs() > max {
                max = a[r][col].abs();
                piv = r;
            }
        }
        if max < 1e-300 {
            return None;
        }
        if piv != col {
            a.swap(piv, col);
            b.swap(piv, col);
        }
        let d = a[col][col];
        for r in (col + 1)..N {
            let f = a[r][col] / d;
            if f != 0.0 {
                for c in col..N {
                    a[r][c] -= f * a[col][c];
                }
                b[r] -= f * b[col];
            }
        }
    }
    // Back-substitution.
    let mut x = [0.0; N];
    for i in (0..N).rev() {
        let mut s = b[i];
        for c in (i + 1)..N {
            s -= a[i][c] * x[c];
        }
        x[i] = s / a[i][i];
    }
    Some(x)
}

/// Ultra-tier nodal diode-ladder filter.
pub struct LadderVcf {
    sr: f64,
    diode: Diode1N4148,
    /// Bias (tail) current; sets the diode operating point ⇒ corner.
    i0: f64,
    /// Resonance feedback gain (1-sample-delayed differential feedback).
    k_reso: f64,
    /// Drive source conductance (1/R_drive) into the bottom rails.
    g_drive: f64,
    /// Differential drive voltage scale.
    vscale: f64,
    /// Output scale on the `tl–tr` differential.
    out_gain: f64,
    /// Warm-start state: last converged node voltages.
    v: [f64; N],
    /// Per-cap trapezoidal state (v_prev, i_prev).
    vc_prev: [f64; 4],
    i_prev: [f64; 4],
    /// Previous-sample raw differential output (resonance feedback tap).
    out_prev: f64,
}

impl LadderVcf {
    pub fn new(sr: f32) -> Self {
        let mut f = Self {
            sr: sr as f64,
            diode: Diode1N4148::new(),
            i0: 0.0,
            k_reso: 0.0,
            g_drive: 1e-3,
            vscale: 0.02,
            out_gain: 1.0,
            v: [0.0; N],
            vc_prev: [0.0; 4],
            i_prev: [0.0; 4],
            out_prev: 0.0,
        };
        f.set_cutoff(1_000.0);
        f
    }

    /// One Newton solve at a fixed `gmin`, warm-started from `v0`. Returns the
    /// converged node voltages, or `None` if it stalled within `MAX_ITERS`.
    fn newton(
        &self,
        gmin: f64,
        v0: [f64; N],
        vin: f64,
        cgeq: &[f64; 4],
        cieq: &[f64; 4],
    ) -> Option<[f64; N]> {
        let mut v = v0;
        for _ in 0..MAX_ITERS {
            let mut g = [[0.0f64; N]; N];
            let mut b = [0.0f64; N];

            // Diode companion stamps (Geq = dI/dV, Ieq = I − g·v).
            for &(a, k) in &DIODES {
                let vd = v[a] - v[k];
                let (id, gd) = self.diode.stamp(vd);
                let ieq = id - gd * vd;
                g[a][a] += gd;
                g[k][k] += gd;
                g[a][k] -= gd;
                g[k][a] -= gd;
                b[a] -= ieq;
                b[k] += ieq;
            }

            // Cap trapezoidal companions.
            for j in 0..4 {
                let (p, n, _) = CAPS[j];
                let geq = cgeq[j];
                g[p][p] += geq;
                g[n][n] += geq;
                g[p][n] -= geq;
                g[n][p] -= geq;
                b[p] += cieq[j];
                b[n] -= cieq[j];
            }

            // Bias tail current into both rail tops.
            b[TL] += self.i0;
            b[TR] += self.i0;

            // Differential drive (Norton): dl → +vin, dr → −vin.
            g[DL][DL] += self.g_drive;
            b[DL] += self.g_drive * vin;
            g[DR][DR] += self.g_drive;
            b[DR] += self.g_drive * (-vin);

            // gmin homotopy + always-on base leakage.
            for d in 0..N {
                g[d][d] += gmin + GMIN_BASE;
            }

            let sol = solve(g, b)?;
            let mut dmax = 0.0f64;
            for d in 0..N {
                let dd = (sol[d] - v[d]).abs();
                if dd > dmax {
                    dmax = dd;
                }
                if !sol[d].is_finite() {
                    return None;
                }
            }
            v = sol;
            if dmax < TOL {
                return Some(v);
            }
        }
        None
    }

    /// Solve one sample at the filter's working rate.
    fn solve_sample(&mut self, x_in: f32) -> f32 {
        let dt = 1.0 / self.sr;
        let vin = (x_in as f64 + self.k_reso * self.out_prev) * self.vscale;

        // Cap companion constants (fixed across Newton iterations this sample).
        let mut cgeq = [0.0f64; 4];
        let mut cieq = [0.0f64; 4];
        for j in 0..4 {
            let (_, _, cap) = CAPS[j];
            let geq = 2.0 * cap / dt;
            cgeq[j] = geq;
            cieq[j] = geq * self.vc_prev[j] + self.i_prev[j];
        }

        // Fast path: warm-started Newton at gmin = 0. On stall, descend a gmin
        // homotopy carrying the solution forward, ending at gmin = 0.
        let v = match self.newton(0.0, self.v, vin, &cgeq, &cieq) {
            Some(v) => v,
            None => {
                let mut vh = self.v;
                let mut ok = None;
                for &gmin in &[1e-1, 1e-2, 1e-3, 1e-4, 0.0] {
                    match self.newton(gmin, vh, vin, &cgeq, &cieq) {
                        Some(v) => {
                            vh = v;
                            ok = Some(v);
                        }
                        None => break,
                    }
                }
                ok.unwrap_or(self.v)
            }
        };
        self.v = v;

        // Update cap trapezoidal state from the converged solution.
        for j in 0..4 {
            let (p, n, _) = CAPS[j];
            let vc = v[p] - v[n];
            let inew = cgeq[j] * vc - cieq[j];
            let vcf = if vc.abs() < DENORM { 0.0 } else { vc };
            let iif = if inew.abs() < DENORM { 0.0 } else { inew };
            self.vc_prev[j] = vcf;
            self.i_prev[j] = iif;
        }

        let diff = v[TL] - v[TR];
        self.out_prev = diff;
        (diff * self.out_gain) as f32
    }
}

impl Filter303 for LadderVcf {
    fn set_cutoff(&mut self, hz: f32) {
        // I_tail = nVt · 2π · fc · C_ref ⇒ r_d = nVt/I ⇒ corner ∝ fc.
        let fc = hz.max(1.0) as f64;
        self.i0 = self.diode.n_vt * TAU * fc * C_REF;
    }

    fn set_reso(&mut self, r: f32) {
        self.k_reso = (r.clamp(0.0, 1.0) as f64) * 0.9;
    }

    fn tick(&mut self, x: f32) -> f32 {
        self.solve_sample(x)
    }

    fn reset(&mut self) {
        self.v = [0.0; N];
        self.vc_prev = [0.0; 4];
        self.i_prev = [0.0; 4];
        self.out_prev = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Small-signal AC gain at `freq`: RMS(out)/RMS(in) over steady state.
    fn ac_gain(f: &mut LadderVcf, freq: f32) -> f32 {
        f.reset();
        let sr = 48_000.0f32;
        let n = 8192usize;
        let amp = 0.05f32;
        let warm = (n * 6) / 10;
        let (mut sin_e, mut sout_e) = (0.0f64, 0.0f64);
        for i in 0..n {
            let x = amp * (TAU as f32 * freq * i as f32 / sr).sin();
            let y = f.tick(x);
            if i >= warm {
                sin_e += (x as f64) * (x as f64);
                sout_e += (y as f64) * (y as f64);
            }
        }
        ((sout_e / sin_e).sqrt()) as f32
    }

    /// −3 dB corner via geometric bisection (lowpass gain is monotone).
    fn corner(f: &mut LadderVcf, fc: f32) -> f32 {
        f.set_cutoff(fc);
        f.set_reso(0.0);
        let g0 = ac_gain(f, 30.0);
        let target = g0 / std::f32::consts::SQRT_2;
        let (mut lo, mut hi) = (30.0f32, 20_000.0f32);
        for _ in 0..28 {
            let mid = (lo * hi).sqrt();
            if ac_gain(f, mid) > target {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        (lo * hi).sqrt()
    }

    #[test]
    fn dc_operating_point_converges() {
        let mut f = LadderVcf::new(48_000.0);
        f.set_cutoff(1_000.0);
        f.set_reso(0.0);
        for _ in 0..256 {
            f.tick(0.0);
        }
        assert!(f.tick(0.0).is_finite());
    }

    #[test]
    fn cutoff_tracks_bias_current() {
        let mut f = LadderVcf::new(48_000.0);
        let a = corner(&mut f, 500.0);
        let b = corner(&mut f, 2_000.0);
        assert!(b > a * 2.5 && b < a * 5.0, "cutoff does not track bias: {a} -> {b}");
    }

    #[test]
    fn slope_is_four_pole_ish() {
        // ~ -20..-24 dB/oct deep stopband (matches vcf303_gold slope ~ -21 dB/oct).
        let mut f = LadderVcf::new(48_000.0);
        let c = corner(&mut f, 1_000.0);
        // Probe well above the spread 4-pole cluster (but below Nyquist) for the
        // asymptote.
        let g1 = ac_gain(&mut f, c * 16.0);
        let g2 = ac_gain(&mut f, c * 32.0);
        let db_per_oct = 20.0 * (g2 / g1).log10();
        assert!(
            (-26.0..=-15.0).contains(&db_per_oct),
            "slope {db_per_oct} dB/oct not four-pole-ish (corner {c})"
        );
    }
}
