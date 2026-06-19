//! levelNorm is now a STATIC per-position circuit-makeup table (`1/G_i`), not a runtime off-thread
//! probe: `circuit_gain` is a 5-position switch, so each position's small-signal voltage gain is a
//! fixed constant embedded in `GAIN_STEPS` (measured offline by `bin/measure_gi`, drift-guarded by
//! `tests/test_stepped_gain.rs`). These tests cover the table accessor + the Drive-cancel formula.
//! (The old `force_probe`/`probe_trim` worker path was removed with the probe.)
use ba283::{norm_apply_gain, Ba283Plugin};

/// Each position publishes a finite, plausible makeup `1/G_v` (`G_v > 1` for the amplifying preamp ⇒
/// makeup < 1), and the makeup DECREASES monotonically as the gain position rises (idx 0 = least
/// gain ⇒ largest makeup). Proves the table is wired and ordered the way `process` relies on.
#[test]
fn makeup_table_plausible_and_monotonic() {
    let mut prev = f32::INFINITY;
    for pos in 0..5 {
        let m = Ba283Plugin::pos_makeup(pos);
        assert!(m.is_finite() && m > 0.0 && m < 1.0, "pos {pos}: implausible makeup {m}");
        assert!(m < prev, "pos {pos}: makeup {m} not below previous {prev} (gain must rise)");
        prev = m;
    }
}

/// The table accessor clamps out-of-range indices to the valid 0..=4 span (never panics).
#[test]
fn pos_makeup_clamps() {
    assert_eq!(Ba283Plugin::pos_makeup(-2), Ba283Plugin::pos_makeup(0));
    assert_eq!(Ba283Plugin::pos_makeup(99), Ba283Plugin::pos_makeup(4));
}

/// `norm_apply_gain` (shared by `process` and the regression tests) cancels the circuit gain and the
/// known Drive scalar: trimming `1/G_v` then dividing Drive lands the applied gain at `1/(G_v*drive)`.
#[test]
fn apply_gain_cancels_drive() {
    let trim = Ba283Plugin::pos_makeup(3); // 1/G_v at the 30 dB position
    let drive = 4.0; // +12 dB
    let g = norm_apply_gain(trim, drive);
    assert!((g - trim / drive).abs() < 1e-6, "drive not cancelled: {g} vs {}", trim / drive);
    // No Drive => the applied gain is just the trim.
    assert!((norm_apply_gain(trim, 1.0) - trim).abs() < 1e-6);
}
