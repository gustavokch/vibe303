//! Stage-0 de-risk for ADAPTIVE level-gated oversampling of the lossy tiers
//! (plan `~/.claude/plans/unaudhd-review-critique-and-flickering-river.md`).
//!
//! Pure measurement — NO DSP changes. Two questions, both must clear before any audio-path build:
//!
//!  1. **Alias prize.** How much does os=1 → os=2 (and → os=4) actually clean up the hot-drive alias
//!     floor? UltraFast/Warp ship at os=1 (no oversampling, no decimation), so the os1→os2 gap is the
//!     entire reward an adaptive bump can deliver on those tiers. Reuses the steep-sinc reference rig
//!     from `headroom_probe.rs` (NOT the 2×-tuned Butterworth) + `capture_internal_direct` (analytic
//!     input, no lerp). GATE: os1→os2 hot gain ≥ ~10 dB, else not worth the switching machinery.
//!
//!  2. **Arm duty cycle.** On representative (synthetic) program material, how often would a
//!     peak-follower + hysteresis + minimum-dwell detector arm the hi-os model, and what average
//!     solves/host-sample does that imply? GATE: avg solves ≤ ~1.2× the os=1 baseline on
//!     sparse-transient material (else adaptive collapses to always-os=2 — no CPU win).
//!
//! Run: `cargo test --features measure --release --test adaptive_os_stage0 -- --nocapture`

#![cfg(feature = "measure")]

use ba283::dsp::INPUT_FS_VOLTS;
use ba283::wdf::{CapTier, QualityConfig, WdfModel};

const HOST: f64 = 48_000.0;
const FS_V: f32 = INPUT_FS_VOLTS;

const N: usize = 1 << 13; // 8192-sample analysis window (host rate)
const DISCARD: usize = 4096; // settle prefix dropped before analysis

// os=1 is the UltraFast/Warp reality; 2/4 are the adaptive-bump candidates.
const OS_SET: [usize; 3] = [1, 2, 4];

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

// ---- helpers copied verbatim from headroom_probe.rs (proven reference rig) ----

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
    let margin = 0.04;
    let k_lo = 2usize;
    let k_hi = ((f0_norm * (1.0 - margin)) * n as f64).floor() as usize;
    let alias: f64 = (k_lo..=k_hi).map(pow).sum();
    let fund = pow(p).max(1e-30);
    10.0 * (alias / fund).log10()
}

/// Steep windowed-sinc decimator (Blackman-Harris); os=1 returns the input unchanged (no decimation,
/// which is exactly the UltraFast/Warp output path).
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

