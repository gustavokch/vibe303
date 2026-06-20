//! Behavior test for the hardware-stepped gain feature: the 5 real BA283 R_KJ switch positions must
//! map to a strictly monotonic, discrete small-signal gain ladder (more feedback ⇒ less R ⇒ more
//! gain), with the open contact (idx 0) the lowest. Validates the table values themselves — see the
//! `GAIN_STEPS` table in lib.rs (kept in sync with this list).

use ba283::dsp::ChannelDsp;
use ba283::wdf::{QualityConfig, WdfModel};
use ba283::{norm_apply_gain, Ba283Plugin, InferenceBackend};

/// R_KJ values stamped per switch position, mirroring `lib.rs::GAIN_STEPS[*].r_kj_ohm`
/// (open → 1e9 sentinel). Index order = ascending dB / descending R.
const STEP_R_KJ: [f64; 5] = [1.0e9, 1_200.0, 470.0, 220.0, 110.0];

#[test]
fn stepped_gain_is_monotonic_and_discrete() {
    let host = 48_000.0_f64;
    let mut gains = Vec::with_capacity(STEP_R_KJ.len());
    for &r in &STEP_R_KJ {
        // Fresh model per step so each gain is measured from the same settled cold state (the plugin
        // re-stamps + re-settles on a real detent crossing; this is the equivalent steady reading).
        let mut m = WdfModel::new_with(QualityConfig::realtime(), host, r, 0.5);
        m.set_rgain(r);
        m.warmup();
        let g = m.measure_gain();
        assert!(g.is_finite() && g > 0.0, "step R={r} Ω gain must be finite/positive, got {g}");
        gains.push(g);
    }

    // Strictly increasing: each lower-R position is louder than the one before.
    for w in gains.windows(2) {
        assert!(
            w[1] > w[0],
            "gain ladder must strictly increase as R_KJ drops; got {:?}",
            gains
        );
    }

    // The open contact (idx 0) is the quietest, the 110 Ω position the loudest, and the steps are
    // distinct (no two positions collapse to the same gain within 1%).
    assert!(gains[0] < gains[STEP_R_KJ.len() - 1], "open step must be the quietest");
    for w in gains.windows(2) {
        assert!(
            (w[1] - w[0]).abs() > 0.01 * w[0],
            "adjacent steps must be audibly distinct (>1% apart); got {:?}",
            gains
        );
    }
}

/// DRIFT GUARD for the embedded DRIVE-swept levelNorm makeup table: re-measure each position's
/// LARGE-SIGNAL voltage gain `G_sat` at every DRIVE grid point (driven by `A0 * 10^(drive/20)`, the
/// exact stimulus `bin/measure_gi` uses) and assert `1/G_sat` matches the curve baked into
/// `GAIN_STEPS[pos].wdf_makeup` (via `Ba283Plugin::pos_makeup`). If the solver / circuit ever changes
/// the measured gain, this fails — forcing a re-run of `bin/measure_gi` to refresh the table instead
/// of silently shipping a levelNorm that no longer holds loudness under Drive.
#[test]
fn embedded_makeup_matches_remeasured_gain() {
    use ba283::dsp::{ChannelDsp, INPUT_FS_VOLTS};
    // MUST mirror `DRIVE_GRID_DB` in lib.rs / bin/measure_gi.
    const DRIVE_GRID_DB: [f32; 13] =
        [-12.0, -9.0, -6.0, -3.0, 0.0, 3.0, 6.0, 9.0, 12.0, 15.0, 18.0, 21.0, 24.0];
    let a0 = INPUT_FS_VOLTS as f64; // 0 dBFS calibration reference
    let host = 48_000.0_f32;
    for (pos, &r) in STEP_R_KJ.iter().enumerate() {
        let mut ch = ChannelDsp::new();
        ch.wdf_rebuild_with(QualityConfig::realtime(), host, r as f32, 0.5);
        ch.wdf_warmup();
        for &d in &DRIVE_GRID_DB {
            let amp = a0 * 10f64.powf(d as f64 * 0.05);
            let measured = 1.0 / ch.wdf_measure_gain_at(amp) as f64;
            let embedded = Ba283Plugin::pos_makeup(pos as i32, d) as f64;
            let rel = (measured - embedded).abs() / embedded;
            assert!(
                rel < 0.02,
                "pos {pos} (R={r}Ω) drive {d:+} dB: embedded makeup {embedded} drifted from \
                 re-measured {measured} (rel {rel:.4} > 2%). Re-run \
                 `cargo run --release --bin measure_gi` and update GAIN_STEPS."
            );
        }
    }
}

