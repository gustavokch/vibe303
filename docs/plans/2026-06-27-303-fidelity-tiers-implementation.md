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

## Phase F2 — Tier scaffold (Filter303 trait, 2× OS, ADAA, Tier param, TPT as Low)

Outcome: a `Tier` enum + `EnumParam`, a tested oversampler and ADAA wrapper (the F3/F4
building blocks), cutoff/reso wired end-to-end (`render303` args become real), and a Rust-side
ESR gate replacing the loose M2.4 shape test. Low stays TPT; High/Ultra are placeholders routed
to TPT until F3/F4. **OS/ADAA are landed + unit-tested as modules in F2; they are not folded into
the voice render path until tiers diverge (F3), so the F2 ESR gate measures the plain TPT Low.**

Design notes baked in from F1.5: Low's gate is the *post-F2 measured* number (cutoff wiring should
beat the +2.3 dB baseline), not −12 dB asserted up front. The voice uses **enum dispatch over
preallocated filters** (no `Box`, no per-sample vtable, no audio-thread alloc on tier switch); the
`Filter303` trait exists so F3/F4 backends slot in.

---

### Task F2.1: `Filter303` trait + `Tier` enum; `TptLadder` impls the trait

**Files:**
- Create: `plugin303/src/dsp/filter.rs`
- Modify: `plugin303/src/dsp/mod.rs` (add `pub mod filter;`)
- Modify: `plugin303/src/dsp/ladder.rs` (impl trait for `TptLadder`)

**Step 1: Write the failing test** — drive a `TptLadder` through `&mut dyn Filter303`.

```rust
// plugin303/src/dsp/filter.rs  (#[cfg(test)] mod tests)
#[test]
fn tpt_ladder_runs_through_trait_object() {
    use crate::dsp::ladder::TptLadder;
    let mut f = TptLadder::new(48_000.0);
    let dynf: &mut dyn Filter303 = &mut f;
    dynf.set_cutoff(1_000.0);
    dynf.set_reso(0.5);
    let mut acc = 0.0f32;
    for i in 0..2_000 { acc += dynf.tick(((i as f32) * 0.01).sin()); }
    assert!(acc.is_finite());
}
```

**Step 2: Run, verify fail.** `cargo test -p plugin303 filter::` → FAIL (no trait).

**Step 3: Implement.**

```rust
// plugin303/src/dsp/filter.rs
//! Tier-swappable filter interface. TPT (Low) impls it now; WDF (High/Ultra) in F3/F4.

/// A 303 filter core: cutoff in Hz, resonance in [0,1], per-sample `tick`.
pub trait Filter303 {
    fn set_cutoff(&mut self, hz: f32);
    fn set_reso(&mut self, r: f32);
    fn tick(&mut self, x: f32) -> f32;
    fn reset(&mut self);
}

/// Fidelity tier. Low = TPT; High = WDF-lite (F4); Ultra = WDF (F3).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Tier {
    #[default]
    Low,
    High,
    Ultra,
}
```

Add to `ladder.rs` (keep the inherent methods; the trait just forwards):

```rust
impl crate::dsp::filter::Filter303 for TptLadder {
    fn set_cutoff(&mut self, hz: f32) { TptLadder::set_cutoff(self, hz); }
    fn set_reso(&mut self, r: f32) { TptLadder::set_reso(self, r); }
    fn tick(&mut self, x: f32) -> f32 { TptLadder::tick(self, x) }
    fn reset(&mut self) { self.z = [0.0; 4]; }
}
```

**Step 4: Run, verify pass.** `cargo test -p plugin303` → all green (existing ladder/voice tests + new).

**Step 5: Commit.** `git commit -m "feat(303): Filter303 trait + Tier enum; TptLadder impls it (F2.1)"`

---

### Task F2.2: `dsp/oversample.rs` — polyphase FIR 2× up/down (cascade for 4×/8×)

A zero-alloc 2× oversampler from a Kaiser-windowed-sinc half-band-style FIR. 4×/8× = cascaded 2×
stages. Coeffs computed in `new()`; `tick` uses fixed-size arrays only.

**Files:**
- Create: `plugin303/src/dsp/oversample.rs`
- Modify: `plugin303/src/dsp/mod.rs` (`pub mod oversample;`)

**Design:** FIR length `L = 64` (even), cutoff `0.45 * f_base` (normalised 0.225 at the 2× rate).
Upsampler splits `h` into 2 polyphase branches (`h[0::2]`, `h[1::2]`); each base input pushes to an
`L/2`-tap history ring and emits 2 hi-rate samples (one per branch). Downsampler runs the same `h`
over the hi-rate stream and decimates by 2. Group delay `(L-1)/2` hi-rate samples ⇒ report
`latency_samples()` at base rate (`(L-1)/2/2` per 2× stage, summed across the cascade).

