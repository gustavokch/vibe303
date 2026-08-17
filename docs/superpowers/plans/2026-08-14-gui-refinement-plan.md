# GUI Refinement Plan

This document synthesizes visual QA findings and defines the actionable refinement plan for the `plugin303` graphical user interface (`nih_plug_egui`).
Evaluation compares visual QA test output against `docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md`.

## 1. Visual QA Scores Summary

Section visual quality scores and weighted calculation:
- **Classic Panel** (35% weight): **72 / 100**
- **Souped-Up Drawer Panel** (30% weight): **70 / 100**
- **Hardware Sequencer Panel** (35% weight): **98 / 100**

### Weighted Overall Score
`Overall Score = (0.35 * 72) + (0.30 * 70) + (0.35 * 98) = 25.20 + 21.00 + 34.30 = 80.50 / 100`

Specification Compliance Status: **`false`** (FAIL due to drawer mini-fader clipping and missing classic panel styling features).

---

## 2. Synthesized QA Findings & Defect Analysis

### 2.1 Classic Panel (Score: 72)
- **Knob Radial Tick Marks (Major - Missing Feature):**
  - *Expected:* Outer bezel ring with radial tick marks spanning a 270° sweep (from 135° to 405° / 7 o'clock to 5 o'clock) as detailed in spec §3.1.
  - *Observed:* Classic knobs feature a smooth orange arc outline around the dial without radial tick marks.
  - *Fix:* Draw radial tick mark lines along the outer arc bezel in `knob.rs` widget implementation.
- **Knob Metallic Finish (Major - Color):**
  - *Expected:* Brushed-aluminum rotary dials with silver metallic gradient (`#d0d4d8` to `#888e96`) as detailed in spec §5 and mockup.
  - *Observed:* Knobs are rendered with flat dark grey disc faces and bright orange perimeter arcs.
  - *Fix:* Apply multi-stop metallic silver gradients to knob face rendering in `knob.rs`.
- **Waveform Switch Layout (Minor - Geometry):**
  - *Expected:* Vertical 3-position toggle/slide switch detent layout matching the 303 front panel design mockup.
  - *Observed:* Waveform control is laid out as a horizontal track slider.
  - *Fix:* Update `switch.rs` / `classic_panel.rs` to support vertical orientation detents for the Waveform selector.
- **Header Chassis Vent Slits (Minor - Missing Feature):**
  - *Expected:* Vertical cooling vent/grille slits in the top center aluminum chassis between TECHNEO logo and TB-303 PLUGIN header text.
  - *Observed:* Header bar features a plain dark panel strip without vent slits.
  - *Fix:* Add procedural vent slit lines to `header_bar.rs` top panel area.
- **TECHNEO Logo & Header Branding (Minor - Typography):**
  - *Expected:* Custom stylized metallic TECHNEO brand mark with horizontal bar block logo.
  - *Observed:* Header uses an orange square dot next to flat sans-serif text.
  - *Fix:* Update `header_bar.rs` branding draw routines to match mockup logotype.

### 2.2 Souped-Up Engine Drawer Panel (Score: 70)
- **MEG / VEG ADSR Mini-Faders (Major - Geometry / Clipping):**
  - *Expected:* All 4 MEG ADSR faders (Attack, Decay, Sustain, Release) and 4 VEG ADSR faders should be fully visible inside the Souped-Up panel container.
  - *Observed:* MEG ADSR faders are truncated at the right edge of the window (only Attack 'A' is partially visible; Decay, Sustain, Release and all VEG faders are cut off beyond the window boundary).
  - *Fix:* Refactor the Souped-Up panel layout grid or reduce panel horizontal padding so that MEG and VEG ADSR vertical fader blocks fit within the 1080px layout bounds.
- **Accurate Mode Dimming Overlay & Badge (Minor - Missing Feature / Validation):**
  - *Expected:* When Circuit Topology is toggled to Accurate mode, the Souped-Up Drawer should render at 30% opacity with an 'Accurate Mode Active (Souped-up Engine Bypassed)' overlay badge and disabled controls.
  - *Observed:* The Circuit Topology switch is currently set to FUN mode, leaving the drawer fully vibrant; Accurate mode dimming and badge overlay are not engaged in this capture.
  - *Fix:* Capture QA screenshot with Circuit Topology set to Accurate mode to verify 30% opacity dimming and overlay badge rendering.

### 2.3 Hardware Sequencer Panel (Score: 98)
- **Grid Layout & Interactive Note Cycling (Pass):** 16 step columns, playhead LED tracking, step gate cycling (Note/Tie/Rest), ACC/SLD toggles, and pitch nudge function cleanly as specified.
  - *Action:* Retain existing layout structure and verified visual presentation.

---

## 3. Actionable Refinement Implementation Plan

### Priority 1: Fix Souped-Up Drawer Fader Clipping & Layout Constraints
1. Edit `plugin303/src/gui/panels/souped_panel.rs` to adjust column spacing and padding.
2. Re-arrange controls so all 4 MEG ADSR faders (Attack, Decay, Sustain, Release) and 4 VEG ADSR faders fit comfortably inside the 1080px layout boundaries.

### Priority 2: Refine Knob Rendering in `knob.rs`
1. Implement multi-stop metallic silver gradients (`#d0d4d8` to `#888e96`) on the knob face.
2. Add procedural radial tick marks along the 270° sweep (135° to 405° / 7 o'clock to 5 o'clock) around the outer bezel ring.

### Priority 3: Reorient Waveform Switch Layout
1. Update `plugin303/src/gui/panels/classic_panel.rs` and `plugin303/src/gui/widgets/switch.rs` to support vertical detent switch rendering for the Waveform selector.

### Priority 4: Polish Header Bar Visuals
1. Update `plugin303/src/gui/panels/header_bar.rs` to render procedural chassis vent slits between the TECHNEO logo and header title.
2. Refine TECHNEO logotype rendering to feature a stylized metallic brand mark.

### Priority 5: Verify Accurate Mode Dimming & Badge
1. Ensure `souped_panel.rs` correctly renders at 30% opacity with an interactive override badge when topology is set to `Accurate`.
