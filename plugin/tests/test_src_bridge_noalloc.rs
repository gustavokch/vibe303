//! Enforces that `SrcBridge::process` performs zero heap allocation in steady state (the
//! real-time-safety invariant). All polyphase banks and history rings are allocated once in
//! `prepare` (init-time); the audio path must only index them. Own test binary so it can install
//! the global allocator without conflicting with the other test files (see `test_wavenet_noalloc.rs`
//! for the warn_debug/warn_release rationale).
use assert_no_alloc::*;
use ba283::oversample::SrcBridge;

#[global_allocator]
static A: AllocDisabler = AllocDisabler;

#[test]
fn process_is_alloc_free_every_rate() {
    for &host in &[44_100u32, 64_000, 88_200, 96_000, 176_400, 192_000] {
        let mut b = SrcBridge::new();
        b.prepare(host, 48_000); // all allocation happens here (init-time)
        for _ in 0..64 {
            let _ = b.process(0.0, |lo| lo);
        }
        reset_violation_count();
        assert_no_alloc(|| {
            for i in 0..4096 {
                let x = (i as f32 * 0.001).sin();
                let _ = b.process(x, |lo| lo * 0.5);
            }
        });
        assert_eq!(
            violation_count(),
            0,
            "SrcBridge::process allocated on the heap at host {host}"
        );
    }
}
