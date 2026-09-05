# PR #63 Remediation (Second Pass) — Extended QA Capture States & Test Rigor

Date: 2026-09-05
Spec: PR #63 second-pass review (https://github.com/gustavokch/vibe303/pull/63#issuecomment-5553800746)

## Goal

Resolve all 1 bug, 2 risks, and 2 nits identified in the second review pass:
1. Fix `tests/test_crop_gui_sections.py` dimension assertions (1366x960).
2. Add unit tests for extended QA state crop handling and zoom75 crop exclusion.
3. Add Rust unit tests in `plugin303/tests/gui_build.rs` for `VIBE303_QA_STATE` parameter initialization.
4. Refactor `plugin303/src/gui/mod.rs` and `plugin303/src/params.rs` QA state parsing to concise `match ... .as_deref()`.

## Architecture & Tech Stack

- Python 3 `unittest` for GUI crop section generator tests.
- Rust integration tests in `plugin303/tests/gui_build.rs` using `EnvGuard` and `ENV_LOCK`.
- Idiomatic Rust pattern matching for environment variable strings.

## Tasks

### Task 1 — Fix & Extend Python Crop Unit Tests
- Target files: `tests/test_crop_gui_sections.py`
- Step 1: Update dimension assertion to 1366x960, add test case for extended QA states (`knobs`, `quality_high`, `generator`, `zoom75`), assert `zoom75_crop_*.png` files are excluded while other crops exist.
- Step 2: Run `python3 -m unittest discover tests` to verify test passes.
- Step 3: Git commit `test(qa): fix dimension assertions and test extended state crops`.

### Task 2 — Rust QA State Integration Tests
- Target files: `plugin303/tests/gui_build.rs`
- Step 1: Add tests for `quality_high` (tier is High), `knobs` (tuning 25.0, cutoff 0.75), `generator` (generator params initialized), and `zoom75` (editor created).
- Step 2: Run `cargo test --test gui_build` to verify pass.
- Step 3: Git commit `test(gui): add unit tests for QA state environment overrides`.

### Task 3 — Refactor Environment Variable Matching
- Target files: `plugin303/src/gui/mod.rs`, `plugin303/src/params.rs`
- Step 1: Replace nested `if let Ok(s)` in `gen_params` and `default_tier` with `match std::env::var("VIBE303_QA_STATE").as_deref()`.
- Step 2: Run `cargo test` and `python3 -m unittest discover tests` to verify green.
- Step 3: Git commit `refactor(gui): simplify QA state env parsing with pattern matching`.

## Verification & PR Push
- `python3 -m unittest discover tests` — all green.
- `cargo test` — full suite green.
- Push to branch `feat/gui-qa-extended-capture-states`.
