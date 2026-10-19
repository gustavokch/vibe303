# PR #72 Review Remediation

**Goal:** Fix the findings posted on PR #72 (tier rename): High↔Best and Low↔High crossfades must not double-tick shared DSP state.

**Architecture:** Each tier owns its filter path (oversampler, ladder, compensation delay). High and Best share only the circuit oscillator source (`vco`, `vco2`, `sub_osc_circuit`), which `Voice` ticks once per sample and feeds to every active circuit path. The setter-side ladder rebuild (`rebuild_circuit_ladder_if_needed`) goes away: `ladder_high` (2x) and `ladder_best` (4x) are built once in `Voice::new`.

**Tech stack:** Rust, `plugin303` crate. Gate: `cargo test -p plugin303 --release`.

**Spec reference:** PR #72 review comment; `.agents/AGENTS.md` (zero-alloc audio path, click-free tier switch).

## Findings

| # | Severity | Finding |
|---|---|---|
| 1 | bug | High↔Best fade ticks the shared circuit VCO and ladder twice per sample. |
| 2 | risk | Ladder rebuild retunes the shared ladder under the outgoing tier; drops brightness / reso-gain overrides. |
| 3 | risk | Low and High share `os2` and the sub oscillator. |
| 4 | nit | No switch test covers High. |
| 5 | nit | `tests/ultra_rate_mapping.rs` asserts nothing. |
| 6 | nit | `eval/303/harness_app.py` tier list omits High. |

## Task 1: Regression tests (red)

- Test: `plugin303/tests/tier_switch.rs`.
- Consumes: `Voice::{set_tier, set_tier_immediate, update_params, tick}`.
- Step 1: add `low_to_high`, `high_to_low`, `high_to_best`, `best_to_high` click-free tests (coverage for finding 4).
- Step 2: add `fade_keeps_shared_oscillators_in_phase`: for High↔Best (Accurate) and Low↔High (Fun mode, sub on), the output after a 10 ms fade must match an immediate switch at the same sample (relative error energy < 1e-3).
- Step 3: `cargo test -p plugin303 --release --test tier_switch` → the phase test fails (High→Best ≈ 0.89).

## Task 2: Per-tier filter paths, single-tick circuit source (green)

- Modify: `plugin303/src/dsp/voice.rs`.
- Produces: fields `ladder_high`, `ladder_best`, `os2_high`, `sub_osc_circuit`; `pub const HIGH_OS_FACTOR`; private `circuit_source()` and `circuit_path(tier, mix)`.
- Step 1: split the High / Best arms of `render_tier` into `circuit_source` + `circuit_path`.
- Step 2: in the crossfade branch, when both tiers are circuit tiers, compute the source once.
- Step 3: delete `rebuild_circuit_ladder_if_needed` and its two calls; apply every ladder setter to both circuit ladders.
- Step 4: rerun `tier_switch`; check steady renders of all four tiers are byte-identical to pre-fix renders (`render303`, saw + square).
- Step 5: `git commit -m "fix(voice): give each tier its own filter path; tick circuit VCO once per sample"`.

## Task 3: Nits

- Delete `plugin303/tests/ultra_rate_mapping.rs`; add `High` to `eval/303/harness_app.py`.
- `git commit -m "chore: drop no-op ultra_rate_mapping test; list High in harness"`.

## Task 4: Verify and push

- `cargo test -p plugin303 --release --no-fail-fast`; `git push origin refactor/quality-tier-names`.
