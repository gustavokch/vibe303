"""Conditioned PyTorch Dataset yielding (audio+bias, wet) windows for BA283."""

from __future__ import annotations

from pathlib import Path

import numpy as np
import pandas as pd
import soundfile as sf
import torch
from torch.utils.data import DataLoader, Dataset

_REQUIRED_COLUMNS = ("dry_path", "wet_path", "bias_value", "duration_s", "stim_kind")

# --- Domain scaling -------------------------------------------------------
# The dry stimulus WAVs sit at ~0.4 peak; INPUT_SCALE lifts them to ~0.9 so the
# GRU input is near unit magnitude.
INPUT_SCALE = 2.0
# The wet (SPICE) targets are RAW VOLTS: peaks reach ~55 V across the dataset.
# Training the model directly on volt-magnitude targets makes the (scale-variant)
# MSE term of the composite loss explode -> the gradient overflows to inf ->
# clip_grad_norm turns inf into NaN (inf * clip_coef where clip_coef -> 0),
# permanently poisoning every weight (this is exactly what killed the Colab run).
# Normalize the target by a fixed reference voltage so it lands in ~[-1, 1].
# OUTPUT_REF_V (64 V) is a power-of-two with headroom over the dataset max
# (~55 V), so every split's target is bounded < 1.0. The model therefore learns
# in a normalized domain; downstream (the Rust plugin) multiplies the model
# output by OUTPUT_REF_V to recover volts. Kept as a module constant (not a
# data-derived statistic) so it is deterministic and embeddable in the weights
# JSON. The ESR loss term is scale-invariant, so this only fixes conditioning.
OUTPUT_REF_V = 64.0
OUTPUT_SCALE = 1.0 / OUTPUT_REF_V


def _resolve_path(raw: str) -> Path:
    """Resolve a manifest path to a usable filesystem path.

    Manifest path convention (matches ``data_gen/resample.py``): entries are
    stored either absolute or relative to the process working directory.
    Absolute paths are used as-is; relative paths are resolved against the
    current working directory (``Path.cwd()``). Production runs are expected to
    launch from the directory the manifest's relative paths are anchored to (the
    repo root), or to use absolute paths. We deliberately do NOT resolve
    relative to the manifest file's own parent directory, because the real
    manifest written by ``resample.py`` stores cwd-relative / absolute paths,
    not manifest-parent-relative ones.
    """
    p = Path(raw)
    return p if p.is_absolute() else Path.cwd() / p


class BA283Dataset(Dataset):
    """Yields fixed-length (dry_scaled, bias) -> wet_scaled windows from a manifest.

    Each item is a tuple ``(x, y)`` where ``x`` has shape ``(window, 2)`` with
    column 0 = dry audio scaled by ``input_scale`` and column 1 = the constant
    bias value (the conditioning), and ``y`` has shape ``(window, 1)`` with the
    wet (volt) target scaled by ``output_scale`` into a normalized ~[-1, 1]
    domain (see ``OUTPUT_SCALE``). The model thus learns normalized output; the
    Rust plugin multiplies by ``OUTPUT_REF_V`` to recover volts.

    Path convention: manifest ``dry_path``/``wet_path`` entries are either
    absolute or relative to the process working directory (matching what
    ``data_gen/resample.py`` writes). Launch training from the directory the
    manifest's relative paths are anchored to (the repo root), or use absolute
    paths.
    """

    def __init__(
        self,
        manifest_csv: str | Path,
        window: int = 4096,
        stride: int = 2048,
        input_scale: float = INPUT_SCALE,
        output_scale: float = OUTPUT_SCALE,
    ) -> None:
        self.window = window
        self.stride = stride
        self.input_scale = input_scale
        self.output_scale = output_scale

        self.manifest = pd.read_csv(manifest_csv)
        missing = [c for c in _REQUIRED_COLUMNS if c not in self.manifest.columns]
        if missing:
            raise ValueError(f"manifest missing required columns: {missing}")

        # Flat index of (row_idx, sample_start) windows across every row. The
        # window count is bounded by min(dry_frames, wet_frames) so a dry file
        # shorter than its wet pair never yields an out-of-range read.
        self.index: list[tuple[int, int]] = []
        for row_idx, row in self.manifest.iterrows():
            dry_frames = self._frame_count(row["dry_path"], int(row_idx), "dry_path")
            wet_frames = self._frame_count(row["wet_path"], int(row_idx), "wet_path")
            frames = min(dry_frames, wet_frames)
            start = 0
            while start + window <= frames:
                self.index.append((int(row_idx), start))
                start += stride

    @staticmethod
    def _frame_count(raw_path: str, row_idx: int, column: str) -> int:
        """Read the frame count of an audio file, validating existence.

        Re-raises any failure (missing/unreadable file) as ``FileNotFoundError``
        naming the manifest row index and which column is broken, so the error
        is diagnosable instead of an opaque ``LibsndfileError`` deep in init.
        """
        path = _resolve_path(raw_path)
        try:
            return sf.info(str(path)).frames
        except Exception as exc:  # re-raised below with row/column context
            raise FileNotFoundError(
                f"manifest row {row_idx} column '{column}': cannot read audio "
                f"file '{path}' ({exc})"
            ) from exc

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

        y = (torch.from_numpy(wet) * self.output_scale).reshape(self.window, 1)
        return x, y

    def _read_window(self, path: str, start: int) -> np.ndarray:
        resolved = _resolve_path(path)
        with sf.SoundFile(str(resolved)) as f:
            f.seek(start)
            data = f.read(self.window, dtype="float32")
        if data.ndim == 2:
            data = data.mean(axis=1)
        # Defensive: the window index is built from min(dry,wet) frames, so a
        # full-length read is expected. A short read means the index and the
        # file disagree (e.g. file changed under us); surface it diagnosably.
        if data.shape[0] != self.window:
            raise ValueError(
                f"short read from '{resolved}' at start={start}: expected "
                f"{self.window} frames, got {data.shape[0]}"
            )
        return np.ascontiguousarray(data, dtype=np.float32)


def make_loader(
    manifest_csv: str | Path,
    *,
    batch_size: int = 64,
    window: int = 4096,
    stride: int = 2048,
    num_workers: int = 6,
    shuffle: bool = True,
    drop_last: bool = True,
) -> DataLoader:
    """Build a ``DataLoader`` wrapping a :class:`BA283Dataset`.

    ``drop_last`` defaults to ``True`` so every batch has a fixed first
    dimension: the training loop pre-allocates the GRU hidden state for the
    batch size, and a short final batch would break the last TBPTT step of each
    epoch. Evaluation can pass ``drop_last=False`` to keep all samples.
    """
    dataset = BA283Dataset(manifest_csv, window=window, stride=stride)
    return DataLoader(
        dataset,
        batch_size=batch_size,
        shuffle=shuffle,
        num_workers=num_workers,
        pin_memory=True,
        persistent_workers=num_workers > 0,
        drop_last=drop_last,
    )
