use plugin303::dsp::filter::Tier;
use plugin303::dsp::voice::Voice;

fn realized_hz(buf: &[f32], sr: f32) -> f32 {
    let mut prev = buf[0];
    let (mut first_t, mut last_t, mut count) = (-1.0f64, 0.0f64, 0i64);
    for (i, &y) in buf.iter().enumerate() {
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

fn test_voice(sr: f32) {
    println!("=== Testing Voice at sample rate {} Hz ===", sr);
    for tier in &[Tier::Low, Tier::High, Tier::Ultra] {
        let mut v = Voice::new(sr);
        v.set_tier_immediate(*tier);
        // Turn off resonance and set cutoff high so we get a clean oscillator signal
        v.set_cutoff(4000.0);
        v.set_reso(0.0);
        
        let target = 220.0f32;
        v.note_on(57); // note 57 = A2 = 110Hz, wait! Let's check note_on note number vs base_hz
        // Let's set the frequency directly to 220Hz to be sure
        v.set_freq(target);
        
        // Render 0.5 seconds
        let n = (sr * 0.5) as usize;
        let mut buf = Vec::with_capacity(n);
        for _ in 0..n {
            buf.push(v.tick());
        }
        
        // Measure realized frequency (skip the first 0.1 seconds to allow settling)
        let skip = (sr * 0.1) as usize;
        let f = realized_hz(&buf[skip..], sr);
        let pct = (f / target - 1.0) * 100.0;
        println!("  Tier {:?}: Target: {:.2} Hz -> Realized: {:.2} Hz ({:+.2}%)", tier, target, f, pct);
    }
}

fn main() {
    for &sr in &[44100.0, 48000.0, 88200.0, 96000.0, 192000.0] {
        test_voice(sr);
    }
}
