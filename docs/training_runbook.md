# BA283 Training Runbook (dormant recurrent engine)

Operator runbook for producing a trained, exported BA283 **recurrent** model from
scratch.

> **This pipeline builds the dormant engine, not the shipped one.** The plugin's
> active neural backend is the FiLM WaveNet (`plugin/src/wavenet.rs`); to train
> *that*, see the [WaveNet Training Runbook](wavenet_training_runbook.md). The
> model below (`plugin/src/model.rs`) is still compiled but **dormant** — train it
> for LSTM/SSM research and A/B comparison.
>
> For *what the numbers mean and why the models are built this way*, see the
> concept companion: [Training, Explained](training_explained.md).

## 1. Overview

This pipeline produces `plugin/weights/ba283_lstm_3x18.json` — the self-describing
weight file the Rust plugin loads into its hand-coded `LSTM(3 × 18)` + FiLM +
Dense(1) inference engine (schema `ba283-lstm-v2`, struct `Ba283Model`). The
in-tree trainer is **selectable** via `--model {lstm|s4d|s6}` (default `lstm`);
only the LSTM exports to the plugin schema (S4D/S6 are research/comparison only).
The flow is:

```
stimulus  ->  run_spice  ->  resample  ->  train  ->  export
 (dry WAVs)   (wet .npz)     (wet WAVs +   (best.pth)   (ba283_lstm_3x18.json)
                              split CSVs)
```

This is a long-running, mostly-unattended data-science task. Rough budget:

| Step              | Time (typical workstation) |
| ----------------- | -------------------------- |
| Generate stimuli  | seconds                    |
| SPICE batch       | **several hours**          |
| Resample + split  | minutes                    |
| Train             | **2–4 h on CPU**           |
| Export            | seconds                    |

Plan for most of a day if running end-to-end. Everything is CPU-bound; no GPU is
needed or used.

## 2. Prerequisites

- **ngspice ≥ 42 on PATH** — verify:
  ```bash
  ngspice --version
  ```
- **Python deps** — from the repo root:
  ```bash
  uv sync
  ```
- **Disk space for `data/`** — the SPICE `.npz` targets plus the resampled wet
  WAVs across 5 biases × ~11 segments add up. Budget **several GB** (rough
  order: a few GB; multi-minute clips at 96 kHz with variable-timestep raw `.npz`
  dominate). `data/` is regenerable and gitignored, so it can be wiped freely.
- **CPU-bound** — no GPU is required. The default device is `cpu`.

## 3. Step 1 — Generate dry stimuli

```bash
uv run python -m data_gen.stimulus --out data/dry/
```

Writes ~11 per-segment dry WAVs (96 kHz, mono, float32) at varying drive levels
to `data/dry/`, so the dataset covers the circuit's level-dependent
nonlinearity:

```
sweep_m18.wav   sweep_m12.wav   sweep_m6.wav   sweep_m1.wav
pink_m18.wav    pink_m6.wav
twotone_m12.wav twotone_m3.wav
impulse.wav
multisine_m12.wav  multisine_m3.wav
```

Optionally append real instrument DIs, each scaled to −6 dBFS and written as its
own dry file (`music_<i>_<stem>.wav`):

```bash
uv run python -m data_gen.stimulus --out data/dry/ --music-paths clip1.wav clip2.wav
```

Re-running skips existing files unless you pass `--overwrite`.

## 4. Step 2 — Run the SPICE batch

```bash
uv run python -m data_gen.run_spice \
  --manifest data/dry/ \
  --biases 0.0,0.25,0.5,0.75,1.0 \
  --workers 6
```

`--manifest` accepts a directory of stimulus WAVs (globs `data/dry/*.wav`).
For `--workers`, recommend `cpu_count - 2` (this is also the default).

**Runtime warning.** Each pair is a stiff nonlinear transient with an XSPICE
nonlinear-magnetics transformer, run at 8× oversample (step = 1 / (96 kHz × 8))
over a 20–60 s clip plus tail. Each sim can take **many minutes**. With ~11
segments × 5 biases = **55 sims**, the batch can run **hours**. Start it and
walk away.

Failures are **isolated** — each pair runs in its own process and any exception
is caught, logged, and appended to `data/failed.csv` (columns `stim,bias,error`);
the batch does **not** crash. Check it after the run:

