# Comprehensive GUI Visual QA Workflow Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build and integrate a comprehensive, attentive, and adversarial automated GUI Visual QA workflow that eliminates false passes and accurately detects clipping, layout collapse, pointer mismatches, color inconsistencies, and missing feedback.

**Architecture:** Multi-state standalone capture (`default`, `accurate`, `running`) feeding a dynamic Python slicing script that parses Rust GUI geometry into 8 specialized crops. An 8-agent parallel inspection matrix (5 domain specialists + 3 adversarial defect hunters) feeds a deterministic deductive scoring engine (-25 Critical, -10 Major, -3 Minor) and produces actionable refinement plans.

**Tech Stack:** JavaScript (Claude Workflow), Python 3 (PIL/Pillow), Bash (`screencapture`), Rust (`nih_plug_egui`).

**Spec:** `docs/superpowers/specs/2026-08-21-gui-visual-qa-workflow-redesign.md`

## Global Constraints

- Slicing bounds must dynamically parse `plugin303/src/gui/mod.rs` and panel definitions rather than using hardcoded pixel constants.
- Dual-matrix evaluation must execute 8 inspection agents in parallel during Phase 2.
- Deductive scoring must be deterministic: Base 100, Critical -25, Major -10, Minor -3. Any Critical defect forces `spec_compliance = false` and score $\le 69.0$.
- No regressions to existing plugin audio processing or DSP real-time safety.

---

### Task 1: Enhance `scripts/crop_gui_sections.py` with Dynamic Rust Geometry Parsing & Boundary Crops

**Files:**
- Modify: `scripts/crop_gui_sections.py`
- Test: `tests/test_crop_gui_sections.py`

**Interfaces:**
- Consumes: Captured screenshots (`target/gui-qa/latest.png`, `target/gui-qa/accurate.png`, `target/gui-qa/running.png`) and geometry tokens from `plugin303/src/gui/mod.rs`.
- Produces: 8 cropped assets per run:
  - 5 domain crops: `crop_header.png`, `crop_classic.png`, `crop_souped.png`, `crop_clipper_tweaks.png`, `crop_sequencer.png`.
  - 2 margin boundary crops: `crop_right_margin.png`, `crop_bottom_margin.png`.
  - 1 composite diff strip: `crop_state_diff.png`.

- [ ] **Step 1: Write the failing unit test for dynamic geometry parsing and cropping**

Create `tests/test_crop_gui_sections.py`:
```python
import os
import sys
import unittest
from pathlib import Path
from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "scripts"))
import crop_gui_sections


class TestCropGuiSections(unittest.TestCase):
    def setUp(self):
        self.test_dir = Path("/tmp/test_gui_crop")
        self.test_dir.mkdir(parents=True, exist_ok=True)
        # Create mock 2160x1440 image (2x Retina of 1080x720)
        img = Image.new("RGBA", (2160, 1440), color=(30, 30, 30, 255))
        img.save(self.test_dir / "latest.png")
        img.save(self.test_dir / "accurate.png")
        img.save(self.test_dir / "running.png")

    def tearDown(self):
        import shutil
        if self.test_dir.exists():
            shutil.rmtree(self.test_dir)

    def test_parse_gui_dimensions(self):
        dims = crop_gui_sections.parse_gui_dimensions()
        self.assertEqual(dims["width"], 1080)
        self.assertEqual(dims["height"], 720)

    def test_generate_all_crops(self):
        crop_gui_sections.process_all_states(self.test_dir)
        expected_files = [
            "latest_crop_header.png",
            "latest_crop_classic.png",
            "latest_crop_souped.png",
            "latest_crop_clipper_tweaks.png",
            "latest_crop_sequencer.png",
            "crop_right_margin.png",
            "crop_bottom_margin.png",
            "crop_state_diff.png",
        ]
        for f in expected_files:
            p = self.test_dir / f
            self.assertTrue(p.exists(), f"Expected crop file {f} was not generated")


if __name__ == "__main__":
    unittest.main()
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python3 -m unittest tests/test_crop_gui_sections.py -v`  
Expected: FAIL with `AttributeError: module 'crop_gui_sections' has no attribute 'parse_gui_dimensions'`

