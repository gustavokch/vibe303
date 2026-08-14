# GUI Visual Refinement Plan (`vibe303`)

**Date:** 2026-08-14  
**Target Spec:** `/Users/gus/Git/vibe303/docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md`  
**Overall Visual QA Weighted Score:** 68.10 / 100  
**Overall Spec Compliance:** Non-Compliant (`false`)

---

## 1. Executive Summary & Score Breakdown

Visual QA inspection was conducted across all three GUI sections against the design specification in `/Users/gus/Git/vibe303/docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md`. The overall weighted score is 68.10 / 100. All three sections failed visual spec compliance due to layout truncation, parameter placement errors, missing engineering unit formatting, and off-center overlay calculations.

| Section | Alignment Score | Weight | Weighted Score | Spec Compliance |
| :--- | :--- | :--- | :--- | :--- |
| **Classic 303 Front Panel** | 78.00 | 35% | 27.300 | `false` |
| **Souped-Up Drawer** | 45.00 | 30% | 13.500 | `false` |
| **Sequencer Console** | 78.00 | 35% | 27.300 | `false` |
| **Total / Overall** | **68.10** | **100%** | **68.100** | `false` |

---

## 2. Detailed Findings & Defect Analysis

### 2.1 Classic 303 Front Panel (`classic_panel`) — Non-Compliant (78.00)

1. **Knob Parameter Labels (`alignment` - Major):**
   - **Observed:** Parameter labels are positioned above the rotary knob dials in `classic_panel.rs` rendering.
   - **Expected:** Parameter labels (TUNING, CUTOFF, RESONANCE, ENV MOD, DECAY, ACCENT, VOLUME) should be positioned below each rotary dial matching hardware mockup `2026-07-02-ui-mockup.jpg`.
   - **Suggested Fix:** Reposition parameter label text below the rotary tick mark bezel in `Knob303` widget to align with vintage 303 hardware layout.

2. **Knob Value Readouts (`typography` - Major):**
   - **Observed:** Values display raw unformatted floating point numbers such as `0.24499997` (Cutoff), `0.2675803` (Resonance), and `0.12999988` (Accent).
   - **Expected:** Human-readable formatted values with engineering units (e.g. Cutoff in Hz/kHz, Resonance/Env Mod/Accent as %, Decay in ms/s, Volume in dB rounded to 1 decimal place).
   - **Suggested Fix:** Update value formatting in `Knob303::ui` to convert normalized values into formatted physical units with appropriate rounding and percentage scaling.

3. **Slide Switch Detent Labels (`geometry` - Minor):**
   - **Observed:** Option labels are rendered in a horizontal inline row below/beside the slider tracks.
   - **Expected:** Position labels (SAW/SQUARE/SWEEP and LOW/HIGH/ULTRA) stacked vertically adjacent to detent positions on the right of the switch track as shown in mockup.
   - **Suggested Fix:** Refactor `SlideSwitch` layout to support vertical label stacking on the right side of the slider track to match hardware detent alignment.

4. **Knob Radial Tick Marks (`geometry` - Minor):**
   - **Observed:** Radial tick marks are rendered close to bezel boundaries, causing visual crowding between top label and bottom readout.
   - **Expected:** 270-degree radial tick mark sweep (135° to 405°) with distinct major/minor tick strokes and adequate clearance from labels and readouts.
   - **Suggested Fix:** Adjust tick inner/outer radial offsets and widget padding in `Knob303` to maintain proper spacing around the bezel ring.

5. **Header Bar Branding & Layout (`alignment` - Minor):**
   - **Observed:** Header renders `TECHNEO • VIBE 303` on left and places `CIRCUIT TOPOLOGY` switch beside `TB-303 PLUGIN` text, crowding right section slightly.
   - **Expected:** Clean header bar layout with `TECHNEO` logo on left, dual 7-slot ventilation grilles in center, and `TB-303 PLUGIN` branding in orange (`#ff8800`) on right.
   - **Suggested Fix:** Adjust padding and font sizing in `header_bar.rs` to balance branding text, ventilation grills, and topology toggle across header width.

---

### 2.2 Souped-Up Engine Drawer (`souped_drawer`) — Non-Compliant (45.00)

1. **Souped-Up Engine Drawer Container (`geometry` - Critical):**
   - **Observed:** Souped-Up Engine Drawer extends past the right window boundary and is horizontally truncated, obscuring controls and faders on the right side.
   - **Expected:** Souped-Up Engine Drawer fits within the 1080px chassis width alongside the Classic 303 Panel without horizontal clipping or extending off-screen.
   - **Suggested Fix:** Reorganize Souped-Up Drawer interior layout into compact multi-row sections and enforce strict maximum width bounds so total panel width fits within the remaining window canvas.

2. **MEG & VEG ADSR Mini-Faders (`missing_feature` - Critical):**
   - **Observed:** MEG and VEG ADSR fader groups are completely pushed off-screen and clipped past the right window margin.
   - **Expected:** MEG ADSR (Attack, Decay, Sustain, Release) and VEG ADSR (Attack, Decay, Sustain, Release) mini-faders with orange glowing fill bars are fully visible in the drawer.
   - **Suggested Fix:** Wrap or position the MEG/VEG ADSR fader groups beneath or alongside the dial sections in a multi-row layout so they remain visible within the drawer bounds.

