# GUI Visual QA Refinement Plan: vibe303 (TECHNEO 303)

Date: 2026-08-23  
Evaluator: Lead Visual QA Evaluator  
Scope: Complete nih_plug_egui interface audit across all 8 inspector dimensions  

---

## Section 1: Executive Scorecard

### Defect Inventory Summary

- Critical Severity Defects (N_critical): 1
- Major Severity Defects (N_major): 5
- Minor Severity Defects (N_minor): 18
- Total Evaluated Findings: 24

### Deductive Scoring Calculation

- Base Score: 100.0
- Critical Deduction: 1 * 25 = 25.0
- Major Deduction: 5 * 10 = 50.0
- Minor Deduction: 18 * 3 = 54.0
- Total Deductions: 25.0 + 50.0 + 54.0 = 129.0
- Raw Calculated Score: max(0.0, 100.0 - 129.0) = 0.0

### Quality Gate Enforcement

1. Critical Defect Gate: N_critical = 1 (>= 1). Specification compliance is FALSE. Maximum allowable score is min(0.0, 69.0) = 0.0.
2. Major Defect Gate: N_major = 5 (>= 2). Specification compliance is FALSE. Maximum allowable score is min(0.0, 79.0) = 0.0.
3. Spec Compliance Passing Threshold: Requires N_critical == 0, N_major == 0, and score >= 95.0.

- Overall Deductive Score: 0.0 / 100.0
- Specification Compliance Status: FAIL (NON-COMPLIANT)

---

## Section 2: Consolidated Defect Matrix

