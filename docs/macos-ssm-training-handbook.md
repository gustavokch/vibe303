# Training the BA283 SSM models on macOS (Apple Silicon) — handbook

Run the LSTM / S4D / S6 training stack **locally on an Apple Silicon Mac**, using the
PyTorch **MPS** GPU backend. This is the local counterpart to the Colab guide
([`colab-ssm-training-guide.md`](colab-ssm-training-guide.md)); same `python -m train.train`
entrypoint, same dataset, no CUDA.

## What runs where

- **Models** — pure-PyTorch S4D / S6 (no `mamba-ssm`, no CUDA kernels). The recurrent scan is
  a Python time-loop, so it is **slower per epoch than a CUDA GPU**. Keep `--window` / `--epochs`
  modest; an M-series GPU is for iteration and small/medium runs, not A100-scale sweeps.
- **`torch.stft`** — returns a complex tensor, which MPS cannot allocate. The loss handles this
  automatically: the STFT magnitude is computed on CPU and moved back to MPS (see
  `train/loss.py`). Nothing to configure.
- **Device** — `--device auto` resolves `cuda → mps → cpu`. On a Mac it lands on `mps`.

## Requirements

- Apple Silicon Mac (M1 or newer), macOS 13+ (MPS needs recent macOS).
- [Homebrew](https://brew.sh).
- The gain-conditioned SPICE dataset (pulled from Drive in step 3). You do **not** need ngspice
  or PySpice unless you intend to regenerate SPICE data locally.

## 1. Install tooling (Homebrew)

```bash
# Python 3.12 (the repo pins >=3.12,<3.13), uv (env + deps), rclone (dataset pull)
brew install python@3.12 uv rclone

# Optional — only if you will REGENERATE SPICE data locally (data_gen/, PySpice):
brew install ngspice
```

`uv` manages the virtualenv and the exact Python; Homebrew's `python@3.12` is the interpreter
it picks up. `torch` installs the arm64 macOS wheel, which ships MPS support built in (no CUDA
suffix).

## 2. Clone + create the environment

```bash
git clone https://github.com/gustavokch/ba283-vst3.git
cd ba283-vst3

# Resolve and install all deps (torch, soundfile, pandas, tensorboard, scipy, numpy) into .venv
uv sync

# Sanity check: MPS must be available
uv run python -c "import torch; print('torch', torch.__version__, 'mps', torch.backends.mps.is_available())"
```

Expect `mps True`. If it prints `False`, your macOS / torch is too old — `brew upgrade` and
re-run `uv sync`.

## 3. Get the dataset + remap manifests to local paths

The dataset (9-point gain set, `train`/`val` manifests, 44.1 kHz wet SPICE targets) lives on a
Drive accessed via rclone. Configure an rclone remote once (`rclone config`), then:

```bash
# Pull the gain set to ./data/full  (copy, not mount — every file lands on local disk)
rclone copy <remote>:ba283_ablation/full9pt ./data/full -P

# Rewrite manifest audio paths to absolute LOCAL paths and assert dry/wet share SR=44100.
# Writes train.csv / val.csv into ./data/manifests.
uv run python - <<'PY'
from train.stage import remap_manifests, assert_matched_sr
man = remap_manifests("data/full", names=("train", "val"), out_dir="data/manifests")
for name, path in man.items():
    sr = assert_matched_sr(path, expected_sr=44100)
    print(f"{name}: {path}  (SR={sr})")
PY
```

`assert_matched_sr` is the guardrail against the dry@96k / wet@44.1k mismatch that silently
collapses a run to zero output — it fails loudly at stage time instead.

## 4. Train

Common flags (match the Colab recipe): seed `745603`, 60 epochs, patience 15, window 2048,
FiLM conditioning, `--device auto`.

```bash
COMMON=(--train-manifest data/manifests/train.csv \
        --val-manifest   data/manifests/val.csv \
        --device auto --seed 745603 --epochs 60 --patience 15 \
        --window 2048 --conditioning film)

REG=(--weight-decay 1e-5 --clip-norm 1.0 --lr 1e-3)   # shared regularisation
STK=(--stft-weight 0.5 --head mlp --film mlp)          # +stft+mlp variants
```

The five canonical configs (pick one or run several):

```bash
# LSTM baseline (+stft+mlp)
uv run python -m train.train "${COMMON[@]}" --model lstm --hidden 64 --layers 2 \
  --dropout 0.1 "${STK[@]}" "${REG[@]}" \
  --checkpoint-dir ckpt_LSTM_REF --log-dir logs_LSTM_REF

# S4D plain
uv run python -m train.train "${COMMON[@]}" --model s4d --hidden 48 --layers 2 \
  --d-state 64 "${REG[@]}" \
  --checkpoint-dir ckpt_S4D_0 --log-dir logs_S4D_0

# S4D +stft+mlp
uv run python -m train.train "${COMMON[@]}" --model s4d --hidden 48 --layers 2 \
  --d-state 64 "${STK[@]}" "${REG[@]}" \
  --checkpoint-dir ckpt_S4D_1 --log-dir logs_S4D_1

# S6 plain
uv run python -m train.train "${COMMON[@]}" --model s6 --hidden 32 --layers 2 \
  --d-state 16 "${REG[@]}" \
  --checkpoint-dir ckpt_S6_0 --log-dir logs_S6_0

# S6 +stft+mlp
uv run python -m train.train "${COMMON[@]}" --model s6 --hidden 32 --layers 2 \
  --d-state 16 "${STK[@]}" "${REG[@]}" \
  --checkpoint-dir ckpt_S6_1 --log-dir logs_S6_1
```

Each run prints `device=mps`, a per-epoch curve (`train_loss / val_esr / best`), writes
`best.pth` + `arch.json` to its `--checkpoint-dir`, and a full resume bundle to `last.pth`.

- **First, smoke it** — add `--smoke --epochs 1 --window 1024 --warmup 256` to one config to
  confirm the MPS path runs end-to-end (a couple of batches per epoch) before a long run.
- **Resume after a crash / Ctrl-C** — re-run the same command with `--resume`; it restarts from
  `last.pth`.

## 5. Monitor

```bash
uv run tensorboard --logdir . --port 6006   # scalars: train/loss, val/esr
```

The per-epoch stdout curve is the quick read; `dt=…s` per epoch tells you the MPS pace.

## 6. Score against the ACME-vs-SPICE floor (optional)

Eval runs on CPU and reads `arch.json` next to the checkpoint, so it reconstructs the model
without re-typing every knob. `--cell` is just the report-row label; `--trained-gains` is the
5-point grid the model saw.

```bash
uv run python -m eval.ablation.run_checkpoint_eval \
  --checkpoint ckpt_S6_1/best.pth --manifest data/manifests/val.csv \
  --model s6 --hidden 32 --d-state 16 \
  --trained-gains 0.0,0.25,0.5,0.75,1.0 \
  --cell S6_1 --out eval_out/S6_1.json
```

Rank on **held-out-gain ESR** vs the ACME floor (≈ −31.6 dB), not on `val_esr` (stem-overlap
optimistic).

## macOS gotchas

- **MPS False** — macOS or torch too old. `brew upgrade python@3.12 && uv sync`; macOS 13+.
- **Slow epochs** — inherent to the Python-loop scan (not a bug). Lower `--window` (e.g. 1024)
  and/or `--epochs`; the architecture is built for portability over scan throughput.
- **DataLoader workers** — `--workers` defaults to 6. If you see fork/semaphore noise, drop to
  `--workers 0` (single-process loading). `pin_memory` is auto-disabled off CUDA, so no warning.
- **Thermals / sleep** — long runs throttle on a hot laptop and pause on sleep. Run plugged in;
  `caffeinate -i uv run python -m train.train …` keeps the machine awake.
- **First MPS op is slow** — Metal shader compilation on the first batch adds a one-off delay;
  steady-state pace is the `dt=` after epoch 0.
- **Python version** — must be 3.12.x (`requires-python >=3.12,<3.13`). `uv` enforces it.
