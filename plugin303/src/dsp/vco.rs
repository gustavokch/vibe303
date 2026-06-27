//! Nodal vco303 relaxation-oscillator solver (F3-VCO V3).
//!
//! Per-sample nodal MNA + Newton of the TB-303 VCO core (`circuit/vco303.toml`,
//! topology in `eval/303/_vco303_netlist_gen.add_elements`): 6 BJTs (2N3904 /
//! 2N3906), one MPF104 JFET reset switch, one 1N4148, an integrator cap C3 and
//! the comparator/output network. The integrator ramps onto C3 then the JFET
//! discharges it — a relaxation oscillator, stiff at the reset edge.
//!
//! 21 nodes; 4 are driven rails (`vp=12`, `vh=5.333`, `cvb=-4.7062+CV`, `cv=0`),
//! leaving 17 unknowns. Rails ramp 0→V over 1 ms to self-start (mirrors the SPICE
//! harness PWL). The saw output is node `o_r`, DC-blocked (the SPICE reference
//! centers it). Pitch is set by CV via the measured calibration; exact pitch /
//! waveform match to SPICE is finalized in V4/V5.
//!
//! Device models: [`super::bjt`], [`super::jfet`], [`super::diode`]. Linear solve:
//! [`super::linalg::lu_solve`]. Zero-alloc hot path.

use super::bjt::{Bjt, BjtParams};
use super::diode::Diode1N4148;
use super::jfet::Jfet;
use super::linalg::lu_solve;

// --- unknown node indices ---------------------------------------------------
const II: usize = 0;
const IB: usize = 1;
const IR: usize = 2;
const CP: usize = 3;
const N4: usize = 4;
const N12: usize = 5;
const N5: usize = 6;
const N10: usize = 7;
const BF: usize = 8;
const SH_IN: usize = 9;
const N17: usize = 10;
const PSI: usize = 11;
const PL: usize = 12;
const D_R: usize = 13;
const O_R: usize = 14;
const D_P: usize = 15;
const O_P: usize = 16;
const NU: usize = 17;

/// A circuit node: either an unknown (solved) or a fixed/driven voltage.
#[derive(Clone, Copy)]
enum Node {
    Unk(usize),
    Fix(f64),
}
use Node::{Fix, Unk};

const GND: Node = Fix(0.0);

const MAX_ITERS: usize = 100;
const TOL: f64 = 1e-7;
const GMIN_BASE: f64 = 1e-12;
const RAMP_S: f64 = 1e-3; // rail ramp time for self-start
/// Per-iteration node-voltage step limit (damped Newton): bounds the exponential
/// overshoot that otherwise diverges on this stiff relaxation core.
const VLIM: f64 = 0.5;

/// Add a current `Σ termsⱼ·Vⱼ + constant` *leaving* `node` to the KCL system.
#[inline]
fn stamp(
    a: &mut [[f64; NU]; NU],
    rhs: &mut [f64; NU],
    node: Node,
    terms: &[(Node, f64)],
    constant: f64,
) {
    let i = match node {
        Unk(i) => i,
        Fix(_) => return, // driven node: no KCL row
    };
    for &(nref, g) in terms {
        match nref {
            Unk(j) => a[i][j] += g,
            Fix(vf) => rhs[i] -= g * vf,
        }
    }
    rhs[i] -= constant;
}

/// Conductance `g` between nodes `p` and `q` (stamps both KCL rows).
#[inline]
fn stamp_res(a: &mut [[f64; NU]; NU], rhs: &mut [f64; NU], p: Node, q: Node, g: f64) {
    stamp(a, rhs, p, &[(p, g), (q, -g)], 0.0);
    stamp(a, rhs, q, &[(q, g), (p, -g)], 0.0);
}

/// Two-terminal companion with branch current `Geq·(Vp−Vq) + ieq` leaving `p`.
#[inline]
fn stamp_branch(a: &mut [[f64; NU]; NU], rhs: &mut [f64; NU], p: Node, q: Node, geq: f64, ieq: f64) {
    stamp(a, rhs, p, &[(p, geq), (q, -geq)], ieq);
    stamp(a, rhs, q, &[(q, geq), (p, -geq)], -ieq);
}

#[inline]
fn vof(node: Node, v: &[f64; NU]) -> f64 {
    match node {
        Unk(i) => v[i],
        Fix(vf) => vf,
    }
}

/// Cap topology entry: the two nodes and capacitance.
struct CapDef {
    p: Node,
    q: Node,
    c: f64,
}

