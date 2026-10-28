mod common;

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
        0.0,
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

    // Voice-path test: bypass the stateful output clipper (its one-pole
    // states decay geometrically, so exact 0.0 asserts would fail on residue).
    v.update_clipper_params_immediate(common::bypassed_clipper());

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

    // Gate off -> output should return to 0.0 once the clipper's 48-sample
    // dry-delay buffer is flushed with silence.
    v.set_gate(false, false, false, false);
    for _ in 0..plugin303::dsp::warm_clipper::CLIPPER_LATENCY {
        v.tick();
    }
    assert_eq!(v.tick(), 0.0);
}

#[test]
fn test_accent_decay_fixed_200ms() {
    let sr = 48000.0;
    let mut v_acc = Voice::new(sr);
    let mut v_norm = Voice::new(sr);

    // Voice-path test: bypass the output clipper. Its JFET shunt compresses
    // the louder accented note more, masking the MEG decay contrast this
    // test guards.
    for v in [&mut v_acc, &mut v_norm] {
        v.update_clipper_params_immediate(common::bypassed_clipper());
    }

    // Accent knob set to maximum (1.0), Decay knob set to maximum (1.0)
    // Cutoff set to min (0.0), env_mod set to max (1.0) to highlight filter decay difference
    let update = |v: &mut Voice| {
        v.update_params(
            0.0,
            0.0,
            0.0,
            0.0,
            plugin303::params::OscWave::Saw,
            0.5,
            0.0,
            0.0, // min cutoff
            0.0, // min reso
            1.0, // max env_mod
            1.0, // max decay knob (3000ms for normal notes)
            1.0, // max accent knob
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
            0.4,
            0.0,
        );
    };

    update(&mut v_acc);
    update(&mut v_norm);

    // Accented note on v_acc, Normal note on v_norm
    v_acc.set_gate(true, true, true, false);
    v_acc.note_on(60);

    v_norm.set_gate(true, true, false, false);
    v_norm.note_on(60);

    // Tick both voices for ~500ms (24000 samples at 48kHz)
    // At 500ms:
    // - Accented MEG decay (fixed ~200ms) will be near 0 (exp(-500/200) = ~0.082)
    // - Normal MEG decay (knob set to 3000ms) will still be high (exp(-500/3000) = ~0.846)
    let mut energy_acc = 0.0f32;
    let mut energy_norm = 0.0f32;

    for i in 0..24000 {
        let s_acc = v_acc.tick();
        let s_norm = v_norm.tick();
        // Measure high-frequency derivative energy over the last 100ms (samples 19200..24000)
        if i >= 19200 {
            energy_acc += s_acc.abs();
            energy_norm += s_norm.abs();
        }
    }

    // High frequency / overall energy of accented note after 500ms should be noticeably lower
    // than normal note with 3000ms decay knob because filter closed down to ~200ms decay.
    assert!(
        energy_acc < energy_norm * 0.7,
        "Expected accented note energy ({}) after 500ms to be < 70% of normal 3000ms decay energy ({})",
        energy_acc,
        energy_norm
    );
}

#[test]
fn test_accent_sweep_resonance_coupling() {
    use plugin303::dsp::rc_env::AccentSweep;
    let mut sweep = AccentSweep::new(48000.0);

    // With zero resonance, drive should still produce non-zero sweep modulation
    let out_zero_reso = sweep.tick(1.0, 0.0);
    assert!(
        out_zero_reso > 0.0,
        "Zero resonance should still produce baseline accent sweep"
    );

    sweep.reset();
    // With maximum resonance, drive should produce stronger sweep modulation
    let out_max_reso = sweep.tick(1.0, 1.0);
    assert!(
        out_max_reso > out_zero_reso,
        "Max resonance should produce higher sweep output than zero resonance ({} vs {})",
        out_max_reso,
        out_zero_reso
    );
}

#[test]
fn test_accent_analog_memory_staircase() {
    use plugin303::dsp::rc_env::AccentSweep;
    let mut sweep = AccentSweep::new(48000.0);

    // Simulate 4 consecutive 16th-note accents at 120 BPM (125ms = 6000 samples per note)
    // 58.3% gate length = 3500 samples on, 2500 samples off
    let mut peaks = Vec::new();

    for _ in 0..4 {
        let mut step_peak = 0.0f64;
        // Gate on (MEG decaying)
        for i in 0..3500 {
            let meg = (-1.0f64 / (0.2 * 48000.0) * i as f64).exp();
            let val = sweep.tick(meg, 0.8);
            if val > step_peak {
                step_peak = val;
            }
        }
        // Gate off (MEG at 0, cap discharging slowly)
        for _ in 0..2500 {
            sweep.tick(0.0, 0.8);
        }
        peaks.push(step_peak);
    }

    // Successive peaks must rise (analog memory staircase)
    assert!(
        peaks[1] > peaks[0],
        "Step 2 peak should exceed Step 1 peak ({} vs {})",
        peaks[1],
        peaks[0]
    );
    assert!(
        peaks[2] > peaks[1],
        "Step 3 peak should exceed Step 2 peak ({} vs {})",
        peaks[2],
        peaks[1]
    );
}
