//! Low fidelity gate (F4.2, spectral).
//!
//! Low = polyBLEP saw → nodal diode-ladder VCF at 4× oversample. It skips the
//! costly nodal circuit VCO (Best's dominant cost) but keeps the nonlinear
//! diode-ladder filter character, so it should land between Draft (TPT, corr≈0.935 at 4×)
//! and Best (circuit VCO→ladder, corr 0.977) on the same spectral metric.
//!
//! Same Welch LSD + dB-spectrum-corr metric as Best (`tests/common/mod.rs`).
//! The budget is necessarily looser than Best: the polyBLEP saw ≠ the circuit
//! relaxation saw, so the source harmonic structure differs even though the
//! filter is identical. The gold-power-weighted LSD is large for any non-circuit
//! saw (it concentrates on the gold's dominant bins), so Low's LSD lands far
//! from Best's — but well below Draft's. Side-by-side at the gold op-point
//! (`low_spectral_report`, note 57 / 1002 Hz / reso 0.85), Draft and Low at 4×:
//!     Draft LSD 41.58 dB  corr 0.935
//!     Low   LSD 12.48 dB  corr 0.978   <- between the two on both axes
//!     Best  LSD  3.15 dB  corr 0.988
//! The gate proves Low is a genuine fidelity step over Draft: budgets sit below
//! Draft's numbers so a regression to the TPT path would fail.

mod common;

use common::{gold_dir, load_f32, load_gold_op, render_plugin_path, spectral_match};
use plugin303::dsp::filter::Tier;
use plugin303::dsp::voice::Voice;

// Low at 4× re-tuned to brightness 0.45 ⇒ LSD 28.82 → 12.48 (`low_brightness_sweep`).
// The optimum is a narrow dip (0.50 gives 28.82), so the budget guards the tuning.
// Low uses the polyBLEP saw (not the circuit VCO), so its LSD floor is set by the
// VCO mismatch, not the filter — this gate is a discriminator (Draft fails worse).
const LSD_BUDGET_DB: f32 = 16.0; // achieved 12.48 dB (deterministic) + margin
const CORR_MIN: f32 = 0.95; // achieved 0.978; Draft's 0.935 fails → real discriminator

/// Renders Low the way the gate does. `cal: Some(c)` overrides the brightness;
/// `None` plays the voice's own (`LOW_BRIGHT`).
fn render_low(sr: f32, cutoff: f32, reso: f32, note: u8, n: usize, cal: Option<f64>) -> Vec<f32> {
    let mut v = Voice::new(sr);
    v.update_clipper_params_immediate(common::bypassed_clipper());
    v.set_tier_immediate(Tier::Low);
    if let Some(cal) = cal {
        v.set_low_brightness(cal);
    }
    v.set_advanced_env(true);
    v.set_cutoff(cutoff);
    v.set_reso(reso);
    v.note_on(note);
    for _ in 0..48 {
        v.tick();
    }
    (0..n).map(|_| v.tick()).collect()
}

fn render_tier(tier: Tier, op: &common::GoldOp, n: usize) -> Vec<f32> {
    let mut v = Voice::new(op.sr);
    v.update_clipper_params_immediate(common::bypassed_clipper());
    v.set_tier_immediate(tier);
    v.set_advanced_env(true);
    v.set_cutoff(op.cutoff);
    v.set_reso(op.reso);
    v.note_on(op.note);
    for _ in 0..48 {
        v.tick();
    }
    (0..n).map(|_| v.tick()).collect()
}

#[test]
#[ignore = "diagnostic: sweep Low VCF brightness to minimise spectral LSD"]
fn low_brightness_sweep() {
    let gold = load_f32(&gold_dir().join("voice_A2_mid.f32"));
    let op = load_gold_op("voice_A2_mid.json");
    for i in 0..=14 {
        let cal = 0.30 + 0.05 * i as f64;
        let m = render_low(op.sr, op.cutoff, op.reso, op.note, gold.len(), Some(cal));
        let (lsd, corr) = spectral_match(&gold, &m, op.sr);
        println!("cal={cal:5.2}  LSD={lsd:.2} dB  corr={corr:.3}");
    }
}