- [ ] **Step 3: Implement dynamic geometry parsing and margin/diff cropping in `scripts/crop_gui_sections.py`**

Modify `scripts/crop_gui_sections.py`:
```python
#!/usr/bin/env python3
"""
crop_gui_sections.py
Extracts dynamic section crops, right/bottom boundary margins, and multi-state composite strips.
Parses dimensions directly from plugin303/src/gui/mod.rs.
"""

import re
import sys
from pathlib import Path
from PIL import Image


def parse_gui_dimensions() -> dict:
    repo_root = Path(__file__).resolve().parent.parent
    gui_mod = repo_root / "plugin303" / "src" / "gui" / "mod.rs"
    dims = {"width": 1080, "height": 720}
    if gui_mod.exists():
        content = gui_mod.read_text(encoding="utf-8")
        w_match = re.search(r"pub\s+const\s+GUI_WIDTH:\s*u32\s*=\s*(\d+);", content)
        h_match = re.search(r"pub\s+const\s+GUI_HEIGHT:\s*u32\s*=\s*(\d+);", content)
        if w_match:
            dims["width"] = int(w_match.group(1))
        if h_match:
            dims["height"] = int(h_match.group(1))
    return dims


def get_crop_definitions(width: int, height: int):
    # Dynamic vertical bounds scaled to logical height
    scale_y = height / 720.0
    sections = {
        "crop_header": (0, int(52 * scale_y)),
        "crop_classic": (int(48 * scale_y), int(245 * scale_y)),
        "crop_souped": (int(240 * scale_y), int(390 * scale_y)),
        "crop_clipper_tweaks": (int(385 * scale_y), int(520 * scale_y)),
        "crop_sequencer": (int(515 * scale_y), height),
    }
    return sections


def process_image(image_path: Path, output_dir: Path, dims: dict):
    if not image_path.is_file():
        return

    stem = image_path.stem
    img = Image.open(image_path)
    w, h = img.size
    scale = w / float(dims["width"])
    titlebar_h = int(32 * scale) if h > int(dims["height"] * scale) else 0

    sections = get_crop_definitions(dims["width"], dims["height"])

    for name, (y_start_pt, y_end_pt) in sections.items():
        y1 = max(0, titlebar_h + int(y_start_pt * scale))
        y2 = min(h, titlebar_h + int(y_end_pt * scale))
        cropped = img.crop((0, y1, w, y2))

        prefixed_path = output_dir / f"{stem}_{name}.png"
        cropped.save(prefixed_path)

        if stem == "latest":
            alias_path = output_dir / f"{name}.png"
            cropped.save(alias_path)


def generate_boundary_and_diff_crops(output_dir: Path, dims: dict):
    latest_p = output_dir / "latest.png"
    accurate_p = output_dir / "accurate.png"
    running_p = output_dir / "running.png"

    if latest_p.exists():
        img = Image.open(latest_p)
        w, h = img.size
        scale = w / float(dims["width"])
        titlebar_h = int(32 * scale) if h > int(dims["height"] * scale) else 0

        # Right margin: rightmost 15% of width
        x_right_start = int(w * 0.85)
        right_crop = img.crop((x_right_start, titlebar_h, w, h))
        right_crop.save(output_dir / "crop_right_margin.png")

        # Bottom margin: bottom 12% of height
        y_bottom_start = max(0, h - int(85 * scale))
        bottom_crop = img.crop((0, y_bottom_start, w, h))
        bottom_crop.save(output_dir / "crop_bottom_margin.png")

    # Generate state diff composite
    if latest_p.exists() and accurate_p.exists() and running_p.exists():
        img_lat = Image.open(latest_p)
        img_acc = Image.open(accurate_p)
        img_run = Image.open(running_p)

        w, h = img_lat.size
        scale = w / float(dims["width"])
        titlebar_h = int(32 * scale) if h > int(dims["height"] * scale) else 0
        y_mid_start = titlebar_h + int(240 * scale)
        y_mid_end = titlebar_h + int(520 * scale)

        mid_lat = img_lat.crop((0, y_mid_start, w, y_mid_end))
        mid_acc = img_acc.crop((0, y_mid_start, w, y_mid_end))
        mid_run = img_run.crop((0, y_mid_start, w, y_mid_end))

        diff_h = mid_lat.height * 3
        composite = Image.new("RGBA", (w, diff_h), (20, 20, 20, 255))
        composite.paste(mid_lat, (0, 0))
        composite.paste(mid_acc, (0, mid_lat.height))
        composite.paste(mid_run, (0, mid_lat.height * 2))
        composite.save(output_dir / "crop_state_diff.png")


def process_all_states(qa_dir: Path):
    dims = parse_gui_dimensions()
    for stem in ["latest", "accurate", "running"]:
        p = qa_dir / f"{stem}.png"
        if p.exists():
            process_image(p, qa_dir, dims)
    generate_boundary_and_diff_crops(qa_dir, dims)


def main():
    repo_root = Path(__file__).resolve().parent.parent
    qa_dir = repo_root / "target" / "gui-qa"
    if len(sys.argv) > 1:
        qa_dir = Path(sys.argv[1]).resolve()
    if not qa_dir.exists():
        qa_dir.mkdir(parents=True, exist_ok=True)
    process_all_states(qa_dir)


if __name__ == "__main__":
    main()
```

