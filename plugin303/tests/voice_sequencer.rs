use plugin303::dsp::sequencer::SeqEvent;
use plugin303::dsp::voice::Voice;

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
        500.0,
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
        500.0,
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
        500.0,
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
        500.0,
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
