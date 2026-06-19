//! A WDF tier/OS switch must be ALLOC-FREE on the audio thread. The whole-model rebuild (which
//! allocates the depletion-charge LUTs + circuit-build scratch) now runs on the background worker;
//! `process` only pointer-swaps the worker-built models in and parks the retired ones for an
//! off-thread drop. These tests drive the worker body + the audio-thread consume swap directly (the
//! exact paths `process` runs) so the alloc-freedom is gated deterministically without a host.
//!
//! Own test binary so it can install the `assert_no_alloc` global allocator without conflicting with
//! the other test files. The dep is built with `warn_debug`/`warn_release`, so an allocation inside
//! the guarded closure bumps `violation_count()` (kept active in `--release`) rather than aborting.
use assert_no_alloc::*;
use ba283::wdf::QualityConfig;
use ba283::Ba283Plugin;

#[global_allocator]
static A: AllocDisabler = AllocDisabler;

/// Build the new model(s) off-thread (worker), then consume the swap on the "audio thread" under
/// `assert_no_alloc`. The consume must do ZERO heap traffic — no alloc AND no free (the retired
/// models are parked for the worker to drop, not dropped here).
#[test]
fn tier_switch_consume_is_alloc_free() {
    let mut p = Ba283Plugin::default();
    // Arm a Warp switch (lossy tier => builds the depletion LUTs; + the hi sibling when adaptive-os
    // is compiled in). Worker builds + publishes (allocates — off the audio thread).
    p.force_arm_rebuild(QualityConfig::warp(), 220.0, 0.5);
    p.force_run_pending_rebuild();

    reset_violation_count();
    let consumed = assert_no_alloc(|| p.force_consume_rebuild());
    assert!(consumed, "worker published models but the consume swap did not happen");
    assert_eq!(
        violation_count(),
        0,
        "tier/OS switch consume allocated (or freed) on the audio thread"
    );
}

/// Repeated switches reuse the pre-allocated boxes + the retire ring (the worker drains it on each
/// build), so a sweep stays alloc-free across many consumes.
#[test]
fn repeated_switch_consumes_stay_alloc_free() {
    let mut p = Ba283Plugin::default();
    let cfgs = [QualityConfig::fast(), QualityConfig::ultra_fast(), QualityConfig::warp()];

    reset_violation_count();
    for _ in 0..8 {
        for cfg in cfgs {
            // Worker leg (allocates off-thread).
            p.force_arm_rebuild(cfg, 220.0, 0.5);
            p.force_run_pending_rebuild();
            // Audio-thread leg (must be alloc-free).
            let consumed = assert_no_alloc(|| p.force_consume_rebuild());
            assert!(consumed, "consume missed a published build");
        }
    }
    assert_eq!(violation_count(), 0, "a repeated tier/OS switch allocated on the audio thread");
}
