"""Build the C5 (9-pt gain grid) ablation subset by fresh SPICE renders.

C5 tests gain *density* + held-out-gain generalization, so it cannot be derived
by resampling the existing 6-pt gain dataset — the 4 held-out gains
``{0.125, 0.375, 0.625, 0.875}`` were never rendered. This re-renders the same
stimulus stems as ``base44k`` across a 9-pt grid at the card's fixed operating
bias (0.77), then resamples to 44.1 kHz.

Unlike :mod:`data_gen.run_spice`'s gain-batch mode (which names npz by R_KJ
*ohms*), this controls the manifest's ``gain_value`` as the **normalized** gain
in ``[0, 1]`` so the held-out-gain eval can filter on it. Output layout matches
the other ablation subsets:

    c5_9pt/
      dry/  wet/
      manifest.csv      # all 9 gains
      train.csv         # 5 trained gains {0, .25, .5, .75, 1}
      val.csv           # all 9 gains (held-out eval filters to the unseen 4)

Train/val here split by GAIN (same stimuli in both), not by stimulus — that is
the point of the held-out-gain probe.
"""
from __future__ import annotations

import argparse
import multiprocessing as mp
import os
from pathlib import Path

import pandas as pd
import soundfile as sf

from data_gen.netlist import gain_norm_to_rkj
from data_gen.resample import (
    _infer_stim_kind,
    resample_to_grid,
    write_aligned_dry,
)
from data_gen.run_spice import simulate_one

GRID = [0.0, 0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875, 1.0]
TRAINED = {0.0, 0.25, 0.5, 0.75, 1.0}
FIXED_BIAS = 0.77


def _render_worker(task: tuple[str, float, str]) -> tuple[str, float, str | None]:
    """Render one (dry_wav, norm_gain) -> raw npz at the fixed bias."""
    dry_wav, g_norm, out_npz = task
    try:
        simulate_one(
            Path(dry_wav), FIXED_BIAS, Path(out_npz), gain_rkj=gain_norm_to_rkj(g_norm)
        )
        return (dry_wav, g_norm, None)
    except Exception as exc:  # noqa: BLE001 - report, don't abort the batch
        return (dry_wav, g_norm, f"{type(exc).__name__}: {exc}")


def _gtag(g: float) -> str:
    return f"{g:.3f}"


def _npz_ok(path: str | Path) -> bool:
    """True iff ``path`` is a complete, readable render (t/v arrays present).

    A render SIGKILL'd mid-write (e.g. an OOM-killed worker) leaves a truncated
    or zero-byte ``.npz`` that *exists* but cannot be loaded. Resume must treat
    those as missing and re-render, not skip them — otherwise the resample pass
    blows up on ``np.load``.
    """
    p = Path(path)
    if not p.exists():
        return False
    try:
        import numpy as _np

        with _np.load(p) as d:
            return "t" in d and "v" in d and d["t"].shape == d["v"].shape
    except Exception:  # noqa: BLE001 - corrupt npz of any kind -> re-render
        p.unlink(missing_ok=True)
        return False


