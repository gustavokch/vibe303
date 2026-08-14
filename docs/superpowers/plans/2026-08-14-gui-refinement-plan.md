# GUI Visual Refinement Plan (`vibe303`)

**Date:** 2026-08-14  
**Target Spec:** `docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md`  
**Overall Visual QA Weighted Score:** 40.80 / 100  
**Overall Spec Compliance:** Non-Compliant (`false`)  

---

## 1. Score Breakdown

- **Classic 303 Front Panel:** 78 / 100 (Weight: 35%, Weighted Contribution: 27.30)
- **Souped-Up Drawer:** 45 / 100 (Weight: 30%, Weighted Contribution: 13.50)
- **Sequencer Console:** 0 / 100 (Null / Unevaluated, Weight: 35%, Weighted Contribution: 0.00)
- **Calculated Total:** 40.80 / 100

---

## 2. Refinement Tasks by Section

### 2.1 Classic 303 Front Panel Refinement Plan

1. **Knob303 Radial Tick Marks** `[Severity: Major, Category: missing_feature]`
   - **Observed:** `Knob303` widget in `knob.rs` paints outer bezel circles but omits radial tick mark line segments along the arc sweep.
   - **Expected:** Outer bezel ring with radial tick marks spanning a 270-degree sweep (from 135 to 405 degrees / 7 o'clock to 5 o'clock).
   - **Actionable Fix:** Implement radial tick mark line segment rendering loop in `plugin303/src/gui/widgets/knob.rs` calculating tick coordinates along the 270-degree arc (135° to 405°).

2. **Knob Parameter Labels Typography** `[Severity: Minor, Category: typography]`
   - **Observed:** Labels above knobs are drawn using standard non-bold proportional font (`FontId::new(10.0, FontFamily::Proportional)`).
   - **Expected:** Parameter label above each dial rendered in bold uppercase typography.
   - **Actionable Fix:** Update label font style in `plugin303/src/gui/widgets/knob.rs` or `classic_panel.rs` to use bold uppercase typography token.

3. **SlideSwitch Option Labels & Thumb Carriage** `[Severity: Minor, Category: geometry]`
   - **Observed:** Option text is drawn inside the narrow moving thumb rectangle, causing label crowding and text truncation.
   - **Expected:** Recessed track with knurled silver sliding carriage and clear detent labels along the track.
   - **Actionable Fix:** Position option labels along switch track detents outside the thumb in `plugin303/src/gui/widgets/switch.rs` and update thumb rendering to knurled silver styling.

4. **Classic 303 Panel Knob Layout** `[Severity: Minor, Category: alignment]`
   - **Observed:** Classic panel row contains 6 knobs (Tuning, Cutoff, Reso, Env Mod, Decay, Accent) while Volume is positioned in the header bar.
   - **Expected:** Classic 303 Front Panel contains 7 vintage rotary dials (Tuning, Cutoff, Resonance, Env Mod, Decay, Accent, Volume).
   - **Actionable Fix:** Adjust panel layout in `plugin303/src/gui/panels/classic_panel.rs` to include all 7 vintage rotary dials in classic alignment.

---

### 2.2 Souped-Up Engine Drawer Refinement Plan

1. **MEG & VEG ADSR Faders Implementation** `[Severity: Critical, Category: missing_feature]`
   - **Observed:** No MEG or VEG ADSR faders are rendered in `souped_panel.rs` or visible in the drawer panel screenshot.
   - **Expected:** Vertical mini-fader tracks with glowing orange fill bars for MEG (Attack, Decay, Sustain, Release) and VEG (Attack, Decay, Sustain, Release) controls in the Souped-Up Engine Drawer (Sections 1.2 & 3.2).
   - **Actionable Fix:** Implement vertical mini-fader widgets in `plugin303/src/gui/widgets/switch.rs` or a dedicated `fader.rs`, and render MEG (A, D, S, R) and VEG (A, D, S, R) fader groups inside `plugin303/src/gui/panels/souped_panel.rs`.

2. **Accurate Mode Dimming Badge & Overlay** `[Severity: Major, Category: missing_feature]`
   - **Observed:** Only a small amber text label `(Bypassed in Accurate Mode)` is rendered in the panel header. Lacks 30% opacity dimming overlay and quick-switch button to toggle to Fun mode.
   - **Expected:** When `CircuitTopology::Accurate` is active, panel renders at 30% opacity with an overlay banner *"Accurate Mode Active (Souped-up Engine Bypassed)"* and a quick-switch button to enable `Fun` mode (Section 4.1).
   - **Actionable Fix:** Add a semi-transparent overlay layer (30% opacity) over `souped_panel.rs` when `Accurate` topology is active, containing the banner frame and quick-switch button that sets `params.topology` to `CircuitTopology::Fun`.

3. **Souped-Up Engine Drawer Panel Bounds** `[Severity: Major, Category: geometry]`
   - **Observed:** Panel text header is truncated at the right edge (`SOUPED-UP ENGINE DRAWER (Bypassed...`), and layout cuts off at the right frame border.
   - **Expected:** The Souped-Up Engine Drawer panel should fit cleanly within the top-right console area with fully visible title text and proper margin padding matching the 1080x640 layout.
   - **Actionable Fix:** Adjust layout grid in `plugin303/src/gui/mod.rs` and width constraints in `souped_panel.rs` so the drawer spans its allotted width without truncating text or controls.

4. **Extended Dials Layout Sub-Groups** `[Severity: Minor, Category: alignment]`
   - **Observed:** Dials are arranged horizontally across two rows with tight spacing, but lack sub-group visual section headers or panel dividers separating Osc, Filter, and Wear controls.
   - **Expected:** Extended dials (OSC1 DET, OSC2 DET, OSC2 MIX, OSC1 PW, SUB VOL, DRIVE, GLIDE, DRIFT, AGE) neatly aligned in visual sub-groups with clear section headers.
   - **Actionable Fix:** Add section sub-headers (OSC, DRIVE, ANALOG WEAR) above knob groups in `souped_panel.rs` for improved visual hierarchy.

---

### 2.3 Sequencer Console Refinement Plan

1. **Sequencer Section Visual Audit** `[Severity: Major, Category: missing_feature]`
   - **Observed:** Sequencer section analysis returned `null` in current visual QA pass.
   - **Expected:** Full 16-step sequencer strip inspection including playhead tracking, gate states (Note/Tie/Rest), ACC/SLD toggles, and pitch display.
   - **Actionable Fix:** Re-run visual inspection on `sequencer_panel.rs` to generate baseline findings and score.

---

## 3. Verification Checkpoints

1. **Cargo Build & Check:** `cargo check -p plugin303 --all-features`
2. **Audio Path Allocation Gate:** `cargo test -p plugin303 --test no_alloc`
3. **Visual Quality Target:** Achieve overall weighted score >= 90 / 100 with `spec_compliance: true`.
