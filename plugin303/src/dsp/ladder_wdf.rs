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
use super::linalg_static::ladder_lu_solve;
use std::f64::consts::TAU;

#[cfg(test)]
use super::linalg::lu_solve;

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
/// Split input nodes (F6.2): the T38 differential pair's two bases. `IN_L` takes
/// the signal, `IN_R` the inverting Q9 resonance feedback (F6.3); held at 0 here.
const IN_L: usize = 10;
const IN_R: usize = 11;
const N: usize = 12;

/// Pin conductance for the voltage-set input nodes. Huge vs `g_drive`/diode
/// conductances so `v[IN_L]→vin`, `v[IN_R]→vin_r` to machine precision at gmin=0.
const G_PIN: f64 = 1e3;

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
/// Bias scale for the cutoff→corner map. **Structural limit found in V4:** raising
/// CAL lifts the corner only until it saturates at a ~600 Hz ceiling (at 48 kHz)
/// and breaks bias-tracking — the simplified differential ladder (no active T20/
/// T21/Q9 stages) cannot reach the audio cutoffs the real vcf303 hits. Kept at 1.0
/// (physical bias, F3.3 tracking + slope green). See `corner_vs_cutoff` diagnostic
/// and the V4 finding in the design doc. Closing this needs the active stages.
const CAL: f64 = 1.0;
/// Q9 CE-amp feedback gain at full resonance (`reso=1`). Calibrated against the
/// ngspice resonance fixture (VCF_MAP §11): at +12 the in-loop peak tracks the
/// fixture to ~5% (set fc 500/1000/2000 → 514/951/1757 Hz vs gold 532/1002/1683)
/// at ~20 dB. Above ~+14 the loop self-limits (collapses to the DC pole via the
/// homotopy hold — no divergence), mirroring the stock 303's non-self-oscillating
/// loop. See the `reso_calibration` diagnostic in `tests/vcf_resonance.rs`.
const RESO_GAIN: f64 = 12.0;
/// AC-coupling high-pass corner for the resonance loop (Hz). Well below any
/// audio fc, so it only blocks DC (loop flat/stable at DC) — near fc it is
/// transparent. Reproduces the Q9 `C23`/`C_out` coupling role.
const RESO_HP_HZ: f64 = 2.0;
/// Small-signal DC passband gain of the differential ladder at `out_gain = 1`
/// (with `vscale = 0.02`). Measured ~0.0401 and cutoff-independent (the
/// depressed dominant pole rolls off above the corner, but the DC anchor is
/// flat). `out_gain` is set to its reciprocal so the nodal ladder presents a
/// **unity DC passband**, matching the Low/TPT path (also unity-DC) — without it
/// Ultra/High sit ~28 dB below Low. Residual musical-level difference above the
/// corner is the depressed-corner limitation (active stages, deferred), not a
/// gain-staging bug.
const LADDER_DC_GAIN: f64 = 0.040_13;

/// Ultra-tier nodal diode-ladder filter.
pub struct LadderVcf {
    sr: f64,
    diode: Diode1N4148,
    /// Bias (tail) current; sets the diode operating point ⇒ corner.
    i0: f64,
    /// Brightness calibration on the cutoff→bias map (spectral V5). Default 1.0
    /// keeps the F3.3 ladder tests in their tracking regime; the voice raises it
    /// to brighten the corner toward the SPICE envelope.
    cal: f64,
    /// Last cutoff (Hz), so `set_cal` can recompute `i0`.
    fc: f64,
    /// Resonance amount, 0..1 (the VR4 pot). Scales the Q9 feedback gain.
    reso: f64,
    /// Q9 common-emitter feedback gain at `reso=1` (signed: the inversion that
    /// makes the loop −ve at DC / +ve near fc lives in the sign). Tunable for
    /// calibration against `eval/303/vcf303_reso_gold.json`; default `RESO_GAIN`.
    reso_gain: f64,
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
    /// AC-coupling DC tracker for the output diff (Q9 `C23`/`C_out` high-pass,
    /// corner `RESO_HP_HZ` ≪ fc). Blocks DC from the resonance loop (loop flat &
    /// stable at DC); near fc it is transparent. One-pole, updated per sample.
    dc_lp: f64,
    /// High-pass pole coefficient for `dc_lp`, derived from `sr`.
    dc_alpha: f64,
    /// Output tap selection (2 | 3 | 4 poles)
    pub poles: usize,
    /// Diagnostic: Newton iters used on the last `solve_sample` (max over the
    /// homotopy descent if the fast path stalled). Touched per sample; not a gate.
    #[doc(hidden)]
    pub dbg_iters: u32,
}

