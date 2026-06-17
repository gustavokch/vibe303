# Scalar Depletion-Charge LUT Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.
> On execution, copy this file to `docs/plans/2026-06-17-wdf-scalar-deplut.md` (the repo plans dir).

**Goal:** Route the *scalar* `depletion_charge_pre` callers (residual-norm + trailing-state recompute) through the existing depletion LUT on lossy tiers, eliminating the remaining `pow` self-time (~6.45% ultra / ~6.62% fast in profile) while keeping `realtime` bit-exact.

**Architecture:** The batch 4-lane path already uses the LUT (`depletion_charge_pre_batch_lut`, shipped PR #68). The scalar fallback path — `depletion_charge_pre` (math.rs) called from `residual_norm` (solvers) and `junction_charges` (trailing update) — was NOT converted and still evaluates two `pow_` per junction. Reuse the existing single-lane kernel `dep_charge_lane_lut` at those two sites, gated by the same `QualityConfig.dep_lut` flag. Leave `depletion_charge_pre` byte-untouched so the bit-exact golden test and realtime path are unaffected.

**Tech Stack:** Rust (crate `ba283`), nih_plug DSP core, x86-64-v3 ISA floor (AVX2+FMA), Python eval harnesses (`uv run`), `perf` profiling.

---

## Context

PR #68 shipped the depletion LUT but only on the **batch** lane (the dominant cost folded into `newton_iterate`). The follow-up profile (`eval/wdf_profile/out/*.report.txt`) showed the *scalar* `depletion_charge_pre` symbol still standing at **6.45% (ultra-fast)** / **6.62% (fast)** — the residual/trailing path was never routed to the table. This plan closes that gap.

There are exactly two scalar call sites, both confirmed to run on fast/ultra and both with the BJT loop index `bi` directly in scope:

| Site | File:line | Cadence on lossy tiers | Index source |
|---|---|---|---|
| `residual_norm` (BE, BC) | `plugin/src/wdf/solvers/mod.rs:76,78` | per-Newton-iteration (non-frozen branch) — **bulk of the cost** | `for bi in 0..self.bjts.len()` |
| `junction_charges` (BE, BC) | `plugin/src/wdf/frozen.rs:17,19` | once/sample, cache-miss trailing update | caller `process/mod.rs:198,207` has `bi`; fn takes `t:&Bjt` |

**Bonus correctness alignment:** `residual_norm`'s comment says its charge "must match newton_iterate's F". Today newton_iterate's F uses the **LUT** (lossy) on fast/ultra while `residual_norm` uses **`pow`** (exact) → the convergence check is measured against a *different* model than the one being solved. Converting `residual_norm` to the LUT makes the residual consistent with the solved F. Same argument for `junction_charges` feeding the next-sample cap-companion state. So this change is a fidelity *improvement*, not just speed.

**Intended outcome:** scalar `depletion_charge_pre` self-% drops toward ~0 on fast/ultra; measurable incremental speedup; fidelity stays inside the −40 dB-vs-realtime budget (and likely improves); `realtime` bit-exact (golden C++ parity untouched).

**Risk:** Low — LUT infra (`DepLut`/`DepLutSet`/`dep_charge_lane_lut`, `Arc<DepLutSet>` storage) already exists and is proven. Main pitfall is the **timing gotcha from PR #68**: a contended-CPU A/B lied (fake regression). Measure idle.

---

## Verified facts (current HEAD, worktree `wdf-depletion-charge-lut`)

- `dep_charge_lane_lut` (`plugin/src/wdf/bjt/mod.rs:478`) is `#[inline(always)] fn` (private to `bjt/mod.rs`). Signature: `(v, vj, cj0, c0, q0, dcdv, lut: &DepLut) -> (f64,f64)`. It already samples the table for the `v < FC*vj` arm, computes the linear arm, branchless-selects, and zero-guards `cj0 <= 0`. **It is exactly the scalar kernel needed.** Note its param order **swaps `vj`/`cj0`** vs `depletion_charge_pre(v, cj0, vj, m, c0, q0, dcdv)`.
- Tables: `WdfModel.dep_luts: Arc<DepLutSet>`; `DepLutSet { be:[DepLut;3], bc:[DepLut;3] }` indexed by BJT order → `self.dep_luts.be[bi]` / `.bc[bi]`.
- Flag: `self.dep_lut: bool` (true on fast/ultra/with_precision(Medium); false realtime/warp/Low).
- `residual_norm`'s LUT branch only replaces the **non-frozen** else-branch (lines 75–78); the `freeze_caps && frozen_caps_valid` short-circuit already skips `depletion_charge_pre`, so it's untouched.
- Bit-exact guard: `plugin/tests/test_wdf.rs::depletion_charge_pre_is_bit_exact` tests `depletion_charge_pre` *directly* against `depletion_charge` (no LUT). Leaving that fn unchanged keeps it green.

---

## Phase 1 — expose the kernel

### Task 1.1: Bump `dep_charge_lane_lut` visibility

**Files:** `plugin/src/wdf/bjt/mod.rs` (~478), `plugin/src/wdf/mod.rs` (re-export, if needed).

**Step 1:** Change `fn dep_charge_lane_lut` → `pub(in crate::wdf) fn dep_charge_lane_lut`. (Sibling `depletion_charge_pre_batch_lut` is already `pub`, so the module boundary is the only blocker.)

**Step 2:** Confirm it's reachable from `solvers/mod.rs` and `frozen.rs` — add a `use crate::wdf::bjt::dep_charge_lane_lut;` (or `use super::bjt::...`) import in each, matching how those files already import `depletion_charge_pre`.

**Step 3:** `cargo build -p ba283` — compiles, no visibility error.

---

## Phase 2 — route the scalar sites, gated

### Task 2.1: `residual_norm` (hot path)

**Files:** `plugin/src/wdf/solvers/mod.rs` (the `else` branch at ~75–78, inside `for bi ...`).

**Step 1:** Replace the two `depletion_charge_pre` calls in the non-frozen else-branch with a `self.dep_lut` gate, mirroring `newton.rs`'s dispatch:
```rust
let (qbe_dep, qbc_dep) = if self.dep_lut {
    let (q_be, _) = dep_charge_lane_lut(
        vbe, t.p.vje, t.p.cje, t.p.je_c0, t.p.je_q0, t.p.je_dcdv, &self.dep_luts.be[bi]);
    let (q_bc, _) = dep_charge_lane_lut(
        vbc, t.p.vjc, t.p.cjc, t.p.jc_c0, t.p.jc_q0, t.p.jc_dcdv, &self.dep_luts.bc[bi]);
    (q_be, q_bc)
} else {
    let (q_be, _) = depletion_charge_pre(
        vbe, t.p.cje, t.p.vje, t.p.mje, t.p.je_c0, t.p.je_q0, t.p.je_dcdv);
    let (q_bc, _) = depletion_charge_pre(
        vbc, t.p.cjc, t.p.vjc, t.p.mjc, t.p.jc_c0, t.p.jc_q0, t.p.jc_dcdv);
    (q_be, q_bc)
};
let (qbe, qbc) = (qbe_dep + tf * o.itf, qbc_dep + tr * o.itr);
```
(Watch the swapped `vj`/`cj0` arg order between the two kernels.) Borrow note: `dep_charge_lane_lut(&self.dep_luts...)` is an immutable borrow of `self` inside `&mut self`; `t = self.bjts[bi]` is a `Copy` of the Bjt (line `let t = self.bjts[bi];`), so it does not alias the borrow. If the borrow-checker complains, bind `let dl = &self.dep_luts;` before the mutable stamping below.

**Step 2:** `cargo build -p ba283`.

### Task 2.2: `junction_charges` (trailing update)

**Files:** `plugin/src/wdf/frozen.rs` (9–21), and its two callers `plugin/src/wdf/process/mod.rs:198,207`.

**Step 1:** `junction_charges` currently takes `t: &Bjt` but needs the index `bi` to pick the table. Change its signature to `fn junction_charges(&self, bi: usize) -> (f64, f64)` and derive `let t = self.bjts[bi];` inside (the BJT is `Copy`). Both callers already have `bi` in scope (`t = self.bjts[bi]`), so pass `bi` instead of `&self.bjts[bi]`.

**Step 2:** Gate the two `depletion_charge_pre` calls the same way as Task 2.1 (LUT when `self.dep_lut`, else current pow), using `&self.dep_luts.be[bi]` / `.bc[bi]`.

**Step 3:** `cargo build -p ba283 --all-targets`.

---

## Phase 3 — verification gates

### Task 3.1: Correctness — realtime bit-exact preserved
**Step 1:** `cargo test -p ba283` — all pass.
**Step 2:** Confirm `depletion_charge_pre_is_bit_exact`, `depletion_charge_pre_batch_is_bit_exact`, and the C++ golden vectors (`plugin/tests/fixtures/`) are green — `depletion_charge_pre` is unchanged and realtime has `dep_lut=false`, so not a single ULP moves.

### Task 3.2: Real-time safety (no alloc)
**Step 1:** Run the existing fast-tier no-alloc test (`plugin/tests/test_wdf_noalloc.rs`) — `dep_luts` is `Arc`, built at construction; the new scalar LUT calls only index/interp, zero alloc. The fast tier already exercises `residual_norm`, so the existing test covers the new path. Assert `violation_count() == 0`. (Add a dedicated assertion only if the existing test doesn't reach `residual_norm`.)

### Task 3.3: Fidelity + speed gate
**Step 1:** `eval/wdf_ultrafast/sweep.py` already has the `dep_lut_off` lever. Run (idle CPU — see gotcha):
```bash
RUSTFLAGS='-C target-cpu=x86-64-v3' uv run python -m eval.wdf_ultrafast.sweep --runs 3
```
**Gate:** added-error ≤ −40 dB vs realtime AND SPICE-ESR within ~0.5 dB of realtime (expect equal-or-better, since residual now matches the solved F). No fidelity regression vs the batch-only LUT baseline.

**Step 2 — isolated A/B (the real speed measure):** the `--dep-lut` flag now toggles *both* batch and scalar LUT, so a same-binary A/B no longer isolates this change. Instead compare **HEAD~1 (batch-only LUT) vs HEAD (batch+scalar LUT)**, both fast-tier with dep_lut on, ≥15 runs each, **on an idle machine**:
```bash
# build both standalone binaries, render identical input, diff wall-time
RUSTFLAGS='-C target-cpu=x86-64-v3' cargo build --release -p ba283 --bin ba283_standalone
```
GOTCHA ([[ba283-wdf-depletion-lut]]): a contended A/B reports a fake regression. Kill background jobs; re-measure idle. Report the median ratio.

### Task 3.4: Profile confirmation
**Step 1:** Build profiling-thin, run profile:
```bash
RUSTFLAGS='-C target-cpu=x86-64-v3 -C force-frame-pointers=yes' cargo build --profile profiling-thin -p ba283 --bin ba283_standalone
eval/wdf_profile/profile.sh --tiers "ultra-fast fast" --hz 999
```
Confirm `depletion_charge_pre` self-% on fast/ultra drops toward ~0 (routed into the lean `dep_charge_lane_lut`/interp). If it doesn't move, investigate inlining/borrow regression before shipping.

### Task 3.5: Commit
```bash
git add plugin/src/wdf/bjt/mod.rs plugin/src/wdf/solvers/mod.rs \
        plugin/src/wdf/frozen.rs plugin/src/wdf/process/mod.rs plugin/src/wdf/mod.rs \
        docs/plans/2026-06-17-wdf-scalar-deplut.md
git commit -m "perf(wdf): route scalar depletion path through LUT on lossy tiers"
```
(Stage explicit paths only — never `git add -A`/`.` per [[git-add-no-all]].)

---

## Critical files (summary)

- `plugin/src/wdf/bjt/mod.rs` — bump `dep_charge_lane_lut` to `pub(in crate::wdf)`.
- `plugin/src/wdf/solvers/mod.rs` — gate `residual_norm`'s scalar charge on `dep_lut` (hot path).
- `plugin/src/wdf/frozen.rs` — gate `junction_charges`; change `t:&Bjt` → `bi:usize`.
- `plugin/src/wdf/process/mod.rs` — update the two `junction_charges` call sites to pass `bi`.
- `plugin/src/wdf/math.rs` — **unchanged** (`depletion_charge_pre` stays bit-exact reference).
- `eval/wdf_ultrafast/sweep.py`, `eval/wdf_profile/profile.sh` — gates (already wired).

## Out of scope (parked)

- Scatter `g0·v` dense matvec (~7%, payoff uncertain).
- Any change to `depletion_charge_pre` itself, the batch path, or the LUT build (all shipped & proven).
- `warp`/`with_precision(Low)` — `cap_tier=Off`, no depletion runs.
