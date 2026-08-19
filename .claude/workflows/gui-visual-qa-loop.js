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
      prompt: "Analyze target/gui-qa/latest.png focusing on Souped-Up Drawer, MEG/VEG Faders, and Topology Dimming. Compare against docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md. Evaluate extended dials, faders, and accurate-mode dimming badge. Also analyze target/gui-qa/accurate.png if present: it shows Accurate mode, where the drawer must be ghosted to ~30% content visibility (pixel-sample the MEG fader fill) with a legible badge and SWITCH TO FUN button. Note: latest.png may show FUN mode where the drawer is fully vibrant — that is the default topology, not a defect."
    },
    {
      name: "sequencer",
      prompt: "Analyze target/gui-qa/latest.png focusing on 16-Step Sequencer Strip, LED states, ACC/SLD toggles, and Gate colors. Compare against docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md. Evaluate step columns, button alignment, pitch readability, and transport bar. Also analyze target/gui-qa/running.png if present: it shows the transport actively running — verify exactly one playhead LED bar glows neon coral (#ff3355) while all other steps stay dim, and note the lit step number. Note: in latest.png the transport may be stopped (RUN button orange with '▶ RUN' label = stopped/armed state, all playhead bars dim = correct); a stopped transport with all LEDs dim is correct behavior, not a defect."
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
    `Synthesize visual QA findings: ${JSON.stringify(lensResults)}. Compare against design spec docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md. Calculate overall weighted score (0.35 classic + 0.30 souped + 0.35 sequencer) and write a complete, actionable refinement plan to docs/superpowers/plans/2026-08-14-gui-refinement-plan.md.`,
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
