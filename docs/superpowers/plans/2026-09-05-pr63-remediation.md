# PR #63 Remediation — Extended QA Capture States

Date: 2026-09-05
Spec: PR #63 review comment (https://github.com/gustavokch/vibe303/pull/63#issuecomment-5553615700)

## Goal

Fix 2 risks + 1 nit found in review of `feat/gui-qa-extended-capture-states`:

1. zoom75 section crops use 100%-zoom coordinates → misleading artifacts.
2. Per-frame `env::var` + `set_zoom_factor` in UI closure.
3. Unused `quality_ultra`/`quality_ultra2x` mappings — keep, document as manual states.

## Architecture

- `scripts/crop_gui_sections.py` — skip per-section crops for zoom-scaled states.
- `plugin303/src/gui/mod.rs` — hoist env read out of frame closure.
- No test infra for python scripts; verification = script run + `cargo test` green.

## Tasks

### Task 1 — zoom75 crops

Modify: `scripts/crop_gui_sections.py`

- Step 1: failing check — after `process_all_states`, `zoom75_crop_*.png` must not
  exist. Run on current tree: exists → fail.
- Step 2: add `ZOOM_SCALED_STATES = {"zoom75"}`; `process_all_states` calls
  `process_image` only for stems not in that set.
- Step 3: rerun `python3 scripts/crop_gui_sections.py target/gui-qa` after removing
  stale `zoom75_crop_*.png`; confirm no regeneration.
- Step 4: commit `fix(qa): skip section crops for zoom-scaled states`.

### Task 2 — hoist QA env read in gui

Modify: `plugin303/src/gui/mod.rs`

- Step 1: compile-check test — `cargo build -p plugin303` (red impossible; this is
  a perf/hygiene fix, verified by grep: no `env::var` inside frame closure).
- Step 2: hoist `let qa_zoom = matches!(std::env::var("VIBE303_QA_STATE").as_deref(), Ok("zoom75"));`
  next to `gen_params`; closure body becomes `if qa_zoom { egui_ctx.set_zoom_factor(0.75); }`.
- Step 3: `cargo test -p plugin303` green; `grep -n 'env::var' plugin303/src/gui/mod.rs`
  shows only hoisted read.
- Step 4: commit `refactor(gui): hoist QA state read out of frame closure`.

### Task 3 — document manual quality states

Modify: `scripts/capture_standalone_gui.sh`

- `quality_ultra`/`quality_ultra2x` stay supported in `params.rs` for manual
  capture; add comment in capture script listing them as manual-only states.
- Commit with Task 2 or separate `docs(qa): note manual-only quality states`.

## Verification

- `python3 scripts/crop_gui_sections.py target/gui-qa` — no `zoom75_crop_*.png`.
- `cargo test` — 89 tests green.
- Push to `feat/gui-qa-extended-capture-states`.
