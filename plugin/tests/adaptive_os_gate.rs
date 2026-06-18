//! Stage-1 gates for adaptive level-gated oversampling (feature `adaptive-os`). Drives the REAL
//! integration path (`ChannelDsp::process_sample`, host-rate streaming, shipped IIR decimator — not
//! the idealized steep-sinc reference), so these numbers reflect what would actually ship.
//!
//!  - **G1 alias payoff** — a sustained hot tone keeps the gate armed (phase Hi → the hi-os sibling).
//!    Armed alias floor must approach the always-hi-os floor and beat the always-os1 floor.
//!  - **G2 average cost** — sparse-transient material: the hi-os duty (and implied avg solves) must
//!    stay well under always-hi-os (the relevant baseline for an os=4 delivery).
//!  - **G3 no thrash / artifact** — a tremolo crossing the arm/disarm thresholds must not chatter
//!    (hysteresis + dwell bound the arm count) and must keep the output finite + step-bounded.
//!
//! Run: `cargo test --features adaptive-os --release --test adaptive_os_gate -- --nocapture`

#![cfg(feature = "adaptive-os")]

use ba283::dsp::ChannelDsp;
use ba283::wdf::QualityConfig;
use ba283::InferenceBackend;

const SR: f32 = 48_000.0;
const N: usize = 1 << 13;
const DISCARD: usize = 4096;
const RGAIN: f32 = 220.0;
const BIAS: f32 = 0.5;
// Oversampling of the resident hi sibling — mirrors `ADAPTIVE_HI_OS` in lib.rs. A0 confirmed the
// existing IIR decimator is clean at 4:1 (zero added latency), so the armed state runs os=4.
const HI_OS: usize = 4;

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

/// Hi sibling cfg = UltraFast levers + os=HI_OS + decimation (mirrors lib.rs).
fn hi_cfg() -> QualityConfig {
    let mut c = QualityConfig::ultra_fast();
    c.os = HI_OS;
    c.decimate = true;
    c
}

/// Build a warmed channel at `cfg`; if `adaptive`, also install the hi-os sibling.
fn build_channel(cfg: QualityConfig, adaptive: bool) -> ChannelDsp {
    let mut ch = ChannelDsp::new();
    ch.eq_prepare(SR);
    ch.wdf_rebuild(SR, RGAIN, BIAS); // prepares the DC blocker + builds realtime
    ch.wdf_rebuild_with(cfg, SR, RGAIN, BIAS); // set the actual tier cfg
    ch.wdf_warmup(); // warm + snapshot (the hi sibling re-seeds from this)
    if adaptive {
        ch.wdf_set_hi(hi_cfg(), SR, RGAIN, BIAS);
    } else {
        ch.wdf_clear_hi();
    }
    ch
}

fn run(ch: &mut ChannelDsp, input: &[f32], drive: f32) -> Vec<f32> {
    ch.reset();
    input
        .iter()
        .map(|&x| ch.process_sample(x, drive, 0.0, 1.0, 1.0, 1.0, false, InferenceBackend::Wdf))
        .collect()
}

fn sine(f0_norm: f64, amp: f32, total: usize) -> Vec<f32> {
    (0..total)
        .map(|k| amp * (std::f64::consts::TAU * f0_norm * k as f64).sin() as f32)
        .collect()
}

