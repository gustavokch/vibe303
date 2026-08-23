# MIDI Output and Full Synth Bypass Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add sample-accurate MIDI Out capability for the built-in 16-step sequencer and an optional Full Synth Bypass mode to turn the plugin into a zero-DSP hardware/software MIDI sequencer.

**Architecture:** Advertise `Plugin::MIDI_OUTPUT = MidiConfig::Basic` in NIH-plug; translate `SeqEvent` triggers, slides, and gate-offs into sample-accurate `NoteEvent::NoteOn` and `NoteEvent::NoteOff` events dispatched via `ProcessContext::send_event`; provide a `synth_bypass` parameter that skips all voice DSP processing and zeros audio output for minimal CPU consumption; add UI controls in the Sequencer Transport panel.

**Tech Stack:** Rust, `nih-plug` (audio plugin framework, VST3/CLAP), `egui` / `nih_plug_egui` (GUI), `assert_no_alloc` (real-time safety gating).

**Spec:** `plugin303` architecture specification.

---

## Global Constraints

- Real-time safety: zero memory allocations on the audio thread (`process()`, `reset()`).
- Sample-accurate MIDI timing: `NoteEvent` timing must match `SeqEvent` sample offset.
- Parameter persistence: all new parameters must derive `Params` with unique IDs.
- No stuck notes: ensure `NoteOff` is dispatched on note transitions, gate-offs, resets, and transport stops.
- Clean GUI integration: match existing dark aesthetic, 24px minimum interactive hit targets, and color tokens from `theme.rs`.

---

## Context

Users want to use TECHNEO 303's authentic Roland 303 sequencer (pattern generation, slide mechanics, accent handling, swing, jitter) to drive external hardware synths, modular gear, or other software instruments in their DAW. Additionally, when using the plugin purely as a MIDI sequencer, running the CPU-intensive 4× oversampled circuit model (nodal ladder Newton solver, warm clipper, etc.) is unnecessary. Adding MIDI Out with an optional Full Synth Bypass allows zero-DSP operation (~0% CPU) while sending sample-accurate MIDI notes to the DAW.

---

## Key Files to Create / Modify

- `plugin303/src/params.rs`: Add `midi_out`, `midi_out_channel`, and `synth_bypass` parameters.
- `plugin303/src/lib.rs`: Set `Plugin::MIDI_OUTPUT`, track active sequencer MIDI note, dispatch `NoteEvent`s in `process()`, bypass voice DSP when `synth_bypass` is active.
- `plugin303/src/gui/panels/sequencer_panel.rs`: Add MIDI Out toggle, MIDI Channel selector, and Synth Bypass toggle to the Transport & Pattern control section.
- `plugin303/tests/midi_out.rs`: New comprehensive integration tests for MIDI event generation, timing accuracy, channel routing, and synth bypass audio zeroing.

---

### Task 1: Add MIDI Out and Synth Bypass Parameters

**Files:**
- Modify: `plugin303/src/params.rs:276-386,410-440`
- Test: `plugin303/tests/midi_out.rs`

**Interfaces:**
- Consumes: `nih_plug::prelude::{BoolParam, IntParam, IntRange, MakeParam}`
- Produces: `params.midi_out: BoolParam`, `params.midi_out_channel: IntParam`, `params.synth_bypass: BoolParam`

- [ ] **Step 1: Write the failing test**

Create `plugin303/tests/midi_out.rs`:
```rust
use nih_plug::prelude::*;
use plugin303::params::Plugin303Params;

#[test]
fn test_midi_and_bypass_params_exist_and_default() {
    let params = Plugin303Params::default();

    assert_eq!(params.midi_out.value(), false);
    assert_eq!(params.midi_out_channel.value(), 1);
    assert_eq!(params.synth_bypass.value(), false);

    // Verify channel range
    let ch_param = &params.midi_out_channel;
    assert_eq!(ch_param.default_value(), 1);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test midi_out`
Expected: FAIL with missing fields `midi_out`, `midi_out_channel`, `synth_bypass`.

- [ ] **Step 3: Implement the parameters in `params.rs`**

In `plugin303/src/params.rs` in `Plugin303Params`:
```rust
    // --- MIDI & Routing ---
    #[id = "midi_out"]
    pub midi_out: BoolParam,
    #[id = "midi_out_channel"]
    pub midi_out_channel: IntParam,
    #[id = "synth_bypass"]
    pub synth_bypass: BoolParam,
```

