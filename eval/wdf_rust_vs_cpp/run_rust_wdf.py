"""
Drive the Rust WDF standalone (``target/release/ba283_standalone``) on a WAV.

Analogue of ``eval.wdf_vs_spice.run_wdf`` for the Rust port. Unlike the C++
binary (hardcoded ``input.wav``/``output.wav`` in cwd), the Rust standalone takes
explicit ``--input``/``--output`` paths, so no temp-dir copy dance is needed.

To match the C++ output domain we drive the WDF backend at unity, 100 % wet:
``--backend wdf --mix 100 --drive-db 0 --output-db 0``. At a 96 kHz input the
full chain runs the WDF model directly (no extra 2x oversampler, no dry blend),
so the result is the **pure WDF wet** sample (volts at SEC_IN, DC removed) — the
same quantity the C++ ``output.wav`` holds and that the in-tree ``OUTPUT_256``
golden vectors null against.
"""
from __future__ import annotations

import os
import re
import subprocess
import tempfile
from pathlib import Path

import numpy as np
import soundfile as sf

REPO = Path(__file__).resolve().parents[2]
BINARY = REPO / "target/release/ba283_standalone"

# The standalone prints e.g. "Newton: samples=1728000 avgIters=2.316 maxIters=6" to STDERR.
_NEWTON_RE = re.compile(r"avgIters=([0-9.eE+-]+).*?maxIters=([0-9]+)")


def parse_newton(text: str) -> dict:
    """Extract {'avg_iters', 'max_iters'} from the ``Newton: ...`` line (or {} if absent)."""
    m = _NEWTON_RE.search(text or "")
    if not m:
        return {}
    return {"avg_iters": float(m.group(1)), "max_iters": int(m.group(2))}


def build_if_needed(allow_build: bool = True, native: bool = True) -> None:
    """Ensure ``ba283_standalone`` is built (release). Build with ``-C target-cpu=native`` by
    default so its codegen matches the C++ Makefile's ``-march=native`` for a fair perf race.

    cargo's own freshness check is authoritative; we just shell out to it (a no-op when current).
    """
    if not allow_build:
        if not BINARY.exists():
            raise RuntimeError(f"No Rust binary at {BINARY} and allow_build=False; build it first.")
        return
    env = dict(os.environ)
    if native:
        # Append so a caller-provided RUSTFLAGS is preserved.
        env["RUSTFLAGS"] = (env.get("RUSTFLAGS", "") + " -C target-cpu=native").strip()
    subprocess.run(
        ["cargo", "build", "--release", "--bin", "ba283_standalone"],
        cwd=REPO,
        env=env,
        check=True,
        capture_output=True,
        text=True,
    )


def run_rust_wdf(
    dry_wav: Path,
    rgain: float = 220.0,
    bias: float = 0.77,
    tier: str = "realtime",
    return_stats: bool = False,
    allow_build: bool = True,
):
    """Run the Rust WDF model on *dry_wav* at *tier* and return (samples_float32, sr).

    Args:
        dry_wav: 96 kHz mono float WAV (volts, peak ≤ 0.5 V).
        rgain:   K↔J gain shunt R_KJ in ohms (``--circuit-gain``).
        bias:    RV1 trim pot ∈ [0,1].
        tier:    ``realtime`` | ``fast`` (``--tier``).
        return_stats: when True, return ``(samples, sr, stats)`` (parsed Newton line).

    Returns:
        ``(samples_float32, sample_rate)`` — or ``(samples, sr, stats)`` if ``return_stats``.
    """
    build_if_needed(allow_build=allow_build)
    dry_wav = Path(dry_wav)

    with tempfile.TemporaryDirectory(prefix="rust_wdf_") as _tmp:
        out_wav = Path(_tmp) / "output.wav"
        result = subprocess.run(
            [
                str(BINARY),
                "--backend", "wdf",
                "--tier", tier,
                "--circuit-gain", str(rgain),
                "--bias", str(bias),
                "--mix", "100",
                "--drive-db", "0",
                "--output-db", "0",
                "--input", str(dry_wav),
                "--output", str(out_wav),
            ],
            capture_output=True,
            text=True,
            timeout=300,
        )
        if result.returncode != 0:
            raise RuntimeError(
                f"Rust standalone failed (rc={result.returncode}):\n{result.stderr}"
            )
        if not out_wav.exists():
            raise RuntimeError("Rust standalone succeeded but output WAV was not written.")

        samples, sr = sf.read(out_wav, dtype="float32")
        if samples.ndim > 1:
            samples = samples[:, 0]
        samples = np.asarray(samples, dtype=np.float32)
        if return_stats:
            return samples, int(sr), parse_newton(result.stderr)
        return samples, int(sr)
