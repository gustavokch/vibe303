//! 1N4148 diode: MNA stamp + junction-cap companion (F3.2).
//!
//! Ported from `~/Git/ba283_wdf/main.cpp` (`diodeEval` + `depletionCharge`).
//! Model params from `eval/303/_vcf303_netlist_gen.py` `TRANSISTOR_MODELS`:
//! `IS=1.42e-11`, `N=1.7`, `CJO=1.74e-12`, `VJ=0.75`, `M=0.333`, `RS=4.2`.

use super::expmath::{fast_exp, fast_exp_branchless, fast_pow_pos};

/// Thermal voltage at 300 K is now dynamically calculated.
/// 1N4148 small-signal diode for the nodal ladder solver.
#[derive(Clone, Copy, Debug)]
pub struct Diode1N4148 {
    pub is: f64,
    pub n: f64,
    /// Emission-scaled thermal voltage `N·Vt`.
    pub n_vt: f64,
    /// SPICE `pnjlim` critical voltage; the exp is linearized beyond it.
    pub vcrit: f64,
    pub cjo: f64,
    pub vj: f64,
    pub m: f64,
}

impl Diode1N4148 {
    pub fn new() -> Self {
        let mut d = Self {
            is: 1.42e-11,
            n: 1.7,
            n_vt: 0.0,
            vcrit: 0.0,
            cjo: 1.74e-12,
            vj: 0.75,
            m: 0.333,
        };
        d.set_temp(25.0);
        d
    }

    pub fn set_temp(&mut self, temp_c: f64) {
        let tk = temp_c + 273.15;
        let vt = 1.380649e-23 * tk / 1.602176634e-19;
        self.n_vt = self.n * vt;
        self.vcrit = self.n_vt * (self.n_vt / (self.is * std::f64::consts::SQRT_2)).ln();
    }

    /// MNA contribution: returns `(I, g)` = (current into the junction,
    /// `dI/dV`). Beyond `vcrit` the exp is frozen to its C1-continuous linear
    /// extension so an overshot Newton step can't explode the residual.
    #[inline]
    pub fn stamp(&self, v: f64) -> (f64, f64) {
        if v < self.vcrit {
            let e = fast_exp(v / self.n_vt);
            let i = self.is * (e - 1.0);
            let g = (self.is / self.n_vt) * e;
            (i, g)
        } else {
            let ec = fast_exp(self.vcrit / self.n_vt);
            let icrit = self.is * (ec - 1.0);
            let gcrit = (self.is / self.n_vt) * ec;
            // C1-continuous linear extension.
            let i = icrit + gcrit * (v - self.vcrit);
            (i, gcrit)
        }
    }

    /// Batched [`stamp`] over 8 junctions of the *same* model (the ladder's 8
    /// diodes). Returns `(I, g)` arrays. Branchless on purpose: both the
    /// conducting Shockley branch and the `vcrit` linear extension are computed
    /// for every lane and blended with a `select`, so the inner exp loop carries
    /// no data-dependent control flow and LLVM packs it 4-wide under x86-64-v3.
    /// Value-identical to calling [`stamp`] per lane (uses [`fast_exp_branchless`],
    /// which matches [`fast_exp`] bit-for-bit on the junction domain).
    #[inline]
    pub fn stamp8(&self, vds: &[f64; 8]) -> ([f64; 8], [f64; 8]) {
        // Mirror the scalar `stamp` arithmetic op-for-op (division, not
        // reciprocal-multiply) so the result is bit-identical, not just close.
        let g_scale = self.is / self.n_vt;
        // vcrit-frozen branch constants (loop-invariant).
        let ec = fast_exp_branchless(self.vcrit / self.n_vt);
        let icrit = self.is * (ec - 1.0);
        let gcrit = g_scale * ec;
        let mut id = [0.0f64; 8];
        let mut gd = [0.0f64; 8];
        for i in 0..8 {
            let v = vds[i];
            let e = fast_exp_branchless(v / self.n_vt);
            let i_lo = self.is * (e - 1.0);
            let g_lo = g_scale * e;
            let i_hi = icrit + gcrit * (v - self.vcrit);
            let below = v < self.vcrit;
            id[i] = if below { i_lo } else { i_hi };
            gd[i] = if below { g_lo } else { gcrit };
        }
        (id, gd)
    }

