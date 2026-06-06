# BA283 Training, Explained

A concept companion to the [training runbook](training_runbook.md). The runbook
tells you *which commands to run and in what order*; this document explains *what
the numbers mean and why the model is built the way it is*. Read the runbook to
operate the pipeline; read this to understand it.

The pipeline learns a black-box neural emulator of the BA283 output stage from
SPICE-rendered (dry → wet) audio pairs, conditioned on the gain knob, and exports
it to a self-describing weight file the Rust plugin runs per-sample in realtime.

---

## 1. Reading the training log

A typical epoch line:

```
epoch  17  train_loss=0.0745  val_esr=0.0969  best=0.0969  *  no_improve=0
```

| term         | meaning |
| ------------ | ------- |
| `epoch`      | one full pass over all training windows (362 in the example run) |
| `train_loss` | mean composite loss on train data this epoch (`0.75·ESR + 0.25·MSE`, both on the pre-emphasized signal). Lower = predictions closer to SPICE on data it is learning from |
| `val_esr`    | pure ESR on the held-out val windows (48 in the example run; different stems, zero leakage). The real score — generalization to unseen audio |
| `best`       | lowest `val_esr` seen so far |
| `*`          | this epoch improved on `best` → `best.pth` checkpoint written |
| `no_improve` | epochs since last improvement. Reaching `patience` triggers early stop |

### ESR

**ESR = Error-to-Signal Ratio** = Σ(pred−target)² / Σ(target)². The energy of the
mistake divided by the energy of the true output. `0` = perfect, `1` = error as
loud as the signal itself. So `0.097` means error energy is 9.7% of signal energy
≈ **−10.3 dB**. Convert with `dB = 10·log10(ESR)` (it is already a power ratio).

ESR is **scale-invariant** — it does not care about absolute volume, because the
denominator normalizes by target energy. That is why it beats raw MSE for audio:
a model that nails the waveform shape but is 2× too loud is heavily penalized by
MSE yet scores well on ESR, which is the perceptually relevant behavior.

### Why `train_loss` ≠ `val_esr` numerically

They are different metrics, not the same number on different data. `train_loss` is
the **composite** (ESR + MSE, on the pre-emphasized signal); `val_esr` is **plain
ESR** on the raw signal. Do not read a gap between them as overfitting on its own —
compare `val_esr` across epochs instead, and watch `best` / `no_improve`.

---

## 2. Architecture — `BA283GRU(hidden=32)`

```
dry audio ─┬─────────────────────→ GRU(in=1, hidden=32) → y[t,32]
           │                                                  │
gain_norm ─┴─→ FiLM: Linear(1→64) → γ[32], β[32]              │
                                       │                      ▼
                                   (1+γ)·y + β  ──→  Dense(32→1) → wet prediction
```

Three pieces:

- **GRU** — 1 layer, 32 hidden units. Recurrent: it carries a hidden state `h[t]`
  sample to sample. This is the *memory of the circuit* — capacitors, the LO1166
  transformer inductance, the feedback loop. The output at time `t` depends on
  input history, not just the current sample. It is causal (only past samples),
  so it is realtime-safe.
- **FiLM** (Feature-wise Linear Modulation) — the gain knob does **not** enter the
  GRU. Instead a tiny `Linear(1→64)` turns `gain_norm` into a per-unit scale `γ`
  and shift `β` that modulate the GRU output: `(1+γ)·y + β`. The `1 +` makes the
  modulation an identity at init (`γ`,`β` start ≈ 0), so training begins as if
  unconditioned, then learns the gain dependence.
- **Dense** `Linear(32→1)` — collapses the 32 hidden features to one output sample.

### The A/B: `film` vs `concat`

The model has two selectable conditioning modes (see `train/model.py`):

| mode    | GRU `input_size` | how gain enters |
| ------- | ---------------- | --------------- |
| `film`  | 1 (audio only)   | separate `Linear(1→2H)` → affine on GRU output |
| `concat`| 2 (audio + gain) | gain is a second input channel into the recurrence |

`concat` is the baseline. Feeding a slow control (gain) as a per-sample audio
channel forces the GRU to spend recurrent capacity tracking a constant. `film`
factors it out: the GRU does audio dynamics, the linear map handles "how does gain
reshape the output." The dataset/training interface is identical in both modes
(`x[...,0]` audio, `x[...,1]` gain), so the A/B isolates the conditioning choice.

---

## 3. What training optimizes for

1. **Functional** — given (dry audio, gain knob 0–1), predict the BA283 wet
   output: its saturation, EQ curve, transformer coloration, across the full
   **14 → 33 dB** gain span.
2. **Generalize** — low `val_esr` on held-out stems, not memorization. The
   no-leakage split (different source stems in train vs val) enforces this.
3. **Condition correctly** — one model spans all gains via the knob, not six
   separate models. The FiLM-vs-concat A/B decides which injection generalizes
   better over the wide gain range.
