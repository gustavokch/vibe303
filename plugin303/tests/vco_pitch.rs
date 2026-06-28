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
    let sr = 192_000.0;
    for &target in &[110.0f32, 220.0, 440.0] {
        let mut v = Vco303::new(sr);
        v.set_freq(target);
        for _ in 0..(sr as usize / 4) {
            v.tick(); // settle
        }
        let f = realized_hz(&mut v, sr);
        assert!(
            (f / target - 1.0).abs() < 0.003,
            "pitch {target} -> {f} (>0.3%)"
        );
    }
}
