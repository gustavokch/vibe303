//! Tests for VCF resonance gain compensation.

use plugin303::dsp::filter::Tier;
mod common;

use plugin303::dsp::voice::Voice;
use plugin303::params::{CircuitTopology, CompTolerance, OscWave, VcaEnvMode, VcfTaps};

fn render_voice_rms(voice: &mut Voice, samples: usize) -> f32 {
    let mut sum_sq = 0.0f64;
    // Discard initial transient
    for _ in 0..1000 {
        voice.tick();
    }
    for _ in 0..samples {
        let s = voice.tick() as f64;
        sum_sq += s * s;
    }
    (sum_sq / samples as f64).sqrt() as f32
}

#[test]
fn test_reso_gain_comp_scaling() {
    let sr = 48000.0;
    let mut voice = Voice::new(sr);
    voice.update_clipper_params_immediate(common::bypassed_clipper());
    voice.set_tier(Tier::Low);
    voice.set_topology(CircuitTopology::Fun);

    // Setup base note: Note 36 (C2, ~65 Hz), high cutoff (1.0), gate on
    voice.update_params(
        0.0,
        0.0,
        0.0,
        0.0,
        OscWave::Saw,
        0.5,
        0.0,
        0.8,
        0.8, // high resonance
        0.0,
        1.0,
        0.0,
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
        VcaEnvMode::Gate,
        0,
        0.0,
        CompTolerance::Zero,
        0,
        CircuitTopology::Fun,
        0.0,
        0,
        0.5,
        0.0, // reso_comp = 0.0
    );
    voice.note_on(36);

    let rms_uncompensated = render_voice_rms(&mut voice, 4000);

    // Now set reso_comp = 1.0 (full compensation)
    voice.set_reso_comp(1.0);
    let rms_compensated = render_voice_rms(&mut voice, 4000);

    // With reso=0.8, effective k = 4 * 0.8 = 3.2.
    // Inverse compensation factor is 1.0 + 1.0 * 3.2 = 4.2 (+12.5 dB).
    // Compensated RMS must be significantly higher than uncompensated RMS.
    assert!(
        rms_compensated > rms_uncompensated * 2.5,
        "Compensated RMS ({}) should be at least 2.5x uncompensated ({})",
        rms_compensated,
        rms_uncompensated
    );

    // Partial compensation (reso_comp = 0.5)
    voice.set_reso_comp(0.5);
    let rms_half = render_voice_rms(&mut voice, 4000);
    assert!(
        rms_half > rms_uncompensated && rms_half < rms_compensated,
        "Partial compensation should be between uncompensated and full: {} < {} < {}",
        rms_uncompensated,
        rms_half,
        rms_compensated
    );
}

#[test]
fn test_reso_gain_comp_zero_at_zero_reso() {
    let sr = 48000.0;
    let mut voice = Voice::new(sr);
    voice.set_tier(Tier::Low);
    voice.set_topology(CircuitTopology::Fun);

    // When resonance is 0.0, reso_comp should have no effect (factor = 1.0 + comp * 0 = 1.0)
    voice.update_params(
        0.0,
        0.0,
        0.0,
        0.0,
        OscWave::Saw,
        0.5,
        0.0,
        0.8,
        0.0, // zero resonance
        0.0,
        1.0,
        0.0,
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
        VcaEnvMode::Gate,
        0,
        0.0,
        CompTolerance::Zero,
        0,
        CircuitTopology::Fun,
        0.0,
        0,
        0.5,
        0.0,
    );
    voice.note_on(36);
    let rms_zero_comp = render_voice_rms(&mut voice, 4000);

    voice.set_reso_comp(1.0);
    let rms_full_comp = render_voice_rms(&mut voice, 4000);

    let diff = (rms_full_comp - rms_zero_comp).abs();
    assert!(
        diff < 0.05 * rms_zero_comp,
        "At reso=0, reso_comp should have negligible effect: diff = {}",
        diff
    );
}
