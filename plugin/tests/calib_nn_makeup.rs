//! Regenerates and validates the `NN_GAIN_CALIB` table baked into `lib.rs`.
//!
//! The NeuralNet (WaveNet) backend tracks circuit-gain via FiLM but its absolute loudness does NOT
//! match the WDF backend: it runs ~6 dB hot and its conditioning axis saturates at 82 Ω. To make the
//! two backends swap at equal loudness, the plugin multiplies the NN wet by a static, gain-indexed
//! makeup curve (`nn_makeup_gain`) and seeds the NN level-norm trim from `1/G_nn` (`nn_norm_trim`).
//!
//! Those constants were a magic table with no committed source. This test IS that source: it measures
//!   - `G_v(R)`  — the WDF small-signal voltage gain, via the SAME 1 kHz / 0.05 V probe the live
//!     levelNorm path uses (`WdfModel::measure_gain`), and
//!   - `G_nn(R)` — the WaveNet output/input RMS at the same probe, at `gain = rgain_to_gain_norm(R)`,
//!     INCLUDING the `output_ref_v / INPUT_FS_VOLTS` (=128×) de-norm the dsp chain applies to the raw
//!     `WaveNet::process` output (`dsp.rs::process_sample`) — without it the units are off by 128×,
//! then forms `makeup = G_v / G_nn` and `gnn_inv = 1 / G_nn` per gain step. It prints a paste-ready
//! table and asserts the committed constants reproduce within tolerance, so the table can never
//! silently drift from the embedded model again.

use ba283::wavenet::WaveNet;
use ba283::wdf::WdfModel;
use ba283::{nn_makeup_gain, nn_norm_trim};

const HOST_SR: f64 = 48_000.0;
const BIAS: f64 = 0.5; // param default
const PROBE_F: f64 = 1000.0;
const PROBE_AMP: f64 = 0.05; // matches WdfModel::measure_gain small-signal level
const INPUT_FS_VOLTS: f64 = 0.5; // dsp.rs: 0 dBFS ≡ 0.5 V

/// `circuit_gain` R_KJ values the calibration is anchored on (Ω), spanning the param range.
const RGAINS: &[f64] = &[47.0, 82.0, 110.0, 220.0, 470.0, 906.0, 1200.0, 10_000.0];

/// Re-derive `gain_norm` exactly as `lib.rs::rgain_to_gain_norm` (private there).
fn rgain_to_gain_norm(rgain_ohm: f64) -> f32 {
    const R_LO: f64 = 10_000.0;
    const R_HI: f64 = 82.0;
    let g = (rgain_ohm.max(1.0).ln() - R_LO.ln()) / (R_HI.ln() - R_LO.ln());
    g.clamp(0.0, 1.0) as f32
}

/// WDF small-signal voltage gain at `rgain`, mirroring the live levelNorm probe.
fn measure_gv(rgain: f64) -> f64 {
    let mut m = WdfModel::new(HOST_SR, rgain, BIAS);
    m.warmup();
    m.measure_gain() as f64
}

/// WaveNet output/input RMS at the same 1 kHz / 0.05 V small-signal probe and a given `gain_norm`,
/// scaled by the dsp chain's 128× de-norm so it is directly comparable to `G_v`. The engine
/// self-warms from zero rings, so a receptive-field + settle pass precedes the measured window.
fn measure_gnn(gain_norm: f32) -> f64 {
    let mut net = WaveNet::new_from_embedded();
    net.reset();
    let deref = net.output_ref_v() as f64 / INPUT_FS_VOLTS; // = 128 at the 64 V ref
    let cycle = (HOST_SR / PROBE_F).max(1.0);
    let n_settle = net.receptive_field() + (cycle * 8.0) as usize;
    let n_meas = (cycle * 8.0) as usize;
    let w = 2.0 * std::f64::consts::PI * PROBE_F / HOST_SR;
    let (mut in_sq, mut out_sq) = (0.0f64, 0.0f64);
    for i in 0..(n_settle + n_meas) {
        let x = PROBE_AMP * (w * i as f64).sin();
        let y = net.process(x as f32, gain_norm) as f64 * deref;
        if i >= n_settle {
            in_sq += x * x;
            out_sq += y * y;
        }
    }
    if in_sq <= 0.0 || !out_sq.is_finite() {
        return 1.0;
    }
    (out_sq / in_sq).sqrt()
}

