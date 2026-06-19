// plugin/tests/test_params.rs
use ba283::{Ba283Params, InferenceBackend};
use nih_plug::prelude::*;
use std::sync::atomic::Ordering;

#[test]
fn default_params_have_expected_values() {
    let p = Ba283Params::default();
    assert!((p.drive_db.value() - 0.0).abs() < 1e-6);
    assert!((p.bias.value() - 0.5).abs() < 1e-6);
    assert!((p.output_db.value() - 0.0).abs() < 1e-6);
    assert!((p.mix.value() - 100.0).abs() < 1e-6);
    // levelNorm defaults ON: inserting the plugin at defaults is level-neutral (unity).
    assert!(p.level_norm.value());
    // New staging controls default neutral / off.
    assert!((p.input_gain.value() - 0.0).abs() < 1e-6);
    assert!(!p.link.value());
}

#[test]
fn default_backend_is_wdf() {
    // The circuit-faithful WDF solver is the default mode; NeuralNet remains selectable.
    let p = Ba283Params::default();
    assert_eq!(p.backend.value(), InferenceBackend::Wdf);
}

#[test]
fn circuit_gain_default_and_range() {
    // The hardware gain switch is an IntParam indexing the 5 wafer positions (0..=4). Default idx 0
    // (15 dB) — the cleanest stock insert.
    let p = Ba283Params::default();
    assert_eq!(p.circuit_gain.value(), 0);
    assert_eq!(p.circuit_gain.preview_plain(0.0), 0);
    assert_eq!(p.circuit_gain.preview_plain(1.0), 4);
    assert!(p.circuit_gain.preview_normalized(0) >= 0.0);
    assert!(p.circuit_gain.preview_normalized(4) <= 1.0);
}

#[test]
fn drive_db_range_bounded() {
    let p = Ba283Params::default();
    assert!(p.drive_db.preview_normalized(-12.0) >= 0.0);
    assert!(p.drive_db.preview_normalized(24.0) <= 1.0);
}

#[test]
fn age_default_and_range() {
    let p = Ba283Params::default();
    // Aging off by default (bit-exact, pristine unit).
    assert!((p.age.value() - 0.0).abs() < 1e-6);
    assert!(!p.age_reroll.value());
    assert!(p.age.preview_normalized(0.0) >= 0.0);
    assert!(p.age.preview_normalized(1.0) <= 1.0);
    // Default unit seed is a fixed nonzero constant (deterministic fresh instance).
    assert_ne!(p.age_seed.load(Ordering::Relaxed), 0);
}

#[test]
fn age_seed_round_trips_through_state() {
    // The seed persists with plugin state (via #[persist]) so a saved project recalls the same
    // "unit". Exercise the Params trait's field (de)serialization directly.
    let p = Ba283Params::default();
    p.age_seed.store(0xC0FFEE_1234, Ordering::Relaxed);
    let fields = p.serialize_fields();
    assert!(fields.contains_key("age_seed"), "age_seed must be persisted");

    let p2 = Ba283Params::default();
    assert_ne!(p2.age_seed.load(Ordering::Relaxed), 0xC0FFEE_1234);
    p2.deserialize_fields(&fields);
    assert_eq!(p2.age_seed.load(Ordering::Relaxed), 0xC0FFEE_1234);
}

#[cfg(feature = "trafo-core")]
#[test]
fn trafo_default_and_range() {
    let p = Ba283Params::default();
    // ON by default + both voicing dials at noon (0.5) = today's LO1166 sound.
    assert!(p.trafo_on.value(), "transformer must default ON (current sound)");
    assert!((p.trafo_hyst.value() - 0.5).abs() < 1e-6);
    assert!((p.trafo_sat.value() - 0.5).abs() < 1e-6);
    for d in [&p.trafo_hyst, &p.trafo_sat] {
        assert!(d.preview_normalized(0.0) >= 0.0);
        assert!(d.preview_normalized(1.0) <= 1.0);
    }
}

#[cfg(feature = "trafo-core")]
#[test]
fn trafo_is_registered_in_param_map() {
    // Guards host visibility: the #[cfg]-gated transformer params must actually appear in the
    // derived Params param map (not merely exist as struct fields).
    let p = Ba283Params::default();
    let ids: Vec<String> = p.param_map().into_iter().map(|(id, _, _)| id).collect();
    for id in ["trafo_on", "trafo_hyst", "trafo_sat"] {
        assert!(ids.iter().any(|i| i == id), "{id} not in param_map: {ids:?}");
    }
}
