# Session Handoff: vibe303 GUI Visual QA Remediation

**Date:** 2026-08-22  
**Branch:** `feat/gui-frontend-refinements`  
**Plan:** `docs/superpowers/plans/2026-08-22-gui-refinement-plan.md`  
**Target Codebase:** `plugin303/src/gui/` (1280x720 Fixed Canvas)

---

## 1. Executive Summary

Visual QA audit (`gui-visual-qa-loop` with dual-matrix adversarial inspection) identified 2 Critical, 4 Major, and 10 Minor defects across topology states and control layouts.

- **Critical:** Accurate mode vertical collapse (146px dead void at bottom) and drawer squishing (156pt -> 102pt).
- **Major:** Sequencer bottom margin collision (<2px clearance), floating overlay badge in Accurate mode, inconsistent dimming across bypassed panels, Volume knob unity gain offset (4:30 instead of 12:00 detent).
- **Minor:** Unipolar arc on bipolar knobs, quality tier color token, raw resistance ohm fallback, crop script coordinate.

---

## 2. Five-Step Remediation Plan

1. **Step 1: Structural Height & Topology Stability (DEF-01, DEF-02, DEF-04)**
   - `plugin303/src/gui/panels/souped_panel.rs`: Enforce fixed container height (156.0pt) regardless of `is_accurate` state.
   - Anchor Accurate mode overlay box to static frame center coordinates.

2. **Step 2: Bipolar Knobs & Value Normalization (DEF-06, DEF-08, DEF-14)**
   - `plugin303/src/gui/widgets/knob.rs`: Add `.bipolar()` builder flag and center-arc painter (12 o'clock detent).
   - Apply to `TUNING`, `VOLUME`, `PITCH1`, `PITCH2`, `TILT`, `IN`, and `OUT`.
   - Remove raw resistance ohm string formatting (`kΩ`); convert to normalized percentages or decibels.

3. **Step 3: Sequencer Boundary & Containment Polish (DEF-03, DEF-10, DEF-11)**
   - `plugin303/src/gui/panels/sequencer_panel.rs`: Increase Frame `inner_margin` bottom from 4.0 to 6.0.
   - `plugin303/src/gui/widgets/step_button.rs`: Adjust vertical item spacing to 1.0px for 4px clearance.

4. **Step 4: Visual Harmony, Color Tokens & Unit Formatting (DEF-05, DEF-07, DEF-12, DEF-13, DEF-15, DEF-16)**
   - `plugin303/src/gui/panels/header_bar.rs`: Set `SlideSwitch` quality thumb color to `theme::ACCENT_RED`.
   - `plugin303/src/gui/panels/output_clipper.rs`: Pass `theme::ACCENT_RUBY` to `SlideSwitch` OS selector; update unselected GE diode button contrast.
   - `plugin303/src/gui/panels/generator_panel.rs`: Add zero-padded formatting (`{:02}`) to `PULSES (k)`, `STEPS (n)`, `SHIFT`.
   - Standardize alpha dimming on bypassed panels in Accurate mode.

5. **Step 5: Test & Crop Script Alignment (DEF-09)**
   - `scripts/crop_gui_sections.py`: Adjust `crop_souped` Y-start from 240 to 230.
   - Run `cargo test --all` and re-run `gui-visual-qa-loop` to verify 100.0% spec compliance.
