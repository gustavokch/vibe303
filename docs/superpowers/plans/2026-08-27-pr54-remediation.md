# PR #54 Review Remediation Plan

- **Goal**: Clean up unused variable warning in `plugin303/src/lib.rs`, add unit tests for `MiniFader` bounds and `SoupedPanel` layout constants, and verify full test suite.
- **Architecture**: Rust audio plugin (`nih-plug`, `nih-plug-egui`).
- **Tech Stack**: Rust 2021 edition, `cargo test`, `egui`.

---

## Task 1: Clean redundant `synth_bypass` variable declaration

- **Target Files**:
  - Modify: `plugin303/src/lib.rs`
- **Consumes**: `params.synth_bypass`
- **Produces**: Warning-free `process` block
- **Step 1**: Run `cargo check --lib -p plugin303` to verify warning exists at line 277.
- **Step 2**: Remove redundant `let synth_bypass = self.params.synth_bypass.value();` at L277 (keep actual usage at L439).
- **Step 3**: Run `cargo check --lib -p plugin303` to confirm zero warnings.
- **Step 4**: Commit fix:
  ```bash
  git add plugin303/src/lib.rs
  git commit -m "fix(dsp): remove unused synth_bypass variable declaration in process"
  ```

---

## Task 2: Add MiniFader track geometry and bounds unit test

- **Target Files**:
  - Modify: `plugin303/src/gui/widgets/fader.rs`
- **Consumes**: `MiniFader` layout dimensions
- **Produces**: Unit tests validating track bounds and thumb travel geometry
- **Step 1**: Write test in `plugin303/src/gui/widgets/fader.rs` verifying `track_top < track_bottom` and track stays inside `total_height`.
- **Step 2**: Run `cargo test --package plugin303 --lib gui::widgets::fader` to confirm pass.
- **Step 3**: Commit test:
  ```bash
  git add plugin303/src/gui/widgets/fader.rs
  git commit -m "test(gui): add unit tests for MiniFader track geometry bounds"
  ```

---

## Task 3: Add SoupedPanel inner height constant unit test

- **Target Files**:
  - Modify: `plugin303/src/gui/panels/souped_panel.rs`
- **Consumes**: `SOUPED_INNER_HEIGHT`
- **Produces**: Unit test asserting `SOUPED_INNER_HEIGHT` pin and overlay card centering
- **Step 1**: Write unit test in `plugin303/src/gui/panels/souped_panel.rs`.
- **Step 2**: Run `cargo test --package plugin303 --lib gui::panels::souped_panel` to confirm pass.
- **Step 3**: Commit test:
  ```bash
  git add plugin303/src/gui/panels/souped_panel.rs
  git commit -m "test(gui): add unit tests for souped panel height and centering bounds"
  ```

---

## Task 4: Verification and Push

- **Step 1**: Run full test suite: `cargo test --package plugin303`.
- **Step 2**: Push to remote PR branch: `git push origin fix/gui-visual-remediation-r5`.
