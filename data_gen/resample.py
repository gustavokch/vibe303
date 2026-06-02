"""Resample raw variable-timestep SPICE outputs to a uniform 96 kHz grid,
build a dataset manifest, and split it leakage-free into train/val/test.

Task 1.3 emits ``data/wet_raw/<stem>_bias<value>.npz`` holding the raw
``(t, v)`` transient on ngspice's adaptive (variable) timestep. This module:

1. ``resample_to_grid`` — PCHIP-interpolates each ``(t, v)`` onto a uniform
   ``1 / target_sr`` grid and writes a float WAV. PCHIP (monotonic Hermite)
   is used over a cubic spline because it does not overshoot at the clipped
   transformer-saturation edges, where a plain cubic would ring.
2. ``write_manifest`` — writes a CSV with an exact, training-consumable
   column order.
3. ``split_manifest`` — partitions the manifest by *unique dry stimulus*, so
   all bias variants of one stimulus land in the same split. The model is
   therefore validated/tested on genuinely unseen stimuli (no leakage).
"""

from __future__ import annotations

import argparse
import re
from pathlib import Path

import numpy as np
import pandas as pd
import soundfile as sf
from scipy.interpolate import PchipInterpolator

TARGET_SR = 96_000
MANIFEST_COLUMNS = ["dry_path", "wet_path", "bias_value", "duration_s", "stim_kind"]

# Substring -> stim_kind, checked in order against the dry stem. Best-effort:
# unmatched stems fall back to "synth".
_STIM_KINDS = ("sweep", "noise", "imd", "impulse", "multisine", "music")

# Matches "<stem>_bias<value>" filename stems, e.g. "sweepA_bias0.50".
_BIAS_RE = re.compile(r"^(?P<stem>.+)_bias(?P<bias>[-+0-9.eE]+)$")


def resample_to_grid(
    npz_path: Path | str,
    out_wav: Path | str,
    target_sr: int = TARGET_SR,
    head_trim_ms: int = 50,
) -> Path:
    """Resample a raw ``(t, v)`` ``.npz`` onto a uniform ``target_sr`` grid.

    Parameters
    ----------
    npz_path:
        Path to a ``.npz`` holding arrays ``t`` (variable timestep) and ``v``.
    out_wav:
        Destination WAV path (written as 32-bit float).
    target_sr:
        Uniform sample rate of the output grid (Hz).
    head_trim_ms:
        Milliseconds of the head to drop as a DC-settle margin.

    Returns
    -------
    ``Path`` to the written WAV (equal to ``out_wav``).
    """
    out_wav = Path(out_wav)
    d = np.load(npz_path)
    t = d["t"]
    v = d["v"]

    # Uniform grid spanning the simulated interval, one sample per 1/target_sr.
    tg = np.arange(t[0], t[-1], 1.0 / target_sr)
    # PCHIP: monotonic Hermite interpolation, no overshoot at clipped edges.
    v_uniform = PchipInterpolator(t, v)(tg)

    trim = int(head_trim_ms * target_sr / 1000)
    # Guard: a short stimulus (or a large head_trim) could trim every sample.
    # If trimming would leave nothing, trim nothing — the WAV must be
    # non-empty, and on a sub-trim-length clip there is no settled head to
    # drop anyway. (A lone surviving sample would also be a degenerate clip.)
    if trim < len(v_uniform):
        v_uniform = v_uniform[trim:]

    out_wav.parent.mkdir(parents=True, exist_ok=True)
    sf.write(out_wav, v_uniform.astype(np.float32), target_sr, subtype="FLOAT")
    return out_wav


def write_manifest(rows: list[dict], out_csv: Path | str) -> Path:
    """Write manifest ``rows`` to ``out_csv`` with the exact column order.

    Columns: ``dry_path,wet_path,bias_value,duration_s,stim_kind``.
    """
    out_csv = Path(out_csv)
    out_csv.parent.mkdir(parents=True, exist_ok=True)
    pd.DataFrame(rows, columns=MANIFEST_COLUMNS).to_csv(out_csv, index=False)
    return out_csv


