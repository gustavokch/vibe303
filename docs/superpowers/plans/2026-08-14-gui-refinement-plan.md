# Vibe303 GUI Visual Refinement Plan

**Date:** 2026-08-14  
**Target:** `nih_plug_egui` frontend implementation for `plugin303`  
**Design Spec Reference:** `docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md`  

---

## Executive Summary & Scorecard

A visual QA audit was conducted comparing the current GUI implementation against the approved design specification (`docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md`). The overall weighted alignment score is **71.1 / 100**, and the GUI currently **fails** spec compliance (`spec_compliance: false`).

| Section | Spec Weight | Section Alignment Score | Weighted Contribution | Compliance Status |
| :--- | :---: | :---: | :---: | :---: |
| **Classic 303 Front Panel** | 35% | 85.0 | 29.75 | Needs Minor Refinement |
| **Souped-Up Engine Drawer** | 30% | 62.0 | 18.60 | **Non-Compliant (Critical)** |
| **16-Step Hardware Sequencer** | 35% | 65.0 | 22.75 | **Non-Compliant (Critical)** |
| **Overall Score** | **100%** | — | **71.1 / 100** | **FAIL (`spec_compliance: false`)** |

---

## Top Visual & Structural Defects

### Critical Severity Defects
1. **Unpopulated Souped-Up Engine Drawer (`souped_drawer`)**
   - *Observation:* The right drawer panel area titled 'SOUPED-UP ENGINE DRAWER' is unpopulated except for a small inline `(Bypassed)` text label. All MEG/VEG ADSR vertical faders, extended oscillator controls, filter slope selectors, and analog drift controls are missing from the panel.
   - *Spec Requirement:* Section 1.2, 3.2, 5. Dedicated drawer containing dual oscillator controls, pulse width, filter slope taps, drive, glide, wear/drift dials, and MEG/VEG ADSR faders.
   - *File(s):* `plugin303/src/gui/panels/souped_panel.rs`, `plugin303/src/gui/mod.rs`

2. **Missing MEG/VEG ADSR Vertical Mini-Faders (`souped_drawer`)**
   - *Observation:* `MiniFader` widget is implemented in `widgets/fader.rs`, but zero faders are rendered on screen in the captured layout.
   - *Spec Requirement:* Section 3.2. Vertical mini-faders with glowing orange fill bars for MEG (A, D, S, R) and VEG (A, D, S, R).
   - *File(s):* `plugin303/src/gui/panels/souped_panel.rs`, `plugin303/src/gui/widgets/fader.rs`

3. **16-Step Sequencer Column Overflow (`sequencer`)**
   - *Observation:* Steps 12 through 16 overflow past the right boundary of the 1080px panel width and are clipped off screen.
   - *Spec Requirement:* Section 1.4, 5. All 16 step columns must be fully visible and fit cleanly within the 1080px fixed window width.
   - *File(s):* `plugin303/src/gui/panels/sequencer_panel.rs`, `plugin303/src/gui/widgets/step_button.rs`

### Major Severity Defects
4. **Incorrect Extended Dials Location (`souped_drawer`)**
   - *Observation:* Extended dials (Distortion/Drive, Sub Vol, Envelope, Glide, Osc2 Pitch, Reso Comp, Osc2 Mix) are placed horizontally in `top_controls.rs` across the middle bar rather than inside the dedicated right-hand Souped-Up Drawer panel.
   - *Spec Requirement:* Section 1.2, 3.2. Extended controls must reside within the Souped-Up Engine Drawer panel on the right side.
   - *File(s):* `plugin303/src/gui/panels/top_controls.rs`, `plugin303/src/gui/panels/souped_panel.rs`

5. **Incomplete Accurate Mode Dimming Badge & Overlay (`souped_drawer`)**
   - *Observation:* In Accurate mode, the right drawer panel is completely unpopulated with a small inline text label `(Bypassed)` rather than a dimmed layout with an interactive quick-switch overlay badge.
   - *Spec Requirement:* Section 1.3, 4.1. Full panel controls rendered at ~30% opacity with non-interactive overlay badge indicating *"Accurate Mode Active (Souped-up Engine Bypassed)"* and a quick-switch button to enable Fun mode.
   - *File(s):* `plugin303/src/gui/panels/souped_panel.rs`

6. **Missing Sequencer Playhead LED Bar (`sequencer`)**
   - *Observation:* No playhead LED indicators are rendered above step numbers.
   - *Spec Requirement:* Section 3.3. Top LED bar above each step column that illuminates in neon coral (`#ff3355`) when playing.
   - *File(s):* `plugin303/src/gui/panels/sequencer_panel.rs`, `plugin303/src/gui/widgets/step_button.rs`, `plugin303/src/gui/widgets/led.rs`

7. **Missing ACC / SLD LED Indicator Dots (`sequencer`)**
   - *Observation:* ACC and SLD buttons are rendered as plain text pills without indicator LED dots.
   - *Spec Requirement:* Section 3.3. ACC button with amber (`#ffaa00`) LED dot and SLD button with cyan (`#00e5ff`) LED dot.
   - *File(s):* `plugin303/src/gui/widgets/step_button.rs`

8. **Clipped Pitch Nudge Octave Arrow Controls (`sequencer`)**
   - *Observation:* Octave up/down nudge arrow buttons (`▲` / `▼`) are vertically clipped at the bottom edge of step column cards.
   - *Spec Requirement:* Section 3.3. Nudge buttons fully visible and interactive below pitch note readout.
   - *File(s):* `plugin303/src/gui/widgets/step_button.rs`

