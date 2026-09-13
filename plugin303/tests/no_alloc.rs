//! Realtime-safety gate: the voice's per-sample hot path must never allocate.
//! `AllocDisabler` (warn features) counts any allocation inside `assert_no_alloc`.
//!
//! deferred: this gates `voice.tick` only. The full RT path (`process()` event
//! drain + `buffer.iter_samples()`) gets its own gate once a nih-plug test-buffer
//! harness exists; the wrapper layers above `voice.tick` are nih-plug-owned.

use assert_no_alloc::*;
use nih_plug::prelude::Plugin;
use plugin303::dsp::filter::Filter303;
use plugin303::dsp::ladder_wdf::LadderVcf;
use plugin303::dsp::oversample::Os4x;
use plugin303::dsp::vco::Vco303;
use plugin303::dsp::voice::Voice;
use plugin303::Plugin303;

#[global_allocator]
static ALLOC: AllocDisabler = AllocDisabler;
/// Pre-warm the VCO LUT cache exactly once across all parallel test threads.
/// Without this, `recalculate_luts_async` spawns background threads that
/// compete for CPU with the test's tight VCO tick loops.
fn ensure_lut_cache_warm() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        Vco303::warm_cache_nominal(48_000.0, false);
        Vco303::warm_cache_nominal(48_000.0, true);
    });
}

#[test]
fn voice_hot_path_is_alloc_free() {
    ensure_lut_cache_warm();
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
    ensure_lut_cache_warm();
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
    ensure_lut_cache_warm();
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
    ensure_lut_cache_warm();
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
    ensure_lut_cache_warm();
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
fn plugin_reset_is_alloc_free() {
    ensure_lut_cache_warm();
    let mut plugin = Plugin303::default();
    reset_violation_count();
    assert_no_alloc(|| {
        for _ in 0..16 {
            plugin.reset();
        }
    });
    assert_eq!(
        violation_count(),
        0,
        "Plugin303::reset allocated (audio-thread path)"
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
    ensure_lut_cache_warm();
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

#[test]
fn sequencer_audio_path_is_alloc_free() {
    ensure_lut_cache_warm();
    use nih_plug::prelude::*;
    use plugin303::params::{Plugin303Params, SequencerPattern, StepGate};
    use plugin303::Plugin303;
    use std::sync::Arc;

    let mut pattern = SequencerPattern::default();
    pattern.steps[0].gate = StepGate::Note;
    pattern.steps[0].semitone = 0;
    pattern.steps[1].gate = StepGate::Note;
    pattern.steps[1].semitone = 2;
    pattern.steps[1].slide = true;

    let mut params = Plugin303Params::default();
    params.seq_run = BoolParam::new("Seq Run", true);
    *params.pattern.write().unwrap() = pattern;

    let mut plugin = Plugin303::new_with_params(Arc::new(params));

    let mut aux = AuxiliaryBuffers {
        inputs: &mut [],
        outputs: &mut [],
    };
    let mut buffer = Buffer::default();
    let mut scratch_l = [0.0f32; 512];
    let mut scratch_r = [0.0f32; 512];
    unsafe {
        buffer.set_slices(512, |slices| {
            slices.push(&mut scratch_l);
            slices.push(&mut scratch_r);
        });
    }

    struct DummyContext {
        transport: Transport,
    }
    impl ProcessContext<Plugin303> for DummyContext {
        fn plugin_api(&self) -> PluginApi {
            PluginApi::Clap
        }
        fn execute_background(&self, _: ()) {}
        fn execute_gui(&self, _: ()) {}
        fn send_event(&mut self, _: NoteEvent<()>) {}
        fn transport(&self) -> &Transport {
            &self.transport
        }
        fn next_event(&mut self) -> Option<PluginNoteEvent<Plugin303>> {
            None
        }
        fn set_latency_samples(&self, _samples: u32) {}
        fn set_current_voice_capacity(&self, _capacity: u32) {}
    }

    let transport: Transport = unsafe { std::mem::zeroed() };
    let mut ctx = DummyContext { transport };

    // Warm-up outside guard
    for _ in 0..4 {
        plugin.process(&mut buffer, &mut aux, &mut ctx);
    }

    reset_violation_count();
    assert_no_alloc(|| {
        for _ in 0..16 {
            plugin.process(&mut buffer, &mut aux, &mut ctx);
        }
    });
    assert_eq!(
        violation_count(),
        0,
        "Plugin303::process with sequencer allocated on audio path"
    );
}

#[test]
fn analog_character_param_sweep_is_alloc_free() {
    ensure_lut_cache_warm();
    use plugin303::dsp::filter::Tier;
    let mut voice = Voice::new(48_000.0);
    voice.set_tier_immediate(Tier::Ultra);
    voice.note_on(60);
    voice.update_params(
        0.0,
        0.0,
        0.0,
        0.0,
        plugin303::params::OscWave::Saw,
        0.5,
        0.0,
        0.5,
        0.5,
        0.5,
        0.5,
        0.5,
        0.0,
        plugin303::params::VcfTaps::FourPole,
        0.0,
        false,
        3.0,
        200.0,
        0.0,
        200.0,
        3.0,
        200.0,
        0.0,
        200.0,
        plugin303::params::VcaEnvMode::Envelope,
        0,                                      // comp_age
        0.0,                                    // comp_drift
        plugin303::params::CompTolerance::Five, // tolerance
        0,                                      // thermal_noise
        plugin303::params::CircuitTopology::Accurate,
        0.0,
        1337, // comp_seed
        0.5,
        0.0,
        1.0, // recalibration_scale
    );
    for _ in 0..128 {
        voice.tick();
    }
    reset_violation_count();
    assert_no_alloc(|| {
        let mut acc = 0.0f32;
        for i in 0..1_024 {
            voice.update_params(
                0.0,
                0.0,
                0.0,
                0.0,
                plugin303::params::OscWave::Saw,
                0.5,
                0.0,
                0.5,
                0.5,
                0.5,
                0.5,
                0.5,
                0.0,
                plugin303::params::VcfTaps::FourPole,
                0.0,
                false,
                3.0,
                200.0,
                0.0,
                200.0,
                3.0,
                200.0,
                0.0,
                200.0,
                plugin303::params::VcaEnvMode::Envelope,
                0,                                      // comp_age (fixed during RT sweep)
                (i as f32 * 0.001) % 1.0,               // comp_drift (real-time sweep)
                plugin303::params::CompTolerance::Five, // tolerance (fixed during RT sweep)
                (i % 50) as i32,                        // thermal_noise (real-time sweep)
                plugin303::params::CircuitTopology::Accurate,
                0.0,
                1337, // comp_seed (fixed during RT sweep)
                0.5,
                0.0,
                1.0, // recalibration_scale (fixed during RT sweep)
            );
            acc += voice.tick();
        }
        core::hint::black_box(acc);
    });
    assert_eq!(
        violation_count(),
        0,
        "Voice::update_params during real-time sweeps allocated on audio path"
    );
}
