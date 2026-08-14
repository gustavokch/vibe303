# GUI Visual Refinement Plan (`vibe303`)

**Date:** 2026-08-14  
**Target Spec:** `/Users/gus/Git/vibe303/docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md`  
**Overall Visual QA Weighted Score:** 91.40 / 100  
**Overall Spec Compliance:** Non-Compliant (`false`)

---

## 1. Executive Summary & Score Breakdown

Visual QA inspection was conducted across all three GUI sections against the design specification in `/Users/gus/Git/vibe303/docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md`. The overall weighted score is 91.40 / 100. `classic_panel` failed visual spec compliance due to missing top-right header branding and unformatted knob value readouts, rendering the overall GUI non-compliant.

| Section | Alignment Score | Weight | Weighted Score | Spec Compliance |
| :--- | :--- | :--- | :--- | :--- |
| **Classic 303 Front Panel** | 88.00 | 35% | 30.800 | `false` |
| **Souped-Up Drawer** | 90.00 | 30% | 27.000 | `true` |
| **Sequencer Console** | 96.00 | 35% | 33.600 | `true` |
| **Total / Overall** | **91.40** | **100%** | **91.400** | `false` |

---

## 2. Detailed Findings & Defect Analysis

### 2.1 Classic 303 Front Panel (`classic_panel`) — Non-Compliant (88.00)

1. **Header Bar Branding (`missing_feature` - Minor):**
   - **Observed:** Top-left header displays `VIBE 303 ANALOG CIRCUIT EMULATION`. Top-right orange `TB-303 PLUGIN` label is omitted.
   - **Expected:** Top-left header bar displays `TECHNEO` branding logo. Top-right header bar includes orange `TB-303 PLUGIN` branding text per design specification.
   - **Suggested Fix:** Update `/Users/gus/Git/vibe303/plugin303/src/gui/panels/header_bar.rs` to display `TECHNEO` branding on the left and render orange `TB-303 PLUGIN` text on the right side of the header bar.

2. **Knob Value Readout Formatting (`typography` - Minor):**
   - **Observed:** Knobs for Cutoff, Resonance, Env Mod, Decay, and Accent display unformatted raw float values (for example `0.5`).
   - **Expected:** Knob value readouts below dials display formatted physical engineering units (e.g. Hz/kHz for Cutoff, % for Resonance/Env Mod/Accent, ms/s for Decay).
   - **Suggested Fix:** Refactor value formatting in `/Users/gus/Git/vibe303/plugin303/src/gui/widgets/knob.rs` to inspect parameter display strings or convert normalized values into physical unit strings.

3. **Classic Panel Layout (`alignment` - Compliant):**
   - **Observed:** All 7 knobs and 2 slide switches render in alignment with 270-degree radial tick arcs and correct color tokens.
   - **Expected:** 7 rotary dials positioned horizontally above Waveform and Fidelity Tier slide switches.
   - **Suggested Fix:** Maintain current layout grid and widget proportions in `/Users/gus/Git/vibe303/plugin303/src/gui/panels/classic_panel.rs`.

---

### 2.2 Souped-Up Engine Drawer (`souped_drawer`) — Compliant (90.00)

1. **Standalone GUI Capture Target (`missing_feature` - Major):**
   - **Observed:** Screenshot capture tool captured Bitwig Studio window rendering Waldorf Blofeld VST interface instead of plugin303 standalone GUI window.
   - **Expected:** Visual QA screenshot capturing plugin303 standalone GUI window rendering the Souped-Up Engine Drawer panel.
   - **Suggested Fix:** Update `/Users/gus/Git/vibe303/scripts/capture_standalone.sh` to target and focus the plugin303 standalone window explicitly by PID or window title before capture.

