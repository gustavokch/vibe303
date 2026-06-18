//! Stage-1 headroom probe + Stage-0 attribution for the in-solve ADAA spike
//! (`docs/plans/2026-06-17-adaa-antialiasing-spike.md`, refined plan
//! `~/.claude/plans/unaudhd-review-critique-and-flickering-river.md`).
//!
//! Question Stage 1 answers: **how much of the os=2 BJT alias floor is even reducible?** We run the
//! same circuit at os = 2 / 4 / 8 / 16 and decimate each back to host rate with a *steep* windowed-
//! sinc lowpass (NOT the shipped 2×-tuned Butterworth — `config.rs:126` notes it aliases at 4:1, so
//! reusing it for an 8:1/16:1 reference would self-alias and corrupt the very number we want). The
//! input is sampled **analytically at the internal rate** and fed through `capture_internal_direct`
//! (no `prev_in` lerp — the lerp injects its own input-stage artifacts that understate headroom).
//!
//! The go/no-go number is the **hot-drive gap** `alias(os=2) − alias(os=16)`: the max recoverable
//! alias reduction. Per Albertini/DAFx-2020 (×2 OS + ADAA1 ≈ one OS octave), the *realistic* ADAA1
//! target is the os=2 → os=4 gap; os=16 is the absolute ceiling.
//!
//! Stage 0 (attribution): at os=2 and os=16, A/B caps Full vs Off. If removing the depletion/
//! diffusion charge model barely moves the floor, the foldback is transport-exp dominated and the
//! plan's transport-only ADAA1 scope is justified; if it moves a lot, depletion is the real aliaser
//! and transport-only ADAA1 would be a wasted Stage 2.
//!
//! Run: `cargo test --features measure --release --test headroom_probe -- --nocapture`

#![cfg(feature = "measure")]

use ba283::dsp::INPUT_FS_VOLTS;
use ba283::wdf::{CapTier, QualityConfig, WdfModel};

const HOST: f64 = 48_000.0;
const FS_V: f32 = INPUT_FS_VOLTS;

const N: usize = 1 << 13; // 8192-sample analysis window (host rate)
const DISCARD: usize = 4096; // settle prefix dropped before analysis

// Internal oversampling factors probed. 2 = shipped realtime tier; 16 = the ceiling reference.
const OS_SET: [usize; 4] = [2, 4, 8, 16];

const F0_TARGETS: [f64; 5] = [0.12, 0.18, 0.24, 0.30, 0.36];
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

/// Aliasing floor (dB): ratio of folded sub-fundamental power to fundamental power. A single sine at
/// `f0_norm = p/N` puts all legitimate energy at DC + harmonics `m·f0`; nothing legitimate lies in
/// the open band `(0, f0)`, so all power there is folded (aliased) high-harmonic content.
fn alias_db(x: &[f32], f0_norm: f64) -> f64 {
    let n = x.len();
    let mut re: Vec<f64> = x.iter().map(|&v| v as f64).collect();
    let mut im = vec![0.0f64; n];
    fft(&mut re, &mut im);
    let pow = |k: usize| re[k] * re[k] + im[k] * im[k];
    let p = (f0_norm * n as f64).round() as usize;
    let margin = 0.04;
    let k_lo = 2usize; // skip DC + bin 1 (≈ HP corner)
    let k_hi = ((f0_norm * (1.0 - margin)) * n as f64).floor() as usize;
    let alias: f64 = (k_lo..=k_hi).map(pow).sum();
    let fund = pow(p).max(1e-30);
    10.0 * (alias / fund).log10()
}

