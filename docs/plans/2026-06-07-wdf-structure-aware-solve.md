# Bit-exact structure-aware WDF solver speedup — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make the Rust WDF solver measurably faster in wall-clock terms with
zero change to output (existing golden vectors + rust-vs-cpp parity gates pass
unchanged).

**Architecture:** Surgical, bit-identical edits to the linear-solve and Newton hot
paths in `plugin/src/wdf.rs`: (1) reset only BJT-touched Jacobian entries each
iter instead of copying the whole 256-element matrix; (2) skip multiply-adds
against structural zeros in Gaussian elimination / LU / back-substitution; (3)
optionally remove `row[]`/`piv[]` indirection to let the elementwise elimination
autovectorize. No new modules, no new public API, no audio-thread allocation, no
`unsafe`. The existing `eval/wdf_rust_vs_cpp/` harness is the gate.

**Tech Stack:** Rust (nih_plug plugin core), `cargo test`, Python eval harness
(`uv run python -m eval.wdf_rust_vs_cpp.{compare,bench}`), C++ reference at
`~/Git/ba283_wdf/ba283_sim`.

**Design:** `docs/plans/2026-06-07-wdf-structure-aware-solve-design.md`.

## Bit-exact discipline (read first)

This is a **refactor under test**, not red-green-refactor. The regression guard is
the *existing* suite — it must stay green at every step:

- `cargo test -p ba283` — in particular `decimated_output_matches_cpp`
  (`OUTPUT_256` ≤ 1e-5), `process_settles_toward_golden` /
  `warmup_reaches_golden_operating_point` (`SETTLED_V` ≤ 1e-6), the `*_lossless`
  lever tests, `fast_tier_is_near_lossless`, and the no-alloc tests
  (`process_host_sample_is_alloc_free`, `fast_tier_process_host_sample_is_alloc_free`).

Every Task 1–3 change must produce **identical** arithmetic in the **same order**
(skipping `x − f·0` and `x − 0·y` no-ops is allowed; reordering / reassociation is
NOT). If any golden/lossless test moves even at the 1e-12 level, the change
reassociated — revert and rethink. Per-task: tests green → bench delta → commit.

## Baseline (do once, before Task 1)

Record the starting numbers so each task's delta is attributable.

**Step 1:** Build both binaries native-opt.

```bash
cd ~/Git/ba283-vst3 && RUSTFLAGS="-C target-cpu=native" cargo build --release --bin ba283_standalone
make -C ~/Git/ba283_wdf
```

**Step 2:** Capture baseline parity + perf.

```bash
cd ~/Git/ba283-vst3
uv run python -m eval.wdf_rust_vs_cpp.compare --stims sweep_m3 --gains 0.5 --tiers realtime,fast --out eval/wdf_rust_vs_cpp/out
uv run python -m eval.wdf_rust_vs_cpp.bench --stim sweep_m3 --out eval/wdf_rust_vs_cpp/out
```

Expected: realtime ESR ≤ −60 dB, fast ≤ −40 dB; bench prints the 4-config
rt-factor table. **Copy the bench table into a scratch note** — it is the
before/after reference for Tasks 1–3.

---

### Task 0: Sparsity census (go/no-go gate)

**Why:** the prior diagnosis assumed `gauss_solve` is dense but never measured the
zero pattern or fill-in. If the 16×16 is near-dense (or fill-in saturates by ~col
3), Tasks 2–3 have little to gain → skip straight to Approach B (BJT-SIMD). This
task only measures; it changes no production code path.

**Files:**
- Test: `plugin/tests/test_wdf.rs` (append one `#[ignore]` diagnostic test)

**Step 1: Write the census test**

Append to `plugin/tests/test_wdf.rs`. It warms up the model, runs a few process
samples to reach a realistic operating point, then snapshots `g0` and simulates
the elimination pattern, counting structural zeros per elimination column
(including fill-in).

