use std::env;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use plugin303::dsp::warm_clipper::{ClipperOs, ClipperParams, WarmClipper};

fn print_usage() {
    eprintln!(
        "Usage: eval_clipper [OPTIONS]\n\n\
         Options:\n\
         --sr <float>        Sample rate (default: 48000.0)\n\
         --os <2x|4x>        Oversampling mode (default: 2x)\n\
         --bypass            Bypass clipper\n\
         --drive <float>     Drive resistance in Ohms (22000.0..100000.0, default: 47000.0)\n\
         --thresh <float>    Threshold resistance in Ohms (2200.0..22000.0, default: 4700.0)\n\
         --bass <float>      Bass resistance in Ohms (4700.0..22000.0, default: 8200.0)\n\
         --tone <float>      Tone capacitance in Farads (default: 1e-9)\n\
         --asym              Asymmetric clipping mode\n\
         --mix <float>       Dry/wet mix 0.0..1.0 (default: 1.0)\n\
         --in-gain <float>   Input gain dB -18..18 (default: 0.0)\n\
         --out-gain <float>  Output gain dB -18..18 (default: 0.0)\n\
         --tilt <float>      Pre/post tilt dB -6..6 (default: 0.0)\n\
         --dynamics <float>  Transient ducking 0.0..1.0 (default: 0.0)\n\
         --signal <mode>     Signal type: sine, sweep, raw (default: sine)\n\
         --freq <float>      Fundamental frequency in Hz for sine signal (default: 1700.0)\n\
         --amp <float>       Peak amplitude (default: 1.0)\n\
         --secs <float>      Duration in seconds (default: 1.0)\n\
         --in <path>         Input f32 little-endian file (for raw signal)\n\
         --out <path>        Output f32 little-endian file (required)\n"
    );
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() <= 1 {
        print_usage();
        return Ok(());
    }

    let mut sr: f32 = 48000.0;
    let mut os_mode = ClipperOs::X2;
    let mut bypass = false;
    let mut drive: f32 = 47000.0;
    let mut thresh: f32 = 4700.0;
    let mut bass: f32 = 8200.0;
    let mut tone: f32 = 1e-9;
    let mut asym = false;
    let mut mix: f32 = 1.0;
    let mut in_gain: f32 = 0.0;
    let mut out_gain: f32 = 0.0;
    let mut tilt: f32 = 0.0;
    let mut dynamics: f32 = 0.0;
    let mut signal_type = String::from("sine");
    let mut freq: f32 = 1700.0;
    let mut amp: f32 = 1.0;
    let mut secs: f32 = 1.0;
    let mut input_path: Option<String> = None;
    let mut output_path: Option<String> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--sr" => {
                i += 1;
                sr = args[i].parse()?;
            }
            "--os" => {
                i += 1;
                match args[i].as_str() {
                    "2x" | "x2" | "2" => os_mode = ClipperOs::X2,
                    "4x" | "x4" | "4" => os_mode = ClipperOs::X4,
                    other => return Err(format!("Unknown --os mode: {}", other).into()),
                }
            }
            "--bypass" => bypass = true,
            "--drive" => {
                i += 1;
                drive = args[i].parse()?;
            }
            "--thresh" => {
                i += 1;
                thresh = args[i].parse()?;
            }
            "--bass" => {
                i += 1;
                bass = args[i].parse()?;
            }
            "--tone" => {
                i += 1;
                tone = args[i].parse()?;
            }
            "--asym" => asym = true,
            "--mix" => {
                i += 1;
                mix = args[i].parse()?;
            }
            "--in-gain" => {
                i += 1;
                in_gain = args[i].parse()?;
            }
            "--out-gain" => {
                i += 1;
                out_gain = args[i].parse()?;
            }
            "--tilt" => {
                i += 1;
                tilt = args[i].parse()?;
            }
            "--dynamics" => {
                i += 1;
                dynamics = args[i].parse()?;
            }
            "--signal" => {
                i += 1;
                signal_type = args[i].to_lowercase();
            }
            "--freq" => {
                i += 1;
                freq = args[i].parse()?;
            }
            "--amp" => {
                i += 1;
                amp = args[i].parse()?;
            }
            "--secs" => {
                i += 1;
                secs = args[i].parse()?;
            }
            "--in" => {
                i += 1;
                input_path = Some(args[i].clone());
            }
            "--out" => {
                i += 1;
                output_path = Some(args[i].clone());
            }
            "--help" | "-h" => {
                print_usage();
                return Ok(());
            }
            unknown => return Err(format!("Unknown option: {}", unknown).into()),
        }
        i += 1;
    }

    let out_path_str = match output_path {
        Some(p) => p,
        None => return Err("Missing required option: --out <path>".into()),
    };

    let total_samples = (sr * secs).round() as usize;

    let input_samples: Vec<f32> = match signal_type.as_str() {
        "sine" => (0..total_samples)
            .map(|n| {
                let t = n as f32 / sr;
                amp * (2.0 * std::f32::consts::PI * freq * t).sin()
            })
            .collect(),
        "sweep" => {
            // Logarithmic frequency sweep from 20 Hz to min(sr*0.48, 20000 Hz)
            let f_start: f32 = 20.0;
            let f_end: f32 = (sr * 0.48).min(20000.0);
            (0..total_samples)
                .map(|n| {
                    let frac = n as f32 / (total_samples as f32 - 1.0).max(1.0);
                    // Phase integral for log sweep
                    let phase = 2.0
                        * std::f32::consts::PI
                        * f_start
                        * secs
                        / (f_end / f_start).ln()
                        * ((f_end / f_start).powf(frac) - 1.0);
                    amp * phase.sin()
                })
                .collect()
        }
        "raw" => {
            let path_str = input_path.ok_or("Missing --in for raw signal mode")?;
            let mut file = File::open(&path_str)?;
            let mut buf = Vec::new();
            file.read_to_end(&mut buf)?;
            if buf.len() % 4 != 0 {
                return Err("Input raw file size must be a multiple of 4 bytes (f32)".into());
            }
            buf.chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect()
        }
        other => return Err(format!("Unknown signal type: {}", other).into()),
    };

    let params = ClipperParams {
        bypass,
        os: os_mode,
        drive_ohms: drive,
        threshold_ohms: thresh,
        bass_ohms: bass,
        tone_f: tone,
        asym,
        mix,
        in_db: in_gain,
        out_db: out_gain,
        tilt_db: tilt,
        dynamics,
    };

    let mut clipper = WarmClipper::new(sr);
    clipper.set_params_immediate(params);

    // Warm-up to flush transient responses
    let warmup_samples = 256;
    for _ in 0..warmup_samples {
        clipper.tick(0.0);
    }

    let output_samples: Vec<f32> = input_samples.iter().map(|&x| clipper.tick(x)).collect();

    let mut out_file = File::create(Path::new(&out_path_str))?;
    for &sample in &output_samples {
        out_file.write_all(&sample.to_le_bytes())?;
    }

    Ok(())
}