def build_c5(
    stems: list[str],
    dry_src_dir: Path | str,
    out_dir: Path | str,
    target_sr: int = 44100,
    workers: int | None = None,
) -> Path:
    """Render ``stems`` x 9-pt grid, resample to ``target_sr``, write the subset."""
    dry_src_dir = Path(dry_src_dir)
    out_dir = Path(out_dir)
    raw_dir = out_dir / "_raw"
    raw_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / "dry").mkdir(parents=True, exist_ok=True)
    (out_dir / "wet").mkdir(parents=True, exist_ok=True)
    if workers is None:
        workers = max(1, (os.cpu_count() or 2) - 2)

    # (dry_wav, gain, npz) render tasks — npz keyed by normalized gain.
    tasks: list[tuple[str, float, str]] = []
    for stem in stems:
        dry_wav = dry_src_dir / f"{stem}_dry.wav"
        for g in GRID:
            npz = raw_dir / f"{stem}_g{_gtag(g)}.npz"
            tasks.append((str(dry_wav), g, str(npz)))

    # PySpice holds each full transient in RAM, so high concurrency OOM-kills
    # the bigger (music) renders -> ngspice "run returned 1". Render with a
    # bounded worker count and retry stragglers at decreasing concurrency until
    # every npz exists (resume-safe: an already-written npz is skipped).
    pending = [t for t in tasks if not _npz_ok(t[2])]
    for w in (workers, max(workers // 2, 1), 1):
        if not pending:
            break
        with mp.Pool(processes=w, maxtasksperchild=1) as pool:
            pool.map(_render_worker, pending)
        pending = [t for t in tasks if not _npz_ok(t[2])]
    # A few (stim, gain) points may never converge in ngspice (broadband stimuli
    # at certain gains force the gear timestep to abort). Production tolerates
    # this via failed.csv; mirror that — drop the stragglers rather than fail the
    # whole subset, but cap it so a systemic break still surfaces loudly.
    max_drop = max(2, len(tasks) // 25)  # ~4% of 135
    if len(pending) > max_drop:
        raise RuntimeError(
            f"{len(pending)} C5 renders failing after retries (> {max_drop} "
            f"tolerated): {[(Path(t[0]).stem, t[1]) for t in pending[:5]]}"
        )
    if pending:
        dropped = [(Path(t[0]).stem.replace('_dry', ''), t[1]) for t in pending]
        print(f"WARNING: dropping {len(pending)} non-convergent C5 point(s): {dropped}")

    aligned_dry: dict[str, Path] = {}
    rows: list[dict] = []
    for stem in stems:
        dry_src = dry_src_dir / f"{stem}_dry.wav"
        kind = _infer_stim_kind(stem)
        if stem not in aligned_dry:
            aligned_dry[stem] = write_aligned_dry(
                dry_src, out_dir / "dry" / f"{stem}_dry.wav", target_sr=target_sr
            )
        for g in GRID:
            npz = raw_dir / f"{stem}_g{_gtag(g)}.npz"
            if not _npz_ok(npz):
                continue  # non-convergent point, dropped above
            wet = out_dir / "wet" / f"{stem}_gain{_gtag(g)}.wav"
            resample_to_grid(npz, wet, target_sr=target_sr)
            rows.append(
                {
                    "dry_path": str(aligned_dry[stem]),
                    "wet_path": str(wet),
                    "gain_value": g,
                    "duration_s": sf.info(str(wet)).duration,
                    "stim_kind": kind,
                }
            )

    df = pd.DataFrame(rows)
    df.to_csv(out_dir / "manifest.csv", index=False)
    df[df["gain_value"].isin(TRAINED)].to_csv(out_dir / "train.csv", index=False)
    df.to_csv(out_dir / "val.csv", index=False)  # all 9 gains for held-out eval

    # Drop the bulky raw npz once resampled.
    for npz in raw_dir.glob("*.npz"):
        npz.unlink()
    raw_dir.rmdir()
    return out_dir


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Build the C5 9-pt gain subset.")
    parser.add_argument(
        "--ref-manifest",
        required=True,
        help="base44k manifest; its stems are reused so C5 matches B0 stimuli.",
    )
    parser.add_argument(
        "--dry-src-dir",
        required=True,
        help="Directory of 96 kHz <stem>_dry.wav SPICE inputs.",
    )
    parser.add_argument("--out-dir", required=True)
    parser.add_argument("--target-sr", type=int, default=44100)
    parser.add_argument("--workers", type=int, default=None)
    args = parser.parse_args(argv)

    ref = pd.read_csv(args.ref_manifest)
    stems = sorted(
        {Path(p).name.replace("_dry.wav", "") for p in ref["dry_path"]}
    )
    out = build_c5(
        stems,
        args.dry_src_dir,
        args.out_dir,
        target_sr=args.target_sr,
        workers=args.workers,
    )
    n = len(pd.read_csv(out / "manifest.csv"))
    print(f"wrote {out} ({len(stems)} stems x {len(GRID)} gains = {n} rows)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
