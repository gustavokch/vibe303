# TB-303 Sequencer Engine & Host Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement sample-accurate multi-block scheduling, DAW transport phase synchronization, dual pattern compilation modes (Direct Grid and Decoupled Roland 303), and wait-free RT audio thread safety for the TB-303 sequencer in `vibe303`.

**Architecture:** Maintain a monotonic running sample counter in `Plugin303` to drive block-segmented scheduling in `Sequencer::process_block`; implement `Sequencer::sync_to_transport` for sub-sample phase realignment during DAW timeline seek and loop boundaries; add dual compilation modes in `SequencerPattern::compile`; and use `try_read()` with local pattern caching to achieve zero blocking on the audio thread.

**Tech Stack:** Rust (edition 2021), `nih-plug`, `assert_no_alloc`.

## Global Constraints

- **Zero Allocations on Audio Thread:** All audio path and event processing in `process()` and `reset()` must remain 100% allocation-free (`assert_no_alloc`).
- **Sample-Accurate & Drift-Free:** Sequencer step scheduling and gate on/off events must remain sample-accurate across arbitrary block sizes (64..8192).
- **TB-303 Circuit & Firmware Parity:** Retain authentic 50% / 62.5% gate duties, 1-step slide lookahead, 60 ms ($\tau = 12\text{ ms}$) pitch glide, and accent charge pump dynamics.
- **Lock-Free Audio Thread:** No blocking locks (`RwLock::read()`) on the audio callback path.

---

## File Structure

- **Modify:** `plugin303/src/params.rs`
  - Define `SequencerMode` enum (`DirectGrid`, `DecoupledHardware`).
  - Define `Timebase` enum (`Straight`, `Triplet`) with `#[derive(Enum)]`.
  - Add `seq_mode`, `seq_timebase`, and `seq_jitter` parameters to `Plugin303Params`.
  - Update `SequencerPattern::compile` to accept `num_steps: u8` and `mode: SequencerMode`.
- **Modify:** `plugin303/src/dsp/sequencer.rs`
  - Re-export or use `Timebase` and `SequencerMode`.
  - Implement `Sequencer::sync_to_transport(pos_samples, block_start, bpm, timebase, num_steps)`.
  - Refine `Sequencer::process_block` to correctly schedule events in absolute sample time across block chunks.
- **Modify:** `plugin303/src/lib.rs`
  - Add `current_sample_pos: u64`, `prev_transport_sample: Option<i64>`, and `cached_pattern: Pattern` to `Plugin303`.
  - Update `initialize` and `reset` to warm the cached pattern and clear transport tracking.
  - In `process`, detect transport jumps/loops to call `sync_to_transport`, use `try_read` for non-blocking pattern caching, and pass `current_sample_pos` to `process_block`.
- **Modify:** `plugin303/tests/sequencer.rs`
  - Add multi-block chunked processing test (`test_sequencer_multiblock_chunked_processing`).
  - Add direct vs decoupled compilation test (`test_sequencer_direct_vs_decoupled_mode`).
  - Add transport seek and loop phase sync test (`test_sequencer_transport_jump_and_loop_sync`).
- **Modify:** `plugin303/tests/params_tier.rs`
  - Add parameter verification for `seq_mode`, `seq_timebase`, and `seq_jitter`.
- **Modify:** `plugin303/tests/no_alloc.rs`
  - Add zero-allocation assertion for `Plugin303::process` with sequencer running.

---

### Task 1: Add Sequencer Parameters and Dual Compilation Modes

**Files:**
- Modify: `plugin303/src/params.rs:1-140`
- Test: `plugin303/tests/params_tier.rs`
- Test: `plugin303/tests/sequencer.rs`

**Interfaces:**
- Consumes: `StepGate`, `SequencerStep`, `SequencerPattern`
- Produces: `SequencerMode` enum, `Timebase` enum, `Plugin303Params.seq_mode`, `Plugin303Params.seq_timebase`, `Plugin303Params.seq_jitter`, `SequencerPattern::compile(&self, num_steps: u8, mode: SequencerMode) -> Pattern`

