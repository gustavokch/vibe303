# Lead Visual QA Synthesis & Remediation Plan: TECHNEO 303 GUI

**Document ID**: 2026-08-22-gui-refinement-plan  
**Date**: 2026-08-22  
**Target Architecture**: `plugin303/src/gui/` (1280x720 Fixed Canvas)  

---

## Section 1: Executive Scorecard

### 1.1 Deductive Scoring Formula
- Formula: $\text{Calculated Score} = \max(0.0, 100.0 - \text{Deductions})$
- Arithmetic:
  - Critical Severity ($N_{\text{critical}} = 2 \times 25\text{ pts}$): $-50.0$
  - Major Severity ($N_{\text{major}} = 4 \times 10\text{ pts}$): $-40.0$
  - Minor Severity ($N_{\text{minor}} = 10 \times 3\text{ pts}$): $-30.0$
  - Total Deductions: $50.0 + 40.0 + 30.0 = 120.0$
  - Calculated Raw Score: $\max(0.0, 100.0 - 120.0) = 0.0$

### 1.2 Quality Gate Enforcements
- $N_{\text{critical}} \ge 1$: Spec Compliance = **FAILED (FALSE)**. Score bounded by $\min(\text{Calculated Score}, 69.0) = 0.0$.
- $N_{\text{major}} \ge 2$: Spec Compliance = **FAILED (FALSE)**. Score bounded by $\min(\text{Calculated Score}, 79.0) = 0.0$.
- Threshold Requirement for Passing: $N_{\text{critical}} = 0 \land N_{\text{major}} = 0 \land \text{Score} \ge 95.0$.

| Metric | Measured Value | Spec Requirement | Status |
| :--- | :--- | :--- | :--- |
| **Overall Visual Score** | **0.0 / 100.0** | $\ge 95.0$ | **FAILED** |
| **Critical Defects ($N_{\text{critical}}$)** | **2** | 0 | **FAIL** |
| **Major Defects ($N_{\text{major}}$)** | **4** | 0 | **FAIL** |
| **Minor Defects ($N_{\text{minor}}$)** | **10** | Allowable if Score $\ge 95.0$ | **OBSERVED** |
| **Spec Compliance** | **FALSE** | TRUE | **NON-COMPLIANT** |

---

## Section 2: Consolidated Defect Matrix

