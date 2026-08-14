# TB-303 Sequencer Model Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement a sample-accurate, drift-free emulation of the TB-303 sequencer with decoupled pitch/timing memory, precise slide/accent/gate semantics, and optional CPU timing jitter.

**Architecture:** A pure `SequencerCore` decoupled from the DSP handles the 16-step grid, emitting `SeqEvent`s (Trigger, SlideTo, GateOff) sample-accurately using an `f64` drift-free block scheduler. The `lib.rs` audio thread merges these events with MIDI events and applies them sequentially to the `Voice`, which now uses a constant-time exponential pitch slew for slides and charge-pump integration for the accent sweep.

**Tech Stack:** Rust, nih-plug.

## Global Constraints

- Zero-alloc audio path: all DSP in `process()` must be allocation-free (tested via `assert_no_alloc`).
- `initialize()` is non-RT. Expensive work goes there.
- Block-rate param reads: Read nih-plug params once per block in `process()`.
- Sample-accurate MIDI and sequencer events must be applied correctly inside `process()`.
- Tests must be executed with `--release` unless debugging.
- No new dependencies can be added for the fixed event buffer; use primitive stack arrays.

---

### Task 1: Sequencer Data Model and Events

**Files:**
- Create: `plugin303/src/dsp/sequencer.rs` (overwrite existing)
- Modify: `plugin303/src/params.rs`
- Modify: `plugin303/src/dsp/mod.rs` (if necessary, though `sequencer` is already exported)

**Interfaces:**
- Produces: `PitchEntry`, `TimeEntry`, `Pattern`, `SeqEvent`, `Timebase` types.

- [ ] **Step 1: Write the failing test**

```rust
// tests/sequencer.rs
use plugin303::dsp::sequencer::{PitchEntry, TimeEntry, Pattern, SeqEvent, Timebase};

#[test]
fn test_data_model_compiles() {
    let _pattern = Pattern {
        pitches: [PitchEntry::default(); 16],
        timing: [TimeEntry::Rest; 16],
        num_steps: 16,
    };
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test sequencer -- test_data_model_compiles`
Expected: FAIL due to missing types.

- [ ] **Step 3: Write minimal implementation**

Overwrite `plugin303/src/dsp/sequencer.rs` with the new data model. Add `pub` to fields.

```rust
pub const MAX_STEPS: usize = 16;

#[derive(Clone, Copy, Default, Debug)]
pub struct PitchEntry { pub key: i8, pub octave: i8, pub accent: bool, pub slide: bool }

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TimeEntry { Note, Tie, Rest }

#[derive(Clone, Debug)]
pub struct Pattern {
    pub pitches: [PitchEntry; MAX_STEPS],
    pub timing: [TimeEntry; MAX_STEPS],
    pub num_steps: u8,
}

#[derive(Clone, Copy, Debug)]
pub enum SeqEvent {
    Trigger { key: i8, octave: i8, accent: bool },
    SlideTo { key: i8, octave: i8, accent: bool },
    GateOff,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Timebase { Straight, Triplet }
impl Timebase {
    pub fn ticks_per_step(self) -> f64 { match self { Self::Straight => 6.0, Self::Triplet => 8.0 } }
    pub fn gate_on_ticks(self)  -> f64 { match self { Self::Straight => 3.0, Self::Triplet => 5.0 } }
    pub fn steps_per_beat(self) -> f64 { match self { Self::Straight => 4.0, Self::Triplet => 8.0/3.0 } }
}
```

Add a compiler helper to `plugin303/src/params.rs` inside `impl SequencerPattern`:

```rust
// In plugin303/src/params.rs
use crate::dsp::sequencer::{Pattern, PitchEntry, TimeEntry};

impl SequencerPattern {
    pub fn compile(&self, num_steps: u8) -> Pattern {
        let mut pitches = [PitchEntry::default(); 16];
        let mut timing = [TimeEntry::Rest; 16];
        for i in 0..16 {
            timing[i] = match self.steps[i].gate {
                StepGate::Note => TimeEntry::Note,
                StepGate::Tie => TimeEntry::Tie,
                StepGate::Rest => TimeEntry::Rest,
            };
            pitches[i] = PitchEntry {
                key: self.steps[i].semitone,
                octave: self.steps[i].octave,
                accent: self.steps[i].accent,
                slide: self.steps[i].slide,
            };
        }
        Pattern { pitches, timing, num_steps }
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test sequencer -- test_data_model_compiles`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add plugin303/src/dsp/sequencer.rs plugin303/src/params.rs tests/sequencer.rs
git commit -m "feat: define decoupled 303 sequencer data model and events"
```

---

### Task 2: Sequencer Core Logic

**Files:**
- Modify: `plugin303/src/dsp/sequencer.rs`
- Modify: `tests/sequencer.rs`

**Interfaces:**
- Consumes: `Pattern`, `SeqEvent`, `Timebase`
- Produces: `Sequencer` struct with `process_block(..., events: &mut [(u32, SeqEvent); 64], count: &mut usize)`

- [ ] **Step 1: Write the failing tests**

```rust
// tests/sequencer.rs
use plugin303::dsp::sequencer::{Sequencer, Pattern, PitchEntry, TimeEntry, SeqEvent, Timebase};

