# GUI Visual Refinement Plan (`vibe303`)

**Date:** 2026-08-14  
**Target Spec:** `/Users/gus/Git/vibe303/docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md`  
**Overall Visual QA Weighted Score:** 77.55 / 100  
**Overall Spec Compliance:** Non-Compliant (`false`)

---

## 1. Executive Summary & Score Breakdown

Visual QA inspection was conducted across all three GUI sections against the design specification in `/Users/gus/Git/vibe303/docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md`. The overall weighted score is 77.55 / 100. `souped_drawer` failed visual spec compliance due to a critical layout collapse bug, rendering the overall GUI non-compliant.

| Section | Alignment Score | Weight | Weighted Score | Spec Compliance |
| :--- | :--- | :--- | :--- | :--- |
| **Classic 303 Front Panel** | 88.00 | 35% | 30.800 | `true` |
| **Souped-Up Drawer** | 45.00 | 30% | 13.500 | `false` |
| **Sequencer Console** | 95.00 | 35% | 33.250 | `true` |
| **Total / Overall** | **77.55** | **100%** | **77.550** | `false` |

---

## 2. Detailed Findings & Defect Analysis

### 2.1 Classic 303 Front Panel (`classic_panel`) — Compliant (88.00)

1. **Knob303 Parameter Labels and Value Readout (`typography` - Minor):**
   - **Observed:** `Knob303` in `/Users/gus/Git/vibe303/plugin303/src/gui/widgets/knob.rs` renders parameter labels above knobs and live formatted numeric values below knobs.
   - **Expected:** Design mockup `ui-mockup.jpg` positions parameter labels directly below each knob without numeric value readouts.
   - **Suggested Fix:** Adjust label positioning and optional value readout rendering to allow labels below knobs when matching classic hardware mockup layout.

2. **Header Bar Ventilation Grill (`geometry` - Minor):**
   - **Observed:** `render_header` in `/Users/gus/Git/vibe303/plugin303/src/gui/panels/header_bar.rs` renders a single continuous 18-slot grill rect (120px width).
   - **Expected:** Design mockup `ui-mockup.jpg` features two distinct 7-slot ventilation grills separated by middle metallic spacing.
   - **Suggested Fix:** Split grill slot rendering into two separate 7-slot groups with spacing between them.

3. **Header Bar Branding and Topology Switch (`alignment` - Minor):**
   - **Observed:** `render_header` in `/Users/gus/Git/vibe303/plugin303/src/gui/panels/header_bar.rs` groups `TECHNEO` and `TB-303 PLUGIN` together on top-left, and places `CIRCUIT TOPOLOGY` switch on top-right.
   - **Expected:** Design mockup `ui-mockup.jpg` places `TECHNEO` on top-left and `TB-303 PLUGIN` branding on top-right.
   - **Suggested Fix:** Reposition `TB-303 PLUGIN` text to the right side of the ventilation grills to preserve mockup header balance.

4. **SlideSwitch Option Text Alignment (`geometry` - Minor):**
   - **Observed:** `SlideSwitch` in `/Users/gus/Git/vibe303/plugin303/src/gui/widgets/switch.rs` places option text horizontally beneath track detents.
   - **Expected:** Design mockup `ui-mockup.jpg` stacks option text vertically to the right of the sliding handle tracks for `WAVEFORM` and `FIDELITY TIER`.
   - **Suggested Fix:** Implement vertical option text layout option in `SlideSwitch` for side-stacked detent labels.

---

### 2.2 Souped-Up Engine Drawer (`souped_drawer`) — Non-Compliant (45.00)

1. **Souped-Up Drawer Controls Layout (`missing_feature` - Critical):**
   - **Observed:** Drawer renders as empty dark panel area with extended controls invisible or collapsed.
   - **Expected:** Drawer displays 10 extended rotary dials (OSC DET, MIX, PW, DRIVE, GLIDE, DRIFT, AGE, RESO COMP), 2 slide switches (VCF TAPS, TOLERANCE), and 8 vertical ADSR mini-faders (MEG/VEG).
   - **Suggested Fix:** Adjust width constraints in `/Users/gus/Git/vibe303/plugin303/src/gui/panels/souped_panel.rs` and ensure child widgets allocate explicit layout space within horizontal containers.

2. **Accurate-Mode Dimming Badge (`color` - Major):**
   - **Observed:** Overlay dark tint renders over empty panel without ghosted background controls visible.
   - **Expected:** Ghosted 30% opacity tint with centered overlay banner `ACCURATE MODE ACTIVE — SOUPED-UP ENGINE BYPASSED` and topology switch button over rendered controls.
   - **Suggested Fix:** Ensure underlying control widgets render before calculating `overlay_rect` in `/Users/gus/Git/vibe303/plugin303/src/gui/panels/souped_panel.rs`.

