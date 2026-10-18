//! Every tier must be padded to the same total latency, so the host PDC is
//! constant and a tier crossfade blends time-aligned paths.
//!
//! High and Best share the circuit VCO and nodal ladder and differ only in the
//! oversampling factor (2× vs 4×), so their outputs are near-identical apart
//! from the oversampler group delay. The cross-correlation peak between them
//! must therefore sit at lag 0. A High path padded for Best's 4× latency
//! instead of its own 2× latency shows up as a 16-sample lead at 48 kHz.

use plugin303::dsp::filter::Tier;
use plugin303::dsp::voice::Voice;
use plugin303::params::VcaEnvMode;

fn render(tier: Tier, n: usize) -> Vec<f32> {
    let mut v = Voice::new(48_000.0);
    v.set_tier_immediate(tier);
    v.set_vca_env_mode(VcaEnvMode::Gate);
    v.set_cutoff(1200.0);
    v.set_reso(0.3);
    v.set_gate(true, true, false, false);
    v.note_on(45);
    (0..n).map(|_| v.tick()).collect()
}

/// Lag of `b` relative to `a` at the cross-correlation peak, in samples.
/// Positive means `b` arrives later than `a`.
fn peak_lag(a: &[f32], b: &[f32], max_lag: isize) -> isize {
    let n = a.len() as isize;
    (-max_lag..=max_lag)
        .max_by(|&x, &y| {
            let c = |lag: isize| -> f64 {
                (0..n)
                    .filter(|&i| (0..n).contains(&(i + lag)))
                    .map(|i| a[i as usize] as f64 * b[(i + lag) as usize] as f64)
                    .sum()
            };
            c(x).total_cmp(&c(y))
        })
        .unwrap()
}

#[test]
fn high_is_time_aligned_with_best() {
    let n = 3_000;
    let best = render(Tier::Best, n);
    let high = render(Tier::High, n);
    let lag = peak_lag(&best, &high, 40);
    assert_eq!(lag, 0, "High output is offset by {lag} samples relative to Best");
}
