//! Offline measurement of the WDF small-signal voltage gain `G_i` at each of the 5 hardware
//! gain-switch positions, used to embed the static levelNorm circuit-makeup table `1/G_i` in
//! `lib.rs` (replacing the old runtime off-thread gain probe — positions are now fixed, so the
//! makeup is a deterministic compile-time constant).
//!
//! Run: `cargo run --release --bin measure_gi`
//!
//! Measures the SAME small-signal gain the live levelNorm path used (`ChannelDsp::wdf_measure_gain`,
//! warmed at the realtime tier, default bias/age/trafo). The drift-guard test
//! `tests/test_stepped_gain.rs` re-measures these and asserts they match the embedded constants, so
//! a silent solver/circuit change can never desync the table.

use ba283::dsp::ChannelDsp;
use ba283::wdf::QualityConfig;

/// Reference operating point for the makeup measurement (must match the embedded-table contract and
/// the drift-guard test): native model SR, default bias, pristine (age 0), default trafo voicing.
const SR: f32 = 48_000.0;
const BIAS: f32 = 0.5;

/// The 5 BA283 wafer R_KJ positions (Ω), idx 0..4 = 15/20/25/30/35 dB. Open contact = large sentinel.
const STEP_R_KJ: [f32; 5] = [1.0e9, 1_200.0, 470.0, 220.0, 110.0];
const LABELS: [&str; 5] = ["15 dB", "20 dB", "25 dB", "30 dB", "35 dB"];

fn main() {
    let mut makeup = [0f32; 5];
    println!("// idx  label   R_KJ(Ω)        G_v        1/G_v");
    for (i, &r) in STEP_R_KJ.iter().enumerate() {
        let mut ch = ChannelDsp::new();
        ch.wdf_rebuild_with(QualityConfig::realtime(), SR, r, BIAS);
        ch.wdf_warmup();
        let g = ch.wdf_measure_gain();
        makeup[i] = 1.0 / g;
        println!("//  {}   {}  {:>12.1}  {:>9.4}  {:.6}", i, LABELS[i], r, g, 1.0 / g);
    }
    println!(
        "static WDF_MAKEUP: [f32; 5] = [{:.6}, {:.6}, {:.6}, {:.6}, {:.6}];",
        makeup[0], makeup[1], makeup[2], makeup[3], makeup[4]
    );
}
