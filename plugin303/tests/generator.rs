use plugin303::dsp::generator::{AcidGenerator, GeneratorParams, MusicalScale};
use plugin303::dsp::sequencer::{SeqEvent, Sequencer, TimeEntry, Timebase};

#[test]
fn test_bjorklund_pulse_counts() {
    for n in 1..=16 {
        for k in 0..=n {
            let mask = AcidGenerator::bjorklund(k as u8, n as u8, 0);
            let active_count = mask.iter().take(n).filter(|&&p| p).count();
            assert_eq!(
                active_count, k,
                "Bjorklund pulse count mismatch for k={}, n={}",
                k, n
            );
        }
    }
}

#[test]
fn test_bjorklund_shift_rotation() {
    let base = AcidGenerator::bjorklund(5, 16, 0);
    let shifted = AcidGenerator::bjorklund(5, 16, 2);

    for i in 0..16 {
        assert_eq!(
            shifted[(i + 2) % 16],
            base[i],
            "Shift rotation mismatch at step {}",
            i
        );
    }
}

#[test]
fn test_generator_determinism() {
    let params = GeneratorParams {
        root_note: 2, // D
        scale: MusicalScale::MinorPentatonic,
        octave_min: -1,
        octave_max: 1,
        rhythm_pulses: 7,
        rhythm_length: 16,
        rhythm_shift: 3,
        pitch_length: 16,
        rest_probability: 0.1,
        accent_probability: 0.4,
        slide_probability: 0.3,
        octave_up_prob: 0.2,
        octave_down_prob: 0.2,
        accent_pitch_bias: 0.6,
        slide_chaining: false,
        mutation_rate: 0.2,
        seed: 0xDEAD_BEEF_CAFE_BABE,
    };

    let pat1 = AcidGenerator::generate(&params);
    let pat2 = AcidGenerator::generate(&params);

    assert_eq!(
        pat1, pat2,
        "Generator must be 100% deterministic for identical params & seed"
    );
}

#[test]
fn test_scale_quantization_and_octave_bounds() {
    let scales = [
        MusicalScale::MinorPentatonic,
        MusicalScale::Dorian,
        MusicalScale::Phrygian,
        MusicalScale::Aeolian,
        MusicalScale::HarmonicMinor,
        MusicalScale::Chromatic,
    ];

    for scale in scales {
        let params = GeneratorParams {
            root_note: 5, // F
            scale,
            octave_min: -1,
            octave_max: 1,
            rhythm_pulses: 16, // all notes
            rhythm_length: 16,
            rest_probability: 0.0,
            seed: 9999,
            ..Default::default()
        };

        let pat = AcidGenerator::generate(&params);
        let intervals = scale.intervals();

        for i in 0..16 {
            if pat.timing[i] == TimeEntry::Note {
                let note_key = pat.pitches[i].key;
                let valid_semitone = intervals
                    .iter()
                    .any(|&interval| (params.root_note as i8 + interval) % 12 == note_key);
                assert!(
                    valid_semitone,
                    "Note key {} not in scale {:?} with root {}",
                    note_key, scale, params.root_note
                );
                assert!(
                    pat.pitches[i].octave >= params.octave_min
                        && pat.pitches[i].octave <= params.octave_max,
                    "Octave {} out of bounds [{}, {}]",
                    pat.pitches[i].octave,
                    params.octave_min,
                    params.octave_max
                );
            }
        }
    }
}

