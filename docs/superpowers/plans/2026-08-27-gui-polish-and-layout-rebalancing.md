# TECHNEO 303 GUI Polish & Layout Rebalancing Plan (Round 5)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebalance TECHNEO 303 GUI layouts to eliminate dead space in Regions 4, 5, and 2, standardize control affordances, expand the JIT label to JITTER, align the Transport column vertically with the sequencer grid, and ensure clean readout spacing under ADSR faders.

**Architecture:** Presentation-layer updates in `plugin303/src/gui/`.
- Rebalances column allocation and horizontal layout in Output Clipper (`output_clipper.rs`) and Tweaks panel (`tweaks_panel.rs`) to span available width (~1334px inner width) symmetrically.
- Adjusts Envelopes sub-panel (`top_controls.rs`) and MiniFader track geometry (`fader.rs`) for vertical clearance and balanced bank positioning.
- Expands JIT checkbox into a clear JITTER control and balances Transport column vertical spacing (`sequencer_panel.rs`).

**Tech Stack:** Rust (edition 2021), egui 0.31, nih_plug, nih_plug_egui.

**Spec:** `TECHNEO 303 GUI Visual Audit Report` (Visual captures: `target/gui-qa/latest.png`, `target/gui-qa/zone_mid.png`, `target/gui-qa/zone_top.png`, `target/gui-qa/zone_bottom.png`).

## Global Constraints

- Presentation layer only: do not change DSP algorithms, parameter ranges, defaults, or automation IDs.
- Preserve confirmed features: integer BPM (e.g. 120), orange selection accents (`#FF8800`), running playhead feedback on Step 04, bottom-row octave arrow clearance (>= 20px).
- Strict target dimensions: 1366×960 logical pixels (2732×1984 @2x).
- All unit tests and geometry invariants must pass (`cargo test --lib`).

---

### Task 1: Rebalance Output Clipper Layout (Region 4)

**Visual Evidence:** `target/gui-qa/zone_mid.png` shows Output Clipper controls terminating at ~45% width, with the right ~55% being empty black void.

**Files:**
- Modify: `plugin303/src/gui/panels/output_clipper.rs:10-170`
- Test: `plugin303/src/gui/panels/output_clipper.rs` (add `tests` module)

**Interfaces:**
- Consumes: `Plugin303Params` clipper parameters (`clipper_bypass`, `clipper_os`, `clipper_drive`, `clipper_threshold`, `clipper_bass`, `clipper_tone`, `clipper_in_gain`, `clipper_out_gain`, `clipper_tilt`, `clipper_dynamics`, `clipper_mix`, `clipper_gain_link`, `clipper_asym`).
- Produces: Fully width-balanced `render_output_clipper` distributing 9 rotary knobs, OS switch, bypass header, and Link/Diode selectors across the entire panel width.

- [ ] **Step 1: Write the failing unit test**

Add layout calculation test in `plugin303/src/gui/panels/output_clipper.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_clipper_knob_column_width() {
        // Inner width for 1366px window with 10px chassis margin and 6px panel inner margin
        let avail_w = 1366.0 - 20.0 - 12.0; // 1334.0
        let header_w = 120.0;
        let right_controls_w = 120.0;
        let knobs_avail_w = avail_w - header_w - right_controls_w;
        let col_w = knobs_avail_w / 9.0;
        assert!(col_w >= 100.0, "Knob column width must provide generous spacing (>=100px), got {col_w}");
    }
}
```

- [ ] **Step 2: Run test to verify it passes math assertion**

Run: `cargo test --lib gui::panels::output_clipper::tests`
Expected: PASS.

- [ ] **Step 3: Implement width-balanced Output Clipper layout**

In `plugin303/src/gui/panels/output_clipper.rs`:
- Calculate available width across the panel.
- Place header (Title + Bypass checkbox + OS slide switch) on the left (~120px).
- Distribute the 9 rotary knobs (`DRIVE`, `THRESH`, `BASS`, `TONE`, `IN`, `OUT`, `TILT`, `DYN`, `MIX`) across 9 equal-width column slots using `allocate_ui_with_layout(vec2(col_w, 64.0), Layout::top_down(Align::Center), ...)`.
- Place the right controls (LINK checkbox + DIODE SI/GE selector) in a clean right-aligned group.
- Maintain parameter bindings, hover tooltips, and interactive gain link sync.

- [ ] **Step 4: Run tests and verify**

