//! Regression: Medium/Low precision must NOT produce loud output peaks on any tier. The os=1 lever
//! (old ultra-fast/warp) left the per-sample Newton solve under-resolved; with the Med/Low fixed-step
//! solver it produced erratic limit-cycle peaks (raw ~60-86 vs ~56 at High) that a constant levelNorm
//! trim cannot tame — "VERY loud peaks on non-realtime tiers at Med/Low". The fix floors os at 2 for
//! ALL tiers. This streams a hot program for several seconds (so a slow drift/limit-cycle shows) and
//! asserts every tier x precision stays within a tight margin of the realtime+High baseline.
use ba283::dsp::ChannelDsp;
use ba283::wdf::{Precision, QualityConfig};
use ba283::InferenceBackend;

const SR: f32 = 48_000.0;
const RGAIN: f32 = 220.0;
const BIAS: f32 = 0.7;
const AGE: f32 = 0.5;
const SEED: u64 = 0xBA283_0000_0001;

/// Raw circuit peak |y| (norm=1, mix=1) over `secs` seconds after a live switch to `cfg`, at hot
/// drive. Mirrors the live path: warm realtime+High, switch tier, stream.
fn raw_peak(cfg: QualityConfig, drive: f32, amp: f32, secs: usize) -> f32 {
    let mut ch = ChannelDsp::new();
    ch.wdf_rebuild(SR, RGAIN, BIAS);
    ch.wdf_set_age(AGE);
    ch.wdf_warmup();
    let w = 2.0 * std::f32::consts::PI * 220.0 / SR;
    for i in 0..(SR as usize / 10) {
        let _ = ch.process_sample(amp * (w * i as f32).sin(), 1.0, drive, BIAS, 0.5, 1.0, 1.0, 1.0, false, InferenceBackend::Wdf);
    }
    ch.wdf_set_tier(cfg, SR, RGAIN, BIAS);
    ch.wdf_set_age_seed(SEED);
    ch.wdf_set_age(AGE);
    ch.wdf_set_rgain(RGAIN);
    ch.wdf_set_bias(BIAS);
    let mut phase = (SR as usize / 10) as f32;
    let mut peak = 0.0f32;
    for _ in 0..(SR as usize * secs) {
        let x = amp * (w * phase).sin();
        phase += 1.0;
        let y = ch.process_sample(x, 1.0, drive, BIAS, 0.5, 1.0, 1.0, 1.0, false, InferenceBackend::Wdf);
        peak = peak.max(y.abs());
    }
    peak
}

#[test]
fn medlow_precision_does_not_peak_on_any_tier() {
    let drive = 4.0; // +12 dB
    let amp = 0.9;
    let baseline = raw_peak(QualityConfig::realtime().with_precision(Precision::High), drive, amp, 2);
    // Med/Low on any tier must stay within 25% of the realtime+High baseline (pre-fix os=1 ultra/warp
    // overshot to ~1.5x and fluctuated).
    let limit = baseline * 1.25;
    for (tn, base) in [
        ("realtime", QualityConfig::realtime()),
        ("fast", QualityConfig::fast()),
        ("ultra_fast", QualityConfig::ultra_fast()),
        ("warp", QualityConfig::warp()),
    ] {
        for (pn, p) in [("High", Precision::High), ("Medium", Precision::Medium), ("Low", Precision::Low)] {
            let pk = raw_peak(base.with_precision(p), drive, amp, 5);
            eprintln!("{tn:>10}+{pn:<6} raw peak = {pk:7.2}  (baseline {baseline:.2}, limit {limit:.2})");
            assert!(
                pk <= limit,
                "{tn}+{pn} peaks at {pk:.2} > {limit:.2} (baseline {baseline:.2}) — os=1 regression?"
            );
        }
    }
}
