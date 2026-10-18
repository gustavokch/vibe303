use plugin303::dsp::filter::Tier;
use plugin303::dsp::vco::Vco303;
use plugin303::dsp::voice::Voice;
use plugin303::params::{CircuitTopology, CompTolerance, OscWave, VcaEnvMode, VcfTaps};
use std::thread::sleep;
use std::time::Duration;

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

#[test]
fn test_extreme_wear_boundary_fallback() {
    let sr = 48000.0;
    let mut vco = Vco303::new(sr);
    vco.set_analog_wear(0.10, 100, 1337);
    vco.recalculate_luts_sync_with_recal(100, 0.10, 1337, 1.0);
    let lut = vco.get_cv_lut();
    for entry in lut.iter() {
        assert!(
            entry.is_finite(),
            "LUT entries must be finite under extreme wear"
        );
    }
}

#[test]
fn test_recal_scale_audio_integration() {
    let sr = 48000.0;
    let mut v1 = Voice::new(sr);
    let mut v2 = Voice::new(sr);

    v1.set_tier_immediate(Tier::Best);
    v2.set_tier_immediate(Tier::Best);

    v1.update_params(
        0.0,
        0.0,
        0.0,
        0.0,
        OscWave::Saw,
        0.5,
        0.0,
        0.5,
        0.5,
        0.5,
        0.5,
        0.5,
        0.0,
        VcfTaps::FourPole,
        0.0,
        false,
        3.0,
        200.0,
        0.0,
        200.0,
        3.0,
        200.0,
        0.0,
        200.0,
        VcaEnvMode::Envelope,
        40,
        0.0,
        CompTolerance::Ten,
        0,
        CircuitTopology::Accurate,
        0.0,
        1337,
        0.5,
        0.0,
        1.0, // recal = 1.0
    );

    v2.update_params(
        0.0,
        0.0,
        0.0,
        0.0,
        OscWave::Saw,
        0.5,
        0.0,
        0.5,
        0.5,
        0.5,
        0.5,
        0.5,
        0.0,
        VcfTaps::FourPole,
        0.0,
        false,
        3.0,
        200.0,
        0.0,
        200.0,
        3.0,
        200.0,
        0.0,
        200.0,
        VcaEnvMode::Envelope,
        40,
        0.0,
        CompTolerance::Ten,
        0,
        CircuitTopology::Accurate,
        0.0,
        1337,
        0.5,
        0.0,
        0.0, // recal = 0.0
    );

    v1.recalculate_luts_sync();
    v2.recalculate_luts_sync();

    assert_ne!(
        v1.get_vco_cv_lut(),
        v2.get_vco_cv_lut(),
        "LUTs should differ between recal=1.0 and recal=0.0"
    );

    // At 1% tolerance the wear detune can fall below the secant solver's 0.1%
    // early-return threshold at any given anchor, leaving that anchor identical
    // between the two LUTs (both collapse to the quadratic seed). Play the note
    // where the LUTs differ most so the comparison does not depend on the
    // tolerance draw landing above the threshold at an arbitrary note.
    let l1 = v1.get_vco_cv_lut();
    let l2 = v2.get_vco_cv_lut();
    let (best_i, _) =
        l1.iter()
            .zip(l2.iter())
            .enumerate()
            .fold((0usize, 0.0f64), |(bi, bd), (i, (a, b))| {
                let d = (a - b).abs();
                if d > bd {
                    (i, d)
                } else {
                    (bi, bd)
                }
            });
    let note = (best_i as u8) * 3; // LUT anchors span MIDI 0..108 in 3-semitone steps

    v1.note_on(note);
    v1.set_gate(true, true, false, false);
    v2.note_on(note);
    v2.set_gate(true, true, false, false);

    // Tick into steady-state oscillation (past initial rail ramp)
    for _ in 0..1000 {
        v1.tick();
        v2.tick();
    }

    // Render comparison block in steady state
    let mut v1_samples = Vec::new();
    let mut v2_samples = Vec::new();
    for _ in 0..200 {
        v1_samples.push(v1.tick());
        v2_samples.push(v2.tick());
    }

    assert_ne!(
        v1_samples, v2_samples,
        "Recal scale 1.0 vs 0.0 must produce different audio output"
    );
}

#[test]
fn test_worker_recal_scale_async() {
    let sr = 48000.0;
    let mut vco_recal = Vco303::new(sr);
    let mut vco_half = Vco303::new(sr);

    vco_recal.set_analog_wear(0.10, 40, 1337);
    vco_half.set_analog_wear(0.10, 40, 1337);

    vco_recal.recalculate_luts_sync_with_recal(40, 0.10, 1337, 1.0);

    assert!(vco_half.recalculate_luts_async(40, 0.10, 1337, 0.5));

    let mut updated = false;
    for _ in 0..100 {
        sleep(Duration::from_millis(10));
        vco_half.poll_lut_update();
        if vco_half.get_cv_lut() != [0.0; 37] && vco_half.get_cv_lut() != vco_recal.get_cv_lut() {
            updated = true;
            break;
        }
    }

    assert!(
        updated,
        "Async worker should finish and update LUT with recal_scale=0.5"
    );
    let lut = vco_half.get_cv_lut();
    for entry in lut.iter() {
        assert!(entry.is_finite(), "Async recal LUT entries must be finite");
    }
}
