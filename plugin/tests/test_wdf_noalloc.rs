//! Enforces that `WdfModel::process_host_sample` performs zero heap allocation (real-time-safety
//! invariant for the audio thread). `build`/`warmup` allocate (Vec scratch in the builder) but run
//! off the audio thread — only the per-sample path must be alloc-free.
//!
//! Own test binary so it can install the global allocator without conflicting with other test
//! files. `assert_no_alloc` is built with `warn_debug`/`warn_release`, so an allocation inside the
//! guarded closure increments `violation_count()` instead of aborting; we then assert it is zero.
use assert_no_alloc::*;
use ba283::wdf::WdfModel;

#[global_allocator]
static A: AllocDisabler = AllocDisabler;

#[test]
fn process_host_sample_is_alloc_free() {
    // Bias value is irrelevant to the allocation check. Skip the full ~5s warmup: prime a few
    // samples outside the guard (covers any first-call effects), then guard the steady path. An
    // unsettled circuit also exercises the gmin-homotopy fallback, widening branch coverage.
    let mut m = WdfModel::new(48_000.0, 220.0, 0.5);
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