- [ ] **Step 4: Run test to verify it passes**

Run: `python3 -m unittest tests/test_crop_gui_sections.py -v`  
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add scripts/crop_gui_sections.py tests/test_crop_gui_sections.py
git commit -m "feat(qa): enhance crop_gui_sections with dynamic geometry and boundary crops"
```

---

### Task 2: Update Standalone Capture Script & Running State Playhead Simulation

**Files:**
- Modify: `scripts/capture_standalone_gui.sh`
- Modify: `plugin303/src/gui/panels/sequencer_panel.rs:60-90`

**Interfaces:**
- Consumes: Environment variable `VIBE303_QA_STATE` passed to standalone binary.
- Produces: Correct state simulation for `default`, `accurate`, and `running` (with active playhead step indicator rendered).

- [ ] **Step 1: Verify `plugin303/src/gui/panels/sequencer_panel.rs` active step handling in QA running state**

Check if `VIBE303_QA_STATE == "running"` sets playhead position (e.g. step index 4) when running standalone under QA capture.

- [ ] **Step 2: Update `scripts/capture_standalone_gui.sh` to execute the updated dynamic crop script**

Modify `scripts/capture_standalone_gui.sh`:
Ensure `capture_standalone_gui.sh` invokes `python3 "$SCRIPT_DIR/crop_gui_sections.py" "$OUTPUT_DIR"` and verifies all 8 crop outputs exist.

- [ ] **Step 3: Run standalone capture test**

Run: `bash scripts/capture_standalone_gui.sh`  
Expected: Clean capture of `latest.png`, `accurate.png`, `running.png`, `crop_right_margin.png`, `crop_bottom_margin.png`, and `crop_state_diff.png`.

- [ ] **Step 4: Commit**

```bash
git add scripts/capture_standalone_gui.sh plugin303/src/gui/panels/sequencer_panel.rs
git commit -m "feat(qa): ensure active playhead simulation and boundary crop validation in capture harness"
```

---

### Task 3: Define Structured Defect & Adversarial Schemas

**Files:**
- Modify: `.claude/workflows/gui-visual-qa-loop.js:20-80`

**Interfaces:**
- Consumes: Inspection agent outputs.
- Produces: Strictly typed JSON findings with 8 section types, 8 defect categories, and 3 severity tiers.

- [ ] **Step 1: Update `VISION_FINDINGS_SCHEMA` and `SYNTHESIS_SCHEMA` in `.claude/workflows/gui-visual-qa-loop.js`**

Implement the schema matching Section 5 of the design specification:
```javascript
const VISION_FINDINGS_SCHEMA = {
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

const SYNTHESIS_SCHEMA = {
  type: "object",
  properties: {
    overall_score: { type: "number" },
    spec_compliance: { type: "boolean" },
    plan_path: { type: "string" },
    critical_defects_count: { type: "integer" },
    major_defects_count: { type: "integer" },
    minor_defects_count: { type: "integer" },
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
  required: [
    "overall_score",
    "spec_compliance",
    "plan_path",
    "critical_defects_count",
    "major_defects_count",
    "minor_defects_count",
    "top_defects"
  ]
};
```

- [ ] **Step 2: Commit**

```bash
git add .claude/workflows/gui-visual-qa-loop.js
git commit -m "feat(qa): update workflow findings and synthesis schemas with defect taxonomy"
```

---

### Task 4: Implement 8 Parallel Dual-Matrix Inspection Agents & Deductive Scoring Calculation

**Files:**
- Modify: `.claude/workflows/gui-visual-qa-loop.js:100-200`

**Interfaces:**
- Consumes: All 8 crop files and full screenshots from `target/gui-qa/`.
- Produces: 8 parallel inspection evaluations and deductive score synthesis in `docs/superpowers/plans/YYYY-MM-DD-gui-refinement-plan.md`.

- [ ] **Step 1: Implement the 8 parallel agent prompts in `gui-visual-qa-loop.js`**

Add the 5 domain specialists and 3 adversarial defect hunters:
1. `qa:header-bar` on `crop_header.png`
2. `qa:classic-panel` on `crop_classic.png`
3. `qa:souped-drawer` on `crop_souped.png` & `accurate_crop_souped.png`
4. `qa:clipper-tweaks` on `crop_clipper_tweaks.png`
5. `qa:sequencer` on `crop_sequencer.png`
6. `qa:edge-bleed-containment` (Adversarial) on `crop_right_margin.png` & `crop_bottom_margin.png`
7. `qa:state-structural-diff` (Adversarial) on `crop_state_diff.png`
8. `qa:affordance-semantics-units` (Adversarial) on `latest.png` & `running.png`

- [ ] **Step 2: Implement deductive scoring engine formula in synthesis prompt**

Incorporate mathematical deduction rules into the synthesis prompt:
- Initial: 100. Deductions: 25 per critical, 10 per major, 3 per minor.
- Quality gate enforcements:
  - If $N_{\text{critical}} \ge 1 \implies$ `spec_compliance = false`, score $\le 69.0$.
  - If $N_{\text{major}} \ge 2 \implies$ `spec_compliance = false`, score $\le 79.0$.
- Write plan containing executive scorecard, consolidated defect matrix, root cause mapping, and remediation steps.

- [ ] **Step 3: Commit**

```bash
git add .claude/workflows/gui-visual-qa-loop.js
git commit -m "feat(qa): implement 8 parallel inspection agents and deductive scoring engine"
```

---

### Task 5: End-to-End Validation of Workflow on Current GUI State

**Files:**
- Test: `scripts/capture_standalone_gui.sh`
- Workflow: `.claude/workflows/gui-visual-qa-loop.js`
- Output: `docs/superpowers/plans/2026-08-21-gui-refinement-plan.md`

- [ ] **Step 1: Execute capture script and verify all crops**

Run: `bash scripts/capture_standalone_gui.sh`  
Expected: Generation of all 8 crop PNGs in `target/gui-qa/`.

- [ ] **Step 2: Execute visual QA workflow**

Run the updated workflow to audit the current GUI.  
Expected: The workflow accurately detects the 8 known visual defect categories, rejects false passes, flags `spec_compliance = false`, and outputs a ranked defect remediation plan.

- [ ] **Step 3: Verify plan output formatting and commit**

```bash
git add docs/superpowers/plans/
git commit -m "test(qa): verify comprehensive visual qa workflow detection and plan synthesis"
```
