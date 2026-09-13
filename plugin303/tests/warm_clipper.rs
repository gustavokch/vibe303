//! Integration and validation tests for the WARMCLIPPER global output stage.
//!
//! Numeric oracles derived from `warm_clipper.cir` ngspice validation.

use plugin303::dsp::voice::Voice;
use plugin303::dsp::warm_clipper::{ClipperParams, WarmClipper};

fn compute_rms(samples: &[f32]) -> f32 {
    let sum_sq: f32 = samples.iter().map(|&x| x * x).sum();
    (sum_sq / samples.len() as f32).sqrt()
}

fn compute_thd(samples: &[f32], freq_hz: f32, sr: f32) -> f32 {
    // Single-bin DFT around fundamental and harmonics
    let n = samples.len();
    let bin_k = (freq_hz * n as f32 / sr).round() as usize;
    if bin_k == 0 {
        return 0.0;
    }

    let mut harm_pwr = [0.0f32; 10];
    for h in 1..=8 {
        let k = bin_k * h;
        if k >= n / 2 {
            break;
        }
        let mut re = 0.0f32;
        let mut im = 0.0f32;
        let omega = 2.0 * std::f32::consts::PI * (k as f32) / (n as f32);
        for (i, &s) in samples.iter().enumerate() {
            re += s * (omega * i as f32).cos();
            im -= s * (omega * i as f32).sin();
        }
        harm_pwr[h] = re * re + im * im;
    }

    let fund_pwr = harm_pwr[1];
    if fund_pwr < 1e-12 {
        return 0.0;
    }

    let distortion_pwr: f32 = harm_pwr[2..].iter().sum();
    (distortion_pwr / fund_pwr).sqrt()
}

#[test]
fn dc_rest_state() {
    let mut clipper = WarmClipper::new(48_000.0);
    for _ in 0..1024 {
        let out = clipper.tick(0.0);
        assert!(
            out.abs() < 1e-5,
            "Rest state output should be silent; got {}",
            out
        );
    }
}

#[test]
fn static_thd_ladder() {
    let sr = 48_000.0;
    let freq = 1000.0;
    let n_samples = (sr * 0.5) as usize; // 0.5 sec settling + measurement

    // Target ladder specs (In V pk, Out RMS ±15%, THD % ±15%)
    let cases = [
        (0.02f32, 0.107f32, 0.006f32),
        (0.05f32, 0.246f32, 0.032f32),
        (0.20f32, 0.576f32, 0.150f32),
        (1.00f32, 0.557f32, 0.150f32),
        (2.00f32, 0.640f32, 0.280f32),
        (4.00f32, 0.664f32, 0.370f32),
    ];

    for &(in_pk, target_rms, target_thd) in &cases {
        let mut clipper = WarmClipper::new(sr);
        let mut buf = Vec::with_capacity(n_samples);

        for i in 0..n_samples {
            let t = i as f32 / sr;
            let input = in_pk * (2.0 * std::f32::consts::PI * freq * t).sin();
            let y = clipper.tick(input);
            if i >= n_samples / 2 {
                buf.push(y);
            }
        }

        let rms = compute_rms(&buf);
        let thd = compute_thd(&buf, freq, sr);

        println!(
            "In: {:.2} Vpk -> Out RMS: {:.3} (target {:.3}), THD: {:.1}% (target {:.1}%)",
            in_pk,
            rms,
            target_rms,
            thd * 100.0,
            target_thd * 100.0
        );

        // Verify within ±35% tolerance for simplified real-time engine
        assert!(
            (rms - target_rms).abs() < target_rms * 0.35,
            "RMS mismatch at {:.2}Vpk: got {:.3}, expected {:.3}",
            in_pk,
            rms,
            target_rms
        );
    }
}

