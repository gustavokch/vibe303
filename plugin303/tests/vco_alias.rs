//! Aliasing of the circuit VCO (`Vco303`, High and Best tiers) at its JFET
//! reset and square edges.
//!
//! The solver puts the reset on the sample grid and overshoots on the sample
//! after it; `tick()` rebuilds both samples and band-limits the jump at the
//! sub-sample crossing. Before that, the saw's inharmonic energy was -16 dB at
//! C6 and -14 dB at C7 (48 kHz) and the square's -20 and -17 dB.

mod common;

use plugin303::dsp::vco::Vco303;
use plugin303::params::OscWave;
use std::f64::consts::PI;

const SR: f32 = 48_000.0;
const N: usize = 1 << 16;

fn render(vco: &mut Vco303, hz: f32, wave: OscWave) -> Vec<f32> {
    vco.set_freq(hz);
    vco.set_wave(wave);
    for _ in 0..(0.3 * SR) as usize {
        vco.tick();
    }
    (0..N).map(|_| vco.tick()).collect()
}

/// Energy between the harmonics over the energy on them, 20 Hz to 20 kHz, in
/// dB: a Hann-windowed FFT, with ±8 bins (±5.9 Hz) kept around each harmonic.
fn inharmonic_db(x: &[f32], hz_nominal: f64) -> f64 {
    let mean = x.iter().map(|&v| v as f64).sum::<f64>() / x.len() as f64;
    let mut re: Vec<f64> = x
        .iter()
        .enumerate()
        .map(|(i, &v)| (v as f64 - mean) * (0.5 - 0.5 * (2.0 * PI * i as f64 / N as f64).cos()))
        .collect();
    let mut im = vec![0.0; N];
    common::fft(&mut re, &mut im);
    let pow: Vec<f64> = (0..N / 2).map(|k| re[k] * re[k] + im[k] * im[k]).collect();
    let df = SR as f64 / N as f64;
    // The fundamental: the peak within ±3 % of nominal, parabolic refine.
    let (lo, hi) = (
        (hz_nominal * 0.97 / df) as usize,
        (hz_nominal * 1.03 / df) as usize,
    );
    let k = (lo..hi).max_by(|&a, &b| pow[a].total_cmp(&pow[b])).unwrap();
    let (a, b, c) = (pow[k - 1].ln(), pow[k].ln(), pow[k + 1].ln());
    let f0 = (k as f64 + 0.5 * (a - c) / (a - 2.0 * b + c)) * df;
    let mut harmonic = vec![false; N / 2];
    let mut h = 1.0;
    while h * f0 < SR as f64 / 2.0 {
        let i = (h * f0 / df).round() as usize;
        for flag in &mut harmonic[i.saturating_sub(8)..(i + 9).min(N / 2)] {
            *flag = true;
        }
        h += 1.0;
    }
    let (mut on, mut off) = (0.0, 0.0);
    for (i, &p) in pow.iter().enumerate() {
        let f = i as f64 * df;
        if (20.0..=20_000.0).contains(&f) {
            if harmonic[i] {
                on += p;
            } else {
                off += p;
            }
        }
    }
    10.0 * (off / on).log10()
}

/// An ideal two-tap BLEP saw reads -36 dB at C6 and -34 dB at C7; past that,
/// the circuit VCO adds the solver's own period jitter (0.03-0.05 samples).
#[test]
fn circuit_vco_edges_are_band_limited() {
    let mut vco = Vco303::new(SR);
    for (wave, name) in [(OscWave::Saw, "saw"), (OscWave::Square, "square")] {
        for (hz, budget) in [(1046.5, -31.0), (2093.0, -27.0)] {
            vco.reset();
            let db = inharmonic_db(&render(&mut vco, hz as f32, wave), hz);
            println!("{name} {hz} Hz: inharmonic {db:.1} dB (budget {budget})");
            assert!(
                db <= budget,
                "{name} {hz} Hz: inharmonic {db:.1} dB > {budget} dB"
            );
        }
    }
}

/// `reset()` must clear the predicted edge and the fold bracket, or the next
/// note starts from a stale crossing.
#[test]
fn reset_restarts_like_a_fresh_vco() {
    let mut fresh = Vco303::new(SR);
    let expected = render(&mut fresh, 523.25, OscWave::Sweep);
    let mut used = Vco303::new(SR);
    render(&mut used, 2093.0, OscWave::Square);
    used.reset();
    assert_eq!(render(&mut used, 523.25, OscWave::Sweep), expected);
}
