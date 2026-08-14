# GUI Visual QA Closed Iteration Loop Specification

**Date:** 2026-08-14  
**Status:** Approved  
**Topic:** Automated visual QA closed iteration workflow for `plugin303` (`nih_plug_egui`)  
**Target:** macOS standalone binary + Claude Workflow engine  

---

## 1. Overview & Objectives

This specification defines an automated closed iteration loop for verifying and refining the `vibe303` plugin graphical user interface (`nih_plug_egui`). The workflow executes as a declarative Claude Workflow script, captures high-resolution screenshots of the standalone GUI application on macOS, runs parallel multi-lens visual QA analysis against the design specification and reference mockup, synthesizes findings into a ranked defect matrix, generates an actionable implementation plan, and optionally applies iterative fixes.

### Key Objectives
1. **Automated Standalone Capture:** Compile and launch `plugin303` standalone, capture the rendered GUI window with macOS `screencapture` without window shadows, and terminate the process cleanly.
2. **Multi-Lens Visual QA:** Analyze the captured screenshot in parallel across three specialized UI domains (Header & Classic Panel, Souped-Up Drawer & Topology, and 16-Step Sequencer Console).
3. **Structured Defect Scoring:** Score visual compliance (0–100%) against `docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md` and reference mockup `docs/plans/2026-07-02-ui-mockup.jpg` using structured JSON schemas.
4. **Actionable Plan Generation:** Synthesize regional findings into a formal refinement plan document saved to `docs/superpowers/plans/YYYY-MM-DD-gui-refinement-plan.md`.
5. **Configurable Iteration Loop:** Support single-pass audit by default (`max_iterations = 1`, `auto_apply = false`) with an optional autonomous fix loop (`auto_apply = true`) that applies code changes and re-evaluates until target quality score is met.

---

## 2. System Architecture & Workflow Pipeline

The workflow is orchestrated via `.claude/workflows/gui-visual-qa-loop.js` using the Claude Workflow execution engine.

```
                  +-----------------------------------+
                  |  1. Standalone Capture Script     |
                  |  - cargo build & run standalone   |
                  |  - screencapture window to disk   |
                  +-----------------+-----------------+
                                    |
            +-----------------------+-----------------------+
            |                       |                       |
            v                       v                       v
  +-------------------+   +-------------------+   +-------------------+
  | 2A. Classic Panel |   | 2B. Souped Drawer |   | 2C. Sequencer     |
  |     Vision Agent  |   |     Vision Agent  |   |     Vision Agent  |
  +---------+---------+   +---------+---------+   +---------+---------+
            |                       |                       |
            +-----------------------+-----------------------+
                                    |
                                    v
                  +-----------------------------------+
                  | 3. Synthesis & QA Judge Agent     |
                  | - Validate findings vs design doc |
                  | - Rank discrepancies by severity  |
                  | - Output refinement plan doc      |
                  +-----------------+-----------------+
                                    |
                      [if loop_mode == "continuous"]
                                    v
                  +-----------------------------------+
                  | 4. Iteration Fixer Agent          |
                  | - Apply Rust/egui code edits      |
                  | - Run cargo test & clippy         |
                  | - Trigger re-capture loop         |
                  +-----------------+-----------------+
```

### Pipeline Phases
1. **Phase: Capture:** Executes `scripts/capture_standalone_gui.sh`. Builds the standalone binary, starts it in the background, waits for window appearance, grabs the window image, and saves to `target/gui-qa/latest.png`.
2. **Phase: Evaluate (Parallel):** Dispatches three independent vision evaluation subagents concurrently:
   - `agent:classic-panel`
   - `agent:souped-drawer`
   - `agent:sequencer-console`
3. **Phase: Synthesize:** Synthesizer subagent aggregates findings, deduplicates issues, calculates visual quality score, and writes the refinement plan document.
4. **Phase: Fix (Optional):** If `auto_apply` is enabled and score < `target_score`, an implementation subagent applies code changes and re-triggers the capture phase.

---

## 3. Standalone Runner & Capture Harness

### 3.1 Standalone Binary Entrypoint (`plugin303/src/bin/standalone.rs`)
Exposes the standalone executable using `nih_plug` export macro:
```rust
use nih_plug::prelude::*;
use plugin303::Plugin303;

fn main() {
    nih_export_standalone!(Plugin303);
}
```

