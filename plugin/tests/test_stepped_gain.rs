//! Behavior test for the hardware-stepped gain feature: the 5 real BA283 R_KJ switch positions must
//! map to a strictly monotonic, discrete small-signal gain ladder (more feedback ⇒ less R ⇒ more
//! gain), with the open contact (idx 0) the lowest. Validates the table values themselves — see the
//! `GAIN_STEPS` table in lib.rs (kept in sync with this list).

use ba283::wdf::{QualityConfig, WdfModel};

/// R_KJ values stamped per switch position, mirroring `lib.rs::GAIN_STEPS[*].r_kj_ohm`
/// (open → 1e9 sentinel). Index order = ascending dB / descending R.
const STEP_R_KJ: [f64; 5] = [1.0e9, 1_200.0, 470.0, 220.0, 110.0];

#[test]
fn stepped_gain_is_monotonic_and_discrete() {
    let host = 48_000.0_f64;
    let mut gains = Vec::with_capacity(STEP_R_KJ.len());
    for &r in &STEP_R_KJ {
        // Fresh model per step so each gain is measured from the same settled cold state (the plugin
        // re-stamps + re-settles on a real detent crossing; this is the equivalent steady reading).
        let mut m = WdfModel::new_with(QualityConfig::realtime(), host, r, 0.5);
        m.set_rgain(r);
        m.warmup();
        let g = m.measure_gain();
        assert!(g.is_finite() && g > 0.0, "step R={r} Ω gain must be finite/positive, got {g}");
        gains.push(g);
    }

    // Strictly increasing: each lower-R position is louder than the one before.
    for w in gains.windows(2) {
        assert!(
            w[1] > w[0],
            "gain ladder must strictly increase as R_KJ drops; got {:?}",
            gains
        );
    }

    // The open contact (idx 0) is the quietest, the 110 Ω position the loudest, and the steps are
    // distinct (no two positions collapse to the same gain within 1%).
    assert!(gains[0] < gains[STEP_R_KJ.len() - 1], "open step must be the quietest");
    for w in gains.windows(2) {
        assert!(
            (w[1] - w[0]).abs() > 0.01 * w[0],
            "adjacent steps must be audibly distinct (>1% apart); got {:?}",
            gains
        );
    }
}
