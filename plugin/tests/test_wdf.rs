//! Golden-vector tests for the WDF/MNA circuit solver (ports ~/Git/ba283_wdf/main.cpp).
//! Fixtures captured from the C++ reference at the realtime tier — see plugin/tests/fixtures/wdf_golden.rs.

use ba283::wdf::{self, node, NUM_NODES};

#[allow(dead_code)]
mod wdf_golden {
    include!("fixtures/wdf_golden.rs");
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
    // idle-settle with tap-delta early-exit (mirrors the C++ idleSettle), capped at 8 s.
    let chunk = (internal_sr * 0.25) as usize;
    let mut prev_tap = 1e9;
    for _ in 0..32 {
        for _ in 0..chunk {
            m.process();
        }
        let tap = m.node_v(node::N_SEC_IN);
        if (tap - prev_tap).abs() < 1e-6 {
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
