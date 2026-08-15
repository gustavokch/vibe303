# Implementation Plan: Probabilistic Acid Euclidean Sequence Generator

## Context
Acid basslines (303-style patterns) rely heavily on rhythmic syncopation, octave leaps, slides, accents, and subtle melodic variations. Manually programming authentic 303 patterns step-by-step can be slow. 

This design adds a probabilistic Acid Euclidean Sequence Generator (`AcidGenerator`) to `plugin303`. It combines Bjorklund Euclidean pulse distribution with musical scale constraints, probabilistic acid features (slides, accents, octave shifts, slide chaining), decoupled polyrhythmic pitch/rhythm step lengths, live pattern mutation, an interactive GUI panel, and a standalone CLI utility.

---

## Proposed Architecture & Design

### 1. Data Structures & Scale Engine (`plugin303/src/dsp/generator.rs`)

#### Scales (`MusicalScale`)
- `MinorPentatonic` (Root, +3, +5, +7, +10 semitones)
- `Dorian` (Root, +2, +3, +5, +7, +9, +10 semitones)
- `Phrygian` (Root, +1, +3, +5, +7, +8, +10 semitones)
- `Aeolian` (Root, +2, +3, +5, +7, +8, +10 semitones)
- `HarmonicMinor` (Root, +2, +3, +5, +7, +8, +11 semitones)
- `Chromatic` (Root + 0..11 semitones)

#### Generator Parameters (`GeneratorParams`)
```rust
#[derive(Clone, Debug, PartialEq)]
pub struct GeneratorParams {
    pub root_note: u8,           // 0..12 (C=0..B=11)
    pub scale: MusicalScale,
    pub octave_min: i8,          // -1, 0, +1
    pub octave_max: i8,          // -1, 0, +1
    pub rhythm_pulses: u8,       // k: 1..16 pulses
    pub rhythm_length: u8,       // n: 1..16 steps
    pub rhythm_shift: u8,        // rotation offset 0..15
    pub pitch_length: u8,        // pitch cycle length 1..16 (decoupled polyrhythm)
    pub fill_probability: f32,   // 0.0..1.0 note vs rest fill
    pub rest_probability: f32,   // 0.0..1.0 rest insertion
    pub accent_probability: f32, // 0.0..1.0 accent chance
    pub slide_probability: f32,  // 0.0..1.0 slide chance
    pub octave_up_prob: f32,     // 0.0..1.0 octave up chance
    pub octave_down_prob: f32,   // 0.0..1.0 octave down chance
    pub accent_pitch_bias: f32,  // 0.0..1.0 higher pitch preference on accents
    pub slide_chaining: bool,    // allow consecutive slides
    pub mutation_rate: f32,      // 0.0..1.0 probability per step on loop wrap
    pub seed: u64,
}
```

---

### 2. Algorithmic Generation Pipeline

1. **Euclidean Pulse Generator (Bjorklund Algorithm)**:
   - Distributes `k` pulses evenly across `n` steps (`rhythm_length`).
   - Applies `rhythm_shift` cyclic rotation.
   - Resulting boolean mask dictates baseline `TimeEntry::Note` vs `TimeEntry::Rest`.

2. **Melodic Markov / Weighted Scale Walk**:
   - Generates array of `pitch_length` notes (1..16).
   - Constrained by `MusicalScale` degrees and octave limits `[octave_min, octave_max]`.
   - Smooth weighted pitch intervals with occasional fifth/octave leaps.

3. **Acid Decorator Stage**:
   - Decorates notes with `accent` and `slide` attributes.
   - If `accent_pitch_bias > 0`, accented notes receive higher scale degree weights or octave-up nudges.
   - Assigns `slide = true` based on `slide_probability` and `slide_chaining` constraints.

4. **Decoupled Polyrhythmic Assembly**:
   - Constructs a 16-step `Pattern` (`pitches: [PitchEntry; 16]`, `timing: [TimeEntry; 16]`).
   - Sets `pattern.decoupled = (rhythm_length != pitch_length)`.

5. **Loop Mutation Engine**:
   - `AcidGenerator::mutate_step(pattern, step_idx, rate, rng)` mutates active steps during sequencer playback loop boundary (step 0 wrap).
   - Enables dynamic, continuously evolving live basslines.

---

## Critical Files to Create & Modify

### 1. New Module: `plugin303/src/dsp/generator.rs`
- Implements `MusicalScale`, `GeneratorParams`, `AcidGenerator`.
- `AcidGenerator::bjorklund(pulses, length, shift) -> Vec<bool>`
- `AcidGenerator::generate(params: &GeneratorParams) -> Pattern`
- `AcidGenerator::mutate(pattern: &mut Pattern, rate: f32, seed: u64)`

### 2. Module Export: `plugin303/src/dsp/mod.rs`
- Add `pub mod generator;` and re-exports.

### 3. Sequencer Integration: `plugin303/src/dsp/sequencer.rs`
- Add optional mutation integration hook when `Sequencer` wraps around step 0.

### 4. GUI Generator Panel: `plugin303/src/gui/panels/generator_panel.rs`
- Interactive control panel in egui containing knobs/sliders for:
  - Root Key & Scale Dropdown / Selector
  - Euclidean Pulses ($k$) & Steps ($n$)
  - Accent %, Slide %, Octave % Probabilities
  - Mutation Rate slider
  - `[GENERATE PATTERN]` and `[MUTATE PATTERN]` buttons

### 5. GUI Integration: `plugin303/src/gui/panels/mod.rs` & `plugin303/src/gui/mod.rs`
- Wire generator panel tab/section into `plugin303` GUI interface.

### 6. CLI Tool: `plugin303/src/bin/gen_acid.rs`
- CLI binary for testing and exporting patterns.
- Usage: `cargo run --bin gen_acid -- --key C --scale minor-pentatonic --pulses 5 --steps 16 --seed 42`
- Prints ASCII visual pattern grid and step note details.

### 7. Integration Tests: `plugin303/tests/generator.rs`
- Determinism tests (same seed -> identical `Pattern`).
- Euclidean pulse density accuracy tests.
- Polyrhythmic phase alignment & loop wrap mutation tests.

---

## Existing Codebase Reuse

- Reuses `Pattern`, `PitchEntry`, `TimeEntry`, `MAX_STEPS` from `plugin303::dsp::sequencer`.
- Reuses egui UI widgets (`Knob`, `Switch`, `StepButton`) from `plugin303::gui::widgets`.
- Reuses `Timebase` and parameters from `plugin303::params`.

---

## Verification & Testing Plan

1. **Unit & Property Tests**:
   - Run `cargo test --package plugin303 --test generator` to test:
     - Bjorklund Euclidean distribution across all $(k, n)$ pairs ($1 \le k \le n \le 16$).
     - Scale interval correctness and octave bounding.
     - Determinism with fixed random seeds.
     - Mutation rate scaling and boundary conditions.

2. **CLI Output Verification**:
   - Run `cargo run --package plugin303 --bin gen_acid` to visually inspect ASCII pattern generation output.

3. **DSP Sequencer Integration Test**:
   - Run `cargo test --package plugin303 --test sequencer` to confirm generated `Pattern` payloads drive `Sequencer` and voice triggers properly.
