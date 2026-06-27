# 303 Voice Fidelity Tiers — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Quantify the 303 voice's time-domain error vs SPICE with an ESR harness, then build a
3-tier quality axis (Ultra=WDF / High=WDF-lite / Low=TPT) whose top tier is ear-indistinguishable.

**Architecture:** A whole-voice SPICE *transient* reference (chained vco303→vcf303→VCA decks) is
rendered alongside the Rust voice; a Python harness computes ESR/NRMSE/Welch-coherence (BA283
methodology). Tiers swap behind a `VoiceEngine` trait, all ≥2× oversampled + ADAA, each gated to
an ESR budget. **F1 (measure the gap) is built first and gates every later scope decision.**

**Tech Stack:** Rust (nih-plug, `plugin303` crate), Python (numpy/scipy via `uv run`), ngspice-46
driven as a subprocess (PySpice stamps netlists only — it can't drive ngspice-46), `serde_json`.

Design: `docs/plans/2026-06-27-303-fidelity-tiers-design.md`. Reference metric source (history):
BA283 `eval/wdf_vs_spice/compare.py::metrics`, `eval/wdf_ultrafast/precision_gate.py`.

---

## Phase F1 — Whole-voice SPICE reference + ESR harness (quantify the gap)

Outcome: `eval/303/compare_voice.py` prints an ESR/NRMSE/coherence table for the current (Low/TPT)
voice vs the SPICE reference, across operating points; a baseline JSON is committed. **No tier work
starts until this number exists.**

### Task F1.1: ESR metrics module + tests

**Files:**
- Create: `eval/303/esr.py`
- Test: `eval/303/test_esr.py`

**Step 1: Write the failing test** — synthetic signals with a known ESR.

```python
# eval/303/test_esr.py
import numpy as np
from esr import metrics, esr_db

def test_identical_signals_have_minus_inf_esr():
    x = np.sin(np.linspace(0, 100, 4096)).astype(np.float64)
    m = metrics(x, x.copy())
    assert m["esr_db"] < -120.0          # numerically ~ -inf
    assert abs(m["nrmse"]) < 1e-9

def test_known_error_ratio():
    x = np.sin(np.linspace(0, 100, 4096))
    e = 0.1 * np.sin(np.linspace(0, 137, 4096))   # uncorrelated-ish error
    m = metrics(x, x + e)
    # esr = sum(e^2)/sum(x^2); both unit-ish sines => ~ (0.1^2)/1 = 0.01 => -20 dB
    assert -23.0 < m["esr_db"] < -17.0

def test_lag_is_removed():
    x = np.sin(np.linspace(0, 200, 8192))
    shifted = np.roll(x, 17)             # pure delay must not count as error
    m = metrics(x, shifted)
    assert m["lag"] == 17 or m["lag"] == -17
    assert m["esr_db"] < -40.0
```

**Step 2: Run, verify fail.** `cd eval/303 && uv run --no-project --with numpy python -m pytest test_esr.py -q` → FAIL (`No module named esr`).

**Step 3: Implement** (port BA283 `metrics`):

```python
# eval/303/esr.py
"""ESR / NRMSE metrics (BA283 methodology). Lag-aligned error-to-signal ratio."""
from __future__ import annotations
import numpy as np

def best_lag(ref: np.ndarray, mod: np.ndarray, max_lag: int = 256) -> int:
    n = min(len(ref), len(mod))
    r, m = ref[:n] - ref[:n].mean(), mod[:n] - mod[:n].mean()
    corr = np.correlate(r, m, mode="full")
    lags = np.arange(-(n - 1), n)
    w = np.abs(lags) <= max_lag
    return int(lags[w][np.argmax(corr[w])])

def _align(ref, mod):
    lag = best_lag(ref, mod)
    if lag > 0:   ref, mod = ref[lag:], mod[: len(ref) - lag]
    elif lag < 0: mod, ref = mod[-lag:], ref[: len(mod) + lag]
    n = min(len(ref), len(mod))
    return ref[:n], mod[:n]

def esr_db(esr: float) -> float:
    return float(10.0 * np.log10(esr + 1e-30))

def metrics(ref: np.ndarray, mod: np.ndarray) -> dict:
    ref = np.asarray(ref, float); mod = np.asarray(mod, float)
    lag = best_lag(ref, mod)
    r, m = _align(ref, mod)
    err = r - m
    esr = float(np.sum(err**2) / max(np.sum(r**2), 1e-20))
    nrmse = float(np.sqrt(np.mean(err**2)) / max(r.max() - r.min(), 1e-20))
    return {"lag": lag, "esr": esr, "esr_db": esr_db(esr), "nrmse": nrmse}

def pre_emphasis(x: np.ndarray, alpha: float = 0.8613) -> np.ndarray:
    """Perceptual HF weighting (matched-z of the paper's 0.85 @ 48 kHz)."""
    y = np.empty_like(x); y[0] = x[0]; y[1:] = x[1:] - alpha * x[:-1]
    return y
```

**Step 4: Run, verify pass.** Same pytest command → PASS (3 tests).

**Step 5: Commit.** `git add eval/303/esr.py eval/303/test_esr.py && git commit -m "feat(eval): ESR/NRMSE metrics for 303 voice fit (F1.1)"`

---

### Task F1.2: Rust offline voice render bin (raw f32 out)

A headless renderer so Python can drive the *exact* DSP the plugin runs. Emits raw `f32le` to a
file (no wav dep). Args: `--note --sr --secs --cutoff --reso --out`.

**Files:**
- Create: `plugin303/src/bin/render303.rs`
- (uses `plugin303::dsp::voice::Voice`, already `pub`)

**Step 1: Write the failing test** — a smoke test in the bin via `#[cfg(test)]` is awkward for a
bin; instead assert behaviour from Python in F1.4. Here the "test" is the build + a render that is
non-silent. Add a tiny Rust unit test in `voice.rs` first if missing (already have
`produces_signal_after_note_on`). So F1.2's gate is: `cargo build --bin render303` clean and the
output is non-empty/non-silent (checked in F1.4).

**Step 2: Implement** (minimal arg parse, no clap):

```rust
// plugin303/src/bin/render303.rs
use plugin303::dsp::voice::Voice;
use std::io::Write;

fn arg(flag: &str, default: f32) -> f32 {
    let a: Vec<String> = std::env::args().collect();
    a.iter().position(|s| s == flag).and_then(|i| a.get(i + 1))
        .and_then(|v| v.parse().ok()).unwrap_or(default)
}
fn arg_s(flag: &str, default: &str) -> String {
    let a: Vec<String> = std::env::args().collect();
    a.iter().position(|s| s == flag).and_then(|i| a.get(i + 1)).cloned()
        .unwrap_or_else(|| default.into())
}

fn main() {
    let sr = arg("--sr", 48_000.0);
    let note = arg("--note", 57.0) as u8;       // A2
    let secs = arg("--secs", 0.5);
    let cutoff = arg("--cutoff", 1_000.0);
    let reso = arg("--reso", 0.0);
    let out = arg_s("--out", "render.f32");

    let mut v = Voice::new(sr);
    // NOTE: cutoff/reso wiring into Voice lands in F2; until then this renders the
    // default-cutoff voice. Pass-through args are accepted now so the harness CLI is stable.
    let _ = (cutoff, reso);
    v.note_on(note);
    let n = (secs * sr) as usize;
    let mut buf = Vec::with_capacity(n);
    for _ in 0..n { buf.push(v.tick()); }

    let mut f = std::fs::File::create(&out).unwrap();
    for s in &buf { f.write_all(&s.to_le_bytes()).unwrap(); }
}
```

**Step 3: Build.** `cargo build --bin render303 -p plugin303` → clean.

**Step 4: Smoke render.** `cargo run --bin render303 -p plugin303 -- --out /tmp/r.f32 && test -s /tmp/r.f32` → file non-empty.

**Step 5: Commit.** `git add plugin303/src/bin/render303.rs && git commit -m "feat(303): headless voice render bin for ESR harness (F1.2)"`

---

### Task F1.3: Whole-voice SPICE transient reference (chained decks)

Render the SPICE reference for one operating point: vco303 `.tran` → PWL → vcf303 `.tran` → VCA.

**Files:**
- Create: `eval/303/voice_ref_spice.py`
- Test: `eval/303/test_voice_ref.py`

**Step 1: Write the failing test** — reference for a note is non-silent and has energy near the
expected pitch.

```python
# eval/303/test_voice_ref.py
import numpy as np
from voice_ref_spice import voice_reference

def test_reference_is_pitched_and_nonsilent():
    sr = 48_000.0
    y = voice_reference(note=57, cutoff_itail=32e-6, rreso=47e3, sr=sr, secs=0.2)
    assert y.size > 0 and np.max(np.abs(y)) > 1e-4
    # dominant spectral peak near A2 (~110 Hz) within a few %
    f = np.fft.rfftfreq(y.size, 1/sr); mag = np.abs(np.fft.rfft(y))
    peak = f[np.argmax(mag[1:]) + 1]
    assert 95.0 < peak < 125.0
```

**Step 2: Run, verify fail** (`No module named voice_ref_spice`).

**Step 3: Implement.** Reuse Phase-1 generators (`_vco303_netlist_gen`, `_vcf303_netlist_gen`) and
the subprocess-ngspice + `wrdata` pattern from `run_vco303_spice.py` / `run_vcf303_spice.py`.
Sketch:

```python
# eval/303/voice_ref_spice.py
"""Whole-voice SPICE transient reference: vco303 .tran -> PWL -> vcf303 .tran -> VCA."""
# 1) note -> VCO CV (1V/oct, Phase-1 mapping: CV 4/5/6 -> 231/477/1000 Hz).
# 2) ngspice .tran on vco303 with ramped-rail self-start (PWL 0->V over 1ms) +
#    Phase-1 solver opts (method=gear, rshunt=1e12, gmin=1e-10, reltol=1e-3,
#    abstol=1e-9, vntol=1e-6, itl4=1000, trtol=7); wrdata V(out).
# 3) resample VCO V(out) to uniform sr, emit as a PWL voltage source.
# 4) ngspice .tran on vcf303 driven by that PWL, with cutoff bias I_tail and reso
#    leg Rreso; wrdata V(out).
# 5) multiply by an AD VCA envelope (matches dsp/env defaults) -> reference array.
# Returns float64 array at `sr`.
```

> Detailed deck strings are filled in during execution against the existing Phase-1 harnesses —
> they already contain the exact netlist stamping + solver options to copy. Keep raw decks starting
> with `* comment` (ngspice eats the first line) and emit sources via `raw_spice` with plain
> numbers (PySpice unit suffixes parse as atto in ngspice).

**Step 4: Run, verify pass** (`uv run --no-project --with PySpice --with numpy --with pytest python -m pytest test_voice_ref.py -q`). May need solver-opt tuning until the VCO self-starts in `.tran` — iterate here; this is the riskiest task.

**Step 5: Commit.** `git add eval/303/voice_ref_spice.py eval/303/test_voice_ref.py && git commit -m "feat(eval): whole-voice SPICE transient reference, chained decks (F1.3)"`

---

### Task F1.4: Compare harness — ESR table + baseline JSON

**Files:**
- Create: `eval/303/compare_voice.py`
- Create (output, committed): `eval/303/voice_esr_baseline.json`

**Step 1–4 (single deliverable, validated by inspection):**

```python
# eval/303/compare_voice.py
"""Render the SPICE reference + the Rust voice per op-point; print ESR/NRMSE; dump JSON."""
# op-points: notes [45,57,69], (cutoff_itail, rreso) pairs, accent off/on.
# for each: ref = voice_reference(...); call `cargo run --bin render303 -- ...` ->
#   read raw f32; m = esr.metrics(ref, rust); collect.
# print table; json.dump({op: {esr_db, nrmse, lag}} ...) to voice_esr_baseline.json.
```

Run: `uv run --no-project --with PySpice --with numpy python eval/303/compare_voice.py`.
Expected: a printed ESR table (the **current Low/TPT gap** — likely poor, e.g. > −6 dB, since
cutoff/reso aren't wired yet and TPT ≠ diode ladder). This *number is the F1 deliverable*.

**Step 5: Commit.** `git add eval/303/compare_voice.py eval/303/voice_esr_baseline.json && git commit -m "feat(eval): whole-voice ESR compare + baseline (F1.4)"`

### Task F1.5: Record findings, decide F3 scope

Append a short findings block to the design doc: measured Low ESR, the aliasing contribution
(re-run compare with 2×/4× internal OS once F2 exists — or a quick offline OS sweep), and whether
Ultra will need 2nd-order ADAA vs 4× OS. **This closes F1 and informs F3.**

Commit: `docs(plan): F1 ESR findings + F3 ADAA/OS decision`.

---

## Phase F2 — Tier scaffold (EnumParam, 2× OS, ADAA, VoiceEngine trait, TPT as Low)

Solid-but-summary tasks (expand to F1-granularity at execution):

- **F2.1** `trait VoiceEngine { fn render(&mut self, note, params) -> f32 }` (or block form);
  `trait Filter303` per design. Refactor `Voice` to hold a boxed/enum engine. TDD: existing voice
  tests pass through the trait.
- **F2.2** `dsp/oversample.rs` — polyphase FIR 2×/4×/8× up/down. TDD: (a) up→down round-trips a
  band-limited signal within tolerance; (b) a tone above Nyquist/2 is attenuated. Pre-size buffers;
  report latency.
- **F2.3** `dsp/adaa.rs` — 1st-order antiderivative AA wrapper for a static nonlinearity
  (`tanh`). TDD: alias-bin energy for a hot sine drops vs naive `tanh` at 2×.
- **F2.4** Wire cutoff/reso into `Voice` + `render303` (the F1.2 pass-through args become real);
  calibrate Low/TPT `set_cutoff` mapping to the SPICE corner (use F1 harness to minimise ESR).
- **F2.5** `EnumParam<Tier>` in `Plugin303Params` (`#[id="tier"]`); block-rate read in `process()`;
  settle-then-crossfade on change. assert_no_alloc per tier path.
- **F2.6** ESR gate test (`tests/voice_esr.rs`): load committed gold; assert Low ≤ its (F1-derived)
  budget + added-error vs reference bound. Replaces the M2.4 loose shape test as the fidelity gate.

## Phase F3 — WDF Ultra

- **F3.1** `dsp/ladder_wdf.rs` — wave-digital 4-rung diode ladder (8×1N4148 + caps) + active Q9
  resonance; nonlinear diode pairs solved per sample (Newton + LUT). Port BA283 WDF discipline
  (structure-aware solve, zero-alloc, autovectorised transcendentals, denormal flush).
- **F3.2** ADAA/OS **decision point** (from F1.5): 2nd-order ADAA on the diode nonlinearity vs 4×
  OS for Ultra — implement the winner; keep the other behind a bench.
- **F3.3** Ultra ESR ≤ −30 dB gate; coherence ~1.0.

## Phase F4 — WDF-lite High

- Cheap transport-`exp` + reduced Newton iterations over the F3 solver (device-model precision
  axis). ESR ≈ −22 dB gate; added-error vs Ultra ≤ budget.

## Phase F5 — Tune + ship

- Hit all three ESR targets; settle-then-crossfade tier switching click-free; finalize the fixed
  2×/4×/8× OS switch param; update HANDOFF; finish-branch / PR.

---

## Notes for the executor

- All Rust hot paths stay **zero-alloc** — extend `tests/no_alloc.rs` per tier (the F1 render bin is
  offline, not gated).
- ngspice gotchas (from Phase-1 HANDOFF): PySpice can't drive ngspice-46 (stamp netlists, run
  ngspice as subprocess, parse `wrdata`); decks start with `* comment`; emit sources via `raw_spice`
  with plain numbers; `np.ptp(x)` not `ndarray.ptp()`.
- Commands: `cargo test -p plugin303`; `cargo run --bin render303 -p plugin303 -- …`;
  `uv run --no-project --with PySpice --with numpy [--with pytest] python …`.
