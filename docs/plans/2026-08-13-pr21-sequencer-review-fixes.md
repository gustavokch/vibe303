# PR #21 Review Fixes: TB-303 Sequencer Modeling Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Resolve all review findings on PR #21 (TB-303 sequencer modeling), fixing multi-block sample tracking in `Plugin303::process`, grid-to-queue pitch alignment in `SequencerPattern::compile`, DAW transport seek/loop synchronization, lock-free pattern reads on the audio thread, and exposing timebase/jitter parameters.

**Architecture:** Maintain a running sample counter in `Plugin303` to drive block-segmented scheduling in `Sequencer::process_block`; pack note-step pitches sequentially in `SequencerPattern::compile` to align grid steps with decoupled playback; add transport phase-sync logic for loop/seek alignment; and use `try_read()` with local caching for wait-free RT audio processing.

**Tech Stack:** Rust (edition 2021), `nih-plug`, `assert_no_alloc`.

## Global Constraints

- **Zero Allocations on Audio Thread:** All audio path and event processing in `process()` and `reset()` must remain 100% allocation-free (`assert_no_alloc`).
- **Sample-Accurate & Drift-Free:** Sequencer step scheduling and gate on/off events must remain sample-accurate across arbitrary block sizes (64..8192).
- **TB-303 Circuit & Firmware Parity:** Retain authentic 50% / 62.5% gate duties, 1-step slide lookahead, 60 ms ($\tau = 12\text{ ms}$) pitch glide, and accent charge pump dynamics.
- **Lock-Free Audio Thread:** No blocking locks (`RwLock::read()`) on the audio callback path.

---

## File Structure

- **Modify:** `plugin303/src/dsp/sequencer.rs`
  - Implement `sync_to_transport(pos_samples, bpm, timebase, num_steps)` to re-align phase and step indices on transport jumps/loops.
- **Modify:** `plugin303/src/params.rs`
  - Update `SequencerPattern::compile` to pack note pitches into the pitch queue so visual step-grid inputs map to hardware decoupled playback without pitch skew.
  - Add `seq_timebase: EnumParam<Timebase>` and `seq_jitter: BoolParam` to `Plugin303Params`.
- **Modify:** `plugin303/src/lib.rs`
  - Add `current_sample_pos: u64` and cached `compiled_pattern: Pattern` to `Plugin303`.
  - Pass `current_sample_pos` and advance it by `buffer.samples() as u64` per block.
  - Call `sync_to_transport` when transport position jumps or loops.
  - Use `try_read()` to update cached pattern without blocking RT thread.
  - Wire `seq_timebase` and `seq_jitter` parameters to `process_block`.
- **Modify:** `plugin303/tests/sequencer.rs`
  - Add multi-block chunked processing test (512 samples per block across 16 steps).
  - Add test for `SequencerPattern::compile` with rests and ties.
  - Add transport jump / loop synchronization test.
- **Modify:** `plugin303/tests/params_tier.rs`
  - Add tests for `seq_timebase` and `seq_jitter` parameter definitions and defaults.

---

### Task 1: Fix `process_block` Sample Positioning & Multi-Block Scheduling

**Files:**
- Modify: `plugin303/src/lib.rs:145-265`
- Test: `plugin303/tests/sequencer.rs`

**Interfaces:**
- Consumes: `Sequencer::process_block(&mut self, pattern: &Pattern, block_start: u64, num_samples: u32, ...)`
- Produces: `Plugin303.current_sample_pos: u64` tracking absolute sample time across audio blocks.

