# NAM WaveNet FiLM Gain Conditioning — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Train one NAM WaveNet that follows the BA283 gain knob (replacing the
fixed-gain-0.5 `model.nam`), by feeding the gain as a 2nd input channel driving
FiLM — Python train + eval only, no `nam`-package fork, Rust plugin deferred.

**Architecture:** WaveNet with `input_size=2` (ch0 audio, ch1 broadcast gain),
`condition_dsp=None` so the condition `c` = the 2-ch input, FiLM hooks active so
the 1×1 FiLM conv learns scale+shift from `[audio, gain]`. Train on a denser
SPICE-reamp gain grid of the T3K sweep, reusing `train/dataset.py` (which already
yields a `[audio, gain]` 2-column input) + `train/loss.py`. Export to `.nam`;
extend `eval/nam_vs_acme/` to sweep gains.

**Tech Stack:** Python 3.12, PyTorch, `neural-amp-modeler` 0.13.0 (installed as the
`eval` extra), `data_gen` SPICE reamp (ngspice/PySpice), soxr, the `ba283-nam-lstm`
checkout (branch `feat/nam-lstm-backend`) for `train/` + `scratch/nam_sweep/`.

> **Where to run:** Python training/data lives in the `ba283-nam-lstm` checkout
> (`~/Git/ba283-nam-lstm`, has `train/` + the T3K data). The eval harness lives in
> `ba283-vst3` (`eval/nam_vs_acme/`, needs `data_gen` + ACME). Run each command from
> the repo that owns the module; the two share the same `feat/nam-lstm-backend`
> code but separate data trees.

> **Key gotcha proven during design:** `nam.models.wavenet._wavenet.WaveNet.init_from_config(cfg)`
> MIS-BUILDS the model (receptive_field collapses to 1, head produces `None`) even on
> the real config. Only `nam.models.init_from_nam(submodel_dict)` constructs it
> correctly (RF=6347). So **build/modify models via `init_from_nam` + in-place edits,
> never by hand-authoring the inner config.**

---

## Task 1: Build a valid 2-channel FiLM WaveNet (round-trip spike)

This de-risks the whole effort: prove a 2-ch-input, FiLM-active WaveNet builds,
forwards, exports to a `.nam` dict, and reloads via `init_from_nam` bit-identically.

**Files:**
- Create: `~/Git/ba283-nam-lstm/scratch/nam_sweep/wavenet_cond.py`
- Test: `~/Git/ba283-nam-lstm/scratch/nam_sweep/test_wavenet_cond.py`

**Step 1: Write the failing test**

```python
# test_wavenet_cond.py — run from the ba283-vst3 repo root with --extra eval
import torch
from scratch.nam_sweep.wavenet_cond import build_cond_wavenet, to_nam_dict
from nam.models import init_from_nam


def test_2ch_film_wavenet_roundtrips():
    m = build_cond_wavenet()              # input_size=2, FiLM active
    rf = m.net.receptive_field if hasattr(m, "net") else m.receptive_field
    assert rf > 1000, f"receptive field collapsed: {rf}"
    x = torch.zeros(1, 2, rf + 256)       # (B, C=2, L)
    y = m(x)
    assert y.shape[-1] == x.shape[-1] or y.shape[-1] == x.shape[-1] - (rf - 1)

    d = to_nam_dict(m, sample_rate=48000)
    assert d["architecture"] == "WaveNet"
    assert d["config"]["layers"][0]["input_size"] == 2
    m2 = init_from_nam(d)                  # MUST reload (the proven entrypoint)
    y2 = m2(x)
    assert torch.allclose(y, y2, atol=1e-6), "reload not bit-identical"
```

**Step 2: Run it, expect failure** (`build_cond_wavenet` undefined).
Run: `cd ~/Git/ba283-nam-lstm && uv run --project ~/Git/ba283-vst3 --extra eval python -m pytest scratch/nam_sweep/test_wavenet_cond.py -v`

**Step 3: Implement `wavenet_cond.py`**

Derive a working model from the shipped `model.nam` submodel (the proven-good
config) and lift it to 2 input/condition channels with FiLM on. Because
`init_from_config` mis-builds, the reliable construction is: take the submodel
dict, edit `config` (input_size/condition_size → 2; activate a FiLM hook on each
layer; widen the first input conv), then `init_from_nam`. If the in-place conv
resize fights weight import, fall back to building from the edited dict with
**freshly initialised weights** (training will fill them) rather than importing
the gain-1 weights.

