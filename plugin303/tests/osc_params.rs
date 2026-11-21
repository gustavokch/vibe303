use plugin303::dsp::osc::BlepOsc;
use plugin303::dsp::vco::Vco303;
use plugin303::params::OscWave;

/// Correlation of a saw and a 50% square, rendered by `tick` from the same start.
fn saw_square_corr(mut tick: impl FnMut(OscWave) -> Vec<f32>) -> f64 {
    let (saw, sq) = (tick(OscWave::Saw), tick(OscWave::Square));
    let dot = |a: &[f32], b: &[f32]| {
        a.iter()
            .zip(b)
            .map(|(&x, &y)| x as f64 * y as f64)
            .sum::<f64>()
    };
    dot(&saw, &sq) / (dot(&saw, &saw) * dot(&sq, &sq)).sqrt()
}

/// SWEEP adds the saw to the square, so every tier's oscillator must play the
/// two in phase: the polyBLEP saw falls like the circuit VCO's `o_r` ramp. With
/// opposite polarities the polyBLEP morph cancelled its fundamental (-6 dB at
/// sweep 0.5) while the circuit's grew (+5.5 dB).
#[test]
fn saw_and_square_share_polarity_on_both_oscillators() {
    let sr = 48_000.0;
    let n = sr as usize / 2;
    let blep = saw_square_corr(|wave| {
        let mut o = BlepOsc::new();
        o.set_freq(110.0, sr);
        o.set_wave(wave);
        (0..n).map(|_| o.tick()).collect()
    });
    let vco = saw_square_corr(|wave| {
        let mut o = Vco303::new(sr);
        o.set_freq(110.0);
        o.set_wave(wave);
        (0..n / 2).for_each(|_| {
            o.tick();
        });
        (0..n).map(|_| o.tick()).collect()
    });
    println!("saw-square correlation: polyBLEP {blep:+.3}, circuit VCO {vco:+.3}");
    assert!(
        blep > 0.5,
        "polyBLEP saw and square are out of phase: {blep:+.3}"
    );
    assert!(
        vco > 0.5,
        "circuit VCO saw and square are out of phase: {vco:+.3}"
    );
}

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
