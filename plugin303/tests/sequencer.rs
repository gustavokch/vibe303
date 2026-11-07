use plugin303::dsp::sequencer::{Pattern, PitchEntry, SeqEvent, Sequencer, TimeEntry, Timebase};

#[test]
fn test_data_model_compiles() {
    let _pattern = Pattern {
        pitches: [PitchEntry::default(); 16],
        timing: [TimeEntry::Rest; 16],
        num_steps: 16,
        decoupled: false,
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
        decoupled: false,
    };
    pat.timing[0] = TimeEntry::Note;

    let mut events = [(0, SeqEvent::GateOff); 64];
    let mut count = 0;

    // 16th note at 120 BPM @ 48000 Hz: (48000 * 60) / (120 * 4) = 6000 samples.
    seq.process_block(
        &pat,
        0,
        6000,
        &mut events,
        &mut count,
        120.0,
        Timebase::Straight,
        false,
    );
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
fn test_triplet_grid_and_gate_duty() {
    let mut seq = Sequencer::new(48000.0);
    seq.set_running(true);
    let mut pat = Pattern {
        pitches: [PitchEntry::default(); 16],
        timing: [TimeEntry::Rest; 16],
        num_steps: 16,
        decoupled: false,
    };
    pat.timing[..4].fill(TimeEntry::Note);

    let mut events = [(0, SeqEvent::GateOff); 64];
    let mut count = 0;

    // Triplet (12th) step: 8 of 24 ticks per quarter note, 3 steps per beat.
    // 120 BPM at 48 kHz: a beat is 24000 samples, a step 8000, the gate 5/8 of
    // a step (5000). The 4th step lands on beat 2.
    seq.process_block(
        &pat,
        0,
        24001,
        &mut events,
        &mut count,
        120.0,
        Timebase::Triplet,
        false,
    );

    let trigger = SeqEvent::Trigger {
        key: 0,
        octave: 0,
        accent: false,
    };
    assert_eq!(
        &events[..count],
        &[
            (0, trigger),
            (5000, SeqEvent::GateOff),
            (8000, trigger),
            (13000, SeqEvent::GateOff),
            (16000, trigger),
            (21000, SeqEvent::GateOff),
            (24000, trigger),
        ]
    );
}

#[test]
fn test_decoupled_pitch_memory() {
    let mut seq = Sequencer::new(48000.0);
    seq.set_running(true);
    let mut pat = Pattern {
        pitches: [PitchEntry::default(); 16],
        timing: [TimeEntry::Rest; 16],
        num_steps: 4,
        decoupled: true,
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
    seq.process_block(
        &pat,
        0,
        24000,
        &mut events,
        &mut count,
        120.0,
        Timebase::Straight,
        false,
    );

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
        decoupled: false,
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

    seq.process_block(
        &pat,
        0,
        24000,
        &mut events,
        &mut count,
        120.0,
        Timebase::Straight,
        false,
    );

    // Step 0: Trigger(0) at sample 0, NO GateOff scheduled because it slides into Note 1.
    // Step 1: SlideTo(2) at sample 6000. It has slide=true, but next step is Rest, so it DOES schedule GateOff at 6000 + 3000 = 9000!
    assert_eq!(count, 3);
    assert_eq!(
        events[0],
        (
            0,
            SeqEvent::Trigger {
                key: 0,
                octave: 0,
                accent: false
            }
        )
    );
    assert_eq!(
        events[1],
        (
            6000,
            SeqEvent::SlideTo {
                key: 2,
                octave: 0,
                accent: false
            }
        )
    );
    assert_eq!(events[2], (9000, SeqEvent::GateOff));
}

#[test]
fn test_sequencer_direct_vs_decoupled_mode() {
    use plugin303::params::{SequencerMode, SequencerPattern, StepGate};

    let mut pat = SequencerPattern::default();
    // Step 0: Note C (semitone 0)
    pat.steps[0].gate = StepGate::Note;
    pat.steps[0].semitone = 0;
    // Step 1: Rest
    pat.steps[1].gate = StepGate::Rest;
    // Step 2: Note D (semitone 2)
    pat.steps[2].gate = StepGate::Note;
    pat.steps[2].semitone = 2;
    // Step 3: Rest
    pat.steps[3].gate = StepGate::Rest;

    // Direct Grid mode
    let compiled_direct = pat.compile(4, SequencerMode::DirectGrid);
    assert_eq!(compiled_direct.pitches[0].key, 0);
    assert_eq!(compiled_direct.pitches[2].key, 2);
    assert_eq!(compiled_direct.timing[0], TimeEntry::Note);
    assert_eq!(compiled_direct.timing[1], TimeEntry::Rest);
    assert_eq!(compiled_direct.timing[2], TimeEntry::Note);

    // Decoupled Hardware mode
    let compiled_decoupled = pat.compile(4, SequencerMode::DecoupledHardware);
    assert_eq!(compiled_decoupled.pitches[0].key, 0);
    assert_eq!(compiled_decoupled.pitches[1].key, 2); // note pitches packed sequentially
    assert_eq!(compiled_decoupled.timing[0], TimeEntry::Note);
    assert_eq!(compiled_decoupled.timing[1], TimeEntry::Rest);
    assert_eq!(compiled_decoupled.timing[2], TimeEntry::Note);
}

#[test]
fn test_sequencer_multiblock_chunked_processing() {
    let mut seq = Sequencer::new(48000.0);
    seq.set_running(true);
    let mut pat = Pattern {
        pitches: [PitchEntry::default(); 16],
        timing: [TimeEntry::Rest; 16],
        num_steps: 4,
        decoupled: false,
    };
    pat.timing[0] = TimeEntry::Note;
    pat.timing[1] = TimeEntry::Note;
    pat.timing[2] = TimeEntry::Note;
    pat.timing[3] = TimeEntry::Note;

    let chunk_size = 512;
    let total_samples = 24000; // 4 steps at 120 bpm = 0.5s = 24000 samples (6000 samples per step)
    let mut current_sample: u64 = 0;
    let mut all_events = Vec::new();

    while current_sample < total_samples {
        let mut events = [(0, SeqEvent::GateOff); 64];
        let mut count = 0;
        seq.process_block(
            &pat,
            current_sample,
            chunk_size,
            &mut events,
            &mut count,
            120.0,
            Timebase::Straight,
            false,
        );

        for i in 0..count {
            let (offset, ev) = events[i];
            let sample_time = current_sample + offset as u64;
            if sample_time < total_samples {
                all_events.push((sample_time, ev));
            }
        }
        current_sample += chunk_size as u64;
    }

    // 4 notes: 4 triggers + 4 gate offs = 8 events within 24000 samples
    assert_eq!(all_events.len(), 8);
    assert_eq!(all_events[0].0, 0); // Step 0 Trigger
    assert_eq!(all_events[1].0, 3000); // Step 0 GateOff (50% duty = 3000 samples)
    assert_eq!(all_events[2].0, 6000); // Step 1 Trigger
    assert_eq!(all_events[3].0, 9000); // Step 1 GateOff
    assert_eq!(all_events[4].0, 12000); // Step 2 Trigger
    assert_eq!(all_events[5].0, 15000); // Step 2 GateOff
    assert_eq!(all_events[6].0, 18000); // Step 3 Trigger
    assert_eq!(all_events[7].0, 21000); // Step 3 GateOff
}

#[test]
fn test_sequencer_transport_jump_and_loop_sync() {
    let mut seq = Sequencer::new(48000.0);
    seq.set_running(true);

    let pat = Pattern {
        pitches: [PitchEntry::default(); 16],
        timing: [TimeEntry::Note; 16],
        num_steps: 16,
        decoupled: false,
    };

    // 120 bpm, straight timebase => 6000 samples per step.
    // Jump to 9000 samples (Step 1, exactly halfway through step).
    seq.sync_to_transport(9000, 0, 120.0, Timebase::Straight, 16, Some(&pat));

    let mut events = [(0, SeqEvent::GateOff); 64];
    let mut count = 0;
    // Process block 1 from 0 to 4000. Next step (Step 2) is at sample 3000 relative to jump point.
    seq.process_block(
        &pat,
        0,
        4000,
        &mut events,
        &mut count,
        120.0,
        Timebase::Straight,
        false,
    );

    assert_eq!(count, 1); // Step 2 Trigger at 3000
    assert_eq!(events[0].0, 3000);

    // Process block 2 from 4000 to 8000. GateOff for Step 2 is at 6000 (offset 2000).
    // Step 3 Trigger is at 9000 (outside this block).
    let mut events2 = [(0, SeqEvent::GateOff); 64];
    let mut count2 = 0;
    seq.process_block(
        &pat,
        4000,
        4000,
        &mut events2,
        &mut count2,
        120.0,
        Timebase::Straight,
        false,
    );

    assert_eq!(count2, 1);
    assert_eq!(events2[0].0, 2000);
    assert_eq!(events2[0].1, SeqEvent::GateOff);
}

#[test]
fn test_gate_off_quantization_boundary() {
    let mut seq = Sequencer::new(48000.0);
    seq.set_running(true);
    let mut pat = Pattern {
        pitches: [PitchEntry::default(); 16],
        timing: [TimeEntry::Rest; 16],
        num_steps: 16,
        decoupled: false,
    };
    pat.timing[0] = TimeEntry::Note;

    // Process block 1 from 0 to 2900 (GateOff is at 3000, outside block 1)
    let mut events1 = [(0, SeqEvent::GateOff); 64];
    let mut count1 = 0;
    seq.process_block(
        &pat,
        0,
        2900,
        &mut events1,
        &mut count1,
        120.0,
        Timebase::Straight,
        true, // hw_jitter enabled
    );
    assert_eq!(count1, 1); // Note 0 Trigger

    // Process block 2 from 2900 to 6000 (GateOff will land inside block 2)
    let mut events2 = [(0, SeqEvent::GateOff); 64];
    let mut count2 = 0;
    seq.process_block(
        &pat,
        2900,
        3100,
        &mut events2,
        &mut count2,
        120.0,
        Timebase::Straight,
        true,
    );
    assert_eq!(count2, 1);
    assert_eq!(events2[0].1, SeqEvent::GateOff);
}

#[test]
fn test_decoupled_pitch_transport_sync() {
    let mut seq = Sequencer::new(48000.0);
    seq.set_running(true);

    let mut pat = Pattern {
        pitches: [PitchEntry::default(); 16],
        timing: [TimeEntry::Rest; 16],
        num_steps: 4,
        decoupled: true,
    };
    // Pitches: 0 (C), 2 (D), 4 (E), 5 (F)
    pat.pitches[0].key = 0;
    pat.pitches[1].key = 2;
    pat.pitches[2].key = 4;
    pat.pitches[3].key = 5;

    // Timing: Step 0 = Note (Pitch 0: key 0), Step 1 = Rest, Step 2 = Note (Pitch 1: key 2), Step 3 = Note (Pitch 2: key 4)
    pat.timing[0] = TimeEntry::Note;
    pat.timing[1] = TimeEntry::Rest;
    pat.timing[2] = TimeEntry::Note;
    pat.timing[3] = TimeEntry::Note;

    // Seek transport directly to Step 2 (sample 12000 at 120 bpm straight)
    seq.sync_to_transport(12000, 0, 120.0, Timebase::Straight, 4, Some(&pat));

    let mut events = [(0, SeqEvent::GateOff); 64];
    let mut count = 0;
    seq.process_block(
        &pat,
        0,
        6000,
        &mut events,
        &mut count,
        120.0,
        Timebase::Straight,
        false,
    );

    assert_eq!(count, 2); // Step 2 Trigger + GateOff
    if let SeqEvent::Trigger { key, .. } = events[0].1 {
        assert_eq!(key, 2); // Correct pitch 1 (key 2) after Rest step 1!
    } else {
        panic!("Expected Trigger event");
    }
}

/// Plays `pat` at 120 BPM against a host transport that starts at sample 0 and
/// runs in `block`-sample blocks (the plugin timeline matches the host's) until
/// `end`. `rate_at` gives the RATE for the block that starts at a host position.
/// Returns every event at its absolute sample.
fn play_synced(
    pat: &Pattern,
    block: u32,
    end: u64,
    rate_at: impl Fn(u64) -> Timebase,
) -> Vec<(u64, SeqEvent)> {
    let mut seq = Sequencer::new(48000.0);
    seq.set_running(true);
    let mut played = Vec::new();
    let mut pos = 0;
    while pos < end {
        let timebase = rate_at(pos);
        seq.follow_transport(
            pos as i64,
            pos,
            block,
            120.0,
            timebase,
            pat.num_steps,
            Some(pat),
        );
        let mut events = [(0, SeqEvent::GateOff); 64];
        let mut count = 0;
        seq.process_block(
            pat,
            pos,
            block,
            &mut events,
            &mut count,
            120.0,
            timebase,
            false,
        );
        played.extend(
            events[..count]
                .iter()
                .map(|&(at, ev)| (pos + at as u64, ev)),
        );
        pos += block as u64;
    }
    played
}

const TRIGGER: SeqEvent = SeqEvent::Trigger {
    key: 0,
    octave: 0,
    accent: false,
};

#[test]
fn test_rate_change_with_sync_moves_steps_to_the_host_grid() {
    let pat = Pattern {
        pitches: [PitchEntry::default(); 16],
        timing: [TimeEntry::Note; 16],
        num_steps: 16,
        decoupled: false,
    };
    // 16th steps every 6000 samples until RATE turns 12th at host sample 8000,
    // then 12th steps on the host's 8000-sample grid. The 16th step from 6000
    // still holds its gate (off at 9000): it goes off where the next step
    // starts, ahead of that step's trigger.
    let played = play_synced(&pat, 2000, 26000, |pos| {
        if pos < 8000 {
            Timebase::Straight
        } else {
            Timebase::Triplet
        }
    });
    assert_eq!(
        played,
        [
            (0, TRIGGER),
            (3000, SeqEvent::GateOff),
            (6000, TRIGGER),
            (8000, SeqEvent::GateOff),
            (8000, TRIGGER),
            (13000, SeqEvent::GateOff),
            (16000, TRIGGER),
            (21000, SeqEvent::GateOff),
            (24000, TRIGGER),
        ]
    );
}

#[test]
fn test_rate_change_with_sync_ends_a_held_slide() {
    let mut pat = Pattern {
        pitches: [PitchEntry::default(); 16],
        timing: [TimeEntry::Rest; 16],
        num_steps: 16,
        decoupled: false,
    };
    pat.timing[5] = TimeEntry::Note;
    pat.pitches[5].slide = true;
    pat.timing[6] = TimeEntry::Note;
    // 16th step 5 (sample 30000) slides into step 6, so it holds its gate. RATE
    // turns 12th at 31000, which re-places the grid: the next step is 12th
    // step 4 (a rest) at 32000, so the slide ends there. 12th step 5 plays at
    // 40000.
    let played = play_synced(&pat, 1000, 41000, |pos| {
        if pos < 31000 {
            Timebase::Straight
        } else {
            Timebase::Triplet
        }
    });
    assert_eq!(
        played,
        [
            (30000, TRIGGER),
            (32000, SeqEvent::GateOff),
            (40000, TRIGGER),
        ]
    );
}
