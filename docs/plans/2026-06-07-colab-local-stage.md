# Colab local-stage dataset Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Stage the entire dataset to Colab local disk once, upfront, so training reads only local files — no Drive/rclone/FUSE reads during the train phase.

**Architecture:** New pure-python `train/stage.py` with two staging front-ends (`stage_rclone` = `rclone copy` down, `stage_zip` = unzip) that produce one local layout, plus `remap_manifests` that rewrites audio paths to absolute local paths. Both Colab notebooks call these instead of mounting Drive / relying on relative-path resolution.

**Tech Stack:** Python 3.12, pandas, soundfile, pytest, uv. rclone (Colab runtime only).

Design doc: `docs/plans/2026-06-07-colab-local-stage-design.md`

---

## Conventions

- Run everything with `uv run` (e.g. `uv run pytest ...`).
- Manifest audio columns are `dry_path`, `wet_path`. The dataset layout keeps audio under an immediate parent folder (`dry/`, `wet/`); remap preserves that parent name: `<dataset_dir>/<parent-name>/<basename>`.
- `train/stage.py` must import **without** torch/Colab — pandas + stdlib only — so it tests on any machine and ships in both the repo clone and the `make_bundle.sh` zip (which already zips `train/`).

---

### Task 1: `remap_manifests` — rewrite manifest paths to local absolute

**Files:**
- Create: `train/stage.py`
- Test: `tests/train/test_stage.py`

**Step 1: Write the failing test**

```python
# tests/train/test_stage.py
import subprocess
import zipfile
from pathlib import Path

import numpy as np
import pandas as pd
import soundfile as sf

from train.stage import remap_manifests, stage_rclone, stage_zip


def _make_dataset(root: Path) -> Path:
    """Build a tiny dataset: dry/ + wet/ wavs, manifests with ABSOLUTE paths."""
    (root / "dry").mkdir(parents=True)
    (root / "wet").mkdir(parents=True)
    sr = 8000
    tone = np.sin(np.linspace(0, 6.28, 4096)).astype(np.float32)
    rows = []
    for tag in ("a", "b"):
        dry = root / "dry" / f"sweep{tag}.wav"
        wet = root / "wet" / f"sweep{tag}_g0.wav"
        sf.write(dry, tone, sr)
        sf.write(wet, tone * 0.5, sr)
        rows.append({"dry_path": str(dry), "wet_path": str(wet),
                     "gain_value": 0.0, "duration_s": 0.5, "stim_kind": "sweep"})
    df = pd.DataFrame(rows)
    for name in ("train", "val", "test"):
        df.to_csv(root / f"{name}.csv", index=False)
    (root / "manifest.csv").write_text("ok\n")
    return root


def test_remap_rewrites_to_local_absolute_and_files_exist(tmp_path):
    ds = _make_dataset(tmp_path / "dataset")
    out = remap_manifests(ds, out_dir=tmp_path / "content")

    assert set(out) == {"train", "val", "test"}
    df = pd.read_csv(out["train"])
    wet0 = df.iloc[0]["wet_path"]
    assert wet0 == str(ds / "wet" / "sweepa_g0.wav")
    assert Path(wet0).exists()
    assert Path(df.iloc[0]["dry_path"]).exists()
```

**Step 2: Run test to verify it fails**

Run: `uv run pytest tests/train/test_stage.py::test_remap_rewrites_to_local_absolute_and_files_exist -v`
Expected: FAIL — `ModuleNotFoundError: No module named 'train.stage'`

**Step 3: Write minimal implementation**

