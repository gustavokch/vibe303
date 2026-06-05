#!/usr/bin/env python3
"""Expand the BA283 training set: dense level grid + real music -> SPICE -> manifest.

Run on a beefy machine. Two output modes:

* default (subprocess stages): writes data/wet_raw/*.npz then resamples -- simple,
  but the raw variable-timestep npz are HUGE (~150 MB each for a 30 s clip) and
  all co-exist until resample, so a few hundred sims need tens of GB.

* --out-base DIR (STREAMING, recommended): each (dry, bias) is simulated to a
  LOCAL scratch npz, immediately PCHIP-resampled to a 96 kHz wav under DIR, and
  the npz is deleted. Peak local disk = workers x one npz (~1 GB), and only the
  small final dataset (wet wavs + aligned dry + manifest + splits) lands in DIR.
  DIR can be an rclone/Drive mount so Colab reads the dataset directly.

Stages:
  1. generate  dense synthetic dry + (optional) music at several levels -> local
                raw-dry dir (the simulator reads these).
  2+3. stream   sim -> resample -> delete, in a worker pool (one fresh ngspice
                process per sim), then aligned dry + manifest + leakage-free split.

Usage:
  python scripts/expand_dataset.py --music-dir ~/ba283_music --workers 6 \
      --duration-scale 0.5 --out-base /home/ubuntu/drive/ba283_spice --yes
"""

from __future__ import annotations

import argparse
import multiprocessing as mp
import subprocess
import sys
from pathlib import Path

import numpy as np
import soundfile as sf

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

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
SCRATCH = Path("/tmp/ba283_scratch")  # local npz scratch (streaming)


def build_dense(duration_scale: float) -> dict[str, np.ndarray]:
    """Dense level-grid synthetic library as ``{name: float32}``."""
    sweep_s = 30.0 * duration_scale
    short_s = 20.0 * duration_scale
    multi_s = 60.0 * duration_scale
    segs: dict[str, np.ndarray] = {}
    for lv in [-24, -20, -16, -13, -10, -8, -6, -4, -3, -2, -1]:
        segs[f"sweep_m{abs(lv)}"] = _scale_to_dbfs(
            log_sine_sweep(sweep_s, 20.0, 20_000.0, 0.0), float(lv)
        )
    for i, lv in enumerate((-18, -12, -6, -3)):
        for s in range(2):
            segs[f"pink_m{abs(lv)}_s{s}"] = _scale_to_dbfs(
                pink_noise_bursts(short_s, 200, 800, float(lv), seed=SEED + 10 * i + s),
                float(lv),
            )
    for pi, (f1, f2) in enumerate(((220.0, 277.18), (330.0, 495.0), (440.0, 660.0))):
        for lv in (-12, -6, -3):
            segs[f"twotone_p{pi}_m{abs(lv)}"] = _scale_to_dbfs(
                two_tone_imd(short_s, f1, f2, 0.0), float(lv)
            )
    for lv in (-12, -6, -3):
        segs[f"multisine_m{abs(lv)}"] = _scale_to_dbfs(
            multisine_schroeder(multi_s, 40, 20.0, 20_000.0), float(lv)
        )
    segs["impulse"] = _scale_to_dbfs(filtered_impulses(short_s), -6.0)
    return segs