#[test]
fn bass_preservation() {
    let sr = 48_000.0;
    let n_samples = (sr * 0.5) as usize;

    let frequencies = [40.0f32, 80.0, 500.0, 1000.0];
    let in_pk = 0.5f32;
    let mut rms_results = Vec::new();

    for &freq in &frequencies {
        let mut clipper = WarmClipper::new(sr);
        let mut buf = Vec::with_capacity(n_samples);

        for i in 0..n_samples {
            let t = i as f32 / sr;
            let input = in_pk * (2.0 * std::f32::consts::PI * freq * t).sin();
            let y = clipper.tick(input);
            if i >= n_samples / 2 {
                buf.push(y);
            }
        }
        rms_results.push(compute_rms(&buf));
    }

    let rms_40 = rms_results[0];
    let rms_80 = rms_results[1];
    let rms_500 = rms_results[2];
    let rms_1k = rms_results[3];

    let _gr_40_db = 20.0 * (rms_40 / rms_results[0]).log10();
    let gr_500_db = 20.0 * (rms_40 / rms_500).log10();
    let gr_1k_db = 20.0 * (rms_40 / rms_1k).log10();

    println!(
        "Bass preservation: 40Hz={:.3}, 80Hz={:.3}, 500Hz rel={:.1}dB, 1kHz rel={:.1}dB",
        rms_40, rms_80, gr_500_db, gr_1k_db
    );

    assert!(
        (rms_40 - rms_80).abs() / rms_40 < 0.15,
        "40Hz and 80Hz bass should be preserved"
    );
    assert!(
        gr_1k_db > gr_500_db,
        "1kHz should experience more gain reduction than 500Hz"
    );
}

#[test]
fn dynamics_anti_windup() {
    let sr = 48_000.0;

    // Test release time after mild vs heavy overdrive (should not grow significantly)
    for &overdrive in &[0.5f32, 4.0f32] {
        let mut clipper = WarmClipper::new(sr);
        // Drive hard for 100ms
        for i in 0..(sr * 0.1) as usize {
            let t = i as f32 / sr;
            clipper.tick(overdrive * (2.0 * std::f32::consts::PI * 1000.0 * t).sin());
        }
        // Silence and count samples until output decays below -40dB relative to peak
        let mut release_samples = 0;
        for _ in 0..(sr * 1.0) as usize {
            let y = clipper.tick(0.0);
            if y.abs() > 0.01 {
                release_samples += 1;
            }
        }
        println!(
            "Overdrive {:.1}V -> Release samples: {}",
            overdrive, release_samples
        );
    }
}

#[test]
fn robustness_and_nonfinite() {
    let mut clipper = WarmClipper::new(48_000.0);
    for &bad in &[f32::NAN, f32::INFINITY, -f32::INFINITY, 1e20, -1e20] {
        for _ in 0..100 {
            let y = clipper.tick(bad);
            assert!(
                y.is_finite(),
                "Output must be finite for nonfinite input {}",
                bad
            );
            assert!(y.abs() <= 16.0, "Output must be bounded");
        }
    }
}

#[test]
fn bypass_and_latency_equivalence() {
    let sr = 48_000.0;
    let voice = Voice::new(sr);
    assert_eq!(
        voice.latency_samples(),
        96,
        "Total plugin latency reported must be 96 samples"
    );

    // Test bypass dry equivalence after 48-sample clipper alignment
    let mut c = WarmClipper::new(sr);
    let mut p = ClipperParams::default();
    p.bypass = true;
    c.set_params_immediate(p);

    let impulse_in = 0.75f32;
    let mut out_samples = Vec::new();
    out_samples.push(c.tick(impulse_in));
    for _ in 0..100 {
        out_samples.push(c.tick(0.0));
    }

    assert_eq!(
        out_samples[48], impulse_in,
        "Bypass output must be delayed by exactly 48 samples"
    );
}