```python
# train/stage.py
"""Stage a BA283 dataset onto local disk before training (Colab helper).

Two front-ends produce an identical local layout, after which manifests are
remapped to absolute local paths so the training loader never reads through a
network / FUSE layer. Pure-python (pandas + stdlib): no torch / Colab imports,
so it unit-tests anywhere and ships in the make_bundle zip.
"""
from __future__ import annotations

import subprocess
import zipfile
from pathlib import Path

import pandas as pd

_PATH_COLUMNS = ("dry_path", "wet_path")


def remap_manifests(
    dataset_dir: str | Path,
    names: tuple[str, ...] = ("train", "val", "test"),
    out_dir: str | Path = "/content",
) -> dict[str, str]:
    """Rewrite manifest audio paths to absolute local paths under ``dataset_dir``.

    Each ``dry_path`` / ``wet_path`` becomes
    ``<dataset_dir>/<immediate-parent-name>/<basename>`` (e.g. ``dry`` / ``wet``),
    written to ``out_dir``. Asserts the row-0 ``wet_path`` exists so a bad/partial
    copy is caught before a long training run. Returns name -> written CSV path.
    """
    dataset_dir = Path(dataset_dir)
    out_dir = Path(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    out: dict[str, str] = {}
    for name in names:
        src = dataset_dir / f"{name}.csv"
        if not src.exists():
            continue
        df = pd.read_csv(src)
        for col in _PATH_COLUMNS:
            df[col] = df[col].map(
                lambda x: str(dataset_dir / Path(x).parent.name / Path(x).name)
            )
        first = df.iloc[0]["wet_path"]
        if not Path(first).exists():
            raise FileNotFoundError(f"remap produced missing path: {first}")
        target = out_dir / f"{name}.csv"
        df.to_csv(target, index=False)
        out[name] = str(target)
    if not out:
        raise FileNotFoundError(f"no manifests {names} found under {dataset_dir}")
    return out
```

**Step 4: Run test to verify it passes**

Run: `uv run pytest tests/train/test_stage.py::test_remap_rewrites_to_local_absolute_and_files_exist -v`
Expected: PASS

**Step 5: Commit**

```bash
git add train/stage.py tests/train/test_stage.py
git commit -m "feat(stage): remap_manifests rewrites manifest paths to local absolute"
```

---

### Task 2: `stage_zip` — unzip bundle to local, return data root

**Files:**
- Modify: `train/stage.py`
- Test: `tests/train/test_stage.py`

**Step 1: Write the failing test**

```python
def test_stage_zip_extracts_and_finds_data_root(tmp_path):
    ds = _make_dataset(tmp_path / "src")
    # bundle stores paths under a leading data/ prefix, like make_bundle.sh
    zip_path = tmp_path / "bundle.zip"
    with zipfile.ZipFile(zip_path, "w") as z:
        for p in ds.rglob("*"):
            if p.is_file():
                z.write(p, Path("data") / p.relative_to(ds))

    data_root = stage_zip(zip_path, dest=tmp_path / "content" / "dataset")
    assert (data_root / "train.csv").exists()
    assert (data_root / "dry").is_dir()
    out = remap_manifests(data_root, out_dir=tmp_path / "content")
    assert Path(pd.read_csv(out["train"]).iloc[0]["wet_path"]).exists()
```

**Step 2: Run test to verify it fails**

Run: `uv run pytest tests/train/test_stage.py::test_stage_zip_extracts_and_finds_data_root -v`
Expected: FAIL — `ImportError: cannot import name 'stage_zip'` (already imported at top; collection error)

**Step 3: Write minimal implementation**

Add to `train/stage.py`:

```python
def stage_zip(zip_path: str | Path, dest: str | Path = "/content/dataset") -> Path:
    """Extract an uploaded bundle zip to ``dest``; return the dir holding train.csv.

    The bundle stores entries under a leading ``data/`` prefix, so the data root
    is wherever ``train.csv`` lands, not ``dest`` itself.
    """
    dest = Path(dest)
    dest.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(zip_path) as z:
        z.extractall(dest)
    hits = list(dest.rglob("train.csv"))
    if not hits:
        raise FileNotFoundError(f"train.csv not found under {dest} after unzip")
    return hits[0].parent
```

**Step 4: Run test to verify it passes**

Run: `uv run pytest tests/train/test_stage.py::test_stage_zip_extracts_and_finds_data_root -v`
Expected: PASS

**Step 5: Commit**

```bash
git add train/stage.py tests/train/test_stage.py
git commit -m "feat(stage): stage_zip extracts bundle and locates data root"
```

---

### Task 3: `stage_rclone` — copy (not mount) from remote to local

**Files:**
- Modify: `train/stage.py`
- Test: `tests/train/test_stage.py`

**Step 1: Write the failing test**

