//! Gummel-Poon BJT: terminal currents + node-form Jacobian (F3-VCO V1).
//!
//! Ported from `~/Git/ba283_wdf/main.cpp` `computeBjt` (lines 208-261): transport
//! currents via the Shockley exp, base-charge factor `qb` with Early effect (VAF)
//! and high-injection (IKF), reduced to plain transport when those are disabled.
//! PNP is handled by flipping the junction voltages and negating the currents;
//! the node-form conductances are sign-invariant under that flip (both `I` and
//! `V` negate), so the Jacobian carries through unchanged.
//!
//! Params from `eval/303/_vco303_netlist_gen.TRANSISTOR_MODELS` (2N3904 / 2N3906).

use super::expmath::{fast_exp, fast_exp_branchless};

/// Thermal voltage at 300 K was previously a constant. Now dynamically calculated in BjtParams.
/// Static Gummel-Poon parameters (the subset the BA283 transport model uses).
#[derive(Clone, Copy, Debug)]
pub struct BjtParams {
    pub is: f64,
    pub bf: f64,
    pub br: f64,
    pub vaf: f64, // Early voltage forward (0 ⇒ disabled)
    pub var: f64, // Early voltage reverse (0 ⇒ disabled)
    pub ikf: f64, // high-injection knee forward (0 ⇒ disabled)
    pub ikr: f64, // high-injection knee reverse (0 ⇒ disabled)
    pub vcrit: f64,
    pub vt: f64,
}

impl BjtParams {
    fn make(is: f64, bf: f64, br: f64, vaf: f64, var: f64, ikf: f64, ikr: f64) -> Self {
        let mut p = Self {
            is,
            bf,
            br,
            vaf,
            var,
            ikf,
            ikr,
            vcrit: 0.0,
            vt: 0.025_852,
        };
        p.set_temp(25.0);
        p
    }

    pub fn set_temp(&mut self, temp_c: f64) {
        let tk = temp_c + 273.15;
        self.vt = 1.380649e-23 * tk / 1.602176634e-19;
        self.vcrit = self.vt * (self.vt / (self.is * std::f64::consts::SQRT_2)).ln();
    }

    /// 2N3904 NPN.
    pub fn n2n3904() -> Self {
        Self::make(6.73e-15, 416.4, 0.7374, 74.03, 0.0, 0.06678, 0.0)
    }

    /// 2N3906 PNP.
    pub fn p2n3906() -> Self {
        Self::make(4e-14, 400.0, 7.5, 50.0, 0.0, 0.02, 0.0)
    }
}

/// Terminal currents (into the device) + node-form Jacobian. `d{Ic,Ib,Ie}_{b,e,c}`
/// are `dI_terminal/dV_node`. `Itf/gtf/Itr/gtr` feed the diffusion-charge caps.
#[derive(Clone, Copy, Debug, Default)]
pub struct BjtOut {
    pub ib: f64,
    pub ic: f64,
    pub ie: f64,
    pub itf: f64,
    pub gtf: f64,
    pub itr: f64,
    pub gtr: f64,
    pub dib_b: f64,
    pub dib_e: f64,
    pub dib_c: f64,
    pub dic_b: f64,
    pub dic_e: f64,
    pub dic_c: f64,
    pub die_b: f64,
    pub die_e: f64,
    pub die_c: f64,
}

/// vcrit-linearized transport current `(I, g)` (SPICE pnjlim spirit; same form as
/// `diode::stamp` but with the device's own `Is`/`Vt`).
#[inline]
fn transport(v: f64, is: f64, vcrit: f64, vt: f64) -> (f64, f64) {
    if v < vcrit {
        let e = fast_exp(v / vt);
        (is * (e - 1.0), (is / vt) * e)
    } else {
        let ec = fast_exp(vcrit / vt);
        let icrit = is * (ec - 1.0);
        let gcrit = (is / vt) * ec;
        (icrit + gcrit * (v - vcrit), gcrit)
    }
}