- [ ] **Step 1: Write the failing tests in `tests/params_tier.rs` and `tests/sequencer.rs`**

In `plugin303/tests/params_tier.rs`:
```rust
#[test]
fn test_sequencer_parameters_declared() {
    let params = Plugin303Params::default();
    assert_eq!(params.seq_mode.value(), SequencerMode::DirectGrid);
    assert_eq!(params.seq_timebase.value(), Timebase::Straight);
    assert!(!params.seq_jitter.value());
}
```

In `plugin303/tests/sequencer.rs`:
```rust
#[test]
fn test_sequencer_direct_vs_decoupled_mode() {
    let mut pat = SequencerPattern::default();
    // Step 0: Note C (semitone 0)
    pat.steps[0].gate = StepGate::Note;
    pat.steps[0].semitone = 0;
    // Step 1: Rest
    pat.steps[1].gate = StepGate::Rest;
    // Step 2: Note D (semitone 2)
    pat.steps[2].gate = StepGate::Note;
    pat.steps[2].semitone = 2;
    // Step 3: Rest
    pat.steps[3].gate = StepGate::Rest;

    // Direct Grid mode
    let compiled_direct = pat.compile(4, SequencerMode::DirectGrid);
    assert_eq!(compiled_direct.pitches[0].key, 0);
    assert_eq!(compiled_direct.pitches[2].key, 2);
    assert_eq!(compiled_direct.timing[0], TimeEntry::Note);
    assert_eq!(compiled_direct.timing[1], TimeEntry::Rest);
    assert_eq!(compiled_direct.timing[2], TimeEntry::Note);

    // Decoupled Hardware mode
    let compiled_decoupled = pat.compile(4, SequencerMode::DecoupledHardware);
    assert_eq!(compiled_decoupled.pitches[0].key, 0);
    assert_eq!(compiled_decoupled.pitches[1].key, 2); // note pitches packed sequentially
    assert_eq!(compiled_decoupled.timing[0], TimeEntry::Note);
    assert_eq!(compiled_decoupled.timing[1], TimeEntry::Rest);
    assert_eq!(compiled_decoupled.timing[2], TimeEntry::Note);
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p plugin303 --test params_tier --test sequencer`
Expected: Compile failure due to missing `SequencerMode`, `Timebase`, and updated `compile()` signature.

- [ ] **Step 3: Implement `SequencerMode`, `Timebase`, and `compile()` in `params.rs`**

In `plugin303/src/params.rs`:
```rust
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

impl Timebase {
    pub fn ticks_per_step(self) -> f64 {
        match self {
            Self::Straight => 6.0,
            Self::Triplet => 8.0,
        }
    }
    pub fn gate_on_ticks(self) -> f64 {
        match self {
            Self::Straight => 3.0,
            Self::Triplet => 5.0,
        }
    }
    pub fn steps_per_beat(self) -> f64 {
        match self {
            Self::Straight => 4.0,
            Self::Triplet => 8.0 / 3.0,
        }
    }
}
```

Add to `Plugin303Params`:
```rust
    #[id = "seq_mode"]
    pub seq_mode: EnumParam<SequencerMode>,
    #[id = "seq_timebase"]
    pub seq_timebase: EnumParam<Timebase>,
    #[id = "seq_jitter"]
    pub seq_jitter: BoolParam,
```

And update `SequencerPattern::compile`:
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

- [ ] **Step 4: Run tests to verify pass**

