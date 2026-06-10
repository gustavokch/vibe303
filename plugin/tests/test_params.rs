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
    // levelNorm defaults OFF: faithful preamp gain out of the box.
    assert!(!p.level_norm.value());
}

#[test]
fn default_backend_is_neural_net() {
    // Preserves current behaviour: the plugin defaults to the existing GRU backend.
    let p = Ba283Params::default();
    assert_eq!(p.backend.value(), InferenceBackend::NeuralNet);
}

#[test]
fn circuit_gain_default_and_range() {
    let p = Ba283Params::default();
    assert!((p.circuit_gain.value() - 220.0).abs() < 1e-6);
    assert!(p.circuit_gain.preview_normalized(47.0) >= 0.0);
    assert!(p.circuit_gain.preview_normalized(10_000.0) <= 1.0);
    assert!((p.circuit_gain.preview_plain(0.0) - 47.0).abs() < 1e-3);
    assert!((p.circuit_gain.preview_plain(1.0) - 10_000.0).abs() < 1e-1);
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
    // Default 0.5 = realistic LO1166 core saturation (the chosen baseline amount).
    assert!((p.trafo.value() - 0.5).abs() < 1e-6);
    assert!(p.trafo.preview_normalized(0.0) >= 0.0);
    assert!(p.trafo.preview_normalized(1.0) <= 1.0);
}

#[cfg(feature = "trafo-core")]
#[test]
fn trafo_is_registered_in_param_map() {
    // Guards host visibility: the #[cfg]-gated #[id="trafo"] field must actually appear in the
    // derived Params param map (not merely exist as a struct field).
    let p = Ba283Params::default();
    let ids: Vec<String> = p.param_map().into_iter().map(|(id, _, _)| id).collect();
    assert!(ids.iter().any(|i| i == "trafo"), "trafo not in param_map: {ids:?}");
}
