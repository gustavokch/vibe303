//! Task 1 SPIKE: feasibility of a branchless batched `exp` kernel that
//! autovectorizes, stays BIT-EXACT vs the scalar `fast_exp`, and saves
//! instructions. This is a GO/NO-GO gate — NOT production code, NOT on the
//! audio hot path.

use ba283::wdf::fast_exp;

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
