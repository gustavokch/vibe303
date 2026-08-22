# GUI Visual QA Synthesis & Remediation Plan

**Date:** 2026-08-21  
**Author:** Lead Visual QA Evaluator  
**Canvas Target:** 1280 x 720 Flat Modular Canvas  
**Status:** BLOCKED (Spec Compliance Failed)

---

## Section 1: Executive Scorecard

### 1.1 Defect Counts by Severity
- **Critical Defects ($N_{critical}$):** 2
- **Major Defects ($N_{major}$):** 5
- **Minor Defects ($N_{minor}$):** 11
- **Total Unique Defects:** 18
- **Positive Compliance Confirmations:** 5 (Waveform glyphs, Rotary sweep arcs, ADSR containment, ADV ENV layout, Sequencer button fill)

### 1.2 Deductive Arithmetic
- **Critical Penalty:** $25 \times 2 = 50.0$
- **Major Penalty:** $10 \times 5 = 50.0$
- **Minor Penalty:** $3 \times 11 = 33.0$
- **Total Deductions:** $50.0 + 50.0 + 33.0 = 133.0$
- **Calculated Raw Score:** $\max(0.0, 100.0 - 133.0) = 0.0 / 100.0$

### 1.3 Quality Gate Enforcement & Compliance Status
- **Critical Gate ($N_{critical} \ge 1$):** FAILED (Score capped at $\le 69.0$).
- **Major Gate ($N_{major} \ge 2$):** FAILED (Score capped at $\le 79.0$).
- **Passing Threshold Requirements:** $N_{critical} = 0 \land N_{major} = 0 \land \text{Score} \ge 95.0$.
- **Overall Quality Score:** **0.0 / 100.0**
- **Specification Compliance:** **NON-COMPLIANT (`false`)**

---

## Section 2: Consolidated Defect Matrix