| ID | Element | Section | Severity | Category | Observed Defect | Expected State | Suggested Fix |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **DEF-01** | Window Canvas / Bottom Margin Area | state_diff | **critical** | layout_collapse | Accurate mode collapses vertical spacing by 108 pixels, which creates a dead black band of 146 pixels (~10% of window height) at the bottom. | Accurate mode maintains identical vertical dimensions and bottom margins as FUN mode without dead bands. | Lock fixed panel heights across topology modes to prevent vertical layout shifts and bottom voids. |
| **DEF-02** | Souped-Up Engine & Analog Drawer | state_diff | **critical** | layout_collapse | Drawer vertically squishes from 156pt (312px) to 102pt (204px) because rotary controls collapse inside disabled layout container. | Drawer maintains full allocated height and stable child control positions when topology switches to Accurate mode. | Set fixed vertical height of 156.0pt on souped panel frame instead of collapsible min-height. |
| **DEF-03** | Sequencer Step Octave Buttons & Transport Row D | sequencer | **major** | containment_overlap | Bottom octave nudge buttons (▲/▼) and Transport bottom row (ComboBoxes, JIT, CLEAR) press directly against the bottom frame stroke with less than 2px clearance. | Uniform vertical padding around all matrix elements with at least 6px margin to the bottom frame border. | Increase bottom inner_margin on the sequencer panel Frame to 6px or reduce step strip vertical spacing from 1.5px to 1.0px to restore bottom breathing room. |
| **DEF-04** | Accurate Mode Overlay Box | state_diff | **major** | containment_overlap | Overlay box floats awkwardly immediately above Output Clipper due to compressed drawer boundary. | Overlay box centers across the complete drawer frame with balanced padding above central controls. | Anchor overlay box to full static drawer frame coordinates to ensure visual balance. |
| **DEF-05** | Disabled Sub-Panels and Controls | state_diff | **major** | color_inconsistency | Disabled controls ghost through unevenly: drawer controls are hidden under heavy dark fill, Output Clipper stays fully saturated, and Tweaks panel mixes bright labels with dimmed knobs. | All bypassed features in Accurate mode display uniform disabled dimming and visual feedback. | Apply unified disabled palette and consistent opacity styling across all bypassed drawer sections in Accurate mode. |
| **DEF-06** | Volume rotary knob | affordance_semantics | **major** | pointer_mismatch | The Volume dial needle points at the 4:30 position (~94% travel) and fills the arc almost completely for +0.0 dB unity gain, while other default centered controls point straight up at the 12:00 position. | The Volume dial needle must point at the 12:00 position for +0.0 dB default unity gain, or use a center-zero reference arc. | Align the normalized parameter scale or adjust knob angle mapping so that the default +0.0 dB value positions the needle at the 12:00 center position. |
| **DEF-07** | Quality Selector Active Pill | header_bar | **minor** | color_inconsistency | Quality selector uses default ACCENT_ORANGE border and highlight for the active 'LOW' detent pill. | Quality (Fidelity Tier) selector active state color-coded with red tier accent as specified in design notes. | Configure SlideSwitch with .with_thumb_color(theme::ACCENT_RED) or tier-specific color token to distinguish fidelity tier from topology selection. |
| **DEF-08** | Knob303 (Tuning Dial) | classic_panel | **minor** | pointer_mismatch | Tuning knob draws a unipolar active value arc from the 7 o'clock minimum position (135 degrees) to the 12 o'clock center position at +0 ct default. | Bipolar parameters such as Tuning should draw the active value arc from the 12 o'clock center detent (270 degrees) toward the current value. | Add an optional is_bipolar flag to Knob303. When enabled, calculate the arc from center_angle (270.0 degrees) to current_angle instead of starting from start_angle. |
| **DEF-09** | Souped Drawer Section Crop Header Bounds | souped_drawer | **minor** | clipping_overflow | The crop script extracts crop_souped starting at Y=240, which clips the upper edge of the 'SOUPED-UP ENGINE & ANALOG DRAWER' panel title in crop_souped.png, even though the full canvas renders it correctly. | The discrete QA crop crop_souped.png must fully include the drawer header label 'SOUPED-UP ENGINE & ANALOG DRAWER' without vertical cropping. | Update scripts/crop_gui_sections.py to adjust the crop_souped start coordinate from Y=240 to Y=230. |
| **DEF-10** | Playhead LED Bar | sequencer | **minor** | branding_affordance | A 4px rectangular bar sits above each step number pill. In the idle state, the bar shows an unlabelled dark outline that can be confused with an accent dash. During playback, both the top bar and the step number pill light up simultaneously. | Playhead indicator must provide unambiguous visual feedback without redundant or confusing unlabelled status bars. | Add explicit hover tooltip to the LED bar or consolidate playhead illumination directly into the step number header pill. |
| **DEF-11** | Accent (ACC) and Slide (SLD) Toggle Buttons | sequencer | **minor** | branding_affordance | ACC and SLD render as full-fill backlit rectangular buttons (solid orange and cyan fills when active) rather than buttons with discrete LED dots. | Spec §3.3.2 and §3.3.3 specify buttons with discrete amber/cyan LED indicator dots. | Document the full-fill backlit button style as an accepted design refinement for 56px strip clarity, or add a discrete 3px circular LED indicator inside the button bounds. |
| **DEF-12** | Clipper diode selector vs engine selectors | affordance_semantics | **minor** | color_inconsistency | The Output Clipper DIODE selector uses ACCENT_RUBY for the active SI state, while the OS selector in the same panel and other engine selectors use ACCENT_ORANGE. | All state selectors across panels must use consistent accent colors for active selection states. | Standardize the active selection highlight color across all selector controls or make the panel accent scheme uniform. |
| **DEF-13** | Numeric formatting across generator and sequencer controls | affordance_semantics | **minor** | formatting_units | Inconsistent integer padding and decimal precision exist across panels. Sequencer LEN uses two-digit format ('16'), while Generator PULSES (k) and SHIFT use single-digit format ('5', '0'). BPM shows one decimal place ('120.0') while knob tempo displays integer format. | Numeric displays within similar control categories must follow uniform zero-padding and precision rules. | Apply consistent digit formatting across numerical inputs in the generator, transport, and knob badges. |
| **DEF-14** | Knob fallback parameter value formatter | affordance_semantics | **minor** | formatting_units | The knob widget value formatter converts unmatched ohm parameters into raw electrical resistance values (e.g., 47.0kΩ) instead of perceptual percentages or acoustic units. | All synth parameters must show user-facing perceptual quantities (percentage, decibels, cents, or time) instead of raw circuit component resistance. | Replace raw electrical resistance formatting with normalized percentage or decibel conversion in Knob303. |
| **DEF-15** | Diode model unselected button state | affordance_semantics | **minor** | branding_affordance | The unselected GE diode button uses TEXT_MUTED with a low-contrast border, which resembles a disabled control rather than an available toggle option. | Unselected interactive options must clearly indicate available affordance through distinct secondary text and visible border styling. | Use TEXT_SECONDARY and standard border contrast for unselected button states to maintain clear click affordance. |
| **DEF-16** | Clipper panel parameter accent inconsistency | affordance_semantics | **minor** | color_inconsistency | In the OUTPUT CLIPPER panel, the OS selector uses ACCENT_ORANGE for the active 2X pill while all surrounding knobs, title, and bypass toggle use ACCENT_RUBY. | All sub-controls within the dedicated Output Clipper panel must share the panel ruby accent color. | Pass ACCENT_RUBY to SlideSwitch in the Output Clipper panel to maintain color harmony. |

