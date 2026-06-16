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
fn channel_sat_wired_on_neuralnet_backend() {
    // The sat stage is inserted on BOTH backends; exercise the NeuralNet path too. OFF must be
    // deterministic/bypassed; ON must change the output (stage wired) and stay finite. Driven hard
    // (norm_gain amplifies the model output into the saturator) so ON ≠ OFF even at modest levels.
    let (mut off_a, mut off_b, mut on) = (ChannelDsp::new(), ChannelDsp::new(), ChannelDsp::new());
    let mut any_diff = false;
    for i in 0..512 {
        let x = (i as f32 * 0.06).sin() * 0.9;
        let ya = off_a.process_sample(x, 8.0, 0.5, 1.0, 4.0, 1.0, false, InferenceBackend::NeuralNet, false, 0.0);
        let yb = off_b.process_sample(x, 8.0, 0.5, 1.0, 4.0, 1.0, false, InferenceBackend::NeuralNet, false, 0.0);
        let yon = on.process_sample(x, 8.0, 0.5, 1.0, 4.0, 1.0, false, InferenceBackend::NeuralNet, true, 0.9);
        assert_eq!(ya, yb, "OFF path not deterministic / state leaked at {i}");
        assert!(yon.is_finite() && yon.abs() < 100.0, "ON output bad at {i}: {yon}");
        if (yon - ya).abs() > 1e-6 {
            any_diff = true;
        }
    }
    assert!(any_diff, "saturation ON did not change the NeuralNet output — stage not wired");
}

#[test]
fn channel_sat_output_is_dc_free() {
    // The shape is asymmetric (biased tanh), so a symmetric drive rectifies a DC term onto the wet
    // path. The post-sat DC blocker must remove it: a symmetric sine through sat ON must leave a
    // near-zero output mean. Compare against the raw saturator (no DC block), which has audible DC.
    let mut ch = ChannelDsp::new();
    ch.wdf_warmup();
    // Integer number of periods in the measurement window so the (harmonic) AC content averages to
    // ~0 and `mean` isolates the DC term. 64 samples/period → the settled second half (n/2 samples)
    // holds a whole number of periods of the fundamental AND every integer harmonic.
    let period = 64.0f64;
    let omega = TWO_PI / period;
    let n = 16384; // n/2 = 8192 = 128 periods
    let (mut sum, mut sq) = (0.0f64, 0.0f64);
    for i in 0..n {
        let x = (omega * i as f64).sin() as f32 * 0.7; // symmetric, well above the 5 Hz HP corner
        let y = ch.process_sample(x, 4.0, 0.5, 1.0, 1.0, 1.0, false, InferenceBackend::Wdf, true, 0.9);
        if i >= n / 2 {
            // measure the settled second half
            sum += y as f64;
            sq += (y * y) as f64;
        }
    }
    let half = (n / 2) as f64;
    let mean = (sum / half).abs();
    let rms = (sq / half).sqrt();
    assert!(rms > 1e-3, "signal too quiet to be a meaningful DC check (rms={rms})");
    // DC must be a tiny fraction of the signal RMS — the blocker has removed the rectified offset.
    assert!(mean < 0.02 * rms, "post-sat DC not blocked: mean={mean}, rms={rms}");
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