/// Steep windowed-sinc decimator: lowpass at host Nyquist (cutoff `0.5/os` of the internal rate),
/// Blackman-Harris window (~ −92 dB sidelobes — far below the floors we measure), then take every
/// `os`-th sample. Tap count scales with `os` so the transition band stays equally sharp at 16:1 as
/// at 2:1. This is deliberately NOT the shipped Butterworth (which is 2×-tuned and self-aliases past
/// 4:1) — the reference floor must come from a decimator that genuinely rejects everything above
/// host-Nyquist, or the headroom number is untrustworthy.
fn sinc_decimate(x: &[f32], os: usize) -> Vec<f32> {
    if os == 1 {
        return x.to_vec();
    }
    let half = 64 * os; // FIR half-width; total taps = 2*half + 1
    let fc = 0.5 / os as f64; // normalized cutoff (cycles/sample at the internal rate)
    let two_fc = 2.0 * fc;
    // Blackman-Harris window coefficients.
    let (a0, a1, a2, a3) = (0.35875, 0.48829, 0.14128, 0.01168);
    let ntap = 2 * half + 1;
    let mut h = vec![0.0f64; ntap];
    let mut sum = 0.0;
    for (i, hi) in h.iter_mut().enumerate() {
        let m = i as f64 - half as f64; // -half..=half
        let sinc = if m == 0.0 {
            two_fc
        } else {
            (std::f64::consts::PI * two_fc * m).sin() / (std::f64::consts::PI * m)
        };
        let wn = i as f64 / (ntap - 1) as f64;
        let w = a0 - a1 * (std::f64::consts::TAU * wn).cos()
            + a2 * (2.0 * std::f64::consts::TAU * wn).cos()
            - a3 * (3.0 * std::f64::consts::TAU * wn).cos();
        *hi = sinc * w;
        sum += *hi;
    }
    for hi in h.iter_mut() {
        *hi /= sum; // unity DC gain
    }
    // Convolve, emitting only the kept (every os-th) output positions. Symmetric FIR → constant
    // group delay (irrelevant to the power-based alias metric).
    let mut out = Vec::with_capacity(x.len() / os);
    let mut o = os - 1; // phase-align to the last subsample of each host frame
    while o < x.len() {
        let mut acc = 0.0f64;
        let lo = o as isize - half as isize;
        for (t, &ht) in h.iter().enumerate() {
            let idx = lo + t as isize;
            if idx >= 0 && (idx as usize) < x.len() {
                acc += ht * x[idx as usize] as f64;
            }
        }
        out.push(acc as f32);
        o += os;
    }
    out
}

/// Build + warm up one model at the given oversampling factor and cap tier. Warmup depends only on
/// the operating point + os (not the input), so we warm up once per (os, cap_tier) and `reset()` per
/// cell rather than re-warming (warmup at os=16 is ~millions of solves).
fn build_warm(os: usize, cap_tier: CapTier) -> WdfModel {
    let mut cfg = QualityConfig::realtime();
    cfg.os = os;
    cfg.decimate = false; // we decimate externally with the steep sinc
    cfg.cap_tier = cap_tier;
    let mut m = WdfModel::new_with(cfg, HOST, 220.0, 0.5);
    m.warmup();
    m
}

/// Analytic internal-rate sine: `total` host samples × `os` internal subsamples, sampled directly at
/// the internal rate. Input volts = `amp·sin · drive · FS_V` (matches `alias_sweep::make_input`).
fn make_internal_input(f0_host: f64, os: usize, op: &Op, total: usize) -> Vec<f32> {
    let f0_internal = f0_host / os as f64;
    (0..total * os)
        .map(|k| {
            let s = (std::f64::consts::TAU * f0_internal * k as f64).sin() as f32;
            op.amp * s * op.drive * FS_V
        })
        .collect()
}

/// Run one (model, f0, op) cell: capture the os-rate stream, steep-decimate to host, return the
/// alias floor over the settled window.
fn cell_alias(m: &mut WdfModel, os: usize, f0_host: f64, op: &Op) -> f64 {
    let total = DISCARD + N;
    let input = make_internal_input(f0_host, os, op, total);
    m.reset();
    let mut raw = Vec::new();
    m.capture_internal_direct(&input, &mut raw);
    let host = sinc_decimate(&raw, os); // length ≈ total
    let settled = &host[DISCARD..DISCARD + N];
    alias_db(settled, f0_host)
}

