# Comprehensive GUI Visual QA Workflow Specification

**Date:** 2026-08-21  
**Status:** Approved  
**Topic:** Redesign of `plugin303` GUI Automated Visual QA Workflow & Adversarial Inspection Pipeline  
**Target:** macOS Standalone Binary + Dynamic Image Slicing + Claude Multi-Agent Workflow Engine  

---

## 1. Overview & Problem Statement

The initial visual QA workflow evaluated GUI screenshots using broad domain prompts and uncalibrated scores, returning a passing score of **98.15 / 100** while 8 critical categories of visual defects were present in the interface:
1. **Right-edge clipping:** Four separate control groups (VEG ADSR faders, QUALITY slider track, SEED hex readout, JIT checkbox) cut off by the right window border.
2. **ACCURATE mode layout breakage:** Large dead black band at the bottom (~10% window height), squished middle engine drawer, floating unintegrated overlay box, uneven element ghosting, and low-contrast subtitle text.
3. **Inconsistent selection/accent colors & units:** Selected LOW quality option rendered in red/pink while all other selectors used orange; inconsistent number formatting ("0%" vs "0.00", "01" vs "16"); potentiometer resistance values (47kΩ) instead of perceptual units.
4. **Knob pointer vs. value mismatch:** Volume knob pointer positioned at 4–5 o'clock for `+0.0 dB` default value while other centered dials point straight up at 12 o'clock.
5. **Containment and layout issues in main panel:** MEG ADSR and VEG ADSR sub-panels overlapping main panel borders and spilling past the edge; ADV ENV checkbox floating isolated in empty space.
6. **Missing playback feedback:** Running sequencer state indistinguishable from stopped state except for button label (no active playhead indicator, static bar clusters).
7. **Sequencer row details:** Bottom octave arrow rows touching window boundary without padding; unexplained dark red dashes clashing with ACC dots; strike-through style on "GE / JFET" reading as disabled text rather than toggle.
8. **Minor/branding inconsistencies:** Inconsistent naming ("TB303" vs "TECHNEO" vs "TB-303 PLUGIN"), waveform icon rendering as generic hamburger glyph, disconnected dual seed fields.

This specification defines a comprehensive, attentive, and adversarial Visual QA pipeline designed to detect these defect classes with zero false passes.

---

## 2. Architecture & Pipeline Flow

The workflow is orchestrated via `.claude/workflows/gui-visual-qa-loop.js` and executes across four distinct phases:

```
                  +----------------------------------------------+
                  | Phase 1: Standalone Capture & Dynamic Crop   |
                  | - Compile & run standalone binary            |
                  | - Capture states: default, accurate, running |
                  | - Dynamically parse GUI geometry from Rust   |
                  | - Generate 5 domain + 2 margin + 1 diff crop |
                  +-----------------------+----------------------+
                                          |
                                          v
                  +----------------------------------------------+
                  | Phase 2: Parallel Dual-Matrix Inspection     |
                  |                                              |
                  | [Group A: 5 Domain Inspection Agents]        |
                  | 1. Header Bar Agent                          |
                  | 2. Classic Panel Agent                       |
                  | 3. Souped-Up Drawer Agent                    |
                  | 4. Clipper & Tweaks Agent                    |
                  | 5. 16-Step Sequencer Agent                   |
                  |                                              |
                  | [Group B: 3 Adversarial Defect Hunters]      |
                  | 6. Edge Bleed & Containment Inspector        |
                  | 7. Multi-State Structural Diff Inspector     |
                  | 8. Affordance, Semantics & Units Inspector   |
                  +-----------------------+----------------------+
                                          |
                                          v
                  +----------------------------------------------+
                  | Phase 3: Deductive Scoring & Plan Synthesis  |
                  | - Deduct points per defect severity          |
                  | - Enforce critical defect fail-gates         |
                  | - Write actionable plan to docs/superpowers/ |
                  +-----------------------+----------------------+
                                          |
                              [if auto_apply == true]
                                          v
                  +----------------------------------------------+
                  | Phase 4: Iterative Fix & Re-Evaluation       |
                  | - Apply Rust/egui code modifications         |
                  | - Verify build and re-trigger capture loop   |
                  +----------------------------------------------+
```

---

## 3. Capture Harness & Dynamic Image Slicing

