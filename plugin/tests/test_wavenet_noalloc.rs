//! Enforces that WaveNet::process performs zero heap allocation in steady state (the real-time-safety
//! invariant). All weight/state storage is heap-`Box`ed once at construction; the audio path must
//! only index it. Own test binary so it can install the global allocator without conflicting with the
//! other test files (see `test_model_noalloc.rs` for the warn_debug/warn_release rationale).
use assert_no_alloc::*;
use ba283::wavenet::WaveNet;

#[global_allocator]
static A: AllocDisabler = AllocDisabler;

#[test]
fn process_is_alloc_free() {
    let mut net = WaveNet::new_from_embedded();
    // warm up (construction + first calls may touch lazy statics)
    for _ in 0..16 {
        let _ = net.process(0.0, &[0.5]);
    }
    reset_violation_count();
    assert_no_alloc(|| {
        for i in 0..4096 {
            let x = (i as f32 * 0.001).sin();
            let _ = net.process(x, &[0.5]);
        }
    });
    assert_eq!(
        violation_count(),
        0,
        "WaveNet::process allocated on the heap inside assert_no_alloc"
    );
}