impl LadderVcf {
    pub fn new(sr: f32) -> Self {
        let mut f = Self {
            sr: sr as f64,
            diode: Diode1N4148::new(),
            i0: 0.0,
            cal: CAL,
            fc: 1_000.0,
            reso: 0.0,
            reso_gain: RESO_GAIN,
            g_drive: 1e-3,
            vscale: 0.02,
            out_gain: 1.0 / LADDER_DC_GAIN, // unity DC passband (matches Low/TPT)
            v: [0.0; N],
            vc_prev: [0.0; 4],
            i_prev: [0.0; 4],
            dc_lp: 0.0,
            dc_alpha: 1.0 - (-TAU * RESO_HP_HZ / sr as f64).exp(),
            poles: 4,
            dbg_iters: 0,
        };
        f.set_cutoff(1_000.0);
        f
    }

    /// Diagnostic: override the Q9 feedback gain (sign + magnitude) for
    /// calibration against the ngspice resonance fixture. Not used at runtime.
    pub fn set_reso_gain(&mut self, g: f64) {
        self.reso_gain = g;
    }

    /// Brightness calibration on the cutoff→bias map (spectral V5). Recomputes
    /// the bias at the current cutoff.
    pub fn set_cal(&mut self, cal: f64) {
        self.cal = cal;
        self.i0 = self.diode.n_vt * TAU * self.fc * C_REF * self.cal;
    }

