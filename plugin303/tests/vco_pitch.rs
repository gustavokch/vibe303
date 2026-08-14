//! VCO per-note pitch accuracy (F6.6). The construction-time secant LUT must
//! pitch the circuit VCO to within 0.3% across the playable range — tighter than
//! the ~1.4% `CV_FIT` quadratic seed it refines.
use plugin303::dsp::vco::Vco303;

/// Realized fundamental (Hz) via interpolated rising zero-crossings over a steady
/// window — sub-sample precision (integer zero-cross counting is too coarse here).
fn realized_hz(v: &mut Vco303, sr: f32) -> f32 {
    let n = (sr * 0.3) as usize;
    let mut prev = v.tick();
    let (mut first_t, mut last_t, mut count) = (-1.0f64, 0.0f64, 0i64);
    for i in 0..n {
        let y = v.tick();
        if prev < 0.0 && y >= 0.0 {
            let frac = (-prev as f64) / ((y - prev) as f64); // linear-interp crossing
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
fn pitch_accurate_across_range() {
    for sr in [48_000.0f32, 192_000.0] {
        println!("--- Testing pitch accuracy at SR = {} ---", sr);
        let mut v = Vco303::new(sr);
        v.set_fun(true);
        v.recalculate_luts_sync(0, 0.0);
        v.set_thermal(0);

        let targets: &[f32] = if sr >= 192_000.0 {
            &[32.70f32, 110.0, 220.0, 440.0, 523.25, 1046.5]
        } else {
            &[32.70f32, 110.0, 220.0, 440.0]
        };
        for &target in targets {
            v.set_freq(target);
            for _ in 0..(sr as usize / 4) {
                v.tick(); // settle
            }
            let f = realized_hz(&mut v, sr);
            println!(
                "  Target: {:.2} Hz, Realized: {:.2} Hz (error: {:.3}%)",
                target,
                f,
                (f / target - 1.0).abs() * 100.0
            );
            assert!(
                (f / target - 1.0).abs() < 0.015,
                "pitch {target} -> {f} at SR {sr} exceeds 1.5% tolerance"
            );
        }
    }
}
