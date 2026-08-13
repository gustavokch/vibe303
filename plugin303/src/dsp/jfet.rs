//! MPF104 N-JFET: Shichman-Hodges channel + gate junction diodes (F3-VCO V2).
//!
//! SPICE NJF level-1 model. Params from `eval/303/_vco303_netlist_gen` MPF104:
//! `BETA=4.889e-4`, `VTO=-1.45` (depletion: conducts at vgs=0), `LAMBDA=3.83e-3`
//! (channel-length modulation), gate diodes `IS=1.813e-13`, `N=1`. The channel is
//! source-drain symmetric — for `vds < 0` the source and drain swap roles.
//!
//! In vco303 `Q1` (drain=`vp`, gate=`cp`, source=`bf`) is the integrator reset
//! switch, so `vds = vp − bf ≥ 0` in normal operation; the symmetric branch keeps
//! Newton well-behaved during transients.

use super::expmath::fast_exp;

const VT: f64 = 0.025_852;

/// MPF104 N-JFET device.
#[derive(Clone, Copy, Debug)]
pub struct Jfet {
    pub beta: f64,
    pub vto: f64,
    pub lambda: f64,
    pub is: f64,
    pub vcrit: f64,
}

/// Channel solution: drain current + the two channel conductances.
#[derive(Clone, Copy, Debug, Default)]
pub struct JfetCh {
    pub id: f64,
    pub gm: f64,  // dId/dVgs
    pub gds: f64, // dId/dVds
}

impl Jfet {
    pub fn mpf104() -> Self {
        let is = 1.813e-13;
        let vcrit = VT * (VT / (is * std::f64::consts::SQRT_2)).ln();
        Self {
            beta: 4.889e-4,
            vto: -1.45,
            lambda: 3.83e-3,
            is,
            vcrit,
        }
    }

    /// Channel drain current `(Id, gm, gds)` for true terminal voltages
    /// `vgs`, `vds`. Symmetric in the source/drain swap for `vds < 0`.
    pub fn channel(&self, vgs: f64, vds: f64) -> JfetCh {
        if vds >= 0.0 {
            self.channel_fwd(vgs, vds)
        } else {
            // Swap drain/source: effective gate-source ref becomes vgs - vds = vgd.
            let r = self.channel_fwd(vgs - vds, -vds);
            // Reverse-mode drain current flows the other way; conductances re-map
            // under the chain rule with (vgs'→vgs−vds, vds'→−vds):
            //   Id  = −Idf,                       so id  = −r.id
            //   dId/dVgs = −(r.gm·1)            = −r.gm
            //   dId/dVds = −(r.gm·(−1)+r.gds·(−1)) = r.gm + r.gds
            JfetCh {
                id: -r.id,
                gm: -r.gm,
                gds: r.gm + r.gds,
            }
        }
    }

    /// Forward channel (`vds ≥ 0`).
    fn channel_fwd(&self, vgs: f64, vds: f64) -> JfetCh {
        let vgst = vgs - self.vto;
        if vgst <= 0.0 {
            return JfetCh::default(); // cutoff
        }
        let lam = self.lambda;
        let b = self.beta;
        if vds < vgst {
            // Triode: Id = β·vds·(2·vgst − vds)·(1 + λ·vds).
            let core = vds * (2.0 * vgst - vds);
            let id = b * core * (1.0 + lam * vds);
            let gm = b * (2.0 * vds) * (1.0 + lam * vds);
            let dcore_dvds = 2.0 * vgst - 2.0 * vds;
            let gds = b * (dcore_dvds * (1.0 + lam * vds) + core * lam);
            JfetCh { id, gm, gds }
        } else {
            // Saturation: Id = β·vgst²·(1 + λ·vds).
            let id = b * vgst * vgst * (1.0 + lam * vds);
            let gm = b * 2.0 * vgst * (1.0 + lam * vds);
            let gds = b * vgst * vgst * lam;
            JfetCh { id, gm, gds }
        }
    }

    /// Gate junction diode current `(I, g)` for a gate-channel voltage `v`
    /// (vcrit-linearized; gates are normally reverse-biased).
    pub fn gate_diode(&self, v: f64) -> (f64, f64) {
        if v < self.vcrit {
            let e = fast_exp(v / VT);
            (self.is * (e - 1.0), (self.is / VT) * e)
        } else {
            let ec = fast_exp(self.vcrit / VT);
            let icrit = self.is * (ec - 1.0);
            let gcrit = (self.is / VT) * ec;
            (icrit + gcrit * (v - self.vcrit), gcrit)
        }
    }

