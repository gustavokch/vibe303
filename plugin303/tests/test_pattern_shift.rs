use plugin303::params::SequencerPattern;

fn shift_pattern_left(pattern: &mut SequencerPattern) {
    pattern.steps.rotate_left(1);
}

fn shift_pattern_right(pattern: &mut SequencerPattern) {
    pattern.steps.rotate_right(1);
}

#[test]
fn test_pattern_cyclic_shift() {
    let mut pattern = SequencerPattern::default();
    pattern.steps[0].semitone = 12;
    pattern.steps[1].semitone = 7;

    shift_pattern_left(&mut pattern);
    assert_eq!(pattern.steps[15].semitone, 12);
    assert_eq!(pattern.steps[0].semitone, 7);

    shift_pattern_right(&mut pattern);
    assert_eq!(pattern.steps[0].semitone, 12);
    assert_eq!(pattern.steps[1].semitone, 7);
}