**Step 1: Write the failing tests.**

```rust
// oversample.rs tests
fn tone(freq: f32, sr: f32, n: usize) -> Vec<f32> {
    (0..n).map(|i| (2.0*std::f32::consts::PI*freq*i as f32/sr).sin()).collect()
}

#[test]
fn up_then_down_roundtrips_band_limited_tone() {
    let sr = 48_000.0; let mut os = Os2x::new();
    let x = tone(1_000.0, sr, 8_192);
    let mut y = vec![0.0; x.len()];
    for i in 0..x.len() {
        let hi = os.up(x[i]);          // [f32; 2]
        y[i] = os.down(hi);            // back to base rate
    }
    // compare past the FIR latency; band-limited tone returns ~itself
    let lat = os.latency_samples();
    let a = &x[..x.len()-lat]; let b = &y[lat..];
    let err: f32 = a.iter().zip(b).map(|(p,q)| (p-q)*(p-q)).sum();
    let sig: f32 = a.iter().map(|p| p*p).sum();
    assert!(10.0*(err/sig).log10() < -40.0, "roundtrip ESR too high");
}

#[test]
fn upsampler_attenuates_above_base_nyquist() {
    // a tone the upsampler must NOT image: feed a hi-rate tone above f_base/2 via down()
    let sr = 96_000.0; let mut os = Os2x::new();
    let hot = tone(36_000.0, sr, 8_192); // 0.75 * hi-Nyquist, above base Nyquist
    let mut out = vec![0.0; hot.len()/2];
    for i in 0..out.len() { out[i] = os.down([hot[2*i], hot[2*i+1]]); }
    let e: f32 = out.iter().map(|v| v*v).sum::<f32>() / out.len() as f32;
    assert!(e.sqrt() < 0.05, "alias not attenuated: {}", e.sqrt());
}
```

**Step 2: Run, verify fail** (`No Os2x`).

**Step 3: Implement** — `struct Os2x { h: [f32; 64], hist: [f32; 32], pos: usize }` (or two
half-length phase arrays). `fn new()` fills `h` with windowed sinc: `h[i] = sinc(2*fc*(i-c)) *
kaiser(i, beta=8.0)`, then normalise DC gain to 1. `up(&mut self, x) -> [f32;2]` and
`down(&mut self, hi:[f32;2]) -> f32` run the polyphase dot products against the ring. `Os4x`/`Os8x`
= `Os2x` cascades. Add `latency_samples()`. **No heap in `up`/`down`.**

**Step 4: Run, verify pass.** `cargo test -p plugin303 oversample::`

**Step 5: no-alloc gate.** Extend `tests/no_alloc.rs` with an `Os4x` up→down loop inside
`assert_no_alloc`. Run `cargo test -p plugin303 --test no_alloc`.

**Step 6: Commit.** `git commit -m "feat(303): polyphase 2x oversampler + cascade, zero-alloc (F2.2)"`

---

### Task F2.3: `dsp/adaa.rs` — 1st-order antiderivative-AA `tanh`

ADAA1 on a static nonlinearity `f`: `y = (F(x) - F(x₁)) / (x - x₁)` with `F = ∫f`; fall back to
`f((x+x₁)/2)` when `|x - x₁| < ε`. For `f = tanh`, `F(x) = ln(cosh x)` (computed stably).

**Files:**
- Create: `plugin303/src/dsp/adaa.rs`
- Modify: `plugin303/src/dsp/mod.rs` (`pub mod adaa;`)

**Step 1: Write the failing test** — alias energy of a hot sine drops vs naive `tanh` at 2×.

```rust
// adaa.rs tests — measure energy in bins that ALIAS (mirror of harmonics above Nyquist)
#[test]
fn adaa_reduces_aliasing_vs_naive_tanh() {
    let sr = 48_000.0; let f0 = 6_000.0; let n = 16_384; let drive = 5.0;
    let naive: Vec<f32> = (0..n).map(|i| (drive*(2.0*std::f32::consts::PI*f0*i as f32/sr).sin()).tanh()).collect();
    let mut a = Adaa1Tanh::new(); let mut adaa = vec![0.0; n];
    for i in 0..n { adaa[i] = a.tick(drive*(2.0*std::f32::consts::PI*f0*i as f32/sr).sin()); }
    // crude alias proxy: energy NOT at harmonics of f0 (see helper in test mod)
    assert!(alias_energy(&adaa, sr, f0) < 0.5 * alias_energy(&naive, sr, f0));
}
```

