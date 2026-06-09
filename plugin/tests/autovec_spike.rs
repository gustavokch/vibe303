//! Task 1 SPIKE: feasibility of a branchless batched `exp` kernel that
//! autovectorizes, stays BIT-EXACT vs the scalar `fast_exp`, and saves
//! instructions. This is a GO/NO-GO gate — NOT production code, NOT on the
//! audio hot path.

use ba283::wdf::{bc184c, fast_exp, q2n3055, BjtParams};

/// Branchless single-lane equivalent of `fast_exp`. The underflow / overflow
/// guards become SELECTS over the already-computed result (no control flow that
/// skips the polynomial), so the batch loop can vectorize. The exponent is
/// clamped BEFORE the IEEE-754 shift so a discarded lane never wraps.
#[inline]
fn exp_bl(x: f64) -> f64 {
    const LN2: f64 = 0.6931471805599453;
    const INV_LN2: f64 = 1.4426950408889634;
    let kf = (x * INV_LN2 + 0.5).floor();
    let k = kf as i32;
    let r = x - kf * LN2;
    let p = 1.0
        + r * (1.0
            + r * (1.0 / 2.0
                + r * (1.0 / 6.0
                    + r * (1.0 / 24.0
                        + r * (1.0 / 120.0
                            + r * (1.0 / 720.0
                                + r * (1.0 / 5040.0
                                    + r * (1.0 / 40320.0 + r * (1.0 / 362880.0)))))))));
    let kc = k.clamp(-1022, 1023);
    let u: u64 = ((kc + 1023) as u64) << 52;
    let pk = f64::from_bits(u);
    let mut res = p * pk;
    if k <= -1023 {
        res = 0.0;
    }
    if k >= 1024 {
        res = p * f64::INFINITY;
    }
    res
}

/// Batched wrapper. `#[inline(never)]` + `no_mangle` so it stays a distinct
/// symbol we can disassemble in objdump.
#[no_mangle]
#[inline(never)]
pub extern "C" fn exp_bl_batch(xs: &[f64], out: &mut [f64]) {
    for i in 0..xs.len() {
        out[i] = exp_bl(xs[i]);
    }
}

/// Scalar baseline with the same loop shape, calling the production `fast_exp`.
#[no_mangle]
#[inline(never)]
pub extern "C" fn fast_exp_batch(xs: &[f64], out: &mut [f64]) {
    for i in 0..xs.len() {
        out[i] = fast_exp(xs[i]);
    }
}

/// Step 2 — bit-exactness across the full junction domain.
#[test]
fn exp_bl_is_bit_exact() {
    let mut mismatches = 0usize;
    let mut first: Option<(f64, u64, u64)> = None;

    let mut check = |x: f64| {
        let a = exp_bl(x).to_bits();
        let b = fast_exp(x).to_bits();
        if a != b {
            mismatches += 1;
            if first.is_none() {
                first = Some((x, a, b));
            }
        }
    };

    // Deep underflow (reverse-biased junctions: x/Vt ~ -1000), coarse step.
    let mut x = -1400.0;
    while x <= -50.0 {
        check(x);
        x += 0.05;
    }

    // The k <= -1023 boundary region, fine step. k = floor(x*INV_LN2 + 0.5);
    // k == -1023 occurs around x ~ -709. Sweep generously around it.
    let mut x = -720.0;
    while x <= -700.0 {
        check(x);
        x += 0.0005;
    }

    // Active / near-zero / forward region up to ~40, fine step.
    let mut x = -50.0;
    while x <= 40.0 {
        check(x);
        x += 0.001;
    }

    // A few explicit edge points.
    for &x in &[
        -1400.0, -1023.0, -709.7827, -709.0, -1.0, -0.5, 0.0, 0.5, 1.0, 39.9, 40.0,
    ] {
        check(x);
    }

    if let Some((x, a, b)) = first {
        panic!(
            "BIT-EXACT FAIL: {} mismatches; first at x={:.9} exp_bl=0x{:016x} fast_exp=0x{:016x}",
            mismatches, x, a, b
        );
    }
    assert_eq!(mismatches, 0);
}

