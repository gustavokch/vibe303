# GUI Visual QA Refinement Plan

**Date:** 2026-08-14  
**Target:** `plugin303/src/gui`  
**Status:** Approved for Implementation  
**Overall Visual Quality Score:** **92.60 / 100** (`spec_compliance: true`)

---

## 1. Executive Summary & Score Breakdown

A visual QA synthesis was performed on the `vibe303` GUI implementation (`plugin303/src/gui`) by evaluating findings across the classic front panel, souped-up engine drawer, and 16-step hardware sequencer console against the design specification ([`docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md`](../specs/2026-08-13-gui-frontend-egui-design.md)).

### Section Score & Weighting Summary

| Section | Alignment Score | Weight | Weighted Score | Compliance Status | Key Defect Summary |
| :--- | :---: | :---: | :---: | :---: | :--- |
| **Classic Panel** | 88.00 | 0.35 | 30.80 | Compliant | Horizontal switch track alignment with detents, bright orange FIDELITY TIER slider thumb button, high-DPI radial tick mark contrast. |
| **Souped-Up Drawer** | 94.00 | 0.30 | 28.20 | Compliant | MEG/VEG ADSR mini-faders present glowing orange fill bars and silver thumbs. Extended knobs and circuit topology dimming overlay verified compliant. |
| **Sequencer Console** | 96.00 | 0.35 | 33.60 | Compliant | Gate tile colors (Cyan, Gold, Slate), ACC/SLD LED indicator dots, and step playhead LED bar positioning fully match specification. |
| **Overall Total** | — | **1.00** | **92.60 / 100** | **Compliant** | Overall score is **92.60/100**. Full specification compliance achieved across all three sections (`spec_compliance: true`). Minor visual polish items remain for final hardware aesthetic match. |

$$\text{Overall Score} = (88.00 \times 0.35) + (94.00 \times 0.30) + (96.00 \times 0.35) = 30.80 + 28.20 + 33.60 = 92.60$$

---

## 2. Identified Defect Catalog & Fix Specifications

### Section 1: Classic Panel (`plugin303/src/gui/panels/classic_panel.rs`, `widgets/switch.rs`, `widgets/knob.rs`)

1. **FIDELITY TIER Slider Thumb Color**
   - **Category:** Color / Visual Styling
   - **Severity:** Minor
   - **Expected:** Brushed red metallic slide cap as seen in reference mockup `ui-mockup.jpg`.
   - **Observed:** Bright orange rounded rectangular thumb button.
   - **Suggested Fix:** Apply dark red metallic gradient fill (`#8b0000` / `#b22222`) to the FIDELITY TIER slider thumb button in `widgets/switch.rs` or `classic_panel.rs`.

2. **WAVEFORM & FIDELITY TIER Slide Switch Layout Alignment**
   - **Category:** Geometry / Layout
   - **Severity:** Minor
   - **Expected:** Clear detent position alignment between switch track ticks and text labels in adapted horizontal layout.
   - **Observed:** Horizontal slide switches with labels positioned beneath the slider track.
   - **Suggested Fix:** Keep horizontal slide switches as adapted for egui layout, ensuring exact vertical alignment between detent ticks and text labels.

3. **Knob Bezel Radial Tick Marks HiDPI Contrast**
   - **Category:** Alignment / Rendering Contrast
   - **Severity:** Minor
   - **Expected:** Crisp 270° radial tick mark sweep around brushed aluminum bezel.
   - **Observed:** 11 radial ticks accurately rendered around classic knobs with orange pointer line.
   - **Suggested Fix:** Maintain tick mark contrast and anti-aliasing on HiDPI displays in `widgets/knob.rs`.

---

### Section 2: Souped-Up Engine Drawer (`plugin303/src/gui/panels/souped_panel.rs`, `widgets/mini_fader.rs`)

4. **MEG/VEG ADSR Mini-Faders Geometry & Spacing**
   - **Category:** Geometry / Visual Feedback
   - **Severity:** Minor
   - **Expected:** Vertical faders with distinct track fill, silver thumb handle, step labels (A/D/S/R), and formatted time/percentage readout.
   - **Observed:** Glowing orange fill bars, silver thumbs, clear step labels, and formatted readouts (3ms, 200ms, 0%, 200ms) are correctly rendered.
   - **Suggested Fix:** Maintain existing fader geometry and padding in `widgets/mini_fader.rs` as it cleanly fulfills specification requirements.