```python
import copy, json
from pathlib import Path
from nam.models import init_from_nam

REF_NAM = Path.home() / "Git/ba283-nam-lstm/exported_models/model.nam"

def _cond_config():
    sm = json.loads(REF_NAM.read_text())["config"]["submodels"][0]["model"]
    cfg = copy.deepcopy(sm)                     # {version, architecture, metadata, config, weights}
    for L in cfg["config"]["layers"]:
        L["input_size"] = 2
        L["condition_size"] = 2
        L["conv_pre_film"] = {"active": True, "shift": True, "groups": 1}
    cfg.pop("weights", None)                    # fresh init; shapes changed
    return cfg

def build_cond_wavenet():
    cfg = _cond_config()
    # init_from_nam requires a "weights" array; supply a correctly-sized zero/
    # random vector. Determine the expected length by building once with random
    # weights via the package's own initialiser (see Step-3 spike below) or by
    # constructing the module and reading export_weights().numel().
    ...  # resolve here; the test pins the contract

def to_nam_dict(model, sample_rate=48000):
    return {"version": "0.7.0", "architecture": "WaveNet",
            "sample_rate": sample_rate, "metadata": {},
            "config": model.export_config() if hasattr(model, "export_config")
                      else model.net.export_config(),
            "weights": (model.export_weights() if hasattr(model, "export_weights")
                        else model.net.export_weights()).tolist()}
```

> Spike note: if `init_from_nam` needs a pre-sized weights array, build the module,
> call `export_weights().numel()` to get the count, then re-init with a zero array
> of that length. Capture the working recipe in `wavenet_cond.py`.

**Step 4: Run the test, expect PASS.**

**Step 5: Commit** `git add scratch/nam_sweep/wavenet_cond.py scratch/nam_sweep/test_wavenet_cond.py && git commit -m "feat(nam): 2-ch FiLM WaveNet builder + round-trip test"`

---

## Task 2: Densify the SPICE-reamp gain grid

**Files:**
- Modify: `~/Git/ba283-nam-lstm/scratch/nam_sweep/gen.py:14` (`GAINS`)
- Run: the reamp + the dataset rebuild

**Step 1:** Change `GAINS = [0.0, 0.5, 1.0]` →
`GAINS = [0.0, 0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875, 1.0]` (9 points).

**Step 2: Reamp.** Run: `cd ~/Git/ba283-nam-lstm && uv run --project ~/Git/ba283-vst3 python scratch/nam_sweep/gen.py`
Expected: one `OK` line per gain; mind the ngspice ~2.1 GB ceiling (one gain per
child via existing `maxtasksperchild=1`; 60 s clip stays under). On overflow, run
gains in two batches.

**Step 3: Rebuild the 48 kHz dataset + manifests** from the new wet set (reuse
whatever produced `ds48/` originally — the resample + manifest/`train,val,test`
split step). **Assert dry SR == wet SR == 48000** for every new clip (guards the
*ba283-dry-resample-bug* class). Verify `manifest.csv` lists all 9 `gain_value`s.

**Step 4: Commit** the `gen.py` change + new manifests (NOT the large wavs if they
are git-ignored; check `.gitignore`). `git commit -m "data(nam): densify T3K reamp to 9 gains"`

---

## Task 3: Confirm the dataset yields the 2-ch [audio, gain] input

`train/dataset.py` already yields a 2-column `[audio, gain]` window (the LSTM
splits it). Confirm the shape contract so the WaveNet can consume it directly.

**Files:** Test: `~/Git/ba283-nam-lstm/train/test_dataset_2ch.py`

**Step 1: Test** — load the new manifest, pull one batch, assert the input tensor
has a channel/last dim of 2 (`[audio, gain]`), gain is constant within a window
and matches the manifest `gain_value` (after any normalisation the dataset
applies), and target is finite. Pin the exact axis order (`(B, T, 2)` vs `(B, 2, T)`)
the dataset returns — the training loop will transpose to WaveNet `(B, 2, T)`.

**Step 2–4:** Run; if the dataset already satisfies it, the test passes with no
code change (document the shape). If gain is delivered separately, add a tiny
adapter in the training loop (Task 4), not in the dataset.

**Step 5: Commit** the test.

---

## Task 4: Thin WaveNet training loop

`train/train.py` is LSTM-specific (stateful `h0/c0`, `model(x)->(pred,state)`,
burn-in). A WaveNet is stateless with receptive-field latency, so it gets its own
loop — but reuses the dataset, the loss, and the finite-guard discipline.

**Files:**
- Create: `~/Git/ba283-nam-lstm/scratch/nam_sweep/train_wavenet.py`
- Test: `~/Git/ba283-nam-lstm/scratch/nam_sweep/test_train_wavenet.py`

**Step 1: Test the loss-region helper** (the only unit-testable piece): given a
WaveNet with `pad_start=True` (output length == input length), the first
`receptive_field - 1` samples are warmup and must be excluded from the loss, like
`train.py`'s burn-in. Test a helper `loss_region(pred, target, rf)` trims both by
`rf-1` and returns equal-length tensors.

**Step 2: Run, expect fail.**

**Step 3: Implement the loop.** Per batch:
1. Build the 2-ch input `x = stack([audio, gain_broadcast])` shaped `(B, 2, T)`.
2. `pred = model(x)` with `pad_start=True` → `(B, T)` (squeeze channel).
3. `pred_r, tgt_r = loss_region(pred, target, rf)`.
4. `loss = composite_loss(pred_r, tgt_r, ...)` from `train/loss.py` (pre-emphasis
   ESR + MSE; `esr` eps already guards near-silent windows, and `dataset.py`'s
   `MIN_TARGET_ENERGY` filter drops the worst).
