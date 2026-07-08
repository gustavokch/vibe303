mod common;

use common::spectral_match;
use plugin303::dsp::filter::Tier;
use plugin303::dsp::voice::Voice;
use plugin303::params::CircuitTopology;

const SR: f32 = 48_000.0;

fn voice_realized_hz(tier: Tier, topology: CircuitTopology, note: u8) -> f32 {
    let mut v = Voice::new(SR);
    v.set_topology(topology);
    let debug = v.get_vco_debug_info();
    println!("topology={:?}, is_fun={}, cv_lut[8]={}", topology, debug.0, debug.1);
    v.set_tier_immediate(tier);
    v.set_vca_env_mode(plugin303::params::VcaEnvMode::Gate);
    v.set_cutoff(4000.0);
    v.set_reso(0.0);
    v.note_on(note);
    v.set_gate(true, false, false, false);

    // Warm up/settle
    for _ in 0..6_000 {
        v.tick();
    }

    let n = 12_000;
    let mut prev = v.tick();
    let (mut first_t, mut last_t, mut count) = (-1.0f64, 0.0f64, 0i64);
    for i in 0..n {
        let y = v.tick();
        if prev < 0.0 && y >= 0.0 {
            let frac = (-prev as f64) / ((y - prev) as f64);
            let t = i as f64 + frac;
            if first_t < 0.0 {
                first_t = t;
            } else {
                last_t = t;
                count += 1;
            }
        }
        prev = y;
    }
    if count == 0 || last_t <= first_t {
        return 0.0;
    }
    (count as f64 * SR as f64 / (last_t - first_t)) as f32
}

#[test]
fn test_pitch_parity_accurate() {
    // Under Accurate topology, playable range is 24..=72
    for note in [24, 36, 48, 60, 72] {
        let f_ultra = voice_realized_hz(Tier::Ultra, CircuitTopology::Accurate, note);
        let f_low = voice_realized_hz(Tier::Low, CircuitTopology::Accurate, note);
        let f_high = voice_realized_hz(Tier::High, CircuitTopology::Accurate, note);

        println!("Accurate Note {}: Ultra={:.2} Hz, Low={:.2} Hz, High={:.2} Hz", note, f_ultra, f_low, f_high);
        
        let err_low = (f_low / f_ultra - 1.0).abs();
        let err_high = (f_high / f_ultra - 1.0).abs();
        
        assert!(err_low < 0.002, "Low pitch mismatch ({:.3}%) at note {}", err_low * 100.0, note);
        assert!(err_high < 0.002, "High pitch mismatch ({:.3}%) at note {}", err_high * 100.0, note);
    }
}

#[test]
fn test_pitch_parity_fun() {
    // Under Fun topology, range extends up to 108
    for note in [24, 48, 72, 96, 108] {
        let f_ultra = voice_realized_hz(Tier::Ultra, CircuitTopology::Fun, note);
        let f_low = voice_realized_hz(Tier::Low, CircuitTopology::Fun, note);
        let f_high = voice_realized_hz(Tier::High, CircuitTopology::Fun, note);

        println!("Fun Note {}: Ultra={:.2} Hz, Low={:.2} Hz, High={:.2} Hz", note, f_ultra, f_low, f_high);
        
        let err_low = (f_low / f_ultra - 1.0).abs();
        let err_high = (f_high / f_ultra - 1.0).abs();
        
        assert!(err_low < 0.002, "Low pitch mismatch ({:.3}%) at note {}", err_low * 100.0, note);
        assert!(err_high < 0.002, "High pitch mismatch ({:.3}%) at note {}", err_high * 100.0, note);
    }
}

fn render_sweep(tier: Tier, note: u8, cutoff_knob: f32, reso_knob: f32, n: usize) -> Vec<f32> {
    let mut v = Voice::new(SR);
    v.set_tier_immediate(tier);
    v.set_vca_env_mode(plugin303::params::VcaEnvMode::Gate);
    v.set_cutoff(200.0 * 20.0f32.powf(cutoff_knob));
    v.set_reso(reso_knob);
    v.set_env_mod(0.7);
    v.note_on(note);
    v.set_gate(true, false, false, false);
    (0..n).map(|_| v.tick()).collect()
}

#[test]
fn test_voicing_parity_sweeps() {
    // Render 1 second (48000 samples) of a dynamic envelope sweep for each tier
    let note = 57; // A2
    let cutoff_knob = 0.4;
    let reso_knob = 0.65;
    let len = 48000;

    let pcm_ultra = render_sweep(Tier::Ultra, note, cutoff_knob, reso_knob, len);
    let pcm_high = render_sweep(Tier::High, note, cutoff_knob, reso_knob, len);
    let pcm_low = render_sweep(Tier::Low, note, cutoff_knob, reso_knob, len);

    let (lsd_high, corr_high) = spectral_match(&pcm_ultra, &pcm_high, SR);
    let (lsd_low, corr_low) = spectral_match(&pcm_ultra, &pcm_low, SR);

    println!("High vs Ultra: LSD = {:.2} dB, corr = {:.3}", lsd_high, corr_high);
    println!("Low vs Ultra: LSD = {:.2} dB, corr = {:.3}", lsd_low, corr_low);

    // High vs Ultra target: LSD <= 15.0 dB, corr >= 0.98
    assert!(lsd_high <= 15.0, "High vs Ultra LSD ({:.2} dB) exceeds budget", lsd_high);
    assert!(corr_high >= 0.98, "High vs Ultra corr ({:.3}) below budget", corr_high);

    // Low vs Ultra target: LSD <= 28.0 dB, corr >= 0.94
    assert!(lsd_low <= 28.0, "Low vs Ultra LSD ({:.2} dB) exceeds budget", lsd_low);
    assert!(corr_low >= 0.94, "Low vs Ultra corr ({:.3}) below budget", corr_low);
}
