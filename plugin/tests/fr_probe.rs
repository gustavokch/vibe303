//! Forward frequency-response probe: attribute the clean-signal HF rolloff at the SHIPPED solve
//! rate (os=2) to the two numerical filters in the host path — the `prev_in` linear-interpolation
//! upsample and the 4th-order Butterworth decimator — independently of the circuit's own FR.
//!
//! Investigation: `~/.claude/plans/systematic-debugging-...-melodic-ullman.md` (HF rolloff).
//!
//! Three paths, all at os=2 (identical solve rate ⇒ the circuit FR is common-mode and cancels in
//! the pairwise differences):
//!   REF  = `capture_internal_direct` (analytic internal-rate sine, NO lerp) + steep sinc decimate.
//!   UPS  = `capture_os_raw`          (host-rate sine, lerp upsample)        + steep sinc decimate.
//!   FULL = `process_host_sample`     (host-rate sine, lerp + shipped Butterworth decimator).
//! Then per test frequency:
//!   UPS − REF  = lerp-upsample droop (dB)
//!   FULL − UPS = Butterworth-decimator droop (dB)
//!   FULL − REF = total numerical rolloff (dB)
//! The signal is a low-level sine (linear regime), so the model output is a clean tone and the
//! magnitude is read at the fundamental bin. Run at 48k and 44.1k host (where fc=21.6k / 19.8k).
//!
//! Run: `cargo test --features measure --release --test fr_probe -- --nocapture`

#![cfg(feature = "measure")]

use ba283::dsp::INPUT_FS_VOLTS;
use ba283::wdf::{QualityConfig, WdfModel};

const FS_V: f32 = INPUT_FS_VOLTS;
const N: usize = 1 << 14; // 16384-sample analysis window (host rate) → ~2.9 Hz bins at 48k
const DISCARD: usize = 8192; // settle prefix dropped before analysis
const OS: usize = 2; // shipped realtime solve rate
const AMP: f32 = 0.03; // low level: stay in the linear (clean) regime

// Host sample rates the user actually runs. 48k: decimator fc=21.6k. 44.1k: fc=19.8k.
const HOSTS: [f64; 2] = [48_000.0, 44_100.0];
// Test frequencies (Hz). 1k is the LF reference each path is normalized against.
const FREQS: [f64; 9] = [1_000.0, 5_000.0, 10_000.0, 12_000.0, 15_000.0, 17_000.0, 19_000.0, 20_000.0, 21_000.0];

/// Iterative radix-2 Cooley-Tukey FFT (in place, `n` a power of two). Copied from `headroom_probe`.
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

