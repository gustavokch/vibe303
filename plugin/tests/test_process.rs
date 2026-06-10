// plugin/tests/test_process.rs
mod common;

use ba283::delay::ShortDelay;
use ba283::dsp::{ChannelDsp, INPUT_FS_VOLTS};
use ba283::model::Ba283Model;
use ba283::InferenceBackend;

#[test]
fn short_delay_returns_n_samples_later() {
    let mut d: ShortDelay<8> = ShortDelay::new();
    for _ in 0..8 {
        let _ = d.push_pop(0.0);
    }
    let out = d.push_pop(0.5);
    assert!(out.abs() < 1e-9);
    for _ in 0..7 {
        let _ = d.push_pop(0.0);
    }
    let delayed = d.push_pop(0.0);
    assert!((delayed - 0.5).abs() < 1e-6);
}

#[test]
fn db_to_gain_correct() {
    fn db_to_gain(db: f32) -> f32 {
        10.0f32.powf(db * 0.05)
    }
    assert!((db_to_gain(0.0) - 1.0).abs() < 1e-6);
    assert!((db_to_gain(6.02) - 2.0).abs() < 0.01);
    assert!((db_to_gain(-6.02) - 0.5).abs() < 0.01);
}

#[test]
fn mix_zero_is_dry_passthrough_at_96k() {
    // At >=96k (no OS, no latency) with mix=0 (fully dry) and drive=out=1,
    // output must equal the input exactly.
    let mut dsp = ChannelDsp::new();
    for i in 0..256 {
        let x = (i as f32 * 0.05).sin();
        let y = dsp.process_sample(x, 1.0, 0.5, 1.0, 1.0, 0.0, true, InferenceBackend::NeuralNet);
        assert!(
            (y - x).abs() < 1e-6,
            "dry passthrough failed at {i}: {y} != {x}"
        );
    }
}

#[test]
fn mix_zero_dry_is_latency_aligned_sub96k() {
    // At <96k with mix=0, output is the dry path delayed by exactly the OS
    // latency (32). After the latency, output[i] == input[i-32].
    let mut dsp = ChannelDsp::new();
    let n = 512;
    let input: Vec<f32> = (0..n).map(|i| (i as f32 * 0.03).sin()).collect();
    let mut out = vec![0.0f32; n];
    for i in 0..n {
        out[i] = dsp.process_sample(input[i], 1.0, 0.5, 1.0, 1.0, 0.0, false, InferenceBackend::NeuralNet);
    }
    let lat = 32;
    let mut max_err = 0.0f32;
    for i in lat..n {
        let e = (out[i] - input[i - lat]).abs();
        if e > max_err {
            max_err = e;
        }
    }
    assert!(
        max_err < 1e-6,
        "dry not aligned at latency {lat}: max_err {max_err}"
    );
}

#[test]
fn nn_output_is_denormed_by_output_ref_over_input_fs() {
    // The NeuralNet wet path must de-norm by `output_ref_v / INPUT_FS_VOLTS` (= x128 at the 64 V
    // ref): recover volts, then express in the 0 dBFS == 0.5 V convention. With drive=out=norm=1
    // and mix=100% (full wet) at >=96k, the chain output must equal a fresh model forward on the
    // same input times that factor. A fresh ChannelDsp and a fresh model share identical initial
    // state, so their hidden-state trajectories match sample-for-sample.
    let mut dsp = ChannelDsp::new();
    let mut model = Ba283Model::new_from_embedded_weights();
    let deref = model.output_ref_v() / INPUT_FS_VOLTS;
    assert!((deref - 128.0).abs() < 1e-3, "expected x128 de-norm, got {deref}");
    let bias = 0.5;
    for i in 0..256 {
        let x = (i as f32 * 0.05).sin() * 0.4;
        let y = dsp.process_sample(x, 1.0, bias, 1.0, 1.0, 1.0, true, InferenceBackend::NeuralNet);
        let expected = model.forward(&[x, bias])[0] * deref;
        assert!(
            (y - expected).abs() <= 1e-4 * expected.abs().max(1.0),
            "de-norm mismatch at {i}: {y} != {expected}"
        );
    }
}

#[test]
fn wdf_backend_produces_finite_output() {
    // The WDF path runs the unwarmed default model here; we only assert it is finite and bounded
    // (no NaN/blowup — guarded by the Newton step-bound + gmin homotopy).
    let mut dsp = ChannelDsp::new();
    dsp.wdf_warmup(); // off-thread settle before audio
    for i in 0..256 {
        let x = (i as f32 * 0.05).sin() * 0.3;
        let y = dsp.process_sample(x, 1.0, 0.5, 0.05, 1.0, 1.0, true, InferenceBackend::Wdf);
        assert!(y.is_finite(), "non-finite WDF output at {i}: {y}");
        assert!(y.abs() < 100.0, "WDF output blew up at {i}: {y}");
    }
}

#[test]
fn wdf_aged_output_finite_and_coherent() {
    // Two channels at the same aged unit (same seed + age) must stay finite AND bit-identical
    // sample-for-sample (stereo coherence — the design applies one seed/age to both channels).
    let mut l = ChannelDsp::new();
    let mut r = ChannelDsp::new();
    for ch in [&mut l, &mut r] {
        ch.wdf_warmup();
        ch.wdf_set_age_seed(0x7777);
        ch.wdf_set_age(1.0);
    }
    let mut base = ChannelDsp::new();
    base.wdf_warmup(); // age=0 reference
    let mut moved = false;
    for i in 0..256 {
        let x = (i as f32 * 0.05).sin() * 0.3;
        let yl = l.process_sample(x, 1.0, 0.5, 0.05, 1.0, 1.0, true, InferenceBackend::Wdf);
        let yr = r.process_sample(x, 1.0, 0.5, 0.05, 1.0, 1.0, true, InferenceBackend::Wdf);
        let yb = base.process_sample(x, 1.0, 0.5, 0.05, 1.0, 1.0, true, InferenceBackend::Wdf);
        assert!(yl.is_finite() && yl.abs() < 100.0, "aged L blew up at {i}: {yl}");
        assert_eq!(yl.to_bits(), yr.to_bits(), "stereo not coherent at {i}: {yl} != {yr}");
        if (yl as f64 - yb as f64).abs() > 1e-4 {
            moved = true;
        }
    }
    assert!(moved, "age=1 should differ from the un-aged reference");
}

#[test]
fn wdf_backend_latency_aligned_sub96k() {
    // Sub-96k, mix=0 (fully dry): the WDF blended output is delayed by exactly OS_LATENCY_SAMPLES
    // (32) so the single host latency report stays valid for both backends. With drive=out=1 and
    // mix=0 the output is the dry input delayed 32 -> output[i] == input[i-32].
    let mut dsp = ChannelDsp::new();
    dsp.wdf_warmup();
    let n = 256;
    let input: Vec<f32> = (0..n).map(|i| (i as f32 * 0.03).sin()).collect();
    let mut out = vec![0.0f32; n];
    for i in 0..n {
        out[i] = dsp.process_sample(input[i], 1.0, 0.5, 1.0, 1.0, 0.0, false, InferenceBackend::Wdf);
    }
    let lat = 32;
    let mut max_err = 0.0f32;
    for i in lat..n {
        max_err = max_err.max((out[i] - input[i - lat]).abs());
    }
    assert!(max_err < 1e-6, "WDF dry not aligned at latency {lat}: max_err {max_err}");
}
