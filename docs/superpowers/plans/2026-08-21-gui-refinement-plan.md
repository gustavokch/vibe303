# vibe303 GUI Visual QA Synthesis and Refinement Plan

**Date:** 2026-08-21  
**Status:** In Review  
**Target File:** `/Users/gus/Git/vibe303/docs/superpowers/plans/2026-08-21-gui-refinement-plan.md`  
**Reference Specification:** `/Users/gus/Git/vibe303/docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md`  

---

## 1. Executive Scorecard

This scorecard synthesizes visual quality assurance findings from eight parallel inspection inspectors.

### Defect Counts

- **Critical Severity Defects ($N_{\text{critical}}$):** 2
- **Major Severity Defects ($N_{\text{major}}$):** 6
- **Minor Severity Defects ($N_{\text{minor}}$):** 9
- **Total Unique Defects:** 17

### Deductive Scoring Arithmetic

$$\text{Deductions} = (25 \times N_{\text{critical}}) + (10 \times N_{\text{major}}) + (3 \times N_{\text{minor}})$$
$$\text{Deductions} = (25 \times 2) + (10 \times 6) + (3 \times 9) = 50 + 60 + 27 = 137.0$$

$$\text{Calculated Score} = \max(0.0, 100.0 - 137.0) = \max(0.0, -37.0) = 0.0$$

### Quality Gate Enforcement

- **Critical Defect Rule:** $N_{\text{critical}} \ge 1 \implies \text{Spec Compliance} = \text{FALSE}$, and $\text{Overall Score} = \min(0.0, 69.0) = 0.0$.
- **Major Defect Rule:** $N_{\text{major}} \ge 2 \implies \text{Spec Compliance} = \text{FALSE}$, and $\text{Overall Score} = \min(0.0, 79.0) = 0.0$.
- **Passing Threshold:** Spec compliance requires $N_{\text{critical}} = 0$, $N_{\text{major}} = 0$, and $\text{Overall Score} \ge 95.0$.

### Summary Table

| Metric | Result | Target / Threshold | Status |
| :--- | :--- | :--- | :--- |
| **Critical Defects** | 2 | 0 | Fail |
| **Major Defects** | 6 | 0 | Fail |
| **Minor Defects** | 9 | $\le 1$ | Fail |
| **Deduction Points** | 137.0 | 0.0 | Fail |
| **Calculated Score** | 0.0 / 100.0 | $\ge 95.0$ | Fail |
| **Overall Score** | 0.0 / 100.0 | $\ge 95.0$ | Fail |
| **Spec Compliance** | **FALSE** | **TRUE** | **REJECTED** |

---

## 2. Consolidated Defect Matrix

The table below consolidates all identified defects across the eight inspection domains.

