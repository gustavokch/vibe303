# Quality Tier Rename Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the quality tier names `Low` / `High` / `Ultra` / `Ultra 2x` with `Draft` / `Low` / `High` / `Best` everywhere the product, the code, the CLIs, the scripts and the docs use them.

**Architecture:** `Tier` (`plugin303/src/dsp/filter.rs`) is the single source of the names. Rename its variants, reorder them so the enum order is the fidelity order, then fix every consumer in four layers: Rust references, GUI + QA states, CLIs + eval scripts, internal identifiers + docs. The old names are *reused* with new meaning (old `High` becomes `Low`), so every rename runs as ONE simultaneous pass (never sequential `sed`), and every CLI that took a tier *index* switches to tier *names* so no old command can silently select a different tier.

**Tech Stack:** Rust (nih-plug, egui), Python 3.12 eval scripts (`uv run --no-project`), bash QA scripts.

**Spec:** The user request in this session: "replace the confusing tier names currently in use, we'll use draft, low, high, best instead". No design doc exists. The decisions below are the spec.

## Decisions (read before Task 1)

| Old name (code today) | New name | Oscillator → filter, oversampling | Old enum index | New enum index |
|---|---|---|---|---|
| `Low` ("Low (TPT)") | `Draft` | polyBLEP saw → TPT ladder, none | 0 | 0 |
| `High` ("High (WDF-lite)") | `Low` | polyBLEP saw → nodal diode ladder, 2x | 1 | 1 |
| `Ultra2x` ("Ultra 2x (WDF)") | `High` | circuit VCO → nodal ladder, 2x | 3 | 2 |
| `Ultra` ("Ultra (WDF)") | `Best` | circuit VCO → nodal ladder, 4x | 2 | 3 |

- The user gave four names for four tiers. The repo has exactly four (`Tier` in `plugin303/src/dsp/filter.rs:17-27`). The user did not give the mapping. This plan maps by **fidelity and CPU cost** (`Ultra` runs the ladder at 4x, `Ultra2x` at 2x; `voice.rs` `rebuild_ultra_ladder_if_needed`). A positional map (old order → new order) would make `Best` the *cheaper* of the two circuit-VCO tiers. If the owner wants the positional map instead, stop and change this table first; every later task follows it.
- **Enum order = saved-state index.** nih-plug stores an `EnumParam` by variant index under the param id `"tier"`. The reorder swaps indices 2 and 3 for sessions saved with old `Ultra` / `Ultra 2x`. `0.1.0` is unreleased (`CHANGELOG.md`), so this plan accepts that and records it in the changelog. The param id `"tier"` does not change (changing it would reset the tier for every saved session, not just two of them).
- The host-visible parameter name `"Tier"` (`params.rs:500`) does not change. Only the variants' display names change.
- The word "WDF" in the old display names is wrong (the filter is nodal MNA + Newton; `.agents/AGENTS.md` gotcha 15). The new display names carry no technique suffix.

## Global Constraints

- Tier names, exactly: `Draft`, `Low`, `High`, `Best` (display, variant and CLI names; GUI labels are upper case: `DRAFT`, `LOW`, `HIGH`, `BEST`).
- Audio output must not change. A pure rename. Task 3 proves this with byte-identical renders against a baseline taken in Task 1 Step 2.
- Zero-alloc audio path stays intact (`tests/no_alloc.rs`). No `process()` logic changes.
- Run Rust tests in release mode: `cargo test -p plugin303 --release` (`.agents/AGENTS.md`).
- Python: ruff `line-length = 100`; run scripts with `uv run --no-project`.
- Do NOT edit historical docs: `docs/plans/**`, `docs/superpowers/plans/**` (except this file), `docs/ACCURATE_ULTRA_TIER_REPORT.md`, `docs/profiling/**`, `worktree-params/**`. They record what the names meant when written. Do NOT edit `.trail/**` (generated cache).
- Out of scope, leave as is: `eval/303/calibrate_vcf.py` (it calls `target/release/vcf_sweep_helper`, which no longer exists in `plugin303/src/bin`; the script is already broken and its `low`/`high` strings are its own CLI), `plugin303/tests/ultra_rate_mapping.rs` (a 289-byte test that asserts `|| true`; flag to the owner, do not touch here).

## File Structure

| File | Responsibility after this plan |
|---|---|
| `plugin303/src/dsp/filter.rs` | `Tier` enum (`Draft/Low/High/Best`, order = saved index), `Tier::parse(&str)` |
| `plugin303/src/dsp/voice.rs` | Tier-bound identifiers renamed so no identifier says `high`/`ultra` for a different tier than its name |
| `plugin303/src/params.rs` | `QaState::{QualityLow, QualityHigh, QualityBest}`, default tier `Draft` |
| `plugin303/src/gui/panels/header_bar.rs` | QUALITY switch labels `DRAFT LOW HIGH BEST` |
| `plugin303/src/bin/render303.rs`, `profile_voice.rs` | `--tier draft\|low\|high\|best` (names only) |
| `eval/303/measure_tiers.py`, `harness_app.py`, `coherence.py` | Tier names in labels + `--tier <name>` |
| `plugin303/tests/*` | Renamed tier references; spectral/ESR/stability test files renamed |
| `README.md`, `.agents/AGENTS.md`, `docs/DAW_SMOKE_TEST.md`, `CHANGELOG.md`, `docs/assets/screenshot.png` | New vocabulary |

## Task 0: Prerequisites

- [ ] **Step 1: Start from a clean base that contains the README on `main`**

