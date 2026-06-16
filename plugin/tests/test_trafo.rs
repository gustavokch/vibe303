//! Spike tests for the nonlinear Jiles-Atherton transformer core (`trafo` knob).
//! Only built/run under `--features trafo-core`. The whole file is feature-gated so the default
//! build is untouched.
#![cfg(feature = "trafo-core")]

use ba283::wdf::{self, QualityConfig};

#[allow(dead_code)]
mod wdf_golden {
    include!("fixtures/wdf_golden.rs");
}

/// Legacy-depth shim: the model now exposes an ON/OFF switch + two voicing dials (HYST, SAT) instead
/// of the single `trafo` knob. These spike tests treat the old 0..1 `trafo` value as SATURATION depth
/// (0 => OFF/linear; >0 => ON, knee driven by `sat`, hysteresis held at the default), which preserves
/// their original semantics (more `trafo` = more saturation, 0 = bit-exact linear).
fn apply_trafo(m: &mut wdf::WdfModel, trafo: f64) {
    if trafo > 0.0 {
        m.set_trafo_enabled(true);
        m.set_trafo_voicing(0.5, trafo);
    } else {
        m.set_trafo_enabled(false);
    }
}

fn run_stream_trafo(trafo: f64) -> Vec<f32> {
    let mut m =
        wdf::WdfModel::new_with(QualityConfig::realtime(), wdf_golden::HOST_SR, 220.0, wdf_golden::BIAS);
    apply_trafo(&mut m, trafo);
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
    apply_trafo(&mut m, 0.0);
    assert_eq!(m.n_sz(), 6, "trafo=0 must keep N-set at 6");
    m.warmup();
    let got: Vec<f32> =
        wdf_golden::INPUT_256.iter().map(|&x| m.process_host_sample(x as f32)).collect();
    for (i, (a, b)) in base.iter().zip(got.iter()).enumerate() {
        assert_eq!(a.to_bits(), b.to_bits(), "trafo=0 sample {i} not bit-identical");
    }
}

#[test]
fn trafo_active_keeps_partition() {
    // Separable RK4 re-architecture: the J-A core is a post-solve lagged current source, NOT a
    // Jacobian contribution, so activating it must NOT touch the Schur N-set (stays 6 always).
    // This is the structural invariant that makes the old +13-iter blow-up impossible by design.
    let mut m =
        wdf::WdfModel::new_with(QualityConfig::realtime(), wdf_golden::HOST_SR, 220.0, wdf_golden::BIAS);
    apply_trafo(&mut m, 0.5);
    assert_eq!(m.n_sz(), 6, "active JA core (decoupled) must keep N-set 6");
    apply_trafo(&mut m, 0.0);
    assert_eq!(m.n_sz(), 6, "deactivating must keep N-set 6");
}

