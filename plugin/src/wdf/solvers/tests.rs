use super::*;

#[test]
fn gauss_solve_3x3() {
    // [2 1 -1; -3 -1 2; -2 1 2] x = [8; -11; -3]  ->  x = [2, 3, -1]
    let n = 3;
    let m = [[2.0, 1.0, -1.0], [-3.0, -1.0, 2.0], [-2.0, 1.0, 2.0]];
    let mut a = [0.0f64; NUM_NODES * NUM_NODES];
    for (i, row) in m.iter().enumerate() {
        for (j, &val) in row.iter().enumerate() {
            a[i * n + j] = val;
        }
    }
    let mut b = [0.0f64; NUM_NODES];
    b[0] = 8.0;
    b[1] = -11.0;
    b[2] = -3.0;
    assert!(WdfModel::gauss_solve(&mut a, &mut b, n));
    assert!((b[0] - 2.0).abs() < 1e-12, "x0={}", b[0]);
    assert!((b[1] - 3.0).abs() < 1e-12, "x1={}", b[1]);
    assert!((b[2] + 1.0).abs() < 1e-12, "x2={}", b[2]);
}

/// `gauss_solve_16` must be BIT-FOR-BIT identical to `gauss_solve(.., 16)` across a wide range
/// of 16x16 systems (well-conditioned, pivot-triggering, near-singular, negative, mixed
/// magnitudes). Physical-swap vs index-swap is bit-identical; this also guards against fma
/// contraction in the vectorized elimination loop (which would diverge in the last bit).
#[test]
fn gauss_solve_16_is_bit_exact() {
    const N: usize = 16;
    // Deterministic LCG (Numerical Recipes constants), no external crates.
    let mut state: u64 = 0x1234_5678_9abc_def0;
    let mut next = || -> f64 {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        // top 53 bits -> [0,1)
        ((state >> 11) as f64) / ((1u64 << 53) as f64)
    };

    // Build a 16x16 system with a chosen "flavor" controlling conditioning/magnitudes.
    let make = |flavor: u32, next: &mut dyn FnMut() -> f64| -> ([f64; N * N], [f64; N]) {
        let mut a = [0.0f64; N * N];
        let mut b = [0.0f64; N];
        for r in 0..N {
            for c in 0..N {
                let base = next() * 2.0 - 1.0; // [-1,1]
                a[r * N + c] = match flavor {
                    // Well-conditioned, diagonally dominant.
                    0 => {
                        if r == c {
                            base + (N as f64) * 4.0
                        } else {
                            base
                        }
                    }
                    // Pivot-triggering: huge off-diagonals, small diagonal.
                    1 => {
                        if r == c {
                            base * 1e-6
                        } else if c > r {
                            base * 1e6
                        } else {
                            base
                        }
                    }
                    // Near-singular-ish: two nearly-proportional rows.
                    2 => {
                        if r == 5 {
                            a[(4) * N + c] * (1.0 + 1e-9) + base * 1e-12
                        } else if r == c {
                            base + 2.0
                        } else {
                            base
                        }
                    }
                    // Mixed magnitudes / negatives.
                    3 => {
                        let scale = 10f64.powi(((next() * 12.0) as i32) - 6);
                        base * scale
                    }
                    // All negative-ish with moderate diagonal.
                    _ => {
                        if r == c {
                            -(base.abs() + 5.0)
                        } else {
                            -base.abs()
                        }
                    }
                };
            }
            b[r] = next() * 20.0 - 10.0;
        }
        (a, b)
    };

    let mut cases = 0usize;
    for flavor in 0..5u32 {
        for _ in 0..400 {
            let (a0, b0) = make(flavor, &mut next);

            // Reference path: gauss_solve over a 16x16 region of an N-wide buffer.
            let mut a_ref = a0;
            let mut b_ref = b0;
            let ok_ref = WdfModel::gauss_solve(&mut a_ref, &mut b_ref, N);

            let mut a_16 = a0;
            let mut b_16 = b0;
            let ok_16 = WdfModel::gauss_solve_16(&mut a_16, &mut b_16);

            assert_eq!(
                ok_ref, ok_16,
                "case {cases} (flavor {flavor}): singular-bool mismatch ref={ok_ref} 16={ok_16}"
            );
            if ok_ref {
                for i in 0..N {
                    let rb = b_ref[i].to_bits();
                    let xb = b_16[i].to_bits();
                    assert_eq!(
                        rb, xb,
                        "case {cases} (flavor {flavor}) elem {i}: bit divergence ref={:#018x} ({}) 16={:#018x} ({})",
                        rb, b_ref[i], xb, b_16[i]
                    );
                }
            }
            cases += 1;
        }
    }
    eprintln!("gauss_solve_16_is_bit_exact: {cases} systems all bit-identical");
}

