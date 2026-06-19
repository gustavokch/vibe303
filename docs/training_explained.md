# BA283 Training, Explained

A concept companion to the training runbooks. The runbooks tell you *which
commands to run and in what order*; this document explains *what the numbers mean
and why the models are built the way they are*. Read a runbook to operate a
pipeline; read this to understand it.

There are **two** neural pipelines:

- The **shipped** backend is a streaming **FiLM WaveNet**
  (`exported_models/model.cond.nam`), trained by the sibling repo's
  `scratch/nam_sweep/` scripts. Operator steps:
  [WaveNet Training Runbook](wavenet_training_runbook.md).
- A **dormant** in-tree **recurrent** engine (`plugin/src/model.rs`) is trained by
  this repo's `train/` pipeline (`--model {lstm|s4d|s6}`). Operator steps:
  [Training Runbook](training_runbook.md).

Both learn a black-box neural emulator of the BA283 output stage from
SPICE-rendered (dry → wet) audio pairs, conditioned on the gain knob. The
**concepts in §1, §3, and §5 below are shared by both**; §2 covers the shipped
WaveNet and §4 the dormant recurrent engine.

---

## 1. Reading the training log

A typical epoch line:

```
epoch  17  train_loss=0.0745  val_esr=0.0969  best=0.0969  *  no_improve=0
```

| term         | meaning |
| ------------ | ------- |
| `epoch`      | one full pass over all training windows |
| `train_loss` | mean composite loss on train data this epoch (pre-emphasised ESR + `0.1·`DC by default). Lower = predictions closer to SPICE on data it is learning from |
| `val_esr`    | pure ESR on the held-out val windows (different stems / unseen time, depending on the split). The real score — generalization |
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
the **composite** (pre-emphasised ESR + DC term); `val_esr` is **plain ESR** on the
raw signal. Do not read a gap between them as overfitting on its own —
compare `val_esr` across epochs instead, and watch `best` / `no_improve`.

---

## 2. Architecture — the shipped FiLM WaveNet

```
dry audio ─┐
           ├─→ [audio·2.0, gain] ─→ WaveNet (23 dilated causal conv layers, 3 ch)
gain_norm ─┘        (2-ch input)         │  per layer: conv_pre_film (FiLM) →
                                         │  dilated conv → LeakyReLU(0.01) → 1×1 residual
                                         ▼
                              head rechannel (3→1) · head_scale ─→ wet/64 prediction
```

The shipped model is a **WaveNet**: a stack of *dilated causal convolutions*, not
a recurrent net. Instead of carrying a hidden state, it looks back over a fixed
window — its **receptive field** of **6347** samples (~132 ms @ 48 kHz). Because
it is causal (only past samples), it is realtime-safe, and the plugin streams it
with **zero added latency**.

Three ideas:

- **Dilated causal convolution.** Each layer convolves over the past with a
  *dilation* (gap between taps) that grows geometrically, so a modest stack reaches
  far back cheaply. The schedule resets across blocks: dilations
  `[1, 3, 7, 17, 41, 101, 239]` repeat (with two extra taps), kernel sizes mostly
  6 with a couple of 15s — RF 6347 in 23 layers at only 3 internal channels.
- **FiLM via `conv_pre_film`.** The gain knob does **not** enter as audio. A 1×1
  conv reads the 2-channel condition `[audio, gain]` and emits a per-channel
  *scale* and *shift* applied to each layer's input (`scale·x + shift`). This is
  Feature-wise Linear Modulation — the convolution does audio dynamics, the FiLM
  hook handles "how does gain reshape the response," learned independently per
  layer. The conditioning is achieved with **stock `neural-amp-modeler` 0.13.0**
  by activating its native `conv_pre_film` and widening the input to 2 channels —
  no fork (see `scratch/nam_sweep/wavenet_cond.py`).
- **Residual stream + head.** Each layer adds a 1×1-projected residual back to the
  stream; a 16-tap head rechannels 3 → 1 and scales to the normalized output.

### Streaming and de-normalization

In training the inner net runs in **VALID** mode: `(B, 2, L) → (B, 1, L−(RF−1))`,
i.e. the first `RF−1` samples have no output yet. The plugin's `wavenet.rs` is a
hand-coded **streaming** port (ring buffers, zero-alloc) that emits one output per
input with no latency, bit-exact with the golden reference in steady state. The
audio channel is fed as `input_scale · x` (`input_scale = 2.0`) and the model
learns `volts / output_ref_v` (`output_ref_v = 64`); the plugin multiplies back to
recover volts (see `plugin/src/dsp.rs`).

### Why a WaveNet (over the recurrent engine)

A dilated-conv stack with a long receptive field captures the circuit's memory
(caps, the LO1166 transformer, the feedback loop) without a stateful recurrence,
parallelizes cleanly at train time, and — at 3 channels / ~2.4 k weights — ports
to a tiny zero-alloc realtime forward. On the 9-gain SPICE reamp it scores
**−28.8 dB worst-gain** vs SPICE, flat across the gain range (see
[`eval/nam_vs_acme/`](../eval/nam_vs_acme/README.md)), beating the fixed-gain
baseline at every operating point.

