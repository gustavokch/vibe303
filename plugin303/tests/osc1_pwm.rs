//! OSC1 PWM: a triangle LFO sweeps oscillator 1's pulse width around the PW
//! knob on every tier, in FUN only, and oscillator 2 keeps the static PW.

mod common;

use std::f64::consts::PI;

use plugin303::dsp::filter::Tier;
use plugin303::dsp::vco::Vco303;
use plugin303::dsp::voice::{midi_to_freq, Voice};
use plugin303::params::{CircuitTopology, CompTolerance, OscWave, VcaEnvMode, VcfTaps};

const SR: f32 = 48_000.0;
/// A2, 110 Hz: its second harmonic sits far below the open filter.
const NOTE: u8 = 45;
const TIERS: [Tier; 4] = [Tier::Draft, Tier::Low, Tier::High, Tier::Best];

/// A gated voice playing a square (FUN: SWEEP at 100%) through the open
/// filter, PW at 50%, with PWM `pwm` at 0.5 Hz.
fn voice(tier: Tier, topology: CircuitTopology, osc2_vol: f32, pwm: bool) -> Voice {
    // As `Plugin303::initialize` does, before the first block switches to FUN.
    Vco303::warm_cache_nominal(SR, true);
    let mut v = Voice::new(SR);
    v.update_clipper_params_immediate(common::bypassed_clipper());
    v.set_tier_immediate(tier);
    v.update_params(
        0.0,             // tuning
        0.0,             // osc1_pitch_st
        0.0,             // osc2_pitch_st
        osc2_vol,        // osc2_vol
        OscWave::Square, // osc1_wave (ACCURATE plays it; FUN plays the sweep)
        0.5,             // osc1_pw
        0.0,             // sub_vol
        1.0,             // cutoff: open
        0.0,             // resonance
        0.0,             // env_mod
        0.5,             // decay
        0.0,             // accent
        0.0,             // volume_db
        VcfTaps::FourPole,
        0.0,  // vcf_drive_db
        true, // advanced_env
        3.0,
        200.0,
        1.0,
        200.0, // MEG
        3.0,
        200.0,
        1.0,
        200.0, // VEG: held while gated
        VcaEnvMode::Gate,
        0,   // comp_age
        0.0, // comp_drift
        CompTolerance::Zero,
        0, // thermal_noise
        topology,
        1.0,  // sweep_pos: all square
        1337, // comp_seed
        0.0,  // glide
        0.0,  // reso_comp
    );
    v.set_osc1_pwm(pwm, 0.5);
    v.note_on(NOTE);
    v.set_gate(true, true, false, false);
    v
}

fn render(v: &mut Voice, secs: f32) -> Vec<f32> {
    for _ in 0..(0.2 * SR) as usize {
        v.tick();
    }
    (0..(secs * SR) as usize).map(|_| v.tick()).collect()
}

/// Hann-windowed DFT magnitude of `w` at `hz`.
fn tone(w: &[f32], hz: f64) -> f64 {
    let n = w.len() as f64;
    let (mut re, mut im) = (0.0, 0.0);
    for (i, &s) in w.iter().enumerate() {
        let t = i as f64;
        let x = s as f64 * (0.5 - 0.5 * (2.0 * PI * t / n).cos());
        let ph = 2.0 * PI * hz * t / SR as f64;
        re += x * ph.cos();
        im -= x * ph.sin();
    }
    re.hypot(im)
}

/// Min and max second-to-first harmonic ratio in 100 ms windows over two
/// seconds: one full PWM cycle. A pulse of width d has |cos(πd)|: 0 at 50%,
/// 0.95 at 10% and 90%. The filter shades H2 a little (Low's ladder most).
fn h2_over_h1(v: &mut Voice) -> (f64, f64) {
    let x = render(v, 2.0);
    let f1 = midi_to_freq(NOTE) as f64;
    let n = (0.1 * SR) as usize;
    x.windows(n)
        .step_by(n / 2)
        .map(|w| tone(w, 2.0 * f1) / tone(w, f1))
        .fold((f64::MAX, f64::MIN), |(lo, hi), r| (lo.min(r), hi.max(r)))
}

/// The circuit VCO's square is high for `pw` of each cycle and carries no DC.
#[test]
fn circuit_vco_pulse_width_sets_its_duty() {
    let mut v = Vco303::new(SR);
    v.set_wave(OscWave::Square);
    for hz in [110.0, 440.0] {
        v.set_freq(hz);
        for pw in [0.2, 0.5, 0.8] {
            v.set_pw(pw);
            for _ in 0..(SR as usize / 4) {
                v.tick();
            }
            let n = SR as usize / 2;
            let (mut high, mut sum) = (0usize, 0.0f64);
            for _ in 0..n {
                let y = v.tick();
                high += usize::from(y > 0.0);
                sum += y as f64;
            }
            let duty = high as f32 / n as f32;
            let dc = sum / n as f64;
            assert!((duty - pw).abs() < 0.03, "{hz} Hz, PW {pw}: duty {duty}");
            assert!(
                dc.abs() < 0.02,
                "{hz} Hz, PW {pw}: DC {dc} V on a ±0.56 V pulse"
            );
        }
    }
}

#[test]
fn pwm_sweeps_osc1_width_on_every_tier() {
    for tier in TIERS {
        let (lo, hi) = h2_over_h1(&mut voice(tier, CircuitTopology::Fun, 0.0, true));
        assert!(
            lo < 0.1 && hi > 0.5,
            "{tier:?}: H2/H1 spans {lo:.3}..{hi:.3}; a 10-90% sweep spans 0..0.95"
        );
    }
}

/// OSC2 alone (MIX2 at 100%) stays at the static 50% while PWM runs.
#[test]
fn pwm_leaves_osc2_at_the_static_width() {
    for tier in TIERS {
        let (_, hi) = h2_over_h1(&mut voice(tier, CircuitTopology::Fun, 1.0, true));
        assert!(hi < 0.1, "{tier:?}: OSC2 H2/H1 reaches {hi:.3} with PWM on");
    }
}

/// ACCURATE plays the stock 50% square: PWM changes no sample.
#[test]
fn accurate_ignores_pwm() {
    for tier in TIERS {
        let off = render(&mut voice(tier, CircuitTopology::Accurate, 0.0, false), 0.5);
        let on = render(&mut voice(tier, CircuitTopology::Accurate, 0.0, true), 0.5);
        assert!(off == on, "{tier:?}: PWM changed the ACCURATE output");
    }
}
