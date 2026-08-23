//! Voice DSP profiling harness.
//! Measures throughput (samples/sec) and real-time speedup factor across Low, High, Ultra tiers.

use plugin303::dsp::filter::Tier;
use plugin303::dsp::sequencer::SeqEvent;
use plugin303::dsp::voice::Voice;
use plugin303::params::CircuitTopology;
use std::time::Instant;

fn arg_f32(flag: &str, default: f32) -> f32 {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|s| s == flag)
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn arg_u64(flag: &str, default: u64) -> u64 {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|s| s == flag)
        .and_then(|i| args.get(i + 1))
        .and_then(|v| {
            if v.starts_with("0x") || v.starts_with("0X") {
                u64::from_str_radix(&v[2..], 16).ok()
            } else {
                v.parse().ok()
            }
        })
        .unwrap_or(default)
}

fn has_flag(flag: &str) -> bool {
    std::env::args().any(|s| s == flag)
}

fn arg_tier(flag: &str) -> Option<i32> {
    let args: Vec<String> = std::env::args().collect();
    let val = args.iter()
        .position(|s| s == flag)
        .and_then(|i| args.get(i + 1))?;

    match val.to_ascii_lowercase().as_str() {
        "0" | "low" => Some(0),
        "1" | "high" => Some(1),
        "2" | "ultra" => Some(2),
        "3" | "sweep" => Some(3),
        _ => val.parse::<i32>().ok(),
    }
}

fn run_tier(name: &str, tier: Tier, secs: f32, sr: f32, seed: u64) -> (f64, f64, f64, bool) {
    let num_samples = (secs * sr) as usize;
    let mut voice = Voice::new(sr);
    voice.set_tier_immediate(tier);
    voice.set_cutoff(800.0);
    voice.set_reso(0.75);
    voice.set_env_mod(0.6);

    let mut rng_state = seed ^ 0x5DEECE66D;

    let start = Instant::now();
    let mut sum = 0.0f32;

    // Classic 16-step 303 acid sequence pattern @ 130 BPM
    let samples_per_step = (sr * 60.0 / (130.0 * 4.0)) as usize;
    let keys = [0, 0, 12, 3, 5, 0, 12, 15, 0, 3, 5, 12, 0, 15, 12, 3];
    let octaves = [0, 0, 1, 0, 0, 0, 1, 1, 0, 0, 0, 1, 0, 1, 1, 0];
    let accents = [true, false, false, true, false, false, true, false, false, true, false, false, true, false, true, false];
    let slides = [false, false, true, false, false, true, false, false, false, false, true, false, false, false, true, false];

    for i in 0..num_samples {
        let step = (i / samples_per_step) % 16;
        let step_offset = i % samples_per_step;

        if step_offset == 0 {
            let key = keys[step];
            let oct = octaves[step];
            let acc = accents[step];
            let slide = slides[step];

            if slide {
                voice.on_seq_event(SeqEvent::SlideTo { key, octave: oct, accent: acc }, 0.8, 300.0);
            } else {
                voice.on_seq_event(SeqEvent::Trigger { key, octave: oct, accent: acc }, 0.8, 300.0);
            }
        } else if step_offset == samples_per_step * 3 / 4 && !slides[step] {
            voice.on_seq_event(SeqEvent::GateOff, 0.0, 0.0);
        }

        // Cutoff modulation and jitter
        if i % 64 == 0 {
            let sweep = ((i as f32 / num_samples as f32) * std::f32::consts::TAU * 4.0).sin() * 0.4 + 0.5;
            let cutoff_hz = 300.0 * 2.0f32.powf(sweep * 3.5);
            voice.set_cutoff(cutoff_hz);

            rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let jitter = ((rng_state >> 32) as u32 as f32 / 4294967296.0) * 0.02 - 0.01;
            voice.set_reso((0.75 + jitter).clamp(0.0, 1.0));
        }

        let out = voice.tick();
        sum += out;
    }

    let elapsed = start.elapsed();
    let elapsed_secs = elapsed.as_secs_f64();
    let samples_per_sec = num_samples as f64 / elapsed_secs;
    let realtime_ratio = samples_per_sec / sr as f64;
    let is_realtime = realtime_ratio >= 1.0;
    let status_str = if is_realtime { "REALTIME" } else { "UNDERRUN" };

    let msg = format!(
        "Tier {:5} | {:8.2} kS/s | {:6.2}x realtime | {:7.3} s elapsed | Status: {} (sum: {:.3})",
        name,
        samples_per_sec / 1000.0,
        realtime_ratio,
        elapsed_secs,
        status_str,
        sum
    );

    println!("{}", msg);
    eprintln!("{}", msg);

    (elapsed_secs, samples_per_sec, realtime_ratio, is_realtime)
}