/// Steep windowed-sinc decimator (Blackman-Harris, ~ −92 dB sidelobes), cutoff at host Nyquist.
/// The clean reference decimator — NOT the shipped Butterworth. Copied from `headroom_probe`.
fn sinc_decimate(x: &[f32], os: usize) -> Vec<f32> {
    if os == 1 {
        return x.to_vec();
    }
    let half = 64 * os;
    let fc = 0.5 / os as f64;
    let two_fc = 2.0 * fc;
    let (a0, a1, a2, a3) = (0.35875, 0.48829, 0.14128, 0.01168);
    let ntap = 2 * half + 1;
    let mut h = vec![0.0f64; ntap];
    let mut sum = 0.0;
    for (i, hi) in h.iter_mut().enumerate() {
        let m = i as f64 - half as f64;
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
        *hi /= sum;
    }
    let mut out = Vec::with_capacity(x.len() / os);
    let mut o = os - 1;
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

/// Magnitude (dB) at the fundamental bin `p` over the settled window. Bin-aligned sine ⇒ no leakage.
fn mag_db_at(x: &[f32], p: usize) -> f64 {
    let settled = &x[DISCARD..DISCARD + N];
    let mut re: Vec<f64> = settled.iter().map(|&v| v as f64).collect();
    let mut im = vec![0.0f64; N];
    fft(&mut re, &mut im);
    let power = re[p] * re[p] + im[p] * im[p];
    10.0 * (power.max(1e-30) / N as f64).log10()
}

fn build_warm(host: f64, decimate: bool) -> WdfModel {
    let mut cfg = QualityConfig::realtime();
    cfg.os = OS;
    cfg.decimate = decimate;
    let mut m = WdfModel::new_with(cfg, host, 220.0, 0.5);
    m.warmup();
    m
}

/// `total` host samples × `OS` internal subsamples, analytic at the internal rate.
/// Normalized frequency (cycles/sample) at the internal rate = `f0_host / (host * OS)`.
fn internal_sine(f0_host: f64, host: f64, total: usize) -> Vec<f32> {
    let cps = f0_host / (host * OS as f64);
    (0..total * OS)
        .map(|k| AMP * (std::f64::consts::TAU * cps * k as f64).sin() as f32 * FS_V)
        .collect()
}

/// `total` host-rate samples of a clean sine.
fn host_sine(f0_host: f64, host: f64, total: usize) -> Vec<f32> {
    (0..total)
        .map(|k| AMP * (std::f64::consts::TAU * (f0_host / host) * k as f64).sin() as f32 * FS_V)
        .collect()
}

#[test]
fn forward_fr_numerical_attribution() {
    for &host in &HOSTS {
        let total = DISCARD + N;
        let mut m_nodec = build_warm(host, false); // REF + UPS (external steep decimate)
        let mut m_dec = build_warm(host, true); // FULL (shipped Butterworth)

        // Bin-align each target frequency, then measure the 3 paths.
        let mut rows: Vec<(f64, f64, f64, f64)> = Vec::new(); // (f, ref, ups, full) dB
        for &f in &FREQS {
            let p = (f / host * N as f64).round() as usize;
            let f0 = p as f64 / N as f64 * host; // exact bin-aligned host frequency

            // REF: analytic internal sine, no lerp, steep decimate.
            m_nodec.reset();
            let mut raw = Vec::new();
            m_nodec.capture_internal_direct(&internal_sine(f0, host, total), &mut raw);
            let ref_db = mag_db_at(&sinc_decimate(&raw, OS), p);

            // UPS: host sine, lerp upsample, steep decimate.
            m_nodec.reset();
            let mut raw2 = Vec::new();
            m_nodec.capture_os_raw(&host_sine(f0, host, total), &mut raw2);
            let ups_db = mag_db_at(&sinc_decimate(&raw2, OS), p);

            // FULL: shipped path (lerp + Butterworth), one output per host sample.
            m_dec.reset();
            let hs = host_sine(f0, host, total);
            let full: Vec<f32> = hs.iter().map(|&x| m_dec.process_host_sample(x)).collect();
            let full_db = mag_db_at(&full, p);

            rows.push((f0, ref_db, ups_db, full_db));
        }

        // Normalize each path to its own 1 kHz value (row 0) → rolloff relative to LF.
        let (_, r0, u0, f0db) = rows[0];
        eprintln!(
            "\n=== Forward FR numerical attribution @ host={:.0} Hz (os=2), values dB re 1 kHz ===",
            host
        );
        eprintln!(
            "{:>8}  {:>8} {:>8} {:>8}   {:>10} {:>10} {:>10}",
            "freq", "REF", "UPS", "FULL", "lerp(U-R)", "dec(F-U)", "total(F-R)"
        );
        for &(f, r, u, fu) in &rows {
            let rn = r - r0;
            let un = u - u0;
            let fn_ = fu - f0db;
            // Differences taken on 1kHz-normalized dB → removes any constant per-path gain offset,
            // leaving pure relative rolloff added by each numerical stage.
            let lerp = un - rn;
            let dec = fn_ - un;
            let total = fn_ - rn;
            eprintln!(
                "{:>8.0}  {:>8.2} {:>8.2} {:>8.2}   {:>+10.2} {:>+10.2} {:>+10.2}",
                f, rn, un, fn_, lerp, dec, total
            );
        }
        eprintln!(
            "Reading: REF col = circuit-only rolloff (faithful). lerp/dec cols = NUMERICAL rolloff\n\
             added by the host path on top of the circuit. total = what a fix could recover.\n"
        );

        // Regression gate: the shipped FIR path (FULL) must add negligible NUMERICAL rolloff on top
        // of the faithful circuit (REF) through 20 kHz. Before the fix this was −5.9 dB @20k (48k) /
        // −9.1 dB (44.1k); the steep linear-phase FIR brings it to ~0. Bound 1.0 dB leaves headroom
        // for the 44.1k case (−0.56 dB @20k, where the fold point is only ~2 kHz above 20 kHz).
        let worst = rows
            .iter()
            .filter(|(f, ..)| *f <= 20_000.0)
            .map(|&(_, r, _, fu)| ((fu - f0db) - (r - r0)).abs())
            .fold(0.0f64, f64::max);
        assert!(
            worst < 1.0,
            "host={host}: numerical rolloff {worst:.2} dB through 20 kHz exceeds 1.0 dB gate"
        );
    }
}
