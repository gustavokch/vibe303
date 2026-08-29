# PR #56 Review Remediation Plan

- **Date:** 2026-08-28
- **PR:** https://github.com/gustavokch/vibe303/pull/56 (`fix/gui-visual-remediation-r7`)
- **Review comment:** https://github.com/gustavokch/vibe303/pull/56#issuecomment-5456422980
- **Source audit:** `docs/superpowers/plans/2026-08-28-gui-audit-round7-report.md`

## Goal

Resolve all 12 review findings on PR #56 without regressing the six round-7 defects the PR
already fixed. Six findings are `risk` level (layout geometry that is calibrated against a
single 1366x992 @ 100% zoom capture and breaks at other zoom factors); six are `nit` level
(dead code, over-wide visibility, stale comments, duplicated reads, tautological tests).

## Architecture

egui immediate-mode GUI inside `plugin303/src/gui/`, rendered by `nih_plug_egui`. Layout is
expressed in logical points; the header exposes a ZOOM selector (75%-200%) that rescales the
whole logical coordinate space, so any absolute point offset measured against the window edge
is zoom-dependent. The guiding principle for this remediation: **derive geometry from
available space or from measured cursor deltas, never from a literal calibrated against one
screenshot.**

## Tech stack

- Rust 2021, `cargo test --package plugin303` (unit tests live in `mod tests` next to the code)
- Visual gate: `scripts/capture_standalone_gui.sh` writes to `target/gui-qa/`

## Test strategy

egui layout cannot be asserted headlessly, so unit tests pin the *pure functions* that compute
the geometry (`transport_bottom_filler`, `compute_needle_angle`, new `right_group_left`,
`seed_row_advance`) and the visual capture is the gate for the composed result. Tests that
compare constants to constants are treated as defects, not coverage.

---

## Task 1 - Collapse the dead branch in `compute_needle_angle`

**Modify:** `plugin303/src/gui/widgets/knob.rs`
**Test:** `plugin303/src/gui/widgets/knob.rs` (`mod needle_tests`)

Consumes: `normalized: f32` in `0.0..=1.0`, `start_angle`, `end_angle`.
Produces: needle angle in radians.

`start_angle + 0.0 * (end_angle - start_angle) == start_angle`, so the `normalized.abs() < 0.001`
early return is unreachable in effect. `.abs()` is redundant for a `0.0..=1.0` input.

