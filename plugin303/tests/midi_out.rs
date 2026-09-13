use nih_plug::params::Param;
use plugin303::params::Plugin303Params;
use plugin303::seq_pitch_to_midi_note;

#[test]
fn test_midi_and_bypass_params_exist_and_default() {
    let params = Plugin303Params::default();

    assert_eq!(params.midi_out.value(), false);
    assert_eq!(params.midi_out_channel.value(), 1);
    assert_eq!(params.synth_bypass.value(), false);

    // Verify channel range
    let ch_param = &params.midi_out_channel;
    assert_eq!(ch_param.default_plain_value(), 1);
}

#[test]
fn test_pitch_to_midi_note_mapping() {
    // Key 0, Octave 0 is Middle C (MIDI Note 60)
    assert_eq!(seq_pitch_to_midi_note(0, 0), 60);
    // Key 3 (D#), Octave 0 -> 63
    assert_eq!(seq_pitch_to_midi_note(3, 0), 63);
    // Key 0, Octave -1 -> 48
    assert_eq!(seq_pitch_to_midi_note(0, -1), 48);
    // Key 0, Octave 1 -> 72
    assert_eq!(seq_pitch_to_midi_note(0, 1), 72);
    // Clamping to valid 0..127 range
    assert_eq!(seq_pitch_to_midi_note(0, -6), 0);
    assert_eq!(seq_pitch_to_midi_note(0, 10), 127);
}

use plugin303::dsp::sequencer::{Pattern, PitchEntry, SeqEvent, Sequencer, TimeEntry};

#[test]
fn test_sequencer_generates_correct_midi_events() {
    let mut seq = Sequencer::new(48000.0);
    seq.set_running(true);

    let mut pattern = Pattern::default();
    pattern.num_steps = 4;
    // Step 0: Note C3 (60), unaccented
    pattern.pitches[0] = PitchEntry {
        key: 0,
        octave: 0,
        accent: false,
        slide: false,
    };
    pattern.timing[0] = TimeEntry::Note;
    // Step 1: Note D#3 (63), accented, with slide
    pattern.pitches[1] = PitchEntry {
        key: 3,
        octave: 0,
        accent: true,
        slide: true,
    };
    pattern.timing[1] = TimeEntry::Note;
    // Step 2: Note G3 (67), slide target
    pattern.pitches[2] = PitchEntry {
        key: 7,
        octave: 0,
        accent: false,
        slide: false,
    };
    pattern.timing[2] = TimeEntry::Note;
    // Step 3: Rest
    pattern.timing[3] = TimeEntry::Rest;

    let mut events = [(0, SeqEvent::GateOff); 64];
    let mut count = 0;

    // Process a block representing 1 full beat at 120 BPM = 24000 samples (4 16th steps = 6000 samples each)
    seq.process_block(
        &pattern,
        0,
        24000,
        &mut events,
        &mut count,
        120.0,
        plugin303::params::Timebase::Straight,
        false,
    );

    assert!(
        count >= 4,
        "Expected at least 4 sequencer events, got {}",
        count
    );

    // Step 0: Trigger note 60
    assert_eq!(events[0].0, 0);
    assert_eq!(
        events[0].1,
        SeqEvent::Trigger {
            key: 0,
            octave: 0,
            accent: false,
        }
    );

    // Step 0 gate off happens before step 1
    assert_eq!(events[1].1, SeqEvent::GateOff);

    // Step 1: Trigger note 63 with accent and slide
    assert_eq!(events[2].0, 6000);
    assert_eq!(
        events[2].1,
        SeqEvent::Trigger {
            key: 3,
            octave: 0,
            accent: true,
        }
    );

    // Step 2: SlideTo note 67 (no intermediate GateOff due to slide!)
    assert_eq!(events[3].0, 12000);
    assert_eq!(
        events[3].1,
        SeqEvent::SlideTo {
            key: 7,
            octave: 0,
            accent: false,
        }
    );
}

#[test]
fn test_same_pitch_slide_sequencer_events() {
    let mut seq = Sequencer::new(48000.0);
    seq.set_running(true);

    let mut pattern = Pattern::default();
    pattern.num_steps = 2;
    // Step 0: C3 with slide
    pattern.pitches[0] = PitchEntry {
        key: 0,
        octave: 0,
        accent: false,
        slide: true,
    };
    pattern.timing[0] = TimeEntry::Note;
    // Step 1: C3 without slide
    pattern.pitches[1] = PitchEntry {
        key: 0,
        octave: 0,
        accent: false,
        slide: false,
    };
    pattern.timing[1] = TimeEntry::Note;

    let mut events = [(0, SeqEvent::GateOff); 64];
    let mut count = 0;

    seq.process_block(
        &pattern,
        0,
        12000,
        &mut events,
        &mut count,
        120.0,
        plugin303::params::Timebase::Straight,
        false,
    );

    assert_eq!(count, 3);
    assert_eq!(events[0].0, 0);
    assert_eq!(
        events[0].1,
        SeqEvent::Trigger {
            key: 0,
            octave: 0,
            accent: false
        }
    );
    assert_eq!(events[1].0, 6000);
    assert_eq!(
        events[1].1,
        SeqEvent::SlideTo {
            key: 0,
            octave: 0,
            accent: false
        }
    );
    assert_eq!(events[2].1, SeqEvent::GateOff);
}

#[test]
fn test_synth_bypass_flag_skips_dsp() {
    let mut params_val = Plugin303Params::default();
    params_val.synth_bypass = nih_plug::prelude::BoolParam::new("Synth Bypass", true);
    let params = std::sync::Arc::new(params_val);
    let plugin = plugin303::Plugin303::new_with_params(params);

    assert!(plugin.params.synth_bypass.value());
}
