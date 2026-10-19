# PR #72 Round 3 Remediation

**Goal:** Fix the two nits posted on PR #72 (tier rename): a sentinel leaked into the circuit-ladder thermal init, and a stale test name.

**Architecture:** No DSP behaviour change beyond the ladder thermal init. Tier names and enum order are unchanged.

**Tech stack:** Rust, `plugin303` crate. Gate: `cargo test -p plugin303 --release`.

**Spec reference:** PR #72 review comment (round 3); `.agents/AGENTS.md`.

## Findings

| # | Severity | Finding |
|---|---|---|
| 1 | nit | `Voice::new` passes `ladder_thermal_state` (-1 sentinel) to `set_thermal`; High/Best ladders start at 19.8 °C until the first `update_params`. |
| 2 | nit | `tests/params_tier.rs::tier_param_defaults_low` asserts `Tier::Draft`. |

## Task 1: Use the real default thermal state in `Voice::new`

- Modify: `plugin303/src/dsp/voice.rs` (`Voice::new`, wear/thermal init block, ~L551-560).
- Consumes: `Voice::thermal_noise` (default 0).
- Step 1: change `ladder.set_thermal(voice.ladder_thermal_state)` to `ladder.set_thermal(voice.thermal_noise)`.
- Step 2: confirm no sentinel is passed: `ladder_thermal_state` is only compared in `update_params`.
- Step 3: `git commit -m "fix(voice): init circuit ladder thermal from the default, not the unset sentinel"`.

## Task 2: Rename the stale default test

- Modify: `plugin303/tests/params_tier.rs`.
- Step 1: rename `tier_param_defaults_low` to `tier_param_defaults_draft`.
- Step 2: `git commit -m "test(params): rename tier default test to Draft"`.

## Task 3: Verify and push

- `cargo test -p plugin303 --release --no-fail-fast`; `uv run --no-project --with pytest --with numpy --with pillow python -m pytest tests/ -q`; `git push origin refactor/quality-tier-names`.
