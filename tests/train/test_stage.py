import subprocess
import zipfile
from pathlib import Path

import numpy as np
import pandas as pd
import soundfile as sf

from train.stage import (
    assert_matched_sr,
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


def test_remapped_manifest_loads_in_make_loader(tmp_path):
    from train.dataset import make_loader

    ds = _make_dataset(tmp_path / "dataset")
    out = remap_manifests(ds, out_dir=tmp_path / "content")
    loader = make_loader(out["train"], batch_size=1, window=2048, stride=2048,
                         num_workers=0, shuffle=False, drop_last=False)
    x, y = next(iter(loader))
    assert x.shape[-1] == 2  # dry + conditioning column
    assert y.shape[-1] == 1


def _write_pair_manifest(root: Path, dry_sr: int, wet_sr: int) -> Path:
    """Write a dry/wet pair at the given rates + a manifest pointing at them."""
    (root / "dry").mkdir(parents=True, exist_ok=True)
    (root / "wet").mkdir(parents=True, exist_ok=True)
    dry = root / "dry" / "sweepa_dry.wav"
    wet = root / "wet" / "sweepa_gain1.000.wav"
    sf.write(dry, np.sin(np.linspace(0, 6.28, dry_sr)).astype(np.float32), dry_sr)
    sf.write(wet, np.sin(np.linspace(0, 6.28, wet_sr)).astype(np.float32), wet_sr)
    mani = root / "train.csv"
    pd.DataFrame([{
        "dry_path": str(dry), "wet_path": str(wet),
        "gain_value": 1.0, "duration_s": 1.0, "stim_kind": "sweep",
    }]).to_csv(mani, index=False)
    return mani


def test_assert_matched_sr_passes_when_equal(tmp_path):
    ds = _make_dataset(tmp_path / "dataset")  # dry+wet both 8000 Hz
    assert assert_matched_sr(ds / "train.csv", expected_sr=8000) == 8000


def test_assert_matched_sr_raises_on_mismatch(tmp_path):
    import pytest
    # The shipped bug: 96 kHz dry vs 44.1 kHz wet -> frame-misaligned -> collapse.
    mani = _write_pair_manifest(tmp_path / "bad", dry_sr=96000, wet_sr=44100)
    with pytest.raises(RuntimeError, match=r"sample-rate mismatch.*96000.*44100"):
        assert_matched_sr(mani, expected_sr=None)


def test_assert_matched_sr_enforces_expected(tmp_path):
    import pytest
    # Matched pair, but not the rate the run expects -> still a hard failure.
    mani = _write_pair_manifest(tmp_path / "wrongrate", dry_sr=8000, wet_sr=8000)
    with pytest.raises(RuntimeError, match=r"!= expected 44100"):
        assert_matched_sr(mani, expected_sr=44100)


def test_stage_rclone_raises_when_manifest_missing(tmp_path, monkeypatch):
    dest = tmp_path / "content" / "dataset"

    def fake_run(argv, **kw):
        # rclone "succeeds" but copies nothing -> manifest.csv absent
        Path(argv[3]).mkdir(parents=True, exist_ok=True)
        return subprocess.CompletedProcess(argv, 0, "", "")

    monkeypatch.setattr(subprocess, "run", fake_run)
    import pytest
    with pytest.raises(FileNotFoundError, match=r"manifest\.csv"):
        stage_rclone("remote:x", dest=dest)
