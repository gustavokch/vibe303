//! Ultra VCF active-resonance gate (F6.3): the inverting AC-coupled Q9 feedback
//! into `in_r` must produce an fc-tracking peak matching the ngspice reference
//! (VCF_MAP §11, fixture `eval/303/vcf303_reso_gold.json`). Replaces the old
//! 1-sample-delayed `k_reso·out_prev` hack (which peaked near DC, no fc tracking).
use plugin303::dsp::filter::Filter303;
use plugin303::dsp::ladder_wdf::LadderVcf;

const SR: f32 = 192_000.0; // Ultra runs the ladder at 4×48k.
const TAU: f32 = std::f32::consts::TAU;

/// Small-signal AC gain at `freq`: RMS(out)/RMS(in) over steady state.
fn ac_gain(f: &mut LadderVcf, freq: f32) -> f32 {
    f.reset();
    let n = 32_768usize;
    let amp = 0.05f32;
    let warm = (n * 6) / 10;
    let (mut sin_e, mut sout_e) = (0.0f64, 0.0f64);
    for i in 0..n {
        let x = amp * (TAU * freq * i as f32 / SR).sin();
        let y = f.tick(x);
        if i >= warm {
            sin_e += (x as f64) * (x as f64);
            sout_e += (y as f64) * (y as f64);
        }
    }
    ((sout_e / sin_e).sqrt()) as f32
}

/// Frequency of max AC gain, log-swept over [lo, hi].
fn peak_hz(f: &mut LadderVcf, lo: f32, hi: f32) -> f32 {
    let steps = 60usize;
    let (mut best_f, mut best_g) = (lo, 0.0f32);
    for k in 0..=steps {
        let t = k as f32 / steps as f32;
        let fr = lo * (hi / lo).powf(t);
        let g = ac_gain(f, fr);
        if g > best_g {
            best_g = g;
            best_f = fr;
        }
    }
    best_f
}

/// Peak AC gain over the sweep, in dB.
fn peak_gain_db(f: &mut LadderVcf) -> f32 {
    let steps = 60usize;
    let (lo, hi) = (100.0f32, 6000.0f32);
    let mut best_g = 0.0f32;
    for k in 0..=steps {
        let t = k as f32 / steps as f32;
        let fr = lo * (hi / lo).powf(t);
        best_g = best_g.max(ac_gain(f, fr));
    }
    20.0 * best_g.log10()
}

/// Cheap AC gain (fewer samples) for the calibration scan only.
fn ac_gain_fast(f: &mut LadderVcf, freq: f32) -> f32 {
    f.reset();
    let n = 12_000usize;
    let amp = 0.05f32;
    let warm = n / 2;
    let (mut si, mut so) = (0.0f64, 0.0f64);
    for i in 0..n {
        let x = amp * (TAU * freq * i as f32 / SR).sin();
        let y = f.tick(x);
        if i >= warm {
            si += (x as f64) * (x as f64);
            so += (y as f64) * (y as f64);
        }
    }
    ((so / si).sqrt()) as f32
}

/// Calibration: scan the Q9 feedback gain (sign + magnitude) and print the
/// resulting peak freq / height at a few cutoffs. Compare to the ngspice fixture
/// `eval/303/vcf303_reso_gold.json` (532/1002/1683 Hz). Run:
///   cargo test -p plugin303 --release --test vcf_resonance reso_calibration -- --ignored --nocapture
#[test]
#[ignore = "diagnostic: calibrate Q9 feedback gain vs the ngspice reso fixture"]
fn reso_calibration() {
    for &g in &[8.0f64, 12.0, 16.0, 20.0, 24.0, 32.0] {
        println!("reso_gain = {g:+.1}");
        for &fc in &[500.0f32, 1000.0, 2000.0] {
            let mut f = LadderVcf::new(SR);
            f.set_reso_gain(g);
            f.set_reso(0.85);
            f.set_cutoff(fc);
            let (steps, lo, hi) = (40usize, 100.0f32, 6000.0f32);
            let (mut bf, mut bg, mut g0) = (lo, 0.0f32, 0.0f32);
            for k in 0..=steps {
                let t = k as f32 / steps as f32;
                let fr = lo * (hi / lo).powf(t);
                let gn = ac_gain_fast(&mut f, fr);
                if k == 0 {
                    g0 = gn;
                }
                if gn > bg {
                    bg = gn;
                    bf = fr;
                }
            }
            let peak_db = 20.0 * (bg / g0).log10();
            println!("  fc {fc:6.0} -> peak {bf:7.1} Hz  (+{peak_db:5.1} dB over LF)");
        }
    }
}

#[test]
fn resonance_peak_tracks_cutoff() {
    let mut f = LadderVcf::new(SR);
    f.set_reso(0.85);
    let mut last = 0.0f32;
    for &fc in &[500.0f32, 1000.0, 2000.0] {
        f.set_cutoff(fc);
        let pk = peak_hz(&mut f, 100.0, 6000.0);
        assert!(pk > last, "peak must rise with cutoff: {fc} -> {pk}");
        assert!(
            (0.5 * fc..2.0 * fc).contains(&pk),
            "peak off fc: {fc} -> {pk}"
        );
        last = pk;
    }
}

#[test]
fn resonance_gain_rises_with_reso() {
    let mut f = LadderVcf::new(SR);
    f.set_cutoff(1000.0);
    let g_lo = {
        f.set_reso(0.1);
        peak_gain_db(&mut f)
    };
    let g_hi = {
        f.set_reso(0.9);
        peak_gain_db(&mut f)
    };
    assert!(
        g_hi > g_lo + 6.0,
        "resonance did not add peak gain: {g_lo} -> {g_hi}"
    );
}
