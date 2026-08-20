use plugin303::dsp::vco::Vco303;

#[test]
fn test_recalibration_scale_alters_lut() {
    let sr = 48000.0;
    let mut vco_recal = Vco303::new(sr);
    let mut vco_uncal = Vco303::new(sr);

    vco_recal.set_analog_wear(0.10, 40, 1337);
    vco_uncal.set_analog_wear(0.10, 40, 1337);

    // Sync build with recal_scale = 1.0 (fully recalibrated) vs 0.0 (unserviced)
    vco_recal.recalculate_luts_sync_with_recal(40, 0.10, 1337, 1.0);
    vco_uncal.recalculate_luts_sync_with_recal(40, 0.10, 1337, 0.0);

    assert_ne!(vco_recal.get_cv_lut(), vco_uncal.get_cv_lut());
}