```rust
#[test]
#[ignore = "diagnostic: run with --ignored --nocapture to print the sparsity census"]
fn sparsity_census() {
    use ba283::wdf::{QualityConfig, WdfModel, NUM_NODES};
    let mut m = WdfModel::new_default(QualityConfig::realtime(), 96_000.0);
    m.warmup();
    for i in 0..256 {
        let x = (i as f64 * 0.05).sin() * 0.1;
        let _ = m.process(x as f32);
    }
    let u = m.u();
    let g0 = m.g0_snapshot(); // flat u*u, row-major (see Step 3)
    // count structural zeros in the static matrix
    let mut zeros = 0usize;
    for i in 0..u {
        for j in 0..u {
            if g0[i * u + j] == 0.0 {
                zeros += 1;
            }
        }
    }
    eprintln!("U={u} entries={} structural_zeros={zeros} density={:.1}%",
        u * u, 100.0 * (u * u - zeros) as f64 / (u * u) as f64);
    // simulate dense elimination fill-in (no pivoting needed for a pattern census)
    let mut nz: Vec<bool> = (0..u * u).map(|k| g0[k] != 0.0).collect();
    for col in 0..u {
        let live: usize = (0..u).flat_map(|r| (0..u).map(move |c| (r, c)))
            .filter(|&(r, c)| r >= col && c >= col && nz[r * u + c]).count();
        eprintln!("col {col:2}: live_nonzeros_in_trailing_submatrix={live}");
        for r in (col + 1)..u {
            if !nz[r * u + col] { continue; }
            for c in (col + 1)..u {
                if nz[col * u + c] { nz[r * u + c] = true; } // fill-in
            }
        }
    }
    let _ = NUM_NODES;
}
```

**Step 2: Add the `g0_snapshot` accessor it needs**

In `plugin/src/wdf.rs`, near the existing `pub fn u()` / `pub fn n_res()`
accessors (~line 649), add a test-support getter (cheap copy, off the audio path):

```rust
/// Flat row-major copy of the constant conductance matrix `g0` (first `u*u`
/// entries). Diagnostic/test use only.
pub fn g0_snapshot(&self) -> Vec<f64> {
    let u = self.u;
    self.g0[..u * u].to_vec()
}
```

