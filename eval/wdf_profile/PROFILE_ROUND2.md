# WDF hot-path profile — round 2 (2026-06-15, post stereo-lockstep NO-GO)

Re-profiled the shipping WDF path on current code to find a real CPU lever after the
stereo-lockstep experiment proved a NO-GO (it targeted the dense solve, which is small AND
already vectorized — see memory `ba283-stereo-lockstep`).

## Rig bug found + fixed

`profile.sh`'s build hint set `RUSTFLAGS='-C force-frame-pointers=yes'`, which **REPLACES**
`.cargo/config.toml`'s `target-cpu=x86-64-v3` (RUSTFLAGS does not merge). Result: the fma=true
tiers (fast/ultra/warp) profiled with **software FMA** — `mul_add` fell back to the libm `fma()`
routine, showing a fake **~25%** `fma_with_fma`+`fma`, and `floor` became a libm call (3.5%)
instead of `vroundsd`. The shipped plugin always has v3, so those numbers were artifacts.
Fixed: the hint now prepends `-C target-cpu=x86-64-v3`. Always profile with v3.

## Flat self-time, v3 (corrected)

| function | ultra-fast | fast | realtime |
|---|---:|---:|---:|
| `newton_iterate` (self, incl. inlined exp/Gummel-Poon/schur) | 52.7% | 51.4% | 47.1% |
| `depletion_charge_pre_batch` (in-Newton cap charge) | 9.7% | 9.7% | 14.9% |
| `residual_norm` | 7.6% | 7.4% | 9.5% |
| `depletion_charge_pre` (scalar, trailing-update recompute) | 6.9% | 6.2% | 8.0% |
| `lu_solve_10` (Schur linear block, flat) | 3.7% | 5.5% | — |
| `compute_bjt` / `diode_branch` (BJT transport, exp) | ~9% | ~14% | ~11% |

`newton_iterate` is NOT all assembly — perf's caller tree shows it inlines the device kernels
(`exp_bl` L499, `compute_bjt_preexp_batch` L745-784) and the solve (`schur_solve` ≈ 18-22% of
newton_iterate → `gauss_solve_6` + `lu_solve_10`). Reconstructed global split (ultra-fast):

- **device eval (exp + Gummel-Poon + depletion batch): ~17%**
- **solve (Schur: gauss_solve_6 + lu_solve_10): ~10-13%** (hidden in the flat table by inlining)
- **scatter/assembly + linear stamps (node_v/ur indirection, conditional writes): ~14%**
- **residual_norm: ~7.5%**  (line-search / gmin re-stamp)
- trailing-update charge recompute (scalar `depletion_charge_pre`): ~7% (ultra/fast: cache is
  disabled by `freeze_caps` + `cheap_exp`, so the epilogue recomputes via `junction_charges`)

## Conclusion: the easy wins are already shipped

The hot path is broadly distributed with **no single dominant low-risk bottleneck**. The big
levers already landed: ISA floor v3 (+9.8%), FMA contraction, trailing-state reuse, the precision
dial, and the warp tier. The solve is small and, as proven, does not benefit from cross-channel
SIMD (the scalar dense elimination is already 16-wide autovectorized).

### Remaining candidate levers — all lossy and/or uncertain (measure before building)

1. **Linear-stamp → `g0·v` matvec** (~7%): replace the per-branch resistor/cap/inductor residual
   stamps (node_v/ur indirection + conditional scatter) with one branchless dense 16×16 matvec
   (`g0` already exists; known-node terms fold into `hist` in the prologue). LOSSY (different
   summation order → not bit-exact), so fast/ultra/warp only; realtime/High must stay scalar.
   Payoff UNCERTAIN: 256 dense MACs vs ~24 sparse stamps — trades branches/indirection for
   vectorizable arithmetic. Needs an A/B measurement.
2. **Depletion-charge LUT** (~16% non-warp): `depletion_charge_pre` uses `pow = exp(ex·log(x))`
   (two transcendentals); the charge is a smooth 1-D function of v per junction → a LUT/spline
   would replace it with a table lookup. LOSSY (resolution), ESR-gated, fast/ultra only.
3. **residual_norm batching** (~7.5%): already tried — NET LOSS (memory `ba283-wdf-phase2-autovec`).

Recommendation: the WDF hot path is near its low-risk floor. Pursue a lever only as a measured
spike (A/B rt_factor + ESR gate) on fast/ultra/warp, expecting single-digit-% gains; do not
expect another large win. Realtime/High is constrained by the bit-exact golden contract.
