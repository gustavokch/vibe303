//! Stage-2 A/B: in-solve ADAA1 on the BJT transport exponentials vs the os=2 baseline
//! (`docs/plans/2026-06-17-adaa-antialiasing-spike.md`; refined plan
//! `~/.claude/plans/unaudhd-review-critique-and-flickering-river.md`).
//!
//! Stage 1 (`headroom_probe.rs`) established the recoverable headroom (hot os2→os16 ≈ 31 dB, realistic
//! os2→os4 ≈ 15 dB) and that transport-exp dominates the foldback. This measures whether the faithful
//! ADAA1 form actually closes a meaningful fraction of it, what it costs in Newton iterations, and how
//! it shifts the small-signal frequency response.
//!
//! Three reads, per the decision metrics:
//!  1. **Alias floor** — os=2 baseline vs os=2+ADAA vs the os=16 ceiling, on the same analytic-input +
//!     steep-sinc-decimator rig as Stage 1. Win = fraction of the os2→os16 gap closed (esp. hot).
//!  2. **Newton cost** — avg/max iters, baseline vs ADAA. The divided-difference + eq-29 fallback must
//!     not blow up iteration counts.
//!  3. **Small-signal FR + DC bias** — quiet stepped-sine gain (output/input) per frequency, baseline
//!     vs ADAA, and the DC operating-point shift. Parker eq 17 predicts a ≈½-sample group delay → mild
//!     HF rolloff; the question is whether it stays bounded and the cross-coupled loop stays stable.
//!     A large FR shift / detuned loop is the EXPECTED naive-form failure (Holters) → escalate to
//!     Stage 2b (delay compensation), NOT an immediate NO-GO.
//!
//! Run: `cargo test --features measure,adaa-insolve --release --test adaa_insolve -- --nocapture`

#![cfg(all(feature = "measure", feature = "adaa-insolve"))]

use ba283::dsp::INPUT_FS_VOLTS;
use ba283::wdf::{QualityConfig, WdfModel};

const HOST: f64 = 48_000.0;
const FS_V: f32 = INPUT_FS_VOLTS;
const N: usize = 1 << 13;
const DISCARD: usize = 4096;

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

fn alias_db(x: &[f32], f0_norm: f64) -> f64 {
    let n = x.len();
    let mut re: Vec<f64> = x.iter().map(|&v| v as f64).collect();
    let mut im = vec![0.0f64; n];
    fft(&mut re, &mut im);
    let pow = |k: usize| re[k] * re[k] + im[k] * im[k];
    let p = (f0_norm * n as f64).round() as usize;
    let k_lo = 2usize;
    let k_hi = ((f0_norm * 0.96) * n as f64).floor() as usize;
    let alias: f64 = (k_lo..=k_hi).map(pow).sum();
    let fund = pow(p).max(1e-30);
    10.0 * (alias / fund).log10()
}

/// Steep windowed-sinc decimator (Blackman-Harris), identical to the Stage-1 reference rig.
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

fn build_warm(os: usize) -> WdfModel {
    build_warm_iters(os, QualityConfig::realtime().max_iters)
}

/// Build at a chosen Newton iteration cap. ADAA's modified residual moves the operating point, so the
/// realtime cap (30) leaves many samples unconverged — raising the cap isolates the *formulation*'s
/// antialiasing quality from mere convergence failure.
fn build_warm_iters(os: usize, max_iters: usize) -> WdfModel {
    let mut cfg = QualityConfig::realtime();
    cfg.os = os;
    cfg.decimate = false;
    cfg.max_iters = max_iters;
    let mut m = WdfModel::new_with(cfg, HOST, 220.0, 0.5);
    m.warmup();
    m
}

fn make_internal_input(f0_host: f64, os: usize, op: &Op, total: usize) -> Vec<f32> {
    let f0_internal = f0_host / os as f64;
    (0..total * os)
        .map(|k| {
            let s = (std::f64::consts::TAU * f0_internal * k as f64).sin() as f32;
            op.amp * s * op.drive * FS_V
        })
        .collect()
}

/// (alias floor dB, avg iters, max iters, DC offset of decimated output) for one cell.
fn cell(m: &mut WdfModel, os: usize, f0_host: f64, op: &Op) -> (f64, f64, usize, f64) {
    let total = DISCARD + N;
    let input = make_internal_input(f0_host, os, op, total);
    m.reset();
    m.reset_iter_stats();
    let mut raw = Vec::new();
    m.capture_internal_direct(&input, &mut raw);
    let (_s, avg, mx) = m.iter_stats();
    let host = sinc_decimate(&raw, os);
    let settled = &host[DISCARD..DISCARD + N];
    let dc = settled.iter().map(|&v| v as f64).sum::<f64>() / settled.len() as f64;
    (alias_db(settled, f0_host), avg, mx, dc)
}

