# Bit-exact structure-aware WDF solver speedup — design

**Date:** 2026-06-07 · **Goal:** absolute wall-clock speedup of the Rust WDF
solver (lang-portable, helps C++ too) · **Constraint:** bit-exact — existing
golden vectors + rust-vs-cpp parity gates must pass unchanged.

## Context

Prior round (`docs/plans/2026-06-07-rust-vs-cpp-wdf-perf-diagnosis.md`) found the
Rust port ~1.17× slower than C++, attributed the residual to GCC-vs-LLVM FP-kernel
codegen, and reverted a `get_unchecked` attempt (recovered only ~2%, not worth
`unsafe`). This round changes target: **absolute** speedup of the solver itself,
not rust-vs-cpp parity. Profile self-time: `gauss_solve` 28% (dominates the
realtime tier's full dense solve), `depletion_charge`/`compute_bjt` 26% (BJT
transcendentals, every iter both tiers), `newton_iterate` 20%.

The circuit is `NUM_NODES=21`, `U=16` unknowns, 3 BJTs (TR1/TR2/TR3). Realtime
tier = full 16×16 Gaussian elimination every Newton iter; fast tier = Schur
partition (small nonlinear N-block + precomputed `A_LL⁻¹`).

### Why bit-exact rules out the obvious lever

Reducing Newton iterations (better predictor/warm-start) would change the
convergence path → different output → fails golden vectors. So all speedup must be
*same arithmetic, same order, faster*. That leaves two real approaches.

## Scope decision

**Approach A now; Approach B (BJT-SIMD) as an in-round fallback** triggered only if
A's realtime-tier wall-clock gain is < ~5%.

## Approach A — structure-aware linear solve (selected)

Surgical edits to `plugin/src/wdf.rs` hot functions: `gauss_solve`, `lu_factor`,
`lu_solve`, `prepare_schur`/`schur_solve`, `newton_iterate`. No new modules, no new
public API, no audio-thread allocation.

Every change below is **bit-identical** (no FP reassociation, no order change):

1. **Sparsity-skip elimination.** In the `c in col..n` elimination update, skip
   columns where the pivot-row entry is zero (`x − f·0 = x` is a no-op). Same for
   back-substitution and `lu_factor`'s update. The existing `if f == 0 continue`
   already skips zero *multipliers* (row sparsity); this adds *column* sparsity.
2. **Indirection removal.** Replace `a[row[r]*n+c]` indirect addressing with a
   physically permuted row buffer, so the elementwise elimination update
   autovectorizes (objdump flagged `row[]` indirection as SIMD-blocking).
   Reductions (back-sub, dot products) stay scalar to preserve FP order.
3. **Dirty-entry `j` reset.** Drop the per-iter full 256-element `g0→j`
   `copy_from_slice`; restore only the BJT-touched entries between iters.

### Step 0 — sparsity census (go/no-go gate)

Before editing code, instrument one render to dump the static zero pattern of
`g0`/`J` and fill-in growth through elimination. If the matrix is near-dense or
fill-in saturates by ~col 3, A's payoff is small → bail early to B. The diagnosis
assumed dense but never measured this.

## Approach B — SIMD the 3-BJT transcendental block (fallback)

Target `compute_bjt`/`depletion_charge` (26%, every iter). Pack the 6 junction
exponentials (3 BJTs × be/bc, using the existing `fast_exp`/`fast_pow`) into SIMD
lanes — same math packed, bit-identical per lane.

Risks: only 6 junctions underfill AVX2 (4×f64) → ragged; param gather overhead;
Rust `std::simd`/intrinsics are not portable to C++ (would need a parallel C++
rewrite to keep parity). Higher effort, bounded payoff. Hence fallback-only.

## Approach C — rejected

Newton warm-start / fewer iterations: violates bit-exact (changes convergence
path → different output). Out.

## Data flow

Unchanged. Same Newton loop, same pivot decisions, same per-sample output. Only
the per-solve cost drops.

## Validation (the gate, in order)

1. `cargo test -p ba283` — golden vectors `OUTPUT_256 ≤ 1e-5`,
   `SETTLED_V ≤ 1e-6`, zero-alloc tests stay green.
2. `uv run python -m eval.wdf_rust_vs_cpp.compare` — realtime ESR ≤ −60 dB, fast
   ≤ −40 dB, FR ≤ 0.1 dB hold **unchanged** (proves bit-exactness preserved).
3. `uv run python -m eval.wdf_rust_vs_cpp.bench` — before/after rt-factor per tier;
   report the delta. Success = measurable realtime-tier wall-clock drop, all gates
   green.
4. **B-fallback trigger:** realtime gain < ~5% after A → proceed to B in-round.

## Error handling

Singular-matrix `< 1e-30` bailout preserved exactly: pivot search still scans the
full column (skipping zeros in the *update* must not mask a true singular pivot).

## Results (measured 2026-06-07/08, sweep_m6 @96k, gain 0.5)

Wall-clock bench was too noisy (machine under load 5–7; the *unchanged* C++ binary
drifted ±0.1–0.4 rt-factor between runs), so the verdict metric is **instructions
retired** (`perf stat -r3`, contention-immune; run-to-run jitter ±0.01–0.02%).

Baseline (post-census, pre-changes): realtime **84.290 B**, fast **45.338 B**.

- **Task 2 (sparsity-skip in gauss/LU) — 0%, REVERTED.** Bit-exact and clean, but
  instruction-flat: realtime 84.296 B (+0.007%), fast 45.331 B (−0.015%) — both
  inside jitter. A per-element `if entry != 0.0` guard does NOT cut the loop
  trip-count; it swaps each skipped FMA for a load+cmp+branch (instruction-neutral).
  Capturing the census's 17% density requires *not iterating* the zeros (compressed
  / codegen sparse solve), which fights partial pivoting — out of scope. Reverted
  (commit `9b84503`) to keep the audio hot path branch-free.
- **Approach B reframed → "B1" (precompute depletion_charge saturated-branch
  constants) — REAL WIN, no `unsafe`.** `perf record` put `depletion_charge` at 25%
  realtime self-time; its `v >= FC·vj` (saturated/forward-biased) branch recomputed
  3 `pow_` (= 3 `fast_exp` + 3 `fast_log`) per call from inputs that are constant
  per junction. Precomputed once into `BjtParams` (`je_*`/`jc_*`). Bit-exact
  (golden/lossless unchanged; pinned by `depletion_charge_pre_is_bit_exact`).
  **Instructions: realtime 84.290→81.036 B (−3.86%), fast 45.338→43.826 B
  (−3.33%).** Commits `dd43943` (opt) + `0d9714b` (equivalence guard test).

**Hot-path after B1 (realtime self-time, pre-B1 profile):** `gauss_solve` 35%,
`depletion_charge` 25% (now reduced), `newton_iterate` 19%, `residual_norm` 12%,
`diode_branch` 5.5%. Remaining bit-exact levers both carry real cost: (a) SIMD the
residual `depletion_charge` depletion-region `pow_` + `diode_branch` `exp` —
needs `std::arch`+`unsafe` (stable Rust has no `portable_simd`), 6-junction AVX2
underfill; (b) a true sparse/codegen replacement of the 35% `gauss_solve` —
larger, partial-pivoting bit-exactness risk.

## Critical files

- `plugin/src/wdf.rs` — sparsity-skip + indirection removal + dirty-`j` reset in
  the linear-solve and Newton hot paths.
- `eval/wdf_rust_vs_cpp/{compare,bench}.py` — existing gate, reused unchanged.
