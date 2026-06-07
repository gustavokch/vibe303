// plugin/tests/test_params.rs
use ba283::{Ba283Params, InferenceBackend};
use nih_plug::prelude::*;

#[test]
fn default_params_have_expected_values() {
    let p = Ba283Params::default();
    assert!((p.drive_db.value() - 0.0).abs() < 1e-6);
    assert!((p.bias.value() - 0.5).abs() < 1e-6);
    assert!((p.output_db.value() - 0.0).abs() < 1e-6);
    assert!((p.mix.value() - 100.0).abs() < 1e-6);
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
