//! The circuit VCO's square (High and Best) follows a note change at once.
//!
//! It is a comparator on the saw: high from the JFET reset until the falling
//! ramp crosses zero. A square timed from a smoothed reset-to-reset period
//! instead kept the old note's period for several cycles: it stuck high for
//! 12 ms after a two-octave jump up and played 10% pulses after a jump down,
//! a DC thump into the filter on every note change.

use plugin303::dsp::vco::Vco303;
use plugin303::params::OscWave;

const SR: f32 = 48_000.0;

fn hz(note: f32) -> f32 {
    440.0 * 2f32.powf((note - 69.0) / 12.0)
}

/// High fraction of each whole cycle, rising edge to rising edge.
fn duties(x: &[f32]) -> Vec<f32> {
    let rises: Vec<usize> = (1..x.len()).filter(|&i| x[i - 1] <= 0.0 && x[i] > 0.0).collect();
    rises
        .windows(2)
        .map(|w| {
            let high = (w[0]..w[1]).filter(|&i| x[i] > 0.0).count();
            high as f32 / (w[1] - w[0]) as f32
        })
        .collect()
}

#[test]
fn square_duty_follows_note_changes() {
    let mut vco = Vco303::new(SR);
    vco.set_wave(OscWave::Square);
    vco.set_freq(hz(45.0));
    for _ in 0..(0.3 * SR) as usize {
        vco.tick();
    }
    for (from, to) in [(45.0, 69.0), (69.0, 40.0), (40.0, 57.0), (59.0, 36.0)] {
        vco.set_freq(hz(from));
        for _ in 0..(0.1 * SR) as usize {
            vco.tick();
        }
        vco.set_freq(hz(to));
        let x: Vec<f32> = (0..(0.1 * SR) as usize).map(|_| vco.tick()).collect();
        let d = duties(&x);
        let mut tail = d[d.len() / 2..].to_vec();
        tail.sort_by(f32::total_cmp);
        let steady = tail[tail.len() / 2];
        let worst = d.iter().map(|v| (v - steady).abs()).fold(0.0f32, f32::max);
        println!("{from} -> {to}: steady duty {steady:.3}, worst cycle off by {worst:.3}");
        assert!(
            worst < 0.05,
            "{from} -> {to}: a cycle's duty is {worst:.3} off the steady {steady:.3}"
        );
    }
}
