export const meta = {
  name: "dsp-cpu-profile",
  description: "Profile the Voice DSP pipeline to identify CPU optimization targets across tiers, topologies, and parameter regimes",
  phases: [
    { title: "Bootstrap", detail: "Install profile_voice binary and compile with profiling profile" },
    { title: "Baseline Timing", detail: "Run per-tier and sweep timing passes to establish realtime ratios" },
    { title: "Profiled Run", detail: "Run with samply/dtrace to capture CPU profile data" },
    { title: "Analysis", detail: "Analyze profile, correlate with DSP modules, rank optimization targets" }
  ]
};

// ──────────────── Configuration ────────────────
const PROFILE_BIN_SRC = "profile_voice.rs";
const PROFILE_BIN_TARGET = "plugin303/src/bin/profile_voice.rs";
const CARGO_TOML = "plugin303/Cargo.toml";
const REPORT_PATH = "docs/profiling/cpu-profile-report.md";

// Allowlist the tier name before any shell-string interpolation.
const TIER_NAMES = ["draft", "low", "high", "best"];
const tierRaw = String(args?.tier ?? "").toLowerCase();
const secsRaw = args?.secs ?? 10;
const samply = args?.samply !== false;
const tier = TIER_NAMES.includes(tierRaw) ? tierRaw : "best";
const secs = Number.isFinite(Number(secsRaw))
  ? Math.max(1, Math.min(300, Math.round(Number(secsRaw))))
  : 10;

// ═══════════════════════════════════════════════════════════════════════════
//  Phase 1: Bootstrap — install the profiling binary and compile
// ═══════════════════════════════════════════════════════════════════════════
phase("Bootstrap");

// Step 1a: Copy the profile_voice.rs harness into the bin directory
const installResult = await agent(
  `Check if ${PROFILE_BIN_TARGET} already exists. If not, look for profile_voice.rs
   in the Claude brain artifacts directory (search for it with find) and copy it to
   ${PROFILE_BIN_TARGET}. If it doesn't exist anywhere, create it by reading the
   artifact from the most recent conversation that generated it.

   Then ensure Cargo.toml at ${CARGO_TOML} has this entry (add it if missing,
   do NOT duplicate):

   [[bin]]
   name = "profile_voice"
   test = false

   Report whether the file was created or already existed.`,
  { label: "bootstrap:install-harness", phase: "Bootstrap" }
);

// Step 1b: Compile with the profiling profile
const compileResult = await agent(
  `Compile the profiling binary with full optimisation + debug symbols + frame pointers.
   Run this exact command from the workspace root:

   RUSTFLAGS='-C force-frame-pointers=yes' cargo build --profile profiling --no-default-features --bin profile_voice

   If compilation fails:
   1. Read the error carefully
   2. If it's a missing import or API mismatch, check the current voice.rs API and fix profile_voice.rs
   3. Retry compilation

   Report: compilation status, binary path, binary size.`,
  { label: "bootstrap:compile", phase: "Bootstrap" }
);

// ═══════════════════════════════════════════════════════════════════════════
//  Phase 2: Baseline Timing — per-tier realtime ratios
// ═══════════════════════════════════════════════════════════════════════════
phase("Baseline Timing");

// Run all 4 timing configurations in parallel
const tierConfigs = [
  { name: "best",  tierFlag: "--tier best",  desc: "Best (circuit VCO + nodal ladder 4× OS)" },
  { name: "low",   tierFlag: "--tier low",   desc: "Low (polyBLEP + nodal ladder 2× OS)" },
  { name: "draft", tierFlag: "--tier draft", desc: "Draft (polyBLEP + TPT ladder, no OS)" },
  { name: "sweep", tierFlag: "",          desc: "Tier sweep (all tiers, topology cycling)" },
];

