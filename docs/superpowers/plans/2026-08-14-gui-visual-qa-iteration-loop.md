# GUI Visual QA Closed Iteration Loop Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build an automated visual QA closed iteration loop for `plugin303` (`nih_plug_egui`) that captures standalone GUI screenshots, runs multi-lens visual evaluations in parallel, synthesizes findings into a ranked refinement plan, and supports an optional autonomous fix loop.

**Architecture:** A standalone binary entrypoint compiled via `nih_export_standalone!`, a macOS screenshot capture utility script using `osascript` / `screencapture`, structured JSON evaluation schemas for visual inspection, and a declarative Claude Workflow orchestrator in `.claude/workflows/gui-visual-qa-loop.js`.

**Tech Stack:** Rust (edition 2021), `nih_plug` (vst3 + standalone), `nih_plug_egui`, macOS Bash / `screencapture` / `osascript`, JavaScript (Claude Workflow script).

**Spec:** `docs/superpowers/specs/2026-08-14-gui-visual-qa-iteration-loop-design.md`

## Global Constraints

- Standalone executable must be exposed via `plugin303/src/bin/standalone.rs` with `nih_export_standalone!(Plugin303)`.
- Capture script must clean up background processes on exit using bash traps.
- Visual findings must adhere to `VISION_FINDINGS_SCHEMA` and `SYNTHESIS_SCHEMA`.
- Workflow script must be located at `.claude/workflows/gui-visual-qa-loop.js` and begin with pure literal `meta`.

---

### Task 1: Standalone Binary Entrypoint

**Files:**
- Create: `plugin303/src/bin/standalone.rs`
- Modify: `plugin303/Cargo.toml` (if required)
- Test: `cargo check --bin standalone`

**Interfaces:**
- Consumes: `plugin303::Plugin303`, `nih_plug::prelude::nih_export_standalone`
- Produces: Executable binary `target/debug/standalone`

- [ ] **Step 1: Write standalone binary entrypoint**

Create `plugin303/src/bin/standalone.rs`:
```rust
use nih_plug::prelude::*;
use plugin303::Plugin303;

fn main() {
    nih_export_standalone!(Plugin303);
}
```

- [ ] **Step 2: Verify binary compilation with cargo check**

Run: `cargo check --bin standalone`  
Expected: Clean compilation with exit code 0.

- [ ] **Step 3: Build standalone debug binary**

Run: `cargo build -p plugin303 --bin standalone`  
Expected: `target/debug/standalone` binary produced successfully.

- [ ] **Step 4: Commit**

```bash
git add plugin303/src/bin/standalone.rs
git commit -m "feat(gui): add standalone binary entrypoint for visual QA"
```

---

### Task 2: macOS Screenshot Capture Script

**Files:**
- Create: `scripts/capture_standalone_gui.sh`
- Test: `bash scripts/capture_standalone_gui.sh`

**Interfaces:**
- Consumes: `target/debug/standalone` executable
- Produces: PNG screenshot at `target/gui-qa/latest.png`

- [ ] **Step 1: Create capture script with window detection and cleanup trap**

Create `scripts/capture_standalone_gui.sh`:
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

echo "Waiting for standalone window..."
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

- [ ] **Step 2: Make capture script executable**

Run: `chmod +x scripts/capture_standalone_gui.sh`  
Expected: Script permissions updated.

- [ ] **Step 3: Test capture script execution**

Run: `bash scripts/capture_standalone_gui.sh`  
Expected: Standalone window opens, captures `target/gui-qa/latest.png`, closes process, exits 0.

- [ ] **Step 4: Verify generated image file**

Run: `file target/gui-qa/latest.png`  
Expected: Output shows valid PNG image data (e.g., `PNG image data, ...`).

- [ ] **Step 5: Commit**

```bash
git add scripts/capture_standalone_gui.sh
git commit -m "feat(scripts): add macOS standalone GUI screenshot capture harness"
```

---

### Task 3: Visual QA Structured Schemas

**Files:**
- Create: `.claude/workflows/schemas/gui-qa-schemas.js`
- Test: Node syntax validation `node -c .claude/workflows/schemas/gui-qa-schemas.js`

**Interfaces:**
- Consumes: None
- Produces: `VISION_FINDINGS_SCHEMA`, `SYNTHESIS_SCHEMA`

- [ ] **Step 1: Create directory for workflow schemas**

Run: `mkdir -p .claude/workflows/schemas`

- [ ] **Step 2: Define JSON validation schemas in JavaScript**

Create `.claude/workflows/schemas/gui-qa-schemas.js`:
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

