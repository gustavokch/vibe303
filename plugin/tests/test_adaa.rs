//! ADAA2 saturation: spectral alias-rejection + real-time hygiene.
//!
//! A pure sine through a memoryless soft-clip generates harmonics; the ones above Nyquist fold back
//! to inharmonic "alias" frequencies. `SatTables::f0` is the raw (no-AA) shaper and `Adaa2Sat` is the
//! antialiased one, sharing the same tables — a fair A/B. We drive a high-frequency sine hard and
//! measure energy at folded-alias bins that sit well away from the fundamental and its true
//! sub-Nyquist harmonics. ADAA2 must crush those aliases relative to the raw shaper.

use ba283::adaa::{Adaa2Sat, SatTables};
use ba283::dsp::ChannelDsp;
use ba283::InferenceBackend;

const TWO_PI: f64 = std::f64::consts::TAU;

/// Single-bin Hann-windowed DFT magnitude at normalised frequency `f` (cycles/sample, 0..0.5).
fn bin_mag(x: &[f32], f: f64) -> f64 {
    let n = x.len();
    let (mut re, mut im, mut wsum) = (0.0f64, 0.0f64, 0.0f64);
    for (i, &s) in x.iter().enumerate() {
        let w = 0.5 - 0.5 * (TWO_PI * i as f64 / n as f64).cos(); // Hann
        let ph = TWO_PI * f * i as f64;
        re += w * s as f64 * ph.cos();
        im += w * s as f64 * ph.sin();
        wsum += w;
    }
    2.0 * (re * re + im * im).sqrt() / wsum
}

/// Total aliased energy (sum of magnitudes at the folded bins) for a hard-driven sine, after the
/// state has settled.
fn alias_energy(mut shape: impl FnMut(f32) -> f32) -> f64 {
    // f0 = 0.227 → true harmonics at 0.227, 0.454 (both < 0.5); 3rd..6th fold to inharmonic aliases
    // at 0.319, 0.092, 0.135, 0.362. Measure the two farthest from the fundamental: 0.092, 0.135.
    let f0 = 0.227f64;
    let amp = 3.0f32; // well into saturation
    let n = 1 << 15;
    let mut out = vec![0.0f32; n];
    for (i, o) in out.iter_mut().enumerate() {
        let x = amp * (TWO_PI * f0 * i as f64).sin() as f32;
        *o = shape(x);
    }
    bin_mag(&out, 0.092) + bin_mag(&out, 0.135)
}

#[test]
fn adaa2_suppresses_aliasing_vs_raw_shaper() {
    let t = SatTables::build();
    let raw = alias_energy(|x| t.f0(x));
    let mut sat = Adaa2Sat::new(t.clone());
    let adaa = alias_energy(move |x| sat.process_sample(x));

    let reduction_db = 20.0 * (raw / adaa).log10();
    eprintln!("raw alias={raw:.6}, adaa2 alias={adaa:.6}, reduction={reduction_db:.1} dB");
    // ADAA2's triangular kernel must give a large alias reduction. Raw shaper aliases must be clearly
    // present (sanity on the metric), ADAA2's clearly lower.
    assert!(raw > 1e-3, "raw shaper should alias audibly (got {raw})");
    assert!(
        reduction_db > 18.0,
        "ADAA2 alias reduction {reduction_db:.1} dB below 18 dB target (raw={raw}, adaa={adaa})"
    );
}

#[test]
fn channel_sat_toggle_is_wired_and_off_is_bypassed() {
    // Drive a hot signal through the WDF backend with the saturation stage OFF and ON. OFF must be
    // bypassed (identical to a parallel OFF channel — proves no state leak); ON must change the
    // output (proves the stage is actually wired into the chain) while staying finite.
    let setup = || {
        let mut ch = ChannelDsp::new();
        ch.wdf_warmup();
        ch
    };
    let (mut off_a, mut off_b, mut on) = (setup(), setup(), setup());
    let mut any_diff = false;
    for i in 0..512 {
        let x = (i as f32 * 0.06).sin() * 0.8;
        let ya = off_a.process_sample(x, 4.0, 0.5, 1.0, 1.0, 1.0, false, InferenceBackend::Wdf, false, 0.0);
        let yb = off_b.process_sample(x, 4.0, 0.5, 1.0, 1.0, 1.0, false, InferenceBackend::Wdf, false, 0.0);
        let yon = on.process_sample(x, 4.0, 0.5, 1.0, 1.0, 1.0, false, InferenceBackend::Wdf, true, 0.8);
        assert_eq!(ya, yb, "OFF path not deterministic / state leaked at {i}");
        assert!(yon.is_finite() && yon.abs() < 100.0, "ON output bad at {i}: {yon}");
        if (yon - ya).abs() > 1e-6 {
            any_diff = true;
        }
    }
    assert!(any_diff, "saturation ON did not change the output — stage not wired");
}

#[test]
fn no_heap_alloc_in_process() {
    use assert_no_alloc::*;
    let mut sat = Adaa2Sat::new(SatTables::build());
    for i in 0..64 {
        sat.process_sample((i as f32 * 0.1).sin());
    }
    reset_violation_count();
    assert_no_alloc(|| {
        let mut acc = 0.0f32;
        for i in 0..2048 {
            acc += sat.process_sample(((i as f32) * 0.013).sin() * 3.0);
        }
        std::hint::black_box(acc);
    });
    assert_eq!(violation_count(), 0, "Adaa2Sat allocated inside process_sample");
}
