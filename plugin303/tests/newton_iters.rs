//! Diagnostic (ignored): Newton iterations-per-sample histogram for the Ultra
//! signal chain (nodal VCO + 4× nodal diode-ladder VCF). Sizes optimization lever
//! #4 (iter count multiplies every per-iter cost). Not a gate.
//!
//! Run: `cargo test -p plugin303 --test newton_iters -- --ignored --nocapture`.
//! Mirrors `Voice::render_tier(Ultra)`: VCO saw ×2.0 gain → Os4x.up → 4× ladder
//! tick → Os4x.down, ladder built at 4×sr with the Ultra brightness (0.70).

use plugin303::dsp::filter::Filter303;
use plugin303::dsp::ladder_wdf::LadderVcf;
use plugin303::dsp::oversample::Os4x;
use plugin303::dsp::vco::Vco303;

fn midi_to_freq(n: u8) -> f32 {
    440.0 * 2.0f32.powf((n as f32 - 69.0) / 12.0)
}

/// Print a compact histogram (count + % + cumulative) over iter counts.
fn dump(label: &str, hist: &[u64], total: u64, homotopy: u64) {
    let sum_iters: u64 = hist.iter().enumerate().map(|(i, &c)| i as u64 * c).sum();
    eprintln!(
        "\n== {label} ==  samples={total}  mean_iters={:.3}  homotopy_fallbacks={homotopy} ({:.2}%)",
        sum_iters as f64 / total as f64,
        100.0 * homotopy as f64 / total as f64,
    );
    let mut cum = 0u64;
    for (i, &c) in hist.iter().enumerate() {
        if c == 0 {
            continue;
        }
        cum += c;
        eprintln!(
            "  {i:3} iters : {c:8}  {:5.2}%   cum {:6.2}%",
            100.0 * c as f64 / total as f64,
            100.0 * cum as f64 / total as f64,
        );
    }
}

#[test]
#[ignore = "diagnostic: Newton iters/sample histogram for the Ultra chain"]
fn ultra_newton_iter_histogram() {
    let sr = 48_000.0f32;
    let note = 45u8; // A1 ≈ 110 Hz
    let cutoff = 1_200.0f32;
    let reso = 0.7f32;
    let secs = 4.0f32;
    let warm = (sr * 0.05) as usize; // skip the rail self-start + ladder warm-up

    let mut vco = Vco303::new(sr);
    vco.set_freq(midi_to_freq(note));
    let mut os = Os4x::new();
    let mut ladder = LadderVcf::new(sr * 4.0);
    ladder.set_cal(0.70);
    ladder.set_cutoff(cutoff);
    ladder.set_reso(reso);

    const HBINS: usize = 110; // MAX_ITERS for both solvers ≤ 100
    let mut vco_hist = [0u64; HBINS];
    let mut lad_hist = [0u64; HBINS];
    let (mut vco_n, mut lad_n) = (0u64, 0u64);
    let (mut vco_homo, mut lad_homo) = (0u64, 0u64);

    let n = (sr * secs) as usize;
    for i in 0..n {
        let s = vco.tick() * 2.0; // VCO_GAIN
        if i >= warm {
            vco_hist[(vco.dbg_iters as usize).min(HBINS - 1)] += 1;
            vco_n += 1;
            if vco.dbg_homotopy {
                vco_homo += 1;
            }
        }
        let hi = os.up(s);
        let mut o = [0.0f32; 4];
        for (k, &h) in hi.iter().enumerate() {
            o[k] = ladder.tick(h);
            if i >= warm {
                lad_hist[(ladder.dbg_iters as usize).min(HBINS - 1)] += 1;
                lad_n += 1;
                // ladder has no homotopy flag; a hit at MAX_ITERS-summed counts as
                // a stall proxy — track separately if iters look pathological.
                if ladder.dbg_iters == 0 {
                    lad_homo += 1; // never-converged (held previous) proxy
                }
            }
        }
        let _ = os.down(o);
    }

    dump("VCO (per base sample)", &vco_hist, vco_n, vco_homo);
    dump("LADDER (per 4× sample)", &lad_hist, lad_n, lad_homo);

    // Cost weighting: VCO solves a 17×17 once per base sample; the ladder solves a
    // 12×12 four times per base sample. Report both per-base-sample iter loads.
    let vco_mean: f64 = vco_hist
        .iter()
        .enumerate()
        .map(|(i, &c)| i as f64 * c as f64)
        .sum::<f64>()
        / vco_n as f64;
    let lad_mean: f64 = lad_hist
        .iter()
        .enumerate()
        .map(|(i, &c)| i as f64 * c as f64)
        .sum::<f64>()
        / lad_n as f64;
    eprintln!(
        "\n== per-base-sample LU calls ==\n  VCO  17x17 : {vco_mean:.3} factorizations\n  LADDER 12x12: {:.3} factorizations (4 ticks × {lad_mean:.3} iters)",
        4.0 * lad_mean,
    );
}