def build_music(music_dir: Path | None, levels: list[int]) -> dict[str, np.ndarray]:
    segs: dict[str, np.ndarray] = {}
    if not music_dir:
        return segs
    clips = sorted(
        p for p in music_dir.iterdir()
        if p.suffix.lower() in (".wav", ".flac", ".aif", ".aiff")
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


def _bias_tag(b: float) -> str:
    return f"{b:.2f}"


def _stream_one(task: tuple) -> tuple:
    """Worker: simulate one (dry,bias), resample to a wav, delete the npz.

    Runs in a fresh process (maxtasksperchild=1) so each ngspice shared-lib load
    is clean. Returns (stem, bias, wav_path_or_None, err_or_None).
    """
    dry_path, bias, wet_dir, head_trim_ms = task
    from data_gen.resample import resample_to_grid
    from data_gen.run_spice import simulate_one

    stem = Path(dry_path).stem
    npz = SCRATCH / f"{stem}_bias{_bias_tag(bias)}.npz"
    wav = Path(wet_dir) / f"{stem}_bias{_bias_tag(bias)}.wav"
    try:
        simulate_one(Path(dry_path), bias, npz)
        resample_to_grid(npz, wav, head_trim_ms=head_trim_ms)
        return (stem, bias, str(wav), None)
    except Exception as exc:  # keep the batch going; record the failure
        return (stem, bias, None, repr(exc))
    finally:
        try:
            npz.unlink()
        except FileNotFoundError:
            pass


LOCAL_OUT = Path("/tmp/ba283_out")  # local staging (libsndfile can't write to rclone)


def stream_expand(
    raw_dry: Path,
    biases: list[float],
    out_base: Path,
    workers: int,
    head_trim_ms: int = 50,
) -> None:
    import shutil

    from data_gen.resample import (
        _infer_stim_kind,
        split_manifest,
        write_aligned_dry,
        write_manifest,
    )

    SCRATCH.mkdir(parents=True, exist_ok=True)
    # Write everything to LOCAL staging first: libsndfile/pandas do seek-on-write
    # which fails on an rclone/FUSE mount ("System error"). We bulk-copy the
    # finished dataset to out_base at the end (sequential copy works on rclone).
    wet_dir = LOCAL_OUT / "wet"
    aligned_dir = LOCAL_OUT / "dry"
    wet_dir.mkdir(parents=True, exist_ok=True)
    aligned_dir.mkdir(parents=True, exist_ok=True)

    dry_files = sorted(raw_dry.glob("*.wav"))
    tasks = [(str(d), b, str(wet_dir), head_trim_ms) for d in dry_files for b in biases]
    total = len(tasks)
    print(f"[stream] {len(dry_files)} dry x {len(biases)} biases = {total} sims, "
          f"{workers} workers", flush=True)

    rows: list[tuple] = []
    fails: list[tuple] = []
    done = 0
    with mp.Pool(processes=workers, maxtasksperchild=1) as pool:
        for stem, bias, wav, err in pool.imap_unordered(_stream_one, tasks):
            done += 1
            if err is None:
                rows.append((stem, bias, wav))
            else:
                fails.append((stem, bias, err))
            if done % 10 == 0 or done == total:
                print(f"[stream] {done}/{total}  ok={len(rows)} fail={len(fails)}", flush=True)

    if fails:
        fp = out_base / "failed.csv"
        fp.write_text("stem,bias,error\n" + "\n".join(
            f'{s},{b},"{e}"' for s, b, e in fails))
        print(f"[stream] {len(fails)} sims failed -> {fp}", flush=True)

    # Aligned (head-trimmed) dry, once per unique stem.
    aligned: dict[str, Path] = {}
    for d in dry_files:
        aligned[d.stem] = write_aligned_dry(
            d, aligned_dir / f"{d.stem}_dry.wav", head_trim_ms=head_trim_ms
        )

    manifest_rows = []
    for stem, bias, wav in rows:
        wet_dur = sf.info(wav).duration
        manifest_rows.append({
            "dry_path": str(aligned[stem]),
            "wet_path": wav,
            "bias_value": bias,
            "duration_s": wet_dur,
            "stim_kind": _infer_stim_kind(stem),
        })
    manifest = LOCAL_OUT / "manifest.csv"
    write_manifest(manifest_rows, manifest)
    split_manifest(manifest, LOCAL_OUT)
    print(f"[stream] staged manifest ({len(manifest_rows)} rows) + split in {LOCAL_OUT}", flush=True)

    # Bulk-copy the finished dataset to out_base (Drive). Pure data copies
    # (copyfile, no metadata/utime/chmod) — sequential writes that rclone allows,
    # unlike libsndfile's seek-on-write or copy2's copystat.
    n_copied = 0
    for src in LOCAL_OUT.rglob("*"):
        if src.is_dir():
            continue
        dest = out_base / src.relative_to(LOCAL_OUT)
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(src, dest)
        n_copied += 1
    print(f"[stream] copied {n_copied} files -> {out_base}", flush=True)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out-dry", type=Path, default=REPO / "data" / "dry",
                    help="local raw-dry dir the simulator reads")
    ap.add_argument("--out-base", type=Path, default=None,
                    help="streaming output dir (e.g. an rclone/Drive mount); enables streaming")
    ap.add_argument("--music-dir", type=Path, default=None)
    ap.add_argument("--music-levels", type=int, nargs="*", default=[-12, -6, -3, -1])
    ap.add_argument("--biases", default="0.0,0.25,0.5,0.75,1.0")
    ap.add_argument("--workers", type=int, default=8)
    ap.add_argument("--duration-scale", type=float, default=1.0)
    ap.add_argument("--overwrite", action="store_true")
    ap.add_argument("--skip-generate", action="store_true")
    ap.add_argument("--yes", action="store_true")
    args = ap.parse_args()

    out_dry = args.out_dry
    biases = [float(b) for b in args.biases.split(",") if b.strip()]

    if not args.skip_generate:
        segs = build_dense(args.duration_scale)
        segs.update(build_music(args.music_dir, args.music_levels))
        w = write_dry(segs, out_dry, args.overwrite)
        print(f"[generate] {len(segs)} dry stimuli, {w} written to {out_dry}", flush=True)
    n_dry = len(sorted(out_dry.glob("*.wav")))
    print(f"[plan] {n_dry} dry x {len(biases)} biases = {n_dry * len(biases)} sims", flush=True)

    if not args.yes:
        resp = input(f"Run {n_dry * len(biases)} SPICE sims on {args.workers} workers? [y/N] ")
        if resp.strip().lower() not in ("y", "yes"):
            print("aborted.")
            return 1

    if args.out_base:
        stream_expand(out_dry, biases, args.out_base, args.workers)
        print(f"\nDone. Dataset under {args.out_base} (manifest + train/val/test.csv).")
        return 0

    # legacy non-streaming path (kept for small local runs)
    subprocess.call([sys.executable, "-m", "data_gen.run_spice",
                     "--manifest", str(out_dry), "--biases", args.biases,
                     "--workers", str(args.workers)], cwd=REPO)
    rc = subprocess.call([sys.executable, "-m", "data_gen.resample",
                          "--raw-dir", str(REPO / "data" / "wet_raw"),
                          "--out-dir", str(REPO / "data" / "wet"),
                          "--dry-dir", str(out_dry),
                          "--manifest", str(REPO / "data" / "manifest.csv")], cwd=REPO)
    print("\nDone. Next: bash colab/make_bundle.sh")
    return rc


if __name__ == "__main__":
    raise SystemExit(main())