#[test]
fn headroom_probe_os2_to_os16() {
    // One warmed model per os (caps Full) reused across all cells.
    let mut models: Vec<(usize, WdfModel)> =
        OS_SET.iter().map(|&os| (os, build_warm(os, CapTier::Full))).collect();

    eprintln!(
        "\n=== Stage 1: headroom probe (alias floor dB, lower = cleaner) ===\n{:<9} {:>6} {:>9} {:>9} {:>9} {:>9} {:>10} {:>10}",
        "op", "f0", "os2", "os4", "os8", "os16", "gap2-16", "gap2-4"
    );
    let mut max_hot_gap = f64::NEG_INFINITY; // most positive os2−os16 at hot drive
    let mut max_hot_gap24 = f64::NEG_INFINITY;
    for op in &OPS {
        for &target in &F0_TARGETS {
            let p = (target * N as f64).round() as usize;
            let f0_host = p as f64 / N as f64;
            let mut floors = [0.0f64; 4];
            for (i, (os, m)) in models.iter_mut().enumerate() {
                floors[i] = cell_alias(m, *os, f0_host, op);
            }
            let gap = floors[0] - floors[3]; // os2 − os16
            let gap24 = floors[0] - floors[1]; // os2 − os4 (realistic ADAA1 target)
            if op.name == "hot" {
                max_hot_gap = max_hot_gap.max(gap);
                max_hot_gap24 = max_hot_gap24.max(gap24);
            }
            eprintln!(
                "{:<9} {:>6.3} {:>9.2} {:>9.2} {:>9.2} {:>9.2} {:>+10.2} {:>+10.2}",
                op.name, f0_host, floors[0], floors[1], floors[2], floors[3], gap, gap24
            );
        }
    }
    eprintln!(
        "\nHot-drive MAX recoverable gap (os2−os16) = {:.2} dB; realistic ADAA1 target (os2−os4) = {:.2} dB.",
        max_hot_gap, max_hot_gap24
    );
    eprintln!(
        "GATE: os2−os16 < ~6 dB ⇒ in-solve ADAA cannot beat oversampling, NO-GO. Here = {:.2} dB ⇒ {}.\n",
        max_hot_gap,
        if max_hot_gap >= 6.0 { "PROCEED to Stage 2" } else { "STOP (documented NO-GO)" }
    );

    // Reference-rig sanity: the os=16 floor must be the lowest (monotone-ish in os). If a higher-os
    // floor is HIGHER than os=2, the steep decimator is itself aliasing and the gap is untrustworthy.
    // (Checked only at moderate drive, where the floor is well-defined; hot drive can be noisy.)
    for &target in &F0_TARGETS {
        let p = (target * N as f64).round() as usize;
        let f0_host = p as f64 / N as f64;
        let op = &OPS[0]; // moderate
        let f2 = cell_alias(&mut models[0].1, 2, f0_host, op);
        let f16 = cell_alias(&mut models[3].1, 16, f0_host, op);
        assert!(
            f16 <= f2 + 1.0,
            "reference rig suspect: os16 floor {f16:.2} dB not below os2 {f2:.2} dB at f0={f0_host:.3} \
             — steep decimator may be aliasing",
        );
    }
}

#[test]
fn attribution_caps_on_vs_off() {
    // Stage 0: does depletion/diffusion charge drive the foldback, or transport exp? A/B caps
    // Full vs Off at os=2 (the ADAA target rate) and os=16 (the reference).
    let mut probes: Vec<(usize, CapTier, WdfModel)> = vec![
        (2, CapTier::Full, build_warm(2, CapTier::Full)),
        (2, CapTier::Off, build_warm(2, CapTier::Off)),
        (16, CapTier::Full, build_warm(16, CapTier::Full)),
        (16, CapTier::Off, build_warm(16, CapTier::Off)),
    ];
    eprintln!(
        "\n=== Stage 0: caps attribution (alias floor dB) ===\n{:<9} {:>6} {:>11} {:>11} {:>11} {:>11}",
        "op", "f0", "os2 Full", "os2 Off", "os16 Full", "os16 Off"
    );
    for op in &OPS {
        for &target in &F0_TARGETS {
            let p = (target * N as f64).round() as usize;
            let f0_host = p as f64 / N as f64;
            let mut v = [0.0f64; 4];
            for (i, (os, _ct, m)) in probes.iter_mut().enumerate() {
                v[i] = cell_alias(m, *os, f0_host, op);
            }
            eprintln!(
                "{:<9} {:>6.3} {:>11.2} {:>11.2} {:>11.2} {:>11.2}",
                op.name, f0_host, v[0], v[1], v[2], v[3]
            );
        }
    }
    eprintln!(
        "\nReading: os2 Full≈Off ⇒ transport-exp dominates foldback (transport-only ADAA1 justified).\n\
         Large Full→Off drop ⇒ depletion charge is a real aliaser (transport-only ADAA1 insufficient).\n"
    );
}