- [ ] **Step 1: Write the failing multi-block integration test in `tests/sequencer.rs`**

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
    // 4 notes: Step 0 (0 samples), Step 1 (6000 samples), Step 2 (12000 samples), Step 3 (18000 samples)
    pat.timing[0] = TimeEntry::Note;
    pat.timing[1] = TimeEntry::Note;
    pat.timing[2] = TimeEntry::Note;
    pat.timing[3] = TimeEntry::Note;

    let chunk_size = 512;
    let total_samples = 24000;
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

    // Expect 4 Triggers (at 0, 6000, 12000, 18000) and 4 GateOffs (at 3000, 9000, 15000, 21000)
    assert_eq!(all_events.len(), 8, "Expected 8 events across multi-block run, got {:?}", all_events);
    assert_eq!(all_events[0].0, 0); // Trigger 0
    assert_eq!(all_events[1].0, 3000); // GateOff 0
    assert_eq!(all_events[2].0, 6000); // Trigger 1
    assert_eq!(all_events[3].0, 9000); // GateOff 1
    assert_eq!(all_events[4].0, 12000); // Trigger 2
    assert_eq!(all_events[5].0, 15000); // GateOff 2
    assert_eq!(all_events[6].0, 18000); // Trigger 3
    assert_eq!(all_events[7].0, 21000); // GateOff 3
}
```

- [ ] **Step 2: Run test to verify it passes for `Sequencer` alone**

Run: `cargo test -p plugin303 --test sequencer test_sequencer_multiblock_chunked_processing`
Expected: PASS (proving `Sequencer::process_block` works correctly when given monotonically increasing `block_start`).

- [ ] **Step 3: Update `Plugin303` in `src/lib.rs` to maintain `current_sample_pos`**

Add `current_sample_pos: u64` to `Plugin303`:
```rust
pub struct Plugin303 {
    params: Arc<Plugin303Params>,
    voice: Voice,
    sequencer: Sequencer,
    active_midi_note: Option<u8>,
    reported_latency: u32,
    sample_rate: f32,
    current_sample_pos: u64,
    cached_pattern: Pattern,
}
```

In `Plugin303::initialize`:
```rust
self.current_sample_pos = 0;
```

In `Plugin303::reset`:
```rust
self.current_sample_pos = 0;
```

In `Plugin303::process`:
```rust
        let block_len = buffer.samples() as u32;
        if let Some(pat) = &pattern {
            self.sequencer.process_block(
                pat,
                self.current_sample_pos,
                block_len,
                &mut seq_events,
                &mut seq_event_count,
                transport.tempo.unwrap_or(seq_tempo),
                Timebase::Straight,
                false,
            );
        }
        self.current_sample_pos += block_len as u64;
