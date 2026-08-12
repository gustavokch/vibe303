# Phase 3: Sequencer-Driven Voicing Features Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement Roland TB-303 Accent, Glide/Slide, and Decay voicing systems inside the `Voice` struct, with support for both Sequencer playback and MIDI legato play.

**Architecture:** Accent is modeled as a sample-accurate leaky integrator that charges on triggers and stacks on consecutive accents, dynamically modulating cutoff frequency, VCF envelope decay time, and VCA gain. Glide is modeled as an exponential pitch slew in semitones (log-frequency CV space) managed entirely inside the `Voice` struct.

**Tech Stack:** Rust, nih-plug, assert_no_alloc, cargo test.

## Global Constraints
- Zero-allocation on the audio thread (`process()`, `tick()`, `set_gate()`, `set_freq()`, `note_on()`).
- All changes must pass the existing fidelity gates (LSD/correlation tests for Low, High, Ultra tiers).
- The compiler optimizations in release mode are required for the nodal solvers to run fast; run tests with `--release` or filter out heavy integration tests.

---

### Task 1: Accent Accumulator and Modulations in `Voice`

**Files:**
- Modify: `plugin303/src/dsp/voice.rs`
- Modify: `plugin303/src/lib.rs`
- Modify: `plugin303/src/bin/render303.rs`
- Modify: `plugin303/tests/no_alloc.rs`
- Modify: `plugin303/tests/tier_levels.rs`
- Modify: `plugin303/tests/tier_switch.rs`

**Interfaces:**
- `Voice::set_gate` signature:
  `pub fn set_gate(&mut self, gate: bool, trigger: bool, accent: bool, slide: bool)`
- `Voice::update_params` signature adds `glide_knob: f32`.

- [ ] **Step 1: Write a failing unit test for Accent Stacking**
  Add the test in a new file `plugin303/tests/voicing_features.rs`:
  ```rust
  use plugin303::dsp::voice::Voice;

  #[test]
  fn test_accent_stacking() {
      let sr = 48000.0;
      let mut v = Voice::new(sr);
      v.update_params(
          0.0, 0.0, 0.0, 0.0, plugin303::params::OscWave::Saw, 0.5, 0.0,
          0.5, 0.5, 0.5, 0.5, 1.0, 0.0, plugin303::params::VcfTaps::FourPole,
          0.0, false, 3.0, 200.0, 0.0, 200.0, 3.0, 200.0, 0.0, 200.0,
          plugin303::params::VcaEnvMode::Envelope, 0, 0.0,
          plugin303::params::CompTolerance::Zero, 0,
          plugin303::params::CircuitTopology::Accurate, 0.0, 0, 0.5
      );
      // Trigger consecutive accents
      v.set_gate(true, true, true, false);
      let initial_accent = v.get_accent_acc();
      assert!(initial_accent > 0.9, "Accent did not charge");

      // Wait a few samples
      for _ in 0..500 { v.tick(); }
      let decayed_accent = v.get_accent_acc();
      assert!(decayed_accent < initial_accent, "Accent did not decay");

      // Trigger again to stack
      v.set_gate(true, true, true, false);
      let stacked_accent = v.get_accent_acc();
      assert!(stacked_accent > initial_accent, "Accent did not stack: {} vs {}", stacked_accent, initial_accent);
  }
  ```

- [ ] **Step 2: Run test to verify it fails**
  Run: `cargo test --test voicing_features`
  Expected: fails to compile (missing `get_accent_acc()`, parameter counts, `set_gate` signature).

