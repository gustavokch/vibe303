//! Offline WAV renderer for the BA283 plugin.
//!
//! This is a CUSTOM offline file renderer, NOT nih_plug's realtime standalone
//! (which cannot file-render). It reads a WAV, runs the per-channel DSP chain,
//! and writes a WAV. It uses the SAME `ChannelDsp` core as the VST3 `process`,
//! so the Phase-4 null target (standalone vs SPICE) matches the plugin exactly.
//!
//! Two modes:
//!   - default (full chain): Drive -> model (direct at >= 96 kHz input, else via
//!     the 2x oversampler) -> Output, blended with a latency-aligned dry path by
//!     Mix. The offline "host SR" is the input WAV's sample rate, so a 48k WAV
//!     exercises the oversampling path and a 96k WAV does not.
//!   - `--raw`: pure model, `out = forward([sample, bias])[0]` (the round-trip
//!     path), with no Drive/Output/Mix/oversampling.
//!
//! Arg parsing is manual over `std::env::args` (no clap dependency):
//!   --input <path>     (required)
//!   --output <path>    (required)
//!   --bias <f32>       (default 0.5)
//!   --drive-db <f32>   (default 0)   full-chain only
//!   --output-db <f32>  (default 0)   full-chain only
//!   --mix <f32>        (default 100) full-chain only, percent (wet %)
//!   --backend <nn|wdf> (default nn)  full-chain only, inference backend
//!   --circuit-gain <f32> (default 220) WDF R_KJ shunt in ohms, --backend wdf only
//!   --tier <realtime|fast|ultra-fast> (default realtime) WDF solver tier, --backend wdf only
//!   --predict / --no-predict        WDF lever overrides (A/B), --backend wdf only
//!   --ls-trim / --no-ls-trim        WDF lever overrides (A/B), --backend wdf only
//!   --freeze-caps / --no-freeze-caps WDF lever overrides (A/B), --backend wdf only
//!   --cheap-exp / --no-cheap-exp    WDF lever override (ultra-fast low-order exp), --backend wdf only
//!   --tol <f64>                     WDF Newton tolerance override (A/B), --backend wdf only
//!   --max-iters <usize>             WDF Newton iteration cap override (A/B), --backend wdf only
//!   --cap-tier <full|depletion|off> WDF BJT charge-cap detail (A/B), --backend wdf only
//!   --os <usize>                    WDF internal oversampling (2 normal, 1 ultra-fast), --backend wdf only
//!   --solver <dense|schur>          WDF linear-solve path (A/B), --backend wdf only
//!   --raw              (flag) run the pure model (round-trip path)

use ba283::dsp::ChannelDsp;
use ba283::model::Ba283Model;
use ba283::wdf::{CapTier, QualityConfig, Solver};
use ba283::InferenceBackend;
use std::process::exit;

fn die(msg: &str) -> ! {
    eprintln!("error: {msg}");
    exit(1);
}

fn db_to_gain(db: f32) -> f32 {
    10.0f32.powf(db * 0.05)
}