// ---- Phase 0.5 — SLP spike: [f64;4] SoA straight-line probe ----
//
// LLVM's SLP vectorizer fuses 4 independent scalar lanes into a single AVX2
// packed-double op when it sees straight-line code on a [f64;4] input.
// `#[no_mangle] #[inline(never)]` keeps it as a distinct symbol for objdump.
//
// GO  → objdump shows vmulpd/vaddpd/etc over 4 lanes → proceed to Phase 1.
// NO-GO → scalar sequence → stop, report, decide with user before any rewrite.
#[no_mangle]
#[inline(never)]
pub extern "C" fn exp_bl_slp4(xs: [f64; 4]) -> [f64; 4] {
    [
        exp_bl(xs[0]),
        exp_bl(xs[1]),
        exp_bl(xs[2]),
        exp_bl(xs[3]),
    ]
}

#[test]
fn exp_bl_slp4_is_bit_exact() {
    for i in 0..100 {
        let x = -20.0 + (i as f64) * 0.6;
        let xs = [x, x + 0.1, x + 0.2, x + 0.3];
        let got = exp_bl_slp4(xs);
        let want = [exp_bl(xs[0]), exp_bl(xs[1]), exp_bl(xs[2]), exp_bl(xs[3])];
        assert_eq!(got[0].to_bits(), want[0].to_bits(), "lane 0 x={x}");
        assert_eq!(got[1].to_bits(), want[1].to_bits(), "lane 1 x={x}");
        assert_eq!(got[2].to_bits(), want[2].to_bits(), "lane 2 x={x}");
        assert_eq!(got[3].to_bits(), want[3].to_bits(), "lane 3 x={x}");
    }
}

// ---- Step 4 — instruction A/B benches (run individually under perf) ----

const BENCH_N: usize = 4096;
const BENCH_ITERS: usize = 200_000;

fn bench_input() -> Vec<f64> {
    // Spread across the active forward junction domain to exercise the poly.
    (0..BENCH_N)
        .map(|i| -20.0 + (i as f64) * (60.0 / BENCH_N as f64))
        .collect()
}

#[test]
#[ignore]
fn bench_batch() {
    let xs = bench_input();
    let mut out = vec![0.0f64; BENCH_N];
    let mut acc = 0.0f64;
    for _ in 0..BENCH_ITERS {
        exp_bl_batch(std::hint::black_box(&xs), std::hint::black_box(&mut out));
        acc += out[BENCH_N - 1];
    }
    println!("bench_batch acc={}", std::hint::black_box(acc));
}

#[test]
#[ignore]
fn bench_scalar() {
    let xs = bench_input();
    let mut out = vec![0.0f64; BENCH_N];
    let mut acc = 0.0f64;
    for _ in 0..BENCH_ITERS {
        fast_exp_batch(std::hint::black_box(&xs), std::hint::black_box(&mut out));
        acc += out[BENCH_N - 1];
    }
    println!("bench_scalar acc={}", std::hint::black_box(acc));
}

// =====================================================================
// Phase 2.0 — f32 device-kernel GO/NO-GO spike (Precision dial Phase 2)
//
// The hot kernel is `compute_bjt_preexp_batch` (in plugin/src/wdf.rs): the
// divide-/sqrt-heavy post-exp Gummel-Poon Jacobian, run width-4 SoA (3 BJTs +
// 1 pad lane).
// Question: does an f32 mirror run materially faster (cheaper vdivps/vsqrtps),
// and does it stay accurate enough (rel-err <= 1e-4) to hold the -50 dB budget
// when wired into Precision::Low?  NOTE: only 3 BJTs → no extra lane-parallelism
// from f32; the win (if any) is per-op div/sqrt latency, not 8-wide packing.
//
// GO  → both: micro-bench shows >= ~15% kernel speedup AND rel-err <= 1e-4.
// NO-GO → drop f32 from Low, record dead lever, consider sub-rate instead.
// =====================================================================

