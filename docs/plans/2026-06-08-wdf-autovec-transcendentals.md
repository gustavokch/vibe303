# Portable autovec of the BJT transcendentals — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> to execute this plan task-by-task.

**Goal:** Cut per-Newton-iter transcendental cost (6 diode `exp_` + up to 6
depletion `pow_`) by restructuring the per-junction math into a contiguous SoA
array + branchless body so LLVM autovectorizes it. No `unsafe`. Bit-exact.

**Architecture:** Branchless, SoA-friendly variants of `fast_exp` (and the
`diode_branch` it backs) that LLVM emits as packed `vmulpd`/`vaddpd` over a small
fixed array. The 3-BJT loops in `residual_norm` and `newton_iterate` are
restructured to (1) gather `vbe`/`vbc` per BJT, (2) evaluate all diode transports
in one vectorizable loop, (3) finish the `compute_bjt` algebra + scatter. Lands on
x86 AVX2 and ARM NEON; the SoA shape ports to C++.

**Tech Stack:** Rust (nih_plug plugin core), `cargo test -p ba283`,
`perf stat -r3` (instructions retired = the verdict metric), `objdump`, Python
eval harness (`uv run python -m eval.wdf_rust_vs_cpp.{compare,bench}`).

**Design:** `docs/plans/2026-06-07-wdf-structure-aware-solve-design.md`
(§ "Next round (2026-06-08): portable autovec").

## Bit-exact discipline (read first)

Refactor under test, NOT red-green-refactor. Guard = the existing suite, green at
every step:
- `cargo test -p ba283` — `decimated_output_matches_cpp` (`OUTPUT_256` ≤ 1e-5),
  `process_settles_toward_golden`/`warmup_reaches_golden_operating_point`
  (`SETTLED_V` ≤ 1e-6), the `*_lossless` zero-alloc tests, and the existing
  `depletion_charge_pre_is_bit_exact`.
- Allowed transforms: provable no-ops only. Branch→`select` is allowed ONLY when
  the selected value equals the scalar branch taken bit-for-bit. NO FP
  reassociation, NO reordering of reductions, NO `fma` contraction.
- Every new kernel gets a `to_bits` equivalence test vs the original scalar fn
  over the full junction domain BEFORE it is wired into the hot path.

## Measurement protocol

Wall-clock on this box is useless (load 5–7; unchanged C++ drifts ±0.1–0.4
rt-factor). Verdict metric = **instructions retired**, `perf stat -r3` on the
standalone at both tiers, A (baseline) vs B (change). Record the baseline once at
Task 1 and compare every subsequent task against it.

Baseline command (run from repo root, release + native):
```
RUSTFLAGS="-C target-cpu=native" cargo build --release --bin ba283_standalone
perf stat -r3 -e instructions target/release/ba283_standalone \
  --backend wdf --tier realtime --circuit-gain 220 --bias 0.77 \
  --mix 100 --drive-db 0 --output-db 0 \
  --input data/dry/sweep_m6.wav --output /tmp/av_rt.wav
# repeat with --tier fast
```
(Use whatever dry stim the existing bench uses; match it across A/B. Known
post-B1 baseline for reference: realtime ≈ 81.0 B, fast ≈ 43.8 B instructions.)

---

### Task 1: Autovec feasibility spike (GO/NO-GO gate)

**Files:**
- Create: `plugin/tests/autovec_spike.rs` (a `#[test]` + a `#[ignore]` micro-driver)
- Read only: `plugin/src/wdf.rs:266-293` (`fast_exp`)

**Intent:** Prove a branchless batched `exp` kernel (a) autovectorizes to packed
`vmulpd`/`vaddpd` with NO `vfmadd`, (b) is `to_bits`-exact vs `fast_exp` over the
junction domain, (c) drops instructions when batched. Do NOT touch the hot path yet.

**Step 1: Write a branchless batched exp prototype** (in the test file, or a
`pub(crate)` helper in `wdf.rs` gated behind the spike — implementer's call, but
keep it out of the audio path until Task 2).