4. **Deployable** — small + stateful + causal → realtime Rust inference, with
   PyTorch == Rust agreement to ~`1e-5` (the #30 round-trip contract).
5. **Stable unattended** — a multi-hour CPU run survives without NaN or
   divergence.

The current dataset is `ba283_spice_gain` (gain-conditioned, leakage-free). The
older `ba283_spice` set was oscillation garbage and must not be used.

---

## 4. Why these choices

### Black-box GRU, vs WDF / SPICE

The real circuit's white-box models either do not run realtime (SPICE) or were
non-deterministic. A recurrent net learns the I/O behavior from SPICE-rendered
pairs and runs cheap per-sample. This is standard for neural VA (Wright/Damskägg,
the GuitarML / NAM lineage). Note that the white-box situation has since improved:
both [`ba283_wdf`](../eval/wdf_vs_spice/) and the
[ACME.jl model](../eval/acme_vs_spice/) now track SPICE closely (ESR ≈ −30 dB,
coherence ≈ 1.0), so they are viable as plugin DSP **and** as alternative data
sources — but the deployed plugin path remains the trained GRU.

### ESR + pre-emphasis loss

ESR is scale-invariant and perceptually closer than MSE. **Pre-emphasis**
(`x[t] − 0.85·x[t−1]`, a high-frequency boost) weights the high frequencies where
the ear is sensitive and where the transformer/clipping character lives — it stops
the model from nailing the bass and smearing the treble. The `0.75 / 0.25` ESR/MSE
blend keeps a DC/level anchor so the absolute output stays correct. The ESR
denominator is floored (`eps = 1e-3`) and the dataset drops near-silent windows
(`MIN_TARGET_ENERGY = 1e-2`) so a quiet window cannot produce a non-finite
gradient.

### TBPTT + warmup

A stateful GRU needs a settled hidden state. Each window: the first **1024**
samples run under `no_grad` to prime `h` (no learning, just warm the memory), then
backprop runs only over the remaining samples with the carried, detached state.
This truncates the backprop length (truncated backprop through time) → bounded
memory and no exploding gradient over the full 4096-step window.

### FiLM conditioning

Injecting a slow control as a per-sample audio channel (`concat`) makes the GRU
waste recurrent capacity on a constant. FiLM factors the control out of the
recurrence; it usually generalizes better over a wide control range — exactly our
14 → 33 dB span. The A/B proves it for this specific circuit.

### Output normalization

SPICE wet output is raw volts (peaks of tens of V). Unnormalized, the MSE term
explodes and earlier runs NaN'd. The dataset scales output by
`OUTPUT_SCALE = 1/OUTPUT_REF_V` with `OUTPUT_REF_V = 64` (a power of two with
headroom over the dataset max) so the loss is well-conditioned. The model learns
the normalized output; the Rust plugin multiplies by `OUTPUT_REF_V` to recover
volts. ESR is scale-invariant, so this choice only fixes conditioning, not the
score.

### Hardening for unattended runs

| guard | why |
| ----- | --- |
| `clip_norm = 5` | GRU BPTT gradients burst non-finite once fitting starts; clipping bounds them |
| finite-guards   | drop poison batches with inf/NaN gradients (clip alone turns inf into NaN via `inf·0`) |
| atomic checkpoint | saves survive a runtime kill mid-write |
| cosine LR       | decays smoothly to 0 over the epoch budget |
| `lr = 1e-3` (not 5e-3) | the higher rate diverged; this is the stable choice |

---

## 5. Key hyperparameters (quick reference)

Defaults from `train/train.py`; override via CLI flags. The example run in §1 used
`patience 40`.

| flag         | default | role |
| ------------ | ------- | ---- |
| `--hidden`   | 32      | GRU hidden units |
| `--window`   | 4096    | samples per training window |
| `--warmup`   | 1024    | warmup samples primed under `no_grad` before backprop |
| `--epochs`   | 200     | cosine LR `T_max` |
| `--lr`       | 1e-3    | Adam learning rate |
| `--clip-norm`| 5.0     | gradient-norm clip |
| `--patience` | 10      | epochs of no `val_esr` improvement before early stop |
| (dataset) `OUTPUT_REF_V` | 64 | volt scale; plugin de-norm multiplier |
| (loss) `esr_weight` | 0.75 | ESR vs MSE blend |
| (loss) pre-emphasis `alpha` | 0.85 | HF-boost coefficient |

---

## Appendix — glossary

- **ESR** — Error-to-Signal Ratio, Σerror² / Σsignal². The audio fidelity metric;
  scale-invariant; `0` perfect.
- **TBPTT** — Truncated Backprop Through Time. Backprop over a bounded window with
  a carried, detached hidden state, instead of the full sequence.
- **FiLM** — Feature-wise Linear Modulation. A control signal drives a per-feature
  affine `(1+γ)·y + β` rather than entering the main network as an input.
- **Pre-emphasis** — `x[t] − α·x[t−1]`, a high-frequency boost applied to pred and
  target before the loss so HF errors are not under-weighted.
- **GRU** — Gated Recurrent Unit. A recurrent cell whose hidden state is the
  model's memory of circuit dynamics (caps, transformer, feedback).
- **Leakage** — train and val sharing source material, which inflates `val_esr`
  optimism. The split is by stem to prevent it.
- **Conditioning** — making one model span all gains via the knob, instead of
  training a separate model per gain setting.
- **#30 round-trip** — the contract that the exported weights reproduce PyTorch
  output in the Rust plugin to ~`1e-5`.
