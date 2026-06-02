"""Conditioned PyTorch Dataset yielding (audio+bias, wet) windows for BA283."""

from __future__ import annotations

from pathlib import Path

import numpy as np
import pandas as pd
import soundfile as sf
import torch
from torch.utils.data import DataLoader, Dataset

_REQUIRED_COLUMNS = ("dry_path", "wet_path", "bias_value", "duration_s", "stim_kind")


class BA283Dataset(Dataset):
    """Yields fixed-length (dry_scaled, bias) -> wet_scaled windows from a manifest.

    Each item is a tuple ``(x, y)`` where ``x`` has shape ``(window, 2)`` with
    column 0 = dry audio scaled by ``input_scale`` and column 1 = the constant
    bias value (the conditioning), and ``y`` has shape ``(window, 1)`` with the
    wet audio scaled by ``input_scale``.
    """

    def __init__(
        self,
        manifest_csv: str | Path,
        window: int = 4096,
        stride: int = 2048,
        input_scale: float = 2.0,
    ) -> None:
        self.window = window
        self.stride = stride
        self.input_scale = input_scale

        self.manifest = pd.read_csv(manifest_csv)
        missing = [c for c in _REQUIRED_COLUMNS if c not in self.manifest.columns]
        if missing:
            raise ValueError(f"manifest missing required columns: {missing}")

        # Flat index of (row_idx, sample_start) windows across every row.
        self.index: list[tuple[int, int]] = []
        for row_idx, row in self.manifest.iterrows():
            frames = sf.info(row["wet_path"]).frames
            start = 0
            while start + window <= frames:
                self.index.append((int(row_idx), start))
                start += stride

    def __len__(self) -> int:
        return len(self.index)

    def __getitem__(self, i: int) -> tuple[torch.Tensor, torch.Tensor]:
        row_idx, start = self.index[i]
        row = self.manifest.iloc[row_idx]

        dry = self._read_window(row["dry_path"], start)
        wet = self._read_window(row["wet_path"], start)

        x = torch.empty(self.window, 2, dtype=torch.float32)
        x[:, 0] = torch.from_numpy(dry) * self.input_scale
        x[:, 1] = float(row["bias_value"])

        y = (torch.from_numpy(wet) * self.input_scale).reshape(self.window, 1)
        return x, y

    def _read_window(self, path: str, start: int) -> np.ndarray:
        with sf.SoundFile(path) as f:
            f.seek(start)
            data = f.read(self.window, dtype="float32")
        if data.ndim == 2:
            data = data.mean(axis=1)
        return np.ascontiguousarray(data, dtype=np.float32)


def make_loader(
    manifest_csv: str | Path,
    *,
    batch_size: int = 64,
    window: int = 4096,
    stride: int = 2048,
    num_workers: int = 6,
    shuffle: bool = True,
) -> DataLoader:
    """Build a ``DataLoader`` wrapping a :class:`BA283Dataset`."""
    dataset = BA283Dataset(manifest_csv, window=window, stride=stride)
    return DataLoader(
        dataset,
        batch_size=batch_size,
        shuffle=shuffle,
        num_workers=num_workers,
        pin_memory=True,
        persistent_workers=num_workers > 0,
    )
