use plugin303::dsp::filter::Filter303;
use plugin303::dsp::ladder::TptLadder;
use plugin303::dsp::ladder_wdf::LadderVcf;

fn measure_slope(f: &mut dyn Filter303, sr: f32, poles: usize) -> f32 {
    f.reset();
    f.set_cutoff(1000.0);
    f.set_reso(0.0);
    f.set_poles(poles);

    // Warm up
    let amp = 0.5f32;

    // RMS at 2 kHz (1 octave above cutoff)
    let mut rms_2k = 0.0;
    let mut ph = 0.0f32;
    for i in 0..4000 {
        let x = amp * (2.0 * std::f32::consts::PI * ph).sin();
        let y = f.tick(x);
        if i >= 2000 {
            rms_2k += y * y;
        }
        ph += 2000.0 / sr;
    }
    rms_2k = (rms_2k / 2000.0).sqrt();

    // RMS at 4 kHz (2 octaves above cutoff)
    f.reset();
    let mut rms_4k = 0.0;
    let mut ph = 0.0f32;
    for i in 0..4000 {
        let x = amp * (2.0 * std::f32::consts::PI * ph).sin();
        let y = f.tick(x);
        if i >= 2000 {
            rms_4k += y * y;
        }
        ph += 4000.0 / sr;
    }
    rms_4k = (rms_4k / 2000.0).sqrt();

    // Slope in dB/octave
    20.0 * (rms_2k / rms_4k.max(1e-9)).log10()
}

#[test]
fn tpt_ladder_slopes() {
    let sr = 48_000.0;
    let mut f = TptLadder::new(sr);

    for (poles, expected) in [(2, 12.0), (3, 18.0), (4, 24.0)] {
        let slope = measure_slope(&mut f, sr, poles);
        assert!(
            (slope - expected).abs() < 3.0,
            "TPT {} poles: expected ~{}, got {}",
            poles,
            expected,
            slope
        );
    }
}

#[test]
fn nodal_ladder_slopes() {
    let sr = 192_000.0;
    // We run it at high rate since oversampling runs it at BEST_OS_FACTOR
    let mut f = LadderVcf::new(sr);

    // Nodal filter poles are physically spread by diode loading (not textbook coincident),
    // so the slopes are naturally shallower in this frequency band.
    for (poles, expected) in [(2, 10.5), (3, 17.5), (4, 18.0)] {
        let slope = measure_slope(&mut f, sr, poles);
        assert!(
            (slope - expected).abs() < 2.0,
            "Nodal {} poles: expected ~{}, got {}",
            poles,
            expected,
            slope
        );
    }
}
