#!/usr/bin/env python3
"""Expand the BA283 training set: dense level grid + real music -> SPICE -> manifest.

Run this on a BEEFY machine (many cores + RAM): the SPICE transient batch is the
cost, and PySpice holds each whole transient in RAM. The earlier 45-sim set
overfit because it had only ~7 unique train stimuli and no coverage of the loud
(clipping) levels. This builds a far larger, level-dense library so the model
sees the full drive range -- including hard clipping -- on varied content.

Three stages (each skippable for resumption):

  1. generate  dense synthetic dry stimuli + (optional) music at several drive
                levels, into the dry dir.
  2. spice      run the netlist over every dry x bias -> data/wet_raw/*.npz
                (parallel, one fresh ngspice process per sim).
  3. resample   PCHIP -> 96 kHz WAVs, head-aligned dry, manifest + leakage-free
                by-stimulus train/val/test split.

Usage (typical):

  python scripts/expand_dataset.py --music-dir ~/ba283_music --workers 16

  # smaller/faster first pass (half-length clips):
  python scripts/expand_dataset.py --music-dir ~/ba283_music --workers 16 \
      --duration-scale 0.5

  # resume after generate already ran:
  python scripts/expand_dataset.py --skip-generate --workers 16

Cost scales with (#stimuli x #biases x clip-duration). Print-only estimate is
shown before stage 2; pass --yes to skip the confirmation.
"""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path

import numpy as np
import soundfile as sf

# Run standalone (python scripts/expand_dataset.py) without needing PYTHONPATH.
sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

# Reuse the validated generators + scaling so the dense set matches the existing
# stimulus conventions exactly (96 kHz mono float32, peak = 0.5 * 10**(dB/20)).
from data_gen.stimulus import (  # noqa: E402
    SAMPLE_RATE,
    SEED,
    _scale_to_dbfs,
    filtered_impulses,
    load_dry,
    log_sine_sweep,
    multisine_schroeder,
    pink_noise_bursts,
    two_tone_imd,
)

REPO = Path(__file__).resolve().parent.parent


def build_dense(duration_scale: float) -> dict[str, np.ndarray]:
    """Dense level-grid synthetic library as ``{name: float32}``.

    The sweep is the workhorse for a nonlinear preamp: a 20 Hz-20 kHz log sweep
    at a DENSE set of levels traces the level-dependent transfer/clipping curve.
    Pink-noise bursts, two-tone IMD and a multisine add broadband + intermod
    content; filtered impulses add transients. Every level appears so the model
    never has to extrapolate the clipping region (the prior split's failure).
    """
    sweep_s = 30.0 * duration_scale
    short_s = 20.0 * duration_scale
    multi_s = 60.0 * duration_scale
    segs: dict[str, np.ndarray] = {}

    # Sweeps: dense level grid, denser toward the loud/clipping end.
    sweep_levels = [-24, -20, -16, -13, -10, -8, -6, -4, -3, -2, -1]
    for lv in sweep_levels:
        segs[f"sweep_m{abs(lv)}"] = _scale_to_dbfs(
            log_sine_sweep(sweep_s, 20.0, 20_000.0, 0.0), float(lv)
        )

    # Pink-noise bursts: a few levels x distinct seeds (reproducible).
    for i, lv in enumerate((-18, -12, -6, -3)):
        for s in range(2):
            segs[f"pink_m{abs(lv)}_s{s}"] = _scale_to_dbfs(
                pink_noise_bursts(short_s, 200, 800, float(lv), seed=SEED + 10 * i + s),
                float(lv),
            )

    # Two-tone IMD: musical-interval pairs x levels.
    pairs = ((220.0, 277.18), (330.0, 495.0), (440.0, 660.0))
    for pi, (f1, f2) in enumerate(pairs):
        for lv in (-12, -6, -3):
            segs[f"twotone_p{pi}_m{abs(lv)}"] = _scale_to_dbfs(
                two_tone_imd(short_s, f1, f2, 0.0), float(lv)
            )

    # Schroeder multisine: broadband, a couple of levels.
    for lv in (-12, -6, -3):
        segs[f"multisine_m{abs(lv)}"] = _scale_to_dbfs(
            multisine_schroeder(multi_s, 40, 20.0, 20_000.0), float(lv)
        )

    # Transients.
    segs["impulse"] = _scale_to_dbfs(filtered_impulses(short_s), -6.0)
    return segs


