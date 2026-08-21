export const meta = {
  name: "gui-visual-qa-loop",
  description: "Closed iteration loop for nih_plug_egui GUI visual QA with dual-matrix adversarial inspection",
  phases: [
    { title: "Capture", detail: "Compile standalone and capture multi-state GUI screenshots & crops", model: "haiku" },
    { title: "Visual QA", detail: "Parallel dual-matrix visual inspection (5 domain + 3 adversarial)", model: "sonnet" },
    { title: "Synthesize", detail: "Deductive scoring calculation and refinement plan generation" },
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
    overall_score: {
      type: "number",
      description: "Overall deductive visual quality score from 0 to 100"
    },
    spec_compliance: {
      type: "boolean",
      description: "True only if 0 critical, 0 major defects and score >= 95.0"
    },
    plan_path: {
      type: "string",
      description: "File path where refinement plan was written"
    },
    critical_defects_count: {
      type: "integer",
      description: "Number of critical severity defects"
    },
    major_defects_count: {
      type: "integer",
      description: "Number of major severity defects"
    },
    minor_defects_count: {
      type: "integer",
      description: "Number of minor severity defects"
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

const { auto_apply = false, max_iterations = 1, target_score = 95 } = args || {};

let iteration = 0;
let currentScore = 0;
let finalPlanPath = "";

while (iteration < max_iterations && currentScore < target_score) {
  iteration++;
  log(`Starting Comprehensive Visual QA iteration ${iteration}/${max_iterations}...`);

  // Phase 1: Capture (Haiku model)
  phase("Capture");
  const captureResult = await agent(
    "Run bash scripts/capture_standalone_gui.sh and verify that target/gui-qa/ snapshots and crops are generated (including latest.png, accurate.png, running.png, crop_header.png, crop_classic.png, crop_souped.png, crop_clipper_tweaks.png, crop_sequencer.png, crop_right_margin.png, crop_bottom_margin.png, and crop_state_diff.png). Run `date +%Y-%m-%d` to get current date_str.",
    { label: "capture:standalone-gui", phase: "Capture", model: "haiku", schema: CAPTURE_SCHEMA }
  );

  const dateStr = args?.date_str || (captureResult && captureResult.date_str ? captureResult.date_str : "2026-08-21");

  // Phase 2: Parallel Dual-Matrix Visual QA (Sonnet model)
  phase("Visual QA");
  const inspectors = [
    // Group A: 5 Domain Inspection Agents
    {
      name: "header_bar",
      prompt: "Read and analyze discrete image crop target/gui-qa/crop_header.png focusing on Header Bar & Branding. Compare against docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md. Evaluate: 1. Title branding consistency (TECHNEO vs TB-303 vs TB303), 2. Circuit topology selector switch (FUN / ACCURATE) positioning and alignment, 3. Polyphony / Voice count display, 4. Master Tuning dial, 5. Drift and Age readouts. Identify all defects with severity, category, observed vs expected, and suggested fix."
    },
    {
      name: "classic_panel",
      prompt: "Read and analyze discrete image crop target/gui-qa/crop_classic.png focusing on Classic Panel (Waveform Selector, 7 Classic Knobs, and ADSR Banks). Compare against docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md. Evaluate: 1. Waveform switch icon (is it a distinct waveform or a generic hamburger/menu glyph?), 2. Classic 7 rotary dials (Tuning, Cutoff, Resonance, Env Mod, Decay, Accent, Volume) with 270° radial ticks and value readouts, 3. ADSR sub-panel frame containment (does VEG/MEG overflow parent panel border?), 4. ADV ENV checkbox placement and negative space balance. Identify all defects with severity, category, observed vs expected, and suggested fix."
    },
    {
      name: "souped_drawer",
      prompt: "Read and analyze discrete image crops target/gui-qa/crop_souped.png (FUN mode) and target/gui-qa/accurate_crop_souped.png (ACCURATE mode) focusing on Souped-Up Engine Drawer. Compare against docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md. Evaluate: 1. Extended rotary dials (VCF Drive, Glide, Age, Drift, Tolerance, Noise, Seed), 2. Filter slope selector & VCA mode switch, 3. MEG / VEG faders, 4. Accurate mode 30% dimming consistency (do knob caps/sliders ghost through at uneven brightness?), 5. Accurate mode overlay badge placement and text contrast (is 'Souped-up engine is bypassed' dark grey on near-black?). Identify all defects with severity, category, observed vs expected, and suggested fix."
    },
    {
      name: "clipper_tweaks",
      prompt: "Read and analyze discrete image crop target/gui-qa/crop_clipper_tweaks.png focusing on Output Clipper and Tweaks Subpanel. Compare against docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md. Evaluate: 1. Clipper model selector, 2. GE / JFET toggle affordance (does it appear with a strikethrough-style line reading as disabled text?), 3. Drive and Ceiling dials, 4. Tweaks parameter units (are DRIVE/THRESH/BASS displaying raw potentiometer resistance like 47kΩ instead of perceptual %/dB?), 5. Seed parameter labeling consistency (TWEAKS SEED vs generator SEED). Identify all defects with severity, category, observed vs expected, and suggested fix."
    },
    {
      name: "sequencer",
      prompt: "Read and analyze discrete image crop target/gui-qa/crop_sequencer.png focusing on 16-Step Sequencer Matrix and Transport Column. Compare against docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md. Evaluate: 1. 16-step grid layout and button alignment, 2. Gate colors (NOTE, TIE, REST), 3. Chromatic pitch readouts, 4. Accent/Slide LED indicators, 5. Red dashes above each step (are they unexplained/confusing next to ACC dots?), 6. Bottom octave up/down arrow rows (do they touch the bottom boundary with zero padding?). Identify all defects with severity, category, observed vs expected, and suggested fix."
    },

    // Group B: 3 Adversarial Defect Hunters
    {
      name: "edge_containment",
      prompt: "ADVERSARIAL INSPECTOR: Read and analyze margin crops target/gui-qa/crop_right_margin.png and target/gui-qa/crop_bottom_margin.png. Hunt aggressively for clipping and containment failures: 1. Is any text cut off or truncated at the right window edge (e.g. '200 m' instead of '200 ms' in VEG ADSR)? 2. Does the QUALITY slider track run off the window edge past ULTRA? 3. Is the SEED hex readout in ACID EUCLIDEAN GENERATOR flush against or cut off by the right border? 4. Is the JIT checkbox jammed against the right edge with zero margin? 5. Do bottom octave arrow rows collide with the bottom edge? 6. Do ADSR sub-panels spill over parent borders? Log every breach as Critical or Major severity."
    },
    {
      name: "state_diff",
      prompt: "ADVERSARIAL INSPECTOR: Read and analyze composite diff strip target/gui-qa/crop_state_diff.png and compare target/gui-qa/latest.png, target/gui-qa/accurate.png, and target/gui-qa/running.png. Hunt aggressively for multi-state breakage: 1. In accurate.png, is there a large dead black band at the bottom (~10% window height)? 2. In accurate.png, is the middle drawer vertically compressed/squished compared to FUN mode? 3. Is the Accurate mode overlay box centered or floating awkwardly over central knobs? 4. Do disabled controls ghost through unevenly? 5. In running.png, is there active playhead feedback (highlighted step LED, moving cursor) or is it visually indistinguishable from stopped state? Log every state regression as Critical or Major severity."
    },
    {
      name: "affordance_semantics",
      prompt: "ADVERSARIAL INSPECTOR: Read and analyze full captures target/gui-qa/latest.png and target/gui-qa/running.png. Hunt aggressively for affordance, semantics, and unit defects: 1. Volume knob pointer angle vs numeric value: does the Volume dial point at ~4–5 o'clock for +0.0 dB while other centered controls (Tuning 0 ct, Pitch 0.00, Tone 1.00) point straight up at 12 o'clock? 2. Selection accent color consistency: is selected LOW quality highlighted in red/pink while all other selectors use orange? 3. Number formatting inconsistencies: mixed '0%' and '0.00', or leading zeros '01'-'09' vs '16' in LEN? 4. Potentiometer resistance values (47kΩ, 4.7kΩ, 8.2kΩ) used instead of perceptual amounts? 5. Strikethrough style on GE / JFET toggle reading as disabled text? Log every inconsistency."
    }
  ];

  const inspectorResults = await parallel(
    inspectors.map(inspector => () =>
      agent(inspector.prompt, {
        label: `qa:${inspector.name}`,
        phase: "Visual QA",
        model: "sonnet",
        schema: VISION_FINDINGS_SCHEMA
      })
    )
  );

  // Phase 3: Deductive Scoring & Plan Synthesis (Default model)
  phase("Synthesize");
  const planFilePath = `docs/superpowers/plans/${dateStr}-gui-refinement-plan.md`;
  const synthesis = await agent(
    `You are the Lead Visual QA Evaluator. Synthesize the 8 parallel inspector findings: ${JSON.stringify(inspectorResults)}.

Apply the DETERMINISTIC DEDUCTIVE SCORING ENGINE:
1. Count all unique defects by severity across all 8 inspectors:
   - N_critical = count of critical severity defects
   - N_major = count of major severity defects
   - N_minor = count of minor severity defects
2. Deductions = (25 * N_critical) + (10 * N_major) + (3 * N_minor)
3. Calculated Score = max(0.0, 100.0 - Deductions)
4. Quality Gate Enforcements:
   - If N_critical >= 1: spec_compliance MUST be FALSE, and overall_score CANNOT exceed 69.0 (overall_score = min(Calculated Score, 69.0)).
   - If N_major >= 2: spec_compliance MUST be FALSE, and overall_score CANNOT exceed 79.0 (overall_score = min(Calculated Score, 79.0)).
   - Passing threshold: spec_compliance = TRUE ONLY if N_critical == 0 AND N_major == 0 AND overall_score >= 95.0.

Write the comprehensive refinement plan to ${planFilePath} including:
- Section 1: Executive Scorecard with exact deduction arithmetic, defect counts, calculated score, and spec compliance status.
- Section 2: Consolidated Defect Matrix (Table: Element, Section, Severity, Category, Observed Defect, Expected State, Suggested Fix).
- Section 3: Root Cause Source Mapping (Exact files and line numbers in plugin303/src/gui/).
- Section 4: Step-by-Step Prioritized Remediation Plan.

Return the structured SYNTHESIS_SCHEMA.`,
    {
      label: "synthesize:deductive-scorer",
      phase: "Synthesize",
      schema: SYNTHESIS_SCHEMA
    }
  );

  currentScore = synthesis ? synthesis.overall_score : 0;
  finalPlanPath = synthesis ? synthesis.plan_path : planFilePath;
  log(`Iteration ${iteration} Visual QA Score: ${currentScore}% (Spec Compliance: ${synthesis?.spec_compliance})`);

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
