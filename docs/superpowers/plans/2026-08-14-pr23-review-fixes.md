# PR #23 Review Followups Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Address all Important and Minor review findings on PR #23: correct upper C octave offset calculation in the piano roll and step button, precompute zero-allocation static note labels for the 61 piano roll rows, eliminate the redundant faux-bold double text draw in knobs, and add comprehensive unit test assertions.

**Architecture:** 
- Centralize and correct 61-row pitch label formatting with a static zero-allocation array in `piano_roll_popup.rs`.
- Update `format_pitch` in `step_button.rs` to handle upper C (`semitone == 12`) with `octave + 1`.
- Clean up text rendering in `knob.rs`.
- Extend `tests/piano_roll_mapping.rs` with full pitch string verification for all 61 rows and boundary notes.

**Tech Stack:** Rust 2021, `nih_plug_egui` / `egui`, `cargo test`.

## Global Constraints

- Audio thread is strictly zero-allocation; all GUI state is decoupled and confined to `plugin303/src/gui/`.
- No breaking changes to existing parameter IDs or DSP structures.
- All unit and integration tests must pass cleanly.

---

### Task 1: Fix Octave Numbering & Static Note Label Table in Piano Roll Popup and Step Button

**Files:**
- Modify: `plugin303/src/gui/widgets/piano_roll_popup.rs:20-65,195-225`
- Modify: `plugin303/src/gui/widgets/step_button.rs:5-15`
- Modify: `plugin303/tests/piano_roll_mapping.rs`

**Interfaces:**
- Produces: `pub fn row_index_to_label(row_idx: usize) -> &'static str` in `piano_roll_popup.rs`
- Produces: `pub fn format_pitch(semitone: i8, octave: i8) -> String` in `step_button.rs` (correct for `semitone == 12`)

- [ ] **Step 1: Write failing test in `plugin303/tests/piano_roll_mapping.rs`**

Add tests for row label mapping and `step_button::format_pitch` upper C handling:

```rust
use plugin303::gui::widgets::piano_roll_popup::row_index_to_label;
use plugin303::gui::widgets::step_button::format_pitch;

#[test]
fn test_row_labels_octave_boundaries() {
    assert_eq!(row_index_to_label(0), "C6");   // Top C (row 0, semitone 12, oct 2)
    assert_eq!(row_index_to_label(1), "B5");
    assert_eq!(row_index_to_label(12), "C5");  // Base C in oct 2 (row 12, semitone 0, oct 2)
    assert_eq!(row_index_to_label(24), "C4");  // Base C in oct 1
    assert_eq!(row_index_to_label(36), "C3");  // Base C in oct 0 (Middle C)
    assert_eq!(row_index_to_label(48), "C2");  // Base C in oct -1
    assert_eq!(row_index_to_label(60), "C1");  // Base C in oct -2 (Lowest note)
}

#[test]
fn test_format_pitch_upper_c() {
    assert_eq!(format_pitch(0, 0), "C 3");
    assert_eq!(format_pitch(12, 0), "C 4");
    assert_eq!(format_pitch(0, 2), "C 5");
    assert_eq!(format_pitch(12, 2), "C 6");
    assert_eq!(format_pitch(0, -2), "C 1");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test piano_roll_mapping`
Expected: FAIL (missing `row_index_to_label` and incorrect `format_pitch` output for upper C).

- [ ] **Step 3: Implement static label array and helper in `piano_roll_popup.rs` and update `step_button.rs`**

In `plugin303/src/gui/widgets/piano_roll_popup.rs`:
```rust
pub const PIANO_ROLL_LABELS: [&str; TOTAL_PIANO_ROLL_ROWS] = [
    "C6", "B5", "A#5", "A5", "G#5", "G5", "F#5", "F5", "E5", "D#5", "D5", "C#5", // Rows 0..=11
    "C5", "B4", "A#4", "A4", "G#4", "G4", "F#4", "F4", "E4", "D#4", "D4", "C#4", // Rows 12..=23
    "C4", "B3", "A#3", "A3", "G#3", "G3", "F#3", "F3", "E3", "D#3", "D3", "C#3", // Rows 24..=35
    "C3", "B2", "A#2", "A2", "G#2", "G2", "F#2", "F2", "E2", "D#2", "D2", "C#2", // Rows 36..=47
    "C2", "B1", "A#1", "A1", "G#1", "G1", "F#1", "F1", "E1", "D#1", "D1", "C#1", // Rows 48..=59
    "C1",                                                                          // Row 60
];

#[inline]
pub fn row_index_to_label(row_idx: usize) -> &'static str {
    if row_idx < TOTAL_PIANO_ROLL_ROWS {
        PIANO_ROLL_LABELS[row_idx]
    } else {
        "C1"
    }
}
```

Inside `render_piano_roll_popup` loop:
```rust
for row_idx in 0..TOTAL_PIANO_ROLL_ROWS {
    let (semitone, octave) = row_index_to_pitch(row_idx);
    let is_active = row_idx == active_row;
    let is_black = is_semitone_black_key(semitone);
    let full_note_str = row_index_to_label(row_idx);
    ...
```

In `plugin303/src/gui/widgets/step_button.rs`:
```rust
pub fn format_pitch(semitone: i8, octave: i8) -> String {
    let note_names = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B", "C",
    ];
    let name = note_names
        .get(semitone.clamp(0, 12) as usize)
        .unwrap_or(&"C");
    let oct_num = 3 + octave + if semitone == 12 { 1 } else { 0 };
    format!("{} {}", name, oct_num)
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test piano_roll_mapping`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add plugin303/src/gui/widgets/piano_roll_popup.rs plugin303/src/gui/widgets/step_button.rs plugin303/tests/piano_roll_mapping.rs
git commit -m "fix(gui): correct octave calculation for upper C and add static piano roll labels"
```

---

### Task 2: Remove Faux-Bold Double Text Draw in Knob Widget

**Files:**
- Modify: `plugin303/src/gui/widgets/knob.rs:125-142`

- [ ] **Step 1: Simplify title text painter call in `knob.rs`**

In `plugin303/src/gui/widgets/knob.rs`, remove the redundant second text pass with horizontal offset:

```rust
// Parameter Title Label below dial
let label_y = center.y + self.radius + 10.0;
let label_pos = pos2(rect.center().x, label_y);
painter.text(
    label_pos,
    nih_plug_egui::egui::Align2::CENTER_CENTER,
    self.label.to_uppercase(),
    FontId::new(9.0, FontFamily::Proportional),
    theme::TEXT_PRIMARY,
);
```

- [ ] **Step 2: Verify compilation and check tests**

Run: `cargo check -p plugin303 --all-targets`
Expected: Success with no warnings or errors.

- [ ] **Step 3: Commit**

```bash
git add plugin303/src/gui/widgets/knob.rs
git commit -m "refactor(gui): remove redundant double text draw for knob labels"
```

---

### Task 3: Full Test Suite and Quality Verification

**Files:**
- None (verification task)

- [ ] **Step 1: Run complete test suite**

Run: `cargo test -p plugin303`
Expected: All tests pass.

- [ ] **Step 2: Run all target checks and lints**

Run: `cargo clippy -p plugin303 --all-targets -- -D warnings`
Expected: 0 errors, 0 warnings.