---

## Section 3: Root Cause Source Mapping

```
plugin303/src/gui/
├── mod.rs
│   └── Lines 34-88: Central layout container without fixed vertical reservation across topology states (DEF-01).
├── panels/
│   ├── header_bar.rs
│   │   └── Lines 117-122: SlideSwitch for quality tier lacks .with_thumb_color() (DEF-07).
│   ├── souped_panel.rs
│   │   ├── Lines 18-21: Frame uses ui.set_min_height(86.0) collapsing in Accurate mode (DEF-01, DEF-02).
│   │   ├── Lines 96-117: Overlay box positioned on squished bounding rect instead of static container (DEF-04).
│   │   └── Lines 23-35: Disabled control styling uses hardcoded TEXT_MUTED without dimming background fills (DEF-05).
│   ├── output_clipper.rs
│   │   ├── Line 50: SlideSwitch for OS selector instantiated without ruby accent color (DEF-12, DEF-16).
│   │   ├── Lines 136-151: Diode toggle unselected state uses TEXT_MUTED and BORDER_SUBTLE (DEF-15).
│   │   └── Lines 11-49: Frame remains undimmed during Accurate mode topology bypass (DEF-05).
│   ├── tweaks_panel.rs
│   │   └── Lines 17-28: Frame mixes bright secondary text with dimmed controls when bypassed (DEF-05).
│   ├── sequencer_panel.rs
│   │   ├── Line 30: Sequencer panel Frame inner_margin(8, 4) compresses bottom boundary (DEF-03).
│   │   ├── Lines 61-62: Vertical spacing in Transport column presses controls against border (DEF-03).
│   │   └── Line 230: LEN format uses {:02} while Generator numeric displays use unpadded formats (DEF-13).
│   └── generator_panel.rs
│       └── Lines 164-189: PULSES (k) and SHIFT DragValue widgets omit custom zero-padded formatters (DEF-13).
├── widgets/
│   ├── knob.rs
│   │   ├── Lines 88-125: Value arc starts unconditionally from 135 deg (unipolar only) (DEF-06, DEF-08).
│   │   └── Lines 178-182: Ohm fallback formats raw resistance (kΩ) rather than perceptual units (DEF-14).
│   └── step_button.rs
│       ├── Lines 32-48: Playhead LED bar rendered as duplicate status element (DEF-10).
│       ├── Lines 80-143: ACC/SLD toggles render solid fill rather than LED indicator dot (DEF-11).
│       └── Lines 226-300: Octave buttons sit tight to step strip bottom edge (DEF-03).
└── scripts/
    └── crop_gui_sections.py
        └── Line 35: crop_souped coordinate (240, 390) clips drawer header text (DEF-09).
```

