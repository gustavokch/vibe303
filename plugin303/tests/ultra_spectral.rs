//! Ultra fidelity gate (F3-VCO V5, spectral redefinition).
//!
//! Sample-exact ESR vs ngspice is structurally out of reach for a real-time
//! bespoke chain (see design doc F3-VCO V4). The acceptance is redefined to a
//! **spectral character match**: does the Ultra voice have the same harmonic
//! structure / brightness / resonance envelope as the whole-voice SPICE gold?
//!
//! The Welch LSD + dB-spectrum-corr metric lives in `tests/common/mod.rs`
//! (shared with the High gate, F4.2).

mod common;

use common::{gold_dir, load_f32, load_gold_op, spectral_match};
use plugin303::dsp::filter::Tier;
use plugin303::dsp::voice::Voice;

const LSD_BUDGET_DB: f32 = 5.5; // achieved 4.42 dB (gold-power-weighted) + margin
const CORR_MIN: f32 = 0.95; // achieved corr 0.977; spectra share their shape
const ULTRA_BRIGHTNESS: f64 = 1.0; // VCF brightness cal (cal=1.0 is the LSD optimum)

fn render_ultra(sr: f32, cutoff: f32, reso: f32, note: u8, n: usize, cal: f64) -> Vec<f32> {
    let mut v = Voice::new(sr);
    v.set_tier_immediate(Tier::Ultra);
    v.set_ultra_brightness(cal);
    v.set_cutoff(cutoff);
    v.set_reso(reso);
    v.note_on(note);
    (0..n).map(|_| v.tick()).collect()
}

#[test]
#[ignore = "diagnostic: sweep VCF brightness to minimise spectral LSD"]
fn brightness_sweep() {
    let gold = load_f32(&gold_dir().join("voice_A2_mid.f32"));
    let op = load_gold_op("voice_A2_mid.json");
    for cal in [1.0f64, 2.0, 4.0, 7.0, 10.0, 15.0, 20.0] {
        let m = render_ultra(op.sr, op.cutoff, op.reso, op.note, gold.len(), cal);
        let (lsd, corr) = spectral_match(&gold, &m, op.sr);
        println!("cal={cal:5.1}  LSD={lsd:.2} dB  corr={corr:.3}");
    }
}

#[test]
fn ultra_voice_spectral_match() {
    let gold = load_f32(&gold_dir().join("voice_A2_mid.f32"));
    let op = load_gold_op("voice_A2_mid.json");

    let model = render_ultra(op.sr, op.cutoff, op.reso, op.note, gold.len(), ULTRA_BRIGHTNESS);
    let (lsd, corr) = spectral_match(&gold, &model, op.sr);
    println!("Ultra spectral match: LSD = {lsd:.2} dB, corr = {corr:.3}");
    assert!(lsd <= LSD_BUDGET_DB, "Ultra LSD {lsd:.2} dB exceeds budget {LSD_BUDGET_DB} dB");
    assert!(corr >= CORR_MIN, "Ultra spectral corr {corr:.3} below {CORR_MIN}");
}
