use plugin303::dsp::vco::Vco303;
use plugin303::params::OscWave;

fn main() {
    let mut vco = Vco303::new(96000.0);
    vco.wave = OscWave::Saw;
    // Set pitch to A2 (110Hz)
    let cv = 2.44; // Approx A2
    vco.set_cv(cv);

    // Warm up
    for _ in 0..10000 {
        vco.tick();
    }

    // Measure min/max over 1 period
    let mut min = 1000.0f32;
    let mut max = -1000.0f32;
    for _ in 0..2000 {
        let val = vco.tick();
        if val < min { min = val; }
        if val > max { max = val; }
    }
    
    println!("A2 Swing: min={}, max={}, pp={}", min, max, max - min);
}