Run: `cargo test -p plugin303 --test params_tier --test sequencer`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add plugin303/src/params.rs plugin303/tests/params_tier.rs plugin303/tests/sequencer.rs
git commit -m "feat(sequencer): add SequencerMode, Timebase parameters and dual compile modes"
```

---

### Task 2: Implement Transport Sync and Multi-Block Scheduling in `Sequencer`

**Files:**
- Modify: `plugin303/src/dsp/sequencer.rs`
- Test: `plugin303/tests/sequencer.rs`

**Interfaces:**
- Consumes: `Pattern`, `Timebase` from `crate::params::Timebase`
- Produces: `Sequencer::sync_to_transport(&mut self, pos_samples: i64, block_start: u64, bpm: f64, timebase: Timebase, num_steps: u8)`

- [ ] **Step 1: Write failing tests for multi-block chunking and transport sync**

In `plugin303/tests/sequencer.rs`:
```rust
#[test]
fn test_sequencer_multiblock_chunked_processing() {
    let mut seq = Sequencer::new(48000.0);
    seq.set_running(true);
    let mut pat = Pattern {
        pitches: [PitchEntry::default(); 16],
        timing: [TimeEntry::Rest; 16],
        num_steps: 4,
    };
    pat.timing[0] = TimeEntry::Note;
    pat.timing[1] = TimeEntry::Note;
    pat.timing[2] = TimeEntry::Note;
    pat.timing[3] = TimeEntry::Note;

    let chunk_size = 512;
    let total_samples = 24000; // 4 steps at 120 bpm = 0.5s = 24000 samples (6000 samples per step)
    let mut current_sample: u64 = 0;
    let mut all_events = Vec::new();

    while current_sample < total_samples {
        let mut events = [(0, SeqEvent::GateOff); 64];
        let mut count = 0;
        seq.process_block(
            &pat,
            current_sample,
            chunk_size,
            &mut events,
            &mut count,
            120.0,
            Timebase::Straight,
            false,
        );

        for i in 0..count {
            let (offset, ev) = events[i];
            all_events.push((current_sample + offset as u64, ev));
        }
        current_sample += chunk_size as u64;
    }

    // 4 notes: 4 triggers + 4 gate offs = 8 events
    assert_eq!(all_events.len(), 8);
    assert_eq!(all_events[0].0, 0); // Step 0 Trigger
    assert_eq!(all_events[1].0, 3000); // Step 0 GateOff (50% duty = 3000 samples)
    assert_eq!(all_events[2].0, 6000); // Step 1 Trigger
    assert_eq!(all_events[3].0, 9000); // Step 1 GateOff
}