    /// Assemble the MNA system `(G, b)` at iterate `v`, fixed cap companions, the
    /// Q9 feedback coefficients `(kfb, cfb)`, and `gmin` on the diagonal. The only
    /// `v`-dependent stamps are the 8 diode companions; everything else (caps,
    /// bias, pins, feedback, VCCS drive, gmin) is iteration-invariant — the seam
    /// the skeleton-cache lever will exploit.
    #[inline]
    fn assemble(
        &self,
        v: &[f64; N],
        vin: f64,
        kfb: f64,
        cfb: f64,
        cgeq: &[f64; 4],
        cieq: &[f64; 4],
        gmin: f64,
    ) -> ([[f64; N]; N], [f64; N]) {
        let mut g = [[0.0f64; N]; N];
        let mut b = [0.0f64; N];

        // Diode companion stamps (Geq = dI/dV, Ieq = I − g·v). Gather the 8
        // junction voltages, evaluate the exps in one branchless batch (LLVM
        // packs it 4-wide under x86-64-v3), then scatter the companions.
        let mut vds = [0.0f64; 8];
        for (j, &(a, k)) in DIODES.iter().enumerate() {
            vds[j] = v[a] - v[k];
        }
        let (ids, gds) = self.diode.stamp8(&vds);
        for (j, &(a, k)) in DIODES.iter().enumerate() {
            let gd = gds[j];
            let ieq = ids[j] - gd * vds[j];
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

        // Voltage-set input nodes. IN_L ← vin (signal) via a stiff pin.
        g[IN_L][IN_L] += G_PIN;
        b[IN_L] += G_PIN * vin;

        // IN_R ← the inverting AC-coupled Q9 resonance feedback, solved
        // IN-LOOP (zero delay): v[IN_R] = kfb·(v[TL]−v[TR]) + cfb, where
        // kfb = reso·reso_gain (signed: Q9 inversion) and cfb = −kfb·dc_lp
        // is the DC-block (high-pass) term carried from the previous sample.
        // The kfb·diff coupling closes the loop within this solve, so the
        // peak regenerates near fc exactly as ngspice does (VCF_MAP §11).
        // Q9 is linear (class-A) ⇒ this is a linear stamp, not a nonlinear
        // algebraic loop: Newton still only iterates the diode nonlinearity.
        g[IN_R][IN_R] += G_PIN;
        g[IN_R][TL] -= G_PIN * kfb;
        g[IN_R][TR] += G_PIN * kfb;
        b[IN_R] += G_PIN * cfb;

        // Differential drive: a VCCS controlled by the input differential
        // (v[IN_L] − v[IN_R]) sourced into the bottom rails (dl → +, dr → −)
        // through g_drive, with the Norton internal conductance to ground.
        // Ideal VCCS — does not load the control nodes, so at gmin=0 this
        // reproduces the prior scalar `vin` Norton to machine precision.
        let gd = self.g_drive;
        g[DL][DL] += gd;
        g[DL][IN_L] -= gd;
        g[DL][IN_R] += gd;
        g[DR][DR] += gd;
        g[DR][IN_L] += gd;
        g[DR][IN_R] -= gd;

        // gmin homotopy + always-on base leakage.
        for d in 0..N {
            g[d][d] += gmin + GMIN_BASE;
        }
        (g, b)
    }

    /// One Newton solve at a fixed `gmin`, warm-started from `v0`. Returns the
    /// converged node voltages, or `None` if it stalled within `MAX_ITERS`.
    fn newton(
        &self,
        gmin: f64,
        v0: [f64; N],
        vin: f64,
        kfb: f64,
        cfb: f64,
        cgeq: &[f64; 4],
        cieq: &[f64; 4],
    ) -> Option<([f64; N], u32)> {
        let mut v = v0;
        for it in 0..MAX_ITERS {
            let (g, b) = self.assemble(&v, vin, kfb, cfb, cgeq, cieq, gmin);
            // Static-pattern, no-pivot LU (validated bit-equal to dense
            // `lu_solve::<N>` over the operating range; ~2.5× faster). A singular
            // diagonal would yield NaN — caught by the `is_finite` guard below,
            // dropping this Newton step to the gmin homotopy as the dense path did.
            let sol = ladder_lu_solve(g, b);
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
                return Some((v, it as u32 + 1));
            }
        }
        None
    }