#[test]
fn test_octave_wide_range_bounds() {
    // Wide -2..=2 range: random-walk must stay clamped yet still reach extremes.
    let mut seen_min = i8::MAX;
    let mut seen_max = i8::MIN;
    for seed in 0..200u64 {
        let params = GeneratorParams {
            root_note: 0,
            scale: MusicalScale::Chromatic,
            octave_min: -2,
            octave_max: 2,
            rhythm_pulses: 16, // all notes -> walk steps every step
            rhythm_length: 16,
            rest_probability: 0.0,
            octave_up_prob: 0.5,
            octave_down_prob: 0.5,
            seed,
            ..Default::default()
        };
        let pat = AcidGenerator::generate(&params);
        for i in 0..16 {
            if pat.timing[i] == TimeEntry::Note {
                let oct = pat.pitches[i].octave;
                assert!(
                    (-2..=2).contains(&oct),
                    "Octave {} out of wide bounds [-2, 2] (seed {})",
                    oct,
                    seed
                );
                seen_min = seen_min.min(oct);
                seen_max = seen_max.max(oct);
            }
        }
    }
    // These two assertions depend on the global RNG call order inside generate().
    // Adding or removing an rng.next_f32() anywhere earlier in the step loop
    // reshuffles all 200 seeds and can break this test from an unrelated change.
    assert_eq!(seen_min, -2, "Walk never reached -2 across seeds");
    assert_eq!(seen_max, 2, "Walk never reached +2 across seeds");
}

#[test]
fn test_slide_chaining_rule() {
    let params = GeneratorParams {
        slide_probability: 1.0, // force slide
        slide_chaining: false,  // no consecutive slides
        rest_probability: 0.0,
        rhythm_pulses: 16,
        rhythm_length: 16,
        seed: 12345,
        ..Default::default()
    };

    let pat = AcidGenerator::generate(&params);

    for i in 1..16 {
        if pat.timing[i] == TimeEntry::Note && pat.timing[i - 1] == TimeEntry::Note {
            if pat.pitches[i - 1].slide {
                assert!(
                    !pat.pitches[i].slide,
                    "Consecutive slides detected at step {} when slide_chaining is false",
                    i
                );
            }
        }
    }
}

#[test]
fn test_slide_into_rest_tie() {
    // Manually test pattern where note slides into rest
    let mut params = GeneratorParams::default();
    params.slide_probability = 1.0;
    params.rest_probability = 0.5;
    params.seed = 42;

    let pat = AcidGenerator::generate(&params);

    for i in 1..16 {
        if pat.timing[i] == TimeEntry::Tie {
            assert_ne!(
                pat.timing[i - 1],
                TimeEntry::Rest,
                "Tie at step {} must be preceded by active note/tie",
                i
            );
            assert_eq!(
                pat.pitches[i].key,
                pat.pitches[i - 1].key,
                "Tie at step {} must preserve previous note pitch",
                i
            );
        }
    }
}

#[test]
fn test_mutation_determinism_and_zero_rate() {
    let params = GeneratorParams {
        seed: 55555,
        mutation_rate: 0.0,
        ..Default::default()
    };

    let original = AcidGenerator::generate(&params);
    let mut mutated = original.clone();

    // With mutation_rate 0.0, no change
    AcidGenerator::mutate(&mut mutated, &params, 1);
    assert_eq!(
        original, mutated,
        "Mutation with rate 0.0 must leave pattern unchanged"
    );

    // With active mutation, deterministic output for same loop count
    let mut mut_params = params.clone();
    mut_params.mutation_rate = 0.5;

    let mut m1 = original.clone();
    let mut m2 = original.clone();
    AcidGenerator::mutate(&mut m1, &mut_params, 42);
    AcidGenerator::mutate(&mut m2, &mut_params, 42);

    assert_eq!(
        m1, m2,
        "Mutate must be deterministic for identical loop seed"
    );
    assert_ne!(original, m1, "Mutation with 0.5 rate must change pattern");
}

#[test]
fn test_sequencer_generative_pattern_playback() {
    let params = GeneratorParams {
        rhythm_pulses: 8,
        rhythm_length: 16,
        seed: 777,
        ..Default::default()
    };

    let mut pat = AcidGenerator::generate(&params);
    let mut seq = Sequencer::new(48000.0);
    seq.set_running(true);

    let mut events = [(0, SeqEvent::GateOff); 64];
    let mut count = 0;

    // Process enough samples for full 16-step loop wrap (16 * 6000 = 96000 samples)
    seq.process_block_mutating(
        &mut pat,
        Some(&params),
        0,
        120000,
        &mut events,
        &mut count,
        120.0,
        Timebase::Straight,
        false,
    );

    assert!(
        count > 0,
        "Sequencer must generate sequence events from generative pattern"
    );
    assert!(
        seq.loop_count > 0,
        "Sequencer loop count must increment upon wrapping step 0"
    );
}

