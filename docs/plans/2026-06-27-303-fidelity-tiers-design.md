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

### F3.4 update — ADAA-vs-OS bench on the nodal ladder (`tests/ladder_alias.rs`)

The design's plan (point 2 above) was to pair ADAA1 on the diode with 4× OS. **That is not
applicable to this topology and is dropped.** ADAA (Parker et al.) requires an *explicit* static
map `y = f(x)` to antidifference; the F3.3 diode is solved *implicitly* inside the per-sample
Newton (rung currents are the coupled MNA unknowns), so there is no closed-form `F = ∫f`. The
residual trimmer is therefore the **oversampling factor**, full stop.

Bench (hot 1.7 kHz sine, off-grid, Hann-windowed alias metric, 1×/2×/4×/8× OS) shows the aliased
fold-down energy sits at a **flat ~−53 dB floor across all OS factors** for a *steady tone*, even
at hard drive. Reason: the VCF is itself a lowpass, so it attenuates the diode's HF products before
they fold — **the ladder is self-anti-aliasing for static tones.** OS therefore matters for the
*dynamic* cases this bench can't isolate: fast cutoff sweeps (303 accent/decay) and resonance
near Nyquist. The binding arbiter is F3.5's whole-voice SPICE ESR gate (≤ −30 dB), which stresses
exactly those. **Decision: start Ultra at 4× OS; escalate to 8× only if the F3.5 ESR gate misses.**

### F3-VCO V1–V4 update — circuit VCO built + chained; two structural walls found

V1/V2/V3 landed the device models (Gummel-Poon BJT, MPF104 JFET) and the **nodal vco303
relaxation-oscillator solver** (`dsp/vco.rs`): 21-node MNA+Newton, damped-Newton + gmin homotopy,
trapezoidal caps, rail self-start. It **self-oscillates** with a clean relaxation sawtooth at ~3
Newton iters/sample, zero-alloc. The decisive bring-up bug was a cap-companion sign (current
leaving `p` = `Geq·(vp−vq) − Ieq`).

V4 wired `Vco303` → `Os4x` → `LadderVcf` for `Tier::Ultra` and measured the whole-voice ESR. **It
rose to +5.2 dB (worse than the +2.73 dB polyBLEP path).** Diagnosis turned up two structural walls
that block the −30 dB gate with the current bespoke chain:

1. **VCO pitch / reset timing.** This solver runs ~1.4× faster than ngspice's VCO at the same CV
   (different time-stepping + reset edge), with curvature (`measure_pitch_map`). Refitting `CV_FIT`
   to *this* solver (`characterize_freq_cv`) pins pitch to ~1.4 %, but ESR needs <0.1 % (pitch
   error is NOT removed by lag-alignment — F1). Closing it needs a per-note secant against this
   solver's own measured pitch, and even then the reset *shape* differs from ngspice.
2. **VCF corner ceiling.** The simplified differential diode-ladder core (passive ladder only, no
   active T20/T21 readout or Q9 resonance) saturates its detected −3 dB corner at a **~600 Hz
   ceiling** at 48 kHz (`corner_vs_cutoff`): raising the bias scale lifts the corner only until it
   plateaus and bias-tracking breaks. It cannot reach the gold's 1002 Hz cutoff, let alone the
   resonance peak. The real corner range needs the active stages, not just the rung core.

**Assessment.** −30 dB whole-voice ESR means matching ngspice's adaptive-timestep transient to
~0.03 % sample-for-sample across VCO pitch+shape, VCF corner+resonance, and reset dynamics. Each
bespoke stage is currently 1–10× off *structurally*, and the ladder has a hard corner ceiling. This
target is likely unreachable with independent fixed-rate reimplementations vs ngspice without
essentially replicating ngspice's solver. The architecture (both nodal solvers) is complete and
correct as circuits; the gap is precision-matching a different simulator. **Open decision: keep
grinding per-stage calibration (uncertain payoff), redefine Ultra's acceptance to a spectral/
coherence metric instead of sample-exact ESR, or expand the VCF to the full active topology.**

### F3-VCO V5 — Ultra acceptance redefined to a spectral character match (DONE)

**Decision (user, 2026-06-27): redefine Ultra's gate to spectral/coherence.** Rationale: a
real-time bespoke chain cannot match ngspice's adaptive-timestep transient sample-for-sample to
−30 dB, but it *can* reproduce the circuit's **sound** — harmonic structure, brightness, resonance
envelope. That is the right acceptance for an audio model.

