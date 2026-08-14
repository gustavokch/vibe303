# GUI Visual Refinement Plan (`vibe303`)

**Date:** 2026-08-14  
**Target Spec:** `docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md`  
**Overall Visual QA Weighted Score:** 82.53 / 100  
**Overall Spec Compliance:** Non-Compliant (`false`)

---

## 1. Executive Summary & Score Breakdown

Visual QA analysis was performed across the three main GUI sections. The Classic 303 Front Panel and Souped-Up Engine Drawer pass spec compliance gates, but the Sequencer Console is non-compliant due to critical layout overflow (only steps 1-10 visible in 1080px width), major widget layout/indicator defects, and typography issues.

| Section | Alignment Score | Weight | Weighted Score | Spec Compliance |
| :--- | :--- | :--- | :--- | :--- |
| **Classic 303 Front Panel** | 91.50 | 35% | 32.025 | `true` |
| **Souped-Up Drawer** | 96.00 | 30% | 28.800 | `true` |
| **Sequencer Console** | 62.00 | 35% | 21.700 | `false` |
| **Total / Overall** | **82.53** | **100%** | **82.525** | `false` |

---

## 2. Design Spec Comparison & Defect Analysis

### 2.1 Classic 303 Front Panel (`classic_panel`) — Compliant (91.50)
- **Header Bar Logo & Branding (`typography` - Minor):** Spec calls for TECHNEO logo on top-left and orange `TB-303 PLUGIN` title text on top-right. Observed header currently displays `VIBE 303 / ANALOG CIRCUIT EMULATION`.
- **Knob Parameter Title Labels (`typography` - Minor):** Spec Section 3.1 requires parameter titles above rotary dials to be rendered in bold uppercase typography. Observed knob labels use standard-weight proportional font with manual offset painting instead of dedicated bold font styling.

### 2.2 Souped-Up Engine Drawer (`souped_drawer`) — Compliant (96.00)
- **Accurate Mode Dimming Overlay & Badge (`alignment` - Minor):** Spec Section 4.1 compliant. 30% alpha premultiplied dark overlay (`rgba(0x0e, 0x11, 0x16, 0x4d)`) covering disabled controls with centered banner and functional button to switch topology. No fix needed.
- **MEG & VEG ADSR MiniFaders (`geometry` - Minor):** Spec Section 3.2 compliant. Dual 4-fader banks with 45px height, recessed tracks, glowing orange fill bars, silver thumbs with center notches, and text readouts. No fix needed.
- **Extended Oscillator & Filter Dials (`color` - Minor):** Spec Section 3.2 compliant. Complete set of 10 extended knobs and VCF TAPS / TOLERANCE slide switches arranged in two compact horizontal rows. No fix needed.

### 2.3 Sequencer Console (`sequencer`) — Non-Compliant (62.00)
- **16-Step Sequencer Strip Layout (`geometry` - Critical):** Spec Section 1 & Section 5 mandate all 16 step columns visible within the panel container boundary (1080px window width). Observed layout only displays steps 01 through 10; steps 11 to 16 overflow and are clipped on the right side.
- **ACC and SLD Toggle Buttons (`alignment` - Major):** Spec Section 3.3 requires ACC and SLD toggle buttons to be rendered as compact side-by-side widgets with label and LED dot contained inside the button rect. Observed layout renders `ACC` and `SLD` text labels at the top of the column while square toggle buttons `[□] [□]` are rendered detached at the bottom below pitch readout.
- **ACC and SLD LED State Indicators (`missing_feature` - Major):** Spec Section 3.3 specifies glowing amber LED dot (`#ffaa00`) for active ACC state and glowing cyan LED dot (`#00e5ff`) for active SLD state. Observed buttons display uncolored empty square outline boxes `[□] [□]` without glowing LED indicator dots.
- **Octave Nudge Button Symbols (`typography` - Minor):** Spec Section 3.3 requires clear arrow symbols `▲` and `▼`. Observed octave nudge buttons display missing character glyph boxes `[□] [□]`.
- **Pitch Readout Formatting (`typography` - Minor):** Spec Section 3.3 specifies formatted pitch string with a space between note name and octave (e.g. `C 3`, `F# 2`). Observed pitch string is formatted without space as `C3`.

---

## 3. Actionable Refinement Tasks by Target File

### 3.1 Sequencer Console (`plugin303/src/gui/panels/sequencer_panel.rs` & `plugin303/src/gui/widgets/step_button.rs`)

1. **Fix Step Column Width & 16-Step Strip Overflow** `[Severity: Critical, Category: geometry]`
   - **Target Files:** `plugin303/src/gui/panels/sequencer_panel.rs`, `plugin303/src/gui/widgets/step_button.rs`
   - **Fix:** Recalculate step column width and padding. Inside `sequencer_panel.rs`, fit 16 columns into available horizontal space (e.g. 1080px panel width minus margins / 16 ≈ 60-64px per step strip, or reduce step_button inner width/spacing so all 16 columns render within bounds without horizontal scrolling/clipping).

2. **Unify ACC/SLD Button Layout with Integrated LED Indicators** `[Severity: Major, Category: alignment & missing_feature]`
   - **Target File:** `plugin303/src/gui/widgets/step_button.rs`
   - **Fix:** Redesign ACC and SLD toggle widgets into unified side-by-side button controls inside `StepButton`. Integrate text label (`ACC` / `SLD`) and glowing LED dot (`circle_filled` with `ACCENT_AMBER` `#ffaa00` when ACC is active, and `SLIDE_CYAN` `#00e5ff` when SLD is active) directly within each button's bounding rectangle.

3. **Fix Octave Nudge Button Characters** `[Severity: Minor, Category: typography]`
   - **Target File:** `plugin303/src/gui/widgets/step_button.rs`
   - **Fix:** Replace missing character boxes in octave nudge buttons with standard ASCII symbols (`^` / `v`) or configure proper unicode font fallbacks for directional triangles (`▲` / `▼`).

4. **Format Pitch Readout with Space** `[Severity: Minor, Category: typography]`
   - **Target File:** `plugin303/src/gui/widgets/step_button.rs`
   - **Fix:** Update `format_pitch` helper function to format string as `format!("{} {}", note_name, octave)` so readout displays `C 3` instead of `C3`.

### 3.2 Classic 303 Front Panel (`plugin303/src/gui/panels/header_bar.rs` & `plugin303/src/gui/widgets/knob.rs`)

1. **Update Header Bar Logo & Branding** `[Severity: Minor, Category: typography]`
   - **Target File:** `plugin303/src/gui/panels/header_bar.rs` (or `classic_panel.rs`)
   - **Fix:** Update header bar title to show `TECHNEO` logo on top-left and add right-aligned orange text `TB-303 PLUGIN`.

2. **Update Knob Parameter Label Styling** `[Severity: Minor, Category: typography]`
   - **Target File:** `plugin303/src/gui/widgets/knob.rs`
   - **Fix:** Update `Knob303` title label painter to use true bold font weight styling (`egui::FontId` with bold weight token or `TextStyle::Name("Bold".into())`).

---

## 4. Verification & Quality Gates

1. **Compilation Check:**
   `cargo check -p plugin303 --all-features`
2. **Audio Thread Safety Check:**
   `cargo test -p plugin303 --test no_alloc`
3. **Visual Quality Gate Target:**
   Re-run visual QA inspection harness and confirm:
   - `overall_score >= 95.0`
   - `spec_compliance == true`
   - All 16 sequencer steps visible and properly rendered in 1080px width.
