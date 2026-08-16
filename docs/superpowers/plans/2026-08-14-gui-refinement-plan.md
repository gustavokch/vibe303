# GUI Refinement Plan

**Date:** 2026-08-14  
**Target:** plugin303 egui GUI Implementation  
**Status:** Approved  

---

## 1. Executive Summary

- **Overall Weighted Visual Score:** 92.9 / 100
  - Classic Panel Score: 88 (Weight: 35%)
  - Souped Drawer Score: 95 (Weight: 30%)
  - Sequencer Score: 96 (Weight: 35%)
- **Spec Compliance:** Pass (true)

---

## 2. Synthesis of Visual QA Findings

### 2.1 Classic Panel (Score: 88)
1. **Fidelity Tier Switch Labels (Minor Geometry Defect):**
   - *Observed:* Labels (LOW, HIGH, ULTRA) position horizontally under the slider track.
   - *Expected:* Labels stack vertically to the right of the horizontal slider slot.
   - *Fix:* Update label layout logic in `classic_panel.rs` to place detent labels vertically adjacent to detent snap positions.

2. **Waveform Selector Switch (Minor Geometry Defect):**
   - *Observed:* Waveform switch uses vertical slide track with stacked labels on the right.
   - *Expected:* Horizontal metallic sliding carriage inside recessed rectangular cutout.
   - *Fix:* Reorient `SlideSwitch` rendering orientation parameter to horizontal in `switch.rs` for waveform selection.

3. **Knob Bezel Tick Marks (Minor Color/Stroke Defect):**
   - *Observed:* High-contrast bright lines around classic dials.
   - *Expected:* Subtle low-contrast bezel integration per specification.
   - *Fix:* Reduce opacity to 30% and line width to 1.0px in `knob.rs` tick mark draw list.

### 2.2 Souped Drawer Panel (Score: 95)
1. **MEG/VEG ADSR Fader Value Readouts (Minor Typography Defect):**
   - *Observed:* Readouts position directly below fader tracks without sufficient vertical margin.
   - *Expected:* Clear readout labels with minimum 3px padding from track bottom.
   - *Fix:* Add 3px top margin allocation above text labels in `souped_panel.rs`.

### 2.3 Sequencer Panel (Score: 96)
1. **Color Tokens & LED Indicators:**
   - *Observed:* NOTE (Cyan #00e5ff), TIE (Gold #ffd700), REST (Slate #333a42), ACC (Amber #ffaa00), SLD (Cyan #00e5ff) strictly comply with theme tokens.
2. **Alignment & Grid Layout:**
   - *Observed:* 16-step grid, pitch nudge buttons, transport bar, and generator controls align with design specifications.

---

## 3. Actionable Refinement Tasks

| Task ID | Component | File Path | Defect | Action Items |
|---|---|---|---|---|
| REFINEMENT-01 | Classic Panel | `plugin303/src/gui/panels/classic_panel.rs` | Fidelity Tier Labels | Reposition LOW, HIGH, ULTRA labels to vertical alignment adjacent to detents. |
| REFINEMENT-02 | Classic Panel | `plugin303/src/gui/widgets/switch.rs` | Waveform Selector Switch | Set horizontal slider orientation and render knurled silver carriage. |
| REFINEMENT-03 | Classic Panel | `plugin303/src/gui/widgets/knob.rs` | Knob Bezel Tick Marks | Reduce radial stroke thickness to 1.0px and alpha to 35%. |
| REFINEMENT-04 | Souped Drawer | `plugin303/src/gui/panels/souped_panel.rs` | ADSR Fader Value Padding | Add 3px vertical spacing between fader track bottom and value label. |

---

## 4. Verification Gate

1. Run `cargo test -p plugin303 --all-features` to verify GUI compilation and unit test status.
2. Verify visual alignment against `docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md`.
