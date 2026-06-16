# Design — FiLM gain conditioning for the NAM WaveNet

Date: 2026-06-16

## Context

`model.nam` (NAM `SlimmableContainer` / WaveNet, evaluated in `eval/nam_vs_acme/`)
reproduces the BA283 well — **−25.5 dB ESR vs the SPICE truth** — but only at a
**single operating point** (`gain_norm 0.5`). It has no gain conditioning, so it
cannot follow the gain knob. The repo's *other* model, the LSTM
(`ba283-lstm-v2`, what the Rust plugin currently runs), already conditions on gain
via FiLM. We want the more-accurate WaveNet to gain the same knob.

The installed `neural-amp-modeler` fork's WaveNet already has FiLM machinery
(`nam/models/wavenet/_film.py`: a 1×1 conv maps a condition tensor `c` →
per-channel scale+shift) and a `condition_dsp` path, but `WaveNet.forward(x)`
derives the condition from the audio only (`c = x`, or `condition_dsp(x)`) — there
is no external knob input. So adding gain conditioning means getting the gain
value into `c`.

## Decisions (from brainstorming)

- **Target**: Python train + eval now; design stays portable for an eventual Rust
  plugin port, but the plugin/Rust work is **deferred**.
- **Mechanism**: gain rides in as an **extra input channel** (no package fork).
- **Data**: reamp a **denser gain grid** on the T3K sweep before training.

## Approach

### 1. Architecture (no package fork)

A NAM WaveNet configured as:

- `input_size = 2` — channel 0 = audio, channel 1 = gain value broadcast constant
  over time.
- `condition_dsp = None` → condition `c` = the 2-channel input (`c = x`).
- `condition_size = 2`; activate FiLM hooks (start with `conv_pre_film` and/or
  `input_mixin_pre_film`). The FiLM 1×1 conv learns per-channel scale+shift from
  `[audio, gain]`.

Pure config + data — no fork of the `nam` package. Caveat vs the LSTM's
gain-*only* FiLM: here FiLM also sees the audio channel; acceptable, the network
learns to use the gain channel for modulation.

### 2. Training data — densify the gain grid

- Extend `scratch/nam_sweep/gen.py` `GAINS` to ~9 points (e.g. 0, 0.125, …, 1.0)
  on the T3K 60 s sweep, bias 0.77 (`BIAS_NORM_FIXED`), via `simulate_one`.
- Honour the ngspice ~2.1 GB output-buffer ceiling (see memory
  *ba283-nam-sweep-train*): one gain per child process (gen.py already uses
  `maxtasksperchild=1`); keep clips at 60 s.
- Emit 48 kHz dry + per-gain wet + a manifest carrying `gain_value` (existing
  `scratch/nam_sweep/ds48/` format; rebuild train/val/test CSVs).

### 3. Training

- Reuse `train/dataset.py` (already yields `audio, gain, target` and applies
  `INPUT_SCALE`/`OUTPUT_*` scaling).
- A thin training loop builds the 2-channel input `[audio ; gain·ones]` per window
  and optimizes the nam WaveNet with the existing pre-emphasized **ESR + MSE**
  loss (`train/loss.py`).
- Export the trained model to `.nam`, ensuring the export captures `input_size=2`
  and the active FiLM hooks so it round-trips through `init_from_nam`.

### 4. Evaluation

- Extend `eval/nam_vs_acme/compare.py` to **sweep gains**: for each gain, feed the
  matching gain channel to the NAM, run ACME at that gain (bias 0.77), score 3-way
  (NAM vs ACME vs SPICE) with the existing lag-align / 20 Hz HP / level-match
  contract.
- **Success** = good ESR (≈ −25 dB, comparable to the fixed-gain model) across the
  *whole* gain grid — not just 0.5 — plus FR within ±1 dB and coherence ≈ 1.

### 5. Deferred (out of scope now)

- Rust plugin port: 2-channel-input WaveNet inference + `.nam` loading + FiLM in
  the VST3. Design keeps the model portable; no Rust work this round.

## Key risks

1. **2-ch FiLM config authoring + `.nam` round-trip.** Building a valid 2-ch
   FiLM WaveNet config and confirming `build → forward → export → init_from_nam →
   reload` is bit-identical is the first implementation step and the main risk (a
   hand-authored config already collapsed the receptive field in a probe).
2. **`.nam` export fidelity** for the 2-ch / FiLM variant — the eval harness must
   reload it via `init_from_nam`.
3. **Data density vs ngspice RAM** ceiling during the denser reamp.
4. **Future plugin `.nam` loader** must accept 2-channel input + FiLM (tracked for
   the deferred Rust work, not solved here).

## Verification

- Round-trip gate: 2-ch FiLM WaveNet exports and reloads bit-identically.
- Dense reamp produces N gain targets at matched 48 kHz SR (assert dry SR == wet
  SR per *ba283-dry-resample-bug*).
- Trained model: held-out ESR per gain reported.
- `eval/nam_vs_acme` gain sweep: ESR / FR / coherence at every grid gain, with a
  pass/fail gate on worst-gain ESR.