pub struct Vco303 {
    sr: f64,
    npn: BjtParams,
    pnp: BjtParams,
    diode: Diode1N4148,
    jfet: Jfet,
    /// Control voltage that sets pitch (drives `cvb = -4.7062 + cv`).
    cv: f64,
    /// Warm-start: previous converged unknown voltages.
    v: [f64; NU],
    /// Trapezoidal cap state (v_prev, i_prev) for the 5 caps.
    cvp: [f64; 5],
    cip: [f64; 5],
    /// Samples since note-on (for the rail self-start ramp).
    t_samples: u64,
    /// DC blocker state for the `o_r` saw output.
    dc_x1: f32,
    dc_y1: f32,
    /// Diagnostics: Newton iters used + whether the last sample fell to homotopy.
    pub dbg_iters: u32,
    pub dbg_homotopy: bool,
}

impl Vco303 {
    pub fn new(sr: f32) -> Self {
        Self {
            sr: sr as f64,
            npn: BjtParams::n2n3904(),
            pnp: BjtParams::p2n3906(),
            diode: Diode1N4148::new(),
            jfet: Jfet::mpf104(),
            cv: 5.0,
            v: [0.0; NU],
            cvp: [0.0; 5],
            cip: [0.0; 5],
            t_samples: 0,
            dc_x1: 0.0,
            dc_y1: 0.0,
            dbg_iters: 0,
            dbg_homotopy: false,
        }
    }

    /// Raw converged node voltages (diagnostics).
    pub fn node(&self, i: usize) -> f64 {
        self.v[i]
    }

    /// Set pitch via the measured CV calibration (`voice_ref_spice._VCO_CAL`,
    /// quadratic `cv = poly(log2 f)`). A seed; V5 refines to match SPICE pitch.
    pub fn set_freq(&mut self, hz: f32) {
        self.cv = cv_for_freq(hz as f64);
    }

    /// Raw control voltage (diagnostics / calibration).
    pub fn set_cv(&mut self, cv: f64) {
        self.cv = cv;
    }

    pub fn reset(&mut self) {
        self.v = [0.0; NU];
        self.cvp = [0.0; 5];
        self.cip = [0.0; 5];
        self.t_samples = 0;
        self.dc_x1 = 0.0;
        self.dc_y1 = 0.0;
    }

    /// Driven rail voltages at the current sample (ramped 0→V over `RAMP_S`).
    fn rails(&self) -> (f64, f64, f64, f64) {
        let t = self.t_samples as f64 / self.sr;
        let k = (t / RAMP_S).min(1.0);
        let vp = 12.0 * k;
        let vh = 5.333 * k;
        let cvb = (-4.7062 + self.cv) * k;
        let cv = 0.0;
        (vp, vh, cvb, cv)
    }

    /// Cap definitions for this sample's rail values.
    fn caps(&self, vp: f64) -> [CapDef; 5] {
        [
            CapDef { p: Fix(vp), q: Unk(CP), c: 1e-8 },   // C3 integrator
            CapDef { p: Unk(BF), q: Unk(N17), c: 1e-8 },  // C1
            CapDef { p: Unk(PSI), q: GND, c: 1e-6 },      // C2
            CapDef { p: Unk(D_R), q: Unk(O_R), c: 1e-7 }, // C90
            CapDef { p: Unk(D_P), q: Unk(O_P), c: 1e-7 }, // C80
        ]
    }