const timingResults = await parallel(
  tierConfigs.map(cfg => () =>
    agent(
      `Run the profiling binary for a timing baseline (no profiler attached).
       Command:
         ./target/profiling/profile_voice --secs ${secs} ${cfg.tierFlag} --seed 0xAC1D

       Parse the stderr output and extract:
       - Elapsed time (seconds)
       - Throughput (samples/sec)
       - Realtime ratio (×)
       - Status (REALTIME or UNDERRUN)

       Return a structured summary: { tier: "${cfg.name}", desc: "${cfg.desc}", elapsed_s, throughput, realtime_ratio, status }`,
      {
        label: `timing:${cfg.name}`,
        phase: "Baseline Timing",
        schema: {
          type: "object",
          properties: {
            tier: { type: "string" },
            desc: { type: "string" },
            elapsed_s: { type: "number" },
            throughput: { type: "number" },
            realtime_ratio: { type: "number" },
            status: { type: "string" }
          },
          required: ["tier", "elapsed_s", "throughput", "realtime_ratio", "status"]
        }
      }
    )
  )
);

log(`Timing results: ${JSON.stringify(timingResults, null, 2)}`);

// ═══════════════════════════════════════════════════════════════════════════
//  Phase 3: Profiled Run — capture CPU profile with samply or dtrace
// ═══════════════════════════════════════════════════════════════════════════
phase("Profiled Run");

const profileTier = `--tier ${tier}`; // default profile Best (heaviest)
const profileSecs = Math.max(secs, 30); // at least 30s for good sample coverage

let profileResult;
if (samply) {
  profileResult = await agent(
    `Run a CPU-profiled session of the Voice DSP using samply.

     First check if samply is installed: \`which samply\`
     If not installed, install it: \`cargo install samply\`

     Then run:
       samply record --save-only -o target/profiling/voice-profile.json -- \\
         ./target/profiling/profile_voice --secs ${profileSecs} ${profileTier} --seed 0xAC1D

     If samply fails or is unavailable, fall back to dtrace:
       sudo dtrace -x ustackframes=100 -n 'profile-997 /pid == $target/ { @[ustack()] = count(); }' \\
         -c './target/profiling/profile_voice --secs ${profileSecs} ${profileTier} --seed 0xAC1D' \\
         -o target/profiling/voice-profile-dtrace.txt

     Report: which profiler was used, output file path, file size, any errors.`,
    { label: "profile:samply-run", phase: "Profiled Run" }
  );
} else {
  profileResult = await agent(
    `Run the profiling binary with simple wall-clock timing (no external profiler).
     Command:
       ./target/profiling/profile_voice --secs ${profileSecs} ${profileTier} --seed 0xAC1D

     Report the full stderr output.`,
    { label: "profile:timing-only", phase: "Profiled Run" }
  );
}

// ═══════════════════════════════════════════════════════════════════════════
//  Phase 4: Analysis — correlate profile data with DSP source, rank targets
// ═══════════════════════════════════════════════════════════════════════════
phase("Analysis");