def build_music(music_dir: Path, levels: list[int]) -> dict[str, np.ndarray]:
    """Each music clip resampled to 96 kHz mono, written at several drive levels."""
    segs: dict[str, np.ndarray] = {}
    if not music_dir:
        return segs
    clips = sorted(
        p for p in music_dir.iterdir() if p.suffix.lower() in (".wav", ".flac", ".aif", ".aiff")
    )
    if not clips:
        print(f"WARNING: no audio files in {music_dir}", file=sys.stderr)
    for ci, clip in enumerate(clips):
        samples, _ = load_dry(clip)
        for lv in levels:
            segs[f"music{ci}_{clip.stem}_m{abs(lv)}"] = _scale_to_dbfs(samples, float(lv))
    return segs


def write_dry(segs: dict[str, np.ndarray], out_dry: Path, overwrite: bool) -> int:
    out_dry.mkdir(parents=True, exist_ok=True)
    written = 0
    for name, sig in segs.items():
        p = out_dry / f"{name}.wav"
        if p.exists() and not overwrite:
            continue
        sf.write(str(p), sig.astype(np.float32), SAMPLE_RATE, subtype="FLOAT")
        written += 1
    return written


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out-dry", type=Path, default=REPO / "data" / "dry")
    ap.add_argument("--music-dir", type=Path, default=None, help="folder of music/DI clips (wav/flac/aiff)")
    ap.add_argument("--music-levels", type=int, nargs="*", default=[-12, -6, -3, -1])
    ap.add_argument("--biases", default="0.0,0.25,0.5,0.75,1.0")
    ap.add_argument("--workers", type=int, default=8)
    ap.add_argument("--duration-scale", type=float, default=1.0)
    ap.add_argument("--overwrite", action="store_true", help="regenerate dry WAVs even if present")
    ap.add_argument("--skip-generate", action="store_true")
    ap.add_argument("--skip-spice", action="store_true")
    ap.add_argument("--skip-resample", action="store_true")
    ap.add_argument("--yes", action="store_true", help="skip the pre-SPICE confirmation")
    args = ap.parse_args()

    out_dry = args.out_dry
    n_biases = len([b for b in args.biases.split(",") if b.strip()])

    # --- Stage 1: generate dry ---
    if not args.skip_generate:
        segs = build_dense(args.duration_scale)
        segs.update(build_music(args.music_dir, args.music_levels))
        w = write_dry(segs, out_dry, args.overwrite)
        print(f"[generate] {len(segs)} dry stimuli defined, {w} written to {out_dry}")
    n_dry = len(sorted(out_dry.glob("*.wav")))
    print(f"[plan] {n_dry} dry stimuli x {n_biases} biases = {n_dry * n_biases} SPICE sims")

    # --- Stage 2: SPICE batch ---
    if not args.skip_spice:
        if not args.yes:
            resp = input(f"Run {n_dry * n_biases} SPICE sims on {args.workers} workers? [y/N] ")
            if resp.strip().lower() not in ("y", "yes"):
                print("aborted before SPICE.")
                return 1
        cmd = [
            sys.executable, "-m", "data_gen.run_spice",
            "--manifest", str(out_dry),
            "--biases", args.biases,
            "--workers", str(args.workers),
        ]
        print("[spice]", " ".join(cmd))
        rc = subprocess.call(cmd, cwd=REPO)
        if rc != 0:
            print(f"SPICE batch returned {rc} (some sims may have failed; check data/failed.csv)")

    # --- Stage 3: resample + manifest + split ---
    if not args.skip_resample:
        cmd = [
            sys.executable, "-m", "data_gen.resample",
            "--raw-dir", str(REPO / "data" / "wet_raw"),
            "--out-dir", str(REPO / "data" / "wet"),
            "--dry-dir", str(out_dry),
            "--manifest", str(REPO / "data" / "manifest.csv"),
        ]
        print("[resample]", " ".join(cmd))
        rc = subprocess.call(cmd, cwd=REPO)
        if rc != 0:
            return rc

    print("\nDone. Next: bash colab/make_bundle.sh  ->  train on GPU.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
