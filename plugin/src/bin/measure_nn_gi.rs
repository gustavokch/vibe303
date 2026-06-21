//! Offline measurement of the embedded WaveNet's DRIVE-swept LARGE-SIGNAL voltage gain
//! `G_nn_sat(pos, drive)` at each of the 5 hardware gain-switch positions, used to embed the
//! Drive-swept levelNorm NN-makeup table `1/G_nn_sat` in `lib.rs` (`GAIN_STEPS[pos].nn_makeup`).
//!
//! Run: `cargo run --release --bin measure_nn_gi`
//!
//! WHY drive-swept (the NN parity fix, mirroring `bin/measure_gi` for the WDF backend): the live NN
//! levelNorm makeup is `nn_norm_trim(R) / drive` with a constant small-signal `1/G_nn`. That assumes
//! the WaveNet is LINEAR, so it divides out the FULL linear Drive — but the gain-conditioned WaveNet
//! models a saturating amp, so its output grows sublinearly under Drive and the makeup over-
//! attenuates (the same sag the WDF path had before its Drive-swept fix). Measuring the LARGE-SIGNAL
//! gain `G_nn_sat` at each Drive (the net driven by an `A0 * 10^(drive/20)` sine) and storing
//! `1/G_nn_sat` makes `trim / drive` level the NN output to constant loudness while the saturation
//! colour stays audible.
//!
//! The WaveNet is a fixed black box (no live probe needed or possible): this is offline +
//! deterministic. The drift-guard test `tests/calib_nn_makeup.rs::nn_drive_makeup_matches_remeasured`
//! re-measures this table and asserts it matches the embedded constants, so a silent weight/units
//! change can never desync it. NOTE the embedded weights are still placeholder — these values are
//! provisional and the drift guard will flag a re-run once real weights land.

use ba283::wavenet::WaveNet;
use ba283::{input_gain_db_to_norm, rgain_to_gain_norm};

const HOST_SR: f64 = 48_000.0;
const PROBE_F: f64 = 1000.0;
const INPUT_FS_VOLTS: f64 = 0.5; // dsp.rs: 0 dBFS ≡ 0.5 V (the NN de-norm convention)

/// Reference INPUT level (the WaveNet's own input convention, NOT volts): at 0 dBFS / unity Drive the
/// dsp chain feeds the net `driven = x` (peak 1.0), since the net applies its own `input_scale`
/// internally (the 0.5 V/dBFS cancels). So the 0 dBFS net-input amplitude is 1.0; Drive scales it by
/// `10^(drive/20)`. Leveling is exact only at this input level (mirrors `measure_gi`'s A0).
const A0_NN: f64 = 1.0;

/// MUST match `DRIVE_GRID_DB` in `lib.rs` / `bin/measure_gi`.
const DRIVE_GRID_DB: [f32; 13] = [
    -12.0, -9.0, -6.0, -3.0, 0.0, 3.0, 6.0, 9.0, 12.0, 15.0, 18.0, 21.0, 24.0,
];

/// GAIN_STEPS R_KJ order (15/20/25/30/35 dB). Position 0 is the open contact (large sentinel).
const STEP_R_KJ: [f32; 5] = [1.0e9, 1_200.0, 470.0, 220.0, 110.0];
const LABELS: [&str; 5] = ["15 dB", "20 dB", "25 dB", "30 dB", "35 dB"];

fn db_to_gain(db: f32) -> f64 {
    10.0f64.powf(db as f64 * 0.05)
}

/// WaveNet output/input RMS at a 1 kHz sine of amplitude `amp`, conditioned exactly as the plugin's
/// levelNorm-ON path: `cond = gain_norm`, `input_cond = 0.5` (the 0 dB neutral). Includes the
/// `output_ref_v / INPUT_FS_VOLTS` (=128×) de-norm the dsp chain applies, so the gain is directly
/// comparable to the WDF `G_v`. Self-warms over the receptive field + several cycles.
fn measure_gnn_sat(gain_norm: f32, amp: f64) -> f64 {
    let mut net = WaveNet::new_from_embedded();
    net.reset();
    let deref = net.output_ref_v() as f64 / INPUT_FS_VOLTS; // = 128 at the 64 V ref
    let input_cond = input_gain_db_to_norm(0.0); // levelNorm-ON neutral (= 0.5)
    let cycle = (HOST_SR / PROBE_F).max(1.0);
    // Longer settle than the small-signal probe: under drive the net's saturated steady state takes
    // more cycles to stabilise. Integrate several cycles for a clean saturated RMS.
    let n_settle = net.receptive_field() + (cycle * 16.0) as usize;
    let n_meas = (cycle * 16.0) as usize;
    let w = 2.0 * std::f64::consts::PI * PROBE_F / HOST_SR;
    let (mut in_sq, mut out_sq) = (0.0f64, 0.0f64);
    for i in 0..(n_settle + n_meas) {
        let x = amp * (w * i as f64).sin();
        let y = net.process(x as f32, &[gain_norm, input_cond]) as f64 * deref;
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

fn main() {
    let mut makeup = [[0f32; DRIVE_GRID_DB.len()]; 5];
    println!("// DRIVE-swept WaveNet makeup `1/G_nn_sat`. Rows = gain positions, cols = DRIVE grid (dB).");
    println!("// A0 = {:.1} (0 dBFS net input), SR = {} Hz, cond = rgain_to_gain_norm(R), input_cond = 0.5.", A0_NN, HOST_SR);
    print!("// pos \\ drive_dB ");
    for d in DRIVE_GRID_DB {
        print!("{:>9.0}", d);
    }
    println!();
    for (i, &r) in STEP_R_KJ.iter().enumerate() {
        let gn = rgain_to_gain_norm(r);
        print!("//  {} {:<6}     ", i, LABELS[i]);
        for (j, &d) in DRIVE_GRID_DB.iter().enumerate() {
            let amp = A0_NN * db_to_gain(d);
            let g_sat = measure_gnn_sat(gn, amp);
            makeup[i][j] = (1.0 / g_sat) as f32;
            print!("{:>9.6}", makeup[i][j]);
        }
        println!();
    }

    // Emit paste-ready rows for `GAIN_STEPS[*].nn_makeup` in `lib.rs`.
    println!("\n// Rows align with GAIN_STEPS order (15/20/25/30/35 dB); paste into each `nn_makeup`.");
    for (i, row) in makeup.iter().enumerate() {
        print!("// {} ({}): &[", i, LABELS[i]);
        for (j, m) in row.iter().enumerate() {
            print!("{:.6}{}", m, if j + 1 < row.len() { ", " } else { "" });
        }
        println!("],");
    }
}