/// Six representative outputs of the post-exp algebra: the three terminal
/// currents plus three **vbe-partial** Jacobian entries (`dic_vbe`/`dib_vbe`/
/// `die_vbe` — NOT the production base derivatives `dic_b = dic_vbe + dic_vbc`;
/// the spike omits the vbc branch, see `preexp_lane_f64` doc). Keeps the
/// divide-heavy derivative chain live so it isn't DCE'd, and gives the accuracy
/// probe coverage on the `/qbsq` terms — the f32-riskiest ops.
#[derive(Clone, Copy, Debug, Default)]
pub struct PreOut6 {
    ib: f64,
    ic: f64,
    ie: f64,
    dic_vbe: f64,
    dib_vbe: f64,
    die_vbe: f64,
}

/// f64 reference: faithful single-lane port of `compute_bjt_preexp_batch`'s
/// per-lane body. `ef/er/ec` are the pre-exp transport terms (computed upstream
/// in f64 and reused — same as production), so this isolates the post-exp
/// divide/sqrt algebra.
///
/// NOTE: only the **vbe-partial** derivatives are kept (`dic_vbe`/`dib_vbe`/
/// `die_vbe`); the production vbc-derivative branch (`dq1_vbc`, `dq2_vbc`,
/// `dqb_vbc`, `dit_vbc`, …) is omitted, so this probes ~half the full Jacobian.
/// That subset suffices for the GO/NO-GO: the omitted half is more divide-heavy
/// algebra where f32 is *cheaper*, so including it could only widen f32's edge —
/// even doubling the measured +6.1% stays under the 15% bar, so the NO-GO holds.
#[inline(always)]
fn preexp_lane_f64(vbe: f64, vbc: f64, ef: f64, er: f64, ec: f64, p: &BjtParams) -> PreOut6 {
    let is = p.is;
    let vcrit = p.vcrit;
    let inv_vt_is = is / p.vt;
    let icrit = is * (ec - 1.0);
    let gcrit = inv_vt_is * ec;
    let (itf, gtf) = if vbe < vcrit {
        (is * (ef - 1.0), inv_vt_is * ef)
    } else {
        (gcrit * (vbe - vcrit) + icrit, gcrit)
    };
    // `_gtr`: production uses gtr only in the omitted vbc branch (see doc above).
    let (itr, _gtr) = if vbc < vcrit {
        (is * (er - 1.0), inv_vt_is * er)
    } else {
        (gcrit * (vbc - vcrit) + icrit, gcrit)
    };
    let inv_af = if p.vaf > 0.0 { 1.0 / p.vaf } else { 0.0 };
    let inv_ar = if p.var > 0.0 { 1.0 / p.var } else { 0.0 };
    let mut inv1 = 1.0 - vbc * inv_af - vbe * inv_ar;
    if inv1 < 0.1 {
        inv1 = 0.1;
    }
    let q1 = 1.0 / inv1;
    let dq1_vbe = q1 * q1 * inv_ar;
    let inv_kf = if p.ikf > 0.0 { 1.0 / p.ikf } else { 0.0 };
    let inv_kr = if p.ikr > 0.0 { 1.0 / p.ikr } else { 0.0 };
    let q2 = itf * inv_kf + itr * inv_kr;
    let dq2_vbe = gtf * inv_kf;
    let s = (4.0 * q2 + 1.0).sqrt();
    let qb = q1 * (1.0 + s) / 2.0;
    let dqb_vbe = dq1_vbe * (1.0 + s) / 2.0 + q1 * dq2_vbe / s;
    let diff = itf - itr;
    let it = diff / qb;
    let qbsq = qb * qb;
    let dit_vbe = (gtf * qb - diff * dqb_vbe) / qbsq;
    let inv_bf = 1.0 / p.bf;
    let inv_br = 1.0 / p.br;
    let ic = it - itr * inv_br;
    let ib = itf * inv_bf + itr * inv_br;
    let ie = -(ic + ib);
    let dic_vbe = dit_vbe;
    let dib_vbe = gtf * inv_bf;
    let die_vbe = -(dic_vbe + dib_vbe);
    PreOut6 {
        ib,
        ic,
        ie,
        dic_vbe,
        dib_vbe,
        die_vbe,
    }
}

