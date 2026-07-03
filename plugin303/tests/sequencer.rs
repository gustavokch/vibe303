use plugin303::dsp::sequencer::{Sequencer, TransportInfo};
use plugin303::params::{SequencerStep, StepGate};

#[test]
fn sequencer_step_advancement_and_gate() {
    let sr = 48_000.0;
    let mut seq = Sequencer::new();
    
    // Create a simple pattern: step 0 is Note, step 1 is Rest
    let mut steps = [SequencerStep::default(); 16];
    steps[0].gate = StepGate::Note;
    steps[0].semitone = 0; // C
    steps[0].octave = 0;   // octave 0
    steps[0].gate_len = 0.5;

    steps[1].gate = StepGate::Rest;

    // Simulate playback
    let mut transport = TransportInfo {
        playing: true,
        pos_beats: Some(0.0), // beat 0 (16th step 0)
        tempo: Some(120.0),
    };

    // Tick at beat 0 (step 0)
    seq.tick(sr, &transport, &steps, 16, 120.0, true, 0.5);
    assert_eq!(seq.gate, true);
    assert_eq!(seq.trigger, true);

    // Tick near end of step 0 (phase = 0.9 beats = 0.225)
    transport.pos_beats = Some(0.2); // 16th note step 0 is pos_beats in [0.0, 0.25)
    seq.tick(sr, &transport, &steps, 16, 120.0, true, 0.5);
    // Since gate_len is 0.5, gate should have closed (phase within step is 0.2 / 0.25 = 0.8 > 0.5)
    assert_eq!(seq.gate, false);

    // Advance to step 1 (Rest step, pos_beats in [0.25, 0.5))
    transport.pos_beats = Some(0.3);
    seq.tick(sr, &transport, &steps, 16, 120.0, true, 0.5);
    assert_eq!(seq.gate, false);
    assert_eq!(seq.trigger, false);
}

#[test]
fn sequencer_slide_frequency_slew() {
    let sr = 48_000.0;
    let mut seq = Sequencer::new();
    
    // Step 0: C1 with Slide
    // Step 1: C2
    let mut steps = [SequencerStep::default(); 16];
    steps[0].gate = StepGate::Note;
    steps[0].semitone = 0;
    steps[0].octave = 0;
    steps[0].slide = true;

    steps[1].gate = StepGate::Note;
    steps[1].semitone = 0;
    steps[1].octave = 1; // 12 semitones up (double frequency)
    steps[1].slide = false;

    let mut transport = TransportInfo {
        playing: true,
        pos_beats: Some(0.0),
        tempo: Some(120.0),
    };

    // Tick step 0
    seq.tick(sr, &transport, &steps, 16, 120.0, true, 0.5);
    let freq_step_0 = seq.freq;

    // Advance to step 1 (C2)
    transport.pos_beats = Some(0.26); // step 1
    seq.tick(sr, &transport, &steps, 16, 120.0, true, 0.5);
    
    // Since step 0 had Slide, the frequency on step 1 should start slewing and not be instantly C2
    assert!(seq.freq < freq_step_0 * 2.0, "Frequency should not jump instantly to C2: {} vs {}", seq.freq, freq_step_0 * 2.0);
    assert!(seq.freq > freq_step_0, "Frequency should have slewed upwards: {} vs {}", seq.freq, freq_step_0);
    // And envelope should NOT have retriggered because it slides
    assert_eq!(seq.trigger, false, "Slide should bypass envelope retrigger");
}
