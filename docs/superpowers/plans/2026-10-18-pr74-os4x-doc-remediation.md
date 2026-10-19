# PR #74 remediation: stale 4x oversampling docs

**Goal:** Bring every doc and comment that still states Draft or Low oversampling in line with the code (both `Os4x`, `DRAFT_OS_FACTOR = LOW_OS_FACTOR = 4.0`).

**Architecture:** Doc and comment edits only. No DSP change. Behaviour is pinned by the existing test suite, which must stay green.

**Tech Stack:** Rust (`plugin303`), cargo test gate, Markdown, shell, JS workflow file.

**Spec reference:** PR #74 review comment (https://github.com/gustavokch/vibe303/pull/74#issuecomment-6257180473).

## Task 1: Tier doc table and README/agent tables

**Target files (Modify):** `plugin303/src/dsp/filter.rs`, `README.md`, `.agents/AGENTS.md`

**Consumes / Produces:** none.

- Step 1: No behavioural test applies. Confirm the stale rows with `grep -n "none\|2x\|2×" <files>`.
- Step 2: Set Draft oversampling to `4x` (README, filter.rs) / `4×` (AGENTS.md). Set Low to `4x` / `4×`. Leave High and Best rows unchanged.
- Step 3: Re-grep to confirm no Draft `none`/`None` or Low `2x`/`2×` remains in these files.
- Step 4: `git add plugin303/src/dsp/filter.rs README.md .agents/AGENTS.md && git commit -m "docs(tiers): Draft and Low run at 4x oversampling"`

## Task 2: Test comments

**Target files (Modify):** `plugin303/tests/tier_levels.rs`, `plugin303/tests/low_spectral.rs`, `plugin303/tests/no_alloc.rs`

**Consumes / Produces:** none.

- Step 1: No behavioural test applies (comments only).
- Step 2: `tier_levels.rs:18`: replace "Low 2× vs Best 4×" with "Low and Best both 4×". `low_spectral.rs:5`: replace "Draft (TPT, corr≈0.27)" with "Draft (TPT, corr≈0.935 at 4×)". `no_alloc.rs:82`: replace "2× OS" with "4× OS".
- Step 3: Re-read the three edited lines.
- Step 4: `git add plugin303/tests/tier_levels.rs plugin303/tests/low_spectral.rs plugin303/tests/no_alloc.rs && git commit -m "test: correct Draft/Low oversampling in comments"`

## Task 3: Script and workflow descriptions

**Target files (Modify):** `scripts/capture_standalone_gui.sh`, `.claude/workflows/dsp-cpu-profile.js`

**Consumes / Produces:** none.

- Step 1: No behavioural test applies.
- Step 2: `capture_standalone_gui.sh:123`: "Low tier: 2x nodal ladder" becomes "Low tier: 4x nodal ladder". `dsp-cpu-profile.js:75-76`: Low desc "nodal ladder 4× OS"; Draft desc "polyBLEP + TPT ladder, 4× OS".
- Step 3: Re-read the edited lines.
- Step 4: `git add scripts/capture_standalone_gui.sh .claude/workflows/dsp-cpu-profile.js && git commit -m "chore: label Draft and Low as 4x in scripts and workflows"`

## Verification

Full gate (run once, after all tasks): `CARGO_TARGET_DIR=/tmp/target-pr74 cargo test -p plugin303 --release`.