`feat/gui-qa-running-note` has local edits that are not part of this work (`.cargo/config.toml`, `.gitignore`, `plugin303/src/lib.rs`, `plugin303/src/params.rs`; the untracked `docs/assets/` and the `README.md` edit are already on `origin/main` as `f7bd8ea`). This plan edits `params.rs` and `lib.rs`. Resolve those local edits with the owner first (commit them on their own branch or stash them).

```bash
git fetch origin
git checkout README.md && rm -rf docs/assets          # same content is on origin/main (f7bd8ea)
git stash push -m "pre-tier-rename local edits" -- .cargo/config.toml .gitignore plugin303/src/lib.rs plugin303/src/params.rs
git checkout -b refactor/quality-tier-names
git merge origin/main
git status --short            # expect only untracked files unrelated to this plan
```

Expected: merge succeeds; `plugin303/src/params.rs` contains `QaState::RunningNote` (this plan's line numbers assume it).

---

## Task 1: Rename the `Tier` enum and every Rust reference

**Files:**
- Modify: `plugin303/src/dsp/filter.rs:1-27` (module doc, enum), plus its `tests` module
- Modify (mechanical, via script): every `*.rs` under `plugin303/` that contains `Tier::` (src, bins, tests)
- Test: `plugin303/src/dsp/filter.rs` (unit tests), `plugin303/tests/params_tier.rs`

**Interfaces:**
- Produces: `pub enum Tier { Draft, Low, High, Best }` (default `Draft`); `Tier::parse(name: &str) -> Option<Tier>` (ASCII case-insensitive; accepts only `draft|low|high|best`). Tasks 2-4 rely on both.

- [ ] **Step 1: Record a green baseline**

```bash
cargo test -p plugin303 --release --no-fail-fast 2>&1 | tail -40
```

Expected: all pass (about 65 tests, about 8 ignored). Write down any failure that exists *before* your edits so you do not blame the rename for it.

- [ ] **Step 2: Render baseline audio for all tiers `render303` can produce today**

Render each twice for tier 2 to prove the renderer is deterministic. The comparison in Task 3 needs this.

```bash
for t in 0 1 2; do
  cargo run --release --bin render303 -p plugin303 -- --note 57 --sr 48000 --secs 0.2 \
    --cutoff 1000 --reso 85 --tier $t --out /tmp/tier_base_$t.f32
done
cargo run --release --bin render303 -p plugin303 -- --note 57 --sr 48000 --secs 0.2 \
  --cutoff 1000 --reso 85 --tier 2 --out /tmp/tier_base_2b.f32
cmp /tmp/tier_base_2.f32 /tmp/tier_base_2b.f32 && echo DETERMINISTIC
```

Expected: `DETERMINISTIC`. If `cmp` reports a difference, STOP: Task 3's byte-compare gate is invalid; use a per-sample tolerance instead and tell the owner.

- [ ] **Step 3: Replace the enum (this will not compile until Step 4)**

In `plugin303/src/dsp/filter.rs` replace line 1 and lines 15-27.

Line 1 becomes:

```rust
//! Filter interface and the quality-tier enum.
```

Lines 15-27 become:

```rust
/// Quality tier, ordered by fidelity and CPU cost.
///
/// The variant order is the saved-state index of the `tier` param: never
/// reorder or insert in the middle.
///
/// | Tier  | Oscillator            | Filter             | Oversampling |
/// |-------|-----------------------|--------------------|--------------|
/// | Draft | polyBLEP saw          | TPT ladder         | none         |
/// | Low   | polyBLEP saw          | nodal diode ladder | 2x           |
/// | High  | nodal circuit VCO     | nodal diode ladder | 2x           |
/// | Best  | nodal circuit VCO     | nodal diode ladder | 4x           |
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Enum)]
pub enum Tier {
    #[default]
    #[name = "Draft"]
    Draft,
    #[name = "Low"]
    Low,
    #[name = "High"]
    High,
    #[name = "Best"]
    Best,
}
```

- [ ] **Step 4: Rewrite every `Tier::X` token in one simultaneous pass**

The old names are reused, so `sed 's/Low/Draft/; s/High/Low/'` would corrupt the result. Use one regex pass with a lookup table. Create `/tmp/rename_tier_tokens.py`:

```python
import pathlib
import re
import sys

MAP = {"Low": "Draft", "High": "Low", "Ultra2x": "High", "Ultra": "Best"}
PAT = re.compile(r"\bTier::(Low|High|Ultra2x|Ultra)\b")

for name in sys.argv[1:]:
    p = pathlib.Path(name)
    old = p.read_text()
    new = PAT.sub(lambda m: "Tier::" + MAP[m.group(1)], old)
    if new != old:
        p.write_text(new)
        print("rewrote", name)
```

Run it ONCE (it is not idempotent; running it twice shifts every name again):

```bash
git ls-files 'plugin303/**/*.rs' | xargs uv run --no-project python /tmp/rename_tier_tokens.py
```

Expected: it lists `params.rs`, `lib.rs`, `voice.rs`, the four bins, and the test files that use `Tier::`. Then:

```bash
cargo build -p plugin303 --release --all-targets 2>&1 | tail -20
```

Expected: compiles. Fix any remaining error by hand (these are references the regex cannot see, for example a bare `Low` after `use Tier::*`; `grep -rn "Tier::\*" plugin303` finds none today).

- [ ] **Step 5: Fix the one test whose name now lies**

In `plugin303/tests/params_tier.rs` rename `tier_enum_has_three_variants` to `tier_enum_has_four_variants` (the body already asserts `4`). Also update the file's header comment: `defaults to Low` → `defaults to Draft`.

- [ ] **Step 6: Run the suite**

```bash
cargo test -p plugin303 --release --no-fail-fast 2>&1 | tail -40
```

Expected: same result as Step 1. The `gui_build` test `test_qa_quality_high_state` still passes: the script rewrote it to assert `Tier::Low` and the QA state maps to `Tier::Low`. Task 2 fixes those names.

- [ ] **Step 7: Write the failing tests for `Tier::parse` and the index order**

Add to the `tests` module in `plugin303/src/dsp/filter.rs`:

```rust
    #[test]
    fn parse_is_case_insensitive_and_rejects_retired_names() {
        assert_eq!(Tier::parse("draft"), Some(Tier::Draft));
        assert_eq!(Tier::parse("Low"), Some(Tier::Low));
        assert_eq!(Tier::parse("HIGH"), Some(Tier::High));
        assert_eq!(Tier::parse("best"), Some(Tier::Best));
        // Retired names and bare indices must fail, not map to a different tier.
        for retired in ["ultra", "ultra2x", "0", "2", ""] {
            assert_eq!(Tier::parse(retired), None, "{retired:?}");
        }
    }

    #[test]
    fn saved_state_index_follows_fidelity_order() {
        let indices = [Tier::Draft, Tier::Low, Tier::High, Tier::Best].map(|t| t.to_index());
        assert_eq!(indices, [0, 1, 2, 3]);
    }
```

```bash
cargo test -p plugin303 --release --lib filter 2>&1 | tail -15
```

Expected: FAIL to compile, `no function or associated item named parse found for enum Tier`.

- [ ] **Step 8: Implement `Tier::parse`**

Add below the enum in `filter.rs`:

```rust
impl Tier {
    /// Parses a CLI tier name (`draft`, `low`, `high`, `best`), ASCII
    /// case-insensitive. Anything else, including the retired `ultra` names and
    /// bare indices, is `None`.
    pub fn parse(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "draft" => Some(Self::Draft),
            "low" => Some(Self::Low),
            "high" => Some(Self::High),
            "best" => Some(Self::Best),
            _ => None,
        }
    }
}
```

- [ ] **Step 9: Run the tests and the full suite**

```bash
cargo test -p plugin303 --release --lib filter 2>&1 | tail -15
cargo test -p plugin303 --release --no-fail-fast 2>&1 | tail -20
```

Expected: PASS for the new tests; the suite matches Step 1.

- [ ] **Step 10: Commit**

```bash
git add -A plugin303
git commit -m "refactor(tier): rename Tier to Draft/Low/High/Best in fidelity order

Old Low->Draft, High->Low, Ultra2x->High, Ultra->Best. Enum order is the
saved-state index, so old Ultra and Ultra 2x sessions swap. Add Tier::parse
for CLI names."
```

---

## Task 2: GUI labels and QA states

**Files:**
- Modify: `plugin303/src/gui/panels/header_bar.rs:95-112`
- Modify: `plugin303/src/params.rs` (`QaState` enum about line 421-432, `parse` about 436-447, `default_tier` about 492-497)
- Modify: `tools/gui_render/src/main.rs:33-43`
- Modify: `scripts/capture_standalone_gui.sh` (about lines 122-127 and 143)
- Modify: `scripts/crop_gui_sections.py:132`
- Test: `plugin303/tests/gui_build.rs:147-161`, `tests/test_crop_gui_sections.py:34,49`

**Interfaces:**
- Consumes: `Tier::{Draft, Low, High, Best}` from Task 1.
- Produces: `VIBE303_QA_STATE` values `quality_low` (Tier::Low), `quality_high` (Tier::High), `quality_best` (Tier::Best). The old values `quality_ultra` and `quality_ultra2x` are removed. **`quality_high` now selects a different tier than before** (it used to select old High = new Low). The automated capture suite keeps capturing the same tier as before, so its file stem changes to `quality_low`.

- [ ] **Step 1: Write the failing test**

Replace `test_qa_quality_high_state` (`plugin303/tests/gui_build.rs:147-161`) with:

```rust
#[test]
fn test_qa_quality_states_select_their_tier() {
    use plugin303::dsp::filter::Tier;
    use plugin303::params::Plugin303Params;

    let _lock = ENV_LOCK.lock().unwrap();
    for (state, tier) in [
        ("quality_low", Tier::Low),
        ("quality_high", Tier::High),
        ("quality_best", Tier::Best),
    ] {
        let _guard = EnvGuard::set("VIBE303_QA_STATE", state);
        assert_eq!(
            Plugin303Params::default().tier.value(),
            tier,
            "{state} must initialize the tier of the same name"
        );
    }
}
```

```bash
cargo test -p plugin303 --release --test gui_build qa_quality 2>&1 | tail -15
```

Expected: FAIL (`quality_low` is an unknown state, so the default `Draft` is used; `quality_high` gives `Low`).

- [ ] **Step 2: Rename the QA states in `params.rs`**

Enum (keep alphabetical order):

```rust
    Knobs,
    QualityBest,
    QualityHigh,
    QualityLow,
    Running,
```

`parse`:

```rust
            "quality_best" => Self::QualityBest,
            "quality_high" => Self::QualityHigh,
            "quality_low" => Self::QualityLow,
```

`default_tier` (replace the whole `match`):

```rust
        let default_tier = match qa_state {
            Some(QaState::QualityLow) => Tier::Low,
            Some(QaState::QualityHigh) => Tier::High,
            Some(QaState::QualityBest) => Tier::Best,
            _ => Tier::Draft,
        };
```

Remove the old `QualityUltra` / `QualityUltra2x` variants, parse arms and match arms.

- [ ] **Step 3: Relabel the header switch**

`plugin303/src/gui/panels/header_bar.rs:103`:

```rust
                &["DRAFT", "LOW", "HIGH", "BEST"],
```

The label order must match the enum order (`SlideSwitch` maps label `i` to variant index `i`). Leave the tooltip text on lines 109-111 as is.

- [ ] **Step 4: Update the renderer, capture script, crop script and their test**

`tools/gui_render/src/main.rs:38`: `("quality_high", "quality_high"),` becomes `("quality_low", "quality_low"),`.

`scripts/capture_standalone_gui.sh`: replace the comment and call near line 122-127 with:

```bash
# Quality tier selection (Low tier: 2x nodal ladder, polyBLEP saw)
# The other non-default tiers (quality_high, quality_best) are supported by
# VIBE303_QA_STATE in plugin303/src/params.rs but captured manually only —
# not part of the automated suite.
capture_single_state "quality_low" "$OUTPUT_DIR/quality_low.png"
```

and in `REQUIRED_CROPS` (about line 143) `"quality_high.png"` becomes `"quality_low.png"`.

`scripts/crop_gui_sections.py:132`: `"quality_high",` becomes `"quality_low",`.

`tests/test_crop_gui_sections.py:34`: `"quality_high"` becomes `"quality_low"` in the stem list. Line 49: `"quality_high_crop_voice.png"` becomes `"quality_low_crop_voice.png"`.

- [ ] **Step 5: Run the tests**

```bash
cargo test -p plugin303 --release --test gui_build 2>&1 | tail -15
uv run --no-project --with pytest --with pillow python -m pytest tests/test_crop_gui_sections.py -q 2>&1 | tail -8
```

Expected: PASS.

- [ ] **Step 6: Look at the real surface**

```bash
bash scripts/render_gui_offscreen.sh /tmp/gui-tiers
```

Open `/tmp/gui-tiers/crop_header.png` and `/tmp/gui-tiers/quality_low.png` (use the `read` tool, which shows images). Check: the QUALITY switch reads `DRAFT LOW HIGH BEST`, the `DRAFT` thumb is active in `crop_header.png`, the `LOW` thumb is active in `quality_low.png`, nothing is clipped or overlaps the TOPOLOGY switch. If the offscreen renderer fails, tell the owner exactly what failed and do the check in the standalone build instead (`bash scripts/capture_standalone_gui.sh`).

- [ ] **Step 7: Commit**

```bash
git add plugin303/src plugin303/tests tools/gui_render scripts tests/test_crop_gui_sections.py
git commit -m "refactor(gui): relabel QUALITY switch and rename QA tier states

QA states are now quality_low/high/best. quality_high previously selected
the tier that is now called Low; the automated suite captures quality_low."
```

---

## Task 3: CLIs and eval scripts take tier names

**Files:**
- Modify: `plugin303/src/bin/render303.rs:2,32,40-44`
- Modify: `plugin303/src/bin/profile_voice.rs` (module doc line 2, `arg_tier`/`has_flag` about lines 33-52, `main` about lines 245-285)
- Modify: `eval/303/measure_tiers.py`, `eval/303/harness_app.py`, `eval/303/coherence.py:10`
- Modify: `README.md:176`

**Interfaces:**
- Consumes: `Tier::parse` (Task 1).
- Produces: `render303 --tier draft|low|high|best` (default `draft`; numbers and unknown names exit with status 2). `profile_voice --tier draft|low|high|best|sweep`.

- [ ] **Step 1: `render303`**

Header comment line 2 stays (`--tier` is still in the list). Replace line 32 and the `match` on lines 40-44.

Line 32 becomes:

```rust
    let tier_name = arg_s("--tier", "draft"); // draft|low|high|best; default keeps F2 golds valid
    let tier = Tier::parse(&tier_name).unwrap_or_else(|| {
        eprintln!("render303: unknown --tier {tier_name:?} (expected draft, low, high or best)");
        std::process::exit(2);
    });
```

The `set_tier_immediate` call becomes:

```rust
    v.set_tier_immediate(tier);
```

- [ ] **Step 2: `profile_voice`**

Module doc line 2: `across Low, High, Ultra tiers` becomes `across Draft, Low, Best tiers`.

Replace `arg_tier` with:

```rust
/// What `--tier` selected: one tier, or the tier-switching sweep.
enum Target {
    One(Tier),
    Sweep,
}

fn arg_target(flag: &str) -> Option<Target> {
    let args: Vec<String> = std::env::args().collect();
    let val = args
        .iter()
        .position(|s| s == flag)
        .and_then(|i| args.get(i + 1))?;

    if val.eq_ignore_ascii_case("sweep") {
        return Some(Target::Sweep);
    }
    match Tier::parse(val) {
        Some(tier) => Some(Target::One(tier)),
        None => {
            eprintln!("profile_voice: unknown {flag} {val:?} (expected draft, low, high, best or sweep)");
            std::process::exit(2);
        }
    }
}
```

In `main`, delete `has_tier_flag` and `target_tier`, and replace the `match target_tier { ... }` block with:

```rust
    match arg_target("--tier") {
        Some(Target::One(tier)) => {
            run_tier(&format!("{tier:?}").to_ascii_lowercase(), tier, secs, sr, seed);
        }
        Some(Target::Sweep) => {
            run_sweep(secs, sr, seed);
        }
        None => {
            run_tier("draft", Tier::Draft, secs, sr, seed);
            run_tier("low", Tier::Low, secs, sr, seed);
            run_tier("best", Tier::Best, secs, sr, seed);
            run_sweep(secs, sr, seed);
        }
    }
```

The default run keeps its old three tiers (old Low, High, Ultra) plus the sweep; do not add `High` to it. If `has_flag` has no other caller, delete it (the compiler reports `dead_code`).

- [ ] **Step 3: Prove the rename changed no audio**

```bash
for pair in draft:0 low:1 best:2; do
  name=${pair%%:*}; idx=${pair##*:}
  cargo run --release --bin render303 -p plugin303 -- --note 57 --sr 48000 --secs 0.2 \
    --cutoff 1000 --reso 85 --tier $name --out /tmp/tier_new_$idx.f32
  cmp /tmp/tier_base_$idx.f32 /tmp/tier_new_$idx.f32 && echo "$name identical to old tier $idx"
done
cargo run --release --bin render303 -p plugin303 -- --tier 2 --out /tmp/x.f32; echo "exit=$?"
cargo run --release --bin render303 -p plugin303 -- --tier ultra --out /tmp/x.f32; echo "exit=$?"
cargo run --release --bin render303 -p plugin303 -- --tier high --secs 0.2 --out /tmp/high.f32; echo "exit=$?"
cargo run --release --bin profile_voice -p plugin303 -- --secs 1 --tier best 2>&1 | tail -3
```

Expected: three `identical` lines; `exit=2` for `--tier 2` and `--tier ultra` with the error message; `exit=0` for `--tier high`; `profile_voice` prints one `Tier best` line. If a `cmp` differs, the rename changed behavior: stop and find the cause (the Step 2 baselines came from the pre-rename commit).

- [ ] **Step 4: `eval/303/measure_tiers.py`**

Run one simultaneous pass over identifiers and labels. Create `/tmp/rename_py_tiers.py`:

```python
import pathlib
import re
import sys

MAP = {
    "Low": "Draft", "High": "Low", "Ultra": "Best",
    "low": "draft", "high": "low", "ultra": "best",
    "low_spectrum": "draft_spectrum", "high_spectrum": "low_spectrum",
    "ultra_spectrum": "best_spectrum",
    "low_p": "draft_p", "high_p": "low_p", "ultra_p": "best_p",
    "low_db": "draft_db", "high_db": "low_db", "ultra_db": "best_db",
    "low_val": "draft_val", "high_val": "low_val", "ultra_val": "best_val",
    "low_baseline": "draft_baseline",
    "lsd_vs_low_db": "lsd_vs_draft_db", "corr_vs_low": "corr_vs_draft",
}
PAT = re.compile(r"\b(" + "|".join(sorted(MAP, key=len, reverse=True)) + r")\b")

for name in sys.argv[1:]:
    p = pathlib.Path(name)
    old = p.read_text()
    new = PAT.sub(lambda m: MAP[m.group(1)], old)
    if new != old:
        p.write_text(new)
        print("rewrote", name)
```

Run once: `uv run --no-project python /tmp/rename_py_tiers.py eval/303/measure_tiers.py`.

Then review `git diff eval/303/measure_tiers.py`. `lowpass_decimate_2` must be untouched (the `\b` boundaries guarantee it). Any bare `low`/`high`/`ultra` that is ordinary English in a comment or docstring (not a tier) must be reverted by hand.

Then switch the CLI argument from index to name. Change `render_rust(..., tier_idx: int, ...)` to `tier: str` and the call line `"--tier", str(tier_idx),` to `"--tier", tier.lower(),`. Replace the loop header (about line 286):

```python
        for tier_name in ["Draft", "Low", "Best"]:
```

and its call argument `tier_idx=tier_idx,` with `tier=tier_name,`.

- [ ] **Step 5: `eval/303/harness_app.py`**

Run the same script over it (`uv run --no-project python /tmp/rename_py_tiers.py eval/303/harness_app.py`). Then edit by hand:

- `render_voice(..., tier_idx: int, ...)` becomes `tier: str`; `"--tier", str(tier_idx),` becomes `"--tier", tier.lower(),`.
- Line 52-54: replace the selectbox, `tier_map` and `selected_tier_idx` with:

```python
tier = st.sidebar.selectbox("Selected Tier", ["Draft", "Low", "Best"])
selected_tier = tier.lower()
```

- Every `selected_tier_idx` argument becomes `selected_tier`; every literal reference-tier `2` in `render_voice(..., 0, 2, sr, secs)` calls (lines 76, 115, 145, 175) becomes `"best"`.

Verify: `grep -n "tier_idx\|tier_map\|, 2, sr" eval/303/harness_app.py eval/303/measure_tiers.py` prints nothing.

- [ ] **Step 6: `eval/303/coherence.py` and README command**

`coherence.py` docstring lines 1-12: `Ultra` becomes `Best` in the title and prose, `ultra_spectral.rs` becomes `best_spectral.rs` (Task 4 renames that file), and `--tier 2` becomes `--tier best`; `/tmp/ultra.f32` becomes `/tmp/best.f32`.

`README.md:176`: `--tier 2` becomes `--tier best`.

- [ ] **Step 7: Check the scripts parse and the eval unit tests still pass**

```bash
uv run --no-project python -m py_compile eval/303/measure_tiers.py eval/303/harness_app.py eval/303/coherence.py
uv run --no-project --with numpy --with pytest python -m pytest eval/303/test_metrics.py eval/303/test_esr.py -q 2>&1 | tail -5
uv run --no-project --with ruff ruff check eval/303/measure_tiers.py eval/303/harness_app.py 2>&1 | tail -5
```

Expected: no error output from `py_compile`; tests pass; `ruff` reports no *new* findings (compare with `git stash` if it reports pre-existing ones).

- [ ] **Step 8: Commit**

```bash
git add plugin303/src/bin eval/303 README.md
git commit -m "refactor(cli): select tiers by name in render303, profile_voice and eval scripts

--tier takes draft|low|high|best. Numeric indices and the retired ultra
name exit with status 2 so an old command cannot silently select a
different tier."
```

---

## Task 4: Internal identifiers, test file names, comments

Every identifier whose name says `high`, `low` or `ultra` for a tier must say what the tier is called now. Otherwise `comp_high` means "Low" and the next reader is confused again.

**Files:**
- Modify: `plugin303/src/dsp/voice.rs` (identifiers + comments)
- Modify (identifier pass): every `*.rs` under `plugin303/` that uses a renamed identifier (`tests/ultra_spectral.rs`, `tests/high_spectral.rs`)
- Rename: `plugin303/tests/high_spectral.rs` → `low_spectral.rs`, `ultra_spectral.rs` → `best_spectral.rs`, `ultra_esr.rs` → `best_esr.rs`, `ultra_stability.rs` → `best_stability.rs`
- Modify (comments): `plugin303/src/**`, `plugin303/tests/**`

**Interfaces:**
- Consumes: Tier names from Task 1.
- Produces (public API changes; all callers are inside `plugin303/`): `Voice::set_circuit_brightness(f64)` (was `set_ultra_brightness`), `Voice::set_circuit_reso_gain` (was `set_ultra_reso_gain`), `Voice::set_low_brightness` (was `set_high_brightness`), `pub const LOW_OS_FACTOR` (was `HIGH_OS_FACTOR`), `pub const BEST_OS_FACTOR` (was `ULTRA_OS_FACTOR`). "circuit" names things shared by the two circuit-VCO tiers (High and Best).

- [ ] **Step 1: Confirm which identifiers are public and who uses them**

```bash
grep -rnE "set_ultra_brightness|set_ultra_reso_gain|set_high_brightness|HIGH_OS_FACTOR|ULTRA_OS_FACTOR" plugin303 tools --include=*.rs | grep -v "src/dsp/voice.rs"
```

Expected: only `tests/ultra_spectral.rs`, `tests/high_spectral.rs` and the `vcf_taps.rs:65` comment. If anything else appears (a bin, `tools/`), it joins the Step 2 file list automatically because Step 2 runs over all of `plugin303/` and `tools/`.

- [ ] **Step 2: Rename identifiers in one simultaneous pass**

Create `/tmp/rename_rust_idents.py`:

```python
import pathlib
import re
import sys

MAP = {
    # fields and locals
    "ladder_hi": "ladder_low", "osc_hi": "osc_low", "osc2_hi": "osc2_low",
    "sub_osc_hi": "sub_osc_low",
    "comp_low": "comp_draft", "comp_high": "comp_low", "comp_ultra": "comp_circuit",
    "lat_low": "lat_draft", "lat_high": "lat_low", "lat_ultra": "lat_best",
    "low_cutoff_mult": "draft_cutoff_mult", "low_reso_mult": "draft_reso_mult",
    "low_fc": "draft_fc", "low_reso": "draft_reso",
    "high_fc": "low_fc", "high_reso": "low_reso",
    # constants
    "ULTRA_OS_FACTOR": "BEST_OS_FACTOR", "HIGH_OS_FACTOR": "LOW_OS_FACTOR",
    "ULTRA_BRIGHT": "CIRCUIT_BRIGHT", "HIGH_BRIGHT": "LOW_BRIGHT",
    "HIGH_LEVEL_TRIM": "LOW_LEVEL_TRIM",
    "LOW_FC_POLY": "DRAFT_FC_POLY", "LOW_RESO_POLY": "DRAFT_RESO_POLY",
    "HIGH_FC_POLY": "LOW_FC_POLY", "HIGH_RESO_POLY": "LOW_RESO_POLY",
    "ULTRA_BRIGHTNESS": "BEST_BRIGHTNESS", "ULTRA_BUDGET_DB": "BEST_BUDGET_DB",
    # methods and test names
    "rebuild_ultra_ladder_if_needed": "rebuild_circuit_ladder_if_needed",
    "set_ultra_brightness": "set_circuit_brightness",
    "set_ultra_reso_gain": "set_circuit_reso_gain",
    "set_high_brightness": "set_low_brightness",
    "high_brightness_sweep": "low_brightness_sweep",
    "high_spectral_report": "low_spectral_report",
    "voice_new_builds_ultra_ladder_at_oversampled_rate":
        "voice_new_builds_best_ladder_at_oversampled_rate",
}
PAT = re.compile(r"\b(" + "|".join(sorted(MAP, key=len, reverse=True)) + r")\b")

for name in sys.argv[1:]:
    p = pathlib.Path(name)
    old = p.read_text()
    new = PAT.sub(lambda m: MAP[m.group(1)], old)
    if new != old:
        p.write_text(new)
        print("rewrote", name)
```

Run it ONCE (not idempotent):

```bash
git ls-files 'plugin303/**/*.rs' 'tools/**/*.rs' | xargs uv run --no-project python /tmp/rename_rust_idents.py
cargo build -p plugin303 --release --all-targets 2>&1 | tail -20
```

Expected: compiles. If the compiler names an identifier that is not in the map (for example `s_hi`), decide whether it is tier-bound; `s_lo`/`s_hi` are filter-state locals and must NOT be renamed.

- [ ] **Step 3: Rename the test files**

```bash
cd plugin303/tests
git mv high_spectral.rs low_spectral.rs
git mv ultra_spectral.rs best_spectral.rs
git mv ultra_esr.rs best_esr.rs
git mv ultra_stability.rs best_stability.rs
cd ../..
```

`.config/nextest.toml` names only `test_vco_sr`, `vco_pitch`, `tier_calibration`: no edit needed.

- [ ] **Step 4: Rewrite the remaining comments and doc strings**

List what is left:

```bash
grep -rnE "Ultra|ultra|ULTRA|\bHigh\b|\bLow\b|UltraHd|High tier|Low tier|WDF" plugin303/src plugin303/tests tools --include=*.rs
```

Rewrite each hit with this vocabulary. Decide per hit what the sentence means:

| Comment says | Rewrite to |
|---|---|
| the 4x circuit tier (`Ultra`, "Ultra tier") | `Best` |
| the 2x circuit tier (`Ultra 2x`) | `High` |
| both circuit-VCO tiers ("Ultra tiers", the shared nodal ladder, `comp_circuit`) | "High and Best" or "the circuit-VCO tiers" |
| the polyBLEP + nodal-ladder tier (old `High`) | `Low` |
| the polyBLEP + TPT tier (old `Low`) | `Draft` |
| `UltraHd` (a hypothetical future tier in the `Tier::Best` arm comment) | "a future full-rate-VCO tier" |
| "WDF" naming the filter | "nodal" |

Known spots (not exhaustive; the grep is the list): `voice.rs` struct field section headers `// --- Low Tier ---`, `// --- High Tier ---`, `// --- Ultra Tier ---` (become `Draft`, `Low`, `Best/High`), the `ULTRA_*`/`HIGH_*` doc block at `voice.rs:31-37`, `Voice::reset` doc ("Leaves a clean steady Low" becomes "Draft"), `lib.rs:274-276` ("silently revert to Low" becomes "Draft"), `ladder_wdf.rs:1,76,117,214`, `oversample.rs:205-208`, `filter.rs` unit-test names, `tests/tier_levels.rs:5-6` (`ultra_spectral`/`high_spectral` become `best_spectral`/`low_spectral`), `tests/best_esr.rs` ("Ultra voice ESR" and the `ultra_spectral.rs` pointers), `tests/vcf_taps.rs:65`. Keep each comment's technical content; change names only. `ladder_wdf.rs.patched` is a stray untracked-style file: do not touch it.

- [ ] **Step 5: Gate: no old name left in code**

```bash
grep -rniE "ultra" plugin303/src plugin303/tests tools --include=*.rs
```

Expected: no output, except `tests/ultra_rate_mapping.rs` (out of scope; see Global Constraints). Then:

```bash
grep -rnE "\bHigh\b|\bLow\b" plugin303/src plugin303/tests --include=*.rs
```

Expected: every hit is a `Tier::High` / `Tier::Low` token or text that means the *new* High / Low (read each one).

- [ ] **Step 6: Run the full suite**

```bash
cargo test -p plugin303 --release --no-fail-fast 2>&1 | tail -30
cargo clippy -p plugin303 --release --all-targets 2>&1 | tail -15
```

Expected: same pass/ignore counts as Task 1 Step 1; `clippy` shows no new warnings (an unused-constant warning means a rename missed a use).

- [ ] **Step 7: Commit**

```bash
git add -A plugin303 tools
git commit -m "refactor(dsp): rename tier-bound identifiers and spectral test files

comp_high/ladder_hi/HIGH_* meant the tier now called Low; ULTRA_* meant
Best or both circuit-VCO tiers. Rename so identifier and tier agree."
```

---

## Task 5: Docs, changelog, screenshot

**Files:**
- Modify: `README.md` (tier section about lines 19-32, line 76 and any other hit)
- Modify: `.agents/AGENTS.md` (tier table, test table, gotchas, status text)
- Modify: `docs/DAW_SMOKE_TEST.md:1-47`
- Modify: `CHANGELOG.md` (`## [0.1.0] - Unreleased`)
- Replace: `docs/assets/screenshot.png`

**Interfaces:**
- Consumes: the names and mapping in "Decisions".

- [ ] **Step 1: `README.md`**

Replace the tier table with the four rows below. Keep each row's cell text from the current table (the owner verified it against the code on 2026-10-17); only the names and row order change. The old `Ultra 2x` row becomes `High` and moves above `Best` (old `Ultra`).

```markdown
|Tier|Oscillator|Filter|Oversampling|
|---|---|---|---|
|**Draft**|PolyBLEP saw|TPT diode-ladder approximation|none|
|**Low**|PolyBLEP saw|Nodal diode ladder with active Q9 resonance|2x|
|**High**|Nodal relaxation VCO at the base rate|Nodal diode ladder|2x|
|**Best**|Nodal relaxation VCO (21 nodes)|Nodal diode ladder with active Q9 resonance|4x|
```

Rewrite the sentence at line 32 to: `The circuit-VCO tiers (High and Best) cost far more CPU than Draft and Low. Use Draft or Low for tracking, Best for bounces.` Line 3 ("four quality tiers") stays. Fix line 76 (`for every tier` stays true; check it names no old tier) and the intro sentence under the heading at line 21 ("The two higher tiers" becomes "The circuit-VCO tiers").

```bash
grep -nEi "ultra|\btiers?\b|\bhigh\b|\blow\b" README.md
```

Expected: every hit uses the new names.

- [ ] **Step 2: `.agents/AGENTS.md`**

Run `grep -nEi "ultra|\bhigh\b|\blow\b|tier" .agents/AGENTS.md` and fix every hit with the vocabulary table from Task 4 Step 4. Specifically:

- Fidelity Tiers table: four rows `Draft`, `Low`, `High`, `Best` in the order of "Decisions". Keep the measured numbers on the rows they were measured for: old `High` "~42% core" goes on `Low`; old `Ultra` "~147% core" goes on `Best`; `High` (old `Ultra2x`) cost cell reads `not profiled`; do not invent a number.
- Test table: `ultra_spectral.rs` → `best_spectral.rs`, `high_spectral.rs` → `low_spectral.rs`, `ultra_stability.rs` → `best_stability.rs`, `ultra_esr.rs` → `best_esr.rs`, and the "High-tier spectral gate" / "for Ultra" descriptions.
- Gotchas 1, 2, 3, 5, 7: `Ultra` → `Best`, `High` → `Low`, `Low` → `Draft`; gotcha 5 ("If Ultra-at-RT is ever required, the active stages could ship High-only") becomes "...could ship Low-only".
- Plugin Architecture bullets (`filter.rs`, `voice.rs`, `ladder_wdf.rs`, `vco.rs`): same mapping.
- `params.rs` bullet says "Currently only `tier: EnumParam<Tier>`": leave unless you can verify and fix it in the same pass; it is outside this task.

- [ ] **Step 3: `docs/DAW_SMOKE_TEST.md`**

Rewrite lines 1-47 with the new names: "all three tiers (Low / High / Ultra)" becomes "all four tiers (Draft / Low / High / Best)", "on Low, High, and Ultra" becomes "on Draft, Low, High, and Best", "on **High/Ultra** the peak tracks cutoff" becomes "on **Low/High/Best**", PDC lines (`High/Ultra report added latency vs Low`) become `Low/High/Best report added latency vs Draft`, and the CPU line becomes `Best is ~1.5 cores/voice (offline-grade — expected); Low ~0.4 core (RT-viable). Confirm Draft/Low run comfortably in real time; Best may glitch at low buffer sizes — expected.` The `Tiers sound` table header and the gold-file A/B line (`the Ultra render` becomes `the Best render`) follow the same mapping. Add a line that `High` exists and must make sound; its CPU was not profiled.

- [ ] **Step 4: `CHANGELOG.md`**

Under `## [0.1.0] - Unreleased`, add a `### Changed` section above `### Removed`:

```markdown
### Changed
- Quality tiers renamed in fidelity order: Draft (was Low), Low (was High),
  High (was Ultra 2x), Best (was Ultra). The GUI QUALITY switch, the host
  parameter value names, `render303 --tier` and `profile_voice --tier` use the
  new names; `--tier` takes names only (`draft|low|high|best`), not indices.
  The QA state `quality_high` now selects the High tier; the capture suite uses
  `quality_low` for what `quality_high` captured before. Sessions saved before
  this change that used Ultra or Ultra 2x load as each other's replacement
  (stored index 2 is now High, index 3 is now Best).
```

- [ ] **Step 5: Replace the README screenshot**

The current image shows the old `LOW HIGH ULTRA ULTRA 2X` labels.

```bash
bash scripts/render_gui_offscreen.sh /tmp/gui-shots
cp /tmp/gui-shots/running_note.png docs/assets/screenshot.png
```

Open `docs/assets/screenshot.png` with `read`. Expected: the header QUALITY switch reads `DRAFT LOW HIGH BEST`; the playhead shows (state `running_note`).

- [ ] **Step 6: Final gates**

```bash
grep -rniE "ultra" README.md .agents/AGENTS.md docs/DAW_SMOKE_TEST.md plugin303/src plugin303/tests tools scripts eval/303/*.py tests/*.py
cargo test -p plugin303 --release --no-fail-fast 2>&1 | tail -20
uv run --no-project --with pytest python -m pytest tests/circuitgen/ -q 2>&1 | tail -3
```

Expected: the `grep` prints only `plugin303/tests/ultra_rate_mapping.rs`, `eval/303/calibrate_vcf.py` has no `ultra` (it never had), and `CHANGELOG.md` is not in the list. `cargo test` matches the Task 1 Step 1 baseline. The circuitgen drift guard still passes (nothing here touches `circuit/*.toml`).

- [ ] **Step 7: Smoke the shipped bundle**

```bash
cargo xtask bundle plugin303 --release 2>&1 | tail -5
```

Expected: `target/bundled/TB303.vst3` and `TB303.clap` build. Then run the manual host check from `docs/DAW_SMOKE_TEST.md` for the tier switch (human step; report it as not done if no DAW is available).

- [ ] **Step 8: Commit**

```bash
git add README.md .agents/AGENTS.md docs/DAW_SMOKE_TEST.md CHANGELOG.md docs/assets/screenshot.png
git commit -m "docs: use Draft/Low/High/Best tier names and refresh screenshot"
```

---

## Self-Review

**Spec coverage.** "replace the confusing tier names currently in use" with `draft, low, high, best`: enum + display names (Task 1), GUI labels + QA states (Task 2), CLI flags + eval scripts (Task 3), identifiers + test files + comments (Task 4), README/AGENTS/DAW/CHANGELOG/screenshot (Task 5). The 4 names for 4 tiers mapping is an explicit decision with its basis and its alternative.

**Placeholder scan.** No TBD/TODO. Task 4 Step 4 and Task 5 Steps 2-3 describe prose rewrites by grep list plus a mapping table because the hits are comments and doc sentences; each gate command proves completion (`grep ... ultra` must be empty).

**Type consistency.** `Tier::{Draft, Low, High, Best}` and `Tier::parse(&str) -> Option<Tier>` are defined in Task 1 and used unchanged in Tasks 2-4. `QaState::{QualityLow, QualityHigh, QualityBest}` appear in Task 2 only. `set_circuit_brightness`, `set_circuit_reso_gain`, `set_low_brightness`, `LOW_OS_FACTOR`, `BEST_OS_FACTOR` are defined by the Task 4 map and used only there and in the files the same pass rewrites.

**Known risks.**
- Index swap for saved sessions of old Ultra / Ultra 2x (accepted; unreleased; in changelog).
- Mapping by fidelity rather than position needs owner confirmation (Decisions).
- `Tier::` token regex and identifier map are NOT idempotent: run each exactly once. If a run is interrupted, `git checkout -- plugin303 tools` and redo the whole step.
