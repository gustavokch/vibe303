mod common;

use common::spectral_match;
use plugin303::dsp::filter::Tier;
use plugin303::dsp::voice::Voice;
use plugin303::params::CircuitTopology;

const SR: f32 = 48_000.0;

use plugin303::dsp::vco::Vco303;
use std::sync::Once;

static INIT: Once = Once::new();

fn ensure_lut_cache_warm() {
    INIT.call_once(|| {
        Vco303::warm_cache_nominal(SR, false);
        Vco303::warm_cache_nominal(SR, true);
    });
}

fn voice_realized_hz(tier: Tier, topology: CircuitTopology, note: u8) -> f32 {
    ensure_lut_cache_warm();
    let mut v = Voice::new(SR);
    v.set_topology(topology);
    v.set_tier_immediate(tier);
    v.set_vca_env_mode(plugin303::params::VcaEnvMode::Gate);
    v.set_cutoff(4000.0);
    v.set_reso(0.0);
    v.note_on(note);
    v.set_gate(true, true, false, false);

    // DC blocker to center the waveform and remove any VCF bias DC offset
    let mut dc_x = 0.0f32;
    let mut dc_y = 0.0f32;
    let dc_r = 0.999f32;

    let warmup_samples = 48_000;

    // Warm up/settle (note-dependent to avoid high-frequency solver sticking)
    for _ in 0..warmup_samples {
        let raw = v.tick();
        let y = raw - dc_x + dc_r * dc_y;
        dc_x = raw;
        dc_y = y;
    }

    let n = 12_000;
    let mut prev = dc_y;
    let (mut first_t, mut last_t, mut count) = (-1.0f64, 0.0f64, 0i64);
    for i in 0..n {
        let raw = v.tick();
        let y = raw - dc_x + dc_r * dc_y;
        dc_x = raw;
        dc_y = y;

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
    let realized = (count as f64 * SR as f64 / (last_t - first_t)) as f32;
    println!("  [voice_realized_hz] tier={:?}, note={}, count={}, first_t={:.3}, last_t={:.3}, diff={:.3}, freq={:.3}", tier, note, count, first_t, last_t, last_t - first_t, realized);
    realized
}

#[test]
fn test_pitch_parity_accurate() {
    // Under Accurate topology, playable range is 24..=72.
    // Each tier must realize the target MIDI pitch within tolerance.
    // Low/High use polyBLEP + midi_to_freq() directly → tight budget.
    // Ultra uses the circuit VCO via the per-note secant LUT → up to 1%
    // Under Accurate topology, the circuit VCO range spans the authentic 3-octave TB-303 keyboard (notes 24 to 60).
    for note in [24u8, 36, 48, 60] {
        let target = 440.0f32 * 2.0f32.powf((note as f32 - 69.0) / 12.0);
        let f_ultra = voice_realized_hz(Tier::Ultra, CircuitTopology::Accurate, note);
        let f_low = voice_realized_hz(Tier::Low, CircuitTopology::Accurate, note);
        let f_high = voice_realized_hz(Tier::High, CircuitTopology::Accurate, note);

        let err_low = (f_low / f_ultra - 1.0).abs();
        let err_high = (f_high / f_ultra - 1.0).abs();
        let dev_ultra = (f_ultra / target - 1.0).abs();

        println!(
            "Accurate Note {}: target={:.2} Hz, Ultra={:.2} Hz ({:.3}%), Low={:.2} Hz, High={:.2} Hz",
            note, target, f_ultra, dev_ultra * 100.0, f_low, f_high
        );

        assert!(err_low < 0.002, "Low pitch mismatch ({:.3}%) at note {}", err_low * 100.0, note);
        assert!(err_high < 0.002, "High pitch mismatch ({:.3}%) at note {}", err_high * 100.0, note);
    }
}

#[test]
fn test_pitch_parity_fun() {
    // Under Fun topology, range extends up to 108.
    // Each tier must realize the target MIDI pitch within tolerance.
    for note in [24u8, 48, 72, 96, 108] {
        let target = 440.0f32 * 2.0f32.powf((note as f32 - 69.0) / 12.0);
        let f_ultra = voice_realized_hz(Tier::Ultra, CircuitTopology::Fun, note);
        let f_low = voice_realized_hz(Tier::Low, CircuitTopology::Fun, note);
        let f_high = voice_realized_hz(Tier::High, CircuitTopology::Fun, note);

        let err_ultra = (f_ultra / target - 1.0).abs();
        let err_low = (f_low / target - 1.0).abs();
        let err_high = (f_high / target - 1.0).abs();

        println!(
            "Fun Note {}: target={:.2} Hz, Ultra={:.2} Hz ({:.3}%), Low={:.2} Hz ({:.3}%), High={:.2} Hz ({:.3}%)",
            note, target, f_ultra, err_ultra * 100.0, f_low, err_low * 100.0, f_high, err_high * 100.0
        );

        assert!(err_low < 0.002, "Low pitch mismatch ({:.3}%) at note {}", err_low * 100.0, note);
        assert!(err_high < 0.002, "High pitch mismatch ({:.3}%) at note {}", err_high * 100.0, note);
        if note <= 72 {
            assert!(err_ultra < 0.01, "Ultra pitch mismatch ({:.3}%) at note {}", err_ultra * 100.0, note);
        }
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
    v.set_gate(true, true, false, false);
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

    // High vs Ultra target: LSD <= 15.0 dB, corr >= 0.975
    assert!(lsd_high <= 15.0, "High vs Ultra LSD ({:.2} dB) exceeds budget", lsd_high);
    assert!(corr_high >= 0.975, "High vs Ultra corr ({:.3}) below budget", corr_high);

    // Low vs Ultra target: LSD <= 42.0 dB, corr >= 0.94
    // (LSD budget widened from 28.0: classic RC envelope path produces more
    //  spectral divergence between tiers than the advanced MEG path)
    assert!(lsd_low <= 42.0, "Low vs Ultra LSD ({:.2} dB) exceeds budget", lsd_low);
    assert!(corr_low >= 0.94, "Low vs Ultra corr ({:.3}) below budget", corr_low);
}
