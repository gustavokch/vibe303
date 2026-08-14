# GUI Visual Refinement Plan (`vibe303`)

**Date:** 2026-08-14  
**Target Spec:** `docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md`  
**Overall Visual QA Weighted Score:** 83.75 / 100  
**Overall Spec Compliance:** Non-Compliant (`false`)

---

## 1. Executive Summary & Score Breakdown

Visual QA inspection was conducted across all three GUI panels against design spec `docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md`. The Souped-Up Engine Drawer passes spec compliance (96.00), while the Classic 303 Front Panel (82.00) and Sequencer Console (75.00) are currently non-compliant due to major layout clipping, unformatted raw float parameter readouts, and widget alignment/styling defects.

| Section | Alignment Score | Weight | Weighted Score | Spec Compliance |
| :--- | :--- | :--- | :--- | :--- |
| **Classic 303 Front Panel** | 82.00 | 35% | 28.700 | `false` |
| **Souped-Up Drawer** | 96.00 | 30% | 28.800 | `true` |
| **Sequencer Console** | 75.00 | 35% | 26.250 | `false` |
| **Total / Overall** | **83.75** | **100%** | **83.750** | `false` |

---

## 2. Detailed Findings & Defect Analysis

### 2.1 Classic 303 Front Panel (`classic_panel`) — Non-Compliant (82.00)

1. **Knob303 Value Readout Formatting (`typography` - Major):**
   - **Observed:** Dials display raw unformatted float strings with up to 8 decimal places (e.g. Cutoff `0.08413537`, Resonance `0.6013354`).
   - **Expected:** Formatted parameter values with appropriate units (e.g., Hz for Cutoff, % for Resonance/Env Mod/Accent, ms for Decay, cents for Tuning) per Section 3.1.
   - **Fix:** In `params.rs` parameter definitions or `Knob303` widget (`knob.rs`), implement human-readable string formatting callbacks or value mappers.

2. **Knob303 Radial Tick Marks (`color` - Minor):**
   - **Observed:** Radial tick marks along the 270-degree arc around dial bezels are faint with low contrast stroke and short line segments.
   - **Expected:** High-contrast vintage radial tick marks along 270° arc per Section 3.1.
   - **Fix:** Increase tick line segment length (e.g. `tick_out + 2.0px`), bump stroke width, and upgrade stroke color to high-contrast `TEXT_SECONDARY` token in `knob.rs`.

3. **Header Bar Logo & Branding (`typography` - Minor):**
   - **Observed:** Header branding text rendered inline with small font sizes (16pt / 11pt) without distinct metallic or glowing orange styling.
   - **Expected:** Prominent metallic `TECHNEO` logo on left and glowing orange `TB-303 PLUGIN` title on right per mockup and Section 1.
   - **Fix:** Update `header_bar.rs` layout, font weights, glow effects, and color tokens.

4. **Fidelity Tier Switch (Accurate Mode State) (`color` - Minor):**
   - **Observed:** When Accurate mode is active, disabled tier switch text contrast drops significantly while red thumb retains highlight.
   - **Expected:** Fidelity Tier slide switch clearly indicates bypassed state while maintaining legible option text (LOW, HIGH, ULTRA).
   - **Fix:** Refine disabled styling in `switch.rs` or `classic_panel.rs` wrapper so option text remains readable while thumb color dims appropriately.

---

### 2.2 Souped-Up Engine Drawer (`souped_drawer`) — Compliant (96.00)

1. **MiniFader Value Readout Clearance (`geometry` - Minor):**
   - **Observed:** Readout text positioned near bottom margin (`rect.bottom() - 3.0`).
   - **Expected:** Generous 6px bottom clearance beneath value readout for crisp fader alignment.
   - **Fix:** Adjust baseline calculation in `fader.rs` to `rect.bottom() - 4.0` or increase vertical padding.

2. **Accurate Mode Dimming Tint (`color` - Pass):**
   - **Observed:** Overlay uses 30.2% (`0x4d`) alpha dark tint with disabled container state, preserving ghosted control outlines while blocking interaction.
   - **Expected:** 30% opacity overlay tint with non-interactive ghosting overlay per Section 4.1.
   - **Fix:** Maintain current parameters.