### 3.1 Stateful Capture (`scripts/capture_standalone_gui.sh`)
Captures three discrete GUI operating states into `target/gui-qa/`:
1. `latest.png` (Default state): Fun mode active, stopped transport.
2. `accurate.png` (Accurate state): Accurate mode active (`VIBE303_QA_STATE="accurate"`), souped-up engine bypassed.
3. `running.png` (Running state): Active playback (`VIBE303_QA_STATE="running"`), sequencer running with active playhead.

Captures cleanly with window-specific ID targeting via `screencapture -l <win_id> -o <path>`.

### 3.2 Dynamic Multi-Crop Generator (`scripts/crop_gui_sections.py`)
Reads layout constants (`GUI_WIDTH`, `GUI_HEIGHT`, header heights, panel paddings) directly from `plugin303/src/gui/` source files.

Generates 8 targeted image assets per evaluation run:
- **5 Discrete Domain Slices:**
  - `crop_header.png`: Header bar and top branding strip.
  - `crop_classic.png`: Vertical waveform selector, 7 classic dials, and ADSR banks.
  - `crop_souped.png`: Souped-Up engine drawer and analog controls.
  - `crop_clipper_tweaks.png`: Output clipper and tweaks parameter row.
  - `crop_sequencer.png`: 16-step sequencer matrix and transport column.
- **2 Margin & Boundary Slices:**
  - `crop_right_margin.png`: Vertical rightmost margin slice (width derived from `GUI_WIDTH`) across full height. Exposes text truncation, slider track overflow, and border breaches.
  - `crop_bottom_margin.png`: Horizontal bottom slice (height derived from `GUI_HEIGHT`). Exposes octave button boundary collisions and bottom dead bands.
- **1 Multi-State Composite Slice:**
  - `crop_state_diff.png`: Side-by-side composite comparing middle drawer and bottom sequencer across Default, Accurate, and Running states.

---

## 4. Dual-Matrix Inspection Agents

Phase 2 dispatches 8 specialized vision agents in parallel.

### 4.1 Group A: Domain Inspection Agents

| Agent Label | Target Crop | Primary Inspection Criteria |
|---|---|---|
| `qa:header-bar` | `crop_header.png` | Title branding consistency (`TECHNEO` vs `TB-303`), topology selector switch position, polyphony readout, master tuning dial, drift/age readouts. |
| `qa:classic-panel` | `crop_classic.png` | Vertical waveform switch icon (distinct waveform vs generic menu glyph), 7 rotary dials with 270° radial ticks and value readouts, ADSR sub-panel frame containment, ADV ENV placement and panel negative space balance. |
| `qa:souped-drawer` | `crop_souped.png` & `accurate_crop_souped.png` | Extended dials (VCF Drive, Glide, Age, Drift, Tolerance, Noise, Seed), Filter slope selector, VCA mode switch, MEG/VEG faders, Accurate mode 30% dimming consistency and non-interactive overlay contrast. |
| `qa:clipper-tweaks` | `crop_clipper_tweaks.png` | Clipper model selector, GE / JFET toggle affordance (avoiding strikethrough styling), Drive/Ceiling dials, Tweaks parameter units (perceptual %/dB vs raw potentiometer resistance kΩ), Seed parameter labeling. |
| `qa:sequencer` | `crop_sequencer.png` | 16-step grid layout, note/tie/rest gate colors, chromatic pitch readouts, accent/slide LED indicators, bottom octave button padding, red step dash clarity vs ACC dots. |

### 4.2 Group B: Adversarial Defect Hunters

