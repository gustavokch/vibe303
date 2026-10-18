//! Crossfade state isolation between quality tiers.
//!
//! During a tier crossfade `Voice` renders the outgoing and the incoming tier
//! in the same output sample. Each tier must own its filter, oversampler and
//! compensation state, and each oscillator must advance once per sample, or
//! one path corrupts the other.

mod common;

use plugin303::dsp::filter::Tier;
use plugin303::dsp::voice::Voice;
use plugin303::params::{CircuitTopology, CompTolerance, OscWave, VcaEnvMode, VcfTaps};

const SR: f32 = 48_000.0;
/// Sample index at which the tier switch is requested.
const SWITCH_AT: usize = 2_400;
/// The fade is 10 ms (480 samples at 48 kHz). 600 leaves margin for the float
/// accumulation of the fade position.
const FADE_END: usize = SWITCH_AT + 600;

/// A voice with a constant gate level (no amplitude envelope), the output
/// clipper bypassed (pure 48-sample delay), osc2 off, and the given sub level
/// and resonance.
fn voice(start: Tier, sub_vol: f32, resonance: f32) -> Voice {
    voice_on(start, sub_vol, resonance, CircuitTopology::Accurate)
}

/// [`voice`] on the given topology. `Accurate` forces the sub to 0, so the sub
/// is audible only on `Fun`.
fn voice_on(start: Tier, sub_vol: f32, resonance: f32, topology: CircuitTopology) -> Voice {
    let mut v = Voice::new(SR);
    v.update_clipper_params_immediate(common::bypassed_clipper());
    v.update_params(
        0.0,                       // tuning_cents
        0.0,                       // osc1_pitch_st
        0.0,                       // osc2_pitch_st
        0.0,                       // osc2_vol
        OscWave::Saw,              // osc1_wave
        0.5,                       // osc1_pw
        sub_vol,                   // sub_vol
        0.5,                       // cutoff (normalised)
        resonance,                 // resonance
        0.5,                       // env_mod
        0.5,                       // decay_knob
        0.0,                       // accent_knob
        0.0,                       // volume_db
        VcfTaps::FourPole,         // vcf_taps
        0.0,                       // vcf_drive_db
        false,                     // advanced_env
        3.0,                       // meg_attack
        200.0,                     // meg_decay_adj
        0.0,                       // meg_sustain
        200.0,                     // meg_release
        3.0,                       // veg_attack
        200.0,                     // veg_decay_adj
        0.0,                       // veg_sustain
        200.0,                     // veg_release
        VcaEnvMode::Gate,          // vca_env_mode
        0,                         // comp_age
        0.0,                       // comp_drift
        CompTolerance::Zero,       // tolerance
        0,                         // thermal_noise
        topology,                  // topology
        0.0,                       // sweep_pos
        0,                         // comp_seed
        0.5,                       // glide_knob
        0.0,                       // reso_comp
        1.0,                       // recalibration_scale
    );
    v.set_tier_immediate(start);
    v.note_on(45);
    v
}

fn render(v: &mut Voice, n: usize) -> Vec<f32> {
    (0..n).map(|_| v.tick()).collect()
}

/// Renders `SWITCH_AT` samples in `from`, requests a faded switch to `to`, and
/// renders `total - SWITCH_AT` more samples.
fn render_switch(from: Tier, to: Tier, sub_vol: f32, resonance: f32, total: usize) -> Vec<f32> {
    let mut v = voice(from, sub_vol, resonance);
    let mut out = render(&mut v, SWITCH_AT);
    v.set_tier(to);
    out.extend(render(&mut v, total - SWITCH_AT));
    out
}

/// Once the fade ends only the incoming path is audible. With exclusive
/// per-tier state, the incoming path cannot depend on which tier it faded from
/// as long as neither outgoing tier touches the incoming tier's sources:
/// Draft and Low never tick the circuit VCO, Draft and High never tick the
/// Low polyBLEP oscillators, and `sub_vol = 0` removes the shared sub from the
/// mix. The post-fade output must therefore be bit-identical.
///
/// Fails when Low and High share one 2x oversampler: the outgoing tier's
/// signal enters the incoming tier's FIR history and, through the nodal
/// ladder's memory, persists after the fade.
#[test]
fn incoming_tier_does_not_depend_on_outgoing_tier() {
    let total = FADE_END + 64 + 4_000;
    for (to, from_a, from_b) in [
        (Tier::High, Tier::Low, Tier::Draft),
        (Tier::Low, Tier::High, Tier::Draft),
    ] {
        let a = render_switch(from_a, to, 0.0, 0.3, total);
        let b = render_switch(from_b, to, 0.0, 0.3, total);
        // +64: past the bypassed clipper's 48-sample delay line.
        let start = FADE_END + 64;
        let first_diff = (start..total).find(|&i| a[i].to_bits() != b[i].to_bits());
        assert!(
            first_diff.is_none(),
            "{to:?} after fading from {from_a:?} differs from {to:?} after fading from \
             {from_b:?} at sample {} ({} vs {})",
            first_diff.unwrap(),
            a[first_diff.unwrap()],
            b[first_diff.unwrap()],
        );
    }
}

