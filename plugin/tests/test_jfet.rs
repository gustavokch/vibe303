//! Tests for the JFET voicing (feature `jfet`): swap the three Class A BJTs for a Shichman-Hodges
//! square-law JFET. A creative tonal alternative, not a faithful model — so these gate STABILITY and
//! the bit-exact-when-off contract, not numeric fidelity to a golden vector.
//!
//! The whole file is feature-gated, so the default build is untouched. Run with:
//!   cargo test --features jfet --release --test test_jfet
#![cfg(feature = "jfet")]

use ba283::wdf::{self, QualityConfig};

#[allow(dead_code)]
mod wdf_golden {
    include!("fixtures/wdf_golden.rs");
}

const RGAIN: f64 = 220.0; // "30 dB" gain-switch position (matches the trafo tests)

fn cfg_jfet(on: bool) -> QualityConfig {
    let mut c = QualityConfig::realtime();
    c.jfet = on;
    c
}

fn build(on: bool) -> wdf::WdfModel {
    wdf::WdfModel::new_with(cfg_jfet(on), wdf_golden::HOST_SR, RGAIN, wdf_golden::BIAS)
}

fn run_stream(m: &mut wdf::WdfModel) -> Vec<f32> {
    wdf_golden::INPUT_256.iter().map(|&x| m.process_host_sample(x as f32)).collect()
}

/// jfet=false must be bit-identical to the stock realtime BJT model: the JFET plumbing (extra
/// `WdfModel` fields, the Newton kernel branch) must not perturb the BJT path by a single ULP.
#[test]
fn jfet_off_is_bit_identical() {
    let mut stock = wdf::WdfModel::new_with(QualityConfig::realtime(), wdf_golden::HOST_SR, RGAIN, wdf_golden::BIAS);
    stock.warmup();
    let base = run_stream(&mut stock);

    let mut off = build(false);
    off.warmup();
    let got = run_stream(&mut off);

    for (i, (a, b)) in base.iter().zip(got.iter()).enumerate() {
        assert_eq!(a.to_bits(), b.to_bits(), "jfet=false sample {i} not bit-identical to stock");
    }
}

/// jfet=true must converge and stay finite + bounded across the golden input. (Non-convergence shows
/// up as NaN or a runaway ~1e5 output, so finite+bounded is the practical convergence gate.)
#[test]
fn jfet_on_finite_and_bounded() {
    let mut m = build(true);
    m.warmup();
    let out = run_stream(&mut m);
    for (i, &y) in out.iter().enumerate() {
        assert!(y.is_finite(), "jfet sample {i} not finite: {y}");
        assert!(y.abs() < 50.0, "jfet sample {i} out of bounds: {y}");
    }
}

/// Hot-drive convergence: push a loud sweep (well past unity) through the JFET model and confirm it
/// stays finite + bounded — the square-law device has different Newton conditioning than the BJT, so
/// this is the decisive stability gate.
#[test]
fn jfet_converges_under_hot_drive() {
    let mut m = build(true);
    m.warmup();
    let sr = wdf_golden::HOST_SR as f32;
    let mut max_abs = 0.0f32;
    for n in 0..8192 {
        // 1 kHz tone ramping from ~unity to a very hot +24 dB-ish amplitude.
        let amp = 1.0 + 15.0 * (n as f32 / 8192.0);
        let x = amp * (2.0 * std::f32::consts::PI * 1000.0 * n as f32 / sr).sin();
        let y = m.process_host_sample(x);
        assert!(y.is_finite(), "hot-drive sample {n} not finite (amp {amp})");
        max_abs = max_abs.max(y.abs());
    }
    assert!(max_abs < 100.0, "hot-drive output runaway: peak {max_abs}");
}

/// Determinism / stereo coherence: two JFET models built with identical params must produce
/// bit-identical output (the per-channel models share seed/age, so L and R stay phase-locked).
#[test]
fn jfet_is_deterministic() {
    let mut a = build(true);
    let mut b = build(true);
    a.warmup();
    b.warmup();
    let oa = run_stream(&mut a);
    let ob = run_stream(&mut b);
    for (i, (x, y)) in oa.iter().zip(ob.iter()).enumerate() {
        assert_eq!(x.to_bits(), y.to_bits(), "jfet sample {i} not deterministic across models");
    }
}

/// DC sanity: after warmup with silence, the idle output must be finite and near zero (the JFET op
/// point settles to a stable quiescent bias, not a rail or a NaN).
#[test]
fn jfet_idle_is_quiet() {
    let mut m = build(true);
    m.warmup();
    let mut peak = 0.0f32;
    for _ in 0..2048 {
        let y = m.process_host_sample(0.0);
        assert!(y.is_finite(), "idle output not finite");
        peak = peak.max(y.abs());
    }
    assert!(peak < 1.0, "idle output not quiet: peak {peak}");
}

/// DRIFT GUARD for the embedded `JFET_MAKEUP` table — the JFET analogue of
/// `test_stepped_gain::embedded_makeup_matches_remeasured_gain`. Re-measure each `circuit_gain`
/// position's LARGE-SIGNAL voltage gain `G_sat` at every DRIVE grid point with the JFET cards on
/// (the exact `A0 * 10^(drive/20)` stimulus `bin/measure_gi --jfet` uses) and assert `1/G_sat`
/// matches the curve baked into `JFET_MAKEUP` (via `Ba283Plugin::jfet_pos_makeup`). Tuning `beta`
/// in `jfet_small`/`jfet_power` — which the card docs invite — shifts the op-point and fails this,
/// forcing a `measure_gi --jfet` re-run instead of silently shipping a stale levelNorm.
#[test]
fn jfet_makeup_matches_remeasured_gain() {
    use ba283::dsp::{ChannelDsp, INPUT_FS_VOLTS};
    use ba283::Ba283Plugin;
    // MUST mirror `DRIVE_GRID_DB` in lib.rs / bin/measure_gi.
    const DRIVE_GRID_DB: [f32; 13] =
        [-12.0, -9.0, -6.0, -3.0, 0.0, 3.0, 6.0, 9.0, 12.0, 15.0, 18.0, 21.0, 24.0];
    // Mirrors `bin/measure_gi::STEP_R_KJ` (open sentinel → 110 Ω), idx 0..4 = 15/20/25/30/35 dB.
    const STEP_R_KJ: [f32; 5] = [1.0e9, 1_200.0, 470.0, 220.0, 110.0];
    let a0 = INPUT_FS_VOLTS as f64; // 0 dBFS calibration reference
    let host = wdf_golden::HOST_SR as f32;
    for (pos, &r) in STEP_R_KJ.iter().enumerate() {
        let mut ch = ChannelDsp::new();
        ch.wdf_rebuild_with(cfg_jfet(true), host, r, 0.5);
        ch.wdf_warmup();
        for &d in &DRIVE_GRID_DB {
            let amp = a0 * 10f64.powf(d as f64 * 0.05);
            let measured = 1.0 / ch.wdf_measure_gain_at(amp) as f64;
            let embedded = Ba283Plugin::jfet_pos_makeup(pos as i32, d) as f64;
            let rel = (measured - embedded).abs() / embedded;
            assert!(
                rel < 0.02,
                "pos {pos} (R={r}Ω) drive {d:+} dB: embedded JFET makeup {embedded} drifted from \
                 re-measured {measured} (rel {rel:.4} > 2%). Re-run \
                 `cargo run --features jfet --release --bin measure_gi -- --jfet` and update JFET_MAKEUP."
            );
        }
    }
}