- [ ] **Step 3: Modify `Voice` struct fields and signatures**
  In `plugin303/src/dsp/voice.rs`:
  - Add fields:
    - `slide_active: bool`
    - `current_pitch_st: f32`
    - `target_pitch_st: f32`
    - `glide_knob: f32`
  - In `Voice::new`:
    Initialize new fields:
    ```rust
    slide_active: false,
    current_pitch_st: 60.0,
    target_pitch_st: 60.0,
    glide_knob: 0.5,
    ```
  - Expose helper `pub fn get_accent_acc(&self) -> f32 { self.accent_acc }` for testing.
  - Update `update_params` to accept `glide_knob: f32` at the end of the argument list:
    ```rust
    pub fn update_params(
        ...
        comp_seed: i32,
        glide_knob: f32,
    ) {
        ...
        self.glide_knob = glide_knob;
    }
    ```
  - Update `set_gate` to accept `slide`:
    ```rust
    pub fn set_gate(&mut self, gate: bool, trigger: bool, accent: bool, slide: bool) {
        self.gate = gate;
        self.accent_active = accent;
        self.slide_active = slide;
        if trigger {
            self.meg.trigger();
            self.veg.trigger();
            if accent {
                self.accent_acc = (self.accent_acc + 1.0).min(1.8);
            }
        }
        if !gate {
            self.meg.release();
            self.veg.release();
        }
    }
    ```
  - In `Voice::tick()`:
    - Remove the old `self.accent_acc` charging logic and replace it with:
      ```rust
      let acc_decay_coeff = (-1.0 / (0.2 * self.sr)).exp();
      self.accent_acc *= acc_decay_coeff;
      ```
    - Modulate filter decay:
      ```rust
      let decay_normal_ms = 200.0 + self.decay_knob * 1800.0;
      let effective_decay_ms = decay_normal_ms + (self.accent_knob_val * self.accent_acc).min(1.0) * (200.0 - decay_normal_ms);
      self.meg.set_decay(effective_decay_ms);
      ```
    - Modulate filter cutoff:
      ```rust
      let base_cutoff_hz = 200.0 * 20.0f32.powf(self.cutoff_knob);
      let mod_cutoff_hz = base_cutoff_hz 
          + meg_val * self.env_mod * 4000.0 
          + self.accent_knob_val * self.accent_acc * 3000.0;
      let final_cutoff_hz = (mod_cutoff_hz * cutoff_drift_factor * cutoff_thermal_factor * age_cutoff_factor)
          .clamp(20.0, self.sr * 0.45);
      ```
  - Fix references to `set_gate` and `update_params` in all files:
    - `plugin303/src/lib.rs`
    - `plugin303/src/bin/render303.rs`
    - `plugin303/tests/no_alloc.rs`
    - `plugin303/tests/tier_levels.rs`
    - `plugin303/tests/tier_switch.rs`

- [ ] **Step 4: Run test to verify it passes**
  Run: `cargo test --test voicing_features`
  Expected: PASS

- [ ] **Step 5: Commit**
  ```bash
  git add plugin303/
  git commit -m "feat(voice): implement stacking accent accumulator and VCF decay modulation"
  ```

---

### Task 2: Glide Slew in `Voice`

**Files:**
- Modify: `plugin303/src/dsp/voice.rs`
- Test: `plugin303/tests/voicing_features.rs`

**Interfaces:**
- Pitch slew uses `glide_knob` and `slide_active`.

- [ ] **Step 1: Write a failing unit test for Glide Slew**
  In `plugin303/tests/voicing_features.rs`, add:
  ```rust
  #[test]
  fn test_pitch_glide_slew() {
      let sr = 48000.0;
      let mut v = Voice::new(sr);
      // Set glide knob to 0.5 (normal) and slide active
      v.set_gate(true, true, false, true);
      v.note_on(60); // C4 (60.0)
      
      // Change note to C5 (72.0)
      v.note_on(72);
      
      // Verify pitch hasn't jumped instantly to 72
      let pitch_immediately = v.get_current_pitch();
      assert!(pitch_immediately < 71.0, "Pitch jumped instantly");
      assert!(pitch_immediately > 60.0, "Pitch did not start slewing");
      
      // Let it tick and verify it approaches 72
      for _ in 0..1000 { v.tick(); }
      let pitch_later = v.get_current_pitch();
      assert!(pitch_later > pitch_immediately, "Pitch did not rise over time");
  }
  ```

- [ ] **Step 2: Run test to verify it fails**
  Run: `cargo test --test voicing_features test_pitch_glide_slew`
  Expected: fails (missing `get_current_pitch`).

