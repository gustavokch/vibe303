// plugin/tests/test_params.rs
use ba283::Ba283Params;
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
fn drive_db_range_bounded() {
    let p = Ba283Params::default();
    assert!(p.drive_db.preview_normalized(-12.0) >= 0.0);
    assert!(p.drive_db.preview_normalized(24.0) <= 1.0);
}