/// REGRESSION GUARD (the bug this fix targets): with the old analytic `1/(G_v·drive)` makeup, a hot
/// input SAGGED ~20 dB at +24 dB Drive because the saturating circuit's gain fell below `G_v` while
/// the makeup still divided out the full linear Drive — the DRIVE knob sounded weak. The Drive-swept
/// `1/G_sat(drive)` makeup must hold output RMS ~constant across the WHOLE Drive sweep (not just the
/// endpoints — the curve bends most mid-sweep) at a 0 dBFS input, at multiple gain positions.
#[test]
fn levelnorm_holds_loudness_across_drive() {
    use std::f32::consts::PI;
    let host = 48_000.0_f32;
    let freq = 1_000.0_f32;
    let amp = 1.0_f32; // 0 dBFS peak → in_v = 0.5 V at unity Drive (the A0 calibration level)
    let measure = |pos: usize, drive_db: f32| -> f64 {
        let r = STEP_R_KJ[pos] as f32;
        let drive = 10f32.powf(drive_db * 0.05);
        // Exactly the plugin's levelNorm makeup: Drive-swept trim divided by the Drive scalar.
        let norm = norm_apply_gain(Ba283Plugin::pos_makeup(pos as i32, drive_db), drive);
        let mut ch = ChannelDsp::new();
        ch.wdf_rebuild_with(QualityConfig::realtime(), host, r, 0.5);
        ch.wdf_warmup();
        let (mut out_sq, mut n) = (0.0f64, 0u32);
        for i in 0..8_000u32 {
            let x = amp * (2.0 * PI * freq * i as f32 / host).sin();
            let y = ch.process_sample(x, 1.0, drive, 0.5, 0.5, 1.0, norm, 1.0, false, InferenceBackend::Wdf);
            if i >= 3_000 {
                out_sq += (y as f64) * (y as f64);
                n += 1;
            }
        }
        (out_sq / n as f64).sqrt()
    };
    // pos 3 (220 Ω, default) and pos 4 (110 Ω, hottest — saturates earliest). Sweep the whole grid
    // span incl. the mid-curve bend (+12 dB), each compared to that position's own Drive-0 loudness.
    for &pos in &[3usize, 4usize] {
        let baseline = measure(pos, 0.0);
        for &drive_db in &[6.0_f32, 12.0, 18.0, 24.0] {
            let rms = measure(pos, drive_db);
            let ratio_db = 20.0 * (rms / baseline).log10();
            assert!(
                ratio_db.abs() < 1.5,
                "pos {pos} DRIVE 0→{drive_db:+} dB loudness must stay within ±1.5 dB (no sag); got \
                 {ratio_db:+.2} dB (baseline_rms {baseline:.4}, rms {rms:.4})"
            );
        }
    }
}

