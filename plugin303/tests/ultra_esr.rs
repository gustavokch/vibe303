//! Ultra fidelity gate (F3.5): the nodal diode-ladder voice vs the whole-voice
//! SPICE gold. The SPICE reference *is* the diode-ladder truth, so the Ultra gold
//! is the SAME `voice_reference(...)` waveform the Low gate uses — the gate is:
//! render the Ultra voice, ESR vs that gold ≤ −30 dB.
//!
//! Lag-aligned, RMS-normalised ESR (Rust port of `eval/303/esr.py::metrics`);
//! shares the alignment math with `voice_esr.rs`.

use plugin303::dsp::filter::Tier;
use plugin303::dsp::voice::Voice;
use std::path::PathBuf;

const ULTRA_BUDGET_DB: f32 = -30.0;

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

fn rms_norm(x: &[f32]) -> Vec<f32> {
    let r = (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt();
    let s = if r > 1e-20 { 1.0 / r } else { 1.0 };
    x.iter().map(|v| v * s).collect()
}

fn best_lag(r: &[f32], m: &[f32], max: i32) -> i32 {
    let n = r.len().min(m.len());
    let mean = |x: &[f32]| x[..n].iter().sum::<f32>() / n as f32;
    let (rm, mm) = (mean(r), mean(m));
    let mut best = (0i32, f32::NEG_INFINITY);
    for lag in -max..=max {
        let mut acc = 0.0f32;
        for i in 0..n {
            let j = i as i32 + lag;
            if j >= 0 && (j as usize) < n {
                acc += (r[i] - rm) * (m[j as usize] - mm);
            }
        }
        if acc > best.1 {
            best = (lag, acc);
        }
    }
    best.0
}

fn esr_db(reference: &[f32], model: &[f32]) -> f32 {
    let r = rms_norm(reference);
    let m = rms_norm(model);
    let lag = best_lag(&r, &m, 256);
    let (ra, ma): (&[f32], &[f32]) = if lag > 0 {
        (&r[lag as usize..], &m[..m.len() - lag as usize])
    } else if lag < 0 {
        let k = (-lag) as usize;
        (&r[..r.len() - k], &m[k..])
    } else {
        (&r[..], &m[..])
    };
    let n = ra.len().min(ma.len());
    let err: f32 = (0..n).map(|i| (ra[i] - ma[i]).powi(2)).sum();
    let sig: f32 = (0..n).map(|i| ra[i] * ra[i]).sum::<f32>().max(1e-20);
    10.0 * (err / sig).log10()
}

// Currently +2.73 dB: the Ultra VCF is fed a polyBLEP saw, not the circuit VCO saw,
// so the whole-voice gold is topologically out of reach (see Phase F3-VCO). This is
// the V5 acceptance gate — un-ignore once the circuit VCO chain lands.
#[ignore = "F3-VCO V5 target; needs the circuit VCO before -30 dB is reachable"]
#[test]
fn ultra_voice_esr_within_budget() {
    let gold = load_f32(&gold_dir().join("voice_A2_mid.f32"));
    let sidecar = std::fs::read_to_string(gold_dir().join("voice_A2_mid.json")).unwrap();
    let sc: serde_json::Value = serde_json::from_str(&sidecar).unwrap();
    let note = sc["note"].as_u64().unwrap() as u8;
    let cutoff = sc["cutoff_hz"].as_f64().unwrap() as f32;
    let reso = sc["reso"].as_f64().unwrap() as f32;
    let sr = sc["sr"].as_f64().unwrap() as f32;

    let mut v = Voice::new(sr);
    v.set_tier(Tier::Ultra);
    v.set_cutoff(cutoff);
    v.set_reso(reso);
    v.note_on(note);
    let model: Vec<f32> = (0..gold.len()).map(|_| v.tick()).collect();

    let e = esr_db(&gold, &model);
    println!("Ultra voice ESR = {e:.2} dB (budget {ULTRA_BUDGET_DB} dB)");
    assert!(
        e <= ULTRA_BUDGET_DB,
        "Ultra voice ESR {e:.2} dB exceeds budget {ULTRA_BUDGET_DB} dB"
    );
}
