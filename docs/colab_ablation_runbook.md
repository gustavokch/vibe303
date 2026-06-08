# BA283 Ablation Studies on Colab — Runbook

Operator runbook for running the fidelity-ablation matrix end-to-end: build the
subsets locally, train the cells on Colab GPU, and read the verdict.

> For *what each cell tests and why* (the `val_esr ≈ 0.097` wall, the decision
> rule), see the design companion:
> [Fidelity Ablation Matrix (Design)](plans/2026-06-06-ablation-matrix-design.md).

## 1. Overview

The notebook [`colab/ba283_ablation_colab.ipynb`](../colab/ba283_ablation_colab.ipynb)
runs the four Core-4 training cells **on one GPU venue** so every cell is scored
against a single baseline (`B0`):

| cell | axis | change vs B0 |
|---|---|---|
| **B0** | — | reference (hidden 32, 44.1 kHz) |
| **C1** | capacity | hidden 32 → **64** |
| **C4** | aliasing | render + train **88.2 kHz** (eval downsampled to 44.1 kHz) |
| **C5** | gain density | **9-pt** grid + held-out-gain eval |

Running all four on Colab (not the design's local/Colab split) keeps absolute ESR
comparable across every cell. **C7** (target floor) is not a training run and stays
local — see §7.

Output: a markdown verdict table + per-cell JSON, `rclone copy`d to
`gdrive:ba283_ablation/results/` (so they survive a runtime kill).

## 2. Prerequisites

- A Colab runtime set to **T4 GPU** (Runtime → Change runtime type → T4 GPU).
- An `rclone.conf` holding the Drive remote token (find it with `rclone config file`).
- A GitHub token with read access to the repo.
- The ablation subsets already uploaded to Drive under `ba283_ablation/`:

| cell | gdrive subdir | dry/wet SR | manifests |
|---|---|---|---|
| B0 / C1 | `ba283_ablation/base44k/` | 44100 | `train.csv`, `val.csv`, `manifest.csv` |
| C4 | `ba283_ablation/c4_88k/` | 88200 | `train.csv`, `val.csv`, `manifest.csv` |
| C5 | `ba283_ablation/c5_9pt/` | 44100 | `train.csv` (5 gains), `val.csv` (9 gains), `manifest.csv` |

`manifest.csv` must be present in each subdir — `train.stage.stage_rclone` asserts
it so a partial copy is caught before training. C4's `val.csv` is at **88.2 kHz**
(model-native); the harness downsamples both prediction and target to 44.1 kHz
before scoring. C5's `val.csv` carries all 9 gains so the held-out-gain ESR can
filter to the 4 unseen gains `{0.125, 0.375, 0.625, 0.875}`.

Steps 3–4 build and upload those subsets (run locally, once). If they already
exist on Drive, skip to §5.

## 3. Step 1 — Build the subsets (local)

This builds on the data-gen pipeline in
[`docs/training_runbook.md`](training_runbook.md) §3–5. The ablation uses a small
**representative subset** (all gains × a few stimulus kinds, trimmed) so each cell
finishes in ~45 min.

**3a. Pick representative stems.** From a full dataset manifest, keep all gains for
one stem of each kind, trimmed to ~20 s, with `eval.ablation.subset.build_subset`:

```bash
uv run python - <<'PY'
import pandas as pd
from eval.ablation.subset import build_subset
df = pd.read_csv("data/manifest.csv")
sub = build_subset(df, kinds=("music", "pink_burst", "sweep", "multisine"), max_trim_s=20.0)
sub.to_csv("data/ablation/subset.csv", index=False)
print(len(sub), "rows;", sorted(set(sub.stim_kind)), "gains", sorted(set(sub.gain_value)))
PY
```

**3b. B0/C1 subset (44.1 kHz).** Resample the existing 96 kHz raw SPICE `.npz`
renders for the subset stems to 44.1 kHz. `data_gen/resample.py` takes
`--target-sr` (default 96000) and writes `train/val/test.csv` alongside the
manifest — no `run_spice` change is needed:

```bash
uv run python -m data_gen.resample \
  --raw-dir data/wet_raw --out-dir data/ablation/base44k/wet \
  --dry-dir data/ablation/base44k/dry \
  --manifest data/ablation/base44k/manifest.csv --target-sr 44100
```

**3c. C4 subset (88.2 kHz).** Same raw renders, `--target-sr 88200`, so the model
trains and evals on native-rate input:

```bash
uv run python -m data_gen.resample \
  --raw-dir data/wet_raw --out-dir data/ablation/c4_88k/wet \
  --dry-dir data/ablation/c4_88k/dry \
  --manifest data/ablation/c4_88k/manifest.csv --target-sr 88200
```

**3d. C5 subset (9-pt gain grid).** Re-render the subset stems across **9** gains
(the dataset stores gains normalized in `[0,1]`; `run_spice --gains` wants K↔J
ohms, so convert with `data_gen.netlist.gain_norm_to_rkj`), then resample to
44.1 kHz:

```bash
uv run python - <<'PY'
from data_gen.netlist import gain_norm_to_rkj
grid = [0.0, 0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875, 1.0]
print(",".join(f"{gain_norm_to_rkj(g):.1f}" for g in grid))
PY
# feed that ohms list to the gain sweep at the dataset's fixed bias (0.77):
uv run python -m data_gen.run_spice --manifest data/ablation/subset_stems/ \
  --gains <ohms-from-above> --fixed-bias 0.77
uv run python -m data_gen.resample \
  --raw-dir data/wet_raw --out-dir data/ablation/c5_9pt/wet \
  --dry-dir data/ablation/c5_9pt/dry \
  --manifest data/ablation/c5_9pt/manifest.csv --target-sr 44100
```

Then split the 9-pt manifest: `train.csv` keeps the 5 trained gains
`{0, .25, .5, .75, 1}`, `val.csv` keeps **all 9** (so held-out-gain eval can run).

Each subdir needs `manifest.csv` + `train.csv`/`val.csv` before upload.

## 4. Step 2 — Upload subsets to Drive

```bash
for d in base44k c4_88k c5_9pt; do
  rclone copy data/ablation/$d gdrive:ba283_ablation/$d -P
done
```

(Use your own remote name in place of `gdrive:`.)

## 5. Step 3 — Run the notebook

Open [`colab/ba283_ablation_colab.ipynb`](../colab/ba283_ablation_colab.ipynb),
set the runtime to T4 GPU, and run top to bottom:

1. **Install rclone + upload `rclone.conf`** — sets `REMOTE`.
2. **Clone repo + deps + GPU check** — paste the GitHub token; asserts CUDA.
3. **Config** — fixed controls (seed `745603`, 60 epochs, patience 15) and the
   `SUBSETS` map.
4. **Stage subsets** — `rclone copy` each to `/content/<key>` and remap manifests.
5–8. **B0 / C1 / C4 / C5** — one training run each (~45 min/cell).
9. **Score + verdict** — builds the table and `rclone copy`s
   `ablation_results.json` + `ablation_table.md` to `ba283_ablation/results/`.

Each cell trains independently, so if the runtime is killed mid-run, re-run from
the last unfinished training cell; finished results are already on Drive.

## 6. Step 4 — Read the verdict

The final cell prints a table (from `eval/ablation/report.py`):

```
| cell | val_esr | baseline | ΔdB | held-out-gain | explains | notes |
```

- **ΔdB** — ESR change vs B0; negative is better. A cell **explains** the gap
  (✅) when it improves ESR by **≥ 3 dB** (a halving) OR closes **≥ 50%** of the
  baseline→target distance.
- **held-out-gain** — only populated for C5 (B0/C1/C4 val gains are all trained,
  so it shows `—`). A held-out-gain ESR much worse than `val_esr` means FiLM is
  interpolating poorly between trained gains.
- **C4** also carries an `aliasing_high_band` field in its JSON: high-band error
  vs the 44.1 kHz reference. Lower than B0 ⇒ aliasing was real.

Winning axes graduate to the full redesign plan; the rest are dropped.

## 7. Step 5 — C7 target floor (local)

C7 needs ngspice + Julia (ACME) + the WDF harness, so it runs locally, not in the
notebook. It estimates the achievable ESR **floor** — the ESR of the SPICE target
itself vs independent circuit models:

```bash
uv run python -m eval.ablation.target_floor --subset data/ablation/subset.csv --bias 0.77
```

**Gate:** if the summary reports `target_bound: true` (ACME/WDF-vs-SPICE median
ESR ≳ 0.05, near the 0.097 neural wall), the fidelity ceiling is the **SPICE
target**, not the model — stop and redesign data-gen (topology / oversampling)
before chasing C1/C4/C5. (A prior full-grid run measured the floor at ≈ −31 dB,
**not** target-bound — but confirm on the actual subset before the final verdict.)

## 8. Step 6 — Confirm the winner

Re-run the winning axis on **2–3 seeds** (e.g. `745603` plus two others) to
separate signal from seed noise before it feeds the redesign plan. Change
`--seed` in the relevant training cell and re-run; the eval cell overwrites the
result JSON, so save each seed's table separately.

## 9. Troubleshooting

- **`manifest.csv missing under … after copy`** — the rclone copy was partial or
  the subdir lacks `manifest.csv`. Re-run the upload (§4); confirm the file exists
  on Drive.
- **`held_out_gain_esr` is `None`** — expected for B0/C1/C4 (their val gains are
  all trained). Only C5's 9-pt val produces a held-out value.
- **Runtime killed mid-run** — finished cells' checkpoints are local
  (`ckpt_<cell>/`); re-run from the unfinished training cell. Final results are on
  Drive once cell 9 completes.
- **`No GPU: set Runtime type to T4 GPU`** — the GPU assert in cell 2 fired; set
  the runtime type and re-run from the top.
- **`remap produced missing path`** — a manifest references a file the copy did
  not bring down; re-stage that subset (§4 then cell 4).
