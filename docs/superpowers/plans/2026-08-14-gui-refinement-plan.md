# GUI Visual Refinement Plan (`vibe303`)

**Date:** 2026-08-14  
**Target Spec:** `/Users/gus/Git/vibe303/docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md`  
**Overall Visual QA Weighted Score:** 70.55 / 100  
**Overall Spec Compliance:** Non-Compliant (`false`)

---

## 1. Executive Summary & Score Breakdown

Visual QA inspection synthesis across three GUI sections against the design specification in `docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md`. The overall weighted score is 70.55 / 100. All three sections failed visual spec compliance due to layout truncation, parameter placement errors, switch text stacking, and off-center overlay calculations.

| Section | Alignment Score | Weight | Weighted Score | Spec Compliance |
| :--- | :--- | :--- | :--- | :--- |
| **Classic 303 Front Panel** | 75.00 | 35% | 26.250 | `false` |
| **Souped-Up Engine Drawer** | 45.00 | 30% | 13.500 | `false` |
| **16-Step Sequencer Console** | 88.00 | 35% | 30.800 | `false` |
| **Total / Overall** | **70.55** | **100%** | **70.550** | `false` |

---

## 2. Detailed Findings & Defect Analysis

### 2.1 Classic 303 Front Panel (`classic_panel`) — Non-Compliant (75.00)

1. **Knob Parameter Label Position (`alignment` - Major):**
   - **Observed:** Parameter titles rendered above the knobs in `knob.rs` (lines 64-77).
   - **Expected:** Parameter titles (TUNING, CUTOFF, RESONANCE, etc.) positioned below each rotary dial as in `2026-07-02-ui-mockup.jpg`.
   - **Suggested Fix:** Update `Knob303` layout in `knob.rs` to place parameter title label below the rotary dial bezel ring and ticks.

2. **Slide Switch Label Layout (`alignment` - Major):**
   - **Observed:** Option text rendered inline below horizontal track detents in `switch.rs` (lines 170-186).
   - **Expected:** Position labels (SAW / SQUARE / SWEEP and LOW / HIGH / ULTRA) arranged vertically adjacent to slider track as in `2026-07-02-ui-mockup.jpg`.
   - **Suggested Fix:** Refactor `SlideSwitch` widget layout to support vertical option label stacking to the right of the slider track.

3. **Knob Label - Resonance (`typography` - Minor):**
   - **Observed:** Label is abbreviated as 'RESO' in `classic_panel.rs` (line 34).
   - **Expected:** Label formatted as 'RESONANCE' matching `2026-07-02-ui-mockup.jpg` and 2026-08-13 GUI spec.
   - **Suggested Fix:** Change 'RESO' to 'RESONANCE' in `classic_panel.rs` `Knob303` constructor call.

4. **Knob Radius Uniformity (`geometry` - Minor):**
   - **Observed:** Knobs have non-uniform radii (14.0, 16.0, 15.0) in `classic_panel.rs`.
   - **Expected:** Consistent dial dimensions across classic 303 panel controls.
   - **Suggested Fix:** Standardize knob radius across all 7 classic dials (e.g. 15.0px or 16.0px).

5. **Fidelity Tier Slider Cap Color (`color` - Minor):**
   - **Observed:** Fidelity Tier thumb uses flat red stroke/fill without red track housing accent.
   - **Expected:** Distinctive red anodized knurled slider cap matching `2026-07-02-ui-mockup.jpg`.
   - **Suggested Fix:** Enhance `SlideSwitch` styling for Fidelity Tier with high-contrast red anodized thumb texture and dark recessed housing.

6. **Header Bar Ventilation Grill & Branding Layout (`missing_feature` - Minor):**
   - **Observed:** Header ventilation grill slots rendered, but 'TECHNEO • VIBE 303' sub-label text hierarchy and grill contrast differ slightly from mockup.
   - **Expected:** Header bar features prominent orange 'TB-303 PLUGIN' title on top right and dual 7-slot chassis ventilation grills.
   - **Suggested Fix:** Adjust `HeaderBar` typography hierarchy and grill line stroke contrast to align with brushed metallic chassis aesthetic.

