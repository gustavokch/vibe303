"""Rebuild scratch/nam_sweep/ds48 (48 kHz) from the 9-gain wet_raw reamp.

Mirrors the original ds48 pipeline:
  1. data_gen.resample.main  -> 48 kHz wet WAVs + an aligned (co-head-trimmed)
     dry WAV + a 5-column manifest.csv. Co-trimming BOTH heads by the same
     50 ms keeps dry[i] <-> wet[i] aligned (the training loader reads dry and
     wet at the SAME frame index; see train/dataset.py:173-174).
  2. assert dry_sr == wet_sr == 48000 for every clip (dry-resample-bug guard).
  3. temporal_split -> train/val/test.csv with frame_start/frame_end columns,
     matching the existing T3K split schema the loader already consumes.
"""
from __future__ import annotations

from pathlib import Path

import pandas as pd
import soundfile as sf

from data_gen.resample import main as resample_main, temporal_split

ROOT = Path(__file__).resolve().parents[2]  # repo root (ba283-nam-lstm)
NS = "scratch/nam_sweep"
RAW = f"{NS}/wet_raw"
OUT = f"{NS}/ds48"
DRY = f"{NS}/dry48"
MANIFEST = f"{OUT}/manifest.csv"
TARGET_SR = 48_000

EXPECTED_GAINS = [0.0, 0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875, 1.0]


def main() -> int:
    # 1. resample raw npz -> 48 kHz WAVs + aligned dry + manifest.
    rc = resample_main(
        [
            "--raw-dir", RAW,
            "--out-dir", OUT,
            "--manifest", MANIFEST,
            "--dry-dir", DRY,
            "--target-sr", str(TARGET_SR),
            # T3K is a single dry stimulus across 9 gains; the by-stimulus split
            # needs >=3 stimuli. Skip it here and split temporally below (step 4).
            "--no-split",
        ]
    )
    if rc != 0:
        raise SystemExit(f"resample.main returned {rc}")

    df = pd.read_csv(MANIFEST)
    print(f"manifest rows: {len(df)}")

    # 2. SR-equality assertion for EVERY clip pair (dry-resample-bug guard).
    for _, row in df.iterrows():
        dry_sr = sf.info(str(ROOT / row["dry_path"])).samplerate
        wet_sr = sf.info(str(ROOT / row["wet_path"])).samplerate
        assert dry_sr == wet_sr == TARGET_SR, (
            f"SR mismatch: dry={dry_sr} wet={wet_sr} target={TARGET_SR} "
            f"({row['dry_path']} / {row['wet_path']})"
        )
    print(f"SR assertion OK: all {len(df)} pairs at {TARGET_SR} Hz "
          "(dry_sr == wet_sr == 48000)")

    # 3. gain coverage check.
    gains = sorted(round(float(g), 3) for g in df["gain_value"].unique())
    print(f"gain_values ({len(gains)}): {gains}")
    assert gains == [round(g, 3) for g in EXPECTED_GAINS], (
        f"gain set mismatch: got {gains}, want {EXPECTED_GAINS}"
    )

    # 4. temporal train/val/test split (80/10/10, frame_start/frame_end cols).
    paths = temporal_split(MANIFEST, OUT)
    for name, p in paths.items():
        n = len(pd.read_csv(p))
        print(f"  {name}.csv: {n} rows -> {p}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