Run: `cargo test --lib gui::panels::output_clipper`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add plugin303/src/gui/panels/output_clipper.rs
git commit -m "fix(gui): distribute output clipper controls across full panel width"
```

---

### Task 2: Rebalance Analog Character & Tweaks Panel (Region 5) & Standardize Affordances

**Visual Evidence:** `target/gui-qa/zone_mid.png` shows controls packed in the left ~25% of the panel with ~75% empty void. `TOL:` has dropdown chevron `▼` while `AGE:` and `NOISE:` use identical rounded boxes without chevrons.

**Files:**
- Modify: `plugin303/src/gui/panels/tweaks_panel.rs:8-185`
- Test: `plugin303/src/gui/panels/tweaks_panel.rs` (add `tests` module)

**Interfaces:**
- Consumes: `params.tolerance`, `params.comp_age`, `params.thermal_noise`, `params.comp_seed`, `params.comp_drift`, `params.recalibration_scale`, `params.calibrate_sync`, `params.topology`.
- Produces: 4 balanced columns/sections spanning full width, with distinct numeric drag styling for `AGE:`, `NOISE:`, `COMP SEED:` vs dropdown `TOL:`.

- [ ] **Step 1: Write the failing unit test**

In `plugin303/src/gui/panels/tweaks_panel.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tweaks_panel_column_geometry() {
        let avail_w = 1366.0 - 20.0 - 12.0; // 1334.0
        let col_w = avail_w / 4.0;
        assert_eq!(col_w, 333.5);
        assert!(col_w > 250.0, "Each of the 4 tweak groups has adequate horizontal space");
    }
}
```

- [ ] **Step 2: Run test to verify**

Run: `cargo test --lib gui::panels::tweaks_panel::tests`
Expected: PASS.

- [ ] **Step 3: Implement balanced 4-section Tweaks layout & affordance styling**

In `plugin303/src/gui/panels/tweaks_panel.rs`:
- Group controls into 4 balanced columns across `available_width()`:
  - Column 1 (Tolerance & Aging): `TOL:` dropdown (with `▼` affordance) + `AGE:` numeric drag badge with clear `[0..100 yrs]` styling.
  - Column 2 (Noise & Drift): `NOISE:` numeric drag badge `[0..100%]` + `DRIFT` rotary knob centered below.
  - Column 3 (Seed & Recalibration): `COMP SEED:` numeric drag badge `[0000..9999]` + `RECAL` rotary knob centered below.
  - Column 4 (Calibration Engine): CALIBRATE trigger button + status description text.
- Eliminate horizontal clustering on the left and remove the 75% dead space.

- [ ] **Step 4: Run tests and verify**

Run: `cargo test --lib gui::panels::tweaks_panel`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add plugin303/src/gui/panels/tweaks_panel.rs
git commit -m "fix(gui): balance analog character tweaks across 4 columns and clarify affordances"
```

---

### Task 3: Rebalance Envelopes Sub-Panel & MiniFader Clearance (Region 2)

**Visual Evidence:** `target/gui-qa/zone_top.png` shows ~40% gap between VEG ADSR bank and ADV ENV toggle on the right. Fader readouts (`3 ms`, `200 ms`, `0%`, `200 ms`) sit close to track bottom.

**Files:**
- Modify: `plugin303/src/gui/panels/top_controls.rs:80-205`
- Modify: `plugin303/src/gui/widgets/fader.rs:35-180`
- Test: `plugin303/src/gui/widgets/fader.rs`

**Interfaces:**
- Consumes: `params.meg_attack`, `params.meg_decay_adj`, `params.meg_sustain`, `params.meg_release`, `params.veg_attack`, `params.veg_decay_adj`, `params.veg_sustain`, `params.veg_release`, `params.advanced_env`.
- Produces: Balanced `ENVELOPES` sub-panel without middle dead gap, and `MiniFader` with 3px increased vertical breathing room between fader track bottom and value readout text.

- [ ] **Step 1: Update MiniFader geometry bounds test for increased vertical clearance**

In `plugin303/src/gui/widgets/fader.rs`:

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn mini_fader_geometry_bounds() {
        let height = 24.0_f32;
        let total_height = height + 30.0;
        let top = 0.0_f32;
        let bottom = top + total_height;
        let track_top = top + 10.0;
        let track_bottom = bottom - 15.0;
        let track_height = track_bottom - track_top;

        assert!(track_top > top, "Track top must have top label clearance");
        assert!(track_bottom < bottom, "Track bottom must have bottom readout clearance");
        assert!(bottom - track_bottom >= 15.0, "Readout text must have at least 15px clearance");
        assert!(track_height > 0.0, "Track height must be positive");
    }
}
```

- [ ] **Step 2: Run test to verify it fails against old constants**

Run: `cargo test --lib gui::widgets::fader::tests`
Expected: FAIL (until `widgets/fader.rs` geometry constants are updated).

- [ ] **Step 3: Implement MiniFader spacing & Envelopes sub-panel rebalancing**

In `plugin303/src/gui/widgets/fader.rs`:
- Update `total_height = self.height + 30.0;`, `track_top = rect.top() + 10.0;`, `track_bottom = rect.bottom() - 15.0;`.
- Set readout text at `pos2(rect.center().x, rect.bottom() - 1.0)`.

In `plugin303/src/gui/panels/top_controls.rs`:
- Rebalance `env_frame`: place `ADV ENV` checkbox adjacent to the `ENVELOPES` header or distribute MEG ADSR and VEG ADSR fader banks with proportional spacing so the 40% empty gap is eliminated.

- [ ] **Step 4: Run tests and verify**

Run: `cargo test --lib gui::widgets::fader gui::panels::top_controls`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add plugin303/src/gui/widgets/fader.rs plugin303/src/gui/panels/top_controls.rs
git commit -m "fix(gui): rebalance envelopes sub-panel and increase minifader readout clearance"
```

