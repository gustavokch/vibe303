# GUI Frontend Refinement Plan

**Date:** 2026-08-14  
**Status:** Draft / Actionable  
**Target:** `plugin303` GUI frontend (`nih_plug_egui`)  
**Input Audit:** Visual QA findings (2026-08-14 synthesis)  
**Reference Specification:** `docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md`

---

## 1. QA Findings & Section Scores

Visual QA evaluation yielded the following section-by-section breakdown:

| Section | Alignment Score | Defect Summary | Spec Compliance Gate |
| :--- | :---: | :--- | :---: |
| **Classic Panel** | 94 / 100 | Left header displays `"TECHNEO"` in orange and top right displays `"CIRCUIT TOPOLOGY"` / `"TB-303 PLUGIN"`; typography/branding label positioning minor deviation. | **PASS** |
| **Souped Drawer** | 98 / 100 | None reported. | **PASS** |
| **Sequencer** | 100 / 100 | None reported. | **PASS** |

### Overall Weighted Score Calculation

Using the specified weight distribution (`0.35 * Classic` + `0.30 * Souped` + `0.35 * Sequencer`):

$$\text{Overall Score} = (0.35 \times 94) + (0.30 \times 98) + (0.35 \times 100) = 32.90 + 29.40 + 35.00 = 97.30$$

- **Overall Weighted Score:** **97.30 / 100**
- **Overall Spec Compliance Gate:** **PASS** (`true`)

---

## 2. Top Visual & Layout Defects

### Defect 1: Header Branding & Title Typography (Classic Panel Header)
- **Severity:** Minor
- **Section:** `classic_panel` / `header_bar`
- **Element:** Header Branding (`TECHNEO` / `CIRCUIT TOPOLOGY` / `TB-303 PLUGIN`)
- **Observed:** Left header renders `"TECHNEO"` in orange accent, top right displays `"CIRCUIT TOPOLOGY"` / `"TB-303 PLUGIN"`.
- **Expected:** Header branding layout and font hierarchy following exact mockup and design spec guidelines.
- **Root Cause / Context:** Header panel (`plugin303/src/gui/panels/header_bar.rs`) utilizes temporary color highlighting for `"TECHNEO"` text label which slightly deviates from standard silver-etched vintage chassis typography.

---

## 3. Actionable Refinement Plan

### Task 1: Refine Classic Panel Header Typography & Branding (`header_bar.rs`)
- **Target File:** `plugin303/src/gui/panels/header_bar.rs`
- **Action Items:**
  1. Audit header title label rendering and font weight.
  2. Harmonize `"TECHNEO"` title color token to match metallic chassis theme styling (`#d0d4d8` silver gradient / off-white) or calibrate orange accent tone to conform with section 5 theme tokens.
  3. Ensure right-hand side labels (`"CIRCUIT TOPOLOGY"`, `"TB-303 PLUGIN"`) maintain consistent vertical alignment and padding relative to the topology switch detents.
- **Verification:** Run visual QA loop to verify branding alignment score reaches ≥ 98.

### Task 2: Maintain & Preserve High Quality Gating (Drawer & Sequencer)
- **Target Files:** `plugin303/src/gui/panels/souped_panel.rs`, `plugin303/src/gui/panels/sequencer_panel.rs`
- **Action Items:**
  1. Retain 100% compliance on 16-step sequencer strip (playhead tracking, mini-keyboard popover, gate cycle styling).
  2. Ensure accurate mode 30% opacity dimming badge in souped drawer remains locked to spec detent logic.

---

## 4. Verification & Testing Protocol

1. **Compilation & Feature Matrix:**
   ```bash
   cargo check -p plugin303 --all-features
   cargo test -p plugin303 --all-features
   ```
2. **Visual Inspection:**
   Verify `header_bar.rs` changes under `nih_plug_egui` standalone runner or DAW plugin host.
