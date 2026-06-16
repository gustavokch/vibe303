//! Diagnostic: Circuit-Gain sweep under the user's rig (Drive up, Age/Bias moved, non-realtime
//! tier) with level norm ON. Mirrors lib.rs: rgain one-pole glide + relative snap, reprobe target
//! 1/(G_v*drive) on each changed block, per-sample ramp of the applied gain. Prints per-block
//! out/in and peak|y| so we can see whether the residual is a continuous-tracking peak (continuous
//! knob problem) or a small static offset.
use ba283::dsp::ChannelDsp;
use ba283::wdf::QualityConfig;
use ba283::{norm_apply_gain, InferenceBackend};

const SR: f32 = 48_000.0;
const PARAM_TAU: f32 = 0.02;
const RGAIN_EPS: f32 = 1.0e-3;
const BLOCK: usize = 256;
const BIAS: f32 = 0.7;
const AGE: f32 = 0.5;
const DRIVE: f32 = 4.0; // +12 dB

fn sweep(base: QualityConfig, from: f32, to: f32, label: &str) -> f32 {
    let mut ch = ChannelDsp::new();
    ch.wdf_rebuild_with(base, SR, from, BIAS);
    ch.wdf_set_age(AGE);
    ch.wdf_warmup();

    let mut rgain_smooth = from;
    let mut last_rgain = from;
    let mut target = norm_apply_gain(1.0 / ch.wdf_measure_gain(), DRIVE);
    let mut norm_trim = target;

    let f = 220.0f32;
    let w = 2.0 * std::f32::consts::PI * f / SR;
    let amp = 0.5f32;
    let mut phase = 0usize;
    let mut worst = f32::NEG_INFINITY;
    let mut worst_peak = 0.0f32;

    for b in 0..120 {
        let rgain_t = if b < 20 { from } else if b < 60 {
            let frac = (b - 20) as f32 / 40.0;
            from + frac * (to - from)
        } else { to };

        let n = BLOCK as f32;
        let a = 1.0 - (-(n / SR) / PARAM_TAU).exp();
        rgain_smooth += a * (rgain_t - rgain_smooth);
        if (rgain_t - rgain_smooth).abs() < RGAIN_EPS * rgain_t.abs().max(1.0) {
            rgain_smooth = rgain_t;
        }
        let mut changed = false;
        if rgain_smooth != last_rgain {
            ch.wdf_set_rgain(rgain_smooth);
            last_rgain = rgain_smooth;
            changed = true;
        }
        if changed {
            target = norm_apply_gain(1.0 / ch.wdf_measure_gain(), DRIVE);
        }

        let g_start = norm_trim;
        let g_step = (target - g_start) / BLOCK as f32;
        let (mut in_sq, mut out_sq, mut peak) = (0.0f64, 0.0f64, 0.0f32);
        for k in 0..BLOCK {
            let x = amp * (w * phase as f32).sin();
            phase += 1;
            let g = g_start + g_step * (k as f32 + 1.0);
            let y = ch.process_sample(x, DRIVE, BIAS, 1.0, g, 1.0, false, InferenceBackend::Wdf);
            in_sq += (x * x) as f64;
            out_sq += (y * y) as f64;
            peak = peak.max(y.abs());
        }
        norm_trim = target;
        let db = 10.0 * (out_sq / in_sq).log10();
        worst = worst.max(db as f32);
        worst_peak = worst_peak.max(peak / amp);
    }
    eprintln!("{label:>22}: worst out/in={worst:+.2} dB  worst peak|y|/in={worst_peak:.2}x");
    worst
}

#[test]
fn cgain_sweep_under_drive_stays_leveled() {
    for (name, base) in [("realtime", QualityConfig::realtime()), ("warp", QualityConfig::warp())] {
        let d = sweep(base, 10_000.0, 47.0, &format!("{name} down"));
        let u = sweep(base, 47.0, 10_000.0, &format!("{name} up"));
        assert!(d < 2.0, "{name} down-sweep peak {d:+.2} dB under drive");
        assert!(u < 2.0, "{name} up-sweep peak {u:+.2} dB under drive");
    }
}
