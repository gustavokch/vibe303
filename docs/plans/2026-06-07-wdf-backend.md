# BA283 WDF Backend — Rust Port & Plugin Integration Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.
>
> **On execution start:** copy this file to `docs/plans/2026-06-07-wdf-backend.md` in the
> `ba283-vst3` repo (plan mode forbade writing there during planning).

**Goal:** Port the C++ coupled-circuit BA283 model (`~/Git/ba283_wdf/main.cpp`) into a pure-Rust,
zero-alloc `f64` solver and wire it into the plugin as a second, user-selectable inference backend
alongside the existing neural net.

**Architecture:** Add `InferenceBackend { NeuralNet, Wdf }` enum param. `ChannelDsp` holds both a
`Ba283Model` (GRU, existing) and a new `WdfModel` (MNA + Newton solver) and dispatches per the param.
The WDF solver is a faithful translation of the C++ `BA283` class running the `realtime` quality tier
(OS=2, RB off → 16 unknowns, tol 1e-7, maxIters 30), self-oversampling + Butterworth-decimating
internally. A bounded DC warmup runs once in `Plugin::initialize` (off the audio thread); the settled
state is snapshotted and restored on `reset`. Live Bias / Circuit-Gain knob turns rebuild the constant
conductance matrix `G0` at block rate (the circuit re-settles dynamically, exactly as the C++ `setBias`
does), so no per-knob re-warm is needed.

**Tech Stack:** Rust 2021, nih-plug (`EnumParam`, `FloatParam`), `f64` dense linear algebra (no deps),
existing `ShortDelay`, `hound` (standalone), `assert_no_alloc` test pattern.

---

## Context

The plugin currently ships only the GRU neural emulation. Two independently-validated traditional
models now exist (`ba283_wdf` MNA solver and an ACME.jl model — see auto-memory `ba283-wdf-model-verdict`,
`ba283-acme-verdict`), both tracking SPICE at ESR ≈ −30 dB, coherence 1.0. The user wants the **WDF/MNA
C++ model** available *inside the plugin* as a selectable alternative to the neural net, so a listener
can A/B the circuit-faithful solver against the learned model in the same session.

The C++ source is **not** a WDF tree (the `chowdsp_wdf` lib in that repo is unused) — it is a direct
Modified Nodal Analysis + Newton-Raphson circuit solver in a single file, `~/Git/ba283_wdf/main.cpp`.
The port must reproduce it numerically, then bolt on plugin plumbing.

### Source-of-truth files (read these first)