> `alias_energy` = total spectral energy minus the energy in ±1 bin of each harmonic `k*f0` whose
> *true* position is below Nyquist; folded harmonics land off-grid and count as alias. Implement it
> in the test module with a naive DFT over the `n`-point buffer (test-only, allocation OK).

**Step 2: Run, verify fail.**

**Step 3: Implement.**

```rust
// plugin303/src/dsp/adaa.rs
//! 1st-order antiderivative anti-aliasing for static nonlinearities (Parker et al.).

/// ADAA1 wrapper around tanh. `F(x)=ln(cosh x)`, stable via `|x| - ln 2 + ln(1+e^-2|x|)`.
pub struct Adaa1Tanh { x1: f32, f1: f32 }

fn ln_cosh(x: f32) -> f32 {
    let a = x.abs();
    a + (-2.0 * a).exp().ln_1p() - std::f32::consts::LN_2
}

impl Adaa1Tanh {
    pub fn new() -> Self { Self { x1: 0.0, f1: ln_cosh(0.0) } }
    pub fn reset(&mut self) { self.x1 = 0.0; self.f1 = ln_cosh(0.0); }
    pub fn tick(&mut self, x: f32) -> f32 {
        let f = ln_cosh(x);
        let dx = x - self.x1;
        let y = if dx.abs() < 1e-5 { (0.5 * (x + self.x1)).tanh() } else { (f - self.f1) / dx };
        self.x1 = x; self.f1 = f;
        y
    }
}

impl Default for Adaa1Tanh { fn default() -> Self { Self::new() } }
```

**Step 4: Run, verify pass.**

**Step 5: Commit.** `git commit -m "feat(303): ADAA1 tanh anti-aliasing wrapper (F2.3)"`

---

### Task F2.4: Wire cutoff/reso into `Voice` + `render303`; calibrate Low cutoff to SPICE

The F1.2 pass-through args become real, and the Low `set_cutoff(Hz)` is calibrated so its ESR
against the SPICE `I_tail` corner is minimised.

**Files:**
- Modify: `plugin303/src/dsp/voice.rs` (add `set_cutoff`/`set_reso`)
- Modify: `plugin303/src/bin/render303.rs` (use the args)
- Modify: `eval/303/voice_ref_spice.py` (add `itail_to_hz()` mapping)
- Modify: `eval/303/compare_voice.py` (drive `render303` with the Hz-equivalent cutoff)

**Step 1: Write the failing Rust test** — cutoff changes the spectrum.

```rust
// voice.rs tests
#[test]
fn cutoff_setter_changes_brightness() {
    fn hf_rms(cut: f32) -> f32 {
        let mut v = Voice::new(48_000.0);
        v.set_cutoff(cut); v.note_on(57);
        for _ in 0..400 { v.tick(); }
        let mut acc = 0.0; for _ in 0..4_000 { let s = v.tick(); acc += s*s; }
        (acc/4_000.0).sqrt()
    }
    assert!(hf_rms(4_000.0) > hf_rms(300.0) * 1.5, "cutoff did not change output energy");
}
```

**Step 2: Run, verify fail.**

**Step 3: Implement.** Add to `Voice`:

```rust
pub fn set_cutoff(&mut self, hz: f32) { self.filter.set_cutoff(hz); }
pub fn set_reso(&mut self, r: f32) { self.filter.set_reso(r); }
```

And in `render303.rs` replace `let _ = (cutoff, reso);` with `v.set_cutoff(cutoff);
v.set_reso(reso / 100.0);` (reso arg is %, mapped to 0..1).

**Step 4: Run, verify pass** (`cargo test -p plugin303 voice::`).

**Step 5: Calibrate.** Add `itail_to_hz()` to `voice_ref_spice.py` from the vcf303 `f_deep`
relationship (reuse `run_vcf303_spice.cutoff_and_slope`; e.g. 32 µA ≈ its measured `f_deep`).
Re-point `compare_voice.render_rust` to pass `itail_to_hz(cutoff_itail)` as `--cutoff`. Re-run
`compare_voice.py`; record the new (improved) ESR in `voice_esr_baseline.json` and note the value.

**Step 6: Commit.** `git commit -m "feat(303): wire cutoff/reso into voice + render303; calibrate Low to SPICE (F2.4)"`

---

### Task F2.5: `EnumParam<Tier>` in params; block-rate read in `process()`

