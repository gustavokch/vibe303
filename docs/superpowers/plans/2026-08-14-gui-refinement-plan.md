# GUI Visual Refinement Plan (`vibe303`)

**Date:** 2026-08-14  
**Target Spec:** `docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md`  
**Overall Visual QA Weighted Score:** 88.70 / 100  
**Overall Spec Compliance:** Non-Compliant (`false`)  

---

## 1. Score Breakdown

- **Classic 303 Front Panel:** 88.00 / 100 (Weight: 35%, Weighted Contribution: 30.80) — `spec_compliance: false`
- **Souped-Up Drawer:** 88.00 / 100 (Weight: 30%, Weighted Contribution: 26.40) — `spec_compliance: true`
- **Sequencer Console:** 90.00 / 100 (Weight: 35%, Weighted Contribution: 31.50) — `spec_compliance: true`
- **Calculated Total:** 88.70 / 100

---

## 2. Refinement Tasks by Section

### 2.1 Classic 303 Front Panel Refinement Tasks

1. **Move RESO COMP Knob to Souped-Up Engine Drawer** `[Severity: Minor, Category: geometry]`
   - **Observed:** The primary classic knob row renders 8 rotary dials because RESO COMP is added directly into the front row.
   - **Expected:** The Classic 303 front panel contains the 7 canonical vintage rotary dials (Tuning, Cutoff, Resonance, Env Mod, Decay, Accent, Volume).
   - **Actionable Fix:** Remove RESO COMP from `plugin303/src/gui/panels/classic_panel.rs` and relocate it into the filter group inside `plugin303/src/gui/panels/souped_panel.rs`.

2. **Fidelity Tier Switch Accent Color** `[Severity: Minor, Category: color]`
   - **Observed:** The Fidelity Tier slide switch uses the default silver metallic carriage identical to the Waveform switch.
   - **Expected:** The Fidelity Tier slide switch uses a red sliding carriage cap to match the design specification mockup.
   - **Actionable Fix:** Add an optional `accent_color: Option<Color32>` parameter or builder field to `SlideSwitch` in `plugin303/src/gui/widgets/switch.rs`, and pass red accent styling for the tier switch in `classic_panel.rs`.

3. **Chassis Header Ventilation Grills** `[Severity: Minor, Category: missing_feature]`
   - **Observed:** The top header bar renders text labels and the topology switch without procedural ventilation grill slots.
   - **Expected:** The top header chassis renders procedural vertical cooling ventilation grill slots flanking the center controls.
   - **Actionable Fix:** Implement procedural vertical ventilation slots in `plugin303/src/gui/panels/header_bar.rs` using `painter.rect_filled()`.

4. **Knob Parameter Labels Bold Typography** `[Severity: Minor, Category: typography]`
   - **Observed:** Parameter labels above rotary dials render with standard font weight.
   - **Expected:** Parameter labels above rotary dials render in bold uppercase typography for high contrast.
   - **Actionable Fix:** Update font styling for dial labels in `plugin303/src/gui/widgets/knob.rs` and `classic_panel.rs` to use bold text styling tokens.

---

### 2.2 Souped-Up Engine Drawer Refinement Tasks

1. **Accurate Mode Ghosted Controls Alpha Layer** `[Severity: Minor, Category: geometry]`
   - **Observed:** The drawer completely hides extended controls in Accurate mode and renders only the overlay box with the quick-switch button.
   - **Expected:** The drawer renders extended controls in a disabled state at 30 percent opacity, with the quick-switch overlay banner rendered on top.
   - **Actionable Fix:** Update `plugin303/src/gui/panels/souped_panel.rs` to render the controls at 30 percent opacity before painting the overlay banner when `CircuitTopology::Accurate` is active.

2. **Standardize VEG ADSR Fader Fill Bar Color** `[Severity: Minor, Category: color]`
   - **Observed:** VEG faders render an amber glow color while MEG faders render an orange glow color.
   - **Expected:** VEG faders render glowing orange fill bars matching the drawer theme, or use a consistent defined secondary envelope token.
   - **Actionable Fix:** Align the VEG fader glow color token in `plugin303/src/gui/widgets/fader.rs` with `theme::ORANGE_ACCENT` or document the intentional amber differentiator in `theme.rs`.

---

### 2.3 Sequencer Console Refinement Tasks

1. **StepGate Rest Color Token Consistency** `[Severity: Minor, Category: color]`
   - **Observed:** `StepGate::Rest` uses hardcoded `Color32::from_rgb(0x2c, 0x32, 0x3c)` in `step_button.rs`.
   - **Expected:** `StepGate::Rest` references the centralized `theme::GATE_REST_COLOR` design token.
   - **Actionable Fix:** Replace the hardcoded `Color32` with `theme::GATE_REST_COLOR` in `plugin303/src/gui/widgets/step_button.rs`.

2. **Accent and Slide Button Typography Constraints** `[Severity: Minor, Category: typography]`
   - **Observed:** Buttons display single-letter labels `A` and `S` due to the 22px width constraint.
   - **Expected:** Buttons display full `ACC` and `SLD` label text or explicit hover tooltips.
   - **Actionable Fix:** Optimize label font size to render `ACC` and `SLD`, or retain `A` and `S` while attaching explicit egui tooltips describing `Accent` and `Slide`.

3. **Multi-Octave Piano Roll Interface Documentation** `[Severity: Minor, Category: missing_feature]`
   - **Observed:** Pitch selection opens a 5-octave vertical scrollable piano roll popup rather than a 13-key horizontal mini-keyboard.
   - **Expected:** 13-key horizontal MiniKeyboard popover or documented upgraded multi-octave piano roll.
   - **Actionable Fix:** Retain the 5-octave scrollable piano roll as an enhanced UI feature, and update `docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md` to reflect the multi-octave roll.

---

## 3. Verification & Quality Gates

1. **Cargo Build:** `cargo check -p plugin303 --all-features`
2. **Audio Path Allocation Test:** `cargo test -p plugin303 --test no_alloc`
3. **Visual Quality Gate Target:** Achieve overall weighted score >= 90 / 100 with `spec_compliance: true`.
