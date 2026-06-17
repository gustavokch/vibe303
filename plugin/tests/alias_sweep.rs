//! Broadband alias sweep — hardens the Phase 2/3 NO-GO verdict
//! (`docs/plans/2026-06-15-adaa-phase23-findings.md`).
//!
//! The prior verdict rested on a single tone (`f0 = 0.227·host`) measured at two folded bins. This
//! harness generalises that to a **broadband** alias metric over a **frequency sweep**, and compares
//! **4th- vs 8th-order decimation** (Phase 2) and **trafo OFF vs ON** (Phase 3) from one controlled
//! solver run per cell.
//!
//! Metric: for a single sine at `f0`, the only legitimate spectrum is DC + harmonics `m·f0` (m≥1) —
//! NOTHING legitimate lies in the open band `(0, f0)`, so all energy there is folded (aliased)
//! high-harmonic content. We drive the WDF model directly (bypassing `ChannelDsp`/saturation),
//! capture the pre-decimation os=2 stream, apply external Butterworth decimators, and FFT the result
//! (coherent sampling → rectangular, no leakage). `alias_db = 10·log10(Σ sub-fundamental power /
//! fundamental power)`.
//!
//! Run: `cargo test --features measure --release --test alias_sweep -- --nocapture`

#![cfg(feature = "measure")]

use ba283::dsp::INPUT_FS_VOLTS;
use ba283::wdf::{Biquad, QualityConfig, WdfModel};

const HOST: f64 = 48_000.0;
const FS_V: f32 = INPUT_FS_VOLTS; // circuit is fed/observed in volts (single source of truth, dsp.rs)
const OS: usize = 2; // realtime tier internal oversampling
// Decimation cutoff/Q's: must match the shipped dec1/dec2 (build.rs:255-257). Drift here is caught by
// the harness-fidelity assert below (external-4th vs live `process_host_sample` within 0.5 dB), which
// fails loudly if these diverge from the model's private dec coeffs.
const FC: f64 = 0.45 * HOST; // decimation cutoff (matches build.rs:255)
const FS2X: f64 = OS as f64 * HOST; // internal solve rate

const N: usize = 1 << 14; // 16384-sample analysis window (host rate)
const DISCARD: usize = 8192; // settle prefix dropped before analysis

// Shipped 4th-order Butterworth decimation Q's (build.rs:256-257) and a steeper 8th-order cascade.
const Q4: [f64; 2] = [0.54119610, 1.30656296];
const Q8: [f64; 4] = [0.50979558, 0.60134489, 0.89997622, 2.56291545];

// f0 sweep (cycles/sample, snapped to exact FFT bins for coherence) and the two operating points.
const F0_TARGETS: [f64; 7] = [0.12, 0.16, 0.20, 0.24, 0.28, 0.32, 0.36];
struct Op {
    name: &'static str,
    amp: f32,
    drive: f32,
}
const OPS: [Op; 2] = [
    Op { name: "moderate", amp: 0.5, drive: 4.0 },
    Op { name: "hot", amp: 0.9, drive: 12.0 },
];

/// Iterative radix-2 Cooley-Tukey FFT (in place, `n` a power of two).
fn fft(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    let mut j = 0usize;
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
    let mut len = 2usize;
    while len <= n {
        let ang = -2.0 * std::f64::consts::PI / len as f64;
        let (wlr, wli) = (ang.cos(), ang.sin());
        let mut i = 0usize;
        while i < n {
            let (mut wr, mut wi) = (1.0f64, 0.0f64);
            for k in 0..len / 2 {
                let a = i + k;
                let b = a + len / 2;
                let tr = re[b] * wr - im[b] * wi;
                let ti = re[b] * wi + im[b] * wr;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let nwr = wr * wlr - wi * wli;
                wi = wr * wli + wi * wlr;
                wr = nwr;
            }
            i += len;
        }
        len <<= 1;
    }
}

