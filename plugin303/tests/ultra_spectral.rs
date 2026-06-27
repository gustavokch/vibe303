//! Ultra fidelity gate (F3-VCO V5, spectral redefinition).
//!
//! Sample-exact ESR vs ngspice is structurally out of reach for a real-time
//! bespoke chain (see design doc F3-VCO V4). The acceptance is redefined to a
//! **spectral character match**: does the Ultra voice have the same harmonic
//! structure / brightness / resonance envelope as the whole-voice SPICE gold?
//!
//! Metric: Welch-averaged log-power spectra (Hann, 50 % overlap), power-
//! normalised, compared over the audible band by
//!   - LSD  : RMS log-spectral distance in dB (lower = closer envelope), and
//!   - corr : Pearson correlation of the two dB spectra (higher = same shape).
//! Both are phase/pitch-micro-tolerant, unlike ESR — they gate *sound*, not
//! sample-for-sample identity.

use plugin303::dsp::filter::Tier;
use plugin303::dsp::voice::Voice;
use std::f64::consts::PI;
use std::path::PathBuf;

const SEG: usize = 1024;
const LSD_BUDGET_DB: f32 = 5.5; // achieved 4.42 dB (gold-power-weighted) + margin
const CORR_MIN: f32 = 0.95; // achieved corr 0.977; spectra share their shape
const ULTRA_BRIGHTNESS: f64 = 1.0; // VCF brightness cal (cal=1.0 is the LSD optimum)

fn gold_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../eval/303/gold")
}

fn load_f32(path: &PathBuf) -> Vec<f32> {
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
fn welch_pow(x: &[f32]) -> Vec<f64> {
    let hop = SEG / 2;
    let win: Vec<f64> = (0..SEG)
        .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f64 / SEG as f64).cos())
        .collect();
    let mut acc = vec![0.0f64; SEG / 2];
    let mut nseg = 0usize;
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
        nseg += 1;
        start += hop;
    }
    let total: f64 = acc.iter().sum::<f64>().max(1e-300);
    acc.iter().map(|&v| v / total).collect()
}

/// (weighted LSD dB, Pearson corr) over the audible band [40 Hz, 16 kHz].
/// The LSD weights each bin by the gold's power, so the score reflects the
/// envelope *where the gold actually has energy* (noise-floor bins above the
/// corner don't dominate). The correlation is over the unweighted dB spectra.
fn spectral_match(gold: &[f32], model: &[f32], sr: f32) -> (f32, f32) {
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

fn render_ultra(sr: f32, cutoff: f32, reso: f32, note: u8, n: usize, cal: f64) -> Vec<f32> {
    let mut v = Voice::new(sr);
    v.set_tier(Tier::Ultra);
    v.set_ultra_brightness(cal);
    v.set_cutoff(cutoff);
    v.set_reso(reso);
    v.note_on(note);
    (0..n).map(|_| v.tick()).collect()
}

#[test]
#[ignore = "diagnostic: sweep VCF brightness to minimise spectral LSD"]
fn brightness_sweep() {
    let gold = load_f32(&gold_dir().join("voice_A2_mid.f32"));
    let sidecar = std::fs::read_to_string(gold_dir().join("voice_A2_mid.json")).unwrap();
    let sc: serde_json::Value = serde_json::from_str(&sidecar).unwrap();
    let note = sc["note"].as_u64().unwrap() as u8;
    let cutoff = sc["cutoff_hz"].as_f64().unwrap() as f32;
    let reso = sc["reso"].as_f64().unwrap() as f32;
    let sr = sc["sr"].as_f64().unwrap() as f32;
    for cal in [1.0f64, 2.0, 4.0, 7.0, 10.0, 15.0, 20.0] {
        let m = render_ultra(sr, cutoff, reso, note, gold.len(), cal);
        let (lsd, corr) = spectral_match(&gold, &m, sr);
        println!("cal={cal:5.1}  LSD={lsd:.2} dB  corr={corr:.3}");
    }
}

#[test]
fn ultra_voice_spectral_match() {
    let gold = load_f32(&gold_dir().join("voice_A2_mid.f32"));
    let sidecar = std::fs::read_to_string(gold_dir().join("voice_A2_mid.json")).unwrap();
    let sc: serde_json::Value = serde_json::from_str(&sidecar).unwrap();
    let note = sc["note"].as_u64().unwrap() as u8;
    let cutoff = sc["cutoff_hz"].as_f64().unwrap() as f32;
    let reso = sc["reso"].as_f64().unwrap() as f32;
    let sr = sc["sr"].as_f64().unwrap() as f32;

    let model = render_ultra(sr, cutoff, reso, note, gold.len(), ULTRA_BRIGHTNESS);
    let (lsd, corr) = spectral_match(&gold, &model, sr);
    println!("Ultra spectral match: LSD = {lsd:.2} dB, corr = {corr:.3}");
    assert!(lsd <= LSD_BUDGET_DB, "Ultra LSD {lsd:.2} dB exceeds budget {LSD_BUDGET_DB} dB");
    assert!(corr >= CORR_MIN, "Ultra spectral corr {corr:.3} below {CORR_MIN}");
}