    /// Junction depletion charge companion: returns `(Q, C)` with the standard
    /// FC=0.5 linear extrapolation above `FC·Vj` so it stays finite forward-biased.
    ///
    /// Not wired into the current ladder solver (its cap companions come from the
    /// explicit shunt `CAPS`, not diode junction caps). Retained + unit-tested as
    /// a validated model for the deferred active-stage Ultra headroom.
    #[inline]
    pub fn charge(&self, v: f64) -> (f64, f64) {
        const FC: f64 = 0.5;
        let (cj0, vj, m) = (self.cjo, self.vj, self.m);
        if cj0 <= 0.0 {
            return (0.0, 0.0);
        }
        if v < FC * vj {
            let f = 1.0 - v / vj;
            let c = cj0 * fast_pow_pos(f, -m);
            let q = cj0 * vj / (1.0 - m) * (1.0 - fast_pow_pos(f, 1.0 - m));
            (q, c)
        } else {
            let f0 = 1.0 - FC;
            let c0 = cj0 * fast_pow_pos(f0, -m);
            let q0 = cj0 * vj / (1.0 - m) * (1.0 - fast_pow_pos(f0, 1.0 - m));
            let dcdv = cj0 * m / vj * fast_pow_pos(f0, -m - 1.0);
            let d = v - FC * vj;
            let c = c0 + dcdv * d;
            let q = q0 + c0 * d + 0.5 * dcdv * d * d;
            (q, c)
        }
    }
}

impl Default for Diode1N4148 {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diode_current_and_conductance_match_shockley() {
        let d = Diode1N4148::new();
        for &v in &[-0.5, 0.0, 0.3, 0.6, 0.7] {
            let (i, g) = d.stamp(v);
            if v < d.vcrit {
                let want = d.is * ((v / d.n_vt).exp() - 1.0);
                assert!((i - want).abs() <= 1e-6 * want.abs().max(1e-9));
            }
            assert!(g > 0.0, "conductance must stay positive for Newton");
        }
    }

    #[test]
    fn diode_is_linear_beyond_vcrit() {
        // above vcrit the stamp must be the C1 linear extension (bounded exp), so an
        // overshot Newton step can't explode the residual.
        let d = Diode1N4148::new();
        let (_, g1) = d.stamp(d.vcrit + 0.1);
        let (_, g2) = d.stamp(d.vcrit + 0.2);
        assert!((g1 - g2).abs() < 1e-6, "conductance not frozen past vcrit");
    }

    #[test]
    fn stamp8_matches_scalar_stamp() {
        // The batched kernel must be value-identical to 8 scalar stamps, incl.
        // the vcrit-frozen branch, or the ladder spectral gates would drift.
        let d = Diode1N4148::new();
        let vds = [
            -0.5,
            0.0,
            0.3,
            0.6,
            d.vcrit - 0.01,
            d.vcrit + 0.05,
            0.9,
            -2.0,
        ];
        let (ids, gds) = d.stamp8(&vds);
        for (i, &v) in vds.iter().enumerate() {
            let (is, gs) = d.stamp(v);
            assert_eq!(ids[i].to_bits(), is.to_bits(), "I lane {i} (v={v})");
            assert_eq!(gds[i].to_bits(), gs.to_bits(), "g lane {i} (v={v})");
        }
    }

    #[test]
    fn depletion_cap_is_finite_forward_biased() {
        let d = Diode1N4148::new();
        let (_q, c) = d.charge(0.74); // near/over FC*Vj must stay finite (FC extrapolation)
        assert!(c.is_finite() && c > 0.0);
    }
}