fn build_warm(os: usize, cap_tier: CapTier) -> WdfModel {
    let mut cfg = QualityConfig::realtime();
    cfg.os = os;
    cfg.decimate = false; // decimate externally with the steep sinc
    cfg.cap_tier = cap_tier;
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

fn cell_alias(m: &mut WdfModel, os: usize, f0_host: f64, op: &Op) -> f64 {
    let total = DISCARD + N;
    let input = make_internal_input(f0_host, os, op, total);
    m.reset();
    let mut raw = Vec::new();
    m.capture_internal_direct(&input, &mut raw);
    let host = sinc_decimate(&raw, os);
    let settled = &host[DISCARD..DISCARD + N];
    alias_db(settled, f0_host)
}

// ----------------------------------------------------------------------------
// Test 1 — the alias prize: os=1 vs os=2 vs os=4, the UltraFast/Warp reward.
// ----------------------------------------------------------------------------
#[test]
fn lossy_tier_alias_prize() {
    let mut models: Vec<(usize, WdfModel)> =
        OS_SET.iter().map(|&os| (os, build_warm(os, CapTier::Full))).collect();

    eprintln!(
        "\n=== Stage 0.1: lossy-tier alias prize (floor dB, lower = cleaner) ===\n\
         {:<9} {:>6} {:>9} {:>9} {:>9} {:>10} {:>10}",
        "op", "f0", "os1", "os2", "os4", "g1-2", "g1-4"
    );
    let mut max_hot_g12 = f64::NEG_INFINITY;
    let mut max_hot_g14 = f64::NEG_INFINITY;
    for op in &OPS {
        for &target in &F0_TARGETS {
            let p = (target * N as f64).round() as usize;
            let f0_host = p as f64 / N as f64;
            let mut floors = [0.0f64; 3];
            for (i, (os, m)) in models.iter_mut().enumerate() {
                floors[i] = cell_alias(m, *os, f0_host, op);
            }
            let g12 = floors[0] - floors[1]; // os1 − os2
            let g14 = floors[0] - floors[2]; // os1 − os4
            if op.name == "hot" {
                max_hot_g12 = max_hot_g12.max(g12);
                max_hot_g14 = max_hot_g14.max(g14);
            }
            eprintln!(
                "{:<9} {:>6.3} {:>9.2} {:>9.2} {:>9.2} {:>+10.2} {:>+10.2}",
                op.name, f0_host, floors[0], floors[1], floors[2], g12, g14
            );
        }
    }
    eprintln!(
        "\nHot-drive MAX prize: os1→os2 = {:.2} dB, os1→os4 = {:.2} dB.",
        max_hot_g12, max_hot_g14
    );
    eprintln!(
        "GATE: os1→os2 ≥ ~10 dB ⇒ adaptive bump worthwhile. Here = {:.2} dB ⇒ {}.\n",
        max_hot_g12,
        if max_hot_g12 >= 10.0 { "PRIZE OK" } else { "WEAK (reconsider — just use Fast)" }
    );
}

// ----------------------------------------------------------------------------
// Test 1b (A0 gate) — decimator quality at 4:1.
//
// The adaptive arm can bump os=1 → os=4 ONLY with a ZERO-LATENCY decimator (the
// 0-PDC contract forbids a per-transient FIR group delay). The shipped decimator
// is a 4th-order Butterworth biquad cascade (fc = 0.45*host) — IIR, ~0 integer
// latency — tuned for 2:1. This test asks whether that, or a steeper still-IIR
// Butterworth, is clean enough at 4:1 (within ~1–2 dB of the steep-sinc ideal).
//
//   GO (adaptive 1↔4): a zero-latency IIR lands within ~2 dB of sinc at 4:1.
//   NO-GO            : nothing zero-latency is clean ⇒ os=4 only via the static
//                      Fast tier with a FIR halfband + fixed reported latency.
// ----------------------------------------------------------------------------

/// Mirror of `wdf::components::Biquad` (RBJ lowpass, Direct Form I) — self-contained so the test
/// does not reach into `pub(in crate::wdf)` internals.
#[derive(Clone, Copy, Default)]
struct TBiquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}
impl TBiquad {
    fn lowpass(fc: f64, fs: f64, q: f64) -> Self {
        let w0 = 2.0 * std::f64::consts::PI * fc / fs;
        let (c, s) = (w0.cos(), w0.sin());
        let al = s / (2.0 * q);
        let a0 = 1.0 + al;
        TBiquad {
            b0: (1.0 - c) / 2.0 / a0,
            b1: (1.0 - c) / a0,
            b2: (1.0 - c) / 2.0 / a0,
            a1: (-2.0 * c) / a0,
            a2: (1.0 - al) / a0,
            ..Default::default()
        }
    }
    #[inline]
    fn process(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2 - self.a1 * self.y1
            - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

// Butterworth section Q tables (even orders). Order 4 row equals the shipped decimator.
const BUTTER_Q4: [f64; 2] = [0.54119610, 1.30656296];
const BUTTER_Q6: [f64; 3] = [0.51763809, 0.70710678, 1.93185165];
const BUTTER_Q8: [f64; 4] = [0.50979558, 0.60134489, 0.89997622, 2.56291545];

/// Filter `raw` (at `os`×host rate) through a Butterworth biquad cascade at `fc = 0.45*host`, then
/// downsample by `os` (taking the last subsample of each host frame, matching `process_host_sample`).
fn butter_decimate(raw: &[f32], os: usize, qs: &[f64]) -> Vec<f32> {
    let fs = HOST * os as f64;
    let fc = 0.45 * HOST;
    let mut secs: Vec<TBiquad> = qs.iter().map(|&q| TBiquad::lowpass(fc, fs, q)).collect();
    let mut out = Vec::with_capacity(raw.len() / os);
    let mut o = os - 1;
    // Filter every subsample (IIR state must see them all); emit at the host grid.
    let mut idx = 0usize;
    while idx < raw.len() {
        let mut v = raw[idx] as f64;
        for s in secs.iter_mut() {
            v = s.process(v);
        }
        if idx == o {
            out.push(v as f32);
            o += os;
        }
        idx += 1;
    }
    out
}

#[test]
fn decimator_4to1_gate() {
    const OS: usize = 4;
    let op = &OPS[1]; // hot
    let mut m = build_warm(OS, CapTier::Full);

    eprintln!(
        "\n=== A0: zero-latency decimator quality at 4:1 (hot drive) ===\n\
         alias floor dB (lower=cleaner); gap = IIR − sinc (→0 = matches the ideal)\n\
         {:>6} {:>9} {:>9} {:>9} {:>9} {:>8} {:>8} {:>8}",
        "f0", "sinc", "butter4", "butter6", "butter8", "g4", "g6", "g8"
    );
    let mut worst_g4 = f64::NEG_INFINITY;
    let mut worst_g6 = f64::NEG_INFINITY;
    let mut worst_g8 = f64::NEG_INFINITY;
    for &target in &F0_TARGETS {
        let p = (target * N as f64).round() as usize;
        let f0_host = p as f64 / N as f64;
        let total = DISCARD + N;
        let input = make_internal_input(f0_host, OS, op, total);

        m.reset();
        let mut raw = Vec::new();
        m.capture_internal_direct(&input, &mut raw);

        let floor = |host: &[f32]| alias_db(&host[DISCARD..DISCARD + N], f0_host);
        let f_sinc = floor(&sinc_decimate(&raw, OS));
        let f_b4 = floor(&butter_decimate(&raw, OS, &BUTTER_Q4));
        let f_b6 = floor(&butter_decimate(&raw, OS, &BUTTER_Q6));
        let f_b8 = floor(&butter_decimate(&raw, OS, &BUTTER_Q8));
        let (g4, g6, g8) = (f_b4 - f_sinc, f_b6 - f_sinc, f_b8 - f_sinc);
        worst_g4 = worst_g4.max(g4);
        worst_g6 = worst_g6.max(g6);
        worst_g8 = worst_g8.max(g8);
        eprintln!(
            "{:>6.3} {:>9.2} {:>9.2} {:>9.2} {:>9.2} {:>+8.2} {:>+8.2} {:>+8.2}",
            f0_host, f_sinc, f_b4, f_b6, f_b8, g4, g6, g8
        );
    }
    eprintln!(
        "\nWORST gap vs sinc: butter4 = {:+.2} dB, butter6 = {:+.2} dB, butter8 = {:+.2} dB.",
        worst_g4, worst_g6, worst_g8
    );
    let best = worst_g4.min(worst_g6).min(worst_g8);
    eprintln!(
        "GATE: a zero-latency IIR within ~2 dB of sinc ⇒ adaptive 1↔4 GO. Best = {:+.2} dB ⇒ {}.\n",
        best,
        if best <= 2.0 { "GO (adaptive os=4 viable)" } else { "NO-GO (fall back to static Fast 2→4 + FIR)" }
    );
}

// ----------------------------------------------------------------------------
// Test 2 — arm duty-cycle simulation: how often would the gate engage hi-os,
// and what average solves/host-sample does that cost?
// ----------------------------------------------------------------------------

/// Peak follower + hysteresis + minimum-dwell, the candidate detector. Returns (armed_fraction,
/// switch_count). `thr_hi`/`thr_lo` in the same volts as `in_v` (= driven signal × FS_V).
fn simulate_detector(sig: &[f32], thr_hi: f64, thr_lo: f64, rel_coef: f64, dwell: usize) -> (f64, usize) {
    let mut env = 0.0f64;
    let mut armed = false;
    let mut dwell_remain = 0usize;
    let mut armed_samples = 0usize;
    let mut switches = 0usize;
    for &x in sig {
        // Instant attack, exponential release peak follower.
        let a = (x as f64).abs();
        env = if a > env { a } else { env * rel_coef };
        if armed {
            if dwell_remain > 0 {
                dwell_remain -= 1;
            }
            // Disarm only after dwell elapsed AND env fell below the low threshold.
            if dwell_remain == 0 && env < thr_lo {
                armed = false;
                switches += 1;
            }
        } else if env >= thr_hi {
            armed = true;
            dwell_remain = dwell;
            switches += 1;
        }
        if armed {
            armed_samples += 1;
        }
    }
    (armed_samples as f64 / sig.len() as f64, switches)
}

/// Cheap deterministic pseudo-noise in [-1,1] (no rand dep); varies per index.
fn prn(i: usize) -> f32 {
    // xorshift-ish hash → [-1,1]
    let mut x = (i as u64).wrapping_mul(0x9E3779B97F4A7C15).wrapping_add(0x1234567);
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58476D1CE4E5B9);
    x ^= x >> 27;
    ((x >> 40) as f32 / (1u32 << 23) as f32) - 1.0
}

/// Synthetic archetypes at host rate, normalized to ~unit peak BEFORE drive is applied.
fn synth(kind: &str, n: usize) -> Vec<f32> {
    let sr = HOST as f32;
    match kind {
        // Sparse drum hits: exponential-decay bursts every ~0.4 s, near-silent between.
        "drums" => (0..n)
            .map(|i| {
                let period = (0.40 * sr) as usize;
                let ph = i % period;
                let t = ph as f32 / sr;
                let env = (-t * 28.0).exp(); // fast decay
                let tone = (std::f32::consts::TAU * 110.0 * t).sin() * 0.6 + prn(i) * 0.4;
                env * tone
            })
            .collect(),
        // Sustained bass: continuous tone at a steady moderate level (rarely silent).
        "bass" => (0..n)
            .map(|i| {
                let t = i as f32 / sr;
                0.7 * (std::f32::consts::TAU * 80.0 * t).sin()
                    + 0.2 * (std::f32::consts::TAU * 160.0 * t).sin()
            })
            .collect(),
        // Busy mix: broadband-ish with frequent but not constant peaks.
        "mix" => (0..n)
            .map(|i| {
                let t = i as f32 / sr;
                let lf = 0.45 * (std::f32::consts::TAU * 220.0 * t).sin();
                let mf = 0.3 * (std::f32::consts::TAU * 1500.0 * t).sin();
                // Slow amplitude swell so peaks come and go.
                let swell = 0.6 + 0.4 * (std::f32::consts::TAU * 1.3 * t).sin();
                (lf + mf + prn(i) * 0.25) * swell
            })
            .collect(),
        _ => unreachable!(),
    }
}

#[test]
fn arm_duty_cycle_sim() {
    let n = (4.0 * HOST) as usize; // 4 s per archetype
    // Detector constants from the plan.
    let tau_rel = 0.050; // 50 ms release
    let rel_coef = (-1.0 / (tau_rel * HOST)).exp();
    let dwell = (0.100 * HOST) as usize; // 100 ms minimum dwell

    // Drive applied to the normalized signal (the "Drive" knob). Hot-tone peak ≈ 0.9*12 = 10.8 ·FS_V;
    // moderate ≈ 0.5*4 = 2.0 ·FS_V. Pick a representative drive and sweep the arm threshold across
    // that range to see the duty/cost trade.
    let drive = 8.0f32;
    let thr_set_fsv = [1.5f64, 2.5, 4.0, 6.0, 8.0]; // T_hi in units of FS_V
    let kinds = ["drums", "bass", "mix"];

    eprintln!(
        "\n=== Stage 0.2: arm duty cycle + avg solves (drive={:.0}, hyst=-6dB, dwell=100ms, rel=50ms) ===",
        drive
    );
    eprintln!(
        "Cost model: avg_solves = (1-duty)*1 + duty*2  (os=1 baseline=1.0; always-os2=2.0).\n\
         {:<7} {:>8} {:>8} {:>8} {:>8}",
        "kind", "Thi/FSV", "duty%", "avgSolv", "switch/s"
    );
    let mut worst_drum_avg = 0.0f64;
    for kind in kinds {
        let base = synth(kind, n);
        // in_v = driven signal × FS_V (mirrors dsp.rs: in_v = driven * INPUT_FS_VOLTS).
        let sig: Vec<f32> = base.iter().map(|&s| s * drive * FS_V).collect();
        for &thi in &thr_set_fsv {
            let thr_hi = thi * FS_V as f64;
            let thr_lo = thr_hi * 0.5; // -6 dB hysteresis
            let (duty, switches) = simulate_detector(&sig, thr_hi, thr_lo, rel_coef, dwell);
            let avg_solves = (1.0 - duty) * 1.0 + duty * 2.0;
            let sw_per_s = switches as f64 / (n as f64 / HOST);
            if kind == "drums" {
                worst_drum_avg = worst_drum_avg.max(avg_solves.min(1.0 + duty)); // track sparse-material cost
            }
            eprintln!(
                "{:<7} {:>8.1} {:>8.1} {:>8.3} {:>8.1}",
                kind, thi, duty * 100.0, avg_solves, sw_per_s
            );
        }
    }
    eprintln!(
        "\nGATE: on sparse-transient material (drums) at a threshold where alias is objectionable,\n\
         avg solves ≤ ~1.2 ⇒ adaptive preserves the os=1 win. Sustained material (bass) correctly\n\
         stays hi-os (duty→100%) — that's right, not a failure. Pick the threshold from Stage 0.1's\n\
         floor knee.\n"
    );
}

// ----------------------------------------------------------------------------
// Test 4 — arm-threshold calibration: sweep the input drive LEVEL at os=1 and find
// the "alias knee" (the in_v peak where the os=1 alias floor crosses audibility).
// That knee is the arm threshold; disarm = knee − 6 dB.
// ----------------------------------------------------------------------------
#[test]
fn calibrate_arm_threshold() {
    // Level = peak |driven| in the test's amp·drive units; in_v peak = level · FS_V volts.
    let levels = [0.5f64, 1.0, 1.5, 2.0, 3.0, 4.0, 6.0, 9.0, 12.0];
    let f0_targets = [0.18f64, 0.24, 0.30]; // hot-foldback band
    const AUDIBLE_DB: f64 = -40.0; // alias floor above this is treated as objectionable

    let mut m1 = build_warm(1, CapTier::Full);
    let mut m2 = build_warm(2, CapTier::Full);

    eprintln!(
        "\n=== Stage 0.4: arm-threshold calibration (os=1 alias vs drive level) ===\n\
         in_v is the peak input volts the envelope follower sees. Knee = first level whose WORST-f0\n\
         os=1 alias exceeds {:.0} dB.\n{:>7} {:>7} {:>10} {:>10}",
        AUDIBLE_DB, "level", "in_vpk", "os1_worst", "os2_worst"
    );
    let mut knee_in_v = f64::NAN;
    for &level in &levels {
        let op = Op { name: "cal", amp: level as f32, drive: 1.0 };
        let mut os1_worst = f64::NEG_INFINITY;
        let mut os2_worst = f64::NEG_INFINITY;
        for &target in &f0_targets {
            let p = (target * N as f64).round() as usize;
            let f0 = p as f64 / N as f64;
            os1_worst = os1_worst.max(cell_alias(&mut m1, 1, f0, &op));
            os2_worst = os2_worst.max(cell_alias(&mut m2, 2, f0, &op));
        }
        let in_vpk = level * FS_V as f64;
        if knee_in_v.is_nan() && os1_worst > AUDIBLE_DB {
            knee_in_v = in_vpk;
        }
        eprintln!("{:>7.1} {:>7.2} {:>10.2} {:>10.2}", level, in_vpk, os1_worst, os2_worst);
    }
    eprintln!(
        "\nAlias knee (os=1 crosses {:.0} dB) at in_v ≈ {:.2} V → suggested OS_ARM_V ≈ {:.2}, \
         OS_DISARM_V ≈ {:.2} (−6 dB).",
        AUDIBLE_DB, knee_in_v, knee_in_v, knee_in_v * 0.5
    );
    eprintln!("(Current shipped-spike values: OS_ARM_V = 1.0 V, OS_DISARM_V = 0.5 V.)\n");
}

// ----------------------------------------------------------------------------
// Test 3 — duty cycle on REAL audio files (firms the synthetic Test 2). Loads
// the repo's dry test signals (pink/sweep/twotone/multisine/impulse — real
// amplitude statistics, not hand-rolled), plus any WAVs in $BA283_REAL_WAV_DIR
// so real program-material (drums/mix) can be dropped in without code change.
// ----------------------------------------------------------------------------

/// Load a WAV as mono f32 (downmix = take channel 0). Handles float + int formats.
fn load_wav_mono(path: &std::path::Path) -> Option<(Vec<f32>, u32)> {
    let mut r = hound::WavReader::open(path).ok()?;
    let spec = r.spec();
    let ch = spec.channels as usize;
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => r.samples::<f32>().filter_map(|s| s.ok()).collect(),
        hound::SampleFormat::Int => {
            let scale = 1.0 / ((1u64 << (spec.bits_per_sample - 1)) as f32);
            r.samples::<i32>().filter_map(|s| s.ok()).map(|v| v as f32 * scale).collect()
        }
    };
    if ch <= 1 {
        Some((samples, spec.sample_rate))
    } else {
        Some((samples.iter().step_by(ch).copied().collect(), spec.sample_rate))
    }
}

