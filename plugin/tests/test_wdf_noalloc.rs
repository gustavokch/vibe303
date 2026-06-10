//! Enforces that `WdfModel::process_host_sample` performs zero heap allocation (real-time-safety
//! invariant for the audio thread). `build`/`warmup` allocate (Vec scratch in the builder) but run
//! off the audio thread — only the per-sample path must be alloc-free.
//!
//! Own test binary so it can install the global allocator without conflicting with other test
//! files. `assert_no_alloc` is built with `warn_debug`/`warn_release`, so an allocation inside the
//! guarded closure increments `violation_count()` instead of aborting; we then assert it is zero.
use assert_no_alloc::*;
use ba283::wdf::{QualityConfig, WdfModel};

#[global_allocator]
static A: AllocDisabler = AllocDisabler;

fn assert_stream_alloc_free(mut m: WdfModel) {
    // Bias value is irrelevant to the allocation check. Skip the full ~5s warmup: prime a few
    // samples outside the guard (covers any first-call effects), then guard the steady path. An
    // unsettled circuit also exercises the gmin-homotopy fallback, widening branch coverage.
    for i in 0..32 {
        let _ = m.process_host_sample((i as f32 * 0.01).sin());
    }
    reset_violation_count();
    assert_no_alloc(|| {
        for i in 0..1024 {
            let _ = m.process_host_sample((i as f32 * 0.001).sin());
        }
    });
    assert_eq!(
        violation_count(),
        0,
        "WdfModel::process_host_sample allocated on the heap inside assert_no_alloc"
    );
}

#[test]
fn process_host_sample_is_alloc_free() {
    assert_stream_alloc_free(WdfModel::new(48_000.0, 220.0, 0.5));
}

#[test]
fn fast_tier_process_host_sample_is_alloc_free() {
    // Fast tier exercises the Schur solver + predictor + freeze-caps + ls-trim scratch — all of
    // which must be fixed-size (no heap on the per-sample path).
    assert_stream_alloc_free(WdfModel::new_with(QualityConfig::fast(), 48_000.0, 220.0, 0.5));
}

#[cfg(feature = "trafo-core")]
#[test]
fn set_trafo_and_core_is_alloc_free() {
    // The separable RK4 J-A core runs on the per-sample path (ja_commit) and set_trafo flips a gate
    // block-rate. Both must be heap-free: fixed-size scalar state, no partition rebuild.
    let mut m = WdfModel::new(48_000.0, 220.0, 0.5);
    m.set_trafo(0.5);
    for i in 0..32 {
        let _ = m.process_host_sample((i as f32 * 0.01).sin());
    }
    reset_violation_count();
    assert_no_alloc(|| {
        for k in 0..8 {
            m.set_trafo(k as f64 / 8.0);
        }
        for i in 0..1024 {
            let _ = m.process_host_sample((i as f32 * 0.001).sin());
        }
    });
    assert_eq!(violation_count(), 0, "set_trafo / JA RK4 core allocated on the heap");
}

#[test]
fn set_age_is_alloc_free() {
    // Aging is applied block-rate from the audio callback (process), so set_age / set_age_seed must
    // not heap-allocate. Both re-derive conductances + rebuild G0/Schur in fixed-size scratch.
    let mut m = WdfModel::new(48_000.0, 220.0, 0.5);
    for i in 0..16 {
        let _ = m.process_host_sample((i as f32 * 0.01).sin());
    }
    reset_violation_count();
    assert_no_alloc(|| {
        m.set_age_seed(0x1357);
        for k in 0..8 {
            m.set_age(k as f64 / 8.0);
        }
    });
    assert_eq!(violation_count(), 0, "set_age/set_age_seed allocated on the heap");
}
