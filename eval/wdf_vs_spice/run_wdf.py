"""
Drive the WDF binary (~/Git/ba283_wdf/ba283_sim) on a WAV file.

The binary has hardcoded filenames (``input.wav`` / ``output.wav``) relative
to the working directory.  To avoid touching the originals (the repo copy of
``input.wav`` is read-only), we copy the binary and the dry WAV to a private
temp directory and run from there.
"""
from __future__ import annotations

import re
import shutil
import subprocess
import tempfile
from pathlib import Path

import numpy as np
import soundfile as sf

# C++ binary prints e.g. "Newton: samples=391272 ... avgIters=2.2 maxIters=5 ...".
_NEWTON_RE = re.compile(r"avgIters=([0-9.eE+-]+).*?maxIters=([0-9]+)")


def parse_newton(text: str) -> dict:
    """Extract {'avg_iters', 'max_iters'} from a binary's ``Newton: ...`` line (or {} if absent)."""
    m = _NEWTON_RE.search(text or "")
    if not m:
        return {}
    return {"avg_iters": float(m.group(1)), "max_iters": int(m.group(2))}

WDF_DIR = Path.home() / "Git/ba283_wdf"
BINARY = WDF_DIR / "ba283_sim"


def build_if_needed(allow_build: bool = True) -> None:
    """Ensure a usable WDF binary exists, rebuilding from main.cpp when sensible.

    The ba283_wdf source is an externally-edited repo that is sometimes mid-edit
    and uncompilable. This function is resilient to that:

      - If the binary is present and at least as new as main.cpp, do nothing.
      - If the source is newer (or the binary is missing), attempt a rebuild —
        but if the rebuild FAILS and a last-good binary already exists, warn and
        fall back to it instead of aborting. Only raise if no binary exists at all.
      - With ``allow_build=False`` (the ``--no-build`` path), never rebuild; just
        use whatever binary is on disk (error if none).
    """
    src = WDF_DIR / "main.cpp"
    fresh = BINARY.exists() and src.exists() and BINARY.stat().st_mtime >= src.stat().st_mtime

    if fresh:
        return

    if not allow_build:
        if BINARY.exists():
            if src.exists() and BINARY.stat().st_mtime < src.stat().st_mtime:
                print("[run_wdf] WARNING: main.cpp newer than ba283_sim; "
                      "using existing binary (--no-build).")
            return
        raise RuntimeError(
            f"No WDF binary at {BINARY} and --no-build set; build it first."
        )

    if BINARY.exists():
        print("[run_wdf] WARNING: main.cpp newer than ba283_sim; rebuilding...")
    else:
        print("[run_wdf] ba283_sim missing; building with clang++...")

    try:
        subprocess.run(
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
            check=True,
            capture_output=True,
            text=True,
        )
        print("[run_wdf] build done.")
    except subprocess.CalledProcessError as exc:
        if BINARY.exists():
            print("[run_wdf] WARNING: rebuild FAILED (source likely mid-edit); "
                  "falling back to last-good binary.")
            print("[run_wdf] build error (first lines):")
            for line in (exc.stderr or "").splitlines()[:5]:
                print(f"    {line}")
        else:
            raise RuntimeError(
                f"WDF build failed and no last-good binary exists:\n{exc.stderr}"
            ) from exc


def run_wdf(
    dry_wav: Path,
    rgain: float = 220.0,
    bias: float = 0.77,
    quality: str | None = None,
    return_stats: bool = False,
):
    """Run the WDF model on *dry_wav* and return (samples_float32, sample_rate).

    Copies the binary and the dry WAV to a temp directory so the original
    ``input.wav`` (which may be read-only) is never modified.

    The full-chain model's CLI is ``ba283_sim [Rgain] [bias]`` and its output is
    **raw volts at node SEC_IN** (DC removed).  The dataset is conditioned on
    GAIN at a fixed bias: Rgain (K↔J shunt R_KJ) is swept while bias is pinned at
    0.77 (≈65 mA TR3 idle, symmetric clipping).  Rgain default 220 Ω = the real
    card's 26 dB tap.

    Args:
        dry_wav: Path to a 96 kHz mono float WAV (peak ≤ 0.5 V, in volts).
        rgain:   K↔J gain shunt R_KJ in ohms (lower → more gain).
        bias:    RV1 trim pot ∈ [0,1] (held at 0.77 for the gain-conditioned set).
        quality: optional tier preset (``realtime``/``fast``/``medium``/``high``)
                 appended as ``--quality <q>``. **Default ``None`` keeps the C++
                 binary's own default tier (``high``, os=4)** — pass an explicit
                 tier when comparing against the Rust port (which is os=2).
        return_stats: when True, return ``(samples, sr, stats)`` where ``stats`` is
                 the parsed ``Newton: ...`` line (avg_iters/max_iters) or ``{}``.

    Returns:
        ``(samples_float32, sample_rate)`` — or ``(samples, sr, stats)`` if
        ``return_stats``.
    """
    build_if_needed()
    dry_wav = Path(dry_wav)

    with tempfile.TemporaryDirectory(prefix="wdf_run_") as _tmp:
        tmp = Path(_tmp)
        # Binary + dry WAV are the only files the process needs.
        shutil.copy2(BINARY, tmp / "ba283_sim")
        shutil.copy2(dry_wav, tmp / "input.wav")

        argv = [str(tmp / "ba283_sim"), str(rgain), str(bias)]
        if quality is not None:
            argv += ["--quality", quality]
        result = subprocess.run(
            argv,
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
        samples = np.asarray(samples, dtype=np.float32)
        if return_stats:
            return samples, int(sr), parse_newton(result.stdout)
        return samples, int(sr)