    /// Assemble + stamp the full system at iterate `v`, fixed rails, cap
    /// companions `(cgeq, cieq)`, and `gmin` on the diagonal.
    fn assemble(
        &self,
        v: &[f64; NU],
        rails: (f64, f64, f64, f64),
        cgeq: &[f64; 5],
        cieq: &[f64; 5],
        gmin: f64,
    ) -> ([[f64; NU]; NU], [f64; NU]) {
        let (vp, vh, cvb, cv) = rails;
        let mut a = [[0.0f64; NU]; NU];
        let mut rhs = [0.0f64; NU];

        // Resistors.
        stamp_res(&mut a, &mut rhs, Fix(cvb), Unk(II), 1.0 / 220e3); // R77
        stamp_res(&mut a, &mut rhs, Fix(cv), Unk(II), 1.0 / 220e3); // R7
        stamp_res(&mut a, &mut rhs, Unk(II), GND, 1.0 / 4040.0); // R8
        stamp_res(&mut a, &mut rhs, Fix(vp), Unk(IB), 1.0 / 100e3); // R9
        stamp_res(&mut a, &mut rhs, Unk(CP), Unk(IR), 1.0 / 4700.0); // R88
        stamp_res(&mut a, &mut rhs, Unk(N10), Fix(vh), 1.0 / 10e3); // R6
        stamp_res(&mut a, &mut rhs, Unk(BF), GND, 1.0 / 10e3); // R1
        stamp_res(&mut a, &mut rhs, Unk(BF), Unk(SH_IN), 1.0 / 100e3); // R2
        stamp_res(&mut a, &mut rhs, Unk(N17), Unk(SH_IN), 1.0 / 10e3); // R3
        stamp_res(&mut a, &mut rhs, Fix(vp), Unk(PSI), 1.0 / 22e3); // R5
        stamp_res(&mut a, &mut rhs, Fix(vh), Unk(PL), 1.0 / 10e3); // R4
        stamp_res(&mut a, &mut rhs, Unk(BF), Unk(D_R), 1.0 / 1e6); // R90
        stamp_res(&mut a, &mut rhs, Unk(O_R), GND, 1.0 / 220e3); // R91
        stamp_res(&mut a, &mut rhs, Unk(PL), Unk(D_P), 1.0 / 1e6); // R80
        stamp_res(&mut a, &mut rhs, Unk(O_P), GND, 1.0 / 220e3); // R81

        // Cap companions.
        let cdefs = self.caps(vp);
        for (j, cd) in cdefs.iter().enumerate() {
            // Trapezoidal: branch current leaving p = Geq·(Vp−Vq) − Ieq.
            stamp_branch(&mut a, &mut rhs, cd.p, cd.q, cgeq[j], -cieq[j]);
        }

        // Diode D1: anode n4 -> cathode n5.
        {
            let vd = vof(Unk(N4), v) - vof(Unk(N5), v);
            let (id, gd) = self.diode.stamp(vd);
            let ieq = id - gd * vd;
            stamp_branch(&mut a, &mut rhs, Unk(N4), Unk(N5), gd, ieq);
        }

        // BJTs: (device, base, collector, emitter).
        let bjts: [(Bjt, Node, Node, Node); 6] = [
            (Bjt::pnp(self.pnp), Unk(II), GND, Unk(IB)),     // Q6
            (Bjt::npn(self.npn), Unk(IB), Unk(IR), GND),     // Q7
            (Bjt::pnp(self.pnp), Unk(N12), Unk(N4), Fix(vp)),// Q4
            (Bjt::npn(self.npn), Unk(N5), Unk(N5), Unk(N10)),// Q5 (diode-connected)
            (Bjt::npn(self.npn), Unk(N5), Unk(N12), Unk(CP)),// Q3
            (Bjt::pnp(self.pnp), Unk(SH_IN), Unk(PL), Unk(PSI)),// Q2
        ];
        for (dev, nb, nc, ne) in bjts {
            let vb = vof(nb, v);
            let vc = vof(nc, v);
            let ve = vof(ne, v);
            let o = dev.solve(vb - ve, vb - vc);
            // Each terminal current (into device) leaving its node, linearized in
            // the node voltages; constant = I0 - Σ g·V0.
            let i_eq_b = o.ib - (o.dib_b * vb + o.dib_c * vc + o.dib_e * ve);
            let i_eq_c = o.ic - (o.dic_b * vb + o.dic_c * vc + o.dic_e * ve);
            let i_eq_e = o.ie - (o.die_b * vb + o.die_c * vc + o.die_e * ve);
            stamp(&mut a, &mut rhs, nb, &[(nb, o.dib_b), (nc, o.dib_c), (ne, o.dib_e)], i_eq_b);
            stamp(&mut a, &mut rhs, nc, &[(nb, o.dic_b), (nc, o.dic_c), (ne, o.dic_e)], i_eq_c);
            stamp(&mut a, &mut rhs, ne, &[(nb, o.die_b), (nc, o.die_c), (ne, o.die_e)], i_eq_e);
        }

        // JFET Q1: drain=vp, gate=cp, source=bf.
        {
            let nd = Fix(vp);
            let ng = Unk(CP);
            let ns = Unk(BF);
            let vd = vof(nd, v);
            let vg = vof(ng, v);
            let vs = vof(ns, v);
            let vgs = vg - vs;
            let vds = vd - vs;
            let ch = self.jfet.channel(vgs, vds);
            // Drain current leaving drain node: gm·Vg + gds·Vd − (gm+gds)·Vs + const.
            let gm = ch.gm;
            let gds = ch.gds;
            let i_eq = ch.id - (gm * vgs + gds * vds);
            stamp(&mut a, &mut rhs, nd, &[(ng, gm), (nd, gds), (ns, -(gm + gds))], i_eq);
            stamp(&mut a, &mut rhs, ns, &[(ng, -gm), (nd, -gds), (ns, gm + gds)], -i_eq);
            // Gate junction diodes: gate->source, gate->drain.
            let vgsj = vg - vs;
            let (igs, ggs) = self.jfet.gate_diode(vgsj);
            stamp_branch(&mut a, &mut rhs, ng, ns, ggs, igs - ggs * vgsj);
            let vgdj = vg - vd;
            let (igd, ggd) = self.jfet.gate_diode(vgdj);
            stamp_branch(&mut a, &mut rhs, ng, nd, ggd, igd - ggd * vgdj);
        }

        // gmin + base leakage on every diagonal.
        for d in 0..NU {
            a[d][d] += gmin + GMIN_BASE;
        }
        (a, rhs)
    }

