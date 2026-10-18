//! Headless 303 voice renderer for the ESR harness. Emits raw `f32le` (no wav dep).
//! Args: --note --sr --secs --cutoff --reso --tier --out
use plugin303::dsp::filter::Tier;
use plugin303::dsp::voice::Voice;
use plugin303::params::OscWave;
use std::io::Write;

fn arg(flag: &str, default: f32) -> f32 {
    let a: Vec<String> = std::env::args().collect();
    a.iter()
        .position(|s| s == flag)
        .and_then(|i| a.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn arg_s(flag: &str, default: &str) -> String {
    let a: Vec<String> = std::env::args().collect();
    a.iter()
        .position(|s| s == flag)
        .and_then(|i| a.get(i + 1))
        .cloned()
        .unwrap_or_else(|| default.into())
}

fn main() {
    let sr = arg("--sr", 48_000.0);
    let note = arg("--note", 57.0) as u8; // A2
    let secs = arg("--secs", 0.5);
    let cutoff = arg("--cutoff", 1_000.0);
    let reso = arg("--reso", 0.0);
    let tier_name = arg_s("--tier", "draft"); // draft|low|high|best; default keeps F2 golds valid
    let tier = Tier::parse(&tier_name).unwrap_or_else(|| {
        eprintln!("render303: unknown --tier {tier_name:?} (expected draft, low, high or best)");
        std::process::exit(2);
    });
    let wave = arg("--wave", 0.0) as i32; // 0=Saw, 1=Square, 2=Sweep
    let out = arg_s("--out", "render.f32");

    let gate_vca = arg("--gate-vca", 0.0) > 0.5;
    let env_mod = arg("--env-mod", 0.0); // Default to 0.0 if not specified to allow static cutoff

    let mut v = Voice::new(sr);
    v.set_tier_immediate(tier);
    let wave_mode = match wave {
        1 => OscWave::Square,
        2 => OscWave::Sweep,
        _ => OscWave::Saw,
    };
    v.set_wave(wave_mode);
    v.set_cutoff(cutoff);
    v.set_reso(reso / 100.0); // --reso is a percentage; the voice wants 0..1

    let topology_mode = if wave_mode == OscWave::Sweep {
        plugin303::params::CircuitTopology::Fun
    } else {
        plugin303::params::CircuitTopology::Accurate
    };

    v.update_params(
        0.0,       // tuning_cents
        0.0,       // osc1_pitch_st
        0.0,       // osc2_pitch_st
        0.0,       // osc2_vol
        wave_mode, // osc1_wave (keep wave set by set_wave)
        0.5,       // osc1_pw
        0.0,       // sub_vol
        cutoff,
        reso / 100.0,
        env_mod,
        0.5, // decay_knob
        0.0, // accent_knob
        0.0, // volume_db
        plugin303::params::VcfTaps::FourPole,
        0.0,   // vcf_drive_db
        false, // advanced_env
        0.0,
        0.0,
        0.0,
        0.0, // meg ADSR
        0.0,
        0.0,
        0.0,
        0.0, // veg ADSR
        if gate_vca {
            plugin303::params::VcaEnvMode::Gate
        } else {
            plugin303::params::VcaEnvMode::Envelope
        },
        0,   // comp_age
        0.0, // comp_drift
        plugin303::params::CompTolerance::Zero,
        0, // thermal_noise
        topology_mode,
        0.0, // sweep_pos
        0,   // comp_seed
        0.5, // glide_knob
        0.0, // reso_comp
        1.0, // recalibration_scale
    );

    if gate_vca {
        v.set_vca_env_mode(plugin303::params::VcaEnvMode::Gate);
    }

    v.note_on(note);
    let n = (secs * sr) as usize;
    let mut buf = Vec::with_capacity(n);
    for _ in 0..n {
        buf.push(v.tick());
    }

    let mut f = std::fs::File::create(&out).unwrap();
    for s in &buf {
        f.write_all(&s.to_le_bytes()).unwrap();
    }
}