const analysisResult = await agent(
  `You are a DSP performance engineer analyzing the CPU profile of a TB-303 voice synthesizer.

   ## Context

   The voice architecture (plugin303/src/dsp/voice.rs) is a monophonic synth with 4 fidelity tiers:

   ### Tier::Best (heaviest)
   - **VCO** (vco.rs): Circuit VCO: nodal MNA + Newton solver, 17-unknown 6-BJT + JFET relaxation oscillator.
     Per-sample: stamp 17×17 Jacobian, LU solve, Newton iterations (up to 100, typically 3-8).
     Has pitch LUT (37-point secant-calibrated) rebuilt on age/tolerance changes.
   - **LadderVcf** (ladder_wdf.rs): Nodal 12-node diode-ladder, 8× 1N4148 diodes + 4 shunt caps.
     Per-sample: stamp + Newton (up to 50 iters), runs at 4× oversampled rate.
     That means 4 ladder.tick() calls per voice sample.
   - **Os4x** (oversample.rs): Kaiser FIR up/down at 4×.
   - **WarmClipper** (warm_clipper.rs): 4-stage Germanium/JFET output clipper with internal 2×/4× OS.

   ### Tier::High
   - Same circuit VCO as Best, feeding **LadderVcf** at 2× OS (2 ladder.tick() calls per sample).
   - **Os2x**: Kaiser FIR at 2×.

   ### Tier::Low
   - **BlepOsc** (osc.rs): polyBLEP oscillator (cheap).
   - **LadderVcf** at 2× OS (2 ladder.tick() calls per sample).
   - **Os2x**: Kaiser FIR at 2×.

   ### Tier::Draft
   - **BlepOsc** + **TptLadder** (ladder.rs): textbook TPT, no Newton, no oversampling, very cheap.

   ### Common per-sample work
   - Envelope chain: DecayEnv, LeakyIntegrator, AccentSweep, RcEnv (rc_env.rs)
   - DriftGenerators (drift.rs): 6 instances, pink-noise random walks
   - Pitch slew (LeakyIntegrator)
   - Frequency computation: 2× powf() calls
   - Filter cutoff modulation: polynomial evaluation, powf() conversions

   ## Timing Results
   ${JSON.stringify(timingResults, null, 2)}

   ## Profile Data
   Check if target/profiling/voice-profile.json exists (samply output).
   If so, read it and extract the top functions by self-time.
   Also check target/profiling/voice-profile-dtrace.txt for dtrace stacks.

   If no profile data files exist, perform analysis based on the timing ratios
   and your knowledge of the DSP architecture.

   ## Your Task

   1. **Read the DSP source files** to understand the hot paths:
      - plugin303/src/dsp/vco.rs (look for the tick() method and Newton loop)
      - plugin303/src/dsp/ladder_wdf.rs (look for tick() and the Newton loop)
      - plugin303/src/dsp/warm_clipper.rs (look for tick() and the per-stage processing)
      - plugin303/src/dsp/oversample.rs (look for up()/down() FIR convolutions)
      - plugin303/src/dsp/voice.rs (look for render_tier() and tick())
      - plugin303/src/dsp/linalg_static.rs (look for vco_lu_solve, ladder_lu_solve)
      - plugin303/src/dsp/bjt.rs, diode.rs, jfet.rs (device model stamp functions)
      - plugin303/src/dsp/expmath.rs (fast math approximations)
      - plugin303/src/dsp/rc_env.rs (envelope chain)
      - plugin303/src/dsp/drift.rs (drift generators)

   2. **Estimate the per-sample cost breakdown** for each tier based on:
      - Newton iteration counts × linear solve size (17×17 for VCO, 12×12 for ladder)
      - Oversampling factor (4× for Best ladder means 4 Newton solves per sample)
      - Number of exp/log/powf transcendental calls per sample
      - FIR filter tap counts in the oversampler

   3. **Rank optimization targets** from highest to lowest impact, considering:
      - Wall-clock contribution (% of total)
      - Feasibility of optimization (algorithmic vs micro-optimization)
      - Risk to audio quality

   4. **Write a comprehensive report** to ${REPORT_PATH} with:
      - Executive summary (1 paragraph)
      - Timing table (all tiers)
      - Architecture diagram (mermaid)
      - Ranked hotspot table with estimated % contribution
      - Detailed analysis of top 5 optimization candidates
      - Specific, actionable recommendations with expected impact
      - Risk assessment for each recommendation

   Format the report in markdown with proper headings, tables, and code references
   (link to source files with line numbers where relevant).`,
  { label: "analysis:correlate-and-report", phase: "Analysis" }
);

// ═══════════════════════════════════════════════════════════════════════════
//  Return summary
// ═══════════════════════════════════════════════════════════════════════════
return {
  timing: timingResults,
  profile_method: samply ? "samply" : "wall-clock",
  report_path: REPORT_PATH,
  analysis: analysisResult
};