#[test]
#[ignore = "diagnostic: report Draft/Low/Best LSD+corr side by side"]
fn low_spectral_report() {
    let gold = load_f32(&gold_dir().join("voice_A2_mid.f32"));
    let op = load_gold_op("voice_A2_mid.json");
    println!(
        "note={} cutoff={:.0}Hz reso={:.2} sr={:.0}",
        op.note, op.cutoff, op.reso, op.sr
    );
    for (name, tier) in [
        ("Draft", Tier::Draft),
        ("Low", Tier::Low),
        ("Best", Tier::Best),
    ] {
        let m = render_tier(tier, &op, gold.len());
        let (lsd, corr) = spectral_match(&gold, &m, op.sr);
        println!("{name:5}  LSD = {lsd:6.2} dB   corr = {corr:.3}");
    }
}

#[test]
#[ignore = "diagnostic: per-sample wall-clock cost Draft/Low/Best"]
fn cost_bench() {
    use std::time::Instant;
    let op = load_gold_op("voice_A2_mid.json");
    let n = (op.sr as usize) * 2; // 2 s
    for (name, tier) in [
        ("Draft", Tier::Draft),
        ("Low", Tier::Low),
        ("Best", Tier::Best),
    ] {
        let mut v = Voice::new(op.sr);
        v.set_tier_immediate(tier);
        v.set_cutoff(op.cutoff);
        v.set_reso(op.reso);
        v.note_on(op.note);
        for _ in 0..256 {
            v.tick(); // warm-start out of the timed region
        }
        let t0 = Instant::now();
        let mut acc = 0.0f32;
        for _ in 0..n {
            acc += v.tick();
        }
        let ns = t0.elapsed().as_nanos() as f64 / n as f64;
        std::hint::black_box(acc);
        let rt = ns * op.sr as f64 / 1e9; // CPU fraction of one core at real time
        println!(
            "{name:5}  {ns:7.1} ns/sample   {:5.2}% of one core @ {:.0} Hz",
            rt * 100.0,
            op.sr
        );
    }
}

#[test]
fn low_voice_spectral_match() {
    let gold = load_f32(&gold_dir().join("voice_A2_mid.f32"));
    let op = load_gold_op("voice_A2_mid.json");

    let model = render_low(op.sr, op.cutoff, op.reso, op.note, gold.len(), None);
    let (lsd, corr) = spectral_match(&gold, &model, op.sr);
    println!("Low spectral match: LSD = {lsd:.2} dB, corr = {corr:.3}");
    assert!(
        lsd <= LSD_BUDGET_DB,
        "Low LSD {lsd:.2} dB exceeds budget {LSD_BUDGET_DB} dB"
    );
    assert!(
        corr >= CORR_MIN,
        "Low spectral corr {corr:.3} below {CORR_MIN}"
    );
}

/// Same gate through the plugin's parameter path (`common::render_plugin_path`):
/// the default patch with first-block analog wear and Env Mod at 0. Measured
/// 10.50 dB / 0.977 (`low_voice_spectral_match`: 12.47 dB). With analog wear
/// resetting the brightness to 1.0, or with the `ladder_low` brightness forced
/// to 1.0 inside `update_params`, it reads 39.56 dB, while
/// `low_voice_spectral_match` still passes (12.47 dB): `Voice::new` does not
/// wear the Low ladder, so only this gate sees it.
#[test]
fn low_plugin_path_spectral_match() {
    let gold = load_f32(&gold_dir().join("voice_A2_mid.f32"));
    let op = load_gold_op("voice_A2_mid.json");

    let model = render_plugin_path(Tier::Low, &op, gold.len());
    let (lsd, corr) = spectral_match(&gold, &model, op.sr);
    println!("Low plugin-path spectral match: LSD = {lsd:.2} dB, corr = {corr:.3}");
    assert!(
        lsd <= LSD_BUDGET_DB,
        "Low plugin-path LSD {lsd:.2} dB exceeds budget {LSD_BUDGET_DB} dB"
    );
    assert!(
        corr >= CORR_MIN,
        "Low plugin-path spectral corr {corr:.3} below {CORR_MIN}"
    );
}