| Element | Section | Severity | Category | Observed Defect | Expected State | Suggested Fix |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **Header Bar Branding & Mode Switches** | `header_bar` | **Critical** | `containment_overlap` | The TOPOLOGY switch collides directly with the TECHNEO 303 title text and BASS SYNTH badge on the left side of the header. | Brand logotype and badges render on the left with dedicated bounds; control group stays on the right without collision. | Refactor `header_bar.rs` to use explicit column constraints or bounding widths rather than unconstrained right-to-left layout packing. |
| **Accurate Mode Bypass Overlay Box** | `souped_drawer` / `state_diff` | **Critical** | `containment_overlap` | The overlay card floats awkwardly over central knobs only. Output Clipper and Analog Character panels remain exposed below the card. | Accurate mode overlay must cover the full Souped-Up Engine and Analog Drawer area or replace the drawer contents. | Expand the overlay boundary to span the full drawer height and width, masking Output Clipper and Analog Character panels. |
| **Voice Mode Indicator Badge** | `header_bar` | **Major** | `containment_overlap` | The MONO badge is completely obscured and hidden under the overlapping TOPOLOGY and QUALITY switches. | The MONO voice mode badge must be clearly visible beside the brand logotype to indicate single-voice operation. | Allocate dedicated width for the brand container and add a minimum spacer before interactive controls. |
| **Circuit Topology and Quality Switch Positioning** | `header_bar` | **Major** | `layout_collapse` | The TOPOLOGY switch is pushed too far to the left due to unconstrained right-to-left layout packing. | The TOPOLOGY and QUALITY slide switches must align cleanly on the right side of the header bar with distinct margins. | Constrain the right-to-left layout container width and adjust item spacing in `header_bar.rs`. |
| **Accurate Mode Disabled Controls Ghosting** | `state_diff` / `clipper_tweaks` | **Major** | `color_inconsistency` | Upper drawer knobs are dimmed behind the overlay box, but Output Clipper red knobs and Analog Character controls remain at full brightness. | All bypassed engine controls must show uniform disabled styling and dimming when Accurate mode is active. | Apply uniform opacity and disable pointer interactions across all souped-up engine child panels in Accurate mode. |
| **Header Bar Topology Label** | `header_bar` / `state_diff` | **Major** | `clipping_overflow` | The label renders as `__ TOPOLOGY ___` with clipped glyph tops across latest, accurate, and running states. | The TOPOLOGY header label must render without text truncation or clipping across all engine states. | Increase header label bounding box height and remove corrupt underline glyph artifacts in `switch.rs`. |
| **Volume Knob Pointer Angle** | `classic_panel` / `affordance_semantics` | **Major** | `pointer_mismatch` | Volume knob pointer sits at ~4:30 o'clock for +0.0 dB while bipolar controls (Tuning +0 ct, Pitch +0.0 st) point at 12:00 o'clock. | Unity gain (+0.0 dB) or default position aligns with a distinct center mark (12:00 o'clock) or matches user expectations for standard level. | Map volume knob default unity gain (+0.0 dB) to 12:00 o'clock center or provide clear scale detent markings in `knob.rs`. |
| **Product Title Branding** | `header_bar` | **Minor** | `branding_affordance` | The logotype displays TECHNEO 303 while the design specification references vibe303 and TB-303 architecture. | The product title must maintain consistent branding across the interface and design specification. | Standardize the product identity across documentation and UI strings. |
| **Accurate Mode Dimming Overlay Scrim** | `souped_drawer` | **Minor** | `color_inconsistency` | The dark overlay scrim allows high-luminance orange value arcs and white text readouts on GLIDE, PITCH1, PITCH2, and PW to ghost through at uneven brightness. | All dimmed controls under the accurate mode overlay must have uniform low-contrast attenuation without high-luminance elements bleeding through. | Dim or desaturate knob arcs and text labels when CircuitTopology is Accurate prior to painting the overlay scrim. |
| **Topology Bypass Indication Consistency** | `souped_drawer` / `clipper_tweaks` | **Minor** | `color_inconsistency` | The souped drawer renders a full dark overlay scrim with a central un-bypass button, while the adjacent Analog Character & Tweaks panel only applies standard widget disabled state. | The bypassed analog tweaks and souped-up engine controls must present a uniform bypass visual state across all relevant panels. | Harmonize the bypass visual treatment between the Souped-Up Drawer and Analog Character & Tweaks panels. |
| **Souped-Up Drawer Header Label Spacing** | `souped_drawer` | **Minor** | `clipping_overflow` | The header label SOUPED-UP ENGINE & ANALOG DRAWER is positioned near the top boundary and clips under tight crop margins. | The drawer title label and selector switches must maintain sufficient vertical padding inside the panel frame. | Refine inner margins and vertical spacing in `souped_panel.rs` to prevent clipping of the top label. |
| **Generator Seed Label Formatting** | `clipper_tweaks` / `generator_panel` | **Minor** | `formatting_units` | Label contains a space before the colon: `GEN SEED :`. | Label format without space before colon (`GEN SEED:`) to match `COMP SEED:`, `TOL:`, `AGE:`, `NOISE:`, and `DIODE:`. | Remove the trailing space before the colon in the generator seed label string. |
| **Bottom Octave Buttons & Transport Row Margins** | `sequencer` | **Minor** | `containment_overlap` | Octave buttons (▲ / ▼) and bottom transport controls sit close to the bottom container border with only 5 px margin. | Sufficient bottom padding (8 px) prevents controls from touching the bottom panel border. | Change Frame `inner_margin` in `sequencer_panel.rs` from `Margin::symmetric(8, 5)` to `Margin::symmetric(8, 8)`. |
| **Playhead Indicator Bar Tooltip** | `sequencer` | **Minor** | `missing_feedback` | Inactive playhead bar renders as an unlabeled horizontal dash above each step number without hover tooltip. | Playhead LED bar shows tooltip or track affordance to identify its function during idle state. | Add hover tooltip 'Playhead position indicator' to the `allocate_exact_size` response for the playhead LED rect. |
| **Diode Model Selector Toggle Affordance** | `clipper_tweaks` / `affordance_semantics` | **Minor** | `branding_affordance` | Diode control is a single momentary button showing only the active state name (SI), which creates ambiguous toggle affordance. | Dual-state toggle displays both state options (SI and GE) with an active selection indicator. | Replace the single button with a two-option segmented `SlideSwitch` widget showing both SI and GE options. |
| **Generator Parameter Redundant % Labels** | `generator_panel` / `affordance_semantics` | **Minor** | `formatting_units` | Labels contain redundant percentage symbols in both header and value (for example, `REST %: 20%` and `ACCENT %: 35%`). | Percentage symbol appears only in the numeric value badge or only in the parameter header. | Remove `%` from parameter labels (`REST:`, `ACCENT:`) and keep `%` exclusively in the value display badge. |
| **Analog Tweaks Drift Decimal Precision** | `clipper_tweaks` / `affordance_semantics` | **Minor** | `formatting_units` | Tweaks Drift displays `10.0%` (one decimal place) while all other percentage controls display integer percentages (`0%`, `50%`). | Percentage values use consistent decimal precision across all panels. | Format percentage values consistently across panels using integer precision or unified single-decimal precision. |
| **Quality Tier Accent Color Keying** | `header_bar` / `affordance_semantics` | **Minor** | `color_inconsistency` | LOW quality selector uses standard orange highlight while Output Clipper uses ruby accent and Sequencer uses cyan/orange dual accents without a unified key. | Selection highlight colors follow a consistent semantic scheme across all controls. | Standardize accent colors to reinforce functional domains (Orange: Synthesis/Engine, Ruby: Saturation/Clipper, Cyan: Modulation/Sequencer). |

---

## Section 3: Root Cause Source Mapping

### 3.1 `plugin303/src/gui/panels/header_bar.rs`
- **Lines 18–74:** Left brand logotype and MONO/BASS SYNTH badges painted at fixed absolute offsets `[left + 10.0, left + 240.0]`.
- **Lines 76–150:** `ui.horizontal` with `ui.add_space(250.0)` feeding into `Layout::right_to_left`. Because `SlideSwitch` width is large, right-to-left layout overpacks and pushes the `TOPOLOGY` switch directly into the left region `[left + 180.0, left + 250.0]`, directly colliding with and obscuring the `MONO` badge and `TECHNEO 303` text.
- **Lines 118–135:** Hardcoded accent colors and unconstrained right-to-left width.

### 3.2 `plugin303/src/gui/panels/souped_panel.rs`
- **Lines 13–82:** `Frame::new().show(...)` encompasses only the upper drawer engine controls.
- **Lines 84–120:** `if is_accurate` overlay paints exclusively over `response.response.rect`, leaving the bottom child panels (`output_clipper.rs` and `tweaks_panel.rs`) un-scrimmed and fully interactive.
- **Lines 23–35:** Top header padding is tight (`Margin::same(8)` with label at y=0), causing glyph clipping on high DPI scaling.

### 3.3 `plugin303/src/gui/widgets/switch.rs`
- **Lines 40–110:** `SlideSwitch` label bounding calculation trims vertical height, causing top glyph clipping and rendered underline artifacts on labels like `__ TOPOLOGY ___`.

### 3.4 `plugin303/src/gui/widgets/knob.rs`
- **Lines 35–115:** Radial arc painter does not desaturate or attenuate luminance when parent UI is disabled or under accurate topology mode.
- **Lines 40–60:** Volume parameter scaling maps 0 dB unity gain to 0.75 normalized (~4:30 o'clock) instead of standard 12:00 center without detent indicators.

### 3.5 `plugin303/src/gui/panels/generator_panel.rs`
- **Lines 55–58:** `format!("GEN SEED : {:016X}")` contains an extraneous space prior to the colon.
- **Lines 264–346:** Labels explicitly include redundant percentage signs (`REST %:`, `ACCENT %:`, `SLIDE %:`, `ACC BIAS %:`, `MUTATION %:`).

### 3.6 `plugin303/src/gui/panels/tweaks_panel.rs`
- **Lines 17–135:** Wrapped only in `ui.add_enabled_ui(!is_accurate)`, which dims widgets slightly but lacks a visual scrim matching `souped_panel.rs`.
- **Lines 125–134:** `comp_drift` formatting defaults to single decimal place (`10.0%`) while surrounding controls use integer precision.

### 3.7 `plugin303/src/gui/panels/sequencer_panel.rs`
- **Lines 15–20:** `Frame::inner_margin(Margin::symmetric(8, 5))` provides insufficient 5px bottom clearance.
- **Lines 120–140:** Playhead indicator bar allocates rect without attaching `.on_hover_text("Playhead position indicator")`.

---

## Section 4: Step-by-Step Prioritized Remediation Plan

### Phase 1: Critical Containment & Collision Fixes (Target: +50.0 pts)
1. **Refactor Header Bar Horizontal Layout (`header_bar.rs`):**
   - Replace unconstrained `right_to_left` layout with structured two-column layout (`ui.columns(2)` or explicit left/right bounding rects).
   - Allocate `left_rect` `[0.0..320.0]` for Brand Title, `BASS SYNTH`, and `MONO` badges.
   - Allocate `right_rect` `[340.0..1280.0]` for `TOPOLOGY`, `QUALITY`, and `ZOOM` selectors with guaranteed minimum spacing.
2. **Expand Accurate Mode Scrim Scope (`souped_panel.rs` / `mod.rs`):**
   - Elevate the Accurate mode overlay scrim to wrap the entire Souped-up / Analog domain container (or coordinate cross-panel scrim state) so Output Clipper and Analog Character panels are consistently masked.

### Phase 2: Major Visual Balance & State Fixes (Target: +50.0 pts)
1. **Fix `SlideSwitch` Label Glyph Clipping (`switch.rs`):**
   - Expand label bounding rect height by 4px and sanitize string rendering to eliminate underline artifacts on `TOPOLOGY`.
2. **Align Volume Knob Unity Center & Attenuation (`knob.rs`):**
   - Add detent markers or align unity gain visual pointer with 12:00 center affordance.
   - Add disabled-state desaturation for active value arcs and readouts when parent hierarchy is disabled.
3. **Ensure Full Child Panel Interaction Disabling (`tweaks_panel.rs`, `output_clipper.rs`):**
   - Ensure accurate topology mode completely bypasses and dims tweaker parameters in synchrony with the engine drawer.

### Phase 3: Minor Formatting, Semantics & Polish (Target: +33.0 pts)
1. **Fix String Formatting & Spacing (`generator_panel.rs`):**
   - Change `"GEN SEED : "` to `"GEN SEED: "`.
   - Strip redundant `%` from label titles (`REST:`, `ACCENT:`, `SLIDE:`, `ACC BIAS:`, `MUTATION:`).
2. **Standardize Decimal Precision (`tweaks_panel.rs`):**
   - Format `DRIFT` percentage with integer precision (`format!("{:.0}%", ...)`).
3. **Improve Sequencer Container Margins & Feedback (`sequencer_panel.rs`):**
   - Increase bottom margin to `8px` (`Margin::symmetric(8, 8)`).
   - Add `.on_hover_text("Playhead position indicator")` to the playhead LED strip rect.
4. **Upgrade Diode Selector Affordance (`tweaks_panel.rs`):**
   - Implement two-option segmented `SlideSwitch` (`["SI", "GE"]`).

### Phase 4: Final Verification & Quality Gate Re-evaluation
- Execute `cargo test` to verify zero functional regressions.
- Capture updated 1280x720 visual snapshots across all 4 operational states (`default`, `tweaks_open`, `accurate_mode`, `running`).
- Re-run the 8 parallel visual QA inspectors to verify all 18 defects are resolved and score achieves **100.0 / 100.0** (`spec_compliance: true`).
