# TB-303 Sequencer Engine & Host Integration Design

## 1. Overview & Objectives

This specification defines the finalized architecture for the TB-303 pattern sequencer in `vibe303` (addressing review findings on PR #21). The goal is to provide a rock-solid, sample-accurate, drift-free 16-step sequencer integrated into the `nih-plug` audio loop with:

1. **Monotonic Multi-Block Scheduling**: Accurate event timing across arbitrary audio block sizes (64..8192 samples).
2. **DAW Transport Phase Synchronization**: Immediate, drift-free phase and step alignment on host transport start, loop wraparounds, and timeline seeks.
3. **Dual Compilation Modes (`SequencerMode`)**:
   - `DirectGrid` (Default): Direct 1:1 mapping from step grid to note triggers (standard modern DAW workflow).
   - `DecoupledHardware`: Authentic Roland TB-303 decoupled memory behavior where pitch and timing queues advance independently.
4. **Zero-Allocation & Wait-Free Real-Time Processing**: Non-blocking `try_read()` pattern access with local caching for uninterrupted audio rendering.
5. **Full Parameter Exposure**: Exposing `seq_mode`, `seq_timebase` (Straight / Triplet), and `seq_jitter` (Roland 1.8ms CPU timer interrupt quantization).

---

## 2. Parameter Declarations & Data Model

### 2.1 Enums and Types in `params.rs`

```rust
use nih_plug::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Enum)]
pub enum SequencerMode {
    #[default]
    #[name = "Direct Grid"]
    DirectGrid,
    #[name = "Decoupled (Roland 303)"]
    DecoupledHardware,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Enum)]
pub enum Timebase {
    #[default]
    #[name = "16th (Straight)"]
    Straight,
    #[name = "12th (Triplet)"]
    Triplet,
}
```

### 2.2 Parameters in `Plugin303Params`

- `#[id = "seq_mode"] pub seq_mode: EnumParam<SequencerMode>`
- `#[id = "seq_timebase"] pub seq_timebase: EnumParam<Timebase>`
- `#[id = "seq_jitter"] pub seq_jitter: BoolParam` (default `false`)

### 2.3 Pattern Compilation (`SequencerPattern::compile`)

```rust
impl SequencerPattern {
    pub fn compile(&self, num_steps: u8, mode: SequencerMode) -> crate::dsp::sequencer::Pattern {
        let num_steps = num_steps.clamp(1, 16);
        let mut pitches = [crate::dsp::sequencer::PitchEntry::default(); 16];
        let mut timing = [crate::dsp::sequencer::TimeEntry::Rest; 16];

        match mode {
            SequencerMode::DirectGrid => {
                for i in 0..16 {
                    timing[i] = match self.steps[i].gate {
                        StepGate::Note => crate::dsp::sequencer::TimeEntry::Note,
                        StepGate::Tie => crate::dsp::sequencer::TimeEntry::Tie,
                        StepGate::Rest => crate::dsp::sequencer::TimeEntry::Rest,
                    };
                    pitches[i] = crate::dsp::sequencer::PitchEntry {
                        key: self.steps[i].semitone,
                        octave: self.steps[i].octave,
                        accent: self.steps[i].accent,
                        slide: self.steps[i].slide,
                    };
                }
            }
            SequencerMode::DecoupledHardware => {
                let mut note_count = 0;
                for i in 0..num_steps as usize {
                    timing[i] = match self.steps[i].gate {
                        StepGate::Note => {
                            pitches[note_count] = crate::dsp::sequencer::PitchEntry {
                                key: self.steps[i].semitone,
                                octave: self.steps[i].octave,
                                accent: self.steps[i].accent,
                                slide: self.steps[i].slide,
                            };
                            note_count += 1;
                            crate::dsp::sequencer::TimeEntry::Note
                        }
                        StepGate::Tie => crate::dsp::sequencer::TimeEntry::Tie,
                        StepGate::Rest => crate::dsp::sequencer::TimeEntry::Rest,
                    };
                }
            }
        }

        crate::dsp::sequencer::Pattern {
            pitches,
            timing,
            num_steps,
        }
    }
}
```

---

## 3. Sequencer Engine & Transport Synchronization

### 3.1 Transport Phase Sync in `Sequencer`

In `dsp/sequencer.rs`:

```rust
impl Sequencer {
    pub fn sync_to_transport(
        &mut self,
        pos_samples: i64,
        block_start: u64,
        bpm: f64,
        timebase: Timebase,
        num_steps: u8,
    ) {
        let step_len = self.samples_per_step(bpm, timebase);
        let loop_len = step_len * (num_steps as f64);
        if loop_len <= 0.0 {
            return;
        }

        let phase_in_loop = (pos_samples.max(0) as f64).rem_euclid(loop_len);
        self.step_idx = ((phase_in_loop / step_len).floor() as usize) % (num_steps as usize);
        let sample_into_step = phase_in_loop - (self.step_idx as f64 * step_len);

        self.next_step_sample = block_start as f64 + (step_len - sample_into_step);
        self.gate_off_sample = None;
        self.prev_note_slide = false;
        self.prev_was_sounding = false;
    }
}
```

### 3.2 Monotonic Scheduling & Multi-Block Processing

In `Plugin303`:
- Tracks `current_sample_pos: u64` and `prev_transport_sample: Option<i64>`.
- Calls `sync_to_transport` when transport position jumps or loops.
- Passes `current_sample_pos` to `Sequencer::process_block(...)` and advances it by `buffer.samples() as u64` each block.

---

## 4. RT Thread Safety & Voice Integration

### 4.1 Non-Blocking Pattern Access

- `Plugin303` maintains a local `cached_pattern: Pattern`.
- Uses `params.pattern.try_read()` on the audio thread. If busy (UI holding write lock), continues safely from `cached_pattern`.
- Pre-warmed during `Plugin303::initialize()`.

### 4.2 Sample-Accurate Event Dispatch

Inside `Plugin303::process()`, sequencer events (`SeqEvent::Trigger`, `SeqEvent::SlideTo`, `SeqEvent::GateOff`) are dispatched sample-by-sample into `Voice::on_seq_event(ev)` at their exact sub-block sample offsets.

---

## 5. Verification Plan

1. **Integration Tests (`tests/sequencer.rs`)**:
   - `test_sequencer_multiblock_chunked_processing`: Multi-block chunking (512 samples) with zero drift.
   - `test_sequencer_direct_vs_decoupled_mode`: Validates step-grid vs decoupled pitch queue indexing.
   - `test_sequencer_transport_jump_and_loop_sync`: Validates exact phase re-alignment on DAW seek/loop.
2. **Audio Path Allocation Gate (`tests/no_alloc.rs`)**:
   - Ensures zero heap allocations in `process()` with sequencer running and receiving pattern updates.
3. **Parameter Tests (`tests/params_tier.rs`)**:
   - Validates parameter IDs, bounds, and defaults for `seq_mode`, `seq_timebase`, and `seq_jitter`.