/// END-TO-END levelNorm neutrality (the loud-insert bug fix): at EVERY gain position, feeding a
/// small-signal sine through the WDF circuit with the static makeup applied (`norm_gain =
/// 1/(G_i·drive)`, Drive at unity) must land output RMS ≈ input RMS — i.e. the default insert is
/// level-neutral regardless of position. Mirrors the plugin's `process` levelNorm path at the
/// `ChannelDsp` level (no host context needed).
#[test]
fn levelnorm_holds_unity_at_every_position() {
    let host = 48_000.0_f32;
    let amp = 0.05_f32; // small-signal (linear region), matching the gain-probe level
    let freq = 1_000.0_f32;
    for pos in 0..5 {
        let r = STEP_R_KJ[pos] as f32;
        let mut ch = ChannelDsp::new();
        ch.wdf_rebuild_with(QualityConfig::realtime(), host, r, 0.5);
        ch.wdf_warmup();
        let norm_gain = norm_apply_gain(Ba283Plugin::pos_makeup(pos as i32, 0.0), 1.0);
        let (mut sin_sq, mut out_sq, mut n) = (0.0f64, 0.0f64, 0u32);
        for i in 0..6_000u32 {
            let x = amp * (2.0 * std::f32::consts::PI * freq * i as f32 / host).sin();
            // input_gain=1, drive=1, cond=bias 0.5, out_gain=1, mix=1 (fully wet), direct (no bridge).
            let y = ch.process_sample(x, 1.0, 1.0, 0.5, 0.5, 1.0, norm_gain, 1.0, false, InferenceBackend::Wdf);
            if i >= 2_000 {
                // skip the settle transient
                sin_sq += (x as f64) * (x as f64);
                out_sq += (y as f64) * (y as f64);
                n += 1;
            }
        }
        let in_rms = (sin_sq / n as f64).sqrt();
        let out_rms = (out_sq / n as f64).sqrt();
        let rel = (out_rms - in_rms).abs() / in_rms;
        assert!(
            rel < 0.06,
            "pos {pos}: levelNorm not unity — in_rms {in_rms:.5} vs out_rms {out_rms:.5} (rel {rel:.4})"
        );
    }
}

/// levelNorm OFF (faithful manual staging): `input_gain` applies linearly pre-circuit, and `link`'s
/// effect (output mirrors input opposite, here +6 dB in / −6 dB out) restores the same net output as
/// unity trims — proving the staging chain `×input_gain → circuit → ×output_gain` is wired right and
/// that the link cancellation is level-neutral while still driving the circuit hotter.
#[test]
fn manual_staging_and_link_are_level_correct() {
    let host = 48_000.0_f32;
    let amp = 0.05_f32;
    let freq = 1_000.0_f32;
    let r = STEP_R_KJ[3] as f32; // 220 Ω position

    // Render one settled run with the given (input_gain, output_gain), levelNorm OFF (norm_gain=1).
    let run = |in_g: f32, out_g: f32| -> f64 {
        let mut ch = ChannelDsp::new();
        ch.wdf_rebuild_with(QualityConfig::realtime(), host, r, 0.5);
        ch.wdf_warmup();
        let (mut out_sq, mut n) = (0.0f64, 0u32);
        for i in 0..6_000u32 {
            let x = amp * (2.0 * std::f32::consts::PI * freq * i as f32 / host).sin();
            let y = ch.process_sample(x, in_g, 1.0, 0.5, 0.5, out_g, 1.0, 1.0, false, InferenceBackend::Wdf);
            if i >= 2_000 {
                out_sq += (y as f64) * (y as f64);
                n += 1;
            }
        }
        (out_sq / n as f64).sqrt()
    };

    let unity = run(1.0, 1.0); // faithful: out ≈ in·G_v
    let louder = run(2.0, 1.0); // +6 dB input ⇒ ~2× output (circuit near-linear at small signal)
    let linked = run(2.0, 0.5); // link: +6 dB in / −6 dB out ⇒ net back to unity-staging level

    let louder_ratio = louder / unity;
    assert!(
        (louder_ratio - 2.0).abs() < 0.15,
        "input_gain +6 dB should ~double output: ratio {louder_ratio:.3}"
    );
    let linked_rel = (linked - unity).abs() / unity;
    assert!(
        linked_rel < 0.05,
        "link (+6 in / −6 out) should restore unity-staging level: {linked:.5} vs {unity:.5} (rel {linked_rel:.4})"
    );
}
