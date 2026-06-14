//! Phase-1 repro: Precision Medium/Low must not produce NaN/Inf on ANY speed tier.
//!
//! Defect under investigation: setting Precision = Medium or Low yields a NaN / dead channel on
//! every tier *except* Realtime (Realtime+Med/Low is fine; Fast/Ultra/Warp + Med/Low diverge).
//!
//! Faithful to the plugin signal path (`dsp.rs::process_sample` / `lib.rs::process`):
//!   - The circuit is fed `x · drive · INPUT_FS_VOLTS` volts. Drive ranges to +24 dB (×15.85), so a
//!     near-full-scale input swings the circuit input to ~7.9 V — the hot operating point where a
//!     fixed-step solver (Med/Low: exactly 2 iters, no residual gate, no line search) can let
//!     `exp(vbe/Vt)` run away to Inf → NaN. The earlier ~0.45 V stimulus never reached that regime.
//!   - `new_with(cfg, host_sr, rgain, bias)`: rgain = Circuit Gain Ω (47..10000, default 220);
//!     bias = the 0..1 Bias param (default 0.5). (NOT 220 — that was a prior repro bug.)
//!
//! Two repro paths, because the plugin never cold-starts a non-Realtime tier:
//!   1. `cold_start_*`  — build+warm each combo fresh, then drive.
//!   2. `switch_into_*` — mirror the live path (`ChannelDsp::wdf_set_tier`): warm at Realtime+High,
//!        snapshot, rebuild at the target tier+precision, RESTORE the Realtime snapshot, then drive.
//!
//! Run in `--release`: shipped plugin is release (FMA/cheap_exp/fast-math differ from debug).

use ba283::dsp::ChannelDsp;
use ba283::wdf::{self, Precision, QualityConfig};
use ba283::InferenceBackend;

const HOST_SR: f64 = 96_000.0;
const INPUT_FS_VOLTS: f64 = 0.5; // dsp.rs: 0 dBFS ≡ 0.5 V

const TIERS: [&str; 4] = ["realtime", "fast", "ultra_fast", "warp"];
const PRECISIONS: [(&str, Precision); 3] = [
    ("High", Precision::High),
    ("Medium", Precision::Medium),
    ("Low", Precision::Low),
];

/// Operating points spanning the param extremes: (circuit_gain Ω, bias 0..1, drive dB).
/// Includes the hottest reachable input (full-scale × +24 dB drive) at both gain extremes.
const OPS: [(f64, f64, f64); 5] = [
    (220.0, 0.5, 0.0),    // defaults
    (220.0, 0.5, 24.0),   // hot drive, default gain
    (47.0, 0.0, 24.0),    // min gain, min bias, hot
    (10_000.0, 1.0, 24.0), // max gain, max bias, hot
    (47.0, 1.0, 12.0),     // mixed
];

/// dBFS→volts at the circuit input for a given drive (dB). `samp` is a host sample in [-1, 1].
fn to_volts(samp: f64, drive_db: f64) -> f32 {
    let drive = 10.0_f64.powf(drive_db / 20.0);
    (samp * drive * INPUT_FS_VOLTS) as f32
}

/// 20 Hz→18 kHz log sweep at near full scale (0.95) with a hard DC step mid-stream, expressed in
/// circuit volts at the given drive.
fn stimulus(drive_db: f64) -> Vec<f32> {
    let n = (HOST_SR * 0.25) as usize;
    let step_at = n / 2;
    let (f0, f1) = (20.0_f64, 18_000.0_f64);
    let k = (f1 / f0).ln();
    let mut phase = 0.0_f64;
    (0..n)
        .map(|i| {
            let t = i as f64 / n as f64;
            let f = f0 * (k * t).exp();
            phase += 2.0 * std::f64::consts::PI * f / HOST_SR;
            let samp = 0.95 * phase.sin() + if i >= step_at { 0.75 } else { 0.0 };
            to_volts(samp.clamp(-1.0, 1.0), drive_db)
        })
        .collect()
}