```rust
// branchless single-lane equivalent of fast_exp (wdf.rs:266-293):
// poly is identical Horner; the k<=-1023 underflow and k>=1024 cases become selects.
fn exp_bl(x: f64) -> f64 {
    const LN2: f64 = 0.6931471805599453;
    const INV_LN2: f64 = 1.4426950408889634;
    let kf = (x * INV_LN2 + 0.5).floor();
    let k = kf as i32;
    let r = x - kf * LN2;
    let p = 1.0 + r*(1.0 + r*(1.0/2.0 + r*(1.0/6.0 + r*(1.0/24.0 + r*(1.0/120.0
          + r*(1.0/720.0 + r*(1.0/5040.0 + r*(1.0/40320.0 + r*(1.0/362880.0)))))))));
    // clamp exponent so the shift never wraps; SELECT 0.0 on underflow, p*inf on overflow.
    let kc = k.clamp(-1022, 1023);
    let u: u64 = ((kc + 1023) as u64) << 52;
    let pk = f64::from_bits(u);
    let mut res = p * pk;
    if k <= -1023 { res = 0.0; }          // select, not control flow over the poly
    if k >= 1024  { res = p * f64::INFINITY; }
    res
}
fn exp_bl_batch(xs: &[f64], out: &mut [f64]) {
    for i in 0..xs.len() { out[i] = exp_bl(xs[i]); }
}
```

**Step 2: Bit-exactness test** — `to_bits` equality vs `fast_exp` over the domain
that diode/junction args actually span (sweep `x` across e.g. `-1400.0..=40.0` in
fine steps; include the `k<=-1023` underflow region and near-zero). Assert
`exp_bl(x).to_bits() == fast_exp(x).to_bits()` for every sample.

```bash
cargo test -p ba283 --test autovec_spike -- --nocapture
# Expected: PASS (bit-exact)
```

**Step 3: Confirm autovec + no fma in objdump.**
```bash
RUSTFLAGS="-C target-cpu=native" cargo build --release --tests
# disassemble exp_bl_batch; confirm packed vmulpd/vaddpd, and assert NO vfmadd:
objdump -d target/release/deps/autovec_spike-*.* | grep -A400 'exp_bl_batch' \
  | grep -E 'vmulpd|vaddpd|vfmadd' | sort | uniq -c
# Expected: vmulpd/vaddpd present (ideally the ...pd ymm packed form), vfmadd ABSENT.
```
If `vfmadd` appears, the kernel is NOT bit-exact vs scalar — STOP, report, do not
proceed (would need explicit `mul_add`-free restructuring or `-ffp-contract=off`).

**Step 4: Instruction A/B on the kernel** — `#[ignore]`d bench test that runs
`exp_bl_batch` over a large array N times; `perf stat -r3` the test binary vs a
scalar-`fast_exp` loop of the same shape. Record both counts.

**Step 5: GO/NO-GO.** GO only if: bit-exact PASS **and** packed SIMD with no
`vfmadd` **and** batched instructions < scalar. Otherwise NO-GO → report, stop the
round (B1 stands). Record the verdict + numbers in the design doc results section.

**Step 6: Commit** the spike (test + any `pub(crate)` helper), `cargo fmt`, message
`spike(wdf): autovec feasibility for batched exp — <GO|NO-GO>, <numbers>`.

---

### Task 2 (only if Task 1 = GO): wire branchless exp into `diode_branch`

**Files:**
- Modify: `plugin/src/wdf.rs` — promote the validated `exp_bl` to back `exp_`/
  `diode_branch` (`wdf.rs:400-410`), keeping `diode_branch`'s public signature and
  output bit-for-bit. Add a `diode_branch` `to_bits` equivalence test vs the
  pre-change version (capture reference values first).

