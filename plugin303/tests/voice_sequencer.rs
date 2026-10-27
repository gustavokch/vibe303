use plugin303::dsp::sequencer::SeqEvent;
use plugin303::dsp::voice::Voice;
use plugin303::params::{CircuitTopology, CompTolerance, OscWave, VcaEnvMode, VcfTaps};

#[test]
fn test_voice_seq_events() {
    let mut voice = Voice::new(48000.0);

    // Trigger with accent
    voice.on_seq_event(
        SeqEvent::Trigger {
            key: 0,
            octave: 0,
            accent: true,
        },
        1.0,
    );
    assert!(voice.gate);
    // Charge pump integration adds (1.0 - accent_acc) * 0.8 => 0.8
    assert!((voice.get_accent_acc() - 0.8).abs() < 1e-4);

    // SlideTo (should slew pitch without retriggering envelopes)
    voice.on_seq_event(
        SeqEvent::SlideTo {
            key: 12,
            octave: 0,
            accent: false,
        },
        1.0,
    );
    assert!(voice.gate);
    assert!((voice.get_accent_acc() - 0.8).abs() < 1e-4); // No new accent addition
}

#[test]
fn test_voice_constant_time_slew() {
    let mut voice = Voice::new(48000.0);
    // Trigger at key 0 (pitch 60)
    voice.on_seq_event(
        SeqEvent::Trigger {
            key: 0,
            octave: 0,
            accent: false,
        },
        0.0,
    );
    assert_eq!(voice.get_current_pitch(), 60.0);

    // Slide to key 12 (pitch 72 = 1 octave up)
    voice.on_seq_event(
        SeqEvent::SlideTo {
            key: 12,
            octave: 0,
            accent: false,
        },
        0.0,
    );

    // After 22 ms (tau) = 0.022 * 48000 = 1056 samples, pitch should have reached ~63.2% of the step (60 + 12 * 0.632 = ~67.58)
    for _ in 0..1056 {
        voice.tick();
    }
    let pitch_after_tau = voice.get_current_pitch();
    let expected_63 = 60.0 + 12.0 * (1.0 - (-1.0f32).exp());
    assert!(
        (pitch_after_tau - expected_63).abs() < 0.25,
        "Expected pitch ~{}, got {}",
        expected_63,
        pitch_after_tau
    );

    // After ~110 ms (5 * tau) = 5280 samples, pitch should reach > 99% of 72.0
    for _ in 1056..5280 {
        voice.tick();
    }
    let pitch_after_5tau = voice.get_current_pitch();
    assert!(
        (pitch_after_5tau - 72.0).abs() < 0.15,
        "Expected pitch ~72.0, got {}",
        pitch_after_5tau
    );
}

/// Classic-envelope voice at DECAY knob `decay`, set up the way
/// `Plugin303::process` does at the start of a block.
fn classic_voice(decay: f32) -> Voice {
    let mut v = Voice::new(48000.0);
    v.update_params(
        0.0,
        0.0,
        0.0,
        0.0,
        OscWave::Saw,
        0.5,
        0.0,
        0.4,
        0.5,
        0.8,
        decay,
        0.5,
        0.0,
        VcfTaps::FourPole,
        0.0,
        false,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        VcaEnvMode::Envelope,
        0,
        0.0,
        CompTolerance::Zero,
        0,
        CircuitTopology::Accurate,
        0.0,
        0,
        0.5,
        0.0,
    );
    v
}

fn render(v: &mut Voice, n: usize) -> Vec<f32> {
    (0..n).map(|_| v.tick()).collect()
}

fn assert_same_audio(a: &[f32], b: &[f32], what: &str, decay: f32) {
    assert!(
        a.iter().any(|s| s.abs() > 1e-3),
        "{what}: DECAY {decay}: render is silent, comparison would prove nothing"
    );
    if let Some(i) = a.iter().zip(b).position(|(x, y)| x != y) {
        panic!(
            "{what}: DECAY {decay}: sequencer and MIDI audio differ at sample {i} ({} vs {})",
            a[i], b[i]
        );
    }
}

/// A sequencer-triggered unaccented note and a MIDI-triggered one (the
/// `set_gate` + `note_on` pair `Plugin303::process` runs for `NoteOn`) set the
/// same filter-envelope decay at the same DECAY knob position, so both render
/// the same audio. Half a second covers the whole mid-range decay.
#[test]
fn seq_trigger_uses_the_midi_decay_curve() {
    for decay in [0.2, 0.5, 0.9] {
        let mut seq = classic_voice(decay);
        let mut midi = classic_voice(decay);

        seq.on_seq_event(
            SeqEvent::Trigger {
                key: 0,
                octave: 0,
                accent: false,
            },
            0.5,
        );
        midi.set_gate(true, true, false, false);
        midi.note_on(60);

        assert_same_audio(
            &render(&mut seq, 24_000),
            &render(&mut midi, 24_000),
            "Trigger",
            decay,
        );
    }
}

/// An unaccented `SlideTo` re-arms the filter-envelope decay of the note that
/// is already sounding. It uses the same curve as a legato MIDI note without a
/// retrigger (`set_gate(.., trigger = false, ..)`).
#[test]
fn seq_slide_uses_the_midi_decay_curve() {
    for decay in [0.2, 0.5, 0.9] {
        let mut seq = classic_voice(decay);
        let mut midi = classic_voice(decay);

        // Accented trigger first: accents lock the filter-envelope decay to
        // 200 ms, so the unaccented slide has to move it to the DECAY curve.
        let accented = SeqEvent::Trigger {
            key: 0,
            octave: 0,
            accent: true,
        };
        seq.on_seq_event(accented, 0.5);
        midi.on_seq_event(accented, 0.5);
        let mut a = render(&mut seq, 2_400);
        let mut b = render(&mut midi, 2_400);

        seq.on_seq_event(
            SeqEvent::SlideTo {
                key: 0,
                octave: 0,
                accent: false,
            },
            0.5,
        );
        midi.set_gate(true, false, false, true);
        a.extend(render(&mut seq, 24_000));
        b.extend(render(&mut midi, 24_000));

        assert_same_audio(&a, &b, "SlideTo", decay);
    }
}
