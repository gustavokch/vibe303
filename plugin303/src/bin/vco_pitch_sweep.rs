use plugin303::dsp::vco::Vco303;

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

fn main() {
    let sr = 48000.0;
    println!("note,nominal_hz,realized_hz");
    for note in 24..=72 {
        let nominal = 440.0 * 2.0f32.powf((note as f32 - 69.0) / 12.0);
        let mut v = Vco303::new(sr);
        v.set_freq(nominal);
        for _ in 0..(sr * 0.1) as usize {
            v.tick();
        }
        let realized = realized_hz(&mut v, sr);
        println!("{},{:.6},{:.6}", note, nominal, realized);
    }
}