2. **MEG/VEG ADSR MiniFader Widgets (`alignment` - Compliant):**
   - **Observed:** MiniFader implementation in `/Users/gus/Git/vibe303/plugin303/src/gui/widgets/fader.rs` renders 45px vertical tracks with glowing orange fill bars, A/D/S/R labels, and formatted units (ms/s/%).
   - **Expected:** Vertical fader tracks with glowing orange fill bars for MEG/VEG ADSR with A/D/S/R headers and formatted unit readouts.
   - **Suggested Fix:** Maintain current implementation adhering to Section 3.2 of the GUI specification.

3. **Accurate-Mode Dimming Badge & Overlay (`color` - Compliant):**
   - **Observed:** `/Users/gus/Git/vibe303/plugin303/src/gui/panels/souped_panel.rs` paints 30% (0x4d) alpha tint over disabled controls with animated hover switch button that updates topology parameter.
   - **Expected:** 30% opacity overlay with non-interactive controls, orange-bordered banner, and topology switch button.
   - **Suggested Fix:** Maintain current implementation matching Section 1 and Section 4.1 specifications.

---

### 2.3 16-Step Sequencer Console (`sequencer`) — Compliant (96.00)

1. **Step Column Grid & Transport Bar (`alignment` - Compliant):**
   - **Observed:** 16 step columns rendered with 42px width and 3px spacing. Transport bar fixed at 125px width with RUN/STOP, SYNC, TEMPO, MODE, and TIMEBASE controls.
   - **Expected:** 16-step strip aligned horizontally in a continuous row alongside a 125px left transport control column.
   - **Suggested Fix:** Maintain current grid structure and column spacing in `/Users/gus/Git/vibe303/plugin303/src/gui/panels/sequencer_panel.rs`.

2. **LED States & Gate Tile Colors (`color` - Compliant):**
   - **Observed:** Color tokens in `/Users/gus/Git/vibe303/plugin303/src/gui/theme.rs` and `/Users/gus/Git/vibe303/plugin303/src/gui/widgets/step_button.rs` match design specification (Playhead LED `#ff3355`, ACC LED `#ffaa00`, SLD LED `#00e5ff`, Note `#00e5ff`, Tie `#ffd700`, Rest `#38404c`).
   - **Expected:** High-contrast LED states and gate tile colors adhering to Section 3.3 and Section 5 design tokens.
   - **Suggested Fix:** Maintain current color token assignments.

3. **Pitch Display & Nudge Controls (`typography` - Compliant):**
   - **Observed:** 9.5pt bold monospace text for note pitch, click opens PianoRollPopup, vector arrows for octave up/down nudge.
   - **Expected:** Monospace pitch readout with clickable popover trigger and octave nudge controls per Section 3.3.
   - **Suggested Fix:** Maintain current pitch display implementation.

---

## 3. Actionable Refinement Tasks by Target File

### 3.1 `/Users/gus/Git/vibe303/plugin303/src/gui/panels/header_bar.rs`
- [ ] **Task 1 (Minor - Missing Feature):** Restore `TECHNEO` branding on top-left of header bar.
- [ ] **Task 2 (Minor - Missing Feature):** Render orange `TB-303 PLUGIN` text on top-right of header bar.

### 3.2 `/Users/gus/Git/vibe303/plugin303/src/gui/widgets/knob.rs`
- [ ] **Task 3 (Minor - Typography):** Update knob value formatting to display physical engineering units (Hz/kHz, %, ms/s) instead of raw normalized float strings.

### 3.3 `/Users/gus/Git/vibe303/scripts/capture_standalone.sh`
- [ ] **Task 4 (Major - Capture Harness):** Update capture script to target and bring plugin303 standalone window into focus before screen capture.

---

## 4. Verification & Quality Gates

1. **Cargo Build & Integration Check:**
   ```bash
   cargo check -p plugin303 --all-features
   ```
2. **Audio Thread Zero-Allocation Gate:**
   ```bash
   cargo test -p plugin303 --test no_alloc
   ```
3. **Visual QA Gate Target:**
   Re-run screenshot inspection harness (`gui-visual-qa-loop`) and verify:
   - `overall_score >= 95.0`
   - `spec_compliance == true`
   - All header branding and knob value readouts pass visual inspection.
