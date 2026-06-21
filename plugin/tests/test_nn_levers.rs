//! Regression: the NeuralNet backend's circuit-gain + level-norm levers, calibrated to match the
//! WDF backend (tests/calib_nn_makeup.rs derives the table in lib.rs::NN_GAIN_CALIB).
//!   • level-norm OFF: `nn_makeup_gain` brings the NN absolute loudness onto the WDF circuit's at the
//!     same gain (within ~1.5 dB), so A/B-ing backends is loudness-matched.
//!   • level-norm ON: `norm_apply_gain(nn_norm_trim(R), drive)` levels the NN output to ~constant
//!     across the gain sweep AND across Drive (no off-thread probe needed — static curve).
//! Drives the real `ChannelDsp::process_sample` NN path, so a weight/table drift trips this.
use ba283::dsp::ChannelDsp;
use ba283::wdf::QualityConfig;
use ba283::{nn_makeup_gain, nn_norm_trim, norm_apply_gain, rgain_to_gain_norm, InferenceBackend};

const SR: f32 = 48_000.0;
const BIAS: f32 = 0.7;
const WARM: usize = 8_000; // > WaveNet RF (6347)
const MEAS: usize = 6_000;
const F: f32 = 220.0;
const AMP: f32 = 0.1; // linear region

// FiLM cond = the plugin's own gain→0..1 map, so the test drives the NN on the exact curve
// `NN_GAIN_CALIB` was measured against (no local replica that could drift).
fn cond_of(r: f32) -> f32 {
    rgain_to_gain_norm(r)
}

/// RMS of the NN path at rgain `r`, Drive `drive`, applied `norm_gain`.
fn nn_rms(r: f32, drive: f32, norm_gain: f32) -> f32 {
    let mut ch = ChannelDsp::new();
    let cond = cond_of(r);
    let w = 2.0 * std::f32::consts::PI * F / SR;
    let mut acc = 0.0f64;
    for i in 0..(WARM + MEAS) {
        let x = AMP * (w * i as f32).sin();
        let y = ch.process_sample(x, 1.0, drive, cond, 0.5, 1.0, norm_gain, 1.0, false, InferenceBackend::NeuralNet);
        if i >= WARM {
            acc += (y * y) as f64;
        }
    }
    (acc / MEAS as f64).sqrt() as f32
}

/// RMS of the settled WDF path at rgain `r`, Drive `drive` (level-norm off → norm_gain 1).
fn wdf_rms(r: f32, drive: f32) -> f32 {
    let mut ch = ChannelDsp::new();
    ch.wdf_rebuild_with(QualityConfig::realtime(), SR, r, BIAS);
    ch.wdf_warmup();
    let w = 2.0 * std::f32::consts::PI * F / SR;
    let mut acc = 0.0f64;
    for i in 0..(WARM + MEAS) {
        let x = AMP * (w * i as f32).sin();
        let y = ch.process_sample(x, 1.0, drive, BIAS, 0.5, 1.0, 1.0, 1.0, false, InferenceBackend::Wdf);
        if i >= WARM {
            acc += (y * y) as f64;
        }
    }
    (acc / MEAS as f64).sqrt() as f32
}

#[test]
fn nn_makeup_matches_wdf_absolute_loudness() {
    // level-norm OFF: NN wet * makeup(R) should land within ~1.5 dB of the WDF circuit at each gain.
    for &r in &[110.0f32, 220.0, 470.0, 1200.0] {
        let nn = nn_rms(r, 1.0, nn_makeup_gain(r));
        let wdf = wdf_rms(r, 1.0);
        let db = 20.0 * (nn / wdf).log10();
        assert!(
            db.abs() < 1.5,
            "rgain={r}: NN vs WDF loudness {db:+.2} dB (nn={nn:.5} wdf={wdf:.5}) — makeup off"
        );
    }
}

#[test]
fn nn_level_norm_holds_loudness_across_gain_and_drive() {
    // level-norm ON: leveled RMS must be ~constant across the gain sweep and across Drive.
    let mut levels = vec![];
    for &r in &[110.0f32, 220.0, 470.0, 1200.0] {
        for &drive in &[1.0f32, 4.0] {
            let ng = norm_apply_gain(nn_norm_trim(r), drive);
            levels.push((r, drive, nn_rms(r, drive, ng)));
        }
    }
    let (min, max) = levels.iter().fold((f32::MAX, 0.0f32), |(mn, mx), &(_, _, v)| (mn.min(v), mx.max(v)));
    let spread_db = 20.0 * (max / min).log10();
    assert!(
        spread_db < 3.0,
        "level-norm ON spread {spread_db:.2} dB across gain×drive: {levels:?}"
    );
}

/// SAFETY: the NN input/output clamps (`NN_INPUT_CLAMP`/`NN_OUTPUT_CEIL`) must bound the path even
/// when a pathological model would diverge. The embedded placeholder weights blow up above ~+12 dB
/// Drive (`bin/measure_nn_gi`: `1/G_nn_sat → 0`); without the clamps a 0 dBFS input at +24 dB Drive
/// drove the NN output to ~1e5 (speaker-damaging). Assert every sample stays finite and within the
/// output ceiling (+ a small margin for the linear DPR all-pass downstream of the clamp), at a hot
/// input across the worst gains, levelNorm OFF (raw NN, norm_gain = 1 — no leveling to mask it).
#[test]
fn nn_output_is_bounded_under_extreme_drive() {
    use ba283::dsp::NN_OUTPUT_CEIL;
    const DRIVE_MAX: f32 = 15.848_932; // +24 dB, the param max
    let w = 2.0 * std::f32::consts::PI * F / SR;
    for &r in &[110.0f32, 220.0, 1200.0] {
        let mut ch = ChannelDsp::new();
        let cond = cond_of(r);
        let mut worst = 0.0f32;
        for i in 0..(WARM + MEAS) {
            let x = 1.0 * (w * i as f32).sin(); // 0 dBFS peak — far into the divergent regime at +24 dB
            // norm_gain = 1.0 (levelNorm OFF): nothing downstream attenuates a blow-up.
            let y = ch.process_sample(x, 1.0, DRIVE_MAX, cond, 0.5, 1.0, 1.0, 1.0, false, InferenceBackend::NeuralNet);
            assert!(y.is_finite(), "NN output non-finite at r={r} i={i}: {y}");
            worst = worst.max(y.abs());
        }
        assert!(
            worst <= NN_OUTPUT_CEIL * 1.5,
            "NN output {worst} exceeded the clamp ceiling {NN_OUTPUT_CEIL} (×1.5 DPR margin) at r={r}"
        );
    }
}
