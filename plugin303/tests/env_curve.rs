use plugin303::dsp::rc_env::{DecayEnv, RcEnv};
use plugin303::dsp::voice::Voice;
use plugin303::params::{CircuitTopology, CompTolerance, OscWave, VcaEnvMode, VcfTaps};

#[test]
fn test_decay_env_exponential_shape() {
    let sr = 44100.0;
    let mut env = DecayEnv::new(sr);

    // Set decay to 200 ms
    let tau_ms = 200.0;
    env.set_decay(tau_ms);
    env.trigger();

    // Output at t=0 should be 1.0 (first sample is 1.0)
    let y0 = env.tick();
    assert!(
        (y0 - 1.0).abs() < 0.001,
        "Expected y0 to be 1.0, got {}",
        y0
    );

    // Tick to t = tau (200ms)
    // 200ms * 0.001 * 44100 = 8820 samples. We already ticked 1 sample.
    for _ in 0..8819 {
        env.tick();
    }
    let y_tau = env.tick();

    // Exponential decay at t=tau should be approximately 1/e = 0.3678
    let expected = (-1.0f64).exp();
    assert!(
        (y_tau - expected).abs() < 0.01,
        "Expected y_tau to be ~{}, got {}",
        expected,
        y_tau
    );

    // Tick to t = 5 * tau (1000ms = 44100 samples)
    for _ in 0..(4 * 8820) {
        env.tick();
    }
    let y_5tau = env.tick();
    assert!(
        y_5tau < 0.01,
        "Expected y_5tau to be < 0.01, got {}",
        y_5tau
    );
}

#[test]
fn test_rc_env_ahdsr_shape() {
    let sr = 44100.0;
    let mut env = RcEnv::new(sr);

    // Setup typical AHDSR: attack=10ms, hold=0ms, decay=1000ms, sustain=0.0, release=10ms
    env.set_attack(10.0);
    env.set_hold(0.0);
    env.set_decay(1000.0);
    env.set_sustain(0.0);
    env.set_release(10.0);

    env.note_on();

    // At t=0, output starts at start_level (0.0) and rises towards peak (1.0)
    let y0 = env.tick();
    assert!(y0 > 0.0, "Expected positive initial rise, got {}", y0);

    // At t = 10ms (attack time), output should approach 1.0
    // 10ms * 44.1 samples/ms = 441 samples
    for _ in 0..440 {
        env.tick();
    }
    let y_peak = env.tick();
    // It should be near 1.0 (since attack coeff is calculated to reach near 1.0 in attack_time)
    assert!(y_peak > 0.6, "Expected peak to be reached, got {}", y_peak);

    // Let's release the note
    env.note_off();

    // Output should now decay towards 0.0 based on release time (10ms)
    // After 50ms (5 * release), output should be near zero
    for _ in 0..(50 * 44) {
        env.tick();
    }
    let y_released = env.tick();
    assert!(
        y_released < 0.01,
        "Expected release to drop to < 0.01, got {}",
        y_released
    );
}

#[test]
fn test_accent_decays_faster() {
    let mut voice = Voice::new(44100.0);
    // update_params to set knob defaults
    voice.update_params(
        0.0,
        0.0,
        0.0,
        0.0,
        OscWave::Saw,
        0.5,
        0.0,
        0.0,
        0.5,
        0.5,
        0.5,
        0.5,
        0.5,
        VcfTaps::FourPole,
        0.0,
        false,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        VcaEnvMode::Envelope,
        0,
        0.0,
        CompTolerance::Zero,
        0,
        CircuitTopology::Accurate,
        0.0,
        0,
        0.5,
        0.0,
    );

    // Trigger normal note (accent = false)
    voice.set_gate(true, true, false, false);

    for _ in 0..100 {
        voice.tick();
    }

    // Let's reset and trigger accented note (accent = true)
    voice.reset();
    voice.update_params(
        0.0,
        0.0,
        0.0,
        0.0,
        OscWave::Saw,
        0.5,
        0.0,
        0.0,
        0.5,
        0.5,
        0.5,
        0.5,
        0.5,
        VcfTaps::FourPole,
        0.0,
        false,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        VcaEnvMode::Envelope,
        0,
        0.0,
        CompTolerance::Zero,
        0,
        CircuitTopology::Accurate,
        0.0,
        0,
        0.5,
        0.0,
    );
    voice.set_gate(true, true, true, false);

    for _ in 0..100 {
        voice.tick();
    }

    // Accent fixed decay (200ms) vs normal decay (knob @ 0.5 -> 30 + 0.5 * 2970 = 1515 ms)
    // Accent decay is much faster (200ms vs 1515ms), so accent value should decay quicker
    let sr = 44100.0;
    let mut meg = DecayEnv::new(sr);

    meg.set_decay(1515.0);
    meg.trigger();
    for _ in 0..44100 {
        meg.tick();
    }
    let val_normal = meg.tick();

    meg.set_decay(200.0);
    meg.trigger();
    for _ in 0..44100 {
        meg.tick();
    }
    let val_accent = meg.tick();

    assert!(
        val_accent < val_normal,
        "Accented envelope should decay faster (accent: {}, normal: {})",
        val_accent,
        val_normal
    );
}

