# Bit-exact `gauss_solve` codegen — design

**Date:** 2026-06-08 · **Goal:** speed up the realtime WDF solver's 35% block
(`gauss_solve` at n=16, `wdf.rs:1611/1613`) bit-exact · **Mode:** spike-only first,
ship/extend decided after the spike measures the real solver.

## Context

Profile (realtime self-time): `gauss_solve` 35%, `depletion_charge` 25% (cut by B1),
`newton_iterate` 19%, `residual_norm` 12%, `diode_branch` 5.5%. Two prior rounds:
B1 (precompute depletion consts) shipped −3.9%; structure-aware sparsity-skip (T2)
and BJT autovec both measured DUD and reverted.

`gauss_solve` (`wdf.rs:1116-1158`): partial-pivot Gaussian elimination, `row[]`
index-swap on A, physical swap on b, in-place. Called at n=16 (realtime dense, the
hot one) and small n (Schur N-block). `lu_factor` (`wdf.rs:1163`) shares the inner
pattern (fast tier) — out of scope for the spike.

## What is bit-exact-safe

- **Pivot sequence is data-dependent** (max-abs per column) → cannot statically
  unroll a fixed pivot order without changing numerics. Pivot search stays runtime.
- **Elimination update** `a[row[r]*n+c] -= f * a[row[col]*n+c]` is the O(n³) bulk.
  SIMD-able IF the row access is contiguous. Currently `row[]` indirection blocks
  vectorization (objdump diagnosis, prior round). **Physical row-swap** (copy the
  16-elem rows on pivot instead of swapping the index) makes it contiguous →
  autovec candidate. Index-swap vs physical-swap is bit-identical (same arithmetic,
  same order; only the address differs — `main.cpp` comment, commit 42b51ef).
  Proven by a `to_bits` equivalence test, not assumed.
- **Back-substitution** `s -= a*b` is a REDUCTION → SIMD changes summation order →
  **stays scalar** (bit-exact rule: no reassociation).
- Fixed N=16 specialization kills bounds checks (the 18 `panic_bounds_check` sites
  the diagnosis found) + loop overhead. Bit-exact (same ops).

## Two risks (both bit prior rounds)

1. **fma contraction.** `a -= f*a` is `a*b+c` shape; LLVM may emit `vfmaddpd` for the
   packed form. If the current scalar path is non-contracted, this changes output in
   the last bit → BREAKS bit-exact. **objdump gate:** elimination loop must show
   packed `vmulpd`+`vsubpd` and **zero `vfmadd…pd`**. The `to_bits` test catches it
   regardless.
2. **get_unchecked redux.** The prior bounds-check round got instructions −12% but
   **wall only −2%** (the branches were ILP-hidden). So an instruction drop ALONE is
   not a win here — the spike must SEE packed `vmulpd` in the hot elimination loop
   (real FP-op reduction, wall-relevant), not merely fewer bounds-check branches.

## Spike (go/no-go)

Build `gauss_solve_16(a, b) -> bool` (hardcoded N=16, physical swaps, contiguous
inner loops), dispatch from the realtime path when `u == 16` (keep generic
`gauss_solve` for the Schur small-n call). Then:

1. **Bit-exact:** `to_bits` test — random + structured 16×16 matrices (incl.
   pivot-triggering and near-singular), assert `gauss_solve_16` produces the same
   solution bits as `gauss_solve`. Plus full `cargo test -p ba283` green.
2. **objdump:** elimination loop autovectorizes to packed `vmulpd`/`vsubpd`, NO
   `vfmadd…pd`.
3. **Real-solver `perf stat -r3 -e instructions`** both tiers vs baseline (NOT a
   microbench — that lied last round). Sanity-check wall (best-of-N) too, since SIMD
   is wall-real unlike bounds-check removal.

**GO** = bit-exact PASS + visible packed SIMD (no fma) + real-solver instructions
drop. Otherwise **NO-GO** — the only remaining lever (static-pattern sparse codegen)
is blocked by data-dependent pivoting bit-exactly. Report numbers; user decides
ship/extend/stop.

## Result (measured 2026-06-08) — GO, SHIPPED. Best win of the effort.

`gauss_solve_16` (hardcoded N=16, physical row swaps, contiguous inner loops),
dispatched from the realtime dense path when `u == 16` (`wdf.rs:1670`). Generic
`gauss_solve` retained for the Schur small-n call + non-16 fallback.

- **Realtime instructions −4.07%** (86.184 → 82.676 B). **Realtime wall-min −5.93%**
  (24.801 → 23.330 s, min-of-5). Wall BEATS instructions — real SIMD throughput, the
  inverse of the get_unchecked round (−12% insns / −2% wall, ILP-hidden). This stacks
  on top of B1's −3.9%.
- **Fast tier unaffected** (+0.15%, noise) — correctly gated on `u == 16`; the fast
  tier's Schur path and the small-n N-block still use the untouched generic solver.
- **Bit-exact:** the relocated in-module `gauss_solve_16_is_bit_exact` (2000 systems:
  diagonally-dominant, pivot-triggering 1e6 off-diagonals, near-singular proportional
  rows, mixed 1e±6 magnitudes, all-negative) — every `b[i].to_bits()` identical and
  the singular-bool matched. End-to-end render `cmp` byte-identical to baseline, so the
  rust-vs-cpp ESR parity gate carries over unchanged. Full suite (36) green.
- **objdump (elimination loop):** packed `vbroadcastsd` f → `vmulpd` (ymm) → `vsubpd`
  (ymm), separate (NOT fused) — **zero `vfmadd…pd`**, which is exactly why output stays
  bit-identical. Scalar `vmulsd`/`vsubsd` remain for the `col..16` remainder and the
  back-sub reduction (kept scalar by design). The physical-swap 16-elem row memcpy also
  vectorized; the elimination SIMD gain more than pays for it.

Commits: `c9b31d2` (spike/impl) + the in-module-test/private-visibility cleanup.
**lu_factor (fast tier) deferred to a follow-on round** (same technique applies).

## Critical files

- `plugin/src/wdf.rs` — `gauss_solve_16` + realtime dispatch (`wdf.rs:1670`); generic
  `gauss_solve` retained; `gauss_solve_16_is_bit_exact` in the in-module test mod.
  `lu_factor` follow-on round.
