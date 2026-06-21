use super::*;

#[test]
fn precision_high_is_identity() {
    // HIGH precision MUST be the identity modifier on every tier — this is what keeps
    // Realtime+High bit-identical to the golden reference. Field-equality is exact (the struct
    // derives PartialEq), so `with_precision(High)` returning `self` is fully verified here.
    for base in [
        QualityConfig::realtime(),
        QualityConfig::fast(),
        QualityConfig::ultra_fast(),
        QualityConfig::warp(),
    ] {
        assert_eq!(
            base,
            base.with_precision(Precision::High),
            "with_precision(High) must not alter the tier config"
        );
        // Precision never enables fixed_step (device-model-only); HIGH especially must not.
        assert!(!base.with_precision(Precision::High).fixed_step);
    }
}

#[test]
fn precision_med_low_step_device_model() {
    // Precision is the DEVICE-MODEL axis: Med/Low step `cap_tier`/`cheap_exp` down and touch
    // NOTHING else — not the iteration strategy (`fixed_step`/`predict`/`max_iters`) nor the
    // oversampling rate (`os`/`decimate`). This is what removed the os=1 perverse-slowdown bug.
    for base in [
        QualityConfig::realtime(),
        QualityConfig::fast(),
        QualityConfig::ultra_fast(),
        QualityConfig::warp(),
    ] {
        for p in [Precision::Medium, Precision::Low] {
            let c = base.with_precision(p);
            assert!(!c.fixed_step, "{p:?} must NOT enable fixed_step (iteration axis untouched)");
            assert_eq!(c.predict, base.predict, "{p:?} must not change predict");
            assert_eq!(c.max_iters, base.max_iters, "{p:?} must not change max_iters");
            assert_eq!(c.os, base.os, "{p:?} must not change the oversampling rate");
            assert_eq!(c.decimate, base.decimate, "{p:?} must not change decimate");
            assert!(c.cheap_exp, "{p:?} must enable the cheap transport exp");
        }
        // Medium drops to Depletion (never *up*grading a tier already at Off, e.g. warp).
        let med = base.with_precision(Precision::Medium).cap_tier;
        if base.cap_tier == CapTier::Off {
            assert_eq!(med, CapTier::Off, "Medium must not upgrade a tier already at Off");
        } else {
            assert_eq!(med, CapTier::Depletion, "Medium drops the charge model to Depletion");
        }
        // Low drops the charge model entirely.
        assert_eq!(base.with_precision(Precision::Low).cap_tier, CapTier::Off);
    }
}

