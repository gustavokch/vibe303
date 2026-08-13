use plugin303::dsp::sequencer::{Pattern, PitchEntry, SeqEvent, Sequencer, TimeEntry, Timebase};

#[test]
fn test_data_model_compiles() {
    let _pattern = Pattern {
        pitches: [PitchEntry::default(); 16],
        timing: [TimeEntry::Rest; 16],
        num_steps: 16,
    };
}

#[test]
fn test_sequencer_drift_free() {
    let mut seq = Sequencer::new(48000.0);
    seq.set_running(true);
    let mut pat = Pattern {
        pitches: [PitchEntry::default(); 16],
        timing: [TimeEntry::Rest; 16],
        num_steps: 16,
    };
    pat.timing[0] = TimeEntry::Note;

    let mut events = [(0, SeqEvent::GateOff); 64];
    let mut count = 0;

    // 16th note at 120 BPM @ 48000 Hz: (48000 * 60) / (120 * 4) = 6000 samples.
    seq.process_block(&pat, 0, 6000, &mut events, &mut count, 120.0, Timebase::Straight, false);
    assert_eq!(count, 2); // Trigger + GateOff

    if let SeqEvent::Trigger { .. } = events[0].1 {
        assert_eq!(events[0].0, 0); // Trigger at sample 0
    } else {
        panic!("Expected Trigger at index 0, got {:?}", events[0].1);
    }

    if let SeqEvent::GateOff = events[1].1 {
        assert_eq!(events[1].0, 3000); // 50% duty = 3000 samples
    } else {
        panic!("Expected GateOff at index 1, got {:?}", events[1].1);
    }
}

#[test]
fn test_gate_duty_triplet() {
    let mut seq = Sequencer::new(48000.0);
    seq.set_running(true);
    let mut pat = Pattern {
        pitches: [PitchEntry::default(); 16],
        timing: [TimeEntry::Rest; 16],
        num_steps: 16,
    };
    pat.timing[0] = TimeEntry::Note;

    let mut events = [(0, SeqEvent::GateOff); 64];
    let mut count = 0;

    // Triplet step duration: 48000 * 60 / (120 * 8/3) = 9000 samples.
    // Gate duration: 5/8 of 9000 = 5625 samples.
    seq.process_block(&pat, 0, 9000, &mut events, &mut count, 120.0, Timebase::Triplet, false);
    assert_eq!(count, 2);

    if let SeqEvent::Trigger { .. } = events[0].1 {
        assert_eq!(events[0].0, 0);
    } else {
        panic!("Expected Trigger");
    }

    if let SeqEvent::GateOff = events[1].1 {
        assert_eq!(events[1].0, 5625); // 62.5% of 9000 = 5625
    } else {
        panic!("Expected GateOff at 5625, got {:?}", events[1].0);
    }
}

#[test]
fn test_decoupled_pitch_memory() {
    let mut seq = Sequencer::new(48000.0);
    seq.set_running(true);
    let mut pat = Pattern {
        pitches: [PitchEntry::default(); 16],
        timing: [TimeEntry::Rest; 16],
        num_steps: 4,
    };
    // Pitches: 0 (C), 2 (D), 4 (E), 5 (F)
    pat.pitches[0].key = 0;
    pat.pitches[1].key = 2;
    pat.pitches[2].key = 4;
    pat.pitches[3].key = 5;

    // Timing: Note, Note, Rest, Note -> Should play pitches 0, 2, (rest), 4
    pat.timing[0] = TimeEntry::Note;
    pat.timing[1] = TimeEntry::Note;
    pat.timing[2] = TimeEntry::Rest;
    pat.timing[3] = TimeEntry::Note;

    let mut events = [(0, SeqEvent::GateOff); 64];
    let mut count = 0;

    // 4 steps of 6000 samples each = 24000 samples
    seq.process_block(&pat, 0, 24000, &mut events, &mut count, 120.0, Timebase::Straight, false);

    let triggered_keys: Vec<i8> = events[..count]
        .iter()
        .filter_map(|(_, ev)| match ev {
            SeqEvent::Trigger { key, .. } | SeqEvent::SlideTo { key, .. } => Some(*key),
            _ => None,
        })
        .collect();

    assert_eq!(triggered_keys, vec![0, 2, 4]); // Pitch 2 was not skipped by the Rest!
}

#[test]
fn test_slide_lookahead() {
    let mut seq = Sequencer::new(48000.0);
    seq.set_running(true);
    let mut pat = Pattern {
        pitches: [PitchEntry::default(); 16],
        timing: [TimeEntry::Rest; 16],
        num_steps: 4,
    };
    pat.pitches[0].key = 0;
    pat.pitches[0].slide = true; // Note 0 slides into Note 1
    pat.pitches[1].key = 2;
    pat.pitches[1].slide = true; // Note 1 slides into Step 2 (Rest)

    pat.timing[0] = TimeEntry::Note;
    pat.timing[1] = TimeEntry::Note;
    pat.timing[2] = TimeEntry::Rest;
    pat.timing[3] = TimeEntry::Rest;

    let mut events = [(0, SeqEvent::GateOff); 64];
    let mut count = 0;

    seq.process_block(&pat, 0, 24000, &mut events, &mut count, 120.0, Timebase::Straight, false);

    // Step 0: Trigger(0) at sample 0, NO GateOff scheduled because it slides into Note 1.
    // Step 1: SlideTo(2) at sample 6000. It has slide=true, but next step is Rest, so it DOES schedule GateOff at 6000 + 3000 = 9000!
    assert_eq!(count, 3);
    assert_eq!(events[0], (0, SeqEvent::Trigger { key: 0, octave: 0, accent: false }));
    assert_eq!(events[1], (6000, SeqEvent::SlideTo { key: 2, octave: 0, accent: false }));
    assert_eq!(events[2], (9000, SeqEvent::GateOff));
}
