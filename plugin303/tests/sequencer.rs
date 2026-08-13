use plugin303::dsp::sequencer::{PitchEntry, TimeEntry, Pattern, SeqEvent, Timebase};

#[test]
fn test_data_model_compiles() {
    let _pattern = Pattern {
        pitches: [PitchEntry::default(); 16],
        timing: [TimeEntry::Rest; 16],
        num_steps: 16,
    };
}
