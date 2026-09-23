# PR #66 Remediation — Windows x86_64 Build Targets

**Goal:** Resolve the 4 review findings posted on PR #66 (https://github.com/gustavokch/vibe303/pull/66#issuecomment-5802937746).

**Architecture:** CI/docs-only PR. No Rust source changes. Verification = YAML lint + PowerShell parse + existing test suite (regression gate only).

**Tech Stack:** GitHub Actions YAML, PowerShell, pytest (via uv) / cargo nextest.

## Task 1: Scope `contents: write` permission to release job

**Files:**
- Modify: `.github/workflows/build-windows.yml`

**Steps:**
1. Failing check: workflow top-level `permissions: contents: write` present.
2. Fix: top-level `permissions: contents: read`; add `permissions: contents: write` under `jobs.release`.
3. Verify: `python3` YAML lint prints top-level read, release-level write.
4. Commit: `ci: scope contents:write permission to release job`

## Task 2: Add `timeout-minutes` to both jobs

**Files:**
- Modify: `.github/workflows/build-windows.yml`

**Steps:**
1. Fix: `timeout-minutes: 60` on `windows-x86_64`, `timeout-minutes: 10` on `release`.
2. Verify: YAML lint prints both values.
3. Commit: `ci: add job timeouts to Windows workflow`

## Task 3: Sync plan doc lint expectation (6 → 7 steps)

**Files:**
- Modify: `docs/superpowers/plans/2026-09-22-windows-x86_64-build-targets.md`

**Steps:**
1. Fix: Task 2 Step 2 expected output `job steps: 6` → `job steps: 7`, plus note that the cache step accounts for the 7th.
2. Commit: `docs: fix step-count expectation in Windows build plan`

## Task 4: Verify + push

1. YAML lint both jobs.
2. PowerShell parse check on `scripts/build-windows-x86_64.ps1` (skip if no pwsh).
3. Regression gate: `cargo nextest run -p plugin303 --no-default-features` (PR touches no Rust; failure = pre-existing).
4. Push to `feat/windows-x86_64-build-targets`.

## Out of scope

- Flipping `release` input default to `false` — plan doc (the spec) pins `default: true`. Flagged in review comment only.
