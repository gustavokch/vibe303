//! Best fidelity gate (F3-VCO V5, spectral redefinition).
//!
//! Sample-exact ESR vs ngspice is structurally out of reach for a real-time
//! bespoke chain (see design doc F3-VCO V4). The acceptance is redefined to a
//! **spectral character match**: does the Best voice have the same harmonic
//! structure / brightness / resonance envelope as the whole-voice SPICE gold?
//!
//! The Welch LSD + dB-spectrum-corr metric lives in `tests/common/mod.rs`
//! (shared with the Low gate, F4.2).
//!
//! Two renders: `render_best` plays a bare `Voice`; `common::render_plugin_path`
//! first runs the plugin's default parameter update, so its gate also covers an
//! update that overrides the calibrated voicing.

mod common;

use common::{gold_dir, load_f32, load_gold_op, render_plugin_path, spectral_match};
use plugin303::dsp::filter::Tier;
use plugin303::dsp::voice::Voice;

// F6.5: active Q9 resonance (F6.3) + re-tuned brightness (CIRCUIT_BRIGHT = 0.70)
// dropped the gold-power-weighted LSD from 4.42 → 3.87 dB. The gate renders
// the voice's own brightness, so it measures what the plugin plays.
const LSD_BUDGET_DB: f32 = 4.0; // achieved 3.15 dB (deterministic render) + margin
const CORR_MIN: f32 = 0.95; // achieved corr 0.988; spectra share their shape

fn render_best(sr: f32, cutoff: f32, reso: f32, note: u8, n: usize, cal: Option<f64>) -> Vec<f32> {
    let mut v = Voice::new(sr);
    v.update_clipper_params_immediate(common::bypassed_clipper());
    v.set_tier_immediate(Tier::Best);
    v.set_advanced_env(true);
    if let Some(cal) = cal {
        v.set_circuit_brightness(cal);
    }
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
        let m = render_best(op.sr, op.cutoff, op.reso, op.note, gold.len(), Some(cal));
        let (lsd, corr) = spectral_match(&gold, &m, op.sr);
        println!("cal={cal:5.2}  LSD={lsd:.2} dB  corr={corr:.3}");
    }
    println!("-- reso_gain sweep at cal=0.70 --");
    for rg in [8.0f64, 10.0, 11.0, 12.0, 13.0, 14.0] {
        let mut v = Voice::new(op.sr);
        v.set_tier_immediate(Tier::Best);
        v.set_circuit_brightness(0.70);
        v.set_circuit_reso_gain(rg);
        v.set_cutoff(op.cutoff);
        v.set_reso(op.reso);
        v.note_on(op.note);
        let m: Vec<f32> = (0..gold.len()).map(|_| v.tick()).collect();
        let (lsd, corr) = spectral_match(&gold, &m, op.sr);
        println!("rg={rg:5.1}  LSD={lsd:.2} dB  corr={corr:.3}");
    }
}

#[test]
fn best_voice_spectral_match() {
    let gold = load_f32(&gold_dir().join("voice_A2_mid.f32"));
    let op = load_gold_op("voice_A2_mid.json");

    let model = render_best(op.sr, op.cutoff, op.reso, op.note, gold.len(), None);
    let (lsd, corr) = spectral_match(&gold, &model, op.sr);
    println!("Best spectral match: LSD = {lsd:.2} dB, corr = {corr:.3}");
    assert!(
        lsd <= LSD_BUDGET_DB,
        "Best LSD {lsd:.2} dB exceeds budget {LSD_BUDGET_DB} dB"
    );
    assert!(
        corr >= CORR_MIN,
        "Best spectral corr {corr:.3} below {CORR_MIN}"
    );
}

/// Same gate through the plugin's parameter path (`common::render_plugin_path`):
/// the default patch with first-block analog wear and Env Mod at 0. Measured
/// 3.50 dB / 0.989. With analog wear resetting the brightness to 1.0 (the bug
/// fixed in `29936e7`) it reads 10.16 dB; `best_voice_spectral_match` fails too
/// (9.59 dB), because `Voice::new` applies analog wear. Only this gate sees an
/// override inside `update_params`: an override placed there fails this gate
/// (10.16 dB) and passes the bare one (3.15 dB).
#[test]
fn best_plugin_path_spectral_match() {
    let gold = load_f32(&gold_dir().join("voice_A2_mid.f32"));
    let op = load_gold_op("voice_A2_mid.json");

    let model = render_plugin_path(Tier::Best, &op, gold.len());
    let (lsd, corr) = spectral_match(&gold, &model, op.sr);
    println!("Best plugin-path spectral match: LSD = {lsd:.2} dB, corr = {corr:.3}");
    assert!(
        lsd <= LSD_BUDGET_DB,
        "Best plugin-path LSD {lsd:.2} dB exceeds budget {LSD_BUDGET_DB} dB"
    );
    assert!(
        corr >= CORR_MIN,
        "Best plugin-path spectral corr {corr:.3} below {CORR_MIN}"
    );
}
