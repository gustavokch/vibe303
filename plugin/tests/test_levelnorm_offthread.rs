//! The levelNorm gain probe runs OFF the audio thread (the full-fidelity probe is ~5–30 ms — far
//! past a block budget — so it must never run inline). `process` posts an operating point via
//! `execute_background` and the worker writes back `1/G_v`. These tests drive the worker body
//! directly (`Ba283Plugin::force_probe`, the exact closure nih_plug runs on its background thread)
//! so the build / re-stamp / measure path is covered deterministically without a host, and print the
//! per-request latency so the worker cost is documented.
use ba283::wdf::QualityConfig;
use ba283::{norm_apply_gain, Ba283Plugin};
use std::time::Instant;

const SR: f32 = 48_000.0;
const SEED: u64 = 0xBA283_0000_0001;

/// First probe builds the worker model (cold warmup), later probes re-stamp it. Both must publish a
/// finite, plausible trim (`1/G_v` with `G_v > 1` for the amplifying preamp -> trim < 1).
#[test]
fn probe_publishes_plausible_trim() {
    let p = Ba283Plugin::default();
    p.force_probe(QualityConfig::realtime(), SR, 220.0, 0.5, 0.0, SEED, 0.5);
    let trim = p.probe_trim();
    assert!(trim.is_finite() && trim > 0.0 && trim < 1.0, "implausible trim {trim}");

    // Second probe at the same operating point must be deterministic (re-stamp path, no rebuild).
    p.force_probe(QualityConfig::realtime(), SR, 220.0, 0.5, 0.0, SEED, 0.5);
    let trim2 = p.probe_trim();
    assert!((trim - trim2).abs() / trim < 0.02, "re-stamp probe drifted: {trim} vs {trim2}");
}

/// The probed trim tracks the Circuit-Gain operating point (G_v swings ~13x across the range), so the
/// re-stamp path actually re-measures — proving the off-thread reprobe is meaningful, not a no-op.
#[test]
fn probe_tracks_circuit_gain() {
    let p = Ba283Plugin::default();
    p.force_probe(QualityConfig::realtime(), SR, 47.0, 0.5, 0.0, SEED, 0.5);
    let lo = p.probe_trim();
    p.force_probe(QualityConfig::realtime(), SR, 10_000.0, 0.5, 0.0, SEED, 0.5);
    let hi = p.probe_trim();
    assert!(
        (lo - hi).abs() / lo.max(hi) > 0.05,
        "circuit-gain change barely moved the trim: {lo} vs {hi}"
    );
}

/// `norm_apply_gain` (shared by `process` and the regression tests) cancels the circuit gain and the
/// known Drive scalar: trimming `1/G_v` then dividing Drive lands the applied gain at `1/(G_v*drive)`.
#[test]
fn apply_gain_cancels_drive() {
    let p = Ba283Plugin::default();
    p.force_probe(QualityConfig::realtime(), SR, 220.0, 0.5, 0.0, SEED, 0.5);
    let trim = p.probe_trim(); // = 1/G_v
    let drive = 4.0; // +12 dB
    let g = norm_apply_gain(trim, drive);
    assert!((g - trim / drive).abs() < 1e-6, "drive not cancelled: {g} vs {}", trim / drive);
    // No Drive => the applied gain is just the trim.
    assert!((norm_apply_gain(trim, 1.0) - trim).abs() < 1e-6);
}

/// Document the worker's per-request latency (cold build vs warm re-stamp). NOT a tight perf assert
/// (timing is noisy in CI) — just a sanity ceiling so a pathological regression is caught.
#[test]
fn probe_latency_is_documented() {
    let p = Ba283Plugin::default();
    let t0 = Instant::now();
    p.force_probe(QualityConfig::realtime(), SR, 220.0, 0.5, 0.0, SEED, 0.5);
    let cold = t0.elapsed().as_secs_f64() * 1e3;
    let t1 = Instant::now();
    p.force_probe(QualityConfig::realtime(), SR, 221.0, 0.5, 0.0, SEED, 0.5);
    let warm = t1.elapsed().as_secs_f64() * 1e3;
    eprintln!("off-thread probe latency: cold build {cold:.1} ms, warm re-stamp {warm:.1} ms");
    // Generous ceilings (timing is noisy + machine-dependent): the cold build does a full ~1 s
    // internal-rate warmup at os=2, the warm path only re-settles. Both run on the background worker,
    // off the audio thread — this only catches a pathological hang, not a perf regression.
    assert!(cold < 15_000.0 && warm < 5_000.0, "probe pathologically slow: cold {cold}, warm {warm}");
}