```python
def test_stage_rclone_copies_not_mounts(tmp_path, monkeypatch):
    ds = _make_dataset(tmp_path / "remote")
    dest = tmp_path / "content" / "dataset"
    calls = []

    def fake_run(argv, **kw):
        calls.append(argv)
        # emulate `rclone copy <remote> <dest>`: mirror files locally
        import shutil
        shutil.copytree(argv[2], argv[3], dirs_exist_ok=True)
        return subprocess.CompletedProcess(argv, 0, "", "")

    monkeypatch.setattr(subprocess, "run", fake_run)
    out_dir = stage_rclone(str(ds), dest=dest)

    assert out_dir == dest
    assert (dest / "manifest.csv").exists()
    argv = calls[0]
    assert argv[:2] == ["rclone", "copy"]
    assert "mount" not in argv  # copy, never mount


def test_stage_rclone_raises_on_failure(tmp_path, monkeypatch):
    def fake_run(argv, **kw):
        return subprocess.CompletedProcess(argv, 1, "", "boom")

    monkeypatch.setattr(subprocess, "run", fake_run)
    import pytest
    with pytest.raises(RuntimeError, match="rclone copy"):
        stage_rclone("remote:x", dest=tmp_path / "d")
```

**Step 2: Run test to verify it fails**

Run: `uv run pytest tests/train/test_stage.py -k stage_rclone -v`
Expected: FAIL — `ImportError: cannot import name 'stage_rclone'`

**Step 3: Write minimal implementation**

Add to `train/stage.py`:

```python
def stage_rclone(remote: str, dest: str | Path = "/content/dataset") -> Path:
    """Copy a dataset from an rclone ``remote`` to local ``dest`` (copy, NOT mount).

    No FUSE / VFS: every file lands on local disk before training reads it.
    Asserts ``manifest.csv`` is present afterwards so a partial copy is caught early.
    """
    dest = Path(dest)
    dest.mkdir(parents=True, exist_ok=True)
    proc = subprocess.run(
        ["rclone", "copy", remote, str(dest), "-P"],
        capture_output=True,
        text=True,
    )
    if proc.returncode:
        raise RuntimeError(
            f"rclone copy {remote} failed (exit {proc.returncode}):\n"
            f"{proc.stderr[-2000:]}"
        )
    if not (dest / "manifest.csv").exists():
        raise FileNotFoundError(f"manifest.csv missing under {dest} after copy")
    return dest
```

**Step 4: Run test to verify it passes**

Run: `uv run pytest tests/train/test_stage.py -k stage_rclone -v`
Expected: PASS (both tests)

**Step 5: Commit**

```bash
git add train/stage.py tests/train/test_stage.py
git commit -m "feat(stage): stage_rclone copies dataset to local disk (no mount)"
```

---

### Task 4: Integration — remapped manifest loads via `make_loader`

**Files:**
- Test: `tests/train/test_stage.py`

**Step 1: Write the failing test**

```python
def test_remapped_manifest_loads_in_make_loader(tmp_path):
    from train.dataset import make_loader

    ds = _make_dataset(tmp_path / "dataset")
    out = remap_manifests(ds, out_dir=tmp_path / "content")
    loader = make_loader(out["train"], batch_size=1, window=2048, stride=2048,
                         num_workers=0, shuffle=False, drop_last=False)
    x, y = next(iter(loader))
    assert x.shape[-1] == 2  # dry + conditioning column
    assert y.shape[-1] == 1
```

**Step 2: Run test to verify it fails (or confirm it passes)**

Run: `uv run pytest tests/train/test_stage.py::test_remapped_manifest_loads_in_make_loader -v`
Expected: PASS once paths resolve (absolute local). If FAIL, the remap parent-name convention is wrong — fix `remap_manifests`, not the test.

> Note: the loader requires a `gain_value` column; `_make_dataset` provides it. If your real dataset uses `bias_value`, that is a separate manifest-schema concern, out of scope here.

**Step 3: (only if failing) fix remap**

No new code expected; this task validates the contract end-to-end.

**Step 4: Run the full stage test file**

Run: `uv run pytest tests/train/test_stage.py -v`
Expected: all PASS

**Step 5: Commit**

```bash
git add tests/train/test_stage.py
git commit -m "test(stage): remapped manifest loads through make_loader"
```

---

### Task 5: Wire `ba283_ab_colab.ipynb` to stage-local

**Files:**
- Modify: `colab/ba283_ab_colab.ipynb`

Edit the notebook JSON (or via `jupyter`/script round-trip). Keep cell ordering; change contents:

**Step 1: Mount cell → copy cell.** Keep rclone install + `rclone.conf` upload + `REMOTE` discovery. Replace the `rclone mount` + VFS flags + `find|cat` warm block with:

```python
import sys; sys.path.insert(0, '/content/ba283')  # repo cloned in a later cell? see note
from train.stage import stage_rclone
DATA = str(stage_rclone(REMOTE + 'ba283_spice_gain', '/content/dataset'))
print('staged ->', DATA)
```

