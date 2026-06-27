# Design — 303 voice fidelity tiers (ESR-gated, ba283 principles)

**Date:** 2026-06-27
**Status:** approved (brainstorm) — implementation plan to follow.
**Branch:** `feat/phase2-303-fidelity-tiers` (off `feat/phase2-303-m2`).

## Problem

The M2 `TptLadder` matches only *shape-invariant AC* bounds (corner ratio, resonance
height, per-pole step) within loose tolerances (`plugin303/tests/vcf303_fit.rs`). It is an
ideal Moog-style ladder, not the 303 diode ladder — so its **time-domain** error vs SPICE is
large and unquantified. We want the voice to *track SPICE*, with a runtime quality axis whose
top tier is indistinguishable by ear, measured the way the BA283 emulation measured fidelity.

## Accuracy metric (ported verbatim from BA283)

Single source of truth = **ESR** (error-to-signal ratio), reported in dB, lag-aligned:

```
lag      = argmax cross-correlation(ref, test)        # remove model latency
err      = align(ref) - align(test)
esr      = sum(err^2) / sum(ref^2)
esr_db   = 10 * log10(esr)
```

Plus **NRMSE** and **Welch FR magnitude + coherence** (the BA283 `metrics()` trio). The
perceptual gate uses **pre-emphasised ESR** (`α = 0.8613` @ 48 kHz — matched-z of the paper's
0.85) so HF error is weighted toward audibility.

**Per-tier ESR targets** (vs the whole-voice SPICE reference):

| Tier  | Engine                              | ESR target | Meaning                          |
|-------|-------------------------------------|-----------:|----------------------------------|
| Ultra | WDF, exact diode `exp`, full Newton |   ≤ −30 dB | indistinguishable (coherence ~1) |
| High  | WDF, cheap transport-`exp`, fewer iters | ≈ −22 dB | transparent in a mix             |
| Low   | fitted TPT (M2 ladder, corrected)   |   ≈ −12 dB | musical, cheapest                |

