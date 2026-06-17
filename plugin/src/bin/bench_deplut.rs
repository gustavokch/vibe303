//! Phase 0 GO/NO-GO micro-bench for the depletion-charge LUT spike.
//!
//! Times the production 4-lane SoA depletion-charge path (`depletion_charge_pre_batch`,
//! the `pow` arm) against a candidate LUT + linear-interp of the same SoA shape, on a
//! representative reverse->forward bias sweep. The candidate uses scalar-per-lane indexed
//! loads (4 independent table fetches per batch) — this IS the "gather" cost the plan flags
//! as the key risk, measured honestly without a vector intrinsic.
//!
//! GATE: candidate must beat baseline net at 4-lane SoA *with the gather included*. If it
//! does not, the spike is NO-GO — record to memory and do not integrate.
//!
//! Build/run with the profiling ISA floor (AVX2+FMA), release, feature-gated:
//!   RUSTFLAGS='-C target-cpu=x86-64-v3' \
//!     cargo run --bin bench_deplut --release -p ba283 --features bench-internals
//!
//! Output: per-N table of baseline ns/elem, candidate ns/elem, ratio, and the
//! candidate-vs-baseline error (max abs rel + RMS in dB vs the −40 dB budget).

use std::hint::black_box;
use std::time::Instant;

use ba283::wdf::{bc184c, dep_sat_consts, depletion_charge, depletion_charge_pre_batch, q2n3055};

const FC: f64 = 0.5;
const V_MIN: f64 = -40.0;

/// One junction's fixed card params + precomputed saturated-branch constants.
#[derive(Clone, Copy)]
struct Junction {
    cj0: f64,
    vj: f64,
    m: f64,
    c0: f64,
    q0: f64,
    dcdv: f64,
}

impl Junction {
    fn new(cj0: f64, vj: f64, m: f64) -> Self {
        let (c0, q0, dcdv) = dep_sat_consts(cj0, vj, m);
        Self { cj0, vj, m, c0, q0, dcdv }
    }
}

/// Precomputed 1-D table over `[V_MIN, FC*vj)` for one junction. `q`/`c` filled from the
/// reference `depletion_charge`; lookup is `v -> (v - v0) * inv_dv` + linear interp.
struct DepLut {
    q: Vec<f64>,
    c: Vec<f64>,
    v0: f64,
    inv_dv: f64,
    n: usize,
    // carry the saturated-branch consts so the candidate can take the linear arm above FC*vj.
    j: Junction,
}

impl DepLut {
    fn build(j: Junction, n: usize) -> Self {
        let v_hi = FC * j.vj;
        let dv = (v_hi - V_MIN) / (n as f64 - 1.0);
        let mut q = vec![0.0; n];
        let mut c = vec![0.0; n];
        for i in 0..n {
            let v = V_MIN + dv * i as f64;
            let (qi, ci) = depletion_charge(v, j.cj0, j.vj, j.m);
            q[i] = qi;
            c[i] = ci;
        }
        Self { q, c, v0: V_MIN, inv_dv: 1.0 / dv, n, j }
    }

    /// Scalar lane lookup with the same select shape as `dep_charge_lane_bl`: LUT interp on the
    /// pow arm (`v < FC*vj`), exact linear arm above. Includes the dependent indexed loads.
    #[inline(always)]
    fn sample(&self, v: f64) -> (f64, f64) {
        if self.j.cj0 <= 0.0 {
            return (0.0, 0.0);
        }
        if v < FC * self.j.vj {
            let x = ((v - self.v0) * self.inv_dv).max(0.0);
            let i = (x as usize).min(self.n - 2);
            let f = x - i as f64;
            let q = self.q[i] + f * (self.q[i + 1] - self.q[i]);
            let c = self.c[i] + f * (self.c[i + 1] - self.c[i]);
            (q, c)
        } else {
            let d = v - FC * self.j.vj;
            let c = self.j.c0 + self.j.dcdv * d;
            let q = self.j.q0 + self.j.c0 * d + 0.5 * self.j.dcdv * d * d;
            (q, c)
        }
    }
}

/// 4-lane SoA candidate: one indexed lookup per lane (the honest gather cost).
#[inline(always)]
fn lut_batch(v: [f64; 4], luts: &[&DepLut; 4]) -> ([f64; 4], [f64; 4]) {
    let r0 = luts[0].sample(v[0]);
    let r1 = luts[1].sample(v[1]);
    let r2 = luts[2].sample(v[2]);
    let r3 = luts[3].sample(v[3]);
    ([r0.0, r1.0, r2.0, r3.0], [r0.1, r1.1, r2.1, r3.1])
}

