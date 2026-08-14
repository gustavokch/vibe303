use plugin303::dsp::osc::BlepOsc;
use plugin303::params::OscWave;

#[test]
fn blep_saw_and_square_bounds() {
    let sr = 48_000.0;
    let mut osc = BlepOsc::new();
    osc.set_freq(440.0, sr);

    // Verify Saw bounds
    osc.set_wave(OscWave::Saw);
    for _ in 0..1000 {
        let v = osc.tick();
        assert!(v.abs() <= 1.5);
    }

    // Verify Square bounds
    osc.set_wave(OscWave::Square);
    for _ in 0..1000 {
        let v = osc.tick();
        assert!(v.abs() <= 1.5);
    }
}

#[test]
fn blep_square_pw_modulation() {
    let sr = 48_000.0;
    let mut osc = BlepOsc::new();
    osc.set_wave(OscWave::Square);
    osc.set_freq(100.0, sr); // 480 samples/period

    // Test with PW 0.25 (should be +1.0 for ~120 samples, -1.0 for ~360 samples)
    osc.set_pw(0.25);
    let mut count_pos = 0;
    let mut _count_neg = 0;

    // Warm up
    for _ in 0..1000 {
        osc.tick();
    }

    // Count one cycle
    for _ in 0..480 {
        let v = osc.tick();
        if v > 0.0 {
            count_pos += 1;
        } else {
            _count_neg += 1;
        }
    }

    // Check that positive vs negative counts approximate 25% vs 75%
    let ratio = count_pos as f32 / 480.0;
    assert!((ratio - 0.25).abs() < 0.05, "Ratio was {}", ratio);
}