**Files:**
- Modify: `plugin303/src/params.rs`
- Modify: `plugin303/src/lib.rs` (`process()`, `Voice` gains `set_tier`)
- Modify: `plugin303/src/dsp/voice.rs` (`tier` field + `set_tier`)

**Step 1: Write the failing test** — params default to `Tier::Low`, and the id is stable.

```rust
// plugin303/tests/params_tier.rs
use plugin303::dsp::filter::Tier;
#[test]
fn tier_param_defaults_low() {
    let p = plugin303::params::Plugin303Params::default();
    assert_eq!(p.tier.value(), Tier::Low);
}
```

> Requires `pub mod params;` in `lib.rs` and `#[derive(Enum)]` on `Tier` (nih-plug). Add a
> `nih_plug::params::enums::Enum` derive to `Tier` (it already derives the std traits).

**Step 2: Run, verify fail.**

**Step 3: Implement.**

```rust
// params.rs
use nih_plug::prelude::*;
use crate::dsp::filter::Tier;

#[derive(Params)]
pub struct Plugin303Params {
    #[id = "tier"]
    pub tier: EnumParam<Tier>,
}
impl Default for Plugin303Params {
    fn default() -> Self { Self { tier: EnumParam::new("Tier", Tier::Low) } }
}
```

Add `#[derive(Enum)]` to `Tier` in `filter.rs`. In `process()`, read once per block before the
sample loop and push to the voice:

```rust
self.voice.set_tier(self.params.tier.value());
```

`Voice::set_tier` stores the tier; `tick` matches on it (High/Ultra route to the TPT path for now
— leave a `// F3/F4: WDF` marker). **No alloc** — all filters preallocated as fields.

**Step 4: Run, verify pass** (`cargo test -p plugin303`). Confirm `cargo test --test no_alloc`
still green (tier switch is a field write).

**Step 5: Commit.** `git commit -m "feat(303): EnumParam<Tier>, block-rate read in process (F2.5)"`

---

### Task F2.6: ESR gate test `tests/voice_esr.rs` (replaces M2.4 loose shape test)

A Rust integration test that loads a **committed SPICE gold waveform** + renders the Low voice and
asserts ESR ≤ the post-F2 budget. No ngspice at test time.

**Files:**
- Create (committed gold): `eval/303/gold/voice_A2_mid.f32` (+ a `.json` sidecar: note, cutoff_hz, sr, secs)
- Create: `plugin303/tests/voice_esr.rs`
- Delete/replace: the loose M2.4 shape assert it supersedes (identify in `tests/vcf303_fit.rs` or voice tests during execution)

**Step 1: Generate the gold.** Add a `--dump-gold` path to `compare_voice.py` (or a tiny script)
that writes `voice_reference(note=57, cutoff_itail=32e-6, rreso=47e3)` RMS-normalised to
`eval/303/gold/voice_A2_mid.f32` (`<f4`) plus the sidecar. Commit the artifact.

**Step 2: Write the failing test.**

```rust
// plugin303/tests/voice_esr.rs
use plugin303::dsp::voice::Voice;

fn esr_db(reference: &[f32], model: &[f32]) -> f32 {
    // RMS-normalise both, brute-force best lag in ±256, then sum(err^2)/sum(ref^2).
    // (port of eval/303/esr.py::metrics; lag search + normalise inline)
    // ... implemented in the test ...
}

#[test]
fn low_voice_esr_within_budget() {
    let gold = load_f32("../eval/303/gold/voice_A2_mid.f32"); // sidecar: note57, cutoff_hz, 48k, 0.2s
    let mut v = Voice::new(48_000.0);
    v.set_cutoff(CUTOFF_HZ_FROM_SIDECAR); v.set_reso(0.0);
    v.note_on(57);
    let model: Vec<f32> = (0..gold.len()).map(|_| v.tick()).collect();
    let e = esr_db(&gold, &model);
    assert!(e <= LOW_BUDGET_DB, "Low ESR {e} dB exceeds budget {LOW_BUDGET_DB} dB");
}
```

> `LOW_BUDGET_DB` and `CUTOFF_HZ_FROM_SIDECAR` are set from the F2.4 measurement (the improved
> baseline + a small margin, e.g. measured + 1.0 dB). This is the **fidelity gate** going forward.

**Step 3: Run, verify fail** (no gold / no test) → implement gold + esr_db → **Step 4: pass.**

**Step 5: Commit.** `git commit -m "test(303): Rust-side ESR gate vs SPICE gold; retire M2.4 shape test (F2.6)"`

**F2 close:** re-run `compare_voice.py`; append the post-F2 Low ESR to the design doc's F1 findings
(the number F3's High/Ultra budgets are set relative to).

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