---

### 2.2 Souped-Up Engine Drawer (`souped_drawer`) — Non-Compliant (45.00)

1. **Souped-Up Engine Drawer Container (`geometry` - Critical):**
   - **Observed:** Drawer layout expands past right window margin due to unconstrained horizontal dial rows, causing right-side controls and faders to be truncated off-screen.
   - **Expected:** Drawer fits completely within 1080px chassis width alongside Classic 303 Panel without horizontal clipping or extending off-screen.
   - **Suggested Fix:** Reorganize drawer interior into compact multi-row sub-panels and enforce strict maximum width bounds.

2. **MEG & VEG ADSR Mini-Faders (`missing_feature` - Critical):**
   - **Observed:** MEG and VEG ADSR fader groups are placed at the far right of an unconstrained horizontal container, pushing them off-screen past the right window boundary.
   - **Expected:** MEG ADSR (Attack, Decay, Sustain, Release) and VEG ADSR (Attack, Decay, Sustain, Release) mini-faders with orange glowing fill bars are fully visible in drawer.
   - **Suggested Fix:** Relocate MEG and VEG ADSR mini-fader groups beneath or alongside dial sections in a visible multi-row structure.

3. **Accurate Mode Overlay Banner & Switch Button (`alignment` - Major):**
   - **Observed:** Overlay rect center is calculated from `content_response.response.rect` (unconstrained child bounds), positioning banner and switch button off-center to the right.
   - **Expected:** Accurate Mode Active banner overlay and switch button are centered over the visible drawer area.
   - **Suggested Fix:** Calculate overlay banner position relative to visible panel bounds (`ui.max_rect()`) rather than unbounded child content response rect.

4. **Extended Oscillator & Drive Dial Rows (`geometry` - Major):**
   - **Observed:** 10 extended dials are placed in wide horizontal rows, consuming excessive horizontal width and forcing adjacent fader widgets off-screen.
   - **Expected:** Extended dials (OSC1 DET, OSC2 DET, OSC2 MIX, OSC1 PW, SUB VOL, DRIVE, GLIDE, DRIFT, AGE, RESO COMP) grouped in structured, compact sub-panels.
   - **Suggested Fix:** Group extended dials into a 2-row grid layout (Row 1: Dual Osc controls; Row 2: Drive, Glide, Analog Wear) for spatial efficiency.

5. **Accurate Mode Dimming Badge & Overlay Tint (`color` - Minor):**
   - **Observed:** Dark overlay tint is applied at ~30% opacity, but banner box frame lacks contrast and uses standard panel border styling.
   - **Expected:** Visually dimmed/ghosted (~30% opacity with non-interactive overlay badge) in Accurate mode with SOUPED_ORANGE accent border banner.
   - **Suggested Fix:** Apply SOUPED_ORANGE accent stroke on the overlay banner card and preserve 30% background dimming opacity.

---

### 2.3 16-Step Sequencer Console (`sequencer`) — Non-Compliant (88.00)

1. **ACC and SLD Toggle Button Labels (`typography` - Minor):**
   - **Observed:** Buttons display single letter abbreviations 'A' and 'S' due to 17px button width constraints.
   - **Expected:** Buttons labeled explicitly as 'ACC' and 'SLD' per section 3.3 of design specification.
   - **Suggested Fix:** Expand button width slightly or adjust font sizing to render full 'ACC' and 'SLD' labels.

2. **Octave Up/Down Nudge Buttons Height (`geometry` - Minor):**
   - **Observed:** Octave nudge buttons are sized at 17x14px, creating a 2px height disparity with the 17x16px ACC/SLD buttons.
   - **Expected:** Consistent button height (16px) matching ACC/SLD modifier buttons directly above.
   - **Suggested Fix:** Standardize octave button height to 16.0px to maintain consistent vertical rhythm across step columns.

