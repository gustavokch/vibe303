# ACME.jl vs SPICE — BA283 feasibility spike

Translates the authoritative ngspice/PySpice netlist (`data_gen/netlist.py`,
drawing EX/10283) into an [ACME.jl](https://hsu-ant.github.io/ACME.jl/dev/)
DK-method model (`ba283_acme.jl`) and checks how faithfully it tracks the SPICE
ground truth on the real 3-BJT + LO1166-transformer circuit.

This is a **feasibility verdict, not a gate**. ACME's `bjt()` is an
Ebers-Moll/Gummel-Poon model *without junction capacitances*, so an exact
−22 dB ESR is not expected. The harness reports ESR / NRMSE / level match, the
Welch FR + coherence (the authoritative linear-fidelity check), and a waveform
overlay, then a one-line verdict in `out/summary.json`.

## Prerequisites

Julia is required (installed per-user via [juliaup](https://github.com/JuliaLang/juliaup)):

```bash
curl -fsSL https://install.julialang.org | sh -s -- --yes   # installs to ~/.juliaup
julia --project=eval/acme_vs_spice -e 'import Pkg; Pkg.add("ACME"); Pkg.instantiate()'
```

`run_acme.py` looks for julia at `~/.juliaup/bin/julia`, then `$JULIA_BIN`, then
`PATH`. Tested with Julia 1.12 + ACME (precompiles in ~2 min on first build).

## Usage

Run under this repo's uv env so `data_gen` (SPICE) resolves:

```bash
uv run python -m eval.acme_vs_spice.compare \
    --stims pink_m3_s0 --gains 0.5 --bias 0.77 \
    --out eval/acme_vs_spice/out
```

The SPICE wet target is regenerated on the fly per (stimulus, gain) from the
current netlist (no stale pre-rendered set). Dry stimuli come from
`~/GoogleDrive/ba283_spice/dry/` (shared with the WDF harness).

Outputs in `out/`: `fr_welch.png`, `fr_coherence.png`, `waveform_overlay_*.png`,
`summary.json`.

## How it works

- `ba283_acme.jl` — circuit, 1:1 from `netlist.py`. CLI:
  `julia ba283_acme.jl DRY_BIN OUT_BIN SR GAIN_RKJ R_UPPER R_LOWER`. Raw
  little-endian Float64 mono IO (no Julia WAV dep). Prepends a 2 s silence
  warmup so the slow electrolytics settle from zero state (ACME has no SPICE
  `.op`); the comparator additionally onset-skips 0.5 s.
- `run_acme.py` — Python wrapper. Computes `R_KJ` / RV1 pot legs from
  `data_gen.netlist` so the ACME circuit is parameterised **identically** to
  SPICE, then subprocesses the Julia script.
- `compare.py` — reuses the validated `eval.wdf_vs_spice.compare` SPICE-regen +
  metric + plotting helpers; the only substitution is `run_acme` for the model.

## BJT parameter mapping (SPICE `.MODEL` → ACME `bjt`)

| device | ACME `bjt(:npn; …)` |
|--------|---------------------|
| BC184C (TR1, TR2) | `is=1.04e-14, βf=600, βr=0.1, vaf=120, var=200, ikf=0.05, ise=1.0e-14, ηe=1.5, isc=1.0e-15, ηc=1.2, rb=50, re=0.5, rc=0.5` |
| 2N3055 (TR3)      | `is=4.66e-12, βf=360, βr=2, vaf=100, ikf=0.25, ise=3.339e-11, isc=5e-9, rb=3, rc=0.04` |

**Dropped** (unsupported by ACME `bjt`): `CJE/CJC/TF/TR/MJ*/VJ*/XTB/IRB/RBM/PTF`
— junction capacitance + temperature terms. The dominant HF shaping is the
*external* Miller comp caps (C2 220 pF, C5 330 pF, C3 4700 pF), which are
modelled explicitly, so the missing intrinsic caps mostly affect the very top
octave and fast-transient edges.

## Known limitations

- No BJT junction caps → top-octave / transient deviation from SPICE.
- LO1166 is the same linear mutual-inductance model as the netlist (no core
  saturation); fine, since the transformer nonlinearity is a v2 concern.
- ACME builds a fresh `DiscreteModel` per run (~30 s JIT); acceptable for a spike,
  not for bulk dataset generation without caching the compiled model.
