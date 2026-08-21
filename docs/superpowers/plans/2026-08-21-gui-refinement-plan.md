# GUI Frontend Refinement Plan

**Date:** 2026-08-21  
**Status:** Draft / Actionable  
**Target File:** `docs/superpowers/plans/2026-08-21-gui-refinement-plan.md`  
**Base Spec:** `docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md`  

---

## 1. Executive Summary & Quality Scorecard

Visual QA analysis was performed across all three core GUI sections: Classic 303 Front Panel, Souped-Up Engine Drawer, and 16-Step Hardware Sequencer Console.

### Overall Visual Quality Score: **78.15 / 100**
- **Overall Spec Compliance:** **FAILED (false)**
- **Weight Formula:** `(0.35 × Classic) + (0.30 × Souped) + (0.35 × Sequencer)`

| Section | Alignment / Quality Score | Spec Compliance | Defect Summary |
| :--- | :---: | :---: | :--- |
| **Classic 303 Panel** | **94** | `Pass` | 2 Minor (Detent label color token, ADV ENV vertical alignment) |
| **Souped-Up Drawer** | **85** | `FAIL` | 1 Major (`accurate.png` env var handling missing), 1 Minor (DRIFT/RECAL placement) |
| **16-Step Sequencer** | **65** | `FAIL` | 1 Critical (640px height clipping full lower strip), 1 Major (Running playhead LED step 4 dim), 1 Minor (Gate tile occlusion) |

---

## 2. Synthesized Visual QA Findings & Top Defects

### 2.1 Critical & Major Defects

1. **[CRITICAL] Sequencer Viewport Height & Controls Truncation**
   - **Section:** `sequencer`
   - **Element:** `16-Step Sequencer Strip Layout & Viewport Clipping`
   - **Observed:** Window height limit (640px) truncates the lower half of the 16-step sequencer strip. Only Playhead LEDs, step numbers, and ACC toggles are visible. SLD toggles, Gate tiles (NOTE/TIE/REST), Pitch readouts, and Octave nudge controls are cut off outside the window border.
   - **Action:** Increase GUI frame height (e.g. from 640px to 720px in `GUI_HEIGHT` constant) or adjust vertical margins and spacing across panels.

2. **[MAJOR] Circuit Topology Accurate Mode QA Snapshot Failure**
   - **Section:** `souped_drawer`
   - **Element:** `Circuit Topology Accurate Mode QA Snapshot (accurate.png)`
   - **Observed:** `target/gui-qa/accurate.png` is identical to `latest.png` (FUN mode) because `standalone.rs` does not read the `VIBE303_QA_STATE` environment variable to toggle `params.topology` to `CircuitTopology::Accurate`.
   - **Action:** Update `standalone.rs` to inspect `VIBE303_QA_STATE` and set `params.topology` accordingly when state is `"accurate"`.

3. **[MAJOR] Running Playhead LED Glow Sync**
   - **Section:** `sequencer`
   - **Element:** `Running Playhead LED Bar (Step 4)`
   - **Observed:** In `running.png` snapshot with active transport, step 4 playhead LED bar remains dim maroon (`#4d1e22`) instead of glowing neon coral (`#ff3355`) with a bloom halo.
   - **Action:** Fix sync between atomic `current_playing_step` in DSP `process_block()` and the UI rendering frame loop for 0-indexed step 3.

---

## 3. Actionable Refinement Plan

### Phase 1: Viewport Geometry & Sequencer Restructuring
- **Task 1.1: Expand Window Viewport Height & Layout Padding**
  - **File:** `plugin303/src/gui/mod.rs` & `plugin303/src/gui/panels/sequencer_panel.rs`
  - Update `GUI_HEIGHT` constant from `640.0` to `720.0` (or `700.0`).
  - Adjust panel container vertical paddings to ensure all step rows (Playhead LED, ACC, SLD, Gate Tile, Pitch Readout, Octave Nudge) fit comfortably within the primary viewport without vertical scrollbars.
- **Task 1.2: Validate & Expose Gate Tile Colors**
  - **File:** `plugin303/src/gui/theme.rs` & `plugin303/src/gui/widgets/step_button.rs`
  - Ensure Gate tiles render unobscured with exact color tokens: `NOTE` (`#00e5ff`), `TIE` (`#ffd700`), and `REST` (`#333a42`).
- **Task 1.3: Audio Transport Playhead Tracking Fix**
  - **File:** `plugin303/src/dsp/sequencer.rs` & `plugin303/src/gui/panels/sequencer_panel.rs`
  - Verify `current_playing_step` atomic store/load logic so step index changes trigger immediate UI update and neon coral (`#ff3355`) LED bloom on the active step.

### Phase 2: Souped-Up Engine Drawer & Topology Dimming
- **Task 2.1: Add QA State Environment Var Handling to Standalone**
  - **File:** `plugin303/src/standalone.rs`
  - Read `std::env::var("VIBE303_QA_STATE")`. If value matches `"accurate"`, initialize or force `params.topology` to `CircuitTopology::Accurate`.
- **Task 2.2: Harmonize Analog Wear Dials Placement**
  - **File:** `plugin303/src/gui/panels/souped_panel.rs` & spec update
  - Formally document the `TWEAKS` subpanel location for `DRIFT` and `RECAL` knobs or incorporate them within the main `souped_panel.rs` drawer grid.

### Phase 3: Classic Panel Visual Polish
- **Task 3.1: Fidelity Tier Active Detent Text Color Token**
  - **File:** `plugin303/src/gui/widgets/switch.rs` & `theme.rs`
  - Update `Fidelity Tier` active detent label color token from standard orange (`#ff8800`) to the red accent family (`#ff3355` / red carriage token) as per spec section 3.2.
- **Task 3.2: ADV ENV Toggle Alignment**
  - **File:** `plugin303/src/gui/panels/classic_panel.rs`
  - Adjust vertical layout offsets so the ADV ENV toggle aligns flush with the ADSR envelope fader group header baseline.

---

## 4. Verification Protocol

1. **Visual QA Snapshot Re-capture:**
   - Execute `cargo xtask gui-qa` to capture updated `latest.png`, `accurate.png`, and `running.png`.
2. **Automated Visual Gate Checks:**
   - Re-run visual QA evaluation to confirm `overall_score >= 90` and `spec_compliance == true`.
3. **Build & Test Verification:**
   - Run `cargo test -p plugin303 --all-features` to ensure zero regression in DSP or GUI tests.
