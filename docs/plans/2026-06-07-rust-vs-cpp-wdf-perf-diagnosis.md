# Why the Rust WDF port is ~1.17× slower than the C++ original — diagnosis

**Date:** 2026-06-07 · **Scope:** diagnose-only (no fix) · **Branch context:** post-merge of #14

## Verdict (one line)

The Rust port is slower **not because of codegen config or algorithm, but because it
retires ~1.5× more instructions / ~1.8× more branches — almost entirely array
**bounds checks** on the flat-matrix accesses in the dense `gauss_solve` elimination
(and the `newton_iterate` stamping). The C++ build has none. Branch mispredict is ~0,
so it is instruction/branch *volume*, not stalls.**

## What was ruled out first

- **Build/codegen config:** workspace `[profile.release]` already has `lto="fat"`,
  `codegen-units=1`, `opt-level=3`; both binaries built with `target-cpu/-march=native`.
  No cheap codegen win available.
- **Algorithm / iteration count:** `avgIters` is **identical** Rust=C++ in every bench
  cell (realtime 2.456, fast 2.215). Same Newton path; the gap is pure per-iter machine cost.

## Evidence

### P1 — `perf stat` (36 s stim, gain 0.5, 2 runs, warmup included)

| metric | cpp rt | rust rt | rust/cpp | cpp fast | rust fast | rust/cpp |
|---|---|---|---|---|---|---|
| instructions | 190.6 B | 297.1 B | **1.56×** | 113.7 B | 158.4 B | **1.39×** |
| branches | 26.2 B | 48.1 B | **1.84×** | 15.6 B | 27.8 B | 1.78× |
| cpu-cycles | 93.6 B | 117.8 B | 1.26× | 54.2 B | 64.7 B | 1.19× |
| IPC | 2.0 | **2.5** | — | 2.1 | **2.4** | — |
| branch-miss | 0.1 % | 0.0 % | — | 0.1 % | 0.1 % | — |

Rust executes far more instructions/branches but at **higher IPC** (2.5 vs 2.0) — it
pipelines the extra scalar work well, which is why the wall gap (1.17× in the clean
bench, 1.26× here with perf overhead) is *smaller* than the instruction gap (1.56×).
Branch-miss ≈ 0 ⇒ the extra branches are perfectly-predicted bounds checks, not stalls.

### P2 — `perf record` (self time, Rust realtime)

| function | self % | nature |
|---|---|---|
| `WdfModel::gauss_solve` | 28.1 % | dense O(n³) elimination, flat `a[]` + `row[]` indirection |
| `depletion_charge` | 25.8 % | junction-cap fast-transcendental series (no array indexing) |
| `WdfModel::newton_iterate` | 20.0 % | Jacobian/residual stamping over node arrays |

`depletion_charge` is heavy but uses the **same** custom `fast_pow_pos`/`fast_exp`
algorithm as C++ (`main.cpp:154-170`) with no array indexing → no bounds checks; it
costs ~the same absolute time in both languages and contributes little to the *gap*.
The gap lives in the array-indexed `gauss_solve` + `newton_iterate`.

### P3 — disassembly (`objdump --demangle`)

- Binary-wide: **340** `panic_bounds_check` references.
- `gauss_solve` block: **513 asm lines, 18 `panic_bounds_check` call sites, 46
  conditional branches, only 4 vector FP ops** (rest scalar `mulsd/subsd/divsd`).
- Explicit guards: `cmp $0x1b9; jae <panic>` (0x1b9 = 441 = `NUM_NODES²`, the flat
  `a[]` length) and `cmp $0x15 / $0x14` (21 = `row[]` length) on each access.
- The inner loop `a[row[r]*n+c] -= f * a[row[col]*n+c]` indexes `a[]` **indirectly**
  through the `row[]` permutation → the LLVM vectorizer bails (only 4 `*pd` ops). The
  **C++ uses the identical index-swap pivot**, so it is *also* scalar here — meaning the
  delta is purely the bounds-check instructions C++ omits, **not** a SIMD-width gap.

### Quantitative attribution

Extra branches realtime: 48.1 B − 26.2 B = **21.9 B**. At ~5 instructions per bounds
check (address calc + `cmp` + `jae`) ≈ **110 B** extra instructions ≈ the measured
**106 B** instruction delta (297.1 − 190.6). Bounds checks account for essentially the
entire gap; no residual unexplained term.

## Conclusion

| factor | contributes to gap? |
|---|---|
| Bounds checks on `gauss_solve`/`newton_iterate` flat-array + `row[]` access | **Yes — dominant (~all of it)** |
| Blocked autovectorization | No — `row[]` indirection makes *both* langs scalar |
| `depletion_charge` transcendentals | No — identical algorithm, no indexing, equal both langs |
| `panic=unwind` landing pads | Negligible |
| Codegen config / target-cpu | No — already at parity |

The port is a faithful, well-pipelined translation; its only systematic tax is Rust's
safety bounds checks on the hot linear-solve arrays.

## (Optional, not in scope) remediation directions

If a fix is later wanted: `get_unchecked`/`get_unchecked_mut` on the proven-in-range
hot loops in `gauss_solve`/`lu_factor`/`schur_solve`/stamping, or restructure the flat
`a[]`/`row[]` access into slice iterators the bounds-check elision can prove safe.
Expected to recover most of the ~1.5× instruction gap; would need re-running the #14
parity gate (golden nulls must stay bit-exact) + this perf harness to confirm.

## Reproduce

```bash
# binaries: cargo build --release --bin ba283_standalone (RUSTFLAGS=-C target-cpu=native);
#           make -C ~/Git/ba283_wdf  (g++ -O3 -march=native)
# P1: perf stat -d -r2 on each; C++ runs from a cwd holding input.wav
# P2: perf record --call-graph fp -- <rust>; perf report --stdio --no-children
# P3: objdump -d --demangle target/release/ba283_standalone | awk '/gauss_solve.*>:/.. '
```
