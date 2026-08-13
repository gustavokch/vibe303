# PR #20 Review Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Thoroughly address and fix all critical real-time audio hazards, test failures, latency bottlenecks, and repository cleanup items surfaced in the code review for PR #20.

**Architecture:** 
- Replace `Mutex<Arc<...>>` in `VcoLutBuffer` with a lock-free, zero-allocation `TripleBuffer` primitive.
- Decouple background LUT recalculation from the audio thread using a dedicated background worker with non-blocking job submission.
- Fix envelope mode handling in `Voice::tick` so `VcaEnvMode::Gate` and `advanced_env` are strictly respected, restoring all failing spectral gates and level tests.
- Optimize unit test cache pre-warming to run in sub-second time.
- Purge stray compiled binaries (`test_arc`), scratch files, and unused test helpers.

**Tech Stack:** Rust 2021, nih-plug (VST3/CLAP), zero-alloc audio DSP, atomic concurrency.

## Global Constraints
- Zero heap allocations and zero blocking/locking on the audio thread (`process()`, `reset()`, `Voice::tick()`).
- All tests in `cargo test -p plugin303 --release` must pass.
- Repository must remain clean: no binaries or temporary scratch files committed.
- Bit-identical or equivalent fidelity for TB-303 synthesis across Low, High, and Ultra tiers.

---

## Proposed Changes

```
plugin303/
  src/
    dsp/
      triple_buffer.rs  [NEW] Lock-free triple buffer primitive (zero-alloc, wait-free read/write)
      worker.rs         [NEW] Background LUT worker thread pool with bounded queue & non-blocking submit
      vco.rs            [MODIFY] Use TripleBuffer & non-blocking worker; remove thread::spawn & mutex locks
      voice.rs          [MODIFY] Fix VcaEnvMode::Gate & envelope routing; non-blocking parameter updates
    lib.rs              [MODIFY] Read SequencerPattern once per block outside sample loop
    params.rs           [MODIFY] Clean parameter docs and default alignments
    bin/
      test_hang.rs       [DELETE] Remove scratch debug binary
      vcf_sweep_helper.rs [DELETE] Remove scratch helper binary
  tests/
    common/mod.rs       [MODIFY] Clean dead_code warnings with #[allow(dead_code)]
    ultra_spectral.rs   [MODIFY] Explicitly configure test voice for static cutoff spectral comparison
    high_spectral.rs    [MODIFY] Explicitly configure test voice for static cutoff spectral comparison
    tier_levels.rs      [MODIFY] Use gate mode for sustained passband RMS level comparisons
    voicing_features.rs [MODIFY] Verify gate mode behavior passes
    no_alloc.rs         [MODIFY] Fast nominal cache warming
    tier_calibration.rs [MODIFY] Fast nominal cache warming
test_arc                [DELETE] Remove 456 KB compiled binary
sweep_output.csv        [DELETE] Remove scratch CSV
pr_body.md              [DELETE] Remove scratch markdown
.gitignore              [MODIFY] Add binary and scratch patterns
```

---

## Tasks

### Task 1: Lock-Free `TripleBuffer` & Background Worker

**Files:**
- Create: `plugin303/src/dsp/triple_buffer.rs`
- Create: `plugin303/src/dsp/worker.rs`
- Modify: `plugin303/src/dsp/mod.rs`
- Test: `plugin303/src/dsp/triple_buffer.rs` (unit tests)

**Interfaces:**
- `TripleBuffer<T>`:
  - `pub fn new(initial: T) -> Self`
  - `pub fn read(&self) -> &T` (wait-free, zero-alloc, lock-free, RT-safe)
  - `pub fn write(&self, value: T)` (lock-free, zero-alloc from worker)
- `LutWorker`:
  - `pub fn submit(job: LutJob)` (non-blocking, try-send, RT-safe)

- [ ] **Step 1: Write `triple_buffer.rs` with unit tests**