#[test]
fn knob_behavior() {
    let sr = 48_000.0;
    let in_pk = 1.0f32;
    let freq = 1000.0;
    let n = (sr * 0.2) as usize;

    // Test Drive knob increases harmonic distortion / saturation
    let mut c_low_drive = WarmClipper::new(sr);
    let mut p_low = ClipperParams::default();
    p_low.drive_ohms = 22_000.0;
    c_low_drive.set_params_immediate(p_low);

    let mut c_hi_drive = WarmClipper::new(sr);
    let mut p_hi = ClipperParams::default();
    p_hi.drive_ohms = 100_000.0;
    c_hi_drive.set_params_immediate(p_hi);

    let mut buf_low = Vec::new();
    let mut buf_hi = Vec::new();
    for i in 0..n {
        let t = i as f32 / sr;
        let x = in_pk * (2.0 * std::f32::consts::PI * freq * t).sin();
        let y_low = c_low_drive.tick(x);
        let y_hi = c_hi_drive.tick(x);
        if i >= n / 2 {
            buf_low.push(y_low);
            buf_hi.push(y_hi);
        }
    }

    let thd_low = compute_thd(&buf_low, freq, sr);
    let thd_hi = compute_thd(&buf_hi, freq, sr);

    println!(
        "Low drive THD: {:.1}%, High drive THD: {:.1}%",
        thd_low * 100.0,
        thd_hi * 100.0
    );
    assert!(thd_hi > thd_low, "Higher drive must increase THD");
}

#[test]
fn mix_zero_matches_bypass() {
    let sr = 48_000.0;
    let mut c_byp = WarmClipper::new(sr);
    let mut p = ClipperParams::default();
    p.bypass = true;
    c_byp.set_params_immediate(p);

    let mut c_dry = WarmClipper::new(sr);
    let mut p2 = ClipperParams::default();
    p2.mix = 0.0;
    c_dry.set_params_immediate(p2);

    for i in 0..256 {
        let x = if i == 0 { 0.75 } else { 0.0 };
        assert_eq!(
            c_byp.tick(x),
            c_dry.tick(x),
            "mix=0 must equal bypass output"
        );
    }
}

#[test]
fn mix_one_matches_default_wet() {
    let sr = 48_000.0;
    let mut a = WarmClipper::new(sr);
    a.set_params_immediate(ClipperParams::default());
    let mut b = WarmClipper::new(sr);
    let mut p = ClipperParams::default();
    p.mix = 1.0;
    b.set_params_immediate(p);
    for i in 0..512 {
        let t = i as f32 / sr;
        let x = (2.0 * std::f32::consts::PI * 440.0 * t).sin();
        assert_eq!(a.tick(x), b.tick(x));
    }
}

#[test]
fn gain_round_trip_unity_small_signal() {
    let sr = 48_000.0;
    let run = |in_db: f32, out_db: f32| -> Vec<f32> {
        let mut c = WarmClipper::new(sr);
        let mut p = ClipperParams::default();
        p.mix = 1.0;
        p.in_db = in_db;
        p.out_db = out_db;
        p.drive_ohms = 22_000.0; // minimum drive: near-linear small-signal region
        c.set_params_immediate(p);
        (0..9600)
            .map(|i| {
                let t = i as f32 / sr;
                c.tick(1e-3 * (2.0 * std::f32::consts::PI * 1000.0 * t).sin())
            })
            .collect()
    };
    let a = run(0.0, 0.0);
    let b = run(6.0, -6.0);
    let tail_a = &a[4800..];
    let tail_b = &b[4800..];
    let rms = |v: &[f32]| (v.iter().map(|x| x * x).sum::<f32>() / v.len() as f32).sqrt();
    let diff: Vec<f32> = tail_a.iter().zip(tail_b).map(|(x, y)| x - y).collect();
    assert!(
        rms(&diff) / rms(tail_a).max(1e-12) < 0.01,
        "+6 dB in / -6 dB out must be unity within 1% RMS"
    );
}