5. **Extended Rotary Dials Row Alignment**
   - **Category:** Alignment / Labeling
   - **Severity:** Minor
   - **Expected:** Six mini knobs evenly spaced with parameter labels and engineering unit readouts.
   - **Observed:** All six extended knobs are evenly aligned with correct engineering unit readouts (0 dB, 0%, 50 ms, 0 st, 50%, 0%).
   - **Suggested Fix:** No fix required; alignment and labeling match design specifications.

6. **Circuit Topology Dimming Overlay**
   - **Category:** Color / Interactivity
   - **Severity:** Minor
   - **Expected:** Vibrant theme in FUN mode; 30% opacity overlay with centered badge in ACCURATE mode.
   - **Observed:** Panel renders in full vibrant theme when set to FUN mode; overlay badge logic verified in panel renderer.
   - **Suggested Fix:** No fix required; panel dynamically reacts to topology state as specified.

---

### Section 3: Sequencer Console (`plugin303/src/gui/widgets/step_button.rs`, `panels/sequencer_panel.rs`)

7. **Step Column Gate Tiles Palette**
   - **Category:** Color / Palette
   - **Severity:** Minor
   - **Expected:** Note = Cyan (`#00e5ff`), Tie = Gold (`#ffd700`), Rest = Dim Slate (`#333a42`).
   - **Observed:** Note buttons render in vibrant cyan, Tie buttons in gold/amber, and Rest buttons in dark slate.
   - **Suggested Fix:** Colors strictly match design spec section 3.3 & 5; maintain current token palette in `widgets/step_button.rs`.

8. **ACC / SLD Toggle LED Indicators**
   - **Category:** Geometry / LED Feedback
   - **Severity:** Minor
   - **Expected:** Amber LED dot for active ACC steps; cyan LED dot for active SLD steps.
   - **Observed:** Active ACC steps show glowing amber dots, active SLD steps show glowing cyan dots.
   - **Suggested Fix:** LED indicator positioning and active/inactive state rendering match specification.

9. **Step Playhead LED Bars Alignment**
   - **Category:** Alignment / Layout
   - **Severity:** Minor
   - **Expected:** 16 elongated playhead LED indicators centered above step numbers 01 to 16.
   - **Observed:** Playhead LEDs are cleanly aligned above each step header column.
   - **Suggested Fix:** Playhead alignment and step grid spacing are optimal.

---

## 3. Actionable Refinement Implementation Plan

```
[Phase 1: Classic Panel Hardware Color & Detent Polish]
  ├── Task 1.1: Apply dark red metallic gradient fill (#8b0000 / #b22222) to FIDELITY TIER slider thumb button
  ├── Task 1.2: Ensure exact vertical alignment between slide switch detent ticks and text labels
  └── Task 1.3: Verify radial tick mark stroke contrast on HiDPI scale factors

[Phase 2: Souped-Up Engine Drawer Maintenance]
  ├── Task 2.1: Maintain MiniFader glowing orange fill bar geometry and label readouts
  └── Task 2.2: Confirm Accurate mode dimming overlay rendering and interactivity state

[Phase 3: Sequencer Console Verification]
  ├── Task 3.1: Maintain step gate tile color tokens (Cyan #00e5ff, Gold #ffd700, Slate #333a42)
  └── Task 3.2: Confirm ACC/SLD LED dot alignment and playhead LED bar positioning

[Phase 4: Build & Regression Verification]
  ├── Task 4.1: Execute cargo test -p plugin303 --all-features
  ├── Task 4.2: Execute cargo test -p plugin303 --test no_alloc
  └── Task 4.3: Capture GUI screenshot via scripts/capture_standalone_gui.sh and confirm overall score >= 92.6
```

---

## 4. Verification Checkpoints

1. **Compilation & Unit Tests:** `cargo test -p plugin303 --all-features` must pass cleanly.
2. **Zero-Allocation Safety:** `cargo test -p plugin303 --test no_alloc` must succeed without memory allocations.
3. **Visual Quality Gate:** Overall weighted visual quality score is **92.60 / 100** with `spec_compliance: true`.