    /// One Newton solve at fixed `gmin`, warm-started from `v0`.
    fn newton(
        &self,
        gmin: f64,
        v0: [f64; NU],
        rails: (f64, f64, f64, f64),
        cgeq: &[f64; 5],
        cieq: &[f64; 5],
    ) -> Option<([f64; NU], u32)> {
        let mut v = v0;
        for it in 0..MAX_ITERS {
            let (a, rhs) = self.assemble(&v, rails, cgeq, cieq, gmin);
            let sol = lu_solve::<NU>(a, rhs)?;
            let mut dmax = 0.0f64;
            for d in 0..NU {
                if !sol[d].is_finite() {
                    return None;
                }
                // Damped step: bound |Δv| per node to VLIM.
                let step = (sol[d] - v[d]).clamp(-VLIM, VLIM);
                v[d] += step;
                let dd = step.abs();
                if dd > dmax {
                    dmax = dd;
                }
            }
            if dmax < TOL {
                return Some((v, it as u32 + 1));
            }
        }
        None
    }

    /// Solve one sample; returns the DC-blocked `o_r` saw.
    pub fn tick(&mut self) -> f32 {
        let dt = 1.0 / self.sr;
        let rails = self.rails();
        let vp = rails.0;

        let cdefs = self.caps(vp);
        let mut cgeq = [0.0f64; 5];
        let mut cieq = [0.0f64; 5];
        for (j, cd) in cdefs.iter().enumerate() {
            let geq = 2.0 * cd.c / dt;
            cgeq[j] = geq;
            cieq[j] = geq * self.cvp[j] + self.cip[j];
        }

        // Fast path: warm-started Newton at gmin=0; else descend a homotopy.
        let v = match self.newton(0.0, self.v, rails, &cgeq, &cieq) {
            Some((v, it)) => {
                self.dbg_iters = it;
                self.dbg_homotopy = false;
                v
            }
            None => {
                self.dbg_homotopy = true;
                let mut vh = self.v;
                let mut ok = self.v;
                let mut its = 0;
                for &gmin in &[1e-1, 1e-2, 1e-3, 1e-4, 0.0] {
                    if let Some((s, it)) = self.newton(gmin, vh, rails, &cgeq, &cieq) {
                        vh = s;
                        ok = s;
                        its += it;
                    } else {
                        break;
                    }
                }
                self.dbg_iters = its;
                ok
            }
        };
        self.v = v;

        // Update cap trapezoidal state.
        for (j, cd) in cdefs.iter().enumerate() {
            let vc = vof(cd.p, &v) - vof(cd.q, &v);
            let inew = cgeq[j] * vc - cieq[j];
            self.cvp[j] = if vc.abs() < 1e-30 { 0.0 } else { vc };
            self.cip[j] = if inew.abs() < 1e-30 { 0.0 } else { inew };
        }
        self.t_samples += 1;

        // o_r saw, DC-blocked (R=0.999 one-pole highpass).
        let x = v[O_R] as f32;
        let y = x - self.dc_x1 + 0.999 * self.dc_y1;
        self.dc_x1 = x;
        self.dc_y1 = y;
        y
    }
}

