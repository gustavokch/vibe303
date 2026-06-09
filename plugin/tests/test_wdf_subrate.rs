//! Sub-rate Newton spike guards (`QualityConfig::subrate`). These are smoke/safety checks, NOT the
//! fidelity gate — the GO/NO-GO accuracy/speed decision lives in `eval/wdf_ultrafast` + `wdf_vs_spice`.
//! Here we only pin: (1) subrate==1 is fully inert (bit-exact contract), (2) the frozen-Jacobian and
//! naive-hold paths never emit NaN/Inf or blow up over the golden stream.

use ba283::wdf::{self, QualityConfig};

#[allow(dead_code)]
mod wdf_golden {
    include!("fixtures/wdf_golden.rs");
}

fn run_stream(cfg: QualityConfig) -> Vec<f32> {
    let mut m = wdf::WdfModel::new_with(cfg, wdf_golden::HOST_SR, 220.0, wdf_golden::BIAS);
    m.warmup();
    wdf_golden::INPUT_256
        .iter()
        .map(|&x| m.process_host_sample(x as f32))
        .collect()
}

fn peak(s: &[f32]) -> f64 {
    s.iter().map(|&x| (x as f64).abs()).fold(0.0, f64::max)
}

/// subrate == 1 must be byte-for-byte the base path (the lever is fully gated off): the realtime
/// golden contract is untouched.
#[test]
fn subrate_1_is_inert() {
    let reference = run_stream(QualityConfig::realtime());
    let mut cfg = QualityConfig::realtime();
    cfg.subrate = 1;
    let got = run_stream(cfg);
    let dev = reference
        .iter()
        .zip(got.iter())
        .map(|(&a, &b)| (a as f64 - b as f64).abs())
        .fold(0.0, f64::max);
    assert_eq!(dev, 0.0, "subrate=1 must be bit-identical to the base path");
}

/// Frozen-Jacobian sub-rate (K = 2,3,4) must stay finite and not diverge from the realtime output
/// (the op-point lag is bounded; a broken freeze would blow up or NaN).
#[test]
fn subrate_k_stays_finite_and_bounded() {
    let ref_peak = peak(&run_stream(QualityConfig::realtime()));
    for k in [2usize, 3, 4] {
        let mut cfg = QualityConfig::fast();
        cfg.subrate = k;
        let out = run_stream(cfg);
        assert!(
            out.iter().all(|x| x.is_finite()),
            "subrate={k}: non-finite output"
        );
        assert!(
            peak(&out) <= 3.0 * ref_peak + 1e-6,
            "subrate={k}: output peak {} diverged from realtime peak {ref_peak}",
            peak(&out)
        );
    }
}

/// The naive-hold baseline must also stay finite (it freezes v between full solves).
#[test]
fn subrate_hold_stays_finite() {
    let mut cfg = QualityConfig::fast();
    cfg.subrate = 2;
    cfg.subrate_hold = true;
    let out = run_stream(cfg);
    assert!(out.iter().all(|x| x.is_finite()));
}