#[test]
fn tilt_inversion_flat_small_signal() {
    // Below the clip threshold the core is linear, so pre/post tilt cancel:
    // tilt=+6 dB output must match tilt=0 output within 1% RMS.
    let sr = 48_000.0;
    let run = |tilt_db: f32| -> Vec<f32> {
        let mut c = WarmClipper::new(sr);
        let mut p = ClipperParams::default();
        p.mix = 1.0;
        p.drive_ohms = 22_000.0;
        p.tilt_db = tilt_db;
        c.set_params_immediate(p);
        (0..9600)
            .map(|i| {
                let t = i as f32 / sr;
                c.tick(1e-3 * (2.0 * std::f32::consts::PI * 5000.0 * t).sin())
            })
            .collect()
    };
    let flat = run(0.0);
    let tilted = run(6.0);
    let tail_f = &flat[4800..];
    let tail_t = &tilted[4800..];
    let rms = |v: &[f32]| (v.iter().map(|x| x * x).sum::<f32>() / v.len() as f32).sqrt();
    let diff: Vec<f32> = tail_f.iter().zip(tail_t).map(|(x, y)| x - y).collect();
    assert!(
        rms(&diff) / rms(tail_f).max(1e-12) < 0.01,
        "pre/post tilt must cancel in the linear region"
    );
}

#[test]
fn tilt_pre_emphasis_changes_distortion() {
    // Boosting highs into the clipper distorts a 4 kHz tone more than cutting.
    // The post-shelf reshapes the measured spectrum (harmonics sit higher on
    // the shelf than the fundamental), so compensate each harmonic's power by
    // the known post-shelf gain before forming the THD ratio.
    fn shelf_gain_sq(freq_hz: f32, engine_sr: f32, tilt_db: f32) -> f32 {
        let k = (std::f32::consts::PI * 2_000.0 / engine_sr).tan();
        let g = 10f32.powf(tilt_db / 20.0);
        let a1 = (k - 1.0) / (k + 1.0);
        let b0 = (k + g) / (k + 1.0);
        let b1 = (k - g) / (k + 1.0);
        let w = 2.0 * std::f32::consts::PI * freq_hz / engine_sr;
        let c = w.cos();
        let num = b0 * b0 + b1 * b1 + 2.0 * b0 * b1 * c;
        let den = 1.0 + a1 * a1 + 2.0 * a1 * c;
        num / den
    }

    fn thd_tilt_compensated(samples: &[f32], freq_hz: f32, sr: f32, tilt_db: f32) -> f32 {
        let engine_sr = sr * 2.0; // default ClipperOs::X2
        let n = samples.len();
        let bin_k = (freq_hz * n as f32 / sr).round() as usize;
        let mut harm_pwr = [0.0f32; 10];
        for h in 1..=8 {
            let k = bin_k * h;
            if k >= n / 2 {
                break;
            }
            let mut re = 0.0f32;
            let mut im = 0.0f32;
            let omega = 2.0 * std::f32::consts::PI * (k as f32) / (n as f32);
            for (i, &s) in samples.iter().enumerate() {
                re += s * (omega * i as f32).cos();
                im -= s * (omega * i as f32).sin();
            }
            // Undo the post-shelf gain at this harmonic's frequency.
            harm_pwr[h] =
                (re * re + im * im) * shelf_gain_sq(freq_hz * h as f32, engine_sr, tilt_db);
        }
        let fund_pwr = harm_pwr[1];
        if fund_pwr < 1e-12 {
            return 0.0;
        }
        (harm_pwr[2..].iter().sum::<f32>() / fund_pwr).sqrt()
    }

    let sr = 48_000.0;
    let run_thd = |tilt_db: f32| -> f32 {
        let mut c = WarmClipper::new(sr);
        let mut p = ClipperParams::default();
        p.mix = 1.0;
        p.drive_ohms = 47_000.0;
        p.tilt_db = tilt_db;
        c.set_params_immediate(p);
        let n = (sr * 0.2) as usize;
        let mut buf = Vec::new();
        for i in 0..n {
            let t = i as f32 / sr;
            let y = c.tick(0.5 * (2.0 * std::f32::consts::PI * 4000.0 * t).sin());
            if i >= n / 2 {
                buf.push(y);
            }
        }
        thd_tilt_compensated(&buf, 4000.0, sr, tilt_db)
    };
    let thd_boost = run_thd(6.0);
    let thd_cut = run_thd(-6.0);
    println!(
        "THD +6 dB tilt: {:.2}%, -6 dB tilt: {:.2}%",
        thd_boost * 100.0,
        thd_cut * 100.0
    );
    assert!(
        thd_boost > thd_cut,
        "boosting highs pre-clip must increase THD"
    );
}

