# Running the BA283 stepped-gain NAM pipeline on an M4 Mac

Apple Silicon works end-to-end: Julia/ACME is ARM-native, PyTorch trains on **MPS**,
ngspice fallback via Homebrew. No CUDA.

Repos:
- **ba283-vst3** (`feat/stepped-retrain-wavenet`) — `data_gen/`, `eval/`, Rust plugin.
- **ba283-nam-lstm** (`feat/stepped-dual-cond-3ch`) — WaveNet training stack (`scratch/nam_sweep/`).

Run each script from its own repo root.

## 0. One-time toolchain

```bash
# Julia (ARM) — run_acme.py looks in ~/.juliaup/bin/julia
curl -fsSL https://install.julialang.org | sh        # juliaup
# ACME.jl + deps for eval/acme_vs_spice/ba283_acme.jl
julia --project=eval/acme_vs_spice -e 'using Pkg; Pkg.instantiate()'

# ngspice (SPICE os=2 fallback renderer)
brew install ngspice

# Python env (uv). torch arm64 wheel ships MPS — no extra index needed.
uv venv && uv pip install torch numpy soxr scipy
```

## 1. Generate + render data (ACME 4×, SPICE os=2 fallback)

`run_acme_batch.py` builds the dry 2-D grid internally (5 `STEPPED_RKJ_OHMS` detents ×
`STEPPED_AMP_DB` input-gain sweep), renders each via ACME upsampled to `os×sr`, decimates
with soxr, falls back to ngspice os=2 on any ACME divergence (all fallbacks logged).
Writes `g_i.json` + `render_summary.json`.

```bash
# in ba283-vst3
uv run python -m data_gen.run_acme_batch \
  --out-dir data/wet_raw_stepped \
  --dry-dir data/dry_stepped \
  --os 4 --gi-tol 0.15
```

Hard-fails if measured `1/G_i` drifts > 15% vs WDF `STEPPED_MAKEUP` (drift gate). If it
trips, that's a real disagreement — investigate, don't bump `--gi-tol`.

## 2. Resample to training grid + wire level-norm

`--stepped` divides each wet target by its position `G_i` (keyed on filename rkj) so each
detent lands ~unity; tags `input_gain_db` into the manifest.

```bash
uv run python -m data_gen.resample \
  --raw-dir data/wet_raw_stepped \
  --dry-dir data/dry_stepped \
  --out-dir data/wet_stepped \
  --manifest data/manifest_stepped.csv \
  --gi-json data/wet_raw_stepped/g_i.json \
  --stepped
```

Produces `manifest_stepped.{train,val,test}.csv`. Assert dry SR == wet SR.

## 3. Train 3-channel FiLM WaveNet (MPS)

```bash
# in ba283-nam-lstm; manifests copied/symlinked from step 2
uv run python scratch/nam_sweep/train_wavenet.py \
  --train-manifest data/manifest_stepped.train.csv \
  --val-manifest   data/manifest_stepped.val.csv \
  --device mps \
  --epochs 200 --batch 16 --lr 4e-3
```

- `--device auto` also resolves to `mps` on M4. Keep `--workers 0` (default) — MPS + fork
  workers is flaky.
- `pin_memory` auto-off on non-CUDA (already handled).
- Dataset auto-detects 3-channel when `input_gain_db` column present (`dual_cond`); builds
  `[audio, circuit_gain_norm, input_gain_norm]`. `build_cond_wavenet(n_inputs=3)`.
- Smoke first: `--device cpu --epochs 1 --smoke` (finite-loss check, seconds).
- Export writes `exported_models/model.cond.nam` with
  `condition.channels=["audio","circuit_gain","input_gain"]`.

> MPS note: if you hit an unimplemented op, fall back with
> `PYTORCH_ENABLE_MPS_FALLBACK=1 uv run python …`. Or train on CPU — these WaveNets are
> small (RF 6347, ~2.4k weights); CPU on M4 is viable for a short run.

## 4. Eval gate (vs ACME/SPICE)

```bash
# in ba283-vst3
uv run python -m eval.nam_vs_acme.compare --stepped \
  --nam path/to/model.cond.nam \
  --hot-amps 1,4,16 --bias 0.77
```

Pass = worst (position × input_gain) ESR ≲ −28 dB. Feeds `rkj_to_gain_norm(rkj)` +
normalized input_gain as the two cond channels (the old idx-as-gain bug is fixed).

## 5. Ship to Rust

1. Copy `model.cond.nam` → `plugin/src/` embed path.
2. Flip `const COND: usize = 2` → `3` in `plugin/src/wavenet.rs` (single flip-point; loops
   already `0..COND`).
3. Recalibrate `NN_GAIN_CALIB` / `nn_norm_trim` in `lib.rs` — after level-normed training
   each detent ≈ unity, so the old huge inverse-gain LUT is wrong; derive the 5-entry makeup
   from measured residual (or unify with WDF `GAIN_STEPS[].wdf_makeup` if they agree).
4. Build (Rust on M4 is native arm64):
   ```bash
   cargo test          # golden parity
   cargo xtask bundle
   ```
   sha256-verify the cdylib into `~/Library/Audio/Plug-Ins/VST3/` (mac path), then DAW A/B
   across detents × input_gain × levelNorm on/off.

## M4 gotchas

- **No CUDA** — never pass `--device cuda`; `mps`/`auto`/`cpu` only.
- **VST3 install dir** differs from Linux: `~/Library/Audio/Plug-Ins/VST3/` (not `~/.vst3`).
  Watch the stale-install trap — bundle from the **same** worktree you fixed.
- **Disk**: full ACME matrix + torch is multi-GB; the render dominates. Short clips,
  peak-normed base already minimize it.
- First `julia` ACME call JIT-compiles (~30–60 s); subsequent calls are fast.