1. Rewrite `needle_angle_positions` so it asserts the *contract* (monotonic lerp, exact endpoints,
   midpoint at 12 o'clock) rather than pinning the dead branch at `0.0005`.
2. `cargo test --package plugin303 needle` - expect failure on the `0.0005` assertion once the
   branch is removed if the test still pins it.
3. Reduce the body to the lerp; rewrite the doc comment to describe a lerp.
4. `cargo test --package plugin303 needle` - green.
5. `git commit -m "refactor(gui): reduce compute_needle_angle to a plain lerp"`

## Task 2 - Restore module-private visibility and fix stale comments

**Modify:** `plugin303/src/gui/panels/sequencer_panel.rs`

`paint_combobox_down_arrow`, `GRID_TOTAL_H`, and `transport_bottom_filler` are `pub` but used
only in this module; the child `mod tests` reaches them via `super::*`. Two comments no longer
describe the code.

1. Drop `pub` from all three items.
2. Use the file's existing `Rect` / `AboveOrBelow` / `WidgetVisuals` imports in
   `paint_combobox_down_arrow` instead of fully-qualified `nih_plug_egui::egui::...` paths.
3. Narrow the icon-painter doc comment: these two combos sit low enough that egui flips the
   glyph when the popup opens upward; the other four never do.
4. Fix the stale `~90px below transport Row D` comment in `transport_bottom_filler_closes_bottom_void`.
5. `cargo test --package plugin303` - green, no new warnings.
6. `git commit -m "refactor(gui): unexport sequencer layout helpers and correct stale comments"`

## Task 3 - Enforce Row D height and make the alignment test exercise real code

**Modify:** `plugin303/src/gui/panels/sequencer_panel.rs`

`row_d_h = 28.0` is *assumed*, not enforced: Row D's real height comes from its ComboBoxes under
`button_padding = (4, 3)` (~20 pt), so the filler places Row D's top at 240 but its bottom short
of `GRID_TOTAL_H`. `sequencer_and_transport_bottom_rows_aligned` compares constants to constants
and never calls the render path, so it passes even if the render-side literal changes.

1. Rewrite the test to route through `transport_bottom_filler` with a shared `ROW_D_H` constant
   and assert `filler + consumed + ROW_D_H == GRID_TOTAL_H` for several `consumed` values,
   including the saturating case where transport content already exceeds the grid.
2. `cargo test --package plugin303 sequencer` - expect failure (no `ROW_D_H` yet).
3. Introduce `const ROW_D_H: f32 = CONTROL_ROW_H;` (28.0, derived from the step grid rather than
   duplicated), use it at the call site, and wrap Row D in a fixed-height
   `allocate_ui_with_layout` so the rendered row actually occupies `ROW_D_H`.
4. `cargo test --package plugin303 sequencer` - green.
5. `git commit -m "fix(gui): enforce transport Row D height and test the real filler path"`

## Task 4 - Restore height-adaptive sequencer frame

**Modify:** `plugin303/src/gui/panels/sequencer_panel.rs`

`ui.set_min_height(280.0)` makes the frame non-adaptive; the 10 px bottom margin then holds only
at the one captured window size and zoom factor. The dead band from Issue 6 returns at any other
zoom.

1. Re-capture the pre-frame available height, restore
   `ui.set_min_height((frame_avail_h - FRAME_VERT_CHROME_PX).max(GRID_TOTAL_H + 12.0))`, and
   recalibrate `FRAME_VERT_CHROME_PX` from the frame's own margins/stroke rather than an
   empirical literal: inner margin top 4 + bottom 8 + stroke 1 + trailing item spacing.
2. Add a unit test for the floor expression as a pure function (`frame_min_height(avail)`), so the
   `.max()` floor and the chrome subtraction are pinned without a live `Ui`.
3. `cargo test --package plugin303` - green.
4. Visual gate deferred to Task 8.
5. `git commit -m "fix(gui): make sequencer frame height adaptive again"`

## Task 5 - Clamp the header right-hand container

**Modify:** `plugin303/src/gui/panels/header_bar.rs`

`bar_rect.right() - 480.0` is an absolute offset against a fixed 280 pt `left_rect`. At 200 % zoom
the header's logical width falls to roughly 640 pt, so `right_rect` starts near 160 and overlaps
the brand group by ~120 pt (the prior `400.0` overlapped by ~40 pt, so the PR widened the bug).

1. Add `fn right_group_left(bar_width: f32, left_width: f32, desired: f32) -> f32` returning
   `(bar_width - desired).max(left_width)`, with tests for the wide case (no clamp), the exact
   boundary, and the 200 % zoom case (clamped to `left_width`, no overlap).
2. `cargo test --package plugin303 header` - expect failure (function absent).
3. Use it at the call site with `LEFT_GROUP_W = 280.0` and `RIGHT_GROUP_W = 480.0` promoted to
   named constants.
4. `cargo test --package plugin303 header` - green.
5. `git commit -m "fix(gui): clamp header right group so it cannot overlap the brand block"`

## Task 6 - Keep the ENVELOPES frame full-width and dedupe the param read

**Modify:** `plugin303/src/gui/panels/top_controls.rs`

Dropping `ui.set_min_width(ui.available_width())` lets the frame shrink to content. It is the last
item in the row, so leftover width becomes dead space to its right - Issue 5's defect relocated
from inside the frame to outside it. `params.advanced_env.value()` is also read twice.

1. Restore `ui.set_min_width(ui.available_width())` on the frame body and let the two ADSR banks
   share the width (spacer between them rather than a trailing gap).
2. Reuse `is_adv` for the checkbox's `adv_env` binding.
3. `cargo test --package plugin303` - green.
4. Visual gate deferred to Task 8.
5. `git commit -m "fix(gui): keep ENVELOPES frame full width and dedupe advanced_env read"`

## Task 7 - Measure the SEED row instead of reconstructing it

**Modify:** `plugin303/src/gui/panels/tweaks_panel.rs`

The DRIFT spacer reconstructs column 3's SEED row height as
`interact_size.y + item_spacing.y + 2.0`. `Ui::add_space` advances the cursor without emitting
item spacing, so the `item_spacing.y` term is double-counted, and the expression breaks if the
label/DragValue row is ever not exactly `interact_size.y` tall.

1. Render column 3 first into a measured pass, or capture `ui.cursor().top()` before and after the
   SEED row and stash the delta; feed that measured delta to column 2's `add_space`.
   Fallback if ordering forbids it: render an invisible placeholder row in column 2 built from the
   same widgets, so both columns advance identically by construction.
2. `cargo test --package plugin303` - green.
3. Visual gate deferred to Task 8.
4. `git commit -m "fix(gui): align DRIFT to RECAL by measurement rather than arithmetic"`

## Task 8 - Full verification

1. `cargo test --package plugin303` - all green, zero warnings.
2. `cargo clippy --package plugin303 --all-targets` - no new lints.
3. `scripts/capture_standalone_gui.sh` - confirm against `target/gui-qa/`:
   - bottom margin below the sequencer still equals the 10 px side margins,
   - transport Row D bottom aligns with the octave arrow row bottom,
   - header brand block and right control group do not overlap,
   - ENVELOPES frame spans to the main panel's right edge,
   - DRIFT and RECAL knob centers share a baseline,
   - all seven bipolar knobs still point to 12 o'clock at `+0.0`.
4. `git push origin fix/gui-visual-remediation-r7`