/// f32 mirror — identical op tree, `f32` throughout; result cast back to f64.
#[inline(always)]
fn preexp_lane_f32(vbe: f32, vbc: f32, ef: f32, er: f32, ec: f32, p: &BjtParams) -> PreOut6 {
    let is = p.is as f32;
    let vcrit = p.vcrit as f32;
    let inv_vt_is = is / p.vt as f32;
    let icrit = is * (ec - 1.0);
    let gcrit = inv_vt_is * ec;
    let (itf, gtf) = if vbe < vcrit {
        (is * (ef - 1.0), inv_vt_is * ef)
    } else {
        (gcrit * (vbe - vcrit) + icrit, gcrit)
    };
    let (itr, _gtr) = if vbc < vcrit {
        (is * (er - 1.0), inv_vt_is * er)
    } else {
        (gcrit * (vbc - vcrit) + icrit, gcrit)
    };
    let inv_af = if p.vaf > 0.0 { 1.0 / p.vaf as f32 } else { 0.0 };
    let inv_ar = if p.var > 0.0 { 1.0 / p.var as f32 } else { 0.0 };
    let mut inv1 = 1.0 - vbc * inv_af - vbe * inv_ar;
    if inv1 < 0.1 {
        inv1 = 0.1;
    }
    let q1 = 1.0 / inv1;
    let dq1_vbe = q1 * q1 * inv_ar;
    let inv_kf = if p.ikf > 0.0 { 1.0 / p.ikf as f32 } else { 0.0 };
    let inv_kr = if p.ikr > 0.0 { 1.0 / p.ikr as f32 } else { 0.0 };
    let q2 = itf * inv_kf + itr * inv_kr;
    let dq2_vbe = gtf * inv_kf;
    let s = (4.0 * q2 + 1.0).sqrt();
    let qb = q1 * (1.0 + s) / 2.0;
    let dqb_vbe = dq1_vbe * (1.0 + s) / 2.0 + q1 * dq2_vbe / s;
    let diff = itf - itr;
    let it = diff / qb;
    let qbsq = qb * qb;
    let dit_vbe = (gtf * qb - diff * dqb_vbe) / qbsq;
    let inv_bf = 1.0 / p.bf as f32;
    let inv_br = 1.0 / p.br as f32;
    let ic = it - itr * inv_br;
    let ib = itf * inv_bf + itr * inv_br;
    let ie = -(ic + ib);
    let dic_vbe = dit_vbe;
    let dib_vbe = gtf * inv_bf;
    let die_vbe = -(dic_vbe + dib_vbe);
    PreOut6 {
        ib: ib as f64,
        ic: ic as f64,
        ie: ie as f64,
        dic_vbe: dic_vbe as f64,
        dib_vbe: dib_vbe as f64,
        die_vbe: die_vbe as f64,
    }
}

/// Width-4 f64 batch — production shape (3 BJTs + pad lane). `#[no_mangle]` gives
/// objdump a stable symbol (expect packed vdivpd/vsqrtpd over the lanes); plain
/// `fn` (not `extern "C"`) avoids FFI-safety lints on the by-ref array args.
#[no_mangle]
#[inline(never)]
pub fn preexp_batch4_f64(
    vbe: &[f64; 4],
    vbc: &[f64; 4],
    ef: &[f64; 4],
    er: &[f64; 4],
    ec: &[f64; 4],
    out: &mut [PreOut6; 4],
) {
    let p = spike_params();
    for l in 0..4 {
        out[l] = preexp_lane_f64(vbe[l], vbc[l], ef[l], er[l], ec[l], &p[l]);
    }
}