| Element | Section | Severity | Category | Observed Defect | Expected State | Suggested Fix |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| Souped-up engine drawer in Accurate mode | `state_diff` | **Critical** | `layout_collapse` | In accurate mode, the souped-up engine drawer collapses vertically, squishing panel height and clipping child controls. | The drawer container must maintain consistent vertical height and layout geometry when toggled between Fun and Accurate modes. | Preserve minimum height of souped-up drawer in Accurate mode to prevent layout collapse. |
| Sequencer running state playhead indicator | `state_diff` | **Critical** | `missing_feedback` | The transport displays STOP active state while sequencer step LEDs (01-16) show no highlighted playhead position. | The active sequencer step LED and column header must illuminate with accent color during playback. | Bind current playback step index to step LED and header highlight styling during active playback. |
| ENVELOPES sub-panel (MEG/VEG ADSR) | `classic_panel` | **Major** | `containment_overlap` | The ENVELOPES sub-panel extends downward across the bottom border of the parent panel. | The ENVELOPES sub-panel must stay completely inside parent classic panel borders. | Adjust vertical fader height and padding or expand parent panel height to contain ADSR controls. |
| Souped-Up Panel Accurate Mode Dimming Overlay | `souped_drawer` | **Major** | `color_inconsistency` | Dark rectangle overlay is drawn over 100% opaque child widgets. Specular highlights ghost through unevenly. | Uniform 30% alpha attenuation across all panel controls without high-contrast specular glare through overlay. | Apply uniform egui layer opacity or attenuate knob and slider base fill colors when topology is Accurate. |
| Sequencer window bottom containment and octave transpose row | `state_diff` | **Major** | `clipping_overflow` | In accurate.png and running.png, the octave transpose button row is missing and clipped at the bottom. | All sequencer controls, including octave transpose row, must render within window bounds without clipping. | Adjust window viewport bounds or layout container constraints so bottom octave transpose row remains visible. |
| Accurate mode overlay banner | `state_diff` | **Major** | `containment_overlap` | The ACCURATE MODE ACTIVE overlay banner floats over central drawer boundaries, colliding with header text. | The overlay banner must be vertically and horizontally centered within disabled souped-up drawer container. | Center overlay banner within dedicated drawer frame using explicit flex centering and fixed padding. |
| Disabled souped-up drawer controls in Accurate mode | `state_diff` | **Major** | `color_inconsistency` | Disabled labels and switches ghost through with uneven contrast and overlap adjacent generator labels. | Disabled controls must render with uniform low-contrast opacity or hide cleanly behind backdrop overlay. | Apply uniform semi-opaque backdrop panel and consistent disabled alpha token to all bypassed drawer components. |
| Top Controls > Volume Knob (Knob303) | `affordance_semantics` | **Major** | `pointer_mismatch` | Volume knob pointer indicates ~4:30 o'clock with orange arc filled to 85% for nominal +0.0 dB value. | Volume dial pointer and arc should align with nominal 0 dB unity level (or center at 12 o'clock if nominal). | Adjust Volume knob normalization or arc rendering to represent unity gain consistently with nominal center positions. |
| Classic panel right negative space | `classic_panel` | **Minor** | `layout_collapse` | Large empty area exists to the right of ENVELOPES sub-panel while vertical space is constrained. | Negative space must be balanced across panel with consistent horizontal distribution. | Reposition or widen envelope sub-panel to utilize available horizontal space. |
| Accurate Mode Overlay Badge Subtitle | `souped_drawer` | **Minor** | `color_inconsistency` | Subtitle text uses TEXT_SECONDARY (#94a3b8) on dark background (#1c2026), reducing readability. | High-contrast text on badge background for clear legibility at 9.0pt font size. | Change subtitle text color to TEXT_PRIMARY (#f8fafc) or brighten badge background to improve contrast ratio. |
| Bipolar Pitch Knobs Indicator Arcs (PITCH1, PITCH2) | `souped_drawer` | **Minor** | `formatting_units` | Value arc draws from minimum position (7 o'clock) to center position (+0.0 st) for bipolar parameters. | Bipolar parameter arcs centered at 12 o'clock (+0.0 st) with bidirectional sweep. | Add bipolar mode to Knob303 widget to draw arc from 12 o'clock center for signed parameters. |
| scripts/crop_gui_sections.py:crop_clipper_tweaks | `clipper_tweaks` | **Minor** | `clipping_overflow` | Crop window has vertical offset that cuts top headers/knob caps of Output Clipper and captures generator header. | Crop window frames Output Clipper and Tweaks subpanel completely without clipping knob caps or leaking. | Shift vertical crop bounds in scripts/crop_gui_sections.py for crop_clipper_tweaks to start earlier (approx y: 350 to 470). |
| Header Bar > Quality SlideSwitch | `affordance_semantics` | **Minor** | `color_inconsistency` | In Accurate topology mode, Quality selector uses add_enabled_ui, causing LOW label to render with muted contrast. | Selection indicators and labels should maintain consistent theme accents and high-contrast styling. | Use explicit lock state indicator or badge instead of default egui disabled widget opacity. |
| Acid Euclidean Generator > Probability Controls | `affordance_semantics` | **Minor** | `formatting_units` | Generator DragValues display raw normalized decimals (e.g. '0.20') despite label reading 'OCT UP %'. | Controls with '%' in label should format and display values as integer percentages (e.g. 20%). | Apply percentage formatting in generator DragValue custom formatters to display whole percentage numbers. |
| Transport & Sequencer > Step Index and Length Readouts | `affordance_semantics` | **Minor** | `formatting_units` | Step button column headers use two-digit format ('01'), whereas transport 'LEN' displays unpadded integers ('16'). | Numerical step indicators and length values should use consistent zero-padding conventions across interface. | Standardize numerical padding across sequencer transport displays. |
| Output Clipper > TONE and R-COMP Knobs | `affordance_semantics` | **Minor** | `formatting_units` | TONE displays raw float '1.00' and R-COMP displays '0.00' without units. | Knob value readouts should include explicit physical units or percent symbols consistently. | Add explicit unit formatting or perceptual percentage suffixes to TONE and R-COMP readouts. |
| Output Clipper > ASYM (GE) Checkbox | `affordance_semantics` | **Minor** | `branding_affordance` | ASYM (GE) uses a standard checkbox control lacking hardware circuit switch affordance. | Diode clipping circuit selection should provide clear switch-style affordance and illuminated active feedback. | Replace checkbox with dedicated toggle switch or illuminated latching button widget. |

---

## 3. Root Cause Source Mapping

The defects map to the following exact source code files and lines in `/Users/gus/Git/vibe303/plugin303/src/gui/`:

1. **Souped-Up Drawer Height Collapse, Centering, and Dimming:**
   - File: `/Users/gus/Git/vibe303/plugin303/src/gui/panels/souped_panel.rs`
   - Lines 18–21: Fixed `min_height(86.0)` is insufficient when Accurate mode changes inner widget layout.
   - Lines 81–125: Overlay rectangle drawing and badge centering do not lock drawer container bounds.
   - Lines 85–89: Solid overlay fill allows high-specular elements to ghost through without widget-level alpha attenuation.

2. **Sequencer Running State Playhead Feedback:**
   - File: `/Users/gus/Git/vibe303/plugin303/src/gui/widgets/step_button.rs`
   - Lines 21–40: Step LED and column header highlight condition evaluates inactive during snapshot render due to transport state binding.
   - File: `/Users/gus/Git/vibe303/plugin303/src/gui/panels/sequencer_panel.rs`
   - Lines 45–55: `is_running` parameter read from `params.seq_run` is not synchronizing with test harness step advancement.

3. **Envelopes Sub-panel Containment and Classic Panel Geometry:**
   - File: `/Users/gus/Git/vibe303/plugin303/src/gui/panels/top_controls.rs`
   - Lines 113–173: `env_frame` inner margins and `render_adsr_bank` vertical mini-fader height exceed parent frame height of 180px.
   - Lines 100–112: Horizontal spacing causes large negative void to the right while vertical bounds clip bottom border.

4. **Sequencer Bottom Octave Transpose Clipping:**
   - File: `/Users/gus/Git/vibe303/plugin303/src/gui/mod.rs`
   - Lines 13–15: Window dimension constants (`GUI_WIDTH = 1080`, `GUI_HEIGHT = 720`).
   - File: `/Users/gus/Git/vibe303/plugin303/src/gui/panels/sequencer_panel.rs`
   - Lines 80–130: Bottom octave transpose row exceeds window layout allocation when vertical spacing accumulates across top panels.

5. **Volume Knob Unity Gain and Bipolar Arcs:**
   - File: `/Users/gus/Git/vibe303/plugin303/src/gui/widgets/knob.rs`
   - Lines 95–96: `param.unmodulated_normalized_value()` maps linearly without compensating for 0 dB center default.
   - Lines 126–162: Arc painter starts unconditionally at `start_angle` (7 o'clock) rather than 12 o'clock center for bipolar parameters.
   - File: `/Users/gus/Git/vibe303/plugin303/src/gui/panels/souped_panel.rs`
   - Lines 70–71: `PITCH1` and `PITCH2` use default unipolar arc drawing.

6. **Formatters, Switches, and Affordances:**
   - File: `/Users/gus/Git/vibe303/plugin303/src/gui/panels/generator_panel.rs`
   - Lines 180–220: `DragValue` custom formatters for percentage fields missing `%.0%` string formatting.
   - File: `/Users/gus/Git/vibe303/plugin303/src/gui/panels/output_clipper.rs`
   - Lines 45–55: `TONE` and `R-COMP` knob value text formatters omit unit suffixes.
   - Lines 108–116: `ASYM (GE)` uses `ui.checkbox` instead of `SlideSwitch` or latching button.
   - File: `/Users/gus/Git/vibe303/plugin303/src/gui/panels/header_bar.rs`
   - Lines 158–170: `ui.add_enabled_ui(!is_accurate, ...)` applies default egui graying to Quality switch.

7. **Crop Script Section Offsets:**
   - File: `/Users/gus/Git/vibe303/scripts/crop_gui_sections.py`
   - Line 36: `"crop_clipper_tweaks": (int(385 * scale_y), int(520 * scale_y))` offsets clip top controls and capture generator header.

---

## 4. Step-by-Step Prioritized Remediation Plan

Remediation follows a strict four-phase priority sequence: Critical Blockers first, then Major Layout & Affordance defects, then Minor Polish & Formatting.

```
+-----------------------------------------------------------------------------------+
| Phase 1: Critical State & Geometry Blockers                                       |
| - Lock souped-up drawer minimum height in Accurate mode                          |
| - Connect active step playback indicator to sequencer playhead LEDs               |
+-----------------------------------------------------------------------------------+
                                         |
                                         v
+-----------------------------------------------------------------------------------+
| Phase 2: Major Containment & Visual Overlay Remediation                           |
| - Rebalance Classic Panel ADSR fader heights and horizontal distribution          |
| - Fix Accurate mode overlay centering, backdrop opacity, and child dimming        |
| - Fix Sequencer bottom boundary containment and octave transpose row clipping     |
| - Calibrate Volume Knob 0 dB nominal center pointer & arc representation         |
+-----------------------------------------------------------------------------------+
                                         |
                                         v
+-----------------------------------------------------------------------------------+
| Phase 3: Minor Formatting, Units, and Affordance Alignment                        |
| - Implement bipolar 12 o'clock center arc in Knob303 for pitch parameters         |
| - Format Generator DragValues as integer percentages (e.g. 20%)                   |
| - Standardize step number zero-padding ('01'..'16') and add unit readouts         |
| - Replace ASYM checkbox with dedicated circuit toggle switch widget               |
| - Update Accurate mode overlay badge subtitle text color to TEXT_PRIMARY          |
| - Refine Quality switch disabled state styling in Header Bar                      |
| - Adjust crop coordinates in scripts/crop_gui_sections.py                         |
+-----------------------------------------------------------------------------------+
                                         |
                                         v
+-----------------------------------------------------------------------------------+
| Phase 4: Automated Visual QA Verification & Gate Re-Evaluation                    |
| - Re-render all snapshot targets (standard, accurate, running, dark)              |
| - Run 8-inspector QA evaluation suite to confirm 0 critical and 0 major defects   |
| - Validate overall visual quality score >= 95.0 for spec compliance pass          |
+-----------------------------------------------------------------------------------+
```

### Phase 1: Critical State & Geometry Blockers

1. **Lock Souped-Up Drawer Container Height (`plugin303/src/gui/panels/souped_panel.rs`):**
   - Set fixed container height for the souped-up drawer across both Fun and Accurate modes.
   - Prevent container collapse and vertical jump when toggling topology.

2. **Synchronize Playhead Tracking (`plugin303/src/gui/widgets/step_button.rs` & `plugin303/src/gui/panels/sequencer_panel.rs`):**
   - Ensure `is_current` and `is_running` correctly evaluate active playhead step index during playback.
   - Highlight active step button header and LED with `ACCENT_ORANGE`.

### Phase 2: Major Containment & Visual Overlay Remediation

3. **Rebalance Classic Panel ADSR Geometry (`plugin303/src/gui/panels/top_controls.rs`):**
   - Reduce mini-fader vertical track height by 6px to fit entirely within the 180px panel bounds.
   - Adjust horizontal spacing across rotary dials to eliminate right-side negative space.

4. **Refactor Accurate Mode Dimming & Overlay (`plugin303/src/gui/panels/souped_panel.rs`):**
   - Dim child controls to 30% base opacity before painting.
   - Center the `ACCURATE MODE ACTIVE` badge strictly inside the drawer frame using explicit bounds centering.

5. **Fix Bottom Sequencer Viewport Bounds (`plugin303/src/gui/panels/sequencer_panel.rs`):**
   - Adjust vertical panel gaps from 5.0px to 4.0px.
   - Ensure the octave transpose row renders with 6px bottom safety margin above chassis border.

6. **Calibrate Volume Knob Pointer & Arc (`plugin303/src/gui/widgets/knob.rs`):**
   - Align nominal 0 dB unity position with 12 o'clock detent.

### Phase 3: Minor Formatting, Units, and Affordance Alignment

7. **Add Bipolar Arc Support to `Knob303` (`plugin303/src/gui/widgets/knob.rs`):**
   - Add `.bipolar(true)` builder option.
   - Sweep arc from 12 o'clock center (+0.0) left for negative values and right for positive values.

8. **Format Generator DragValues & Unit Suffixes (`plugin303/src/gui/panels/generator_panel.rs` & `output_clipper.rs`):**
   - Add custom formatters showing whole percentage numbers (`{:.0}%`) for `OCT UP`, `OCT DN`, and `REST`.
   - Add `x` unit to `TONE` and `%` unit to `R-COMP`.

9. **Replace ASYM Checkbox with SlideSwitch (`plugin303/src/gui/panels/output_clipper.rs`):**
   - Replace standard checkbox with mini toggle switch matching the UI hardware aesthetic.

10. **Refine Crop Bounds (`scripts/crop_gui_sections.py`):**
    - Update `crop_clipper_tweaks` bounds to `(int(350 * scale_y), int(480 * scale_y))`.

### Phase 4: Automated Visual QA Verification

11. **Run Full Visual QA Pipeline:**
    - Execute snapshot capture and multi-crop extraction.
    - Run the 8 parallel inspectors.
    - Verify $N_{\text{critical}} = 0$, $N_{\text{major}} = 0$, $N_{\text{minor}} \le 1$, and $\text{Overall Score} \ge 95.0$.
