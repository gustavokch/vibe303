//! Fidelity gate: the Low/TPT voice vs a committed whole-voice SPICE gold waveform.
//! Lag-aligned, RMS-normalised ESR (the Rust port of `eval/303/esr.py::metrics`).
//!
//! Per the F2.4 finding the Low/TPT gap is topological (~+2.5 dB ESR) and cannot be
//! calibrated toward SPICE, so this is a REGRESSION GUARD on the TPT path, not a
//! fidelity target. The WDF tiers (F3/F4) get their own, far tighter gates.

mod common;

use plugin303::dsp::voice::Voice;
use std::path::PathBuf;

const BUDGET_DB: f32 = 3.5; // measured ~+2.5 dB (A2 mid) + margin

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

/// Best integer lag in ±max that aligns `m` to `r` (demeaned cross-correlation).
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

/// Lag-aligned error-to-signal ratio in dB.
fn esr_db(reference: &[f32], model: &[f32]) -> f32 {
    let r = rms_norm(reference);
    let m = rms_norm(model);
    let lag = best_lag(&r, &m, 256);
    // Align like esr.py::_align.
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

#[test]
fn low_voice_esr_within_budget() {
    let gold = load_f32(&gold_dir().join("voice_A2_mid.f32"));
    let sidecar = std::fs::read_to_string(gold_dir().join("voice_A2_mid.json")).unwrap();
    let sc: serde_json::Value = serde_json::from_str(&sidecar).unwrap();
    let note = sc["note"].as_u64().unwrap() as u8;
    let cutoff = sc["cutoff_hz"].as_f64().unwrap() as f32;
    let reso = sc["reso"].as_f64().unwrap() as f32;
    let sr = sc["sr"].as_f64().unwrap() as f32;

    let mut v = Voice::new(sr);
    // Gold is the raw voice SPICE sim without the output clipper; bypass it
    // so this stays a regression guard on the TPT voice path only.
    v.update_clipper_params_immediate(common::bypassed_clipper());
    v.set_cutoff(cutoff);
    v.set_reso(reso);
    v.note_on(note);
    let model: Vec<f32> = (0..gold.len()).map(|_| v.tick()).collect();

    let e = esr_db(&gold, &model); // measured ~2.44 dB at F2.6
    assert!(
        e <= BUDGET_DB,
        "Low voice ESR {e:.2} dB exceeds budget {BUDGET_DB} dB (regressed?)"
    );
}
