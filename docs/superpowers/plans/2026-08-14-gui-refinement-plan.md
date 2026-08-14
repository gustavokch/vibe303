# GUI Visual Refinement Plan (`vibe303`)

**Date:** 2026-08-14  
**Target Spec:** `docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md`  
**Overall Visual QA Weighted Score:** 91.83 / 100  
**Overall Spec Compliance:** Non-Compliant (`false`)

---

## 1. Executive Summary & Score Breakdown

Visual QA analysis was performed across the three GUI sections. While Classic 303 Front Panel and Souped-Up Engine Drawer pass spec compliance gates, the Sequencer Console is non-compliant due to typography and layout deviations.

| Section | Alignment Score | Weight | Weighted Score | Spec Compliance |
| :--- | :--- | :--- | :--- | :--- |
| **Classic 303 Front Panel** | 91.50 | 35% | 32.025 | `true` |
| **Souped-Up Drawer** | 92.00 | 30% | 27.600 | `true` |
| **Sequencer Console** | 92.00 | 35% | 32.200 | `false` |
| **Total / Overall** | **91.83** | **100%** | **91.825** | `false` |

---

## 2. Design Spec Comparison & Defect Analysis

### 2.1 Classic 303 Front Panel (`classic_panel`)
- **Header Bar Branding (`typography`):** Spec calls for TECHNEO logo on top-left and TB-303 PLUGIN branding in orange caps on top-right per UI mockup. Observed header currently shows `VIBE 303 / ANALOG CIRCUIT EMULATION`.
- **Knob Parameter Labels (`typography`):** Spec Section 3.1 requires parameter titles above rotary dials to be rendered in **bold uppercase** typography. Observed labels use standard-weight proportional font (`FontId 9.5`).

### 2.2 Souped-Up Engine Drawer (`souped_drawer`)
- **VEG ADSR MiniFaders Color (`color`):** Spec Section 3.2 & Section 5 define glowing orange fill bars (`#ff8800`) matching `theme::SOUPED_ORANGE`. Observed VEG ADSR faders use amber glow (`#ffaa00`) while MEG ADSR faders use orange (`#ff8800`).
- **Accurate Mode Overlay Tint (`geometry`):** Spec Section 4.1 specifies ghosted/dimmed extended controls at ~30% opacity with non-interactive overlay badge. Observed implementation uses a heavy 70% dark tint overlay rect (`0xb8` alpha) over `add_enabled_ui(false)`.

### 2.3 Sequencer Console (`sequencer`) — Non-Compliant
- **ACC / SLD Button Labels (`typography`):** Spec Section 3.3 calls for buttons labeled `ACC` (Accent) and `SLD` (Slide). Observed implementation displays single-letter abbreviations `A` and `S`.
- **Pitch Readout Formatting (`typography`):** Spec Section 3.3 specifies formatted note name with a space between note and octave (e.g., `C 3`, `F# 2`). Observed readout concatenates directly without space (e.g., `C3`, `F#2`).
- **Octave Nudge Control Buttons (`alignment`):** Spec Section 3.3 specifies full-width horizontal alignment spanning the 48px step column evenly. Observed nudge buttons (`▲` / `▼`) are 21px wide each with 2px spacing, leaving an unwanted gap on the 48px column.

---

## 3. Actionable Refinement Tasks by File

### 3.1 `plugin303/src/gui/panels/classic_panel.rs` & `plugin303/src/gui/widgets/knob.rs`
1. **Knob Parameter Label Weight** `[Severity: Minor, Category: typography]`
   - **Target File:** `plugin303/src/gui/widgets/knob.rs`
   - **Fix:** Update label painter text style in `Knob303` to use strong/bold font weight token (`egui::FontId::new(9.5, egui::FontFamily::Proportional)` with bold styling or `TextStyle::Name("Bold".into())`).

2. **Header Branding Strings** `[Severity: Minor, Category: typography]`
   - **Target File:** `plugin303/src/gui/panels/header_bar.rs` / `classic_panel.rs`
   - **Fix:** Update header branding text strings to `TECHNEO` / `TB-303 PLUGIN` if exact mockup parity is mandated, or document brand name configuration.

### 3.2 `plugin303/src/gui/panels/souped_panel.rs`
1. **Unify VEG ADSR Fader Glow Color** `[Severity: Minor, Category: color]`
   - **Target File:** `plugin303/src/gui/panels/souped_panel.rs`
   - **Fix:** Change VEG ADSR fader color token from amber (`#ffaa00`) to `theme::SOUPED_ORANGE` (`#ff8800`) to align with MEG faders and Souped-Up drawer color palette.

2. **Adjust Accurate Mode Overlay Tint** `[Severity: Minor, Category: geometry]`
   - **Target File:** `plugin303/src/gui/panels/souped_panel.rs`
   - **Fix:** Reduce background overlay rect tint alpha from `0xb8` (~70%) to `0x4d` (~30% opacity) so underlying ghosted controls remain clearly visible per Spec Section 4.1.

### 3.3 `plugin303/src/gui/widgets/step_button.rs`
1. **Update Accent & Slide Button Labels** `[Severity: Minor, Category: typography]`
   - **Target File:** `plugin303/src/gui/widgets/step_button.rs`
   - **Fix:** Update modifier button labels from `"A"` and `"S"` to `"ACC"` and `"SLD"`. Adjust font size slightly if necessary to fit the step button bounds cleanly.

2. **Format Pitch Readout with Space** `[Severity: Minor, Category: typography]`
   - **Target File:** `plugin303/src/gui/widgets/step_button.rs`
   - **Fix:** Update `format_pitch` function to return `format!("{} {}", name, oct_num)` instead of `format!("{}{}", name, oct_num)`.

3. **Expand Octave Nudge Buttons to 48px Width** `[Severity: Minor, Category: alignment]`
   - **Target File:** `plugin303/src/gui/widgets/step_button.rs`
   - **Fix:** Adjust octave button `min_size` to `vec2(23.0, 14.0)` with exact layout expansion, or use `ui.allocate_ui_with_layout` with full horizontal fill across the 48px step column width.

---

## 4. Verification & Quality Gates

1. **Compilation Check:**
   `cargo check -p plugin303 --all-features`
2. **Audio Path Allocation Gate:**
   `cargo test -p plugin303 --test no_alloc`
3. **Visual Quality Gate Target:**
   Re-run Visual QA harness to verify `overall_score >= 95.0` and `spec_compliance == true`.
