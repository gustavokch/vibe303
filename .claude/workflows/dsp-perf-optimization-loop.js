export const meta = {
  name: "dsp-perf-optimization-loop",
  description: "Closed iteration loop for DSP optimization: baseline profiling -> surgical implementation -> multi-gate verification -> differential benchmark -> go/no-go synthesis",
  phases: [
    { title: "Baseline", detail: "Run baseline timing and iteration benchmark across tiers" },
    { title: "Candidate Design", detail: "Formulate candidate optimization plan and invariant constraints" },
    { title: "Implementation", detail: "Apply surgical DSP code edits" },
    { title: "Gate Verification", detail: "Run parallel quality gates: unit tests, no_alloc, spectral LSD, alias/iters" },
    { title: "Differential Benchmark", detail: "Measure post-edit timing and calculate exact performance delta" },
    { title: "Synthesis", detail: "Decide Go/No-Go, record changelog, and summarize verified speedup" }
  ]
};

// ──────────────── Configuration ────────────────
const PROFILE_BIN = "./target/profiling/profile_voice";
const REPORT_PATH = "docs/profiling/cpu-profile-report.md";

// Args: candidate target name or priority (e.g., "fir-symmetry", "bjt-batch", "static-lu-fma", "clipper-minimax")
const targetCandidate = args?.target ?? "fir-symmetry";
const secs = args?.secs ?? 10;
const seed = args?.seed ?? "0xAC1D";

// ═══════════════════════════════════════════════════════════════════════════
//  Phase 1: Baseline — establish pre-edit performance metrics
// ═══════════════════════════════════════════════════════════════════════════
phase("Baseline");

const baselineResult = await agent(
  `Run the profiling binary to establish pre-edit baseline timing across all tiers:
   Command:
     ${PROFILE_BIN} --secs ${secs} --seed ${seed}

   Parse the output for Draft, Low, Best, and Sweep tiers.
   Extract:
   - Throughput (kS/s)
   - Realtime ratio (x)
   - Elapsed time (s)

   Return structured baseline metrics.`,
  {
    label: "baseline:benchmark",
    phase: "Baseline",
    schema: {
      type: "object",
      properties: {
        best_ratio: { type: "number" },
        best_ksps: { type: "number" },
        low_ratio: { type: "number" },
        low_ksps: { type: "number" },
        draft_ratio: { type: "number" },
        draft_ksps: { type: "number" },
        sweep_ratio: { type: "number" },
        sweep_ksps: { type: "number" }
      },
      required: ["best_ratio", "best_ksps", "low_ratio", "low_ksps", "draft_ratio", "draft_ksps"]
    }
  }
);

log(`Baseline Best: ${baselineResult.best_ratio}x realtime (${baselineResult.best_ksps} kS/s)`);

// ═══════════════════════════════════════════════════════════════════════════
//  Phase 2: Candidate Design — select optimization and verify invariants
// ═══════════════════════════════════════════════════════════════════════════
phase("Candidate Design");

const designResult = await agent(
  `You are a DSP performance architect preparing an optimization step for target: "${targetCandidate}".

   Review the hotspot analysis from ${REPORT_PATH} and the relevant DSP source files:
   - Candidate "fir-symmetry": plugin303/src/dsp/oversample.rs (symmetric MACs + circular buffer)
   - Candidate "bjt-batch": plugin303/src/dsp/bjt.rs & plugin303/src/dsp/vco.rs (vectorized 12-junction batching)
   - Candidate "static-lu-fma": plugin303/src/dsp/linalg_static.rs (FMA intrinsics / register-pinned LU)
   - Candidate "clipper-minimax": plugin303/src/dsp/warm_clipper.rs (rational minimax tanh)
   - Candidate "ladder-warmstart": plugin303/src/dsp/ladder_wdf.rs (linear extrapolation initial guess)

   Requirements:
   1. Specify the exact files and functions to modify.
   2. Identify non-negotiable invariants (bit-identity, RT no-alloc, LSD budget <= 4.0 dB).
   3. Detail the step-by-step mathematical and code transformation.`,
  {
    label: "design:optimization-spec",
    phase: "Candidate Design",
    schema: {
      type: "object",
      properties: {
        candidate_name: { type: "string" },
        target_files: { type: "array", items: { type: "string" } },
        expected_speedup_pct: { type: "number" },
        risk_level: { type: "string", enum: ["low", "medium", "high"] },
        invariants: { type: "array", items: { type: "string" } },
        implementation_steps: { type: "array", items: { type: "string" } }
      },
      required: ["candidate_name", "target_files", "expected_speedup_pct", "risk_level", "invariants", "implementation_steps"]
    }
  }
);

log(`Design finalized for ${designResult.candidate_name}: expected speedup ~${designResult.expected_speedup_pct}%`);

// ═══════════════════════════════════════════════════════════════════════════
//  Phase 3: Implementation — apply surgical DSP changes
// ═══════════════════════════════════════════════════════════════════════════
phase("Implementation");

const implResult = await agent(
  `Apply the optimization specified in the design phase for "${designResult.candidate_name}".
   Target files: ${JSON.stringify(designResult.target_files)}
   Steps: ${JSON.stringify(designResult.implementation_steps)}

   Guidelines:
   - Keep comments concise and match surrounding code style.
   - Do NOT introduce heap allocations in any audio rendering method.
   - Recompile the profiling binary:
     RUSTFLAGS='-C force-frame-pointers=yes' cargo build --profile profiling --no-default-features --bin profile_voice

   Report: files modified, compilation status, warnings or errors resolved.`,
  {
    label: "impl:apply-code-changes",
    phase: "Implementation",
    schema: {
      type: "object",
      properties: {
        success: { type: "boolean" },
        modified_files: { type: "array", items: { type: "string" } },
        summary: { type: "string" }
      },
      required: ["success", "modified_files", "summary"]
    }
  }
);