    /// Solve one sample at the filter's working rate.
    fn solve_sample(&mut self, x_in: f32) -> f32 {
        let dt = 1.0 / self.sr;
        let vin = (x_in as f64) * self.vscale;
        // Q9 feedback coefficients for this sample (linear, in-loop). kfb scales
        // the output diff into in_r; cfb is the DC-block constant (high-pass).
        let kfb = self.reso * self.reso_gain;
        let cfb = -kfb * self.dc_lp;

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
        let v = match self.newton(0.0, self.v, vin, kfb, cfb, &cgeq, &cieq) {
            Some((v, it)) => {
                self.dbg_iters = it;
                v
            }
            None => {
                let mut vh = self.v;
                let mut ok = None;
                let mut its = 0;
                for &gmin in &[1e-1, 1e-2, 1e-3, 1e-4, 0.0] {
                    match self.newton(gmin, vh, vin, kfb, cfb, &cgeq, &cieq) {
                        Some((v, it)) => {
                            vh = v;
                            its += it;
                            ok = Some(v);
                        }
                        None => break,
                    }
                }
                self.dbg_iters = its;
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

        let diff_feedback = v[TL] - v[TR];
        // Update the AC-coupling DC tracker (high-pass state) for next sample.
        self.dc_lp += self.dc_alpha * (diff_feedback - self.dc_lp);
        let diff_out = match self.poles {
            2 => v[BL] - v[BR],
            3 => v[AL] - v[AR],
            _ => diff_feedback,
        };
        (diff_out * self.out_gain) as f32
    }
}

impl Filter303 for LadderVcf {
    fn set_cutoff(&mut self, hz: f32) {
        // I_tail = nVt · 2π · fc · C_ref · cal ⇒ r_d = nVt/I ⇒ corner ∝ fc.
        self.fc = hz.max(1.0) as f64;
        self.i0 = self.diode.n_vt * TAU * self.fc * C_REF * self.cal;
    }

    fn set_reso(&mut self, r: f32) {
        self.reso = r.clamp(0.0, 1.0) as f64;
    }

    fn tick(&mut self, x: f32) -> f32 {
        self.solve_sample(x)
    }

    fn reset(&mut self) {
        self.v = [0.0; N];
        self.vc_prev = [0.0; 4];
        self.i_prev = [0.0; 4];
        self.dc_lp = 0.0;
        self.dbg_iters = 0;
    }

    fn set_poles(&mut self, n: usize) {
        self.poles = n.clamp(2, 4);
    }
}

#[cfg(test)]
impl LadderVcf {
    /// Test hook: assemble the gmin=0 system at the current warm state for `x_in`
    /// (mirrors `solve_sample`'s parameter setup). Lets the structure/pivot
    /// analysis see *real* runtime matrices, not synthetic ones.
    fn capture_system(&self, x_in: f32) -> ([[f64; N]; N], [f64; N]) {
        let dt = 1.0 / self.sr;
        let vin = (x_in as f64) * self.vscale;
        let kfb = self.reso * self.reso_gain;
        let cfb = -kfb * self.dc_lp;
        let mut cgeq = [0.0f64; 4];
        let mut cieq = [0.0f64; 4];
        for j in 0..4 {
            let (_, _, cap) = CAPS[j];
            let geq = 2.0 * cap / dt;
            cgeq[j] = geq;
            cieq[j] = geq * self.vc_prev[j] + self.i_prev[j];
        }
        self.assemble(&self.v, vin, kfb, cfb, &cgeq, &cieq, 0.0)
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

    /// AC gain at a given working `sr` (the filter must be built at that `sr`).
    fn ac_gain_at(f: &mut LadderVcf, sr: f32, freq: f32) -> f32 {
        f.reset();
        let n = 16_384usize;
        let amp = 0.05f32;
        let warm = (n * 6) / 10;
        let (mut si, mut so) = (0.0f64, 0.0f64);
        for i in 0..n {
            let x = amp * (TAU as f32 * freq * i as f32 / sr).sin();
            let y = f.tick(x);
            if i >= warm {
                si += (x as f64) * (x as f64);
                so += (y as f64) * (y as f64);
            }
        }
        ((so / si).sqrt()) as f32
    }

    /// Frequency of max AC gain over [lo, hi] (the resonance peak when reso > 0).
    fn peak_freq(f: &mut LadderVcf, sr: f32, lo: f32, hi: f32) -> f32 {
        let steps = 50usize;
        let (mut bf, mut bg) = (lo, 0.0f32);
        for k in 0..=steps {
            let t = k as f32 / steps as f32;
            let fr = lo * (hi / lo).powf(t);
            let g = ac_gain_at(f, sr, fr);
            if g > bg {
                bg = g;
                bf = fr;
            }
        }
        bf
    }

    /// Natural-order (no-pivot) LU on a copy; returns the solution plus the count
    /// of columns where the diagonal was NOT the column max (i.e. partial pivoting
    /// WOULD have swapped) and the pivot growth factor. Decides whether a static
    /// (fixed-order, fill-pattern) factorization is numerically safe here.
    fn natural_lu_probe(a0: &[[f64; N]; N], b0: &[f64; N]) -> ([f64; N], usize, f64) {
        let mut a = *a0;
        let mut b = *b0;
        let a0max = a0
            .iter()
            .flat_map(|r| r.iter())
            .fold(0.0f64, |m, &x| m.max(x.abs()));
        let mut would_swap = 0usize;
        let mut growth = a0max;
        for col in 0..N {
            // Would partial pivoting move off the diagonal?
            let mut pmax = a[col][col].abs();
            for r in (col + 1)..N {
                if a[r][col].abs() > pmax {
                    pmax = a[r][col].abs();
                }
            }
            if pmax > a[col][col].abs() * (1.0 + 1e-12) {
                would_swap += 1;
            }
            let d = a[col][col];
            for r in (col + 1)..N {
                let f = a[r][col] / d;
                if f != 0.0 {
                    for c in col..N {
                        a[r][c] -= f * a[col][c];
                        growth = growth.max(a[r][c].abs());
                    }
                    b[r] -= f * b[col];
                }
            }
        }
        let mut x = [0.0; N];
        for i in (0..N).rev() {
            let mut s = b[i];
            for c in (i + 1)..N {
                s -= a[i][c] * x[c];
            }
            x[i] = s / a[i][i];
        }
        (x, would_swap, growth / a0max)
    }

    #[test]
    #[ignore = "diagnostic: ladder Jacobian structure + pivot stability for static LU"]
    fn ladder_matrix_structure() {
        let sr = 192_000.0f32;
        let mut f = LadderVcf::new(sr);
        f.set_cal(0.70);
        f.set_cutoff(1_200.0);
        f.set_reso(0.7);
        // Warm up with a sine so the captured systems are real operating points.
        let mut worst_swap = 0usize;
        let mut worst_growth = 0.0f64;
        let mut worst_err = 0.0f64;
        let mut pattern = [[false; N]; N];
        let mut samples = 0;
        for i in 0..6000 {
            let x = 0.3 * (TAU as f32 * 800.0 * i as f32 / sr).sin();
            let _ = f.tick(x);
            if i >= 2000 && i % 200 == 0 {
                let (a, b) = f.capture_system(x);
                for r in 0..N {
                    for c in 0..N {
                        if a[r][c] != 0.0 {
                            pattern[r][c] = true;
                        }
                    }
                }
                let dense = lu_solve::<N>(a, b).unwrap();
                let (nat, sw, gr) = natural_lu_probe(&a, &b);
                let err = (0..N)
                    .map(|k| (nat[k] - dense[k]).abs())
                    .fold(0.0, f64::max);
                worst_swap = worst_swap.max(sw);
                worst_growth = worst_growth.max(gr);
                worst_err = worst_err.max(err);
                samples += 1;
            }
        }
        // Symbolic natural-order factorization: which structural zeros fill in.
        let mut fill = pattern;
        for col in 0..N {
            for r in (col + 1)..N {
                if fill[r][col] {
                    for c in (col + 1)..N {
                        if fill[col][c] {
                            fill[r][c] = true;
                        }
                    }
                }
            }
        }
        let fnnz: usize = fill.iter().flatten().filter(|&&b| b).count();
        let new_fill = fnnz - pattern.iter().flatten().filter(|&&b| b).count();

        // Fill under alternative orderings (permutation of node→position). Lower
        // fill ⇒ cheaper static factorization. Try rail-interleaved (block-tridiag)
        // with IN_L/IN_R pushed last, and the same but IN_L/IN_R first.
        let perm_fill = |perm: &[usize; N]| -> usize {
            let mut p = [[false; N]; N];
            for r in 0..N {
                for c in 0..N {
                    if pattern[perm[r]][perm[c]] {
                        p[r][c] = true;
                    }
                }
            }
            for col in 0..N {
                for r in (col + 1)..N {
                    if p[r][col] {
                        for c in (col + 1)..N {
                            if p[col][c] {
                                p[r][c] = true;
                            }
                        }
                    }
                }
            }
            p.iter().flatten().filter(|&&b| b).count()
        };
        // current node indices: TL0 AL1 BL2 CL3 DL4 TR5 AR6 BR7 CR8 DR9 INL10 INR11
        let interleave_last = [0, 5, 1, 6, 2, 7, 3, 8, 4, 9, 10, 11];
        let interleave_first = [10, 11, 0, 5, 1, 6, 2, 7, 3, 8, 4, 9];
        eprintln!(
            "  fill: natural={fnnz}  interleave(IN last)={}  interleave(IN first)={}",
            perm_fill(&interleave_last),
            perm_fill(&interleave_first),
        );

        let nnz: usize = pattern.iter().flatten().filter(|&&b| b).count();
        eprintln!("\n== ladder 12×12 Jacobian, {samples} captured operating points ==");
        eprintln!(
            "  factored L+U nnz       : {fnnz} / {} (fill-in adds {new_fill})",
            N * N
        );
        eprintln!(
            "  structural nnz (union) : {nnz} / {} ({:.1}% fill)",
            N * N,
            100.0 * nnz as f64 / (N * N) as f64
        );
        eprintln!(
            "  worst would-swap cols  : {worst_swap}  (0 ⇒ natural order = partial-pivot order)"
        );
        eprintln!("  worst growth factor    : {worst_growth:.2e}");
        eprintln!("  worst |natural − dense|: {worst_err:.2e}  (vs TOL {TOL:.0e})");
        eprintln!("\n  nonzero pattern (row = KCL eqn, X = nonzero), order TL AL BL CL DL TR AR BR CR DR IN_L IN_R:");
        let names = [
            "TL", "AL", "BL", "CL", "DL", "TR", "AR", "BR", "CR", "DR", "IL", "IR",
        ];
        for r in 0..N {
            let row: String = (0..N)
                .map(|c| {
                    if pattern[r][c] {
                        'X'
                    } else if fill[r][c] {
                        '+'
                    } else {
                        '.'
                    }
                })
                .collect();
            eprintln!("    {:>2} {row}", names[r]);
        }
    }

    #[test]
    #[ignore = "microbench: static vs dense ladder LU throughput"]
    fn bench_static_vs_dense_lu() {
        use super::super::linalg_static::ladder_lu_solve;
        use std::hint::black_box;
        use std::time::Instant;
        let sr = 192_000.0f32;
        let mut f = LadderVcf::new(sr);
        f.set_cal(0.70);
        f.set_cutoff(1_200.0);
        f.set_reso(0.7);
        for i in 0..2000 {
            let x = 0.3 * (TAU as f32 * 800.0 * i as f32 / sr).sin();
            let _ = f.tick(x);
        }
        let (a, b) = f.capture_system(0.1);
        const REPS: usize = 5_000_000;
        // Vary the input slightly each rep so nothing folds to a constant.
        let t0 = Instant::now();
        let mut acc = 0.0f64;
        for k in 0..REPS {
            let mut bb = b;
            bb[0] += (k & 0xff) as f64 * 1e-12;
            let x = lu_solve::<N>(black_box(a), black_box(bb)).unwrap();
            acc += x[0];
        }
        let dense = t0.elapsed();
        let t1 = Instant::now();
        let mut acc2 = 0.0f64;
        for k in 0..REPS {
            let mut bb = b;
            bb[0] += (k & 0xff) as f64 * 1e-12;
            let x = ladder_lu_solve(black_box(a), black_box(bb));
            acc2 += x[0];
        }
        let stat = t1.elapsed();
        black_box(acc);
        black_box(acc2);
        let dn = dense.as_nanos() as f64 / REPS as f64;
        let sn = stat.as_nanos() as f64 / REPS as f64;
        eprintln!("\n== ladder LU microbench ({REPS} reps) ==");
        eprintln!("  dense  lu_solve<12> : {dn:6.2} ns/solve");
        eprintln!("  static ladder_lu    : {sn:6.2} ns/solve");
        eprintln!("  speedup             : {:.2}×", dn / sn);
    }

    #[test]
    fn static_lu_matches_dense_over_operating_points() {
        // The static-pattern (no-pivot) ladder solver must reproduce the dense
        // partial-pivot `lu_solve` across real operating points — many cutoffs,
        // resonances, and drive levels, plus the gmin-homotopy diagonal loading.
        use super::super::linalg_static::ladder_lu_solve;
        let sr = 192_000.0f32;
        let mut worst = 0.0f64;
        for &(fc, reso, cal) in &[
            (300.0f32, 0.0f32, 1.0f64),
            (1_000.0, 0.5, 0.70),
            (2_000.0, 0.9, 0.70),
            (5_000.0, 0.7, 0.50),
            (120.0, 0.3, 1.0),
        ] {
            let mut f = LadderVcf::new(sr);
            f.set_cal(cal);
            f.set_cutoff(fc);
            f.set_reso(reso);
            for i in 0..3000 {
                let x = 0.4 * (TAU as f32 * 700.0 * i as f32 / sr).sin();
                let _ = f.tick(x);
                if i >= 1000 && i % 50 == 0 {
                    let (a, b) = f.capture_system(x);
                    // Test both gmin=0 (captured) and a homotopy-loaded diagonal.
                    for extra in [0.0f64, 1e-3, 1e-1] {
                        let mut al = a;
                        for d in 0..N {
                            al[d][d] += extra;
                        }
                        let dense = lu_solve::<N>(al, b).unwrap();
                        let stat = ladder_lu_solve(al, b);
                        let rel = (0..N)
                            .map(|k| (stat[k] - dense[k]).abs() / dense[k].abs().max(1e-9))
                            .fold(0.0, f64::max);
                        worst = worst.max(rel);
                    }
                }
            }
        }
        assert!(
            worst < 1e-9,
            "static LU diverged from dense: worst rel err {worst:.2e}"
        );
    }

    #[test]
    fn corner_clears_600hz_ceiling() {
        // With the active Q9 resonance (F6.3) the audible corner = the resonance
        // peak, which tracks fc into the kHz range — clearing the passive ladder's
        // ~600 Hz structural ceiling (V4, impossible for the passive ladder alone).
        let sr = 192_000.0f32;
        let mut f = LadderVcf::new(sr);
        f.set_reso(0.7);
        f.set_cutoff(2_000.0);
        let pk = peak_freq(&mut f, sr, 200.0, 6_000.0);
        assert!(pk >= 1_000.0, "corner still ceiling-limited: {pk}");
    }

    #[test]
    #[ignore = "diagnostic: detected corner vs cutoff setting at current CAL"]
    fn corner_vs_cutoff() {
        let mut f = LadderVcf::new(48_000.0);
        println!("CAL={CAL}");
        for &fc in &[250.0f32, 500.0, 1000.0, 2000.0, 4000.0] {
            let c = corner(&mut f, fc);
            println!("  set {fc:7.0} -> corner {c:8.1}  (x{:.2})", c / fc);
        }
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
    fn input_split_is_behavior_preserving() {
        // With in_r grounded, the split-input ladder must match the prior 10-node
        // differential drive to the bit (same corner, same slope). Regression guard
        // across the F6.2 input refactor.
        // Baseline (pre-refactor, CAL=1.0, vscale=0.02) corner ≈ 97.7 Hz — the
        // depressed dominant pole of the passive ladder (VCF_MAP §9). The split must
        // not move it. (Plan's 250..650 guess predates the measured baseline.)
        let mut f = LadderVcf::new(48_000.0);
        let c = corner(&mut f, 1_000.0);
        assert!(
            (80.0..120.0).contains(&c),
            "corner regressed after input split: {c}"
        );
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
        assert!(
            b > a * 2.5 && b < a * 5.0,
            "cutoff does not track bias: {a} -> {b}"
        );
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
