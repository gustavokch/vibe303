"""Phase 1 (data generation) gate: fast end-to-end pipeline check.

This is NOT a pytest test. It runs real ngspice simulations and is too slow
for the unit suite. It proves the FULL data-generation chain works end to end
and produces a valid, trainable, leakage-free dataset manifest:

    generate short dry stimuli  ->  SPICE batch (x3 biases)
        ->  PCHIP resample to 96 kHz  ->  manifest  ->  4/1/1 split

To stay gate-appropriate (seconds, not hours) it uses six SHORT (~0.15 s)
dry stimuli of varied generators and levels instead of the multi-minute
production segments. Six distinct dry files give the leakage-free-by-file
split enough unique stimuli for a non-empty train/val/test partition.

Run:
    uv run python tests/gate_phase1.py

Exits 0 and prints ``PHASE 1 GATE: PASS`` on success; exits 1 and prints
``PHASE 1 GATE: FAIL`` with the reason on any failure.
"""

from __future__ import annotations

import multiprocessing as mp
import shutil
import sys
from pathlib import Path

import numpy as np
import soundfile as sf

# Allow `python tests/gate_phase1.py` from the repo root: when run as a script,
# sys.path[0] is tests/, not the repo root, so the data_gen package is not found.
_REPO_ROOT = Path(__file__).resolve().parent.parent
if str(_REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT))

from data_gen.resample import resample_to_grid, split_manifest, write_manifest  # noqa: E402
from data_gen.run_spice import simulate_one  # noqa: E402
from data_gen.stimulus import (  # noqa: E402
    SAMPLE_RATE,
    _scale_to_dbfs,
    log_sine_sweep,
    multisine_schroeder,
    pink_noise_bursts,
    two_tone_imd,
)

# Working dirs (all under gitignored data/).
GATE_DIR = Path("data/gate")
DRY_DIR = GATE_DIR / "dry"
WET_RAW_DIR = GATE_DIR / "wet_raw"
WET_DIR = GATE_DIR / "wet"
MANIFEST_CSV = GATE_DIR / "manifest.csv"
FAILED_CSV = GATE_DIR / "failed.csv"

# Short clips: ~0.15 s so each SPICE sim is ~1-2 s.
CLIP_S = 0.15
BIASES = [0.0, 0.5, 1.0]
WORKERS = 3
# head_trim_ms=10 < 150 ms clip, so the real trim path runs and output is
# non-empty (vs. the default 50 ms, which would still fit but we keep margin).
HEAD_TRIM_MS = 10


class GateError(Exception):
    """Raised when a gate assertion fails."""


def _make_dry_stimuli() -> dict[str, np.ndarray]:
    """Build six short, varied dry stimuli (mono float32, peak <= 0.5)."""
    return {
        "sweep_a": _scale_to_dbfs(
            log_sine_sweep(CLIP_S, 100.0, 5000.0, 0.0), -12.0
        ),
        "sweep_b": _scale_to_dbfs(
            log_sine_sweep(CLIP_S, 200.0, 8000.0, 0.0), -6.0
        ),
        "pink_a": _scale_to_dbfs(
            pink_noise_bursts(CLIP_S, 50.0, 50.0, -12.0, seed=1), -12.0
        ),
        "pink_b": _scale_to_dbfs(
            pink_noise_bursts(CLIP_S, 50.0, 50.0, -3.0, seed=2), -3.0
        ),
        "multisine_a": _scale_to_dbfs(
            multisine_schroeder(CLIP_S, 20, 100.0, 5000.0), -12.0
        ),
        "twotone_a": _scale_to_dbfs(
            two_tone_imd(CLIP_S, 300.0, 450.0, 0.0), -6.0
        ),
    }


def _infer_stim_kind(stem: str) -> str:
    """Map a dry stem prefix to a coarse stim_kind label."""
    low = stem.lower()
    if low.startswith("sweep"):
        return "sweep"
    if low.startswith("pink"):
        return "noise"
    if low.startswith("multisine"):
        return "multisine"
    if low.startswith("twotone"):
        return "imd"
    return "synth"


def _run_pair(args: tuple[Path, float]) -> tuple[Path, float, str | None]:
    """Worker: simulate one (dry, bias) pair into WET_RAW_DIR; return error or None."""
    dry, bias = args
    out = WET_RAW_DIR / f"{dry.stem}_bias{bias:.2f}.npz"
    try:
        simulate_one(dry, bias, out)
        return (dry, bias, None)
    except Exception as exc:
        return (dry, bias, f"{type(exc).__name__}: {exc}")


