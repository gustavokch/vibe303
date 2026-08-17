# GUI Refinement Plan

This document synthesizes visual QA findings and defines the actionable refinement plan for the `plugin303` graphical user interface (`nih_plug_egui`).
Evaluation compares visual QA test output against `docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md`.

## 1. Visual QA Scores Summary

Section visual quality scores and weighted calculation:
- **Classic Panel** (35% weight): **88 / 100**
- **Souped-Up Drawer Panel** (30% weight): **95 / 100**
- **Hardware Sequencer Panel** (35% weight): **92 / 100**

### Weighted Overall Score
`Overall Score = (0.35 * 88) + (0.30 * 95) + (0.35 * 92) = 30.80 + 28.50 + 32.20 = 91.50 / 100`

Specification Compliance Status: **`false`** (FAIL due to minor non-compliant features across classic panel and sequencer).

---

## 2. Synthesized QA Findings & Defect Analysis

### 2.1 Classic Panel (Score: 88)
- **Header Bar Grill Vents (Minor - Missing Feature):**
  - *Expected:* Header bar features industrial brushed metal texture with centered speaker/grill vent slots between TECHNEO logo and TB-303 PLUGIN branding per design mockup.
  - *Observed:* Flat dark header background without metallic grill or vent slots.
  - *Fix:* Add procedural grill vent lines and metallic gradient background in `header_bar.rs`.
- **Waveform & Fidelity Tier Slide Switches (Minor - Geometry):**
  - *Expected:* Slide switches with vertical detent position labels aligned cleanly next to detent tracks as depicted in hardware mockup.
  - *Observed:* Horizontal slide switches with detent labels positioned horizontally underneath the track.
  - *Fix:* Adjust detent label alignment and track margins in `switch.rs` to match detent positions cleanly.
- **Knob Value Readouts (Minor - Typography):**
  - *Expected:* Clear separation and high contrast between parameter name and value readout.
  - *Observed:* Value readouts (+0 ct, 2.1 kHz, 50%) are displayed in low-contrast grey text close to parameter label.
  - *Fix:* Increase text contrast and vertical padding for knob value readouts in `knob.rs`.
- **Panel Background & Metallic Borders (Minor - Color):**
  - *Expected:* Bevelled metallic lines (`#2a2e33`) and industrial brushed steel chassis background (`#16181b`) surrounding classic 7-knob panel per Spec §5.
  - *Observed:* Flat dark panel frame without metallic gradient strokes or bevelled inset lines.
  - *Fix:* Apply bevelled stroke borders and metallic gradient background to `classic_panel.rs` container frame.

### 2.2 Souped-Up Engine Drawer Panel (Score: 95)
- **Circuit Topology Accurate-Mode Dimming Overlay (Minor - Alignment / Validation):**
  - *Expected:* In Accurate mode, Souped-Up Drawer should dim to ~30% opacity with non-interactive overlay badge 'Accurate Mode Active (Souped-up Engine Bypassed)'.
  - *Observed:* Circuit Topology switch is currently set to FUN mode; drawer is fully vibrant and active as specified for FUN mode.
  - *Fix:* Ensure Accurate mode visual state testing confirms 30% opacity dimming and overlay badge presentation when toggled to ACCURATE.

### 2.3 Hardware Sequencer Panel (Score: 92)
- **Step Pitch Readout Octave Nudge Buttons (Minor - Missing Feature):**
  - *Expected:* Step pitch buttons should feature Up/Down arrows (▲ / ▼) to nudge octaves up or down (-2 to +2) per section 3.3 of design spec.
  - *Observed:* Pitch buttons display formatted note text (e.g., 'C 3', 'D# 3') without dedicated octave nudge arrows (▲ / ▼).
  - *Fix:* Add ▲ / ▼ nudge buttons inside or adjacent to pitch readout tile in `step_button.rs` for quick octave transposition.

---

## 3. Actionable Refinement Implementation Plan

### Priority 1: Add Sequencer Octave Nudge Buttons
1. Update `plugin303/src/gui/widgets/step_button.rs` and `sequencer_panel.rs` to render dedicated ▲ / ▼ octave nudge arrows adjacent to pitch readout tiles.

### Priority 2: Refine Header Bar Industrial Finish & Grill Vents
1. Update `plugin303/src/gui/panels/header_bar.rs` to apply procedural brushed metal background texture and centered speaker/grill vent slots between branding elements.

### Priority 3: Polish Classic Panel Frame & Knob Readouts
1. Update `plugin303/src/gui/panels/classic_panel.rs` container frame with bevelled metallic stroke borders (`#2a2e33`) and brushed steel background (`#16181b`).
2. Update `plugin303/src/gui/widgets/knob.rs` to increase contrast and vertical spacing for engineering unit value readouts.

### Priority 4: Re-align Slide Switch Labels & Track Geometry
1. Update `plugin303/src/gui/widgets/switch.rs` to align detent position labels cleanly with vertical detent tracks.

### Priority 5: Verify Accurate-Mode Drawer Dimming & Overlay Badge
1. Verify `souped_panel.rs` 30% opacity dimming overlay and non-interactive badge when Circuit Topology is set to `ACCURATE`.
