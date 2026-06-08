# WDF hot-path profile — profile-first optimization round

Goal: find where the `ultra-fast` WDF tier actually spends per-sample time on the
**VST3 real-time path** (single-core, sequential), then spend the round on the
single highest-leverage, lowest-risk lever the data points to.

Rig: `eval/wdf_profile/profile.sh` — `perf record -e task-clock --call-graph
dwarf` on `ba283_standalone --backend wdf` (full chain, the real DSP the plugin
runs; **not** `--raw`, which bypasses the WDF and runs the NN). Thin-LTO build
with frame pointers for clean attribution. Stimulus: sweep + music + pink, 96 kHz.

## Flat self-time (ultra-fast tier, 95k samples)

| function | self % | bucket |
|---|---:|---|
| `WdfModel::newton_iterate` | **52.9%** | matrix assembly / scatter stamps + inline arithmetic |
| `depletion_charge_pre`     | 10.8% | BJT cap companion — `pow_` = `fast_exp(ex·fast_log(x))` |
| `diode_branch`             | 7.6%  | transcendental `exp_` |
| `lu_solve_10`              | 7.1%  | linear solve (Schur linear block) |
| `residual_norm`            | 5.5%  | Newton residual |
| `compute_bjt`              | 4.2%  | BJT outer |
| `WdfModel::process`        | 3.4%  | os loop / companion update |
| `floor` (libm)             | 3.1%  | `fast_exp`/`fast_log` range reduction (see ISA finding) |
| libm anon + misc           | ~5%   | overhead, I/O |

`fast`/`realtime` tiers show the same distribution (±1 pt) — the hot path is
tier-independent; ultra-fast just runs it at `os=1`.

Transcendental total ≈ **22%** (`depletion_charge_pre` + `diode_branch` + part of
`compute_bjt`). Linear solve ≈ **7%** (small: N=16, Schur shrinks to 6×6). The
solve is *not* the bottleneck; **matrix assembly + transcendentals dominate.**

## The actual headline: the build, not a source hotspot

The shipped VST3 (`xtask` bundler) and `.cargo/config.toml` carried **no ISA
flags → generic x86-64 baseline (SSE2)**: no AVX2, no FMA, and the `floor` in
range-reduction compiled to a **libm call** instead of an inlined `vroundsd`
(that's the 3.1% `floor` line). Only `eval/wdf_rust_vs_cpp/bench.py` built with
`target-cpu=native` — so the number we benchmark (rt_factor 0.77) was never what
users got from the shipped plugin.

### ISA experiment (fat-LTO release = ship config; best-of-3, 35.75 s stim)

| ISA | rt_factor | ×realtime | vs baseline | output |
|---|---:|---:|---:|---|
| baseline (was shipping) | 0.616 | 1.62× | — | — |
| **x86-64-v3 (AVX2+FMA)** | **0.556** | **1.80×** | **+9.8%** | **bit-identical** |
| native | 0.572 | 1.75× | +7.2% | bit-identical |

Bit-identical because Rust does **not** auto-contract `a*b+c` into FMA (LLVM
contraction is off by default in rustc) — `target-cpu=v3` only unlocks AVX2 width,
`vroundsd`, and better register allocation, with **no numeric change**. So
rust↔cpp parity and vs-SPICE ESR are unaffected by construction. v3 even beat
native here (256-bit AVX2 sweet spot; native's wider/older paths didn't help).

## Decision

**Ship `target-cpu=x86-64-v3` as the distributable x86_64 ISA floor.** +9.8% on
the RT plugin, zero numeric risk, zero source change, covers Haswell-2013+ /
Excavator-2015+ (≈ all DAW-capable hardware). Wired in `.cargo/config.toml`
scoped to `cfg(target_arch = "x86_64")` so aarch64 (Apple Silicon) and host
build-tools are untouched; env `RUSTFLAGS` still overrides, so the eval/data-gen
harnesses stay `native`.

Confirmed: config build emits 55 AVX/FMA ops (baseline: 0) and renders
bit-identical to the old baseline binary.

## Scoped out (next-tier levers, not this round)

- **Source-SIMD / SoA restructure of `newton_iterate` assembly (52.9%)** — the big
  prize, but the scatter-stamp is indexed (hard to auto- *or* hand-vectorize) and
  the `gauss_solve_6/16` + `lu_solve_10` carry bit-for-bit identity test contracts.
  v3's auto-vectorizer already captured ~10%; the marginal hand-SIMD gain is
  uncertain and would need its own measure+parity cycle.
- **Cheaper `pow_` in `depletion_charge_pre` (10.8%)** — fixed junction-grading
  exponent could use a specialized fractional power, but it's a numeric change
  requiring a C++ mirror + bit-exact reconcile.