fn tier_config(tier: &str) -> QualityConfig {
    match tier {
        "realtime" => QualityConfig::realtime(),
        "fast" => QualityConfig::fast(),
        "ultra_fast" => QualityConfig::ultra_fast(),
        "warp" => QualityConfig::warp(),
        _ => unreachable!(),
    }
}

/// Host-sample sweep (NOT pre-scaled to volts): `ChannelDsp::process_sample` applies
/// `drive · INPUT_FS_VOLTS` itself. 20 Hz→18 kHz at 0.95 FS + a 0.75 FS DC step mid-stream.
/// `sr` selects the sweep length / Nyquist; the sub-96k path engages the oversampler + the WDF's
/// internal os=2 decimation biquads (reset to zero on a tier rebuild) and uses a larger solve `dt`.
fn host_sweep(sr: f64) -> Vec<f32> {
    let n = (sr * 0.2) as usize;
    let step_at = n / 2;
    let f1 = (sr * 0.45).min(18_000.0); // stay under Nyquist at 44.1k
    let (f0, k) = (20.0_f64, (f1 / 20.0).ln());
    let mut phase = 0.0_f64;
    (0..n)
        .map(|i| {
            let t = i as f64 / n as f64;
            let f = f0 * (k * t).exp();
            phase += 2.0 * std::f64::consts::PI * f / sr;
            let s = 0.95 * phase.sin() + if i >= step_at { 0.75 } else { 0.0 };
            s.clamp(-1.0, 1.0) as f32
        })
        .collect()
}

/// Sample rates exercised by the live-switch repro: 44.1/48k drive the sub-96k oversampling +
/// decimation path; 96k is the direct path the WdfModel-level tests already covered.
const SAMPLE_RATES: [f64; 3] = [44_100.0, 48_000.0, 96_000.0];