| Element | Section | Severity | Category | Observed Defect | Expected State | Suggested Fix |
|---|---|---|---|---|---|---|
| Main Window / Bottom Margin | state_diff | critical | layout_collapse | In accurate mode, the middle drawer collapses, causing lower panels to shift up and leaving a 140px dead black gap (9.3% window height) at the bottom. | Window layout maintains a rigid fixed vertical structure across all modes with zero dead margins. | Assign a persistent fixed height or min-height to the middle drawer frame in both FUN and ACCURATE modes. |
| Souped-Up Engine & Analog Drawer | state_diff | major | layout_collapse | Drawer vertical height compresses from 260px in FUN mode to 140px in ACCURATE mode, causing control displacement. | Middle drawer keeps a constant vertical dimension in both operational topologies. | Lock middle drawer container height in `souped_panel.rs` regardless of topology selection. |
| Accurate Mode Overlay Card | state_diff | major | containment_overlap | Accurate mode overlay banner floats without a bounded parent card and displaces/overlaps neighbor controls in diff view. | Mode overlay remains centered and strictly bounded within its module container without displacing neighboring elements. | Wrap overlay in fixed-dimension card centered within the persistent drawer bounds. |
| Disabled Parameter Controls | state_diff | major | color_inconsistency | Disabled and bypassed control dimming is inconsistent; Analog Tweaks and Envelopes dim while Output Clipper remains bright; borders do not dim. | All bypassed modules and container frames apply uniform visual disabled styling and opacity dimming. | Implement unified container-level disabled styling across Output Clipper and Analog Tweaks. |
| Sequencer Step Playhead Indicator | state_diff | major | missing_feedback | During sequencer playback, only step number label 04 gets a thin red border; active column, buttons, and LEDs show no visual state change. | Active step column provides prominent visual feedback (column glow, illuminated background, active playhead cursor). | Highlight active step column background and augment active step button styling during playback. |
| Top panel Volume knob | affordance_semantics | major | pointer_mismatch | Volume dial needle points at ~4:30 to 5:00 o'clock for nominal unity gain (0.0 dB), while other 0 dB/0 ct knobs center at 12 o'clock. | Nominal 0.0 dB unity gain aligns at 12 o'clock (0.5 normalized position) to match user mental model. | Adjust Volume parameter range skew factor in `params.rs` so 0.0 dB maps to normalized 0.5 center position. |
| Header Bar Brand Title | header_bar | minor | branding_affordance | Header displays "TECHNEO 303" logotype, whereas project documentation references "vibe303", "plugin303", and "TB-303". | Branding taxonomy remains uniform across user interface, specifications, and project documentation. | Standardize product branding taxonomy across GUI title and project documentation. |
| Header Bar Quality Tier Switch | header_bar | minor | color_inconsistency | Active "LOW" tier switch detent renders with ACCENT_ORANGE instead of the dedicated red tier accent. | Fidelity tier detent renders with red accent (ACCENT_RED) per specification section 3.2 to distinguish tier from topology. | Pass `.with_active_text_color(theme::ACCENT_RED)` and `.with_thumb_color(theme::ACCENT_RED)` to SlideSwitch. |
| Header Bar Metadata Badges (BASS SYNTH / MONO) | header_bar | minor | color_inconsistency | "BASS SYNTH" badge renders with ACCENT_ORANGE text, while adjacent "MONO" badge renders with TEXT_SECONDARY. | Informational metadata badges in the brand group use a unified color scheme and visual hierarchy. | Standardize text color tokens across header metadata badges in `header_bar.rs`. |
| Master Tuning Dial Allocation | header_bar | minor | branding_affordance | Master Tuning dial is located in the Classic Panel rather than the Header Bar. | Master Tuning dial resides in Classic Panel 7-knob row per accepted design deviation to keep Header Bar at 34px. | Retain Master Tuning dial in Classic Panel as specified by accepted design deviations. |
| Drift and Age Parameter Readouts Allocation | header_bar | minor | branding_affordance | Drift and Age readouts reside in Analog Tweaks panel rather than Header Bar. | Component drift and age controls reside in Analog Tweaks panel per accepted design deviation. | Retain Drift and Age controls in Analog Tweaks subpanel as specified. |
| Knob303 Rotary Dials | classic_panel | minor | branding_affordance | Rotary dials render continuous smooth perimeter arc tracks without discrete radial tick marks. | Outer bezel renders discrete radial tick marks across the 270-degree sweep per section 3.1. | Add discrete radial tick mark painting along the 270-degree sweep in `widgets/knob.rs`. |
| ADV ENV Checkbox in Envelopes Frame | classic_panel | minor | layout_collapse | ADV ENV toggle sits directly adjacent to ENVELOPES title with 8px gap, leaving unbalanced empty space on the right. | ADV ENV toggle aligns cleanly to the right side of the ENVELOPES header frame. | Right-align ADV ENV toggle within the ENVELOPES subpanel header using layout space allocation. |
| MEG and VEG ADSR Mini-Faders Inactive State | classic_panel | minor | missing_feedback | When ADV ENV is unchecked, faders dim in color but remain interactive to mouse drag gestures. | When ADV ENV is disabled, ADSR faders are disabled and do not accept user input. | Wrap ADSR fader bank container in `ui.add_enabled_ui(is_adv, ...)` in `top_controls.rs`. |
| Output Clipper header and Tweaks top row crop | clipper_tweaks | minor | clipping_overflow | Screenshot crop coordinate boundary clips the Output Clipper title and top row of Tweaks panel. | Screenshot capture script crops full bounding box including titles and top controls. | Update crop coordinate rectangle in `scripts/crop_gui_sections.py`. |
| Clipper parameter unit readouts | clipper_tweaks | minor | formatting_units | Parameters display perceptual units (% and dB) rather than raw potentiometer resistance. | Parameter readouts display %, dB, and ms perceptual units. | Verified compliant. Retain perceptual unit formatting. |
| Diode model selector (SI / GE) | clipper_tweaks | minor | branding_affordance | Diode push buttons display SI and GE clearly with high contrast and active orange border. | High-contrast dual state buttons with clear active affordance. | Verified compliant. Retain clear push button affordance. |
| Seed parameter labels | clipper_tweaks | minor | branding_affordance | Tweaks panel designates COMP SEED while Generator panel designates GEN SEED. | Distinct label prefixes differentiate component tolerance seed from pattern seed. | Verified compliant. Retain COMP SEED and GEN SEED prefixes. |
| Octave shift button row (▲ / ▼) | sequencer | minor | containment_overlap | Octave shift arrow buttons have less than 2px clearance against bottom panel border. | Bottom control row maintains balanced padding (6px to 8px) matching top and side margins. | Increase bottom inner margin of sequencer panel frame from 4px to 8px in `sequencer_panel.rs`. |
| Step playhead LED bar | sequencer | minor | branding_affordance | Inactive playhead LEDs render with elevated surface fill and border, appearing as dark red/gray dashes when stopped. | Inactive playhead LEDs appear subtle and recessed without visual ambiguity. | Lower contrast of inactive playhead LED indicator when sequencer playback is stopped. |
| Header Quality selector and Transport highlights | affordance_semantics | minor | color_inconsistency | Quality selector uses orange, playhead uses ruby red, transport RUN uses orange. | State highlights and interactive selections follow a consistent semantic theme hierarchy. | Align state indicators with documented semantic tokens in `theme.rs`. |
| Percentage knobs and parameter readouts | affordance_semantics | minor | formatting_units | Percentage readouts inconsistently mix single-decimal (50.0%) and integer (50%) formats. | All percentage parameters use uniform formatting precision across all panels. | Standardize percentage formatting in `widgets/knob.rs` to consistent precision. |
| Sequencer and Generator numeric input fields | affordance_semantics | minor | formatting_units | Digit padding varies inconsistently between two-digit zero-padded (05, 00) and unpadded (16). | Numeric input fields follow uniform zero-padding conventions. | Standardize DragValue formatters across Generator and Sequencer panels. |
| Output Clipper DIODE selector affordance | affordance_semantics | minor | branding_affordance | Push buttons display clear active/inactive state without strikethrough artifacts. | Model selectors provide clear tactile state affordance. | Verified compliant. Retain current push button styling. |

