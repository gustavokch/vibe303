//! Regression: a rapid Precision SWEEP (High<->Med<->Low changes faster than the 60 ms crossfade and
//! the model re-settle) must not produce louder peaks than a steady single switch. `test_precision_peaks`
//! only switches ONCE then streams; this stresses repeated `wdf_set_tier` rebuilds + overlapping
//! crossfades under hot Drive — the user's actual "sweeping precision" gesture. (Investigation also
//! confirmed the off-thread worker trim and leveled output stay stable across every tier x precision,
//! so the real-world loud peaks traced to a stale installed build, not the DSP.)
use ba283::dsp::ChannelDsp;
use ba283::wdf::{Precision, QualityConfig};
use ba283::InferenceBackend;

const SR: f32 = 48_000.0;
const RGAIN: f32 = 220.0;
const BIAS: f32 = 0.7;
const AGE: f32 = 0.5;
const SEED: u64 = 0xBA283_0000_0001;

fn warm() -> ChannelDsp {
    let mut ch = ChannelDsp::new();
    ch.wdf_rebuild(SR, RGAIN, BIAS);
    ch.wdf_set_age(AGE);
    ch.wdf_warmup();
    ch
}

/// Apply a precision/tier change exactly as `process()` does on an actual change.
fn apply_switch(ch: &mut ChannelDsp, cfg: QualityConfig) {
    ch.wdf_set_tier(cfg, SR, RGAIN, BIAS);
    ch.wdf_set_age_seed(SEED);
    ch.wdf_set_age(AGE);
    ch.wdf_set_rgain(RGAIN);
    ch.wdf_set_bias(BIAS);
}

/// Raw circuit peak |y| over `secs` while the precision is swept at `switch_ms` cadence.
fn sweep_raw_peak(base: QualityConfig, switch_ms: f32, drive: f32, amp: f32, secs: f32) -> f32 {
    let mut ch = warm();
    let w = 2.0 * std::f32::consts::PI * 220.0 / SR;
    let switch_n = (SR * switch_ms / 1000.0).max(1.0) as usize;
    let total = (SR * secs) as usize;
    let order = [Precision::High, Precision::Medium, Precision::Low, Precision::Medium];
    let mut peak = 0.0f32;
    for i in 0..total {
        if i % switch_n == 0 {
            apply_switch(&mut ch, base.with_precision(order[(i / switch_n) % order.len()]));
        }
        let x = amp * (w * i as f32).sin();
        let y = ch.process_sample(x, 1.0, drive, BIAS, 1.0, 1.0, 1.0, false, InferenceBackend::Wdf);
        peak = peak.max(y.abs());
        assert!(y.is_finite(), "non-finite output during sweep on {base:?}");
    }
    peak
}

#[test]
fn precision_sweep_does_not_amplify_peaks() {
    let (drive, amp) = (4.0, 0.9);
    // Steady realtime+High single-switch baseline.
    let mut base_ch = warm();
    apply_switch(&mut base_ch, QualityConfig::realtime().with_precision(Precision::High));
    let w = 2.0 * std::f32::consts::PI * 220.0 / SR;
    let mut baseline = 0.0f32;
    for i in 0..(SR as usize * 2) {
        let y = base_ch.process_sample(amp * (w * i as f32).sin(), 1.0, drive, BIAS, 1.0, 1.0, 1.0, false, InferenceBackend::Wdf);
        baseline = baseline.max(y.abs());
    }
    // A crossfade overlap adds a small, bounded bump; well under 1.4x the steady peak on every tier.
    let limit = baseline * 1.4;
    for (tn, base) in [
        ("realtime", QualityConfig::realtime()),
        ("fast", QualityConfig::fast()),
        ("ultra_fast", QualityConfig::ultra_fast()),
        ("warp", QualityConfig::warp()),
    ] {
        // 20 ms cadence is faster than the 60 ms crossfade -> overlapping fades, the worst case.
        for sw_ms in [60.0, 20.0] {
            let pk = sweep_raw_peak(base, sw_ms, drive, amp, 3.0);
            eprintln!("{tn:>10} sweep@{sw_ms:>4.0}ms raw peak = {pk:7.2} (baseline {baseline:.2}, limit {limit:.2})");
            assert!(pk <= limit, "{tn} sweep@{sw_ms}ms peaks at {pk:.2} > {limit:.2}");
        }
    }
}
