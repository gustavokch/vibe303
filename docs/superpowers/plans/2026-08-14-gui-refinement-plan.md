# GUI Visual QA Refinement Plan

**Date:** 2026-08-21  
**Spec Reference:** `docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md`  
**Input Findings:** 5 findings across 3 layout sections (`classic_panel`, `souped_drawer`, `sequencer`).

---

## 1. Score Synthesis & Overall Alignment

The visual QA scores across each section are weighted as specified (35% Classic Panel, 30% Souped Drawer, 35% Sequencer):

| Section | Score | Weight | Weighted Score |
| :--- | :---: | :---: | :---: |
| **Classic Panel** | 85 | 0.35 | 29.75 |
| **Souped Drawer** | 92 | 0.30 | 27.60 |
| **Sequencer Panel** | 95 | 0.35 | 33.25 |
| **Overall Weighted Score** | — | — | **90.60 / 100** |

All sections pass visual quality gate (`spec_compliance: true`), with overall score meeting the standard threshold (>80.0).

---

## 2. Synthesized Visual QA Findings & Accepted Deviations

### 2.1 Classic Panel (`classic_panel`)
1. **Header Bar Branding Text (Minor / Informational)**
   - *Observed:* Header displays `"TECHNEO"` logotype and `"TB-303 PLUGIN"` text.
   - *Expected:* Branding should reflect standard TB-303 or `vibe303` design specifications.
   - *Resolution:* Update header bar branding or maintain project identity consistent with spec.
2. **Waveform and Tier Selector Placement (Minor / Accepted Deviation)**
   - *Observed:* Waveform and Fidelity Tier selectors format horizontally right of the 7 vintage knobs.
   - *Expected:* Placed on Classic Panel according to original layout mockup.
   - *Resolution:* Keep as accepted layout deviation per 2026-08-19 design spec updates to preserve single-row layout without scrollbars.

### 2.2 Souped-Up Engine Drawer (`souped_drawer`)
3. **MEG / VEG ADSR Faders Placement (Minor / Accepted Deviation)**
   - *Observed:* MEG ADSR and VEG ADSR vertical mini-faders are positioned in the top classic panel adjacent to ADV ENV checkbox.
   - *Expected:* MEG and VEG ADSR faders located inside Souped-Up Engine Drawer section per Section 1.2 and 3.2.
   - *Resolution:* Document explicitly as an accepted top-panel positioning deviation or update layout documentation.
4. **Accurate Topology Test Screenshot Missing (Minor / Action Item)**
   - *Observed:* `target/gui-qa/accurate.png` is missing (only `latest.png` showing `Fun` mode is present).
   - *Expected:* `target/gui-qa/accurate.png` present to evaluate ~30% ghosting opacity, pixel-sampled MEG fill opacity, legible dimming badge, and quick-switch button.
   - *Resolution:* Generate `target/gui-qa/accurate.png` with Circuit Topology set to `Accurate` during GUI snapshot verification runs.

### 2.3 Sequencer Panel (`sequencer`)
5. **Gate Rest Button Background Color (Minor / Fix Required)**
   - *Observed:* Rest state gate button background renders with darker contrast (`#20242c`) than Dim Slate (`#333a42`).
   - *Expected:* Dim Slate (`#333a42`) per Section 3.3 item 4 of `SPEC.md`.
   - *Resolution:* Update `theme::GATE_REST_COLOR` in `plugin303/src/gui/theme.rs` to `Color32::from_rgb(0x33, 0x3a, 0x42)`. Note: Verified `theme.rs` already contains `pub const GATE_REST_COLOR: Color32 = Color32::from_rgb(0x33, 0x3a, 0x42);`. Ensure caller widgets in `step_button.rs` use `theme::GATE_REST_COLOR` directly rather than overriding with `#20242c`.

---

## 3. Actionable Refinement Plan

### Action Step 1: Enforce `GATE_REST_COLOR` in Step Button Widget
- **Target File:** `plugin303/src/gui/widgets/step_button.rs` (or `sequencer_panel.rs`)
- **Task:** Verify that rest state background for sequencer gate tiles uses `crate::gui::theme::GATE_REST_COLOR` (`#333a42`) rather than hardcoded `#20242c`.

### Action Step 2: Add Snapshot Artifact Generation for Accurate Mode
- **Target File:** `plugin303/tests/gui_qa.rs` or xtask screenshot runner
- **Task:** Ensure snapshot test script generates both `latest.png` (Fun mode) and `target/gui-qa/accurate.png` (Accurate mode) for visual QA automation.

### Action Step 3: Document Accepted Layout Deviations
- **Target File:** `docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md`
- **Task:** Explicitly record top-panel positioning of MEG/VEG ADSR faders alongside the 2026-08-19 selector positioning notes.
