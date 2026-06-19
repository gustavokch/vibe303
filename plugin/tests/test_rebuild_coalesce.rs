//! Gen-stamped coalescing of the off-thread tier/OS rebuild. A fast tier/OS sweep bumps the request
//! `gen` each switch; the audio thread must consume ONLY the build it is currently waiting for and
//! ignore (drop) any superseded build. These are pure-logic tests of that handoff — no allocator
//! guard (so they live in their own binary, away from `test_rebuild_noalloc`'s `#[global_allocator]`,
//! which would otherwise force alloc discipline on the off-thread build legs here too).
use ba283::wdf::QualityConfig;
use ba283::Ba283Plugin;

/// No pending request ⇒ consume is a harmless no-op, not a panic.
#[test]
fn consume_with_nothing_pending_is_a_noop() {
    let mut p = Ba283Plugin::default();
    assert!(!p.force_consume_rebuild(), "consume returned true with no pending rebuild");
}

/// A build published for gen N must NOT be consumed once the pending request has advanced to gen N+1
/// (the stale set is ignored, then dropped by the next worker run). The newer build, once run, lands.
#[test]
fn superseded_build_is_not_consumed() {
    let mut p = Ba283Plugin::default();

    // gen1: arm + run the worker (publishes ready_gen = gen1).
    p.force_arm_rebuild(QualityConfig::fast(), 220.0, 0.5);
    p.force_run_pending_rebuild();

    // A second switch arrives before gen1 is consumed: pending advances to gen2 (newer `gen`).
    p.force_arm_rebuild(QualityConfig::warp(), 220.0, 0.5);

    // Consume must reject the stale gen1 build (ready_gen lags the pending gen2).
    assert!(
        !p.force_consume_rebuild(),
        "consumed a superseded (gen1) build while pending had advanced to gen2"
    );

    // Run the worker for the current pending (gen2); now the gens match and the swap lands.
    p.force_run_pending_rebuild();
    assert!(p.force_consume_rebuild(), "the latest (gen2) build was not consumed after its worker run");

    // Drained: nothing left pending.
    assert!(!p.force_consume_rebuild(), "a second consume swapped again with nothing pending");
}

/// Re-arming before running the worker keeps only the latest request pending; a single worker run +
/// consume converges (older arm is overwritten, never built or installed).
#[test]
fn rapid_rearm_converges_to_latest() {
    let mut p = Ba283Plugin::default();

    // Two switches back-to-back with no worker run between: pending = the latest only.
    p.force_arm_rebuild(QualityConfig::fast(), 220.0, 0.5);
    p.force_arm_rebuild(QualityConfig::warp(), 220.0, 0.5);

    // Nothing built yet ⇒ consume is a no-op.
    assert!(!p.force_consume_rebuild(), "consumed before any worker run");

    // One worker run builds the latest pending; consume lands it.
    p.force_run_pending_rebuild();
    assert!(p.force_consume_rebuild(), "the coalesced latest build did not land");
    assert!(!p.force_consume_rebuild(), "consume repeated with nothing pending");
}