// GO-CRITERIA (now GO under the separable-RK4 re-architecture): the J-A core is a decoupled
// post-solve filter injecting a lagged current source, NOT a Jacobian contribution, so it cannot
// perturb Newton convergence (gate below) and the gapped-core voicing keeps the magnetizing
// deviation bounded (no divergence). These two tests encode the GO bar and now pass.
#[test]
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
fn trafo_reenable_reseeds_core_no_flux_jump() {
    // Re-enabling the knob mid-stream must NOT integrate flux against a stale `ja_vm_prev` frozen
    // from before it was switched off (would jump the flux → audible click). `set_trafo_enabled` re-seeds
    // the core on the inactive→active edge, so the operating point is clean (H,B)=(0,0) right after.
    let mut m = wdf::WdfModel::new_with(
        QualityConfig::realtime(),
        wdf_golden::HOST_SR,
        220.0,
        wdf_golden::BIAS,
    );
    apply_trafo(&mut m, 0.5);
    m.warmup();
    // Drive a hot tone to push the core off the origin (builds up ja_vm_prev / flux / M and a large
    // lagged `ja_i_dev`).
    for i in 0..512 {
        m.process_host_sample((i as f32 * 0.06).sin() * 0.5);
    }
    let (h_hot, b_hot) = m.ja_hb();
    assert!(h_hot != 0.0 || b_hot != 0.0, "core should be off-origin after hot drive");
    // The lagged deviation current is non-trivial here — exactly the stale value a re-enable must
    // not carry over.
    assert!(m.ja_i_dev() != 0.0, "hot drive should leave a non-zero lagged deviation current");

    // Switch off (freezes the stale state, incl. `ja_i_dev`), run linear, then re-enable.
    apply_trafo(&mut m, 0.0);
    for _ in 0..64 {
        m.process_host_sample(0.0);
    }
    apply_trafo(&mut m, 0.5);
    // Immediately after the re-enable edge, before any further commit, the core must be re-seeded:
    // clean operating point AND — the actual click source — a zeroed lagged deviation current. A
    // stale `ja_i_dev` would be injected on N_PA's RHS next sample as a one-step current jump.
    let (h0, b0) = m.ja_hb();
    assert_eq!((h0, b0), (0.0, 0.0), "re-enable must reseed core to origin, got ({h0},{b0})");
    assert_eq!(m.ja_i_dev(), 0.0, "re-enable must zero the lagged deviation current (click source)");

    // And the first samples after re-enable stay finite/bounded (no flux-jump blow-up).
    for i in 0..256 {
        let y = m.process_host_sample((i as f32 * 0.06).sin() * 0.5);
        assert!(y.is_finite() && y.abs() < 100.0, "post-reenable sample {i} blew up: {y}");
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
fn trafo_convergence_within_budget() {
    // THE DECISIVE GO SIGNAL. avg Newton iters at trafo=0.5 must stay ≈ the linear baseline. With
    // the J-A decoupled from the global solve (lagged current source, not a Jacobian block) this is
    // guaranteed by construction — the old in-loop stamp blew this to ~16 iters vs ~3 (NO-GO).
    // Drive a hot sine to push the core toward saturation.
    let drive: Vec<f32> = (0..2048).map(|i| (i as f32 * 0.06).sin() * 0.5).collect();

    let avg = |trafo: f64| -> f64 {
        let mut m = wdf::WdfModel::new_with(
            QualityConfig::realtime(),
            wdf_golden::HOST_SR,
            220.0,
            wdf_golden::BIAS,
        );
        apply_trafo(&mut m, trafo);
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

/// THD of the steady-state output for a pure sine at frequency bin `cycles/n`, via fundamental
/// projection (no FFT crate): THD = sqrt(total_ac_rms^2 − fund_rms^2) / fund_rms.
fn thd(trafo: f64, level: f32) -> f64 {
    let sr = wdf_golden::HOST_SR;
    let n = 8192usize;
    let cycles = (100.0 * n as f64 / sr).round(); // ≈100 Hz, exact DFT bin
    let f0 = sr * cycles / n as f64;
    let mut m =
        wdf::WdfModel::new_with(QualityConfig::realtime(), sr, 220.0, wdf_golden::BIAS);
    apply_trafo(&mut m, trafo);
    m.warmup();
    let omega = 2.0 * std::f64::consts::PI * f0 / sr;
    // settle transient
    for i in 0..4096 {
        m.process_host_sample((level as f64 * (omega * i as f64).sin()) as f32);
    }
    let mut y = vec![0.0f64; n];
    for (i, yi) in y.iter_mut().enumerate() {
        let x = (level as f64 * (omega * (i + 4096) as f64).sin()) as f32;
        *yi = m.process_host_sample(x) as f64;
    }
    let mean = y.iter().sum::<f64>() / n as f64;
    for yi in &mut y {
        *yi -= mean;
    }
    let k = cycles;
    let (mut ar, mut ai) = (0.0f64, 0.0f64);
    for (i, &yi) in y.iter().enumerate() {
        let ph = 2.0 * std::f64::consts::PI * k * i as f64 / n as f64;
        ar += yi * ph.cos();
        ai += yi * ph.sin();
    }
    let fund_amp = 2.0 / n as f64 * (ar * ar + ai * ai).sqrt();
    let fund_rms = fund_amp / std::f64::consts::SQRT_2;
    let total_rms = (y.iter().map(|v| v * v).sum::<f64>() / n as f64).sqrt();
    let resid = (total_rms * total_rms - fund_rms * fund_rms).max(0.0).sqrt();
    resid / fund_rms.max(1e-12)
}

#[test]
fn trafo_thd_rises_with_level_and_knob() {
    // Plausibility (no nonlinear ground truth): core saturation must (a) increase THD with drive
    // level and (b) at a hot level, increase THD as `trafo` opens. Compared with a margin so circuit
    // (BJT) distortion alone doesn't carry the assertion.
    let thd_lo = thd(0.6, 0.05);
    let thd_hi = thd(0.6, 0.5);
    assert!(thd_hi > thd_lo * 1.05, "THD must rise with level: lo={thd_lo:.5} hi={thd_hi:.5}");

    let thd_off = thd(0.0, 0.5);
    let thd_on = thd(0.8, 0.5);
    assert!(
        thd_on > thd_off * 1.02,
        "THD must rise as trafo opens at hot level: off={thd_off:.5} on={thd_on:.5}"
    );
}

/// THD with the core ON and the two dials set explicitly (vs `thd`, which maps a single legacy depth).
fn thd_voiced(hyst: f64, sat: f64, level: f32) -> f64 {
    let sr = wdf_golden::HOST_SR;
    let n = 8192usize;
    let cycles = (100.0 * n as f64 / sr).round();
    let f0 = sr * cycles / n as f64;
    let mut m = wdf::WdfModel::new_with(QualityConfig::realtime(), sr, 220.0, wdf_golden::BIAS);
    m.set_trafo_enabled(true);
    m.set_trafo_voicing(hyst, sat);
    m.warmup();
    let omega = 2.0 * std::f64::consts::PI * f0 / sr;
    for i in 0..4096 {
        m.process_host_sample((level as f64 * (omega * i as f64).sin()) as f32);
    }
    let mut y = vec![0.0f64; n];
    for (i, yi) in y.iter_mut().enumerate() {
        let x = (level as f64 * (omega * (i + 4096) as f64).sin()) as f32;
        *yi = m.process_host_sample(x) as f64;
    }
    let mean = y.iter().sum::<f64>() / n as f64;
    for yi in &mut y {
        *yi -= mean;
    }
    let (mut ar, mut ai) = (0.0f64, 0.0f64);
    for (i, &yi) in y.iter().enumerate() {
        let ph = 2.0 * std::f64::consts::PI * cycles * i as f64 / n as f64;
        ar += yi * ph.cos();
        ai += yi * ph.sin();
    }
    let fund_rms = 2.0 / n as f64 * (ar * ar + ai * ai).sqrt() / std::f64::consts::SQRT_2;
    let total_rms = (y.iter().map(|v| v * v).sum::<f64>() / n as f64).sqrt();
    (total_rms * total_rms - fund_rms * fund_rms).max(0.0).sqrt() / fund_rms.max(1e-12)
}

#[test]
fn trafo_dials_voice_independently() {
    // The two dials must move SEPARATE behaviors of the core (the point of splitting the knob):
    //   • SATURATION sets the knee — higher `sat` => earlier knee => more THD at a hot level.
    //   • HYSTERESIS sets the loop width — changing it must move the output, and at HYST=0 the loop
    //     collapses (near-anhysteretic), so its distortion is below a wide-loop setting.
    let level = 0.5;

    // SATURATION sweep at fixed (default) hysteresis.
    let sat_clean = thd_voiced(0.5, 0.15, level);
    let sat_hard = thd_voiced(0.5, 0.9, level);
    assert!(
        sat_hard > sat_clean * 1.05,
        "SATURATION must raise THD as the knee tightens: clean={sat_clean:.5} hard={sat_hard:.5}"
    );

    // HYSTERESIS sweep at fixed (default) saturation must change the rendered stream.
    let lo = run_stream_voiced(0.0, 0.5);
    let hi = run_stream_voiced(1.0, 0.5);
    assert!(
        max_dev(&lo, &hi) > 1e-5,
        "HYSTERESIS dial must change the output independent of saturation"
    );

    // Every grid cell stays finite/bounded (no flux runaway at any dial combination).
    for &h in &[0.0_f64, 0.5, 1.0] {
        for &s in &[0.0_f64, 0.5, 1.0] {
            for (i, &y) in run_stream_voiced(h, s).iter().enumerate() {
                assert!(y.is_finite() && y.abs() < 100.0, "hyst={h} sat={s} sample {i} blew up: {y}");
            }
        }
    }
}

fn run_stream_voiced(hyst: f64, sat: f64) -> Vec<f32> {
    let mut m =
        wdf::WdfModel::new_with(QualityConfig::realtime(), wdf_golden::HOST_SR, 220.0, wdf_golden::BIAS);
    m.set_trafo_enabled(true);
    m.set_trafo_voicing(hyst, sat);
    m.warmup();
    wdf_golden::INPUT_256.iter().map(|&x| m.process_host_sample(x as f32)).collect()
}

/// Diagnostic: dump a B-H loop CSV (run with `--features trafo-core -- --ignored --nocapture`).
/// Drives a low-frequency sine hard enough to traverse the saturation knee and prints (H,B).
#[test]
#[ignore = "diagnostic: dumps B-H loop to stdout"]
fn dump_bh_loop() {
    let sr = wdf_golden::HOST_SR;
    for trafo in [0.25_f64, 0.5, 1.0] {
        let mut m = wdf::WdfModel::new_with(QualityConfig::realtime(), sr, 220.0, wdf_golden::BIAS);
        apply_trafo(&mut m, trafo);
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
