# PR 57 Review Remediation Plan

## Goal
Remediate code review findings on PR #57:
1. Fix `gui_layout_test.rs` compilation failure under `--no-default-features` by gating with `#[cfg(feature = "gui")]`.
2. Extract named constants for separator geometry in `tweaks_panel.rs` for improved clarity and maintainability.

## Target Files
- Modify: `plugin303/tests/gui_layout_test.rs`
- Modify: `plugin303/src/gui/panels/tweaks_panel.rs`

## Task 1: Gate `gui_layout_test.rs` behind `#[cfg(feature = "gui")]`
- Step 1: Verify failure with `cargo test --no-default-features --test gui_layout_test`
- Step 2: Add `#[cfg(feature = "gui")]` to `plugin303/tests/gui_layout_test.rs`
- Step 3: Run `cargo test --no-default-features` to confirm pass
- Step 4: Run `cargo test --test gui_layout_test` to confirm pass with default features
- Step 5: Git commit

## Task 2: Clarify separator constants in `tweaks_panel.rs`
- Step 1: Refactor separator overhead formula in `tweaks_panel.rs` using named constants or explicit documentation
- Step 2: Run `cargo test --bin standalone --lib` to confirm pass
- Step 3: Git commit
