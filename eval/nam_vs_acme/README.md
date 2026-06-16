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

## Run

```bash
cd /home/gustavo/Git/ba283-vst3
uv run --extra eval python -m eval.nam_vs_acme.compare \
    --gain 0.5 --bias 0.77 --max-seconds 30 --out eval/nam_vs_acme/out
```

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
