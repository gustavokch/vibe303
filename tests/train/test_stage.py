import subprocess
import zipfile
from pathlib import Path

import numpy as np
import pandas as pd
import soundfile as sf

from train.stage import (
    remap_manifests,
    stage_rclone,
    stage_zip,
)


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
