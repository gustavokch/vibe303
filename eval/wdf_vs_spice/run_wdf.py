"""
Drive the WDF binary (~/Git/ba283_wdf/ba283_sim) on a WAV file.

The binary has hardcoded filenames (``input.wav`` / ``output.wav``) relative
to the working directory.  To avoid touching the originals (the repo copy of
``input.wav`` is read-only), we copy the binary and the dry WAV to a private
temp directory and run from there.
"""
from __future__ import annotations

import shutil
import subprocess
import tempfile
from pathlib import Path

import numpy as np
import soundfile as sf

WDF_DIR = Path.home() / "Git/ba283_wdf"
BINARY = WDF_DIR / "ba283_sim"


def build_if_needed() -> None:
    """Build the WDF binary with clang++ if it is not already present."""
    if BINARY.exists():
        return
    print("[run_wdf] building ba283_sim with clang++...")
    subprocess.check_call(
        [
            "clang++",
            "-std=c++17",
            "-O2",
            "-I",
            "chowdsp_wdf/single_include",
            "main.cpp",
            "-o",
            "ba283_sim",
        ],
        cwd=WDF_DIR,
    )
    print("[run_wdf] build done.")


def run_wdf(dry_wav: Path, rgain: float = 1200.0) -> tuple[np.ndarray, int]:
    """Run the WDF model on *dry_wav* and return (samples_float32, sample_rate).

    Copies the binary and the dry WAV to a temp directory so the original
    ``input.wav`` (which may be read-only) is never modified.

    Args:
        dry_wav: Path to a 96 kHz mono float WAV (peak ≤ 0.5 V).
        rgain:   Gain resistor in ohms (CLI argument to ba283_sim).

    Returns:
        Tuple of (samples as float32 ndarray, sample_rate as int).
    """
    build_if_needed()
    dry_wav = Path(dry_wav)

    with tempfile.TemporaryDirectory(prefix="wdf_run_") as _tmp:
        tmp = Path(_tmp)
        # Binary + dry WAV are the only files the process needs.
        shutil.copy2(BINARY, tmp / "ba283_sim")
        shutil.copy2(dry_wav, tmp / "input.wav")

        result = subprocess.run(
            [str(tmp / "ba283_sim"), str(rgain)],
            cwd=tmp,
            capture_output=True,
            text=True,
            timeout=300,
        )
        if result.returncode != 0:
            raise RuntimeError(
                f"WDF binary failed (rc={result.returncode}):\n{result.stderr}"
            )

        out_wav = tmp / "output.wav"
        if not out_wav.exists():
            raise RuntimeError("WDF binary succeeded but output.wav was not written.")

        samples, sr = sf.read(out_wav, dtype="float32")
        if samples.ndim > 1:
            samples = samples[:, 0]
        return np.asarray(samples, dtype=np.float32), int(sr)
