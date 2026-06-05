# WDF vs SPICE accuracy evaluation

Compares the [ba283_wdf](../../../ba283_wdf) Wave Digital Filter model against the
pre-rendered SPICE validation set produced by the `data_gen` pipeline.

## Quick start

```bash
cd ~/Git/ba283-vst3
uv run python -m eval.wdf_vs_spice.compare \
    --spice-set ~/GoogleDrive/ba283_spice \
    --bias 0.5 \
    --rgain 1200 \
    --out eval/wdf_vs_spice/out
```

All plots and metrics land in `--out` (default `eval/wdf_vs_spice/out/`).

## Outputs

| File | What it shows |
|---|---|
| `fr_magnitude.png` | Magnitude response (dB, normalised at 1 kHz): SPICE vs WDF |
| `fr_delta.png` | Shape error: SPICE − WDF in dB |
| `fr_phase.png` | Phase response comparison |
| `level_ladder.png` | Output RMS vs input level (sweep_m1 → sweep_m24) |
| `esr_vs_level.png` | Null depth (ESR dB) vs input level |
| `waveform_overlay_sweep_m6.png` | 50 ms waveform overlay + residual |
| `bias_tracking.png` | SPICE output RMS at each bias vs WDF fixed output |
| `val_esr_bar.png` | ESR (dB) for every val.csv pair at the chosen bias |
| `val_esr_table.csv` | Same data in machine-readable form |
| `summary.json` | All headline numbers |

## What is being measured

All comparisons are **shape-only** after best-fit gain normalisation.  A single
scalar `g` (in V/unit) is fitted on the lowest-level sweep (sweep_m24, linear
region) and then applied to *all* WDF outputs.  This removes the gain difference
and lets the metrics capture how well the *shape* of each model's response matches.

**ESR (Error-to-Signal Ratio):**  
`10 * log10(||g·wdf − spice||² / ||spice||²)` in dB.  
0 dB = error has the same energy as the reference (no improvement over silence).  
−20 dB = the model captures the reference to within ~10%.  
The project's existing gate for the neural model is −22 dB.

## Structural caveats (why perfect accuracy is impossible)

### 1. Single stage vs full chain

The WDF model (`~/Git/ba283_wdf/main.cpp`) models **only the TR1 input stage**
(one Ebers-Moll BJT via a custom damped fixed-point solver) and approximates the
output transformer as a **first-order IIR low-pass filter** (`y = 0.6·x + 0.4·y`).

The SPICE reference models the full BA283 chain:
- **TR1** (BC184C) — common-emitter input stage
- **TR2** (BC184C) — driver stage (DC-coupled)
- **TR3** (2N3055) — output stage
- **LO1166** (Marinair) — linear coupled-inductor transformer model
- 600 Ω secondary termination

Transfer functions differ by construction.  Any residual error after gain
normalisation reflects these two missing stages plus the transformer model error,
not just solver accuracy.

### 2. No bias control

The WDF has a single fixed DC operating point (`targetBiasVoltage = 0.865 V`).  It
has no equivalent to SPICE's RV1 bias pot (`bias` knob, 0–1).

The **bias-tracking gap** plot shows how much the SPICE output shifts across the
five biases while the WDF output stays constant.

### 3. Fake transformer LPF scales with sample rate

The one-pole IIR approximation (`alpha = 0.6`) is defined in the **digital** domain,
so its pole frequency is proportional to the sample rate:

| Sample rate | LPF − 3 dB pole |
|---|---|
| 44.1 kHz | ~ 6.4 kHz |
| 96 kHz   | ~13.9 kHz |

Since the SPICE dataset (and this harness) operates at **96 kHz**, the fake
transformer's rolloff appears at ~14 kHz rather than the ~6 kHz it would produce at
the originally-intended 44.1 kHz.  The `fr_delta.png` plot shows the resulting
high-frequency shape error.

### 4. Non-physical input/output scaling

The WDF binary applies several non-physical transforms that are removed by the
gain scalar `g`:
- Input scaled by **0.02** (so a 0.25 V-peak dry signal drives the BJT at 5 mV peak)
- Output `dynamicOutputPad = 0.85 × (Rgain / 1200)` applied before hard clipping to [−1, 1]
- DC offset subtracted (AC coupling)

THD comparisons use **output RMS**, not input RMS, to avoid the confound that the
WDF and SPICE see the same dry WAV at very different effective input levels.

## Options

```
--spice-set  PATH   SPICE dataset root directory  (default: ~/GoogleDrive/ba283_spice)
--bias       FLOAT  SPICE bias value to compare   (default: 0.5)
--rgain      FLOAT  WDF gain resistor in ohms     (default: 1200)
--out        PATH   Output directory              (default: eval/wdf_vs_spice/out)
```
