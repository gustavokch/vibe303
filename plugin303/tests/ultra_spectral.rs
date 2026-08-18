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

// F6.5: active Q9 resonance (F6.3) + re-tuned brightness (0.70) dropped the
// gold-power-weighted LSD from 4.42 → 3.87 dB. Budget tightened below the old 5.5.
const LSD_BUDGET_DB: f32 = 4.0; // achieved 3.87 dB (deterministic render) + margin
const CORR_MIN: f32 = 0.95; // achieved corr 0.979; spectra share their shape
const ULTRA_BRIGHTNESS: f64 = 0.70; // re-tuned VCF brightness (resonance-active LSD min)

fn render_ultra(sr: f32, cutoff: f32, reso: f32, note: u8, n: usize, cal: f64) -> Vec<f32> {
    let mut v = Voice::new(sr);
    v.update_clipper_params_immediate(true, plugin303::params::ClipperOversampling::X2, 47000.0, 4700.0, 8200.0, 1e-9, false, 1.0, 0.0, 0.0, 0.0, 0.0);
    v.set_tier_immediate(Tier::Ultra);
    v.set_advanced_env(true);
    v.set_ultra_brightness(cal);
    v.set_cutoff(cutoff);
    v.set_reso(reso);
    v.note_on(note);
    for _ in 0..48 {
        v.tick();
    }
    (0..n).map(|_| v.tick()).collect()
}

#[test]
#[ignore = "diagnostic: sweep VCF brightness to minimise spectral LSD"]
fn brightness_sweep() {
    let gold = load_f32(&gold_dir().join("voice_A2_mid.f32"));
    let op = load_gold_op("voice_A2_mid.json");
    for cal in [0.64f64, 0.66, 0.68, 0.70, 0.72, 0.74, 0.76] {
        let m = render_ultra(op.sr, op.cutoff, op.reso, op.note, gold.len(), cal);
        let (lsd, corr) = spectral_match(&gold, &m, op.sr);
        println!("cal={cal:5.2}  LSD={lsd:.2} dB  corr={corr:.3}");
    }
    println!("-- reso_gain sweep at cal=0.70 --");
    for rg in [8.0f64, 10.0, 11.0, 12.0, 13.0, 14.0] {
        let mut v = Voice::new(op.sr);
        v.set_tier_immediate(Tier::Ultra);
        v.set_ultra_brightness(0.70);
        v.set_ultra_reso_gain(rg);
        v.set_cutoff(op.cutoff);
        v.set_reso(op.reso);
        v.note_on(op.note);
        let m: Vec<f32> = (0..gold.len()).map(|_| v.tick()).collect();
        let (lsd, corr) = spectral_match(&gold, &m, op.sr);
        println!("rg={rg:5.1}  LSD={lsd:.2} dB  corr={corr:.3}");
    }
}

#[test]
fn ultra_voice_spectral_match() {
    let gold = load_f32(&gold_dir().join("voice_A2_mid.f32"));
    let op = load_gold_op("voice_A2_mid.json");

    let model = render_ultra(
        op.sr,
        op.cutoff,
        op.reso,
        op.note,
        gold.len(),
        ULTRA_BRIGHTNESS,
    );
    let (lsd, corr) = spectral_match(&gold, &model, op.sr);
    println!("Ultra spectral match: LSD = {lsd:.2} dB, corr = {corr:.3}");
    assert!(
        lsd <= LSD_BUDGET_DB,
        "Ultra LSD {lsd:.2} dB exceeds budget {LSD_BUDGET_DB} dB"
    );
    assert!(
        corr >= CORR_MIN,
        "Ultra spectral corr {corr:.3} below {CORR_MIN}"
    );
}