---

## 3. What training optimizes for

1. **Functional** — given (dry audio, gain knob 0–1), predict the BA283 wet
   output: its saturation, transformer coloration, across the full **14 → 33 dB**
   gain span.
2. **Generalize** — low `val_esr` on held-out data, not memorization.
3. **Condition correctly** — one model spans all gains via the FiLM knob, not a
   separate model per gain setting.
4. **Deployable** — small + causal → realtime Rust inference, PyTorch == Rust to
   ~`1e-5` (steady-state bit-exact for the WaveNet).
5. **Stable unattended** — a multi-hour run survives without NaN or divergence.

The current dataset is the gain-conditioned SPICE reamp (leakage-free or temporal
split depending on the set). The older `ba283_spice` (bias) set was oscillation
garbage and must not be used.

### ESR + pre-emphasis + DC loss (shared)

The objective follows Wright & Välimäki, *"A Perceptual Loss Function for Neural
Modelling of Audio Systems"* (arXiv:1911.08922). Two parts:

**Pre-emphasised ESR.** ESR is scale-invariant and perceptually closer than MSE.
**Pre-emphasis** (`x[t] − α·x[t−1]`, a high-frequency boost) weights the high
frequencies where the ear is sensitive and where the transformer/clipping character
lives — it stops the model from nailing the bass and smearing the treble. The filter
is applied to **both** prediction and target before ESR. `α = 0.8613`: the paper's
`0.85` is for 44.1 kHz; our data is 48 kHz, so the filter's zero is matched-z
mapped to keep the same corner in Hz (`0.85 ** (44100/48000)`).

**DC loss.** The high-pass pre-emphasis zeroes DC/low-freq *in the error signal*, so
pre-emphasised ESR alone cannot penalise a constant output offset. The paper pairs
pre-emphasis with a DC term: `mean(pred−target)² / mean(target²)`, on the **raw**
signals. **On by default** (`dc_weight = 0.1`); `--dc-weight 0` recovers legacy
pure-ESR.

The ESR denominator is floored (`eps = 1e-3`) and near-silent windows are dropped
(`MIN_TARGET_ENERGY = 1e-2`) so a quiet window cannot produce a non-finite
gradient.

**A-weighting variant (opt-in).** `--preemph-filter aweight` swaps the flat
first-order high-pass for an IEC 61672 A-weighting FIR — perceptual loudness
weighting (peak ~2–4 kHz) instead of a monotone HF boost. It is a fixed
linear-phase FIR (its group delay cancels inside ESR); heavier per step than the
one-tap high-pass, so expect slower training. Keep `first_order` as the baseline.

### Output normalization (shared)

SPICE wet output is raw volts (peaks of tens of V). Unnormalized, the loss
explodes and earlier runs NaN'd. The dataset scales output by `1/OUTPUT_REF_V`
with `OUTPUT_REF_V = 64` (a power of two with headroom over the dataset max) so the
loss is well-conditioned. The model learns the normalized output; the plugin
multiplies by `OUTPUT_REF_V` to recover volts. ESR is scale-invariant, so this only
fixes conditioning, not the score.

---

## 4. Legacy recurrent engine (dormant, in-tree)

Before the WaveNet, the deployed path was a recurrent net (`plugin/src/model.rs`,
schema `ba283-lstm-v2`, weights `plugin/weights/ba283_lstm_3x18.json`). It is
**still compiled but dormant** — the plugin runs the WaveNet. The in-tree `train/`
pipeline still builds it, and it is the architecture the
[Training Runbook](training_runbook.md) operates.

### Selectable architectures

`train/train.py` exposes `--model {lstm|s4d|s6}` (default `lstm`):

| `--model` | architecture | export |
| --------- | ------------ | ------ |
| `lstm` (default) | `BA283LSTM(hidden=18, num_layers=3)` — NAM-faithful stacked LSTM, FiLM | ✅ `ba283-lstm-v2` |
| `s4d` | diagonal S4D state-space model | ✗ (training/research only) |
| `s6` | selective (Mamba-style) S6 state-space model | ✗ (training/research only) |

The S4D/S6 paths exist for apples-to-apples comparison against the LSTM baseline;
**only the LSTM exports** to the plugin's JSON schema. The earlier `BA283GRU` was
replaced by the LSTM (the extra forget/cell gating gives more capacity per unit).

### How the recurrent engine works

- **LSTM** — 3 layers × 18 hidden units. Recurrent: it carries hidden + cell state
  sample to sample — the *memory of the circuit*. Causal, so realtime-safe. The
  learned initial state (`h0`/`c0`) is trained, so it only fits if the burn-in is
  in-graph (a TBPTT subtlety).
