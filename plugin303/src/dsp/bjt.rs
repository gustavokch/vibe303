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

use super::expmath::fast_exp;

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
}