fn run_sweep(secs: f32, sr: f32, _seed: u64) -> (f64, f64, f64, bool) {
    let num_samples = (secs * sr) as usize;
    let mut voice = Voice::new(sr);
    voice.set_tier_immediate(Tier::Ultra);
    voice.set_cutoff(800.0);
    voice.set_reso(0.75);
    voice.set_env_mod(0.6);

    let topologies = [
        CircuitTopology::Accurate,
        CircuitTopology::Fun,
    ];

    let start = Instant::now();
    let mut sum = 0.0f32;
    let samples_per_step = (sr * 60.0 / (130.0 * 4.0)) as usize;
    let tiers = [Tier::Ultra, Tier::High, Tier::Low, Tier::Ultra];

    for i in 0..num_samples {
        let step = (i / samples_per_step) % 16;
        let step_offset = i % samples_per_step;

        if step_offset == 0 {
            let tier_idx = (i / (samples_per_step * 8)) % tiers.len();
            voice.set_tier(tiers[tier_idx]);

            let topo_idx = (i / (samples_per_step * 16)) % topologies.len();
            voice.set_topology(topologies[topo_idx]);

            let key = (step * 7 % 12) as i8;
            let oct = (step % 2) as i8;
            let acc = step % 4 == 0;
            let slide = step % 3 == 0;

            if slide {
                voice.on_seq_event(SeqEvent::SlideTo { key, octave: oct, accent: acc }, 0.8, 300.0);
            } else {
                voice.on_seq_event(SeqEvent::Trigger { key, octave: oct, accent: acc }, 0.8, 300.0);
            }
        } else if step_offset == samples_per_step * 3 / 4 {
            voice.on_seq_event(SeqEvent::GateOff, 0.0, 0.0);
        }

        if i % 64 == 0 {
            let sweep = ((i as f32 / num_samples as f32) * std::f32::consts::TAU * 8.0).sin() * 0.4 + 0.5;
            let cutoff_hz = 300.0 * 2.0f32.powf(sweep * 3.5);
            voice.set_cutoff(cutoff_hz);
        }

        let out = voice.tick();
        sum += out;
    }

    let elapsed = start.elapsed();
    let elapsed_secs = elapsed.as_secs_f64();
    let samples_per_sec = num_samples as f64 / elapsed_secs;
    let realtime_ratio = samples_per_sec / sr as f64;
    let is_realtime = realtime_ratio >= 1.0;
    let status_str = if is_realtime { "REALTIME" } else { "UNDERRUN" };

    let msg = format!(
        "Tier {:5} | {:8.2} kS/s | {:6.2}x realtime | {:7.3} s elapsed | Status: {} (sum: {:.3})",
        "sweep",
        samples_per_sec / 1000.0,
        realtime_ratio,
        elapsed_secs,
        status_str,
        sum
    );

    println!("{}", msg);
    eprintln!("{}", msg);

    (elapsed_secs, samples_per_sec, realtime_ratio, is_realtime)
}

fn main() {
    let secs = arg_f32("--secs", 10.0);
    let sr = arg_f32("--sr", 48_000.0);
    let seed = arg_u64("--seed", 0xAC1D);
    let has_tier_flag = has_flag("--tier");
    let target_tier = arg_tier("--tier");

    let header = format!("== Voice DSP Profile ({} seconds @ {} Hz, seed: 0x{:X}) ==", secs, sr, seed);
    println!("{}", header);
    eprintln!("{}", header);

    match target_tier {
        Some(0) => { run_tier("low", Tier::Low, secs, sr, seed); }
        Some(1) => { run_tier("high", Tier::High, secs, sr, seed); }
        Some(2) => { run_tier("ultra", Tier::Ultra, secs, sr, seed); }
        Some(3) => { run_sweep(secs, sr, seed); }
        _ if has_tier_flag => { run_sweep(secs, sr, seed); }
        _ => {
            run_tier("low", Tier::Low, secs, sr, seed);
            run_tier("high", Tier::High, secs, sr, seed);
            run_tier("ultra", Tier::Ultra, secs, sr, seed);
            run_sweep(secs, sr, seed);
        }
    }
}
