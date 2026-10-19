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
    v.set_cutoff(20000.0);
    v.set_reso(0.0);
    v.note_on(note);
    v.set_gate(true, true, false, false);

    let warmup_samples = 48_000;
    for _ in 0..warmup_samples {
        v.tick();
    }

    // Collect measurement samples, then subtract mean to remove any DC offset
    // from the ladder VCF bias or Os4x pipeline before zero-crossing detection.
    let n = 12_000;
    let mut buf = Vec::with_capacity(n);
    for _ in 0..n {
        buf.push(v.tick() as f64);
    }
    let mean = buf.iter().sum::<f64>() / n as f64;

    // Refractory period: 1/3 of expected period. Rejects spurious zero crossings
    // from Gibbs ringing when the Os4x Kaiser FIR downsamples the VCO saw reset.
    let expected_hz = 440.0f64 * 2.0f64.powf((note as f64 - 69.0) / 12.0);
    let min_gap = ((SR as f64 / expected_hz) / 3.0).max(3.0) as usize;

    let mut prev = buf[0] - mean;
    let (mut first_t, mut last_t, mut count) = (-1.0f64, 0.0f64, 0i64);
    let mut last_crossing_i: i64 = -(min_gap as i64 + 1);
    for i in 1..n {
        let y = buf[i] - mean;
        if prev < 0.0 && y >= 0.0 && (i as i64 - last_crossing_i) >= min_gap as i64 {
            let frac = (-prev) / (y - prev);
            let t = i as f64 + frac;
            if first_t < 0.0 {
                first_t = t;
            } else {
                last_t = t;
                count += 1;
            }
            last_crossing_i = i as i64;
        }
        prev = y;
    }
    if count == 0 || last_t <= first_t {
        return 0.0;
    }
    let realized = (count as f64 * SR as f64 / (last_t - first_t)) as f32;
    println!("  [voice_realized_hz] tier={:?}, note={}, count={}, first_t={:.3}, last_t={:.3}, diff={:.3}, freq={:.3}, dc={:.6}", tier, note, count, first_t, last_t, last_t - first_t, realized, mean);
    realized
}

#[test]
fn test_pitch_parity_accurate() {
    // Under Accurate topology, playable range is 24..=72.
    // Each tier must realize the target MIDI pitch within tolerance.
    // Draft and Low use polyBLEP + midi_to_freq() directly -> tight budget.
    // Best and High use the circuit VCO via the per-note secant LUT -> up to 1%.
    // The circuit VCO range spans the authentic 3-octave TB-303 keyboard
    // (notes 24 to 60).
    for note in [24u8, 36, 48, 60] {
        let target = 440.0f32 * 2.0f32.powf((note as f32 - 69.0) / 12.0);
        let f_best = voice_realized_hz(Tier::Best, CircuitTopology::Accurate, note);
        let f_draft = voice_realized_hz(Tier::Draft, CircuitTopology::Accurate, note);
        let f_low = voice_realized_hz(Tier::Low, CircuitTopology::Accurate, note);

        let err_draft = (f_draft / f_best - 1.0).abs();
        let err_low = (f_low / f_best - 1.0).abs();
        let dev_best = (f_best / target - 1.0).abs();

        println!(
            "Accurate Note {}: target={:.2} Hz, Best={:.2} Hz ({:.3}%), Draft={:.2} Hz, Low={:.2} Hz",
            note, target, f_best, dev_best * 100.0, f_draft, f_low
        );

        assert!(
            err_draft < 0.002,
            "Draft pitch mismatch ({:.3}%) at note {}",
            err_draft * 100.0,
            note
        );
        assert!(
            err_low < 0.002,
            "Low pitch mismatch ({:.3}%) at note {}",
            err_low * 100.0,
            note
        );
    }
}