#[test]
fn precision_med_low_finite_and_within_budget() {
    // Behavioral guard for the Medium/Low device-model downgrade — the config field-mapping is
    // covered by `precision_med_low_step_device_model`; this drives the actual `process()` path:
    //   (1) a hot, clipping-level input must never yield non-finite output. Med/Low keep full
    //       convergence (precision no longer caps iterations), so the gmin-homotopy + H1 guard in
    //       `process` both hold — a single NaN/Inf can never propagate through the forward state
    //       (`v_prev`/`hist`/cap+BJT currents).
    //   (2) on a representative swept tone the Medium/Low ESR loss vs Realtime+High must stay
    //       inside the −50 dB harness budget (the reduced charge model + cheap_exp is a small step).
    let host = 48_000.0;
    // Match the offline harness operating point (gain 0.5 → R_KJ 905.5 Ω, RV1 bias 0.77) so the
    // measured loss is comparable to the PR's documented −60 dB sweep point.
    let (rgain, bias) = (905.5, 0.77);

    // Warm once at the realtime tier (the DC op-point is tier-independent) and install the settled
    // snapshot into each model — the exact path the plugin uses, so we isolate audio-pass loss
    // from cold-start bias settling.
    let mut warm = WdfModel::new(host, rgain, bias);
    warm.warmup();
    let snap = warm.snapshot();
    let build = |cfg| {
        let mut m = WdfModel::new_with(cfg, host, rgain, bias);
        m.restore(&snap);
        m
    };
    let rt = QualityConfig::realtime();
    let mut high = build(rt);
    let mut med = build(rt.with_precision(Precision::Medium));
    let mut low = build(rt.with_precision(Precision::Low));

    // (1) Hot input: alternating rail-to-rail ±4.0 (well past clip) must stay finite on both
    // fixed-step tiers. Without the H1 guard a single divergent sample would poison every later
    // one; with it, output is always finite.
    for i in 0..1024 {
        let x = if i % 2 == 0 { 4.0 } else { -4.0 };
        assert!(med.process_host_sample(x).is_finite(), "Medium non-finite (hot) at {i}");
        assert!(low.process_host_sample(x).is_finite(), "Low non-finite (hot) at {i}");
    }

    // (2) Tracking guardrail: re-install the clean snapshot, then score an 80 Hz→2.5 kHz chirp
    // (integrated phase) at a musical level against Realtime+High, lag-aligned (the reduced device
    // model shifts the op-point slightly, adding a small group-delay — same reason the offline
    // harness carries a `lag` field). The authoritative −50 dB budget is the `eval/plugin_vs_acme`
    // sweep on the real `sweep_m3` corpus + full metric, which a unit test can't reproduce (a
    // broadband synthetic
    // chirp excites the reduced charge model harder, ≈−35 dB here). So this asserts the robust
    // property — Medium/Low TRACK Realtime+High (aligned ESR ≤ −25 dB), not collapsed/diverged —
    // with ~10 dB margin; a real regression (NaN, silence, instability) blows far past it.
    med.restore(&snap);
    low.restore(&snap);
    let n = 8192usize;
    let skip = 512usize; // drop the post-install transient before scoring
    let (f0, f1) = (80.0_f64, 2500.0_f64);
    let k = (f1 - f0) / (n as f64 / host); // chirp rate (Hz/s)
    let mut yh = vec![0.0f64; n];
    let mut ym = vec![0.0f64; n];
    let mut yl = vec![0.0f64; n];
    for i in 0..n {
        let t = i as f64 / host;
        let phase = std::f64::consts::TAU * (f0 * t + 0.5 * k * t * t); // integrated chirp phase
        let x = (0.25 * phase.sin()) as f32;
        yh[i] = high.process_host_sample(x) as f64;
        ym[i] = med.process_host_sample(x) as f64;
        yl[i] = low.process_host_sample(x) as f64;
    }
    // Lag-aligned ESR over the scored window: pick the integer shift (|lag| ≤ 8) of `test`
    // against `ref` that minimizes residual energy, then report that minimum vs the ref energy.
    let aligned_esr_db = |refs: &[f64], test: &[f64]| -> f64 {
        let mut best = f64::INFINITY;
        for lag in -8i64..=8 {
            let (mut e, mut sig) = (0.0f64, 0.0f64);
            for i in skip..n {
                let j = i as i64 + lag;
                if j < 0 || j as usize >= n {
                    continue;
                }
                let d = test[j as usize] - refs[i];
                e += d * d;
                sig += refs[i] * refs[i];
            }
            best = best.min(e / sig);
        }
        10.0 * best.log10()
    };
    let m_db = aligned_esr_db(&yh, &ym);
    let l_db = aligned_esr_db(&yh, &yl);
    assert!(m_db <= -25.0, "Medium ESR {m_db:.1} dB — fixed-step not tracking Realtime+High");
    assert!(l_db <= -25.0, "Low ESR {l_db:.1} dB — fixed-step not tracking Realtime+High");
}

#[test]
fn ultra_exp_within_budget() {
    // ultra_exp must stay within ~1e-4 relative over the transport-exp operating range
    // (v/vt up to ~vcrit/vt ~ 28 forward; reverse-biased args go very negative -> ~0).
    let mut worst = 0.0f64;
    let mut x = -40.0;
    while x <= 30.0 {
        let a = ultra_exp(x);
        let e = x.exp();
        if e > 1e-12 {
            worst = worst.max(((a - e) / e).abs());
        }
        x += 0.01;
    }
    assert!(worst < 1e-4, "ultra_exp worst rel err {worst:e} exceeds 1e-4 budget");
    // exponent guards match fast_exp (no garbage for very negative k).
    assert_eq!(ultra_exp(-1000.0), 0.0);
}

