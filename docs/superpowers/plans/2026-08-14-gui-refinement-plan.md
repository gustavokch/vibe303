# GUI Visual Refinement Plan (`vibe303`)

**Date:** 2026-08-14  
**Target Spec:** `/Users/gus/Git/vibe303/docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md`  
**Overall Visual QA Weighted Score:** 93.90 / 100  
**Overall Spec Compliance:** Compliant (`true`)

---

## 1. Executive Summary & Score Breakdown

Visual QA inspection synthesis across three GUI sections against the design specification in `docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md`. The overall weighted score is 93.90 / 100. All three sections pass visual quality compliance (`true`).

| Section | Alignment Score | Weight | Weighted Score | Spec Compliance |
| :--- | :--- | :--- | :--- | :--- |
| **Classic 303 Front Panel** | 92.00 | 35% | 32.200 | `true` |
| **Souped-Up Engine Drawer** | 96.00 | 30% | 28.800 | `true` |
| **16-Step Sequencer Console** | 94.00 | 35% | 32.900 | `true` |
| **Total / Overall** | **93.90** | **100%** | **93.900** | `true` |

---

## 2. Detailed Findings & Defect Analysis

### 2.1 Classic 303 Front Panel (`classic_panel`) — Compliant (92.00)

1. **SlideSwitch Option Layout (`geometry` - Minor):**
   - **Observed:** Waveform and Fidelity Tier option labels (SAW, SQUARE, SWEEP / LOW, HIGH, ULTRA) are aligned horizontally below the slider track.
   - **Expected:** UI mockup (2026-07-02-ui-mockup.jpg) displays switch option labels stacked vertically to the right of the slider handle.
   - **Suggested Fix:** Optionally introduce a vertical label orientation option in SlideSwitch widget to match the mockup's right-aligned vertical stack if strict visual parity is desired.

2. **Knob Label Placement (`typography` - Minor):**
   - **Observed:** Parameter title label (e.g. TUNING, CUTOFF) is rendered directly below the dial, followed by the formatted value readout below the title.
   - **Expected:** Spec Section 3.1 mentions parameter label above dial and live readout below, whereas UI mockup shows labels below.
   - **Suggested Fix:** Maintain current layout (label below knob) as it matches the visual mockup, and update spec documentation to reflect label positioning below dial.

3. **Header Bar Branding & Topology Switch (`alignment` - Minor):**
   - **Observed:** Header bar accurately renders TECHNEO • VIBE 303 branding, dual 7-slot ventilation grills, TB-303 PLUGIN orange text, and top-right CIRCUIT TOPOLOGY switch.
   - **Expected:** Header bar renders TECHNEO branding, ventilation grills, TB-303 PLUGIN indicator, and Circuit Topology switch.
   - **Suggested Fix:** No fix needed; header bar matches visual layout and functional requirements cleanly.

---

### 2.2 Souped-Up Engine Drawer (`souped_drawer`) — Compliant (96.00)

1. **MEG and VEG ADSR Mini-Faders (`geometry` - Minor):**
   - **Observed:** MEG and VEG ADSR mini-fader groups sit side-by-side with 4.0px horizontal spacing.
   - **Expected:** MEG and VEG ADSR mini-fader groups are clearly separated into distinct 4-slider vertical card sections with glowing orange fill bars and group headers.
   - **Suggested Fix:** Maintain current group separation and glowing orange accent fills, increasing padding between MEG and VEG fader banks slightly for improved visual isolation.

---

### 2.3 16-Step Sequencer Console (`sequencer`) — Compliant (94.00)

1. **Playhead LED Indicator (`geometry` - Minor):**
   - **Observed:** 5px top playhead LED bar rendering slate in idle state and neon coral/red with glow halo when active.
   - **Expected:** 5px top playhead indicator bar with active glow overlay on currently playing step.
   - **Suggested Fix:** Keep existing top bar rendering with pulse animation on live playback.

2. **ACC / SLD Toggle Buttons (`alignment` - Minor):**
   - **Observed:** ACC and SLD buttons side-by-side (17x16px) with discrete LED indicator dots on right margin.
   - **Expected:** Dedicated ACC (amber LED dot) and SLD (cyan LED dot) modifier toggles per step column.
   - **Suggested Fix:** Ensure 2px horizontal spacing between modifier buttons is maintained across DPI scales.

3. **Gate Badge (NOTE / TIE / REST) (`color` - Minor):**
   - **Observed:** High-contrast gate state badge correctly cycling through Cyan Note, Gold Tie, and Slate Rest.
   - **Expected:** Cyclic gate badge showing Note (Cyan #00e5ff), Tie (Gold #ffd700), and Slate Rest (Slate #333a42).
   - **Suggested Fix:** No fix needed; gate colors strictly comply with spec §3.3 color tokens.

4. **Pitch Readout & Nudge (`typography` - Minor):**
   - **Observed:** Pitch readout rendered in bold 9pt monospace font, clicking opens piano roll popover, side-by-side up/down arrows nudge octave.
   - **Expected:** Monospace pitch display (e.g. C 3) with interactive octave nudge buttons.
   - **Suggested Fix:** Maintain monospace font clarity for pitch readouts across step columns.

---

## 3. Actionable Refinement Tasks by Target File

### 3.1 `/Users/gus/Git/vibe303/plugin303/src/gui/widgets/switch.rs`
- [ ] **Task 1 (Minor - Geometry):** Add optional vertical label orientation layout in `SlideSwitch` to align detent text to the right of slider track.

### 3.2 `/Users/gus/Git/vibe303/docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md`
- [ ] **Task 2 (Minor - Typography):** Update Section 3.1 spec text to reflect knob parameter label positioning below dial rather than above.

### 3.3 `/Users/gus/Git/vibe303/plugin303/src/gui/panels/souped_panel.rs`
- [ ] **Task 3 (Minor - Geometry):** Increase padding between MEG and VEG ADSR fader banks slightly to improve group separation.

### 3.4 `/Users/gus/Git/vibe303/plugin303/src/gui/widgets/step_button.rs`
- [ ] **Task 4 (Minor - Alignment):** Verify 2px horizontal spacing between ACC and SLD modifier buttons across DPI scales.

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
   - `overall_score >= 93.0`
   - `spec_compliance == true`
   - All 3 sections pass visual quality gate.