**Step 1:** Add `diode_branch_is_bit_exact` test pinning current `(i,g)` outputs
(generate a reference table from the CURRENT `diode_branch` before editing).
**Step 2:** Replace the internal `exp_` with the branchless variant. Run
`cargo test -p ba283` — all green incl. `OUTPUT_256`/`SETTLED_V`.
**Step 3:** `perf stat -r3` A/B vs baseline, both tiers. Record.
**Step 4:** Commit `perf(wdf): branchless exp in diode_branch (<±x.x%>)`.

> If this alone (without the SoA batch loop) already moves instructions, it may be
> the whole win — re-evaluate whether Task 3's invasive SoA wiring is worth it.

---

### Task 3 (only if Task 2 underdelivers AND Task 1 showed batch > scalar): SoA batch the 6 diode evals

**Files:**
- Modify: `plugin/src/wdf.rs` — split `compute_bjt` so the 4 diode transports
  (`itf,gtf,itr,gtr`) can be supplied precomputed; add a batched evaluator that
  fills a `[f64; 6]` (or `[(f64,f64); 6]`) from the 3 BJTs' `vbe`/`vbc` in one
  vectorizable loop; restructure the loops at `residual_norm:1056` and
  `newton_iterate:~1449` into gather → batch-diode → finish+scatter.

**Step 1:** Extract `compute_bjt_from_diode(vbe, vbc, p, itf,gtf,itr,gtr) -> BjtOut`
(pure algebra, no transcendentals); keep `compute_bjt` as a thin wrapper that calls
`diode_branch` then `compute_bjt_from_diode` — assert byte-identical via existing
golden tests.
**Step 2:** In each hot loop: pass 1 gathers `vbe`/`vbc` + junction params into
fixed arrays; one batched loop evaluates all diode transports; pass 2 calls
`compute_bjt_from_diode` and scatters into `rn`/jacobian exactly as before (same
order, same branches).
**Step 3:** `cargo test -p ba283` green. `objdump` the batch loop → packed, no
`vfmadd`. `perf stat -r3` A/B both tiers.
**Step 4:** Commit `perf(wdf): SoA-batch BJT diode evals for autovec (<−x.x%>)`.

---

### Task 4: depletion `pow_` (optional, only if forward branch shows in profile)

The depletion forward branch (`wdf.rs:352-353`, two `pow_` = `exp∘log`) fires only
when a junction is reverse/low-biased. If `perf record` after Task 3 still shows it
hot, apply the same branchless+batch treatment to `fast_log`/`fast_pow_pos` over
the 6 junctions. Same gate: `to_bits` test, objdump no-`vfmadd`, instruction A/B.
Skip if not in the profile.

---

## Validation (the gate, in order)

1. `cargo test -p ba283` — all golden/settle/lossless + new `to_bits` equivalence
   tests green at every task.
2. `uv run python -m eval.wdf_rust_vs_cpp.compare` — realtime ESR ≤ −60 dB, fast
   ≤ −40 dB, FR ≤ 0.1 dB UNCHANGED (proves output preserved).
3. `perf stat -r3 -e instructions` A/B both tiers — report the delta per task.
   Success = measurable instruction drop with all gates green. No-op or regression
   on any task → revert that task.
4. Append measured results to the design doc; update the eval memory.

## Notes / risks

- **Autovec may not fire** (bitcast `from_bits`, selects). Task 1 is the hard gate
  — if NO-GO, the round ends with B1 as the win. Do not force it with `unsafe`.
- **`vfmadd` = silent bit-exactness break.** Every kernel objdump must show
  `vmulpd`+`vaddpd` and NO `vfmadd…pd`. This is non-negotiable.
- **`clamp` vs scalar wrap:** scalar `fast_exp` relies on the early `return 0.0`
  for `k<=-1023` (the raw shift would wrap). The branchless version clamps the
  exponent THEN selects 0.0 — the clamp only protects the discarded lane; the
  selected 0.0 matches scalar exactly. The `to_bits` test must cover this region.
- **Scatter stays scalar.** Only the transcendental kernels vectorize; the
  `self.rn[..] +=` / jacobian scatter keeps its exact order and index guards.
