# Fix PR #29 Code Review Findings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Resolve code review findings on PR #29 regarding key semitone modulo calculations, generator GUI panel mutate seed progression, sequencer loop-wrap mutation timing, and scale degree position matching.

**Architecture:** Use `.rem_euclid(12)` in `AcidGenerator` key calculations, mutate `gen_params.seed` upon manual GUI mutate clicks, trigger pattern mutation during step 0 boundary transitions within `Sequencer::process_block_mutating`, and use `.iter().position()` for scale degree indexing.

**Tech Stack:** Rust, `cargo test`, `plugin303` DSP engine & GUI.

**Spec:** Review findings posted on GitHub PR #29 (issue comment 5304532052).

## Global Constraints

- Must maintain 100% test pass rate (`cargo test`).
- Zero audio dropouts or allocation in real-time audio paths (`Sequencer::process_block_mutating`).
- Deterministic behavior preserved for all seeds.

---

### Task 1: Fix Modulo Key Calculation (`rem_euclid(12)`) in `AcidGenerator`

**Files:**
- Modify: `plugin303/src/dsp/generator.rs:243`, `plugin303/src/dsp/generator.rs:304`
- Test: `plugin303/tests/generator.rs`

**Interfaces:**
- Consumes: `params.root_note`, `intervals` semitone offsets.
- Produces: Guaranteed non-negative key semitones in range `0..12`.

- [ ] **Step 1: Write failing test for negative / boundary root note quantization**

```rust
// In plugin303/tests/generator.rs
#[test]
fn test_root_note_euclidean_rem_safety() {
    let mut params = GeneratorParams::default();
    params.root_note = 250; // Large u8
    let pat = AcidGenerator::generate(&params);
    for p in &pat.pitches {
        assert!(p.key >= 0 && p.key < 12, "Key {} out of range 0..12", p.key);
    }
}
```

- [ ] **Step 2: Run test to verify behavior**

Run: `cargo test --test generator test_root_note_euclidean_rem_safety`
Expected: PASS or FAIL depending on wrapping.

- [ ] **Step 3: Update `AcidGenerator::generate` and `mutate` to use `.rem_euclid(12)`**

In `plugin303/src/dsp/generator.rs`:
Replace `(params.root_note as i8 + intervals[degree_idx]) % 12` with:
```rust
(params.root_note as i8 + intervals[degree_idx]).rem_euclid(12)
```
And in `mutate`:
```rust
(params.root_note as i8 + intervals[new_degree]).rem_euclid(12)
```

- [ ] **Step 4: Verify tests pass**

Run: `cargo test --test generator`
Expected: PASS

- [ ] **Step 5: Commit changes**

```bash
git add plugin303/src/dsp/generator.rs plugin303/tests/generator.rs
git commit -m "fix(dsp): use rem_euclid for non-negative pitch key calculation"
```

---

### Task 2: Advance `gen_params.seed` on Manual `[MUTATE]` Click in Generator GUI Panel

**Files:**
- Modify: `plugin303/src/gui/panels/generator_panel.rs:195`

**Interfaces:**
- Consumes: Generator GUI `[MUTATE]` button click handler.
- Produces: Updated `gen_params.seed` ensuring consecutive manual mutations generate non-identical variations.

- [ ] **Step 1: Update GUI mutate action handler in `generator_panel.rs`**

In `plugin303/src/gui/panels/generator_panel.rs`:
```rust
if ui.add(mutate_btn).clicked() {
    if let Ok(mut seq_pat) = params.pattern.write() {
        let mode = params.seq_mode.value();
        let num_steps = params.seq_length.value() as u8;
        let mut pat = seq_pat.compile(num_steps, mode);
        gen_params.seed = gen_params.seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        AcidGenerator::mutate(&mut pat, gen_params, gen_params.seed);
        // ... rest of step sync logic
    }
}
```

- [ ] **Step 2: Check compilation**

Run: `cargo check --package plugin303`
Expected: PASS

- [ ] **Step 3: Commit GUI fix**