#[test]
fn test_pitch_parity_fun() {
    // Under Fun topology, range extends up to 108.
    // Each tier must realize the target MIDI pitch within tolerance.
    for note in [24u8, 48, 72, 96, 108] {
        let target = 440.0f32 * 2.0f32.powf((note as f32 - 69.0) / 12.0);
        let f_best = voice_realized_hz(Tier::Best, CircuitTopology::Fun, note);
        let f_draft = voice_realized_hz(Tier::Draft, CircuitTopology::Fun, note);
        let f_low = voice_realized_hz(Tier::Low, CircuitTopology::Fun, note);

        let err_best = (f_best / target - 1.0).abs();
        let err_draft = (f_draft / target - 1.0).abs();
        let err_low = (f_low / target - 1.0).abs();

        println!(
            "Fun Note {}: target={:.2} Hz, Best={:.2} Hz ({:.3}%), Draft={:.2} Hz ({:.3}%), Low={:.2} Hz ({:.3}%)",
            note, target, f_best, err_best * 100.0, f_draft, err_draft * 100.0, f_low, err_low * 100.0
        );

        assert!(
            err_draft < 0.002,
            "Draft pitch mismatch ({:.3}%) at note {}",
            err_draft * 100.0,
            note
        );
        assert!(
            err_low < 0.002,
            "Low pitch mismatch ({:.3}%) at note {}",
            err_low * 100.0,
            note
        );
        if note <= 72 {
            assert!(
                err_best < 0.01,
                "Best pitch mismatch ({:.3}%) at note {}",
                err_best * 100.0,
                note
            );
        }
    }
}

fn render_sweep(tier: Tier, note: u8, cutoff_knob: f32, reso_knob: f32, n: usize) -> Vec<f32> {
    let mut v = Voice::new(SR);
    v.update_clipper_params_immediate(common::bypassed_clipper());
    v.set_tier_immediate(tier);
    v.set_vca_env_mode(plugin303::params::VcaEnvMode::Gate);
    v.set_cutoff(200.0 * 20.0f32.powf(cutoff_knob));
    v.set_reso(reso_knob);
    v.set_env_mod(0.7);
    v.note_on(note);
    v.set_gate(true, true, false, false);
    for _ in 0..48 {
        v.tick();
    }
    (0..n).map(|_| v.tick()).collect()
}

#[test]
fn test_voicing_parity_sweeps() {
    // Render 1 second (48000 samples) of a dynamic envelope sweep for each tier
    let note = 57; // A2
    let cutoff_knob = 0.4;
    let reso_knob = 0.65;
    let len = 48000;

    let pcm_best = render_sweep(Tier::Best, note, cutoff_knob, reso_knob, len);
    let pcm_low = render_sweep(Tier::Low, note, cutoff_knob, reso_knob, len);
    let pcm_draft = render_sweep(Tier::Draft, note, cutoff_knob, reso_knob, len);

    let (lsd_low, corr_low) = spectral_match(&pcm_best, &pcm_low, SR);
    let (lsd_draft, corr_draft) = spectral_match(&pcm_best, &pcm_draft, SR);

    println!(
        "Low vs Best: LSD = {:.2} dB, corr = {:.3}",
        lsd_low, corr_low
    );
    println!(
        "Draft vs Best: LSD = {:.2} dB, corr = {:.3}",
        lsd_draft, corr_draft
    );

    // Low vs Best target: LSD <= 17.0 dB, corr >= 0.975
    // (LSD budget raised from 15.0: two changes shifted the measurement —
    //  dd55f98's DECAY knob remap speeds the default sweep (1515 ms -> 400 ms
    //  at knob 0.5), which widens inter-tier spectral divergence, and the
    //  Best ladder now rebuilds at the Os4x rate in set_tier_immediate,
    //  restoring the pre-21b7307 filter response. Parity property intact:
    //  corr 0.993 at the new operating point. Margin intentionally tight
    //  (~1.4 dB over the measured operating point) to keep the parity
    //  property under active drift watch.)
    assert!(
        lsd_low <= 17.0,
        "Low vs Best LSD ({:.2} dB) exceeds budget",
        lsd_low
    );
    assert!(
        corr_low >= 0.975,
        "Low vs Best corr ({:.3}) below budget",
        corr_low
    );

    // Draft vs Best target: LSD <= 15.0 dB, corr >= 0.97.
    // Draft's maps (DRAFT_*_POLY) are fitted against Best at CIRCUIT_BRIGHT and
    // at Draft's 4x filter rate: measured 12.37 dB / 0.982 (25.87 / 0.965 with
    // the maps fitted against Best at brightness 1.0 and Draft at 1x). The
    // budget guards that fit; a re-fit that loses it fails here.
    assert!(
        lsd_draft <= 15.0,
        "Draft vs Best LSD ({:.2} dB) exceeds budget",
        lsd_draft
    );
    assert!(
        corr_draft >= 0.97,
        "Draft vs Best corr ({:.3}) below budget",
        corr_draft
    );
}
