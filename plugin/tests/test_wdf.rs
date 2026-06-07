//! Golden-vector tests for the WDF/MNA circuit solver (ports ~/Git/ba283_wdf/main.cpp).
//! Fixtures captured from the C++ reference at the realtime tier — see plugin/tests/fixtures/wdf_golden.rs.

use ba283::wdf::{node, NUM_NODES};

#[test]
fn node_count_and_ids() {
    assert_eq!(NUM_NODES, 21);
    assert_eq!(node::N_A, 0);
    assert_eq!(node::N_INPUT, 1);
    assert_eq!(node::N_SEC_IN, 19); // listening tap (NOT last node — N_ZOBEL follows)
    assert_eq!(node::N_ZOBEL, 20); // last node; primary Zobel series-RC mid-node
}
