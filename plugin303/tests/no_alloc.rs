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
    assert_eq!(
        violation_count(),
        0,
        "voice.tick allocated on the audio path"
    );
}

#[test]
fn ultra_voice_hot_path_is_alloc_free() {
    use plugin303::dsp::filter::Tier;
    let mut voice = Voice::new(48_000.0); // full Ultra chain: VCO + 4× OS + nodal ladder
    voice.set_tier_immediate(Tier::Ultra);
    voice.set_cutoff(1_000.0);
    voice.set_reso(0.5);
    voice.note_on(57);
    for _ in 0..128 {
        voice.tick(); // self-start + warm-start outside the guard
    }
    reset_violation_count();
    assert_no_alloc(|| {
        let mut acc = 0.0f32;
        for _ in 0..4_096 {
            acc += voice.tick();
        }
        core::hint::black_box(acc);
    });
    assert_eq!(
        violation_count(),
        0,
        "Ultra voice.tick allocated on the audio path"
    );
}

#[test]
fn high_voice_hot_path_is_alloc_free() {
    use plugin303::dsp::filter::Tier;
    let mut voice = Voice::new(48_000.0); // High chain: polyBLEP saw + 2× OS + nodal ladder
    voice.set_tier(Tier::High);
    voice.set_cutoff(1_000.0);
    voice.set_reso(0.5);
    voice.note_on(57);
    for _ in 0..128 {
        voice.tick(); // warm-start the ladder Newton outside the guard
    }
    reset_violation_count();
    assert_no_alloc(|| {
        let mut acc = 0.0f32;
        for _ in 0..4_096 {
            acc += voice.tick();
        }
        core::hint::black_box(acc);
    });
    assert_eq!(
        violation_count(),
        0,
        "High voice.tick allocated on the audio path"
    );
}

#[test]
fn tier_crossfade_hot_path_is_alloc_free() {
    use plugin303::dsp::filter::Tier;
    // A crossfade renders BOTH tiers per sample — the heaviest hot path. Must
    // stay alloc-free while the fade is in flight.
    let mut voice = Voice::new(48_000.0);
    // Initialize VCO LUTs and warm-start both backends outside the guard
    voice.set_tier_immediate(Tier::Ultra);
    voice.set_cutoff(1_000.0);
    voice.set_reso(0.5);
    voice.note_on(57);
    for _ in 0..128 {
        voice.tick();
    }
    voice.set_tier_immediate(Tier::Low);
    for _ in 0..128 {
        voice.tick();
    }
    reset_violation_count();
    assert_no_alloc(|| {
        voice.set_tier(Tier::Ultra); // kicks off Low→Ultra crossfade
        let mut acc = 0.0f32;
        for _ in 0..4_096 {
            acc += voice.tick();
        }
        core::hint::black_box(acc);
    });
    assert_eq!(
        violation_count(),
        0,
        "tier crossfade allocated on the audio path"
    );
}

#[test]
fn voice_reset_is_alloc_free() {
    use plugin303::dsp::filter::Tier;
    // nih-plug's `reset()` may fire from the audio thread, so `Voice::reset` must
    // not allocate. It resets in place (no `Voice::new` reconstruction) precisely
    // so the VCO pitch-LUT cache — warmed once by the first `Voice::new` below —
    // is never rebuilt here. Guards against a regression to the old
    // `self.voice = Voice::new(..)` reset, which would `Vec::push` on a cold cache.
    let mut voice = Voice::new(48_000.0); // warms the per-sr LUT cache (may allocate)
    voice.set_tier(Tier::Ultra);
    voice.set_cutoff(1_000.0);
    voice.set_reso(0.5);
    voice.note_on(57);
    for _ in 0..128 {
        voice.tick();
    }
    reset_violation_count();
    assert_no_alloc(|| {
        for _ in 0..16 {
            voice.reset(); // repeated, to catch any per-call alloc
            voice.set_tier_immediate(Tier::Ultra);
            voice.note_on(57);
            voice.tick();
        }
    });
    assert_eq!(
        violation_count(),
        0,
        "Voice::reset allocated (audio-thread path)"
    );
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
    assert_eq!(
        violation_count(),
        0,
        "LadderVcf::tick allocated on the audio path"
    );
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
    assert_eq!(
        violation_count(),
        0,
        "Vco303::tick allocated on the audio path"
    );
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
    assert_eq!(
        violation_count(),
        0,
        "Os4x up/down allocated on the audio path"
    );
}