/// Broadband alias-to-fundamental ratio (dB): sum of power in the sub-fundamental band `(f_lo, f0)`
/// — pure aliasing — over the fundamental power. `f0_norm` in cycles/sample, snapped to bin `p`.
fn alias_db(x: &[f32], f0_norm: f64) -> f64 {
    let n = x.len();
    let mut re: Vec<f64> = x.iter().map(|&v| v as f64).collect();
    let mut im = vec![0.0f64; n];
    fft(&mut re, &mut im);
    let pow = |k: usize| re[k] * re[k] + im[k] * im[k];
    let p = (f0_norm * n as f64).round() as usize; // fundamental bin
    let margin = 0.04;
    let k_lo = 2usize; // skip DC + bin 1 (≈5 Hz HP corner)
    let k_hi = ((f0_norm * (1.0 - margin)) * n as f64).floor() as usize;
    let alias: f64 = (k_lo..=k_hi).map(pow).sum();
    let fund = pow(p).max(1e-30);
    10.0 * (alias / fund).log10()
}

/// External Butterworth decimator: cascade biquads over the `OS×host` stream, take every `OS`-th
/// sample (phase-aligned to `process_host_sample`'s last-subsample tap). Reuses the shipped `Biquad`.
fn butter_decimate(x: &[f32], qs: &[f64]) -> Vec<f32> {
    let mut bqs: Vec<Biquad> = qs
        .iter()
        .map(|&q| {
            let mut b = Biquad::default();
            b.set_lowpass(FC, FS2X, q);
            b
        })
        .collect();
    let mut out = Vec::with_capacity(x.len() / OS);
    for (i, &s) in x.iter().enumerate() {
        let mut v = s as f64;
        for b in bqs.iter_mut() {
            v = b.process(v);
        }
        if i % OS == OS - 1 {
            out.push(v as f32);
        }
    }
    out
}

/// Coherent input: tone at `f0_norm = p/N` so it lands exactly on bin `p` (no window needed).
/// Returns the WDF input in volts (`amp·sin · drive · FS_V`), `total` host samples.
fn make_input(f0_norm: f64, op: &Op, total: usize) -> Vec<f32> {
    (0..total)
        .map(|i| {
            let s = (std::f64::consts::TAU * f0_norm * i as f64).sin() as f32;
            op.amp * s * op.drive * FS_V
        })
        .collect()
}

fn make_model(trafo: bool) -> WdfModel {
    // new_with(cfg, host_sr, rgain=220Ω, bias=0.5) — a representative mid-gain operating point.
    let mut m = WdfModel::new_with(QualityConfig::realtime(), HOST, 220.0, 0.5);
    m.warmup();
    #[cfg(feature = "trafo-core")]
    if trafo {
        m.set_trafo_enabled(true);
        m.set_trafo_voicing(0.5, 1.0); // SATURATION dial maxed (hardest knee), default hysteresis
    }
    let _ = trafo;
    m
}

struct Cell {
    trafo: bool,
    op: &'static str,
    f0: f64,
    alias4: f64, // shipped 4th-order decimation
    alias8: f64, // steeper 8th-order decimation
    alias_live: f64, // live process_host_sample (4th-order, fidelity cross-check)
    alias_raw: f64, // pre-decimation os=2 stream (where the alias energy already sits)
}