#[test]
fn adaa_alias_and_cost() {
    let mut base = build_warm(2);
    let mut adaa = build_warm(2); // ADAA at the realtime cap (30) — what a shippable solve would see
    adaa.set_adaa(true);
    let mut adaa_hi = build_warm_iters(2, 400); // ADAA with convergence headroom — formulation-only read
    adaa_hi.set_adaa(true);
    let mut refm = build_warm(16); // os=16 ceiling

    eprintln!(
        "\n=== Stage 2: alias floor + Newton cost (os=2 base vs ADAA@30 vs ADAA@400-iter vs os=16) ===\n\
         {:<9} {:>6} {:>8} {:>8} {:>8} {:>8} {:>7} {:>7} {:>6} {:>6} {:>6}",
        "op", "f0", "base", "adaa30", "adaaHi", "os16", "Δhi", "clos%", "itB", "itHi", "mxHi"
    );
    let mut worst_close_hot = f64::INFINITY;
    let mut best_close_hot = f64::NEG_INFINITY;
    for op in &OPS {
        for &target in &F0_TARGETS {
            let p = (target * N as f64).round() as usize;
            let f0 = p as f64 / N as f64;
            let (a_base, it_b, _mx_b, _d) = cell(&mut base, 2, f0, op);
            let (a_adaa, _it_a, _mx_a, _d2) = cell(&mut adaa, 2, f0, op);
            let (a_hi, it_hi, mx_hi, _d3) = cell(&mut adaa_hi, 2, f0, op);
            let (a_ref, _ir, _mr, _dr) = cell(&mut refm, 16, f0, op);
            let improv = a_base - a_hi; // converged ADAA's true floor improvement (positive = cleaner)
            let headroom = a_base - a_ref;
            let closed = if headroom > 1.0 { 100.0 * improv / headroom } else { f64::NAN };
            if op.name == "hot" && headroom > 3.0 {
                worst_close_hot = worst_close_hot.min(closed);
                best_close_hot = best_close_hot.max(closed);
            }
            eprintln!(
                "{:<9} {:>6.3} {:>8.2} {:>8.2} {:>8.2} {:>8.2} {:>+7.2} {:>7.1} {:>6.2} {:>6.2} {:>6}",
                op.name, f0, a_base, a_adaa, a_hi, a_ref, improv, closed, it_b, it_hi, mx_hi
            );
        }
    }
    eprintln!(
        "\nHot-drive headroom closed by CONVERGED ADAA1 (@400 iters): {:.1}% .. {:.1}% (of os2→os16).",
        worst_close_hot, best_close_hot
    );
    eprintln!(
        "BRANCH: meaningful close (≳ one octave, ~30%+) ⇒ ADAA1 viable; near-zero or NEGATIVE ⇒ the\n\
         naive in-loop form is fighting the feedback delay → escalate to Stage 2b (Holters), not NO-GO.\n"
    );
}

#[test]
fn adaa_small_signal_fr_and_dc() {
    // Quiet stepped-sine gain per frequency through the full os=2 path (process_host_sample), baseline
    // vs ADAA. amp = 0.05 V keeps the circuit near-linear (same probe level as `measure_gain`).
    let mut base = build_warm(2);
    let mut adaa = build_warm(2);
    adaa.set_adaa(true);
    let amp = 0.05_f64 * FS_V as f64;
    let freqs = [500.0, 1000.0, 2000.0, 4000.0, 8000.0, 12000.0, 16000.0, 20000.0];

    let gain = |m: &mut WdfModel, f: f64| -> f64 {
        m.reset();
        let w = std::f64::consts::TAU * f / HOST;
        let cyc = (HOST / f).max(1.0);
        let n_settle = (cyc * 8.0) as usize;
        let n_meas = (cyc * 16.0) as usize;
        let (mut isq, mut osq) = (0.0f64, 0.0f64);
        for i in 0..(n_settle + n_meas) {
            let x = amp * (w * i as f64).sin();
            let y = m.process_host_sample(x as f32) as f64;
            if i >= n_settle {
                isq += x * x;
                osq += y * y;
            }
        }
        (osq / isq).sqrt()
    };

    eprintln!(
        "\n=== Stage 2: small-signal FR (gain dB) baseline vs ADAA1 ===\n{:>8} {:>10} {:>10} {:>9}",
        "freq", "base(dB)", "adaa(dB)", "Δ(dB)"
    );
    let mut max_abs_shift = 0.0f64;
    for &f in &freqs {
        let gb = 20.0 * gain(&mut base, f).log10();
        let ga = 20.0 * gain(&mut adaa, f).log10();
        let d = ga - gb;
        max_abs_shift = max_abs_shift.max(d.abs());
        eprintln!("{:>8.0} {:>10.3} {:>10.3} {:>+9.3}", f, gb, ga, d);
    }
    eprintln!(
        "\nMax |FR shift| to 20 kHz = {:.3} dB. Target ≤ ~0.5 dB and loop stable (finite gains).",
        max_abs_shift
    );

    // DC bias: settle both with a small DC input and compare the steady output tap. The eq-29 fallback
    // is exact at DC (Δ=0 ⇒ midpoint = operating point), so near-zero shift is expected; measure it.
    let dc_out = |m: &mut WdfModel| -> f64 {
        m.reset();
        let mut last = 0.0;
        for _ in 0..4096 {
            last = m.process_host_sample((0.01 * FS_V) as f64 as f32) as f64;
        }
        last
    };
    let db = dc_out(&mut base);
    let da = dc_out(&mut adaa);
    eprintln!("DC operating point (0.01 FS in): base={:.6}  adaa={:.6}  Δ={:.2e}\n", db, da, da - db);

    // Sanity: ADAA must not destabilize the loop into non-finite output.
    for &f in &freqs {
        assert!(gain(&mut adaa, f).is_finite(), "ADAA gain non-finite at {f} Hz — loop unstable");
    }
}
