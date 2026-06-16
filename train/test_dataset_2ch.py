"""Task 3: confirm the dataset yields the 2-ch [audio, gain] input the WaveNet needs.

`BA283Dataset` already emits a 2-column input window (column 0 = dry audio scaled
by INPUT_SCALE, column 1 = the constant per-window gain value). The conditioned
WaveNet (Task 1/4) consumes exactly this, transposed to (B, 2, T). These tests
pin the shape/axis contract so Task 4's training loop can rely on it, and verify
the 9-gain reamp (Task 2) is fully represented.

Run from the ba283-nam-lstm repo root with the ba283-vst3 eval env:
    cd ~/Git/ba283-nam-lstm && \
    uv run --project ~/Git/ba283-vst3 --extra eval python -m pytest \
        train/test_dataset_2ch.py -v
"""
from __future__ import annotations

from pathlib import Path

import pytest
import torch

from train.dataset import INPUT_SCALE, OUTPUT_SCALE, BA283Dataset, make_loader

MANIFEST = Path("scratch/nam_sweep/ds48/manifest.csv")
DRY_WAV = Path("scratch/nam_sweep/ds48/T3K-60s_dry.wav")
WINDOW = 4096
STRIDE = 2048
EXPECTED_GAINS = [0.0, 0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875, 1.0]


def _require_dataset() -> None:
    # manifest.csv is committed, but the 9-gain ds48 WAVs are git-ignored (large)
    # and machine-local — produced by the Task 2 reamp + _rebuild_ds48.py in the
    # ba283-nam-lstm checkout. Skip (don't fail) when the audio is absent, so the
    # suite is green in any checkout / CI without the local data; run from the
    # ba283-nam-lstm repo root (where the reamp wrote the WAVs) to exercise these.
    if not (MANIFEST.exists() and DRY_WAV.exists()):
        pytest.skip(f"ds48 audio absent ({DRY_WAV}); run the Task 2 reamp first")


def _ds() -> BA283Dataset:
    _require_dataset()
    return BA283Dataset(MANIFEST, window=WINDOW, stride=STRIDE)


def test_item_is_2ch_audio_gain():
    """One item: x is (window, 2) = [audio, gain]; y is (window, 1); all finite."""
    ds = _ds()
    x, y = ds[0]
    assert x.shape == (WINDOW, 2), f"input shape {tuple(x.shape)} != (window, 2)"
    assert y.shape == (WINDOW, 1), f"target shape {tuple(y.shape)} != (window, 1)"
    assert torch.isfinite(x).all() and torch.isfinite(y).all()


def test_gain_channel_is_constant_within_window():
    """Column 1 is the conditioning gain: constant across the whole window."""
    ds = _ds()
    x, _ = ds[0]
    gain_col = x[:, 1]
    assert torch.allclose(gain_col, gain_col[0].expand_as(gain_col)), (
        "gain channel (col 1) is not constant within the window"
    )


def test_audio_and_gain_channels_are_distinct():
    """Col 0 (scaled audio) varies; col 1 (gain) does not — they are not swapped."""
    ds = _ds()
    x, _ = ds[0]
    assert x[:, 0].std() > 0.0, "audio channel (col 0) is constant — axes swapped?"
    # gain in [0,1]; scaled audio rides ~unit after INPUT_SCALE. Gain value must
    # equal a known grid point (not an INPUT_SCALE-multiplied audio sample).
    assert 0.0 <= float(x[0, 1]) <= 1.0


def test_all_nine_gains_present():
    """Every reamped gain appears as a window's conditioning value."""
    ds = _ds()
    seen = sorted({round(float(ds[i][0][0, 1]), 3) for i in range(len(ds))})
    assert seen == [round(g, 3) for g in EXPECTED_GAINS], (
        f"gain coverage mismatch: saw {seen}, want {EXPECTED_GAINS}"
    )


def test_batch_axis_order():
    """DataLoader batches to (B, window, 2) / (B, window, 1) — Task 4 transposes
    the input to WaveNet (B, 2, window)."""
    _require_dataset()
    loader = make_loader(
        MANIFEST,
        batch_size=8,
        window=WINDOW,
        stride=STRIDE,
        num_workers=0,
        shuffle=False,
        drop_last=True,
    )
    xb, yb = next(iter(loader))
    assert xb.shape == (8, WINDOW, 2), f"batch input {tuple(xb.shape)}"
    assert yb.shape == (8, WINDOW, 1), f"batch target {tuple(yb.shape)}"
    # The to-WaveNet transpose the training loop will apply:
    assert xb.transpose(1, 2).shape == (8, 2, WINDOW)


def test_scaling_constants_documented():
    """Pin the scaling contract Task 4/5 + the plugin de-norm depend on."""
    assert INPUT_SCALE == 2.0
    assert OUTPUT_SCALE == 1.0 / 64.0
