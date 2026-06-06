# WDF vs SPICE accuracy evaluation (gain-conditioned)

Compares the [ba283_wdf](../../../../ba283_wdf) full-chain Wave Digital / nodal-MNA
model against the **SPICE reference regenerated on the fly** from this repo's
corrected netlist (`data_gen/netlist.py`, drawing EX/10283).

The dataset is conditioned on **gain at a fixed bias**: RV1 (the set-once bias trim)
is pinned at **0.77** (~65 mA TR3 idle, symmetric clipping), and the K↔J gain
resistor `R_KJ` is swept via `gain_norm ∈ [0,1]` → `data_gen.netlist.gain_norm_to_rkj`
(≈ 82 Ω…10 kΩ ≈ 33…14 dB). Both the model output and the SPICE wet are **raw volts at
node SEC_IN**, so they are compared **directly** (no gain normalisation).

This mirrors the WDF repo's authoritative `validate/gain_validate.py` and adds plots
+ a `summary.json` + a Welch FR/coherence check on top of its text table.

## Quick start

```bash
cd ~/Git/ba283-vst3
uv run python -m eval.wdf_vs_spice.compare \
    --stims "pink_m3_s0,sweep_m3,music3_08 - Shed - Rave Echoes_m1" \
    --gains 0.0,0.25,0.5,0.75,1.0 \
    --bias 0.77 \
    --out eval/wdf_vs_spice/out
```

Dry stimuli are read from `~/GoogleDrive/ba283_spice/dry/` (inputs are
topology-independent, so the dry set is reusable); the wet target is regenerated per
point — no stale pre-rendered wet set.

## Outputs

| File | What it shows |
|---|---|
| `fr_welch.png` | Welch \|H(f)\| (dB) SPICE vs WDF + error, on pink noise |
| `fr_coherence.png` | Model coherence vs dry input |
| `esr_vs_gain.png` | Time-aligned ESR (dB) vs `gain_norm`, per stimulus |
| `waveform_overlay_*.png` | 50 ms waveform overlay + residual (mid-gain) |
| `summary.json` | All headline numbers + per-point ESR/NRMSE/level/peaks |

## What is being measured

Output is raw volts at SEC_IN; comparison is **direct** (no gain fit). For each
(stimulus, gain) the harness regenerates the SPICE wet, runs the C++ model on the
same dry, time-aligns (`|lag| ≤ 64`), skips the first **0.5 s** (startup transient),
then reports:

**ESR (Error-to-Signal Ratio):** `10·log10(‖spice − wdf‖² / ‖spice‖²)` in dB.
−22 dB = the project's neural-model gate; lower is better. On clean broadband/sweeps
the time-domain L2 is **phase-sensitive**, so the **Welch FR + coherence**
(`fr_welch.png`) is the authoritative linear-fidelity check.

`gain` in the per-point metrics is `mod_rms / ref_rms` in dB — ≈ 0 dB confirms the
volt scale matches.

## Important notes

- **0.5 s onset skip.** SPICE starts from its `.op` steady state; the model starts
  from a rail-ramp warmup, so the large coupling/feedback electrolytics sit at
  slightly different charge at t=0. On deterministic stimuli the switch-on makes an
  LF overshoot SPICE flat-tops and the model doesn't — a startup artifact a streaming
  plugin never sees, confined to the first ~0.5 s. `ONSET_SKIP` guards it.
- **ngspice memory cap.** A ~5 s 96 kHz×2-oversampled sim of the 21-node corrected
  netlist can exceed ngspice's internal memory limit (affects `gain_validate.py`
  too). Each sim runs in a one-shot child (no cross-sim RAM leak) and is retried;
  points that still fail are recorded under `summary.json → spice_failures` and
  skipped, not fatal.
- **WDF intermittent UB.** The binary occasionally segfaults non-deterministically;
  `try_run_wdf` retries up to 3× and records `recovered_on_retry` / hard
  `crashed_stimuli` in `summary.json → wdf_stability`.

## Options

```
--stims   CSV    dry-stimulus stems without _dry.wav
                 (default: pink_m3_s0, sweep_m3, music3_08 - Shed - Rave Echoes_m1)
--gains   CSV    gain_norm grid in [0,1]            (default: 0.0,0.25,0.5,0.75,1.0)
--bias    FLOAT  fixed RV1 bias                     (default: 0.77)
--out     PATH   output directory                  (default: eval/wdf_vs_spice/out)
--no-build       never rebuild ba283_sim; use the on-disk binary as-is
```

## Relationship to the WDF repo's validator

`~/Git/ba283_wdf/validate/gain_validate.py` is the authoritative text-table validator
(same SPICE-on-the-fly + gain conditioning + onset-skip metric). This harness reuses
that exact methodology and adds the plots, Welch FR, and machine-readable summary.
