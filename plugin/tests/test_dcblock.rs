//! The wet-path DC blocker (Issue 2) is applied UNCONDITIONALLY on the WDF output to kill
//! operating-point DC offsets from G0 re-stamps. It is a 5 Hz first-order high-pass, so it must
//! leave the audio band essentially untouched (the behavioural-change concern from review): these
//! tests pin that it removes a DC offset but passes audio-band tones at ~unity.
use ba283::dcblock::DcBlock;

const SR: f32 = 48_000.0;

/// RMS gain of the DC blocker at frequency `f` (Hz), measured after letting it settle.
fn gain_at(f: f32) -> f32 {
    let mut b = DcBlock::new();
    b.prepare(SR);
    let w = 2.0 * std::f32::consts::PI * f / SR;
    let n = (SR / f) as usize * 8; // several cycles
    let (mut in_sq, mut out_sq) = (0.0f64, 0.0f64);
    for i in 0..n {
        let x = (w * i as f32).sin();
        let y = b.process(x);
        if i >= n / 2 {
            in_sq += (x * x) as f64;
            out_sq += (y * y) as f64;
        }
    }
    (out_sq / in_sq).sqrt() as f32
}

#[test]
fn passband_is_essentially_flat() {
    // Per-frequency ceiling (dB). A 5 Hz first-order high-pass is ≈ −0.26 dB at the very bottom of
    // the audio band (20 Hz) and falls to negligible above it — well below audibility everywhere.
    for &(f, max_db) in &[(20.0f32, 0.30f32), (100.0, 0.02), (1_000.0, 0.01), (10_000.0, 0.01)] {
        let g_db = 20.0 * gain_at(f).log10();
        eprintln!("DC blocker @ {f:>7.0} Hz: {g_db:+.3} dB");
        assert!(g_db.abs() < max_db, "DC blocker alters {f} Hz by {g_db:+.3} dB (max {max_db})");
    }
}

#[test]
fn removes_dc_offset() {
    let mut b = DcBlock::new();
    b.prepare(SR);
    let mut last = 0.0f32;
    for _ in 0..SR as usize {
        last = b.process(1.0); // constant DC input
    }
    // A first-order high-pass drives a steady DC input to ~0.
    assert!(last.abs() < 1e-2, "DC not blocked: steady output {last}");
}