```bash
cat data/failed.csv   # absent if everything succeeded
```

Outputs:
- `data/wet_raw/<stem>_bias<v>.npz` — raw `(t, v)` transient on ngspice's
  variable timestep.
- `data/spice_logs/<stem>_bias<v>.log` — per-pair error log (only on failure).

## 5. Step 3 — Resample + manifest + split

```bash
uv run python -m data_gen.resample \
  --raw-dir data/wet_raw \
  --out-dir data/wet \
  --manifest data/manifest.csv \
  --dry-dir data/dry
```

**`--dry-dir data/dry` is critical.** Without it the manifest's `dry_path`
column points at the default `data/dry` only by luck; always pass it explicitly
so each wet target resolves to its dry stimulus. A missing dry WAV emits a
warning and leaves a dangling manifest row.

This resamples each raw `.npz` onto a uniform 96 kHz grid (PCHIP, no overshoot at
the saturation edges), trims a 50 ms head settle margin, writes the wet WAVs to
`data/wet/`, then writes the manifest and the splits.

Where the CSVs land — `split_manifest(manifest, manifest.parent)` writes the
splits **alongside the manifest** (i.e. into `manifest.parent`). With
`--manifest data/manifest.csv` you get:

```
data/manifest.csv
data/train.csv
data/val.csv
data/test.csv
```

The split is **leakage-free by stimulus file**: all bias variants of one dry
stimulus go to the same split (80/10/10). This needs **≥3 unique dry files**;
with ~11 we are comfortably fine. With <3 the split raises an error.

## 6. Step 4 — Train

```bash
uv run python -m train.train \
  --train-manifest data/train.csv \
  --val-manifest data/val.csv \
  --model lstm \
  --epochs 200 \
  --batch 64 \
  --lr 5e-3 \
  --hidden 18 \
  --layers 3 \
  --conditioning film \
  --warmup 1024 \
  --window 4096 \
  --stride 2048 \
  --workers 6 \
  --checkpoint-dir checkpoints/ \
  --log-dir runs/
```

Roughly **2–4 h on CPU**. TBPTT loop: a `--warmup`-sample no-grad region primes
the LSTM hidden/cell state, then backprop runs over the rest of the window against
the composite pre-emphasised ESR + DC loss.

**Architecture flags** — `--model {lstm|s4d|s6}` (default `lstm`; only `lstm`
exports), `--hidden 18`, `--layers 3`, `--conditioning {film|concat}` (default
`film`), `--head {linear|mlp}`, `--film {linear|mlp}`. SSM-only knobs (when
`--model` is `s4d`/`s6`): `--d-state`, `--d-conv`, `--expand`. The S4D/S6 paths
train and score but **do not export** to the plugin schema.

Robustness features for unattended runs:
- **`--resume`** — restart from `checkpoints/last.pth` (full optimizer +
  scheduler + epoch bundle) after a crash. Re-run the exact same command with
  `--resume` appended.
- **`--seed`** (default `0xBA283`) — reproducibility.
- **Gradient clipping** (`--clip-norm`, default `5.0`) — guards against LSTM
  divergence; non-finite batches are skipped with a warning.
- **Atomic checkpoints** — written via tmp file + rename, so a kill mid-write
  never leaves a truncated file.
- **Early stopping** (`--patience`, default `10`) — stops after N epochs with no
  val/esr improvement.

Checkpoints in `checkpoints/`:
- **`best.pth`** — best-val-ESR weights; this is what you export.
- **`last.pth`** — full resume bundle (model + optimizer + scheduler + epoch).
- `epoch_<NNN>.pth` — snapshot saved each time val/esr improves.

## 7. Step 5 — Monitor

```bash
tensorboard --logdir runs/
```

Watch `train/loss` (should fall steadily) and `val/esr` (the early-stopping
metric; lower is better).

## 8. Step 6 — Export weights

`train/export.py` exposes `export_rtneural_json(model, out_path, meta)`
(LSTM-only — S4D/S6 have no exporter). The convenience script
`scratch/nam_sweep/export_checkpoint.py` wraps it:

```bash
uv run python scratch/nam_sweep/export_checkpoint.py \
    --ckpt checkpoints/best.pth \
    --out  plugin/weights/ba283_lstm_3x18.json \
    --hidden 18 --layers 3 --sample-rate 48000
```

