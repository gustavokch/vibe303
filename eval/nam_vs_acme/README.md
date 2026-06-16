# NAM vs ACME (vs SPICE) — BA283 accuracy harness

Scores the trained NAM model (`ba283-nam-lstm/exported_models/model.nam`) against
the validated ACME.jl reference and the SPICE training target, on the model's own
T3K sweep.

## What it does

`model.nam` is a NAM `SlimmableContainer` / WaveNet with **no gain conditioning**.
It was trained (by the `neural-amp-modeler` package) on the T3K sweep:

* input  = `scratch/nam_sweep/ds48/T3K-60s_dry.wav` (48 kHz, the exact training input)
* target = `scratch/nam_sweep/ds48/T3K-60s_gain{G}.wav` (SPICE reamp, bias 0.77)

With no conditioning the model tracks **one** operating point; it matches
`gain_norm 0.5` best. So the harness is a 3-way comparison at that point:

```
NAM (candidate)  vs  ACME.jl (validated near-ground-truth)  vs  SPICE (truth)
```

ACME runs at its proven 96 kHz (the 48 kHz dry is soxr-upsampled, the wet
downsampled back); SPICE is the on-disk training target. Scoring is at 48 kHz.

### Scoring contract (important)

* **Latency**: the dataset bakes in NAM's calibrated −24-sample delay; every pair
  is cross-correlation lag-aligned (`|lag| ≤ SPAN`).
* **DC/sub-bass**: ACME/SPICE carry a bias-settling offset the NAM (trained on AC)
  lacks; both signals get a 20 Hz high-pass before scoring.
* **Level**: the NAM output is normalized, not volts; ESR is level-matched by an
  optimal least-squares scalar (`fit_gain`, lands near `OUTPUT_REF_V ≈ 64`).

## Gain sweep (conditioned model — Task 6)

The FiLM gain-**conditioned** model (`exported_models/model.cond.nam`, a 2-channel
`[audio, gain]` WaveNet) is scored across the full gain range, not one point. The
harness defaults to `--nam model.cond.nam` and sweeps the 9-point ds48 grid; at
each gain it feeds the gain into the NAM conditioning channel, runs ACME at the
matching `R_KJ`, and pulls the per-gain SPICE reamp target. The same 3-way
contract (lag-align / 20 Hz HP / level-match) is applied per gain.

Conditioned-path I/O (in `run_nam.NamModel`): the audio channel is scaled by
`audio_input_scale` (= the training `INPUT_SCALE` 2.0) and **front-padded by
`RF − 1` zeros** so the VALID-mode inner-net output (the public wrapper can't take
2 channels) is length-matched and time-aligned to the dry timeline; the gain
channel is the constant gain over the full padded length.

```bash
cd /home/gustavo/Git/ba283-vst3
uv run --extra eval python -m eval.nam_vs_acme.compare \
    --gains 0.0,0.125,0.25,0.375,0.5,0.625,0.75,0.875,1.0 \
    --bias 0.77 --max-seconds 30 --out eval/nam_vs_acme/out
```

`summary.json` then carries a `per_gain_table` (NAM-vs-SPICE / NAM-vs-ACME /
ACME-vs-SPICE ESR per gain) plus a **worst-gain** verdict (the gate tracks the
worst operating point so a single weak gain isn't hidden by the average).

## Run (single point — legacy / spot check)

```bash
cd /home/gustavo/Git/ba283-vst3
uv run --extra eval python -m eval.nam_vs_acme.compare \
    --gain 0.5 --bias 0.77 --max-seconds 30 --out eval/nam_vs_acme/out
```

`--gain` is a single-point shortcut (overrides `--gains`). The legacy
`SlimmableContainer` `model.nam` has no gain input, so pass `--nam .../model.nam`
with a single `--gain` to spot-check it; the conditioning channel is ignored for
that arch.

`--max-seconds` trims the 60 s sweep to keep the ACME (Julia) sim tractable.
Needs the `eval` extra (`uv add --optional eval neural-amp-modeler` already done)
and a working `julia` + the `eval/acme_vs_spice` Julia project instantiated.

### Render just the NAM (no ACME/Julia)

```bash
uv run --extra eval python -m eval.nam_vs_acme.run_nam \
    --dry <dry48.wav> --out /tmp/nam.wav
```

## Outputs (`out/`)

* `summary.json` — `per_gain_table`, `worst_gain`/`best_gain`, per-gain
  `nam_vs_acme`/`nam_vs_spice`/`acme_vs_spice` (ESR raw + level-matched, fit_gain,
  lag), FR error, `skipped_gains`, verdict.
* `fr_welch_T3K_g{gain:.3f}.png` — per-gain 3-way Welch |H(f)| + error vs SPICE
  (±1 dB band), e.g. `fr_welch_T3K_g0.500.png`.
* `fr_coherence_T3K_g{gain:.3f}.png` — NAM coherence vs dry, per gain.
* `waveform_overlay_T3K_g{gain:.3f}.png` — 3-way 50 ms overlay (highest-energy
  window), per gain.

## Latest result

### Conditioned WaveNet — full gain sweep (2026-06-16)

The FiLM gain-conditioned `model.cond.nam` (one model, 9-gain T3K reamp, 40-epoch
CPU run, val_esr 0.00172) scored 3-way at every grid gain (30 s, bias 0.77):

| gain_norm | NAM vs SPICE | NAM vs ACME | ACME vs SPICE |
|----------:|-------------:|------------:|--------------:|
| 0.000 | −29.75 dB | −28.88 dB | −39.68 dB |
| 0.125 | −29.84 dB | −29.03 dB | −39.75 dB |
| 0.250 | −29.42 dB | −28.71 dB | −39.88 dB |
| 0.375 | −29.00 dB | −28.38 dB | −40.04 dB |
| 0.500 | −28.87 dB | −28.33 dB | −40.30 dB |
| 0.625 | −29.72 dB | −29.13 dB | −40.52 dB |
| 0.750 | −30.49 dB | −29.82 dB | −40.25 dB |
| 0.875 | **−30.64 dB** (best) | −29.65 dB | −37.85 dB |
| 1.000 | **−28.80 dB** (worst) | −27.51 dB | −34.05 dB |

**VERDICT: STRONG** — worst-gain NAM vs SPICE **−28.8 dB** @ gain 1.0 (gate is the
worst operating point). Clears the ≤ −20 dB target by ~9 dB and is **flat across
the whole gain range** — proving the FiLM conditioning works. It also *beats* the
old fixed-gain `model.nam` (−25.5 dB at its single best point) at every gain.
`fit_gain` lands 63.8–64.0 ≈ `OUTPUT_REF_V` 64 at all gains (scale/pad correct).

Note: this is a **provisional** CPU run (40 epochs, T3K single-sweep only). Already
strong; a longer run or the richer `gdrive:ba283_spice_gain` set (risk-register
fallback) could tighten the high-gain end (1.0 is the weakest, where ACME-vs-SPICE
itself degrades to −34 dB).

### Fixed-gain model (original `model.nam`, single point)

NAM vs SPICE-truth **−25.5 dB** ESR, FR midband error **0.05 dB**, coherence 1.0;
ACME vs SPICE **−42.3 dB** (reference sanity). The residual is the NAM's own
modelling error at `gain_norm 0.5` — concentrated at HF/transients.
