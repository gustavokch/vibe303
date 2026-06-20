//! Validates the general rational SRC bridge (`SrcBridge`): it pins the model to a
//! fixed native rate for any host SR, keeps the per-sample 1-in-1-out contract,
//! reports a fixed integer latency, and stays bit-exact at host == model rate.

use ba283::oversample::SrcBridge;

const MODEL_SR: u32 = 48_000;
const HOST_RATES: [u32; 6] = [44_100, 48_000, 64_000, 88_200, 96_000, 192_000];

#[test]
fn bypass_is_bit_exact_at_model_rate() {
    let mut b = SrcBridge::new();
    b.prepare(MODEL_SR, MODEL_SR);
    assert_eq!(b.latency_host(), 0);
    for i in 0..512 {
        let x = (i as f32 * 0.013).sin();
        // Identity model → output must equal input, bit-for-bit (true passthrough).
        assert_eq!(b.process(x, |lo| lo), x);
    }
}

#[test]
fn dc_returns_constant_every_rate() {
    for &host in &HOST_RATES {
        let mut b = SrcBridge::new();
        b.prepare(host, MODEL_SR);
        let mut last = 0.0;
        for _ in 0..2048 {
            last = b.process(1.0, |lo| lo);
        }
        assert!((last - 1.0).abs() < 0.01, "host {host}: DC out {last}");
    }
}

#[test]
fn identity_after_reported_latency() {
    // With an identity model the bridge is a band-limited delayed identity. The
    // best-aligning delay must equal the reported latency (the dry-path delay reads
    // this), and the residual must be small for an in-band slow sine.
    for &host in &HOST_RATES {
        let mut b = SrcBridge::new();
        b.prepare(host, MODEL_SR);
        let lat = b.latency_host();
        // Slow sine well inside both passbands (period ~ 600 samples).
        let input: Vec<f32> = (0..4096).map(|i| (i as f32 * 0.0105).sin()).collect();
        let output: Vec<f32> = input.iter().map(|&x| b.process(x, |lo| lo)).collect();
        let mut max_err = 0.0f32;
        for i in (lat + 256)..input.len() {
            let e = (output[i] - input[i - lat]).abs();
            if e > max_err {
                max_err = e;
            }
        }
        assert!(
            max_err < 0.03,
            "host {host}: identity err {max_err} at reported latency {lat}"
        );
    }
}

#[test]
fn model_clock_is_pinned_to_native_rate() {
    // Over N host samples the model must be evaluated ~N * model_SR/host_SR times,
    // i.e. the model runs at exactly its native rate regardless of host SR.
    for &host in &HOST_RATES {
        let mut b = SrcBridge::new();
        b.prepare(host, MODEL_SR);
        let n = 10_000usize;
        let mut calls = 0u64;
        for i in 0..n {
            let x = (i as f32 * 0.01).sin();
            let _ = b.process(x, |lo| {
                calls += 1;
                lo
            });
        }
        let expected = (n as f64 * MODEL_SR as f64 / host as f64).round() as i64;
        let diff = (calls as i64 - expected).abs();
        assert!(
            diff <= 4,
            "host {host}: model called {calls}, expected ~{expected} (diff {diff})"
        );
    }
}

#[test]
fn alias_rejection_on_decimation() {
    // Feed a tone above the model Nyquist (24 kHz) at a high host rate; after the
    // round trip the aliased image must be heavily suppressed. Use 96 kHz host and a
    // 30 kHz tone (folds to 18 kHz if it aliases through the 48 kHz model).
    let host = 96_000u32;
    let mut b = SrcBridge::new();
    b.prepare(host, MODEL_SR);
    let f = 30_000.0f32;
    let n = 8192usize;
    let out: Vec<f32> = (0..n)
        .map(|i| {
            let x = (2.0 * std::f32::consts::PI * f * i as f32 / host as f32).sin();
            b.process(x, |lo| lo)
        })
        .collect();
    // RMS of the tail (post-latency) must be small: the >24 kHz tone is removed by
    // the model-Nyquist low-pass and should not reappear as an alias.
    let tail = &out[2048..];
    let rms = (tail.iter().map(|&v| v * v).sum::<f32>() / tail.len() as f32).sqrt();
    assert!(rms < 0.05, "host {host}: residual/alias RMS {rms} too high");
}

#[test]
fn stereo_determinism() {
    let mut a = SrcBridge::new();
    let mut b = SrcBridge::new();
    a.prepare(44_100, MODEL_SR);
    b.prepare(44_100, MODEL_SR);
    for i in 0..2048 {
        let x = (i as f32 * 0.02).sin();
        assert_eq!(a.process(x, |lo| lo * 0.5), b.process(x, |lo| lo * 0.5));
    }
}
