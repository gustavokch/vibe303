use plugin303::dsp::filter::Filter303;
use plugin303::dsp::ladder::TptLadder;
use plugin303::dsp::ladder_wdf::LadderVcf;
use plugin303::dsp::oversample::{Os2x, Os4x};

const SR: f32 = 48_000.0;

fn log_sweep(lo: f32, hi: f32, n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| lo * (hi / lo).powf(i as f32 / (n - 1) as f32))
        .collect()
}

fn gain_at_freq(tier: &str, fc: f32, reso: f32, poles: usize, freq: f32) -> f32 {
    let amp = 0.02f32;

    // We instantiate the specific filters and oversamplers locally to keep the measurement isolated.
    let mut low_filter = TptLadder::new(SR);
    low_filter.set_cutoff(fc);
    low_filter.set_reso(reso);
    low_filter.set_poles(poles);

    let mut high_filter = LadderVcf::new(SR * 2.0);
    high_filter.set_cutoff(fc);
    high_filter.set_reso(reso);
    high_filter.set_poles(poles);
    let mut os2 = Os2x::new();

    let mut ultra_filter = LadderVcf::new(SR * 4.0);
    ultra_filter.set_cutoff(fc);
    ultra_filter.set_reso(reso);
    ultra_filter.set_poles(poles);
    let mut os4 = Os4x::new();

    let warmup = 6_000;
    let n_samples = 12_000;
    let mut ph = 0.0f32;
    let mut acc = 0.0f32;
    for i in 0..(warmup + n_samples) {
        let x = amp * (2.0f32 * std::f32::consts::PI * ph).sin();
        let y = match tier {
            "low" => low_filter.tick(x),
            "high" => {
                let up = os2.up(x);
                os2.down([high_filter.tick(up[0]), high_filter.tick(up[1])])
            }
            "ultra" => {
                let up = os4.up(x);
                os4.down([
                    ultra_filter.tick(up[0]),
                    ultra_filter.tick(up[1]),
                    ultra_filter.tick(up[2]),
                    ultra_filter.tick(up[3]),
                ])
            }
            _ => panic!("unknown tier"),
        };
        if i >= warmup {
            acc += y * y;
        }
        ph += freq / SR;
        if ph >= 1.0 {
            ph -= 1.0;
        }
    }
    let out_rms = (acc / n_samples as f32).sqrt();
    let in_rms = amp / 2.0f32.sqrt();
    out_rms / in_rms
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut tier = "ultra";
    let mut fc = 1000.0f32;
    let mut reso = 0.0f32;
    let mut poles = 4usize;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--tier" => {
                tier = args[i + 1].as_str();
                i += 2;
            }
            "--fc" => {
                fc = args[i + 1].parse().unwrap();
                i += 2;
            }
            "--reso" => {
                reso = args[i + 1].parse().unwrap();
                i += 2;
            }
            "--poles" => {
                poles = args[i + 1].parse().unwrap();
                i += 2;
            }
            _ => {
                eprintln!("Unknown argument: {}", args[i]);
                std::process::exit(1);
            }
        }
    }

    // Measure gain at 50 Hz to establish passband level
    let pb_gain = gain_at_freq(tier, fc, reso, poles, 50.0);

    // Measure gains at 60 log-spaced frequencies from 50 Hz to 20,000 Hz
    let freqs = log_sweep(50.0, 20000.0, 60);
    let mut results = Vec::new();
    for &freq in &freqs {
        let gain = gain_at_freq(tier, fc, reso, poles, freq);
        let gain_db = 20.0 * (gain / pb_gain.max(1e-5)).log10();
        results.push(format!("{:.6}", gain_db));
    }

    println!("{}", results.join(" "));
}
