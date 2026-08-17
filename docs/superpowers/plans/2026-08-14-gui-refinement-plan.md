# GUI Refinement Plan

This document synthesizes visual QA findings and defines the actionable refinement plan for the `plugin303` graphical user interface (`nih_plug_egui`).
Evaluation compares visual QA test output against `docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md`.

## 1. Visual QA Scores Summary

Section visual quality scores and weighted calculation:
- **Classic Panel** (35% weight): **85 / 100**
- **Souped-Up Drawer Panel** (30% weight): **65 / 100**
- **Hardware Sequencer Panel** (35% weight): **96 / 100**

### Weighted Overall Score
`Overall Score = (0.35 * 85) + (0.30 * 65) + (0.35 * 96) = 29.75 + 19.50 + 33.60 = 82.85 / 100`

Specification Compliance Status: **`false`** (FAIL due to critical drawer fader clipping and missing branding/radial ticks).

---

## 2. Synthesized QA Findings & Defect Analysis

### 2.1 Classic Panel (Score: 85)
- **Header Bar Branding (Major - Missing Feature):**
  - *Expected:* 'TB-303 PLUGIN' title branding rendered in orange on top-right of header bar.
  - *Observed:* Contains Circuit Topology switch but lacks 'TB-303 PLUGIN' text branding.
  - *Fix:* Add 'TB-303 PLUGIN' text branding in orange color (`#ff8800` / `#ffaa00`) to top-right header bar in `plugin303/src/gui/panels/header_bar.rs`.
- **Knob Tick Marks (Minor - Geometry):**
  - *Expected:* Discrete radial tick mark lines surrounding each knob outer bezel across a 270° sweep.
  - *Observed:* Knobs render smooth orange indicator arcs without radial tick mark lines around outer bezels.
  - *Fix:* Render procedural radial tick marks along knob outer bezel 270° sweep in `plugin303/src/gui/widgets/knob.rs`.
- **Waveform Selector Switch (Minor - Alignment):**
  - *Expected:* Horizontal slide switch track with SAW, SQUARE, SWEEP detents as depicted in UI mockup.
  - *Observed:* Waveform selector uses vertical sliding track orientation.
  - *Fix:* Update Waveform `SlideSwitch` layout orientation from vertical to horizontal track in `plugin303/src/gui/panels/classic_panel.rs` / `plugin303/src/gui/widgets/switch.rs`.

### 2.2 Souped-Up Engine Drawer Panel (Score: 65)
- **MEG and VEG ADSR Faders (Critical - Missing Feature/Clipping):**
  - *Expected:* Eight vertical mini-faders with glowing orange fill bars for MEG (Attack, Decay, Sustain, Release) and VEG (Attack, Decay, Sustain, Release) rendered inside the Souped-Up Drawer.
  - *Observed:* MEG and VEG faders are clipped and missing from drawer view; only faint text 'MEG A 3ms' is partially visible at extreme right edge.
  - *Fix:* Adjust panel width allocation, padding, or dial layout in `plugin303/src/gui/panels/souped_panel.rs` so all 8 envelope faders fit completely within drawer bounds.
- **Drawer Control Layout (Major - Geometry):**
  - *Expected:* Rotary dials, filter tap switches, age/noise inputs, and MEG/VEG faders proportioned to fit cleanly within right panel grid.
  - *Observed:* Rotary dials (Drive, Sub, Glide, Pitch1, Pitch2, Mix2, PW, Sweep, R-Comp) and Age/Noise controls span full drawer width, forcing envelope section out of bounds.
  - *Fix:* Re-align extended dials into tighter columns or stacked sub-rows to preserve layout budget for right-hand fader strip.
- **Accurate Mode Dimming Overlay Badge (Minor - Validation):**
  - *Expected:* When Circuit Topology is set to Accurate, Souped-Up Drawer renders at 30% opacity with non-interactive overlay badge 'Accurate Mode Active (Souped-up Engine Bypassed)'.
  - *Observed:* Needs explicit toggle layer verification to ensure overlay banner badge renders properly when Accurate mode is enabled.
  - *Fix:* Verify and enforce egui layer painting for Accurate mode banner overlay and opacity response in `plugin303/src/gui/panels/souped_panel.rs`.

### 2.3 Hardware Sequencer Panel (Score: 96)
- **Gate Tiles Color Mapping (Pass - Minor):** NOTE in cyan (`#00e5ff`), TIE in gold (`#ffd700`), REST in dark slate (`#333a42`) strictly adhere to color specs.
  - *Action:* Maintain current distinct color mapping for step visibility.
- **ACC and SLD LED Status Indicators (Pass - Minor):** Amber dot for ACC toggle, cyan dot for SLD toggle illumination.
  - *Action:* Maintain high-contrast LED dot rendering across step columns.
- **16-Step Grid Alignment & Pitch Controls (Pass - Minor):** Uniform step column widths, playhead LEDs, note readouts, and pitch nudge buttons (`▲` / `▼`).
  - *Action:* Retain precise column grid layout and alignment in `plugin303/src/gui/widgets/step_button.rs` and `plugin303/src/gui/panels/sequencer_panel.rs`.

---

## 3. Actionable Refinement Implementation Plan

### Step 1: Fix Souped-Up Drawer Layout & Envelope Fader Clipping
1. Modify `plugin303/src/gui/panels/souped_panel.rs` column structure.
2. Re-arrange rotary dials into compact sub-grids (e.g. 3×3 or 4×2 tight grid).
3. Allocate dedicated width (e.g. 260px) for MEG and VEG ADSR vertical mini-faders.
4. Ensure full drawer width fits within the 1080px layout bounds without clipping.

### Step 2: Add Header Bar Orange 'TB-303 PLUGIN' Branding
1. Open `plugin303/src/gui/panels/header_bar.rs`.
2. Add orange title branding label `'TB-303 PLUGIN'` on the top-right section preceding the Topology toggle switch.

### Step 3: Implement Knob Bezel Procedural Radial Tick Marks
1. Open `plugin303/src/gui/widgets/knob.rs`.
2. Draw 11 procedural radial tick mark lines along the 270° sweep (135° to 405°) around outer bezel ring.

### Step 4: Reorient Waveform Selector Switch to Horizontal Track
1. Open `plugin303/src/gui/panels/classic_panel.rs` / `plugin303/src/gui/widgets/switch.rs`.
2. Configure Waveform slide switch layout orientation to render as horizontal track with SAW, SQUARE, SWEEP detents.

### Step 5: Verification & Quality Gate Check
1. Run `cargo test -p plugin303 --all-features`.
2. Confirm overall weighted visual quality score meets target gate (>= 90/100) with zero clipping and `spec_compliance: true`.