/// `gauss_solve_6` must be BIT-FOR-BIT identical to `gauss_solve(.., 6)` across a wide range
/// of 6x6 systems (well-conditioned, pivot-triggering, near-singular, negative, mixed
/// magnitudes). Physical-swap vs index-swap is bit-identical; this also guards against fma
/// contraction in the elimination loop (which would diverge in the last bit).
#[test]
fn gauss_solve_6_is_bit_exact() {
    const N: usize = 6;
    // Deterministic LCG (Numerical Recipes constants), no external crates.
    let mut state: u64 = 0x1234_5678_9abc_def0;
    let mut next = || -> f64 {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((state >> 11) as f64) / ((1u64 << 53) as f64)
    };

    // Build a 6x6 system with a chosen "flavor" controlling conditioning/magnitudes.
    let make = |flavor: u32, next: &mut dyn FnMut() -> f64| -> ([f64; N * N], [f64; N]) {
        let mut a = [0.0f64; N * N];
        let mut b = [0.0f64; N];
        for r in 0..N {
            for c in 0..N {
                let base = next() * 2.0 - 1.0; // [-1,1]
                a[r * N + c] = match flavor {
                    // Well-conditioned, diagonally dominant.
                    0 => {
                        if r == c {
                            base + (N as f64) * 4.0
                        } else {
                            base
                        }
                    }
                    // Pivot-triggering: huge off-diagonals, small diagonal.
                    1 => {
                        if r == c {
                            base * 1e-6
                        } else if c > r {
                            base * 1e6
                        } else {
                            base
                        }
                    }
                    // Near-singular-ish: two nearly-proportional rows.
                    2 => {
                        if r == 3 {
                            a[(2) * N + c] * (1.0 + 1e-9) + base * 1e-12
                        } else if r == c {
                            base + 2.0
                        } else {
                            base
                        }
                    }
                    // Mixed magnitudes / negatives.
                    3 => {
                        let scale = 10f64.powi(((next() * 12.0) as i32) - 6);
                        base * scale
                    }
                    // All negative-ish with moderate diagonal.
                    _ => {
                        if r == c {
                            -(base.abs() + 5.0)
                        } else {
                            -base.abs()
                        }
                    }
                };
            }
            b[r] = next() * 20.0 - 10.0;
        }
        (a, b)
    };

    let mut cases = 0usize;
    for flavor in 0..5u32 {
        for _ in 0..400 {
            let (a0, b0) = make(flavor, &mut next);

            // Reference path: gauss_solve over a 6x6 region of an N-wide buffer.
            let mut a_ref = a0;
            let mut b_ref = b0;
            let ok_ref = WdfModel::gauss_solve(&mut a_ref, &mut b_ref, N);

            let mut a_6 = a0;
            let mut b_6 = b0;
            let ok_6 = WdfModel::gauss_solve_6(&mut a_6, &mut b_6);

            assert_eq!(
                ok_ref, ok_6,
                "case {cases} (flavor {flavor}): singular-bool mismatch ref={ok_ref} 6={ok_6}"
            );
            if ok_ref {
                for i in 0..N {
                    let rb = b_ref[i].to_bits();
                    let xb = b_6[i].to_bits();
                    assert_eq!(
                        rb, xb,
                        "case {cases} (flavor {flavor}) elem {i}: bit divergence ref={:#018x} ({}) 6={:#018x} ({})",
                        rb, b_ref[i], xb, b_6[i]
                    );
                }
            }
            cases += 1;
        }
    }
    eprintln!("gauss_solve_6_is_bit_exact: {cases} systems all bit-identical");
}