And in `Plugin303Params::default()`:
```rust
        Self {
            // ... existing fields ...
            midi_out: BoolParam::new("MIDI Out", false),
            midi_out_channel: IntParam::new(
                "MIDI Out Ch",
                1,
                IntRange::Linear { min: 1, max: 16 },
            ),
            synth_bypass: BoolParam::new("Synth Bypass", false),
            // ... remaining fields ...
        }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test midi_out`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add plugin303/src/params.rs plugin303/tests/midi_out.rs
git commit -m "feat(params): add midi_out, midi_out_channel, and synth_bypass parameters"
```

---

### Task 2: Configure Plugin MIDI Output and Note Helper

**Files:**
- Modify: `plugin303/src/lib.rs:170-190,40-80`
- Test: `plugin303/tests/midi_out.rs`

**Interfaces:**
- Consumes: `nih_plug::prelude::MidiConfig::Basic`
- Produces: `Plugin303::MIDI_OUTPUT = MidiConfig::Basic`, `active_seq_midi_note: Option<u8>`, `seq_pitch_to_midi_note(key: i8, octave: i8) -> u8`

- [ ] **Step 1: Write the failing test**

Add to `plugin303/tests/midi_out.rs`:
```rust
use plugin303::dsp::sequencer::SeqEvent;
use plugin303::seq_pitch_to_midi_note;

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
    assert_eq!(seq_pitch_to_midi_note(0, -5), 0);
    assert_eq!(seq_pitch_to_midi_note(0, 10), 127);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test midi_out -- test_pitch_to_midi_note_mapping`
Expected: FAIL with cannot find function `seq_pitch_to_midi_note`.

- [ ] **Step 3: Implement MIDI output configuration and note conversion helper**

In `plugin303/src/lib.rs`:
```rust
/// Convert sequencer step key (0..12) and octave (-2..2) to standard MIDI note number (0..127).
pub fn seq_pitch_to_midi_note(key: i8, octave: i8) -> u8 {
    let note = 60 + key as i16 + (octave as i16 * 12);
    note.clamp(0, 127) as u8
}
```

In `struct Plugin303`:
```rust
    /// Tracked active MIDI note generated by the internal sequencer.
    active_seq_midi_note: Option<u8>,
```

Initialize in `Plugin303::new_with_params`:
```rust
    active_seq_midi_note: None,
```

And in `Plugin` implementation in `plugin303/src/lib.rs`:
```rust
    const MIDI_INPUT: MidiConfig = MidiConfig::Basic;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::Basic;
```

In `initialize()` and `reset()`:
```rust
    self.active_seq_midi_note = None;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test midi_out`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add plugin303/src/lib.rs plugin303/tests/midi_out.rs
git commit -m "feat(midi): advertise MIDI_OUTPUT and add note mapping helper"
```

---

### Task 3: Implement Sequencer MIDI Out Event Dispatching

**Files:**
- Modify: `plugin303/src/lib.rs:370-445`
- Test: `plugin303/tests/midi_out.rs`

**Interfaces:**
- Consumes: `ProcessContext::send_event`, `SeqEvent`, `NoteEvent`
- Produces: Sample-accurate NoteOn/NoteOff dispatch to host DAW

- [ ] **Step 1: Write integration tests for MIDI event generation**

