//! Realtime-safety gate: the voice's per-sample hot path must never allocate.
//! `AllocDisabler` (warn features) counts any allocation inside `assert_no_alloc`.
//!
//! deferred: this gates `voice.tick` only. The full RT path (`process()` event
//! drain + `buffer.iter_samples()`) gets its own gate once a nih-plug test-buffer
//! harness exists; the wrapper layers above `voice.tick` are nih-plug-owned.

use assert_no_alloc::*;
use plugin303::dsp::voice::Voice;

#[global_allocator]
static ALLOC: AllocDisabler = AllocDisabler;

#[test]
fn voice_hot_path_is_alloc_free() {
    let mut voice = Voice::new(48_000.0);
    voice.note_on(57); // setup (may allocate) happens before the guarded region
    reset_violation_count();
    assert_no_alloc(|| {
        let mut acc = 0.0f32;
        for _ in 0..4_096 {
            acc += voice.tick();
        }
        core::hint::black_box(acc);
    });
    assert_eq!(violation_count(), 0, "voice.tick allocated on the audio path");
}
