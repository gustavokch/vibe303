//! High fidelity gate (F4.2, spectral).
//!
//! High = polyBLEP saw → nodal diode-ladder VCF at 2× oversample. It skips the
//! costly nodal circuit VCO (Ultra's dominant cost) but keeps the nonlinear
//! diode-ladder filter character, so it should land between Low (TPT, corr≈0.27)
//! and Ultra (circuit VCO→ladder, corr 0.977) on the same spectral metric.
//!
//! Same Welch LSD + dB-spectrum-corr metric as Ultra (`tests/common/mod.rs`).
//! The budget is necessarily looser than Ultra: the polyBLEP saw ≠ the circuit
//! relaxation saw, so the source harmonic structure differs even though the
//! filter is identical. The gold-power-weighted LSD is large for any non-circuit
//! saw (it concentrates on the gold's dominant bins), so High's LSD lands far
//! from Ultra's — but well below Low's. Side-by-side at the gold op-point
//! (`high_spectral_report`, note 57 / 1002 Hz / reso 0.85):
//!     Low    LSD 42.16 dB  corr 0.931
//!     High   LSD 28.78 dB  corr 0.971   <- between the two on both axes
//!     Ultra  LSD  4.42 dB  corr 0.977
//! The gate proves High is a genuine fidelity step over Low: budgets sit below
//! Low's numbers so a regression to the TPT path would fail.

mod common;

use common::{gold_dir, load_f32, load_gold_op, spectral_match};
use plugin303::dsp::filter::Tier;
use plugin303::dsp::voice::Voice;

const LSD_BUDGET_DB: f32 = 32.0; // achieved 28.78 dB + margin; Low's 42.16 fails
const CORR_MIN: f32 = 0.95; // achieved 0.971; Low's 0.931 fails → real discriminator

fn render_high(sr: f32, cutoff: f32, reso: f32, note: u8, n: usize) -> Vec<f32> {
    let mut v = Voice::new(sr);
    v.set_tier_immediate(Tier::High);
    v.set_cutoff(cutoff);
    v.set_reso(reso);
    v.note_on(note);
    (0..n).map(|_| v.tick()).collect()
}

fn render_tier(tier: Tier, op: &common::GoldOp, n: usize) -> Vec<f32> {
    let mut v = Voice::new(op.sr);
    v.set_tier_immediate(tier);
    v.set_cutoff(op.cutoff);
    v.set_reso(op.reso);
    v.note_on(op.note);
    (0..n).map(|_| v.tick()).collect()
}

#[test]
#[ignore = "diagnostic: report Low/High/Ultra LSD+corr side by side"]
fn high_spectral_report() {
    let gold = load_f32(&gold_dir().join("voice_A2_mid.f32"));
    let op = load_gold_op("voice_A2_mid.json");
    println!("note={} cutoff={:.0}Hz reso={:.2} sr={:.0}", op.note, op.cutoff, op.reso, op.sr);
    for (name, tier) in [("Low", Tier::Low), ("High", Tier::High), ("Ultra", Tier::Ultra)] {
        let m = render_tier(tier, &op, gold.len());
        let (lsd, corr) = spectral_match(&gold, &m, op.sr);
        println!("{name:5}  LSD = {lsd:6.2} dB   corr = {corr:.3}");
    }
}

#[test]
#[ignore = "diagnostic: per-sample wall-clock cost Low/High/Ultra"]
fn cost_bench() {
    use std::time::Instant;
    let op = load_gold_op("voice_A2_mid.json");
    let n = (op.sr as usize) * 2; // 2 s
    for (name, tier) in [("Low", Tier::Low), ("High", Tier::High), ("Ultra", Tier::Ultra)] {
        let mut v = Voice::new(op.sr);
        v.set_tier_immediate(tier);
        v.set_cutoff(op.cutoff);
        v.set_reso(op.reso);
        v.note_on(op.note);
        for _ in 0..256 {
            v.tick(); // warm-start out of the timed region
        }
        let t0 = Instant::now();
        let mut acc = 0.0f32;
        for _ in 0..n {
            acc += v.tick();
        }
        let ns = t0.elapsed().as_nanos() as f64 / n as f64;
        std::hint::black_box(acc);
        let rt = ns * op.sr as f64 / 1e9; // CPU fraction of one core at real time
        println!("{name:5}  {ns:7.1} ns/sample   {:5.2}% of one core @ {:.0} Hz", rt * 100.0, op.sr);
    }
}

#[test]
fn high_voice_spectral_match() {
    let gold = load_f32(&gold_dir().join("voice_A2_mid.f32"));
    let op = load_gold_op("voice_A2_mid.json");

    let model = render_high(op.sr, op.cutoff, op.reso, op.note, gold.len());
    let (lsd, corr) = spectral_match(&gold, &model, op.sr);
    println!("High spectral match: LSD = {lsd:.2} dB, corr = {corr:.3}");
    assert!(lsd <= LSD_BUDGET_DB, "High LSD {lsd:.2} dB exceeds budget {LSD_BUDGET_DB} dB");
    assert!(corr >= CORR_MIN, "High spectral corr {corr:.3} below {CORR_MIN}");
}