- **FiLM vs concat.** `--conditioning {film|concat}` (default `film`). In `film`
  the LSTM sees audio only (`input_size=1`) and a tiny `Linear(1→2H)` turns
  `gain_norm` into a per-unit `(1+γ)·y + β` on the LSTM output; the `1+` makes it
  an identity at init. In `concat` the gain is a second LSTM input channel
  (`input_size=2`), forcing the recurrence to spend capacity tracking a constant —
  FiLM usually generalizes better over the wide 14 → 33 dB span.
- **Dense** `Linear(H→1)` collapses the hidden features to one output sample.

### TBPTT + warmup (recurrent only)

A stateful LSTM needs a settled hidden state. Each window: the first **1024**
samples run under `no_grad` to prime the state (no learning), then backprop runs
only over the remaining samples with the carried, detached state — truncated
backprop through time → bounded memory and no exploding gradient over the full
window. (The WaveNet is stateless and has no warmup.)

### Hardening for unattended runs

| guard | why |
| ----- | --- |
| `clip_norm = 5` | recurrent BPTT gradients burst non-finite once fitting starts; clipping bounds them |
| finite-guards   | drop poison batches with inf/NaN gradients |
| atomic checkpoint | saves survive a runtime kill mid-write |
| cosine LR       | decays smoothly over the epoch budget |

---

## 5. Key hyperparameters (quick reference)

### Shipped WaveNet (`scratch/nam_sweep/train_wavenet.py`)

| flag | default | role |
| ---- | ------- | ---- |
| `--n-out` | 4096 | supervised samples/window; `window = RF(6347) + n_out` |
| `--window` | derived | input window; **must exceed RF 6347** (#1 footgun) |
| `--stride` | `n_out` | window hop (non-overlapping supervision) |
| `--epochs` | 200 | with `--patience 20` early stop |
| `--batch` | 16 | |
| `--lr` | 4e-3 | |
| `--clip-norm` | 5.0 | gradient-norm clip |
| `--stft-weight` | 0.0 | MR-STFT term (0 = pure pre-emphasised ESR) |
| `input_scale` | 2.0 | audio fed as `2·x` |
| `output_ref_v` | 64 | volt scale; plugin de-norm multiplier |

### Dormant recurrent engine (`train/train.py`)

| flag | default | role |
| ---- | ------- | ---- |
| `--model` | `lstm` | `lstm` / `s4d` / `s6` (only `lstm` exports) |
| `--hidden` | 18 | recurrent width |
| `--layers` | 3 | stack depth |
| `--conditioning` | `film` | `film` / `concat` |
| `--head` | `linear` | `linear` / `mlp` (saturating readout) |
| `--film` | `linear` | FiLM map: `linear` / `mlp` |
| `--window` | 4096 | samples per training window |
| `--warmup` | 1024 | samples primed under `no_grad` before backprop |
| `--epochs` | 200 | cosine LR `T_max` |
| `--lr` | 1e-3 | Adam learning rate |
| `--patience` | 10 | epochs of no `val_esr` improvement before early stop |
| `--dc-weight` | 0.1 | DC loss term (ON; `0` = legacy pure ESR) |
| `--preemph-alpha` | 0.8613 | first-order pre-emphasis coefficient (48 kHz) |
| `--preemph-filter` | `first_order` | `first_order` or `aweight` |

---

## Appendix — glossary

- **ESR** — Error-to-Signal Ratio, Σerror² / Σsignal². Scale-invariant; `0` perfect.
- **WaveNet** — a stack of dilated causal convolutions; its receptive field (not a
  hidden state) is its memory of circuit dynamics.
- **Receptive field (RF)** — how far back the WaveNet output depends on input;
  6347 samples here. The training window must exceed it.
- **FiLM** — Feature-wise Linear Modulation. A control signal drives a per-feature
  affine (`scale·x + shift` for the WaveNet; `(1+γ)·y + β` for the LSTM) rather
  than entering the main network as an input.
- **`conv_pre_film`** — the native `neural-amp-modeler` WaveNet FiLM hook,
  activated by config to condition each layer's input on `[audio, gain]`.
- **TBPTT** — Truncated Backprop Through Time. Backprop over a bounded window with
  a carried, detached state (recurrent engine only).
- **Pre-emphasis** — `x[t] − α·x[t−1]` (`α = 0.8613` at 48 kHz), a HF boost applied
  to pred and target before the loss so HF errors are not under-weighted.
- **DC loss** — `mean(pred−target)² / mean(target²)` on the raw signals. Penalises
  a constant offset that pre-emphasised ESR is blind to. On by default.
- **Leakage** — train and val sharing source material, inflating `val_esr`
  optimism. The richer sets split by stem to prevent it.
- **Conditioning** — making one model span all gains via the knob, instead of one
  model per gain setting.
- **#30 round-trip** — the contract that the exported weights reproduce PyTorch
  output in the Rust plugin to ~`1e-5` (steady-state bit-exact for the WaveNet).
