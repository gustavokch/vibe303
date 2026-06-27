//! Realtime-safety gate: the voice's per-sample hot path must never allocate.
//! `AllocDisabler` (warn features) counts any allocation inside `assert_no_alloc`.
//!
//! deferred: this gates `voice.tick` only. The full RT path (`process()` event
//! drain + `buffer.iter_samples()`) gets its own gate once a nih-plug test-buffer
//! harness exists; the wrapper layers above `voice.tick` are nih-plug-owned.

use assert_no_alloc::*;
use plugin303::dsp::filter::Filter303;
use plugin303::dsp::ladder_wdf::LadderVcf;
use plugin303::dsp::oversample::Os4x;
use plugin303::dsp::vco::Vco303;
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

#[test]
fn ladder_vcf_hot_path_is_alloc_free() {
    let mut f = LadderVcf::new(48_000.0); // setup (may allocate) before the guard
    f.set_cutoff(1_000.0);
    f.set_reso(0.3);
    for _ in 0..64 {
        f.tick(0.0); // warm-start the Newton state outside the guarded region
    }
    reset_violation_count();
    assert_no_alloc(|| {
        let mut acc = 0.0f32;
        for i in 0..4_096 {
            acc += f.tick((i as f32 * 0.01).sin() * 0.1);
        }
        core::hint::black_box(acc);
    });
    assert_eq!(violation_count(), 0, "LadderVcf::tick allocated on the audio path");
}

#[test]
fn vco_hot_path_is_alloc_free() {
    let mut osc = Vco303::new(48_000.0);
    osc.set_freq(231.0);
    for _ in 0..128 {
        osc.tick(); // self-start ramp + warm-start, outside the guard
    }
    reset_violation_count();
    assert_no_alloc(|| {
        let mut acc = 0.0f32;
        for _ in 0..4_096 {
            acc += osc.tick();
        }
        core::hint::black_box(acc);
    });
    assert_eq!(violation_count(), 0, "Vco303::tick allocated on the audio path");
}

#[test]
fn oversampler_hot_path_is_alloc_free() {
    let mut os = Os4x::new(); // coeff build (may allocate) happens before the guard
    reset_violation_count();
    assert_no_alloc(|| {
        let mut acc = 0.0f32;
        for i in 0..4_096 {
            let hi = os.up((i as f32 * 0.01).sin());
            acc += os.down(hi);
        }
        core::hint::black_box(acc);
    });
    assert_eq!(violation_count(), 0, "Os4x up/down allocated on the audio path");
}