Tiers also gate **added error vs Ultra** (BA283's same-reference budget), so a lossy tier can
never regress below its own engine's clean output by more than its budget.

## Tier → engine map

All tiers run **≥ 2× oversampling + ADAA** (no 1× tier — locked decision).

- **Ultra** — `dsp/ladder_wdf.rs` wave-digital model of the 4-rung diode ladder (8×1N4148 +
  .033/.033/.033/.018 µF) + the active Q9 common-emitter resonance feedback. Nonlinear diode
  pairs solved per sample (Newton + LUT). Circuit-exact.
- **High** — same WDF topology, **cheap-exp charge model** + reduced Newton iterations (BA283's
  device-model precision axis). Same structure, lower cost.
- **Low** — the M2 `TptLadder`, with `set_cutoff/reso/gain` *calibrated to SPICE* (the corner
  mapping the M2.4 fit deferred). Cheap escape hatch.

Runtime control = nih-plug `EnumParam<Tier>`, read **block-rate**. Engine swap uses
**settle-then-crossfade** (BA283 lesson, commit `bb1d8b2`: settle the restored model before the
fade, else a Medium-tier swap clicks). Tier-invariant latency reporting (BA283 `0e66121`).

## ADAA order — per-tier, ESR-gated (not a global choice)

ADAA order is decided **empirically by the ESR harness**, not fixed up front:

- **Low** — 1st-order ADAA. Static `tanh`, no Newton; 2× OS clears the rest cheaply.
- **High** — 1st-order ADAA. The Newton-nesting argument (below) dominates.
- **Ultra** — start 1st-order; **escalate to 2nd-order ADAA on the diode nonlinearity *only if*
  F1 shows aliasing is what keeps Ultra above −30 dB.** The cheaper alternative — bump Ultra to
  **4× OS** — is evaluated head-to-head at F3 (often cheaper than 2nd-order-inside-Newton and
  avoids its conditioning hazards).

Why not 2nd-order everywhere:

- **Ill-conditioning.** ADAA divides by input differences (`x[n]−x[n−1]`). 1st-order has one
  divided difference; 2nd-order has *nested* ones → blows up when consecutive samples are close,
  needing Taylor-fallback branches with tuned epsilons in the hot loop (more denormal/NaN risk).
- **Nesting in the WDF Newton solve.** ADAA assumes a known input trajectory over the sample;
  inside an implicit nonlinear WDF solve the diode "input" is itself iterated, so 2nd-order's
  two-sample memory + ill-conditioning compounds with Newton. 1st-order composes cleanly.
- **Cost + state.** Extra per-nonlinearity work and memory.
- With ≥2× OS mandatory, 1st-order's residual alias floor is already low — 2nd-order's gain has
  diminishing returns except possibly at the Ultra −30 dB target, which F1 will confirm or deny.

## ESR harness — whole voice, offline (chosen scope)

Reference = SPICE **transient** of the whole voice (VCO → VCF → VCA), per
`(note, cutoff, reso, accent)` operating point.

**Reference generation = chained Phase-1 decks (recommended), not one monolithic deck:**

```
vco303 .tran (ramped-rail self-start, gear, Phase-1 solver opts)
   → V(out) sampled → emitted as a PWL source
       → vcf303 .tran (driven; cutoff bias I_tail + reso leg Rreso)
           → × VCA envelope scale → reference wav
```

Rationale: a VCO relaxation oscillator *and* a stiff diode ladder in a single `.tran` is a
convergence minefield; chaining isolates the finicky VCO self-start from the ladder and is still
end-to-end faithful. *(Alt — single mega-deck — rejected on convergence risk.)*

Plugin side: an **offline render** entry point renders the Rust voice at matched SR / note /
params / tier. Python compare lives under `eval/303/` (reuse the Phase-1
PySpice-netlist → subprocess-ngspice → `wrdata`/numpy pattern). Output: a committed **ESR gold
fixture** (JSON) + a per-tier ESR/coherence table.

**Stimuli:** pitched notes across the keyboard, filter cutoff sweeps, accent steps, resonance
sweeps — the operating points that exercise the nonlinear ladder.

## DSP components (new)

- **Oversampling** — fixed 2×/4×/8× (default 2×; locked, no adaptive-OS). Polyphase FIR
  up/down-samplers, pre-sized in `initialize()`; report added latency to the host.
- **ADAA** — antiderivative antialiasing on the nonlinearities (ladder `tanh`, diode `exp`),
  order per the gated table above.
- **WDF backend** `dsp/ladder_wdf.rs` — port the BA283 WDF discipline: structure-aware solve,
  zero-alloc, autovectorised transcendentals, denormal flush.

## Structure & realtime safety

- Traits `Filter303 { set_cutoff/reso/poles, tick }` and `VoiceEngine` so tiers swap behind one
  interface; `Voice` owns the active engine.
- **Zero-alloc per tier** — extend the `assert_no_alloc` gate to every tier's hot path. Tier /
  OS buffers pre-allocated in `initialize()`. Denormal/NaN guards; self-osc bounded by `tanh`.

## Testing

- **ESR gate tests** replace M2.4's loose shape bounds: Rust loads the committed ESR gold
  fixture and asserts each tier ≤ its budget (and added-error vs Ultra ≤ budget).
- **Component TDD**: WDF stage vs analytic; OS up→down round-trip identity; ADAA measurably
  lowers aliasing (THD/alias-bin energy) vs naive.
- **assert_no_alloc per tier**; self-osc bounded; tier-switch crossfade click-free.

## Sequencing (new fidelity milestone — supersedes the M8 tier outline; replaces M2.4 loose fit)

- **F1** — whole-voice SPICE-transient harness + ESR/NRMSE/coherence compare. *Quantify the
  current gap* (Low/TPT ESR today) and the aliasing contribution. Gates every later decision.
- **F2** — tier scaffold: `EnumParam<Tier>`, 2× OS, ADAA(1st), `VoiceEngine` trait, TPT as Low,
  ESR gate test wired. Low tier calibrated to SPICE.
- **F3** — WDF Ultra. **Decision point:** 2nd-order ADAA vs 4× OS to clear −30 dB (driven by F1
  numbers).
- **F4** — WDF-lite High + charge-model precision axis.
- **F5** — tune all tiers to targets; settle-then-crossfade tier switching; finalize fixed-OS
  switch.

## Risks

- **WDF scope** — the BA283 WDF backend was many PRs. The 303 ladder + active resonance is a
  comparable build. Mitigate by gating on F1 first (measure before committing to Ultra scope).
- **Whole-voice SPICE transient** — VCO self-start in `.tran` is delicate; chained decks reduce
  but don't eliminate convergence risk. F1 proves the reference pipeline before any tier work.
- **TPT Low may not reach −12 dB** — if calibration plateaus high, Low's target is revised from
  measured F1 data rather than asserted up front.

---

## F1 Findings (measured 2026-06-27)

Harness: `eval/303/compare_voice.py` — chained-deck SPICE reference (`voice_ref_spice.py`,
vco303 `.tran` → vcf303 `.tran` → linear AD VCA) vs the Rust Low/TPT voice (`render303`),
RMS-normalised and lag-aligned ESR (BA283). Baseline JSON: `eval/303/voice_esr_baseline.json`.

### The gap (current Low/TPT voice)

| metric        | value                              |
|---------------|------------------------------------|
| ESR mean      | **+2.29 dB**                       |
| ESR worst     | **+2.79 dB** (A3, bright/open)     |
| NRMSE         | 0.45 – 0.52                        |

Positive ESR ⇒ error energy *exceeds* signal energy: after RMS-norm + lag-align, ref and voice
are near-uncorrelated (ESR ≈ 2(1−corr) ⇒ corr ≈ 0.27). Expected — cutoff/reso are unwired in
the voice (every corner renders the same fixed-cutoff TPT voice) and TPT ≠ diode ladder. This is
the floor every tier improves on.

> Note: plan's "note 57 ≈ 110 Hz (A2)" was off by an octave; Rust `midi_to_freq(57)` = 220 Hz.
> The reference is pitched to the Rust voice via a secant CV solve, else the ESR is meaningless.
> Op-points are notes 45/57/69 × {mid 32 µA/47 k, bright 128 µA/open}; accent omitted (not on
> either side until F2/F4).

### Aliasing contribution

Isolated by rendering the whole voice at 8× (384 kHz) and AA-decimating back to 48 kHz, then ESR
vs the native 48 kHz render (`/tmp/alias.py` probe):

| note | aliasing ESR | nrmse  |
|------|--------------|--------|
| A1 (45) | −10.1 dB  | 0.065  |
| A2 (57) | −11.7 dB  | 0.053  |
| A3 (69) | −15.4 dB  | 0.034  |

The native voice already aliases at **−10 dB worst** (low notes, dense harmonics) — and that is
the *linear* path (polyBLEP saw + TPT ladder, no nonlinearity). The WDF diode ladder is a hard
nonlinearity that will generate far more out-of-band energy.

### Decision: F3 ADAA vs OS

Aliasing is the dominant non-topology error and sits an order of magnitude above the Ultra
−30 dB target. Conclusions:

1. **Oversampling is mandatory for Ultra regardless of topology** — 2× is insufficient; the
   linear path alone aliases at −10 dB. Implement **≥4× OS** as the Ultra baseline.
2. **ADAA on the diode nonlinearity, not as an OS substitute.** Pair **1st-order ADAA** on the
   diode pair *with* 4× OS for F3.1/F3.2; bench **2nd-order ADAA @ 2×** as the cheaper candidate
   (F3.2 keeps the loser behind a bench). The F1 numbers say OS carries the alias budget and ADAA
   trims the residual — not either/or.
3. **Low/TPT target revised from data:** at +2.3 dB ESR the original −12 dB Low goal is
   unreachable by calibration alone. Once F2 wires cutoff/reso and 2× OS, re-measure; set Low's
   gate to the *post-F2* number, not −12 dB asserted up front.

### Post-F2.4 update (cutoff/reso wired)

Wiring the SPICE-calibrated cutoff (`itail_to_hz`, resonance-peak corner) and a coarse resonance
map into the Rust voice moved the ESR **not at all** — mean +2.53 dB / worst +2.86 dB, vs the
+2.29 / +2.79 dB pre-calibration baseline (the slight rise is the crude resonance map adding a
ring mismatch). **The gap is topological, not parametric:** polyBLEP saw + TPT ladder is
near-uncorrelated with the circuit saw + diode ladder regardless of cutoff/reso tuning. This
hard-confirms F1.5 — Low cannot be calibrated toward SPICE; audible fidelity requires the WDF
backends (F3 Ultra / F4 High). Low's ESR gate (F2.6) is therefore the *measured* number
(~+2.5 dB) + margin: a regression guard on the TPT path, not a fidelity target.