#[test]
fn test_root_note_euclidean_rem_safety() {
    let mut params = GeneratorParams::default();
    params.root_note = 250; // Large u8, wraps i8 arithmetic
    let pat = AcidGenerator::generate(&params);
    for p in &pat.pitches {
        assert!(p.key >= 0 && p.key < 12, "Key {} out of range 0..12", p.key);
    }
}

#[test]
fn test_sequencer_loop_wrap_mutates_before_step_zero_playback() {
    let params = GeneratorParams {
        rhythm_pulses: 16,
        rhythm_length: 16,
        mutation_rate: 1.0, // force 100% mutation
        seed: 42,
        ..Default::default()
    };
    let mut pat = AcidGenerator::generate(&params);
    let pat_initial = pat.clone();

    let mut seq = Sequencer::new(48000.0);
    seq.set_running(true);

    let mut events = [(0, SeqEvent::GateOff); 64];
    let mut count = 0;

    // Process steps 0..14 (90000 samples at 120bpm = 15 steps of 6000 samples)
    seq.process_block_mutating(
        &mut pat,
        Some(&params),
        0,
        90000,
        &mut events,
        &mut count,
        120.0,
        Timebase::Straight,
        false,
    );

    assert_eq!(
        pat, pat_initial,
        "Pattern should not mutate before loop wrap"
    );

    // Process block crossing step 15 and wrapping to step 0 of loop 1 (samples 90000..102000)
    seq.process_block_mutating(
        &mut pat,
        Some(&params),
        90000,
        12000,
        &mut events,
        &mut count,
        120.0,
        Timebase::Straight,
        false,
    );

    assert_ne!(
        pat, pat_initial,
        "Pattern should be mutated when loop count increments at step 0 boundary"
    );
}

#[test]
fn test_inverted_octave_range_does_not_panic() {
    // The GUI exposes MIN and MAX as independent DragValues, so an inverted
    // range is reachable. `Ord::clamp` panics when min > max; generate() runs
    // on the editor thread, so that panic would take down the host editor.
    let params = GeneratorParams {
        octave_min: 2,
        octave_max: -2,
        rhythm_pulses: 16,
        rhythm_length: 16,
        rest_probability: 0.0,
        octave_up_prob: 0.5,
        octave_down_prob: 0.5,
        seed: 7,
        ..Default::default()
    };
    let pat = AcidGenerator::generate(&params);
    for i in 0..16 {
        if pat.timing[i] == TimeEntry::Note {
            let oct = pat.pitches[i].octave;
            assert!(
                (-2..=2).contains(&oct),
                "Octave {} outside the bounds implied by an inverted range",
                oct
            );
        }
    }

    // The GUI normalises an inverted pair with a swap, not a widening. Pin that
    // a swapped (ordered) pair produces the identical pattern to the inverted
    // one, since generator.rs itself already derives oct_lo/oct_hi via min/max.
    let ordered_params = GeneratorParams {
        octave_min: -2,
        octave_max: 2,
        ..params.clone()
    };
    let ordered_pat = AcidGenerator::generate(&ordered_params);
    assert_eq!(
        pat, ordered_pat,
        "Inverted range and its swapped equivalent must produce identical patterns"
    );
}

