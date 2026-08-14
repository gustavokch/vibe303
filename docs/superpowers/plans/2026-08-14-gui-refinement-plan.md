# GUI Visual Refinement Plan (`vibe303`)

**Date:** 2026-08-14  
**Target Spec:** `docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md`  
**Overall Visual QA Weighted Score:** 82.00 / 100  
**Overall Spec Compliance:** Non-Compliant (`false`)

---

## 1. Executive Summary & Score Breakdown

Visual QA inspection was conducted across all three GUI panels against the design specification `docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md`. All three panels (`classic_panel`, `souped_drawer`, and `sequencer`) are currently non-compliant. The overall weighted score is 82.00 / 100.

| Section | Alignment Score | Weight | Weighted Score | Spec Compliance |
| :--- | :--- | :--- | :--- | :--- |
| **Classic 303 Front Panel** | 82.00 | 35% | 28.700 | `false` |
| **Souped-Up Drawer** | 75.00 | 30% | 22.500 | `false` |
| **Sequencer Console** | 88.00 | 35% | 30.800 | `false` |
| **Total / Overall** | **82.00** | **100%** | **82.000** | `false` |

---

## 2. Detailed Findings & Defect Analysis

### 2.1 Classic 303 Front Panel (`classic_panel`) — Non-Compliant (82.00)

1. **Knob303 Value Readout Formatting (`typography` - Major):**
   - **Observed:** Dials display raw unformatted float strings with up to 8 decimal places (e.g. Cutoff `0.08413537`, Resonance `0.6013354`).
   - **Expected:** Formatted parameter values with appropriate units (e.g., Hz for Cutoff, % for Resonance/Env Mod/Accent, ms for Decay, cents for Tuning) per Section 3.1.
   - **Suggested Fix:** In `params.rs` parameter definitions or `Knob303` widget (`knob.rs`), implement human-readable string formatting callbacks or value mappers.

2. **Knob303 Radial Tick Marks (`color` - Minor):**
   - **Observed:** Radial tick marks along the 270-degree arc around dial bezels are faint with low contrast stroke and short line segments.
   - **Expected:** High-contrast vintage radial tick marks along 270° arc per Section 3.1.
   - **Suggested Fix:** Increase tick line segment length (e.g., `tick_out + 2.0px`), bump stroke width, and upgrade stroke color to high-contrast `TEXT_SECONDARY` token in `knob.rs`.

3. **Header Bar Logo & Branding (`typography` - Minor):**
   - **Observed:** Header branding text rendered inline with small font sizes (16pt / 11pt) without distinct metallic or glowing orange styling.
   - **Expected:** Prominent metallic `TECHNEO` logo on left and glowing orange `TB-303 PLUGIN` title on right per mockup and Section 1.
   - **Suggested Fix:** Update `header_bar.rs` layout, font weights, glow effects, and color tokens.

4. **Fidelity Tier Switch (Accurate Mode State) (`color` - Minor):**
   - **Observed:** When Accurate mode is active, disabled tier switch text contrast drops significantly while red thumb retains highlight.
   - **Expected:** Fidelity Tier slide switch clearly indicates bypassed state while maintaining legible option text (LOW, HIGH, ULTRA).
   - **Suggested Fix:** Refine disabled styling in `switch.rs` or `classic_panel.rs` wrapper so option text remains readable while thumb color dims appropriately.

---

### 2.2 Souped-Up Engine Drawer (`souped_drawer`) — Non-Compliant (75.00)

1. **Custom Egui GUI Window Canvas (`missing_feature` - Major):**
   - **Observed:** `target/gui-qa/latest.png` displays Bitwig Studio's generic DAW host parameter controls instead of bringing the custom egui window to the foreground.
   - **Expected:** Standalone/VST3 window displaying the custom vector-rendered Souped-Up Engine Drawer with dark industrial brushed steel chassis (`#16181b`), MEG/VEG ADSR faders, extended rotary dials, and topology dimming overlay per Section 1 & 4.1.
   - **Suggested Fix:** Update `capture_standalone_gui.sh` to focus/bring the standalone window to the foreground using window activation before calling `screencapture`.

2. **MEG/VEG MiniFader Readout Text Alignment (`alignment` - Minor):**
   - **Observed:** MiniFader width of 22.0px displays raw parameter float string truncated to 5 characters (e.g., `'94.49'`) when unformatted.
   - **Expected:** Fader value readout formatted consistently below track with explicit units (e.g. ms / %) without string truncation per Section 3.2.
   - **Suggested Fix:** Apply explicit integer/unit formatting (e.g., `format!("{:.0}ms", ms)`) in MiniFader value text rendering in `fader.rs`.