---

## Section 3: Root Cause Source Mapping

### 1. `plugin303/src/gui/mod.rs`
- **Lines 30-33**: Hardcoded window title `"TECHNEO 303"`.
- **Lines 40-71**: CentralPanel layout does not enforce a rigid vertical fill for the middle drawer container, allowing vertical collapsing when drawer height changes.

### 2. `plugin303/src/gui/panels/souped_panel.rs`
- **Lines 19-28**: Drawer container specifies `ui.set_min_height(86.0)` in Accurate mode instead of matching the full Fun mode height (~260px), causing the 140px dead gap at the bottom of the window.
- **Lines 31-76**: Accurate mode overlay card renders inside dynamic vertical layout with manual padding calculations rather than occupying a fixed-size container frame.

### 3. `plugin303/src/gui/panels/header_bar.rs`
- **Lines 27-30**: Branding logotype string `"TECHNEO 303"`.
- **Lines 48, 68**: Metadata badges use discordant text colors (`theme::ACCENT_ORANGE` for "BASS SYNTH" vs `theme::TEXT_SECONDARY` for "MONO").
- **Lines 124-131**: `SlideSwitch` for `QUALITY` tier is instantiated without custom accent colors, falling back to default `theme::ACCENT_ORANGE` instead of `theme::ACCENT_RED`.

### 4. `plugin303/src/gui/panels/top_controls.rs`
- **Lines 122**: Top panel Volume knob binding.
- **Lines 138-160**: ADV ENV checkbox positioned directly in `ui.horizontal` next to `"ENVELOPES"` label with fixed `ui.add_space(8.0)` without right alignment.
- **Lines 164-189**: `render_adsr_bank` invocations pass `is_adv` for color styling but do not wrap the bank UI in `ui.add_enabled_ui(is_adv, ...)`, leaving faders interactive when disabled.

### 5. `plugin303/src/params.rs`
- **Lines 451-460**: Parameter definition for `volume` uses `FloatRange::SymmetricalSkewed` with `factor: FloatRange::skew_factor(-2.0)` and `min: -90.0, max: 6.0`, mapping 0.0 dB to normalized ~0.85 (pointing at ~4:45 o'clock) rather than normalized 0.5 (12 o'clock).

### 6. `plugin303/src/gui/panels/output_clipper.rs`
- **Lines 11-16, 49-97**: Frame container and title remain at full opacity when bypassed; disabled state is only applied to internal controls via `ui.add_enabled_ui(!bypassed, ...)`.

