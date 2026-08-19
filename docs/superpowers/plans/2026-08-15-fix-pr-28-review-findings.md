# Fix PR #28 Code Review Findings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Resolve code review findings on PR #28 regarding test assertions, dummy struct initialization, and dynamic reset state logic.

**Architecture:** Update `plugin303/tests/voicing_features.rs` to include concrete output assertions for 200ms fixed accent decay, refine `AccentSweep::new` struct initialization in `plugin303/src/dsp/rc_env.rs` to avoid hardcoded dummy sample rates, and update `Voice::reset` in `plugin303/src/dsp/voice.rs` to calculate `classic_slide_tau_ms` based on topology and glide knob state.

**Tech Stack:** Rust, `cargo test`, `plugin303` DSP engine.

**Spec:** Review findings posted on GitHub PR #28 (issue comment 5304211111).

## Global Constraints

- Must maintain 100% test pass rate (`cargo test`).
- Zero API or behavior breaking changes outside the specific fixes.
- No allocation in real-time audio paths (`reset`, `tick`).

---

### Task 1: Add Output Assertion to `test_accent_decay_fixed_200ms`

**Files:**
- Modify: `plugin303/tests/voicing_features.rs:L208-L219`

**Interfaces:**
- Consumes: `Voice::tick()` audio sample generator.
- Produces: Verified decay assertion enforcing `< 0.25` signal amplitude after 300 ms.

- [ ] **Step 1: Inspect test implementation and write failing assertion if decay is unconstrained**

```rust
// In plugin303/tests/voicing_features.rs inside test_accent_decay_fixed_200ms
let mut peak_late = 0.0f32;
for _ in 0..14400 {
    let sample = v.tick();
    peak_late = sample.abs();
}

assert!(
    peak_late < 0.25,
    "Expected audio peak after 300ms of ~200ms MEG decay to be < 0.25, got {}",
    peak_late
);
```

- [ ] **Step 2: Run test to verify it passes with current 200ms accent decay**

Run: `cargo test --test voicing_features test_accent_decay_fixed_200ms`
Expected: PASS

- [ ] **Step 3: Commit assertion update**

```bash
git add plugin303/tests/voicing_features.rs
git commit -m "test(dsp): add missing peak signal assertion to test_accent_decay_fixed_200ms"
```

---

### Task 2: Refine `AccentSweep::new` Initial Field Assignment

**Files:**
- Modify: `plugin303/src/dsp/rc_env.rs:L339-L347`

**Interfaces:**
- Consumes: `AccentSweep::new(fs: f64)`
- Produces: Clean struct initialization without dummy magic sample rate `44100.0`.

- [ ] **Step 1: Update `AccentSweep::new` in `plugin303/src/dsp/rc_env.rs`**

```rust
impl AccentSweep {
    pub fn new(fs: f64) -> Self {
        let mut s = Self {
            v_c13: 0.0,
            fs: 0.0,
            coeff_chg: 0.0,
            coeff_dis: 0.0,
        };
        s.set_sample_rate(fs);
        s
    }
```

- [ ] **Step 2: Run all env_curve and voicing tests**

Run: `cargo test --test env_curve --test voicing_features`
Expected: PASS

- [ ] **Step 3: Commit `AccentSweep::new` cleanup**

```bash
git add plugin303/src/dsp/rc_env.rs
git commit -m "refactor(dsp): initialize AccentSweep fs cleanly without dummy rate"
```

---

### Task 3: Use Dynamic Topology and Knob Calculation in `Voice::reset()`

**Files:**
- Modify: `plugin303/src/dsp/voice.rs:L835-L842`

**Interfaces:**
- Consumes: `calculate_classic_slide_tau_ms(self.topology, self.glide_knob)`
- Produces: Correct `classic_slide_tau_ms` cached value after host reset.

- [ ] **Step 1: Update `Voice::reset()` in `plugin303/src/dsp/voice.rs`**

```rust
        self.target_pitch_st = 60.0;
        self.glide_knob = 0.5;
        self.reso_comp = 0.0;
        self.classic_slide_tau_ms = calculate_classic_slide_tau_ms(self.topology, self.glide_knob);
        self.xfade_from = None;
```

- [ ] **Step 2: Run test suite**

Run: `cargo test`
Expected: PASS

- [ ] **Step 3: Commit `Voice::reset()` update**

```bash
git add plugin303/src/dsp/voice.rs
git commit -m "fix(dsp): preserve topology and glide knob state when resetting classic slide tau"
```
