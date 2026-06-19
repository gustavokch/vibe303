# BA283 WaveNet Training Runbook (shipped neural backend)

Operator runbook for producing `exported_models/model.cond.nam` — the **active**
neural backend the plugin runs per-sample (`plugin/src/wavenet.rs`). For *what the
numbers mean and why the model is built this way*, see the concept companion:
[Training, Explained](training_explained.md).

> **The in-tree `train/` pipeline does NOT build this model.** It trains the
> *dormant* recurrent engine (`plugin/src/model.rs`); see
> [`training_runbook.md`](training_runbook.md). The shipped WaveNet is trained by
> the scripts under **`~/Git/ba283-nam-lstm/scratch/nam_sweep/`** (a sibling repo).
> Only the **reamp data-gen** (this repo's SPICE netlist), the **embed**, and the
> **scoring** steps touch `ba283-vst3`.

## 1. Overview

The shipped model is a 2-channel-input, FiLM-conditioned **WaveNet** built on
**stock `neural-amp-modeler` 0.13.0** — *not a fork*. The conditioned topology is
produced by config alone (`scratch/nam_sweep/wavenet_cond.py`): it deep-copies a
proven WaveNet `.nam` config, sets every layer's `input_size`/`condition_size` to
2, and activates the native `conv_pre_film` FiLM hook. `nam` is pulled in as this
repo's `eval` extra (`neural-amp-modeler>=0.13.0` in `pyproject.toml`).

```
gen.py            SPICE reamp: T3K-60s dry @ 9-gain grid, bias 0.77  ->  wet_raw/*.npz
   │              (drives data_gen.run_spice.simulate_one from THIS repo's netlist)
   ▼
_rebuild_ds48.py  resample 96k -> 48k + aligned dry + manifest + temporal split  ->  ds48/
   │              (data_gen.resample; tags each clip with gain_value ∈ [0,1])
   ▼
train_wavenet.py  stateless WaveNet loop (pre-emphasised ESR), window = RF + n_out
   │              ->  checkpoints_wavenet/best.pth
   ▼
train_wavenet.py --export   reconstruct via build_cond_wavenet, inject scaling
   │                        metadata  ->  model.cond.nam
   ▼
exported_models/model.cond.nam   copy into ba283-vst3, embedded at build time
   ▼
eval/nam_vs_acme/                3-way scoring (NAM vs ACME vs SPICE), worst-gain gate
```

**Model facts** (baked into `model.cond.nam`):

| field | value |
|-------|-------|
| architecture | WaveNet, 23 dilated causal conv layers, 3 channels |
| input | 2-channel `[audio, gain]` (channel 1 = constant normalized gain ∈ [0,1]) |
| conditioning | `conv_pre_film` (FiLM) active on every layer |
| receptive field | **6347** samples (~132 ms @ 48 kHz); zero added latency (streaming/causal in the plugin) |
| sample rate | 48000 |
| `input_scale` | 2.0 (model is fed `2.0 · x`) |
| `output_ref_v` | 64.0 (model learns volts/64; the plugin multiplies back) |

## 2. Prerequisites

- **The sibling repo** `~/Git/ba283-nam-lstm` checked out, with this repo
  (`ba283-vst3`) available as its `--project` for the `eval` extra (`nam`).
- **ngspice ≥ 42 on PATH** (for the reamp step) — `ngspice --version`.
- **Python deps** — `nam` comes from the `eval` extra; training also needs
  `torch`, `pandas`, `soundfile`, `tensorboard` (all in `pyproject.toml`).
- **`PYTHONPATH` = the `ba283-nam-lstm` root** so `scratch.nam_sweep` and `train`
  resolve (the run commands set this inline).

## 3. Step 1 — Reamp the SPICE training data

```bash
cd ~/Git/ba283-nam-lstm
uv run --project ~/Git/ba283-vst3 python scratch/nam_sweep/gen.py
```

`gen.py` drives `data_gen.run_spice.simulate_one` (this repo's BA283 netlist)
directly so each wet `.npz` is tagged with the **normalized** gain knob, not raw
`R_KJ` ohms. It reamps `dry96/T3K-60s.wav` at the 9-point grid
`[0.0, 0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875, 1.0]`, bias **0.77** fixed,
into `wet_raw/`.

Each sim is a stiff nonlinear transient — the batch runs serially
(`maxtasksperchild=1`) to stay under ngspice's ~2.1 GB output-buffer ceiling.
To split it into RAM-safe batches, set `GEN_GAINS` (e.g. `GEN_GAINS="0.0,0.125"`)
and run once per batch.

## 4. Step 2 — Build the 48 kHz dataset

```bash
uv run --project ~/Git/ba283-vst3 python scratch/nam_sweep/_rebuild_ds48.py
```

This resamples each raw `.npz` to a uniform **48 kHz** WAV (via
`data_gen.resample.main`), co-trims a 50 ms head off **both** dry and wet to keep
them frame-aligned, asserts `dry_sr == wet_sr == 48000` (the dry-resample-bug
guard), checks the 9-gain coverage, and writes a **temporal** 80/10/10 split into
`scratch/nam_sweep/ds48/{train,val,test}.csv` (with `frame_start`/`frame_end`
columns). T3K is a single dry stimulus, so the by-stimulus split is skipped
(`--no-split`) in favour of the temporal one.

> Validation here is unseen *time* of seen gains, not unseen stimuli. For a
> richer, leakage-by-stimulus set, fold in `gdrive:ba283_spice_gain` — see the
> sibling repo's `REMOTE_TRAINING.md` §6 (the recipe is engine-agnostic; the
> manifests share the `gain_value ∈ [0,1]` convention).

## 5. Step 3 — Train

```bash
cd ~/Git/ba283-nam-lstm && PYTHONPATH=$PWD \
  uv run --project ~/Git/ba283-vst3 --extra eval \
  python scratch/nam_sweep/train_wavenet.py \
    --train-manifest scratch/nam_sweep/ds48/train.csv \
    --val-manifest   scratch/nam_sweep/ds48/val.csv \
    --epochs 200 --batch 16 --lr 4e-3 \
    --device auto --workers 0 \
    --checkpoint-dir scratch/nam_sweep/ckpt_wavenet \
    --log-dir        scratch/nam_sweep/runs_wavenet
```

> **#1 footgun — the window must exceed the receptive field.** The WaveNet's
> forward is VALID (no `pad_start`): `(B, 2, L) -> (B, 1, L − (RF − 1))`, so a
> window ≤ RF (6347) yields **zero** valid output. The loop derives
> `window = RF + n_out` from `--n-out` (default 4096 → window 10443) and asserts
> `window > RF`. Override with `--window` only if you know what you're doing.

Key flags (defaults in `train_wavenet.py`):

| flag | default | role |
|------|---------|------|
| `--n-out` | 4096 | supervised samples per window; `window = RF + n_out` |
| `--window` | (derived) | explicit input window; must exceed RF 6347 |
| `--stride` | `n_out` | window hop (non-overlapping supervision) |
| `--epochs` | 200 | with `--patience` early stop |
| `--batch` | 16 | |
| `--lr` | 4e-3 | |
| `--device` | `auto` | `cuda → mps → cpu`, or force `cuda`/`mps`/`cpu` |
| `--patience` | 20 | early-stop after N epochs with no `val_esr` improvement |
| `--clip-norm` | 5.0 | gradient-norm clip |
| `--stft-weight` | 0.0 | MR-STFT term (0 = pure pre-emphasised ESR; MPS-fussy) |
| `--seed` | `0xBA283` | reproducibility |
| `--resume` | — | restore model/optimizer/scheduler/epoch/best from `last.pth` |
| `--smoke` / `--smoke-steps` | — / 200 | short finite-loss sanity run |

One stdout line per epoch (`train_loss`, `val_esr`, `best`, `*` on a new best,
checkpointed to `<checkpoint-dir>/best.pth`). The loss reuses `train.loss`
(pre-emphasised ESR + DC); the model is stateless, so there is **no** TBPTT
warmup (unlike the LSTM loop). A good fit on this clean SPICE target lands well
below `val_esr 0.1` (−20 dB); the provisional 40-epoch CPU run reached `0.00172`.

Smoke test first (finite-loss, no real training):

```bash
cd ~/Git/ba283-nam-lstm && PYTHONPATH=$PWD \
  uv run --project ~/Git/ba283-vst3 --extra eval \
  python scratch/nam_sweep/train_wavenet.py --device cpu --epochs 1 --smoke
```

## 6. Step 4 — Export to `.nam`

`--export PATH` skips training, reconstructs the architecture via
`build_cond_wavenet` (same seed), loads the checkpoint weights, injects the
scaling/conditioning provenance, and writes the `.nam` atomically:

```bash
cd ~/Git/ba283-nam-lstm && PYTHONPATH=$PWD \
  uv run --project ~/Git/ba283-vst3 --extra eval \
  python scratch/nam_sweep/train_wavenet.py \
    --export       scratch/nam_sweep/model.cond.nam \
    --export-ckpt  scratch/nam_sweep/ckpt_wavenet/best.pth
```

Export `best.pth` (best-val), **not** `last.pth` (the resume bundle / final
epoch) — the exporter warns loudly if you point it at `last.pth`. The written
`.nam` carries `metadata.input_scale = 2.0`, `metadata.output_ref_v = 64.0`,
`sample_rate = 48000`, and a `condition` block recording `channels =
["audio","gain"]`, `gain_channel = 1`, `gain_range = [0,1]`, so every consumer
(plugin, eval) reads the contract from the file instead of re-deriving it.

## 7. Step 5 — Embed into the plugin & rebuild

```bash
cp ~/Git/ba283-nam-lstm/scratch/nam_sweep/model.cond.nam \
   ~/Git/ba283-vst3/exported_models/model.cond.nam
cd ~/Git/ba283-vst3
cargo xtask bundle ba283 --release
```

`plugin/src/wavenet.rs` embeds the file at build time
(`include_str!("../../exported_models/model.cond.nam")`) and **validates the
topology at parse time** (channels, layer count, kernel/dilation schedule, FiLM
hooks). A topology mismatch fails the build with an explicit message — re-export
from a matching `build_cond_wavenet` config if it does.

## 8. Step 6 — Score against ACME + SPICE

The 3-way harness renders the embedded WaveNet, ACME.jl, and the SPICE target at
each gain and reports per-gain ESR with a **worst-gain** verdict (the gate tracks
the weakest operating point, not the average; target ≤ −20 dB).

```bash
cd ~/Git/ba283-vst3
uv run --extra eval python -m eval.nam_vs_acme.compare \
    --gains 0.0,0.125,0.25,0.375,0.5,0.625,0.75,0.875,1.0 \
    --bias 0.77 --max-seconds 30 --out eval/nam_vs_acme/out
```

Single-point spot check: `--gain 0.5` (overrides `--gains`). Render the WaveNet
alone (no Julia): `python -m eval.nam_vs_acme.run_nam --dry <dry48.wav> --out
/tmp/nam.wav`. Scoring contract, outputs, and the latest result table live in
[`eval/nam_vs_acme/README.md`](../eval/nam_vs_acme/README.md). The provisional
shipped model scores **−28.8 dB worst-gain** (gain 1.0), flat across the range —
clearing the gate by ~9 dB and beating the old fixed-gain `model.nam`.

## 9. PyTorch ↔ Rust parity

The plugin's `wavenet.rs` forward is a hand-coded zero-alloc port of the inner
`_wavenet.WaveNet`; it is **bit-exact with the golden reference in steady state**.
`eval/nam_vs_acme/run_nam.py` is the PyTorch reference renderer (it front-pads by
`RF − 1` to match the VALID-mode inner net to the dry timeline), and
`eval/nam_vs_acme/test_cond_runner.py` exercises conditioned `.nam`
loading/running. There is no separate `verify_roundtrip` for the WaveNet path
(that tool targets the LSTM JSON); parity is enforced by the parse-time topology
check plus the eval harness.

## 10. Troubleshooting

- **Zero / empty supervised output** — window ≤ RF. Use the derived
  `window = RF + n_out` (≥ 10443) or a larger `--window`.
- **`unrecognized arguments`** — stale sibling checkout; the WaveNet loop and the
  conditioned dataset path post-date earlier branches. Pull the sibling repo.
- **SPICE non-convergence in `gen.py`** — check the per-sim log; the tolerances in
  `data_gen.run_spice.simulate_one` are tuned for the stiff transient. Relax
  carefully (too loose → silent accuracy loss).
- **High-gain weakness (gain 1.0)** — the weakest operating point, where
  ACME-vs-SPICE itself degrades to ~−34 dB. A longer run or the richer
  `gdrive:ba283_spice_gain` set tightens it.