### 3.2 macOS Capture Script (`scripts/capture_standalone_gui.sh`)
```bash
#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
OUTPUT_DIR="$REPO_ROOT/target/gui-qa"
OUTPUT_FILE="$OUTPUT_DIR/latest.png"
LOG_FILE="$OUTPUT_DIR/standalone.log"

mkdir -p "$OUTPUT_DIR"

echo "Building standalone binary..."
cargo build --manifest-path "$REPO_ROOT/Cargo.toml" -p plugin303 --bin standalone

echo "Launching standalone GUI..."
"$REPO_ROOT/target/debug/standalone" > "$LOG_FILE" 2>&1 &
APP_PID=$!

cleanup() {
    if kill -0 "$APP_PID" 2>/dev/null; then
        echo "Terminating standalone process (PID: $APP_PID)..."
        kill -TERM "$APP_PID" 2>/dev/null || true
        sleep 0.5
        kill -KILL "$APP_PID" 2>/dev/null || true
    fi
}
trap cleanup EXIT

echo "Waiting for window..."
WIN_ID=""
for i in {1..30}; do
    WIN_ID=$(osascript -e '
        tell application "System Events"
            set proc to first process whose unix id is '"$APP_PID"'
            if exists proc then
                set winList to windows of proc
                if (count of winList) > 0 then
                    return id of first window of proc
                end if
            end if
        end tell
    ' 2>/dev/null || true)
    
    if [[ -n "$WIN_ID" && "$WIN_ID" != "missing value" ]]; then
        break
    fi
    sleep 0.3
done

if [[ -z "$WIN_ID" || "$WIN_ID" == "missing value" ]]; then
    # Fallback to CGWindowList / window name query
    WIN_ID=$(python3 -c "
import Quartz
wl = Quartz.CGWindowListCopyWindowInfo(Quartz.kCGWindowListOptionOnScreenOnly, Quartz.kCGNullWindowID)
for w in wl:
    if w.get('kCGWindowOwnerPID') == $APP_PID:
        print(w.get('kCGWindowNumber'))
        break
" 2>/dev/null || true)
fi

if [[ -z "$WIN_ID" ]]; then
    echo "ERROR: Standalone window did not appear within timeout." >&2
    cat "$LOG_FILE" >&2
    exit 1
fi

echo "Capturing window ID: $WIN_ID to $OUTPUT_FILE..."
screencapture -l "$WIN_ID" -o "$OUTPUT_FILE"

if [[ ! -s "$OUTPUT_FILE" ]]; then
    echo "ERROR: Screenshot output file is empty or missing." >&2
    exit 1
fi

echo "Screenshot captured successfully: $OUTPUT_FILE"
```

---

## 4. Multi-Lens Vision QA Subagents

### 4.1 Evaluation Domains

| Subagent | Scope | Key Inspection Criteria |
|---|---|---|
| **Classic Panel & Header** | Header bar & 7 Vintage 303 rotary dials | Header layout, typography, master volume dial, 7 rotary knobs (Tuning, Cutoff, Resonance, Env Mod, Decay, Accent, Volume) with 270° radial ticks and pointer lines, Waveform switch detents, Tier selector |
| **Souped-Up Drawer & Topology** | Right drawer panel, extended controls, accurate-mode dimming | Extended dials (VCF Drive, Glide, Age, Drift, Tolerance, Thermal Noise, Seed), Filter slope selector (`2-Pole`, `3-Pole`, `4-Pole`), VCA envelope mode (`Gate`, `Env`), MEG/VEG vertical ADSR faders, Accurate mode ~30% opacity overlay badge |
| **Sequencer Console** | Bottom 16-step grid & transport controls | 16 step columns, playhead LED tracking, Accent (`ACC`) amber LEDs, Slide (`SLD`) cyan LEDs, Gate tile colors (`NOTE` = `#00e5ff`, `TIE` = `#ffd700`, `REST` = `#333a42`), pitch readout, octave buttons (`▲`/`▼`), transport controls |

### 4.2 Structured Output Schema (`VISION_FINDINGS_SCHEMA`)
```javascript
export const VISION_FINDINGS_SCHEMA = {
  type: "object",
  properties: {
    section: {
      type: "string",
      enum: ["classic_panel", "souped_drawer", "sequencer"]
    },
    alignment_score: {
      type: "number",
      description: "Visual alignment score from 0 to 100"
    },
    spec_compliance: {
      type: "boolean",
      description: "True if section strictly complies with the design specification"
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
            enum: ["alignment", "color", "geometry", "typography", "missing_feature"]
          },
          observed: { type: "string" },
          expected: { type: "string" },
          suggested_fix: { type: "string" }
        },
        required: ["element", "severity", "category", "observed", "expected", "suggested_fix"]
      }
    }
  },
  required: ["section", "alignment_score", "spec_compliance", "findings"]
};
```

---

## 5. Synthesis & Refinement Plan Generator

### 5.1 Synthesis Subagent
- Evaluates the three regional JSON outputs alongside:
  1. `docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md`
  2. `docs/plans/2026-07-02-ui-mockup.jpg`
