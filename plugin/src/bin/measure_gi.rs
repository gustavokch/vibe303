//! Offline measurement of the WDF voltage gain at each of the 5 hardware gain-switch positions,
//! across a grid of DRIVE settings, used to embed the static Drive-swept levelNorm circuit-makeup
//! table `1/G_sat(pos, drive)` in `lib.rs`.
//!
//! Run: `cargo run --release --bin measure_gi`
//!
//! WHY drive-swept (not a single small-signal `1/G_v`): the live levelNorm makeup is
//! `trim / drive`. With a constant small-signal `trim = 1/G_v` this assumes the circuit is LINEAR,
//! so it divides out the FULL linear Drive — but a saturating circuit's output grows sublinearly,
//! so the output sags hard (~−27 dBFS at +24 dB Drive) and the DRIVE knob sounds weak. Measuring
//! the LARGE-SIGNAL gain `G_sat` at each Drive (the circuit driven by a `A0 * 10^(drive/20)` sine)
//! and storing `trim = 1/G_sat` makes `trim / drive` level the output to constant loudness while
//! the saturation colour stays audible. (`G_sat → G_v` as Drive → small, but A0 = 0 dBFS already
//! sits in light nonlinearity, so even the Drive-0 column is the large-signal value at A0.)
//!
//! A live large-signal probe is deliberately NOT used — `WdfModel::measure_gain`'s comment
//! documents that probing the saturated operating point on the audio thread jitters 22×→250× and
//! peaks loudly. This is offline + deterministic; the drift-guard test `tests/test_stepped_gain.rs`
//! re-measures the table and asserts it matches the embedded constants.

use ba283::dsp::{ChannelDsp, INPUT_FS_VOLTS};
use ba283::wdf::QualityConfig;

/// Reference operating point for the makeup measurement (must match the embedded-table contract and
/// the drift-guard test): native model SR, default bias, pristine (age 0), default trafo voicing.
const SR: f32 = 48_000.0;
const BIAS: f32 = 0.5;

/// The 5 BA283 wafer R_KJ positions (Ω), idx 0..4 = 15/20/25/30/35 dB. Open contact = large sentinel.
const STEP_R_KJ: [f32; 5] = [1.0e9, 1_200.0, 470.0, 220.0, 110.0];
const LABELS: [&str; 5] = ["15 dB", "20 dB", "25 dB", "30 dB", "35 dB"];

/// DRIVE grid (dB), spanning the `drive_db` param range (−12..+24) every 3 dB. The live makeup
/// lookup linearly interpolates `1/G_sat` over this grid in dB; 3 dB steps are dense enough for the
/// smooth saturation curve. MUST match `DRIVE_GRID_DB` in `lib.rs`.
const DRIVE_GRID_DB: [f32; 13] = [
    -12.0, -9.0, -6.0, -3.0, 0.0, 3.0, 6.0, 9.0, 12.0, 15.0, 18.0, 21.0, 24.0,
];

/// Calibration reference INPUT level (volts). The circuit is probed with a sine of amplitude
/// `A0 * 10^(drive/20)`, i.e. a 0 dBFS-peak signal scaled by Drive (`0 dBFS ≡ INPUT_FS_VOLTS`).
/// Leveling is exact only at this input level; lower-level program saturates less, leaving a
/// bounded residual. Tune by listening if 0 dBFS over-represents typical program saturation.
const A0_VOLTS: f64 = INPUT_FS_VOLTS as f64;

fn db_to_gain(db: f32) -> f64 {
    10.0f64.powf(db as f64 * 0.05)
}

/// Build the measurement config, optionally with the JFET voicing on (`--jfet`, requires the
/// `jfet` feature). The JFET op-point differs from the BJT's, so its makeup row (`JFET_MAKEUP` in
/// `lib.rs`) is measured separately.
fn measure_cfg() -> (QualityConfig, bool) {
    let jfet = std::env::args().any(|a| a == "--jfet");
    #[cfg(feature = "jfet")]
    {
        let mut c = QualityConfig::realtime();
        c.jfet = jfet;
        (c, jfet)
    }
    #[cfg(not(feature = "jfet"))]
    {
        if jfet {
            eprintln!("--jfet requires `--features jfet`; rebuild with it to measure the JFET table");
            std::process::exit(1);
        }
        (QualityConfig::realtime(), false)
    }
}

fn main() {
    let (cfg, jfet) = measure_cfg();
    let mut makeup = [[0f32; DRIVE_GRID_DB.len()]; 5];
    println!(
        "// DRIVE-swept {} makeup `1/G_sat`. Rows = gain positions, cols = DRIVE grid (dB).",
        if jfet { "JFET" } else { "WDF" }
    );
    println!(
        "// A0 = {:.4} V (0 dBFS), SR = {} Hz, bias = {}, age 0, default trafo, realtime tier.",
        A0_VOLTS, SR, BIAS
    );
    print!("// pos \\ drive_dB ");
    for d in DRIVE_GRID_DB {
        print!("{:>9.0}", d);
    }
    println!();
    for (i, &r) in STEP_R_KJ.iter().enumerate() {
        let mut ch = ChannelDsp::new();
        ch.wdf_rebuild_with(cfg, SR, r, BIAS);
        ch.wdf_warmup();
        print!("//  {} {:<6}     ", i, LABELS[i]);
        for (j, &d) in DRIVE_GRID_DB.iter().enumerate() {
            let amp = A0_VOLTS * db_to_gain(d);
            let g_sat = ch.wdf_measure_gain_at(amp);
            makeup[i][j] = 1.0 / g_sat;
            print!("{:>9.6}", makeup[i][j]);
        }
        println!();
    }

    // Emit paste-ready table for `lib.rs`.
    println!("\nconst DRIVE_GRID_DB: &[f32] = &[");
    print!("    ");
    for d in DRIVE_GRID_DB {
        print!("{:.1}, ", d);
    }
    println!("\n];");
    println!(
        "// Rows align with GAIN_STEPS order (15/20/25/30/35 dB); paste into {}.",
        if jfet { "`JFET_MAKEUP`" } else { "each `wdf_makeup`" }
    );
    for (i, row) in makeup.iter().enumerate() {
        print!("// {} ({}): &[", i, LABELS[i]);
        for (j, m) in row.iter().enumerate() {
            print!("{:.6}{}", m, if j + 1 < row.len() { ", " } else { "" });
        }
        println!("],");
    }
}