if (!implResult.success) {
  throw new Error(`Implementation compilation failed: ${implResult.summary}`);
}

// ═══════════════════════════════════════════════════════════════════════════
//  Phase 4: Gate Verification — parallel test quality gates
// ═══════════════════════════════════════════════════════════════════════════
phase("Gate Verification");

const gateChecks = [
  {
    name: "unit-tests",
    cmd: "cargo test -p plugin303 --lib",
    desc: "Functional DSP unit tests (62 tests)"
  },
  {
    name: "no-alloc",
    cmd: "cargo test -p plugin303 --test no_alloc",
    desc: "Real-time thread safety & zero heap allocation verification"
  },
  {
    name: "spectral-gold",
    cmd: "cargo test -p plugin303 --test best_spectral --test low_spectral",
    desc: "Spectral LSD vs SPICE gold (LSD <= 4.0 dB, corr >= 0.95)"
  },
  {
    name: "aliasing-iters",
    cmd: "cargo test -p plugin303 --test ladder_alias --test vco_alias --test newton_iters",
    desc: "Anti-aliasing bounds and Newton iteration convergence"
  }
];

const gateResults = await parallel(
  gateChecks.map(gate => () =>
    agent(
      `Execute quality gate check:
       Command: ${gate.cmd}
       Description: ${gate.desc}

       Parse stdout and stderr to verify if all tests passed without failure.
       Report status: { passed: boolean, details: string }`,
      {
        label: `gate:${gate.name}`,
        phase: "Gate Verification",
        schema: {
          type: "object",
          properties: {
            passed: { type: "boolean" },
            details: { type: "string" }
          },
          required: ["passed", "details"]
        }
      }
    )
  )
);

const allGatesPassed = gateResults.every(r => r?.passed);
log(`Quality Gates: ${allGatesPassed ? "ALL PASSED" : "GATE FAILED"}`);

// ═══════════════════════════════════════════════════════════════════════════
//  Phase 5: Differential Benchmark — measure exact performance delta
// ═══════════════════════════════════════════════════════════════════════════
phase("Differential Benchmark");

let postResult;
if (allGatesPassed) {
  postResult = await agent(
    `Run post-edit differential benchmark across all tiers:
     Command:
       ${PROFILE_BIN} --secs ${secs} --seed ${seed}

     Parse Draft, Low, Best, and Sweep results.
     Extract throughput and realtime ratios.`,
    {
      label: "benchmark:differential",
      phase: "Differential Benchmark",
      schema: {
        type: "object",
        properties: {
          best_ratio: { type: "number" },
          best_ksps: { type: "number" },
          low_ratio: { type: "number" },
          low_ksps: { type: "number" },
          draft_ratio: { type: "number" },
          draft_ksps: { type: "number" }
        },
        required: ["best_ratio", "best_ksps", "low_ratio", "low_ksps", "draft_ratio", "draft_ksps"]
      }
    }
  );
} else {
  postResult = {
    best_ratio: 0.0,
    best_ksps: 0.0,
    low_ratio: 0.0,
    low_ksps: 0.0,
    draft_ratio: 0.0,
    draft_ksps: 0.0
  };
}

// ═══════════════════════════════════════════════════════════════════════════
//  Phase 6: Synthesis — Go / No-Go decision & changelog recording
// ═══════════════════════════════════════════════════════════════════════════
phase("Synthesis");

const synthesisResult = await agent(
  `Analyze the pre- vs post-optimization results:

   Baseline:
   ${JSON.stringify(baselineResult, null, 2)}

   Post-Edit:
   ${JSON.stringify(postResult, null, 2)}

   Quality Gates Status:
   ${JSON.stringify(gateResults, null, 2)}

   Decision Criteria:
   - "GO (Ship)": All quality gates passed AND Best realtime ratio improved (delta > 0).
   - "NO-GO (Revert)": Any quality gate failed OR performance regressed (delta <= 0).

   Update ${REPORT_PATH} under a new section for this iteration:
   - Candidate tested
   - Pre vs Post timing comparison table
   - Quality gate verification status
   - Decision (Ship or Revert)

   If NO-GO is decided, revert the modified files using git checkout.

   Return the final structured decision summary.`,
  {
    label: "synthesis:decision-and-log",
    phase: "Synthesis",
    schema: {
      type: "object",
      properties: {
        decision: { type: "string", enum: ["GO", "NO_GO"] },
        best_speedup_pct: { type: "number" },
        baseline_best_ratio: { type: "number" },
        post_best_ratio: { type: "number" },
        summary: { type: "string" },
        revert_performed: { type: "boolean" }
      },
      required: ["decision", "best_speedup_pct", "baseline_best_ratio", "post_best_ratio", "summary"]
    }
  }
);

log(`Final Synthesis Decision: ${synthesisResult.decision} (${synthesisResult.best_speedup_pct.toFixed(2)}% Best speedup)`);

return {
  candidate: targetCandidate,
  baseline: baselineResult,
  post: postResult,
  gates_passed: allGatesPassed,
  synthesis: synthesisResult
};
