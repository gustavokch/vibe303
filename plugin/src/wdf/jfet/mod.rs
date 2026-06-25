//! JFET device model (feature `jfet`) — a **creative tonal voicing**, not a faithful model of any
//! real JFET-based BA283 variant. Drops a Shichman-Hodges square-law N-channel JFET into all three
//! transistor slots of the Class A circuit as an A/B alternative to the stock Gummel-Poon BJTs.
//!
//! ## How it slots into the existing solver
//! The Newton scatter (`newton.rs`) consumes a [`BjtOutSoA`] per width-4 batch — 3 terminal currents
//! + a 16-field node-form Jacobian — and stamps it into `F`/`J` unchanged. The JFET kernel here
//! returns that **same** struct, with the device's three terminals mapped onto the BJT node slots:
//!
//! | JFET terminal | BJT slot | gather voltage |
//! |---------------|----------|----------------|
//! | gate          | base     | `vgs = vbe4`   |
//! | source        | emitter  | (— )           |
//! | drain         | collector| `vgd = vbc4`   |
//!
//! So `vgs = Vg − Vs` reuses the BJT's `vbe = Vb − Ve` gather and `vgd = Vg − Vd` reuses `vbc`. The
//! node-form Jacobian conversion is identical to the BJT's (`d/dVg = d/dvgs + d/dvgd`, etc.).
//!
//! ## Two gate junctions for stability
//! A pure square-law channel has zero conductance in cutoff (`gm = gds = 0`), which would leave the
//! Jacobian singular. The two reverse gate junctions (gate-source, gate-drain) — evaluated as plain
//! diodes off the **already-batched** `ef/er` exponentials (no extra transcendentals) — supply a
//! small diagonal conductance that keeps `J` non-singular, plus physically-real gate leakage.
//!
//! ## Charge storage
//! The gate-junction capacitances Cgs/Cgd are encoded as the `cje`/`cjc` fields of an ordinary
//! [`BjtParams`] card (see [`jfet_small`]/[`jfet_power`]), so the existing depletion-charge batch and
//! its trapezoidal trailing update model them with **zero** change. Diffusion charge is disabled
//! (`tf = tr = 0`) since a reverse-biased gate stores none.

use super::*;

/// Per-device JFET channel parameters (Shichman-Hodges). Held alongside the gate-junction
/// [`BjtParams`] card in `WdfModel::jfet_params`; the card carries the gate diodes + Cgs/Cgd.
#[derive(Clone, Copy, Debug, Default)]
pub struct JfetExtra {
    /// Threshold (pinch-off) voltage `Vto`. N-channel JFET ⇒ negative (e.g. −1.0 V).
    pub vto: f64,
    /// Transconductance parameter `Beta` (A/V²): `Id = Beta·(Vgs−Vto)²·(1+λ·Vds)` in saturation.
    pub beta: f64,
    /// Channel-length modulation `Lambda` (1/V) — the output-conductance / soft-knee term.
    pub lambda: f64,
}

const SQRT2_TRUNC: f64 = 1.41421356;

/// Build a gate-junction [`BjtParams`] card for a JFET: gate leakage diode (`is`), the gate-source
/// and gate-drain depletion capacitances (`cje`←Cgs, `cjc`←Cgd), no diffusion charge (`tf=tr=0`),
/// and the BJT-only effects (Early/Kirk/β) left at the inert defaults — the JFET kernel never reads
/// them. `vcrit` matches the BJT cards' construction so the diode overflow guard is consistent.
fn gate_card(is: f64, cgs: f64, cgd: f64, pb: f64, m: f64) -> BjtParams {
    let mut p = BjtParams {
        is,
        vt: 0.02585,
        vcrit: 0.02585 * (0.02585 / (SQRT2_TRUNC * is)).ln(),
        cje: cgs,
        vje: pb,
        mje: m,
        cjc: cgd,
        vjc: pb,
        mjc: m,
        tf: 0.0,
        tr: 0.0,
        bf: 1.0,
        br: 1.0,
        ..BjtParams::default()
    };
    let (c0, q0, dcdv) = dep_sat_consts(p.cje, p.vje, p.mje);
    p.je_c0 = c0;
    p.je_q0 = q0;
    p.je_dcdv = dcdv;
    let (c0, q0, dcdv) = dep_sat_consts(p.cjc, p.vjc, p.mjc);
    p.jc_c0 = c0;
    p.jc_q0 = q0;
    p.jc_dcdv = dcdv;
    p
}

