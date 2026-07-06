//! Diagnostic: measure the pitch produced by each tier for the same MIDI note
//! to identify any octave discrepancy.

use plugin303::dsp::filter::Tier;
use plugin303::dsp::voice::Voice;
use plugin303::params::OscWave;

/// Realized fundamental (Hz) via interpolated rising zero-crossings.
fn realized_hz(buf: &[f32], sr: f32) -> f32 {
    let (mut first_t, mut last_t, mut count) = (-1.0f64, 0.0f64, 0i64);
    for i in 1..buf.len() {
        if buf[i - 1] < 0.0 && buf[i] >= 0.0 {
            let frac = (-buf[i - 1] as f64) / ((buf[i] - buf[i - 1]) as f64);
            let t = (i - 1) as f64 + frac;
            if first_t < 0.0 {
                first_t = t;
            } else {
                last_t = t;
                count += 1;
            }
        }
    }
    if count == 0 || last_t <= first_t {
        return 0.0;
    }
    (count as f64 * sr as f64 / (last_t - first_t)) as f32
}

#[test]
fn tier_pitch_comparison() {
    for &sr in &[48_000.0f32] {
        eprintln!("\n==================================================");
        eprintln!("=== TESTING SAMPLE RATE: {} Hz ===", sr);
        eprintln!("==================================================");

        for &wave in &[OscWave::Saw, OscWave::Square] {
            eprintln!("\n--- Waveform: {:?} ---", wave);
            for &note in &[33u8, 45, 57, 69, 84] {
                let target = 440.0 * 2.0f32.powf((note as f32 - 69.0) / 12.0);
                eprintln!("\n=== MIDI note {note}, target {target:.2} Hz ===");

                for &tier in &[Tier::Low, Tier::High, Tier::Ultra] {
                    let mut v = Voice::new(sr);
                    v.set_tier_immediate(tier);
                    v.set_wave(wave);
                    v.set_cutoff(4000.0);  // bright filter so pitch is clear
                    v.set_reso(0.0);
                    v.note_on(note);
                    v.set_gate(true, true, false);

                    // Warm up
                    for _ in 0..(sr * 0.1) as usize {
                        v.tick();
                    }
                    // Measure
                    let n = sr as usize;
                    let buf: Vec<f32> = (0..n).map(|_| v.tick()).collect();
                    let hz = realized_hz(&buf, sr);
                    let cents = 1200.0 * (hz / target).log2();
                    eprintln!("  {tier:?}: {hz:.2} Hz (target {target:.2} Hz, error {cents:+.1} cents)");
                }
            }
        }
    }
}

/// Test what happens when switching tiers mid-note
#[test]
fn tier_switch_pitch_stability() {
    let sr = 48_000.0;
    let note = 45u8;
    let target = 440.0 * 2.0f32.powf((note as f32 - 69.0) / 12.0);

    eprintln!("\n=== Switching tier mid-note, MIDI {note}, target {target:.2} Hz ===");

    for &(from, to) in &[
        (Tier::Low, Tier::Ultra),
        (Tier::Ultra, Tier::Low),
        (Tier::Low, Tier::High),
        (Tier::High, Tier::Ultra),
    ] {
        let mut v = Voice::new(sr);
        v.set_tier_immediate(from);
        v.set_cutoff(4000.0);
        v.set_reso(0.0);
        v.note_on(note);
        v.set_gate(true, true, false);

        // Play for a while on the "from" tier
        for _ in 0..24_000 {
            v.tick();
        }

        // Switch tier
        v.set_tier(to);

        // Wait for crossfade to finish (10ms = 480 samples) + settle
        for _ in 0..4800 {
            v.tick();
        }

        // Measure the pitch after switch
        let n = 48_000;
        let buf: Vec<f32> = (0..n).map(|_| v.tick()).collect();
        let hz = realized_hz(&buf, sr);
        let cents = 1200.0 * (hz / target).log2();
        eprintln!("  {from:?}->{to:?}: {hz:.2} Hz (error {cents:+.1} cents)");
    }
}