    /// Current-iteration variant of gate_diode: maintains full exponential beyond V_CRIT.
    #[cfg(feature = "current-iteration")]
    pub fn gate_diode_current_iteration(&self, v: f64) -> (f64, f64) {
        if v < self.vcrit {
            let e = fast_exp(v / VT);
            (self.is * (e - 1.0), (self.is / VT) * e)
        } else {
            // Full exponential beyond V_CRIT - no linearization
            let e = fast_exp(v / VT);
            (self.is * (e - 1.0), (self.is / VT) * e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cutoff_below_pinchoff() {
        let j = Jfet::mpf104();
        // vgs below VTO=-1.45 ⇒ no channel.
        let c = j.channel(-2.0, 5.0);
        assert_eq!(c.id, 0.0);
        assert_eq!(c.gm, 0.0);
    }

    #[test]
    fn conducts_at_zero_vgs() {
        // Depletion JFET: vgs=0 is well above pinchoff, conducts.
        let j = Jfet::mpf104();
        let c = j.channel(0.0, 5.0);
        assert!(c.id > 0.0 && c.gm > 0.0, "should conduct at vgs=0: {c:?}");
    }

    #[test]
    fn square_law_in_saturation() {
        let j = Jfet::mpf104();
        // vgst halves (1.45 -> 0.725) ⇒ Id quarters (square law), modulo small λ.
        let big = j.channel(0.0, 5.0).id; // vgst = 1.45
        let half = j.channel(-0.725, 5.0).id; // vgst = 0.725
        let ratio = big / half;
        assert!((3.6..4.4).contains(&ratio), "not square-law: ratio {ratio}");
    }

    #[test]
    fn triode_has_positive_output_conductance() {
        let j = Jfet::mpf104();
        // Small vds (< vgst=1.45) ⇒ triode region: gds > 0 and rises with vds toward sat.
        let lo = j.channel(0.0, 0.2);
        let hi = j.channel(0.0, 0.6);
        assert!(
            lo.gds > 0.0 && hi.gds > 0.0,
            "gds must be positive in triode"
        );
        assert!(hi.id > lo.id, "Id must rise with vds in triode");
    }

    #[test]
    fn reverse_mode_gm_matches_finite_difference() {
        // For vds < 0 the channel runs in reverse; the returned gm = dId/dVgs
        // must match a numeric derivative (regression for the dropped sign).
        let j = Jfet::mpf104();
        let vgs = 0.3;
        let vds = -0.4; // reverse mode
        let h = 1e-6;
        let c = j.channel(vgs, vds);
        let num_gm = (j.channel(vgs + h, vds).id - j.channel(vgs - h, vds).id) / (2.0 * h);
        let num_gds = (j.channel(vgs, vds + h).id - j.channel(vgs, vds - h).id) / (2.0 * h);
        assert!(
            (c.gm - num_gm).abs() < 1e-4 * num_gm.abs().max(1e-6),
            "gm {} != d/dVgs {num_gm}",
            c.gm
        );
        assert!(
            (c.gds - num_gds).abs() < 1e-4 * num_gds.abs().max(1e-6),
            "gds {} != d/dVds {num_gds}",
            c.gds
        );
    }

    #[test]
    fn gate_diode_conducts_forward() {
        let j = Jfet::mpf104();
        let (i, g) = j.gate_diode(0.4);
        assert!(i > 0.0 && g > 0.0);
        let (ir, _) = j.gate_diode(-1.0);
        assert!(ir < 0.0 && ir > -2.0 * j.is, "reverse current ~ -Is");
    }

    #[cfg(feature = "current-iteration")]
    #[test]
    fn gate_diode_current_iteration_matches_below_vcrit() {
        // Below V_CRIT, current-iteration should match standard gate_diode
        let j = Jfet::mpf104();
        let v = 0.4; // Below vcrit (~0.66V for MPF104)

        let (i_std, g_std) = j.gate_diode(v);
        let (i_ci, g_ci) = j.gate_diode_current_iteration(v);

        assert_eq!(i_std.to_bits(), i_ci.to_bits(), "current mismatch below V_CRIT");
        assert_eq!(g_std.to_bits(), g_ci.to_bits(), "conductance mismatch below V_CRIT");
    }

    #[cfg(feature = "current-iteration")]
    #[test]
    fn gate_diode_current_iteration_exponential_above_vcrit() {
        // Above V_CRIT, current-iteration should use full exponential
        let j = Jfet::mpf104();
        let v_above = j.vcrit + 0.1;

        let (i_std, g_std) = j.gate_diode(v_above);
        let (i_ci, g_ci) = j.gate_diode_current_iteration(v_above);

        // Standard linearizes (conductance frozen), current-iteration uses full exponential
        assert!((g_std - g_ci).abs() > 1e-6, "current-iteration should differ from linear extension");
        // Current-iteration should give higher current (full exponential)
        assert!(i_ci > i_std, "current-iteration current should be higher above V_CRIT");
    }

    #[cfg(feature = "current-iteration")]
    #[test]
    fn gate_diode_current_iteration_conductance_positive() {
        // Current-iteration should maintain positive conductance for convergence
        let j = Jfet::mpf104();
        for v in [-2.0, -1.0, -0.5, 0.0, 0.3, 0.4, j.vcrit - 0.01, j.vcrit + 0.05, 0.8] {
            let (_, g) = j.gate_diode_current_iteration(v);
            assert!(g > 0.0, "conductance must be positive at v={}", v);
        }
    }
}