export const SYNTHESIS_SCHEMA = {
  type: "object",
  properties: {
    overall_score: {
      type: "number",
      description: "Overall weighted visual quality score from 0 to 100"
    },
    spec_compliance: {
      type: "boolean",
      description: "True if all sections pass visual quality gate"
    },
    plan_path: {
      type: "string",
      description: "File path where refinement plan was written"
    },
    top_defects: {
      type: "array",
      items: {
        type: "object",
        properties: {
          section: { type: "string" },
          element: { type: "string" },
          severity: { type: "string" },
          summary: { type: "string" }
        },
        required: ["section", "element", "severity", "summary"]
      }
    }
  },
  required: ["overall_score", "spec_compliance", "plan_path", "top_defects"]
};
```

- [ ] **Step 3: Validate syntax**

Run: `node --input-type=module -e 'import "./.claude/workflows/schemas/gui-qa-schemas.js";'`  
Expected: Exit code 0 with no syntax errors.

- [ ] **Step 4: Commit**

```bash
git add .claude/workflows/schemas/gui-qa-schemas.js
git commit -m "feat(workflow): add structured JSON schemas for visual QA inspection"
```

---

### Task 4: Workflow Orchestration Script

**Files:**
- Create: `.claude/workflows/gui-visual-qa-loop.js`
- Test: Node syntax validation `node --input-type=module -c .claude/workflows/gui-visual-qa-loop.js`

**Interfaces:**
- Consumes: `scripts/capture_standalone_gui.sh`, `VISION_FINDINGS_SCHEMA`, `SYNTHESIS_SCHEMA`
- Produces: Execution output object `{ iterations_completed, final_score, plan_path }`

- [ ] **Step 1: Implement workflow script with parallel lenses and configurable loop**

Create `.claude/workflows/gui-visual-qa-loop.js`:
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

const VISION_FINDINGS_SCHEMA = {
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

const SYNTHESIS_SCHEMA = {
  type: "object",
  properties: {
    overall_score: {
      type: "number",
      description: "Overall weighted visual quality score from 0 to 100"
    },
    spec_compliance: {
      type: "boolean",
      description: "True if all sections pass visual quality gate"
    },
    plan_path: {
      type: "string",
      description: "File path where refinement plan was written"
    },
    top_defects: {
      type: "array",
      items: {
        type: "object",
        properties: {
          section: { type: "string" },
          element: { type: "string" },
          severity: { type: "string" },
          summary: { type: "string" }
        },
        required: ["section", "element", "severity", "summary"]
      }
    }
  },
  required: ["overall_score", "spec_compliance", "plan_path", "top_defects"]
};

const { auto_apply = false, max_iterations = 1, target_score = 90 } = args || {};

let iteration = 0;
let currentScore = 0;
let finalPlanPath = "";

while (iteration < max_iterations && currentScore < target_score) {
  iteration++;
  log(`Starting Visual QA iteration ${iteration}/${max_iterations}...`);

  // Phase 1: Capture
  phase("Capture");
  const captureResult = await agent(
    "Run bash scripts/capture_standalone_gui.sh and verify that target/gui-qa/latest.png is created and non-empty.",
    { label: "capture:standalone-gui", phase: "Capture" }
  );

  // Phase 2: Parallel Visual QA
  phase("Visual QA");
  const lenses = [
    {
      name: "classic_panel",
      prompt: "Analyze target/gui-qa/latest.png focusing on Header Bar and Classic 7-Knob Panel. Compare against docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md and docs/plans/2026-07-02-ui-mockup.jpg. Evaluate knob tick marks, labels, switches, and header branding."
    },
    {
      name: "souped_drawer",
      prompt: "Analyze target/gui-qa/latest.png focusing on Souped-Up Drawer, MEG/VEG Faders, and Topology Dimming. Compare against docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md. Evaluate extended dials, faders, and accurate-mode dimming badge."
    },
    {
      name: "sequencer",
      prompt: "Analyze target/gui-qa/latest.png focusing on 16-Step Sequencer Strip, LED states, ACC/SLD toggles, and Gate colors. Compare against docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md. Evaluate step columns, button alignment, pitch readability, and transport bar."
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
    `Synthesize visual QA findings: ${JSON.stringify(lensResults)}. Compare against design spec docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md. Calculate overall weighted score (0.35 classic + 0.30 souped + 0.35 sequencer) and write a complete, actionable refinement plan to docs/superpowers/plans/YYYY-MM-DD-gui-refinement-plan.md.`,
    {
      label: "synthesize:plan-generator",
      phase: "Synthesize",
      schema: SYNTHESIS_SCHEMA
    }
  );

  currentScore = synthesis ? synthesis.overall_score : 0;
  finalPlanPath = synthesis ? synthesis.plan_path : "";
  log(`Iteration ${iteration} Visual QA Score: ${currentScore}%`);

  if (!auto_apply || currentScore >= target_score) {
    break;
  }

  // Phase 4: Iterative Fix (if auto_apply is enabled)
  phase("Iterative Fix");
  await agent(
    `Apply fixes from ${finalPlanPath} to plugin303/src/gui/ codebase. Run cargo test and cargo check to verify clean compilation.`,
    { label: "fix:apply-refinements", phase: "Iterative Fix" }
  );
}

return {
  iterations_completed: iteration,
  final_score: currentScore,
  plan_path: finalPlanPath
};
```

- [ ] **Step 2: Validate JavaScript syntax**

Run: `node --input-type=module -c .claude/workflows/gui-visual-qa-loop.js`  
Expected: Syntax check passes without error.

- [ ] **Step 3: Commit**

```bash
git add .claude/workflows/gui-visual-qa-loop.js
git commit -m "feat(workflow): add gui-visual-qa-loop orchestration script"
```

---

### Task 5: End-to-End Smoke Test & Documentation

**Files:**
- Modify: `docs/superpowers/specs/2026-08-14-gui-visual-qa-iteration-loop-design.md` (if needed)
- Test: Full capture and workflow dry-run

- [ ] **Step 1: Run standalone capture script to generate reference artifact**

Run: `bash scripts/capture_standalone_gui.sh`  
Expected: `target/gui-qa/latest.png` generated and verified.

- [ ] **Step 2: Run workflow via Workflow tool in single-pass audit mode**

Test invocation with `args: { auto_apply: false, max_iterations: 1 }` to produce first baseline plan document.

- [ ] **Step 3: Verify output refinement plan artifact**

Run: `ls -la docs/superpowers/plans/*gui-refinement-plan.md`  
Expected: New refinement plan document exists and contains defect matrix.

- [ ] **Step 4: Commit all remaining changes**

```bash
git add docs/superpowers/
git commit -m "docs: finalize gui visual qa iteration loop workflow and baseline plan"
```
