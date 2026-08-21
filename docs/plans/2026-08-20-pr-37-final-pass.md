# Plan for Remaining PR #37 Review Findings & Remediation

## Context

Pull Request #37 (`feat/analog-wear-refinement`) introduces physical component wear formulas, seed-based component variation, temperature drift, and recalibration scaling (`recalibration_scale`) to the TB-303 synthesis engine in `plugin303`. 

A code review identified 19 findings across critical, high, medium, and low severities. The majority of review fixes have been implemented in the working tree across `vco.rs`, `voice.rs`, `worker.rs`, `lib.rs`, and test files. This plan outlines the exact remediation state, validates the remaining open items, addresses the thermal LUT scaling strategy, and details verification steps for concluding PR #37.

---

## Code Review Findings & Current Remediation State

### 1. Critical Finding: Thermal LUT CV Scaling Formula
* **Location**: `vco.rs:743-756` and `worker.rs:47-64`
* **Problem**: Scaling base LUT values by `temp_ratio` was linear and mathematically invalid for an exponential VCO (compensated $V_T$ but ignored $I_s(T)$ semiconductor physics).
* **PR #37 Fix**: Implemented a 2-endpoint thermal solve at $0^\circ\text{C}$ (step 0) and $21^\circ\text{C}$ (step 21) with linear interpolation across the 21 intermediate steps. Replaced single-solve temp-ratio scaling.
* **Long-Term Note**: Reworking thermal drift as a live pitch/frequency offset inside the nodal Newton solver itself is deferred to `docs/plans/2026-08-20-dsp-nodal-solver-optimization-plan.md`. The 2-endpoint LUT model is the active, verified mechanism for PR #37.

### 2. High Severity Findings
* **Hardcoded `recal_scale = 1.0` in Tier Transitions (`voice.rs:862, 952`)**: Replaced `1.0` with `self.recalibration_scale` in `set_tier` and `set_tier_immediate` when launching background async LUT jobs.
* **Unvalidated Extrapolation Fallback Anchors (`vco.rs:648-664`)**: Added checks ensuring `lut[0]` and `lut[LUT_K-1]` fall back to linear step extrapolation or nominal CV if anchors fail to converge or yield non-finite values.
* **Interior Anchor Convergence Validation (`vco.rs:633-646`)**: Added post-loop validation interpolating non-finite interior LUT anchors from nearest valid neighbors.
* **Fast-Path Sequence Increment (`vco.rs:686, 693`)**: Incremented sequence counter `lut_buffer.sequence.fetch_add(1, Ordering::SeqCst)` prior to fast-path cache store or worker job submit to invalidate stale in-flight jobs.
* **Thermal Model Tests**: Added dedicated thermal model ramp & persistence test (`test_thermal_model_ramp_and_persistence`) in `tests/age_drift.rs`.

### 3. Medium & Low Severity Findings
* **Worker Job Submission Retry Logic (`voice.rs:864, 954`)**: `vco_dirty` flag is cleared only when both `vco` and `vco2` background job submissions succeed (`ok1 && ok2`). Retries on full channel buffer.
* **`Voice::reset()` Temperature Persistence (`voice.rs:882, 938`)**: Removed temperature zeroing from `reset()`. Temperature persists across transport events per spec. `comp_seed` set to `-1` on reset to force re-seeding on next parameter update.
* **Mid-Playback Drift Re-Seed**: Documented as intended DAW parameter interaction (non-RT parameter change).
* **Ungated `println!` Cleanups**: Removed un-gated `println!` calls in `tests/recalibration_scale.rs`.

### 4. Pre-Existing Branch Solver Regressions (Deferred)
Two pre-existing test failures exist at branch HEAD (also fail on main):
1. `test_pitch_parity_fun`: Fun topology note 72 renders +32.6% sharp due to Newton solver sticking.
2. `vco_sr_pitch_tracking @ 88.2 kHz`: Pitch tracking error 1.61% exceeds 0.5% tolerance.

* **Resolution**: Both are Newton/secant solver-calibration regressions owned by `docs/plans/2026-08-20-dsp-nodal-solver-optimization-plan.md` and are explicitly excluded from PR #37 review-fix scope.

---

## Actionable Execution Plan

### Step 1: Code Review Verification of Modified Files
Verify modified files in working tree for clean styling, proper comments, and standard Rust idioms:
* `plugin303/src/dsp/vco.rs`
* `plugin303/src/dsp/voice.rs`
* `plugin303/src/dsp/worker.rs`
* `plugin303/src/lib.rs`
* `plugin303/tests/age_drift.rs`
* `plugin303/tests/recalibration_scale.rs`
* `plugin303/tests/tier_calibration.rs`

### Step 2: Test Suite Execution & Acceptance Validation
Run full integration test suite to verify no regressions:
```bash
cargo test -p plugin303 --test recalibration_scale
cargo test -p plugin303 --test age_drift
cargo test -p plugin303 --test tier_calibration
cargo test -p plugin303 --test no_alloc
```

### Step 3: Git Commit & PR Summary Update
1. Stage modified source and test files.
2. Commit changes with Conventional Commit message:
   `fix(dsp): resolve PR #37 review findings for thermal LUT, recalibration scale and test coverage`
3. Update PR #37 description with final review fix summary and status of deferred items.

---

## Verification Section

* **Automated Tests**: Pass `recalibration_scale.rs`, `age_drift.rs`, `tier_calibration.rs`, and `no_alloc.rs`.
* **Thermal Model Persistence**: Verified by `test_thermal_model_ramp_and_persistence`.
* **Async Recalibration Scale**: Verified by `test_worker_recal_scale_async`.
* **Audio Waveform Parity**: Verified by `test_recal_scale_audio_integration`.