- Calculates overall weighted GUI Quality Score:
  $$\text{Score} = 0.35 \times S_{\text{classic}} + 0.30 \times S_{\text{souped}} + 0.35 \times S_{\text{seq}}$$
- Emits structured JSON summary and writes the refinement plan document.

### 5.2 Refinement Plan Format
The plan document is written to `docs/superpowers/plans/YYYY-MM-DD-gui-refinement-plan.md` with:
1. **Executive Summary & Overall Quality Score**
2. **Defect Matrix Table:** Element, section, severity, category, observed vs. expected.
3. **Target Files to Edit:** Exact paths in `plugin303/src/gui/`.
4. **Step-by-Step Task Breakdown:** Concrete implementation steps with verification criteria.

---

## 6. Workflow Implementation (`.claude/workflows/gui-visual-qa-loop.js`)

```javascript
export const meta = {
  name: "gui-visual-qa-loop",
  description: "Closed iteration loop for nih_plug_egui GUI visual QA and refinement",
  phases: [
    { title: "Capture", detail: "Compile standalone and capture GUI screenshot" },
    { title: "Visual QA", detail: "Parallel multi-lens visual inspection" },
    { title: "Synthesize", detail: "Aggregate findings and generate refinement plan" },
    { title: "Iterative Fix", detail: "Apply code edits and verify build (optional)" }
  ]
};

// Main workflow body
const { auto_apply = false, max_iterations = 1, target_score = 90 } = args || {};

let iteration = 0;
let currentScore = 0;
let finalPlanPath = "";

while (iteration < maxIterations && currentScore < targetScore) {
  iteration++;
  log(`Starting Visual QA iteration ${iteration}/${maxIterations}...`);

  // Phase 1: Capture
  phase("Capture");
  const captureResult = await agent(
    "Run scripts/capture_standalone_gui.sh and verify that target/gui-qa/latest.png is created.",
    { label: "capture:standalone-gui", phase: "Capture" }
  );

  // Phase 2: Parallel Visual QA
  phase("Visual QA");
  const lenses = [
    {
      name: "classic_panel",
      prompt: "Analyze target/gui-qa/latest.png focusing on Header Bar and Classic 7-Knob Panel. Compare against docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md and docs/plans/2026-07-02-ui-mockup.jpg."
    },
    {
      name: "souped_drawer",
      prompt: "Analyze target/gui-qa/latest.png focusing on Souped-Up Drawer, MEG/VEG Faders, and Topology Dimming. Compare against docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md."
    },
    {
      name: "sequencer",
      prompt: "Analyze target/gui-qa/latest.png focusing on 16-Step Sequencer Strip, LED states, ACC/SLD toggles, and Gate colors. Compare against docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md."
    }
  ];

  const lensResults = await parallel(
    lenses.map(lens => () =>
      agent(lens.prompt, {
        label: `qa:${lens.name}`,
        phase: "Visual QA",
        schema: VISION_FINDINGS_SCHEMA
      })
    )
  );

  // Phase 3: Synthesize & Plan
  phase("Synthesize");
  const synthesis = await agent(
    `Synthesize visual QA findings: ${JSON.stringify(lensResults)}. Compare against design spec docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md. Calculate overall quality score and write a complete refinement plan to docs/superpowers/plans/YYYY-MM-DD-gui-refinement-plan.md.`,
    {
      label: "synthesize:plan-generator",
      phase: "Synthesize",
      schema: SYNTHESIS_SCHEMA
    }
  );

  currentScore = synthesis.overall_score;
  finalPlanPath = synthesis.plan_path;
  log(`Iteration ${iteration} Visual QA Score: ${currentScore}%`);

  if (!auto_apply || currentScore >= targetScore) {
    break;
  }

  // Phase 4: Iterative Fix (if auto_apply is enabled)
  phase("Iterative Fix");
  await agent(
    `Apply fixes from ${finalPlanPath} to plugin303/src/gui/ codebase. Run cargo test and verify clean compilation.`,
    { label: "fix:apply-refinements", phase: "Iterative Fix" }
  );
}

return {
  iterations_completed: iteration,
  final_score: currentScore,
  plan_path: finalPlanPath
};
```

---

## 7. Error Handling, Verification & Safety

1. **Compilation Guard:** Capture script aborts immediately if `cargo build` fails and prints compiler diagnostics.
2. **Process Cleanup:** Trap ensures `kill -TERM` / `kill -KILL` on the standalone PID on any script exit or interruption.
3. **No Display / Headless Handling:** If window detection times out (e.g. running in headless CI without display server), script fails with clear diagnostics.
4. **Zero Audio Thread Impact:** GUI testing runs completely out-of-process from DAW audio engines; does not affect RT audio safety.
