//! Accuracy guard for the depletion-charge LUT (PR #68).
//!
//! On lossy tiers the LUT replaces the per-junction `pow` in the depletion-charge kernel. The PR
//! measured the added error vs the exact `pow` path at ≈ −78 dB (well inside the −40 dB budget),
//! but that number came from a manual `bench_deplut` run — nothing in CI failed if a later edit
//! (smaller `DEP_LUT_N`, broken interp, swapped `vj`/`cj0` args) blew the budget.
//!
//! This pins the invariant: render the SAME tier (`fast`) twice — once with the LUT (`dep_lut =
//! true`, the shipped fast tier) and once forced onto the exact `pow` path (`dep_lut = false`) —
//! through an identical hot stimulus, and assert the LUT's added error stays below −60 dB (a
//! comfortable margin under the −40 dB budget, tight enough to catch a real regression).
//!
//! Run in `--release`: the shipped plugin is release (FMA / cheap-exp / fast-math differ from
//! debug), and the LUT lives on the release hot path.

use ba283::wdf::{QualityConfig, WdfModel};

const HOST_SR: f64 = 96_000.0;
const INPUT_FS_VOLTS: f64 = 0.5; // dsp.rs: 0 dBFS ≡ 0.5 V

/// 20 Hz → 18 kHz log sweep at near full scale with a hard DC step mid-stream, driven hot
/// (+12 dB) and expressed in circuit volts — exercises the full bias range the depletion arm sees,
/// including the deep-forward operating point where the `pow` arm dominates.
fn stimulus() -> Vec<f32> {
    let n = (HOST_SR * 0.25) as usize;
    let step_at = n / 2;
    let drive = 10.0_f64.powf(12.0 / 20.0);
    let (f0, f1) = (20.0_f64, 18_000.0_f64);
    let k = (f1 / f0).ln();
    let mut phase = 0.0_f64;
    (0..n)
        .map(|i| {
            let t = i as f64 / n as f64;
            let inst = f0 * (k * t).exp();
            phase += 2.0 * std::f64::consts::PI * inst / HOST_SR;
            let dc = if i >= step_at { 0.3 } else { 0.0 };
            let samp = 0.95 * phase.sin() + dc;
            (samp * drive * INPUT_FS_VOLTS) as f32
        })
        .collect()
}

fn render(cfg: QualityConfig, sig: &[f32]) -> Vec<f64> {
    let mut m = WdfModel::new_with(cfg, HOST_SR, 220.0, 0.5);
    sig.iter().map(|&x| m.process_host_sample(x) as f64).collect()
}

#[test]
fn dep_lut_added_error_within_budget() {
    let sig = stimulus();

    let lut_cfg = QualityConfig::fast();
    assert!(lut_cfg.dep_lut, "fast tier must ship with the depletion LUT on");
    let mut pow_cfg = QualityConfig::fast();
    pow_cfg.dep_lut = false; // same tier, exact `pow` depletion path

    let y_lut = render(lut_cfg, &sig);
    let y_pow = render(pow_cfg, &sig);
    assert_eq!(y_lut.len(), y_pow.len());

    // Skip the initial warmup transient (first 10%).
    let warm = y_lut.len() / 10;
    let (mut sum_err2, mut sum_ref2) = (0.0_f64, 0.0_f64);
    for i in warm..y_lut.len() {
        let d = y_lut[i] - y_pow[i];
        sum_err2 += d * d;
        sum_ref2 += y_pow[i] * y_pow[i];
    }
    assert!(sum_ref2 > 1e-12, "reference signal is silent — stimulus/setup broken");
    let err_db = 10.0 * (sum_err2 / sum_ref2).log10();
    assert!(
        err_db < -60.0,
        "dep-LUT added error {err_db:.1} dB exceeds the −60 dB guard (budget −40 dB) — \
         check DEP_LUT_N, the interp, or the dep_charge_lane_lut arg order"
    );
}
