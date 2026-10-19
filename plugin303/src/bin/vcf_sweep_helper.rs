//! VCF magnitude-response probe for `eval/303/calibrate_vcf.py`.
//!
//! Prints the gain (dB, relative to 50 Hz) of one tier's filter path at 60
//! log-spaced frequencies from 50 Hz to 20 kHz. Each tier's filter is built the
//! way `Voice` builds it (same oversampling factor and brightness calibration).
//!
//! Usage: vcf_sweep_helper --tier <draft|low|best> --fc <Hz> --reso <0..1> [--poles <n>]

use plugin303::dsp::filter::Filter303;
use plugin303::dsp::ladder::TptLadder;
use plugin303::dsp::ladder_wdf::LadderVcf;
use plugin303::dsp::oversample::Os4x;
use plugin303::dsp::voice::{
    BEST_OS_FACTOR, CIRCUIT_BRIGHT, DRAFT_OS_FACTOR, LOW_BRIGHT, LOW_OS_FACTOR,
};

const SR: f32 = 48_000.0;

#[derive(Clone, Copy)]
enum Path {
    Draft,
    Low,
    Best,
}

fn log_sweep(lo: f32, hi: f32, n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| lo * (hi / lo).powf(i as f32 / (n - 1) as f32))
        .collect()
}

fn gain_at_freq(path: Path, fc: f32, reso: f32, poles: usize, freq: f32) -> f32 {
    let amp = 0.02f32;
    let mut process: Box<dyn FnMut(f32) -> f32> = match path {
        Path::Draft => {
            let mut f = TptLadder::new(SR * DRAFT_OS_FACTOR);
            f.set_poles(poles);
            f.set_cutoff(fc);
            f.set_reso(reso);
            let mut os = Os4x::new();
            Box::new(move |x| {
                let up = os.up(x);
                os.down([f.tick(up[0]), f.tick(up[1]), f.tick(up[2]), f.tick(up[3])])
            })
        }
        Path::Low => {
            let mut f = LadderVcf::new(SR * LOW_OS_FACTOR);
            f.set_cal(LOW_BRIGHT);
            f.set_poles(poles);
            f.set_cutoff(fc);
            f.set_reso(reso);
            let mut os = Os4x::new();
            Box::new(move |x| {
                let up = os.up(x);
                os.down([f.tick(up[0]), f.tick(up[1]), f.tick(up[2]), f.tick(up[3])])
            })
        }
        Path::Best => {
            let mut f = LadderVcf::new(SR * BEST_OS_FACTOR);
            f.set_cal(CIRCUIT_BRIGHT);
            f.set_poles(poles);
            f.set_cutoff(fc);
            f.set_reso(reso);
            let mut os = Os4x::new();
            Box::new(move |x| {
                let up = os.up(x);
                os.down([f.tick(up[0]), f.tick(up[1]), f.tick(up[2]), f.tick(up[3])])
            })
        }
    };

    let warmup = 6_000;
    let n_samples = 12_000;
    let mut ph = 0.0f32;
    let mut acc = 0.0f32;
    for i in 0..(warmup + n_samples) {
        let y = process(amp * (std::f32::consts::TAU * ph).sin());
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

fn usage_exit(msg: &str) -> ! {
    eprintln!("{msg}");
    eprintln!("usage: vcf_sweep_helper --tier <draft|low|best> --fc <Hz> --reso <0..1> [--poles <n>]");
    std::process::exit(2);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut path = Path::Best;
    let mut fc = 1000.0f32;
    let mut reso = 0.0f32;
    let mut poles = 4usize;

    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let Some(value) = it.next() else {
            usage_exit(&format!("missing value for {flag}"));
        };
        let num = |v: &str| -> f32 {
            v.parse()
                .unwrap_or_else(|_| usage_exit(&format!("invalid number for {flag}: {v}")))
        };
        match flag.as_str() {
            "--tier" => {
                path = match value.as_str() {
                    "draft" => Path::Draft,
                    "low" => Path::Low,
                    "best" => Path::Best,
                    other => usage_exit(&format!("unknown tier: {other}")),
                }
            }
            "--fc" => fc = num(value),
            "--poles" => {
                poles = value
                    .parse()
                    .unwrap_or_else(|_| usage_exit(&format!("invalid integer for {flag}: {value}")))
            }
            "--reso" => reso = num(value),
            other => usage_exit(&format!("unknown argument: {other}")),
        }
    }

    let pb_gain = gain_at_freq(path, fc, reso, poles, 50.0).max(1e-5);
    let results: Vec<String> = log_sweep(50.0, 20_000.0, 60)
        .into_iter()
        .map(|freq| {
            let gain = gain_at_freq(path, fc, reso, poles, freq);
            format!("{:.6}", 20.0 * (gain / pb_gain).log10())
        })
        .collect();
    println!("{}", results.join(" "));
}