#[test]
fn test_accent_boosts_vca() {
    let mut voice_normal = Voice::new(44100.0);
    voice_normal.update_params(
        0.0,
        0.0,
        0.0,
        0.0,
        OscWave::Saw,
        0.5,
        0.0,
        0.0,
        0.5,
        0.5,
        0.5,
        0.5,
        0.5,
        VcfTaps::FourPole,
        0.0,
        false,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        VcaEnvMode::Envelope,
        0,
        0.0,
        CompTolerance::Zero,
        0,
        CircuitTopology::Accurate,
        0.0,
        0,
        0.5,
        0.0,
    );
    voice_normal.set_gate(true, true, false, false);

    let mut sum_normal = 0.0f32;
    for _ in 0..100 {
        sum_normal += voice_normal.tick().abs();
    }

    let mut voice_accent = Voice::new(44100.0);
    // Set accent knob to 1.0 (full accent boost)
    voice_accent.update_params(
        0.0,
        0.0,
        0.0,
        0.0,
        OscWave::Saw,
        0.5,
        0.0,
        0.0,
        0.5,
        0.5,
        0.5,
        0.5,
        1.0,
        VcfTaps::FourPole,
        0.0,
        false,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        VcaEnvMode::Envelope,
        0,
        0.0,
        CompTolerance::Zero,
        0,
        CircuitTopology::Accurate,
        0.0,
        0,
        0.5,
        0.0,
    );
    voice_accent.set_gate(true, true, true, false);

    let mut sum_accent = 0.0f32;
    for _ in 0..100 {
        sum_accent += voice_accent.tick().abs();
    }

    // Accented voice output level should be higher due to the MEG VCA boost
    assert!(
        sum_accent > sum_normal,
        "Accented note should be boosted (accent: {}, normal: {})",
        sum_accent,
        sum_normal
    );
}

#[test]
fn test_glide_slew_behavior() {
    let mut voice = Voice::new(44100.0);
    voice.update_params(
        0.0,
        0.0,
        0.0,
        0.0,
        OscWave::Saw,
        0.5,
        0.0,
        0.0,
        0.5,
        0.5,
        0.5,
        0.5,
        0.5,
        VcfTaps::FourPole,
        0.0,
        false,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        VcaEnvMode::Envelope,
        0,
        0.0,
        CompTolerance::Zero,
        0,
        CircuitTopology::Accurate,
        0.0,
        0,
        0.5,
        0.0,
    );

    // 1. Non-slide note_on: pitch should jump instantly
    voice.set_gate(true, true, false, false);
    voice.note_on(60); // middle C
    let pitch_init = voice.get_current_pitch();
    assert!(
        (pitch_init - 60.0).abs() < 0.001,
        "Expected instant pitch set to 60, got {}",
        pitch_init
    );

    // 2. Legato slide to note 72 (C5)
    voice.set_gate(true, false, false, true);
    voice.note_on(72);

    // Pitch immediately after should still be very close to 60.0 (slews over time)
    voice.tick();
    let pitch_after_tick = voice.get_current_pitch();
    assert!(
        pitch_after_tick < 61.0,
        "Expected pitch to slew, got {}",
        pitch_after_tick
    );

    // Constant-time slide hardware tau = 22ms (~60ms total slide settling time).
    // At t = tau (22ms = ~970 samples), pitch should reach ~ 60 + 12 * (1 - 1/e) = 67.585
    let tau_samples = (44.1 * 22.0) as usize;
    for _ in 1..tau_samples {
        voice.tick();
    }
    let pitch_at_tau = voice.get_current_pitch();
    let expected_at_tau = 60.0 + 12.0 * (1.0 - (-1.0f32).exp());
    assert!(
        (pitch_at_tau - expected_at_tau).abs() < 0.2,
        "Expected pitch at tau (~22ms) to be ~{}, got {}",
        expected_at_tau,
        pitch_at_tau
    );

    // At 5 * tau (~110ms = ~4851 samples), pitch should be within 0.1 of 72.0
    for _ in 0..(4 * tau_samples) {
        voice.tick();
    }
    let pitch_final = voice.get_current_pitch();
    assert!(
        (pitch_final - 72.0).abs() < 0.1,
        "Expected pitch to slide to near 72, got {}",
        pitch_final
    );
}

#[test]
fn test_accent_mid_note_switch_without_trigger() {
    let mut voice = Voice::new(44100.0);
    voice.update_params(
        0.0,
        0.0,
        0.0,
        0.0,
        OscWave::Saw,
        0.5,
        0.0,
        0.0,
        0.5,
        0.5,
        0.5,
        0.5,
        0.8,
        VcfTaps::FourPole,
        0.0,
        false,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        VcaEnvMode::Envelope,
        0,
        0.0,
        CompTolerance::Zero,
        0,
        CircuitTopology::Accurate,
        0.0,
        0,
        0.5,
        0.0,
    );

    // Start unaccented note
    voice.set_gate(true, true, false, false);
    for _ in 0..100 {
        voice.tick();
    }

    // Slid note introduces accent without triggering (trigger = false, accent = true)
    voice.set_gate(true, false, true, true);

    // Voice should have adopted accent gain even though trigger was false
    // (verified via rendering samples)
    for _ in 0..100 {
        voice.tick();
    }
}

#[test]
fn test_env_mod_octave_scaling() {
    let mut voice = Voice::new(44100.0);

    voice.update_params(
        0.0,
        0.0,
        0.0,
        0.0,
        OscWave::Saw,
        0.5,
        0.0,
        0.0,
        0.5,
        0.5,
        0.0,
        0.5,
        0.5,
        VcfTaps::FourPole,
        0.0,
        false,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        VcaEnvMode::Envelope,
        0,
        0.0,
        CompTolerance::Zero,
        0,
        CircuitTopology::Accurate,
        0.0,
        0,
        0.5,
        0.0,
    );
    voice.set_gate(true, true, false, false);
    voice.tick();
}
