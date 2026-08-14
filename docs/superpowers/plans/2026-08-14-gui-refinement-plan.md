# GUI Visual Refinement Plan (`vibe303`)

**Date:** 2026-08-14  
**Target Spec:** `docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md`  
**Overall Visual QA Weighted Score:** 59.95 / 100  
**Overall Spec Compliance:** Non-Compliant  

---

## 1. Score Breakdown

- **Classic Front Panel:** 72 / 100 (Weight: 35%, Weighted Contribution: 25.20)
- **Souped-Up Drawer:** 40 / 100 (Weight: 30%, Weighted Contribution: 12.00)
- **Sequencer Console:** 65 / 100 (Weight: 35%, Weighted Contribution: 22.75)
- **Calculated Total:** 59.95 / 100

---

## 2. Refinement Tasks by Section

### 2.1 Classic 303 Front Panel Refinement Plan

1. **Knob Bezel Radial Tick Marks (`Knob303` Widget)**
   - **Defect:** Bezel renders as a simple solid ring without radial tick marks.
   - **Fix:** Add a procedural painter loop in `plugin303/src/gui/widgets/knob.rs` to render tick marks along a 270-degree sweep (135 degrees to 405 degrees).

2. **Classic Dial Row Layout & Labeling**
   - **Defect:** Rendered panel contains 6 knobs instead of 7 (VOLUME moved to header bar), and RESONANCE label displays as RESO.
   - **Fix:** Retain 7 rotary dials in `classic_panel.rs` (TUNING, CUTOFF, RESONANCE, ENV MOD, DECAY, ACCENT, VOLUME) and change label text from "RESO" to "RESONANCE".

3. **Fidelity Tier Slide Switch Visual Accent**
   - **Defect:** Slider thumb uses generic silver tone instead of brushed red fill.
   - **Fix:** Pass custom accent color parameter to `SlideSwitch` in `classic_panel.rs` for Tier selector thumb.

4. **Waveform Switch Option Labels**
   - **Defect:** Option labels show `SAW`, `SQR`, `SWEEP`.
   - **Fix:** Update string array in `classic_panel.rs` to display `SAW`, `SQUARE`, `SWEEP`.

5. **Header Bar Styling & Typography**
   - **Defect:** Header branding font weight and amber glow contrast do not match spec tokens.
   - **Fix:** Apply theme typography tokens and amber glow highlights to `header_bar.rs`.

---

### 2.2 Souped-Up Engine Drawer Refinement Plan

1. **MEG and VEG ADSR Vertical Mini-Faders**
   - **Defect:** Fader controls for MEG and VEG envelopes are missing.
   - **Fix:** Implement `MiniFader` widget with glowing orange fill bars in `plugin303/src/gui/widgets/fader.rs`. Add MEG (Attack, Decay, Sustain, Release) and VEG (Attack, Decay, Sustain, Release) fader groups to `souped_panel.rs`.

2. **Accurate Mode Dimming Badge & Quick-Switch Banner**
   - **Defect:** Uses standard egui disabled state with simple text label. Missing 30% translucent overlay container and quick-switch banner.
   - **Fix:** Draw translucent frame (30% opacity) over `souped_panel.rs` when `params.topology` is `Accurate`. Render banner with message *"Accurate Mode Active (Souped-up Engine Bypassed)"* and a quick-switch button to enable `Fun` mode.

3. **Drawer Layout Grouping**
   - **Defect:** Dials align in horizontal wrapping rows without visual envelope fader sections.
   - **Fix:** Group `souped_panel.rs` controls into distinct labeled sub-frames: Oscillator/Drive, Filter/Drift, MEG ADSR, and VEG ADSR.

---

### 2.3 16-Step Hardware Sequencer Refinement Plan

1. **Step Grid Column Width & Row Wrapping**
   - **Defect:** Step columns wrap after step 11 onto a second row, vertically clipping steps 12-16.
   - **Fix:** In `sequencer_panel.rs`, reduce step column `min_width` from 54px to 42px. Adjust item spacing to keep all 16 steps side-by-side on one row.

2. **ACC and SLD Procedural LED Indicators**
   - **Defect:** Buttons use solid background fill without procedural LED dots or halos.
   - **Fix:** Integrate `led.rs` widget inside `step_button.rs`. Draw discrete LED dot with amber (#ffaa00) bloom halo for ACC and cyan (#00e5ff) bloom halo for SLD.

3. **Pitch & Octave Controls Vertical Footprint**
   - **Defect:** High vertical height per step column contributes to container clipping.
   - **Fix:** Make octave nudge buttons inline with pitch readout in `step_button.rs` to reduce total step column height.

4. **REST Gate Badge Visual Contrast**
   - **Defect:** Low text contrast on dark slate REST badge.
   - **Fix:** Update REST badge color token to high-contrast slate text palette (#38404c background, light gray text).

---

## 3. Infrastructure & Capture Fixes

1. **Standalone Headless Screen Capture**
   - **Defect:** `target/gui-qa/latest.png` captures full IDE window because standalone window handle was not detected.
   - **Fix:** Update standalone runner script window registration logic to ensure headless capture tools detect the baseview GUI window handle directly.

---

## 4. Verification Checkpoints

1. **Visual QA Execution:** Run `cargo xtask gui-qa` to verify rendered components match spec `docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md`.
2. **Audio Safety:** Run `cargo test -p plugin303 --test no_alloc` to verify zero allocations on audio thread.
3. **Spec Alignment Goal:** Achieve overall weighted visual QA score >= 90 / 100 with spec_compliance set to true.