---

### Task 4: Transport Panel Full-Height Alignment & JITTER Label (Region 8)

**Visual Evidence:** `target/gui-qa/zone_bottom.png` shows Transport column stopping higher than sequencer Step 09-16 octave shift buttons, leaving a bottom-right void. Label shows cryptic `JIT`.

**Files:**
- Modify: `plugin303/src/gui/panels/sequencer_panel.rs:100-480`
- Test: `plugin303/src/gui/panels/sequencer_panel.rs`

**Interfaces:**
- Consumes: `params.seq_jitter`, `params.seq_run`, `params.seq_sync`, `params.midi_out`, `params.midi_out_channel`, `params.synth_bypass`, `params.seq_tempo`, `params.seq_length`, `params.seq_timebase`, `params.seq_mode`.
- Produces: Transport column that vertically spans the entire sequencer panel height down to the bottom margin, with `"JIT"` checkbox clearly labeled `"JITTER"`.

- [ ] **Step 1: Write the unit test for Transport Column layout metrics**

In `plugin303/src/gui/panels/sequencer_panel.rs`:

```rust
#[test]
fn jitter_button_dimensions_accommodate_label() {
    let btn_w = 56.0_f32;
    let min_w_for_jitter = 50.0_f32;
    assert!(btn_w >= min_w_for_jitter, "JITTER button width must be at least 50px to fit full label");
}
```

- [ ] **Step 2: Run test to verify**

Run: `cargo test --lib gui::panels::sequencer_panel::tests`
Expected: PASS.

- [ ] **Step 3: Update JITTER label and vertical distribution in Transport panel**

In `plugin303/src/gui/panels/sequencer_panel.rs`:
- Replace `"JIT"` text with `"JITTER"` and expand hit rectangle width to 56.0px.
- Adjust vertical spacing between Transport rows (`ui.spacing_mut().item_spacing = vec2(3.0, 5.0)`) or distribute rows to align the bottom of Row D with the bottom of Step 16 octave shift buttons.

- [ ] **Step 4: Run tests and verify**

Run: `cargo test --lib gui::panels::sequencer_panel`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add plugin303/src/gui/panels/sequencer_panel.rs
git commit -m "fix(gui): rename JIT to JITTER and align transport column height with sequencer grid"
```

---

### Task 5: End-to-End Visual QA & Capture Verification

**Files:**
- Run: `scripts/capture_standalone_gui.sh`
- Verify: `target/gui-qa/*.png`

- [ ] **Step 1: Run full unit test suite**

Run: `cargo test --lib`
Expected: 78+ tests passed, 0 failures.

- [ ] **Step 2: Run standalone capture script**

Run: `./scripts/capture_standalone_gui.sh`
Expected: Generates `latest.png`, `fun.png`, `running.png`, and all 8 dynamic crops without errors.

- [ ] **Step 3: Verify visual captures against the 7 audit items**

Inspect generated crops in `target/gui-qa/`:
1. `crop_clipper_tweaks.png` / `zone_mid.png`: Output Clipper spans full width without right-side black void.
2. `crop_clipper_tweaks.png` / `zone_mid.png`: Analog Character & Tweaks is distributed across 4 balanced columns.
3. `crop_classic.png` / `zone_top.png`: Envelopes sub-panel has no middle void; ADSR readouts have clear breathing room.
4. `crop_sequencer.png` / `crop_bottom_margin.png`: Transport panel aligns cleanly with sequencer bottom edge.
5. `crop_sequencer.png`: Checkbox clearly shows `JITTER`.
6. `crop_clipper_tweaks.png`: Affordances between TOL (dropdown) and numeric drag badges (AGE, NOISE, SEED) are consistent and distinct.

- [ ] **Step 4: Commit any script or test adjustments**

```bash
git commit --allow-empty -m "test(gui): verify visual QA captures across all 8 regions"
```