// G1 — armed alias floor approaches os2, beats os1.
#[test]
fn g1_alias_payoff() {
    let f0 = (0.30f64 * N as f64).round() / N as f64; // hot, high-foldback region
    let drive = 12.0;
    let amp = 0.9; // in_v peak ≈ 0.9*12*0.5 = 5.4 V ≫ arm threshold → stays armed
    let total = DISCARD + N;
    let input = sine(f0, amp, total);

    let mut lo = build_channel(QualityConfig::ultra_fast(), false);
    let mut hi = build_channel(hi_cfg(), false);
    let mut ad = build_channel(QualityConfig::ultra_fast(), true);

    let y_lo = run(&mut lo, &input, drive);
    let y_hi = run(&mut hi, &input, drive);
    let y_ad = run(&mut ad, &input, drive);

    let a_lo = alias_db(&y_lo[DISCARD..], f0);
    let a_hi = alias_db(&y_hi[DISCARD..], f0);
    let a_ad = alias_db(&y_ad[DISCARD..], f0);
    let (arm, hidur) = ad.os_stats();

    eprintln!("\n=== G1: alias floor (dB) hot f0={:.3}, real integration path ===", f0);
    eprintln!("  os1  (lo)     = {:.2}", a_lo);
    eprintln!("  os{}  (hi)     = {:.2}", HI_OS, a_hi);
    eprintln!("  adaptive(arm) = {:.2}  (arms={}, hi_samples={})", a_ad, arm, hidur);
    eprintln!("  adaptive vs hi = {:+.2} dB; vs os1 = {:+.2} dB", a_ad - a_hi, a_lo - a_ad);

    assert!(arm >= 1, "gate never armed on a sustained hot tone");
    // Armed adaptive must deliver ~hi-os quality (within 3 dB of the always-hi-os floor) ...
    assert!(
        a_ad <= a_hi + 3.0,
        "armed adaptive alias {a_ad:.2} not within 3 dB of os{HI_OS} {a_hi:.2}"
    );
    // ... and clearly beat os1 (the whole point).
    assert!(a_lo - a_ad >= 5.0, "armed adaptive ({a_ad:.2}) not ≥5 dB cleaner than os1 ({a_lo:.2})");
}

/// Sparse exponential-decay bursts (drum-like): high crest, mostly quiet between hits.
fn sparse_bursts(total: usize) -> Vec<f32> {
    let period = (0.40 * SR) as usize;
    (0..total)
        .map(|i| {
            let t = (i % period) as f32 / SR;
            let env = (-t * 28.0).exp();
            env * (std::f32::consts::TAU * 1500.0 * t).sin()
        })
        .collect()
}

// G2 — sparse-transient duty stays well under always-os2.
#[test]
fn g2_average_cost() {
    let total = (4.0 * SR) as usize; // 4 s
    let input = sparse_bursts(total);
    let drive = 12.0;

    let mut ad = build_channel(QualityConfig::ultra_fast(), true);
    ad.reset();
    ad.os_reset_stats();
    for &x in &input {
        ad.process_sample(x, drive, 0.0, 1.0, 1.0, 1.0, false, InferenceBackend::Wdf);
    }
    let (arm, hidur) = ad.os_stats();
    let duty = hidur as f64 / total as f64;
    // Armed sample costs HI_OS Newton solves, quiet sample costs 1: avg = (1-duty)*1 + duty*HI_OS.
    let avg_solves = 1.0 + duty * (HI_OS as f64 - 1.0);
    let static_hi = HI_OS as f64; // the baseline an os=4 delivery is measured against
    let saving = 1.0 - avg_solves / static_hi;

    eprintln!("\n=== G2: average cost on sparse bursts (4 s, hi os={}) ===", HI_OS);
    eprintln!(
        "  arms={}, hi_samples={}, duty={:.1}%, avg_solves≈{:.3} (static os{}={:.1}, saving {:.0}%)",
        arm, hidur, duty * 100.0, avg_solves, HI_OS, static_hi, saving * 100.0
    );
    // The win is delivering hi-os alias quality cheaper than always running hi-os. Require a clear
    // margin under static-hi-os (≥20 % saved) so adaptive is not just an expensive always-on.
    assert!(
        avg_solves < static_hi * 0.8,
        "sparse-material avg solves {avg_solves:.3} not <80% of static os{HI_OS} ({static_hi:.1}) — gate armed too often"
    );
}