/// Small-signal JFET card (TR1/TR2 slots) — 2N5457-flavoured. Vto=−1.0 V, Beta=1.25 mA/V²,
/// Lambda=2.5 m/V; Cgs=4.5 pF, Cgd=1.5 pF. Creative values, plausible for an audio N-channel JFET.
pub fn jfet_small() -> (BjtParams, JfetExtra) {
    (
        gate_card(1.0e-14, 4.5e-12, 1.5e-12, 0.6, 0.5),
        JfetExtra { vto: -1.0, beta: 1.25e-3, lambda: 2.5e-3 },
    )
}

/// Power JFET card (TR3 slot) — scaled for the output device. Vto=−3.0 V, Beta=50 mA/V²,
/// Lambda=1 m/V; Cgs=60 pF, Cgd=15 pF. `Beta` is the knob to tune against the TR3 idle current.
pub fn jfet_power() -> (BjtParams, JfetExtra) {
    (
        gate_card(1.0e-13, 60.0e-12, 15.0e-12, 0.6, 0.5),
        JfetExtra { vto: -3.0, beta: 0.05, lambda: 1.0e-3 },
    )
}

/// Square-law channel current `Id(vgs, vgd)` and its partials, via `gm = ∂Id/∂Vgst` and
/// `gds = ∂Id/∂Vds`. Returns `(id, did_vgs, did_vgd)` where, by the chain rule
/// (`Vgst = Vgs − Vto`, `Vds = Vgs − Vgd`):
///   `did_vgs = gm + gds`, `did_vgd = −gds`  (in every region).
/// The triode branch also covers reverse `Vds < 0` (the JFET channel is ~symmetric), keeping the
/// model finite if drain swings below source under program drive.
#[inline(always)]
fn channel(vgs: f64, vgd: f64, jx: &JfetExtra) -> (f64, f64, f64) {
    let vgst = vgs - jx.vto;
    if vgst <= 0.0 {
        return (0.0, 0.0, 0.0); // cutoff
    }
    let vds = vgs - vgd;
    let b = jx.beta;
    let l = jx.lambda;
    let one_lvds = 1.0 + l * vds;
    if vds >= vgst {
        // saturation
        let id = b * vgst * vgst * one_lvds;
        let gm = 2.0 * b * vgst * one_lvds;
        let gds = b * vgst * vgst * l;
        (id, gm + gds, -gds)
    } else {
        // triode (and reverse vds < 0)
        let id = b * vds * (2.0 * vgst - vds) * one_lvds;
        let gm = 2.0 * b * vds * one_lvds;
        // gds = ∂Id/∂Vds at fixed Vgst
        let gds = b * (one_lvds * (2.0 * vgst - 2.0 * vds) + l * vds * (2.0 * vgst - vds));
        (id, gm + gds, -gds)
    }
}