| Agent Label | Target Asset | Adversarial Inspection Checklist |
|---|---|---|
| `qa:edge-bleed-containment` | `crop_right_margin.png`<br>`crop_bottom_margin.png` | 1. Any text cut off or truncated (e.g. "200 m" instead of "200 ms")?<br>2. Any slider track or control element running past panel/window borders?<br>3. Any hex readout or label flush against the window edge without margin?<br>4. Any sub-panel (e.g. ADSR) overlapping its parent panel frame?<br>5. Do bottom octave buttons collide with the bottom window edge? |
| `qa:state-structural-diff` | `crop_state_diff.png` | 1. Is there an empty dead black band at the bottom of the window in Accurate mode?<br>2. Is the middle drawer vertically compressed or squished in Accurate mode compared to Fun mode?<br>3. Is the Accurate mode overlay box centered, well-integrated, and legible with high-contrast text?<br>4. Do disabled controls ghost through unevenly at higher brightness than labels?<br>5. Is the running state transport playhead visibly active on a specific step? |
| `qa:affordance-semantics-units` | `latest.png`<br>`running.png` | 1. Does the Volume dial pointer angle visually contradict its numeric value (e.g. pointing at 4–5 o'clock for +0.0 dB while centered dials point at 12 o'clock)?<br>2. Are selection highlights consistent (e.g. orange across all controls, no warning red/pink on valid settings)?<br>3. Is numeric formatting standardized (no mixed "0%" with "0.00", or leading zeros "01" with "16")?<br>4. Are parameter units perceptual (%/dB/ms) rather than raw electrical resistance (kΩ)?<br>5. Do toggle controls have clear affordances (no disabled strikethroughs on active choices)? |

---

## 5. Structured Output Schemas & Deductive Scoring Engine

### 5.1 Findings Schema (`VISION_FINDINGS_SCHEMA`)
```javascript
export const VISION_FINDINGS_SCHEMA = {
  type: "object",
  properties: {
    section: {
      type: "string",
      enum: [
        "header_bar",
        "classic_panel",
        "souped_drawer",
        "clipper_tweaks",
        "sequencer",
        "edge_containment",
        "state_diff",
        "affordance_semantics"
      ]
    },
    findings: {
      type: "array",
      items: {
        type: "object",
        properties: {
          element: { type: "string" },
          severity: { type: "string", enum: ["critical", "major", "minor"] },
          category: {
            type: "string",
            enum: [
              "clipping_overflow",
              "layout_collapse",
              "pointer_mismatch",
              "color_inconsistency",
              "formatting_units",
              "containment_overlap",
              "missing_feedback",
              "branding_affordance"
            ]
          },
          observed: { type: "string" },
          expected: { type: "string" },
          suggested_fix: { type: "string" }
        },
        required: ["element", "severity", "category", "observed", "expected", "suggested_fix"]
      }
    }
  },
  required: ["section", "findings"]
};
```

### 5.2 Deterministic Deductive Scoring Engine
The synthesis agent calculates overall GUI quality using strict deduction rules:

$$\text{Initial Score} = 100.0$$
$$\text{Deductions} = 25 \times N_{\text{critical}} + 10 \times N_{\text{major}} + 3 \times N_{\text{minor}}$$
$$\text{Calculated Score} = \max(0.0, 100.0 - \text{Deductions})$$

#### Quality Gate Enforcements:
- **Critical Defect Rule:** If $N_{\text{critical}} \ge 1$:
  - `spec_compliance = false`
  - $\text{Overall Score} = \min(\text{Calculated Score}, 69.0)$
- **Major Defect Rule:** If $N_{\text{major}} \ge 2$:
  - `spec_compliance = false`
  - $\text{Overall Score} = \min(\text{Calculated Score}, 79.0)$
- **Pass Threshold:** Minimum passing score is **95.0 / 100** with $N_{\text{critical}} = 0$ and $N_{\text{major}} = 0$.

---

## 6. Synthesis & Refinement Plan Generation

The synthesis agent aggregates all 8 inspector outputs, deduplicates findings, runs the deductive scoring calculation, and generates `docs/superpowers/plans/YYYY-MM-DD-gui-refinement-plan.md` containing:
1. **Executive Scorecard:** Calculated overall score, defect counts by severity, and spec compliance status.
2. **Consolidated Defect Matrix:** Tabular breakdown of element, section, category, severity, observed defect, and expected state.
3. **Root Cause Source Mapping:** Exact file and line references in `plugin303/src/gui/`.
4. **Step-by-Step Remediation Plan:** Concrete, testable code change tasks.

---

## 7. Verification & Safety Guards

1. **Compilation Guard:** Standalone build fails fast if Rust compilation or unit tests fail.
2. **Process Cleanup:** Trap ensures clean termination of standalone process instances.
3. **Deterministic Evaluation:** Structured output schemas and rigid mathematical scoring prevent subjective hallucination of inflated quality scores.
4. **Headless Fallback:** Detects and reports display availability errors in automated CI environments.