- [ ] **Step 3: Implement pitch slew in `Voice`**
  In `plugin303/src/dsp/voice.rs`:
  - Add helper `pub fn get_current_pitch(&self) -> f32 { self.current_pitch_st }`
  - In `Voice::note_on(note)`:
    ```rust
    pub fn note_on(&mut self, note: u8) {
        self.current_note = note;
        let target = note as f32;
        self.target_pitch_st = target;
        if !self.slide_active {
            self.current_pitch_st = target;
        }
        self.meg.trigger();
        self.veg.trigger();
    }
    ```
  - In `Voice::set_freq(hz)`:
    ```rust
    pub fn set_freq(&mut self, hz: f32) {
        let target = 12.0 * (hz / 440.0).log2() + 69.0;
        self.target_pitch_st = target;
        if !self.slide_active {
            self.current_pitch_st = target;
        }
    }
    ```
  - In `Voice::tick()`:
    - At the start of the sample loop, compute pitch slew:
      ```rust
      let glide_time_ms = 10.0 + self.glide_knob * 400.0;
      let glide_alpha = 1.0 - (-1.0 / (glide_time_ms * 0.001 * self.sr)).exp();
      self.current_pitch_st += glide_alpha * (self.target_pitch_st - self.current_pitch_st);
      
      let osc1_f = 440.0 * 2.0f32.powf((self.current_pitch_st - 69.0 + self.osc1_pitch_st + tuning_st + pitch_drift_st + pitch_thermal_st) / 12.0);
      ```
    - Replace the old `osc1_f` / `osc2_f` / `sub_f` calculations to use the new `self.current_pitch_st` base:
      ```rust
      self.osc.set_freq(osc1_f, self.sr);
      self.osc_hi.set_freq(osc1_f, self.sr);
      self.vco.set_freq(osc1_f);

      let sub_f = osc1_f * 0.5;
      self.sub_osc.set_freq(sub_f, self.sr);
      self.sub_osc_hi.set_freq(sub_f, self.sr);

      if self.osc2_vol > 0.0 {
          let osc2_f = 440.0 * 2.0f32.powf((self.current_pitch_st - 69.0 + self.osc2_pitch_st + tuning_st + pitch_drift_st + pitch_thermal_st) / 12.0);
          self.osc2.set_freq(osc2_f, self.sr);
          self.osc2_hi.set_freq(osc2_f, self.sr);
          self.vco2.set_freq(osc2_f);
      }
      ```

- [ ] **Step 4: Run test to verify it passes**
  Run: `cargo test --test voicing_features test_pitch_glide_slew`
  Expected: PASS

- [ ] **Step 5: Commit**
  ```bash
  git add plugin303/
  git commit -m "feat(voice): implement exponential pitch slew in pitch space"
  ```

---

### Task 3: Sequencer Slide Cleanup and MIDI Legato Slew

**Files:**
- Modify: `plugin303/src/dsp/sequencer.rs`
- Modify: `plugin303/src/lib.rs`
- Modify: `plugin303/tests/sequencer.rs`
- Test: `plugin303/tests/voicing_features.rs`

**Interfaces:**
- `Sequencer::tick` no longer slews internal frequency.
- `lib.rs` detects overlapping notes to trigger MIDI legato.

- [ ] **Step 1: Write a failing integration test for MIDI Legato Glide**
  In `plugin303/tests/voicing_features.rs`, add:
  ```rust
  #[test]
  fn test_midi_legato_glide() {
      // We will verify that in lib.rs process flow, playing overlapping MIDI notes
      // triggers pitch slide automatically. We can test this by running a mock process loop.
      // We can also test it by calling set_gate and note_on sequentially.
  }
  ```
  *(Let's refine the test code in implementation to match nih-plug's process invocation, or test `Voice` directly for legato transition helper).*

- [ ] **Step 2: Run test to verify it fails**
  Run: `cargo test --test voicing_features test_midi_legato_glide`
  Expected: FAIL

- [ ] **Step 3: Modify `Sequencer` and `lib.rs`**
  In `plugin303/src/dsp/sequencer.rs`:
  - Add field: `pub slide: bool` to the sequencer.
  - In `Sequencer::tick`:
    - Remove `glide_alpha` and the `sliding` pitch slew logic.
    - Set `self.freq = self.target_freq;`
    - Calculate and set `self.slide`:
      ```rust
      let prev_step = steps[(step_idx + length - 1) % length];
      self.slide = prev_step.slide && prev_step.gate != StepGate::Rest;
      ```
  In `plugin303/src/lib.rs`:
  - Pass `self.params.glide.value()` as the last argument to `self.voice.update_params(...)`.
  - In the MIDI note handler block inside `process()`:
    - Detect legato:
      ```rust
      NoteEvent::NoteOn { note, .. } => {
          let is_legato = active_midi_note.is_some();
          active_midi_note = Some(note);
          
          let slide = is_legato && self.params.glide.value() > 0.0;
          self.voice.set_gate(true, true, false, slide);
          self.voice.note_on(note);
      }
      ```

- [ ] **Step 4: Run test to verify it passes**
  Run: `cargo test --test voicing_features` and `cargo test --test sequencer`
  Expected: PASS

- [ ] **Step 5: Commit**
  ```bash
  git add plugin303/
  git commit -m "feat(plugin): delegate slide to voice and add MIDI legato glide support"
  ```