> Ordering note: `stage_rclone` lives in the repo, which is cloned in cell 4. Either (a) move the repo-clone cell **before** the copy cell, or (b) keep the copy cell as a raw `subprocess.run(['rclone','copy', REMOTE+'ba283_spice_gain', '/content/dataset','-P'], check=True)` and only use the module for `remap_manifests` after the clone. Prefer (a): clone first, then stage, so all logic flows through the tested module.

**Step 2: Remap cell.** Replace the inline pandas remap loop with:

```python
from train.stage import remap_manifests
M = remap_manifests('/content/dataset', out_dir='/content')
print(M)
```

Train cells already point at `/content/train.csv` / `/content/val.csv` — unchanged.

**Step 3: Result-upload cells.** Cell that writes exports to the mount (`DATA + '/ba283_*.json'`) must instead write to a local dir and `rclone copy` up (the FiLM cell already shows this pattern):

```python
import os, subprocess
os.makedirs('/content/ab_save', exist_ok=True)
export_rtneural_json(m, '/content/ab_save/ba283_%s.json'%cond, meta={...})
subprocess.run(['rclone','copy','/content/ab_save', REMOTE+'ba283_spice_gain/ab_save','-P'], check=True)
```

**Step 4: Validate notebook JSON parses.**

Run: `uv run python -c "import json; json.load(open('colab/ba283_ab_colab.ipynb')); print('ok')"`
Expected: `ok`

**Step 5: Commit**

```bash
git add colab/ba283_ab_colab.ipynb
git commit -m "feat(colab): ab notebook stages dataset to local disk (no FUSE mount)"
```

---

### Task 6: Wire `ba283_train_colab.ipynb` to stage-local

**Files:**
- Modify: `colab/ba283_train_colab.ipynb`

**Step 1: Unzip cell → stage_zip.** Replace the raw `!unzip ... -d /content/ba283` with a `stage_zip` call after the repo/bundle is available. Since this notebook uploads the bundle (which contains `train/`), unzip the bundle first to get `train/stage.py`, then call it:

```python
!unzip -q -o ba283_train_bundle.zip -d /content/ba283
%cd /content/ba283
!pip -q install soundfile pandas
from train.stage import stage_zip, remap_manifests
DATA = str(stage_zip('ba283_train_bundle.zip', '/content/dataset'))
M = remap_manifests(DATA, out_dir='/content')
print(M)
```

**Step 2: Train cell.** Point `--train-manifest` / `--val-manifest` at `/content/train.csv` / `/content/val.csv` (was `data/train.csv` / `data/val.csv`).

**Step 3: Validate notebook JSON parses.**

Run: `uv run python -c "import json; json.load(open('colab/ba283_train_colab.ipynb')); print('ok')"`
Expected: `ok`

**Step 4: Commit**

```bash
git add colab/ba283_train_colab.ipynb
git commit -m "feat(colab): train notebook stages bundle to local disk via stage_zip"
```

---

### Task 7: Confirm bundle ships `stage.py` + update runbook

**Files:**
- Verify: `colab/make_bundle.sh` (zips `train/` → already includes new `train/stage.py`)
- Modify: `docs/training_runbook.md`

**Step 1: Rebuild the bundle and assert stage.py is inside.**

Run:
```bash
bash colab/make_bundle.sh
uv run python -c "import zipfile; z=zipfile.ZipFile('colab/ba283_train_bundle.zip'); assert 'train/stage.py' in z.namelist(), z.namelist(); print('stage.py shipped')"
```
Expected: `stage.py shipped`

**Step 2: Update the runbook.** In `docs/training_runbook.md`, replace any "mount Drive / VFS cache" description of the Colab flow with the new "copy dataset to `/content/dataset`, train against local manifests" flow. Keep it to the changed steps.

**Step 3: Full test suite green.**

Run: `uv run pytest tests/train -v`
Expected: all PASS

**Step 4: Commit**

```bash
git add colab/ba283_train_bundle.zip docs/training_runbook.md
git commit -m "chore(colab): rebuild bundle with stage.py; document local-stage flow"
```

---

## Done criteria

- `uv run pytest tests/train` green, including `tests/train/test_stage.py`.
- Neither notebook performs an `rclone mount`; both copy/unzip the dataset to `/content/dataset` before any training cell.
- `train/stage.py` imports with pandas + stdlib only (no torch/Colab).
- Bundle zip contains `train/stage.py`.
