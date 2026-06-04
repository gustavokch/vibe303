from pathlib import Path

import numpy as np
import pandas as pd
import pytest
import soundfile as sf
import torch

from train.dataset import BA283Dataset, make_loader

MANIFEST = Path("tests/fixtures/manifest_tiny.csv")


def _write_manifest(path: Path, rows: list[dict]) -> Path:
    cols = ["dry_path", "wet_path", "bias_value", "duration_s", "stim_kind"]
    pd.DataFrame(rows, columns=cols).to_csv(path, index=False)
    return path


def test_dataset_returns_correct_shapes():
    ds = BA283Dataset(MANIFEST, window=4096, stride=2048)
    x, y = ds[0]
    assert x.shape == (4096, 2)
    assert y.shape == (4096, 1)
    assert x.dtype == torch.float32
    assert y.dtype == torch.float32


def test_bias_constant_across_window():
    ds = BA283Dataset(MANIFEST, window=4096, stride=2048)
    x, _ = ds[0]
    bias_col = x[:, 1]
    assert torch.all(bias_col == bias_col[0])


def test_dataset_covers_all_files_and_biases():
    ds = BA283Dataset(MANIFEST, window=4096, stride=2048)
    seen_bias = set()
    for i in range(len(ds)):
        x, _ = ds[i]
        seen_bias.add(round(float(x[0, 1]), 4))
    assert seen_bias == {0.0, 0.5, 1.0}


def test_input_scale_applied():
    ds = BA283Dataset(MANIFEST, window=4096, stride=2048)
    x, _ = ds[0]
    audio = x[:, 0]
    assert 0.7 < audio.abs().max().item() < 0.9


def test_target_is_output_normalized(tmp_path):
    # Real SPICE wet targets are raw volts (peaks ~55 V). The target must be
    # normalized by OUTPUT_SCALE (volt -> ~unit), NOT by the input_scale used for
    # the dry audio. A constant 50 V wet must map to 50 * OUTPUT_SCALE (< 1.0),
    # bounded, so the MSE loss term cannot explode. The bug: y = wet * 2.0 = 100.
    from train.dataset import OUTPUT_SCALE

    sr = 96_000
    n = 12_000
    dry = (0.3 * np.ones(n)).astype(np.float32)
    wet = (50.0 * np.ones(n)).astype(np.float32)
    dry_path = tmp_path / "dry.wav"
    wet_path = tmp_path / "wet.wav"
    sf.write(dry_path, dry, sr, subtype="FLOAT")
    sf.write(wet_path, wet, sr, subtype="FLOAT")
    manifest = _write_manifest(
        tmp_path / "m.csv",
        [
            {
                "dry_path": str(dry_path),
                "wet_path": str(wet_path),
                "bias_value": 0.5,
                "duration_s": n / sr,
                "stim_kind": "synth",
            }
        ],
    )
    ds = BA283Dataset(manifest, window=4096, stride=2048)
    _, y = ds[0]
    assert y.abs().max().item() < 1.0
    assert abs(y[0, 0].item() - 50.0 * OUTPUT_SCALE) < 1e-4


def test_make_loader_yields_batches():
    loader = make_loader(
        MANIFEST, batch_size=4, window=4096, stride=2048, num_workers=0, shuffle=False
    )
    batch_x, batch_y = next(iter(loader))
    assert batch_x.shape == (4, 4096, 2)
    assert batch_y.shape == (4, 4096, 1)


def test_dry_shorter_than_wet_is_handled(tmp_path):
    sr = 96_000
    n = 12_000
    wet = np.zeros(n, dtype=np.float32)
    dry = np.zeros(n // 2, dtype=np.float32)
    dry_path = tmp_path / "dry.wav"
    wet_path = tmp_path / "wet.wav"
    sf.write(dry_path, dry, sr, subtype="FLOAT")
    sf.write(wet_path, wet, sr, subtype="FLOAT")

    manifest = _write_manifest(
        tmp_path / "m.csv",
        [
            {
                "dry_path": str(dry_path),
                "wet_path": str(wet_path),
                "bias_value": 0.5,
                "duration_s": n / sr,
                "stim_kind": "synth",
            }
        ],
    )

    window, stride = 4096, 2048
    ds = BA283Dataset(manifest, window=window, stride=stride)

    # C1: index is bounded by min(dry, wet) frames, so windows never exceed the
    # shorter (dry) file. With dry = n//2 = 6000 frames, only one window fits.
    assert len(ds) == 1
    # Iterating must not crash and must yield full-length windows.
    for i in range(len(ds)):
        x, y = ds[i]
        assert x.shape == (window, 2)
        assert y.shape == (window, 1)


def test_missing_file_raises_clear_error(tmp_path):
    sr = 96_000
    dry = np.zeros(8192, dtype=np.float32)
    dry_path = tmp_path / "dry.wav"
    sf.write(dry_path, dry, sr, subtype="FLOAT")
    missing_wet = tmp_path / "does_not_exist.wav"

    manifest = _write_manifest(
        tmp_path / "m.csv",
        [
            {
                "dry_path": str(dry_path),
                "wet_path": str(missing_wet),
                "bias_value": 0.0,
                "duration_s": 0.085,
                "stim_kind": "synth",
            }
        ],
    )

    with pytest.raises((FileNotFoundError, ValueError)) as excinfo:
        BA283Dataset(manifest, window=4096, stride=2048)
    msg = str(excinfo.value)
    assert "does_not_exist.wav" in msg
    assert "wet_path" in msg


def test_num_workers_smoke():
    loader = make_loader(
        MANIFEST,
        batch_size=2,
        window=4096,
        stride=2048,
        num_workers=2,
        shuffle=False,
    )
    batch_x, batch_y = next(iter(loader))
    assert batch_x.shape == (2, 4096, 2)
    assert batch_y.shape == (2, 4096, 1)


def test_drop_last_default():
    # Default drop_last=True: every batch has exactly batch_size samples.
    loader = make_loader(
        MANIFEST,
        batch_size=64,
        window=4096,
        stride=2048,
        num_workers=0,
        shuffle=False,
    )
    n_batches = 0
    for batch_x, _ in loader:
        assert batch_x.shape[0] == 64
        n_batches += 1
    assert n_batches >= 1

    # drop_last=False keeps the partial final batch, so total samples are
    # preserved across batches.
    loader_keep = make_loader(
        MANIFEST,
        batch_size=64,
        window=4096,
        stride=2048,
        num_workers=0,
        shuffle=False,
        drop_last=False,
    )
    total_keep = sum(bx.shape[0] for bx, _ in loader_keep)
    total_drop = sum(bx.shape[0] for bx, _ in loader)
    assert total_keep > total_drop
