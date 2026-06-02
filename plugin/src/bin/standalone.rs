//! Offline WAV renderer for the BA283 model.
//!
//! This is a CUSTOM offline file renderer, NOT nih_plug's realtime standalone
//! (which cannot file-render). It reads a WAV, runs the hand-coded GRU per
//! sample, and writes a WAV. In Tasks 3.2/3.3 ONLY the `--raw` (pure model)
//! path is supported. The full DSP chain (Drive/Output/Mix/oversampling) and
//! the `--drive-db` / `--output-db` / `--mix` flags land in Task 3.4; until
//! then any invocation that implies the full chain fails loudly so a premature
//! Phase-4 run cannot pass against the bare model by mistake.
//!
//! Arg parsing is manual over `std::env::args` (no clap dependency):
//!   --input <path>   (required)
//!   --output <path>  (required)
//!   --bias <f32>     (default 0.5)
//!   --raw            (flag, REQUIRED for now) run pure model:
//!                    out = forward([sample, bias])[0]
//!   --drive-db / --output-db / --mix <f32>  NOT implemented until 3.4;
//!     passing any of them is a hard error.

use ba283::model::Ba283Model;
use std::process::exit;

fn die(msg: &str) -> ! {
    eprintln!("error: {msg}");
    exit(1);
}

fn main() {
    let mut input: Option<String> = None;
    let mut output: Option<String> = None;
    let mut bias: f32 = 0.5;
    let mut raw = false;

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
            // Not implemented until 3.4 — fail loudly if present (with any value).
            "--drive-db" | "--output-db" | "--mix" => {
                die("--drive-db/--output-db/--mix not implemented until Task 3.4");
            }
            other => die(&format!("unknown argument: {other}")),
        }
    }

    let input = input.unwrap_or_else(|| die("--input is required"));
    let output = output.unwrap_or_else(|| die("--output is required"));
    // The full DSP chain lands in 3.4; until then only --raw is supported.
    if !raw {
        die(
            "full DSP chain (drive/output/mix/oversampling) not wired until Task 3.4; \
             pass --raw for pure-model rendering",
        );
    }

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

    // One model per channel.
    let mut models: Vec<Ba283Model> = (0..channels)
        .map(|_| Ba283Model::new_from_embedded_weights())
        .collect();

    let out_spec = hound::WavSpec {
        channels: spec.channels,
        sample_rate: spec.sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(&output, out_spec)
        .unwrap_or_else(|e| die(&format!("cannot create output WAV {output}: {e}")));

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

    // Number of model invocations = frames * channels (mono round-trip => frames).
    println!("rendered {} samples", n_frames * channels);
}