fn main() {
    let mut input: Option<String> = None;
    let mut output: Option<String> = None;
    let mut bias: f32 = 0.5;
    let mut drive_db: f32 = 0.0;
    let mut output_db: f32 = 0.0;
    let mut mix_pct: f32 = 100.0;
    let mut backend = InferenceBackend::NeuralNet;
    let mut circuit_gain: f32 = 220.0;
    let mut raw = false;
    // WDF quality config: start at the realtime tier; `--tier fast` swaps in the fast preset, and
    // the per-lever flags toggle individual levers on top (for A/B validation).
    let mut wdf_cfg = QualityConfig::realtime();

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--raw" => raw = true,
            "--input" => {
                input = Some(args.next().unwrap_or_else(|| die("--input needs a value")));
            }
            "--output" => {
                output = Some(args.next().unwrap_or_else(|| die("--output needs a value")));
            }
            "--bias" => {
                let v = args.next().unwrap_or_else(|| die("--bias needs a value"));
                bias = v.parse().unwrap_or_else(|_| die("--bias must be a float"));
            }
            "--drive-db" => {
                let v = args
                    .next()
                    .unwrap_or_else(|| die("--drive-db needs a value"));
                drive_db = v
                    .parse()
                    .unwrap_or_else(|_| die("--drive-db must be a float"));
            }
            "--output-db" => {
                let v = args
                    .next()
                    .unwrap_or_else(|| die("--output-db needs a value"));
                output_db = v
                    .parse()
                    .unwrap_or_else(|_| die("--output-db must be a float"));
            }
            "--mix" => {
                let v = args.next().unwrap_or_else(|| die("--mix needs a value"));
                mix_pct = v.parse().unwrap_or_else(|_| die("--mix must be a float"));
            }
            "--backend" => {
                let v = args.next().unwrap_or_else(|| die("--backend needs a value"));
                backend = match v.as_str() {
                    "nn" => InferenceBackend::NeuralNet,
                    "wdf" => InferenceBackend::Wdf,
                    _ => die("--backend must be `nn` or `wdf`"),
                };
            }
            "--circuit-gain" => {
                let v = args
                    .next()
                    .unwrap_or_else(|| die("--circuit-gain needs a value"));
                circuit_gain = v
                    .parse()
                    .unwrap_or_else(|_| die("--circuit-gain must be a float"));
            }
            "--tier" => {
                let v = args.next().unwrap_or_else(|| die("--tier needs a value"));
                wdf_cfg = match v.as_str() {
                    "realtime" => QualityConfig::realtime(),
                    "fast" => QualityConfig::fast(),
                    "ultra-fast" => QualityConfig::ultra_fast(),
                    _ => die("--tier must be `realtime`, `fast`, or `ultra-fast`"),
                };
            }
            "--predict" => wdf_cfg.predict = true,
            "--no-predict" => wdf_cfg.predict = false,
            "--ls-trim" => wdf_cfg.ls_trim = true,
            "--no-ls-trim" => wdf_cfg.ls_trim = false,
            "--freeze-caps" => wdf_cfg.freeze_caps = true,
            "--no-freeze-caps" => wdf_cfg.freeze_caps = false,
            "--cheap-exp" => wdf_cfg.cheap_exp = true,
            "--no-cheap-exp" => wdf_cfg.cheap_exp = false,
            "--os" => {
                let v = args.next().unwrap_or_else(|| die("--os needs a value"));
                wdf_cfg.os = v.parse().unwrap_or_else(|_| die("--os must be a positive integer"));
                if wdf_cfg.os == 0 {
                    die("--os must be >= 1");
                }
            }
            "--decimate" => wdf_cfg.decimate = true,
            "--no-decimate" => wdf_cfg.decimate = false,
            "--tol" => {
                let v = args.next().unwrap_or_else(|| die("--tol needs a value"));
                wdf_cfg.newton_tol = v.parse().unwrap_or_else(|_| die("--tol must be a float"));
            }
            "--max-iters" => {
                let v = args.next().unwrap_or_else(|| die("--max-iters needs a value"));
                wdf_cfg.max_iters = v.parse().unwrap_or_else(|_| die("--max-iters must be an integer"));
            }
            "--cap-tier" => {
                let v = args.next().unwrap_or_else(|| die("--cap-tier needs a value"));
                wdf_cfg.cap_tier = match v.as_str() {
                    "full" => CapTier::Full,
                    "depletion" => CapTier::Depletion,
                    "off" => CapTier::Off,
                    _ => die("--cap-tier must be `full`, `depletion`, or `off`"),
                };
            }
            "--solver" => {
                let v = args.next().unwrap_or_else(|| die("--solver needs a value"));
                wdf_cfg.solver = match v.as_str() {
                    "dense" => Solver::Dense,
                    "schur" => Solver::Schur,
                    _ => die("--solver must be `dense` or `schur`"),
                };
            }
            other => die(&format!("unknown argument: {other}")),
        }
    }

    let input = input.unwrap_or_else(|| die("--input is required"));
    let output = output.unwrap_or_else(|| die("--output is required"));

    let reader = hound::WavReader::open(&input)
        .unwrap_or_else(|e| die(&format!("cannot open input WAV {input}: {e}")));
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;

    // Read all samples interleaved, preserving full float range (no clipping).
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .into_samples::<f32>()
            .collect::<Result<_, _>>()
            .unwrap_or_else(|e| die(&format!("error reading float samples: {e}"))),
        hound::SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .into_samples::<i32>()
                .map(|r| r.map(|v| v as f32 / max))
                .collect::<Result<_, _>>()
                .unwrap_or_else(|e| die(&format!("error reading int samples: {e}")))
        }
    };

    let out_spec = hound::WavSpec {
        channels: spec.channels,
        sample_rate: spec.sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(&output, out_spec)
        .unwrap_or_else(|e| die(&format!("cannot create output WAV {output}: {e}")));

    // The offline "host SR" is the input WAV's sample rate: a 48k input exercises
    // the oversampling path, a 96k input runs the model directly.
    let host_ge_96k = spec.sample_rate >= 96_000;

    if raw {
        // Pure model per channel — the round-trip path (no Drive/Output/Mix/OS).
        let mut models: Vec<Ba283Model> = (0..channels)
            .map(|_| Ba283Model::new_from_embedded_weights())
            .collect();
        let mut n_frames = 0usize;
        for frame in samples.chunks(channels) {
            for (ch, &s) in frame.iter().enumerate() {
                let wet = models[ch].forward(&[s, bias])[0];
                writer
                    .write_sample(wet)
                    .unwrap_or_else(|e| die(&format!("error writing sample: {e}")));
            }
            n_frames += 1;
        }
        writer
            .finalize()
            .unwrap_or_else(|e| die(&format!("error finalizing output WAV: {e}")));
        println!("rendered {} samples", n_frames * channels);
        return;
    }

    // Full chain — the SAME ChannelDsp the VST3 process uses, one per channel.
    let drive = db_to_gain(drive_db);
    let out_gain = db_to_gain(output_db);
    let mix = mix_pct * 0.01;
    let mut chans: Vec<ChannelDsp> = (0..channels).map(|_| ChannelDsp::new()).collect();
    // WDF backend: rebuild each channel at the input SR / circuit-gain / bias and warm it (off the
    // audio thread here) so the offline render mirrors the warmed plugin. NeuralNet needs no setup.
    if backend == InferenceBackend::Wdf {
        for ch in chans.iter_mut() {
            ch.wdf_rebuild_with(wdf_cfg, spec.sample_rate as f32, circuit_gain, bias);
            ch.wdf_warmup();
            // Zero the Newton counters so the reported avgIters reflect the audio pass only (warmup
            // transients run many iters and would skew the average), matching the C++ binary's count.
            ch.wdf_reset_iter_stats();
        }
    }

    let mut n_frames = 0usize;
    for frame in samples.chunks(channels) {
        for (ch, &s) in frame.iter().enumerate() {
            let y = chans[ch]
                .process_sample(s, drive, bias, out_gain, mix, host_ge_96k, backend);
            writer
                .write_sample(y)
                .unwrap_or_else(|e| die(&format!("error writing sample: {e}")));
        }
        n_frames += 1;
    }

    writer
        .finalize()
        .unwrap_or_else(|e| die(&format!("error finalizing output WAV: {e}")));

    // WDF: report Newton-iteration stats (audio pass only) to stderr, mirroring the C++ binary's
    // `Newton: ...` line so the offline A/B harness can compare per-iter solver cost across tiers.
    if backend == InferenceBackend::Wdf {
        let (mut samples, mut total, mut max) = (0u64, 0f64, 0usize);
        for ch in chans.iter() {
            let (s, avg, mx) = ch.wdf_iter_stats();
            samples += s;
            total += avg * s as f64;
            max = max.max(mx);
        }
        let avg = if samples > 0 { total / samples as f64 } else { 0.0 };
        eprintln!("Newton: samples={samples} avgIters={avg:.3} maxIters={max}");
    }

    println!("rendered {} samples", n_frames * channels);
}