/// Live mid-stream switch through the full `ChannelDsp` chain, mirroring `lib.rs::process`:
/// warm at Realtime+High, stream, then `wdf_set_tier(target)` + the forced age/rgain/bias re-stamps
/// (last_* = NaN path), then keep streaming. Returns the first non-finite output index post-switch.
#[test]
fn live_switch_all_combos_finite() {
    let mut failures = Vec::new();
    // The two hottest operating points (default-gain hot drive, and min-gain/min-bias hot).
    let ops = [(220.0_f64, 0.5_f64, 24.0_f64), (47.0, 0.0, 24.0)];
    // Trafo (JA-core saturation) knob: 0.5 is the shipped default; 1.0 is max saturation. With
    // `trafo-core` (a default feature) the DAW applies this at block rate; a value of 0 keeps the
    // core linear. Only the higher values exercise the nonlinear RK4 post-solve filter.
    #[cfg(feature = "trafo-core")]
    let trafos: &[f32] = &[0.5, 1.0];
    #[cfg(not(feature = "trafo-core"))]
    let trafos: &[f32] = &[0.0];

    for sr in [48_000.0_f64, 96_000.0] {
        let sweep = host_sweep(sr);
        let host_ge_96k = sr >= 96_000.0;
        for &(rgain_f, bias_f, drive_db) in &ops {
            let (rgain, bias) = (rgain_f as f32, bias_f as f32);
            let drive = 10.0_f32.powf(drive_db as f32 / 20.0);
            for &trafo in trafos {
                for tier in TIERS {
                    for (pname, p) in PRECISIONS {
                        let mut ch = ChannelDsp::new();
                        // initialize(): realtime tier, warm + carry snapshot, apply trafo.
                        ch.wdf_rebuild(sr as f32, rgain, bias);
                        ch.wdf_warmup();
                        #[cfg(feature = "trafo-core")]
                        ch.wdf_set_trafo(trafo);
                        // A "dead channel" is reported by the user as "loud garbage then dies" —
                        // i.e. the fixed-step solver DIVERGES to large *finite* values (not Inf), so
                        // a bare is_finite() check misses it. Flag non-finite OR a runaway magnitude
                        // far above any musical level (the circuit output rarely exceeds ~10).
                        const DIVERGE: f32 = 1.0e3;
                        let run = |ch: &mut ChannelDsp, s: &[f32]| -> Option<(usize, f32)> {
                            for (i, &x) in s.iter().enumerate() {
                                let y = ch.process_sample(
                                    x, drive, bias, 1.0, 1.0, 1.0, host_ge_96k,
                                    InferenceBackend::Wdf,
                                );
                                if !y.is_finite() || y.abs() > DIVERGE {
                                    return Some((i, y));
                                }
                            }
                            None
                        };
                        // Pre-switch stream at Realtime+High.
                        if let Some((i, y)) = run(&mut ch, &sweep) {
                            failures.push(format!(
                                "{tier}+{pname} @ {sr}Hz drive={drive_db}dB trafo={trafo}: diverged PRE-switch (y={y:.3e} @ {i})"
                            ));
                            continue;
                        }
                        // The live precision/tier change (lib.rs:458-519): set_tier + forced re-stamps.
                        let cfg = tier_config(tier).with_precision(p);
                        ch.wdf_set_tier(cfg, sr as f32, rgain, bias);
                        ch.wdf_set_age_seed(DEFAULT_AGE_SEED);
                        ch.wdf_set_age(0.0);
                        #[cfg(feature = "trafo-core")]
                        ch.wdf_set_trafo(trafo);
                        ch.wdf_set_rgain(rgain);
                        ch.wdf_set_bias(bias);
                        // Post-switch stream at the target combo.
                        if let Some((idx, y)) = run(&mut ch, &sweep) {
                            failures.push(format!(
                                "{tier}+{pname} @ ({sr}Hz, rgain={rgain}, bias={bias}, drive={drive_db}dB, trafo={trafo}): diverged post-switch (y={y:.3e} @ sample {idx})"
                            ));
                        }
                    }
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "[live-switch] non-finite WDF output on {} cell(s):\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}

const DEFAULT_AGE_SEED: u64 = 0xBA283_0000_0001;

/// Stream a hot-drive sweep through a model built at `cfg` (post a Realtime+High warm/switch) and
/// return the peak |output|. Used to prove the predict-under-fixed-step divergence and its fix.
fn peak_after_switch(cfg: QualityConfig, sr: f64) -> f32 {
    let (rgain, bias, drive_db, _trafo) = (220.0_f32, 0.5_f32, 24.0_f32, 0.5_f32);
    let drive = 10.0_f32.powf(drive_db / 20.0);
    let sweep = host_sweep(sr);
    let mut ch = ChannelDsp::new();
    ch.wdf_rebuild(sr as f32, rgain, bias);
    ch.wdf_warmup();
    #[cfg(feature = "trafo-core")]
    ch.wdf_set_trafo(_trafo);
    for &x in &sweep {
        ch.process_sample(x, drive, bias, 1.0, 1.0, 1.0, false, InferenceBackend::Wdf);
    }
    ch.wdf_set_tier(cfg, sr as f32, rgain, bias);
    ch.wdf_set_age_seed(DEFAULT_AGE_SEED);
    ch.wdf_set_age(0.0);
    #[cfg(feature = "trafo-core")]
    ch.wdf_set_trafo(_trafo);
    ch.wdf_set_rgain(rgain);
    ch.wdf_set_bias(bias);
    let mut peak = 0.0f32;
    for &x in &sweep {
        let y = ch.process_sample(x, drive, bias, 1.0, 1.0, 1.0, false, InferenceBackend::Wdf);
        if y.is_finite() && y.abs() > peak {
            peak = y.abs();
        }
    }
    peak
}

/// Root cause + fix regression. The Newton predictor under the fixed-step solver (Med/Low) was the
/// divergence driver (lever isolation: predict=false bounded the output while Dense/freeze_caps/fma
/// did not). The fix has two layers, both asserted here:
///   1. `with_precision(Med/Low)` clears `predict` on every tier (self-consistent config).
///   2. A runtime guard skips prediction whenever `fixed_step` — so even a hand-assembled
///      predict+fixed_step config stays bounded.
#[test]
fn fixed_step_never_predicts_and_stays_bounded() {
    // Layer 1: the config-level fix — predict is off for Med/Low on every tier.
    for cfg in [
        QualityConfig::fast(),
        QualityConfig::ultra_fast(),
        QualityConfig::warp(),
        QualityConfig::realtime(),
    ] {
        for p in [Precision::Medium, Precision::Low] {
            let c = cfg.with_precision(p);
            assert!(c.fixed_step, "Med/Low must be fixed-step");
            assert!(!c.predict, "Med/Low must not carry the Newton predictor (divergence driver)");
        }
    }

    // Layer 2: the runtime guard — a deliberately mis-assembled config that still has predict=true
    // alongside fixed_step must NOT diverge, because the solver skips prediction under fixed-step.
    let mut broken = QualityConfig::fast();
    broken.fixed_step = true;
    broken.max_iters = 2;
    broken.cap_tier = wdf::CapTier::Depletion;
    broken.predict = true; // the pre-fix shape that ran away to ~1e8
    let peak = peak_after_switch(broken, 48_000.0);
    assert!(
        peak < 1.0e3,
        "runtime guard failed: predict+fixed_step diverged (peak {peak:.3e})"
    );

    // And the shipped path (fast+Medium) is comfortably bounded.
    let shipped = peak_after_switch(QualityConfig::fast().with_precision(Precision::Medium), 48_000.0);
    assert!(shipped < 1.0e3, "fast+Medium still diverges (peak {shipped:.3e})");
}

fn drive_first_nonfinite(m: &mut wdf::WdfModel, input: &[f32]) -> Option<usize> {
    for (i, &x) in input.iter().enumerate() {
        if !m.process_host_sample(x).is_finite() {
            return Some(i);
        }
    }
    None
}

#[test]
fn cold_start_all_combos_finite() {
    let mut failures = Vec::new();
    for &(rgain, bias, drive_db) in &OPS {
        let input = stimulus(drive_db);
        for tier in TIERS {
            for (pname, p) in PRECISIONS {
                let cfg = tier_config(tier).with_precision(p);
                let mut m = wdf::WdfModel::new_with(cfg, HOST_SR, rgain, bias);
                m.warmup();
                if let Some(idx) = drive_first_nonfinite(&mut m, &input) {
                    failures.push(format!(
                        "{tier}+{pname} @ (rgain={rgain}, bias={bias}, drive={drive_db}dB): NaN at sample {idx}"
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "[cold-start] non-finite WDF output on {} cell(s):\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}

#[test]
fn switch_into_all_combos_finite() {
    let mut failures = Vec::new();
    for &(rgain, bias, drive_db) in &OPS {
        let input = stimulus(drive_db);
        // Warm the baseline tier the plugin always starts on; capture its settled snapshot.
        let mut base = wdf::WdfModel::new_with(
            QualityConfig::realtime().with_precision(Precision::High),
            HOST_SR,
            rgain,
            bias,
        );
        base.warmup();
        let snap = base.snapshot();
        for tier in TIERS {
            for (pname, p) in PRECISIONS {
                let cfg = tier_config(tier).with_precision(p);
                // Mirror wdf_set_tier: fresh model at cfg, restore the foreign (Realtime) snapshot.
                let mut m = wdf::WdfModel::new_with(cfg, HOST_SR, rgain, bias);
                m.restore(&snap);
                m.save_snapshot();
                if let Some(idx) = drive_first_nonfinite(&mut m, &input) {
                    failures.push(format!(
                        "{tier}+{pname} @ (rgain={rgain}, bias={bias}, drive={drive_db}dB): NaN at sample {idx}"
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "[switch] non-finite WDF output on {} cell(s):\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}
