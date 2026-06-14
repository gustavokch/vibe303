//! Static levelNorm (Issue 3): the plugin replaces the continuous RMS estimator with a one-shot
//! probe of the settled WDF circuit's voltage gain (`WdfModel::measure_gain`). The plugin caches
//! `norm_trim = 1 / gain` and applies it as a constant — no per-sample pumping. These tests pin the
//! probe's properties: deterministic, finite, plausible, responsive to the operating point, and that
//! the trim actually levels the signal it was measured on.

use ba283::wdf::{self, QualityConfig};

const HOST_SR: f64 = 48_000.0;

fn warmed(rgain: f64, bias: f64) -> wdf::WdfModel {
    let mut m = wdf::WdfModel::new_with(QualityConfig::realtime(), HOST_SR, rgain, bias);
    m.warmup();
    m
}

/// The probe must be a pure function of the settled model: identical on repeated calls (it runs on a
/// scratch clone and never mutates the live model) — i.e. a constant trim, not a slewing estimator.
#[test]
fn probe_is_deterministic_and_pure() {
    let m = warmed(220.0, 0.5);
    let g1 = m.measure_gain();
    let g2 = m.measure_gain();
    assert_eq!(g1.to_bits(), g2.to_bits(), "probe not deterministic: {g1} vs {g2}");
    // Probing must not disturb the live model — a third call after others is still identical.
    let g3 = m.measure_gain();
    assert_eq!(g1.to_bits(), g3.to_bits(), "probe mutated the live model: {g1} vs {g3}");
}

/// The measured gain is finite and plausible for the BA283 preamp (it amplifies: g > 1), and not
/// absurd. Checked across the gain/bias extremes.
#[test]
fn probe_gain_finite_and_plausible() {
    for &(rgain, bias) in &[(220.0, 0.5), (47.0, 0.0), (10_000.0, 1.0), (47.0, 1.0)] {
        let g = warmed(rgain, bias).measure_gain();
        assert!(g.is_finite() && g > 1.0 && g < 1.0e4, "implausible gain {g} @ (rgain={rgain}, bias={bias})");
    }
}

/// The probe responds to the operating point: changing the circuit-gain knob changes the measured
/// gain (so the trim re-probe on a knob change is meaningful, not a no-op).
#[test]
fn probe_gain_tracks_circuit_gain() {
    let lo = warmed(47.0, 0.5).measure_gain();
    let hi = warmed(10_000.0, 0.5).measure_gain();
    assert!(
        (lo - hi).abs() / lo.max(hi) > 0.05,
        "circuit-gain change barely moved the probe: {lo} vs {hi}"
    );
}

/// Applying the trim `1/g` to the probe stimulus levels its output RMS to match the input RMS — the
/// defining property of the normalizer. Reproduces the probe internally and checks the trimmed
/// output energy equals the input energy within a tight tolerance.
#[test]
fn trim_levels_output_to_input() {
    let m = warmed(220.0, 0.5);
    let g = m.measure_gain();
    let trim = 1.0 / g;

    // Same stimulus the probe uses (1 kHz, small-signal), driven through a fresh clone via warmup.
    let mut probe = warmed(220.0, 0.5);
    let f = 1000.0_f64;
    let amp = 0.05_f64;
    let cycle = (HOST_SR / f) as usize;
    let n = cycle * 16;
    let w = 2.0 * std::f64::consts::PI * f / HOST_SR;
    let (mut in_sq, mut out_sq) = (0.0f64, 0.0f64);
    for i in 0..n {
        let x = amp * (w * i as f64).sin();
        let y = probe.process_host_sample(x as f32) as f64 * trim as f64;
        if i >= n / 2 {
            in_sq += x * x;
            out_sq += y * y;
        }
    }
    let ratio_db = 10.0 * (out_sq / in_sq).log10();
    assert!(ratio_db.abs() < 1.0, "trim did not level the signal: {ratio_db:.2} dB off unity");
}
