#![cfg(feature = "gui")]

use plugin303::gui::widgets::piano_roll_popup::{
    is_semitone_black_key, note_name_for_semitone, pitch_to_row_index, row_index_to_label,
    row_index_to_pitch, TOTAL_PIANO_ROLL_ROWS,
};
use plugin303::gui::widgets::step_button::format_pitch;

#[test]
fn test_total_rows_count() {
    assert_eq!(TOTAL_PIANO_ROLL_ROWS, 61);
}

#[test]
fn test_pitch_round_trip() {
    for octave in -2..=2 {
        for semitone in 0..=11 {
            let row = pitch_to_row_index(semitone, octave);
            assert!(
                row < TOTAL_PIANO_ROLL_ROWS,
                "Row {} out of bounds for semitone {} octave {}",
                row,
                semitone,
                octave
            );
            let (s_out, o_out) = row_index_to_pitch(row);
            assert_eq!(
                (s_out, o_out),
                (semitone, octave),
                "Mismatch for semitone {} octave {}",
                semitone,
                octave
            );
        }
    }

    // Test top C (semitone 12, octave 2 -> row 0)
    let top_c_row = pitch_to_row_index(12, 2);
    assert_eq!(top_c_row, 0);
    let (s_top, o_top) = row_index_to_pitch(0);
    assert_eq!((s_top, o_top), (12, 2));
}

#[test]
fn test_boundary_pitches() {
    // Lowest note: C1 (semitone 0, octave -2) -> row 60
    let c1_row = pitch_to_row_index(0, -2);
    assert_eq!(c1_row, 60);
    assert_eq!(row_index_to_pitch(60), (0, -2));

    // Middle C: C3 (semitone 0, octave 0) -> row 36
    let c3_row = pitch_to_row_index(0, 0);
    assert_eq!(c3_row, 36);
    assert_eq!(row_index_to_pitch(36), (0, 0));

    // C4 (semitone 0, octave 1) -> row 24
    let c4_row = pitch_to_row_index(0, 1);
    assert_eq!(c4_row, 24);
    assert_eq!(row_index_to_pitch(24), (0, 1));

    // C5 (semitone 0, octave 2) -> row 12
    let c5_row = pitch_to_row_index(0, 2);
    assert_eq!(c5_row, 12);
    assert_eq!(row_index_to_pitch(12), (0, 2));

    // C6 (semitone 12, octave 2) -> row 0
    let c6_row = pitch_to_row_index(12, 2);
    assert_eq!(c6_row, 0);
    assert_eq!(row_index_to_pitch(0), (12, 2));
}

#[test]
fn test_black_key_classification() {
    // C, D, E, F, G, A, B are white keys
    assert!(!is_semitone_black_key(0)); // C
    assert!(!is_semitone_black_key(2)); // D
    assert!(!is_semitone_black_key(4)); // E
    assert!(!is_semitone_black_key(5)); // F
    assert!(!is_semitone_black_key(7)); // G
    assert!(!is_semitone_black_key(9)); // A
    assert!(!is_semitone_black_key(11)); // B
    assert!(!is_semitone_black_key(12)); // C'

    // C#, D#, F#, G#, A# are black keys
    assert!(is_semitone_black_key(1)); // C#
    assert!(is_semitone_black_key(3)); // D#
    assert!(is_semitone_black_key(6)); // F#
    assert!(is_semitone_black_key(8)); // G#
    assert!(is_semitone_black_key(10)); // A#
}

#[test]
fn test_note_names() {
    assert_eq!(note_name_for_semitone(0), "C");
    assert_eq!(note_name_for_semitone(1), "C#");
    assert_eq!(note_name_for_semitone(2), "D");
    assert_eq!(note_name_for_semitone(3), "D#");
    assert_eq!(note_name_for_semitone(4), "E");
    assert_eq!(note_name_for_semitone(5), "F");
    assert_eq!(note_name_for_semitone(6), "F#");
    assert_eq!(note_name_for_semitone(7), "G");
    assert_eq!(note_name_for_semitone(8), "G#");
    assert_eq!(note_name_for_semitone(9), "A");
    assert_eq!(note_name_for_semitone(10), "A#");
    assert_eq!(note_name_for_semitone(11), "B");
    assert_eq!(note_name_for_semitone(12), "C");
}

#[test]
fn test_row_labels_octave_boundaries() {
    assert_eq!(row_index_to_label(0), "C6"); // Top C (row 0, semitone 12, oct 2)
    assert_eq!(row_index_to_label(1), "B5");
    assert_eq!(row_index_to_label(12), "C5"); // Base C in oct 2 (row 12, semitone 0, oct 2)
    assert_eq!(row_index_to_label(24), "C4"); // Base C in oct 1
    assert_eq!(row_index_to_label(36), "C3"); // Base C in oct 0 (Middle C)
    assert_eq!(row_index_to_label(48), "C2"); // Base C in oct -1
    assert_eq!(row_index_to_label(60), "C1"); // Base C in oct -2 (Lowest note)
}

#[test]
fn test_format_pitch_upper_c() {
    assert_eq!(format_pitch(0, 0), "C 3");
    assert_eq!(format_pitch(12, 0), "C 4");
    assert_eq!(format_pitch(0, 2), "C 5");
    assert_eq!(format_pitch(12, 2), "C 6");
    assert_eq!(format_pitch(0, -2), "C 1");
}
