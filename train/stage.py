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


def _resolve(raw: str) -> Path:
    p = Path(raw)
    return p if p.is_absolute() else Path.cwd() / p


def assert_matched_sr(
    manifest: str | Path | pd.DataFrame,
    expected_sr: int | None = 44100,
    n_check: int = 12,
) -> int:
    """Assert staged dry/wet pairs share one sample rate (== ``expected_sr`` if set).

    The training loader (``train.dataset.BA283Dataset``) reads FRAME-indexed
    windows assuming dry and wet share a rate. A dry left at a different rate than
    its wet (e.g. a 96 kHz SPICE-source dry vs a 44.1 kHz wet) makes the
    ``(dry[i], wet[i])`` window pair span different DURATIONS -> the pair is
    decorrelated and the dry->wet map is unlearnable, so the model silently
    collapses to zero output (ESR -> 1.0 flat). That bug shipped once and only
    surfaced after a multi-hour run; this turns it into an immediate, named
    failure at stage time for the cost of a few WAV-header reads.

    Up to ``n_check`` rows are sampled by an even stride across the manifest (so a
    single off-rate stem anywhere is likely caught, not just the head); if a
    ``stim_kind`` column is present, one row per kind is forced into the sample
    first. ``soundfile`` is imported lazily so this module stays pure-stdlib for
    bundling. Raises ``RuntimeError`` naming the offending pair and its rates.
    Returns the confirmed shared sample rate.
    """
    import soundfile as sf  # lazy: keep module import pure-stdlib for bundling

    df = manifest if isinstance(manifest, pd.DataFrame) else pd.read_csv(manifest)
    if len(df) == 0:
        raise RuntimeError(f"manifest {manifest} is empty; nothing to check")
    for col in _PATH_COLUMNS:
        if col not in df.columns:
            raise RuntimeError(f"manifest missing required column: {col}")

    # Build the row sample: one row per stim_kind first (coverage), then fill by
    # even stride to n_check without duplicates, preserving manifest order.
    idx: list[int] = []
    if "stim_kind" in df.columns:
        for _, g in df.groupby("stim_kind"):
            idx.append(int(g.index[0]))
    if len(df) <= n_check:
        idx = list(range(len(df)))
    else:
        stride = max(1, len(df) // n_check)
        for i in range(0, len(df), stride):
            if i not in idx:
                idx.append(i)
    idx = sorted(set(idx))[:n_check]

    shared: int | None = None
    for i in idx:
        row = df.iloc[i]
        dry_sr = sf.info(str(_resolve(row["dry_path"]))).samplerate
        wet_sr = sf.info(str(_resolve(row["wet_path"]))).samplerate
        if dry_sr != wet_sr:
            raise RuntimeError(
                f"dry/wet sample-rate mismatch at row {i}: "
                f"dry={dry_sr} Hz ({Path(row['dry_path']).name}) != "
                f"wet={wet_sr} Hz ({Path(row['wet_path']).name}). "
                "Frame-indexed training windows would be time-misaligned "
                "(zero-output collapse). Rebuild the dry at the wet rate."
            )
        if expected_sr is not None and dry_sr != expected_sr:
            raise RuntimeError(
                f"sample rate {dry_sr} Hz at row {i} "
                f"({Path(row['dry_path']).name}) != expected {expected_sr} Hz."
            )
        if shared is None:
            shared = dry_sr
        elif dry_sr != shared:
            raise RuntimeError(
                f"inconsistent sample rate across manifest: row {i} is {dry_sr} Hz "
                f"but an earlier checked row was {shared} Hz."
            )
    assert shared is not None  # idx is non-empty (len(df) > 0)
    return shared


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
