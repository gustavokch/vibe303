# PR #61 Review Remediation — 2026-09-03

## Goal

Fix the four findings from the PR #61 code review (comment posted 2026-09-03).

## Architecture

Rust plugin GUI (`plugin303/src/gui/`), egui immediate mode. Test suite: `cargo test` (workspace), plus `cargo check` gate.

## Tasks

### Task 1 — 🟡 top_controls.rs:59 per-frame alloc on disabled bank title

- Modify: `plugin303/src/gui/panels/top_controls.rs`
- Problem: `title.to_string()` + galley every frame when bank inactive (default state).
- Fix: cache measured width in egui temp data keyed by `Id::new(("adsr_title_w", title))`; compute galley only on cache miss.
- Steps:
  1. `cargo check` baseline.
  2. Apply fix.
  3. `cargo check` + `cargo test -p plugin303 gui` — green.
  4. Commit `fix(gui): cache disabled ADSR bank title width (PR#61 review)`.

### Task 2 — 🔵 sequencer_panel.rs row_h magic-number coupling

- Modify: `plugin303/src/gui/panels/sequencer_panel.rs`
- Test: add unit test in-file for extracted pure fn.
- Fix: extract `const TRANSPORT_ROW_GAP: f32 = 6.0;` used by both `item_spacing` and `row_h`; extract pure fn `transport_row_height(avail_h: f32) -> f32` and test: fills column exactly, clamps at `TRANSPORT_ROW_MIN_H`.
- Steps: failing test → impl → pass → commit `refactor(gui): share transport row gap constant (PR#61 review)`.

### Task 3 — 🔵 step_button.rs:72 stale LED hover text

- Modify: `plugin303/src/gui/widgets/step_button.rs`
- Fix: hover text → "Step column LED strip" (bar is uniform now).
- Steps: edit → `cargo test -p plugin303 step_button` → commit `fix(gui): correct step LED hover text (PR#61 review)`.

### Task 4 — 🔵 generator_panel.rs:62 hardcoded right group width

- Modify: `plugin303/src/gui/panels/generator_panel.rs`
- Fix: measure NEW SEED button text width via `WidgetText` galley + padding instead of literal 110.0.
- Steps: edit → `cargo check` → commit `fix(gui): measure generator header right group width (PR#61 review)`.

## Gate

`cargo test` 100% green, then `git push origin fix/gui-visual-remediation-r13`.