#[test]
fn ultra_fast_os1_no_floor() {
    // ultra-fast runs os=1 (its speed lever) and STAYS os=1 at every precision — the old
    // fixed_step os-floor (which made Med/Low slower than High) is gone. solve_os is now identity.
    let cfg = QualityConfig::ultra_fast();
    assert_eq!(cfg.os, 1, "ultra-fast runs os=1 (the speed lever)");
    assert!(!cfg.decimate, "ultra-fast bypasses decimation (nothing to decimate at os=1)");
    assert!(cfg.cheap_exp, "ultra-fast keeps the cheap transport exp lever");
    assert!(cfg.predict && cfg.solver == Solver::Schur && cfg.freeze_caps,
            "ultra-fast inherits the fast-tier levers");
    assert_eq!(cfg.solve_os(), 1, "solve_os is the identity now: os=1 stands");
    assert!(!cfg.solve_decimate(), "os=1 -> no decimation");

    // Med/Low keep os=1 — precision is device-model-only, never floors the rate.
    for p in [Precision::Medium, Precision::Low] {
        let c = cfg.with_precision(p);
        assert_eq!(c.solve_os(), 1, "{p:?}: precision must NOT floor os");
        assert!(!c.solve_decimate(), "{p:?}: still os=1, no decimation");
        let m = WdfModel::new_with(c, 48_000.0, 220.0, 0.5);
        assert_eq!(m.os, 1, "{p:?}: built model must still solve at os=1");
        assert!(!m.decimate, "{p:?}: built model must not decimate at os=1");
    }
    // ultra-fast produces finite output (no NaN/instability) on a quick ramp.
    let mut m = WdfModel::new_with(cfg, 48_000.0, 220.0, 0.5);
    assert_eq!(m.os, 1);
    for i in 0..256 {
        let y = m.process_host_sample((i as f32 / 256.0) * 0.4 - 0.2);
        assert!(y.is_finite(), "ultra-fast output non-finite at sample {i}");
    }
}

#[test]
fn oversampling_dial_overrides_tier() {
    use crate::wdf::Oversampling;
    // Auto defers to the tier; X1/X2 force the rate (and the matching decimate setting).
    for base in [
        QualityConfig::realtime(),
        QualityConfig::fast(),
        QualityConfig::ultra_fast(),
        QualityConfig::warp(),
    ] {
        assert_eq!(base, base.with_oversampling(Oversampling::Auto), "Auto must be identity");

        let x1 = base.with_oversampling(Oversampling::X1);
        assert_eq!(x1.os, 1, "X1 forces os=1");
        assert!(!x1.decimate, "X1 bypasses decimation");

        let x2 = base.with_oversampling(Oversampling::X2);
        assert_eq!(x2.os, 2, "X2 forces os=2");
        assert!(x2.decimate, "X2 enables the antialiasing decimator");

        // Built models honour the forced rate.
        assert_eq!(WdfModel::new_with(x1, 48_000.0, 220.0, 0.5).os, 1);
        assert_eq!(WdfModel::new_with(x2, 48_000.0, 220.0, 0.5).os, 2);
    }
}

#[test]
fn warp_drops_charge_model_only() {
    let cfg = QualityConfig::warp();
    // It is ultra-fast with cap_tier=Off and nothing else changed.
    assert_eq!(cfg.cap_tier, CapTier::Off, "must drop the BJT charge model");
    assert_eq!(cfg.os, 1, "inherits ultra-fast os=1 (High speed lever)");
    assert!(!cfg.decimate, "inherits ultra-fast os=1 decimation bypass");
    assert!(cfg.predict && cfg.solver == Solver::Schur && cfg.freeze_caps,
            "inherits the fast-tier levers");
    // warp reaches cap_tier=Off via the tier, keeping full convergence (no fixed_step at all).
    assert!(!cfg.fixed_step, "warp keeps the convergence gate");
    assert_eq!(cfg, QualityConfig { cap_tier: CapTier::Off, ..QualityConfig::ultra_fast() });
    // Finite, stable output on a quick ramp.
    let mut m = WdfModel::new_with(cfg, 48_000.0, 220.0, 0.5);
    for i in 0..256 {
        let y = m.process_host_sample((i as f32 / 256.0) * 0.4 - 0.2);
        assert!(y.is_finite(), "warp output non-finite at sample {i}");
    }
}

