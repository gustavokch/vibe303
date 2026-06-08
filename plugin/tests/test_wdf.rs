//! Golden-vector tests for the WDF/MNA circuit solver (ports ~/Git/ba283_wdf/main.cpp).
//! Fixtures captured from the C++ reference at the realtime tier — see plugin/tests/fixtures/wdf_golden.rs.

use ba283::wdf::{self, node, QualityConfig, NUM_NODES};

#[allow(dead_code)]
mod wdf_golden {
    include!("fixtures/wdf_golden.rs");
}

/// Render the golden INPUT_256 stream through a warmed WDF model at the given config.
fn run_stream(cfg: QualityConfig) -> Vec<f32> {
    let mut m = wdf::WdfModel::new_with(cfg, wdf_golden::HOST_SR, 220.0, wdf_golden::BIAS);
    m.warmup();
    wdf_golden::INPUT_256
        .iter()
        .map(|&x| m.process_host_sample(x as f32))
        .collect()
}

/// Max absolute deviation of a lever config's output stream from the realtime reference stream.
fn max_dev_vs_realtime(cfg: QualityConfig) -> f64 {
    let reference = run_stream(QualityConfig::realtime());
    let got = run_stream(cfg);
    reference
        .iter()
        .zip(got.iter())
        .map(|(&a, &b)| (a as f64 - b as f64).abs())
        .fold(0.0, f64::max)
}

#[test]
fn node_count_and_ids() {
    assert_eq!(NUM_NODES, 21);
    assert_eq!(node::N_A, 0);
    assert_eq!(node::N_INPUT, 1);
    assert_eq!(node::N_SEC_IN, 19); // listening tap (NOT last node — N_ZOBEL follows)
    assert_eq!(node::N_ZOBEL, 20); // last node; primary Zobel series-RC mid-node
}

#[test]
fn fast_math_matches_std() {
    for &x in &[-30.0f64, -1.0, 0.0, 0.5, 5.0, 25.0] {
        assert!(
            (wdf::fast_exp(x) - x.exp()).abs() <= 1e-9 * x.exp().max(1.0),
            "fast_exp({x})"
        );
    }
    for &x in &[0.05f64, 0.5, 1.0, 1.4, 10.0] {
        assert!((wdf::fast_log(x) - x.ln()).abs() <= 1e-9, "fast_log({x})");
    }
}

/// `depletion_charge_pre` (precomputed saturated-branch constants) must be BIT-for-bit
/// identical to the original `depletion_charge` across both branches. Pins the 3-way
/// duplication so a future edit to one copy can't silently break parity with the golden
/// vectors. Sweeps `v` through the reverse (`v < FC*vj`) and saturated (`v >= FC*vj`)
/// regions for every real device junction.
#[test]
fn depletion_charge_pre_is_bit_exact() {
    // (cj0, vj, m) for BC184C be/bc and 2N3055 be/bc — the only real junctions.
    let junctions = [
        (14.0e-12, 0.75, 0.33),
        (4.5e-12, 0.75, 0.33),
        (580.2e-12, 1.2, 0.45),
        (212.1e-12, 0.75, 0.4),
    ];
    for (cj0, vj, m) in junctions {
        let (c0, q0, dcdv) = wdf::dep_sat_consts(cj0, vj, m);
        for k in -200i32..=200 {
            let v = k as f64 * 0.01; // spans both branches around FC*vj
            let (q_ref, c_ref) = wdf::depletion_charge(v, cj0, vj, m);
            let (q_pre, c_pre) = wdf::depletion_charge_pre(v, cj0, vj, m, c0, q0, dcdv);
            assert_eq!(q_ref.to_bits(), q_pre.to_bits(), "q mismatch v={v} cj0={cj0}");
            assert_eq!(c_ref.to_bits(), c_pre.to_bits(), "c mismatch v={v} cj0={cj0}");
        }
    }
}

#[test]
fn compute_bjt_against_golden() {
    // BC184C (TR1/TR2 card) and Q2N3055 (TR3 card) at fixed (vbe, vbc) points captured from the
    // C++ computeBjt. Fast-math + f64 determinism makes near-bit-exact agreement realistic.
    let bc = wdf::bc184c();
    let g = &wdf_golden::BJT_BC184C;
    let o = wdf::compute_bjt(g.vbe, g.vbc, &bc);
    let rel = |a: f64, b: f64| (a - b).abs() <= 1e-9 * b.abs().max(1e-12);
    assert!(rel(o.ib, g.ib), "bc184c ib: {} vs {}", o.ib, g.ib);
    assert!(rel(o.ic, g.ic), "bc184c ic: {} vs {}", o.ic, g.ic);
    assert!(rel(o.ie, g.ie), "bc184c ie: {} vs {}", o.ie, g.ie);
    assert!(rel(o.dib_b, g.dib_b), "bc184c dib_b");
    assert!(rel(o.dic_b, g.dic_b), "bc184c dic_b");
    assert!(rel(o.dic_c, g.dic_c), "bc184c dic_c");

    let q = wdf::q2n3055();
    let g = &wdf_golden::BJT_Q2N3055;
    let o = wdf::compute_bjt(g.vbe, g.vbc, &q);
    assert!(rel(o.ib, g.ib), "q2n3055 ib: {} vs {}", o.ib, g.ib);
    assert!(rel(o.ic, g.ic), "q2n3055 ic: {} vs {}", o.ic, g.ic);
    assert!(rel(o.ie, g.ie), "q2n3055 ie: {} vs {}", o.ie, g.ie);
    assert!(rel(o.dib_b, g.dib_b), "q2n3055 dib_b");
    assert!(rel(o.dic_b, g.dic_b), "q2n3055 dic_b");
    assert!(rel(o.dic_c, g.dic_c), "q2n3055 dic_c");
}