/// Width-4 f32 batch — same shape, f32 algebra. Expect packed vdivps/vsqrtps.
#[no_mangle]
#[inline(never)]
pub fn preexp_batch4_f32(
    vbe: &[f32; 4],
    vbc: &[f32; 4],
    ef: &[f32; 4],
    er: &[f32; 4],
    ec: &[f32; 4],
    out: &mut [PreOut6; 4],
) {
    let p = spike_params();
    for l in 0..4 {
        out[l] = preexp_lane_f32(vbe[l], vbc[l], ef[l], er[l], ec[l], &p[l]);
    }
}

/// The BA283's three BJTs (TR1=BC184C, TR2=BC184C, TR3=Q2N3055) + a pad lane.
fn spike_params() -> [BjtParams; 4] {
    [bc184c(), bc184c(), q2n3055(), bc184c()]
}

/// f64 exp of v/vt for the spike's transport terms (reference path; production
/// computes these once and the f32 kernel reuses them as f32 casts).
fn ex(v: f64, vt: f64) -> f64 {
    (v / vt).exp()
}

/// Step 2.0a — accuracy: f32 vs f64 post-exp algebra over the junction domain.
/// Reports max relative error of the six outputs.
///
/// RESULT (2026-06-09, NO-GO): max_rel = 1.86e-4 (field `ie` at vbe=0.35,
/// vbc=0.35 — the forward-BC saturation edge where ic+ib cancel). This EXCEEDS
/// the 1e-4 GO pre-screen, so f32 risks an op-point bias that erodes the −50 dB
/// budget. Paired with the bench (`bench_preexp`: only +6.1% — see asm: f64
/// already packs width-4 vdivpd/vsqrtpd, f32 just swaps pd→ps with no extra
/// lanes at batch-3), f32 device kernels are a dead lever. The assert below is
/// loosened to a sanity bound to keep the record green; it is NOT the GO bar.
#[test]
fn preexp_f32_accuracy() {
    let p = spike_params();
    let mut max_rel = 0.0f64;
    let mut worst = (0.0f64, 0.0f64, "");
    let mut n = 0usize;
    // Forward-active BE (0.3..0.85 V) × reverse BC (-12..0.4 V): the operating
    // envelope the Newton solve actually visits.
    let mut vbe = 0.30;
    while vbe <= 0.85 {
        let mut vbc = -12.0;
        while vbc <= 0.4 {
            for l in 0..3 {
                let vt = p[l].vt;
                let (ef, er, ecc) = (ex(vbe, vt), ex(vbc, vt), ex(p[l].vcrit, vt));
                let a = preexp_lane_f64(vbe, vbc, ef, er, ecc, &p[l]);
                let b = preexp_lane_f32(
                    vbe as f32,
                    vbc as f32,
                    ef as f32,
                    er as f32,
                    ecc as f32,
                    &p[l],
                );
                let mut rel = |x: f64, y: f64, name: &'static str| {
                    let denom = x.abs().max(1e-12);
                    let r = (x - y).abs() / denom;
                    n += 1;
                    if r > max_rel {
                        max_rel = r;
                        worst = (vbe, vbc, name);
                    }
                };
                rel(a.ib, b.ib, "ib");
                rel(a.ic, b.ic, "ic");
                rel(a.ie, b.ie, "ie");
                rel(a.dic_vbe, b.dic_vbe, "dic_vbe");
                rel(a.dib_vbe, b.dib_vbe, "dib_vbe");
                rel(a.die_vbe, b.die_vbe, "die_vbe");
            }
            vbc += 0.05;
        }
        vbe += 0.005;
    }
    println!(
        "preexp_f32_accuracy: {n} samples, max_rel={max_rel:.3e} at vbe={:.3} vbc={:.3} field={}",
        worst.0, worst.1, worst.2
    );
    // Sanity bound only (the 1e-4 GO bar was missed — see fn doc). Trips if a
    // future edit grossly regresses the f32 algebra.
    assert!(
        max_rel <= 1e-3,
        "f32 post-exp rel-error {max_rel:.3e} unexpectedly large (field={}, vbe={:.3}, vbc={:.3})",
        worst.2,
        worst.0,
        worst.1
    );
}

