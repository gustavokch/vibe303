use plugin303::dsp::vco::Vco303;

fn realized_hz(v: &mut Vco303, sr: f32, nominal: f32) -> f32 {
    let period_samples = (sr / nominal).max(1.0);
    let n = (sr * 0.3).max(period_samples * 4.0) as usize;
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

fn main() {
    let sr = 48000.0;
    println!("topology,note,nominal_hz,realized_hz");
    for &is_fun in &[false, true] {
        let topo_str = if is_fun { "fun" } else { "accurate" };
        for note in 0..=108 {
            let nominal = 440.0 * 2.0f32.powf((note as f32 - 69.0) / 12.0);
            let mut v = Vco303::new(sr);
            v.set_fun(is_fun);
            v.recalculate_luts_sync(0, 0.0);
            v.set_thermal(0);
            v.set_freq(nominal);
            let period_samples = (sr / nominal).max(1.0);
            let warm = (sr * 0.1).max(period_samples * 2.0) as usize;
            for _ in 0..warm {
                v.tick();
            }
            let realized = realized_hz(&mut v, sr, nominal);
            println!("{},{},{:.6},{:.6}", topo_str, note, nominal, realized);
        }
    }
}
