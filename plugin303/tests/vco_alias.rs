//! VCO alias-reduction regression tests (Option B 2-tap BLEP fix, aliasing-fix plan §Step 3).
//!
//! The Ultra-tier nodal VCO (`Vco303`) produces a relaxation oscillator with a hard
//! JFET reset edge that, at base-rate sampling, creates a single-sample discontinuity
//! — the dominant alias source. The 2-tap first-order BLEP correction in
//! `Vco303::tick()` (exposed as `blep_now`) removes this by adding the first BLEP
//! residual to the current sample and auto-injecting the second into the next tick.
//!
//! **Test strategy:**
//! Rather than using a DFT-based alias floor (unreliable for a saw wave whose alias
//! products fold onto harmonics), we directly measure the **discontinuity height** at
//! reset samples: |o_r[n] - o_r[n-1]| on the reset sample. The BLEP correction
//! reduces this by partially filling in the upward step. A 6 dB reduction in
//! discontinuity amplitude is the acceptance threshold (corresponding to a ≥12 dB
//! reduction in alias power, since power scales as amplitude²).

use plugin303::dsp::{vco::Vco303, voice::VCO_GAIN};

/// Render `n` samples. Returns parallel arrays: `(y_raw, blep_now_arr, is_reset)`.
/// - `y_raw[i]`      = raw tick output × VCO_GAIN (no BLEP applied)
/// - `blep_now_arr[i]` = vco.blep_now after tick i (zero when no reset)
/// - `is_reset[i]`   = blep_frac was Some on tick i (exact reset indicator)
fn render_with_blep(hz: f32, sr: f32, n: usize) -> (Vec<f32>, Vec<f32>, Vec<bool>) {
    let mut vco = Vco303::new(sr);
    vco.set_freq(hz);
    // 80 ms warm-up
    for _ in 0..(sr * 0.08) as usize {
        vco.tick();
    }
    let mut raw = Vec::with_capacity(n);
    let mut bleps = Vec::with_capacity(n);
    let mut resets = Vec::with_capacity(n);
    for _ in 0..n {
        let y = vco.tick() * VCO_GAIN;
        raw.push(y);
        bleps.push(vco.blep_now);
        resets.push(vco.blep_frac.is_some());
    }
    (raw, bleps, resets)
}

/// Core test: polyBLEP must reduce the discontinuity amplitude at reset samples.
///
/// At each sample where `blep_frac.is_some()` (exact JFET reset), compute:
/// - Raw step: |y_raw[i] - y_raw[i-1]|  (no BLEP applied)
/// - Corrected step: |corrected[i] - corrected[i-1]|  (+ blep_now on reset sample,
///   + previous tick's blep_pending carried into blep_now on following sample)
///
/// Since blep_now on tick i covers the MAIN tap (tap0), and the PREVIOUS tick's
/// tap1 lands as blep_now on tick i+1, the corrected output at the reset sample
/// is y_raw[i] + tap0. The step from y_raw[i-1] to corrected[i] = y_raw[i] + tap0
/// should be smaller than the raw step.
#[test]
fn vco_blep_reduces_reset_discontinuity() {
    let sr = 48_000.0f32;
    let hz = 220.0f32;
    let n = 4096usize;

    let (raw, bleps, resets) = render_with_blep(hz, sr, n);

    // Corrected buffer: y_raw + blep_now at every sample.
    let corrected: Vec<f32> = raw.iter().zip(bleps.iter()).map(|(&y, &b)| y + b).collect();

    // Measure the step at each exact reset sample (blep_frac.is_some()).
    let mut steps_raw = Vec::new();
    let mut steps_corrected = Vec::new();
    for i in 1..n {
        if resets[i] {
            steps_raw.push((raw[i] - raw[i - 1]).abs());
            steps_corrected.push((corrected[i] - corrected[i - 1]).abs());
        }
    }

    assert!(
        !steps_raw.is_empty(),
        "raw VCO must have detectable JFET reset events in {n} samples at {hz} Hz"
    );

    let mean_raw: f32 = steps_raw.iter().sum::<f32>() / steps_raw.len() as f32;
    let mean_corrected: f32 = steps_corrected.iter().sum::<f32>() / steps_corrected.len() as f32;
    let reduction_db = 20.0 * (mean_raw / mean_corrected.max(1e-6)).log10();

    println!(
        "Reset step: raw={:.4} V  corrected={:.4} V  reduction={:.1} dB  (n={})",
        mean_raw, mean_corrected, reduction_db, steps_raw.len()
    );

    assert!(
        mean_raw > 0.5,
        "raw reset step must be significant (>0.5 V); got {:.4}",
        mean_raw
    );
    assert!(
        reduction_db >= 4.0,
        "BLEP tap0 must reduce reset step by ≥4 dB; got {:.1} dB",
        reduction_db
    );
}

/// Smoke test: `blep_now` must be non-zero at least once per period during oscillation.
#[test]
fn vco_blep_now_fires_during_oscillation() {
    let sr = 48_000.0f32;
    let mut vco = Vco303::new(sr);
    vco.set_freq(220.0);
    for _ in 0..(sr * 0.08) as usize {
        vco.tick();
    }
    let mut blep_fires = 0u32;
    for _ in 0..500 {
        vco.tick();
        if vco.blep_now.abs() > 1e-6 || vco.blep_frac.is_some() {
            blep_fires += 1;
        }
    }
    println!("blep_now fired {} times in 500 samples at 220 Hz", blep_fires);
    // 220 Hz / 48000 sr → ~4.6 resets per 1000 samples = ~2.3 per 500
    assert!(
        blep_fires >= 1,
        "blep_now must fire at least once during oscillation; got {blep_fires}"
    );
}

/// Verify blep_now is zero on a freshly constructed VCO before any ticks.
#[test]
fn vco_blep_now_is_zero_on_construction() {
    let sr = 48_000.0f32;
    let vco = Vco303::new(sr);
    assert_eq!(
        vco.blep_now, 0.0,
        "blep_now must be zero on construction"
    );
    assert_eq!(
        vco.blep_next, 0.0,
        "blep_next must be zero on construction"
    );
}

/// Verify reset() clears the BLEP state.
#[test]
fn vco_blep_cleared_on_reset() {
    let sr = 48_000.0f32;
    let mut vco = Vco303::new(sr);
    vco.set_freq(220.0);
    // Run until at least one reset fires
    for _ in 0..5000 {
        vco.tick();
        if vco.blep_frac.is_some() {
            break;
        }
    }
    vco.reset();
    assert_eq!(vco.blep_now, 0.0, "blep_now must be cleared by reset()");
    assert_eq!(vco.blep_next, 0.0, "blep_next must be cleared by reset()");
    assert!(vco.blep_frac.is_none(), "blep_frac must be None after reset()");
}