#[test]
fn build_topology_matches_cpp() {
    let m = wdf::WdfModel::build(48_000.0 * 2.0, 220.0, wdf_golden::BIAS); // internal = 96k
    assert_eq!(m.u(), 16, "RB off => 16 unknowns");
    assert_eq!(m.n_res(), 17);
    assert_eq!(m.n_caps(), 7);
    assert_eq!(m.n_bjts(), 3);
    // known nodes (A, INPUT, 3 *_BI) are pinned out of the unknown map.
    for &id in &[node::N_A, node::N_INPUT, node::N_TR1_BI, node::N_TR2_BI, node::N_TR3_BI] {
        assert_eq!(m.urow_of(id), -1, "node {id} should be pinned");
    }
    // a solved node has a valid row.
    assert!(m.urow_of(node::N_SEC_IN) >= 0);
}

#[test]
fn process_settles_toward_golden() {
    // Mini rail-ramp + idle settle (the real warmup schedule is A5). At the realtime tier with
    // f64 determinism the operating point should land within 1e-4 of the C++ settled vector.
    let internal_sr = 96_000.0;
    let mut m = wdf::WdfModel::build(internal_sr, 220.0, wdf_golden::BIAS);
    let ramp = internal_sr as usize; // 1 s
    for i in 0..ramp {
        m.v[node::N_INPUT as usize] = 0.0;
        m.v[node::N_A as usize] = 24.0 * (i as f64) / (ramp as f64);
        m.process();
    }
    m.v[node::N_A as usize] = 24.0;
    // idle-settle with tap-delta early-exit, matching WdfModel::warmup's schedule (1e-4 threshold,
    // 4 s cap) so we land on the same early-exit operating point as the golden capture.
    let chunk = (internal_sr * 0.25) as usize;
    let mut prev_tap = 1e9;
    for _ in 0..16 {
        for _ in 0..chunk {
            m.process();
        }
        let tap = m.node_v(node::N_SEC_IN);
        if (tap - prev_tap).abs() < 1e-4 {
            break;
        }
        prev_tap = tap;
    }
    // check the DC operating-point nodes
    for &id in &[node::N_TR1_C, node::N_TR2_E, node::N_TR3_E, node::N_F, node::N_RV1_W] {
        let got = m.node_v(id);
        let exp = wdf_golden::SETTLED_V[id as usize];
        assert!((got - exp).abs() <= 1e-4, "node {id}: {got} vs {exp}");
    }
}

// ---- A5: the acceptance gate — null the Rust port against the C++ golden vectors ----

#[test]
fn warmup_reaches_golden_operating_point() {
    let mut m = wdf::WdfModel::new(wdf_golden::HOST_SR, 220.0, wdf_golden::BIAS); // internal = *OS
    m.warmup();
    for (i, &exp) in wdf_golden::SETTLED_V.iter().enumerate() {
        assert!((m.node_v(i as i32) - exp).abs() <= 1e-6, "node {i}: {} vs {exp}", m.node_v(i as i32));
    }
    assert!(
        (m.sec_in_dc() - wdf_golden::SEC_IN_DC).abs() <= 1e-6,
        "sec_in_dc: {} vs {}",
        m.sec_in_dc(),
        wdf_golden::SEC_IN_DC
    );
}

#[test]
fn decimated_output_matches_cpp() {
    let mut m = wdf::WdfModel::new(wdf_golden::HOST_SR, 220.0, wdf_golden::BIAS);
    m.warmup();
    for (i, &x) in wdf_golden::INPUT_256.iter().enumerate() {
        let y = m.process_host_sample(x as f32);
        assert!(
            (y as f64 - wdf_golden::OUTPUT_256[i]).abs() <= 1e-5,
            "sample {i}: {y} vs {}",
            wdf_golden::OUTPUT_256[i]
        );
    }
}

// ---- round-2 speed levers: A/B against the realtime reference (same fixed point) ----

#[test]
fn predict_lever_is_lossless() {
    // Newton predictor only changes the warm-start guess. Same fixed point, but at the realtime
    // tol (1e-7) the warm start lands a different (equally valid) iterate within tol — a ~1e-5
    // output blip on the odd hard sample, not the ≤1-ULP agreement a tight tol would give. Bound
    // tracks Newton tol, not f32 rounding.
    let cfg = QualityConfig { predict: true, ..QualityConfig::realtime() };
    let dev = max_dev_vs_realtime(cfg);
    assert!(dev <= 1e-4, "predict deviation {dev} exceeds 1e-4");
}