// G3 — tremolo across the thresholds must not thrash; output finite + step-bounded.
#[test]
fn g3_no_thrash_no_artifact() {
    let total = (4.0 * SR) as usize;
    let drive = 12.0;
    // 14 kHz carrier amplitude-modulated by a 6 Hz tremolo that sweeps in_v through the arm/disarm
    // band (peak in_v ranges ~0.3..2.0 V around OS_ARM_V=1.0 / OS_DISARM_V=0.5).
    // Low carrier (1 kHz): the os=2 hi model has decimation group delay while os=1 lo has none, so a
    // near-Nyquist carrier phase-decorrelates the two references and the envelope test is meaningless.
    // At 1 kHz they stay phase-aligned; a genuine switch click is broadband and still escapes the band.
    let carrier = 1_000.0 / SR as f64;
    // Quiet lead-in (300 ms, in_v ≈ 0.3 V < disarm) so the startup resume-fade completes and the
    // detector stays at Lo before the tremolo begins — arming then happens in steady state, as in
    // real playback (not tangled with the reset ramp).
    let lead = (0.30 * SR) as usize;
    let input: Vec<f32> = (0..total)
        .map(|i| {
            let s = (std::f64::consts::TAU * carrier * i as f64).sin() as f32;
            if i < lead {
                0.05 * s
            } else {
                let t = (i - lead) as f32 / SR;
                // 1.5 Hz so the gaps (333 ms vs 50 ms release) let the envelope decay below disarm —
                // forces repeated arm AND disarm, exercising both seam directions (ToHi + ToLo).
                let trem = 0.5 + 0.5 * (std::f32::consts::TAU * 1.5 * t).sin();
                0.55 * trem * s
            }
        })
        .collect();

    // Reference models with NO switching, on the identical input.
    let mut lo = build_channel(QualityConfig::ultra_fast(), false);
    let mut hi = build_channel(hi_cfg(), false);
    let mut ad = build_channel(QualityConfig::ultra_fast(), true);
    let y_lo = run(&mut lo, &input, drive);
    let y_hi = run(&mut hi, &input, drive);
    ad.os_reset_stats();
    let y = run(&mut ad, &input, drive);
    let (arm, _hidur) = ad.os_stats();

    let all_finite = y.iter().all(|v| v.is_finite());
    let arms_per_s = arm as f64 / (total as f64 / SR as f64);

    // Click detector via the output's OWN second difference. A switch click is a discontinuity →
    // a 2nd-difference spike; a smooth 1 kHz tone + a smoothstep crossfade of two bounded models
    // stays small. Comparing against the NON-switching references (which see the identical loud
    // tremolo but never switch) isolates the switching's contribution: the smoothstep blend of two
    // slightly group-delay-shifted models adds at most a small factor, never an impulse. (A
    // reference-envelope test is invalid here — a re-seeded model never bit-matches a continuously
    // run one, so deviation from it measures reseed mismatch, not audible discontinuity.)
    let peak = y_hi.iter().fold(0.0f32, |m, &v| m.max(v.abs())).max(1e-6);
    let max_d2 = |s: &[f32]| {
        s.windows(3).map(|w| (w[2] - 2.0 * w[1] + w[0]).abs()).fold(0.0f32, f32::max)
    };
    let d2_ad = max_d2(&y);
    let d2_ref = max_d2(&y_lo).max(max_d2(&y_hi));
    let ratio = d2_ad / d2_ref.max(1e-9);

    eprintln!("\n=== G3: tremolo across thresholds (4 s, 1.5 Hz, 0.3 s lead-in) ===");
    eprintln!(
        "  arms={} ({:.1}/s), finite={}, peak={:.2}, max|Δ²| adaptive={:.4} vs ref={:.4} (ratio {:.2}×)",
        arm, arms_per_s, all_finite, peak, d2_ad, d2_ref, ratio
    );
    assert!(all_finite, "non-finite output — loop unstable");
    assert!(arm >= 3, "tremolo did not exercise repeated arm/disarm cycles (arms={arm})");
    // 1.5 Hz tremolo → ~1.5 arm/s; hysteresis + 100 ms dwell must keep it bounded (no chatter).
    assert!(arms_per_s <= 10.0, "switch rate {arms_per_s:.1}/s too high — detector chattering");
    // Switching must not add a discontinuity beyond what the models themselves produce. A real click
    // would be many× the steady 2nd-difference; allow 2× for the crossfade's two-model phase blend.
    assert!(ratio <= 2.0, "adaptive 2nd-difference {ratio:.2}× the non-switching reference — switch click");
}
