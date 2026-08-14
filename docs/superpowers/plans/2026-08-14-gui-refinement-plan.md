# GUI Visual Refinement Plan (`vibe303`)

**Date:** 2026-08-14  
**Target Spec:** `/Users/gus/Git/vibe303/docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md`  
**Overall Visual QA Weighted Score:** 88.30 / 100  
**Overall Spec Compliance:** Non-Compliant (`false`)

---

## 1. Executive Summary & Score Breakdown

Visual QA inspection synthesis across three GUI sections against the design specification in `docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md`. The overall weighted score is 88.30 / 100. The Classic 303 Front Panel section failed visual spec compliance (`false`) due to parameter label positioning and switch detent label layout, while Souped-Up Engine Drawer and 16-Step Sequencer Console passed spec compliance (`true`).

| Section | Alignment Score | Weight | Weighted Score | Spec Compliance |
| :--- | :--- | :--- | :--- | :--- |
| **Classic 303 Front Panel** | 75.00 | 35% | 26.250 | `false` |
| **Souped-Up Engine Drawer** | 96.00 | 30% | 28.800 | `true` |
| **16-Step Sequencer Console** | 95.00 | 35% | 33.250 | `true` |
| **Total / Overall** | **88.30** | **100%** | **88.300** | `false` |

---

## 2. Detailed Findings & Defect Analysis

### 2.1 Classic 303 Front Panel (`classic_panel`) — Non-Compliant (75.00)

1. **Knob Parameter Title Label Position (`alignment` - Major):**
   - **Observed:** Parameter title labels are rendered above the knobs in `Knob303` widget (`knob.rs`).
   - **Expected:** Parameter titles (TUNING, CUTOFF, RESONANCE, ENV MOD, DECAY, ACCENT, VOLUME) positioned below each rotary dial per design mockup.
   - **Suggested Fix:** Update `Knob303` layout in `knob.rs` to place parameter title label below the rotary dial bezel ring and radial tick marks.

2. **Slide Switch Option Detent Labels (`alignment` - Major):**
   - **Observed:** Option text rendered horizontally inline below track detents in `switch.rs`.
   - **Expected:** Position labels (SAW / SQUARE / SWEEP and LOW / HIGH / ULTRA) arranged vertically adjacent to slider track per design mockup.
   - **Suggested Fix:** Refactor `SlideSwitch` widget layout to support vertical option label stacking to the right of the slider track.

3. **Fidelity Tier Slide Switch Thumb Cap (`color` - Minor):**
   - **Observed:** Fidelity Tier thumb uses flat red stroke/fill without red track housing accent.
   - **Expected:** Distinctive red anodized knurled slider cap matching design mockup (`2026-07-02-ui-mockup.jpg`).
   - **Suggested Fix:** Enhance `SlideSwitch` styling for Fidelity Tier with high-contrast red anodized thumb texture and dark recessed housing.

4. **Header Bar Branding & Ventilation Grill Hierarchy (`missing_feature` - Minor):**
   - **Observed:** Header ventilation grill slots rendered, but 'TECHNEO • VIBE 303' sub-label text hierarchy and grill line stroke contrast differ slightly from mockup.
   - **Expected:** Header bar features prominent orange 'TB-303 PLUGIN' title on top right and dual 7-slot chassis ventilation grills matching brushed metallic chassis aesthetic.
   - **Suggested Fix:** Adjust `HeaderBar` typography hierarchy and grill line stroke contrast to align with brushed metallic chassis aesthetic.

5. **Knob303 Radial Tick Mark Bounds (`geometry` - Minor):**
   - **Observed:** Tick marks extend beyond knob bezel with non-standard vertical offset relative to parameter readout text.
   - **Expected:** Radial tick marks spanning 270 degrees centered around knob disc with uniform 15.0px radius.
   - **Suggested Fix:** Standardize `Knob303` bounding box and align center point for radial tick mark rendering.

---

### 2.2 Souped-Up Engine Drawer (`souped_drawer`) — Compliant (96.00)

1. **ADSR Mini-Fader Spacing (`geometry` - Minor):**
   - **Observed:** MEG and VEG fader groups sit side-by-side with 4.0px group spacing and orange headers.
   - **Expected:** MEG and VEG ADSR mini-faders are clearly separated into distinct 4-slider groups with glowing orange fill bars.
   - **Suggested Fix:** Maintain current group separation and orange accent fills for clear visual hierarchy.

---

### 2.3 16-Step Sequencer Console (`sequencer`) — Compliant (95.00)

1. **ACC / SLD Modifier Labels (`typography` - Minor):**
   - **Observed:** Font size for ACC and SLD button labels is set to 6.0pt.
   - **Expected:** High legibility across standard and HiDPI displays.
   - **Suggested Fix:** Increase font size from 6.0pt to 7.0pt or adjust label padding.

2. **Step Column Item Spacing (`alignment` - Minor):**
   - **Observed:** Step columns use 3.0px horizontal spacing to fit 16 steps in 1040px min-width.
   - **Expected:** Even layout distribution across full 1080px window width.
   - **Suggested Fix:** Fine-tune container padding to maximize visual spacing between step columns.

---

## 3. Actionable Refinement Tasks by Target File

### 3.1 `/Users/gus/Git/vibe303/plugin303/src/gui/widgets/knob.rs`
- [ ] **Task 1 (Major - Alignment):** Move parameter title label below the rotary dial bezel ring and radial tick marks in `Knob303`.
- [ ] **Task 2 (Minor - Geometry):** Standardize `Knob303` bounding box and align center point for radial tick mark rendering with uniform 15.0px radius.

### 3.2 `/Users/gus/Git/vibe303/plugin303/src/gui/widgets/switch.rs`
- [ ] **Task 3 (Major - Alignment):** Refactor `SlideSwitch` layout to stack option detent labels vertically to the right of the slider track.
- [ ] **Task 4 (Minor - Color):** Add high-contrast red anodized thumb texture and dark recessed track housing styling for Fidelity Tier slider switch.

### 3.3 `/Users/gus/Git/vibe303/plugin303/src/gui/panels/header_bar.rs`
- [ ] **Task 5 (Minor - Missing Feature):** Adjust `HeaderBar` typography hierarchy and ventilation grill stroke contrast to match brushed metallic aesthetic.

### 3.4 `/Users/gus/Git/vibe303/plugin303/src/gui/panels/souped_panel.rs` & `fader.rs`
- [ ] **Task 6 (Minor - Geometry):** Maintain clear group separation and orange accent fills on MEG and VEG ADSR mini-fader groups.

### 3.5 `/Users/gus/Git/vibe303/plugin303/src/gui/widgets/step_button.rs` & `sequencer_panel.rs`
- [ ] **Task 7 (Minor - Typography):** Increase ACC and SLD button label font size from 6.0pt to 7.0pt or adjust padding for HiDPI legibility.
- [ ] **Task 8 (Minor - Alignment):** Fine-tune container padding in `sequencer_panel.rs` to maximize visual spacing between step columns across full 1080px width.

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
   - All 3 sections pass visual quality gate.