### 7. `plugin303/src/gui/panels/sequencer_panel.rs`
- **Lines 27-35**: Outer Frame uses `inner_margin(Margin::symmetric(8, 4))`, causing octave shift buttons on the bottom row to sit <2px from the frame border.
- **Lines 77-96**: Transport RUN button styling and color definitions.

### 8. `plugin303/src/gui/widgets/step_button.rs`
- **Lines 31-48**: Playhead LED bar renders with `theme::SURFACE_ELEVATED` and `theme::BORDER_DEFAULT` when stopped, appearing as dark dashed artifacts.
- **Lines 50-74**: Step pill header only illuminates the step number text and border, providing insufficient visual feedback for the entire active step column during playback.

### 9. `plugin303/src/gui/widgets/knob.rs`
- **Lines 88-125**: Knob arc renderer draws smooth continuous 24-segment line arc without discrete radial tick marks along the 270-degree bezel.
- **Lines 155-200**: Parameter value formatting logic mixes `{:.0}%` and `{:.1}%` formatting dynamically.

### 10. `scripts/crop_gui_sections.py`
- **Crop coordinate mapping**: Bounding box coordinates for Clipper and Tweaks subpanels truncate the top title and first control row.

---

## Section 4: Step-by-Step Prioritized Remediation Plan

### Phase 1: Critical Layout Stabilization (Fix Window Gap & Drawer Collapse)
1. **Fix Middle Drawer Fixed Dimensions (`souped_panel.rs`)**:
   - Set a constant height (e.g., fixed `min_height` / `height` of 260px) for the middle drawer container across both FUN and ACCURATE modes.
   - Center the Accurate Mode overlay card vertically and horizontally within the fixed-dimension drawer container.
   - Ensure the main window vertical layout consumes the full 720px height without any dead black gap at the bottom.

### Phase 2: Major Visual & Control Fixes
2. **Normalize Volume Knob Unity Gain (`params.rs` & `top_controls.rs`)**:
   - Adjust `FloatRange` for `volume` in `plugin303/src/params.rs` so 0.0 dB sits at the center normalized value (0.5), pointing directly at 12 o'clock.
3. **Enhance Sequencer Playhead Active Feedback (`widgets/step_button.rs` & `sequencer_panel.rs`)**:
   - Highlight the entire active step column during playback with an elevated background fill or subtle column glow.
   - Lower inactive playhead LED contrast when stopped (`theme::WELL_BG` with no border stroke) to eliminate dash artifacts.
4. **Enforce Disabled UI State on Inactive Envelopes (`top_controls.rs`)**:
   - Wrap the ADSR bank rendering in `ui.add_enabled_ui(is_adv, ...)` so mini-faders reject mouse interactions when ADV ENV is unchecked.
5. **Harmonize Bypassed Container Styling (`output_clipper.rs` & `tweaks_panel.rs`)**:
   - Apply uniform dimmed border and label styling to bypassed sections.

### Phase 3: Minor Visual Polish & Alignment
6. **Correct Header Quality Tier Colors & Badges (`header_bar.rs`)**:
   - Pass `theme::ACCENT_RED` to the Quality `SlideSwitch` widget.
   - Standardize text colors for "BASS SYNTH" and "MONO" metadata badges to `theme::TEXT_SECONDARY`.
7. **Right-Align ADV ENV Toggle (`top_controls.rs`)**:
   - Use `ui.with_layout(Layout::right_to_left(...))` or layout spacing in the ENVELOPES subpanel header to place the ADV ENV checkbox on the far right.
8. **Add Radial Tick Marks to Knobs (`widgets/knob.rs`)**:
   - Render subtle discrete radial tick mark segments around the 270-degree sweep.
9. **Fix Sequencer Bottom Margin (`sequencer_panel.rs`)**:
   - Increase bottom margin from 4px to 8px on the sequencer panel container.
10. **Standardize Numeric & Percentage Formatting (`widgets/knob.rs`, `generator_panel.rs`)**:
    - Unify percentage formatting precision and digit zero-padding conventions.
11. **Update Section Crop Coordinates (`scripts/crop_gui_sections.py`)**:
    - Expand crop bounding box for Clipper and Tweaks subpanels.
