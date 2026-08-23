# Fix PR #44 Review Findings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Resolve all 3 code review findings on PR #44 across DSP coefficient caching, workflow schema resilience, and profiling CLI argument parsing.

**Architecture:** Cache Bessel lowpass FIR coefficients once globally via `std::sync::LazyLock` to avoid recomputing Bessel series expansions on every voice reset/allocation; support string tier names (`"low"`, `"high"`, `"ultra"`, `"sweep"`) in the profiling harness; and default missing differential benchmark ratios to `0.0` when quality gates fail in the DSP optimization workflow.

**Tech Stack:** Rust (edition 2021), `cargo test`, JavaScript (`.claude/workflows`).

**Spec:** Pull Request #44 Code Review (`https://github.com/gustavokch/vibe303/pull/44#issuecomment-5387179280`).

---

## Global Constraints

- Real-time safety: Audio hot paths must remain strictly zero-allocation.
- Exact DSP invariants: FIR coefficients, linear-phase group delay, and sample rate conversion mathematics must remain bit-identical.
- Fast execution: `Os2x::new()` should perform a cheap array copy without Bessel series iteration.

---

### Task 1: Cache Polyphase FIR Coefficients in `Os2x`

**Files:**
- Modify: `plugin303/src/dsp/oversample.rs:78-100`
- Test: `plugin303/src/dsp/oversample.rs`

**Interfaces:**
- Consumes: `std::sync::LazyLock`, `build_lp`
- Produces: Precomputed static coefficient tables for `he`, `ho`, and `h`

- [ ] **Step 1: Write test verifying coefficient constancy and zero runtime Bessel evaluation**

Add a unit test in `plugin303/src/dsp/oversample.rs` asserting that `Os2x::new()` produces identical coefficients and does not re-derive them differently.

- [ ] **Step 2: Run test suite to verify baseline**
Run: `cargo test -p plugin303 --lib oversample`

- [ ] **Step 3: Implement `LazyLock` coefficient caching**
Introduce a static `COEFFS: LazyLock<([f32; 17], [f32; 16], [f32; 33])>` computed via `build_lp(8.0, 0.25)`. Update `Os2x::new()` to copy directly from the static table.

- [ ] **Step 4: Verify test passes**
Run: `cargo test -p plugin303 --lib oversample`

- [ ] **Step 5: Commit changes**
`git commit -am "perf(dsp): cache polyphase FIR coefficients globally with LazyLock"`

---

### Task 2: Enhance Profiling CLI `--tier` Flag Parsing

**Files:**
- Modify: `plugin303/src/bin/profile_voice.rs:191-215`
- Test: `plugin303/src/bin/profile_voice.rs`

**Interfaces:**
- Consumes: CLI args
- Produces: Robust tier selection supporting integer indices (`0`, `1`, `2`) and named strings (`"low"`, `"high"`, `"ultra"`, `"sweep"`)

- [ ] **Step 1: Write helper function for parsing tier arguments**
Implement `arg_tier(flag: &str) -> Option<i32>` that parses integers as well as case-insensitive string names (`low` -> 0, `high` -> 1, `ultra` -> 2, `sweep` -> 3).

- [ ] **Step 2: Test CLI compilation & execution with tier names**
Run: `cargo run -p plugin303 --bin profile_voice -- --secs 0.1 --tier low`
Run: `cargo run -p plugin303 --bin profile_voice -- --secs 0.1 --tier ultra`

- [ ] **Step 3: Commit changes**
`git commit -am "fix(profiling): support string tier names in profile_voice CLI"`

---

### Task 3: Harden DSP Optimization Workflow Schema on Gate Failure

**Files:**
- Modify: `.claude/workflows/dsp-perf-optimization-loop.js:195-255`

**Interfaces:**
- Consumes: `gateResults`, `postResult`
- Produces: Resilient synthesis payload with default values on quality gate failure

- [ ] **Step 1: Provide safe fallback differential metrics when gates fail**
If `allGatesPassed` is false, populate `postResult` with `{ ultra_ratio: 0.0, ultra_ksps: 0.0, high_ratio: 0.0, high_ksps: 0.0, low_ratio: 0.0, low_ksps: 0.0 }` so `synthesisResult` receives well-formed numeric values matching schema requirements.

- [ ] **Step 2: Commit changes**
`git commit -am "fix(workflow): provide safe fallback differential metrics on gate failure"`
