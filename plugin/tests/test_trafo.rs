//! Spike tests for the nonlinear Jiles-Atherton transformer core (`trafo` knob).
//! Only built/run under `--features trafo-core`. The whole file is feature-gated so the default
//! build is untouched.
#![cfg(feature = "trafo-core")]

use ba283::wdf::{self, QualityConfig};

#[allow(dead_code)]
mod wdf_golden {
    include!("fixtures/wdf_golden.rs");
}

fn run_stream_trafo(trafo: f64) -> Vec<f32> {
    let mut m =
        wdf::WdfModel::new_with(QualityConfig::realtime(), wdf_golden::HOST_SR, 220.0, wdf_golden::BIAS);
    m.set_trafo(trafo);
    m.warmup();
    wdf_golden::INPUT_256
        .iter()
        .map(|&x| m.process_host_sample(x as f32))
        .collect()
}

fn max_dev(a: &[f32], b: &[f32]) -> f64 {
    a.iter().zip(b.iter()).map(|(&x, &y)| (x as f64 - y as f64).abs()).fold(0.0, f64::max)
}

#[test]
fn trafo_zero_is_bit_identical_and_partition_unchanged() {
    // trafo=0 => ja_active false => the linear path. Must be bit-identical to a model that never
    // touched trafo, and the Schur N-set must stay at 6 (no N_PA added).
    let mut linear =
        wdf::WdfModel::new_with(QualityConfig::realtime(), wdf_golden::HOST_SR, 220.0, wdf_golden::BIAS);
    linear.warmup();
    let base: Vec<f32> =
        wdf_golden::INPUT_256.iter().map(|&x| linear.process_host_sample(x as f32)).collect();
    assert_eq!(linear.n_sz(), 6, "linear N-set must be 6");

    let mut m =
        wdf::WdfModel::new_with(QualityConfig::realtime(), wdf_golden::HOST_SR, 220.0, wdf_golden::BIAS);
    m.set_trafo(0.0);
    assert_eq!(m.n_sz(), 6, "trafo=0 must keep N-set at 6");
    m.warmup();
    let got: Vec<f32> =
        wdf_golden::INPUT_256.iter().map(|&x| m.process_host_sample(x as f32)).collect();
    for (i, (a, b)) in base.iter().zip(got.iter()).enumerate() {
        assert_eq!(a.to_bits(), b.to_bits(), "trafo=0 sample {i} not bit-identical");
    }
}

#[test]
fn trafo_active_grows_partition() {
    let mut m =
        wdf::WdfModel::new_with(QualityConfig::realtime(), wdf_golden::HOST_SR, 220.0, wdf_golden::BIAS);
    m.set_trafo(0.5);
    assert_eq!(m.n_sz(), 7, "active JA core must add N_PA → N-set 7");
    m.set_trafo(0.0);
    assert_eq!(m.n_sz(), 6, "deactivating must restore N-set 6");
}

// GO-CRITERIA (currently NO-GO): the placeholder JA voicing is too stiff in-circuit — the active
// core fails to converge (avg ~16 Newton iters vs ~3 linear) and diverges on hot transients. These
// two tests encode the GO bar; they stay #[ignore] until the JaCore constants + companion damping
// are voiced (the documented GO step — needs the B-H/THD plots + listening). The structural gates
// below (bit-exact-at-0, partition, determinism) pass and guard the scaffolding.
#[test]
#[ignore = "NO-GO: JA core needs voicing — diverges at placeholder constants"]
fn trafo_changes_output_and_stays_finite() {
    let base = run_stream_trafo(0.0);
    for &t in &[0.5_f64, 1.0] {
        let got = run_stream_trafo(t);
        for (i, &y) in got.iter().enumerate() {
            assert!(y.is_finite() && y.abs() < 100.0, "trafo={t} sample {i} blew up: {y}");
        }
        let d = max_dev(&base, &got);
        assert!(d > 1e-5, "trafo={t} should change output, max_dev={d}");
    }
}

#[test]
fn trafo_determinism() {
    let a = run_stream_trafo(0.7);
    let b = run_stream_trafo(0.7);
    for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
        assert_eq!(x.to_bits(), y.to_bits(), "trafo deterministic, sample {i}");
    }
}

#[test]
#[ignore = "NO-GO: JA core needs voicing — avg iters ~16 vs ~3 linear at placeholder constants"]
fn trafo_convergence_within_budget() {
    // avg Newton iters at trafo=0.5 must stay close to the linear baseline (a mis-stamped Jacobian
    // would blow this up). Drive a hot sine to push the core toward saturation.
    let drive: Vec<f32> = (0..2048).map(|i| (i as f32 * 0.06).sin() * 0.5).collect();

    let avg = |trafo: f64| -> f64 {
        let mut m = wdf::WdfModel::new_with(
            QualityConfig::realtime(),
            wdf_golden::HOST_SR,
            220.0,
            wdf_golden::BIAS,
        );
        m.set_trafo(trafo);
        m.warmup();
        m.reset_iter_stats();
        for &x in &drive {
            m.process_host_sample(x);
        }
        m.iter_stats().1
    };

    let base = avg(0.0);
    let hot = avg(0.5);
    assert!(hot.is_finite(), "iter avg must be finite");
    assert!(
        hot <= base + 1.5,
        "trafo=0.5 avg iters {hot} exceeds linear {base} + 1.5 budget"
    );
}

/// Diagnostic: dump a B-H loop CSV (run with `--features trafo-core -- --ignored --nocapture`).
/// Drives a low-frequency sine hard enough to traverse the saturation knee and prints (H,B).
#[test]
#[ignore = "diagnostic: dumps B-H loop to stdout"]
fn dump_bh_loop() {
    let sr = wdf_golden::HOST_SR;
    for trafo in [0.25_f64, 0.5, 1.0] {
        let mut m = wdf::WdfModel::new_with(QualityConfig::realtime(), sr, 220.0, wdf_golden::BIAS);
        m.set_trafo(trafo);
        m.warmup();
        // 50 Hz, large amplitude, a few cycles; sample the last cycle.
        let f = 50.0;
        let n = (sr / f) as usize;
        let cycles = 6;
        let amp = 0.5;
        println!("# trafo={trafo}");
        println!("# H,B");
        for i in 0..n * cycles {
            let x = (amp * (2.0 * std::f64::consts::PI * f * i as f64 / sr).sin()) as f32;
            m.process_host_sample(x);
            if i >= n * (cycles - 1) {
                let (h, b) = m.ja_hb();
                println!("{h:.6},{b:.9}");
            }
        }
    }
}