9. **Knob Disc Fill Color (`classic_panel`)**
   - *Observation:* Knob body fill is flat dark slate (`theme::KNOB_BODY_DARK #1a1d21`) instead of brushed aluminum.
   - *Spec Requirement:* Section 3.1, 5. Brushed-aluminum metallic disc styling with silver gradient (`#d0d4d8` to `#888e96`).
   - *File(s):* `plugin303/src/gui/widgets/knob.rs`, `plugin303/src/gui/theme.rs`

### Minor Severity Defects
10. **Knob Bezel Tick Marks (`classic_panel`)**
    - *Observation:* Continuous arc progress bar rendered around the knob without discrete radial tick marks.
    - *Spec Requirement:* Section 3.1. Outer bezel ring with discrete radial tick marks spanning a 270° sweep (from 135° to 405°).
    - *File(s):* `plugin303/src/gui/widgets/knob.rs`

11. **Pitch Note Readout Spacing (`sequencer`)**
    - *Observation:* Note readout displayed without space separator (`C3`).
    - *Spec Requirement:* Section 3.3. Formatted note readout with space separator (e.g. `C 3`, `F# 2`).
    - *File(s):* `plugin303/src/gui/widgets/step_button.rs`

---

## Step-by-Step Actionable Refinement Plan

### Phase 1: Classic Panel & Knob Visual Polish (`classic_panel`)
- **Task 1.1: Update `Knob303` Metallic Styling & Bezel Ticks (`knob.rs`, `theme.rs`)**
  - Modify `Knob303` rendering in `plugin303/src/gui/widgets/knob.rs` to draw discrete radial tick lines across the 270° sweep angle range (135° to 405°).
  - Update center knob circle fill to use brushed aluminum metallic tokens (`#d0d4d8` to `#888e96`) defined in `plugin303/src/gui/theme.rs`.

### Phase 2: Souped-Up Engine Drawer Implementation & Topology Overlay (`souped_drawer`)
- **Task 2.1: Re-architect Souped-Up Panel Layout (`souped_panel.rs`, `top_controls.rs`, `mod.rs`)**
  - Create `plugin303/src/gui/panels/souped_panel.rs` and wire it into the master layout grid in `plugin303/src/gui/mod.rs` on the right side.
  - Relocate extended controls (Distortion/Drive, Sub Vol, Envelope, Glide, Osc2 Pitch, Reso Comp, Osc2 Mix) from `top_controls.rs` into `souped_panel.rs`.
- **Task 2.2: Implement MEG/VEG ADSR Mini-Faders (`souped_panel.rs`, `fader.rs`)**
  - Add MEG ADSR vertical fader group (Attack, Decay, Sustain, Release) and VEG ADSR vertical fader group (Attack, Decay, Sustain, Release) using `MiniFader` instances with orange accent fill bars (`#ff8800`).
- **Task 2.3: Implement Accurate Mode Dimming & Quick-Switch Overlay (`souped_panel.rs`)**
  - When `params.topology` is set to `CircuitTopology::Accurate`, render the complete `souped_panel` visual hierarchy at ~30% opacity with input handling disabled (`Ui::set_enabled(false)`).
  - Add a floating/centered overlay badge indicating *"Accurate Mode Active (Souped-up Engine Bypassed)"* containing a quick-switch button to enable `Fun` mode.

### Phase 3: 16-Step Sequencer Console Layout & Visual Refinements (`sequencer`)
- **Task 3.1: Fix Step Column Grid Spacing & Overflow (`sequencer_panel.rs`)**
  - Adjust step column width, margins, and padding in `plugin303/src/gui/panels/sequencer_panel.rs` so all 16 step columns fit within the available 1080px panel width without clipping or horizontal scroll.
- **Task 3.2: Implement Playhead LED Bar (`step_button.rs`, `led.rs`, `sequencer_panel.rs`)**
  - Add a neon coral (`#ff3355`) top LED bar indicator at the top of each step column in `step_button.rs`, driven by `current_playing_step` atomic state read from `Plugin303`.
- **Task 3.3: Add ACC / SLD LED Indicator Dots (`step_button.rs`)**
  - Render an explicit amber (`#ffaa00`) glowing LED dot next to the `ACC` text label and a cyan (`#00e5ff`) LED dot next to the `SLD` text label on step modifier buttons.
- **Task 3.4: Fix Pitch Nudge Button Vertical Bounds & Note Readout Formatting (`step_button.rs`)**
  - Increase vertical bounds / adjust padding inside step card widgets so octave up/down arrows (`▲` / `▼`) fit completely without bottom truncation.
  - Format pitch readout string to include a space separator between note name and octave number (e.g. `C 3` instead of `C3`).

---

## Verification & Acceptance Criteria

1. **Visual QA Re-Audit Gate:**
   - `classic_panel` alignment score >= 95
   - `souped_drawer` alignment score >= 95
   - `sequencer` alignment score >= 95
   - **Overall Weighted Alignment Score >= 95** with `spec_compliance: true`.
2. **Build & Automated Verification:**
   - `cargo test -p plugin303 --all-features` passes cleanly without warnings or errors.
   - `cargo test -p plugin303 --test no_alloc` passes to confirm zero UI thread lock leakage into audio processing.