New gate `plugin303/tests/ultra_spectral.rs` (+ offline cross-check `eval/303/coherence.py`):
Welch-averaged spectra (Hann, 50 % overlap), compared over [40 Hz, 16 kHz] by
- **LSD** — gold-power-weighted RMS log-spectral distance (weights bins where the gold has energy,
  so the noise floor above the corner doesn't dominate), and
- **corr** — Pearson correlation of the dB spectra (gross-shape agreement).

**Achieved: LSD = 4.42 dB, corr = 0.977** (Rust gate and Python companion agree exactly). The high
correlation confirms the nodal VCO→VCF chain genuinely captures the circuit's spectral character
despite the +5.2 dB sample-exact ESR. Gate budget set to LSD ≤ 5.5 dB, corr ≥ 0.95 (achieved +
margin). Brightness sweep (`brightness_sweep`) confirmed the physical bias (cal = 1.0) is the LSD
optimum — raising it distorts the envelope away from the gold. The old `ultra_esr.rs` is kept
`#[ignore]` as an informational sample-exact measurement, not the gate.

**Remaining headroom (future, optional):** the residual ~4.4 dB LSD is the VCF corner ceiling
(~600 Hz vs the gold's 1002 Hz) + resonance-peak shape + VCO saw harmonics. Closing it needs the
active T20/T21/Q9 VCF stages and a per-note VCO pitch secant — the "expand VCF active stages" path,
deferred. Ultra now ships as a circuit-faithful (corr 0.977) tier under the spectral gate.

### F4 — High (WDF-lite) tier, spectral-gated (DONE)

**Definition (user, 2026-06-27): High = polyBLEP saw → `LadderVcf` @ 2× oversample.** High keeps the
nonlinear nodal diode-ladder VCF (the audible circuit character) but drives it with the cheap
polyBLEP saw, skipping Ultra's dominant cost — the 21-node nodal VCO Newton. This places High
between Low (TPT, closed-form) and Ultra (circuit VCO → ladder @ 4×) on both fidelity and cost.

**Wiring (`voice.rs`):** added `ladder_hi: LadderVcf::new(sr*2)` + `os2: Os2x`. `Tier::High` routes
`osc.tick()` → `os2.up` → 2× `ladder_hi.tick` → `os2.down`. `set_cutoff`/`set_reso` drive all three
backends so tier switches stay click-free on the param axis. `latency_samples()` returns the 2× OS
group delay for High. Zero-alloc (gated by `no_alloc::high_voice_hot_path_is_alloc_free`).

**Spectral fidelity** (`tests/high_spectral.rs`, shared metric in `tests/common/mod.rs`, gold op
note 57 / 1002 Hz / reso 0.85):

| Tier  | LSD (dB) | corr  |
|-------|----------|-------|
| Low   | 42.16    | 0.931 |
| High  | 28.78    | 0.971 |
| Ultra | 4.42     | 0.977 |

High lands between Low and Ultra on **both** axes — a genuine fidelity step over Low. The
gold-power-weighted LSD stays large for any non-circuit saw (it concentrates on the gold's dominant
bins, which only the circuit VCO reproduces), so High's LSD sits far from Ultra's but well below
Low's. Gate budget LSD ≤ 32 dB, corr ≥ 0.95: Low's 42.16 / 0.931 both fail, so the gate is a real
discriminator, not a rubber stamp.

**Cost** (release, 2 s render, warm-started; `high_spectral::cost_bench`):

| Tier  | ns/sample | % of one core @ 48 kHz | per-sample solves |
|-------|-----------|------------------------|-------------------|
| Low   | 125       | 0.6 %                  | TPT (closed-form, no Newton) |
| High  | 6051      | 29 %                   | 2× `LadderVcf` (10-node Newton) |
| Ultra | 24413     | 117 %                  | 1× `Vco303` (21-node Newton) + 4× `LadderVcf` |

**Cost/fidelity gap.** High reaches corr 0.971 (vs Ultra's 0.977 — within 0.6 %) at **~4× lower
cost** and, critically, stays real-time on one core (29 %), whereas Ultra at 117 % of a core is
effectively offline-grade for a single voice. The 21-node VCO Newton is the entire gap: skipping it
costs almost nothing in correlation because the shared diode-ladder VCF dominates the audible
spectral envelope. High is the RT-viable fidelity tier; Ultra is the reference/bounce tier.

**Same corner ceiling.** High inherits `LadderVcf`'s ~600 Hz corner ceiling (passive ladder, no
active stages) — `set_cal(1.0)` remains the LSD optimum; brightening via bias distorts the envelope.
The active-stage headroom path (deferred) would lift both High and Ultra.
