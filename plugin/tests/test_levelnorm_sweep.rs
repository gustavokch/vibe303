//! Regression for the levelNorm sweep peak (Issue 3 follow-up): a Circuit-Gain sweep with level
//! norm ON must stay leveled, because the circuit's voltage gain G_v swings ~13x across the range
//! (47 Ohm -> 70.8x, 10000 Ohm -> 5.4x). The original static trim was FROZEN across the sweep, so a
//! down-sweep peaked at +23 dB. This mirrors lib.rs::process's track-and-slew normalizer (re-probe
//! the trim target on every changed block, slew the applied trim toward it) and asserts no peak.
use ba283::dsp::ChannelDsp;
use ba283::wdf::QualityConfig;
use ba283::{norm_apply_gain, InferenceBackend};

const SR: f32 = 48_000.0;
const PARAM_TAU: f32 = 0.02; // WDF_PARAM_TAU_S
const RGAIN_EPS: f32 = 1.0e-3; // relative
const BLOCK: usize = 256;

/// Sweep Circuit Gain from `from` to `to` (Ohm) over 40 blocks under a continuous tone with level
/// norm tracking ON. Returns the worst out/in ratio (dB) over the whole run. Mirrors lib.rs.
fn sweep_worst_db(from: f32, to: f32) -> f32 {
    let mut ch = ChannelDsp::new();
    ch.wdf_rebuild_with(QualityConfig::realtime(), SR, from, 0.5);
    ch.wdf_warmup();

    let mut rgain_smooth = from;
    let mut last_rgain = from;
    let mut norm_trim_target = norm_apply_gain(1.0 / ch.wdf_measure_gain(), 1.0);
    let mut norm_trim = norm_trim_target; // applied gain at block start (forced-probe snap on init)

    let f = 220.0f32;
    let w = 2.0 * std::f32::consts::PI * f / SR;
    let amp = 0.3f32;
    let mut phase = 0usize;
    let mut worst = f32::NEG_INFINITY;

    for b in 0..120 {
        let rgain_t = if b < 20 {
            from
        } else if b < 60 {
            let frac = (b - 20) as f32 / 40.0;
            from + frac * (to - from)
        } else {
            to
        };

        // block-rate glide + relative snap (mirror lib.rs)
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
        // re-probe the target on a changed block (mirror lib.rs)
        if changed {
            norm_trim_target = norm_apply_gain(1.0 / ch.wdf_measure_gain(), 1.0);
        }

        // audio: ramp the applied gain per-sample from block-start `norm_trim` to `norm_trim_target`
        // (no cross-block lag, exact per-block cancellation), then carry the end value forward.
        let g_start = norm_trim;
        let g_step = (norm_trim_target - g_start) / BLOCK as f32;
        let (mut in_sq, mut out_sq) = (0.0f64, 0.0f64);
        for k in 0..BLOCK {
            let x = amp * (w * phase as f32).sin();
            phase += 1;
            let g = g_start + g_step * (k as f32 + 1.0);
            let y = ch.process_sample(x, 1.0, 0.5, 1.0, g, 1.0, false, InferenceBackend::Wdf);
            in_sq += (x * x) as f64;
            out_sq += (y * y) as f64;
        }
        norm_trim = norm_trim_target;
        let db = 10.0 * (out_sq / in_sq).log10();
        worst = worst.max(db as f32);
    }
    worst
}

#[test]
fn sweep_down_circuit_gain_stays_leveled() {
    let worst = sweep_worst_db(10_000.0, 47.0); // G_v 5.4 -> 70.8 (the +23 dB case)
    eprintln!("down-sweep worst out/in = {worst:+.2} dB");
    assert!(worst < 1.5, "down-sweep peak {worst:+.2} dB (frozen-trim regression)");
}

#[test]
fn sweep_up_circuit_gain_stays_leveled() {
    let worst = sweep_worst_db(47.0, 10_000.0); // G_v 70.8 -> 5.4
    eprintln!("up-sweep worst out/in = {worst:+.2} dB");
    assert!(worst < 1.5, "up-sweep peak {worst:+.2} dB");
}
