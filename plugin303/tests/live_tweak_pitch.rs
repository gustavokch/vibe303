use plugin303::dsp::filter::Tier;
use plugin303::dsp::voice::Voice;
use plugin303::params::{CircuitTopology, CompTolerance, OscWave, VcaEnvMode, VcfTaps};

fn realized_hz(buf: &[f32], sr: f32) -> f32 {
    let (mut first_t, mut last_t, mut count) = (-1.0f64, 0.0f64, 0i64);
    for i in 1..buf.len() {
        if buf[i - 1] < 0.0 && buf[i] >= 0.0 {
            let frac = (-buf[i - 1] as f64) / ((buf[i] - buf[i - 1]) as f64);
            let t = (i - 1) as f64 + frac;
            if first_t < 0.0 {
                first_t = t;
            } else {
                last_t = t;
                count += 1;
            }
        }
    }
    if count == 0 || last_t <= first_t {
        return 0.0;
    }
    (count as f64 * sr as f64 / (last_t - first_t)) as f32
}

#[test]
fn test_live_tweaks_during_playback() {
    let sr = 48000.0;
    let mut v = Voice::new(sr);
    v.set_tier_immediate(Tier::Best);
    v.set_wave(OscWave::Saw);
    v.set_cutoff(4000.0);
    v.set_reso(0.0);

    let note = 60u8; // Middle C = 261.63 Hz
    let target = 440.0 * 2.0f32.powf((note as f32 - 69.0) / 12.0);

    // Start playing note
    v.note_on(note);
    v.set_gate(true, true, false, false);

    // Warm up 1 second
    for _ in 0..48000 {
        v.tick();
    }

    let buf0: Vec<f32> = (0..4800).map(|_| v.tick()).collect();
    let hz0 = realized_hz(&buf0, sr);
    let error0 = (1200.0 * (hz0 / target).log2()).abs();
    eprintln!("Initial steady pitch: {hz0:.2} Hz (target {target:.2} Hz, error {error0:.1} cents)");

    // Tweak age + tolerance live while playing
    v.update_params(
        0.0,
        0.0,
        0.0,
        0.0,
        OscWave::Saw,
        0.5,
        0.0,
        0.5,
        0.5,
        0.5,
        0.5,
        0.5,
        0.0,
        VcfTaps::FourPole,
        0.0,
        false,
        3.0,
        200.0,
        0.0,
        200.0,
        3.0,
        200.0,
        0.0,
        200.0,
        VcaEnvMode::Envelope,
        50,
        0.0,
        CompTolerance::Five,
        0,
        CircuitTopology::Accurate,
        0.0,
        1337,
        0.5,
        0.0,
        1.0,
    );

    // Sample immediately after update_params
    let buf_imm: Vec<f32> = (0..4800).map(|_| v.tick()).collect();
    let hz_imm = realized_hz(&buf_imm, sr);
    let error_imm = (1200.0 * (hz_imm / target).log2()).abs();
    eprintln!(
        "Pitch immediately after live age tweak: {hz_imm:.2} Hz (error {error_imm:.1} cents)"
    );

    // Wait 1.0s for worker thread + thermal settling
    for _ in 0..48000 {
        v.tick();
    }

    let buf_settled: Vec<f32> = (0..4800).map(|_| v.tick()).collect();
    let hz_settled = realized_hz(&buf_settled, sr);
    let error_settled = (1200.0 * (hz_settled / target).log2()).abs();
    eprintln!("Pitch 1s after live age tweak: {hz_settled:.2} Hz (error {error_settled:.1} cents)");
}