/// High and Best share the circuit VCO and its sub (`sub_osc_circuit`). A fade
/// between them must advance the shared VCO exactly once per sample, so after
/// the fade the voice is in phase with a voice that ran the incoming tier all
/// along.
///
/// The voices use `CircuitTopology::Accurate`, which forces the sub to 0. The
/// sub is therefore muted here: this test guards the circuit VCO only. Sub
/// advance is guarded by `subs_advance_once_per_sample_in_low_high_fades`,
/// which runs on `Fun`.
///
/// Resonance is 0 for both voices. A resonant nodal ladder keeps cold-start
/// state for about a second (τ ≈ 0.7 s, measured at resonance 0.3), so a
/// fade into a cold resonant ladder never matches a warm reference within a
/// short window. Only a non-resonant ladder isolates the oscillator-advance
/// invariant. The comparison window starts 100 ms after the fade.
///
/// Fails when the shared VCO is ticked once per rendered tier: during the
/// 480-sample fade it runs at double speed and ends 480 samples ahead, a phase
/// jump of 44 samples at note 45 (110 Hz, 436-sample period).
#[test]
fn shared_sources_advance_once_per_sample_in_high_best_fades() {
    let window = FADE_END + 4_800..FADE_END + 9_600;
    for (from, to) in [(Tier::High, Tier::Best), (Tier::Best, Tier::High)] {
        let switched = render_switch(from, to, 0.5, 0.0, window.end);
        let mut reference_voice = voice(to, 0.5, 0.0);
        let reference = render(&mut reference_voice, window.end);

        let (mut err, mut energy) = (0.0f64, 0.0f64);
        for i in window.clone() {
            let d = (switched[i] - reference[i]) as f64;
            err += d * d;
            energy += (reference[i] as f64).powi(2);
        }
        let rel = (err / energy).sqrt();
        assert!(
            rel < 1e-3,
            "{from:?} -> {to:?}: post-fade output deviates from a {to:?}-only voice by \
             relative RMS {rel:.3e} (limit 1e-3)"
        );
    }
}

/// Low ticks `sub_osc_low` and High ticks `sub_osc_circuit`; during a fade
/// between them each sub must still advance exactly once per sample. Resonance
/// is 0 for the same reason as the High↔Best test above.
///
/// Runs on the `Fun` topology: `Accurate` forces the sub to 0, so the sub is
/// inaudible there.
///
/// The reference is not a target-only voice: the target's own oscillator (the
/// circuit VCO for High, `osc_low` for Low) is idle while the outgoing tier
/// plays, so a target-only voice would be out of phase. The reference instead
/// switches hard at the same sample, so every oscillator keeps the same
/// timeline and only the fade differs.
///
/// Fails when a sub is ticked once per rendered tier (as when Low and High
/// shared one sub): during the 480-sample fade it runs at double speed and
/// ends a fade's length ahead.
#[test]
fn subs_advance_once_per_sample_in_low_high_fades() {
    let window = FADE_END + 4_800..FADE_END + 9_600;
    for (from, to) in [(Tier::Low, Tier::High), (Tier::High, Tier::Low)] {
        let mut v = voice_on(from, 0.5, 0.0, CircuitTopology::Fun);
        let mut switched = render(&mut v, SWITCH_AT);
        v.set_tier(to);
        switched.extend(render(&mut v, window.end - SWITCH_AT));
        let mut reference_voice = voice_on(from, 0.5, 0.0, CircuitTopology::Fun);
        let mut reference = render(&mut reference_voice, SWITCH_AT);
        reference_voice.set_tier_immediate(to);
        reference.extend(render(&mut reference_voice, window.end - SWITCH_AT));

        let (mut err, mut energy) = (0.0f64, 0.0f64);
        for i in window.clone() {
            let d = (switched[i] - reference[i]) as f64;
            err += d * d;
            energy += (reference[i] as f64).powi(2);
        }
        let rel = (err / energy).sqrt();
        assert!(
            rel < 1e-3,
            "{from:?} -> {to:?}: post-fade output deviates from a hard-switched reference by \
             relative RMS {rel:.3e} (limit 1e-3)"
        );
    }
}

/// Returning to a tier must not resume the filter state the tier held when it
/// last ran. Plays note 33 on `to`, fades to `from`, changes to note 52, and
/// fades back to `to` 0.5 s later. The reference voice stays on `to` with the
/// same notes; High and Best share the circuit VCO, so both are in phase.
///
/// With the stale note-33 state the resonant ladder deviates by relative RMS
/// about 0.67 at 100-200 ms after the fade; a cold start gives about 0.09.
#[test]
fn returning_tier_starts_cold_not_stale() {
    let back = 48_000;
    let window = back + 600 + 4_800..back + 600 + 9_600;
    for (to, away) in [(Tier::High, Tier::Best), (Tier::Best, Tier::High)] {
        let play = |switch: bool| {
            let mut v = voice_on(to, 0.5, 0.3, CircuitTopology::Fun);
            v.note_on(33);
            let mut out = render(&mut v, SWITCH_AT);
            if switch {
                v.set_tier(away);
            }
            out.extend(render(&mut v, 24_000 - SWITCH_AT));
            v.note_on(52);
            out.extend(render(&mut v, back - 24_000));
            if switch {
                v.set_tier(to);
            }
            out.extend(render(&mut v, window.end - back));
            out
        };
        let (switched, reference) = (play(true), play(false));

        let (mut err, mut energy) = (0.0f64, 0.0f64);
        for i in window.clone() {
            let d = (switched[i] - reference[i]) as f64;
            err += d * d;
            energy += (reference[i] as f64).powi(2);
        }
        let rel = (err / energy).sqrt();
        assert!(
            rel < 0.2,
            "{to:?} after a round trip via {away:?}: deviates from a {to:?}-only voice by \
             relative RMS {rel:.3e} (limit 0.2)"
        );
    }
}
