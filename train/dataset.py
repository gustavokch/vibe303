"""Conditioned PyTorch Dataset yielding (audio+gain, wet) windows for BA283."""

from __future__ import annotations

from pathlib import Path

import numpy as np
import pandas as pd
import soundfile as sf
import torch
from torch.utils.data import DataLoader, Dataset, WeightedRandomSampler

_REQUIRED_COLUMNS = ("dry_path", "wet_path", "gain_value", "duration_s", "stim_kind")

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

# Minimum scaled-target energy (sum of squares over a window) for a window to be
# kept. The composite loss uses ESR (= error_energy / target_energy), which is
# mathematically ill-defined as the target energy -> 0: near-silent windows
# (e.g. impulse tails, quiet stimulus regions) drive the ESR denominator toward
# zero, so the backward `num / denom**2` term overflows to a non-finite gradient.
# These windows also carry no learnable input->output transfer. ~25% of the train
# windows are near-silent (min target energy ~4e-16). Dropping windows below this
# floor removes the gradient pathology at its source; the surviving minimum
# energy (1e-2) keeps denom**2 >= 1e-4, so the ESR gradient stays bounded.
MIN_TARGET_ENERGY = 1e-2

# STEPPED (WS4) ------------------------------------------------------------
# The 5 hardware gain-switch detents, as R_KJ (ohms), idx 0..4 = 15/20/25/30/35
# dB (balanced). idx 0 is the "open" sentinel (1e9 Ω); 1..4 are real resistors.
# Mirrors the Rust plugin's R_KJ table exactly so the NN learns the SAME 5
# operating points the host can select. The conditioning column for the
# ``stepped`` mode is the INDEX into this table, not the raw R_KJ value.
STEPPED_RKJ_OHMS = (1.0e9, 1200.0, 470.0, 220.0, 110.0)