/// Current-iteration variant of transport: maintains full exponential beyond V_CRIT.
/// Follows the Newton-Raphson document's approach, preserving device physics at all
/// bias levels for potentially faster convergence.
#[cfg(feature = "current-iteration")]
#[inline]
fn transport_current_iteration(v: f64, is: f64, vcrit: f64, vt: f64) -> (f64, f64) {
    if v < vcrit {
        let e = fast_exp(v / vt);
        (is * (e - 1.0), (is / vt) * e)
    } else {
        // Full exponential beyond V_CRIT - no linearization
        let e = fast_exp(v / vt);
        (is * (e - 1.0), (is / vt) * e)
    }
}

/// A BJT device: parameters + polarity.
#[derive(Clone, Copy, Debug)]
pub struct Bjt {
    pub p: BjtParams,
    pnp: bool,
}

impl Bjt {
    pub fn npn(p: BjtParams) -> Self {
        Self { p, pnp: false }
    }
    pub fn pnp(p: BjtParams) -> Self {
        Self { p, pnp: true }
    }

    /// Solve at junction voltages `vbe`, `vbc` (true terminal voltages, any
    /// polarity). Returns currents into the device and the node-form Jacobian.
    pub fn solve(&self, vbe: f64, vbc: f64) -> BjtOut {
        // PNP: flip junction voltages; currents negate; node conductances invariant.
        let s = if self.pnp { -1.0 } else { 1.0 };
        let mut o = self.solve_npn(s * vbe, s * vbc);
        if self.pnp {
            o.ib = -o.ib;
            o.ic = -o.ic;
            o.ie = -o.ie;
            o.itf = -o.itf;
            o.itr = -o.itr;
        }
        o
    }

    fn solve_npn(&self, vbe: f64, vbc: f64) -> BjtOut {
        let p = &self.p;
        let (itf, gtf) = transport(vbe, p.is, p.vcrit, p.vt);
        let (itr, gtr) = transport(vbc, p.is, p.vcrit, p.vt);
        Self::finish_npn(p, vbe, vbc, itf, gtf, itr, gtr)
    }

    /// Batched solve over the VCO's 6 BJTs (12 junctions), branchless on the
    /// data-dependent `v < vcrit` transport branch (stamp8 pattern): both
    /// branches are computed per lane and blended with a `select`, so the exp
    /// loop carries no data-dependent control flow. Bit-identical to calling
    /// [`solve`](Self::solve) per device — same op order, `fast_exp_branchless`
    /// matches `fast_exp` bit-for-bit on the junction domain.
    pub fn solve_batch6(devs: &[Bjt; 6], vbe: &[f64; 6], vbc: &[f64; 6]) -> [BjtOut; 6] {
        // Gather the 12 signed junction voltages (pnp: s = -1.0) and per-lane
        // params (each device spans 2 lanes: vbe then vbc).
        let mut vs = [0.0f64; 12];
        let mut is = [0.0f64; 12];
        let mut vt = [0.0f64; 12];
        let mut vcrit = [0.0f64; 12];
        let mut icrit = [0.0f64; 12];
        let mut gcrit = [0.0f64; 12];
        for j in 0..6 {
            let s = if devs[j].pnp { -1.0 } else { 1.0 };
            vs[2 * j] = s * vbe[j];
            vs[2 * j + 1] = s * vbc[j];
            let p = &devs[j].p;
            // vcrit-frozen branch constants (loop-invariant per device; the
            // scalar else-branch recomputes them per call — values identical).
            let ec = fast_exp_branchless(p.vcrit / p.vt);
            let ic = p.is * (ec - 1.0);
            let gc = (p.is / p.vt) * ec;
            for l in 0..2 {
                is[2 * j + l] = p.is;
                vt[2 * j + l] = p.vt;
                vcrit[2 * j + l] = p.vcrit;
                icrit[2 * j + l] = ic;
                gcrit[2 * j + l] = gc;
            }
        }
        // One branchless batch over 12 lanes. Mirror the scalar `transport`
        // arithmetic op-for-op (division, not reciprocal-multiply).
        let mut it = [0.0f64; 12];
        let mut gt = [0.0f64; 12];
        for i in 0..12 {
            let v = vs[i];
            let e = fast_exp_branchless(v / vt[i]);
            let i_lo = is[i] * (e - 1.0);
            let g_lo = (is[i] / vt[i]) * e;
            let i_hi = icrit[i] + gcrit[i] * (v - vcrit[i]);
            let below = v < vcrit[i];
            it[i] = if below { i_lo } else { i_hi };
            gt[i] = if below { g_lo } else { gcrit[i] };
        }
        let mut out = [BjtOut::default(); 6];
        for j in 0..6 {
            let mut o = Self::finish_npn(
                &devs[j].p,
                vs[2 * j],
                vs[2 * j + 1],
                it[2 * j],
                gt[2 * j],
                it[2 * j + 1],
                gt[2 * j + 1],
            );
            // PNP output negation, same 5 fields as `solve`.
            if devs[j].pnp {
                o.ib = -o.ib;
                o.ic = -o.ic;
                o.ie = -o.ie;
                o.itf = -o.itf;
                o.itr = -o.itr;
            }
            out[j] = o;
        }
        out
    }

