//! Enforces that Ba283Model::forward performs zero heap allocation in steady
//! state (real-time-safety invariant). Guards against regressions when the
//! drive/output/mix/oversampling chain is wired in Tasks 3.3-3.4.
//!
//! This is its OWN test binary so it can install the global allocator without
//! conflicting with the other test files. The `assert_no_alloc` dep is built
//! with `warn_debug`/`warn_release` (see Cargo.toml), so an allocation inside
//! the guarded closure increments `violation_count()` instead of aborting the
//! whole process; we then assert the count is zero. This keeps the check active
//! in `--release` (the default `disable_release` would make it a no-op).
use assert_no_alloc::*;
use ba283::model::Ba283Model;

#[global_allocator]
static A: AllocDisabler = AllocDisabler;

#[test]
fn forward_is_alloc_free() {
    let mut model = Ba283Model::new_from_embedded_weights();
    // warm up (construction + first call may touch lazy statics)
    for _ in 0..16 {
        let _ = model.forward(&[0.0, 0.5]);
    }
    reset_violation_count();
    assert_no_alloc(|| {
        for i in 0..4096 {
            let x = (i as f32 * 0.001).sin();
            let _ = model.forward(&[x, 0.5]);
        }
    });
    assert_eq!(
        violation_count(),
        0,
        "Ba283Model::forward allocated on the heap inside assert_no_alloc"
    );
}
