// plugin/tests/test_levelnorm.rs
use ba283::levelnorm::LevelNorm;

/// Steady input/wet energy => gain converges to sqrt(in_ms / wet_ms), the feed-forward RMS match.
#[test]
fn gain_converges_to_rms_ratio() {
    let mut l = LevelNorm::new();
    l.prepare(96_000.0);
    // Input energy 1.0, wet energy 0.25 (wet 6 dB quieter) => needed gain = sqrt(1/0.25) = 2.0.
    for _ in 0..96_000 {
        l.update(1.0, 0.25);
    }
    assert!((l.gain() - 2.0).abs() < 0.02, "gain {} != 2.0", l.gain());
}

/// Below the input energy floor (near silence) the gain glides back toward unity, never chasing
/// noise up to the clamp.
#[test]
fn silence_holds_gain_near_unity() {
    let mut l = LevelNorm::new();
    l.prepare(96_000.0);
    // Drive it up first...
    for _ in 0..96_000 {
        l.update(1.0, 0.01);
    }
    assert!(l.gain() > 5.0, "expected high gain pre-silence, got {}", l.gain());
    // ...then feed silence. The input envelope must first decay below the floor (~1 s) before the
    // gain glides back toward unity, so allow a few seconds.
    for _ in 0..(96_000 * 3) {
        l.update(0.0, 0.0);
    }
    assert!((l.gain() - 1.0).abs() < 0.02, "silence gain {} != 1.0", l.gain());
}

/// The gain is clamped: a near-silent wet path against loud input cannot blow the gain past the
/// +48 dB ceiling (256x).
#[test]
fn gain_is_clamped() {
    let mut l = LevelNorm::new();
    l.prepare(96_000.0);
    for _ in 0..96_000 {
        l.update(1.0, 1.0e-20);
    }
    assert!(l.gain() <= 256.0 + 1e-3, "gain {} exceeded clamp", l.gain());
}