#[test]
fn test_sequencer_drift_free() {
    let mut seq = Sequencer::new(48000.0);
    seq.set_running(true);
    let mut pat = Pattern {
        pitches: [PitchEntry::default(); 16],
        timing: [TimeEntry::Rest; 16],
        num_steps: 16,
    };
    pat.timing[0] = TimeEntry::Note; // One note to trigger events
    
    let mut events = [(0, SeqEvent::GateOff); 64];
    let mut count = 0;
    
    // Process exactly 1 step (16th note at 120 BPM = 125ms = 6000 samples)
    seq.process_block(&pat, 0, 6000, &mut events, &mut count, 120.0, Timebase::Straight, false);
    assert_eq!(count, 2); // Trigger and GateOff
    
    // Verify Gate duty is exactly 50% in straight time (3000 samples)
    if let SeqEvent::Trigger { .. } = events[0].1 {
        assert_eq!(events[0].0, 0); // At sample 0
    } else { panic!("Expected Trigger"); }
    
    if let SeqEvent::GateOff = events[1].1 {
        assert_eq!(events[1].0, 3000); // 50% of 6000
    } else { panic!("Expected GateOff"); }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test sequencer -- test_sequencer_drift_free`
Expected: FAIL (Sequencer not defined/implemented).

- [ ] **Step 3: Write minimal implementation**

Add `Sequencer` struct to `plugin303/src/dsp/sequencer.rs`:

```rust
pub struct Sequencer {
    sample_rate: f64,
    running: bool,
    step_idx: usize,
    pitch_idx: usize,
    next_step_sample: f64,
    gate_off_sample: Option<f64>,
    prev_note_slide: bool,
    prev_was_sounding: bool,
    isr_origin: f64,
}

const ISR_PERIOD_S: f64 = 0.0018;
const GATE_ON_LATENCY_S: f64 = 0.0010;
const GATE_OFF_LATENCY_S: f64 = 0.00235;

impl Sequencer {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            sample_rate, running: false, step_idx: 0, pitch_idx: 0,
            next_step_sample: 0.0, gate_off_sample: None,
            prev_note_slide: false, prev_was_sounding: false, isr_origin: 0.0,
        }
    }

    pub fn set_running(&mut self, running: bool) {
        if running && !self.running {
            self.step_idx = 0;
            self.pitch_idx = 0;
            self.next_step_sample = 0.0;
            self.gate_off_sample = None;
            self.prev_note_slide = false;
            self.prev_was_sounding = false;
        }
        self.running = running;
    }

    pub fn reset_transport(&mut self, _pos_beats: f64) {
        self.next_step_sample = 0.0; // Phase resetting for absolute beat syncing
    }

    fn samples_per_step(&self, bpm: f64, timebase: Timebase) -> f64 {
        self.sample_rate * 60.0 / (bpm * timebase.steps_per_beat())
    }

    fn isr_quantise(&self, t: f64, latency_s: f64, hw_jitter: bool) -> f64 {
        if !hw_jitter { return t; }
        let period = ISR_PERIOD_S * self.sample_rate;
        let rel = t - self.isr_origin;
        self.isr_origin + (rel / period).ceil() * period + latency_s * self.sample_rate
    }

    pub fn process_block(&mut self, pattern: &Pattern, block_start: u64, num_samples: u32,
                         out_events: &mut [(u32, SeqEvent); 64], out_count: &mut usize,
                         bpm: f64, timebase: Timebase, hw_jitter: bool) {
        if !self.running { return; }
        let block_end = block_start as f64 + num_samples as f64;

        if let Some(g) = self.gate_off_sample {
            if g >= block_start as f64 && g < block_end {
                let g_quant = self.isr_quantise(g, GATE_OFF_LATENCY_S, hw_jitter);
                if g_quant < block_end {
                    let offset = (g_quant as u32).saturating_sub(block_start as u32);
                    if *out_count < 64 {
                        out_events[*out_count] = (offset, SeqEvent::GateOff);
                        *out_count += 1;
                    }
                    self.gate_off_sample = None;
                }
            }
        }

        while self.next_step_sample < block_end {
            let t = self.isr_quantise(self.next_step_sample, GATE_ON_LATENCY_S, hw_jitter);
            let offset = (t as u32).saturating_sub(block_start as u32);
            
            match pattern.timing[self.step_idx] {
                TimeEntry::Rest => { self.prev_was_sounding = false; }
                TimeEntry::Tie => {}
                TimeEntry::Note => {
                    let p = pattern.pitches[self.pitch_idx];
                    let next_timing = pattern.timing[(self.step_idx + 1) % pattern.num_steps as usize];
                    
                    let ev = if self.prev_note_slide && self.prev_was_sounding {
                        SeqEvent::SlideTo { key: p.key, octave: p.octave, accent: p.accent }
                    } else {
                        SeqEvent::Trigger { key: p.key, octave: p.octave, accent: p.accent }
                    };
                    
                    if *out_count < 64 {
                        out_events[*out_count] = (offset, ev);
                        *out_count += 1;
                    }

                    if !(p.slide && next_timing == TimeEntry::Note) {
                        let spt = self.samples_per_step(bpm, timebase) / timebase.ticks_per_step();
                        self.gate_off_sample = Some(self.next_step_sample + timebase.gate_on_ticks() * spt);
                    } else {
                        self.gate_off_sample = None;
                    }
                    
                    self.prev_note_slide = p.slide;
                    self.prev_was_sounding = true;
                    self.pitch_idx = (self.pitch_idx + 1) % pattern.num_steps as usize;
                }
            }
            
            self.next_step_sample += self.samples_per_step(bpm, timebase);
            self.step_idx = (self.step_idx + 1) % pattern.num_steps as usize;
        }
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test sequencer -- test_sequencer_drift_free`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add plugin303/src/dsp/sequencer.rs tests/sequencer.rs
git commit -m "feat: implement drift-free SequencerCore scheduling logic"
```

---

### Task 3: Voice DSP Adaptation

**Files:**
- Modify: `plugin303/src/dsp/voice.rs`

**Interfaces:**
- Consumes: `SeqEvent`
- Produces: `Voice::on_seq_event(ev: SeqEvent, accent_amount: f32, decay_knob_ms: f32)`

- [ ] **Step 1: Write the failing test**

```rust
// tests/voice_sequencer.rs
use plugin303::dsp::voice::Voice;
use plugin303::dsp::sequencer::SeqEvent;

#[test]
fn test_voice_seq_events() {
    let mut voice = Voice::new(48000.0);
    // Trigger with accent
    voice.on_seq_event(SeqEvent::Trigger { key: 0, octave: 0, accent: true }, 1.0, 500.0);
    assert!(voice.gate);
    assert_eq!(voice.get_accent_acc(), 0.8); // Charge pump integration (1.0 - 0.0) * 0.8
    
    // SlideTo (should slew pitch)
    voice.on_seq_event(SeqEvent::SlideTo { key: 12, octave: 0, accent: false }, 1.0, 500.0);
    assert_eq!(voice.get_accent_acc(), 0.8); // Slew, no gate close, no accent
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test voice_sequencer -- test_voice_seq_events`
Expected: FAIL (missing method).

- [ ] **Step 3: Write minimal implementation**

In `plugin303/src/dsp/voice.rs`, add `pitch_slew: LeakyIntegrator` to the `Voice` struct and initialize it in `Voice::new()` with `set_time_constant(12.0)`. Update `Voice::reset()` to reset it.

```rust
// In Voice struct:
    pitch_slew: LeakyIntegrator,
// In Voice::new():
    pitch_slew: {
        let mut li = LeakyIntegrator::new(sr as f64);
        li.set_time_constant(12.0); // 60ms constant-time slide
        li
    },
// In Voice::reset():
    self.pitch_slew.reset();
```

Add the `on_seq_event` method to `Voice` and update `tick()` to use `self.pitch_slew.tick(target_pitch)`. Remove the old `glide_alpha` logic from `tick`.

```rust
// In plugin303/src/dsp/voice.rs
use crate::dsp::sequencer::SeqEvent;

impl Voice {
    pub fn on_seq_event(&mut self, ev: SeqEvent, accent_amount: f32, decay_knob_ms: f32) {
        match ev {
            SeqEvent::Trigger { key, octave, accent } => {
                let target = key as f32 + 12.0 * octave as f32 + 60.0; // C4 base
                self.target_pitch_st = target;
                self.current_pitch_st = target;
                self.pitch_slew.set_state(target as f64);
                
                self.set_gate(true, true, accent, false);
                if accent { self.accent_acc += (1.0 - self.accent_acc) * 0.8; }
            }
            SeqEvent::SlideTo { key, octave, accent } => {
                let target = key as f32 + 12.0 * octave as f32 + 60.0;
                self.target_pitch_st = target;
                // No envelope retrigger, but update accent states
                self.accent_active = accent;
                if accent {
                    self.accent_gain = self.accent_knob_val;
                    self.meg_decay.set_decay(self.accent_decay_ms);
                    self.vca_env.set_release(50.0);
                    self.accent_acc += (1.0 - self.accent_acc) * 0.8;
                } else {
                    self.accent_gain = 0.0;
                    self.meg_decay.set_decay(self.normal_decay_ms);
                    self.vca_env.set_release(0.5);
                }
            }
            SeqEvent::GateOff => {
                self.set_gate(false, false, false, false);
            }
        }
    }
}
```

In `Voice::tick()`, replace the old `glide_alpha` logic:
```rust
    // In Voice::tick, near line 1024
    // REMOVE glide_alpha calculation and manual self.current_pitch_st interpolations.
    // ADD:
    self.current_pitch_st = self.pitch_slew.tick(self.target_pitch_st as f64) as f32;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test voice_sequencer -- test_voice_seq_events`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add plugin303/src/dsp/voice.rs tests/voice_sequencer.rs
git commit -m "feat: implement constant-time slide slew and on_seq_event in Voice"
```

---

### Task 4: Plugin Audio Thread Integration

**Files:**
- Modify: `plugin303/src/lib.rs`

**Interfaces:**
- Instantiates the new `Sequencer` core with sr.
- In `process()`, uses the compiler `Pattern` and renders events into a 64-item array, iterating it synchronously with the sample block.

- [ ] **Step 1: Write the minimal implementation**

In `plugin303/src/lib.rs`:

```rust
// Add import:
use crate::dsp::sequencer::{Timebase, SeqEvent, Pattern};

// In Plugin303::initialize:
self.sequencer = Sequencer::new(self.sample_rate as f64);

// In Plugin303::reset:
self.sequencer.set_running(false);

// In Plugin303::process:
// Before the sample loop:
        let seq_run = self.params.seq_run.value();
        let pattern_steps = if seq_run {
            self.params.pattern.read().ok().map(|p| p.compile(self.params.seq_length.value() as u8))
        } else {
            None
        };
        let seq_tempo = self.params.seq_tempo.value() as f64;
        
        let mut seq_events = [(0, SeqEvent::GateOff); 64];
        let mut seq_event_count = 0;
        let mut seq_event_idx = 0;
        
        let transport = context.transport();
        self.sequencer.set_running(transport.playing && seq_run);
        
        if let Some(pat) = &pattern_steps {
            self.sequencer.process_block(
                pat, 0, buffer.samples() as u32,
                &mut seq_events, &mut seq_event_count,
                seq_tempo, Timebase::Straight, false // jitter=false for now
            );
        }

// Inside the sample loop (for (sample_idx, mut frame) in buffer.iter_samples().enumerate()):
            // Apply sequencer events for this sample
            while seq_event_idx < seq_event_count && seq_events[seq_event_idx].0 == sample_idx as u32 {
                self.voice.on_seq_event(
                    seq_events[seq_event_idx].1,
                    self.params.accent.value(),
                    self.params.decay.value() * 3000.0 // approx conversion
                );
                seq_event_idx += 1;
            }
```
*Note: Make sure to remove the old `self.sequencer.tick(...)` and `self.voice.set_gate(...)` calls that were in the `if active_midi_note.is_none()` block, substituting it strictly with the `while seq_event_idx < seq_event_count` handler loop.*

- [ ] **Step 2: Test plugin builds**

Run: `cargo test -p plugin303` (to ensure `lib.rs` and the tests compile and pass).

- [ ] **Step 3: Commit**

```bash
git add plugin303/src/lib.rs
git commit -m "feat: integrate block-based SequencerCore into nih-plug audio loop"
```
