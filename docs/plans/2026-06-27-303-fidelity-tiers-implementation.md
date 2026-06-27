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

## Phase F3 — Ultra: nonlinear diode-ladder VCF (nodal Newton)

Outcome: a per-sample **nodal MNA + Newton-Raphson** solve of the vcf303 4-rung diode ladder
(8× 1N4148 + 4 shunt caps), biased by the cutoff tail current and closed by the resonance leg,
running at 4× oversample. Wired behind `Tier::Ultra`, ESR ≤ −30 dB vs the whole-voice SPICE
reference, coherence ≈ 1.0. Zero-alloc hot path.

**Reality check (corrects the design's "WDF" label):** the BA283 reference (`~/Git/ba283_wdf/
main.cpp`) is *not* a wave-digital tree — it solves the circuit by **nodal MNA + Newton with
companion models** (Ebers-Moll/diode stamps, trapezoidal cap companions, gmin-homotopy, a Schur
complement that factors the constant linear block once and Newton-iterates only the nonlinear
block). That is the discipline we port. The vcf303 ladder rungs are literal **1N4148 diodes**
(see `eval/303/_vcf303_netlist_gen.py`: `Diode("1L"..."4R")`), so Ultra models the 8-diode +
4-cap ladder core directly; the active stages (T38 input gm, T20/T21 readout, Q9 resonance) enter
as the linear drive/read/feedback wrappers exactly as the SPICE harness parameterises them
(`Itail` cutoff sink, `Rreso` feedback leg). We validate against the *existing*
`run_vcf303_spice.py` gold — no new SPICE topology.

**Order:** F3.1 (transcendentals) → F3.2 (diode + cap companion) → F3.3 (the solver) →
F3.4 (ADAA/OS decision bench) → F3.5 (wire Ultra + ESR gate) → F3.6 (no-alloc + tune/close).

---

### Task F3.1: `dsp/expmath.rs` — fast exp/log (BA283 transcendentals)

Autovectorisable `exp`/`log` via IEEE-754 exponent assembly + Taylor (ported from
`ba283_wdf/main.cpp` `fast_exp`/`fast_log`). The diode stamp calls `exp` per node per Newton
iter, so this is the hot transcendental.

**Files:**
- Create: `plugin303/src/dsp/expmath.rs`
- Modify: `plugin303/src/dsp/mod.rs` (`pub mod expmath;`)

**Step 1: Write the failing tests** — match `std` within tolerance over the junction domain.

```rust
// expmath.rs tests
#[test]
fn fast_exp_matches_std_over_junction_domain() {
    for i in -2000..=200 {            // v/Vt ranges deeply negative (reverse bias) to ~+5
        let x = i as f64 * 0.05;
        let (a, b) = (fast_exp(x), x.exp());
        let rel = (a - b).abs() / b.max(1e-300);
        assert!(rel < 1e-9 || b < 1e-300, "exp({x}) rel err {rel}");
    }
    assert_eq!(fast_exp(-800.0), 0.0); // deep reverse bias underflows cleanly, no NaN
}

#[test]
fn fast_log_matches_std() {
    for i in 1..100_000 {
        let x = i as f64 * 1e-3;
        assert!((fast_log(x) - x.ln()).abs() < 1e-9, "log({x})");
    }
}
```

**Step 2: Run, verify fail.**

**Step 3: Implement** — port `fast_exp` (k via `(k+1023)<<52` with the `k<=-1023 => 0.0` underflow
guard — critical: reverse-biased junctions give `v/Vt ~ -1000`) and `fast_log` (exponent-field
decompose + `atanh` series). Use `f64::from_bits`/`to_bits` (Rust's strict-aliasing-safe analog of
the C++ `memcpy`).

**Step 4: Run, verify pass.**

**Step 5: Commit.** `git commit -m "feat(303): fast exp/log for the nodal solver (F3.1)"`

---

### Task F3.2: `dsp/diode.rs` — 1N4148 stamp + junction-cap companion

**Files:**
- Create: `plugin303/src/dsp/diode.rs`
- Modify: `plugin303/src/dsp/mod.rs`

1N4148 params (from `_vcf303_netlist_gen.TRANSISTOR_MODELS`): `IS=1.42e-11`, `N=1.7`,
`CJO=1.74e-12`, `VJ=0.75`, `M=0.333`, `RS=4.2`. Thermal `Vt=0.025852` (300 K) ⇒ emission
`nVt = N*Vt`. `vcrit = nVt*ln(nVt/(IS*√2))` (SPICE pnjlim).

The stamp method is named `stamp` (returns the MNA contribution `(I, g)`); the charge companion
is `charge` (returns `(Q, C)`).

**Step 1: Write the failing tests.**

```rust
// diode.rs tests
#[test]
fn diode_current_and_conductance_match_shockley() {
    let d = Diode1N4148::new();
    for &v in &[-0.5, 0.0, 0.3, 0.6, 0.7] {
        let (i, g) = d.stamp(v);                // (current, dI/dV)
        if v < d.vcrit {
            let want = d.is * ((v / d.n_vt).exp() - 1.0);
            assert!((i - want).abs() <= 1e-6 * want.abs().max(1e-9));
        }
        assert!(g > 0.0, "conductance must stay positive for Newton");
    }
}

#[test]
fn diode_is_linear_beyond_vcrit() {
    // above vcrit the stamp must be the C1 linear extension (bounded exp), so an
    // overshot Newton step can't explode the residual.
    let d = Diode1N4148::new();
    let (_, g1) = d.stamp(d.vcrit + 0.1);
    let (_, g2) = d.stamp(d.vcrit + 0.2);
    assert!((g1 - g2).abs() < 1e-6, "conductance not frozen past vcrit");
}

#[test]
fn depletion_cap_is_finite_forward_biased() {
    let d = Diode1N4148::new();
    let (_q, c) = d.charge(0.74);               // near/over FC*Vj must stay finite (FC extrapolation)
    assert!(c.is_finite() && c > 0.0);
}
```

**Step 2–4: Implement + pass** — port the C++ `diodeEval` routine (vcrit linearization) as `stamp`
and `depletionCharge` (FC=0.5 extrapolation) as `charge`, using `expmath`.

**Step 5: Commit.** `git commit -m "feat(303): 1N4148 diode stamp + junction-cap companion (F3.2)"`

---

### Task F3.3: `dsp/ladder_wdf.rs` — nodal diode-ladder solver (`impl Filter303`)

The core. A fixed-size dense MNA solve of the 4-rung ladder, Newton per sample with gmin homotopy,
trapezoidal cap companions. **Module name kept `ladder_wdf` for continuity with the design doc,
though the method is nodal-Newton.**

**Topology** (from `_vcf303_netlist_gen.py`): two rails L/R, nodes
`tl,al,bl,cl,dl` and `tr,ar,br,cr,dr`. Rungs = diodes `tl–al, al–bl, bl–cl, cl–dl` (and the R
mirror). Shunt caps bridge the rails per rung: `C98 al–ar (33n), C99 bl–br (33n), C100 cl–cr (33n),
C101 dl–dr (18n)`. Differential signal drives the bottom (`dl,dr`); cutoff is the ladder bias
current `I_tail` pulled from the rail tops (sets each diode's `r_d = nVt/I_d` ⇒ corner ∝ I); the
differential output is read across `tl–tr`. (The active T38/T20/T21/Q9 stages are the linear
drive/read/feedback wrappers — F3.5.)

**Numerics:**
- **Cap companion (trapezoidal):** `Geq = 2C/dt`, `Ieq = -(Geq*v_prev + i_prev)`; per-cap state
  `(v_prev, i_prev)` updated after convergence. `dt = 1/(sr*OS)`.
- **MNA assembly:** node-admittance `G` (`N×N`) + RHS `i` (`N`). Stamp each diode rung with
  `diode.stamp` (g into G, companion current into i), each cap with `(Geq, Ieq)`, the bias current
  source, and the differential drive (a Norton source from the input wrapper).
- **Newton:** `F(v) = G(v)·v − i(v)`; solve `J·Δv = −F` (J = G plus the diode `dg/dv` terms, but
  the stamp already returns `g = dI/dV`, so `G` *is* the Jacobian for the static part). Iterate to
  `‖F‖₂ < 1e-9` or `maxIters=50`; on stall, **gmin homotopy** (add `gmin` to each node's diagonal,
  sweep `gmin = 1e-3 → 0`). 
- **Linear solve:** fixed `N`-dim dense LU with partial pivot on stack arrays (`[[f64; N]; N]`),
  zero-alloc. `N ≤ 12` (10 rail nodes + slack).
- **Discipline (BA283):** denormal flush (FTZ via explicit `if x.abs() < 1e-30 { 0.0 }` on states),
  `fast_exp`, warm-start each sample from the previous converged `v` (the cross-sample predictor).

**Step 1: Write the failing tests.**

```rust
// ladder_wdf.rs tests
#[test]
fn dc_operating_point_converges() {
    let mut f = LadderVcf::new(48_000.0);
    f.set_cutoff(1_000.0);
    f.set_reso(0.0);
    for _ in 0..256 { f.tick(0.0); }            // settle; must not NaN / stall
    assert!(f.tick(0.0).is_finite());
}

#[test]
fn cutoff_tracks_bias_current() {
    // doubling cutoff ~ shifts the -3dB corner up ~2x (small-signal AC probe).
    fn corner(fc: f32) -> f32 { /* sweep sines, find -3dB from passband; helper in mod */ 0.0 }
    let (a, b) = (corner(500.0), corner(2_000.0));
    assert!(b > a * 2.5 && b < a * 5.0, "cutoff does not track bias: {a} -> {b}");
}

#[test]
fn slope_is_four_pole_ish() {
    // ~ -20..-24 dB/oct deep stopband (matches vcf303_gold slope ~ -21 dB/oct).
    // measure two octaves above corner.
}
```

**Step 2–4: Implement + pass.** Iterate convergence/solver until the AC probes match the
`eval/303/vcf303_gold.json` trend (corner tracks bias, slope ≈ −21 dB/oct). This is the riskiest
task — expect to tune gmin schedule + warm-start. Validate the DC op against
`run_vcf303_spice.py --op` node voltages where practical.

**Step 5: no-alloc gate.** Extend `tests/no_alloc.rs` with a `LadderVcf::tick` loop.

**Step 6: Commit.** `git commit -m "feat(303): nodal diode-ladder VCF solver, Newton+gmin (F3.3)"`

---

### Task F3.4: ADAA/OS decision bench (resolves F1.5)

Decide 2nd-order ADAA on the diode nonlinearity **vs** 4× OS for Ultra (F1.5 said OS is mandatory;
this picks the *residual* trimmer). Build a bench, measure alias ESR each way, implement the winner.

**Files:**
- Create: `plugin303/benches/ladder_alias.rs` (or a `#[test]` reporting numbers, `--nocapture`)
- Possibly: `dsp/adaa.rs` (add `Adaa2` if 2nd-order wins)

**Step 1:** Drive the F3.3 ladder with a hot saw at three notes; render at (a) 4× OS naive,
(b) 2× OS + ADAA1 on the diode, (c) 2× OS + ADAA2. Compute alias-bin ESR (reuse the FFT metric
from `adaa.rs` tests / `eval` analog).

**Step 2:** Record the table in the design doc; **implement the winner** as Ultra's path; keep the
loser behind a `#[cfg(feature = "bench")]` or a bench-only constructor.

**Step 3: Commit.** `git commit -m "bench(303): Ultra ADAA-vs-4xOS decision; implement winner (F3.4)"`

---

### Task F3.5: Wire `Tier::Ultra` → ladder solver at OS; Ultra ESR gate

**Files:**
- Modify: `plugin303/src/dsp/voice.rs` (Ultra routes to `LadderVcf` inside the oversampler)
- Modify: `plugin303/src/bin/render303.rs` (a `--tier` arg so the harness can render Ultra)
- Create (gold): `eval/303/gold/ultra_A2_mid.f32` + sidecar (via `gen_gold.py --tier ultra`… see note)
- Create: `plugin303/tests/ultra_esr.rs`

**Step 1: Voice wiring.** Add `LadderVcf` as a preallocated field; `tick` routes `Tier::Ultra`
through `Os{4}x` (up → solver at hi rate → down). Keep Low/High on TPT. The OS adds latency — the
voice reports it (deferred to F5 for host PDC, but record it).

**Step 2: render303 `--tier`.** Map `0/1/2 → Low/High/Ultra`; default Low (keeps F2 golds valid).

**Step 3: Generate Ultra gold + gate.** The whole-voice SPICE reference is unchanged (it already
*is* the diode-ladder truth); the Ultra gold is the SAME `voice_reference(...)` waveform the Low
gate used — so the gate is: render the **Ultra** voice, ESR vs that SPICE gold ≤ −30 dB. Add
`tests/ultra_esr.rs` (reuse the `voice_esr.rs` ESR port).

```rust
// ultra_esr.rs
const ULTRA_BUDGET_DB: f32 = -30.0;
// render Tier::Ultra at the sidecar cutoff/reso; esr_db(gold, ultra) <= -30
```

**Step 4: Run.** Iterate solver/OS until ≤ −30 dB. If 4× can't reach it, escalate to 8× (the OS
switch is already there) or revisit ADAA2 (F3.4). **Add a coherence check** (Welch, in a Python
companion `eval/303/coherence.py`) asserting ≈ 1.0 in-band.

**Step 5: Commit.** `git commit -m "feat(303): Ultra tier = diode-ladder solver at OS; ESR <= -30 dB gate (F3.5)"`

---

### Task F3.6: no-alloc, denormal, tune, close F3

- Confirm `cargo test --test no_alloc` covers the Ultra path (solver + OS).
- Denormal/NaN guard: a long-render test that asserts no NaN/inf and bounded output across a cutoff
  sweep (the Newton solver must never emit a bad sample).
- Append F3 findings to the design doc: achieved Ultra ESR/coherence, OS factor chosen, ADAA
  decision, per-sample Newton iteration count (cost). **Closes F3; informs F4 (High = cheaper
  solver: fewer Newton iters + `fast_exp` already in).**
- Commit: `docs(plan): F3 Ultra results + F4 cost budget`.

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
