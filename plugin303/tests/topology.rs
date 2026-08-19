use plugin303::dsp::voice::Voice;
use plugin303::params::{CircuitTopology, OscWave, Plugin303Params, VcaEnvMode, VcfTaps};

#[test]
fn test_topology_param_defaults_and_blackouts() {
    let params = Plugin303Params::default();
    assert_eq!(params.topology.value(), CircuitTopology::Fun);

    // Fun mode: only the classic waveform selector is blacked out
    assert!(!params.is_param_active("waveform"));

    // Souped-up parameters should be active in Fun mode
    assert!(params.is_param_active("osc1_pitch"));
    assert!(params.is_param_active("osc2_pitch"));
    assert!(params.is_param_active("osc2_vol"));
    assert!(params.is_param_active("osc1_pw"));
    assert!(params.is_param_active("sub_vol"));
    assert!(params.is_param_active("sweep_pos"));
    assert!(params.is_param_active("vcf_taps"));
    assert!(params.is_param_active("vcf_drive"));
    assert!(params.is_param_active("advanced_env"));
    assert!(params.is_param_active("meg_attack"));
    assert!(params.is_param_active("veg_decay_adj"));
    assert!(params.is_param_active("vca_env_mode"));

    // Classic parameters should be active
    assert!(params.is_param_active("tuning"));
    assert!(params.is_param_active("cutoff"));
    assert!(params.is_param_active("resonance"));
    assert!(params.is_param_active("reso_comp"));
}

#[test]
fn test_accurate_topology_dsp_coercion() {
    let sr = 48_000.0;

    // Render a voice in Accurate mode with classic parameters
    let mut v_base = Voice::new(sr);
    v_base.update_params(
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
        0,
        0.0,
        plugin303::params::CompTolerance::Zero,
        0,
        CircuitTopology::Accurate,
        0.0,
        0,
        0.5,
        0.0,
    );

    v_base.note_on(60);
    v_base.set_gate(true, true, false, false);
    for _ in 0..120 {
        v_base.tick();
    }
    let base_sample = v_base.tick();

    // Try modifying advanced features in Accurate mode and verify they are ignored (output remains identical)
    let mut v_adv = Voice::new(sr);
    v_adv.update_params(
        0.0,
        5.0,
        -12.0,
        0.8,
        OscWave::Sweep,
        0.85,
        0.9, // Osc 2 vol/detune, Sweep wave, pw, sub_vol active
        0.5,
        0.5,
        0.5,
        0.5,
        0.5,
        0.0,
        VcfTaps::TwoPole,
        12.0, // VCF taps 2-pole, drive 12dB
        true,
        50.0,
        1000.0,
        0.5,
        500.0,
        50.0,
        1000.0,
        0.5,
        500.0, // Advanced envelopes enabled
        VcaEnvMode::Gate,
        0,
        0.0,
        plugin303::params::CompTolerance::Zero,
        0,
        CircuitTopology::Accurate,
        0.75,
        0,
        0.5,
        0.0,
    );

    v_adv.note_on(60);
    v_adv.set_gate(true, true, false, false);
    for _ in 0..120 {
        v_adv.tick();
    }
    let adv_sample = v_adv.tick();

    assert_eq!(
        base_sample, adv_sample,
        "Advanced parameter adjustments must be ignored in Accurate mode"
    );
}

#[test]
fn test_fun_topology_dsp_activation() {
    let sr = 48_000.0;

    // In Fun mode, setting Osc 2 volume and detune should modify the audio output
    let mut v_base = Voice::new(sr);
    v_base.update_params(
        0.0,
        0.0,
        0.0,
        0.0,
        OscWave::Sweep,
        0.5,
        0.0,
        0.5,
        0.0,
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
        0,
        0.0,
        plugin303::params::CompTolerance::Zero,
        0,
        CircuitTopology::Fun,
        0.0,
        0,
        0.5,
        0.0,
    );
    v_base.note_on(60);
    v_base.set_gate(true, true, false, false);

    let mut v_fun = Voice::new(sr);
    v_fun.update_params(
        0.0,
        0.0,
        5.0,
        0.5,
        OscWave::Sweep,
        0.5,
        0.0, // Osc 2 detuned and active at 50% volume
        0.5,
        0.0,
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
        0,
        0.0,
        plugin303::params::CompTolerance::Zero,
        0,
        CircuitTopology::Fun,
        0.0,
        0,
        0.5,
        0.0,
    );
    v_fun.note_on(60);
    v_fun.set_gate(true, true, false, false);

    for _ in 0..120 {
        v_base.tick();
        v_fun.tick();
    }
    let base_sample = v_base.tick();
    let fun_sample = v_fun.tick();

    assert_ne!(
        base_sample, fun_sample,
        "Adjusting advanced features in Fun mode must change the output"
    );
}