#[test]
fn schur_lever_is_lossless() {
    // Schur static-block solve: different elimination order, same solution within tol.
    let cfg = QualityConfig { solver: wdf::Solver::Schur, ..QualityConfig::realtime() };
    let dev = max_dev_vs_realtime(cfg);
    assert!(dev <= 1e-9, "schur deviation {dev} exceeds 1e-9");
}

#[test]
fn ls_trim_lever_is_lossless() {
    // Deferred-acceptance line search: bit-identical on the monotone path, ~1e-6 on hard onsets.
    let cfg = QualityConfig { ls_trim: true, ..QualityConfig::realtime() };
    let dev = max_dev_vs_realtime(cfg);
    assert!(dev <= 1e-6, "ls-trim deviation {dev} exceeds 1e-6");
}

#[test]
fn lossless_lever_combo_matches_realtime() {
    // predict + schur + ls-trim (the fast tier MINUS the lossy freeze-caps) stays tol-equivalent.
    let cfg = QualityConfig {
        predict: true,
        solver: wdf::Solver::Schur,
        ls_trim: true,
        ..QualityConfig::realtime()
    };
    let dev = max_dev_vs_realtime(cfg);
    assert!(dev <= 1e-4, "lossless combo deviation {dev} exceeds 1e-4");
}

#[test]
fn fast_tier_is_near_lossless() {
    // Fast tier adds freeze-caps (lossy). Bound is FR-gated, not bit-exact: tiny deviation only.
    let dev = max_dev_vs_realtime(QualityConfig::fast());
    assert!(dev <= 2e-3, "fast tier deviation {dev} exceeds 2e-3");
}

#[test]
fn snapshot_restore_round_trips() {
    let mut m = wdf::WdfModel::new(wdf_golden::HOST_SR, 220.0, wdf_golden::BIAS);
    m.warmup();
    m.save_snapshot();
    // run some audio, then reset → should return to the settled operating point
    for &x in wdf_golden::INPUT_256.iter().take(64) {
        m.process_host_sample(x as f32);
    }
    m.reset();
    for (i, &exp) in wdf_golden::SETTLED_V.iter().enumerate() {
        assert!((m.node_v(i as i32) - exp).abs() <= 1e-6, "node {i} after reset");
    }
    // and replaying the input from the start should reproduce the golden onset again
    for i in 0..4 {
        let y = m.process_host_sample(wdf_golden::INPUT_256[i] as f32);
        assert!((y as f64 - wdf_golden::OUTPUT_256[i]).abs() <= 1e-5, "replayed sample {i}");
    }
}

#[test]
#[ignore = "diagnostic: run with --ignored --nocapture to print the sparsity census"]
fn sparsity_census() {
    // Build + warm a realtime model EXACTLY as process_settles_toward_golden does
    // (mini rail-ramp + idle settle), then drive ~256 samples to a realistic
    // operating point before snapshotting the constant conductance matrix g0.
    let internal_sr = 96_000.0;
    let mut m = wdf::WdfModel::build(internal_sr, 220.0, wdf_golden::BIAS);
    let ramp = internal_sr as usize; // 1 s
    for i in 0..ramp {
        m.v[node::N_INPUT as usize] = 0.0;
        m.v[node::N_A as usize] = 24.0 * (i as f64) / (ramp as f64);
        m.process();
    }
    m.v[node::N_A as usize] = 24.0;
    let chunk = (internal_sr * 0.25) as usize;
    let mut prev_tap = 1e9;
    for _ in 0..16 {
        for _ in 0..chunk {
            m.process();
        }
        let tap = m.node_v(node::N_SEC_IN);
        if (tap - prev_tap).abs() < 1e-4 {
            break;
        }
        prev_tap = tap;
    }
    // drive ~256 samples to a realistic operating point
    for _ in 0..256 {
        m.process();
    }

    let u = m.u();
    let g0 = m.g0_snapshot();
    let mut zeros = 0usize;
    for i in 0..u {
        for j in 0..u {
            if g0[i * u + j] == 0.0 {
                zeros += 1;
            }
        }
    }
    eprintln!(
        "U={u} entries={} structural_zeros={zeros} density={:.1}%",
        u * u,
        100.0 * (u * u - zeros) as f64 / (u * u) as f64
    );

    // simulate dense-elimination fill-in: print per-column trailing-submatrix nonzero counts
    let mut nz: Vec<bool> = (0..u * u).map(|k| g0[k] != 0.0).collect();
    for col in 0..u {
        let live = (0..u)
            .flat_map(|r| (0..u).map(move |c| (r, c)))
            .filter(|&(r, c)| r >= col && c >= col && nz[r * u + c])
            .count();
        eprintln!("col {col:2}: live_nonzeros_in_trailing_submatrix={live}");
        for r in (col + 1)..u {
            if !nz[r * u + col] {
                continue;
            }
            for c in (col + 1)..u {
                if nz[col * u + c] {
                    nz[r * u + c] = true;
                }
            }
        }
    }
}
