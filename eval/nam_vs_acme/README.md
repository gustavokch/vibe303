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

* `summary.json` — `nam_vs_acme`, `nam_vs_spice`, `acme_vs_spice` (ESR raw +
  level-matched, fit_gain, lag), FR error, verdict.
* `fr_welch_T3K_g0.50.png` — 3-way Welch |H(f)| + error vs SPICE (±1 dB band).
* `fr_coherence_T3K_g0.50.png` — NAM coherence vs dry.
* `waveform_overlay_T3K_g0.50.png` — 3-way 50 ms overlay (highest-energy window).

## Latest result

NAM vs SPICE-truth **−25.5 dB** ESR, FR midband error **0.05 dB**, coherence 1.0;
ACME vs SPICE **−42.3 dB** (reference sanity). The residual is the NAM's own
modelling error at `gain_norm 0.5` — concentrated at HF/transients.
