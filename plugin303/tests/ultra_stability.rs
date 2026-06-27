//! Ultra numerical-stability guard (F3.6): the nodal VCO→VCF Newton chain must
//! never emit a NaN/inf or an unbounded sample, even under a full cutoff +
//! resonance sweep (the 303's accent/decay modulates the corner hard, which is
//! where a stiff per-sample solver is most likely to diverge).

use plugin303::dsp::filter::Tier;
use plugin303::dsp::voice::Voice;

#[test]
fn ultra_voice_is_finite_and_bounded_under_sweep() {
    let sr = 48_000.0;
    let mut v = Voice::new(sr);
    v.set_tier_immediate(Tier::Ultra);
    v.set_reso(0.85);
    v.note_on(45); // low note → many VCO periods over the render

    let n = (sr * 1.0) as usize; // 1 s
    let mut peak = 0.0f32;
    for i in 0..n {
        // Sweep cutoff 200 Hz → 5 kHz → 200 Hz across the render.
        let phase = (i as f32 / n as f32 * std::f32::consts::PI).sin();
        v.set_cutoff(200.0 + 4_800.0 * phase);
        let y = v.tick();
        assert!(y.is_finite(), "non-finite Ultra sample at i={i}: {y}");
        peak = peak.max(y.abs());
    }
    // The voice is enveloped (AD decay) and RMS-scaled internally; a loose bound
    // catches runaway Newton without over-constraining the legitimate level.
    assert!(peak < 50.0, "Ultra output unbounded under sweep: peak {peak}");
}
