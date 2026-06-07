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
