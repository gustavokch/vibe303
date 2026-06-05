from pathlib import Path

import numpy as np
import pandas as pd
import pytest
import soundfile as sf

from data_gen.resample import (
    main,
    resample_to_grid,
    split_manifest,
    temporal_split,
    write_manifest,
)


def test_resample_lands_on_96k_grid(tmp_path):
    t = np.cumsum(np.random.default_rng(0).uniform(5e-7, 1.5e-6, size=20_000))
    v = np.sin(2 * np.pi * 1000 * t).astype(np.float32)
    npz = tmp_path / "raw.npz"
    np.savez(npz, t=t, v=v)
    out = tmp_path / "wet.wav"
    resample_to_grid(npz, out, target_sr=96_000, head_trim_ms=50)
    data, sr = sf.read(out, dtype="float32")
    assert sr == 96_000
    assert data.shape[0] > 0
    assert 0.9 <= np.max(np.abs(data)) <= 1.1


def test_write_manifest_columns(tmp_path):
    rows = [
        {
            "dry_path": "a.wav",
            "wet_path": "a_bias0.50.wav",
            "gain_value": 0.5,
            "duration_s": 1.0,
            "stim_kind": "sweep",
        },
    ]
    out = tmp_path / "manifest.csv"
    write_manifest(rows, out)
    text = out.read_text()
    assert "dry_path,wet_path,gain_value,duration_s,stim_kind" in text.splitlines()[0]
    assert "a.wav" in text


def test_split_by_stimulus_file_no_leakage(tmp_path):
    rows = []
    for stim in ["sweepA", "sweepB", "sweepC", "noiseA", "noiseB"]:
        for bias in [0.0, 0.5, 1.0]:
            rows.append({
                "dry_path": f"dry/{stim}.wav",
                "wet_path": f"wet/{stim}_bias{bias:.2f}.wav",
                "gain_value": bias,
                "duration_s": 60.0,
                "stim_kind": "sweep" if "sweep" in stim else "noise",
            })
    manifest = tmp_path / "manifest.csv"
    pd.DataFrame(rows).to_csv(manifest, index=False)
    split_manifest(manifest, tmp_path, ratios=(0.6, 0.2, 0.2), seed=0)
    train = pd.read_csv(tmp_path / "train.csv")
    val = pd.read_csv(tmp_path / "val.csv")
    test = pd.read_csv(tmp_path / "test.csv")
    train_stems = set(train["dry_path"])
    val_stems = set(val["dry_path"])
    test_stems = set(test["dry_path"])
    assert train_stems.isdisjoint(val_stems)
    assert train_stems.isdisjoint(test_stems)
    assert val_stems.isdisjoint(test_stems)


def test_temporal_split_disjoint_time_slices(tmp_path):
    sr = 96_000
    n = 100_000
    sig = np.zeros(n, dtype=np.float32)
    dry = tmp_path / "s_dry.wav"
    wet = tmp_path / "s_bias0.50.wav"
    sf.write(dry, sig, sr, subtype="FLOAT")
    sf.write(wet, sig, sr, subtype="FLOAT")
    manifest = write_manifest(
        [{
            "dry_path": str(dry), "wet_path": str(wet), "gain_value": 0.5,
            "duration_s": n / sr, "stim_kind": "synth",
        }],
        tmp_path / "m.csv",
    )
    paths = temporal_split(manifest, tmp_path)
    tr = pd.read_csv(paths["train"]); va = pd.read_csv(paths["val"]); te = pd.read_csv(paths["test"])
    # every split keeps the file; the time ranges are contiguous and disjoint
    assert len(tr) == len(va) == len(te) == 1
    assert tr.iloc[0]["frame_start"] == 0
    assert tr.iloc[0]["frame_end"] == va.iloc[0]["frame_start"]
    assert va.iloc[0]["frame_end"] == te.iloc[0]["frame_start"]
    assert te.iloc[0]["frame_end"] == n


def _write_manifest_with_stems(path, stems):
    rows = []
    for stim in stems:
        for bias in [0.0, 0.5]:
            rows.append({
                "dry_path": f"dry/{stim}.wav",
                "wet_path": f"wet/{stim}_bias{bias:.2f}.wav",
                "gain_value": bias,
                "duration_s": 60.0,
                "stim_kind": "synth",
            })
    pd.DataFrame(rows).to_csv(path, index=False)


