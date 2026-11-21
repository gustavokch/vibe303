//! Output polarity vs the whole-voice SPICE gold. The circuit VCF inverts and
//! the Rust filters do not, so the voice inverts before the output clipper
//! (`Voice::tick`). The clipper rectifies only positive peaks, so a polarity
//! flip anywhere in the chain changes how every tier drives it.

mod common;

use plugin303::dsp::filter::Tier;
use plugin303::dsp::voice::{midi_to_freq, Voice};

/// Samples skipped at the start of the gold and the renders (the attack).
const SKIP: usize = 2_400;

/// Phase (deg) of harmonic `h` of `x` at fundamental `f` Hz (Hann window).
fn phase_deg(x: &[f32], f: f64, h: f64, sr: f64) -> f64 {
    let n = x.len() as f64;
    let (mut re, mut im) = (0.0, 0.0);
    for (i, &s) in x.iter().enumerate() {
        let w = 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / n).cos();
        let ph = std::f64::consts::TAU * f * h * i as f64 / sr;
        re += w * s as f64 * ph.cos();
        im -= w * s as f64 * ph.sin();
    }
    im.atan2(re).to_degrees()
}

fn wrap_deg(d: f64) -> f64 {
    (d + 540.0).rem_euclid(360.0) - 180.0
}

/// Saw polarity as `phase(h2) - 2 phase(h1)`: independent of a time shift and,
/// to first order, of a small error in `f`. Inverting the signal moves it 180°.
fn polarity_deg(x: &[f32], f: f64, sr: f64) -> f64 {
    wrap_deg(phase_deg(x, f, 2.0, sr) - 2.0 * phase_deg(x, f, 1.0, sr))
}

fn render(tier: Tier, op: &common::GoldOp, n: usize) -> Vec<f32> {
    let mut v = Voice::new(op.sr);
    v.update_clipper_params_immediate(common::bypassed_clipper());
    v.set_tier_immediate(tier);
    v.set_advanced_env(true);
    v.set_cutoff(op.cutoff);
    v.set_reso(op.reso);
    v.note_on(op.note);
    for _ in 0..v.latency_samples() {
        v.tick();
    }
    (0..n).map(|_| v.tick()).collect()
}

fn assert_near(what: &str, off: f64) {
    println!("{what}: {off:+.0}°");
    assert!(
        off.abs() < 60.0,
        "{what} is {off:+.0}° off: the voice is inverted"
    );
}

/// Best plays the gold's polarity at the gold patch (measured −15°; without
/// the voice's inversion it reads +165°).
#[test]
fn best_matches_the_gold_polarity() {
    let op = common::load_gold_op("voice_A2_mid.json");
    let gold = common::load_f32(&common::gold_dir().join("voice_A2_mid.f32"));
    let (f, sr) = (midi_to_freq(op.note) as f64, op.sr as f64);
    let best = render(Tier::Best, &op, gold.len());
    let off = polarity_deg(&best[SKIP..], f, sr) - polarity_deg(&gold[SKIP..], f, sr);
    assert_near("Best vs the gold", wrap_deg(off));
}

/// Every tier plays Best's polarity on an open, resonance-free patch, where
/// the filters barely shift the harmonics' phases. At the resonant gold patch
/// Low's darker filter alone moves the reading by about 50–100°.
#[test]
fn every_tier_matches_best_polarity() {
    let op = common::GoldOp {
        note: 45,
        cutoff: 3_000.0,
        reso: 0.0,
        sr: 48_000.0,
    };
    let (f, sr) = (midi_to_freq(op.note) as f64, op.sr as f64);
    let n = (0.5 * sr) as usize;
    let best = polarity_deg(&render(Tier::Best, &op, n)[SKIP..], f, sr);
    for tier in [Tier::Draft, Tier::Low, Tier::High] {
        let x = render(tier, &op, n);
        let off = wrap_deg(polarity_deg(&x[SKIP..], f, sr) - best);
        assert_near(&format!("{tier:?} vs Best"), off);
    }
}