/// Deterministic pseudo-random v sequence across the hot region (reverse -> mild forward),
/// shuffled by an LCG so branch prediction sees a realistic mix (not a monotone ramp).
fn make_v_seq(vj_max: f64, count: usize) -> Vec<f64> {
    let lo = V_MIN;
    let hi = FC * vj_max + 0.05; // a little past FC*vj to exercise the linear arm too
    let span = hi - lo;
    let mut out = Vec::with_capacity(count);
    let mut state: u64 = 0x9E3779B97F4A7C15;
    for _ in 0..count {
        // xorshift64* — deterministic, no wall-clock/random.
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        let r = (state.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64 / (1u64 << 53) as f64;
        out.push(lo + span * r);
    }
    out
}

fn time_min<F: FnMut() -> f64>(runs: usize, mut f: F) -> f64 {
    let mut best = f64::INFINITY;
    for _ in 0..runs {
        let t = Instant::now();
        let sink = f();
        let dt = t.elapsed().as_secs_f64();
        black_box(sink);
        if dt < best {
            best = dt;
        }
    }
    best
}

fn db(x: f64) -> f64 {
    20.0 * x.max(1e-300).log10()
}

fn main() {
    // 4 distinct junctions (BC184C BE/BC, 2N3055 BE/BC).
    let bc = bc184c();
    let q2 = q2n3055();
    let jbe1 = Junction::new(bc.cje, bc.vje, bc.mje); // BC184C base-emitter
    let jbe2 = Junction::new(q2.cje, q2.vje, q2.mje); // 2N3055 base-emitter
    // BC junctions exist on the cards too; the BE batch is representative for timing.

    // Mimic the production BE batch lanes: [TR1=bc, TR2=bc, TR3=q2, pad=q2].
    let lanes = [jbe1, jbe1, jbe2, jbe2];
    let cj0 = [lanes[0].cj0, lanes[1].cj0, lanes[2].cj0, lanes[3].cj0];
    let vj = [lanes[0].vj, lanes[1].vj, lanes[2].vj, lanes[3].vj];
    let mm = [lanes[0].m, lanes[1].m, lanes[2].m, lanes[3].m];
    let c0 = [lanes[0].c0, lanes[1].c0, lanes[2].c0, lanes[3].c0];
    let q0 = [lanes[0].q0, lanes[1].q0, lanes[2].q0, lanes[3].q0];
    let dcdv = [lanes[0].dcdv, lanes[1].dcdv, lanes[2].dcdv, lanes[3].dcdv];

    let vj_max = vj.iter().cloned().fold(0.0_f64, f64::max);

    // A v sequence per lane (4 columns), shuffled differently per lane.
    const SEQ: usize = 4096;
    let seq: Vec<[f64; 4]> = {
        let cols: Vec<Vec<f64>> = (0..4).map(|k| make_v_seq(vj_max, SEQ + k * 7)).collect();
        (0..SEQ)
            .map(|i| [cols[0][i], cols[1][i], cols[2][i], cols[3][i]])
            .collect()
    };

    const ITERS: usize = 2000; // repeat the SEQ sweep to get a stable reading
    const RUNS: usize = 12;
    let n_elem = (SEQ * ITERS * 4) as f64;

    // ---- baseline: production batched-pow path ----
    let base_t = time_min(RUNS, || {
        let mut acc = 0.0;
        for _ in 0..ITERS {
            for v in &seq {
                let (q, c) = depletion_charge_pre_batch::<true>(*v, cj0, vj, mm, c0, q0, dcdv);
                acc += q[0] + q[1] + q[2] + q[3] + c[0] + c[1] + c[2] + c[3];
            }
        }
        acc
    });
    let base_ns = base_t / n_elem * 1e9;

    println!(
        "baseline (batched pow, FMA): {:.3} ns/elem  ({:.1} M elem/s)",
        base_ns,
        n_elem / base_t / 1e6
    );
    println!();
    println!("{:>6}  {:>10}  {:>10}  {:>7}  {:>11}  {:>11}", "N", "cand ns", "ratio", "speedup", "maxrel q/c", "rms dB q/c");

    for &n in &[128usize, 256, 512, 1024] {
        let luts: [DepLut; 4] = [
            DepLut::build(lanes[0], n),
            DepLut::build(lanes[1], n),
            DepLut::build(lanes[2], n),
            DepLut::build(lanes[3], n),
        ];
        let lref: [&DepLut; 4] = [&luts[0], &luts[1], &luts[2], &luts[3]];

        let cand_t = time_min(RUNS, || {
            let mut acc = 0.0;
            for _ in 0..ITERS {
                for v in &seq {
                    let (q, c) = lut_batch(*v, &lref);
                    acc += q[0] + q[1] + q[2] + q[3] + c[0] + c[1] + c[2] + c[3];
                }
            }
            acc
        });
        let cand_ns = cand_t / n_elem * 1e9;

        // error vs baseline (pow) across the sweep
        let mut max_rel_q = 0.0_f64;
        let mut max_rel_c = 0.0_f64;
        let (mut sq_e_q, mut sq_s_q) = (0.0_f64, 0.0_f64);
        let (mut sq_e_c, mut sq_s_c) = (0.0_f64, 0.0_f64);
        for v in &seq {
            let (qb, cb) = depletion_charge_pre_batch::<true>(*v, cj0, vj, mm, c0, q0, dcdv);
            let (qc, cc) = lut_batch(*v, &lref);
            for k in 0..4 {
                let eq = (qc[k] - qb[k]).abs();
                let ec = (cc[k] - cb[k]).abs();
                if qb[k].abs() > 0.0 {
                    max_rel_q = max_rel_q.max(eq / qb[k].abs());
                }
                if cb[k].abs() > 0.0 {
                    max_rel_c = max_rel_c.max(ec / cb[k].abs());
                }
                sq_e_q += eq * eq;
                sq_s_q += qb[k] * qb[k];
                sq_e_c += ec * ec;
                sq_s_c += cb[k] * cb[k];
            }
        }
        let rms_db_q = db((sq_e_q / sq_s_q.max(1e-300)).sqrt());
        let rms_db_c = db((sq_e_c / sq_s_c.max(1e-300)).sqrt());

        println!(
            "{:>6}  {:>10.3}  {:>10.3}  {:>6.2}x  {:>5.1e}/{:<5.1e}  {:>5.1}/{:<5.1}",
            n,
            cand_ns,
            cand_ns / base_ns,
            base_ns / cand_ns,
            max_rel_q,
            max_rel_c,
            rms_db_q,
            rms_db_c,
        );
    }

    println!();
    println!("GATE: candidate beats baseline (speedup > 1.0x) AND rms dB <= -40 for usable N.");
}