Add to `plugin303/tests/midi_out.rs`:
```rust
use plugin303::dsp::sequencer::{Pattern, PitchEntry, Sequencer, SeqEvent, TimeEntry};
use plugin303::seq_pitch_to_midi_note;

#[test]
fn test_sequencer_generates_correct_midi_events() {
    let mut seq = Sequencer::new(48000.0);
    seq.set_running(true);

    let mut pattern = Pattern::default();
    pattern.num_steps = 4;
    // Step 0: Note C3 (60), unaccented
    pattern.pitches[0] = PitchEntry { key: 0, octave: 0, accent: false, slide: false };
    pattern.timing[0] = TimeEntry::Note;
    // Step 1: Note D#3 (63), accented, with slide
    pattern.pitches[1] = PitchEntry { key: 3, octave: 0, accent: true, slide: true };
    pattern.timing[1] = TimeEntry::Note;
    // Step 2: Note G3 (67), slide target
    pattern.pitches[2] = PitchEntry { key: 7, octave: 0, accent: false, slide: false };
    pattern.timing[2] = TimeEntry::Note;
    // Step 3: Rest
    pattern.timing[3] = TimeEntry::Rest;

    let mut events = [(0, SeqEvent::GateOff); 64];
    let mut count = 0;

    // Process a block representing 1 full beat at 120 BPM = 24000 samples (4 16th steps = 6000 samples each)
    seq.process_block(
        &pattern,
        0,
        24000,
        &mut events,
        &mut count,
        120.0,
        plugin303::params::Timebase::Straight,
        false,
    );

    assert!(count >= 4, "Expected at least 4 sequencer events, got {}", count);
    
    // Step 0: Trigger note 60
    assert_eq!(events[0].0, 0);
    assert_eq!(events[0].1, SeqEvent::Trigger { key: 0, octave: 0, accent: false });
    
    // Step 0 gate off happens before step 1
    assert_eq!(events[1].1, SeqEvent::GateOff);
    
    // Step 1: Trigger note 63 with accent and slide
    assert_eq!(events[2].0, 6000);
    assert_eq!(events[2].1, SeqEvent::Trigger { key: 3, octave: 0, accent: true });
    
    // Step 2: SlideTo note 67 (no intermediate GateOff due to slide!)
    assert_eq!(events[3].0, 12000);
    assert_eq!(events[3].1, SeqEvent::SlideTo { key: 7, octave: 0, accent: false });
}
```

- [ ] **Step 2: Run test to verify sequencer behavior**

Run: `cargo test --test midi_out -- test_sequencer_generates_correct_midi_events`
Expected: PASS

- [ ] **Step 3: Implement MIDI Out event dispatch in `Plugin303::process`**

In `plugin303/src/lib.rs` in `process()`:
```rust
        let midi_out_enabled = self.params.midi_out.value();
        let midi_channel = (self.params.midi_out_channel.value() - 1).clamp(1, 16) as u8 - 1; // 0..15
```

When handling `seq_events` during the sample loop:
```rust
            // If no MIDI note is held, run the sequencer events for this sample
            if active_midi_note.is_none() {
                while seq_event_idx < seq_event_count
                    && seq_events[seq_event_idx].0 == sample_idx as u32
                {
                    let ev = seq_events[seq_event_idx].1;
                    let timing = sample_idx as u32;

                    if midi_out_enabled {
                        match ev {
                            SeqEvent::Trigger { key, octave, accent } => {
                                // Turn off any previously held sequencer note
                                if let Some(prev_note) = self.active_seq_midi_note.take() {
                                    context.send_event(NoteEvent::NoteOff {
                                        timing,
                                        voice_id: None,
                                        channel: midi_channel,
                                        note: prev_note,
                                        velocity: 0.0,
                                    });
                                }
                                let note = seq_pitch_to_midi_note(key, octave);
                                let velocity = if accent { 1.0 } else { 0.70 };
                                context.send_event(NoteEvent::NoteOn {
                                    timing,
                                    voice_id: None,
                                    channel: midi_channel,
                                    note,
                                    velocity,
                                });
                                self.active_seq_midi_note = Some(note);
                            }
                            SeqEvent::SlideTo { key, octave, accent } => {
                                let note = seq_pitch_to_midi_note(key, octave);
                                let velocity = if accent { 1.0 } else { 0.70 };
                                // Legato overlap: Send NoteOn for target pitch first, then NoteOff for previous
                                context.send_event(NoteEvent::NoteOn {
                                    timing,
                                    voice_id: None,
                                    channel: midi_channel,
                                    note,
                                    velocity,
                                });
                                if let Some(prev_note) = self.active_seq_midi_note {
                                    if prev_note != note {
                                        context.send_event(NoteEvent::NoteOff {
                                            timing,
                                            voice_id: None,
                                            channel: midi_channel,
                                            note: prev_note,
                                            velocity: 0.0,
                                        });
                                    }
                                }
                                self.active_seq_midi_note = Some(note);
                            }
                            SeqEvent::GateOff => {
                                if let Some(prev_note) = self.active_seq_midi_note.take() {
                                    context.send_event(NoteEvent::NoteOff {
                                        timing,
                                        voice_id: None,
                                        channel: midi_channel,
                                        note: prev_note,
                                        velocity: 0.0,
                                    });
                                }
                            }
                        }
                    }

                    if !self.params.synth_bypass.value() {
                        self.voice.on_seq_event(
                            ev,
                            self.params.accent.value(),
                            self.params.decay.value() * 3000.0,
                        );
                    }
                    seq_event_idx += 1;
                }
            }
```