/// CV for a target pitch: quadratic fit of CV in `log2(freq)` calibrated to **this
/// solver's** measured `freq(cv)` (not SPICE's — our reset timing differs ~1.4×).
/// Good to ~1.4 % pitch; a per-note secant refinement for tighter match is V5.
fn cv_for_freq(freq: f64) -> f64 {
    const A: f64 = CV_FIT[0];
    const B: f64 = CV_FIT[1];
    const C: f64 = CV_FIT[2];
    let l = freq.max(1.0).log2();
    A * l * l + B * l + C
}

/// Quadratic coefficients [a, b, c] for cv = a·x² + b·x + c, x = log2(freq).
/// Refit offline from `characterize_freq_cv` (this solver's measured points).
const CV_FIT: [f64; 3] = [0.011_078_504_3, 0.844_571_322_2, -3.808_754_753_1];

#[cfg(test)]
mod tests {
    use super::*;

    /// Rising zero-crossings → fundamental of a centered saw.
    fn measure_freq(buf: &[f32], sr: f32) -> f32 {
        let mean = buf.iter().sum::<f32>() / buf.len() as f32;
        let mut zc = 0;
        for w in buf.windows(2) {
            if w[0] - mean < 0.0 && w[1] - mean >= 0.0 {
                zc += 1;
            }
        }
        zc as f32 * sr / buf.len() as f32
    }

    fn render(cv_hz: f32, sr: f32, secs: f32) -> Vec<f32> {
        let mut osc = Vco303::new(sr);
        osc.set_freq(cv_hz);
        let n = (sr * secs) as usize;
        let settle = (sr * 0.03) as usize;
        let mut out = Vec::with_capacity(n);
        for i in 0..(n + settle) {
            let y = osc.tick();
            if i >= settle {
                out.push(y);
            }
        }
        out
    }

    #[test]
    #[ignore = "diagnostic dump"]
    fn dump_trajectory() {
        let sr = 48_000.0;
        let mut o = Vco303::new(sr);
        o.set_freq(231.0);
        println!("cv={:.3}", super::cv_for_freq(231.0));
        for i in 0..2400 {
            let y = o.tick();
            if i % 30 == 0 {
                println!(
                    "{i:5} cp={:+.3} bf={:+.3} n5={:+.3} n4={:+.3} o_r={:+.3} o_p={:+.3} | or_dc={y:+.4} it={} homo={}",
                    o.node(CP), o.node(BF), o.node(N5), o.node(N4),
                    o.node(O_R), o.node(O_P), o.dbg_iters, o.dbg_homotopy
                );
            }
        }
    }

    fn render_cv(cv: f64, sr: f32, secs: f32) -> Vec<f32> {
        let mut osc = Vco303::new(sr);
        osc.set_cv(cv);
        let n = (sr * secs) as usize;
        let settle = (sr * 0.04) as usize;
        let mut out = Vec::with_capacity(n);
        for i in 0..(n + settle) {
            let y = osc.tick();
            if i >= settle {
                out.push(y);
            }
        }
        out
    }

    #[test]
    #[ignore = "diagnostic: characterize freq(cv) for MY solver to refit CV_FIT"]
    fn characterize_freq_cv() {
        let sr = 48_000.0;
        // Long render for fine zero-cross resolution.
        for cv in [2.5f64, 3.0, 3.5, 4.0, 4.5, 5.0, 5.5, 6.0, 6.5] {
            let f = measure_freq(&render_cv(cv, sr, 0.4), sr);
            println!("cv={cv:.2} freq={f:8.2} log2f={:.4}", (f as f64).log2());
        }
    }

    #[test]
    fn self_oscillates() {
        let sr = 96_000.0;
        let y = render(231.0, sr, 0.1);
        let max = y.iter().cloned().fold(0.0f32, |a, b| a.max(b.abs()));
        let min = y.iter().cloned().fold(0.0f32, |a, b| a.min(b));
        assert!(max > 0.01, "no oscillation: peak {max}");
        assert!((max - min.abs()).is_finite());
        for &s in &y {
            assert!(s.is_finite(), "non-finite sample");
        }
    }

    #[test]
    fn pitch_in_audio_range_and_tracks_cv() {
        let sr = 96_000.0;
        let f_low = measure_freq(&render(231.0, sr, 0.1), sr);
        let f_hi = measure_freq(&render(462.0, sr, 0.1), sr);
        assert!(f_low > 50.0 && f_low < 4_000.0, "low pitch out of range: {f_low}");
        // An octave CV step should raise pitch (~tracking; not exact yet).
        assert!(f_hi > f_low * 1.4, "pitch does not track CV: {f_low} -> {f_hi}");
    }
}
