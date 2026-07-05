use plugin303::dsp::voice::Voice;
use plugin303::params::CircuitTopology;

#[test]
fn component_drift_produces_fluctuations() {
    let sr = 48_000.0;
    let mut v = Voice::new(sr);
    
    // Set parameters
    v.update_params(
        0.0, 0.0, 0.0, 0.0, plugin303::params::OscWave::Saw, 0.5, 0.0,
        0.5, 0.5, 0.5, 0.5, 0.5, 0.0, plugin303::params::VcfTaps::FourPole, 0.0,
        false, 3.0, 200.0, 0.0, 200.0, 3.0, 200.0, 0.0, 200.0,
        plugin303::params::VcaEnvMode::Envelope,
        0, // age
        0.5, // comp_drift
        plugin303::params::CompTolerance::Ten, // tolerance (for drift)
        5, // thermal_noise (for drift)
        CircuitTopology::Accurate,
        0.0, // sweep_pos
    );

    v.note_on(60);
    v.set_gate(true, true, false);
    
    // Check that ticking multiple times shifts the internal drift state
    let mut last_tick = v.tick();
    let mut distinct_values = false;
    for _ in 0..100 {
        let val = v.tick();
        if val != last_tick {
            distinct_values = true;
        }
        last_tick = val;
    }
    assert!(distinct_values, "drift should create evolving waveforms");
}

#[test]
fn component_age_affects_tuning_and_cutoff() {
    let sr = 48_000.0;
    let mut v_new = Voice::new(sr);
    let mut v_old = Voice::new(sr);

    // Factory calibration (age = 0.0)
    v_new.update_params(
        0.0, 0.0, 0.0, 0.0, plugin303::params::OscWave::Saw, 0.5, 0.0,
        0.5, 0.5, 0.5, 0.5, 0.5, 0.0, plugin303::params::VcfTaps::FourPole, 0.0,
        false, 3.0, 200.0, 0.0, 200.0, 3.0, 200.0, 0.0, 200.0,
        plugin303::params::VcaEnvMode::Envelope,
        0, // age = 0
        0.0, // comp_drift = 0.0
        plugin303::params::CompTolerance::Zero,
        0, // thermal noise
        CircuitTopology::Accurate,
        0.0, // sweep_pos
    );

    // Aged (age = 1.0)
    v_old.update_params(
        0.0, 0.0, 0.0, 0.0, plugin303::params::OscWave::Saw, 0.5, 0.0,
        0.5, 0.5, 0.5, 0.5, 0.5, 0.0, plugin303::params::VcfTaps::FourPole, 0.0,
        false, 3.0, 200.0, 0.0, 200.0, 3.0, 200.0, 0.0, 200.0,
        plugin303::params::VcaEnvMode::Envelope,
        10, // age = 10
        0.0, // comp_drift = 0.0
        plugin303::params::CompTolerance::Zero,
        0, // thermal noise
        CircuitTopology::Accurate,
        0.0, // sweep_pos
    );

    v_new.note_on(60);
    v_new.set_gate(true, true, false);
    v_old.note_on(60);
    v_old.set_gate(true, true, false);

    // Verify they output different waveforms because age modifies cutoff and tuning.
    // We tick 50 times to pass the latency compensation delay line.
    for _ in 0..50 {
        v_new.tick();
        v_old.tick();
    }
    let sample_new = v_new.tick();
    let sample_old = v_old.tick();
    assert_ne!(sample_new, sample_old, "Aged voice should differ from new voice");
}

#[test]
fn test_instance_specific_tolerance_seeds() {
    use plugin303::dsp::vco::Vco303;
    
    let mut vco1 = Vco303::new(48000.0);
    let mut vco2 = Vco303::new(48000.0);
    vco1.set_instance_id(1);
    vco2.set_instance_id(2);
    vco1.set_analog_wear(0.05, 0); // 5% tolerance
    vco2.set_analog_wear(0.05, 0); // 5% tolerance
    
    // Verify that capacitor multipliers differ due to separate seeds
    assert_ne!(vco1.get_c_mults(), vco2.get_c_mults());
    assert_ne!(vco1.get_r_mults(), vco2.get_r_mults());
}