3. **Accurate Mode Overlay Banner & Switch Button (`alignment` - Major):**
   - **Observed:** The overlay banner center is calculated using the unconstrained child content rectangle center (`overlay_rect.center()`), causing the overlay badge and switch button to be positioned off-center and clipped off-screen to the right.
   - **Expected:** Accurate Mode Active banner overlay (*"ACCURATE MODE ACTIVE — SOUPED-UP ENGINE BYPASSED"* with *"SWITCH TO FUN TOPOLOGY"* button) is centered over the visible drawer area.
   - **Suggested Fix:** Calculate overlay banner position relative to the visible panel bounds (`ui.max_rect()`) rather than unbounded child content response rect.

4. **Oscillator, Drive & Filter Dial Rows (`geometry` - Major):**
   - **Observed:** 7 oscillator & drive dials are placed in a single unbroken horizontal line, consuming ~420px width and forcing filter switches and faders off-screen.
   - **Expected:** Extended dials (OSC1 DET, OSC2 DET, OSC2 MIX, OSC1 PW, SUB VOL, DRIVE, GLIDE, DRIFT, AGE, RESO COMP) are neatly grouped in structured, compact sub-panels.
   - **Suggested Fix:** Group extended dials into a 2-row grid layout (e.g. Row 1: Dual Osc controls; Row 2: Drive, Glide, Analog Wear) to optimize spatial efficiency.

---

### 2.3 16-Step Sequencer Console (`sequencer`) — Non-Compliant (78.00)

1. **16-Step Sequencer Strip (`geometry` - Critical):**
   - **Observed:** Only 14 step columns (01 through 14) are visible; steps 15 and 16 are truncated and cut off at the right edge of the plugin window.
   - **Expected:** All 16 step columns (01 through 16) fully visible and accessible within the console container bounds.
   - **Suggested Fix:** Reduce step column width (e.g., from 42.0 to 36.0-38.0px) and item spacing in `render_sequencer` / `render_step_strip` so all 16 step columns fit within the total 1080px window width.

2. **Transport & Controls Left Sidebar (`alignment` - Major):**
   - **Observed:** Left transport sidebar consumes 125px width plus margins and separators, crowding out the rightmost steps 15 and 16.
   - **Expected:** Left transport sidebar width optimized (~100px) so the 16-step grid receives adequate layout space.
   - **Suggested Fix:** Tighten left transport sidebar padding and set width to ~100px to free up horizontal space for the full 16-step grid.

---

## 3. Actionable Refinement Tasks by Target File

### 3.1 `/Users/gus/Git/vibe303/plugin303/src/gui/panels/classic_panel.rs` & `knob.rs`
- [ ] **Task 1 (Major - Alignment):** Reposition parameter label text below the rotary dial bezel in `Knob303`.
- [ ] **Task 2 (Major - Typography):** Implement formatted value readouts with physical engineering units (Hz/kHz, %, ms/s, dB) and proper decimal rounding in `Knob303`.
- [ ] **Task 3 (Minor - Geometry):** Refactor `SlideSwitch` layout to position detent option labels vertically on the right side of the slider track.
- [ ] **Task 4 (Minor - Geometry):** Adjust radial tick inner and outer offsets and widget padding in `Knob303` to eliminate visual crowding.

### 3.2 `/Users/gus/Git/vibe303/plugin303/src/gui/panels/header_bar.rs`
- [ ] **Task 5 (Minor - Alignment):** Rebalance spacing and text layout for TECHNEO logo, ventilation grills, and topology toggle in the header bar.

### 3.3 `/Users/gus/Git/vibe303/plugin303/src/gui/panels/souped_panel.rs`
- [ ] **Task 6 (Critical - Geometry):** Reorganize Souped-Up Engine Drawer interior into compact multi-row sub-panels (Row 1: Dual Osc controls; Row 2: Drive, Glide, Wear) so total drawer width stays within 1080px canvas bounds.
- [ ] **Task 7 (Critical - Missing Feature):** Relocate MEG and VEG ADSR mini-fader groups into visible multi-row bounds within the drawer canvas.
- [ ] **Task 8 (Major - Alignment):** Calculate Accurate Mode overlay banner center relative to visible canvas bounds (`ui.max_rect()`) instead of unconstrained content bounds.

### 3.4 `/Users/gus/Git/vibe303/plugin303/src/gui/panels/sequencer_panel.rs` & `step_button.rs`
- [ ] **Task 9 (Critical - Geometry):** Reduce sequencer step column width (to 36.0-38.0px) and item spacing so all 16 steps (01 through 16) are fully visible.
- [ ] **Task 10 (Major - Alignment):** Tighten left transport sidebar width to ~100px and adjust margins to grant sufficient width to the 16-step strip.

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
   - All 16 step columns, MEG/VEG faders, formatted knob readouts, and centered banners pass visual inspection.
