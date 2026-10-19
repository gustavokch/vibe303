# PR #73 Remediation Plan

**Goal:** Fix the review findings on PR #73 (`fix/ladder-brightness-wear`) without changing code behavior.

**Architecture:** Documentation and artifact hygiene only. The Rust fix and its tests are unchanged.

**Tech Stack:** Markdown, plain-text audit output, `cargo test -p plugin303 --release` gate.

**Spec:** PR #73 review comment (https://github.com/gustavokch/vibe303/pull/73#issuecomment-6257184877).

## Global Constraints

- No Rust or test changes. TDD does not apply: each task is a text correction, verified by the gate.
- Do not alter any measured number. Only restate it with its context, or strip local path prefixes.

## File Structure

| File | Change | Responsibility |
|---|---|---|
| `CHANGELOG.md` | Modify | Qualify spectral figures; note Draft-vs-Best parity shift |
| `eval/303/results/2026-10-18-brightness-audit-before.txt` | Modify | Drop absolute `/tmp/` prefix on shasum lines (hashes unchanged) |
| `eval/303/results/2026-10-18-brightness-audit.md` | Modify | Record the best_spectral default-path gap |

---

### Task 1: Qualify changelog spectral figures

**Files:**
- Modify: `CHANGELOG.md:50-53`

- [ ] **Step 1: Replace the Best sentence with both render paths**

Current text (lines 50-53) states only the brightness-only figure (9.59 to 3.15 dB). Replace it so it reads:

```
  brightness instead of scaling it. Wear now scales it. Against the SPICE gold,
  Best's brightness-only render moves from 9.59 to 3.15 dB LSD; the full default
  patch moves from 29.61 to 22.24 dB. High's full default patch moves from 28.98
  to 21.58 dB. Low, High and Best sound darker than before (in the plugin, Low's
  VCF brightness goes from 1.0 to 0.50 and High/Best from 1.0 to 0.70); sessions
  keep their settings. Draft-to-Best parity moves apart (20.41 to 24.81 dB LSD),
  because Best is darker.
```

- [ ] **Step 2: Commit**

```bash
git add CHANGELOG.md
git commit -m "docs(changelog): qualify brightness LSD figures by render path"
```

### Task 2: Remove local paths from the audit output

**Files:**
- Modify: `eval/303/results/2026-10-18-brightness-audit-before.txt:37-44`

- [ ] **Step 1: Replace each `/tmp/bright_before_<tier>_<wave>.f32` with `bright_before_<tier>_<wave>.f32`**

Hashes stay as recorded. Only the path prefix changes.

- [ ] **Step 2: Commit**

```bash
git add eval/303/results/2026-10-18-brightness-audit-before.txt
git commit -m "docs(eval): drop local paths from audit shasum lines"
```

### Task 3: Record the best_spectral gate gap

**Files:**
- Modify: `eval/303/results/2026-10-18-brightness-audit.md` (Gates section, after the best_spectral paragraph)

- [ ] **Step 1: Add one paragraph**

```
`best_spectral` renders through `render_best`, which does not call `apply_plugin_defaults`. It therefore does not exercise the first-block wear path. The first-block brightness is covered by `tests/ladder_brightness.rs`, which reads the effective brightness through `dbg_ladder_cals`. Spectrally, the default patch still sits at 22.24 dB LSD for Best, so a default-path spectral gate needs its own investigation.
```

- [ ] **Step 2: Commit**

```bash
git add eval/303/results/2026-10-18-brightness-audit.md
git commit -m "docs(eval): record best_spectral default-path gap"
```

## Verification

Run the gate once after all tasks: `cargo test -p plugin303 --release`.