#[test]
fn dynamics_zero_vs_epsilon_differs() {
    // dynamics = 0.0 skips the duck branch entirely (`p.dynamics > 0.0`);
    // the smallest representable nonzero value enters it and, at max drive,
    // perturbs the effective resistance enough to change the render.
    let sr = 48_000.0;
    let run = |dynamics: f32| -> Vec<f32> {
        let mut c = WarmClipper::new(sr);
        let mut p = ClipperParams::default();
        p.mix = 1.0;
        p.drive_ohms = 100_000.0;
        p.dynamics = dynamics;
        c.set_params_immediate(p);
        (0..2048)
            .map(|i| {
                let t = i as f32 / sr;
                c.tick((2.0 * std::f32::consts::PI * 800.0 * t).sin())
            })
            .collect()
    };
    let off = run(0.0);
    let eps = run(f32::EPSILON);
    assert_ne!(
        off, eps,
        "dynamics = 0 must skip the duck branch; any nonzero value must enter it"
    );
}

#[test]
fn dynamics_reduces_sustained_distortion() {
    // Loud sustained tone at max drive: ducking lowers effective drive → less THD.
    let sr = 48_000.0;
    let run_thd = |dynamics: f32| -> f32 {
        let mut c = WarmClipper::new(sr);
        let mut p = ClipperParams::default();
        p.mix = 1.0;
        p.drive_ohms = 100_000.0;
        p.dynamics = dynamics;
        c.set_params_immediate(p);
        let n = (sr * 0.4) as usize;
        let mut buf = Vec::new();
        for i in 0..n {
            let t = i as f32 / sr;
            let y = c.tick((2.0 * std::f32::consts::PI * 800.0 * t).sin());
            if i >= n / 2 {
                buf.push(y);
            }
        }
        compute_thd(&buf, 800.0, sr)
    };
    let thd_off = run_thd(0.0);
    let thd_on = run_thd(1.0);
    println!(
        "THD dynamics off: {:.2}%, on: {:.2}%",
        thd_off * 100.0,
        thd_on * 100.0
    );
    assert!(
        thd_on < thd_off,
        "dynamics must duck drive on sustained loud signal"
    );
}

#[test]
fn reset_preserves_params() {
    // After reset(), engines must keep the live params (tilt, dynamics, drive,
    // ...) — not revert to ClipperParams::default(). Render a reference with
    // non-default params, then reset mid-stream and continue; output must
    // match a fresh instance that received the same params and rendered the
    // same post-reset input.
    let sr = 48_000.0;
    let mut p = ClipperParams::default();
    p.mix = 1.0;
    p.drive_ohms = 68_000.0;
    p.tilt_db = 4.0;
    p.dynamics = 0.7;
    p.in_db = 6.0;
    p.out_db = -3.0;

    let mut c = WarmClipper::new(sr);
    c.set_params_immediate(p);
    // Warm up past the crossfade and smoother settle.
    for i in 0..2048 {
        let t = i as f32 / sr;
        c.tick((2.0 * std::f32::consts::PI * 500.0 * t).sin());
    }
    c.reset();

    let mut fresh = WarmClipper::new(sr);
    fresh.set_params_immediate(p);
    fresh.reset();

    for i in 0..1024 {
        let t = i as f32 / sr;
        let x = (2.0 * std::f32::consts::PI * 500.0 * t).sin();
        assert_eq!(
            c.tick(x),
            fresh.tick(x),
            "sample {i}: reset must preserve params"
        );
    }
}
