mod common;

use ba283::oversample::Oversample2x;

#[test]
fn upsample_dc_returns_constant() {
    let mut os = Oversample2x::new();
    let (mut last_a, mut last_b) = (0.0, 0.0);
    for _ in 0..200 {
        let (a, b) = os.upsample(1.0);
        last_a = a;
        last_b = b;
    }
    assert!((last_a - 1.0).abs() < 0.01);
    assert!((last_b - 1.0).abs() < 0.01);
}

#[test]
fn upsample_then_downsample_is_unity_after_latency() {
    let mut os = Oversample2x::new();
    let input: Vec<f32> = (0..1024).map(|i| ((i as f32) * 0.01).sin()).collect();
    let mut output = vec![0.0f32; 1024];
    for i in 0..1024 {
        let (u0, u1) = os.upsample(input[i]);
        output[i] = os.downsample(u0, u1);
    }
    let latency = 16;
    let mut max_err = 0.0f32;
    for i in latency..1024 {
        let e = (output[i] - input[i - latency]).abs();
        if e > max_err {
            max_err = e;
        }
    }
    assert!(max_err < 0.05, "max err {max_err}");
}

#[test]
fn no_heap_alloc_in_steady_state() {
    // The `assert_no_alloc` dep is built with `warn_debug`/`warn_release` (see
    // Cargo.toml), so `assert_no_alloc(closure)` only RECORDS allocations into
    // `violation_count()` rather than aborting/panicking on its own. To make
    // this test actually enforce zero-alloc we reset the counter before the
    // guarded block and assert it stayed at zero afterwards (same pattern as
    // tests/test_model_noalloc.rs). Verified to catch a deliberately injected
    // `Vec` alloc during development.
    use assert_no_alloc::*;
    let mut os = Oversample2x::new();
    for _ in 0..64 {
        os.upsample(0.0);
    }
    reset_violation_count();
    assert_no_alloc(|| {
        for _ in 0..1024 {
            let (a, b) = os.upsample(0.123);
            let _ = os.downsample(a, b);
        }
    });
    assert_eq!(
        violation_count(),
        0,
        "Oversample2x allocated on the heap inside assert_no_alloc"
    );
}
