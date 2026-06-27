//! 1N4148 diode: MNA stamp + junction-cap companion (F3.2).
//!
//! Ported from `~/Git/ba283_wdf/main.cpp` (`diodeEval` + `depletionCharge`).
//! Model params from `eval/303/_vcf303_netlist_gen.py` `TRANSISTOR_MODELS`:
//! `IS=1.42e-11`, `N=1.7`, `CJO=1.74e-12`, `VJ=0.75`, `M=0.333`, `RS=4.2`.

use super::expmath::{fast_exp, fast_pow_pos};

/// Thermal voltage at 300 K.
const VT: f64 = 0.025_852;

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
        let is = 1.42e-11;
        let n = 1.7;
        let n_vt = n * VT;
        // SPICE pnjlim: vcrit = nVt * ln(nVt / (IS * sqrt(2))).
        let vcrit = n_vt * (n_vt / (is * std::f64::consts::SQRT_2)).ln();
        Self {
            is,
            n,
            n_vt,
            vcrit,
            cjo: 1.74e-12,
            vj: 0.75,
            m: 0.333,
        }
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
    fn depletion_cap_is_finite_forward_biased() {
        let d = Diode1N4148::new();
        let (_q, c) = d.charge(0.74); // near/over FC*Vj must stay finite (FC extrapolation)
        assert!(c.is_finite() && c > 0.0);
    }
}