#[test]
fn arm_duty_cycle_real_wavs() {
    let drive = 8.0f32;
    let thr_set_fsv = [1.5f64, 2.5, 4.0, 6.0];

    // Repo dry signals (real amplitude statistics) + optional external real program material.
    let dry_dir = format!("{}/../data/dry", env!("CARGO_MANIFEST_DIR"));
    let mut files: Vec<std::path::PathBuf> = [
        "pink_m18.wav", "pink_m6.wav", "multisine_m12.wav", "multisine_m3.wav",
        "twotone_m12.wav", "twotone_m3.wav", "sweep_m6.wav", "impulse.wav",
    ]
    .iter()
    .map(|f| std::path::PathBuf::from(format!("{dry_dir}/{f}")))
    .filter(|p| p.exists())
    .collect();
    if let Ok(extra) = std::env::var("BA283_REAL_WAV_DIR") {
        if let Ok(rd) = std::fs::read_dir(&extra) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().and_then(|s| s.to_str()) == Some("wav") {
                    files.push(p);
                }
            }
        }
    }

    eprintln!(
        "\n=== Stage 0.3: arm duty cycle on REAL files (drive={:.0}, hyst=-6dB, dwell=100ms, rel=50ms) ===\n\
         {:<22} {:>6} {:>7} {:>8} {:>8} {:>8} {:>8}",
        drive, "file", "sr", "peak", "Thi1.5", "Thi2.5", "Thi4.0", "Thi6.0"
    );
    if files.is_empty() {
        eprintln!("(no WAVs found — set BA283_REAL_WAV_DIR or check data/dry)");
        return;
    }
    for path in &files {
        let Some((raw, sr)) = load_wav_mono(path) else { continue };
        if raw.is_empty() {
            continue;
        }
        // SR-relative detector coefficients.
        let rel_coef = (-1.0 / (0.050 * sr as f64)).exp();
        let dwell = (0.100 * sr as f64) as usize;
        let sig: Vec<f32> = raw.iter().map(|&s| s * drive * FS_V).collect();
        let peak = sig.iter().fold(0.0f32, |m, &v| m.max(v.abs())) / FS_V; // in FS_V units
        let mut avg = [0.0f64; 4];
        for (i, &thi) in thr_set_fsv.iter().enumerate() {
            let thr_hi = thi * FS_V as f64;
            let (duty, _sw) = simulate_detector(&sig, thr_hi, thr_hi * 0.5, rel_coef, dwell);
            avg[i] = 1.0 + duty; // avg_solves = (1-duty)*1 + duty*2
        }
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("?");
        eprintln!(
            "{:<22} {:>6} {:>7.2} {:>8.3} {:>8.3} {:>8.3} {:>8.3}",
            name, sr, peak, avg[0], avg[1], avg[2], avg[3]
        );
    }
    eprintln!(
        "\nValues = avg solves/host-sample (1.0=always os1, 2.0=always os2). Continuous material\n\
         (pink/sweep/multisine) correctly pins ~2.0 when above threshold; impulse ~1.0. The sparse-\n\
         transient WIN case needs a real drum/mix WAV via BA283_REAL_WAV_DIR.\n"
    );
}