def nearest_stepped_index(gain_value: float) -> int:
    """Map a dataset ``gain_value`` (R_KJ ohms) to the nearest of the 5 positions.

    The gain-sweep manifest stores ``gain_value`` = the R_KJ ohms the SPICE
    target was rendered at (``data_gen.run_spice`` filename ``_gain<rkj>``). For
    the ``stepped`` conditioning the data-gen is expected to render EXACTLY at
    ``STEPPED_RKJ_OHMS``, so this is a snap-to-grid that also tolerates tiny
    float drift. Distance is measured in log-resistance (the gain axis is
    log-linear in R_KJ), so a value between two detents snaps to the nearer in
    dB rather than in raw ohms. Returns the index 0..4.
    """
    target = np.log(max(float(gain_value), 1e-9))
    grid = np.log(np.asarray(STEPPED_RKJ_OHMS, dtype=np.float64))
    return int(np.argmin(np.abs(grid - target)))


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
    """Yields fixed-length (dry_scaled, gain) -> wet_scaled windows from a manifest.

    Each item is a tuple ``(x, y)`` where ``x`` has shape ``(window, 2)`` with
    column 0 = dry audio scaled by ``input_scale`` and column 1 = the constant
    gain value (the conditioning), and ``y`` has shape ``(window, 1)`` with the
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
        min_target_energy: float = MIN_TARGET_ENERGY,
        conditioning: str = "film",
    ) -> None:
        self.window = window
        self.stride = stride
        self.input_scale = input_scale
        self.output_scale = output_scale
        self.min_target_energy = min_target_energy
        # STEPPED (WS4): selects what the conditioning column (x[:, 1]) carries.
        # film/concat -> the continuous gain_value (unchanged); stepped -> the
        # discrete class index (0..4) snapped from gain_value via
        # nearest_stepped_index. The model side must match (see BA283LSTM).
        if conditioning not in ("film", "concat", "stepped"):
            raise ValueError(
                f"conditioning must be 'film', 'concat' or 'stepped', got {conditioning!r}"
            )
        self.conditioning = conditioning

        self.manifest = pd.read_csv(manifest_csv)
        missing = [c for c in _REQUIRED_COLUMNS if c not in self.manifest.columns]
        if missing:
            raise ValueError(f"manifest missing required columns: {missing}")

        # Flat index of (row_idx, sample_start) windows across every row. The
        # window count is bounded by min(dry_frames, wet_frames) so a dry file
        # shorter than its wet pair never yields an out-of-range read. Windows
        # whose scaled-target energy is below ``min_target_energy`` are dropped
        # (see ``MIN_TARGET_ENERGY``): the wet file is read once per row to gate
        # them, avoiding a per-window seek at training time.
        has_frame_range = (
            "frame_start" in self.manifest.columns
            and "frame_end" in self.manifest.columns
        )
        self.index: list[tuple[int, int]] = []
        for row_idx, row in self.manifest.iterrows():
            dry_frames = self._frame_count(row["dry_path"], int(row_idx), "dry_path")
            wet_frames = self._frame_count(row["wet_path"], int(row_idx), "wet_path")
            frames = min(dry_frames, wet_frames)

            # Optional temporal sub-range: a row may restrict windows to
            # ``[frame_start, frame_end)`` so the SAME file can supply disjoint
            # time slices to train/val/test (see resample.temporal_split). Absent
            # columns -> the whole file.
            lo = 0
            hi = frames
            if has_frame_range:
                lo = max(0, int(row["frame_start"]))
                hi = min(frames, int(row["frame_end"]))

            sq = None
            if self.min_target_energy > 0.0 and window <= frames:
                wet_full = self._read_full(row["wet_path"]).astype(np.float64)
                sq = (wet_full * self.output_scale) ** 2

            start = lo
            while start + window <= hi:
                if sq is not None:
                    if float(sq[start : start + window].sum()) < self.min_target_energy:
                        start += stride
                        continue
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

    def _read_full(self, path: str) -> np.ndarray:
        """Read an entire (mono-mixed) audio file as float32 for energy gating."""
        resolved = _resolve_path(path)
        with sf.SoundFile(str(resolved)) as f:
            data = f.read(dtype="float32")
        if data.ndim == 2:
            data = data.mean(axis=1)
        return np.ascontiguousarray(data, dtype=np.float32)

    def __len__(self) -> int:
        return len(self.index)

    def __getitem__(self, i: int) -> tuple[torch.Tensor, torch.Tensor]:
        row_idx, start = self.index[i]
        row = self.manifest.iloc[row_idx]

        dry = self._read_window(row["dry_path"], start)
        wet = self._read_window(row["wet_path"], start)

        x = torch.empty(self.window, 2, dtype=torch.float32)
        x[:, 0] = torch.from_numpy(dry) * self.input_scale
        if self.conditioning == "stepped":
            # STEPPED (WS4): emit the discrete switch-position index (0..4)
            # instead of the continuous R_KJ. Stored as float (the x tensor is
            # float32); the model rounds + casts it back to long for the codebook
            # lookup. Snap-to-grid tolerates tiny float drift in gain_value.
            x[:, 1] = float(nearest_stepped_index(float(row["gain_value"])))
        else:
            x[:, 1] = float(row["gain_value"])

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


def _kind_balanced_sampler(
    dataset: BA283Dataset, *, max_ratio: float = 8.0
) -> WeightedRandomSampler:
    """Inverse-frequency window sampler over ``stim_kind``, ratio-capped.

    The corpus is ~56% music; energy-weighted training then under-sees sweep /
    synth / impulse character. Weighting each window by ``1 / count(its kind)``
    makes the expected per-epoch draw uniform across kinds (with replacement),
    de-skewing the imbalance without re-rendering. The per-window kind is the
    kind of its source manifest row.

    ``max_ratio`` caps how hard a rare kind can be upweighted: a class with very
    few windows (e.g. the 9-pt set's ~5 impulse rows) would otherwise get a
    ~count(music)/count(impulse) ≈ 40x per-window weight and be drawn almost
    every step, so the model memorises those few files and the loss destabilises.
    The effective count is floored at ``max_count / max_ratio``, bounding any
    kind's per-window weight to ``max_ratio`` x the most-common kind's. The rare
    kind stays under-sampled (honest: 5 files cannot carry equal epoch mass) but
    no longer dominates. ``max_ratio = inf`` recovers pure inverse-frequency.
    """
    kinds = dataset.manifest["stim_kind"].to_numpy()
    window_kinds = [kinds[row_idx] for row_idx, _ in dataset.index]
    counts: dict[str, int] = {}
    for k in window_kinds:
        counts[k] = counts.get(k, 0) + 1
    floor = max(counts.values()) / max_ratio if counts else 1.0
    eff = {k: max(float(c), floor) for k, c in counts.items()}
    weights = torch.as_tensor(
        [1.0 / eff[k] for k in window_kinds], dtype=torch.double
    )
    return WeightedRandomSampler(weights, num_samples=len(weights), replacement=True)


def make_loader(
    manifest_csv: str | Path,
    *,
    batch_size: int = 64,
    window: int = 4096,
    stride: int = 2048,
    num_workers: int = 6,
    shuffle: bool = True,
    drop_last: bool = True,
    balance_by_kind: bool = False,
    pin_memory: bool = True,
    conditioning: str = "film",
) -> DataLoader:
    """Build a ``DataLoader`` wrapping a :class:`BA283Dataset`.

    ``drop_last`` defaults to ``True`` so every batch has a fixed first
    dimension: the training loop pre-allocates the GRU hidden state for the
    batch size, and a short final batch would break the last TBPTT step of each
    epoch. Evaluation can pass ``drop_last=False`` to keep all samples.

    ``balance_by_kind`` replaces uniform shuffling with an inverse-frequency
    ``WeightedRandomSampler`` over ``stim_kind`` (see ``_kind_balanced_sampler``)
    to de-skew the music-dominated corpus. It implies shuffling, so it is only
    honoured when ``shuffle`` is set (validation/eval loaders pass
    ``shuffle=False`` and keep deterministic order).
    """
    # STEPPED (WS4): forward the conditioning mode so the dataset emits the
    # right conditioning column (float gain vs discrete index).
    dataset = BA283Dataset(
        manifest_csv, window=window, stride=stride, conditioning=conditioning
    )
    sampler = None
    if balance_by_kind and shuffle:
        sampler = _kind_balanced_sampler(dataset)
    return DataLoader(
        dataset,
        batch_size=batch_size,
        # A sampler and shuffle are mutually exclusive; the sampler carries the
        # randomised draw when balancing is on.
        shuffle=shuffle if sampler is None else False,
        sampler=sampler,
        num_workers=num_workers,
        # pin_memory only helps host->CUDA copies; it warns/wastes work on MPS/CPU.
        pin_memory=pin_memory,
        persistent_workers=num_workers > 0,
        drop_last=drop_last,
    )
