"""Stage a BA283 dataset onto local disk before training (Colab helper).

Two front-ends produce an identical local layout, after which manifests are
remapped to absolute local paths so the training loader never reads through a
network / FUSE layer. Pure-python (pandas + stdlib): no torch / Colab imports,
so it unit-tests anywhere and ships in the make_bundle zip.
"""
from __future__ import annotations

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


def stage_rclone(remote, dest="/content/dataset"):
    raise NotImplementedError  # Task 3