---

### 2.3 16-Step Sequencer Console (`sequencer`) — Non-Compliant (75.00)

1. **Sequencer Step Strip Container Overflow (`geometry` - Major):**
   - **Observed:** Steps 15 and 16 are horizontally clipped past the right boundary of the window, rendering only steps 01 through 14.
   - **Expected:** All 16 step columns must be fully visible and accessible within the 1080px window width per Section 1 & 3.3.
   - **Fix:** Reduce individual step column width (e.g. from 48px to 42px) and adjust item spacing/margins in `step_button.rs` and `sequencer_panel.rs`.

2. **Souped-Up Engine Drawer Panel Clipping (`alignment` - Major):**
   - **Observed:** Right panel is clipped horizontally at the right edge of the window, truncating knobs and parameter labels.
   - **Expected:** Right-hand Souped-Up Engine Drawer panel fits completely inside the main window frame.
   - **Fix:** Rebalance top horizontal grid layout width distribution between Classic panel and Souped drawer in `gui/mod.rs` and `souped_panel.rs` to fit master 1080px canvas.

3. **Octave Up/Down Nudge Buttons (`alignment` - Minor):**
   - **Observed:** Octave buttons render small square outlines resembling empty boxes (`□ □`) rather than distinct arrow chevrons.
   - **Expected:** Centered, distinct up and down arrow chevron icons (`▲` / `▼`) for octave shifting per Section 3.3.
   - **Fix:** Adjust painter line stroke coordinates or use crisp rich text chevron symbols in `step_button.rs`.

4. **Gate Badges & ACC/SLD LEDs (`color` - Pass):**
   - **Observed:** Gate colors (Cyan NOTE / Gold TIE / Slate REST) and LED indicators (Amber ACC / Cyan SLD) match specification perfectly.
   - **Fix:** Maintain existing color token definitions and LED rendering logic.

---

## 3. Actionable Refinement Tasks by Target File

### 3.1 `plugin303/src/gui/widgets/knob.rs` & `plugin303/src/params.rs`
- [ ] **Task 1 (Major - Typography):** Implement human-readable formatted parameter value strings (units: Hz, %, ms, cents) for `Knob303` readouts instead of raw floating point numbers.
- [ ] **Task 2 (Minor - Color):** Lengthen tick mark lines (`tick_out + 2.0px`) and increase stroke contrast (`TEXT_SECONDARY`) on `Knob303` radial 270° dial sweeps.

### 3.2 `plugin303/src/gui/panels/header_bar.rs` & `plugin303/src/gui/panels/classic_panel.rs`
- [ ] **Task 3 (Minor - Typography):** Update `TECHNEO` logo styling and glowing orange `TB-303 PLUGIN` text in header bar.
- [ ] **Task 4 (Minor - Color):** Refine disabled option text contrast and thumb dimming for `Fidelity Tier` switch in Accurate mode.

### 3.3 `plugin303/src/gui/panels/sequencer_panel.rs`, `plugin303/src/gui/widgets/step_button.rs`, & `plugin303/src/gui/mod.rs`
- [ ] **Task 5 (Major - Geometry):** Reduce step strip width from 48px to 42px and item spacing to 2px so all 16 sequencer step columns fit completely inside 1080px width without horizontal clipping.
- [ ] **Task 6 (Major - Alignment):** Adjust main horizontal panel container width bounds in `gui/mod.rs` and `souped_panel.rs` to ensure Souped drawer fits cleanly within window borders.
- [ ] **Task 7 (Minor - Alignment):** Fix Octave Up/Down nudge button chevron drawing in `step_button.rs` so crisp `▲` and `▼` directional icons render without box artifacts.

### 3.4 `plugin303/src/gui/widgets/fader.rs`
- [ ] **Task 8 (Minor - Geometry):** Increase bottom clearance beneath `MiniFader` value readouts from 3px to 4-6px.

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
   Re-run screenshot inspection harness and verify:
   - `overall_score >= 95.0`
   - `spec_compliance == true`
   - All 16 sequencer steps visible and legible within 1080px window frame.