def main() -> int:
    """Run the Phase 1 gate; return process exit code (0 pass, 1 fail)."""
    try:
        # 1. Clean + recreate working dirs for deterministic re-runs.
        shutil.rmtree(GATE_DIR, ignore_errors=True)
        for d in (DRY_DIR, WET_RAW_DIR, WET_DIR):
            d.mkdir(parents=True, exist_ok=True)

        # 2. Generate the short dry stimuli.
        stimuli = _make_dry_stimuli()
        dry_paths: list[Path] = []
        for name, sig in stimuli.items():
            peak = float(np.max(np.abs(sig)))
            if peak > 0.5 + 1e-6:
                raise GateError(f"dry stimulus {name} peak {peak:.4f} exceeds 0.5")
            path = DRY_DIR / f"{name}.wav"
            sf.write(str(path), sig, SAMPLE_RATE, subtype="FLOAT")
            dry_paths.append(path)
        n_dry = len(dry_paths)
        print(f"generated {n_dry} short dry stimuli in {DRY_DIR}")

        # 3. SPICE batch: every dry x every bias.
        pairs = [(dry, b) for dry in dry_paths for b in BIASES]
        n_expected = len(pairs)
        print(f"running SPICE batch: {n_expected} pairs (workers={WORKERS})...")
        with mp.Pool(processes=WORKERS, maxtasksperchild=1) as pool:
            results = pool.map(_run_pair, pairs)

        failures = [(d, b, e) for (d, b, e) in results if e is not None]
        n_ok = n_expected - len(failures)
        if failures:
            with FAILED_CSV.open("w", newline="") as fh:
                fh.write("stim,bias,error\n")
                for d, b, e in failures:
                    fh.write(f"{d},{b:.2f},{e}\n")
            print(f"WARNING: {len(failures)} SPICE pair(s) failed; see {FAILED_CSV}:")
            for d, b, e in failures:
                print(f"  {d.name} bias={b:.2f}: {e}")
        print(f"SPICE batch done: {n_ok}/{n_expected} ok, {len(failures)} failed")

        # Tolerate a few failures; fail the gate only if >20% failed.
        if len(failures) > 0.20 * n_expected:
            raise GateError(
                f"{len(failures)}/{n_expected} SPICE pairs failed "
                f"(>20% threshold); see {FAILED_CSV}"
            )

        # 4. Resample every produced npz -> wet WAV, accumulating manifest rows.
        rows: list[dict] = []
        npz_files = sorted(WET_RAW_DIR.glob("*.npz"))
        if not npz_files:
            raise GateError("no wet_raw .npz files produced by SPICE batch")
        for npz in npz_files:
            # stem is "<dry_stem>_bias<value>".
            stem, _, bias_str = npz.stem.rpartition("_bias")
            wet_wav = WET_DIR / f"{npz.stem}.wav"
            resample_to_grid(npz, wet_wav, head_trim_ms=HEAD_TRIM_MS)
            info = sf.info(str(wet_wav))
            rows.append(
                {
                    "dry_path": f"{stem}.wav",
                    "wet_path": str(wet_wav),
                    "bias_value": float(bias_str),
                    "duration_s": info.duration,
                    "stim_kind": _infer_stim_kind(stem),
                }
            )
        print(f"resampled {len(rows)} wet WAVs into {WET_DIR}")

        # 5. Write manifest + leakage-free split.
        write_manifest(rows, MANIFEST_CSV)
        split_paths = split_manifest(
            MANIFEST_CSV, GATE_DIR, ratios=(0.6, 0.2, 0.2), seed=0
        )

        # 6. Assertions.
        import pandas as pd

        if not MANIFEST_CSV.is_file():
            raise GateError(f"manifest not written: {MANIFEST_CSV}")
        manifest_df = pd.read_csv(MANIFEST_CSV)
        if len(manifest_df) < 1:
            raise GateError("manifest has no data rows")

        split_dfs: dict[str, pd.DataFrame] = {}
        for name in ("train", "val", "test"):
            path = split_paths[name]
            if not path.is_file():
                raise GateError(f"split file missing: {path}")
            df = pd.read_csv(path)
            if len(df) < 1:
                raise GateError(f"split '{name}' is empty ({path})")
            split_dfs[name] = df

        # Pairwise-disjoint dry_path sets (leakage-free).
        sets = {n: set(df["dry_path"]) for n, df in split_dfs.items()}
        for a, b in (("train", "val"), ("train", "test"), ("val", "test")):
            overlap = sets[a] & sets[b]
            if overlap:
                raise GateError(f"leakage: {a}/{b} share dry stimuli {sorted(overlap)}")

        # At least one wet WAV decodes at 96 kHz with finite, non-empty samples.
        decoded_ok = False
        for wet in sorted(WET_DIR.glob("*.wav")):
            data, sr = sf.read(str(wet), dtype="float32")
            if sr == 96000 and data.size > 0 and np.all(np.isfinite(data)):
                decoded_ok = True
                break
        if not decoded_ok:
            raise GateError("no wet WAV decoded at sr=96000 with finite, >0 samples")

        split_sizes = {n: len(df) for n, df in split_dfs.items()}
        print("PHASE 1 GATE: PASS")
        print(
            f"  dry files:    {n_dry}\n"
            f"  SPICE ok:     {n_ok}/{n_expected} ({len(failures)} failed)\n"
            f"  manifest rows: {len(manifest_df)}\n"
            f"  split sizes:  train={split_sizes['train']} "
            f"val={split_sizes['val']} test={split_sizes['test']} "
            f"(unique dry: train={len(sets['train'])} "
            f"val={len(sets['val'])} test={len(sets['test'])})"
        )
        return 0

    except GateError as exc:
        print(f"PHASE 1 GATE: FAIL\n  reason: {exc}")
        return 1
    except Exception as exc:
        print(f"PHASE 1 GATE: FAIL\n  unexpected error: {type(exc).__name__}: {exc}")
        return 1


if __name__ == "__main__":
    sys.exit(main())