And on transport stop / `reset()`:
Ensure any pending note-off is cleared.

- [ ] **Step 4: Run tests to verify compilation and execution**

Run: `cargo test --test midi_out`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add plugin303/src/lib.rs plugin303/tests/midi_out.rs
git commit -m "feat(midi): dispatch sample-accurate MIDI NoteOn and NoteOff events from sequencer"
```

---

### Task 4: Implement Full Synth Audio Bypass

**Files:**
- Modify: `plugin303/src/lib.rs:258-444`
- Test: `plugin303/tests/midi_out.rs`

**Interfaces:**
- Consumes: `params.synth_bypass.value()`
- Produces: Zero DSP computation and silent output buffer when active

- [ ] **Step 1: Write the failing test**

Add to `plugin303/tests/midi_out.rs`:
```rust
#[test]
fn test_synth_bypass_flag_skips_dsp() {
    let params = std::sync::Arc::new(Plugin303Params::default());
    params.synth_bypass.set_value(true);
    let plugin = Plugin303::new_with_params(params);

    assert!(plugin.params.synth_bypass.value());
}
```

- [ ] **Step 2: Run test to verify**

Run: `cargo test --test midi_out -- test_synth_bypass_flag_skips_dsp`
Expected: PASS

- [ ] **Step 3: Implement zero-cost DSP bypass in `Plugin303::process`**

In `plugin303/src/lib.rs` in `process()`:
```rust
        let synth_bypass = self.params.synth_bypass.value();

        if !synth_bypass {
            // Expose parameters block-rate only when audio synth is active
            self.voice.update_params(
                // ... all voice params ...
            );
            self.voice.update_clipper_params(VoiceClipperParams {
                // ... clipper params ...
            });
            self.voice.set_tier(self.params.tier.value());
        }
```

In the per-sample audio rendering loop:
```rust
        if synth_bypass {
            // Zero the audio output channels with zero DSP load
            for channel_samples in buffer.as_slice() {
                channel_samples.fill(0.0);
            }
        } else {
            for frame in buffer.iter_samples() {
                let out = self.voice.tick();
                for s in frame {
                    *s = out;
                }
            }
        }
```

Note: Sequencer processing and MIDI out event dispatch remain fully functional even when `synth_bypass` is active.

- [ ] **Step 4: Run test suite to verify no audio distortion or regressions**

Run: `cargo test`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add plugin303/src/lib.rs plugin303/tests/midi_out.rs
git commit -m "feat(dsp): implement zero-cost full synth bypass mode"
```

---

### Task 5: Add MIDI Out and Synth Bypass GUI Controls to Sequencer Transport Panel

**Files:**
- Modify: `plugin303/src/gui/panels/sequencer_panel.rs:115-165`
- Test: `plugin303/tests/gui_build.rs`

**Interfaces:**
- Consumes: `params.midi_out`, `params.midi_out_channel`, `params.synth_bypass`, `ParamSetter`
- Produces: Interactive buttons/drag values in the Transport & Pattern control column

- [ ] **Step 1: Add MIDI and Bypass controls to `sequencer_panel.rs`**

