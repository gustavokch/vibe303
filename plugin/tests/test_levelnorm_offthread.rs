//! levelNorm is now a STATIC per-position, DRIVE-swept circuit-makeup table (`1/G_sat(drive)`), not a
//! runtime off-thread probe: `circuit_gain` is a 5-position switch, so each position's voltage gain
//! is a fixed curve over Drive embedded in `GAIN_STEPS` (measured offline by `bin/measure_gi`,
//! drift-guarded by `tests/test_stepped_gain.rs`). These tests cover the table accessor + the
//! Drive-divide formula. (The old `force_probe`/`probe_trim` worker path was removed with the probe.)
use ba283::{norm_apply_gain, Ba283Plugin};

/// At unity Drive each position publishes a finite, plausible makeup `1/G_v` (`G_v > 1` for the
/// amplifying preamp ⇒ makeup < 1), and the makeup DECREASES monotonically as the gain position rises
/// (idx 0 = least gain ⇒ largest makeup). Proves the table is wired and ordered the way `process`
/// relies on.
#[test]
fn makeup_table_plausible_and_monotonic() {
    let mut prev = f32::INFINITY;
    for pos in 0..5 {
        let m = Ba283Plugin::pos_makeup(pos, 0.0);
        assert!(m.is_finite() && m > 0.0 && m < 1.0, "pos {pos}: implausible makeup {m}");
        assert!(m < prev, "pos {pos}: makeup {m} not below previous {prev} (gain must rise)");
        prev = m;
    }
}

/// The Drive-swept makeup RISES with Drive at a saturating position: as Drive pushes the circuit into
/// saturation `G_sat` drops, so `1/G_sat` grows — this is what stops the `/drive` from over-
/// attenuating the output (the regression). Pin the direction at the hottest 35 dB position.
#[test]
fn makeup_rises_with_drive_under_saturation() {
    let unity = Ba283Plugin::pos_makeup(4, 0.0);
    let driven = Ba283Plugin::pos_makeup(4, 24.0);
    assert!(
        driven > unity * 2.0,
        "makeup must rise with Drive under saturation: +24 dB {driven} vs unity {unity}"
    );
}

/// The table accessor clamps out-of-range indices to the valid 0..=4 span (never panics).
#[test]
fn pos_makeup_clamps() {
    assert_eq!(Ba283Plugin::pos_makeup(-2, 0.0), Ba283Plugin::pos_makeup(0, 0.0));
    assert_eq!(Ba283Plugin::pos_makeup(99, 0.0), Ba283Plugin::pos_makeup(4, 0.0));
}

/// `norm_apply_gain` (shared by `process` and the regression tests) divides the known Drive scalar
/// out of the circuit-makeup trim: applied gain = `trim / drive`. With the Drive-swept trim this
/// lands at `1/(G_sat(drive)·drive)`, holding loudness even under saturation.
#[test]
fn apply_gain_cancels_drive() {
    let trim = Ba283Plugin::pos_makeup(3, 12.0); // 1/G_sat at the 30 dB position, +12 dB Drive
    let drive = 4.0; // +12 dB
    let g = norm_apply_gain(trim, drive);
    assert!((g - trim / drive).abs() < 1e-6, "drive not divided: {g} vs {}", trim / drive);
    // No Drive => the applied gain is just the trim.
    assert!((norm_apply_gain(trim, 1.0) - trim).abs() < 1e-6);
}
