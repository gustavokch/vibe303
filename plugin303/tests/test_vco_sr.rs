//! VCO pitch tracking must work at all common sample rates, not just 48 kHz.
//! The secant solver that builds the CV LUT must converge correctly regardless
//! of the host's sample rate.
use plugin303::dsp::vco::Vco303;

/// Measure the realized fundamental (Hz) via interpolated rising zero-crossings.
fn realized_hz(v: &mut Vco303, sr: f32) -> f32 {
    let n = (sr * 0.3) as usize;
    let mut prev = v.tick();
    let (mut first_t, mut last_t, mut count) = (-1.0f64, 0.0f64, 0i64);
    for i in 0..n {
        let y = v.tick();
        if prev < 0.0 && y >= 0.0 {
            let frac = (-prev as f64) / ((y - prev) as f64);
            let t = i as f64 + frac;
            if first_t < 0.0 {
                first_t = t;
            } else {
                last_t = t;
                count += 1;
            }
        }
        prev = y;
    }
    if count == 0 || last_t <= first_t {
        return 0.0;
    }
    (count as f64 * sr as f64 / (last_t - first_t)) as f32
}

#[test]
fn vco_sr_pitch_tracking() {
    Vco303::disable_warm_cache();

    // Test at all common sample rates — the bug only manifests at non-48kHz rates.
    for is_fun in [false, true] {
        for sr in [44100.0f32, 48000.0, 88200.0, 96000.0, 192000.0] {
            let mut vco = Vco303::new(sr);
            if is_fun {
                vco.set_fun(true);
                vco.build_step0_sync();
            }

            // First: assert the LUT contains valid CV values (not garbage/zero).
            for i in 0..vco.cv_lut.len() {
                assert!(
                    vco.cv_lut[i].is_finite() && vco.cv_lut[i].abs() < 20.0,
                    "SR {sr} (Fun={is_fun}): cv_lut[{i}] = {} is invalid (NaN/Inf/out-of-range)",
                    vco.cv_lut[i]
                );
            }

            // Second: verify pitch tracking at A4 = 440 Hz (< 0.5% error).
            let target = 440.0f32;
            vco.set_freq(target);
            // Settle the oscillator.
            for _ in 0..(sr as usize / 4) {
                vco.tick();
            }
            let f = realized_hz(&mut vco, sr);
            let err = ((f / target) - 1.0).abs();
            assert!(
                err < 0.005,
                "SR {sr} (Fun={is_fun}): target={target} Hz, realized={f} Hz, error={:.2}% (> 0.5%)",
                err * 100.0
            );
        }
    }
}