    /// Gummel-Poon algebra after the two junction transports: base-charge factor
    /// `qb` (Early + high-injection), terminal currents, node-form Jacobian.
    /// Shared by the scalar and batched paths — both feed it identical
    /// `(itf, gtf, itr, gtr)` values.
    fn finish_npn(
        p: &BjtParams,
        vbe: f64,
        vbc: f64,
        itf: f64,
        gtf: f64,
        itr: f64,
        gtr: f64,
    ) -> BjtOut {
        // Base-charge factor qb = (q1/2)(1+sqrt(1+4 q2)).
        let inv_af = if p.vaf > 0.0 { 1.0 / p.vaf } else { 0.0 };
        let inv_ar = if p.var > 0.0 { 1.0 / p.var } else { 0.0 };
        let mut inv1 = 1.0 - vbc * inv_af - vbe * inv_ar;
        let inv1_clamped = inv1 < 0.1;
        if inv1_clamped {
            inv1 = 0.1; // guard against blow-up in saturation
        }
        let q1 = 1.0 / inv1;
        // When inv1 is clamped, q1 is frozen ⇒ its derivatives vanish. Keeping
        // the unclamped derivatives would feed Newton a Jacobian that disagrees
        // with the clamped residual, stalling convergence in deep saturation.
        let (dq1_vbe, dq1_vbc) = if inv1_clamped {
            (0.0, 0.0)
        } else {
            (q1 * q1 * inv_ar, q1 * q1 * inv_af)
        };
        let inv_kf = if p.ikf > 0.0 { 1.0 / p.ikf } else { 0.0 };
        let inv_kr = if p.ikr > 0.0 { 1.0 / p.ikr } else { 0.0 };
        let q2 = itf * inv_kf + itr * inv_kr;
        let dq2_vbe = gtf * inv_kf;
        let dq2_vbc = gtr * inv_kr;
        let sq = (1.0 + 4.0 * q2).sqrt();
        let qb = q1 * (1.0 + sq) / 2.0;
        let dqb_vbe = dq1_vbe * (1.0 + sq) / 2.0 + q1 * dq2_vbe / sq;
        let dqb_vbc = dq1_vbc * (1.0 + sq) / 2.0 + q1 * dq2_vbc / sq;

        let diff = itf - itr;
        let it = diff / qb;
        let dit_vbe = (gtf * qb - diff * dqb_vbe) / (qb * qb);
        let dit_vbc = (-gtr * qb - diff * dqb_vbc) / (qb * qb);

        let inv_bf = 1.0 / p.bf;
        let inv_br = 1.0 / p.br;
        let ic = it - itr * inv_br;
        let ib = itf * inv_bf + itr * inv_br;
        let ie = -(ic + ib);

        let dic_vbe = dit_vbe;
        let dic_vbc = dit_vbc - gtr * inv_br;
        let dib_vbe = gtf * inv_bf;
        let dib_vbc = gtr * inv_br;
        let die_vbe = -(dic_vbe + dib_vbe);
        let die_vbc = -(dic_vbc + dib_vbc);

        // Node form: d/dVb = d/dvbe + d/dvbc, d/dVe = -d/dvbe, d/dVc = -d/dvbc.
        BjtOut {
            ib,
            ic,
            ie,
            itf,
            gtf,
            itr,
            gtr,
            dic_b: dic_vbe + dic_vbc,
            dic_e: -dic_vbe,
            dic_c: -dic_vbc,
            dib_b: dib_vbe + dib_vbc,
            dib_e: -dib_vbe,
            dib_c: -dib_vbc,
            die_b: die_vbe + die_vbc,
            die_e: -die_vbe,
            die_c: -die_vbc,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solve_batch6_matches_scalar_solve_bitwise() {
        // The batched kernel must be value-identical to 6 scalar solves (stamp8
        // pattern: both transport branches computed per lane, blended by select),
        // or the VCO Newton trajectory — and the spectral gates — would drift.
        let npn = Bjt::npn(BjtParams::n2n3904());
        let pnp = Bjt::pnp(BjtParams::p2n3906());
        let devs = [pnp, npn, pnp, npn, npn, pnp]; // Q6,Q7,Q4,Q5,Q3,Q2 order
        // Biases straddling vcrit on both junctions, incl. vbc = 0.0 (Q5 case),
        // deep saturation (inv1 < 0.1 clamp), and forward-active.
        let cases: &[([f64; 6], [f64; 6])] = &[
            // forward-active, all below vcrit
            ([0.60, 0.65, 0.55, 0.70, 0.62, 0.58], [-1.0, 0.5, -2.0, 0.0, -0.3, 1.2]),
            // saturated: vbc > 0 (forward-active BC junction), hits inv1 clamp
            ([0.75, 0.80, 0.72, 0.78, 0.76, 0.74], [0.60, 0.55, 0.65, 0.0, 0.50, 0.62]),
            // beyond vcrit (linearized branch): vcrit ~ 0.4-0.6 for these presets
            ([1.20, 1.10, 1.30, 0.95, 1.05, 1.15], [1.00, 0.90, 1.10, 0.0, 0.85, 0.95]),
        ];
        for (vbe, vbc) in cases {
            let batch = Bjt::solve_batch6(&devs, vbe, vbc);
            for j in 0..6 {
                let s = devs[j].solve(vbe[j], vbc[j]);
                let b = &batch[j];
                assert_eq!(b.ib.to_bits(), s.ib.to_bits(), "ib dev {j}");
                assert_eq!(b.ic.to_bits(), s.ic.to_bits(), "ic dev {j}");
                assert_eq!(b.ie.to_bits(), s.ie.to_bits(), "ie dev {j}");
                assert_eq!(b.itf.to_bits(), s.itf.to_bits(), "itf dev {j}");
                assert_eq!(b.gtf.to_bits(), s.gtf.to_bits(), "gtf dev {j}");
                assert_eq!(b.itr.to_bits(), s.itr.to_bits(), "itr dev {j}");
                assert_eq!(b.gtr.to_bits(), s.gtr.to_bits(), "gtr dev {j}");
                assert_eq!(b.dib_b.to_bits(), s.dib_b.to_bits(), "dib_b dev {j}");
                assert_eq!(b.dib_e.to_bits(), s.dib_e.to_bits(), "dib_e dev {j}");
                assert_eq!(b.dib_c.to_bits(), s.dib_c.to_bits(), "dib_c dev {j}");
                assert_eq!(b.dic_b.to_bits(), s.dic_b.to_bits(), "dic_b dev {j}");
                assert_eq!(b.dic_e.to_bits(), s.dic_e.to_bits(), "dic_e dev {j}");
                assert_eq!(b.dic_c.to_bits(), s.dic_c.to_bits(), "dic_c dev {j}");
                assert_eq!(b.die_b.to_bits(), s.die_b.to_bits(), "die_b dev {j}");
                assert_eq!(b.die_e.to_bits(), s.die_e.to_bits(), "die_e dev {j}");
                assert_eq!(b.die_c.to_bits(), s.die_c.to_bits(), "die_c dev {j}");
            }
        }
    }

    #[test]
    fn forward_active_beta_near_bf() {
        let q = Bjt::npn(BjtParams::n2n3904());
        // vbe = 0.65 forward, collector reverse-biased (vbc = -2).
        let o = q.solve(0.65, -2.0);
        assert!(
            o.ic > 0.0 && o.ib > 0.0,
            "should conduct: ic {} ib {}",
            o.ic,
            o.ib
        );
        let beta = o.ic / o.ib;
        assert!(
            beta > 0.7 * 416.4 && beta < 1.05 * 416.4,
            "beta {beta} not near BF=416.4"
        );
    }

    #[test]
    fn kcl_holds() {
        let q = Bjt::npn(BjtParams::n2n3904());
        let o = q.solve(0.6, -1.0);
        assert!((o.ie + o.ic + o.ib).abs() < 1e-12, "Ie != -(Ic+Ib)");
    }

    #[test]
    fn collector_current_monotone_in_vbe() {
        let q = Bjt::npn(BjtParams::n2n3904());
        let a = q.solve(0.60, -1.0).ic;
        let b = q.solve(0.65, -1.0).ic;
        let c = q.solve(0.70, -1.0).ic;
        assert!(a < b && b < c, "Ic not monotone: {a} {b} {c}");
        // Transconductance gm = dIc/dVb must stay positive (Newton needs it).
        assert!(q.solve(0.65, -1.0).dic_b > 0.0, "gm <= 0");
    }

    #[test]
    fn pnp_mirrors_npn() {
        let n = Bjt::npn(BjtParams::n2n3904());
        let p = Bjt::pnp(BjtParams::p2n3906());
        // PNP forward = base below emitter ⇒ vbe < 0; collector reverse ⇒ vbc > 0.
        let on = n.solve(0.65, -2.0);
        let op = p.solve(-0.65, 2.0);
        assert!(
            op.ic < 0.0 && op.ib < 0.0,
            "PNP currents should be negative"
        );
        assert!(op.dic_b > 0.0, "PNP node gm sign should match NPN");
        assert!(on.ic > 0.0);
    }

    #[cfg(feature = "current-iteration")]
    #[test]
    fn transport_current_iteration_matches_below_vcrit() {
        // Below V_CRIT, current-iteration should match standard transport
        let p = BjtParams::n2n3904();
        let v = 0.6; // Below vcrit (~0.73V for 2N3904)

        let (i_std, g_std) = transport(v, p.is, p.vcrit, p.vt);
        let (i_ci, g_ci) = transport_current_iteration(v, p.is, p.vcrit, p.vt);

        assert_eq!(
            i_std.to_bits(),
            i_ci.to_bits(),
            "current mismatch below V_CRIT"
        );
        assert_eq!(
            g_std.to_bits(),
            g_ci.to_bits(),
            "conductance mismatch below V_CRIT"
        );
    }

    #[cfg(feature = "current-iteration")]
    #[test]
    fn transport_current_iteration_exponential_above_vcrit() {
        // Above V_CRIT, current-iteration should use full exponential
        let p = BjtParams::n2n3904();
        let v_above = p.vcrit + 0.1;

        let (i_std, g_std) = transport(v_above, p.is, p.vcrit, p.vt);
        let (i_ci, g_ci) = transport_current_iteration(v_above, p.is, p.vcrit, p.vt);

        // Standard linearizes (conductance frozen), current-iteration uses full exponential
        assert!(
            (g_std - g_ci).abs() > 1e-6,
            "current-iteration should differ from linear extension"
        );
        // Current-iteration should give higher current (full exponential)
        assert!(
            i_ci > i_std,
            "current-iteration current should be higher above V_CRIT"
        );
    }

    #[cfg(feature = "current-iteration")]
    #[test]
    fn transport_current_iteration_conductance_positive() {
        // Current-iteration should maintain positive conductance for convergence
        let p = BjtParams::n2n3904();
        for v in [
            -2.0,
            -0.5,
            0.0,
            0.3,
            0.6,
            p.vcrit - 0.01,
            p.vcrit + 0.05,
            1.0,
        ] {
            let (_, g) = transport_current_iteration(v, p.is, p.vcrit, p.vt);
            assert!(g > 0.0, "conductance must be positive at v={}", v);
        }
    }
}
