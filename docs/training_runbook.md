# BA283 Training Runbook

Operator runbook for producing a trained, exported BA283 model from scratch.

## 1. Overview

This pipeline produces `plugin/weights/ba283_gru32.json` — the self-describing
weight file the Rust plugin (Phase 3) loads into its hand-coded GRU(2, 32) +
Dense(1) inference engine. The flow is:

```
stimulus  ->  run_spice  ->  resample  ->  train  ->  export
 (dry WAVs)   (wet .npz)     (wet WAVs +   (best.pth)   (ba283_gru32.json)
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
  --epochs 200 \
  --batch 64 \
  --lr 5e-3 \
  --hidden 32 \
  --warmup 1024 \
  --window 4096 \
  --stride 2048 \
  --workers 6 \
  --checkpoint-dir checkpoints/ \
  --log-dir runs/
```

Roughly **2–4 h on CPU**. TBPTT loop: a `--warmup`-sample no-grad region primes
the GRU hidden state, then backprop runs over the rest of the window against the
composite ESR + MSE pre-emphasis loss.

Robustness features for unattended runs:
- **`--resume`** — restart from `checkpoints/last.pth` (full optimizer +
  scheduler + epoch bundle) after a crash. Re-run the exact same command with
  `--resume` appended.
- **`--seed`** (default `0xBA283`) — reproducibility.
- **Gradient clipping** (`--clip-norm`, default `5.0`) — guards against GRU
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

`train/export.py` has **no CLI** — it exposes `export_rtneural_json(model,
out_path, meta)`. Load `checkpoints/best.pth` into a fresh `BA283GRU(32)` and
call the exporter:

```bash
uv run python -c '
from pathlib import Path
import torch
from train.model import BA283GRU
from train.export import export_rtneural_json

model = BA283GRU(32)
state = torch.load("checkpoints/best.pth", map_location="cpu", weights_only=True)
model.load_state_dict(state)
model.train(False)

export_rtneural_json(
    model,
    Path("plugin/weights/ba283_gru32.json"),
    meta={"sample_rate": 96000, "input_scale": 2.0},
)
print("wrote plugin/weights/ba283_gru32.json")
'
```

This writes the self-describing weight file plus a `ba283_gru32.meta.json`
sidecar next to it.

## 9. Step 7 — Round-trip verification (Phase 3)

The PyTorch ↔ Rust numerical round-trip (max abs diff **< 1e-5**) is the
**Phase 3 / Task 3.2** acceptance gate and is **NOT runnable now**: it requires
the Rust inference engine, which does not yet exist.

`train/verify_roundtrip.py` is a Phase-2 stub — it only loads `best.pth` and
saves a PyTorch reference output (`/tmp/pytorch_ref.npy`) for the Rust side to
compare against later. It does not, on its own, verify the round-trip. Defer the
real comparison to Task 3.2.

## 10. Troubleshooting

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
