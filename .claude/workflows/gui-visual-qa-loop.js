export const meta = {
  name: "gui-visual-qa-loop",
  description: "Closed iteration loop for nih_plug_egui GUI visual QA and refinement",
  phases: [
    { title: "Capture", detail: "Compile standalone and capture GUI screenshot", model: "haiku" },
    { title: "Visual QA", detail: "Parallel multi-lens visual inspection", model: "sonnet" },
    { title: "Synthesize", detail: "Aggregate findings and generate refinement plan" },
    { title: "Iterative Fix", detail: "Apply code edits and verify build (optional)" }
  ]
};

const CAPTURE_SCHEMA = {
  type: "object",
  properties: {
    success: { type: "boolean" },
    date_str: { type: "string", description: "Current date in YYYY-MM-DD format" }
  },
  required: ["success", "date_str"]
};

const VISION_FINDINGS_SCHEMA = {
  type: "object",
  properties: {
    section: {
      type: "string",
      enum: ["header_bar", "classic_panel", "souped_drawer", "clipper_tweaks", "sequencer"]
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

const { auto_apply = false, max_iterations = 1, target_score = 95 } = args || {};

let iteration = 0;
let currentScore = 0;
let finalPlanPath = "";

while (iteration < max_iterations && currentScore < target_score) {
  iteration++;
  log(`Starting Visual QA iteration ${iteration}/${max_iterations}...`);

  // Phase 1: Capture (Haiku model)
  phase("Capture");
  const captureResult = await agent(
    "Run bash scripts/capture_standalone_gui.sh and verify that target/gui-qa/ snapshots and crops are generated (including latest_crop_header.png, latest_crop_classic.png, latest_crop_souped.png, latest_crop_clipper_tweaks.png, latest_crop_sequencer.png, accurate_crop_souped.png, running_crop_sequencer.png, and broad zones). Run `date +%Y-%m-%d` to get current date_str.",
    { label: "capture:standalone-gui", phase: "Capture", model: "haiku", schema: CAPTURE_SCHEMA }
  );

  const dateStr = args?.date_str || (captureResult && captureResult.date_str ? captureResult.date_str : "2026-08-21");

  // Phase 2: Parallel Visual QA (Sonnet model)
  phase("Visual QA");
  const lenses = [
    {
      name: "header_bar",
      prompt: "Analyze discrete crop target/gui-qa/latest_crop_header.png and broad zone target/gui-qa/latest_zone_top.png focusing on Header Bar. Compare against docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md. Evaluate VIBE-303 title typography/branding, FUN / ACCURATE mode selector switch, Polyphony / Voice count, Master Tuning, Drift / Age readouts, and Output Level knob."
    },
    {
      name: "classic_panel",
      prompt: "Analyze discrete crop target/gui-qa/latest_crop_classic.png and broad zone target/gui-qa/latest_zone_top.png focusing on Vertical Waveform Switch and Classic 7-Knob + ADSR Panel. Compare against docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md and docs/plans/2026-07-02-ui-mockup.jpg. Evaluate vertical slide switch (SAW/SQR/SWP), 7 classic dials (Tuning, Cutoff, Reso, Env Mod, Decay, Accent, Distortion) tick marks, labels, and ADSR fader controls."
    },
    {
      name: "souped_drawer",
      prompt: "Analyze discrete crops target/gui-qa/latest_crop_souped.png and target/gui-qa/accurate_crop_souped.png, alongside broad zone target/gui-qa/latest_zone_mid.png focusing on Souped-Up Engine Drawer. Compare against docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md. In latest_crop_souped.png (FUN mode), evaluate extended dials (VCF Exp, Cutoff Offset, Reso Bias, Drive/Saturate) and MEG/VEG Attack/Decay/Sustain/Release faders. In accurate_crop_souped.png (Accurate mode), verify ~30% ghosting/dimming on MEG fader fill, legible accurate dimming badge, and active 'SWITCH TO FUN' button."
    },
    {
      name: "clipper_tweaks",
      prompt: "Analyze discrete crop target/gui-qa/latest_crop_clipper_tweaks.png and broad zone target/gui-qa/latest_zone_mid.png focusing on Output Clipper and Tweaks Subpanel. Compare against docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md. Evaluate Clipper model selector (Off, Germanium, Warm JFET, Diode Clip, Hard), Drive/Ceiling controls, and Tweaks panel parameters (Accent behavior, High-pass mod, Slide time, Mod/Age options)."
    },
    {
      name: "sequencer",
      prompt: "Analyze discrete crops target/gui-qa/latest_crop_sequencer.png and target/gui-qa/running_crop_sequencer.png, alongside broad zone target/gui-qa/latest_zone_bottom.png focusing on 16-Step Sequencer Grid and Transport Column. Compare against docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md. In latest_crop_sequencer.png, verify 16-step button layouts, pitch labels, ACC/SLD toggles, and Gate colors (NOTE = #00e5ff, TIE = #ffd700, REST = #333a42 / theme::GATE_REST_COLOR). In running_crop_sequencer.png, verify running transport playhead LED bar glows neon coral (#ff3355) on active step 4 while other step LEDs remain dim."
    }
  ];

  const lensResults = await parallel(
    lenses.map(lens => () =>
      agent(lens.prompt, {
        label: `qa:${lens.name}`,
        phase: "Visual QA",
        model: "sonnet",
        schema: VISION_FINDINGS_SCHEMA
      })
    )
  );

  // Phase 3: Synthesize & Plan (Default model)
  phase("Synthesize");
  const planFilePath = `docs/superpowers/plans/${dateStr}-gui-refinement-plan.md`;
  const synthesis = await agent(
    `Synthesize visual QA findings: ${JSON.stringify(lensResults)}. Compare against design spec docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md. Calculate overall weighted score (0.15 header_bar + 0.25 classic_panel + 0.20 souped_drawer + 0.15 clipper_tweaks + 0.25 sequencer) and write a complete, actionable refinement plan to ${planFilePath}.`,
    {
      label: "synthesize:plan-generator",
      phase: "Synthesize",
      schema: SYNTHESIS_SCHEMA
    }
  );

  currentScore = synthesis ? synthesis.overall_score : 0;
  finalPlanPath = synthesis ? synthesis.plan_path : planFilePath;
  log(`Iteration ${iteration} Visual QA Score: ${currentScore}% (Target: ${target_score}%)`);

  if (!auto_apply || currentScore >= target_score) {
    break;
  }

  // Phase 4: Iterative Fix (Default model)
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