#[test]
fn measure_gain_finite_on_os1_tiers() {
    // measure_gain clones the model and sets fixed_step=true while keeping the tier's os — which
    // is 1 for UltraFast/Warp at High. That is the one place fixed_step runs at os=1 (the probe
    // bypasses solve_os deliberately; see its NOTE). Pin that this near-linear probe stays finite
    // and returns a sane positive gain, so a future change to the probe or the floor can't break it.
    for cfg in [QualityConfig::ultra_fast(), QualityConfig::warp()] {
        assert_eq!(cfg.os, 1, "precondition: tier runs os=1 at High");
        let mut m = WdfModel::new_with(cfg, 48_000.0, 220.0, 0.5);
        assert_eq!(m.os, 1, "built High model must solve at os=1");
        m.warmup();
        let g = m.measure_gain();
        assert!(g.is_finite() && g > 0.0, "os=1 probe gain must be finite and positive, got {g}");
        assert!(g < 1.0e3, "os=1 probe gain must stay in a sane band (no limit cycle), got {g}");
    }
}

#[test]
fn gain_probe_os1_tiers_track_high() {
    // After the 3-axis refactor, Med/Low build at the tier os (1 for UltraFast/Warp) instead of the
    // old floored os=2, so their levelNorm probe now runs at os=1 too. The probe is a 0.05 V
    // near-linear small-signal measurement, so the os=1/os=2 trapezoidal-warp difference at 1 kHz
    // must be negligible: Med/Low measured gain must track High closely. (Guards the documented
    // probe-rate behavior change in `measure_gain`'s NOTE — a regression here means the levelNorm
    // trim would jump audibly when only the precision dial moved.)
    for cfg in [QualityConfig::ultra_fast(), QualityConfig::warp()] {
        let mut hi = WdfModel::new_with(cfg.with_precision(Precision::High), 48_000.0, 220.0, 0.5);
        hi.warmup();
        let g_hi = hi.measure_gain();
        assert!(g_hi.is_finite() && g_hi > 0.0, "High probe gain must be sane, got {g_hi}");
        for p in [Precision::Medium, Precision::Low] {
            let mut m = WdfModel::new_with(cfg.with_precision(p), 48_000.0, 220.0, 0.5);
            assert_eq!(m.os, 1, "{p:?}: os=1 tier must stay os=1 (no floor)");
            m.warmup();
            let g = m.measure_gain();
            assert!(g.is_finite() && g > 0.0, "{p:?} probe gain must be sane, got {g}");
            // Device-model step at a near-linear probe level moves the gain only slightly; 5% is a
            // generous band that still catches an os-mismatch or limit-cycle (which would be >>1).
            let rel = (g - g_hi).abs() / g_hi;
            assert!(rel < 0.05, "{p:?} probe gain {g} must track High {g_hi} (rel {rel:.3} >= 0.05)");
        }
    }
}

#[test]
fn decimate_flag_changes_output() {
    // Guards against the `decimate` lever silently no-op'ing: at os=2 the decimation biquads are
    // the antialiasing decimator, so toggling them MUST change the output. (Same input, same
    // circuit, same seed — only `decimate` toggled.) The `decimate` flag gates the LEGACY Butterworth
    // decimator; the shipped FIR resampler (default) ignores it (it always resamples), so pin
    // `legacy_resample` here to exercise the lever this test is about.
    let host = 48_000.0;
    let mut base = QualityConfig::fast(); // os=2 tier; toggle the decimator only
    base.legacy_resample = true;
    base.decimate = false;
    let mut filt = base; // os=2, decimate=true
    filt.decimate = true;

    let mut m_off = WdfModel::new_with(base, host, 220.0, 0.5);
    let mut m_on = WdfModel::new_with(filt, host, 220.0, 0.5);
    // Warm both so the decimator acts on the real operating point, not the cold ramp.
    for _ in 0..256 {
        let _ = m_off.process_host_sample(0.0);
        let _ = m_on.process_host_sample(0.0);
    }
    let mut max_abs_diff = 0.0f32;
    for i in 0..1024 {
        // HOT, bright tone so the circuit nonlinearity generates HF harmonics above Nyquist for
        // the os=2 decimator to antialias (vs the naive every-other-subsample drop with decimate
        // off). A small near-linear tone produces too little HF for the biquads to matter.
        let x = (i as f32 * 0.7).sin() * 4.0;
        let d = (m_off.process_host_sample(x) - m_on.process_host_sample(x)).abs();
        max_abs_diff = max_abs_diff.max(d);
    }
    assert!(max_abs_diff > 1e-5,
            "decimate flag had no audible effect (max |Δ| {max_abs_diff:e}) — lever may be a no-op");
}