#[test]
fn test_sequencer_transport_jump_and_loop_sync() {
    let mut seq = Sequencer::new(48000.0);
    seq.set_running(true);

    // 120 bpm, straight timebase => 6000 samples per step.
    // Jump to 9000 samples (Step 1, exactly halfway through step).
    seq.sync_to_transport(9000, 0, 120.0, Timebase::Straight, 16);

    let pat = Pattern {
        pitches: [PitchEntry::default(); 16],
        timing: [TimeEntry::Note; 16],
        num_steps: 16,
    };

    let mut events = [(0, SeqEvent::GateOff); 64];
    let mut count = 0;
    // Process block from 0 to 4000. Next step (Step 2) is at sample 3000 relative to jump point.
    seq.process_block(
        &pat,
        0,
        4000,
        &mut events,
        &mut count,
        120.0,
        Timebase::Straight,
        false,
    );

    assert_eq!(count, 2); // Step 2 Trigger at 3000, GateOff at 6000 (which is outside 4000)
    assert_eq!(events[0].0, 3000);
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p plugin303 --test sequencer`
Expected: Compile failure / assertions failing on `sync_to_transport`.

- [ ] **Step 3: Implement `sync_to_transport` and import `Timebase` in `dsp/sequencer.rs`**

In `plugin303/src/dsp/sequencer.rs`:
```rust
pub use crate::params::Timebase;

impl Sequencer {
    pub fn sync_to_transport(
        &mut self,
        pos_samples: i64,
        block_start: u64,
        bpm: f64,
        timebase: Timebase,
        num_steps: u8,
    ) {
        let num_steps = (num_steps as usize).clamp(1, 16);
        let step_len = self.samples_per_step(bpm, timebase);
        let loop_len = step_len * (num_steps as f64);
        if loop_len <= 0.0 {
            return;
        }

        let pos = pos_samples.max(0) as f64;
        let phase_in_loop = pos.rem_euclid(loop_len);
        self.step_idx = ((phase_in_loop / step_len).floor() as usize) % num_steps;
        self.pitch_idx = self.step_idx;
        let sample_into_step = phase_in_loop - (self.step_idx as f64 * step_len);

        self.next_step_sample = block_start as f64 + (step_len - sample_into_step);
        self.gate_off_sample = None;
        self.prev_note_slide = false;
        self.prev_was_sounding = false;
    }
}
```

- [ ] **Step 4: Run tests to verify pass**

Run: `cargo test -p plugin303 --test sequencer`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add plugin303/src/dsp/sequencer.rs plugin303/tests/sequencer.rs
git commit -m "feat(sequencer): implement sync_to_transport and multi-block sample scheduling"
```

---

### Task 3: Integrate Wait-Free Audio Loop, Monotonic Tracking, and Param Wiring in `Plugin303`

**Files:**
- Modify: `plugin303/src/lib.rs`
- Test: `plugin303/tests/voice_sequencer.rs`
- Test: `plugin303/tests/no_alloc.rs`

**Interfaces:**
- Consumes: `Sequencer::process_block`, `Sequencer::sync_to_transport`, `SequencerPattern::compile`, `params.seq_mode`, `params.seq_timebase`, `params.seq_jitter`
- Produces: `Plugin303.current_sample_pos`, `Plugin303.cached_pattern`, `Plugin303.prev_transport_sample`

- [ ] **Step 1: Update `Plugin303` struct definition in `lib.rs`**

```rust
pub struct Plugin303 {
    params: std::sync::Arc<Plugin303Params>,
    voice: Voice,
    sequencer: Sequencer,
    reported_latency: u32,
    active_midi_note: Option<u8>,
    sample_rate: f32,
    current_sample_pos: u64,
    prev_transport_sample: Option<i64>,
    cached_pattern: crate::dsp::sequencer::Pattern,
}
```

- [ ] **Step 2: Update `default()`, `initialize()`, and `reset()` in `lib.rs`**

```rust
impl Default for Plugin303 {
    fn default() -> Self {
        let params = std::sync::Arc::new(Plugin303Params::default());
        let cached_pattern = params.pattern.read().unwrap().compile(
            params.seq_length.value() as u8,
            params.seq_mode.value(),
        );
        Self {
            params,
            voice: Voice::new(48000.0),
            sequencer: Sequencer::new(48000.0),
            reported_latency: 0,
            active_midi_note: None,
            sample_rate: 48000.0,
            current_sample_pos: 0,
            prev_transport_sample: None,
            cached_pattern,
        }
    }
}
```

In `initialize()`:
```rust
        if let Ok(guard) = self.params.pattern.read() {
            self.cached_pattern = guard.compile(
                self.params.seq_length.value() as u8,
                self.params.seq_mode.value(),
            );
        }
        self.current_sample_pos = 0;
        self.prev_transport_sample = None;
```

In `reset()`:
```rust
        self.voice.reset();
        self.sequencer.reset();
        self.active_midi_note = None;
        self.current_sample_pos = 0;
        self.prev_transport_sample = None;
```

- [ ] **Step 3: Update `process()` in `lib.rs` for wait-free pattern update and transport sync**

```rust
        let seq_run = self.params.seq_run.value();
        let seq_sync = self.params.seq_sync.value();
        let transport = context.transport();
        let is_running = seq_run && (transport.playing || !seq_sync);
        self.sequencer.set_running(is_running);

        if seq_run {
            if let Ok(guard) = self.params.pattern.try_read() {
                self.cached_pattern = guard.compile(
                    self.params.seq_length.value() as u8,
                    self.params.seq_mode.value(),
                );
            }
        }

        let bpm = transport.tempo.unwrap_or(self.params.seq_tempo.value() as f64);
        let timebase = self.params.seq_timebase.value();
        let jitter = self.params.seq_jitter.value();
        let num_steps = self.params.seq_length.value() as u8;

        if is_running && seq_sync && transport.playing {
            if let Some(pos_samples) = transport.pos_samples() {
                let is_discontinuous = match self.prev_transport_sample {
                    None => true,
                    Some(prev) => (pos_samples - prev).abs() > (buffer.samples() as i64 + 64),
                };
                if is_discontinuous {
                    self.sequencer.sync_to_transport(
                        pos_samples,
                        self.current_sample_pos,
                        bpm,
                        timebase,
                        num_steps,
                    );
                }
                self.prev_transport_sample = Some(pos_samples + buffer.samples() as i64);
            }
        } else {
            self.prev_transport_sample = None;
        }

        let mut seq_events = [(0, SeqEvent::GateOff); 64];
        let mut seq_event_count = 0;
        let mut seq_event_idx = 0;

        if is_running {
            self.sequencer.process_block(
                &self.cached_pattern,
                self.current_sample_pos,
                buffer.samples() as u32,
                &mut seq_events,
                &mut seq_event_count,
                bpm,
                timebase,
                jitter,
            );
        }

        // Advance monotonic sample counter
        self.current_sample_pos += buffer.samples() as u64;
```

- [ ] **Step 4: Run tests to verify pass**

Run: `cargo test -p plugin303`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add plugin303/src/lib.rs
git commit -m "feat(plugin): wire transport sync, wait-free pattern cache, and monotonic sample tracking"
```

---

### Task 4: Add Zero-Allocation Real-Time Test in `tests/no_alloc.rs`

**Files:**
- Modify: `plugin303/tests/no_alloc.rs`

- [ ] **Step 1: Write `sequencer_audio_path_is_alloc_free` in `tests/no_alloc.rs`**

```rust
#[test]
fn sequencer_audio_path_is_alloc_free() {
    let mut plugin = Plugin303::default();
    let mut aux = AuxiliaryBuffers::default();
    let mut buffer = Buffer::default();
    let mut scratch = [0.0f32; 512];
    buffer.set_slices(512, |slices| {
        slices.push(&mut scratch);
    });

    struct DummyContext;
    impl ProcessContext<Plugin303> for DummyContext {
        fn transport(&self) -> &Transport {
            static TRANSPORT: Transport = Transport::new(48000.0);
            &TRANSPORT
        }
        fn next_event(&mut self) -> Option<PluginNoteEvent<Plugin303>> {
            None
        }
        fn set_latency_samples(&self, _samples: u32) {}
        fn set_current_voice_capacity(&self, _capacity: u32) {}
    }
    let mut ctx = DummyContext;

    plugin.params.seq_run.set_value(true);

    assert_no_alloc(|| {
        for _ in 0..10 {
            plugin.process(&mut buffer, &mut aux, &mut ctx);
        }
    });
}
```

- [ ] **Step 2: Run test to verify pass**

Run: `cargo test -p plugin303 --test no_alloc`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add plugin303/tests/no_alloc.rs
git commit -m "test(no_alloc): assert zero-allocation safety for sequencer audio loop"
```

---

### Task 5: Final Verification & Full Regression Gate

**Files:**
- Test: All tests in `plugin303`

- [ ] **Step 1: Run full release test suite**

Run: `cargo test -p plugin303 --release`
Expected: All tests pass (0 failures).

- [ ] **Step 2: Run python drift guard**

Run: `uv run --no-project --with pytest python -m pytest tests/circuitgen/ -q`
Expected: PASS
