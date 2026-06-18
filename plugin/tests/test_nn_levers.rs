//! Regression: the NeuralNet backend's circuit-gain + level-norm levers, calibrated to match the
//! WDF backend (tests/calib_nn_makeup.rs derives the table in lib.rs::NN_GAIN_CALIB).
//!   • level-norm OFF: `nn_makeup_gain` brings the NN absolute loudness onto the WDF circuit's at the
//!     same gain (within ~1.5 dB), so A/B-ing backends is loudness-matched.
//!   • level-norm ON: `norm_apply_gain(nn_norm_trim(R), drive)` levels the NN output to ~constant
//!     across the gain sweep AND across Drive (no off-thread probe needed — static curve).
//! Drives the real `ChannelDsp::process_sample` NN path, so a weight/table drift trips this.
use ba283::dsp::ChannelDsp;
use ba283::wdf::QualityConfig;
use ba283::{nn_makeup_gain, nn_norm_trim, norm_apply_gain, InferenceBackend};

const SR: f32 = 48_000.0;
const BIAS: f32 = 0.7;
const WARM: usize = 8_000; // > WaveNet RF (6347)
const MEAS: usize = 6_000;
const F: f32 = 220.0;
const AMP: f32 = 0.1; // linear region

fn cond_of(r: f32) -> f32 {
    let (lo, hi) = (10_000.0f32.ln(), 82.0f32.ln());
    ((r.max(1.0).ln() - lo) / (hi - lo)).clamp(0.0, 1.0)
}

/// RMS of the NN path at rgain `r`, Drive `drive`, applied `norm_gain`.
fn nn_rms(r: f32, drive: f32, norm_gain: f32) -> f32 {
    let mut ch = ChannelDsp::new();
    let cond = cond_of(r);
    let w = 2.0 * std::f32::consts::PI * F / SR;
    let mut acc = 0.0f64;
    for i in 0..(WARM + MEAS) {
        let x = AMP * (w * i as f32).sin();
        let y = ch.process_sample(x, drive, cond, 1.0, norm_gain, 1.0, false, InferenceBackend::NeuralNet);
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
        let y = ch.process_sample(x, drive, BIAS, 1.0, 1.0, 1.0, false, InferenceBackend::Wdf);
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
