// plugin/tests/test_process.rs
mod common;

use ba283::delay::ShortDelay;
use ba283::dsp::ChannelDsp;
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
        let y = dsp.process_sample(x, 1.0, 0.5, 1.0, 0.0, true, InferenceBackend::NeuralNet);
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
        out[i] = dsp.process_sample(input[i], 1.0, 0.5, 1.0, 0.0, false, InferenceBackend::NeuralNet);
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
fn wdf_backend_produces_finite_output() {
    // The WDF path runs the unwarmed default model here; we only assert it is finite and bounded
    // (no NaN/blowup — guarded by the Newton step-bound + gmin homotopy).
    let mut dsp = ChannelDsp::new();
    dsp.wdf_warmup(); // off-thread settle before audio
    for i in 0..256 {
        let x = (i as f32 * 0.05).sin() * 0.3;
        let y = dsp.process_sample(x, 1.0, 0.5, 0.05, 1.0, true, InferenceBackend::Wdf);
        assert!(y.is_finite(), "non-finite WDF output at {i}: {y}");
        assert!(y.abs() < 100.0, "WDF output blew up at {i}: {y}");
    }
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
        out[i] = dsp.process_sample(input[i], 1.0, 0.5, 1.0, 0.0, false, InferenceBackend::Wdf);
    }
    let lat = 32;
    let mut max_err = 0.0f32;
    for i in lat..n {
        max_err = max_err.max((out[i] - input[i - lat]).abs());
    }
    assert!(max_err < 1e-6, "WDF dry not aligned at latency {lat}: max_err {max_err}");
}