def test_split_too_few_stems_raises(tmp_path):
    manifest = tmp_path / "manifest.csv"
    _write_manifest_with_stems(manifest, ["sweepA", "sweepB"])
    with pytest.raises(ValueError, match="need >=3"):
        split_manifest(manifest, tmp_path)


def test_split_empty_split_raises(tmp_path):
    manifest = tmp_path / "manifest.csv"
    _write_manifest_with_stems(manifest, ["sweepA", "sweepB", "sweepC", "sweepD"])
    with pytest.raises(ValueError, match="empty splits"):
        split_manifest(manifest, tmp_path, ratios=(1.0, 0.0, 0.0))


def test_resample_missing_keys_raises(tmp_path):
    t = np.linspace(0.0, 1e-3, 100)
    npz = tmp_path / "raw.npz"
    np.savez(npz, t=t)
    out = tmp_path / "wet.wav"
    with pytest.raises(ValueError, match="missing required arrays"):
        resample_to_grid(npz, out)


def test_resample_non_monotonic_time_raises(tmp_path):
    t = np.array([0.0, 1e-4, 0.5e-4, 2e-4])
    v = np.zeros_like(t)
    npz = tmp_path / "raw.npz"
    np.savez(npz, t=t, v=v)
    out = tmp_path / "wet.wav"
    with pytest.raises(ValueError, match="strictly increasing"):
        resample_to_grid(npz, out)


def test_write_aligned_dry_trims_head(tmp_path):
    # The aligned dry must drop the same head (head_trim_ms) the wet resample
    # drops, so dry[i] <-> wet[i]. Here: 96k sr, 50 ms -> 4800 samples trimmed.
    from data_gen.resample import write_aligned_dry

    sr = 96_000
    n = 12_000
    data = np.arange(n, dtype=np.float32)  # ramp: trivially checks the offset
    src = tmp_path / "dry.wav"
    sf.write(src, data, sr, subtype="FLOAT")
    out = write_aligned_dry(src, tmp_path / "dry_aligned.wav", target_sr=sr, head_trim_ms=50)
    got, _ = sf.read(out, dtype="float32")
    assert len(got) == n - 4800
    assert got[0] == 4800.0  # first kept sample is the old index 4800


def test_main_writes_resolvable_dry_path(tmp_path):
    dry_dir = tmp_path / "dry"
    dry_dir.mkdir()
    raw_dir = tmp_path / "raw"
    raw_dir.mkdir()
    rng = np.random.default_rng(0)
    # split_manifest (called by main) needs >=3 unique stems with non-empty
    # train/val/test under the default 0.8/0.1/0.1 ratios; 10 satisfies that.
    stems = [f"sweep{i}" for i in range(10)]
    for stem in stems:
        sf.write(
            dry_dir / f"{stem}.wav",
            np.zeros(96, dtype=np.float32),
            96_000,
            subtype="FLOAT",
        )
        t = np.cumsum(rng.uniform(5e-7, 1.5e-6, size=20_000))
        v = np.sin(2 * np.pi * 1000 * t).astype(np.float32)
        np.savez(raw_dir / f"{stem}_gain0.500.npz", t=t, v=v)

    out_dir = tmp_path / "wet"
    manifest = tmp_path / "manifest.csv"
    rc = main([
        "--raw-dir", str(raw_dir),
        "--out-dir", str(out_dir),
        "--manifest", str(manifest),
        "--dry-dir", str(dry_dir),
        "--head-trim-ms", "0",
    ])
    assert rc == 0

    df = pd.read_csv(manifest)
    assert len(df) == 10
    # main writes a head-trimmed dry copy (aligned to the wet target) into the
    # out dir and points the manifest at it (see write_aligned_dry).
    row = df[df["dry_path"].str.endswith("sweep0_dry.wav")].iloc[0]
    assert row.dry_path == str(out_dir / "sweep0_dry.wav")
    assert Path(row.dry_path).exists()
