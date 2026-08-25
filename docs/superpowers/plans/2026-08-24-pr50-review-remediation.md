# PR #50 Review Remediation Plan

**Date:** 2026-08-24
**PR:** [#50 — fix(gui): sequencer 2x8 grid, hit targets, DECAY format — audit phase 4](https://github.com/) (`fix/gui-audit-phase4`)
**Goal:** Resolve all findings from the PR review comment (posted in Phase 2) without changing any visual behavior at the current fixed window size (1366×960).
**Tech stack:** Rust / nih-plug / egui. Workspace at repo root; plugin crate is `plugin303`. Test command: `cargo test -p plugin303` (skill template says pytest/uv; this project is Rust — adapted).

**Architecture notes:**
- `render_step_strip` (`plugin303/src/gui/widgets/step_button.rs`) draws a 140 px column of hand-painted egui widgets. Column background must be painted *before* children (paint order within layer), so its height cannot be measured post-layout — it must be derived from the same constants as the children.
- Grid geometry lives inline in `render_sequencer_and_transport` (`plugin303/src/gui/panels/sequencer_panel.rs`). Pure arithmetic; extractable to a testable function.
- DECAY knob formatting branches on `ms >= 1000.0`; rounding makes values in [999.5, 1000.0) display "1000 ms".

**Spec reference:** audit findings in PR review comment; `audit-report.md` issues #1/#11/#15.

---

## Task 1 — Step strip geometry: single source of truth

**Files:**
- Modify: `plugin303/src/gui/widgets/step_button.rs`
- Modify: `plugin303/src/gui/panels/sequencer_panel.rs`
- Test: unit `#[cfg(test)] mod tests` in `step_button.rs`

**Consumes:** none. **Produces:** `pub const LED_H, HEADER_H, CONTROL_ROW_H, STRIP_HEIGHT` in `step_button.rs`; `sequencer_panel` uses `STRIP_HEIGHT` instead of literal `140.0`.

1. **Red** — add failing test:
   ```rust
   #[cfg(test)]
   mod tests {
       use super::*;
       #[test]
       fn strip_height_equals_sum_of_child_allocations() {
           // LED bar + step-number header + ACC/SLD row + gate + pitch + octave row
           assert_eq!(LED_H, 4.0);
           assert_eq!(HEADER_H, 16.0);
           assert_eq!(CONTROL_ROW_H, 30.0);
           let children = LED_H + HEADER_H + 4.0 * CONTROL_ROW_H;
           assert_eq!(STRIP_HEIGHT, children);
       }
   }
   ```
   Run: `cargo test -p plugin303 strip_height -- --nocapture`. Fails to compile (consts do not exist) = red.
2. Define consts; replace literals `4.0`, `16.0`, `30.0` at allocation sites (L52, L71, L101/L134/L173/L204/L269/L309) and drawn-pill size `26.0` stays as `CONTROL_ROW_H - 4.0`.
3. `sequencer_panel.rs:46`: `let cell_h = STRIP_HEIGHT;` (import from widget module).
4. Fix stale comment at `step_button.rs:44-45`: correctness comes from paint order within the layer, not layer separation.
5. **Green:** re-run test. Passes.
6. Commit: `refactor(gui): step strip geometry consts — single source of truth`

## Task 2 — Grid geometry extraction + fit invariant

**Files:**
- Modify: `plugin303/src/gui/panels/sequencer_panel.rs`
- Test: unit tests in same file (`#[cfg(test)]`) or `plugin303/tests/gui_build.rs`

**Produces:** `pub(crate) fn grid_cell_width(avail_w: f32) -> f32` plus named consts `TRANSPORT_COLUMN_W = 232.0`, `GRID_GAP = 4.0`, `SEPARATOR_CHROME_W = 18.0` (8 + separator 2 + 8), `MIN_CELL_W = 72.0`.

1. **Red** — add tests:
   ```rust
   #[test]
   fn grid_cells_fill_available_width_when_not_clamped() {
       let avail = TRANSPORT_COLUMN_W + SEPARATOR_CHROME_W + 8.0 * 90.0 + 7.0 * GRID_GAP;
       assert!((grid_cell_width(avail) - 90.0).abs() < 1e-4);
   }
   #[test]
   fn grid_cell_width_clamps_to_minimum() {
       assert_eq!(grid_cell_width(100.0), MIN_CELL_W);
   }
   ```
   Compile failure = red.
2. Implement `grid_cell_width` with `debug_assert!` when result would exceed clamp (narrow-window overflow surfaces in debug instead of silently squeezing transport).
3. Replace inline arithmetic at `sequencer_panel.rs:44-45`; name the `12.0` at L40 as `FRAME_VERT_CHROME_PX = 12.0` with derivation comment (inner_margin top 4 + border stroke ≈2×1 + layout slack).
4. **Green:** re-run tests.
5. Commit: `refactor(gui): extract sequencer grid geometry, debug-assert fit invariant`

## Task 3 — DECAY boundary formatting

**Files:**
- Modify: `plugin303/src/gui/widgets/knob.rs`
- Test: unit tests in `knob.rs` `#[cfg(test)]`

1. **Red** — add failing tests:
   ```rust
   #[test]
   fn decay_format_boundary_no_false_1000ms() {
       // 999.6 ms must render "1000 ms"? No — must render "1.0 s" or stay below;
       // rule: threshold at rounding boundary 999.5 ms.
       assert_eq!(format_decay_label((999.4 - 200.0) / 1800.0), "799 ms"); // sanity mid-range
       assert_eq!(format_decay_label((999.6 - 200.0) / 1800.0), "1.0 s");
       assert_eq!(format_decay_label(0.0), "200 ms");
       assert_eq!(format_decay_label(1.0), "2.0 s");
   }
   ```
   Compile failure = red.
2. Extract `pub fn format_decay_label(normalized: f32) -> String`; threshold `ms >= 999.5`; call site replaces inline branch at `knob.rs:208-214`.
3. **Green:** re-run tests.
4. Commit: `fix(gui): DECAY label threshold at rounding boundary 999.5 ms`

## Task 4 — Audit report hygiene (docs-only)

**Files:**
- Move: `audit-report.md` → `docs/gui-audit-report.md`
- Edit: merge stacked `Status:` lines on issue #11 into one chronological line; add note that screenshot evidence set (`latest.png`, zone/crop crops) lives outside the repo.

No test applies. Commit: `docs(gui): relocate audit report under docs/, dedupe status lines`

## Task 5 — Full verification + push

1. `cargo test -p plugin303` — gate: all green.
2. Retry Phase-2 PR comment if classifier was down earlier; then `git push origin fix/gui-audit-phase4`.
3. Summary of resolved findings + PR link.
