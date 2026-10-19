//! Each nodal tier must play its calibrated VCF brightness (Low `LOW_BRIGHT`,
//! High/Best `CIRCUIT_BRIGHT`), scaled only by the analog-wear age factor. The
//! plugin applies analog wear on its first block and after every host reset,
//! so this must hold through those updates, not only on a fresh `Voice`.

mod common;

use common::apply_plugin_defaults;
use plugin303::dsp::filter::Tier;
use plugin303::dsp::voice::{Voice, CIRCUIT_BRIGHT, LOW_BRIGHT};

fn assert_cals(v: &Voice, age_factor: f64, when: &str) {
    let expected = [LOW_BRIGHT, CIRCUIT_BRIGHT, CIRCUIT_BRIGHT].map(|c| c * age_factor);
    let actual = v.dbg_ladder_cals();
    for ((name, e), a) in ["low", "high", "best"].iter().zip(expected).zip(actual) {
        assert!((a - e).abs() < 1e-12, "{when}: {name} ladder cal {a:.4}, expected {e:.4}");
    }
}

#[test]
fn tier_brightness_survives_plugin_param_updates() {
    let mut v = Voice::new(48_000.0);
    assert_cals(&v, 1.0, "Voice::new");

    for tier in [Tier::Low, Tier::High, Tier::Best, Tier::Draft] {
        v.set_tier_immediate(tier);
    }
    assert_cals(&v, 1.0, "after tier changes");

    apply_plugin_defaults(&mut v, 0);
    assert_cals(&v, 1.0, "first plugin block (age 0)");

    apply_plugin_defaults(&mut v, 50);
    assert_cals(&v, 1.0 - 0.15 * 0.5, "age 50");

    v.reset();
    apply_plugin_defaults(&mut v, 0);
    assert_cals(&v, 1.0, "host reset + first block");
}