#[test]
fn broadband_alias_sweep_confirms_phase23_nogo() {
    let total = DISCARD + N;
    let mut cells: Vec<Cell> = Vec::new();

    for &trafo in &[false, true] {
        let mut m = make_model(trafo);
        for op in &OPS {
            for &target in &F0_TARGETS {
                let p = (target * N as f64).round() as usize;
                let f0_host = p as f64 / N as f64; // exact host-rate bin
                let f0_2x = p as f64 / (OS * N) as f64; // same absolute freq at 2× rate
                let input = make_input(f0_host, op, total);

                // --- captured pre-decimation os=2 stream → external 4th/8th decimation ---
                m.reset();
                let mut raw2x = Vec::new();
                m.capture_os_raw(&input, &mut raw2x);
                let raw_settled = &raw2x[DISCARD * OS..]; // N*OS samples
                let dec4 = butter_decimate(&raw2x, &Q4);
                let dec8 = butter_decimate(&raw2x, &Q8);

                // --- live shipped path (independent run from the same settled snapshot) ---
                m.reset();
                let live: Vec<f32> = input.iter().map(|&x| m.process_host_sample(x)).collect();

                let alias4 = alias_db(&dec4[DISCARD..], f0_host);
                let alias8 = alias_db(&dec8[DISCARD..], f0_host);
                let alias_live = alias_db(&live[DISCARD..], f0_host);
                let alias_raw = alias_db(raw_settled, f0_2x);

                cells.push(Cell {
                    trafo,
                    op: op.name,
                    f0: f0_host,
                    alias4,
                    alias8,
                    alias_live,
                    alias_raw,
                });
            }
        }
    }

    // Guard the grid actually ran in full — a trimmed loop bound must fail loudly, not pass vacuously.
    assert_eq!(cells.len(), F0_TARGETS.len() * OPS.len() * 2, "alias sweep grid is incomplete");

    // -------- table --------
    eprintln!(
        "\n{:<6} {:<9} {:>6}  {:>9} {:>9} {:>9}  {:>9} {:>9}",
        "trafo", "op", "f0", "4th(dB)", "8th(dB)", "live(dB)", "raw2x(dB)", "Δ8-4"
    );
    for c in &cells {
        eprintln!(
            "{:<6} {:<9} {:>6.3}  {:>9.2} {:>9.2} {:>9.2}  {:>9.2} {:>+9.2}",
            c.trafo, c.op, c.f0, c.alias4, c.alias8, c.alias_live, c.alias_raw, c.alias8 - c.alias4
        );
    }
    eprintln!();

    // -------- assertions (verdict gates) --------
    // 1. Harness fidelity: external 4th-order reproduces the shipped path.
    for c in &cells {
        assert!(
            (c.alias4 - c.alias_live).abs() < 0.5,
            "harness 4th vs live mismatch ({:.2} vs {:.2} dB) — decimator unfaithful at trafo={} {} f0={:.3}",
            c.alias4, c.alias_live, c.trafo, c.op, c.f0
        );
    }
    // 2. Phase 2: steeper decimation does not lower the floor anywhere → in-solve foldback.
    for c in &cells {
        assert!(
            (c.alias8 - c.alias4).abs() < 1.0,
            "8th-order decimation changed the alias floor by {:.2} dB at trafo={} {} f0={:.3} — \
             Phase 2 may need reopening",
            c.alias8 - c.alias4, c.trafo, c.op, c.f0
        );
    }
    // 3. Phase 3: the JA transformer must ADD no aliasing. The check is ONE-SIDED: the core is
    //    RK4-band-limited and one-step-lag-injected, so it can only band-limit/compress — never fold
    //    new energy in. Across the sweep ON is always ≤ OFF (delta 0 → −5 dB; it sometimes REDUCES
    //    aliasing). A trafo that adds no aliasing is exactly why ADAA-ing it is pointless (NO-GO).
    //    Gate: ON may not exceed OFF by more than 1 dB anywhere. (The prior single-point 0.227
    //    measurement read −0.0 dB by sitting near a null; the broadband sweep shows the full picture.)
    let mut max_reduction = 0.0f64;
    for op in &OPS {
        for &target in &F0_TARGETS {
            let p = (target * N as f64).round() as usize;
            let f0 = p as f64 / N as f64;
            let off = cells.iter().find(|c| !c.trafo && c.op == op.name && c.f0 == f0).unwrap();
            let on = cells.iter().find(|c| c.trafo && c.op == op.name && c.f0 == f0).unwrap();
            let delta = on.alias4 - off.alias4; // <0 ⇒ trafo ON has LESS aliasing
            max_reduction = max_reduction.min(delta);
            assert!(
                delta < 1.0,
                "trafo ON ADDED {:.2} dB of aliasing at {} f0={:.3} — Phase 3 (trafo adds no \
                 aliasing) would need reopening",
                delta, op.name, f0
            );
        }
    }
    eprintln!("Phase 3: trafo ON never adds aliasing; max reduction {:.2} dB.\n", max_reduction);
}