(If a `new_default` constructor with that exact name does not exist, use whatever
the other tests in `test_wdf.rs` use to build a realtime model — match
`process_settles_toward_golden`'s construction verbatim.)

**Step 3: Run the census**

```bash
cd ~/Git/ba283-vst3 && cargo test -p ba283 --test test_wdf sparsity_census -- --ignored --nocapture
```

Expected: prints `density=...%` and the per-column trailing-submatrix nonzero
count.

**Step 4: Go/no-go decision**

- **density ≤ ~60% AND trailing nonzeros stay well below dense through ~col 6** →
  proceed to Tasks 1–3 (sparsity has real headroom).
- **density > ~80% OR fill-in saturates within 2–3 columns** → Tasks 2–3 unlikely
  to pay; do Task 1 (free), then jump to the Approach B addendum. Record the
  decision in the commit message.

**Step 5: Commit**

```bash
git add plugin/tests/test_wdf.rs plugin/src/wdf.rs
git commit -m "test(wdf): add ignored sparsity census + g0_snapshot accessor"
```

---

### Task 1: Dirty-entry Jacobian reset (free, always do)

**Why:** `newton_iterate` rebuilds `J` each iter with
`self.j[..u*u].copy_from_slice(&self.g0[..u*u])` — a full 256-element copy. Only
the BJT loop writes to `J` (linear branches stamp `F` only; their `J` entries are
already baked into `g0`). So between iters we need restore only the BJT-touched
entries. The touched index set is static (same terminals every iter) → precompute
once. Bit-identical: `J` ends each iter holding exactly the same values.

**Files:**
- Modify: `plugin/src/wdf.rs` — add a `nl_j_idx` field, populate it where the
  Schur partition is built (it already iterates `self.bjts`), use it in
  `newton_iterate` (~line 1323).

**Step 1: Add the field**

In the `WdfModel` struct, alongside the other Newton scratch/index fields, add:

```rust
/// Flat `j[]` indices the BJT stamping writes each iter (static, from the BJT
/// terminal node rows). Lets `newton_iterate` restore only these to `g0`
/// instead of copying the whole `u*u` matrix.
nl_j_idx: Vec<usize>,
```

Initialize it empty (`nl_j_idx: Vec::new(),`) in the constructor, next to the
other field inits. (It is sized once at build time, never on the audio path.)

**Step 2: Populate it once at build time**

In `build_schur_partition` (it already loops `self.bjts` over `[b, e, c]`), OR in
the main build after `urow`/`u` are final, compute every `(row*u + col)` pair the
BJT stamping can touch. The stamping writes `j[rb*u+rb]`, `j[rb*u+re]`,
`j[rb*u+rc]`, `j[re*u+rb]`, `j[re*u+re]`, `j[re*u+rc]`, `j[rc*u+rb]`,
`j[rc*u+re]`, `j[rc*u+rc]` for each BJT (only where each row index ≥ 0):

```rust
let u = self.u;
let mut idx = Vec::new();
for t in self.bjts.iter() {
    let rows = [self.urow[t.b as usize], self.urow[t.e as usize], self.urow[t.c as usize]];
    for &r in rows.iter() {
        if r < 0 { continue; }
        for &c in rows.iter() {
            if c < 0 { continue; }
            idx.push(r as usize * u + c as usize);
        }
    }
}
idx.sort_unstable();
idx.dedup();
self.nl_j_idx = idx;
```

Place this where it runs once per build/rebuild (so a tier or gain change
re-derives it). Put it after the partition is known to be valid.

**Step 3: Use it in `newton_iterate`**

Replace the full-matrix copy at the top of the `while iter < self.max_iters` loop
(`plugin/src/wdf.rs:1323`):

```rust
self.j[..u * u].copy_from_slice(&self.g0[..u * u]);
```

with a restore of only the previously-touched entries. On the **first** iter the
matrix must equal `g0`; the cleanest bit-identical form keeps the full copy on
iter 0 and does the cheap restore on subsequent iters:

```rust
if iter == 0 {
    self.j[..u * u].copy_from_slice(&self.g0[..u * u]);
} else {
    for &k in self.nl_j_idx.iter() {
        self.j[k] = self.g0[k];
    }
}
```

(Iter 0 still pays one full copy per Newton solve, but the inner iters — the
majority at avgIters ≈ 2.2–2.5 — drop from 256 writes to ≤27. Output identical:
every `J` entry the next BJT stamp touches is restored to `g0`; all others were
never modified.)

**Step 4: Tests stay green**

```bash
cd ~/Git/ba283-vst3 && cargo test -p ba283
```

Expected: ALL pass, unchanged — especially `decimated_output_matches_cpp`,
`process_settles_toward_golden`, `fast_tier_is_near_lossless`, and both
`*_alloc_free` tests (the `Vec` is built at construction, not in `process`).

**Step 5: Bench delta**

```bash
RUSTFLAGS="-C target-cpu=native" cargo build --release --bin ba283_standalone
uv run python -m eval.wdf_rust_vs_cpp.bench --stim sweep_m3 --out eval/wdf_rust_vs_cpp/out
```

Expected: rt-factor ≤ baseline at both tiers (copy reduction helps fast tier most,
where the dense solve is small so the copy is a larger share).

**Step 6: Commit**

```bash
git add plugin/src/wdf.rs
git commit -m "perf(wdf): restore only BJT-touched Jacobian entries between Newton iters"
```

---

### Task 2: Column-sparsity skip in elimination (gated by Task 0)

**Skip this task if Task 0 said no-go.** Otherwise:

**Why:** the elimination row-update loop runs `c in col..n` densely. When the
pivot-row entry `a[pivrow*n+c]` is zero, `a[r*n+c] -= f*0` is a no-op — skipping
it is bit-identical. Front-loaded benefit (sparser early, fills in later).

**Files:**
- Modify: `plugin/src/wdf.rs` — `gauss_solve` (~1063), `lu_factor` (~1109),
  `lu_solve` back-substitution (~1130), and the back-sub in `gauss_solve` (~1071).

**Step 1: gauss_solve elimination inner loop**

Current (`wdf.rs:1058-1067`):

```rust
for r in (col + 1)..n {
    let f = a[row[r] * n + col] / diag;
    if f == 0.0 { continue; }
    for c in col..n {
        a[row[r] * n + c] -= f * a[row[col] * n + c];
    }
    b[r] -= f * b[col];
}
```

The `if f == 0.0 continue` already skips zero multipliers. Add the symmetric
column skip — cache the pivot row, skip columns where its entry is zero:

```rust
for r in (col + 1)..n {
    let f = a[row[r] * n + col] / diag;
    if f == 0.0 { continue; }
    let pr = row[col] * n;
    let rr = row[r] * n;
    for c in col..n {
        let piv_entry = a[pr + c];
        if piv_entry != 0.0 {
            a[rr + c] -= f * piv_entry;
        }
    }
    b[r] -= f * b[col];
}
```

**Step 2: same skip in `lu_factor`** (`wdf.rs:1103-1112`) — its inner
`for c in (col+1)..n` loop, identical transform (skip where `a[piv[col]*n+c]==0`).

**Step 3: same skip in the two back-substitution loops** — `gauss_solve`
(`wdf.rs:1069-1075`) and `lu_solve` (`wdf.rs:1119-1135`): in `s -= a[..]*x[c]`,
skip when `a[..]==0`. Bit-identical (subtracting zero).

**Step 4: Tests stay green**

```bash
cd ~/Git/ba283-vst3 && cargo test -p ba283
```

Expected: ALL pass unchanged. If `OUTPUT_256` moves, a skip guarded the wrong
operand — revert and recheck (only multiply-by-zero / subtract-zero may be
skipped).

**Step 5: Bench delta**

```bash
RUSTFLAGS="-C target-cpu=native" cargo build --release --bin ba283_standalone
uv run python -m eval.wdf_rust_vs_cpp.bench --stim sweep_m3 --out eval/wdf_rust_vs_cpp/out
```

Expected: realtime-tier rt-factor drop (full dense solve benefits most).

**Step 6: Commit**

```bash
git add plugin/src/wdf.rs
git commit -m "perf(wdf): skip structural-zero multiply-adds in gauss/LU solve"
```

---

### Task 3: Indirection removal for autovectorization (gated by Task 2 bench)

**Only if Task 2's realtime gain is promising AND census showed exploitable
structure.** This is the riskiest task for bit-exactness — do it last, behind its
own commit so it's trivially revertable (like the prior `get_unchecked` round).

**Why:** objdump showed `a[row[r]*n+c]` indirect addressing blocks the LLVM
vectorizer (only 4 `*pd` ops in the whole `gauss_solve`). The elementwise
elimination update `a[r][c] -= f*a[col][c]` over `c` is independent across `c` (no
reduction) → vectorizable with **no reassociation** if the indirection is removed.

**Files:**
- Modify: `plugin/src/wdf.rs` — `gauss_solve` row addressing.

**Step 1: Physically permute instead of indexing through `row[]`**

Replace the `row[]` permutation-index scheme with an explicit row-pointer / copy
so the inner `c` loop addresses a contiguous slice. Two viable forms — pick the
one that the bench rewards and that keeps tests bit-green:

- **Slice form (preferred):** lift the pivot row and target row into `&[f64]` /
  `&mut [f64]` slices of length `n` via `split_at_mut`, then
  `for (t, &p) in tgt[col..].iter_mut().zip(piv_row[col..].iter()) { *t -= f * p; }`.
  Keep the column-zero skip from Task 2 (`if p != 0.0`). No `unsafe`.
- **Copy form:** maintain a physically-reordered scratch matrix; copy on pivot
  swap only.

Back-substitution stays scalar (it is a reduction; vectorizing it would
reassociate and break bit-exactness).

**Step 2: Tests stay green — this is the make-or-break gate**

```bash
cd ~/Git/ba283-vst3 && cargo test -p ba283
```

Expected: ALL pass **bit-unchanged**. The elementwise update must not reassociate.
If `OUTPUT_256`/`SETTLED_V` move at all, the restructure changed FP order — revert
this task entirely (keep Tasks 1–2).

**Step 3: Confirm vectorization actually happened**

```bash
objdump -d --demangle target/release/ba283_standalone | awk '/gauss_solve.*>:/{f=1} f{print} /ret/{if(f)f=0}' | grep -c 'pd'
```

Expected: more `*pd` (packed-double) ops than the baseline 4. If unchanged, the
restructure didn't help the vectorizer — revert (no benefit, added complexity).

**Step 4: Bench delta**

```bash
RUSTFLAGS="-C target-cpu=native" cargo build --release --bin ba283_standalone
uv run python -m eval.wdf_rust_vs_cpp.bench --stim sweep_m3 --out eval/wdf_rust_vs_cpp/out
```

**Step 5: Commit (or revert)**

```bash
git add plugin/src/wdf.rs
git commit -m "perf(wdf): remove row[] indirection in gauss_solve to enable SIMD"
```

If Steps 2–4 didn't clear the bar: `git checkout -- plugin/src/wdf.rs` and note
the negative result in the final summary.

---

### Task 4: Full parity gate + perf report + B-fallback decision

**Files:**
- Modify: `eval/wdf_rust_vs_cpp/out/*` (regenerated artifacts), final summary in
  the design doc or a short results note.

**Step 1: Full parity sweep (must be unchanged from baseline)**

```bash
cd ~/Git/ba283-vst3
RUSTFLAGS="-C target-cpu=native" cargo build --release --bin ba283_standalone
uv run python -m eval.wdf_rust_vs_cpp.compare --stims pink_m3_s0,sweep_m3 --gains 0.0,0.5,1.0 --tiers realtime,fast --out eval/wdf_rust_vs_cpp/out
```

Expected: realtime ESR ≤ −60 dB, fast ≤ −40 dB, FR ≤ 0.1 dB — **identical** to
baseline (proves no numeric drift was introduced).

**Step 2: Final bench + write up the deltas**

```bash
uv run python -m eval.wdf_rust_vs_cpp.bench --stim sweep_long --out eval/wdf_rust_vs_cpp/out
```

Record before/after rt-factor per tier per task in a results section appended to
`docs/plans/2026-06-07-wdf-structure-aware-solve-design.md` (or a sibling
`-results.md`).

**Step 3: B-fallback decision**

If realtime-tier wall-clock improvement after Tasks 1–3 is **< ~5%**, append the
Approach B (BJT-SIMD) addendum below as the next round; otherwise stop here and
land the PR.

**Step 4: Commit**

```bash
git add eval/wdf_rust_vs_cpp/out docs/plans/
git commit -m "perf(wdf): record structure-aware solve speedup results"
```

---

## Approach B addendum (only if triggered)

If Tasks 1–3 underdeliver, plan a follow-up round to pack the 3-BJT junction
exponentials (`compute_bjt`/`depletion_charge`, 26% self-time) into SIMD lanes:
- Evaluate the 6 junctions (3 BJTs × be/bc) with the existing `fast_exp`/`fast_pow`
  approximation, packed (bit-identical per lane, no new approximation).
- Caveats already noted in the design: 6 junctions underfill AVX2 (4×f64), param
  gather overhead, and Rust SIMD is not portable to the C++ reference (would need
  a parallel C++ rewrite to keep the parity gate meaningful).
- This is a separate brainstorming/plan cycle — do not start it inside this PR.

## Final verification checklist

- [ ] `cargo test -p ba283` fully green, golden/lossless/no-alloc unchanged.
- [ ] `compare` parity gates identical to baseline (no numeric drift).
- [ ] `bench` shows a recorded wall-clock improvement at ≥1 tier.
- [ ] No `unsafe` added; no audio-thread allocation added.
- [ ] Results written up; B-fallback decision recorded.
