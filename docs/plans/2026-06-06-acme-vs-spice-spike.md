# ACME.jl vs SPICE — BA283 feasibility spike

**Date:** 2026-06-06
**Status:** Done — verdict FEASIBLE (linear + clipping).

## Context

The BA283 emulation already has two physics references: the authoritative
ngspice/PySpice netlist (`data_gen/netlist.py`, drawing EX/10283) that produces
the training wet targets, and the external C++ WDF model (now validated:
ESR −30 dB, coherence 1.0). The neural GRU is trained against the SPICE ground
truth.

This spike asked a third question: can [ACME.jl](https://hsu-ant.github.io/ACME.jl/dev/)
— a Julia DK-method (discretised K-method) analog circuit modeller — faithfully
model the same circuit straight from the netlist? A pass opens ACME as a faster
ground-truth generator, an independent cross-check, or an RT-engine source. The
goal here was a verdict, not production: build the circuit, compare to SPICE,
decide the role later. Nothing in `data_gen/`, `train/`, or `plugin/` was changed.

## What was built

New isolated harness `eval/acme_vs_spice/`, mirroring `eval/wdf_vs_spice/` and
reusing its validated SPICE-regen + metric + plotting code:

- **`ba283_acme.jl`** — the BA283 circuit translated 1:1 from `netlist.py`
  (TR1 BC184C CE input, TR2 BC184C + TR3 2N3055 common-collector compound pair,
  LO1166 transformer, RV1 bias loop, K↔J gain network, AC feedback, Miller comp,
  Zobel). Raw little-endian Float64 mono IO (no Julia WAV dependency). Prepends a
  2 s silence warmup so the slow electrolytics settle from zero state (ACME has no
  SPICE `.op`).
- **`run_acme.py`** — Python wrapper. Computes `R_KJ` and the RV1 pot legs from
  `data_gen.netlist` so the ACME circuit is parameterised *identically* to SPICE,
  then subprocesses the Julia script.
- **`compare.py`** — comparator. Imports `run_spice`, `metrics`, `_welch_tf`,
  `_overlay`, and the `SR`/`ONSET_SKIP`/`SPAN`/`DRY_DIR` constants from
  `eval.wdf_vs_spice.compare`; the only substitution is `run_acme` for the model.
  Emits per-point Welch FR + coherence + waveform overlay + `summary.json` + a
  one-line verdict.
- **`README.md`**, **`Project.toml`** / **`Manifest.toml`** (Julia 1.12 + ACME).

### BJT parameter mapping (SPICE `.MODEL` → ACME `bjt(:npn; …)`)

| device | params |
|--------|--------|
| BC184C (TR1, TR2) | `is=1.04e-14, βf=600, βr=0.1, vaf=120, var=200, ikf=0.05, ise=1.0e-14, ηe=1.5, isc=1.0e-15, ηc=1.2, rb=50, re=0.5, rc=0.5` |
| 2N3055 (TR3)      | `is=4.66e-12, βf=360, βr=2, vaf=100, ikf=0.25, ise=3.339e-11, isc=5e-9, rb=3, rc=0.04` |

Dropped (unsupported by ACME `bjt`): `CJE/CJC/TF/TR/MJ*/VJ*/XTB/IRB/RBM/PTF`
(junction-capacitance + temperature terms). The dominant HF shaping is the
external Miller comp caps C2 (220 pF), C5 (330 pF), C3 (4700 pF), modelled
explicitly — so the missing intrinsic caps have little audible-band effect.

## Results

4 points: pink + sweep × gain_norm {0.5, 1.0}, bias fixed 0.77, raw-volts ESR,
|lag| ≤ 64, 0.5 s onset-skip. SPICE regenerated on the fly per point.

| stimulus | gain_norm | R_KJ | ESR | FR err 20Hz–20kHz | FR midband | coherence | peak ref/ACME |
|----------|-----------|------|-----|-------------------|-----------|-----------|---------------|
| pink  | 0.5 | 906 Ω | −30.9 dB | +0.7 dB | +0.0 dB | 1.000 | 2.19 / 2.21 V |
| pink  | 1.0 |  82 Ω | −30.5 dB | +0.1 dB | +0.1 dB | 1.000 | 9.31 / 9.29 V |
| sweep | 0.5 | 906 Ω | −36.6 dB | +0.4 dB | +0.0 dB | 1.000 | 3.37 / 3.36 V |
| sweep | 1.0 |  82 Ω | −32.3 dB | +0.5 dB | +0.1 dB | 1.000 | 16.13 / 16.04 V |

Worst ESR −30.5 dB; worst midband ±0.1 dB; RMS match ≤0.07 dB; lag 0 everywhere.

**Key finding:** gain_norm 1.0 drives hard clipping (16 V sweep peak into the
24 V rail) and ACME still matches SPICE — Ebers-Moll captures the large-signal
nonlinearity as well as Gummel-Poon here. The missing BJT junction caps do not
bite in the audio band.

## Widening 2 — music DI + bias sweep

Confirmed generality beyond the synthetic stimuli / fixed bias.

**Music DI** (`music3_08 - Shed - Rave Echoes_m1`, bias 0.77) → `out/music/`:

| gain_norm | R_KJ | ESR | FR midband | coherence | peak ref/ACME |
|-----------|------|-----|-----------|-----------|---------------|
| 0.5 | 906 Ω | −50.7 dB | +0.0 dB | 1.000 | 4.19 / 4.18 V |
| 1.0 |  82 Ω | −43.9 dB | +0.1 dB | 1.000 | 20.74 / 20.64 V |

Cleaner than the synthetic stimuli (program material has less broadband energy
stressing the residual), even at 20.7 V hard clipping.

**Bias sweep** (`sweep_m3`, gain_norm 0.5) → `out/biassweep/`:

| bias | TR3 idle | ESR | FR midband | coherence | peak ref/ACME |
|------|----------|-----|-----------|-----------|---------------|
| 0.00 | hottest | −35.6 dB | +0.0 dB | 1.000 | 3.50 / 3.46 V |
| 0.25 |         | −36.3 dB | +0.0 dB | 1.000 | 3.42 / 3.42 V |
| 0.50 |         | −36.5 dB | +0.0 dB | 1.000 | 3.40 / 3.39 V |
| 0.77 | spec    | −36.6 dB | +0.0 dB | 1.000 | 3.37 / 3.36 V |
| 1.00 | coolest | −36.7 dB | +0.0 dB | 1.000 | 3.33 / 3.33 V |

ACME tracks SPICE at every RV1 operating point; the bias-driven peak shift
(3.50 → 3.33 V as TR3 idle cools) is reproduced identically. The hottest bias
(0.0, ~160 mA TR3 idle) is marginally the worst (−35.6 dB) but still excellent.

**Verdict holds across all dimensions:** stimulus type (synthetic + music),
gain (linear + clipping), and the full bias range.

## Caveats / next steps

- ACME JITs a fresh `DiscreteModel` per run (~30 s). Cache the compiled model
  before any bulk dataset generation.
- LO1166 is the same linear mutual-inductance model as the netlist (no core
  saturation) — a v2 concern, not a spike blocker.
- ACME is now a third validated physics reference (SPICE + WDF + ACME). Decide
  its role (faster ground-truth gen / cross-check / RT-engine source) next.

## Reproduce

```bash
julia --project=eval/acme_vs_spice -e 'import Pkg; Pkg.instantiate()'
uv run python -m eval.acme_vs_spice.compare --stims "pink_m3_s0,sweep_m3" --gains 0.5,1.0 --bias 0.77
```
