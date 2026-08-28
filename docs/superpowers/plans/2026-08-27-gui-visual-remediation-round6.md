# GUI Visual Remediation & Polish (Round 6) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Resolve all 5 GUI visual defects identified in the Round 6 visual audit (Tweaks panel grid rebalance, sequencer pitch kerning, playhead active column highlight, envelope fader disabled contrast, and octave down label standardization).

**Architecture:** Presentation-only polish across 4 egui GUI modules (`tweaks_panel.rs`, `step_button.rs`, `fader.rs` / `top_controls.rs`, `generator_panel.rs`). Ensure strict isolation from DSP/audio logic, zero window geometry changes, and preserve 2732×1984 HiDPI render.

**Tech Stack:** Rust, `egui`, `nih_plug_egui`, `sips` for image dimension validation.

**Spec:** GUI Visual Audit Report: TECHNEO 303 (Round 6).

## Global Constraints

- **Presentation Layer Only**: Do not modify DSP parameters, audio processing logic, parameter layout, or MIDI routing.
- **Zero Layout Breakage**: Target window resolution remains 2732 × 1984 (HiDPI 2x scale). No overflow or scrollbars.
- **Preserve Fixed Alignment**: Keep Transport panel Row D bottom edge aligned with Step 16 bottom, Output Clipper controls inside panel bounds.
- **Verification Gate**: `cargo test --lib` must pass, `./scripts/capture_standalone_gui.sh` must run cleanly, and `sips -g pixelWidth target/gui-qa/latest.png` must be exactly 2732.

---

### Task 1: Rebalance Analog Character & Tweaks Panel Grid

**Files:**
- Modify: `plugin303/src/gui/panels/tweaks_panel.rs:40-250`
- Test: `plugin303/tests/gui_build.rs`

**Interfaces:**
- Consumes: `Plugin303Params`, `ParamSetter`, `theme::*`
- Produces: `render_tweaks_panel(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter)`

- [ ] **Step 1: Inspect existing layout allocation bug in tweaks_panel.rs**

Verify why columns bunched to the left: `allocate_ui_with_layout` lacked `ui.set_min_width(col_w)` / `ui.set_width(col_w)`, causing each column to shrink to its minimum child width and leaving 1000px of dead space on the right.

- [ ] **Step 2: Restructure 4 columns with explicit width expansion**

In `plugin303/src/gui/panels/tweaks_panel.rs`:
1. Calculate `col_w = (total_w - sep_w) / 4.0`.
2. Inside each of the 4 `ui.allocate_ui_with_layout` blocks, call `ui.set_min_width(col_w);` and `ui.set_width(col_w);`.
3. Column 1 (Tolerances & Aging): `TOL` ComboBox, `AGE` DragValue, and `NOISE` DragValue arranged vertically with clean spacing.
4. Column 2 (Drift & Instability): `DRIFT` Knob centered vertically.
5. Column 3 (Seed & Recalibration): `SEED` DragValue and `RECAL` Knob centered vertically.
6. Column 4 (Circuit Calibration): `CALIBRATE` action Button (120×24px) and status label ("CALIBRATED" / "SYNC PENDING") centered vertically.

- [ ] **Step 3: Run cargo test to verify GUI compilation and tests**

Run: `cargo test --lib --test gui_build`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add plugin303/src/gui/panels/tweaks_panel.rs
git commit -m "fix(gui): rebalance tweaks panel into 4 full-width columns"
```

---

### Task 2: Standardize Step Pitch Text Spacing & Font Kerning

**Files:**
- Modify: `plugin303/src/gui/widgets/step_button.rs:19-28, 250-274`
- Test: `plugin303/tests/piano_roll_mapping.rs`

**Interfaces:**
- Consumes: `format_pitch(semitone: i8, octave: i8) -> String`
- Produces: Proportional pitch string rendering centered in step box pill

- [ ] **Step 1: Check format_pitch and font rendering in step_button.rs**

In `plugin303/src/gui/widgets/step_button.rs`:
1. Ensure `format_pitch` returns `"{} {}"` (single space between note and octave).
2. Change the pitch label painter font in line 270 from `FontFamily::Monospace` (which creates wide faux-double-space gaps) to `FontId::new(9.5, FontFamily::Proportional)` (matching the gate and octave button text).
3. Ensure `pitch_rect.center()` and `Align2::CENTER_CENTER` keep the pitch label perfectly centered in the pitch pill.

- [ ] **Step 2: Run unit tests**

Run: `cargo test --test piano_roll_mapping`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add plugin303/src/gui/widgets/step_button.rs
git commit -m "fix(gui): render step pitch with proportional font to fix kerning"
```

---

### Task 3: Enhance Active Playhead Column Visibility

**Files:**
- Modify: `plugin303/src/gui/widgets/step_button.rs:40-220`
- Test: `plugin303/tests/gui_build.rs`

**Interfaces:**
- Consumes: `is_active = is_current && is_running`
- Produces: Visible playhead column outline, header glow, and gate cell highlight

- [ ] **Step 1: Add playhead column stroke and gate cell highlight**