---

## Section 4: Step-by-Step Prioritized Remediation Plan

### Step 1: Structural Height & Topology Stability (Resolve DEF-01, DEF-02, DEF-04)
1. In `plugin303/src/gui/panels/souped_panel.rs`:
   - Replace dynamic height allocation with a fixed panel container height (`ui.allocate_exact_size(vec2(ui.available_width(), 108.0), Sense::hover())` or fixed inner content height).
   - Ensure the outer Frame retains identical geometry regardless of the `is_accurate` boolean flag.
   - Anchor the "ACCURATE MODE ACTIVE" overlay box to the full fixed static frame dimensions (`badge_rect` centered at the fixed panel mid-point).

### Step 2: Bipolar Knobs & Value Normalization (Resolve DEF-06, DEF-08, DEF-14)
1. In `plugin303/src/gui/widgets/knob.rs`:
   - Add `.bipolar()` builder flag and field to `Knob303`.
   - In arc painter (lines 105–125): when `is_bipolar` is active, compute the active value arc from `center_angle = 270.0 * PI / 180.0` (12 o'clock) to `current_angle`.
   - Update `params.volume` mapping or `Knob303` display calculation so that +0.0 dB unity gain centers at 12:00 detent.
   - Apply `.bipolar()` to `TUNING`, `VOLUME`, `PITCH1`, `PITCH2`, `TILT`, `IN`, and `OUT` parameter knobs.
   - Remove raw resistance ohm string formatting (`kΩ`, `Ω`); standardize all potentiometer fallbacks to normalized percentages or decibels.

### Step 3: Sequencer Boundary & Containment Polish (Resolve DEF-03, DEF-10, DEF-11)
1. In `plugin303/src/gui/panels/sequencer_panel.rs`:
   - Increase bottom margin of sequencer Frame from `4.0` to `6.0` (`inner_margin(Margin::symmetric(8, 6))`).
2. In `plugin303/src/gui/widgets/step_button.rs`:
   - Adjust vertical item spacing from `1.5` to `1.0` to gain 4.0px total clearance.
   - Consolidate the 4px top LED bar into an explicit tooltip and ensure visual distinctness during idle and active playhead states.
   - Document or refine ACC / SLD full-fill backlit styling to ensure visual balance within 56px step columns.

### Step 4: Visual Harmony, Color Tokens & Unit Formatting (Resolve DEF-05, DEF-07, DEF-12, DEF-13, DEF-15, DEF-16)
1. In `plugin303/src/gui/panels/header_bar.rs`:
   - Configure `SlideSwitch` for `QUALITY` with `.with_thumb_color(theme::ACCENT_RED)`.
2. In `plugin303/src/gui/panels/output_clipper.rs`:
   - Pass `.with_thumb_color(theme::ACCENT_RUBY)` and `.with_active_text_color(theme::ACCENT_RUBY)` to `SlideSwitch` for `OS`.
   - Update unselected GE diode button stroke and text to `theme::TEXT_SECONDARY` and `theme::BORDER_DEFAULT`.
3. In `plugin303/src/gui/panels/tweaks_panel.rs` and `souped_panel.rs`:
   - Harmonize disabled subpanel styling in Accurate mode with consistent alpha masking (`theme::WELL_BG` with dim overlays).
4. In `plugin303/src/gui/panels/generator_panel.rs`:
   - Add `.custom_formatter(|n, _| format!("{:02}", n as i32))` to `PULSES (k)`, `STEPS (n)`, `SHIFT`, and `PITCH LEN`.

### Step 5: Test & Crop Script Alignment (Resolve DEF-09)
1. In `scripts/crop_gui_sections.py`:
   - Change `"crop_souped"` definition from `(int(240 * scale_y), int(390 * scale_y))` to `(int(230 * scale_y), int(390 * scale_y))`.
2. Re-run `cargo test --all` and GUI visual QA capture pipeline to verify zero defects and 100.0% spec compliance.