3. **Accurate Mode Dimming Overlay Banner Centering (`geometry` - Minor):**
   - **Observed:** Overlay implemented in `souped_panel.rs` using `painter.rect_filled` with `Color32::from_rgba_premultiplied(0x0e, 0x11, 0x16, 0x4d)` tint.
   - **Expected:** 30% opacity background tint (`0x4d` alpha) over `content_response.response.rect` with centered banner (380x86px) and switch button per Section 4.1.
   - **Suggested Fix:** Maintain current 30% alpha tint rect calculation and verify visual contrast when rendered on host display.

---

### 2.3 16-Step Sequencer Console (`sequencer`) — Non-Compliant (88.00)

1. **ACC / SLD Toggle Buttons (`alignment` - Minor):**
   - **Observed:** Label text `'ACC'`/`'SLD'` centered at left offset 7.0 visual center creates tight spacing near the 3px radius LED dot at right-4.0 in a 20px button frame.
   - **Expected:** Un-crowded label typography with side-by-side LED status dot inside button frame per Section 3.3.
   - **Suggested Fix:** Adjust text anchor x-position to left-align at 5.0 and place LED dot centered at right-3.5 for optimal spacing in `step_button.rs`.

2. **Pitch Readout Button (`typography` - Minor):**
   - **Observed:** Monospace 10.0pt note font in 18px button height has tight top/bottom margins.
   - **Expected:** Centered note string display badge with balanced vertical padding per Section 3.3.
   - **Suggested Fix:** Set font size to 9.5pt monospace and increase button height to 20px in `step_button.rs`.

3. **Step Column Spacing (`geometry` - Minor):**
   - **Observed:** Item spacing between step frames is set to 2.0px, resulting in dense horizontal packing.
   - **Expected:** Consistent column separation matching chassis grid rhythm per Section 3.3 & Section 5.
   - **Suggested Fix:** Increase step item spacing to 3.0px for clearer step column separation in `sequencer_panel.rs`.

---

## 3. Actionable Refinement Tasks by Target File

### 3.1 `plugin303/src/gui/widgets/knob.rs` & `plugin303/src/params.rs`
- [ ] **Task 1 (Major - Typography):** Implement human-readable formatted parameter value strings (units: Hz, %, ms, cents) for `Knob303` readouts instead of raw floating point numbers.
- [ ] **Task 2 (Minor - Color):** Lengthen tick mark lines (`tick_out + 2.0px`) and increase stroke contrast (`TEXT_SECONDARY`) on `Knob303` radial 270° dial sweeps.

### 3.2 `plugin303/src/gui/panels/header_bar.rs` & `plugin303/src/gui/panels/classic_panel.rs`
- [ ] **Task 3 (Minor - Typography):** Update `TECHNEO` logo styling and glowing orange `TB-303 PLUGIN` text in `header_bar.rs`.
- [ ] **Task 4 (Minor - Color):** Refine disabled option text contrast and thumb dimming for `Fidelity Tier` switch in Accurate mode in `switch.rs` / `classic_panel.rs`.

### 3.3 `plugin303/src/gui/widgets/fader.rs` & `capture_standalone_gui.sh`
- [ ] **Task 5 (Major - Missing Feature):** Update `capture_standalone_gui.sh` to focus/bring the standalone application window to the foreground before triggering screenshot capture.
- [ ] **Task 6 (Minor - Alignment):** Apply explicit integer and unit formatting (`format!("{:.0}ms", ms)`) to `MiniFader` value text in `fader.rs`.
- [ ] **Task 7 (Minor - Geometry):** Verify and confirm 30% alpha tint rect calculation and banner centering in `souped_panel.rs`.

### 3.4 `plugin303/src/gui/widgets/step_button.rs` & `plugin303/src/gui/panels/sequencer_panel.rs`
- [ ] **Task 8 (Minor - Alignment):** Left-align `'ACC'` / `'SLD'` text at offset 5.0 and center LED dot at right-3.5 in `step_button.rs`.
- [ ] **Task 9 (Minor - Typography):** Set pitch button font size to 9.5pt monospace and increase button height to 20px in `step_button.rs`.
- [ ] **Task 10 (Minor - Geometry):** Increase sequencer step item spacing from 2.0px to 3.0px in `sequencer_panel.rs`.

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
   - All panels pass visual inspection without text truncation, focus issues, or layout clipping.