In `plugin303/src/gui/widgets/step_button.rs`:
1. When `is_active` is true:
   - In step header cell (Step 01-16): paint background with `Color32::from_rgba_unmultiplied(224, 90, 71, 60)` and stroke with `Stroke::new(1.5, theme::ACCENT_RUBY)`.
   - On full `strip_rect`: draw an outer border stroke `p.rect_stroke(strip_rect, CornerRadius::same(3), Stroke::new(1.5, theme::ACCENT_RUBY.linear_multiply(0.7)), StrokeKind::Inside);` so the entire column is outlined.
   - In gate cell (NOTE/TIE/REST): when `is_active`, add subtle glow fill `Color32::from_rgba_unmultiplied(224, 90, 71, 35)` and stroke with `Stroke::new(1.0, theme::ACCENT_RUBY)`.

- [ ] **Step 2: Run cargo test**

Run: `cargo test --lib`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add plugin303/src/gui/widgets/step_button.rs
git commit -m "feat(gui): highlight entire active playhead column during playback"
```

---

### Task 4: Polish Envelope Fader Inactive State Contrast

**Files:**
- Modify: `plugin303/src/gui/widgets/fader.rs:40-120`
- Modify: `plugin303/src/gui/panels/top_controls.rs:170-220`
- Test: `plugin303/tests/gui_build.rs`

**Interfaces:**
- Consumes: `MiniFader::new(&Param, setter, label, min_label, max_label, is_enabled)`
- Produces: Legible fader track, thumb, and labels when `ADV ENV` is unchecked (≥ 35% luminance)

- [ ] **Step 1: Update MiniFader inactive styling in fader.rs**

In `plugin303/src/gui/widgets/fader.rs`:
1. Track stroke: when `!is_enabled`, use `theme::BORDER_DEFAULT` (instead of invisible `theme::BORDER_SUBTLE`).
2. Thumb cap outline: when `!is_enabled`, use `theme::BORDER_DEFAULT`.
3. Thumb cap fill: when `!is_enabled`, use `theme::SURFACE_SUBTLE`.
4. Header label (`A`, `D`, `S`, `R`): when `!is_enabled`, use `theme::TEXT_SECONDARY` (instead of dark `TEXT_MUTED`).
5. Readout text below fader: when `!is_enabled`, use `theme::TEXT_SECONDARY`.

- [ ] **Step 2: Update Envelope Bank Title styling in top_controls.rs**

In `plugin303/src/gui/panels/top_controls.rs`:
1. `MEG ADSR` and `VEG ADSR` titles: when `!is_adv_env`, use `theme::TEXT_SECONDARY` (instead of dark `TEXT_MUTED`).

- [ ] **Step 3: Run cargo test**

Run: `cargo test --lib`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add plugin303/src/gui/widgets/fader.rs plugin303/src/gui/panels/top_controls.rs
git commit -m "fix(gui): increase contrast of inactive envelope faders and labels"
```

---

### Task 5: Standardize Octave Abbreviation in Euclidean Generator

**Files:**
- Modify: `plugin303/src/gui/panels/generator_panel.rs:258-270`
- Test: `plugin303/tests/generator.rs`

**Interfaces:**
- Consumes: `AcidGenParams`
- Produces: `OCT DOWN:` label in Section 2

- [ ] **Step 1: Replace OCT DN label with OCT DOWN**

In `plugin303/src/gui/panels/generator_panel.rs` line 260:
Change `nih_plug_egui::egui::RichText::new("OCT DN:")` to `nih_plug_egui::egui::RichText::new("OCT DOWN:")`.

- [ ] **Step 2: Run cargo test**

Run: `cargo test --lib --test generator`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add plugin303/src/gui/panels/generator_panel.rs
git commit -m "fix(gui): standardize octave abbreviation to OCT DOWN"
```

---

### Task 6: Visual QA Capture & Resolution Verification

**Files:**
- Run: `./scripts/capture_standalone_gui.sh`
- Verify: `target/gui-qa/latest.png`, `target/gui-qa/running.png`, `target/gui-qa/fun.png`

- [ ] **Step 1: Run capture script**

Run: `./scripts/capture_standalone_gui.sh`
Expected: Exit code 0, captures generated in `target/gui-qa/`

- [ ] **Step 2: Verify pixel dimensions**

Run: `sips -g pixelWidth target/gui-qa/latest.png`
Expected: `pixelWidth: 2732`

- [ ] **Step 3: Verify visual QA crops for all 5 issues**

1. Check `latest_crop_clipper_tweaks.png`: 4 balanced columns across full width, no dead space gap.
2. Check `latest_crop_sequencer.png`: Step pitch formatted as `"C 3"`, `"D# 3"` with crisp proportional kerning.
3. Check `running_crop_sequencer.png`: Step 04 active column highlighted clearly from header to gate.
4. Check `latest_crop_classic.png`: Inactive `MEG ADSR` / `VEG ADSR` faders and values legible with high contrast.
5. Check `latest_crop_clipper_tweaks.png` generator section: `OCT DOWN:` label cleanly rendered.

- [ ] **Step 4: Commit QA test artifacts or updates**

```bash
git add -A
git commit -m "test(gui): verify round-6 GUI remediation visual captures"
```