/// Width-4 SoA JFET batch — the JFET analogue of [`super::bjt::compute_bjt_preexp_batch`]. Returns
/// the **same** [`BjtOutSoA`] (lane 3 = padding, `beta=0` ⇒ `Id=0`, never scattered). Reuses the
/// caller's already-batched gate-junction exponentials `ef = exp(vgs/vt)`, `er = exp(vgd/vt)`,
/// `ec = exp(vcrit/vt)` — no transcendental is recomputed here.
///
/// Terminal currents (into the device), gate→base / source→emitter / drain→collector:
///   `ib = igs + igd`, `ic = id − igd`, `ie = −(ib + ic)`  (KCL exact).
/// `itf/gtf/itr/gtr` carry the gate-diode current/conductance for the diffusion-charge tail; since
/// the JFET cards set `tf = tr = 0` that tail contributes nothing, leaving only Cgs/Cgd depletion.
#[inline]
pub(in crate::wdf) fn compute_jfet_preexp_batch(
    vgs: &[f64; 4],
    vgd: &[f64; 4],
    cards: &[Bjt; 3],
    jext: &[JfetExtra; 3],
    ef: &[f64; 4],
    er: &[f64; 4],
    ec: &[f64; 4],
) -> BjtOutSoA {
    let is4 = [cards[0].p.is, cards[1].p.is, cards[2].p.is, 1e-14];
    let vt4 = [cards[0].p.vt, cards[1].p.vt, cards[2].p.vt, 0.02585];
    let vcrit4 = [cards[0].p.vcrit, cards[1].p.vcrit, cards[2].p.vcrit, 0.0];
    let jx4 = [jext[0], jext[1], jext[2], JfetExtra::default()];

    let mut o = BjtOutSoA::default();
    for l in 0..4 {
        let inv_vt_is = is4[l] / vt4[l];
        let icrit = is4[l] * (ec[l] - 1.0);
        let gcrit = inv_vt_is * ec[l];
        // Gate-source diode (reverse-biased in normal operation) — same linearized-beyond-vcrit
        // form as `diode_branch`, off the pre-batched exponential.
        let (igs, ggs) = if vgs[l] < vcrit4[l] {
            (is4[l] * (ef[l] - 1.0), inv_vt_is * ef[l])
        } else {
            (gcrit * (vgs[l] - vcrit4[l]) + icrit, gcrit)
        };
        // Gate-drain diode.
        let (igd, ggd) = if vgd[l] < vcrit4[l] {
            (is4[l] * (er[l] - 1.0), inv_vt_is * er[l])
        } else {
            (gcrit * (vgd[l] - vcrit4[l]) + icrit, gcrit)
        };

        let (id, did_vgs, did_vgd) = channel(vgs[l], vgd[l], &jx4[l]);

        // Terminal currents into the device.
        let ib = igs + igd;
        let ic = id - igd;
        let ie = -(ib + ic);

        // Partials wrt the gather variables vgs (≡ vbe slot) and vgd (≡ vbc slot).
        let dib_vgs = ggs;
        let dib_vgd = ggd;
        let dic_vgs = did_vgs;
        let dic_vgd = did_vgd - ggd;
        let die_vgs = -(dib_vgs + dic_vgs);
        let die_vgd = -(dib_vgd + dic_vgd);

        o.ib[l] = ib;
        o.ic[l] = ic;
        o.ie[l] = ie;
        // Gate-diode "transport" fields — consumed by the diffusion-charge tail (inert: tf=tr=0).
        o.itf[l] = igs;
        o.gtf[l] = ggs;
        o.itr[l] = igd;
        o.gtr[l] = ggd;
        // Node form: d/dVg = d/dvgs + d/dvgd, d/dVs = −d/dvgs, d/dVd = −d/dvgd.
        o.dic_b[l] = dic_vgs + dic_vgd;
        o.dic_e[l] = -dic_vgs;
        o.dic_c[l] = -dic_vgd;
        o.dib_b[l] = dib_vgs + dib_vgd;
        o.dib_e[l] = -dib_vgs;
        o.dib_c[l] = -dib_vgd;
        o.die_b[l] = die_vgs + die_vgd;
        o.die_e[l] = -die_vgs;
        o.die_c[l] = -die_vgd;
    }
    // The JFET path is a creative voicing with no golden contract, so it uses plain `a*b+c`
    // arithmetic regardless of tier (no FMA generic, unlike the BJT batch kernel).
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mirror the newton.rs gather of ef/er/ec (plain exp; tests don't need the SLP path).
    fn exps(vg: &[f64; 4], vd: &[f64; 4], cards: &[Bjt; 3]) -> ([f64; 4], [f64; 4], [f64; 4]) {
        let vt = [cards[0].p.vt, cards[1].p.vt, cards[2].p.vt, 0.02585];
        let vcrit = [cards[0].p.vcrit, cards[1].p.vcrit, cards[2].p.vcrit, 0.0];
        let mut ef = [0.0; 4];
        let mut er = [0.0; 4];
        let mut ec = [0.0; 4];
        for l in 0..4 {
            ef[l] = (vg[l] / vt[l]).exp();
            er[l] = (vd[l] / vt[l]).exp();
            ec[l] = (vcrit[l] / vt[l]).exp();
        }
        (ef, er, ec)
    }

    // Terminal currents (ib, ic, ie) for one device at (vgs, vgd) via the batch path.
    fn terminal(
        dev: usize,
        vgs: f64,
        vgd: f64,
        cards: &[Bjt; 3],
        jext: &[JfetExtra; 3],
    ) -> (f64, f64, f64) {
        let mut vg = [0.0; 4];
        let mut vd = [0.0; 4];
        vg[dev] = vgs;
        vd[dev] = vgd;
        let (ef, er, ec) = exps(&vg, &vd, cards);
        let o = compute_jfet_preexp_batch(&vg, &vd, cards, jext, &ef, &er, &ec);
        (o.ib[dev], o.ic[dev], o.ie[dev])
    }

    /// Finite-difference contract: the analytic Jacobian returned by the batch kernel must match a
    /// central difference of the terminal currents wrt the gather variables (vgs, vgd) for every
    /// device, across cutoff / triode / saturation operating points.
    #[test]
    fn jacobian_matches_finite_difference() {
        let (cs, jxs) = jfet_small();
        let (cp, jxp) = jfet_power();
        let cards = [
            Bjt::new(0, 1, 2, cs),
            Bjt::new(3, 4, 5, cs),
            Bjt::new(6, 7, 8, cp),
        ];
        let jext = [jxs, jxs, jxp];

        let h = 1e-7;
        // Operating points spanning the three regions for an N-channel JFET (Vto<0).
        let points = [(-0.5, -0.5), (-0.9, -2.0), (0.1, -3.0), (-2.0, 0.05), (0.0, -5.0)];
        for dev in 0..3 {
            for &(vgs, vgd) in &points {
                let mut vg = [0.0; 4];
                let mut vd = [0.0; 4];
                vg[dev] = vgs;
                vd[dev] = vgd;
                let (ef, er, ec) = exps(&vg, &vd, &cards);
                let o = compute_jfet_preexp_batch(&vg, &vd, &cards, &jext, &ef, &er, &ec);

                // d/dvgs of (ib, ic) via central difference.
                let (ib_p, ic_p, _) = terminal(dev, vgs + h, vgd, &cards, &jext);
                let (ib_m, ic_m, _) = terminal(dev, vgs - h, vgd, &cards, &jext);
                let dib_vgs = (ib_p - ib_m) / (2.0 * h);
                let dic_vgs = (ic_p - ic_m) / (2.0 * h);
                // d/dvgd.
                let (ib_p, ic_p, _) = terminal(dev, vgs, vgd + h, &cards, &jext);
                let (ib_m, ic_m, _) = terminal(dev, vgs, vgd - h, &cards, &jext);
                let dib_vgd = (ib_p - ib_m) / (2.0 * h);
                let dic_vgd = (ic_p - ic_m) / (2.0 * h);

                // Recover node-form analytic partials back to (vgs, vgd):
                //   dib_b = dib_vgs + dib_vgd, dib_e = -dib_vgs  ⇒  dib_vgs = -dib_e, dib_vgd = dib_b + dib_e.
                let a_dib_vgs = -o.dib_e[dev];
                let a_dib_vgd = o.dib_b[dev] + o.dib_e[dev];
                let a_dic_vgs = -o.dic_e[dev];
                let a_dic_vgd = o.dic_b[dev] + o.dic_e[dev];

                let tol = 1e-3 * (1.0 + dib_vgs.abs() + dic_vgs.abs() + dib_vgd.abs() + dic_vgd.abs());
                assert!((a_dib_vgs - dib_vgs).abs() <= tol, "dib_vgs dev{dev} @({vgs},{vgd}): {a_dib_vgs} vs {dib_vgs}");
                assert!((a_dib_vgd - dib_vgd).abs() <= tol, "dib_vgd dev{dev} @({vgs},{vgd}): {a_dib_vgd} vs {dib_vgd}");
                assert!((a_dic_vgs - dic_vgs).abs() <= tol, "dic_vgs dev{dev} @({vgs},{vgd}): {a_dic_vgs} vs {dic_vgs}");
                assert!((a_dic_vgd - dic_vgd).abs() <= tol, "dic_vgd dev{dev} @({vgs},{vgd}): {a_dic_vgd} vs {dic_vgd}");
            }
        }
    }

    /// KCL: terminal currents must sum to zero at every operating point.
    #[test]
    fn terminal_currents_sum_to_zero() {
        let (cs, jxs) = jfet_small();
        let cards = [Bjt::new(0, 1, 2, cs), Bjt::new(3, 4, 5, cs), Bjt::new(6, 7, 8, cs)];
        let jext = [jxs; 3];
        let vg = [-0.5, -0.9, 0.1, 0.0];
        let vd = [-0.5, -2.0, -3.0, 0.0];
        let (ef, er, ec) = exps(&vg, &vd, &cards);
        let o = compute_jfet_preexp_batch(&vg, &vd, &cards, &jext, &ef, &er, &ec);
        for l in 0..3 {
            let s = o.ib[l] + o.ic[l] + o.ie[l];
            assert!(s.abs() < 1e-12, "KCL lane {l}: {s}");
        }
    }
}