def split_manifest(
    manifest_csv: Path | str,
    out_dir: Path | str,
    ratios: tuple[float, float, float] = (0.8, 0.1, 0.1),
    seed: int = 42,
) -> dict[str, Path]:
    """Split a manifest into train/val/test by *unique dry stimulus*.

    All bias variants of one ``dry_path`` go to the same split, so the val/test
    sets contain only genuinely unseen stimuli (no leakage). Writes
    ``train.csv``/``val.csv``/``test.csv`` (all manifest columns preserved) to
    ``out_dir``.

    Returns a mapping ``{"train": path, "val": path, "test": path}``.
    """
    out_dir = Path(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    df = pd.read_csv(manifest_csv)

    stems = np.asarray(df["dry_path"].unique(), dtype=object)
    rng = np.random.default_rng(seed)
    rng.shuffle(stems)

    n = len(stems)
    r_train, r_val, _ = ratios
    n_train = round(n * r_train)
    n_val = round(n * r_val)
    n_test = n - n_train - n_val

    train_stems = set(stems[:n_train])
    val_stems = set(stems[n_train : n_train + n_val])
    test_stems = set(stems[n_train + n_val :])

    splits = {
        "train": df[df["dry_path"].isin(train_stems)],
        "val": df[df["dry_path"].isin(val_stems)],
        "test": df[df["dry_path"].isin(test_stems)],
    }
    out_paths: dict[str, Path] = {}
    for name, part in splits.items():
        path = out_dir / f"{name}.csv"
        part.to_csv(path, index=False)
        out_paths[name] = path

    # n_test is implied by the partition above; reference it so the rounding
    # arithmetic stays self-documenting and lint-clean.
    assert n_test == len(test_stems)
    return out_paths


def _infer_stim_kind(stem: str) -> str:
    """Best-effort substring match of a dry stem to a stim_kind."""
    low = stem.lower()
    for kind in _STIM_KINDS:
        if kind in low:
            return kind
    return "synth"


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Resample raw SPICE .npz outputs to 96k WAVs + build manifest.",
    )
    parser.add_argument(
        "--raw-dir",
        required=True,
        help="Directory of raw <stem>_bias<value>.npz files.",
    )
    parser.add_argument(
        "--out-dir",
        required=True,
        help="Directory to write resampled wet WAVs.",
    )
    parser.add_argument(
        "--manifest",
        required=True,
        help="Output manifest CSV path (train/val/test written alongside).",
    )
    parser.add_argument(
        "--target-sr",
        type=int,
        default=TARGET_SR,
        help="Uniform output sample rate (Hz).",
    )
    parser.add_argument(
        "--head-trim-ms",
        type=int,
        default=50,
        help="Milliseconds trimmed from each head (DC-settle margin).",
    )
    args = parser.parse_args(argv)

    raw_dir = Path(args.raw_dir)
    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    rows: list[dict] = []
    for npz_path in sorted(raw_dir.glob("*.npz")):
        m = _BIAS_RE.match(npz_path.stem)
        if m is None:
            # Skip files that don't follow <stem>_bias<value>.npz.
            continue
        stem = m.group("stem")
        bias = float(m.group("bias"))

        wet_path = out_dir / f"{npz_path.stem}.wav"
        resample_to_grid(
            npz_path,
            wet_path,
            target_sr=args.target_sr,
            head_trim_ms=args.head_trim_ms,
        )
        info = sf.info(str(wet_path))
        rows.append(
            {
                "dry_path": f"{stem}.wav",
                "wet_path": str(wet_path),
                "bias_value": bias,
                "duration_s": info.duration,
                "stim_kind": _infer_stim_kind(stem),
            }
        )

    manifest = Path(args.manifest)
    write_manifest(rows, manifest)
    split_manifest(manifest, manifest.parent)
    print(f"wrote manifest: {manifest} ({len(rows)} rows)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
