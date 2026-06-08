"""Unit tests for the WAV->WAV resampler used to build ablation subsets."""
from __future__ import annotations

import numpy as np
import pandas as pd
import soundfile as sf

from data_gen.resample_wav import (
    _select_subset,
    build_resampled_subset,
    resample_wav,
)


def _sine(path, sr, secs, f=1000.0):
    path.parent.mkdir(parents=True, exist_ok=True)
    t = np.arange(int(sr * secs)) / sr
    sf.write(str(path), (0.5 * np.sin(2 * np.pi * f * t)).astype(np.float32), sr, subtype="FLOAT")


def test_resample_changes_sr_and_length(tmp_path):
    src = tmp_path / "in.wav"
    dst = tmp_path / "out.wav"
    _sine(src, 96000, 1.0)
    n = resample_wav(src, dst, 44100)
    info = sf.info(str(dst))
    assert info.samplerate == 44100
    # ~1 s at 44.1k, within polyphase edge tolerance.
    assert abs(info.frames - 44100) <= 2
    assert info.frames == n


def test_resample_passthrough_when_rate_matches(tmp_path):
    src = tmp_path / "in.wav"
    dst = tmp_path / "out.wav"
    _sine(src, 44100, 0.5)
    resample_wav(src, dst, 44100)
    a, _ = sf.read(str(src), dtype="float32")
    b, _ = sf.read(str(dst), dtype="float32")
    assert np.array_equal(a, b)


def test_select_subset_caps_stems_per_kind_keeping_all_gains(tmp_path):
    rows = []
    for stem in ("musicA", "musicB", "musicC"):
        for g in (0.0, 0.5, 1.0):
            rows.append(
                {
                    "dry_path": f"/x/dry/{stem}_dry.wav",
                    "wet_path": f"/x/wet/{stem}_gain{g:.3f}.wav",
                    "gain_value": g,
                    "duration_s": 5.0,
                    "stim_kind": "music",
                }
            )
    df = pd.DataFrame(rows)
    sub = _select_subset(df, {"music": 2})
    assert sub["wet_path"].map(lambda p: p.split("/")[-1].split("_gain")[0]).nunique() == 2
    assert len(sub) == 6  # 2 stems x 3 gains, all gains retained


def test_build_resampled_subset_writes_manifest_and_splits(tmp_path):
    root = tmp_path / "src"
    stems = tuple("abcdefghij")  # 10 stems -> non-empty 0.8/0.1/0.1 split
    for stem in stems:
        _sine(root / "dry" / f"{stem}_dry.wav", 96000, 0.6)
        for g in (0.0, 1.0):
            _sine(root / "wet" / f"{stem}_gain{g:.3f}.wav", 96000, 0.6)
    rows = [
        {
            "dry_path": f"/orig/dry/{stem}_dry.wav",
            "wet_path": f"/orig/wet/{stem}_gain{g:.3f}.wav",
            "gain_value": g,
            "duration_s": 0.6,
            "stim_kind": "synth",
        }
        for stem in stems
        for g in (0.0, 1.0)
    ]
    out = build_resampled_subset(pd.DataFrame(rows), root, tmp_path / "out", 44100)
    assert (out / "manifest.csv").exists()
    for name in ("train", "val", "test"):
        assert (out / f"{name}.csv").exists()
    m = pd.read_csv(out / "manifest.csv")
    assert len(m) == 20
    assert sf.info(str(out / "wet" / "a_gain0.000.wav")).samplerate == 44100
