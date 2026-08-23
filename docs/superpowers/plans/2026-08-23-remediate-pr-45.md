# PR 45 Remediation Implementation Plan

**Goal:** Fix all code review findings on PR #45 (MIDI Out stuck note edge cases, channel tracking, synth bypass voice lifecycle, and GUI button hit target size).

**Architecture:**
- Track `active_seq_midi_note` as `Option<(u8, u8)>` (channel, note) so NoteOff events are dispatched to the channel where NoteOn originated, preventing stuck notes on channel changes.
- Handle `SeqEvent::SlideTo` gracefully when sliding to the same pitch: avoid duplicate NoteOn events on the same note without NoteOff.
- When `midi_out` is toggled off while a note is active, flush `NoteOff` immediately at timing 0.
- When `synth_bypass` is active, maintain voice gate state or reset voice on transition to avoid pops/stuck sound on unbypass.
- Adjust sequencer transport button height to 24px minimum hit target in GUI.

**Tech Stack:** Rust, `nih-plug`, `egui`, `cargo test`.

---

## Tasks

### Task 1: Fix MIDI Out Channel Tracking & Slide Duplicate NoteOn
- **Target:** `plugin303/src/lib.rs`
- **Changes:**
  - Change `active_seq_midi_note` field to `Option<(u8, u8)>`.
  - Fix `SlideTo` logic: if `prev_note == note`, retain existing note; if `prev_note != note`, send `NoteOn` then `NoteOff` on `prev_channel`.
  - Fix `Trigger` / `GateOff` / transport stop to use `prev_channel` from `active_seq_midi_note`.
  - Flush `NoteOff` immediately when `!midi_out_enabled && active_seq_midi_note.is_some()`.

### Task 2: Fix Synth Bypass Voice State Transitions
- **Target:** `plugin303/src/lib.rs`
- **Changes:**
  - In MIDI event loop and sequencer event loop, ensure voice gate state stays consistent or is cleanly managed when `synth_bypass` is toggled.
  - Reset or clear voice gate on bypass to prevent stuck oscillators.

### Task 3: Sequencer Panel Hit Target Size
- **Target:** `plugin303/src/gui/panels/sequencer_panel.rs`
- **Changes:**
  - Set MIDI Out, Sync, Run, and Synth Bypass button heights to 24.0px.

### Task 4: Add Verification Tests
- **Target:** `plugin303/tests/midi_out.rs`
- **Changes:**
  - Add unit tests for same-pitch slide, channel change tracking, and MIDI out disable flushing.
