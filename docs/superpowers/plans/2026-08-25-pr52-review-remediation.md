# PR #52 Remediation Plan

## Goal
Address review findings on PR #52 (`fix/gui-visual-audit-remediation`). No 🔴 bugs; only risk/nit. Focus: card underflow guard, zero-center duplication, comment rot, and TEMPO branch consistency.

## Architecture
- Presentation-layer only. No DSP change.
- Keep `ui.vertical` for output row (correct).
- Keep `card_w = 300.0`, `card_h = 52.0` (audit-decided).

## Tech Stack
- Rust + egui (nih_plug_egui).
- Existing `theme::*` constants.

## Spec Reference
- PR review posted: https://github.com/gustavokch/vibe303/pull/52#issuecomment-5417587288
- Audit prompt: `docs/superpowers/plans/2026-08-24-gui-audit-remaining-findings.md`

## Tasks

### Task 1 — card underflow guard (risk)
- File: `plugin303/src/gui/panels/souped_panel.rs`
- Add `MIN_CARD_W: f32 = 120.0` const. `card_w = 300.0_f32.min((total_w - 20.0).max(MIN_CARD_W))`.
- Step 1: Edit file. Step 2: `cargo check -p plugin303` (compiles). Step 3: `git commit -m "fix(gui): guard souped overlay card width against narrow drawer"`.

### Task 2 — derive card_h from drawer (nit)
- File: `plugin303/src/gui/panels/souped_panel.rs`
- Replace `let card_h = 52.0_f32;` with `let card_h = (DRAWER_INNER_HEIGHT - 20.0).max(24.0);`.
- Step 1: Edit. Step 2: `cargo check -p plugin303`. Step 3: `git commit -m "refactor(gui): derive overlay card_h from drawer inner height"`.

### Task 3 — drop or relink stale dB comment (nit)
- File: `plugin303/src/gui/widgets/knob.rs`
- Either link the doc or drop the "~10° CCW offset" claim. Drop it (no doc reference exists).
- Step 1: Edit. Step 2: `cargo check -p plugin303`. Step 3: `git commit -m "docs(gui): drop undocumented dB offset claim in knob formatter"`.

### Task 4 — TEMPO zero-center guard (risk)
- File: `plugin303/src/gui/widgets/knob.rs`
- Add the same `f.abs() < 0.05` guard for TEMPO as dB/TUNING branches: print `"120 BPM"` (or current integer value with no decimals). Actually BPM values are 60..200; print `"{:.0} BPM"` always — keep it as-is and drop the divergent claim. Decision: keep `{:.0} BPM` for all TEMPO values; no special-case needed since BPM integer rounding is the spec.
- Action: No code change. Document in commit message that the divergence is intentional (integer BPM display matches PR #12 consistency rule).
- Step 1: No edit. Step 2: None. Step 3: Skip commit.

### Task 5 — extract zero-center formatter (nit)
- File: `plugin303/src/gui/widgets/knob.rs`
- Add small helper `fn format_zero_centered(f: f32, unit: &str) -> String` to dedupe the 3x `if f.abs() < 0.05` pattern.
- Use for TUNING, PITCH, dB branches.
- Step 1: Edit. Step 2: `cargo check -p plugin303`. Step 3: `git commit -m "refactor(gui): extract zero-center formatter helper"`.

### Task 6 — verify build + tests
- `cargo test -p plugin303` (if any). `cargo check -p plugin303 --tests`.
- Confirm no clippy regression: `cargo clippy -p plugin303 --no-deps -- -D warnings` (advisory).

## Commit Plan
Atomic commits per task. Final push to `fix/gui-visual-audit-remediation`.
