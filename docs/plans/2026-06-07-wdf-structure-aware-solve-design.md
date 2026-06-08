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

## Next round (2026-06-08): portable autovec of the BJT transcendentals

After B1, the remaining transcendental cost is the per-Newton-iter **6 diode
`exp_`** (3 BJTs × `vbe`/`vbc`, inside `compute_bjt`, `wdf.rs:439-440`) plus up to
**6 depletion `pow_`** (forward branch of `depletion_charge`, `wdf.rs:352-353`).
The original Approach B proposed hand-written `std::arch` AVX2. **Rejected** —
adds `unsafe` to the audio path (the project already reverted `unsafe` for −2%),
x86-only (nothing on ARM/Apple Silicon, which nih_plug VST3 ships), and the AVX2
intrinsics don't port to C++, defeating the round's stated "both langs" goal.

**Chosen: portable autovectorization.** Restructure the per-junction
transcendentals into a contiguous SoA array + branchless body so LLVM emits packed
SIMD on its own — lands on x86 AVX2 **and** ARM NEON, no `unsafe`, and the same
SoA refactor ports to C++. Expected ~3–6% (same ballpark as B1; uncertain to fire).

### Bit-exactness under autovec

Per-lane packed `mul`/`add` is bit-identical to scalar `mul`/`add` — IEEE-754,
no reassociation without fast-math (off). The ONLY hazard is `fma` contraction
(`a*b+c → vfmadd`), which Rust does **not** enable by default (no
`-ffp-contract=fast`) even under `target-cpu=native`. The spike must confirm
`objdump` shows `vmulpd`+`vaddpd` (NOT `vfmadd…pd`) in the kernel. The branchy
`fast_exp` k-guard (`wdf.rs:284`) and `depletion_charge` `FC` branch become
compute-both-sides + `select`; the selected result is bit-identical to the scalar
branch taken, pinned by a `to_bits` equivalence test (same pattern as
`depletion_charge_pre_is_bit_exact`).

### Spike gate (go/no-go, BEFORE the invasive wiring)

The SoA wiring through `compute_bjt`'s signature + two hot call sites
(`residual_norm:1060`, `newton_iterate:1449`) is invasive. Gate it: build ONE
branchless batched `exp_` kernel over a `[f64; N]` array, microbench it, and
`objdump` the kernel. Proceed only if (a) it autovectorizes (packed `vmulpd`,
no `vfmadd`), (b) it is `to_bits`-exact vs scalar `fast_exp` over the junction
domain, and (c) the batched form drops instructions in `perf stat`. If any fail,
bail — B1 is the round (same discipline that killed T2 at 0%).

### Result (measured 2026-06-08) — spike PASS, real-path DUD, reverted

**Spike (Task 1, commit `8625362`) — GO in isolation.** A branchless `exp_bl`
(k-guard + overflow as `select`s, exponent pre-clamped before the IEEE shift) is
`to_bits`-exact vs `fast_exp` across the full junction domain (deep underflow,
the `k<=-1023` boundary, the active region). It autovectorizes to 256-bit `ymm`
`vmulpd`/`vaddpd` with **zero `vfmadd`** (bit-exact). Batch-vs-scalar in a tight
loop: **−2.6%** instructions. Kept as a record/guard (`plugin/tests/autovec_spike.rs`).
*Caveat surfaced by the spike:* scalar `fast_exp` ALSO partially autovectorizes
when placed in a batch loop, so the branchless form's value is mainly *guaranteeing*
full-width packing — the raw delta is small.

**Real-path wiring (Task 3, commits `6416ecf`+`b6e5d59`) — DUD, REVERTED.** Split
`compute_bjt` into `diode` + pure algebra (no-op, golden-verified) and rewired the
`residual_norm`/`newton_iterate` 3-BJT loops into gather → batched-diode → scatter.
The batch loop vectorized correctly (objdump: packed `vmulpd`/`vaddpd`/`vsubpd`,
**zero `vfmadd`**, all golden/lossless tests bit-exact green), but **REGRESSED both
tiers: realtime +1.98% (86.18→87.89 B), fast +2.94% (45.31→46.64 B).** Two reasons:
(1) only 6 junctions — too small a working set to amortize the SoA gather/scatter
marshalling; (2) the branchless batch must evaluate **both** diode branches
unconditionally (2 `exp` per junction), whereas scalar `diode_branch` takes the
single `v < vcrit` branch (1 `exp`) for the common case — the doubled transcendental
work outweighs the 6-wide packing. Reverted in full; **the round ends at B1.**

**Lesson:** autovec GO in a microbench does NOT imply a win at the call site. The
batchable set here (6) is below the break-even where SIMD packing beats both the
marshalling overhead AND the compute-both-branches tax that branchless vectorization
forces. A real autovec win would need either a much larger junction batch or a way
to keep the cheap single-branch path — neither available in this circuit. Don't retry
SIMD/autovec on the BJT transcendentals for BA283.

## Critical files

- `plugin/src/wdf.rs` — sparsity-skip + indirection removal + dirty-`j` reset in
  the linear-solve and Newton hot paths; (next round) branchless SoA
  transcendental kernels + autovec wiring through `compute_bjt`.
- `eval/wdf_rust_vs_cpp/{compare,bench}.py` — existing gate, reused unchanged.