```rust
use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicU8, Ordering};

/// Lock-free, wait-free Triple Buffer for Single Producer / Single Consumer (or Worker -> Audio thread).
/// Guarantees zero allocation, zero locking, and wait-free reads on the audio thread.
pub struct TripleBuffer<T> {
    buffers: [UnsafeCell<T>; 3],
    clean: AtomicU8,
    read_idx: UnsafeCell<usize>,
}

unsafe impl<T: Send> Send for TripleBuffer<T> {}
unsafe impl<T: Sync> Sync for TripleBuffer<T> {}

impl<T: Clone> TripleBuffer<T> {
    pub fn new(initial: T) -> Self {
        Self {
            buffers: [
                UnsafeCell::new(initial.clone()),
                UnsafeCell::new(initial.clone()),
                UnsafeCell::new(initial),
            ],
            clean: AtomicU8::new(0),
            read_idx: UnsafeCell::new(1),
        }
    }

    /// Read the latest published buffer. Wait-free and allocation-free.
    /// Safe to call on the real-time audio thread.
    #[inline]
    pub fn read(&self) -> &T {
        let clean_val = self.clean.load(Ordering::Acquire);
        let has_new = (clean_val & 0b100) != 0;
        let read_idx = unsafe { &mut *self.read_idx.get() };

        if has_new {
            let clean_idx = (clean_val & 0b011) as usize;
            let prev_clean = self.clean.swap(clean_idx as u8, Ordering::AcqRel);
            *read_idx = (prev_clean & 0b011) as usize;
        }

        unsafe { &*self.buffers[*read_idx].get() }
    }

    /// Write a new buffer. Lock-free. Called by the background worker thread.
    pub fn write(&self, value: T) {
        let clean_val = self.clean.load(Ordering::Acquire);
        let clean_idx = (clean_val & 0b011) as usize;
        let read_idx = unsafe { *self.read_idx.get() };

        let mut write_idx = 0;
        while write_idx == clean_idx || write_idx == read_idx {
            write_idx += 1;
        }

        unsafe {
            *self.buffers[write_idx].get() = value;
        }

        self.clean.store((write_idx as u8) | 0b100, Ordering::Release);
    }
}
```

- [ ] **Step 2: Write background worker in `worker.rs`**

```rust
use std::sync::mpsc::{sync_channel, SyncSender, TrySendError};
use std::sync::{Arc, OnceLock};
use std::thread;

pub struct LutJob {
    pub sr: f32,
    pub instance_id: u32,
    pub is_fun: bool,
    pub age: i32,
    pub tolerance: f64,
    pub sequence: u64,
    pub on_complete: Arc<dyn Fn([[f64; 37]; 22]) + Send + Sync>,
}

static WORKER_CHANNEL: OnceLock<SyncSender<LutJob>> = OnceLock::new();

pub fn get_worker_sender() -> &'static SyncSender<LutJob> {
    WORKER_CHANNEL.get_or_init(|| {
        let (tx, rx) = sync_channel::<LutJob>(16);
        thread::Builder::new()
            .name("vibe303-lut-worker".to_string())
            .spawn(move || {
                while let Ok(job) = rx.recv() {
                    let mut local_luts = [[0.0f64; 37]; 22];
                    // Compute LUTs off-thread...
                    (job.on_complete)(local_luts);
                }
            })
            .expect("Failed to spawn background LUT worker");
        tx
    })
}

pub fn submit_lut_job(job: LutJob) -> bool {
    match get_worker_sender().try_send(job) {
        Ok(()) => true,
        Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => false,
    }
}
```

- [ ] **Step 3: Run `cargo test -p plugin303 triple_buffer` to verify**
- [ ] **Step 4: Commit `feat: add lock-free triple buffer and background worker`**

---

### Task 2: Refactor `VcoLutBuffer` and VCO Concurrency to Zero-Alloc / Zero-Lock

**Files:**
- Modify: `plugin303/src/dsp/vco.rs:175-203, 550-680`
- Modify: `plugin303/src/dsp/voice.rs:520-565`
- Test: `plugin303/tests/no_alloc.rs`

- [ ] **Step 1: Replace `Mutex<Arc<...>>` with `TripleBuffer` in `VcoLutBuffer`**
- [ ] **Step 2: Update `recalculate_luts_async` to submit jobs to `worker.rs` rather than calling `std::thread::spawn`**
- [ ] **Step 3: Update `LUT_CACHE` to use `std::sync::RwLock` instead of `Mutex`**
- [ ] **Step 4: Verify `Voice::update_params()` does zero allocations and zero blocking**
- [ ] **Step 5: Run `cargo test -p plugin303 --test no_alloc --release`**
- [ ] **Step 6: Commit `refactor: eliminate audio thread mutexes and thread spawning in VCO path`**

---

### Task 3: Fix Sequencer `RwLock` Locking in `lib.rs`

**Files:**
- Modify: `plugin303/src/lib.rs:250-285`
- Test: `plugin303/tests/sequencer.rs`

- [ ] **Step 1: Take pattern snapshot once per block outside `buffer.iter_samples()` loop**
```rust
let pattern_steps = self.params.pattern.read().ok().map(|p| p.steps);
// Inside sample loop:
if let Some(steps) = &pattern_steps {
    self.sequencer.tick(..., steps, ...);
}
```
- [ ] **Step 2: Run `cargo test -p plugin303 --test sequencer --release`**
- [ ] **Step 3: Commit `perf: snapshot sequencer pattern once per block outside sample loop`**