#[test]
fn regenerate_and_validate_nn_gain_calib() {
    println!("\n   R(Ω)   gain_norm      G_v      G_nn    makeup   gnn_inv   want_mk   want_gi");
    let (mut worst_mk, mut worst_gi) = (0.0f64, 0.0f64);
    for &r in RGAINS {
        let gn = rgain_to_gain_norm(r);
        let gv = measure_gv(r);
        let gnn = measure_gnn(gn);
        let makeup = gv / gnn;
        let gnn_inv = 1.0 / gnn;

        // The committed lookup is what the plugin actually applies — assert it reproduces the freshly
        // measured ratio. Tolerance is loose enough to absorb probe-window / solver-tier / probe-level
        // differences but tight enough to catch a real drift (wrong embedded model, broken FiLM, a
        // missing/extra units scale): a 128× units error like the one this test guards against is a
        // 12700% miss, nowhere near the band below.
        let want_makeup = nn_makeup_gain(r as f32) as f64;
        let want_gnn_inv = nn_norm_trim(r as f32) as f64;
        let mk_rel = (want_makeup - makeup).abs() / makeup.max(1e-9);
        let gi_rel = (want_gnn_inv - gnn_inv).abs() / gnn_inv.max(1e-9);
        worst_mk = worst_mk.max(mk_rel);
        worst_gi = worst_gi.max(gi_rel);
        println!(
            "{r:8.0}   {gn:7.4}   {gv:7.4}   {gnn:7.4}   {makeup:7.4}   {gnn_inv:8.6}   {want_makeup:7.4}   {want_gnn_inv:8.6}"
        );
        assert!(
            mk_rel < 0.20,
            "makeup at {r} Ω: committed {want_makeup:.4} vs measured {makeup:.4} (rel {mk_rel:.3})"
        );
        assert!(
            gi_rel < 0.20,
            "gnn_inv at {r} Ω: committed {want_gnn_inv:.6} vs measured {gnn_inv:.6} (rel {gi_rel:.3})"
        );
    }
    println!("worst-case rel error: makeup {worst_mk:.3}, gnn_inv {worst_gi:.3}");
}

/// Pure lookup-behavior guards on the public API — deterministic, no model build.
#[test]
fn lookup_endpoints_clamp_and_monotonic() {
    // Flat-hold past either anchor end (47 Ω lowest, 10 kΩ highest in the table). Compare two points
    // that are both unambiguously outside the range — using the boundary value itself is brittle, as
    // `ln(47.0_f32)` rounds just above the stored `3.8501` anchor and takes the interpolation path.
    assert_eq!(nn_makeup_gain(10.0), nn_makeup_gain(30.0), "below-range makeup not flat-held");
    assert_eq!(nn_makeup_gain(20_000.0), nn_makeup_gain(50_000.0), "above-range makeup not flat-held");
    assert_eq!(nn_norm_trim(10.0), nn_norm_trim(30.0), "below-range gnn_inv not flat-held");
    assert_eq!(nn_norm_trim(20_000.0), nn_norm_trim(50_000.0), "above-range gnn_inv not flat-held");

    // gnn_inv (= 1/G_nn) rises monotonically with R: less circuit gain → smaller G_nn → larger trim.
    let rs = [47.0, 110.0, 220.0, 470.0, 906.0, 1200.0, 10_000.0];
    for w in rs.windows(2) {
        assert!(
            nn_norm_trim(w[1]) >= nn_norm_trim(w[0]),
            "gnn_inv not monotonic from {} to {} Ω",
            w[0],
            w[1]
        );
    }

    // Everything stays finite, positive, and the makeup respects the shared trim clamp.
    for &r in &[1.0, 47.0, 220.0, 1200.0, 10_000.0, 1.0e6] {
        let m = nn_makeup_gain(r);
        let t = nn_norm_trim(r);
        assert!(m.is_finite() && m > 0.0, "makeup non-finite/≤0 at {r}");
        assert!(t.is_finite() && t > 0.0, "gnn_inv non-finite/≤0 at {r}");
        assert!((1.0e-4..=8.0).contains(&m), "makeup {m} at {r} outside the trim clamp band");
    }
}
