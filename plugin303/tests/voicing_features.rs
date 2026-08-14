use plugin303::dsp::voice::Voice;

#[test]
fn test_accent_stacking() {
    let sr = 48000.0;
    let mut v = Voice::new(sr);
    v.update_params(
        0.0,
        0.0,
        0.0,
        0.0,
        plugin303::params::OscWave::Saw,
        0.5,
        0.0,
        0.5,
        0.5,
        0.5,
        0.5,
        1.0,
        0.0,
        plugin303::params::VcfTaps::FourPole,
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
        plugin303::params::VcaEnvMode::Envelope,
        0,
        0.0,
        plugin303::params::CompTolerance::Zero,
        0,
        plugin303::params::CircuitTopology::Accurate,
        0.0,
        0,
        0.5,
    );
    // Trigger consecutive accents
    v.set_gate(true, true, true, false);
    let initial_accent = v.get_accent_acc();
    assert!(initial_accent > 0.9, "Accent did not charge");

    // Wait a few samples
    for _ in 0..500 {
        v.tick();
    }
    let decayed_accent = v.get_accent_acc();
    assert!(decayed_accent < initial_accent, "Accent did not decay");

    // Trigger again to stack
    v.set_gate(true, true, true, false);
    let stacked_accent = v.get_accent_acc();
    assert!(
        stacked_accent > initial_accent,
        "Accent did not stack: {} vs {}",
        stacked_accent,
        initial_accent
    );
}

#[test]
fn test_pitch_glide_slew() {
    let sr = 48000.0;
    let mut v = Voice::new(sr);
    // Set glide knob to 0.5 (normal) and slide active
    v.set_gate(true, true, false, true);
    v.note_on(60); // C4 (60.0)

    // Change note to C5 (72.0)
    v.note_on(72);

    // Slew has not processed yet
    assert_eq!(
        v.get_current_pitch(),
        60.0,
        "Pitch jumped instantly before ticking"
    );

    // Process one sample to start slewing
    v.tick();

    // Verify pitch is slewing but hasn't jumped instantly to 72
    let pitch_immediately = v.get_current_pitch();
    assert!(pitch_immediately > 60.0, "Pitch did not start slewing");
    assert!(pitch_immediately < 71.0, "Pitch jumped too fast");

    // Let it tick and verify it approaches 72
    for _ in 0..1000 {
        v.tick();
    }
    let pitch_later = v.get_current_pitch();
    assert!(
        pitch_later > pitch_immediately,
        "Pitch did not rise over time"
    );
}

#[test]
fn test_midi_legato_glide() {
    let sr = 48000.0;
    let mut v = Voice::new(sr);

    // Play note 60 (C4) without slide
    v.set_gate(true, true, false, false);
    v.note_on(60);
    v.tick();
    assert_eq!(v.get_current_pitch(), 60.0);

    // Play note 72 (C5) legato (slide = true, trigger = false)
    v.set_gate(true, false, false, true);
    v.note_on(72);

    // Slew has not processed yet
    assert_eq!(v.get_current_pitch(), 60.0);

    // Process one sample to start slewing
    v.tick();

    // Verify pitch is slewing
    let pitch_immediately = v.get_current_pitch();
    assert!(pitch_immediately > 60.0, "Pitch did not start slewing");
    assert!(pitch_immediately < 71.0, "Pitch jumped too fast");
}

#[test]
fn test_vca_gate_mode() {
    let sr = 48000.0;
    let mut v = Voice::new(sr);

    // In Gate mode:
    v.set_vca_env_mode(plugin303::params::VcaEnvMode::Gate);

    // Gate off -> vca_level is 0, so tick should output 0.0
    v.set_gate(false, false, false, false);
    for _ in 0..10 {
        assert_eq!(v.tick(), 0.0);
    }

    // Gate on -> output should be non-zero after a few samples (PDC delay is 48 samples)
    v.set_gate(true, true, false, false);
    v.note_on(60);

    let mut non_zero = false;
    for _ in 0..100 {
        if v.tick() != 0.0 {
            non_zero = true;
            break;
        }
    }
    assert!(
        non_zero,
        "Expected non-zero output in Gate mode when gate is active"
    );

    // Gate off -> output should immediately return to 0.0
    v.set_gate(false, false, false, false);
    assert_eq!(v.tick(), 0.0);
}