| What | Path |
|---|---|
| C++ solver (the port source) | `~/Git/ba283_wdf/main.cpp` (lines 22–794 are the model; 798–966 is the offline driver) |
| Existing per-channel DSP dispatch point | `plugin/src/dsp.rs` (`ChannelDsp::process_sample`, lines 50–75) |
| Plugin params + process loop + initialize/reset | `plugin/src/lib.rs` (params 29–78, process 154–179, initialize 134–146, reset 148–152) |
| NN model (template for the new struct's shape & no-alloc contract) | `plugin/src/model.rs` |
| Offline renderer (needs a backend flag) | `plugin/src/bin/standalone.rs` |
| No-alloc test pattern | `plugin/tests/test_model_noalloc.rs`, `plugin/tests/common/mod.rs` |

### Key porting facts (from reading the C++)

- **All math is `f64`.** Keep `f64` throughout the solver; convert to `f32` only at the plugin boundary.
- **Realtime tier ⇒ RB off ⇒ 3 intrinsic-base nodes pinned.** `NUM_NODES = 21`; with RB off the unknown
  count `U = 16`. Size all fixed scratch arrays to `NUM_NODES` (21) and `NUM_NODES*NUM_NODES` (441) so the
  same code also supports RB-on (U=19) if ever needed. **No `Vec` — these become fixed `[f64; 21]` /
  `[f64; 441]` arrays** to satisfy the zero-alloc audio-thread contract (the C++ `std::vector` scratch is
  allocated once in `build()`; we make it stack/struct-resident instead).
- **`GND = -1` sentinel.** All node access goes through `node_v(id) = if id < 0 {0.0} else {v[id]}` and
  `ur(id) = if id < 0 {-1} else {urow[id]}`. Port these helpers verbatim.
- **Self-contained oversampling.** The C++ driver upsamples host→`host*OS` by linear interpolation, runs
  `OS` Newton solves per host sample, and decimates with two RBJ-biquad Butterworth lowpasses at
  `fc = 0.45*hostSr`. The WDF backend owns this internally — it does **not** use the existing
  `Oversample2x` (that exists for the 96 kHz-trained GRU). Output tap is `v[N_SEC_IN] - secInDc`.
- **Warmup.** From cold (zero state) the circuit needs a rail-ramp + idle-settle to reach its DC
  operating point and capture `secInDc` (the output DC offset to subtract). `idleSettle` early-exits when
  the tap stops moving (`|Δtap| < 1e-4`), so the real cost is ≈1 s of internal-rate solves, not 5 s.
- **Live param changes are cheap.** `setBias` / a new `setRgain` only recompute the RV1 split / R_KJ
  conductance and rebuild `G0`; integrator state is preserved and the circuit re-settles over a few ms.

### Architecture decisions (locked with the user)

1. **Perf target — realtime tier.** Port the `realtime` `QualityConfig`: `OS=2, useRB=false, tol=1e-7,
   maxIters=30, capTier=FULL, lsTrim=false`. Aim to stream in-DAW; accept it is CPU-heavy.
2. **Warmup — snapshot + restore.** Bounded warmup once in `initialize` (off audio thread); snapshot the
   settled node/integrator state + `secInDc`; restore on `reset`. Live Bias/Gain changes re-settle
   dynamically via `G0` rebuild (no re-warm). Background re-warm for instant settle is **out of scope v1**.
3. **Gain — separate R_KJ param.** Keep `Drive (dB)` as an input-level pre-multiply for **both** backends
   (parity). Add a new WDF-only `Circuit Gain` param mapping to R_KJ (47–10 kΩ, log, default 220 Ω). It is
   inert for the NN backend.

---

## Phase A — Port the solver core (`plugin/src/wdf.rs`)

The heart of the work. A pure module with **no nih-plug dependency**, fully covered by golden-vector
tests captured from the C++ binary, so correctness is locked before any plugin wiring.

### Task A0: Capture golden vectors from the C++ reference

**Files:**
- Create: `plugin/tests/fixtures/wdf_golden.rs` (Rust-literal fixtures, `include!`-ed by tests)

**Step 1:** Build the C++ reference and instrument it (temporary local edit, do NOT commit to the C++ repo)
to print, for the **realtime tier**, fixed `Rgain=220`, **auto-bias to the 65 mA TR3 idle target**, and a
**48 kHz** input WAV (so `input.wav` must be 48 kHz; internal rate = 48 kHz × OS(2) = **96 kHz**):

```bash
cd ~/Git/ba283_wdf && make   # default target ONLY — NOT `make fast` (-ffast-math breaks fast_exp/fast_log
                             # IEEE bit manipulation and the f64 determinism the 1e-6 fixtures rely on)
# run with realtime tier; add a temporary printf of (a) the auto-calibrated bias, (b) secInDc and the
# settled v[] vector, (c) v[N_SEC_IN] for the first ~256 samples of a known 48 kHz input.wav
# (e.g. a 1 kHz sine at -6 dBFS). `auto` triggers the bias bisection to --idle mA.
BA283_QUALITY=realtime ./ba283_sim 220 auto --idle 65 --no-rb
```

**Step 2:** Record into `wdf_golden.rs` as `pub const` literals (full `f64` precision):
- `BIAS: f64` — the **auto-calibrated bias** the C++ prints (`Auto-bias: X -> TR3 idle 65 mA`). The Rust
  port does NOT run auto-bias (the Bias knob sets RV1 directly), so all downstream tests feed **this value**
  to `WdfModel::new(48_000.0, 220.0, BIAS)` to reproduce the same operating point.
- `HOST_SR: f64 = 48_000.0` — the capture host rate (internal = `HOST_SR * 2` = 96 kHz).
- `SETTLED_V: [f64; 21]` — node voltages after warmup (realtime tier, 48 kHz host / 96 kHz internal,
  auto-bias→65 mA, Rgain 220).
- `SEC_IN_DC: f64` — the settled output tap.
- `INPUT_256: [f64; 256]` and `OUTPUT_256: [f64; 256]` — host-rate (48 kHz) input samples and the decimated
  `output.wav` samples the C++ produces for them (post-warmup).
- `OP_POINT: { tr1_c, tr2_e, tr3_e, f, tr1_b, rv1_w }` — the printed DC operating point (sanity anchor).

> **Capture-schedule correction (found during execution):** `auto` bias runs a ~16-step bisection,
> each with its own `idleSettle`, which drifts slow nodes (e.g. the C1 input-coupling cap) much
> closer to DC equilibrium than a single `idleSettle(4.0)`. Feeding the auto-calibrated bias to
> `WdfModel::warmup` (ramp + one `idleSettle(4.0)`) then lands at a *different, less-settled* point →
> the null misses by ~1e-3 on slow nodes. **Fix:** use `auto` only to DISCOVER the bias value, then
> **re-capture the golden with that value passed as a DIRECT bias** (`./ba283_sim 220 <BIAS> --no-rb`,
> no auto) so the C++ schedule is exactly `ramp + idleSettle(4.0)` — identical to `WdfModel::warmup`.
> After re-capture all 21 nodes null at 1e-6.

**Step 3:** Commit fixtures.

```bash
git add plugin/tests/fixtures/wdf_golden.rs
git commit -m "test(wdf): capture C++ realtime-tier golden vectors"
```

> These fixtures are the spec. Every later task asserts against them. Tolerance: `1e-6` absolute on node
> voltages / DC, and `1e-5` (or ESR < −60 dB) on the 256-sample output stream — the fast-vs-exact math and
> f64 determinism make near-bit-exact agreement realistic.

### Task A1: Node enum, device params, fixed-size data structures

**Files:**
- Create: `plugin/src/wdf.rs`
- Modify: `plugin/src/lib.rs` (add `pub mod wdf;` next to the other `pub mod` lines, ~line 8)

**Step 1 (test):** `plugin/tests/test_wdf.rs` — assert the constants compile and match the C++ topology:

```rust
use ba283::wdf::{NUM_NODES, node};
#[test]
fn node_count_and_ids() {
    assert_eq!(NUM_NODES, 21);
    assert_eq!(node::N_A, 0);
    assert_eq!(node::N_INPUT, 1);
    assert_eq!(node::N_SEC_IN, 19); // listening tap (NOT last node — N_ZOBEL follows)
    assert_eq!(node::N_ZOBEL, 20);  // last node; primary Zobel series-RC mid-node
}
```

**Step 2:** Run → fails (module/consts missing).

**Step 3 (impl):** Port verbatim from `main.cpp:44-97`:
- `pub const NUM_NODES: usize = 21;` and a `pub mod node { pub const N_A: i32 = 0; ... }` mirroring the
  `NodeId` enum **in the exact same order** (the order defines `urow` mapping). `GND` = `-1`.
- `struct BjtParams { is, vt, alpha_f, alpha_r, vcrit, cje, vje, mje, cjc, vjc, mjc, tf, tr, bf, br, vaf,
  var, ikf, ikr: f64 }` and the two `const` device cards `BC184C` / `Q2N3055` (`main.cpp:296-301`).
  `vcrit` is `0.02585 * (0.02585/(2f64.sqrt()*is)).ln()` — compute in a small `fn bc184c() -> BjtParams`.
- `struct Bjt { b, e, c: i32, p: BjtParams, qbe_prev, ibe_prev, qbc_prev, ibc_prev: f64 }`
- `struct Res { a, b: i32, g: f64 }`, `struct Cap { a, b: i32, c, geq, i_prev: f64 }`
- `struct Biquad { ... }` with `set_lowpass` / `process` (`main.cpp:22-34`).

**Step 4:** Run → passes. **Step 5:** Commit `feat(wdf): node map, device cards, core structs`.

### Task A2: Fast transcendentals + diode/depletion/BJT kernels

**Files:** `plugin/src/wdf.rs`, `plugin/tests/test_wdf.rs`

**Step 1 (test):** Port the C++ values directly. `fast_exp`/`fast_log` claim `|rel err| < 1e-11`:

```rust
#[test]
fn fast_math_matches_std() {
    for &x in &[-30.0, -1.0, 0.0, 0.5, 5.0, 25.0] {
        assert!((wdf::fast_exp(x) - x.exp()).abs() <= 1e-9 * x.exp().max(1.0));
    }
    for &x in &[0.05f64, 0.5, 1.0, 1.4, 10.0] {
        assert!((wdf::fast_log(x) - x.ln()).abs() <= 1e-9);
    }
}
#[test]
fn compute_bjt_against_golden() {
    // one (vbe, vbc) point through compute_bjt; compare Ic/Ib/Ie to a value
    // printed from the C++ computeBjt for the same inputs (add to wdf_golden.rs).
}
```

**Step 2:** Run → fails.

**Step 3 (impl):** Translate `main.cpp:109-241`:
- `fast_exp` (note the `k <= -1023 → 0.0` underflow guard and IEEE-754 exponent assembly via
  `f64::from_bits`), `fast_log` (`f64::to_bits`/`from_bits` decomposition), `fast_pow_pos`.
  **Realtime tier uses fast math** (`gExactMath=false`); port that path. Skip the std-switch macros —
  hardcode fast (or keep a `const EXACT: bool = false` for the validation A/B).
- `depletion_charge(v, cj0, vj, m) -> (q, c)` with the `FC=0.5` linear extrapolation (`main.cpp:154-170`).
- `diode_branch(v, is, vt, vcrit) -> (i, g)` with `vcrit` linearization (C++ `diodeEval`, `main.cpp:174-187`).
- `compute_bjt(vbe, vbc, p) -> BjtOut` — the full Gummel-Poon transport + Early + high-injection +
  node-form Jacobian (`main.cpp:197-241`). `struct BjtOut { ib, ic, ie, itf, gtf, itr, gtr, dib_b, dib_e,
  dib_c, dic_b, dic_e, dic_c, die_b, die_e, die_c }`.

**Step 4:** Run → passes. **Step 5:** Commit `feat(wdf): fast math + diode/depletion/Gummel-Poon kernels`.

### Task A3: `WdfModel` struct, `build`, `build_g0`, `set_bias`, `set_rgain`

**Files:** `plugin/src/wdf.rs`, `plugin/tests/test_wdf.rs`

**Step 1 (test):** After `build(48000.0*2.0 /*internalSr=96k*/, 220.0, wdf_golden::BIAS)` and zero state, assert `U == 16`
(RB off), `urow` pins `N_A`/`N_INPUT`/the 3 `*_BI` nodes to `-1`, and the resistor/cap/bjt counts match
the C++ `build()` (count the `R(...)`/`C(...)`/`bjts.push_back` calls).

**Step 3 (impl):** Translate `main.cpp:255-438`:
- `pub struct WdfModel` with fixed arrays: `v: [f64; 21]`, `v_prev: [f64; 21]`, `known: [bool; 21]`,
  `urow: [i32; 21]`, `u: usize`, fixed `res: [Res; 24], n_res: usize` (the C++ has ≤ ~20 resistors),
  `caps: [Cap; 8], n_caps: usize`, `bjts: [Bjt; 3]`, scratch `hist/f/delta/rn: [f64; 21]`,
  `j/g0: [f64; 441]`, `gl: [[f64;2];2]`, `i_hist_p/s, v_pprev/sprev, dt, newton_tol, max_iters, cap_tier`,
  plus optional diagnostics. **Avoid `Vec`** — fixed arrays sized to the worst case. (Builder/test setup
  code may use `Vec`, but the struct and `process` path must be array-only.)
- `rv1_split(bias) -> (r_upper, r_lower)` (`main.cpp:307-312`).
- `build(internal_sr, rgain, bias)` hardcoding the realtime tier (RB off): pin known nodes, build `urow`,
  push the resistor/cap/bjt lists (`main.cpp:350-407`), compute the coupled-inductor companion `gL`,
  call `build_g0`. Store the R_KJ resistor index (`r_kj_idx` — capture the push index of the
  `R(N_TR1_E, N_J, Rgain)` stamp at `main.cpp:356`; the C++ has NO such index, only `rUpperIdx`/`rLowerIdx`
  for RV1 at `main.cpp:303`) and the RV1 indices for cheap setters.
- `build_g0()` (`main.cpp:413-428`) — stamp resistor + cap `Geq` + asymmetric inductor companion into
  `g0`.
- `set_bias(bias)` (`main.cpp:432-438`) and **new** `set_rgain(rgain)` (mirror `set_bias`: update
  `res[r_kj_idx].g = 1.0/rgain; build_g0();`). Both preserve integrator state.

**Step 4:** Run → passes. **Step 5:** Commit `feat(wdf): WdfModel build + G0 + bias/rgain setters`.

### Task A4: `gauss_solve`, `residual_norm`, `newton_iterate`, `process`

**Files:** `plugin/src/wdf.rs`, `plugin/tests/test_wdf.rs`

**Step 1 (test):** Drive `build` then call `process()` repeatedly with `v[N_A]=24, v[N_INPUT]=0` for the
warmup ramp and assert the settled `v[]` converges toward `SETTLED_V` (within `1e-4` after enough
samples — exact match comes in A5 with the real warmup schedule). Also a direct `gauss_solve` unit test
on a small known 3×3 system.

**Step 3 (impl):** Translate, preserving every stability trick (the fixtures will punish any omission):
- `gauss_solve(a: &mut [f64;441], b: &mut [f64;21], n)` — dense partial-pivot elimination via
  **index-swap pivoting** (`main.cpp:764-794`, commit `42b51ef`). Port exactly: a `row: [usize; 21]`
  permutation, `A` accessed through `A[row[r]*n + c]`, **but `b` swapped physically** (`b.swap(col, piv)`)
  with forward-elim / back-sub reading physical `b` indices. `best = |A[row[col]*n+col]|`, scan rows
  `col+1..n`, `best < 1e-30 → return false`. Back-sub: `b[r] = (b[r] − Σ A[row[r]*n+c]·b[c]) /
  A[row[r]*n+r]`. The A-indirect / b-physical asymmetry is bit-identical to row-swap — match it verbatim
  or the "near bit-exact" golden tolerances will not hold.
- `residual_norm(hist, gmin) -> f64` (`main.cpp:717-761`) — must stamp the **same** branches as
  `newton_iterate`'s F (resistors, caps, inductor, BJT transport + charge caps, gmin) for line-search
  consistency.
- `newton_iterate(hist, gmin) -> iters` (`main.cpp:520-714`), realtime path only. `lsTrim=false` ⇒
  `useTrim=false`, so **skip all three trim regions** (commits `d4df60d`/`28b2a9a`/`d797be0`):
  retroactive-reject `main.cpp:634-651`, optimistic-step `main.cpp:665-679`, post-loop maxIters verify
  `main.cpp:702-712`. **Keep** (eager path): copy `g0→j` + `hist→f` each iter, linear/cap/inductor F stamps,
  BJT transport stamps into F & J, charge-cap (depletion+diffusion) stamps, gmin homotopy, the per-iteration
  2-norm convergence vs `newton_tol` (`main.cpp:626-629`), the **2 V step-magnitude bound** (`656-663`),
  the 12-step backtracking line search on `residual_norm` (`681-700`), and the `maxd < 1e-9` early break
  (`698-700`).
- `process()` (`main.cpp:452-516`): build per-sample `hist` (cap + inductor history), `newton_iterate`,
  the **gmin-stepping homotopy fallback** when `finalResid > 1e-3` (`gmins = [1e-1,1e-2,1e-3,1e-4,1e-5,0]`),
  then update cap `i_prev`, BJT charge state, inductor winding currents, and `v_prev = v`.

**Step 4:** Run → passes. **Step 5:** Commit `feat(wdf): Newton solver + per-sample process`.

### Task A5: Warmup, snapshot, oversampling driver, and the golden null test

**Files:** `plugin/src/wdf.rs`, `plugin/tests/test_wdf.rs`

**Step 1 (test):** The decisive test — replicate the C++ driver and null against fixtures:

```rust
#[test]
fn warmup_reaches_golden_operating_point() {
    let mut m = WdfModel::new(48_000.0, 220.0, wdf_golden::BIAS); // host SR; internal = *OS = 96k
    m.warmup();
    for (i, &exp) in wdf_golden::SETTLED_V.iter().enumerate() {
        assert!((m.node_v(i) - exp).abs() <= 1e-6, "node {i}");
    }
    assert!((m.sec_in_dc() - wdf_golden::SEC_IN_DC).abs() <= 1e-6);
}
#[test]
fn decimated_output_matches_cpp() {
    let mut m = WdfModel::new(48_000.0, 220.0, wdf_golden::BIAS);
    m.warmup();
    for (i, &x) in wdf_golden::INPUT_256.iter().enumerate() {
        let y = m.process_host_sample(x as f32);
        assert!((y as f64 - wdf_golden::OUTPUT_256[i]).abs() <= 1e-5, "sample {i}");
    }
}
```

**Step 3 (impl):**
- `WdfModel::new(host_sr, rgain, bias)` → `build(host_sr * OS, rgain, bias)` where `const OS: usize = 2`.
  Store the two decimation `Biquad`s set to `fc = 0.45*host_sr` at `internal_sr` with the C++ Q values
  `0.54119610` / `1.30656296` (`main.cpp:933-936`).
- `warmup(&mut self)` — port `main.cpp:865-916` **minus auto-bias** (bias is a direct param): rail-ramp
  (`rampSamples = internal_sr*1.0`), `idle_settle(4.0)` with the `|Δtap|<1e-4` early-exit, then capture
  `self.sec_in_dc = v[N_SEC_IN]`. (Auto-bias bisection is **not** ported — the plugin's Bias knob sets the
  operating point directly.)
- `process_host_sample(&mut self, x: f32) -> f32` — port `main.cpp:942-953` for one host sample: linear
  upsample to `OS` subsamples, `process()` each, `dec2.process(dec1.process(v[N_SEC_IN] - sec_in_dc))`,
  return the last decimated value as `f32`.
- **Snapshot API:** `struct WdfSnapshot { v: [f64;21], v_prev: [f64;21], caps_i_prev: [f64;8],
  bjt_state: [(f64,f64,f64,f64);3], i_hist_p, i_hist_s, v_pprev, v_sprev, sec_in_dc: f64 }` and
  `fn snapshot(&self) -> WdfSnapshot` / `fn restore(&mut self, s: &WdfSnapshot)`. Decimation biquad state
  is **not** snapshotted (cleared on restore — transient is sub-millisecond).
- `reset(&mut self)` → restore from the last snapshot if present, else zero + clear biquads.

**Step 4:** Run → both pass (this is the port's acceptance gate). **Step 5:** Commit
`feat(wdf): warmup, snapshot/restore, oversampling driver — nulls vs C++`.

### Task A6: Zero-allocation guard on the audio path

**Files:** Create `plugin/tests/test_wdf_noalloc.rs` (mirror `test_model_noalloc.rs` + `tests/common/mod.rs`)

**Step 1 (test):** Warm up *outside* the guard, then assert `process_host_sample` allocates nothing:

```rust
#[path = "common/mod.rs"] mod common;
#[test]
fn process_is_zero_alloc() {
    let mut m = ba283::wdf::WdfModel::new(48_000.0, 220.0, 0.5);
    m.warmup(); // allocation here is fine — off the audio thread (bias value irrelevant to this test)
    assert_no_alloc(|| { let _ = m.process_host_sample(0.1); });
}
```

**Step 2:** Run → must pass; if it fails, a `Vec`/`Box` leaked into `process` — convert to fixed arrays.
**Step 3:** Commit `test(wdf): enforce zero-alloc process path`.

---

## Phase B — Plugin parameters

### Task B1: `InferenceBackend` enum param + `Circuit Gain` param

**Files:** `plugin/src/lib.rs`, `plugin/tests/test_params.rs`

**Step 1 (test):** Extend `test_params.rs`: default backend is `NeuralNet` (preserves current behaviour),
`circuit_gain` default `220.0`, range `47.0..=10000.0`.

**Step 3 (impl):** In `lib.rs`:
```rust
#[derive(Enum, PartialEq, Eq, Clone, Copy)]
pub enum InferenceBackend {
    #[name = "Neural Net"] NeuralNet,
    #[name = "Circuit (WDF)"] Wdf,
}
```
Add to `Ba283Params`:
```rust
#[id = "backend"]      pub backend: EnumParam<InferenceBackend>,
#[id = "circuit_gain"] pub circuit_gain: FloatParam, // R_KJ ohms, WDF-only
```
`Default`: `backend: EnumParam::new("Backend", InferenceBackend::NeuralNet)`; `circuit_gain:
FloatParam::new("Circuit Gain", 220.0, FloatRange::Skewed { min: 47.0, max: 10_000.0, factor:
FloatRange::skew_factor(-2.0) }).with_unit(" Ohm")` (skew so low-ohm/high-gain end has resolution; **no
smoother** — it is consumed at block rate). Keep `Drive` semantics unchanged.

**Step 4/5:** Run → pass. Commit `feat(params): backend selector + circuit-gain param`.

---

## Phase C — `ChannelDsp` dispatch

### Task C1: Hold both backends, dispatch, honour latency

**Files:** `plugin/src/dsp.rs`, `plugin/tests/test_process.rs`

**Step 1 (test):** In `test_process.rs`: a `ChannelDsp` with `backend = Wdf` (after a `warmup`/`restore`)
produces finite output and, sub-96 kHz, its blended output is delayed by `OS_LATENCY_SAMPLES` so the
host-reported latency stays valid for **both** backends (assert an impulse appears 32 samples late).

**Step 3 (impl):** Extend `ChannelDsp`:
```rust
pub struct ChannelDsp {
    model: Ba283Model,
    os: Oversample2x,
    dry_delay: ShortDelay<OS_LATENCY_SAMPLES>,
    wdf: WdfModel,
    wdf_out_delay: ShortDelay<OS_LATENCY_SAMPLES>, // honours the reported latency on the WDF path
}
```
- `new()` builds `wdf: WdfModel::new(48_000.0, 220.0, 0.5)` (rebuilt with the real SR in `initialize`).
- `reset()` also calls `self.wdf.reset()` (restore snapshot) and `self.wdf_out_delay.reset()`.
- `process_sample` gains a `backend: InferenceBackend` arg; Bias/Circuit-Gain are applied at block rate
  (see Phase D) so they are **not** per-sample args. New dispatch:
```rust
let driven = x * drive;
let blended = match backend {
    InferenceBackend::NeuralNet => { /* existing path unchanged */ }
    InferenceBackend::Wdf => {
        let wet = self.wdf.process_host_sample(driven) * out_gain;
        wet * mix + x * (1.0 - mix) // WDF decimation is IIR ⇒ ~0 integer latency
    }
};
// Keep host-reported latency honest for BOTH backends:
match backend {
    InferenceBackend::NeuralNet => blended,            // already latency-aligned internally
    InferenceBackend::Wdf if host_ge_96k => blended,   // report 0, no delay
    InferenceBackend::Wdf => self.wdf_out_delay.push_pop(blended), // delay 32 to match the report
}
```
Add WDF block-rate config setters used by Phase D: `fn wdf_set_bias(&mut self, b)`,
`fn wdf_set_rgain(&mut self, r)`, `fn wdf_rebuild(&mut self, sr, rgain, bias)`,
`fn wdf_restore(&mut self, snap)`.

**Step 4/5:** Run → pass. Commit `feat(dsp): dual-backend dispatch with latency-honest WDF path`.

> **Latency note:** Both backends keep the existing `latency_samples()` report (32 sub-96 kHz, 0 at
> ≥96 kHz). The WDF wet path has no integer latency, so we delay its blended output by 32 sub-96 kHz to
> match. This means **no runtime latency renegotiation** when the user switches backends — a deliberate
> simplification.

---

## Phase D — Plugin wiring (`lib.rs`)

### Task D1: Block-rate WDF param handling + per-sample dispatch

**Files:** `plugin/src/lib.rs`

**Step 3 (impl):** In `process()`:
- Read `let backend = self.params.backend.value();` once per block.
- When `backend == Wdf`, at block start read `circuit_gain.value()` and `bias.value()` (unsmoothed); if
  either changed vs a cached `last_rgain`/`last_bias` on `self`, call `wdf_set_rgain` / `wdf_set_bias` on
  **both** channels (rebuild `G0`, cheap; integrator state preserved → dynamic re-settle).
- Per sample: still smooth `drive`, `out_gain`, `mix`; pass `backend` into `process_sample`. (Bias is
  per-sample-smoothed only for the NN path; the WDF path uses the block-rate bias above.)

Add `last_rgain: f32, last_bias: f32` fields to `Ba283Plugin` (init `NAN` to force first-block apply).

### Task D2: Warmup in `initialize`, snapshot, restore in `reset`

**Files:** `plugin/src/lib.rs`

**Step 3 (impl):**
- `initialize()`: after `self.sample_rate = buffer_config.sample_rate;`, build+warm **one** `WdfModel`
  at the real SR / current `circuit_gain` / `bias` (single shared warmup — both channels are identical),
  take its `snapshot()`, and install it into both channels via `wdf_rebuild` + `wdf_restore`. Cache
  `last_rgain`/`last_bias`. Keep the existing `set_latency_samples` call. Warmup is bounded (~1 s of
  internal-rate solves with early-exit) and runs off the audio thread — acceptable for plugin load.
- `reset()`: `ch.reset()` already cascades to `wdf.reset()` (restore snapshot). No re-warm.

**Step 4 (verify):** `cargo build --release` and `cargo test --all` green. **Step 5:** Commit
`feat(plugin): warm WDF in initialize, snapshot/restore, block-rate bias/gain`.

> **Deferred (v1 out of scope):** background re-warm for *instant* settle after large Bias/Gain jumps.
> The dynamic re-settle (a few ms of audio) is acceptable for v1; note it in the PR description.

---

## Phase E — Offline standalone parity

### Task E1: `--backend wdf|nn` flag

**Files:** `plugin/src/bin/standalone.rs`, `plugin/tests/` (optional integration test)

**Step 3 (impl):** Add a `--backend` arg (default `nn`). When `wdf`, instantiate `WdfModel` per channel,
`warmup()`, then run `process_host_sample` through the same Drive→(model)→Output→Mix chain (reuse
`ChannelDsp` with `backend = Wdf`, plus `--circuit-gain <ohms>`). This keeps the standalone a faithful
mirror of the plugin and gives a CLI to regenerate null-test material against SPICE / the C++ binary.

**Step 4 (verify):**
```bash
cargo build --release --bin ba283_standalone
./target/release/ba283_standalone --backend wdf --input in.wav --output out_wdf.wav \
    --bias 0.5 --circuit-gain 220 --drive-db 0 --output-db 0 --mix 100
```
**Step 5:** Commit `feat(standalone): --backend wdf with circuit-gain`.

---

## Phase F — End-to-end verification

### Task F1: Null test — Rust WDF vs C++ reference

**Files:** none (a verification script / manual run)

1. Render the same 48 kHz input through the C++ binary (`BA283_QUALITY=realtime ./ba283_sim 220 auto
   --idle 65 --no-rb`, producing `output.wav`) and through `ba283_standalone --backend wdf` at the C++
   auto-calibrated bias (`--bias <wdf_golden::BIAS>`, full chain, Drive/Output 0 dB, Mix 100,
   circuit-gain 220).
2. Compute ESR / NRMSE between the two WAVs (reuse the metrics in `~/Git/ba283_wdf/validate/`). **Accept**
   ESR < −60 dB (f64 determinism should make them nearly bit-identical post-warmup; the only divergences
   are warmup-length and biquad-state edge effects, which the onset guard in the validate harness skips).

### Task F2: Plugin smoke test in a host

1. `cargo xtask build-release --bundle ba283`; load the VST3/CLAP in a DAW.
2. Toggle **Backend** between *Neural Net* and *Circuit (WDF)*; confirm audio continues, no clicks beyond
   a brief settle, CPU rises on WDF (expected).
3. Sweep **Bias** and **Circuit Gain** on the WDF backend; confirm the tone changes and re-settles within
   a few ms, no NaNs/blowups (the Newton step-bound + gmin homotopy guard this).
4. Confirm **Drive/Output/Mix** behave identically across both backends (parity for Drive as input level).

### Task F3: Full suite + finish

```bash
cargo test --all          # all unit + golden + no-alloc tests green
cargo clippy --all-targets
```
Then use **superpowers:finishing-a-development-branch** to open the PR. PR body must call out: (a) WDF is
the `realtime` tier and CPU-heavy; (b) latency is unchanged across backends by design; (c) background
re-warm is deferred.

---

## Risks & mitigations

| Risk | Mitigation |
|---|---|
| WDF not realtime on weak CPUs | `realtime` tier (RB off, U=16, maxIters 30, tol 1e-7) is the C++ ~1.03× tier; document the cost; backend is opt-in and defaults to NN. |
| Hidden `Vec` alloc on audio path | Task A6 `assert_no_alloc` gate; all scratch is fixed `[f64; 21/441]`. |
| Warmup blocks plugin load too long | `idle_settle` early-exits (~1 s); single shared warmup for both channels. If still slow, move to a nih-plug `BackgroundTask` with dry-passthrough-until-ready (noted, not v1). |
| Numeric drift vs C++ | Golden-vector fixtures (Task A0) lock node voltages, DC, and a 256-sample stream at `1e-5`–`1e-6`. |
| Latency confusion on backend switch | Single static latency report for both backends; WDF blended output delayed 32 sub-96 kHz to match. |
| Param smoothing too coarse for WDF | Bias/Circuit-Gain applied at block rate via `G0` rebuild (matches C++ `setBias`); circuit re-settles dynamically — same behaviour as turning a real knob. |

## Definition of done

- `plugin/src/wdf.rs` ports `main.cpp:22-794` (realtime tier), zero-alloc `process`, nulls vs C++ golden
  vectors at ESR < −60 dB.
- Backend selectable in-plugin; Drive/Output/Mix parity; Circuit-Gain + Bias drive R_KJ / RV1 live.
- Warmup in `initialize`, snapshot/restore on `reset`; no audio-thread blocking.
- `cargo test --all` + `cargo clippy` green; DAW smoke test passes; PR opened.