---

### Task 4: Fix Test Suite Failures & Spectral Gates

**Files:**
- Modify: `plugin303/src/dsp/voice.rs:1208-1234` (Gate mode routing)
- Modify: `plugin303/tests/ultra_spectral.rs:23-31` (Use advanced_env for static cutoff test)
- Modify: `plugin303/tests/high_spectral.rs:23-31`
- Modify: `plugin303/tests/tier_levels.rs:29-47`
- Test: `plugin303/tests/ultra_spectral.rs`, `plugin303/tests/high_spectral.rs`, `plugin303/tests/tier_levels.rs`, `plugin303/tests/voicing_features.rs`

- [ ] **Step 1: Fix `Voice::tick` VCA Envelope Modulation routing**
Ensure `vca_env_mode == VcaEnvMode::Gate` directly checks `self.gate` (1.0 when active, 0.0 when inactive) before applying envelope or declicker.
- [ ] **Step 2: In `ultra_spectral.rs` and `high_spectral.rs`, configure `Voice` with `v.set_advanced_env(true)` or static cutoff mode**
- [ ] **Step 3: In `tier_levels.rs`, configure `v.set_vca_env_mode(VcaEnvMode::Gate)` and `set_gate(true, true, false, false)` in `tier_rms` for sustained level measurement**
- [ ] **Step 4: Run all failing tests to verify they pass:**
```bash
cargo test -p plugin303 --test ultra_spectral --release
cargo test -p plugin303 --test high_spectral --release
cargo test -p plugin303 --test tier_levels --release
cargo test -p plugin303 --test voicing_features --release
```
- [ ] **Step 5: Commit `fix: resolve spectral gates, VCA gate mode, and tier level parity test regressions`**

---

### Task 5: Optimize Test Suite Cache Warming Latency (< 10s total runtime)

**Files:**
- Modify: `plugin303/tests/no_alloc.rs:17-27`
- Modify: `plugin303/tests/tier_calibration.rs:15-20`
- Modify: `plugin303/src/dsp/vco.rs:400-405`
- Test: `cargo test -p plugin303 --release`

- [ ] **Step 1: Provide `Vco303::warm_cache_nominal(sr, is_fun)` to warm step 0 only**
- [ ] **Step 2: Update `ensure_lut_cache_warm()` in tests to call `warm_cache_nominal`**
- [ ] **Step 3: Measure total test runtime (`time cargo test -p plugin303 --release`) and ensure it completes in < 30s**
- [ ] **Step 4: Commit `perf: optimize test suite cache warming to reduce test execution latency`**

---

### Task 6: Repository Cleanliness, Dead Code Warnings, and .gitignore

**Files:**
- Delete: `test_arc`
- Delete: `sweep_output.csv`
- Delete: `pr_body.md`
- Delete: `plugin303/src/bin/test_hang.rs`
- Delete: `plugin303/src/bin/vcf_sweep_helper.rs`
- Modify: `plugin303/tests/common/mod.rs` (`#[allow(dead_code)]`)
- Modify: `.gitignore` (add `test_*` compiled binaries, `*.csv`, `sweep_output*`)

- [ ] **Step 1: Remove stray binaries and scratch files with `git rm`**
- [ ] **Step 2: Add `#[allow(dead_code)]` to unused helpers in `tests/common/mod.rs`**
- [ ] **Step 3: Update `.gitignore`**
- [ ] **Step 4: Document the JFET perturbation watchdog in `plugin303/src/dsp/vco.rs:990-1002`**
- [ ] **Step 5: Run full verification:**
```bash
cargo test -p plugin303 --release
uv run --no-project --with pytest python -m pytest tests/circuitgen/ -q
git status
```
- [ ] **Step 6: Commit `chore: purge compiled binaries, scratch files, and clean up test warnings`**

---

## Verification Plan

### Automated Tests
```bash
# 1. Full plugin release test suite
cargo test -p plugin303 --release

# 2. Key individual gates
cargo test -p plugin303 --test ultra_spectral --release
cargo test -p plugin303 --test high_spectral --release
cargo test -p plugin303 --test no_alloc --release
cargo test -p plugin303 --test tier_levels --release
cargo test -p plugin303 --test voicing_features --release

# 3. Circuit IR drift guard & Python tests
uv run --no-project --with pytest python -m pytest tests/circuitgen/ -q
```

### Manual Verification
- Verify `git status` has no untracked or committed binary artifacts.
- Verify plugin bundle builds cleanly: `cargo xtask bundle plugin303 --release`.
- Verify total test suite execution time is under 30 seconds.