```

- [ ] **Step 4: Run all tests to verify no regressions**

Run: `cargo test -p plugin303`
Expected: All tests PASS.

- [ ] **Step 5: Commit changes**

```bash
git add plugin303/src/lib.rs plugin303/tests/sequencer.rs
git commit -m "fix(sequencer): track cumulative sample position across audio blocks"
```

---

### Task 2: Fix Grid-to-Queue Pitch Compilation in `SequencerPattern::compile`

**Files:**
- Modify: `plugin303/src/params.rs:110-135`
- Test: `plugin303/tests/sequencer.rs`

**Interfaces:**
- Consumes: `SequencerPattern.steps: [SequencerStep; 16]`
- Produces: `SequencerPattern::compile(&self, num_steps: u8) -> Pattern` with note steps mapped 1:1 to pitch queue entries.

- [ ] **Step 1: Write failing unit test for `compile` with interleaved rests in `tests/sequencer.rs`**

```rust
#[test]
fn test_sequencer_pattern_compile_with_rests() {
    let mut seq_pat = SequencerPattern::default();
    // Step 0: Note C (semitone 0)
    seq_pat.steps[0].gate = StepGate::Note;
    seq_pat.steps[0].semitone = 0;
    // Step 1: Rest (semitone set to 7 in UI)
    seq_pat.steps[1].gate = StepGate::Rest;
    seq_pat.steps[1].semitone = 7;
    // Step 2: Note E (semitone 4)
    seq_pat.steps[2].gate = StepGate::Note;
    seq_pat.steps[2].semitone = 4;

    let compiled = seq_pat.compile(3);
    assert_eq!(compiled.timing[0], TimeEntry::Note);
    assert_eq!(compiled.timing[1], TimeEntry::Rest);
    assert_eq!(compiled.timing[2], TimeEntry::Note);

    // Pitches queue entry 0 must be C (0), entry 1 must be E (4)
    assert_eq!(compiled.pitches[0].key, 0);
    assert_eq!(compiled.pitches[1].key, 4, "Pitch queue index 1 should be step 2's note (E)");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p plugin303 --test sequencer test_sequencer_pattern_compile_with_rests`
Expected: FAIL with `compiled.pitches[1].key == 7 != 4`.

- [ ] **Step 3: Update `compile()` in `src/params.rs`**

```rust
impl SequencerPattern {
    pub fn compile(&self, num_steps: u8) -> crate::dsp::sequencer::Pattern {
        let mut pitches = [crate::dsp::sequencer::PitchEntry::default(); 16];
        let mut timing = [crate::dsp::sequencer::TimeEntry::Rest; 16];
        let active_steps = num_steps.clamp(1, 16) as usize;
        let mut pitch_write_idx = 0;

        for i in 0..active_steps {
            timing[i] = match self.steps[i].gate {
                StepGate::Note => {
                    pitches[pitch_write_idx] = crate::dsp::sequencer::PitchEntry {
                        key: self.steps[i].semitone,
                        octave: self.steps[i].octave,
                        accent: self.steps[i].accent,
                        slide: self.steps[i].slide,
                    };
                    pitch_write_idx += 1;
                    crate::dsp::sequencer::TimeEntry::Note
                }
                StepGate::Tie => crate::dsp::sequencer::TimeEntry::Tie,
                StepGate::Rest => crate::dsp::sequencer::TimeEntry::Rest,
            };
        }

        // Fill remaining unused pitch slots with defaults or first note
        for i in pitch_write_idx..16 {
            pitches[i] = pitches[0];
        }

        crate::dsp::sequencer::Pattern {
            pitches,
            timing,
            num_steps: active_steps as u8,
        }
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p plugin303 --test sequencer test_sequencer_pattern_compile_with_rests`
Expected: PASS.

- [ ] **Step 5: Commit changes**

```bash
git add plugin303/src/params.rs plugin303/tests/sequencer.rs
git commit -m "fix(sequencer): pack note pitches sequentially during pattern compilation"
```

---

### Task 3: Implement Host Transport Loop & Seek Sync in `Sequencer`

**Files:**
- Modify: `plugin303/src/dsp/sequencer.rs:120-140`
- Modify: `plugin303/src/lib.rs:230-260`
- Test: `plugin303/tests/sequencer.rs`

**Interfaces:**
- Consumes: `Sequencer::sync_to_transport(&mut self, pos_samples: u64, bpm: f64, timebase: Timebase, pattern: &Pattern)`
- Produces: Resynchronized `step_idx`, `pitch_idx`, and `next_step_sample` aligned to host DAW position.

- [ ] **Step 1: Write unit test for `sync_to_transport` in `tests/sequencer.rs`**

```rust
#[test]
fn test_sequencer_transport_sync_and_seek() {
    let mut seq = Sequencer::new(48000.0);
    seq.set_running(true);
    let mut pat = Pattern {
        pitches: [PitchEntry::default(); 16],
        timing: [TimeEntry::Note; 16],
        num_steps: 16,
    };
    for i in 0..16 {
        pat.pitches[i].key = i as i8;
    }

    // Step duration at 120 BPM = 6000 samples.
    // Seek transport to sample 13000 (which is step 2 at offset 1000).
    seq.sync_to_transport(13000, 120.0, Timebase::Straight, &pat);

    let mut events = [(0, SeqEvent::GateOff); 64];
    let mut count = 0;
    // Process block of 5500 samples (samples 13000..18500)
    // Next step boundary should be step 3 at sample 18000 (offset 5000 from block start 13000).
    seq.process_block(&pat, 13000, 5500, &mut events, &mut count, 120.0, Timebase::Straight, false);

    assert!(count >= 1);
    let (offset, ev) = events[count - 1];
    assert_eq!(offset, 5000, "Next step boundary should be scheduled at offset 5000");
    if let SeqEvent::Trigger { key, .. } = ev {
        assert_eq!(key, 3, "Step 3 should trigger key 3");
    } else {
        panic!("Expected Trigger for step 3");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p plugin303 --test sequencer test_sequencer_transport_sync_and_seek`
Expected: FAIL (`sync_to_transport` method not found).

- [ ] **Step 3: Implement `sync_to_transport` in `src/dsp/sequencer.rs` and wire in `src/lib.rs`**

In `src/dsp/sequencer.rs`:
```rust
    pub fn sync_to_transport(
        &mut self,
        pos_samples: u64,
        bpm: f64,
        timebase: Timebase,
        pattern: &Pattern,
    ) {
        let num_steps = (pattern.num_steps as usize).clamp(1, 16);
        let sps = self.samples_per_step(bpm, timebase);
        let pos_f64 = pos_samples as f64;
        let total_steps = (pos_f64 / sps).floor();
        let step_in_pat = (total_steps as usize) % num_steps;
        let step_start_sample = total_steps * sps;
        let next_step = step_start_sample + sps;

        self.step_idx = step_in_pat;
        self.next_step_sample = next_step;
        self.gate_off_sample = None;
        self.prev_note_slide = false;
        self.prev_was_sounding = false;

        // Calculate pitch queue index at this step
        let mut p_idx = 0;
        for i in 0..step_in_pat {
            if pattern.timing[i] == TimeEntry::Note {
                p_idx = (p_idx + 1) % num_steps;
            }
        }
        self.pitch_idx = p_idx;
    }
```

In `src/lib.rs:process()`:
```rust
        let transport = context.transport();
        let is_running = seq_run && (transport.playing || !self.params.seq_sync.value());
        self.sequencer.set_running(is_running);

        let tempo = transport.tempo.unwrap_or(seq_tempo);
        let timebase = self.params.seq_timebase.value();
        let hw_jitter = self.params.seq_jitter.value();

        if self.params.seq_sync.value() && transport.playing {
            if let Some(pos_samples) = transport.pos_samples {
                let pos_u64 = pos_samples.max(0) as u64;
                // Detect transport jump/discontinuity > 1 buffer
                if (pos_u64 as i64 - self.current_sample_pos as i64).abs() > buffer.samples() as i64 {
                    if let Some(pat) = &pattern {
                        self.sequencer.sync_to_transport(pos_u64, tempo, timebase, pat);
                    }
                    self.current_sample_pos = pos_u64;
                }
            }
        }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p plugin303 --test sequencer test_sequencer_transport_sync_and_seek`
Expected: PASS.

- [ ] **Step 5: Commit changes**

```bash
git add plugin303/src/dsp/sequencer.rs plugin303/src/lib.rs plugin303/tests/sequencer.rs
git commit -m "feat(sequencer): implement host transport loop and seek synchronization"
```

---

### Task 4: Lock-Free Audio Thread Pattern Reading with Local Cache

**Files:**
- Modify: `plugin303/src/lib.rs:225-255`
- Test: `plugin303/tests/no_alloc.rs`

**Interfaces:**
- Consumes: `self.params.pattern.try_read()`
- Produces: Wait-free, zero-blocking pattern updates during audio processing.

- [ ] **Step 1: Check existing `no_alloc` tests**

Run: `cargo test -p plugin303 --test no_alloc`
Expected: PASS.

- [ ] **Step 2: Update `Plugin303::process` to use `try_read()` with local fallback**

In `src/lib.rs`:
```rust
        let seq_run = self.params.seq_run.value();
        if seq_run {
            if let Ok(guard) = self.params.pattern.try_read() {
                self.cached_pattern = guard.compile(self.params.seq_length.value() as u8);
            }
        }
```

Then in `process_block`:
```rust
        if seq_run {
            self.sequencer.process_block(
                &self.cached_pattern,
                self.current_sample_pos,
                block_len,
                &mut seq_events,
                &mut seq_event_count,
                tempo,
                timebase,
                hw_jitter,
            );
        }
```

- [ ] **Step 3: Run `cargo test -p plugin303 --test no_alloc` to verify zero-alloc safety**

Run: `cargo test -p plugin303 --test no_alloc`
Expected: PASS.

- [ ] **Step 4: Commit changes**

```bash
git add plugin303/src/lib.rs
git commit -m "perf(sequencer): use try_read and cached pattern for RT-safe audio thread access"
```

---

### Task 5: Expose Timebase & Authentic Jitter Parameters

**Files:**
- Modify: `plugin303/src/params.rs`
- Modify: `plugin303/src/lib.rs`
- Test: `plugin303/tests/params_tier.rs`

**Interfaces:**
- Consumes: `Plugin303Params.seq_timebase`, `Plugin303Params.seq_jitter`
- Produces: Exposed VST3/CLAP parameters for straight/triplet timebase and authentic 1.8ms ISR timing jitter.

- [ ] **Step 1: Write test for new params in `tests/params_tier.rs`**

```rust
#[test]
fn test_sequencer_parameters() {
    let params = Plugin303Params::default();
    assert_eq!(params.seq_timebase.value(), Timebase::Straight);
    assert_eq!(params.seq_jitter.value(), false);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p plugin303 --test params_tier test_sequencer_parameters`
Expected: FAIL (missing fields `seq_timebase` and `seq_jitter`).

- [ ] **Step 3: Add parameters to `Plugin303Params` and derive `Enum` for `Timebase`**

In `src/dsp/sequencer.rs`:
```rust
use nih_plug::prelude::Enum;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Enum)]
pub enum Timebase {
    #[default]
    #[name = "16th (Straight)"]
    Straight,
    #[name = "Triplet"]
    Triplet,
}
```

In `src/params.rs`:
```rust
    #[id = "seq_timebase"]
    pub seq_timebase: EnumParam<Timebase>,
    #[id = "seq_jitter"]
    pub seq_jitter: BoolParam,
```
And in `Plugin303Params::default()`:
```rust
    seq_timebase: EnumParam::new("Seq Timebase", Timebase::Straight),
    seq_jitter: BoolParam::new("Authentic CPU Jitter", false),
```

In `src/lib.rs:process()`:
Pass `self.params.seq_timebase.value()` and `self.params.seq_jitter.value()` to `process_block`.

- [ ] **Step 4: Run all crate tests**

Run: `cargo test -p plugin303`
Expected: All tests PASS.

- [ ] **Step 5: Commit changes**

```bash
git add plugin303/src/dsp/sequencer.rs plugin303/src/params.rs plugin303/src/lib.rs plugin303/tests/params_tier.rs
git commit -m "feat(params): expose sequencer timebase and authentic cpu jitter parameters"
```

---

## Verification Plan

### Automated Tests
1. **Multi-block chunking:** `cargo test -p plugin303 --test sequencer test_sequencer_multiblock_chunked_processing`
2. **Pitch queue packing:** `cargo test -p plugin303 --test sequencer test_sequencer_pattern_compile_with_rests`
3. **Transport sync:** `cargo test -p plugin303 --test sequencer test_sequencer_transport_sync_and_seek`
4. **All test gates:** `cargo test -p plugin303`
5. **Zero allocations:** `cargo test -p plugin303 --test no_alloc`

### Manual Verification
- Test VST3/CLAP build bundling: `cargo xtask bundle plugin303 --release`