In `plugin303/src/gui/panels/sequencer_panel.rs` right column (under Transport buttons):
```rust
                    // Row: MIDI Out Toggle + Channel + Synth Bypass
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = vec2(4.0, 0.0);

                        // MIDI OUT button
                        let midi_val = params.midi_out.value();
                        let midi_btn = Button::new(
                            nih_plug_egui::egui::RichText::new(if midi_val { "MIDI: ON" } else { "MIDI: OFF" })
                                .font(FontId::proportional(8.5))
                                .strong()
                                .color(if midi_val { theme::TEXT_ON_ACCENT } else { theme::TEXT_SECONDARY }),
                        )
                        .fill(if midi_val { theme::ACCENT_ORANGE } else { theme::WELL_BG })
                        .stroke(Stroke::new(1.0_f32, if midi_val { theme::ACCENT_ORANGE } else { theme::BORDER_DEFAULT }))
                        .min_size(vec2(60.0, 20.0))
                        .corner_radius(CornerRadius::same(2));

                        if ui.add(midi_btn).on_hover_text("Toggle MIDI Output from 16-step sequencer").clicked() {
                            setter.begin_set_parameter(&params.midi_out);
                            setter.set_parameter(&params.midi_out, !midi_val);
                            setter.end_set_parameter(&params.midi_out);
                        }

                        // MIDI Channel selector
                        ui.label(
                            nih_plug_egui::egui::RichText::new("CH:")
                                .font(FontId::proportional(8.0))
                                .color(theme::TEXT_MUTED),
                        );
                        let mut ch_val = params.midi_out_channel.value();
                        if ui
                            .add(
                                DragValue::new(&mut ch_val)
                                    .range(1..=16)
                                    .custom_formatter(|n, _| format!("{:02}", n as i32)),
                            )
                            .on_hover_text("MIDI Output Channel (1-16)")
                            .changed()
                        {
                            setter.begin_set_parameter(&params.midi_out_channel);
                            setter.set_parameter(&params.midi_out_channel, ch_val);
                            setter.end_set_parameter(&params.midi_out_channel);
                        }

                        ui.add_space(2.0);

                        // SYNTH BYPASS button
                        let bypass_val = params.synth_bypass.value();
                        let bypass_btn = Button::new(
                            nih_plug_egui::egui::RichText::new(if bypass_val { "SYNTH: MUTE" } else { "SYNTH: ON" })
                                .font(FontId::proportional(8.5))
                                .strong()
                                .color(if bypass_val { theme::ACCENT_RED } else { theme::TEXT_PRIMARY }),
                        )
                        .fill(if bypass_val { theme::SURFACE_ELEVATED } else { theme::WELL_BG })
                        .stroke(Stroke::new(1.0_f32, if bypass_val { theme::ACCENT_RED } else { theme::BORDER_DEFAULT }))
                        .min_size(vec2(72.0, 20.0))
                        .corner_radius(CornerRadius::same(2));

                        if ui.add(bypass_btn).on_hover_text("Bypass internal synth voice (outputs silence, 0% CPU)").clicked() {
                            setter.begin_set_parameter(&params.synth_bypass);
                            setter.set_parameter(&params.synth_bypass, !bypass_val);
                            setter.end_set_parameter(&params.synth_bypass);
                        }
                    });
```

- [ ] **Step 2: Run GUI build test**

Run: `cargo test --test gui_build`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add plugin303/src/gui/panels/sequencer_panel.rs
git commit -m "feat(gui): add MIDI Out and Synth Bypass controls to sequencer panel"
```

---

### Task 6: Real-time Safety and Full Test Suite Verification

**Files:**
- Test: `plugin303/tests/no_alloc.rs`, `plugin303/tests/midi_out.rs`

- [ ] **Step 1: Run RT allocation gate**

Run: `cargo test --test no_alloc`
Expected: 0 allocations in voice tick and process loops.

- [ ] **Step 2: Run complete workspace test suite**

Run: `cargo test`
Expected: All tests pass.

- [ ] **Step 3: Commit final plan verification**

```bash
git commit --allow-empty -m "chore: verify midi out and synth bypass test suite and rt safety"
```

---

## Verification Plan

### Automated Tests
1. `cargo test --test midi_out` — validates parameter defaults, note pitch calculation, and sequencer event generation.
2. `cargo test --test no_alloc` — ensures zero allocation violation on the audio path.
3. `cargo test --test gui_build` — ensures GUI renders and compiles without layout panics.
4. `cargo test` — full regression pass across all DSP tiers, ladder solvers, and voice tests.

### Manual Verification
1. Load plugin in DAW (Reaper / Bitwig / Ableton).
2. Create MIDI track routing TECHNEO 303 MIDI output to an external instrument / VST synth.
3. Turn `MIDI: ON` and hit `RUN`. Verify external instrument plays the 303 sequence in sync with slide and accent dynamics.
4. Toggle `SYNTH: MUTE` (Bypass). Verify TECHNEO 303 audio falls silent, CPU drops to near 0%, and external instrument continues playing uninterrupted.
