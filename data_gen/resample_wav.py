"""Resample an *already-rendered* WAV dataset to a new uniform sample rate.

Unlike :mod:`data_gen.resample` (which PCHIP-interpolates raw variable-timestep
SPICE ``.npz`` transients onto a grid), this module takes a dataset that is
already a set of uniform-rate float WAVs plus a manifest, and produces a
sample-rate-converted copy with a remapped manifest. It exists because the
ablation subsets (``base44k``, ``c4_88k``) are faithful downsamples of the
*validated* 96 kHz gain dataset — re-rendering them through SPICE would diverge
from the B0 reference, so the existing wet WAVs must be resampled directly.

Resampling uses the SoX resampler (``soxr``) at the **VHQ** (very-high-quality)
preset: a long, steep anti-alias FIR with excellent stopband rejection and flat
passband — the quality bar for the 96 kHz -> 48 kHz NAM-grid conversion. dry and
wet of a pair are resampled by the identical ratio with the identical filter, so
their relative head-trim alignment is preserved exactly.
"""
from __future__ import annotations

import argparse
from pathlib import Path

import numpy as np
import pandas as pd
import soundfile as sf
import soxr

from data_gen.resample import split_manifest, write_manifest

# SoX resampler quality preset. VHQ = very high quality (longest FIR, deepest
# stopband). Used for every dry/wet conversion so the 48 kHz dataset is a
# transparent downsample of the validated 96 kHz one.
SOXR_QUALITY = "VHQ"


def resample_wav(
    src: Path | str,
    dst: Path | str,
    target_sr: int,
) -> int:
    """Resample one float WAV ``src`` to ``target_sr``, write ``dst`` (32-bit float).

    Mono is enforced (stereo is averaged). Resampling is soxr VHQ. When the
    source already sits at ``target_sr`` the samples are copied verbatim (no
    needless filtering). Returns the written frame count.
    """
    dst = Path(dst)
    data, sr = sf.read(str(src), dtype="float32")
    if data.ndim == 2:
        data = data.mean(axis=1)
    if sr != target_sr:
        data = soxr.resample(
            data, int(sr), int(target_sr), quality=SOXR_QUALITY
        ).astype(np.float32)
    dst.parent.mkdir(parents=True, exist_ok=True)
    sf.write(dst, np.ascontiguousarray(data, dtype=np.float32), target_sr, subtype="FLOAT")
    return len(data)


def _local(path: str, root: Path) -> Path:
    """Map a manifest path (any prefix) to ``root/<dry|wet>/<basename>``."""
    p = Path(path)
    return root / p.parent.name / p.name


def build_resampled_subset(
    manifest: pd.DataFrame,
    src_root: Path | str,
    out_dir: Path | str,
    target_sr: int,
) -> Path:
    """Resample every dry/wet pair in ``manifest`` to ``target_sr`` under ``out_dir``.

    ``manifest`` rows carry ``dry_path``/``wet_path`` (resolved against
    ``src_root`` by basename), ``gain_value``, ``stim_kind``. Writes resampled
    WAVs to ``out_dir/{dry,wet}/``, a remapped ``manifest.csv`` with local
    relative paths, and a leakage-free ``train/val/test`` split. Each dry stem is
    resampled once and reused across its gains. Returns ``out_dir``.
    """
    src_root = Path(src_root)
    out_dir = Path(out_dir)
    (out_dir / "dry").mkdir(parents=True, exist_ok=True)
    (out_dir / "wet").mkdir(parents=True, exist_ok=True)

    dry_done: dict[str, Path] = {}
    rows: list[dict] = []
    for _, row in manifest.iterrows():
        wet_src = _local(row["wet_path"], src_root)
        wet_dst = out_dir / "wet" / wet_src.name
        n_wet = resample_wav(wet_src, wet_dst, target_sr)

        dry_src = _local(row["dry_path"], src_root)
        if dry_src.name not in dry_done:
            dry_dst = out_dir / "dry" / dry_src.name
            resample_wav(dry_src, dry_dst, target_sr)
            dry_done[dry_src.name] = dry_dst
        dry_dst = dry_done[dry_src.name]

        # Guard the past dry-resample bug: a dry left at the source rate while
        # the wet is resampled decorrelates the frame-indexed (dry[i], wet[i])
        # pairs and collapses training to zero output (ESR -> 1.0). Both legs
        # went through resample_wav(target_sr); assert the WRITTEN files agree.
        dry_sr = sf.info(str(dry_dst)).samplerate
        wet_sr = sf.info(str(wet_dst)).samplerate
        if not (dry_sr == wet_sr == target_sr):
            raise ValueError(
                f"sample-rate mismatch after resample: dry={dry_sr} wet={wet_sr} "
                f"target={target_sr} ({dry_dst.name} / {wet_dst.name})"
            )

        rows.append(
            {
                "dry_path": str(dry_dst),
                "wet_path": str(wet_dst),
                "gain_value": row["gain_value"],
                "duration_s": n_wet / target_sr,
                "stim_kind": row["stim_kind"],
            }
        )

    manifest_path = out_dir / "manifest.csv"
    write_manifest(rows, manifest_path)
    split_manifest(manifest_path, out_dir)
    return out_dir


def _select_subset(
    df: pd.DataFrame,
    per_kind: dict[str, int],
) -> pd.DataFrame:
    """Keep all gains for the first ``per_kind[k]`` sorted stems of each kind."""
    df = df.copy()
    df["_stem"] = df["wet_path"].map(lambda p: Path(p).name.split("_gain")[0])
    keep_stems: set[str] = set()
    for kind, n in per_kind.items():
        stems = sorted(df.loc[df["stim_kind"] == kind, "_stem"].unique())
        keep_stems.update(stems[:n])
    sub = df[df["_stem"].isin(keep_stems)].drop(columns="_stem").reset_index(drop=True)
    return sub


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Resample an already-rendered WAV dataset to a new sample rate.",
    )
    parser.add_argument("--manifest", required=True, help="Source manifest CSV.")
    parser.add_argument(
        "--src-root",
        required=True,
        help="Local root holding dry/ and wet/ WAVs the manifest references.",
    )
    parser.add_argument("--out-dir", required=True, help="Output subset directory.")
    parser.add_argument("--target-sr", type=int, required=True)
    parser.add_argument(
        "--per-kind",
        default="impulse:1,music:6,sweep:4,synth:4",
        help="kind:count pairs; keep all gains for the first <count> stems per kind.",
    )
    parser.add_argument(
        "--full",
        action="store_true",
        help="resample the ENTIRE manifest (ignore --per-kind); used to build the "
        "full 48 kHz training set, not an ablation subset.",
    )
    args = parser.parse_args(argv)

    df = pd.read_csv(args.manifest)
    if args.full:
        sub = df.reset_index(drop=True)
    else:
        per_kind = {
            k: int(v)
            for k, v in (pair.split(":") for pair in args.per_kind.split(","))
        }
        sub = _select_subset(df, per_kind)
    out = build_resampled_subset(sub, args.src_root, args.out_dir, args.target_sr)
    n_stems = sub["dry_path"].nunique()
    print(f"wrote {out} ({len(sub)} rows, {n_stems} stems) @ {args.target_sr} Hz")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
