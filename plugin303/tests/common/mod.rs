//! Shared spectral-match helpers for the tier fidelity gates (F3-VCO V5, F4.2).
//!
//! Welch-averaged log-power spectra (Hann, 50 % overlap), power-normalised,
//! compared over the audible band by:
//!   - LSD  : gold-power-weighted RMS log-spectral distance in dB, and
//!   - corr : Pearson correlation of the two dB spectra.
//! Both gate *sound* (harmonic structure / brightness / resonance envelope),
//! not sample-for-sample identity — sample-exact ESR vs ngspice is structurally
//! unreachable for a real-time bespoke chain (design doc F3-VCO V4).

#![allow(dead_code)]

use std::f64::consts::PI;
use std::path::PathBuf;

pub const SEG: usize = 1024;

pub fn gold_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../eval/303/gold")
}

pub fn load_f32(path: &PathBuf) -> Vec<f32> {
    let bytes = std::fs::read(path).expect("gold pcm missing; run eval/303/gen_gold.py");
    bytes
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}

fn fft(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * PI / len as f64;
        let (wr, wi) = (ang.cos(), ang.sin());
        let mut i = 0;
        while i < n {
            let (mut cr, mut ci) = (1.0f64, 0.0f64);
            for k in 0..len / 2 {
                let a = i + k;
                let b = i + k + len / 2;
                let tr = cr * re[b] - ci * im[b];
                let ti = cr * im[b] + ci * re[b];
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let ncr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = ncr;
            }
            i += len;
        }
        len <<= 1;
    }
}

/// Welch-averaged linear power spectrum (normalised to unit sum).
pub fn welch_pow(x: &[f32]) -> Vec<f64> {
    let hop = SEG / 2;
    let win: Vec<f64> = (0..SEG)
        .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f64 / SEG as f64).cos())
        .collect();
    let mut acc = vec![0.0f64; SEG / 2];
    let mut start = 0;
    while start + SEG <= x.len() {
        let mut re = vec![0.0f64; SEG];
        let mut im = vec![0.0f64; SEG];
        for i in 0..SEG {
            re[i] = x[start + i] as f64 * win[i];
        }
        fft(&mut re, &mut im);
        for k in 0..SEG / 2 {
            acc[k] += re[k] * re[k] + im[k] * im[k];
        }
        start += hop;
    }
    let total: f64 = acc.iter().sum::<f64>().max(1e-300);
    acc.iter().map(|&v| v / total).collect()
}

/// (gold-power-weighted LSD dB, Pearson corr) over the audible band
/// [40 Hz, 16 kHz]. The LSD weights each bin by the gold's power so the score
/// reflects the envelope *where the gold actually has energy* (noise-floor bins
/// above the corner don't dominate). The correlation is over the unweighted dB
/// spectra.
pub fn spectral_match(gold: &[f32], model: &[f32], sr: f32) -> (f32, f32) {
    let gp = welch_pow(gold);
    let mp = welch_pow(model);
    let df = sr / SEG as f32;
    let lo = (40.0 / df) as usize;
    let hi = ((16_000.0 / df) as usize).min(gp.len());

    let to_db = |p: f64| 10.0 * (p.max(1e-300)).log10();
    let demean = |v: &[f64]| {
        let m: f64 = v.iter().map(|&p| to_db(p)).sum::<f64>() / v.len() as f64;
        v.iter().map(|&p| to_db(p) - m).collect::<Vec<f64>>()
    };
    let gdb = demean(&gp[lo..hi]);
    let mdb = demean(&mp[lo..hi]);
    let gw = &gp[lo..hi];

    // Gold-power-weighted RMS log-spectral distance.
    let (mut num, mut den) = (0.0f64, 0.0f64);
    for i in 0..gdb.len() {
        num += gw[i] * (gdb[i] - mdb[i]).powi(2);
        den += gw[i];
    }
    let lsd = (num / den.max(1e-300)).sqrt() as f32;

    // Unweighted Pearson correlation of the dB spectra.
    let n = gdb.len() as f64;
    let (gm, mm) = (gdb.iter().sum::<f64>() / n, mdb.iter().sum::<f64>() / n);
    let (mut cov, mut gv, mut mv) = (0.0, 0.0, 0.0);
    for i in 0..gdb.len() {
        cov += (gdb[i] - gm) * (mdb[i] - mm);
        gv += (gdb[i] - gm).powi(2);
        mv += (mdb[i] - mm).powi(2);
    }
    let corr = (cov / (gv.sqrt() * mv.sqrt()).max(1e-12)) as f32;
    (lsd, corr)
}