#[test]
fn test_octave_walk_is_adjacent_between_notes() {
    // The generator walks from the previous *note* step's octave, one step at a
    // time. Rest and Tie steps do not advance the walk. This pins adjacency
    // against a revert to absolute octave selection; it does NOT distinguish
    // reflect from a plain clamp, since a clamp also yields |delta| <= 1 at the
    // boundary. `test_octave_walk_does_not_park_at_upper_bound` is the test
    // that pins reflection specifically.
    for seed in 0..200u64 {
        let params = GeneratorParams {
            scale: MusicalScale::Chromatic,
            octave_min: -2,
            octave_max: 2,
            rhythm_pulses: 16,
            rhythm_length: 16,
            rest_probability: 0.0,
            octave_up_prob: 0.5,
            octave_down_prob: 0.5,
            seed,
            ..Default::default()
        };
        let pat = AcidGenerator::generate(&params);
        let mut prev: Option<i8> = None;
        for i in 0..16 {
            if pat.timing[i] == TimeEntry::Note {
                let cur = pat.pitches[i].octave;
                if let Some(p) = prev {
                    assert!(
                        (cur - p).abs() <= 1,
                        "Octave jumped from {} to {} at step {} (seed {})",
                        p,
                        cur,
                        i,
                        seed
                    );
                }
                prev = Some(cur);
            }
        }
    }
}

#[test]
fn test_octave_walk_does_not_park_at_upper_bound() {
    // Clamping makes the boundary absorbing, so an asymmetric up/down bias
    // parks the walk against octave_max. Reflecting keeps the distribution off
    // the wall. Measured over the default probabilities.
    let mut at_max = 0usize;
    let mut total = 0usize;
    for seed in 0..400u64 {
        let params = GeneratorParams {
            octave_min: -1,
            octave_max: 1,
            rhythm_pulses: 16,
            rhythm_length: 16,
            rest_probability: 0.0,
            octave_up_prob: 0.2,
            octave_down_prob: 0.1,
            seed,
            ..Default::default()
        };
        let pat = AcidGenerator::generate(&params);
        for i in 0..16 {
            if pat.timing[i] == TimeEntry::Note {
                total += 1;
                if pat.pitches[i].octave == 1 {
                    at_max += 1;
                }
            }
        }
    }
    let share = at_max as f32 / total as f32;
    assert!(
        share < 0.40,
        "Walk parks at octave_max {:.1}% of the time; boundary is absorbing",
        share * 100.0
    );
}

#[test]
fn test_extreme_octave_bounds_do_not_panic() {
    // `GeneratorParams` derives `Deserialize`, so a hand-edited or version-skewed
    // preset can carry i8::MIN/i8::MAX bounds. `prev_octave + step` used to
    // overflow before the bounds check ran; saturating arithmetic must hold.
    let params = GeneratorParams {
        octave_min: i8::MIN,
        octave_max: i8::MAX,
        rhythm_pulses: 16,
        rhythm_length: 16,
        rest_probability: 0.0,
        octave_up_prob: 0.5,
        octave_down_prob: 0.5,
        seed: 11,
        ..Default::default()
    };
    let pat = AcidGenerator::generate(&params);
    for i in 0..16 {
        if pat.timing[i] == TimeEntry::Note {
            let oct = pat.pitches[i].octave;
            assert!(
                (i8::MIN..=i8::MAX).contains(&oct),
                "Octave {} outside i8 range",
                oct
            );
        }
    }
}

#[test]
fn test_extreme_octave_bounds_degenerate_does_not_panic() {
    // Degenerate at-the-extreme range: min == max == i8::MAX pins the
    // saturating-add overflow path specifically.
    let params = GeneratorParams {
        octave_min: i8::MAX,
        octave_max: i8::MAX,
        rhythm_pulses: 16,
        rhythm_length: 16,
        rest_probability: 0.0,
        octave_up_prob: 0.5,
        octave_down_prob: 0.5,
        seed: 13,
        ..Default::default()
    };
    let pat = AcidGenerator::generate(&params);
    for i in 0..16 {
        if pat.timing[i] == TimeEntry::Note {
            assert_eq!(pat.pitches[i].octave, i8::MAX);
        }
    }
}