5. Reuse `train.py`'s non-finite loss/grad guard (skip the step, zero grads) and
   `clip_grad_norm_`.
6. Adam + CosineAnnealingLR (mirror `train.py:160-161`), TensorBoard scalar,
   atomic best-checkpoint save (`train.py:_atomic_save`).

**Step 4:** Smoke-train a few hundred steps on the new dataset; assert train loss
is finite and decreasing. Run on MPS/CPU/CUDA via a `--device` flag.

**Step 5: Commit** `git commit -m "feat(nam): WaveNet gain-conditioned training loop"`

---

## Task 5: Export the trained model to `.nam`

**Files:** Modify: `scratch/nam_sweep/train_wavenet.py` (add `--export`) reusing
`to_nam_dict` from Task 1.

**Step 1: Test** — after a tiny train run, export `model.cond.nam`, reload via
`init_from_nam`, and assert reload runs on a 2-ch input and matches the in-memory
model (bit-identical, same gate as Task 1).

**Step 3:** Write the export: `to_nam_dict(model, 48000)` → JSON to
`exported_models/model.cond.nam`. Keep `input_scale`/`OUTPUT_REF_V` provenance in
the metadata (the eval + any future plugin need the de-norm scale).

**Step 5: Commit** `git commit -m "feat(nam): export gain-conditioned .nam"`

---

## Task 6: Extend `eval/nam_vs_acme` to sweep gains

**Files:**
- Modify: `~/Git/ba283-vst3/eval/nam_vs_acme/run_nam.py` (accept a gain, feed ch1)
- Modify: `~/Git/ba283-vst3/eval/nam_vs_acme/compare.py` (sweep gains, 3-way per gain)
- Test: `~/Git/ba283-vst3/eval/nam_vs_acme/test_cond_runner.py`

**Step 1: Test** `run_nam` with a conditioned model: `NamModel(...).run(dry, gain=g)`
builds the 2-ch input `[dry, g·ones]` and returns finite output of `len(dry)`;
different `g` yields different output (conditioning is live).

**Step 3: Implement.**
- `run_nam.NamModel`: detect `input_size==2` (conditioned); `run(dry, gain)` stacks
  `[dry, full(len(dry), gain)]` → `(1, 2, L)`; for the legacy 1-ch model, ignore
  `gain` (back-compat). Keep SlimmableContainer routing.
- `compare.py`: add `--gains 0.0,0.125,...,1.0`; for each gain, `run_nam(dry, gain)`,
  `run_acme` at that gain, score the existing 3-way (lag-align / 20 Hz HP /
  level-match). Reuse the SPICE target per gain from Task 2's `ds48/`.
- Plots/summary per gain; a `per_gain` ESR table + a worst-gain gate.

**Step 4:** Run the full sweep:
`cd ~/Git/ba283-vst3 && uv run --extra eval python -m eval.nam_vs_acme.compare --gains 0.0,0.25,0.5,0.75,1.0 --bias 0.77 --nam ~/Git/ba283-nam-lstm/exported_models/model.cond.nam --max-seconds 30`

**Step 5: Commit** `git commit -m "feat(eval): gain sweep for conditioned NAM"`

---

## Task 7: End-to-end verification + verdict

- Run Task 6's sweep across all trained gains.
- **Gate:** worst-gain matched ESR ≤ −20 dB (vs the fixed model's −25.5 dB at 0.5),
  FR within ±1 dB midband, coherence ≈ 1 at every gain. If a gain is far worse, it
  flags under-conditioning (densify data / activate more FiLM hooks / longer train).
- Record the result table in `eval/nam_vs_acme/README.md` and update memory
  `ba283-nam-acme-eval` with the conditioned-model outcome.
- **Commit** the README/summary.

---

## Deferred (NOT this plan)

Rust plugin port: 2-ch-input WaveNet inference + `.nam` load + FiLM in the VST3.
The `.nam` carries `input_size=2`; the plugin's NN backend would need a WaveNet
runtime (it currently runs only the LSTM). Tracked for a later effort.

## Risk register

1. **2-ch FiLM construction** (Task 1) — `init_from_config` mis-builds; must go via
   `init_from_nam`. Highest risk; front-loaded as the spike.
2. **`.nam` export fidelity** for the 2-ch/FiLM variant (Tasks 1, 5 gates).
3. **ngspice RAM** during the denser reamp (Task 2) — batch gains if needed.
4. **Under-conditioning from sparse/short data** — T3K is one 60 s sweep; if ESR is
   poor at off-center gains, fall back to the richer `gdrive:ba283_spice_gain`
   452-clip set the LSTM used (*ba283-spice-valset*).
5. **Future plugin loader** must accept 2-ch input (deferred).