3. **Drawer Header Title (`alignment` - Minor):**
   - **Observed:** Header text `SOUPED-UP ENGINE DRAWER (Bypassed...` is truncated at right panel boundary.
   - **Expected:** Header text `SOUPED-UP ENGINE DRAWER (Bypassed in Accurate Mode)` fits fully within panel bounds.
   - **Suggested Fix:** Reduce font size or wrap title text inside flexible layout container in `/Users/gus/Git/vibe303/plugin303/src/gui/panels/souped_panel.rs`.

4. **MiniFader Value Readout (`typography` - Minor):**
   - **Observed:** Readout text is constrained near fader track bottom at height 45.0px.
   - **Expected:** Clean value readout with ms/s/% units positioned with 8px margin below fader track.
   - **Suggested Fix:** Increase vertical padding and bottom text offset in `/Users/gus/Git/vibe303/plugin303/src/gui/widgets/fader.rs`.

---

### 2.3 16-Step Sequencer Console (`sequencer`) — Compliant (95.00)

1. **ACC/SLD Button Labels (`typography` - Minor):**
   - **Observed:** Font size set to 6.5px proportional font, which can be small on non-scaled standard DPI displays.
   - **Expected:** High legibility text on small button targets across HiDPI scales.
   - **Suggested Fix:** Increase font size from 6.5px to 7.5px with tight tracking or adjust button padding in `/Users/gus/Git/vibe303/plugin303/src/gui/widgets/step_button.rs`.

2. **16-Step Column Horizontal Grid (`geometry` - Minor):**
   - **Observed:** Step columns fit flushly in continuous 16-step horizontal row with 3.0px spacing.
   - **Expected:** 42px column width with 3px inter-step spacing matching total container width.
   - **Suggested Fix:** Maintain explicit min/max width constraints on step cards to prevent flex stretching on wider windows in `/Users/gus/Git/vibe303/plugin303/src/gui/widgets/step_button.rs` and `/Users/gus/Git/vibe303/plugin303/src/gui/panels/sequencer_panel.rs`.

---

## 3. Actionable Refinement Tasks by Target File

### 3.1 `/Users/gus/Git/vibe303/plugin303/src/gui/panels/souped_panel.rs` & `/Users/gus/Git/vibe303/plugin303/src/gui/widgets/fader.rs`
- [ ] **Task 1 (Critical - Layout):** Fix drawer layout collapse by adjusting width constraints and explicit widget layout allocations in `souped_panel.rs`.
- [ ] **Task 2 (Major - Color):** Ensure underlying control widgets render before calculating and drawing `overlay_rect` in `souped_panel.rs`.
- [ ] **Task 3 (Minor - Alignment):** Reduce header text font size or wrap title text inside container in `souped_panel.rs`.
- [ ] **Task 4 (Minor - Typography):** Increase vertical padding and bottom text offset in `fader.rs` for `MiniFader` value readouts.

### 3.2 `/Users/gus/Git/vibe303/plugin303/src/gui/widgets/knob.rs`, `switch.rs`, & `/Users/gus/Git/vibe303/plugin303/src/gui/panels/header_bar.rs`
- [ ] **Task 5 (Minor - Typography):** Support label placement below `Knob303` without live value readout to match hardware mockup layout in `knob.rs`.
- [ ] **Task 6 (Minor - Geometry):** Split header ventilation grill rendering into two separate 7-slot groups in `header_bar.rs`.
- [ ] **Task 7 (Minor - Alignment):** Reposition `TB-303 PLUGIN` branding to top-right of ventilation grills in `header_bar.rs`.
- [ ] **Task 8 (Minor - Geometry):** Add vertical option text layout support to `SlideSwitch` in `switch.rs`.

### 3.3 `/Users/gus/Git/vibe303/plugin303/src/gui/widgets/step_button.rs` & `/Users/gus/Git/vibe303/plugin303/src/gui/panels/sequencer_panel.rs`
- [ ] **Task 9 (Minor - Typography):** Increase `ACC` and `SLD` button label font size to 7.5px in `step_button.rs`.
- [ ] **Task 10 (Minor - Geometry):** Set explicit 42px min/max width constraints on step cards in `sequencer_panel.rs` / `step_button.rs`.

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
   - All panels pass visual inspection without text truncation, layout collapse, or alignment defects.