/// The physical-swap `lu_factor_10` + `lu_solve_10` pair must produce BIT-FOR-BIT identical
/// solutions to the generic index-swap `lu_factor(.., 10)` + `lu_solve(.., 10, ..)` pair across
/// many 10x10 systems (well-conditioned, pivot-triggering, near-singular, negative, mixed).
/// Storage layout (physical rows + perm vs `piv[]` indirection) differs; arithmetic must not.
#[test]
fn lu_10_is_bit_exact() {
    const N: usize = 10;
    let mut state: u64 = 0x0fed_cba9_8765_4321;
    let mut next = || -> f64 {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((state >> 11) as f64) / ((1u64 << 53) as f64)
    };

    let make = |flavor: u32, next: &mut dyn FnMut() -> f64| -> ([f64; N * N], [f64; N]) {
        let mut a = [0.0f64; N * N];
        let mut b = [0.0f64; N];
        for r in 0..N {
            for c in 0..N {
                let base = next() * 2.0 - 1.0;
                a[r * N + c] = match flavor {
                    0 => {
                        if r == c {
                            base + (N as f64) * 4.0
                        } else {
                            base
                        }
                    }
                    1 => {
                        if r == c {
                            base * 1e-6
                        } else if c > r {
                            base * 1e6
                        } else {
                            base
                        }
                    }
                    2 => {
                        if r == 6 {
                            a[(5) * N + c] * (1.0 + 1e-9) + base * 1e-12
                        } else if r == c {
                            base + 2.0
                        } else {
                            base
                        }
                    }
                    3 => {
                        let scale = 10f64.powi(((next() * 12.0) as i32) - 6);
                        base * scale
                    }
                    _ => {
                        if r == c {
                            -(base.abs() + 5.0)
                        } else {
                            -base.abs()
                        }
                    }
                };
            }
            b[r] = next() * 20.0 - 10.0;
        }
        (a, b)
    };

    let mut cases = 0usize;
    for flavor in 0..5u32 {
        for _ in 0..400 {
            let (a0, b0) = make(flavor, &mut next);

            // Reference path: generic index-swap lu_factor + lu_solve.
            let mut a_ref = a0;
            let mut piv_ref = [0usize; NUM_NODES];
            let ok_ref = WdfModel::lu_factor(&mut a_ref, &mut piv_ref, N);
            let mut b_ref = b0;
            if ok_ref {
                WdfModel::lu_solve(&a_ref, &piv_ref, N, &mut b_ref);
            }

            // Physical-swap path.
            let mut a_10 = a0;
            let mut perm_10 = [0usize; NUM_NODES];
            let ok_10 = WdfModel::lu_factor_10(&mut a_10, &mut perm_10);
            let mut b_10 = b0;
            if ok_10 {
                WdfModel::lu_solve_10(&a_10, &perm_10, &mut b_10);
            }

            assert_eq!(
                ok_ref, ok_10,
                "case {cases} (flavor {flavor}): singular-bool mismatch ref={ok_ref} 10={ok_10}"
            );
            if ok_ref {
                for i in 0..N {
                    let rb = b_ref[i].to_bits();
                    let xb = b_10[i].to_bits();
                    assert_eq!(
                        rb, xb,
                        "case {cases} (flavor {flavor}) elem {i}: bit divergence ref={:#018x} ({}) 10={:#018x} ({})",
                        rb, b_ref[i], xb, b_10[i]
                    );
                }
            }
            cases += 1;
        }
    }
    eprintln!("lu_10_is_bit_exact: {cases} systems all bit-identical");
}

