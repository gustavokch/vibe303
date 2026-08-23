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