/// Gold sidecar fields (note, cutoff, reso, sr) for the committed voice gold.
pub struct GoldOp {
    pub note: u8,
    pub cutoff: f32,
    pub reso: f32,
    pub sr: f32,
}

pub fn load_gold_op(name: &str) -> GoldOp {
    let sidecar = std::fs::read_to_string(gold_dir().join(name)).unwrap();
    let sc: serde_json::Value = serde_json::from_str(&sidecar).unwrap();
    GoldOp {
        note: sc["note"].as_u64().unwrap() as u8,
        cutoff: sc["cutoff_hz"].as_f64().unwrap() as f32,
        reso: sc["reso"].as_f64().unwrap() as f32,
        sr: sc["sr"].as_f64().unwrap() as f32,
    }
}

/// Clipper settings for tests that bypass the output clipper entirely.
pub fn bypassed_clipper() -> plugin303::dsp::voice::VoiceClipperParams {
    plugin303::dsp::voice::VoiceClipperParams {
        bypass: true,
        os: plugin303::params::ClipperOversampling::X2,
        drive: 47_000.0,
        threshold: 4_700.0,
        bass: 8_200.0,
        tone: 1e-9,
        asym: false,
        mix: 1.0,
        in_db: 0.0,
        out_db: 0.0,
        tilt_db: 0.0,
        dynamics: 0.0,
    }
}

/// Applies the default parameter set the way `Plugin303::process` does on every
/// block (`lib.rs` `update_params` call), with `comp_age` overridden. The plugin
/// re-applies analog wear on its first block (`comp_seed` 1337 vs `Voice::new`'s
/// 0) and after each host reset; tests that skip this never see that re-wear.
pub fn apply_plugin_defaults(v: &mut plugin303::dsp::voice::Voice, comp_age: i32) {
    let env_mod = plugin303::params::Plugin303Params::default()
        .env_mod
        .value();
    apply_plugin_patch(v, comp_age, env_mod);
}

/// [`apply_plugin_defaults`] with the Env Mod knob overridden as well.
fn apply_plugin_patch(v: &mut plugin303::dsp::voice::Voice, comp_age: i32, env_mod: f32) {
    let p = plugin303::params::Plugin303Params::default();
    v.update_params(
        p.tuning.value(),
        p.osc1_pitch.value(),
        p.osc2_pitch.value(),
        p.osc2_vol.value(),
        p.waveform.value(),
        p.osc1_pw.value(),
        p.sub_vol.value(),
        p.cutoff.value(),
        p.resonance.value(),
        env_mod,
        p.decay.value(),
        p.accent.value(),
        p.volume.value(),
        p.vcf_taps.value(),
        p.vcf_drive.value(),
        p.advanced_env.value(),
        p.meg_attack.value(),
        p.meg_decay_adj.value(),
        p.meg_sustain.value(),
        p.meg_release.value(),
        p.veg_attack.value(),
        p.veg_decay_adj.value(),
        p.veg_sustain.value(),
        p.veg_release.value(),
        p.vca_env_mode.value(),
        comp_age,
        p.comp_drift.value(),
        p.tolerance.value(),
        p.thermal_noise.value(),
        p.topology.value(),
        p.sweep_pos.value(),
        p.comp_seed.value(),
        p.glide.value(),
        p.reso_comp.value(),
    );
}

/// Renders `tier` at gold operating point `op` through the plugin's parameter
/// path, then plays the note the way the spectral gates do (bypassed clipper,
/// advanced envelopes, 48 warm-up samples). The path is the default patch (Fun
/// topology, component seed 1337, drift 0.1, first-block analog wear) with Env
/// Mod at 0: the gold holds its cutoff still, and the default Env Mod (0.5)
/// sweeps it, which alone moves Best from 3.5 to 22 dB LSD. A bare `Voice::new`
/// render never runs `update_params`, so only this render sees an update that
/// overrides a tier's calibrated voicing.
pub fn render_plugin_path(tier: plugin303::dsp::filter::Tier, op: &GoldOp, n: usize) -> Vec<f32> {
    // As `Plugin303::initialize` does: build the FUN nominal LUT before the
    // first block switches the voice to FUN.
    plugin303::dsp::vco::Vco303::warm_cache_nominal(op.sr, true);
    let mut v = plugin303::dsp::voice::Voice::new(op.sr);
    v.update_clipper_params_immediate(bypassed_clipper());
    v.set_tier_immediate(tier);
    apply_plugin_patch(&mut v, 0, 0.0);
    v.set_advanced_env(true);
    v.set_cutoff(op.cutoff);
    v.set_reso(op.reso);
    v.note_on(op.note);
    for _ in 0..48 {
        v.tick();
    }
    (0..n).map(|_| v.tick()).collect()
}
