# PR 57 Review Remediation Plan — Round 2

## Goal

Remediate round-2 code review findings on PR #57:

1. Kill formula duplication between `render_tweaks_panel` and its unit test (drift risk: stale `317.0` assertion passes vacuously after any constant change).
2. Remove magic assertion bound `1356.5` (7x) from `gui_layout_test.rs`; derive from `GUI_WIDTH`.
3. Clarify `unimplemented!()` panic in `DummyGuiContext::get_state`; drop redundant assert at line 83.
4. Update stale working-tree note in round-9 report (both artifacts now committed in this PR).

## Architecture

Rust workspace, `plugin303` crate. GUI behind `gui` feature (nih_plug + egui 0.31.1). Layout regression covered by:

- `plugin303/src/gui/panels/tweaks_panel.rs` — render fn + inline `#[cfg(test)] mod tests`
- `plugin303/tests/gui_layout_test.rs` — headless `egui::Context` full-panel-stack render test

## Verified facts (no re-derivation needed)

- egui 0.31.1 `Separator::default()` allocates `spacing = 6.0` px in horizontal layout — `SEPARATOR_WIDTH = 6.0` matches widget.
- Overhead math: `3 * (6 + 2*8) = 66` px correct (4 cols + 3 separators = 7 widgets, 6 gaps of `item_spacing.x = 8`).
- Content right edge = `GUI_WIDTH (1366) - 10` (CentralPanel `Margin::same(10)`); bound 1356.5 = right edge + 0.5 slack.

## Task 1: Share separator geometry constants with unit test

- Modify: `plugin303/src/gui/panels/tweaks_panel.rs`
- Consumes: none. Produces: module-scope `NUM_COLS`, `NUM_SEPARATORS`, `SEPARATOR_WIDTH`, `ITEM_SPACING_X`, `SEPARATOR_OVERHEAD`, plus `TWEAKS_AVAIL_W` derivation for the test.
- Step 1 (Red): rewrite unit test to reference `super::<CONST>` names — compile error since constants are function-local.
- Step 2: `cargo test --lib tweaks_panel` — expect compile failure.
- Step 3 (Green): hoist constants from inside `render_tweaks_panel` to module scope (private, `#[cfg(test)]`-visible via `super::`). Keep render fn behavior identical.
- Step 4: `cargo test --lib tweaks_panel` — pass, and `cargo test --test gui_layout_test` — pass.
- Step 5: commit `refactor(gui): share tweaks separator constants with unit test`

## Task 2: Derive layout-test bounds from GUI_WIDTH

- Modify: `plugin303/tests/gui_layout_test.rs`
- Step 1: replace `1356.5` literals with `let content_right = GUI_WIDTH as f32 - 10.0 + 0.5;` asserted via closure `assert_no_overflow!(ui, "Header")` style helper (plain closure, no macro needed if closure borrows cleanly — use small fn taking `&Ui`, bound, label).
- Step 2: `unimplemented!()` → `unimplemented!("state query not used by layout tests")`.
- Step 3: drop redundant assert after the `ui.vertical` block (line 83; parent min_rect is union of children, inner assert already binds).
- Step 4: `cargo test --test gui_layout_test` — pass.
- Step 5: commit `test(gui): derive layout bounds from GUI_WIDTH, drop redundant assert`

## Task 3: Update round-9 report working-tree note

- Modify: `docs/superpowers/plans/2026-08-29-gui-audit-round9-report.md`
- Step 1: replace working-tree paragraph: both separator fix and layout test committed in PR #57 (aba73a0, bb1c7ba); baseline matches HEAD.
- Step 2: commit `docs: mark round-9 working-tree note as landed`

## Verification

- `cargo test` — 100% green.
- `cargo test --no-default-features` — green (cfg gate intact).
- `cargo fmt --check` + `cargo clippy` on touched crate — clean.
- Push `fix/gui-visual-remediation-r8`.
