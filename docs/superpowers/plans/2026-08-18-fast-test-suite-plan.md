# Make `cargo test` fast: profile tuning + feature gating + nextest partitioning

## Context

`plugin303` test suite (72 unit + ~122 integration tests) too slow for iteration. Measured debug runtimes: `tier_calibration` 214s, `ultra_spectral` 49s, `warm_clipper` >40s — plus compile time.

Root causes:
1. **No test profile tuning** — Newton-solver DSP runs at opt-level 0; tests render 10k–144k samples each, often x3 tiers.
2. **GUI stack in every test build** — default features `["gui", "standalone"]` pull egui/glow/winit/baseview; only `gui_build.rs` + `piano_roll_mapping.rs` touch GUI.
3. **9 auxiliary bins built by `cargo test`** — none contain `#[test]`, pure overhead.
4. Hand-rolled FFT in `tests/common/mod.rs` amplifies (1) — follow-up only.

Scope decisions (user): cargo test only; `--no-default-features` OK; cargo-nextest approved.

Verified facts (Plan agent checked the tree):
- `src/lib.rs` already feature-gates all GUI code — no lib changes needed.
- Two `--no-default-features` blockers: `src/bin/standalone.rs` needs `nih_plug/standalone`; `tests/piano_roll_mapping.rs:1-5` imports `plugin303::gui::widgets` unconditionally.
- `tests/gui_build.rs` already has per-fn `#[cfg(feature = "gui")]` — fine as-is.
- `no_alloc.rs` assertions all `violation_count() == 0` — safe under optimization. Needs `debug-assertions = true` kept (assert_no_alloc `warn_debug` compiles under `cfg(debug_assertions)`).
- No test relies on overflow panics; no test shells out to bins; CI does not run `cargo test`.
- nextest: `default-filter` per-profile filterset; `binary_id` like `plugin303::tier_calibration` (verify with `cargo nextest list`); nextest builds with cargo's test profile so `[profile.test]` applies to both runners.

## Steps

### 1. Profile tuning — `/Users/gus/Git/vibe303/Cargo.toml` (append)

```toml
# Tests render 10k-144k samples of Newton-solver DSP; opt-level 0 makes them
# 5-15x slower. debug-assertions MUST stay on: assert_no_alloc's warn_debug
# (tests/no_alloc.rs) is compiled under cfg(debug_assertions).
[profile.test]
inherits = "dev"
opt-level = 1            # dependencies: cheap to compile
debug-assertions = true
overflow-checks = false  # verified: no test relies on overflow panics

# plugin303's own DSP is the hot loop; opt-3 only this crate, not egui/nih_plug.
[profile.test.package.plugin303]
opt-level = 3
```

No LTO, no codegen-units change (33 test binaries = link cost).

### 2. Feature gating — `plugin303/tests/piano_roll_mapping.rs`

Add as new line 1: `#![cfg(feature = "gui")]`
(Optional symmetry: hoist gui_build.rs per-fn gate to file level.)

### 3. Bin build cost — `plugin303/Cargo.toml`

```toml
# standalone needs nih_plug/standalone; without this, --no-default-features
# test builds fail.
[[bin]]
name = "standalone"
required-features = ["standalone"]

# Bins are test-exempt by policy (no #[test] in any bin; skips codegen+link
# of 9 bins in test builds). Does not affect `cargo run --bin ...`.
[[bin]]
name = "debug_note84"
test = false
# ... same `test = false` for: eval_clipper, gen_acid, measure_swing,
#     render303, scratch_pitch, scratch_voice, vco_pitch_sweep
```

### 4. Nextest partitioning — new file `.config/nextest.toml`

Do AFTER step 1 + re-measure (opt bump reshuffles slow set). Verify IDs with `cargo nextest list`.

```toml
[profile.default]
# Fast suite. Slow set = spectral/calibration render gates.
default-filter = '''not (
    binary_id(plugin303::tier_calibration) |
    binary_id(plugin303::ultra_spectral) |
    binary_id(plugin303::warm_clipper) |
    binary_id(plugin303::vcf303_fit) |
    binary_id(plugin303::high_spectral) |
    binary_id(plugin303::voice_esr) |
    binary_id(plugin303::tier_levels) |
    binary_id(plugin303::env_curve)
)'''
slow-timeout = { period = "10s", terminate-after = 6 }

[profile.slow]
default-filter = 'all()'
slow-timeout = { period = "60s", terminate-after = 10 }
```

No `#[ignore]` attributes — plain `cargo test` stays the run-everything path (~60-120s after step 1); one source of truth.
Refinement loop: run slow profile once, move any binary > ~15s into exclusion list (candidates: `ultra_esr`, `ultra_stability`, `age_drift` — measure, don't guess).

### 5. Docs — `README.md` "Running Tests" (~lines 90-97)

Document: `cargo nextest run -p plugin303 --no-default-features` (fast), `--profile slow` (full), plain `cargo test` paths, note that `--no-default-features` skips gui-gated tests (run `cargo test` with default features to cover `gui_build`/`piano_roll_mapping`).

## Follow-ups (NOT this change)

- Replace hand-rolled FFT with `realfft` in `tests/common/mod.rs` + hoist Welch allocs — only if slow profile still shows FFT time.
- De-duplicate per-tier renders in tier_calibration/ultra_spectral — separate PR, touches test semantics.

## Risks

- **Float behavior at opt-level 3**: low risk (Rust: no reassociation/FMA contraction) but run full slow suite once after step 1, diff pass/fail vs baseline before landing partition.
- **overflow-checks off**: could mask future overflow bugs in test builds; dev `cargo build` unaffected.
- **binary_id syntax**: verify with `cargo nextest list`; eyeball listed vs run test counts.
- **`test = false` on bins**: future `#[test]` in a bin silently skipped — Cargo.toml comment covers it.

## Verification

Baseline first: `time cargo test -p plugin303` (full suite).

After steps 1–3:
- `cargo check -p plugin303 --no-default-features --all-targets` — clean
- `time cargo test -p plugin303 --no-default-features` — full suite green, expect ~60-120s
- `cargo test -p plugin303 --no-default-features --test no_alloc` — alloc gates pass

After step 4:
- `cargo nextest list -p plugin303 --no-default-features` — count sanity
- `time cargo nextest run -p plugin303 --no-default-features` — target < 60s
- `cargo nextest run -p plugin303 --no-default-features --profile slow` — full suite green
- `cargo test -p plugin303 --test gui_build --test piano_roll_mapping` (default features) — GUI tests still run

Success: fast suite < 60s; full suite green; no test-count regression (~194 total across profiles).