```bash
git add plugin303/src/gui/panels/generator_panel.rs
git commit -m "fix(gui): advance generator params seed on manual mutate click"
```

---

### Task 3: Use `.iter().position()` for Scale Degree Matching in `AcidGenerator::mutate`

**Files:**
- Modify: `plugin303/src/dsp/generator.rs:295-305`

**Interfaces:**
- Consumes: `pattern.pitches[i].key` and `params.scale.intervals()`.
- Produces: Scale degree index or default fallback without unneeded loop state.

- [ ] **Step 1: Refactor scale degree lookup in `AcidGenerator::mutate`**

In `plugin303/src/dsp/generator.rs`:
```rust
let current_key = pattern.pitches[i].key;
let degree = intervals
    .iter()
    .position(|&interval| (params.root_note as i8 + interval).rem_euclid(12) == current_key)
    .unwrap_or(0);

let shift = if rng.next_f32() < 0.5 { 1 } else { num_degrees - 1 };
let new_degree = (degree + shift) % num_degrees;
pattern.pitches[i].key = (params.root_note as i8 + intervals[new_degree]).rem_euclid(12);
```

- [ ] **Step 2: Run generator tests**

Run: `cargo test --test generator`
Expected: PASS

- [ ] **Step 3: Commit scale degree refactor**

```bash
git add plugin303/src/dsp/generator.rs
git commit -m "refactor(dsp): use iter().position() for scale degree lookup in mutation"
```

---

### Task 4: Align Sequencer Mutating Loop Boundary Execution in `process_block_mutating`

**Files:**
- Modify: `plugin303/src/dsp/sequencer.rs:292-321`
- Test: `plugin303/tests/generator.rs`

**Interfaces:**
- Consumes: `Sequencer::process_block_mutating`.
- Produces: Immediate pattern mutation at step 0 wrap before event generation for step 0.

- [ ] **Step 1: Write test checking pattern mutation timing on loop boundary**

In `plugin303/tests/generator.rs`:
```rust
#[test]
fn test_sequencer_loop_wrap_mutates_before_step_zero_playback() {
    let params = GeneratorParams {
        rhythm_pulses: 16,
        rhythm_length: 16,
        mutation_rate: 1.0, // force 100% mutation
        seed: 42,
        ..Default::default()
    };
    let mut pat = AcidGenerator::generate(&params);
    let mut seq = Sequencer::new(48000.0);
    seq.set_running(true);

    let mut events = [(0, SeqEvent::GateOff); 64];
    let mut count = 0;

    // Process step 0..15 (96000 samples at 120bpm = 16 steps of 6000 samples)
    seq.process_block_mutating(
        &mut pat,
        Some(&params),
        0,
        96000,
        &mut events,
        &mut count,
        120.0,
        Timebase::Straight,
        false,
    );

    let pat_before_wrap = pat.clone();

    // Process block crossing step 0 of loop 1
    seq.process_block_mutating(
        &mut pat,
        Some(&params),
        96000,
        6000,
        &mut events,
        &mut count,
        120.0,
        Timebase::Straight,
        false,
    );

    assert_ne!(pat, pat_before_wrap, "Pattern should be mutated when loop count increments");
}
```

- [ ] **Step 2: Update `process_block_mutating` or step wrap hook**

In `plugin303/src/dsp/sequencer.rs`, in step boundary evaluation loop where `next_step == 0`:
```rust
if next_step == 0 {
    self.loop_count = self.loop_count.wrapping_add(1);
    if let Some(params) = gen_params {
        AcidGenerator::mutate(pattern, params, self.loop_count);
    }
}
```
Refactor `process_block` to accept an optional mutation closure/callback or pass `gen_params` directly into step wrap boundary logic.

- [ ] **Step 3: Verify all unit tests pass**

Run: `cargo test`
Expected: PASS

- [ ] **Step 4: Commit sequencer loop wrap mutation fix**

```bash
git add plugin303/src/dsp/sequencer.rs plugin303/tests/generator.rs
git commit -m "fix(dsp): trigger pattern mutation at step 0 boundary during block processing"
```