fn preexp_bench_inputs() -> ([f64; 4], [f64; 4], [f64; 4], [f64; 4], [f64; 4]) {
    let p = spike_params();
    let vbe = [0.62, 0.60, 0.55, 0.0];
    let vbc = [-6.0, -5.0, -3.0, 0.0];
    let mut ef = [0.0; 4];
    let mut er = [0.0; 4];
    let mut ec = [0.0; 4];
    for l in 0..4 {
        let vt = if l < 3 { p[l].vt } else { 0.02585 };
        ef[l] = ex(vbe[l], vt);
        er[l] = ex(vbc[l], vt);
        ec[l] = ex(p[l.min(2)].vcrit, vt);
    }
    (vbe, vbc, ef, er, ec)
}

const PRE_ITERS: usize = 8_000_000;

/// Step 2.0b — A/B throughput, both loops timed in-process so cargo/harness
/// overhead cancels. Reports ns/call and the f32 speedup. GO bar >= ~15%.
/// Run with: `cargo test --release -p ba283 --test autovec_spike bench_preexp -- --ignored --nocapture`
#[test]
#[ignore]
fn bench_preexp() {
    use std::time::Instant;
    let (vbe, vbc, ef, er, ec) = preexp_bench_inputs();
    let (vbe32, vbc32) = (vbe.map(|x| x as f32), vbc.map(|x| x as f32));
    let (ef32, er32, ec32) = (ef.map(|x| x as f32), er.map(|x| x as f32), ec.map(|x| x as f32));
    let mut out = [PreOut6::default(); 4];

    // Warm up both paths (page in code, settle the clock).
    for _ in 0..200_000 {
        preexp_batch4_f64(&vbe, &vbc, &ef, &er, &ec, &mut out);
        preexp_batch4_f32(&vbe32, &vbc32, &ef32, &er32, &ec32, &mut out);
    }

    let mut best_f64 = f64::INFINITY;
    let mut best_f32 = f64::INFINITY;
    let mut acc = 0.0f64;
    for _ in 0..5 {
        let t = Instant::now();
        for _ in 0..PRE_ITERS {
            preexp_batch4_f64(
                std::hint::black_box(&vbe),
                std::hint::black_box(&vbc),
                std::hint::black_box(&ef),
                std::hint::black_box(&er),
                std::hint::black_box(&ec),
                std::hint::black_box(&mut out),
            );
            acc += out[2].ic + out[0].dic_vbe;
        }
        best_f64 = best_f64.min(t.elapsed().as_secs_f64());

        let t = Instant::now();
        for _ in 0..PRE_ITERS {
            preexp_batch4_f32(
                std::hint::black_box(&vbe32),
                std::hint::black_box(&vbc32),
                std::hint::black_box(&ef32),
                std::hint::black_box(&er32),
                std::hint::black_box(&ec32),
                std::hint::black_box(&mut out),
            );
            acc += out[2].ic + out[0].dic_vbe;
        }
        best_f32 = best_f32.min(t.elapsed().as_secs_f64());
    }
    let ns64 = best_f64 / PRE_ITERS as f64 * 1e9;
    let ns32 = best_f32 / PRE_ITERS as f64 * 1e9;
    println!(
        "bench_preexp: f64={ns64:.3} ns/call  f32={ns32:.3} ns/call  speedup={:.2}x ({:+.1}%)  acc={}",
        ns64 / ns32,
        (ns64 / ns32 - 1.0) * 100.0,
        std::hint::black_box(acc)
    );
}