Or call the exporter directly:

```bash
uv run python -c '
from pathlib import Path
import torch
from train.model import BA283LSTM
from train.export import export_rtneural_json

model = BA283LSTM(hidden=18, num_layers=3)
state = torch.load("checkpoints/best.pth", map_location="cpu", weights_only=True)
model.load_state_dict(state)
model.train(False)

export_rtneural_json(
    model,
    Path("plugin/weights/ba283_lstm_3x18.json"),
    meta={"sample_rate": 48000, "input_scale": 2.0},
    sample_rate=48000,
)
print("wrote plugin/weights/ba283_lstm_3x18.json")
'
```

This writes the self-describing `ba283-lstm-v2` weight file (per-layer cells +
learned init state) plus a `.meta.json` sidecar next to it. Copy it over the
embedded placeholder (`include_str!` in `plugin/src/model.rs`) and rebuild.

## 9. Step 7 — Round-trip verification

The Rust inference engine now exists (`plugin/src/model.rs`, the dormant LSTM),
so the PyTorch ↔ Rust numerical round-trip (max abs diff **< 1e-5**) is runnable:

```bash
cargo build --release --bin ba283_standalone
uv run python -m train.verify_roundtrip \
    --weights plugin/weights/ba283_lstm_3x18.json \
    --bin target/release/ba283_standalone --tol 1e-5
# -> max abs diff < 1e-5  (PyTorch vs the zero-alloc Rust forward)
```

Note this verifies the **dormant LSTM** path. The shipped WaveNet's parity is
enforced separately (parse-time topology check + the `eval/nam_vs_acme/` harness);
see the [WaveNet Training Runbook](wavenet_training_runbook.md) §9.

## 10. Running on Colab (local staging)

Both Colab notebooks stage the dataset onto the runtime's local disk **before**
training, so the training phase never reads through Google Drive / a FUSE mount /
a VFS cache. There is no `drive.mount`, no `rclone mount`, and no cache-warming
step — the data is *copied* to `/content/dataset` first, then the manifests are
remapped to those local paths.

- **Ablation notebook (`ba283_ab_colab.ipynb`)** — copies the expanded dataset
  from the rclone remote with `train.stage.stage_rclone` (`rclone copy <remote>
  /content/dataset -P`, asserting `manifest.csv` landed), then rewrites
  train/val/test paths with `train.stage.remap_manifests(... out_dir="/content")`
  and trains against the `/content` manifests.
- **Train notebook (`ba283_train_colab.ipynb`)** — upload
  `ba283_train_bundle.zip` (built by `colab/make_bundle.sh`), then
  `train.stage.stage_zip` extracts it to `/content/dataset` and
  `train.stage.remap_manifests` rewrites the paths before training.
- **Ablation notebook (`ba283_ablation_colab.ipynb`)** — runs the Core-4
  fidelity-ablation cells (B0/C1/C4/C5) on a single GPU venue and scores them with
  the `eval/ablation/` harness. Stages each subset with `train.stage.stage_rclone`
  from `gdrive:ba283_ablation/`. See the dedicated runbook:
  [`docs/colab_ablation_runbook.md`](colab_ablation_runbook.md).

`train.stage` is pure-python (pandas + stdlib) and ships inside
`ba283_train_bundle.zip`, so rebuild the bundle after changing it:

```bash
bash colab/make_bundle.sh
```

## 11. Troubleshooting

- **SPICE non-convergence** — check `data/failed.csv` and the matching
  `data/spice_logs/*.log`. The tolerances in `simulate_one` (reltol=1e-4,
  abstol=1e-9, vntol=1e-6, gmin=1e-13, gear) are tuned for the stiff XSPICE
  transient; if a specific clip won't converge, relaxing tolerances is the lever,
  but loosen carefully — too loose risks silent accuracy loss.
- **NaN / non-finite loss** — already mitigated: grad clipping is on by default
  and non-finite batches are skipped. If it persists, lower `--lr` (e.g. `2e-3`).
- **Empty val/test split** — `split_manifest` needs **≥3 unique dry stimuli**.
  Generate more (Step 1) or add `--music-paths`.
- **Out of disk** — `data/` is fully regenerable and gitignored (along with
  `checkpoints/` and `runs/`). Wipe and re-run from the failed step.