3. **Transport Bar Column Width (`alignment` - Minor):**
   - **Observed:** Transport column set to fixed 100.0px width, causing tight layout margin against the vertical separator.
   - **Expected:** Sufficient horizontal padding for TEMPO knob, MODE switch, and TIMEBASE controls without crowding step 01.
   - **Suggested Fix:** Increase transport container width to 110.0px or optimize item spacing in transport vertical layout.

4. **Gate Rest Badge Contrast (`color` - Minor):**
   - **Observed:** Rest badge background color (#38404c) with text (#d8dee8) has lower contrast ratio compared to Note/Tie badges.
   - **Expected:** Clear high-contrast text across Note, Tie, and Rest gate badge states.
   - **Suggested Fix:** Brighten text color or refine REST badge fill color to maximize legibility.

---

## 3. Actionable Refinement Tasks by Target File

### 3.1 `/Users/gus/Git/vibe303/plugin303/src/gui/widgets/knob.rs` & `classic_panel.rs`
- [ ] **Task 1 (Major - Alignment):** Reposition parameter label text below the rotary dial bezel in `Knob303`.
- [ ] **Task 2 (Minor - Typography):** Change 'RESO' label to 'RESONANCE' in `classic_panel.rs` `Knob303` instantiation.
- [ ] **Task 3 (Minor - Geometry):** Standardize dial radius across all 7 classic panel knobs (15.0px).

### 3.2 `/Users/gus/Git/vibe303/plugin303/src/gui/widgets/switch.rs` & `classic_panel.rs`
- [ ] **Task 4 (Major - Alignment):** Refactor `SlideSwitch` layout to stack option detent labels vertically on the right side of the slider track.
- [ ] **Task 5 (Minor - Color):** Enhance Fidelity Tier slide switch styling with red anodized thumb texture and dark recessed track housing.

### 3.3 `/Users/gus/Git/vibe303/plugin303/src/gui/panels/header_bar.rs`
- [ ] **Task 6 (Minor - Missing Feature):** Adjust HeaderBar typography hierarchy and ventilation grill stroke contrast.

### 3.4 `/Users/gus/Git/vibe303/plugin303/src/gui/panels/souped_panel.rs`
- [ ] **Task 7 (Critical - Geometry):** Reorganize drawer interior into a 2-row grid layout for extended dials (Row 1: Dual Osc; Row 2: Drive, Glide, Wear) to fit within 1080px window width.
- [ ] **Task 8 (Critical - Missing Feature):** Relocate MEG and VEG ADSR mini-fader groups into visible multi-row bounds within the drawer panel container.
- [ ] **Task 9 (Major - Alignment):** Calculate Accurate Mode overlay banner center relative to visible canvas bounds (`ui.max_rect()`).
- [ ] **Task 10 (Minor - Color):** Add `SOUPED_ORANGE` accent border stroke to the Accurate Mode overlay card.

### 3.5 `/Users/gus/Git/vibe303/plugin303/src/gui/panels/sequencer_panel.rs` & `step_button.rs`
- [ ] **Task 11 (Minor - Typography):** Expand button width or reduce font size to display full 'ACC' and 'SLD' labels on step modifier buttons.
- [ ] **Task 12 (Minor - Geometry):** Standardize octave up/down nudge button height to 16.0px.
- [ ] **Task 13 (Minor - Alignment):** Increase transport container width to 110.0px to improve spacing before step 01.
- [ ] **Task 14 (Minor - Color):** Improve text contrast ratio on the REST gate badge state.

---

## 4. Verification & Quality Gates

1. **Cargo Build Verification:**
   ```bash
   cargo check -p plugin303 --all-features
   ```
2. **Audio Thread Zero-Allocation Gate:**
   ```bash
   cargo test -p plugin303 --test no_alloc
   ```
3. **Visual QA Gate Target:**
   Re-run visual QA screenshot harness (`gui-visual-qa-loop`) and verify:
   - `overall_score >= 95.0`
   - `spec_compliance == true`
   - Zero horizontal clipping or off-screen truncation in Souped-Up Engine Drawer.
